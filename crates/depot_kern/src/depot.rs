//! Toepfe, Positionen, Kurse und KI-Vorschlaege.

use std::collections::BTreeMap;

use crate::geld::Cent;
use crate::instrument::Isin;

/// Die beiden KIs. Dachs = Claude, Maus = GPT.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BeraterId {
    Dachs,
    Maus,
}

/// Jede KI verwaltet ihren eigenen Topf (je X/2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TopfId {
    A,
    B,
}

impl TopfId {
    pub fn berater(self) -> BeraterId {
        match self {
            TopfId::A => BeraterId::Dachs,
            TopfId::B => BeraterId::Maus,
        }
    }
}

/// Ein virtueller Topf auf dem gemeinsamen Cash-Konto.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Topf {
    pub id: TopfId,
    pub bargeld: Cent,
    /// Stueckzahlen je Wert; nur ganze Stuecke, nie negativ.
    pub positionen: BTreeMap<Isin, u64>,
}

impl Topf {
    pub fn neu(id: TopfId, bargeld: Cent) -> Topf {
        Topf {
            id,
            bargeld,
            positionen: BTreeMap::new(),
        }
    }

    /// Gesamtwert zu den gegebenen Kursen. `None`, wenn ein Kurs fehlt oder ein Ueberlauf droht.
    pub fn wert(&self, kurse: &Kurse) -> Option<Cent> {
        let mut summe = self.bargeld;
        for (isin, stueck) in &self.positionen {
            let kurs = kurse.kurs(isin)?;
            summe = summe.checked_add(kurs.mal_stueck(*stueck)?)?;
        }
        Some(summe)
    }

    pub fn stueck(&self, isin: &Isin) -> u64 {
        self.positionen.get(isin).copied().unwrap_or(0)
    }
}

/// Schlusskurse eines Handelstags, je Stueck.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Kurse(pub BTreeMap<Isin, Cent>);

impl Kurse {
    pub fn kurs(&self, isin: &Isin) -> Option<Cent> {
        self.0.get(isin).copied()
    }
}

/// Was eine KI liefern darf: Zielgewichte in Basispunkten (10.000 = 100 %) je ISIN.
/// Nicht genannte Werte haben Ziel 0. Der Rest bis 100 % ist Bargeld.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vorschlag {
    pub berater: BeraterId,
    pub zielgewichte_bp: BTreeMap<Isin, i64>,
    pub begruendung: BTreeMap<Isin, String>,
}
