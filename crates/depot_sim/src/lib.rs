//! DepotSchlopf – simulierter Lauf mit erfundenen Daten und Papiergeld.
//!
//! Leere Schnittstelle fuer die roten Tests.

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

pub mod szenario;

use depot_journal::Journal;

pub use szenario::{Antwort, Berater, Drehbuch, Lage, Szenario, Tag, Urteil};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaufFehler {
    /// Der Lauf steht. Was bis dahin abgeschlossen ist, steht im Journal.
    Halt(String),
}

/// Leere Schnittstelle.
///
/// # Errors
/// Immer.
pub fn lauf(
    _journal: &mut Journal,
    _szenario: &Szenario,
    _dachs: &mut dyn Berater,
    _maus: &mut dyn Berater,
) -> Result<(), LaufFehler> {
    Err(LaufFehler::Halt("fehlt".into()))
}
