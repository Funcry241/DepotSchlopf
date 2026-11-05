//! DepotSchlopf – Regelwerk.
//!
//! Das Regelwerk entscheidet. Es ist eine reine Funktion ohne Netz- und Dateizugriff und der
//! einzige Ort, an dem ein [`GenehmigterAuftrag`] entstehen kann. Die KIs liefern nur Zielgewichte.

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

pub mod auftrag;
pub mod daten;
pub mod entscheidung;
pub mod waechter;

pub use auftrag::{
    Anlass, Auftrag, AuftragsKennung, GenehmigterAuftrag, Kennungsvergabe, RegelBeleg, Richtung,
};
pub use daten::{DatenFehler, Tageswerte, pruefe_kurse, waehle_tageswerte};
pub use entscheidung::{Befund, Eingabe, Entscheidung, VerwerfGrund, entscheide};
pub use waechter::{WaechterBefund, notbremse_auftraege, waechter};
