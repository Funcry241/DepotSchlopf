//! DepotSchlopf – simulierter Lauf mit erfundenen Daten und Papiergeld.
//!
//! Dachs und Maus beobachten ein erfundenes Bild aus Nachrichten, Terminen und Kursen,
//! schreiben ihre Einschaetzung und Begruendung ins Journal und treffen Papierentscheidungen.
//! Der Rahmen haelt nur den Schutz: Kostenschutz, Zweitmeinung, Regelwerk, Datenpruefung,
//! Waechter, Kontowaechter und Kodex. Ausgefuehrt wird ueber ein erfundenes Papierkonto, mit
//! dem der Kontowaechter abgleicht. Echte Schnittstellen, echte Kurse und echtes Geld gibt es hier nicht;
//! ein Kurs, der nicht als erfunden markiert ist, haelt den Lauf an.

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

mod abdruck;
pub mod lauf;
pub mod papier;
pub mod szenario;

pub use lauf::{LaufFehler, lauf};
pub use papier::{Broker, Papierkonto, Sendeantwort};
pub use szenario::{Antwort, Berater, Drehbuch, Lage, Szenario, Tag, Urteil};
