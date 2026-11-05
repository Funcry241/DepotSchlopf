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
use depot_kern::{BeraterId, Datum, Zeitpunkt};

use crate::betrieb::wer_fuer;
use crate::ortszeit;

/// So viele Handelstage zeigt das Lagebild hoechstens.
pub const TAGE_MAX: usize = 10;

/// Wie ein Handelstag ausging.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TagStand {
    /// Der Papiertag lief und ist abgeschlossen.
    Abgeschlossen,
    /// Abgeschlossen, aber die Kursprüfung liess den Zyklus ausfallen: niemand gefragt, kein
    /// Auftrag. Mit den Befunden in Worten.
    OhneHandel(Vec<String>),
    /// Begonnen und nicht abgeschlossen (das Lagebild zeigt den Halt).
    Angefangen,
    /// Kein Papiertag an diesem Tag, mit Grund.
    Ausgefallen(String),
    /// Heute, noch nicht gelaufen.
    Offen,
}

impl TagStand {
    pub fn klasse(&self) -> &'static str {
        match self {
            TagStand::Abgeschlossen => "gut",
            TagStand::OhneHandel(_) | TagStand::Offen => "offen",
            TagStand::Angefangen | TagStand::Ausgefallen(_) => "halt",
        }
    }

    pub fn text(&self) -> String {
        match self {
            TagStand::Abgeschlossen => "Papiertag abgeschlossen".into(),
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

fn handelstag(d: Datum) -> bool {
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
            "Kurse zu alt: mehr als 20 Minuten zwischen Xetra-Schluss und Abruf (etwa ein \
             nachgeholter Abruf, weil der Rechner zur Abrufzeit schlief)"
        }
        "KursFehlt" => "Kurs fehlt",
        "KursNichtPositiv" => "Kurs nicht positiv",
        "Kurssprung" => "Kurssprung über der Grenze",
        "KeinVergleichskurs" => "kein Vortagskurs zum Vergleich",
        "AbrufNichtAmHandelstag" => "Abruf nicht am Handelstag",
        "KursNichtAmHandelstag" => "Kurs nicht vom Handelstag",
        "VergleichskursNichtVorher" => "Vergleichskurs nicht von einem früheren Tag",
        "VergleichsabrufNachAbruf" => "Vergleichsabruf nach dem Abruf",
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

/// Was eine KI an einem Tag beigetragen hat: aus ihren Begruendungen im Journal und ihren
/// Aufrufen im Anbieterprotokoll.
fn beitrag(begruendungen: &[&str], aufrufe: &[&Eintrag]) -> KiBeitrag {
    let ok = aufrufe
        .iter()
        .filter(|e| e.ergebnis == Ergebnis::Ok)
        .count();
    let fehl: Vec<&str> = aufrufe
        .iter()
        .filter(|e| e.ergebnis != Ergebnis::Ok && e.ergebnis != Ergebnis::Neu)
        .map(|e| ergebnis_text(e.ergebnis))
        .collect();
    let fehl_text = || {
        let mut f: Vec<&str> = Vec::new();
        for t in &fehl {
            if !f.contains(t) {
                f.push(t);
            }
        }
        f.join(", ")
    };
    if ok > 0 {
        let mut text = format!(
            "hat beigetragen ({ok} {})",
            if ok == 1 { "Antwort" } else { "Antworten" }
        );
        if !fehl.is_empty() {
            let _ = write!(text, "; außerdem: {}", fehl_text());
        }
        return KiBeitrag {
            klasse: "gut",
            text,
        };
    }
    if let Some(n) = begruendungen
        .iter()
        .find(|t| t.contains("nicht angebunden"))
    {
        let grund = n
            .split_once("nicht angebunden (")
            .and_then(|(_, r)| r.split_once(')'))
            .map_or("kein Schlüssel, Sperre oder Anbieterpause", |(g, _)| g);
        return KiBeitrag {
            klasse: "halt",
            text: format!("ausgefallen, nicht gefragt: {grund}"),
        };
    }
    if !fehl.is_empty() {
        return KiBeitrag {
            klasse: "halt",
            text: format!("ausgefallen: {}", fehl_text()),
        };
    }
    match begruendungen.first() {
        Some(t) => KiBeitrag {
            klasse: "offen",
            text: format!("ohne verwertbare Antwort: {t}"),
        },
        None => KiBeitrag {
            klasse: "offen",
            text: "nicht gefragt".into(),
        },
    }
}

/// Die letzten Handelstage seit dem ersten Papiertag, neueste zuerst.
pub fn tage(stand: &Stand, protokoll: &[Eintrag], heute: Datum) -> Vec<TagErgebnis> {
    let begr = stand.begruendungen();
    let gelaufen: BTreeSet<Datum> = begr
        .iter()
        .filter(|(_, wer, _)| wer == PAPIERTAG)
        .map(|(d, _, _)| *d)
        .collect();
    let Some(erster) = gelaufen.first().copied() else {
        return Vec::new();
    };
    let kurse_da: BTreeSet<Datum> = stand
        .kurse()
        .iter()
        .map(|k| k.kurs().handelstag())
        .collect();
    let abschluss = stand.abschluss().map(|a| a.datum);
    let mut tage = Vec::new();
    let mut d = heute;
    while d >= erster && tage.len() < TAGE_MAX {
        if handelstag(d) {
            tage.push(ein_tag(
                stand, protokoll, d, heute, &gelaufen, &kurse_da, abschluss,
            ));
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
    protokoll: &[Eintrag],
    d: Datum,
    heute: Datum,
    gelaufen: &BTreeSet<Datum>,
    kurse_da: &BTreeSet<Datum>,
    abschluss: Option<Datum>,
) -> TagErgebnis {
    let begr: Vec<(&str, &str)> = stand
        .begruendungen()
        .iter()
        .filter(|(datum, _, _)| *datum == d)
        .map(|(_, wer, text)| (wer.as_str(), text.as_str()))
        .collect();
    if !gelaufen.contains(&d) {
        let stand = if d == heute {
            TagStand::Offen
        } else if kurse_da.contains(&d) {
            TagStand::Ausgefallen(
                "Schlusskurse liegen vor, aber der Papiertag lief an dem Tag nicht (Rechner \
                 aus, Zeitplan aus oder Lauf gescheitert, siehe Tätigkeit). Nachholen geht \
                 nicht: Ein Tag wird nur mit seinen eigenen Kursen am selben Tag entschieden."
                    .into(),
            )
        } else {
            TagStand::Ausgefallen(
                "keine Schlusskurse für diesen Tag (Rechner aus oder Zeitplan aus zur \
                 Abrufzeit, Abruf gescheitert, oder Börsenfeiertag)"
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
    let stand_tag = if abschluss.is_some_and(|a| a >= d) {
        if daten.is_empty() {
            TagStand::Abgeschlossen
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
            let beitrag = if matches!(stand_tag, TagStand::OhneHandel(_)) && aufrufe.is_empty() {
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
