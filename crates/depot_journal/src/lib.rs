//! DepotSchlopf – Journal. Stand der roten Tests: nur die Schnittstelle, noch kein Verhalten.

#![forbid(
    unsafe_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::float_arithmetic,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::collections::BTreeMap;
use std::path::Path;

use depot_kern::{Datum, Topf, TopfId};
use depot_regelwerk::{Anlass, Auftrag, GenehmigterAuftrag, Kennungsvergabe};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JournalFehler {
    Belegt,
    Halt(String),
    Abgelehnt(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    Beobachtung,
    Topf,
    Auftrag,
    Ki,
    KiGrenzen,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuftragsBeleg {
    pub topf: TopfId,
    pub auftrag: Auftrag,
    pub anlass: Anlass,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stand {
    toepfe: BTreeMap<TopfId, Topf>,
    auftraege: Vec<AuftragsBeleg>,
    beobachtungen: Vec<(Datum, String)>,
    letzte_kennung: u64,
}

impl Stand {
    pub fn toepfe(&self) -> &BTreeMap<TopfId, Topf> {
        &self.toepfe
    }
    pub fn auftraege(&self) -> &[AuftragsBeleg] {
        &self.auftraege
    }
    pub fn beobachtungen(&self) -> &[(Datum, String)] {
        &self.beobachtungen
    }
    pub fn letzte_kennung(&self) -> u64 {
        self.letzte_kennung
    }
}

#[derive(Debug)]
pub struct Journal {
    stand: Stand,
    eintraege: Vec<(Art, String)>,
}

fn offen() -> JournalFehler {
    JournalFehler::Halt("noch nicht gebaut".into())
}

impl Journal {
    pub fn einrichten(_: &Path) -> Result<(), JournalFehler> {
        Err(offen())
    }
    pub fn oeffnen(_: &Path) -> Result<Journal, JournalFehler> {
        Err(offen())
    }
    pub fn eintraege(&self) -> &[(Art, String)] {
        &self.eintraege
    }
    pub fn stand(&self) -> &Stand {
        &self.stand
    }
    pub fn bestaetigt(&self) -> (u64, String) {
        (0, String::new())
    }
    pub fn kennungsvergabe(&self) -> Kennungsvergabe {
        Kennungsvergabe::nach(self.stand.letzte_kennung)
    }
    pub fn anhaengen(&mut self, _: Art, _: &str) -> Result<(), JournalFehler> {
        Err(offen())
    }
    pub fn beobachte(&mut self, _: Datum, _: &str) -> Result<(), JournalFehler> {
        Err(offen())
    }
    pub fn setze_topf(&mut self, _: &Topf) -> Result<(), JournalFehler> {
        Err(offen())
    }
    pub fn belege(&mut self, _: TopfId, _: &GenehmigterAuftrag) -> Result<(), JournalFehler> {
        Err(offen())
    }
}

pub fn inhalt_fuer(_: &[(Art, &str)]) -> Vec<u8> {
    Vec::new()
}

pub fn vermerk_fuer(_: &[u8], _: u64) -> Option<String> {
    None
}
