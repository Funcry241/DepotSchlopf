//! Das Papierdepot, wie es aus dem Journal folgt, bewertet mit den juengsten belegten
//! Schlusskursen (abgerundet, wie der Waechter den heutigen Wert misst).

use depot_journal::Stand;
use depot_kern::liste::{Wert, als_universum};
use depot_kern::{Cent, Datum, Rundung, Topf, TopfId};
use depot_regelwerk::{Anlass, Richtung};
use depot_sim::tageskurse;

/// Eine Position eines Topfes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Position {
    pub isin: String,
    pub name: String,
    pub stueck: u64,
    /// Wert zum juengsten Schlusskurs, abgerundet; `None` ohne Kurs.
    pub wert: Option<Cent>,
}

/// Ein Topf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopfLage {
    pub topf: TopfId,
    pub bargeld: Cent,
    pub wert: Option<Cent>,
    pub positionen: Vec<Position>,
}

/// Ein Auftrag zur Anzeige.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuftragZeile {
    pub nr: u64,
    pub topf: TopfId,
    pub name: String,
    pub richtung: &'static str,
    pub stueck: u64,
    pub limit: Cent,
    pub anlass: &'static str,
    pub zustand: String,
}

/// Das Papierdepot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepotLage {
    /// Handelstag der Kurse, mit denen bewertet ist.
    pub kurse_vom: Option<Datum>,
    pub toepfe: Vec<TopfLage>,
    pub gesamt: Option<Cent>,
    pub letzter_abschluss: Option<Datum>,
    pub hoechststand: Option<Cent>,
    /// Neueste zuerst.
    pub auftraege: Vec<AuftragZeile>,
    /// Vergleich seit Start (#118); `None` vor dem ersten abgeschlossenen Papiertag.
    pub vergleich: Option<crate::vergleich::VergleichLage>,
}

/// So viele Auftraege zeigt das Lagebild hoechstens.
pub const AUFTRAEGE_MAX: usize = 20;

fn name(werte: &[Wert], isin: &str) -> String {
    werte
        .iter()
        .find(|w| w.isin.als_str() == isin)
        .map_or_else(|| isin.to_string(), |w| w.name.clone())
}

fn topf_lage(t: &Topf, kurse: Option<&depot_kern::Kurse>, werte: &[Wert]) -> TopfLage {
    let positionen = t
        .positionen()
        .iter()
        .map(|(isin, stueck)| Position {
            isin: isin.als_str().to_string(),
            name: name(werte, isin.als_str()),
            stueck: *stueck,
            wert: kurse
                .and_then(|k| k.kurs(isin))
                .and_then(|p| p.mal_stueck(*stueck, Rundung::Ab)),
        })
        .collect();
    TopfLage {
        topf: t.id(),
        bargeld: t.bargeld(),
        wert: kurse.and_then(|k| t.wert_gerundet(k, Rundung::Ab)),
        positionen,
    }
}

pub(crate) fn depot_lage(s: &Stand, werte: &[Wert]) -> DepotLage {
    let kurse = tageskurse(s, &als_universum(werte)).ok();
    let toepfe: Vec<TopfLage> = s
        .toepfe()
        .values()
        .map(|t| topf_lage(t, kurse.as_ref().map(|k| &k.heute), werte))
        .collect();
    let gesamt = toepfe
        .iter()
        .try_fold(Cent::NULL, |summe, t| summe.checked_add(t.wert?))
        .filter(|_| !toepfe.is_empty());
    let auftraege = s
        .auftraege()
        .iter()
        .rev()
        .take(AUFTRAEGE_MAX)
        .map(|a| AuftragZeile {
            nr: a.beleg.nr,
            topf: a.beleg.topf,
            name: name(werte, a.beleg.isin.als_str()),
            richtung: match a.beleg.richtung {
                Richtung::Kauf => "Kauf",
                Richtung::Verkauf => "Verkauf",
            },
            stueck: a.beleg.stueck,
            limit: a.beleg.limit,
            anlass: match a.beleg.anlass {
                Anlass::Umschichtung => "Vorschlag",
                Anlass::AusstiegPositivliste => "Ausstieg Positivliste",
                Anlass::Abbau => "Abbau",
                Anlass::Notbremse => "Notbremse",
            },
            zustand: format!("{:?}", a.zustand),
        })
        .collect();
    DepotLage {
        kurse_vom: kurse.map(|k| k.handelstag),
        toepfe,
        gesamt,
        letzter_abschluss: s.abschluss().map(|a| a.datum),
        hoechststand: s.historischer_hoechststand(),
        auftraege,
        vergleich: crate::vergleich::vergleich_lage(s, werte),
    }
}
