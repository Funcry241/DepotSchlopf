//! DepotSchlopf – Kern.
//!
//! Grundtypen ohne Netz- und Dateizugriff: Geld in Cent, ISIN, Instrumente, Toepfe,
//! Kurse, KI-Vorschlaege und die festgeschriebenen Grenzen.
//!
//! Leitsatz: KI schlaegt vor. Regelwerk entscheidet. Auftragsstelle schreibt.
//! Journal belegt. Abgleich prueft.

pub mod depot;
pub mod geld;
pub mod grenzen;
pub mod instrument;

pub use depot::{BeraterId, Kurse, Topf, TopfId, Vorschlag};
pub use geld::{BP_GANZ, Cent};
pub use grenzen::{Gebuehren, Grenzen, Universum};
pub use instrument::{Instrument, InstrumentArt, Isin, IsinFehler};
