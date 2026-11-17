//! Rangfolge der Belege für eine KI-Frage (#325, Vorbereitung NeuroSchlopf).
//!
//! Ein [`Sortierer`] ordnet die Belege eines Stichtags danach, wie wichtig sie für die
//! bestehenden Thesen sind. Der [`Grundrang`] kommt ohne Lernen aus: Wertbezug, Termin,
//! Aktualität. Er ist der Maßstab für jeden späteren Sortierer (Gemini, NeuroSchlopf). Ein
//! lernender Sortierer wird erst verwendet, wenn er den Grundrang in [`messe`] schlägt.
//!
//! Nur lesend: Das Journal ändert sich nicht, die Frage an Dachs und Maus ebenfalls nicht.

use std::collections::BTreeSet;

use depot_journal::Stand;
use depot_kern::{BeraterId, Datum, Isin, Universum};

/// Eine bestehende These: ein Wert, den der Berater vor dem Stichtag genannt hat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct These {
    pub isin: Isin,
    /// Name aus der Werteliste, falls bekannt (Belege nennen meist den Namen, nicht die ISIN).
    pub name: Option<String>,
}

/// Ordnet Belege. Liefert die Indizes der Kandidaten, wichtigste zuerst, jeden genau einmal.
pub trait Sortierer {
    fn name(&self) -> &'static str;
    fn ordne(&self, kandidaten: &[String], thesen: &[These]) -> Vec<usize>;
}

/// Die Reihenfolge, in der die Frage die Belege heute zeigt.
#[derive(Clone, Copy, Debug)]
pub struct Heute;

impl Sortierer for Heute {
    fn name(&self) -> &'static str {
        "heutige Reihenfolge"
    }
    fn ordne(&self, kandidaten: &[String], _: &[These]) -> Vec<usize> {
        (0..kandidaten.len()).collect()
    }
}

/// Einfache Rangfolge ohne Lernen. Punkte: Wertbezug 4 je genannter These (höchstens 8),
/// Termin 2, sonst 0. Bei Gleichstand bleibt die heutige Reihenfolge, die bei Nachrichten
/// schon die neueste zuerst nennt (Aktualität).
#[derive(Clone, Copy, Debug)]
pub struct Grundrang;

impl Grundrang {
    fn punkte(beleg: &str, thesen: &[These]) -> u32 {
        let klein = beleg.to_lowercase();
        let bezug = thesen
            .iter()
            .filter(|t| {
                klein.contains(&t.isin.als_str().to_lowercase())
                    || t.name
                        .as_deref()
                        .and_then(kernname)
                        .is_some_and(|n| klein.contains(&n))
            })
            .count();
        let bezug = u32::try_from(bezug.min(2)).unwrap_or(2).saturating_mul(4);
        let termin = if beleg.starts_with("Termin ") { 2 } else { 0 };
        bezug.saturating_add(termin)
    }
}

impl Sortierer for Grundrang {
    fn name(&self) -> &'static str {
        "Grundrang"
    }
    fn ordne(&self, kandidaten: &[String], thesen: &[These]) -> Vec<usize> {
        let mut rang: Vec<usize> = (0..kandidaten.len()).collect();
        // Stabil: Gleichstand behält die heutige Reihenfolge.
        rang.sort_by_key(|i| {
            std::cmp::Reverse(kandidaten.get(*i).map_or(0, |b| Self::punkte(b, thesen)))
        });
        rang
    }
}

/// Erstes prägendes Wort eines Namens, klein ("SAP SE" → "sap", "Deutsche Telekom AG" →
/// "deutsche telekom"). Rechtsformen und zu kurze Namen zählen nicht.
fn kernname(name: &str) -> Option<String> {
    const FORMEN: [&str; 9] = [
        "ag", "se", "inc", "inc.", "corp", "corp.", "plc", "nv", "sa",
    ];
    let teile: Vec<String> = name
        .split_whitespace()
        .map(str::to_lowercase)
        .filter(|t| !FORMEN.contains(&t.as_str()))
        .take(2)
        .collect();
    let kern = match teile.as_slice() {
        [a, b] if a.chars().count() < 5 || ["deutsche", "the"].contains(&a.as_str()) => {
            format!("{a} {b}")
        }
        [a, ..] => a.clone(),
        [] => return None,
    };
    (kern.chars().count() >= 3).then_some(kern)
}

/// Thesen eines Beraters zum Stichtag: alle Werte, die er in den fünf Kalendertagen davor
/// mit einem Zielgewicht über 0 genannt hat. Der Stichtag selbst zählt nicht, sonst spräche
/// die Antwort des Tages für sich selbst.
pub fn thesen_vor(stand: &Stand, universum: &Universum, wer: BeraterId, tag: Datum) -> Vec<These> {
    let name = match wer {
        BeraterId::Dachs => "Dachs",
        BeraterId::Maus => "Maus",
    };
    let fenster: BTreeSet<Datum> = (-5..=-1).filter_map(|n: i64| tag.plus_tage(n)).collect();
    let mut isins = BTreeSet::new();
    for (d, w, text) in stand.begruendungen() {
        if w != name || !fenster.contains(d) {
            continue;
        }
        let Some((_, rest)) = text.split_once(": ") else {
            continue;
        };
        if let Some((kopf, _)) = rest.split_once(" bp: ")
            && let Some((isin, bp)) = kopf.split_once(' ')
            && bp.parse::<i64>().is_ok_and(|bp| bp > 0)
            && let Ok(isin) = Isin::neu(isin)
        {
            isins.insert(isin);
        }
    }
    isins
        .into_iter()
        .map(|isin| These {
            name: universum
                .instrumente()
                .iter()
                .find(|i| i.isin == isin)
                .map(|i| i.name.clone()),
            isin,
        })
        .collect()
}

/// Ergebnis einer Messung über alle Weltprognosen im Journal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Messung {
    /// Weltprognosen im Journal.
    pub faelle: u64,
    /// Davon stand der gestützte Beleg unter den ersten drei.
    pub unter_drei: u64,
}

/// Die Belege, die eine Frage an diesem Tag gesehen hat, in ihrer Reihenfolge. Der Papiertag
/// vermerkt sie vor der ersten Frage als Beobachtungen (`lauf::ein_tag`); die Liste ist also
/// zum Stichtag reproduzierbar, ohne dass das Journal etwas Neues speichern muss.
pub fn kandidaten_am(stand: &Stand, tag: Datum) -> Vec<String> {
    stand
        .beobachtungen()
        .iter()
        .filter(|(d, _)| *d == tag)
        .map(|(_, t)| t.clone())
        .collect()
}

/// Wie oft steht der Beleg, auf den eine KI ihre Weltprognose gestützt hat, beim Sortierer
/// unter den ersten drei? Das Journal prüft beim Schreiben, dass `ereignis_nr` genau auf
/// diesen Beleg des Tages zeigt. Gezählt werden Prognosetage in `zeitraum` (einschließlich).
pub fn messe(
    stand: &Stand,
    universum: &Universum,
    sortierer: &dyn Sortierer,
    zeitraum: std::ops::RangeInclusive<Datum>,
) -> Messung {
    let mut m = Messung::default();
    for p in stand
        .weltprognosen()
        .iter()
        .filter(|p| zeitraum.contains(&p.datum))
    {
        let kandidaten = kandidaten_am(stand, p.datum);
        let Some(gewaehlt) = p.prognose.ereignis_nr.checked_sub(1) else {
            continue;
        };
        m.faelle = m.faelle.saturating_add(1);
        let thesen = thesen_vor(stand, universum, p.berater, p.datum);
        if sortierer
            .ordne(&kandidaten, &thesen)
            .iter()
            .take(3)
            .any(|i| *i == gewaehlt)
        {
            m.unter_drei = m.unter_drei.saturating_add(1);
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn these(isin: &str, name: &str) -> Result<These, String> {
        Ok(These {
            isin: Isin::neu(isin).map_err(|e| format!("{e:?}"))?,
            name: Some(name.into()),
        })
    }

    fn texte(t: &[&str]) -> Vec<String> {
        t.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn heute_behaelt_die_reihenfolge() {
        assert_eq!(Heute.ordne(&texte(&["a", "b", "c"]), &[]), vec![0, 1, 2]);
    }

    /// Ein Beleg zum gehaltenen Wert rückt vor neuere fremde Meldungen, ein Termin vor eine
    /// Meldung ohne Bezug. Gleichstand behält die Reihenfolge (neueste zuerst).
    #[test]
    fn grundrang_bevorzugt_wertbezug_dann_termin() -> Result<(), String> {
        let thesen = [these("DE0007164600", "SAP SE")?];
        let k = texte(&[
            "Nachricht: Ölpreis steigt",
            "Nachricht: Autobauer im Minus",
            "Termin 18.11.: Zinsentscheid (bestätigt)",
            "Nachricht: SAP hebt Prognose an",
        ]);
        assert_eq!(Grundrang.ordne(&k, &thesen), vec![3, 2, 0, 1]);
        Ok(())
    }

    /// Jeder Index genau einmal, auch ohne Thesen und bei leerer Liste.
    #[test]
    fn ordnung_ist_eine_vertauschung() -> Result<(), String> {
        let thesen = [these("DE0007164600", "SAP SE")?];
        for n in 0..12_usize {
            let k: Vec<String> = (0..n)
                .map(|i| {
                    if i % 3 == 0 {
                        format!("Termin {i}")
                    } else {
                        format!("SAP {i}")
                    }
                })
                .collect();
            for t in [&thesen[..], &[]] {
                let mut r = Grundrang.ordne(&k, t);
                r.sort_unstable();
                assert_eq!(r, (0..n).collect::<Vec<_>>());
            }
        }
        Ok(())
    }

    /// Die ISIN im Text zählt wie der Name; Rechtsformen allein nie („SE“ träfe sonst fast
    /// jede Meldung).
    #[test]
    fn kernname_und_isin() -> Result<(), String> {
        assert_eq!(kernname("SAP SE").as_deref(), Some("sap"));
        assert_eq!(
            kernname("Deutsche Telekom AG").as_deref(),
            Some("deutsche telekom")
        );
        assert_eq!(kernname("SE"), None);
        let thesen = [these("DE0007164600", "SE")?];
        assert_eq!(Grundrang::punkte("Meldung zu DE0007164600", &thesen), 4);
        assert_eq!(Grundrang::punkte("Meldung einer SE", &thesen), 0);
        Ok(())
    }
}
