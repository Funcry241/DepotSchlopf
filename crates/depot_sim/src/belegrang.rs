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
use depot_kern::{BeraterId, Datum, Isin, Universum, liste::Wert};

use crate::nachrichtenbezug::Wortsuche;

/// Absender der Kaufbelege (#325): `"{auftrag} {Berater} {n},{m}"`, Nummern der Beobachtungen
/// des Kauftags als Tagesnummern ab 1.
pub const KAUFBELEG: &str = "Kaufbeleg";
/// Journalzweck für verworfene Belege zur NeuroSchlopf-Messung.
pub const BELEGPRUEFUNG: &str = "Belegprüfung";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verwerfungsgrund {
    PrognoseUngueltig,
    PrognoseAusserhalbUniversum,
    PrognoseNichtSichtbar,
    PrognoseOhneKurs,
    ZielNichtSichtbar,
    ZielOhneJournalbeleg,
}

impl Verwerfungsgrund {
    pub fn text(self) -> &'static str {
        match self {
            Self::PrognoseUngueltig => "Prognose unvollständig oder ungültig",
            Self::PrognoseAusserhalbUniversum => "Prognosewert außerhalb der Beobachtungsliste",
            Self::PrognoseNichtSichtbar => "Prognoseereignis nicht in der Frage sichtbar",
            Self::PrognoseOhneKurs => "Prognosewert ohne Kurs",
            Self::ZielNichtSichtbar => "Zielbeleg nicht in der Frage sichtbar",
            Self::ZielOhneJournalbeleg => "Zielbeleg ohne passende Journalbeobachtung",
        }
    }
}

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

/// Journalbegründung für ein belegtes Ziel aus der Erstmeinung.
pub const ZIELBELEG: &str = "Zielbeleg";

/// Liefert je auflösbarem Vorschlagsziel seinen Tag, einmal je Berater und Wert.
pub fn tage_mit_zielbelegen(stand: &Stand) -> impl Iterator<Item = Datum> + '_ {
    let mut gesehen = BTreeSet::new();
    stand
        .begruendungen()
        .iter()
        .filter_map(move |(datum, wer, text)| {
            if wer != ZIELBELEG {
                return None;
            }
            let mut felder = text.split_whitespace();
            let berater = felder.next()?;
            if !matches!(berater, "Dachs" | "Maus") {
                return None;
            }
            let isin = Isin::neu(felder.next()?).ok()?;
            let gewicht = felder.next()?.parse::<i64>().ok()?;
            let nummern = felder
                .next()?
                .split(',')
                .map(|n| n.parse::<usize>().ok().filter(|n| *n > 0))
                .collect::<Option<Vec<_>>>()?;
            if felder.next().is_some()
                || !(0..=10_000).contains(&gewicht)
                || !hat_fragebeleg(stand, *datum, &nummern)
            {
                return None;
            }
            gesehen.insert((*datum, berater, isin)).then_some(*datum)
        })
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

/// Einfache Rangfolge ohne Lernen. Punkte: 4 je belegerwähnter ISIN (höchstens 8),
/// Termin 2. Gleichstand behält die bisherige Kandidatenreihenfolge.
#[derive(Clone, Copy, Debug)]
pub struct Grundrang<'a> {
    werte: &'a [Wert],
}

impl<'a> Grundrang<'a> {
    #[must_use]
    pub fn neu(werte: &'a [Wert]) -> Self {
        Self { werte }
    }

    fn beleginhalt(beleg: &str) -> String {
        // Quellenlink und angehängte Wertmarkierungen begründen selbst keinen Bezug.
        let inhalt = beleg.split(" | ").next().unwrap_or(beleg);
        inhalt
            .split(" [betrifft: ")
            .next()
            .unwrap_or(inhalt)
            .to_lowercase()
    }

    fn ist_termin(beleg: &str) -> bool {
        beleg.starts_with("Termin ")
    }

    /// Vorbereitungsformat grundrang-paar-v1: Wertbezug, Termin, einsbasierter Fragenplatz.
    pub fn paarmerkmale(&self, beleg: &str, these: &These, platz: usize) -> [usize; 3] {
        let inhalt = Self::beleginhalt(beleg);
        [
            usize::from(self.trifft(&Wortsuche::neu(&inhalt), these)),
            usize::from(Self::ist_termin(beleg)),
            platz,
        ]
    }

    fn punkte(&self, beleg: &str, thesen: &[These]) -> u32 {
        let inhalt = Self::beleginhalt(beleg);
        let suche = Wortsuche::neu(&inhalt);
        let isins: BTreeSet<&Isin> = thesen
            .iter()
            .filter(|t| self.trifft(&suche, t))
            .map(|t| &t.isin)
            .collect();
        let bezug = u32::try_from(isins.len().min(2))
            .unwrap_or(2)
            .saturating_mul(4);
        let termin = if Self::ist_termin(beleg) { 2 } else { 0 };
        bezug.saturating_add(termin)
    }

    fn trifft(&self, suche: &Wortsuche<'_>, these: &These) -> bool {
        let isin = these.isin.als_str();
        if suche.trifft(isin) {
            return true;
        }
        if isin == "DE0007236101"
            && ["Siemens Energy", "Siemens Healthineers", "Siemens Gamesa"]
                .iter()
                .any(|name| suche.trifft(name))
        {
            return suche.trifft("Siemens AG") || suche.trifft("SIE");
        }
        let alias = match isin {
            "US02079K3059" => "Google",
            "DE0008430026" => "Munich Re",
            "DE0007100000" => "Mercedes",
            "US46625H1005" => "JP Morgan",
            _ => "",
        };
        suche.trifft(alias)
            || self
                .werte
                .iter()
                .find(|w| w.isin == these.isin)
                .map_or_else(
                    || these.name.as_deref().is_some_and(|n| suche.trifft(n)),
                    |wert| suche.wert(wert),
                )
    }
}

impl Sortierer for Grundrang<'_> {
    fn name(&self) -> &'static str {
        "Grundrang v2"
    }
    fn ordne(&self, kandidaten: &[String], thesen: &[These]) -> Vec<usize> {
        let mut rang: Vec<usize> = (0..kandidaten.len()).collect();
        rang.sort_by_cached_key(|i| {
            std::cmp::Reverse(kandidaten.get(*i).map_or(0, |b| self.punkte(b, thesen)))
        });
        rang
    }
}

/// Thesen eines Beraters zum Stichtag: alle Werte, die er in den fünf Kalendertagen davor
/// mit einem Zielgewicht über 0 genannt hat. Der Stichtag selbst zählt nicht, sonst spräche
/// die Antwort des Tages für sich selbst.
pub fn thesen_vor(stand: &Stand, universum: &Universum, wer: BeraterId, tag: Datum) -> Vec<These> {
    thesen_aus(stand.begruendungen(), universum, wer, tag)
}

/// Positive Zielvorschläge aus den fünf vorausgehenden Kalendertagen im übergebenen Journalpräfix.
pub fn thesen_aus(
    begruendungen: &[(Datum, String, String)],
    universum: &Universum,
    wer: BeraterId,
    tag: Datum,
) -> Vec<These> {
    let name = match wer {
        BeraterId::Dachs => "Dachs",
        BeraterId::Maus => "Maus",
    };
    let fenster: BTreeSet<Datum> = (-5..=-1).filter_map(|n: i64| tag.plus_tage(n)).collect();
    let mut isins = BTreeSet::new();
    for (d, w, text) in begruendungen {
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
    /// Prognosefälle mit höchstens drei Kandidaten, bei denen jeder Rang trifft.
    pub prognosen_ohne_rangunterschied: u64,
    /// Käufe mit genannten Belegen (#325, Beleg↔These).
    pub kauf_faelle: u64,
    /// Davon stand mindestens einer der genannten Belege unter den ersten drei.
    pub kauf_unter_drei: u64,
    /// Kauffälle mit höchstens drei Kandidaten, bei denen jeder Rang trifft.
    pub kaeufe_ohne_rangunterschied: u64,
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
            lese_abschnitt(text)
        })
}

fn lese_abschnitt(text: &str) -> Option<std::ops::Range<usize>> {
    let (start, anzahl) = text.split_once(' ')?;
    let start: usize = start.parse().ok()?;
    Some(start..start.checked_add(anzahl.parse().ok()?)?)
}

/// Genau ein vollständig belegter Frageabschnitt. Für Modelldaten sind Tageslisten ohne
/// Marker und Mehrfachläufe ohne Zuordnung ungeeignet.
pub fn eindeutige_frageliste(stand: &Stand, tag: Datum) -> Result<Vec<String>, &'static str> {
    let mut marker = stand
        .begruendungen()
        .iter()
        .filter(|(d, wer, _)| *d == tag && wer == FRAGELISTE);
    let (_, _, text) = marker.next().ok_or("Frageabschnitt fehlt")?;
    if marker.next().is_some() {
        return Err("Frageabschnitt mehrdeutig");
    }
    let abschnitt = lese_abschnitt(text).ok_or("Frageabschnitt ungültig")?;
    if abschnitt.end > beobachtungen_am(stand, tag).count() {
        return Err("Frageabschnitt unvollständig");
    }
    Ok(kandidaten_im(stand, tag, abschnitt))
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
        if kandidaten.len() <= 3 {
            m.prognosen_ohne_rangunterschied = m.prognosen_ohne_rangunterschied.saturating_add(1);
        }
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
        let belege: BTreeSet<_> = k
            .nummern
            .iter()
            .filter_map(|n| n.checked_sub(1))
            .filter_map(|i| i.checked_sub(start))
            .filter(|i| *i < kandidaten.len())
            .collect();
        if belege.is_empty() {
            continue;
        }
        m.kauf_faelle = m.kauf_faelle.saturating_add(1);
        if kandidaten.len() <= 3 {
            m.kaeufe_ohne_rangunterschied = m.kaeufe_ohne_rangunterschied.saturating_add(1);
        }
        let thesen = thesen_vor(stand, universum, k.berater, k.datum);
        if sortierer
            .ordne(&kandidaten, &thesen)
            .iter()
            .take(3)
            .any(|i| belege.contains(i))
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
        let werte = depot_kern::liste::beobachtungsliste()?;
        let rang = Grundrang::neu(&werte);
        let thesen = [these("DE0007164600", "SAP SE")?];
        let k = texte(&[
            "Nachricht: Ölpreis steigt",
            "Nachricht: Autobauer im Minus",
            "Termin 18.11.: Zinsentscheid (bestätigt)",
            "Nachricht: SAP hebt Prognose an",
        ]);
        assert_eq!(rang.ordne(&k, &thesen), vec![3, 2, 0, 1]);
        Ok(())
    }

    /// Jeder Index genau einmal, auch ohne Thesen und bei leerer Liste.
    #[test]
    fn ordnung_ist_eine_vertauschung() -> Result<(), String> {
        let werte = depot_kern::liste::beobachtungsliste()?;
        let rang = Grundrang::neu(&werte);
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
                let mut r = rang.ordne(&k, t);
                r.sort_unstable();
                assert_eq!(r, (0..n).collect::<Vec<_>>());
            }
        }
        Ok(())
    }

    #[test]
    fn grundrang_fehlzuordnungen() -> Result<(), String> {
        let werte = depot_kern::liste::beobachtungsliste()?;
        let rang = Grundrang::neu(&werte);
        for (isin, meldung, punkte) in [
            ("DE0007164600", "Fed will Zinsen asap senken", 0),
            ("US92826C8394", "USA verschärfen Visapflicht", 0),
            ("US0231351067", "Abholzung im Amazonas", 0),
            ("US0378331005", "Pineapple wird teurer", 0),
            ("US4781601046", "Boris Johnson kritisiert die Regierung", 0),
            ("DE0007236101", "Siemens Energy erhält Großauftrag", 0),
            ("DE0007236101", "Siemens Healthineers senkt Margenziel", 0),
            (
                "DE0007236101",
                "Siemens AG und Siemens Energy melden Zahlen",
                4,
            ),
            ("US02079K3059", "Google verliert Kartellverfahren", 4),
            ("DE0008430026", "Munich Re erhöht Dividende", 4),
            ("DE0007100000", "Mercedes ruft Autos zurück", 4),
            ("US46625H1005", "JP Morgan senkt Kursziel", 4),
            ("US1912161007", "Coca Cola erhöht Preise", 4),
            ("DE0007164600", "Meldung zu DE0007164600", 4),
            (
                "DE0007164600",
                "Irrelevant | https://example.org/sap [betrifft: SAP]",
                0,
            ),
            (
                "DE0007236101",
                "Siemens Energy erhält Großauftrag [betrifft: Siemens]",
                0,
            ),
        ] {
            let these = these(isin, isin)?;
            assert_eq!(
                rang.punkte(meldung, &[these.clone(), these]),
                punkte,
                "{meldung}"
            );
        }
        let etfs = [
            these("IE00B4L5Y983", "iShares")?,
            these("IE00B5BMR087", "iShares")?,
            these("IE0008471009", "iShares")?,
        ];
        assert_eq!(
            rang.punkte("BlackRock senkt Gebühren für iShares-ETFs", &etfs),
            0
        );
        Ok(())
    }
}
