//! Toepfe, Positionen und KI-Vorschlaege.

use std::collections::BTreeMap;

use crate::geld::Cent;
use crate::instrument::Isin;
use crate::kurs::Kurse;
use crate::preis::Rundung;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TopfFehler {
    /// Bargeld ist nie negativ (R20, 1·R06).
    NegativesBargeld,
}

/// Ein virtueller Topf auf dem gemeinsamen Cash-Konto. Bargeld ist nie negativ; die Felder
/// sind nur ueber gepruefte Methoden zu erreichen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Topf {
    id: TopfId,
    bargeld: Cent,
    /// Stueckzahlen je Wert; nur ganze Stuecke, nie negativ, nie null.
    positionen: BTreeMap<Isin, u64>,
}

impl Topf {
    /// # Errors
    /// Negatives Bargeld.
    pub fn neu(id: TopfId, bargeld: Cent) -> Result<Topf, TopfFehler> {
        if bargeld.0 < 0 {
            return Err(TopfFehler::NegativesBargeld);
        }
        Ok(Topf {
            id,
            bargeld,
            positionen: BTreeMap::new(),
        })
    }

    /// Setzt den Bestand eines Werts, wie ihn das Konto meldet; 0 entfernt ihn.
    pub fn setze_bestand(&mut self, isin: Isin, stueck: u64) {
        if stueck == 0 {
            self.positionen.remove(&isin);
        } else {
            self.positionen.insert(isin, stueck);
        }
    }

    pub fn id(&self) -> TopfId {
        self.id
    }

    pub fn bargeld(&self) -> Cent {
        self.bargeld
    }

    pub fn positionen(&self) -> &BTreeMap<Isin, u64> {
        &self.positionen
    }

    /// Gesamtwert zu den gegebenen Kursen, jede Position auf ganze Cent abgerundet: Das Depot
    /// rechnet sich nie reicher. `None`, wenn ein Kurs fehlt oder ein Ueberlauf droht.
    pub fn wert(&self, kurse: &Kurse) -> Option<Cent> {
        let mut summe = self.bargeld;
        for (isin, stueck) in &self.positionen {
            let kurs = kurse.kurs(isin)?;
            summe = summe.checked_add(kurs.mal_stueck(*stueck, Rundung::Ab)?)?;
        }
        Some(summe)
    }

    pub fn stueck(&self, isin: &Isin) -> u64 {
        self.positionen.get(isin).copied().unwrap_or(0)
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
