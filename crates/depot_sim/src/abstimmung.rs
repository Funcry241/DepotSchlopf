//! Begrenzter Austausch nach fixierten unabhängigen Erst- und Zweitmeinungen.
use crate::lauf::{TopfAntwort, Zyklus, journal};
use crate::{Antwort, Berater, Lage, LaufFehler};
use depot_journal::{Journal, KiZweck};
use depot_kern::{BeraterId, Vorschlag};

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
fn frage(
    buch: &mut Journal,
    zyklus: &Zyklus<'_>,
    zweite: &mut dyn Berater,
    topf: &TopfAntwort,
    dialog: &AbstimmungsStand,
) -> Result<AbstimmungsAntwort, LaufFehler> {
    let erlaubnis = buch
        .reserviere_fuer(
            topf.topf.id().berater(),
            zyklus.tag.datum,
            zyklus.nr,
            KiZweck::Abstimmung,
        )
        .map_err(|s| crate::lauf::halt(format!("Abstimmungsreservierung: {s:?}")))?;
    let lage = Lage {
        datum: zyklus.tag.datum,
        topf: &topf.topf,
        kurse: &zyklus.tag.kurse,
        ereignisse: &zyklus.tag.ereignisse,
    };
    let erste = zweite.abstimmen(erlaubnis, &lage, dialog);
    if let Some(abrechnung) = zweite.abrechnung() {
        buch.rechne_ki_ab(&abrechnung).map_err(journal)?;
    }
    let text = format!(
        "Abstimmung Runde {}: verwertbar={}; Änderung={}; Ziele={:?}; Zustimmung zum fremden Stand={}; Grund: {}",
        dialog.runde,
        erste.eigene.verwertbar,
        erste.eigene.vorschlag != dialog.eigener_vorschlag,
        erste.eigene.vorschlag.as_ref().map(|v| &v.zielgewichte_bp),
        erste.zustimmung_fremd,
        crate::vorbereitung::text_fuer_frage(&erste.eigene.einschaetzung)
    );
    buch.begruende(
        zyklus.tag.datum,
        match topf.topf.id().berater() {
            BeraterId::Dachs => "Dachs",
            BeraterId::Maus => "Maus",
        },
        &text,
    )
    .map_err(journal)?;
    Ok(erste)
}
fn uebernehme(
    gueltig: &mut TopfAntwort,
    bisher: Option<&Vorschlag>,
    neu: &AbstimmungsAntwort,
    peer: &AbstimmungsAntwort,
) {
    if neu.eigene.vorschlag.is_none() {
        gueltig.vorschlag = None;
    } else if neu.eigene.vorschlag.as_ref() == bisher && peer.zustimmung_fremd {
        gueltig.vorschlag.clone_from(&neu.eigene.vorschlag);
    } else if gueltig.vorschlag.as_ref() == bisher && !peer.zustimmung_fremd {
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
        let frei = [BeraterId::Dachs, BeraterId::Maus].into_iter().all(|wer| {
            buch.pruefe_ki(wer, zyklus.tag.datum, zyklus.nr, KiZweck::Abstimmung)
                .is_ok()
        });
        if !frei {
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
        let erste = frage(buch, zyklus, dachs, &ea, &sa)?;
        if !erste.eigene.verwertbar || !crate::lauf::beratungszeit(zyklus) {
            break;
        }
        let zweite = frage(buch, zyklus, maus, &eb, &sb)?;
        if !zweite.eigene.verwertbar {
            break;
        }
        let [ga, gb] = &mut gueltig;
        uebernehme(ga, va.as_ref(), &erste, &zweite);
        uebernehme(gb, vb.as_ref(), &zweite, &erste);
        let gleich = va == erste.eigene.vorschlag && vb == zweite.eigene.vorschlag;
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
