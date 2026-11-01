//! Auftraege. `GenehmigterAuftrag` kann ausserhalb dieses Crates niemand bauen:
//! Das Feld `_siegel` hat einen Typ, dessen Konstruktor privat ist. Der Broker nimmt nur diesen Typ an.

use depot_kern::{Cent, Isin, TopfId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Richtung {
    Kauf,
    Verkauf,
}

/// Eindeutige Kennung je Auftrag, damit ein doppeltes Senden nach einem Absturz erkennbar ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AuftragsKennung {
    pub zyklus: u32,
    pub topf: TopfId,
    pub nr: u8,
}

/// Ein Limit-Auftrag. Andere Auftragsarten gibt es nicht.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Auftrag {
    pub kennung: AuftragsKennung,
    pub isin: Isin,
    pub richtung: Richtung,
    pub stueck: u64,
    pub limit: Cent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anlass {
    Umschichtung,
    Notbremse,
}

/// Wofuer und unter welchen Grenzen der Auftrag genehmigt wurde.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegelBeleg {
    pub grenzen_abdruck: String,
    pub anlass: Anlass,
}

mod siegel {
    /// Nur innerhalb von `depot_regelwerk` erzeugbar.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Siegel(());

    impl Siegel {
        pub(crate) fn neu() -> Siegel {
            Siegel(())
        }
    }
}

/// Der einzige Auftragstyp, den die Auftragsstelle an den Broker senden kann.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenehmigterAuftrag {
    auftrag: Auftrag,
    beleg: RegelBeleg,
    _siegel: siegel::Siegel,
}

impl GenehmigterAuftrag {
    pub(crate) fn neu(auftrag: Auftrag, beleg: RegelBeleg) -> GenehmigterAuftrag {
        GenehmigterAuftrag {
            auftrag,
            beleg,
            _siegel: siegel::Siegel::neu(),
        }
    }

    pub fn auftrag(&self) -> &Auftrag {
        &self.auftrag
    }

    pub fn beleg(&self) -> &RegelBeleg {
        &self.beleg
    }
}
