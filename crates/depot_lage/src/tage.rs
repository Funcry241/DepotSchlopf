//! Tagesergebnis: je Handelstag seit dem ersten Papiertag, ob der Tag lief, ohne Handel blieb
//! oder ausfiel, und wer von Dachs und Maus tatsaechlich beigetragen hat.
//!
//! Alles kommt aus dem Journal (Begruendungen, Kursbelege, Abschluss) und dem Anbieterprotokoll.
//! Nichts davon entscheidet etwas; die Anzeige sagt nur, was war (Maus: Ein gruener Abschluss
//! allein sagt nicht, wer mitgewirkt hat, und ein verpasster Tag muss am naechsten Morgen auf
//! einen Blick zu sehen sein).

use std::collections::BTreeSet;
use std::fmt::Write as _;

use depot_journal::Stand;
use depot_journal::anbieter::{Eintrag, Ergebnis, Wer};
use depot_kern::liste;
use depot_kern::{BeraterId, Datum, Grenzen, Universum, Zeitpunkt};
use depot_regelwerk::waehle_tageswerte;
use depot_sim::{AUSWAHL, Gefragt, gefragt, tageskurse_am};

use crate::betrieb::wer_fuer;
use crate::ortszeit;

/// So viele Handelstage zeigt das Lagebild hoechstens.
pub const TAGE_MAX: usize = 10;

/// Wie ein Handelstag ausging.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TagStand {
    /// Der Papiertag lief und ist abgeschlossen.
    Abgeschlossen,
    /// Abgeschlossen; einige Kandidaten waren ohne gueltigen Kurs und heute nicht handelbar.
    /// Mit den Gruenden in Worten.
    MitAuslassung(Vec<String>),
    /// Abgeschlossen, aber die Kursprüfung liess den Zyklus ausfallen: niemand gefragt, kein
    /// Auftrag. Mit den Befunden in Worten.
    OhneHandel(Vec<String>),
    /// Begonnen und nicht abgeschlossen (das Lagebild zeigt den Halt).
    Angefangen,
    /// Kein Papiertag an diesem Tag, mit Grund.
    Ausgefallen(String),
    /// Heute, noch nicht gelaufen.
    Offen,
    /// Keine Boerse der Liste hat gehandelt (Feiertag), mit Grund: nichts verpasst.
    Geschlossen(String),
}

impl TagStand {
    pub fn klasse(&self) -> &'static str {
        match self {
            TagStand::Abgeschlossen | TagStand::MitAuslassung(_) => "gut",
            TagStand::OhneHandel(_) | TagStand::Offen | TagStand::Geschlossen(_) => "offen",
            TagStand::Angefangen | TagStand::Ausgefallen(_) => "halt",
        }
    }

    pub fn text(&self) -> String {
        match self {
            TagStand::Abgeschlossen => "Papiertag abgeschlossen".into(),
            TagStand::MitAuslassung(b) => format!(
                "Papiertag abgeschlossen; heute nicht handelbar und ausgelassen: {}",
                b.join("; ")
            ),
            TagStand::OhneHandel(b) => format!(
                "Abgeschlossen ohne Handel: Die Kursprüfung ließ den Tag ausfallen, niemand \
                 wurde gefragt ({})",
                b.join("; ")
            ),
            TagStand::Angefangen => {
                "Angefangen, nicht abgeschlossen: siehe Halt unter Bereitschaft".into()
            }
            TagStand::Ausgefallen(g) => format!("Ausgefallen: {g}"),
            TagStand::Offen => "Heute noch nicht gelaufen".into(),
            TagStand::Geschlossen(g) => format!("Börse geschlossen ({g}): kein Papiertag nötig"),
        }
    }
}

/// Was eine KI an einem Tag beigetragen hat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KiBeitrag {
    /// `gut`: hat geantwortet; `halt`: ausgefallen; `offen`: gefragt, aber ohne Antwort, die
    /// zaehlt, oder nichts bekannt.
    pub klasse: &'static str,
    pub text: String,
}

/// Ein Handelstag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagErgebnis {
    pub datum: Datum,
    pub stand: TagStand,
    /// Dachs, dann Maus; leer, wenn der Papiertag nicht lief.
    pub ki: Vec<(BeraterId, KiBeitrag)>,
}

const PAPIERTAG: &str = "Papiertag";
const DATEN: &str = "Daten";

/// Montag bis Freitag. Ob dort eine Boerse handelte, sagt `liste::handelstag`.
fn werktag(d: Datum) -> bool {
    d.wochentag().is_some_and(|w| (1..=5).contains(&w))
}

fn tag_davor(d: Datum) -> Option<Datum> {
    let z = Zeitpunkt::neu(d, 12, 0, 0)?;
    Zeitpunkt::aus_sekunden(z.sekunden().checked_sub(86_400)?)?.datum()
}

fn ortsdatum(z: Zeitpunkt) -> Option<Datum> {
    ortszeit(z).map(|(d, ..)| d)
}

/// Ein Befund der Kursprüfung in Worten. Im Journal steht er als Rust-Name
/// (`KursZuAlt(...)`); hier zaehlt nur die Art.
fn befund_text(roh: &str) -> &'static str {
    let art = roh.split(['(', ' ', '{']).next().unwrap_or_default();
    match art {
        "KursZuAlt" => {
            "Kurse zu alt: mehr als 20 Minuten zwischen dem Schluss der Börse und dem Abruf (etwa ein \
             nachgeholter Abruf, weil der Rechner zur Abrufzeit schlief)"
        }
        "KursFehlt" => "Kurs fehlt",
        "KursNichtPositiv" => "Kurs nicht positiv",
        "Kurssprung" => "Kurssprung über der Grenze",
        "KeinVergleichskurs" => "kein Vortagskurs zum Vergleich",
        "AbrufNichtAmHandelstag" => "Abruf nicht am Handelstag",
        "KursNichtVomBezugstag" => "Kurs stammt nicht vom Bezugstag",
        "BezugstagUnbekannt" => "Börsenkalender für den Bezugstag fehlt",
        "KursNichtAmHandelstag" => "Kurs nicht vom Handelstag",
        "VergleichskursNichtVorher" => "Vergleichskurs nicht von einem früheren Tag",
        "VergleichsabrufNachAbruf" => "Vergleichsabruf nach dem Abruf",
        "KeinWertHandelbar" => "kein Wert mit gültigem Kurs",
        _ => "sonstiger Befund",
    }
}

/// Die Befunde der Kursprüfung in Worten, jede Art einmal, mit Anzahl.
pub fn befunde_text<'a>(roh: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut arten: Vec<(&'static str, usize)> = Vec::new();
    for r in roh {
        let t = befund_text(r);
        match arten.iter_mut().find(|(a, _)| *a == t) {
            Some((_, n)) => *n = n.saturating_add(1),
            None => arten.push((t, 1)),
        }
    }
    arten
        .into_iter()
        .map(|(t, n)| {
            if n > 1 {
                format!("{n}× {t}")
            } else {
                t.to_string()
            }
        })
        .collect()
}

fn ergebnis_text(e: Ergebnis) -> &'static str {
    match e {
        Ergebnis::Ok => "geantwortet",
        Ergebnis::Limit => "Anbieterlimit erreicht",
        Ergebnis::Guthaben => "Guthaben beim Anbieter fehlte",
        Ergebnis::Abgelehnt => "Schlüssel vom Anbieter abgelehnt",
        Ergebnis::Ueberlastet => "Anbieter überlastet oder gestört",
        Ergebnis::Fehler => "Anbieter meldete einen Fehler",
        Ergebnis::Netz => "keine Verbindung",
        Ergebnis::Unklar => "Antwort fehlte, Kosten unbekannt",
        Ergebnis::Kosten => "Kosten über der Reservierung, Antwort verworfen",
        Ergebnis::Neu => "Schlüssel neu hinterlegt",
    }
}

/// Was eine KI an einem Tag beigetragen hat. Ob sie beigetragen hat, sagt allein das Journal:
/// nur eine Antwort, die der Papiertag verwendet hat (`depot_sim::gefragt`), zaehlt. Das
/// Anbieterprotokoll erklaert nur, warum eine Antwort fehlte; ein erfolgreicher Aufruf dort
/// macht nichts gruen, denn er kann aus einem anderen Lauf stammen oder spaeter verworfen worden
/// sein (Maus, Befund 2 zu PR #33).
fn beitrag(begruendungen: &[&str], aufrufe: &[&Eintrag]) -> KiBeitrag {
    let gefragt: Vec<Gefragt> = begruendungen.iter().filter_map(|t| gefragt(t)).collect();
    let verwendet = gefragt.iter().filter(|g| **g == Gefragt::Verwendet).count();
    let verworfen = gefragt.iter().filter(|g| **g == Gefragt::Verworfen).count();
    let mut fehl: Vec<&str> = Vec::new();
    for e in aufrufe {
        if e.ergebnis != Ergebnis::Ok && e.ergebnis != Ergebnis::Neu {
            let t = ergebnis_text(e.ergebnis);
            if !fehl.contains(&t) {
                fehl.push(t);
            }
        }
    }
    let grund = || {
        if fehl.is_empty() {
            "Antwort verworfen (unlesbar oder nicht in der verlangten Form)".to_string()
        } else {
            fehl.join(", ")
        }
    };
    if verwendet > 0 {
        let mut text = format!(
            "hat beigetragen ({verwendet} {} verwendet)",
            if verwendet == 1 {
                "Antwort"
            } else {
                "Antworten"
            }
        );
        if verworfen > 0 {
            let _ = write!(text, "; {verworfen} verworfen: {}", grund());
        }
        return KiBeitrag {
            klasse: "gut",
            text,
        };
    }
    if verworfen > 0 {
        return KiBeitrag {
            klasse: "halt",
            text: format!("ausgefallen, gefragt ohne verwendbare Antwort: {}", grund()),
        };
    }
    if let Some(g) = gefragt.iter().find_map(|g| match g {
        Gefragt::NichtAngebunden(g) => Some(g),
        _ => None,
    }) {
        return KiBeitrag {
            klasse: "halt",
            text: format!(
                "ausgefallen, nicht gefragt: {}",
                g.as_deref()
                    .unwrap_or("kein Schlüssel, Sperre oder Anbieterpause")
            ),
        };
    }
    match begruendungen.first() {
        Some(t) => KiBeitrag {
            klasse: "offen",
            text: format!("kein verwendeter Beitrag im Journal: {t}"),
        },
        None => KiBeitrag {
            klasse: "offen",
            text: "nicht gefragt".into(),
        },
    }
}

/// Die Befunde der Kurspruefung, an denen ein Papiertag am Tag `d` mit den bis dahin
/// abgerufenen Kursen ausgefallen waere, in Worten. Pflicht sind die Werte, die heute gehalten
/// werden. `None` ohne Kurse dieses Tages oder ohne Befund.
fn kurspruefung_am(stand: &Stand, universum: &Universum, d: Datum) -> Option<Vec<String>> {
    let k = tageskurse_am(stand, universum, d).ok()?;
    let wahl = waehle_tageswerte(
        &Grenzen::standard(),
        universum,
        &stand.gehalten(),
        d,
        &k.heute,
        &k.vortag,
    );
    if wahl.fehler.is_empty() {
        return None;
    }
    let befunde: Vec<_> = wahl
        .fehler
        .iter()
        .chain(&wahl.ausgelassen)
        .filter(|b| **b != depot_regelwerk::DatenFehler::KeinWertHandelbar)
        .collect();
    if befunde.is_empty() {
        return None;
    }
    let roh: Vec<String> = befunde.iter().map(|b| format!("{b:?}")).collect();
    Some(befunde_text(roh.iter().map(String::as_str)))
}

/// Die letzten Handelstage seit dem ersten Papiertag, neueste zuerst.
pub fn tage(
    stand: &Stand,
    universum: &Universum,
    protokoll: &[Eintrag],
    heute: Datum,
) -> Vec<TagErgebnis> {
    tage_bis(stand, universum, protokoll, heute, TAGE_MAX)
}

pub(crate) fn tage_bis(
    stand: &Stand,
    universum: &Universum,
    protokoll: &[Eintrag],
    heute: Datum,
    max: usize,
) -> Vec<TagErgebnis> {
    let begr = stand.begruendungen();
    let gelaufen: BTreeSet<Datum> = begr
        .iter()
        .filter(|(_, wer, _)| wer == PAPIERTAG)
        .map(|(d, _, _)| *d)
        .collect();
    let Some(erster) = gelaufen.first().copied() else {
        return Vec::new();
    };
    let mut tage = Vec::new();
    let mut d = heute;
    while d >= erster && tage.len() < max {
        if werktag(d) {
            tage.push(match liste::feiertag(d) {
                Some(grund) => TagErgebnis {
                    datum: d,
                    stand: TagStand::Geschlossen(grund),
                    ki: Vec::new(),
                },
                None => ein_tag(stand, universum, protokoll, d, heute),
            });
        }
        let Some(v) = tag_davor(d) else {
            break;
        };
        d = v;
    }
    tage
}

fn ein_tag(
    stand: &Stand,
    universum: &Universum,
    protokoll: &[Eintrag],
    d: Datum,
    heute: Datum,
) -> TagErgebnis {
    let begr: Vec<(&str, &str)> = stand
        .begruendungen()
        .iter()
        .filter(|(datum, _, _)| *datum == d)
        .map(|(_, wer, text)| (wer.as_str(), text.as_str()))
        .collect();
    let abschluss = stand.abschluss().map(|a| a.datum);
    if !begr.iter().any(|(wer, _)| *wer == PAPIERTAG) {
        let stand = if d == heute {
            TagStand::Offen
        } else if let Some(befunde) = kurspruefung_am(stand, universum, d) {
            TagStand::Ausgefallen(format!(
                "Die Kursprüfung ließ den Tag ausfallen, darum lief kein Papiertag: {}",
                befunde.join("; ")
            ))
        } else if stand.kurse().iter().any(|k| k.kurs().handelstag() == d) {
            TagStand::Ausgefallen(
                "Schlusskurse liegen vor, aber der Papiertag lief an dem Tag nicht (Rechner \
                 aus, Zeitplan aus oder Lauf gescheitert, siehe Tätigkeit). Nachholen geht \
                 nicht: Ein Tag wird nur mit seinen eigenen Kursen am selben Tag entschieden."
                    .into(),
            )
        } else {
            TagStand::Ausgefallen(
                "keine Schlusskurse für diesen Tag (Rechner aus oder Zeitplan aus zur \
                 Abrufzeit, oder Abruf gescheitert)"
                    .into(),
            )
        };
        return TagErgebnis {
            datum: d,
            stand,
            ki: Vec::new(),
        };
    }
    let daten: Vec<&str> = begr
        .iter()
        .filter(|(wer, text)| *wer == DATEN && !text.starts_with("Zyklus faellt aus"))
        .map(|(_, text)| *text)
        .collect();
    let ausgelassen: Vec<&str> = begr
        .iter()
        .filter(|(wer, _)| *wer == AUSWAHL)
        .map(|(_, text)| text.strip_prefix("heute nicht handelbar: ").unwrap_or(text))
        .collect();
    let stand_tag = if abschluss.is_some_and(|a| a >= d) {
        if daten.is_empty() && ausgelassen.is_empty() {
            TagStand::Abgeschlossen
        } else if daten.is_empty() {
            TagStand::MitAuslassung(befunde_text(ausgelassen.into_iter()))
        } else if daten.iter().all(|t| t.starts_with("KeinWertHandelbar")) {
            // Kein Kandidat handelbar: Die Gruende stehen bei den ausgelassenen.
            TagStand::OhneHandel(befunde_text(ausgelassen.into_iter()))
        } else {
            TagStand::OhneHandel(befunde_text(daten.into_iter()))
        }
    } else {
        TagStand::Angefangen
    };
    let ki = [BeraterId::Dachs, BeraterId::Maus]
        .into_iter()
        .map(|b| {
            let name = match b {
                BeraterId::Dachs => "Dachs",
                BeraterId::Maus => "Maus",
            };
            let texte: Vec<&str> = begr
                .iter()
                .filter(|(wer, _)| *wer == name)
                .map(|(_, t)| *t)
                .collect();
            let wer: Wer = wer_fuer(b);
            let aufrufe: Vec<&Eintrag> = protokoll
                .iter()
                .filter(|e| e.wer == wer && ortsdatum(e.zeit) == Some(d))
                .collect();
            let beitrag = if matches!(stand_tag, TagStand::OhneHandel(_)) && texte.is_empty() {
                KiBeitrag {
                    klasse: "offen",
                    text: "nicht gefragt: Die Kursprüfung ließ den Tag ausfallen".into(),
                }
            } else {
                beitrag(&texte, &aufrufe)
            };
            (b, beitrag)
        })
        .collect();
    TagErgebnis {
        datum: d,
        stand: stand_tag,
        ki,
    }
}
