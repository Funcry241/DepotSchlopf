//! Begrenzter Austausch nach fixierten unabhängigen Erst- und Zweitmeinungen.
use crate::lauf::{TopfAntwort, Zyklus, begruende, journal, topf_name};
use crate::{Antwort, Berater, Lage, LaufFehler};
use depot_journal::{Journal, KiZweck};
use depot_kern::{BeraterId, Isin, Kurse, Topf, Vorschlag};
use depot_ki::Sperre;

pub(crate) const ABSTIMMUNG_RUNDEN: u32 = 2;
pub(crate) const OHNE_BEGRUENDUNG: &str = "ohne Begründung";

fn protokollgrund(grund: Option<&str>) -> &str {
    grund
        .filter(|text| !text.trim().is_empty())
        .unwrap_or(OHNE_BEGRUENDUNG)
}

#[derive(Clone, Debug)]
pub struct AbstimmungsStand {
    pub runde: u32,
    pub eigener_vorschlag: Option<Vorschlag>,
    pub fremder_vorschlag: Option<Vorschlag>,
    pub eigene_erstmeinung: String,
    pub fremde_beitraege: String,
    pub kaufkosten: String,
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
        .map(|(_, _, t)| {
            if t.contains(": Veto:") {
                // Die Zweitmeinung begrenzt die Gründe je Wert und ihren Freitext separat.
                t.clone()
            } else {
                crate::vorbereitung::text_fuer_frage(t)
            }
        })
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

fn runde_frei(
    buch: &Journal,
    zyklus: &Zyklus<'_>,
    dachs: &dyn Berater,
    maus: &dyn Berater,
) -> Result<bool, LaufFehler> {
    for (wer, berater) in [(BeraterId::Dachs, dachs), (BeraterId::Maus, maus)] {
        match buch.pruefe_ki_mit_einstellung(
            wer,
            zyklus.tag.datum,
            zyklus.nr,
            KiZweck::Abstimmung,
            berater.einstellung(),
        ) {
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
    teilveto: &[Isin],
) -> Result<Option<AbstimmungsAntwort>, LaufFehler> {
    let kaufbar = crate::lauf::kaufbudget(buch, zyklus.grenzen, topf.topf.id())?;
    let wer = topf.topf.id().berater();
    let erlaubnis = match buch.reserviere_mit_einstellung(
        wer,
        zyklus.tag.datum,
        zyklus.nr,
        KiZweck::Abstimmung,
        zweite.einstellung(),
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
    let (relative_staerke, relativ_kurz) = crate::relative_staerke::fuer_frage(
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
        relativ_kurz,
        positionskaeufe: &positionskaeufe,
        ereignisse: &zyklus.tag.ereignisse,
    };
    let bilanz = crate::lauf::prognosebilanz(buch.stand());
    let mut erste = zweite.abstimmen_mit_bilanz(erlaubnis, &lage, dialog, &bilanz);
    crate::lauf::belege_ki(buch, zweite).map_err(journal)?;
    let wer = match topf.topf.id().berater() {
        BeraterId::Dachs => "Dachs",
        BeraterId::Maus => "Maus",
    };
    if erste.eigene.verwertbar {
        let runde = u8::try_from(dialog.runde)
            .map_err(|_| crate::lauf::halt("Abstimmungsrunde außerhalb des Kennungsbereichs"))?;
        crate::schattenkandidaten::belege(
            buch,
            zyklus,
            crate::schattenkandidaten::Phase::Runde(runde),
            &topf.topf,
            erste.eigene.vorschlag.as_ref(),
        )?;
        crate::ausgelassen::belege_ausgelassene(
            buch,
            zyklus,
            &lage,
            &mut erste.eigene.vorschlag,
            wer,
        )?;
        erste.eigene.vorschlag = bereinige(erste.eigene.vorschlag.take(), teilveto);
    }
    let vorschlag = erste.eigene.vorschlag.as_ref();
    let geaendert =
        !gleiche_abstimmungsziele(vorschlag, dialog.eigener_vorschlag.as_ref(), teilveto);
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
            let grund = protokollgrund(vorschlag.begruendung.get(isin).map(String::as_str));
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

fn gleiche_abstimmungsziele(
    a: Option<&Vorschlag>,
    b: Option<&Vorschlag>,
    teilveto: &[Isin],
) -> bool {
    if !gleiche_ziele(a, b) {
        return false;
    }
    if teilveto.is_empty() {
        return true;
    }
    let (Some(a), Some(b)) = (a, b) else {
        return true;
    };
    let gleicher_park = match (&a.park, &b.park) {
        (Some(a), Some(b)) => {
            a.isin == b.isin
                && a.reserve_bp == b.reserve_bp
                && a.naechster_kauf_tage == b.naechster_kauf_tage
        }
        (a, b) => a == b,
    };
    let plan = |v: &Vorschlag, isin| {
        v.begruendung
            .get(isin)
            .and_then(|grund| crate::positionskauf::Ausstiegsplan::lies(grund))
    };
    gleicher_park
        && a.zielgewichte_bp
            .keys()
            .all(|isin| plan(a, isin) == plan(b, isin))
}

fn nennt(v: &Vorschlag, isin: &Isin) -> bool {
    v.zielgewichte_bp.contains_key(isin) || v.park.as_ref().is_some_and(|p| &p.isin == isin)
}

/// Übernimmt nur einen Rest, den genau das Teilveto aus der Erstmeinung bilden kann.
fn abstimmungsstart(
    erst: Option<&Vorschlag>,
    gueltig: Option<&Vorschlag>,
) -> (Option<Vorschlag>, Vec<Isin>) {
    let (Some(v), Some(rest)) = (erst, gueltig) else {
        return (erst.cloned(), Vec::new());
    };
    let gestrichen: Vec<_> = v
        .zielgewichte_bp
        .keys()
        .chain(v.park.iter().map(|p| &p.isin))
        .filter(|isin| !nennt(rest, isin))
        .cloned()
        .collect();
    if let crate::teilveto::Urteilsfolge::Teilveto {
        vorschlag,
        abgelehnt,
    } = crate::teilveto::urteilsfolge(v, false, Some(&gestrichen))
        && vorschlag.as_ref() == rest
    {
        return (Some(*vorschlag), abgelehnt);
    }
    (erst.cloned(), Vec::new())
}

fn bereinige(neu: Option<Vorschlag>, teilveto: &[Isin]) -> Option<Vorschlag> {
    let neu = neu?;
    let gestrichen: Vec<_> = teilveto
        .iter()
        .filter(|isin| nennt(&neu, isin))
        .cloned()
        .collect();
    if gestrichen.is_empty() {
        return Some(neu);
    }
    match crate::teilveto::urteilsfolge(&neu, false, Some(&gestrichen)) {
        crate::teilveto::Urteilsfolge::Teilveto { vorschlag, .. } => Some(*vorschlag),
        _ => None,
    }
}

pub(crate) fn nur_verkleinerung(topf: &Topf, kurse: &Kurse, v: &Vorschlag) -> bool {
    use depot_regelwerk::entscheidung::ist_gewicht_bp;
    // Ein Parkwert kann kaufen: Er braucht wie jeder Kauf die Zweitmeinung (#311 H3).
    if v.berater != topf.id().berater() || v.park.is_some() {
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
    teilveto: &[Isin],
) -> Result<bool, LaufFehler> {
    gueltig.eigener_einsatz_bp =
        crate::mindesteinsatz::zielanteil(&gueltig.topf, &zyklus.tag.kurse, neu);
    let nur_verkleinerung =
        neu.is_some_and(|v| nur_verkleinerung(&gueltig.topf, &zyklus.tag.kurse, v));
    let geaendert = !gleiche_abstimmungsziele(gueltig.vorschlag.as_ref(), neu, teilveto);
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
    bisher: Option<&Vorschlag>,
    neu: &AbstimmungsAntwort,
    peer: &AbstimmungsAntwort,
    teilveto: &[Isin],
) -> Option<Vorschlag> {
    if gleiche_abstimmungsziele(neu.eigene.vorschlag.as_ref(), bisher, teilveto)
        && peer.zustimmung_fremd
    {
        // Die Kaufthese stammt aus dem Stand, den der andere Berater gesehen hat.
        bisher.cloned()
    } else {
        None
    }
}

fn widerrufe_fremden_stand(
    gueltig: &mut TopfAntwort,
    gezeigt: Option<&Vorschlag>,
    antwort: &AbstimmungsAntwort,
    kurse: &Kurse,
    teilveto: &[Isin],
) {
    if !antwort.zustimmung_fremd
        && gleiche_abstimmungsziele(gueltig.vorschlag.as_ref(), gezeigt, teilveto)
        && !gueltig
            .vorschlag
            .as_ref()
            .is_some_and(|v| nur_verkleinerung(&gueltig.topf, kurse, v))
    {
        gueltig.vorschlag = None;
    }
}

/// Ohne offenen Punkt klärt die Abstimmung nichts und kostet nur Aufrufe (#312-1, P35).
fn entfaellt(
    buch: &mut Journal,
    zyklus: &Zyklus<'_>,
    erste: [&TopfAntwort; 2],
    gueltig: &[TopfAntwort; 2],
) -> Result<bool, LaufFehler> {
    let [ea, eb] = erste;
    let [ga, gb] = gueltig;
    let toepfe = [(ea, ga), (eb, gb)].map(|(e, g)| {
        (
            e.vorschlag.as_ref(),
            g.vorschlag.as_ref(),
            g.ersatzkauf_offen(),
        )
    });
    if crate::offener_punkt::offener_punkt(toepfe) {
        return Ok(false);
    }
    begruende(
        buch,
        zyklus.tag,
        crate::lauf::REGELWERK,
        "Abstimmung entfällt: kein offener Punkt",
    )?;
    Ok(true)
}

pub(crate) fn berate(
    buch: &mut Journal,
    zyklus: &Zyklus<'_>,
    erste: [TopfAntwort; 2],
    gueltig: [TopfAntwort; 2],
    dachs: &mut dyn Berater,
    maus: &mut dyn Berater,
) -> Result<[TopfAntwort; 2], LaufFehler> {
    if zyklus.uhr.is_none() || !dachs.kann_abstimmen() || !maus.kann_abstimmen() {
        return Ok(gueltig);
    }
    if erste
        .iter()
        .any(|e| e.erstmeinung != crate::lauf::Erstmeinung::Verwertbar)
    {
        return Ok(gueltig);
    }
    let [ea, eb] = &erste;
    if entfaellt(buch, zyklus, [ea, eb], &gueltig)? {
        return Ok(gueltig);
    }
    abstimmungsrunden(buch, zyklus, erste, gueltig, dachs, maus)
}

fn abstimmungsrunden(
    buch: &mut Journal,
    zyklus: &Zyklus<'_>,
    [ea, eb]: [TopfAntwort; 2],
    mut gueltig: [TopfAntwort; 2],
    dachs: &mut dyn Berater,
    maus: &mut dyn Berater,
) -> Result<[TopfAntwort; 2], LaufFehler> {
    let (da, db) = (
        beitraege(buch, zyklus, BeraterId::Dachs),
        beitraege(buch, zyklus, BeraterId::Maus),
    );
    let [ga, gb] = &gueltig;
    let (mut va, teilveto_a) = abstimmungsstart(ea.vorschlag.as_ref(), ga.vorschlag.as_ref());
    let (mut vb, teilveto_b) = abstimmungsstart(eb.vorschlag.as_ref(), gb.vorschlag.as_ref());
    let (mut letzte_a, mut letzte_b) = (String::new(), String::new());
    let max = depot_ki::KiGrenzen::standard()
        .aufrufe_je_zyklus()
        .min(ABSTIMMUNG_RUNDEN);
    for runde in 1..=max {
        if !crate::lauf::beratungszeit(zyklus) || !dachs.angebunden() || !maus.angebunden() {
            break;
        }
        if !runde_frei(buch, zyklus, dachs, maus)? {
            break;
        }
        let kaufkosten = crate::kostenhuerde::abstimmung(
            &zyklus.tag.kurse,
            &[(&ea.topf, va.as_ref()), (&eb.topf, vb.as_ref())],
        );
        let sa = AbstimmungsStand {
            runde,
            eigener_vorschlag: va.clone(),
            fremder_vorschlag: vb.clone(),
            eigene_erstmeinung: da.clone(),
            fremde_beitraege: format!("{db}\nLetzte Abstimmung: {letzte_b}"),
            kaufkosten: kaufkosten.clone(),
        };
        let sb = AbstimmungsStand {
            runde,
            eigener_vorschlag: vb.clone(),
            fremder_vorschlag: va.clone(),
            eigene_erstmeinung: db.clone(),
            fremde_beitraege: format!("{da}\nLetzte Abstimmung: {letzte_a}"),
            kaufkosten,
        };
        // Beide sehen denselben vor der Runde fixierten Stand; die zweite Anfrage sieht die erste Antwort nicht.
        let Some(erste) = frage(buch, zyklus, dachs, &ea, &sa, &teilveto_a)? else {
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
        let verkleinert_a = eigener_stand(
            buch,
            zyklus,
            ga,
            erste.eigene.vorschlag.as_ref(),
            &teilveto_a,
        )?;
        widerrufe_fremden_stand(gb, vb.as_ref(), &erste, &zyklus.tag.kurse, &teilveto_b);
        if !crate::lauf::beratungszeit(zyklus) {
            break;
        }
        let Some(zweite) = frage(buch, zyklus, maus, &eb, &sb, &teilveto_b)? else {
            break;
        };
        if !zweite.eigene.verwertbar {
            break;
        }
        let [ga, gb] = &mut gueltig;
        if !verkleinert_a {
            ga.vorschlag = uebernehme(va.as_ref(), &erste, &zweite, &teilveto_a);
        }
        if !eigener_stand(
            buch,
            zyklus,
            gb,
            zweite.eigene.vorschlag.as_ref(),
            &teilveto_b,
        )? {
            gb.vorschlag = uebernehme(vb.as_ref(), &zweite, &erste, &teilveto_b);
        }
        let gleich =
            gleiche_abstimmungsziele(va.as_ref(), erste.eigene.vorschlag.as_ref(), &teilveto_a)
                && gleiche_abstimmungsziele(
                    vb.as_ref(),
                    zweite.eigene.vorschlag.as_ref(),
                    &teilveto_b,
                );
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
    #[test]
    fn abstimmungsjournal_kennzeichnet_fehlende_gruende() {
        let leer = "  ";
        let begruendung = "Beleg nennt einen neuen Auftrag";
        assert_eq!(super::protokollgrund(None), "ohne Begründung");
        assert_eq!(super::protokollgrund(Some(leer)), "ohne Begründung");
        assert_eq!(super::protokollgrund(Some(begruendung)), begruendung);
    }

    use super::*;
    use depot_kern::{Cent, Datum, Herkunft, Isin, Kurs, Kurse, Topf, TopfId, Zeitpunkt};
    use std::collections::BTreeMap;

    fn kandidat(isins: &[Isin]) -> Vorschlag {
        Vorschlag {
            berater: BeraterId::Maus,
            zielgewichte_bp: isins.iter().cloned().map(|i| (i, 2_000)).collect(),
            limit_bp: BTreeMap::new(),
            stopp_bp: BTreeMap::new(),
            erwartung_bp: BTreeMap::new(),
            begruendung: BTreeMap::new(),
            belege: BTreeMap::new(),
            park: None,
        }
    }

    fn stimme(vorschlag: Option<Vorschlag>, zustimmung_fremd: bool) -> AbstimmungsAntwort {
        AbstimmungsAntwort {
            eigene: Antwort {
                vorschlag,
                verwertbar: true,
                halten: BTreeMap::new(),
                prognose: None,
                einschaetzung: String::new(),
            },
            zustimmung_fremd,
        }
    }

    #[test]
    fn teilveto_zeigt_und_bestaetigt_nur_den_geprueften_rest() -> Result<(), String> {
        let sap = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let alv = Isin::neu("DE0008404005").map_err(|e| format!("{e:?}"))?;
        let erst = kandidat(&[sap.clone(), alv.clone()]);
        let rest = kandidat(std::slice::from_ref(&sap));
        let (gezeigt, teilveto) = abstimmungsstart(Some(&erst), Some(&rest));
        assert_eq!(gezeigt.as_ref(), Some(&rest));
        assert_eq!(teilveto.as_slice(), std::slice::from_ref(&alv));

        // Der Autor wiederholt den alten Vorschlag. Der Prüfer bestätigt den tatsächlich
        // gezeigten Rest. Die gestrichene ISIN kommt auch mit diesem Ja nicht zurück.
        let autor = stimme(bereinige(Some(erst), &teilveto), false);
        let pruefer = stimme(None, true);
        assert_eq!(
            uebernehme(gezeigt.as_ref(), &autor, &pruefer, &teilveto),
            Some(rest.clone())
        );
        assert_eq!(bereinige(Some(kandidat(&[alv])), &teilveto), None);

        let mut geaendert = rest.clone();
        geaendert.zielgewichte_bp.insert(sap.clone(), 2_500);
        assert_eq!(
            uebernehme(
                Some(&rest),
                &stimme(Some(geaendert.clone()), false),
                &pruefer,
                &teilveto
            ),
            None
        );
        assert_eq!(
            uebernehme(
                Some(&geaendert),
                &stimme(Some(geaendert.clone()), false),
                &pruefer,
                &teilveto
            ),
            Some(geaendert)
        );

        let mut anderer_ausstieg = rest.clone();
        anderer_ausstieg
            .begruendung
            .insert(sap, "[Kursbewegung] [Ausstieg +300bp 5] Probe".into());
        assert_eq!(
            uebernehme(
                Some(&rest),
                &stimme(Some(anderer_ausstieg), false),
                &pruefer,
                &teilveto
            ),
            None
        );
        Ok(())
    }

    #[test]
    fn rueckzug_und_veto_geben_keinen_kauf_frei() -> Result<(), String> {
        let sap = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let alv = Isin::neu("DE0008404005").map_err(|e| format!("{e:?}"))?;
        let erst = kandidat(&[sap.clone(), alv]);
        let rest = kandidat(&[sap]);
        let (gezeigt, teilveto) = abstimmungsstart(Some(&erst), Some(&rest));
        assert_eq!(
            uebernehme(
                gezeigt.as_ref(),
                &stimme(None, false),
                &stimme(None, true),
                &teilveto
            ),
            None
        );
        assert_eq!(
            uebernehme(
                gezeigt.as_ref(),
                &stimme(Some(rest), false),
                &stimme(None, false),
                &teilveto
            ),
            None
        );

        let (gezeigt, teilveto) = abstimmungsstart(Some(&erst), None);
        assert!(teilveto.is_empty());
        assert_eq!(
            uebernehme(
                gezeigt.as_ref(),
                &stimme(Some(erst), false),
                &stimme(None, false),
                &teilveto
            ),
            None
        );
        Ok(())
    }

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
                        belege: std::collections::BTreeMap::new(),
                        park: None,
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
                            vor_valuta: &[],
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
            belege: std::collections::BTreeMap::new(),
            park: None,
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
