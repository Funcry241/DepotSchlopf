//! DepotSchlopf – Kodex.
//!
//! Die Essenz beraet vor der Aenderung. Der Kodex entscheidet nach der Aenderung.
//!
//! Der Kodex ist ein ausfuehrbares Gate. Er prueft jede Aenderung gegen die Basis,
//! billige Strukturpruefungen zuerst, teure Beweise zuletzt:
//!
//! 1. Deklaration: `kodex/aenderung.txt` nennt Klasse, Zweck und Pfade. Deklarierter und
//!    realer Diff muessen exakt passen (nichts Undeklariertes, nichts Deklariertes ohne Aenderung).
//! 2. Pfadrollen: Jede Klasse darf nur bestimmte Rollen beruehren.
//! 3. Festschreibung: Harte Grenzen, KI-Budget, Produktarten, Abhaengigkeiten und Netzzugang
//!    sind in `kodex/festschreibung.txt` festgeschrieben. Abweichung = Ablehnung.
//!    Jede Aenderung der Festschreibung braucht eine an sie gebundene Grenzfreigabe.
//! 4. Quellscan: Code syntaktisch, Manifeste semantisch. Harte Lints im Kopf jedes
//!    Produktiv-Targets auf `forbid`; keine Ausnahme dafuer, auch nicht hinter `cfg_attr`;
//!    Kern und Regelwerk rein; kein Netz ohne Erlaubnis; Lesefehler und Links sind Fehler.
//!    Keine Datei des Projekts enthaelt ein Geheimnismuster (R28).
//! 5. Essenz-Kaltstart: Die Essenz entspricht ihrem festen Register und bleibt aufloesbar.
//! 6. (mit `--voll`) Format, Clippy mit `-D warnings`, alle Tests.

// Die harten Lints stehen auch im Kodex selbst auf `forbid` (R26, Befund 1·G01).
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

pub mod deklaration;
pub mod essenz;
pub mod festschreibung;
pub mod freigabe;
pub mod geheimnis;
pub mod git;
pub mod lexer;
pub mod manifest;
pub mod pfade;
pub mod pr;
pub mod quellscan;
pub mod rust;
pub mod toml;

use std::path::Path;

pub use deklaration::{Deklaration, Klasse};
pub use pfade::Rolle;

pub const DEKLARATION: &str = "kodex/aenderung.txt";
pub const FESTSCHREIBUNG: &str = "kodex/festschreibung.txt";

/// Projektordner zur Laufzeit: vom aktuellen Verzeichnis aufwaerts der erste Ordner mit
/// `Cargo.toml` und `kodex/festschreibung.txt`. Nie der Pfad zur Uebersetzungszeit,
/// sonst prueft ein wiederverwendetes Programm den falschen Baum.
pub fn finde_wurzel() -> Option<std::path::PathBuf> {
    let start = std::env::current_dir().ok()?;
    start
        .ancestors()
        .find(|p| p.join("Cargo.toml").is_file() && p.join(FESTSCHREIBUNG).is_file())
        .map(Path::to_path_buf)
}

/// Prueft Schritte 1 bis 5. Leere Liste = Gate offen.
pub fn pruefe(wurzel: &Path, basis: &str) -> Vec<String> {
    let mut fehler = Vec::new();

    let diff = match git::aenderungen(wurzel, basis) {
        Ok(d) => d,
        Err(e) => return vec![format!("Diff nicht bestimmbar: {e}")],
    };
    // Keine Aenderung gegenueber der Basis (z. B. frischer Klon von main): Es gibt nichts zu
    // deklarieren. Festschreibung, Quellscan und Essenz gelten trotzdem.
    if diff.pfade.is_empty() {
        return pruefe_ohne_diff(wurzel);
    }
    let deklaration = match std::fs::read_to_string(wurzel.join(DEKLARATION)) {
        Ok(t) => match deklaration::lies(&t) {
            Ok(d) => d,
            Err(e) => return vec![format!("{DEKLARATION}: {e}")],
        },
        Err(e) => return vec![format!("{DEKLARATION} fehlt: {e}")],
    };

    fehler.extend(deklaration::vergleiche(&deklaration, &diff.pfade));
    fehler.extend(pfade::pruefe_rollen(
        deklaration.klasse,
        &diff.pfade,
        diff.basis_hat_kodex,
    ));
    fehler.extend(festschreibung::pruefe(
        wurzel,
        deklaration.klasse,
        &diff.pfade,
    ));
    fehler.extend(freigabe::pruefe(wurzel, &diff.pfade));
    fehler.extend(quellscan::pruefe(wurzel));
    fehler.extend(geheimnis::pruefe(wurzel));
    fehler.extend(essenz::pruefe(wurzel));
    fehler
}

/// Fuer einen entpackten Stand ohne Git: Deklaration und Pfadrollen sind nicht pruefbar,
/// Festschreibung, Quellscan und Essenz schon. Ersetzt nie das Gate auf GitHub.
pub fn pruefe_ohne_diff(wurzel: &Path) -> Vec<String> {
    let mut fehler =
        festschreibung::pruefe(wurzel, Klasse::Funktion, &std::collections::BTreeSet::new());
    fehler.extend(quellscan::pruefe(wurzel));
    fehler.extend(geheimnis::pruefe(wurzel));
    fehler.extend(essenz::pruefe(wurzel));
    fehler
}
