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

/// Absender der Kaufbelege (#325): `"{auftrag} {Berater} {n},{m}"`, Nummern der Beobachtungen
/// des Kauftags als Tagesnummern ab 1.
pub const KAUFBELEG: &str = "Kaufbeleg";

/// Ein Kauf mit den Ereignissen, auf die die KI ihn gestützt hat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kaufbeleg {
    pub datum: Datum,
    pub auftrag: u64,
    pub berater: BeraterId,
    /// Einsbasierte Tagesnummern im Journal.
    pub nummern: Vec<usize>,
}

/// Alle Kaufbelege im Journal. Unlesbare Zeilen entfallen; ältere Journale haben keine.
pub fn kaufbelege(stand: &Stand) -> Vec<Kaufbeleg> {
    stand
        .begruendungen()
        .iter()
        .filter(|(_, wer, _)| wer == KAUFBELEG)
        .filter_map(|(datum, _, text)| {
            let mut teile = text.split(' ');
            let auftrag = teile.next()?.parse().ok()?;
            let berater = match teile.next()? {
                "Dachs" => BeraterId::Dachs,
                "Maus" => BeraterId::Maus,
                _ => return None,
            };
            let nummern = teile
                .next()?
                .split(',')
                .map(|n| n.parse::<usize>().ok().filter(|n| *n > 0))
                .collect::<Option<Vec<_>>>()?;
            teile.next().is_none().then_some(Kaufbeleg {
                datum: *datum,
                auftrag,
                berater,
                nummern,
            })
        })
        .collect()
}

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
    /// Käufe mit genannten Belegen (#325, Beleg↔These).
    pub kauf_faelle: u64,
    /// Davon stand mindestens einer der genannten Belege unter den ersten drei.
    pub kauf_unter_drei: u64,
}

pub(crate) const FRAGELISTE: &str = "Frageliste";

/// Start und Ende eines Frageabschnitts als nullbasierte Tagesindizes.
fn abschnitte(stand: &Stand, tag: Datum) -> impl Iterator<Item = std::ops::Range<usize>> + '_ {
    stand
        .begruendungen()
        .iter()
        .filter_map(move |(datum, wer, text)| {
            if *datum != tag || wer != FRAGELISTE {
                return None;
            }
            let (start, anzahl) = text.split_once(' ')?;
            let start: usize = start.parse().ok()?;
            Some(start..start.checked_add(anzahl.parse().ok()?)?)
        })
}

fn beobachtungen_am(stand: &Stand, tag: Datum) -> impl Iterator<Item = &String> {
    stand
        .beobachtungen()
        .iter()
        .filter(move |(datum, _)| *datum == tag)
        .map(|(_, text)| text)
}

/// Übersetzt eine Fragennummer nach Textabgleich in die übliche Journalnummer
/// des Tages. Ältere Leser können damit dieselbe Beobachtung auflösen.
pub(crate) fn journalnummer(
    stand: &Stand,
    tag: Datum,
    frageindex: usize,
    text: &str,
) -> Option<usize> {
    let index = match abschnitte(stand, tag).last() {
        Some(abschnitt) => {
            let index = abschnitt.start.checked_add(frageindex)?;
            abschnitt.contains(&index).then_some(index)?
        }
        None => frageindex,
    };
    (beobachtungen_am(stand, tag).nth(index)? == text)
        .then(|| index.checked_add(1))
        .flatten()
}

fn abschnitt_fuer(stand: &Stand, tag: Datum, nummern: &[usize]) -> Option<std::ops::Range<usize>> {
    let mut bekannt = false;
    for abschnitt in abschnitte(stand, tag) {
        bekannt = true;
        if nummern
            .iter()
            .any(|n| n.checked_sub(1).is_some_and(|i| abschnitt.contains(&i)))
        {
            return Some(abschnitt);
        }
    }
    (!bekannt).then(|| 0..beobachtungen_am(stand, tag).count())
}

fn kandidaten_im(stand: &Stand, tag: Datum, abschnitt: std::ops::Range<usize>) -> Vec<String> {
    beobachtungen_am(stand, tag)
        .skip(abschnitt.start)
        .take(abschnitt.len())
        .cloned()
        .collect()
}

/// Kandidaten des letzten belegten Frageabschnitts. Altstände nutzen die Tagesliste.
pub fn kandidaten_am(stand: &Stand, tag: Datum) -> Vec<String> {
    abschnitte(stand, tag).last().map_or_else(
        || beobachtungen_am(stand, tag).cloned().collect(),
        |abschnitt| kandidaten_im(stand, tag, abschnitt),
    )
}

/// Zahl der belegten Kandidaten über alle Frageabschnitte eines Tages.
pub fn kandidatenanzahl(stand: &Stand, tag: Datum) -> usize {
    let beobachtungen = beobachtungen_am(stand, tag).count();
    let mut abschnitte = abschnitte(stand, tag).peekable();
    if abschnitte.peek().is_none() {
        return beobachtungen;
    }
    abschnitte.fold(0_usize, |summe, a| {
        summe.saturating_add(a.end.min(beobachtungen).saturating_sub(a.start))
    })
}

/// Mindestens eine Tagesnummer verweist auf einen Kandidaten der Frage.
pub fn hat_fragebeleg(stand: &Stand, tag: Datum, nummern: &[usize]) -> bool {
    let beobachtungen = beobachtungen_am(stand, tag).count();
    abschnitt_fuer(stand, tag, nummern).is_some_and(|a| {
        nummern.iter().any(|n| {
            n.checked_sub(1)
                .is_some_and(|i| i < beobachtungen && a.contains(&i))
        })
    })
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
        let nummer = p.prognose.ereignis_nr;
        let Some(abschnitt) = abschnitt_fuer(stand, p.datum, &[nummer]) else {
            continue;
        };
        let start = abschnitt.start;
        let kandidaten = kandidaten_im(stand, p.datum, abschnitt);
        let Some(gewaehlt) = nummer
            .checked_sub(1)
            .and_then(|n| n.checked_sub(start))
            .filter(|i| *i < kandidaten.len())
        else {
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
    for k in kaufbelege(stand)
        .iter()
        .filter(|k| zeitraum.contains(&k.datum))
    {
        let Some(abschnitt) = abschnitt_fuer(stand, k.datum, &k.nummern) else {
            continue;
        };
        let start = abschnitt.start;
        let kandidaten = kandidaten_im(stand, k.datum, abschnitt);
        let thesen = thesen_vor(stand, universum, k.berater, k.datum);
        let oben: Vec<usize> = sortierer
            .ordne(&kandidaten, &thesen)
            .into_iter()
            .take(3)
            .collect();
        m.kauf_faelle = m.kauf_faelle.saturating_add(1);
        if k.nummern
            .iter()
            .filter_map(|n| n.checked_sub(1))
            .filter_map(|i| i.checked_sub(start))
            .any(|i| oben.contains(&i))
        {
            m.kauf_unter_drei = m.kauf_unter_drei.saturating_add(1);
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
