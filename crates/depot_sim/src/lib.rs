//! DepotSchlopf – simulierter Lauf mit erfundenen Daten und Papiertag mit echten Kursen, beides
//! nur mit Papiergeld.
//!
//! Dachs und Maus beobachten ein erfundenes Bild aus Nachrichten, Terminen und Kursen,
//! schreiben ihre Einschaetzung und Begruendung ins Journal und treffen Papierentscheidungen.
//! Der Rahmen haelt nur den Schutz: Kostenschutz, Zweitmeinung, Regelwerk, Datenpruefung,
//! Waechter, Kontowaechter und Kodex. Ausgefuehrt wird ueber ein erfundenes Papierkonto, mit
//! dem der Kontowaechter abgleicht. Im simulierten Lauf haelt ein Kurs, der nicht als erfunden
//! markiert ist, den Lauf an. Der Papiertag (`papiertag`) nimmt nur die vom Sammler belegten
//! Schlusskurse des heutigen Tages. Einen Broker ausser dem Papierkonto und echtes Geld gibt es
//! hier nicht.

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
pub mod abstimmung;
pub mod lauf;
pub mod papier;
pub mod papierdatei;
pub mod papierlauf;
pub mod papiertag;
pub mod szenario;
pub mod vorbereitung;

pub use lauf::{Gefragt, LaufFehler, gefragt, lauf};
pub use papier::{Broker, Papierkonto, Sendeantwort};
pub use papierdatei::Papierdatei;
pub use papiertag::{
    AUSWAHL, Ausgang, MAX_NACHRICHTEN_LAGE, Tageskurse, Tageslage, kurse_von_heute, papiertag,
    papiertag_wiederabspielen, tageskurse, tageskurse_am, tageslage, tagnummer, weltbelege_am,
    weltlage_am,
};
pub use szenario::{
    Antwort, Berater, Drehbuch, KEINE_VERWERTBARE_ANTWORT, Lage, Stumm, Szenario, Tag, Urteil,
};
