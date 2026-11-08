//! DepotSchlopf – harter Budgetwaechter fuer Dachs (Claude) und Maus (GPT).
//!
//! Dieses Crate enthaelt die Regeln des Budgets ohne Dateizugriff: Grenzen und Deckel allein
//! aus der Festschreibung (R19), die Pruefung `pruefe` und den Grenzabdruck. Das Kassenbuch
//! und die `AufrufErlaubnis` stehen im Journal (`depot_journal`): Eine Erlaubnis entsteht nur,
//! wenn das Journal die Buchung der Hoechstkosten nach genau diesen Regeln geprueft und
//! dauerhaft geschrieben hat.
//!
//! Jeder Fehler sperrt (fail-closed). Eine Endlosschleife, die jede Sekunde fragt,
//! bekommt pro Zyklus höchstens 30 Erlaubnisse und danach nur noch Sperren.

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
mod tagesrahmen;
pub use tagesrahmen::{handelstage_rest, tagesrahmen};

pub use budget::{Buchung, Datum, KiDeckel, KiGrenzen, Preise, Sperre, grenzabdruck, pruefe};
