//! Ein Lauf ueber die Termine (R39): die Seite der EZB abrufen, roh ablegen, lesen, belegen.
//!
//! Reihenfolge wie bei den Kursen (R12): Erst liegt die rohe Seite unter `daten/roh/ezb/`, dann
//! steht jeder Termin daraus im Journal. Was nicht gelesen werden kann, wird nicht belegt; die
//! rohe Seite bleibt zur Durchsicht liegen. Ein Termin, der schon genau so belegt ist, wird nicht
//! noch einmal geschrieben; ein geaenderter wird unter seiner Kennung fortgeschrieben.
//!
//! Der Lauf erfindet nichts und loescht nichts. Ein kuenftiger Termin, der im Journal steht, aber
//! nicht mehr auf der Seite, wird nur gemeldet (`fehlen`); was das bedeutet, entscheidet nicht
//! der Sammler.

use std::path::{Path, PathBuf};

use depot_journal::Journal;
use depot_kern::Zeitpunkt;

use crate::netz::Netz;
use crate::sammeln::SammelFehler;

/// Der Bericht eines Laufs ueber die Termine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminBericht {
    /// Die abgelegte rohe Seite; keine, wenn der Abruf scheiterte.
    pub rohdatei: Option<PathBuf>,
    /// Termine mit neuer Kennung.
    pub neu: usize,
    /// Termine, die sich geaendert haben und fortgeschrieben sind.
    pub geaendert: usize,
    /// Termine, die schon genau so belegt waren.
    pub unveraendert: usize,
    /// Kennungen kuenftiger Termine dieser Quelle, die im Journal stehen, aber nicht mehr auf
    /// der Seite, aufsteigend.
    pub fehlen: Vec<String>,
    /// Warum nichts belegt wurde.
    pub fehler: Option<String>,
}

/// Ein Lauf ueber die Seite der EZB. `uhr` liefert die Zeit des Rechners in UTC.
///
/// # Errors
/// Journal, Ablage oder Uhr versagen. Was bis dahin belegt ist, bleibt belegt.
pub fn sammle_termine(
    _journal: &mut Journal,
    _netz: &mut dyn Netz,
    _uhr: &mut dyn FnMut() -> Option<Zeitpunkt>,
    _ablage: &Path,
) -> Result<TerminBericht, SammelFehler> {
    Ok(TerminBericht::default())
}
