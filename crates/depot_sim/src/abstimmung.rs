//! Begrenzter Austausch nach fixierten unabhängigen Erst- und Zweitmeinungen.
use crate::lauf::{TopfAntwort, Zyklus, begruende, journal};
use crate::{Antwort, Berater, Lage, LaufFehler};
use depot_journal::{Journal, KiZweck};
use depot_kern::{BeraterId, Vorschlag};
use depot_ki::Sperre;

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
    let lage = Lage {
        datum: zyklus.tag.datum,
        topf: &topf.topf,
        kaufbar,
        kurse: &zyklus.tag.kurse,
        verlauf: &verlauf,
        positionskaeufe: &positionskaeufe,
        ereignisse: &zyklus.tag.ereignisse,
    };
    let erste = zweite.abstimmen(erlaubnis, &lage, dialog);
    if let Some(abrechnung) = zweite.abrechnung() {
        buch.rechne_ki_ab(&abrechnung).map_err(journal)?;
    }
    let vorschlag = erste.eigene.vorschlag.as_ref();
    let geaendert = !gleiche_ziele(vorschlag, dialog.eigener_vorschlag.as_ref());
    let runde = dialog.runde;
    let wer = match topf.topf.id().berater() {
        BeraterId::Dachs => "Dachs",
        BeraterId::Maus => "Maus",
    };
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
fn gleiche_ziele(a: Option<&Vorschlag>, b: Option<&Vorschlag>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.berater == b.berater && a.zielgewichte_bp == b.zielgewichte_bp,
        (None, None) => true,
        _ => false,
    }
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
    if !ea.erste_verwertbar || !eb.erste_verwertbar {
        return Ok(gueltig);
    }
    let (da, db) = (
        beitraege(buch, zyklus, BeraterId::Dachs),
        beitraege(buch, zyklus, BeraterId::Maus),
    );
    let (mut va, mut vb) = (ea.vorschlag.clone(), eb.vorschlag.clone());
    let (mut letzte_a, mut letzte_b) = (String::new(), String::new());
    let max = depot_ki::KiGrenzen::standard().aufrufe_je_zyklus();
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
        if !erste.eigene.verwertbar {
            break;
        }
        // Rückzug, Wechsel und Widerruf gelten auch bei einer unvollständigen Runde.
        let [ga, gb] = &mut gueltig;
        if !gleiche_ziele(ga.vorschlag.as_ref(), erste.eigene.vorschlag.as_ref()) {
            ga.vorschlag = None;
        }
        if !erste.zustimmung_fremd && gleiche_ziele(gb.vorschlag.as_ref(), vb.as_ref()) {
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
        uebernehme(ga, va.as_ref(), &erste, &zweite);
        uebernehme(gb, vb.as_ref(), &zweite, &erste);
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
