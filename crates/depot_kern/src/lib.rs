//! DepotSchlopf – Kern.
//!
//! Grundtypen ohne Netz- und Dateizugriff: Geld in Cent, exakte Preise, ISIN, Instrumente, Toepfe,
//! gepruefte Kurse und ihr Lesen aus einer Quelle, Termine, Kalenderdaten, KI-Vorschlaege und die festgeschriebenen Grenzen.
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

pub mod boerse;
pub mod depot;
pub mod devisen;
pub mod geld;
pub mod grenzen;
pub mod hinweis;
pub mod instrument;
pub mod kurs;
pub mod liste;
pub mod preis;
pub mod quelle;
pub mod termin;
pub mod zeit;

pub use boerse::Boerse;
pub use depot::{BeraterId, Topf, TopfFehler, TopfId, Vorschlag};
pub use devisen::{Devisen, DevisenFehler};
pub use geld::{BP_GANZ, Cent};
pub use grenzen::{Gebuehren, Grenzen, Universum};
pub use instrument::{Instrument, InstrumentArt, Isin, IsinFehler};
pub use kurs::{Herkunft, Kurs, KursFehler, Kurse, Waehrung};
pub use preis::{Preis, Rundung};
pub use quelle::{
    QuellenFehler, Tageskurs, lies_preis, lies_tageskurse, schlusskurse, schlusskurse_in_euro,
    xetra_schluss,
};
pub use termin::{Betroffen, Termin, TerminFehler, Verlaesslichkeit, Zeitbezug};
pub use zeit::{Datum, Zeitpunkt};
