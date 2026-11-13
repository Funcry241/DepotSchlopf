//! Historische Depotbewertung aus abgeschlossenen Papiertagen und belegten Kursen.

use std::collections::{BTreeMap, BTreeSet};

use depot_journal::Stand;
use depot_kern::liste::{Wert, als_universum};
use depot_kern::{Cent, Datum, Isin, Rundung, Topf, TopfId, Universum};
use depot_sim::{AUSWAHL, ERSATZKURS, LaufFehler, tageskurse_am};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tagespunkt {
    pub tag: Datum,
    pub wertpapiere: Option<Cent>,
    pub bargeld: Option<Cent>,
    pub gesamt: Option<Cent>,
    pub grund: Option<String>,
    pub kurstage: Vec<Datum>,
    pub kursbezug: BTreeMap<Isin, Datum>,
    pub ersatz: Vec<Isin>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zeitraum {
    Monat,
    DreiMonate,
    Jahr,
    Alles,
}

impl Zeitraum {
    pub fn aus_abfrage(text: &str) -> Self {
        match text {
            "1m" => Self::Monat,
            "3m" => Self::DreiMonate,
            "1j" => Self::Jahr,
            _ => Self::Alles,
        }
    }

    pub fn als_abfrage(self) -> &'static str {
        match self {
            Self::Monat => "1m",
            Self::DreiMonate => "3m",
            Self::Jahr => "1j",
            Self::Alles => "alles",
        }
    }

    /// Erster Tag, einschließlich. Fehlt der Kalendertag, gilt das Monatsende.
    pub fn ab(self, heute: Datum) -> Option<Datum> {
        let monate = match self {
            Self::Monat => 1,
            Self::DreiMonate => 3,
            Self::Jahr => 12,
            Self::Alles => return None,
        };
        let (jahr, monat) = match heute.monat().checked_sub(monate) {
            Some(m) if m >= 1 => (heute.jahr(), m),
            _ => (
                heute.jahr().checked_sub(1)?,
                heute.monat().checked_add(12)?.checked_sub(monate)?,
            ),
        };
        (1..=heute.tag())
            .rev()
            .find_map(|t| Datum::neu(jahr, monat, t))
    }
}

pub fn depotverlauf(stand: &Stand, werte: &[Wert]) -> Vec<Tagespunkt> {
    let universum = als_universum(werte);
    let mut ersatz = BTreeMap::<Datum, BTreeSet<Isin>>::new();
    let mut abgelehnt = BTreeSet::new();
    for (tag, wer, grund) in stand.begruendungen() {
        if wer == "Daten"
            && grund.starts_with("Zyklus ")
            && grund.ends_with("niemand wird gefragt, kein Auftrag")
        {
            abgelehnt.insert(*tag);
        }
        if wer == AUSWAHL
            && let Some((isin, _)) = grund
                .strip_prefix(ERSATZKURS)
                .and_then(|text| text.split_once(" · "))
            && let Ok(isin) = Isin::neu(isin)
        {
            ersatz.entry(*tag).or_default().insert(isin);
        }
    }
    stand
        .tagesschluesse()
        .iter()
        .map(|(tag, toepfe)| {
            let mut punkt = punkt(
                stand,
                &universum,
                werte,
                *tag,
                toepfe,
                &ersatz.remove(tag).unwrap_or_default(),
            );
            if abgelehnt.contains(tag) && punkt.gesamt.is_some() {
                punkt.wertpapiere = None;
                punkt.gesamt = None;
                punkt.grund = Some("Kursprüfung am Tag fehlgeschlagen".into());
                punkt.kurstage.clear();
                punkt.kursbezug.clear();
            }
            punkt
        })
        .collect()
}

fn punkt(
    stand: &Stand,
    universum: &Universum,
    werte: &[Wert],
    tag: Datum,
    toepfe: &BTreeMap<TopfId, Topf>,
    ersatz: &BTreeSet<Isin>,
) -> Tagespunkt {
    let bargeld = toepfe
        .values()
        .try_fold(Cent::NULL, |summe, topf| summe.checked_add(topf.bargeld()));
    let mut punkt = Tagespunkt {
        tag,
        wertpapiere: None,
        bargeld,
        gesamt: None,
        grund: None,
        kurstage: Vec::new(),
        kursbezug: BTreeMap::new(),
        ersatz: ersatz.iter().cloned().collect(),
    };
    let Some(bargeld) = bargeld else {
        punkt.grund = Some("Rechenfehler beim Bargeld".into());
        return punkt;
    };
    let gehalten: BTreeSet<_> = toepfe
        .values()
        .flat_map(|t| t.positionen().keys())
        .collect();
    let ausserhalb: Vec<_> = gehalten
        .iter()
        .filter(|i| !werte.iter().any(|w| w.isin == ***i))
        .map(ToString::to_string)
        .collect();
    if !ausserhalb.is_empty() {
        punkt.grund = Some(format!(
            "{}, nicht mehr in der Beobachtungsliste",
            ausserhalb.join(", ")
        ));
        return punkt;
    }
    let kurse = match tageskurse_am(stand, universum, tag) {
        Ok(k) => k,
        Err(LaufFehler::Halt(grund)) => {
            punkt.grund = Some(grund);
            return punkt;
        }
    };
    let mut fehlt = Vec::new();
    for isin in &gehalten {
        let beleg = if ersatz.contains(*isin) {
            &kurse.vortag
        } else {
            &kurse.heute
        };
        if beleg.kurs(isin).is_none() {
            fehlt.push(crate::depot::name(werte, isin.als_str()));
        }
    }
    if !fehlt.is_empty() {
        punkt.grund = Some(format!("Kurs fehlte für {}", fehlt.join(", ")));
        return punkt;
    }

    let kurse = if ersatz.is_empty() {
        kurse.heute
    } else {
        match depot_sim::bewertungskurse(&kurse.heute, &kurse.vortag, ersatz) {
            Ok(k) => k,
            Err(LaufFehler::Halt(grund)) => {
                punkt.grund = Some(grund);
                return punkt;
            }
        }
    };
    punkt.kursbezug = gehalten
        .into_iter()
        .filter_map(|isin| Some((isin.clone(), kurse.eintrag(isin)?.handelstag())))
        .collect();
    punkt.kurstage = punkt.kursbezug.values().copied().collect();
    punkt.kurstage.sort_unstable();
    punkt.kurstage.dedup();

    punkt.gesamt = toepfe.values().try_fold(Cent::NULL, |summe, topf| {
        summe.checked_add(topf.wert_gerundet(&kurse, Rundung::Ab)?)
    });
    punkt.wertpapiere = punkt.gesamt.and_then(|wert| wert.checked_sub(bargeld));
    if punkt.wertpapiere.is_none() {
        punkt.gesamt = None;
        punkt.grund = Some("Rechenfehler bei der Bewertung".into());
    }
    punkt
}

/// Die Punkte ab `ab` (einschließlich), in der Reihenfolge aus `depotverlauf`.
pub fn im_zeitraum(punkte: &[Tagespunkt], ab: Option<Datum>) -> &[Tagespunkt] {
    let i = ab.map_or(0, |ab| punkte.partition_point(|p| p.tag < ab));
    punkte.get(i..).unwrap_or_default()
}
