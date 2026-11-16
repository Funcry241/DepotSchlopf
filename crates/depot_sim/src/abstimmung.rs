//! Begrenzter Austausch nach fixierten unabhängigen Erst- und Zweitmeinungen.
use crate::lauf::{TopfAntwort, Zyklus, begruende, journal, topf_name};
use crate::{Antwort, Berater, Lage, LaufFehler};
use depot_journal::{Journal, KiZweck};
use depot_kern::{BeraterId, Kurse, Topf, Vorschlag};
use depot_ki::Sperre;

pub(crate) const ABSTIMMUNG_RUNDEN: u32 = 2;

#[derive(Clone, Debug)]
pub struct AbstimmungsStand {
    pub runde: u32,
    pub eigener_vorschlag: Option<Vorschlag>,
    pub fremder_vorschlag: Option<Vorschlag>,
    pub eigene_erstmeinung: String,
    pub fremde_beitraege: String,
}
#[derive(Clone, Debug)]
pub struct AbstimmungsAntwort {
    pub eigene: Antwort,
    /// Bezieht sich ausschließlich auf den vor dieser Runde fixierten fremden Vorschlag.
    pub zustimmung_fremd: bool,
}
impl AbstimmungsAntwort {
    pub fn unverwertbar() -> Self {
        Self {
            eigene: Antwort {
                halten: std::collections::BTreeMap::new(),
                prognose: None,
                verwertbar: false,
                einschaetzung: "Keine verwertbare Abstimmung".into(),
                vorschlag: None,
            },
            zustimmung_fremd: false,
        }
    }
}
fn beitraege(buch: &Journal, zyklus: &Zyklus<'_>, wer: BeraterId) -> String {
    let name = match wer {
        BeraterId::Dachs => "Dachs",
        BeraterId::Maus => "Maus",
    };
    buch.stand()
        .begruendungen()
        .iter()
        .filter(|(datum, w, t)| {
            let (eigen, fremd) = match wer {
                BeraterId::Dachs => ("Topf A:", "Topf B:"),
                BeraterId::Maus => ("Topf B:", "Topf A:"),
            };
            *datum == zyklus.tag.datum
                && w == name
                && (t
                    .strip_prefix(eigen)
                    .is_some_and(|t| t.starts_with(" Antwort "))
                    || t.strip_prefix(fremd).is_some_and(|t| {
                        t.starts_with(" Zustimmung:")
                            || t.starts_with(" Veto:")
                            || t.starts_with(" Antwort verworfen, kein Urteil:")
                    }))
        })
        .map(|(_, _, t)| crate::vorbereitung::text_fuer_frage(t))
        .collect::<Vec<_>>()
        .join("\n")
}
fn budgetgrenze(sperre: &Sperre) -> bool {
    matches!(
        sperre,
        Sperre::ZyklusVoll
            | Sperre::TagVoll
            | Sperre::MonatVoll
            | Sperre::MonatsbudgetErschoepft
            | Sperre::JahresbudgetErschoepft
            | Sperre::TagesrahmenErschoepft
            | Sperre::ErstmeinungReserviert
    )
}

fn runde_frei(buch: &Journal, zyklus: &Zyklus<'_>) -> Result<bool, LaufFehler> {
    for wer in [BeraterId::Dachs, BeraterId::Maus] {
        match buch.pruefe_ki(wer, zyklus.tag.datum, zyklus.nr, KiZweck::Abstimmung) {
            Ok(()) => {}
            Err(s) if budgetgrenze(&s) => return Ok(false),
            Err(s) => return Err(crate::lauf::halt(format!("Abstimmungsprüfung: {s:?}"))),
        }
    }
    Ok(true)
}

fn frage(
    buch: &mut Journal,
    zyklus: &Zyklus<'_>,
    zweite: &mut dyn Berater,
    topf: &TopfAntwort,
    dialog: &AbstimmungsStand,
) -> Result<Option<AbstimmungsAntwort>, LaufFehler> {
    let kaufbar = crate::lauf::kaufbudget(buch, zyklus.grenzen, topf.topf.id())?;
    let erlaubnis = match buch.reserviere_fuer(
        topf.topf.id().berater(),
        zyklus.tag.datum,
        zyklus.nr,
        KiZweck::Abstimmung,
    ) {
        Ok(erlaubnis) => erlaubnis,
        Err(s) if budgetgrenze(&s) => return Ok(None),
        Err(s) => return Err(crate::lauf::halt(format!("Abstimmungsreservierung: {s:?}"))),
    };
    let positionskaeufe = crate::positionskauf::lies(buch.stand(), &topf.topf);
    let verlauf = crate::kursverlauf::fuer_lage(
        buch.stand(),
        &zyklus.tag.kurse,
        zyklus.universum,
        zyklus.tag.datum,
    );
    let relative_staerke = crate::relative_staerke::fuer_lage(
        buch.stand(),
        &zyklus.tag.kurse,
        zyklus.universum,
        zyklus.tag.datum,
    );
    let lage = Lage {
        anlass: None,
        datum: zyklus.tag.datum,
        topf: &topf.topf,
        kaufbar,
        kurse: &zyklus.tag.kurse,
        verlauf: &verlauf,
        kaufbilanz: crate::ausfuehrung::kaufbilanz(buch.stand(), topf.topf.id(), zyklus.tag.datum),
        relative_staerke: &relative_staerke,
        positionskaeufe: &positionskaeufe,
        ereignisse: &zyklus.tag.ereignisse,
    };
    let mut erste = zweite.abstimmen(erlaubnis, &lage, dialog);
    crate::lauf::belege_ki(buch, zweite).map_err(journal)?;
    let wer = match topf.topf.id().berater() {
        BeraterId::Dachs => "Dachs",
        BeraterId::Maus => "Maus",
    };
    if erste.eigene.verwertbar {
        crate::ausgelassen::belege_ausgelassene(
            buch,
            zyklus,
            &lage,
            &mut erste.eigene.vorschlag,
            wer,
        )?;
    }
    let vorschlag = erste.eigene.vorschlag.as_ref();
    let geaendert = !gleiche_ziele(vorschlag, dialog.eigener_vorschlag.as_ref());
    let runde = dialog.runde;
    let text = format!(
        "Abstimmung Runde {runde}: verwertbar={}; Änderung={geaendert}; Ziele={:?}; Zustimmung zum fremden Stand={}; Grund: {}",
        erste.eigene.verwertbar,
        vorschlag.map(|v| &v.zielgewichte_bp),
        erste.zustimmung_fremd,
        crate::vorbereitung::text_fuer_frage(&erste.eigene.einschaetzung)
    );
    begruende(buch, zyklus.tag, wer, &text)?;
    if erste.eigene.verwertbar
        && geaendert
        && let Some(vorschlag) = vorschlag
    {
        for (isin, bp) in &vorschlag.zielgewichte_bp {
            let grund = vorschlag.begruendung.get(isin).map_or("", String::as_str);
            begruende(
                buch,
                zyklus.tag,
                wer,
                &format!("Abstimmung Runde {runde}: {isin} {bp} bp: {grund}"),
            )?;
        }
    }
    Ok(Some(erste))
}

/// Zustimmung gilt für Berater und Zielgewichte. `None` ist ein Rückzug.
pub(crate) fn gleiche_ziele(a: Option<&Vorschlag>, b: Option<&Vorschlag>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            a.berater == b.berater
                && a.zielgewichte_bp == b.zielgewichte_bp
                && a.limit_bp == b.limit_bp
                && a.stopp_bp == b.stopp_bp
        }
        (None, None) => true,
        _ => false,
    }
}

pub(crate) fn nur_verkleinerung(topf: &Topf, kurse: &Kurse, v: &Vorschlag) -> bool {
    use depot_regelwerk::entscheidung::ist_gewicht_bp;
    if v.berater != topf.id().berater() {
        return false;
    }
    let mut reduziert = false;
    for (isin, ziel) in &v.zielgewichte_bp {
        if topf.stueck(isin) == 0 {
            if *ziel != 0 {
                return false;
            }
            continue;
        }
        let Some(ist) = ist_gewicht_bp(topf, kurse, isin) else {
            return false;
        };
        if !(0..=ist).contains(ziel) {
            return false;
        }
        reduziert |= *ziel == 0 || *ziel < ist;
    }
    reduziert
}

pub(crate) fn belege_verkleinerung(
    buch: &mut Journal,
    zyklus: &Zyklus<'_>,
    topf: &Topf,
) -> Result<(), LaufFehler> {
    begruende(
        buch,
        zyklus.tag,
        crate::lauf::REGELWERK,
        &format!("{}: Verkleinerung ohne Zweitmeinung", topf_name(topf.id())),
    )
}

fn eigener_stand(
    buch: &mut Journal,
    zyklus: &Zyklus<'_>,
    gueltig: &mut TopfAntwort,
    neu: Option<&Vorschlag>,
) -> Result<bool, LaufFehler> {
    let nur_verkleinerung =
        neu.is_some_and(|v| nur_verkleinerung(&gueltig.topf, &zyklus.tag.kurse, v));
    let geaendert = !gleiche_ziele(gueltig.vorschlag.as_ref(), neu);
    if nur_verkleinerung {
        if geaendert {
            belege_verkleinerung(buch, zyklus, &gueltig.topf)?;
        }
        gueltig.vorschlag = neu.cloned();
    } else if geaendert {
        gueltig.vorschlag = None;
    }
    Ok(nur_verkleinerung)
}

fn uebernehme(
    gueltig: &mut TopfAntwort,
    bisher: Option<&Vorschlag>,
    neu: &AbstimmungsAntwort,
    peer: &AbstimmungsAntwort,
) {
    if gleiche_ziele(neu.eigene.vorschlag.as_ref(), bisher) && peer.zustimmung_fremd {
        // Die Kaufthese stammt aus dem Stand, den der andere Berater gesehen hat.
        gueltig.vorschlag = bisher.cloned();
    } else {
        gueltig.vorschlag = None;
    }
}

pub(crate) fn berate(
    buch: &mut Journal,
    zyklus: &Zyklus<'_>,
    erste: [TopfAntwort; 2],
    mut gueltig: [TopfAntwort; 2],
    dachs: &mut dyn Berater,
    maus: &mut dyn Berater,
) -> Result<[TopfAntwort; 2], LaufFehler> {
    if zyklus.uhr.is_none() || !dachs.kann_abstimmen() || !maus.kann_abstimmen() {
        return Ok(gueltig);
    }
    let [ea, eb] = erste;
    if [ea.erstmeinung, eb.erstmeinung]
        .iter()
        .any(|e| *e != crate::lauf::Erstmeinung::Verwertbar)
    {
        return Ok(gueltig);
    }
    let (da, db) = (
        beitraege(buch, zyklus, BeraterId::Dachs),
        beitraege(buch, zyklus, BeraterId::Maus),
    );
    let (mut va, mut vb) = (ea.vorschlag.clone(), eb.vorschlag.clone());
    let (mut letzte_a, mut letzte_b) = (String::new(), String::new());
    let max = depot_ki::KiGrenzen::standard()
        .aufrufe_je_zyklus()
        .min(ABSTIMMUNG_RUNDEN);
    for runde in 1..=max {
        if !crate::lauf::beratungszeit(zyklus) || !dachs.angebunden() || !maus.angebunden() {
            break;
        }
        if !runde_frei(buch, zyklus)? {
            break;
        }
        let sa = AbstimmungsStand {
            runde,
            eigener_vorschlag: va.clone(),
            fremder_vorschlag: vb.clone(),
            eigene_erstmeinung: da.clone(),
            fremde_beitraege: format!("{db}\nLetzte Abstimmung: {letzte_b}"),
        };
        let sb = AbstimmungsStand {
            runde,
            eigener_vorschlag: vb.clone(),
            fremder_vorschlag: va.clone(),
            eigene_erstmeinung: db.clone(),
            fremde_beitraege: format!("{da}\nLetzte Abstimmung: {letzte_a}"),
        };
        // Beide sehen denselben vor der Runde fixierten Stand; die zweite Anfrage sieht die erste Antwort nicht.
        let Some(erste) = frage(buch, zyklus, dachs, &ea, &sa)? else {
            break;
        };
        // Ein neuer Austausch muss die aktuelle Zustimmung selbst belegen.
        for antwort in &mut gueltig {
            antwort.verwerfe_ersatzkauf();
        }
        if !erste.eigene.verwertbar {
            break;
        }
        // Rückzug, Wechsel und Widerruf gelten auch bei einer unvollständigen Runde.
        let [ga, gb] = &mut gueltig;
        let verkleinert_a = eigener_stand(buch, zyklus, ga, erste.eigene.vorschlag.as_ref())?;
        if !erste.zustimmung_fremd
            && gleiche_ziele(gb.vorschlag.as_ref(), vb.as_ref())
            && !gb
                .vorschlag
                .as_ref()
                .is_some_and(|v| nur_verkleinerung(&gb.topf, &zyklus.tag.kurse, v))
        {
            gb.vorschlag = None;
        }
        if !crate::lauf::beratungszeit(zyklus) {
            break;
        }
        let Some(zweite) = frage(buch, zyklus, maus, &eb, &sb)? else {
            break;
        };
        if !zweite.eigene.verwertbar {
            break;
        }
        let [ga, gb] = &mut gueltig;
        if !verkleinert_a {
            uebernehme(ga, va.as_ref(), &erste, &zweite);
        }
        if !eigener_stand(buch, zyklus, gb, zweite.eigene.vorschlag.as_ref())? {
            uebernehme(gb, vb.as_ref(), &zweite, &erste);
        }
        let gleich = gleiche_ziele(va.as_ref(), erste.eigene.vorschlag.as_ref())
            && gleiche_ziele(vb.as_ref(), zweite.eigene.vorschlag.as_ref());
        letzte_a = crate::vorbereitung::text_fuer_frage(&erste.eigene.einschaetzung);
        letzte_b = crate::vorbereitung::text_fuer_frage(&zweite.eigene.einschaetzung);
        va = erste.eigene.vorschlag;
        vb = zweite.eigene.vorschlag;
        if gleich {
            break;
        }
    }
    Ok(gueltig)
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_kern::{Cent, Datum, Herkunft, Isin, Kurs, Kurse, Topf, TopfId, Zeitpunkt};

    #[test]
    fn invariante_nie_kauf_ohne_zustimmung() -> Result<(), String> {
        use depot_kern::{Grenzen, Instrument, InstrumentArt, Universum};
        use depot_regelwerk::{Eingabe, Kennungsvergabe, entscheide_mit_kaufbudget};
        let tag = Datum::neu(2026, 11, 10).ok_or("Datum")?;
        let zeit = Zeitpunkt::neu(tag, 17, 35, 0).ok_or("Zeit")?;
        let sap = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let alv = Isin::neu("DE0008404005").map_err(|e| format!("{e:?}"))?;
        let kurs = Kurs::neu(Cent(10_000), "EUR", tag, zeit, Herkunft::Simulation)
            .map_err(|e| format!("{e:?}"))?;
        let kurse = Kurse::neu(zeit, vec![(sap.clone(), kurs.clone()), (alv.clone(), kurs)])
            .map_err(|e| format!("{e:?}"))?;
        let grenzen = Grenzen::standard();
        let universum = Universum::neu(
            [&sap, &alv]
                .into_iter()
                .map(|isin| Instrument {
                    isin: isin.clone(),
                    name: isin.als_str().into(),
                    art: InstrumentArt::Aktie,
                })
                .collect(),
        );
        let mut verkauf_geprueft = false;
        for bestand in [0_i64, 5, 10, 15] {
            let bestand_bp = bestand.saturating_mul(200);
            let mut topf = Topf::neu(
                TopfId::A,
                Cent(500_000_i64.saturating_sub(bestand.saturating_mul(10_000))),
            )
            .map_err(|e| format!("{e:?}"))?;
            topf.setze_bestand(
                sap.clone(),
                u64::try_from(bestand).map_err(|e| e.to_string())?,
            );
            for ziel in [
                -1,
                0,
                500,
                1500,
                2000,
                3000,
                4000,
                10001,
                bestand_bp,
                bestand_bp.saturating_add(1),
            ] {
                for fremd in [-1, 0, 1, 2000] {
                    let v = Vorschlag {
                        berater: BeraterId::Dachs,
                        zielgewichte_bp: std::collections::BTreeMap::from([
                            (sap.clone(), ziel),
                            (alv.clone(), fremd),
                        ]),
                        begruendung: std::collections::BTreeMap::new(),
                        erwartung_bp: std::collections::BTreeMap::new(),
                        limit_bp: std::collections::BTreeMap::new(),
                        stopp_bp: std::collections::BTreeMap::new(),
                    };
                    let ohne_zustimmung = nur_verkleinerung(&topf, &kurse, &v);
                    assert_eq!(
                        ohne_zustimmung,
                        fremd == 0 && (0..bestand_bp).contains(&ziel),
                        "Bestand {bestand}, Ziel {ziel}, neues Ziel {fremd}"
                    );
                    let plan = entscheide_mit_kaufbudget(
                        &Eingabe {
                            grenzen: &grenzen,
                            universum: &universum,
                            topf: &topf,
                            kurse: &kurse,
                            vorschlag: ohne_zustimmung.then_some(&v),
                            halt: false,
                        },
                        &mut Kennungsvergabe::nach(0),
                        topf.bargeld(),
                    );
                    assert!(plan.kaeufe.is_empty(), "{plan:?}");
                    verkauf_geprueft |= !plan.verkaeufe.is_empty();
                }
            }
        }
        assert!(verkauf_geprueft);
        Ok(())
    }

    #[test]
    fn abstimmung_ausgelassen_gleich() -> Result<(), String> {
        let tag = Datum::neu(2026, 11, 10).ok_or("Datum")?;
        let zeit = Zeitpunkt::neu(tag, 17, 35, 0).ok_or("Zeit")?;
        let sap = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let kurs = Kurs::neu(Cent(10_000), "EUR", tag, zeit, Herkunft::Simulation)
            .map_err(|e| format!("{e:?}"))?;
        let kurse = Kurse::neu(zeit, vec![(sap.clone(), kurs)]).map_err(|e| format!("{e:?}"))?;
        let mut topf = Topf::neu(TopfId::A, Cent(400_000)).map_err(|e| format!("{e:?}"))?;
        topf.setze_bestand(sap.clone(), 10);
        let mut neu = Vorschlag {
            erwartung_bp: std::collections::BTreeMap::new(),
            limit_bp: std::collections::BTreeMap::new(),
            stopp_bp: std::collections::BTreeMap::new(),
            berater: BeraterId::Dachs,
            zielgewichte_bp: std::collections::BTreeMap::new(),
            begruendung: std::collections::BTreeMap::new(),
        };
        let mut alt = neu.clone();
        alt.zielgewichte_bp.insert(sap.clone(), 2_000);
        assert_eq!(
            crate::ausgelassen::ergaenze_ausgelassene(&topf, &kurse, &mut neu),
            [sap]
        );
        assert!(gleiche_ziele(Some(&alt), Some(&neu)));
        Ok(())
    }
}
