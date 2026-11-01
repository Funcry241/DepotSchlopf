//! DepotSchlopf – Kern.
//!
//! Grundtypen ohne Netz- und Dateizugriff: Geld in Cent, ISIN, Instrumente, Toepfe,
//! Kurse, KI-Vorschlaege und die festgeschriebenen Grenzen.
//!
//! Leitsatz: KI schlaegt vor. Regelwerk entscheidet. Auftragsstelle schreibt.
//! Journal belegt. Abgleich prueft.

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

pub mod depot;
pub mod geld;
pub mod grenzen;
pub mod instrument;

pub use depot::{BeraterId, Kurse, Topf, TopfId, Vorschlag};
pub use geld::{BP_GANZ, Cent};
pub use grenzen::{Gebuehren, Grenzen, Universum};
pub use instrument::{Instrument, InstrumentArt, Isin, IsinFehler};
