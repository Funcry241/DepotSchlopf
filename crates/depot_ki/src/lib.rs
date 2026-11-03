//! DepotSchlopf – harter Budgetwaechter fuer Dachs (Claude) und Maus (GPT).
//!
//! Kein KI-Aufruf ohne `AufrufErlaubnis`. Eine Erlaubnis entsteht nur, wenn
//! 1. das Kassenbuch eingerichtet, vom Betriebssystem exklusiv gesperrt und samt Anker und
//!    ganzer Historie geprueft ist,
//! 2. alle Grenzen fuer Zyklus, Tag, Monat und Jahr eingehalten sind; Grenzen und Deckel
//!    stammen allein aus der Festschreibung und sind beim Oeffnen gebunden (R19),
//! 3. die Hoechstkosten des Aufrufs vorab gebucht und auf die Platte geschrieben sind.
//!
//! Jeder Fehler sperrt (fail-closed). Eine Endlosschleife, die jede Sekunde fragt,
//! bekommt pro Zyklus hoechstens zwei Erlaubnisse und danach nur noch Sperren.

// Die harten Lints stehen auf `forbid`: Der Compiler lehnt jede lokale Ausnahme ab, auch eine
// hinter `cfg_attr` (R26, Befund 1·G01). Die Liste haelt der Kodex fest.
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

pub mod budget;
pub mod kassenbuch;

pub use budget::{AufrufErlaubnis, Buchung, Datum, KiDeckel, KiGrenzen, Preise, Sperre, pruefe};
pub use kassenbuch::{Kassenbuch, anker_fuer};
