//! DepotSchlopf – Dachs (Claude) und Maus (GPT) als Berater im Papierhandel.
//!
//! Das einzige Crate ausser dem Sammler mit Netzzugang (Kodex `NETZ_ERLAUBT`, Wolfgang
//! 05.11.2026), und ein Blatt im Abhaengigkeitsbaum: Es haengt von `depot_sim` ab, nie
//! umgekehrt. Gefragt wird nur mit einer `AufrufErlaubnis`, die das Journal vorher gebucht hat.
//! Gesendet werden nur Papierdepot, Kurse, Termine und die belegten Nachrichten. Die Schluessel
//! liegen nur in der Windows-Anmeldeinformationsverwaltung. Nach jedem Aufruf werden die
//! tatsaechlichen Kosten mit der Reservierung verglichen; jede Ueberschreitung sperrt die KIs (`daten/ki.halt`).

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

pub mod anbieter;
pub mod frage;
mod gemini_netz;
pub use depot_kern::json;
pub mod ki;
pub mod netz;
pub mod schluessel;
pub mod zusatz;

pub use anbieter::{Anbieter, Gelesen, Verbrauch};
pub use ki::KiBerater;
pub use netz::{Https, Netz, NetzFehler, Rueckmeldung};
pub use schluessel::Schluessel;

/// Name der Sperrdatei im Ordner `daten/`.
pub const SPERRDATEI: &str = "ki.halt";
