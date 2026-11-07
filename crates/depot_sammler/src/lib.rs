//! DepotSchlopf – Sammelstelle: Kurse (Kap. 6, R12), Termine (R39) und Nachrichten (R40).
//!
//! Der Sammler holt Schlusskurse aus den freigegebenen, lesenden Quellen (Xetra: Alpha Vantage,
//! NYSE: Tiingo, umgerechnet mit den Referenzkursen der EZB),
//! legt jede rohe Antwort unter `daten/roh/` ab, belegt jeden Kurs vor seiner Verwendung im
//! Journal und prueft die Kurse danach wie jeden Kurs (`pruefe_kurse`). Ebenso belegt er Termine
//! der EZB (`termine`) und kostenlose RSS-Schlagzeilen (`nachrichten`). Er sendet keine
//! Auftraege und fragt keine KI.
//!
//! Netzzugang hat nur dieses Crate (Kodex, `NETZ_ERLAUBT`), und kein anderes Crate haengt von
//! ihm ab. Der Schluessel der Quelle liegt in der Windows-Anmeldeinformationsverwaltung; er steht
//! nie im Journal, in einer Datei des Rahmens, in einer Meldung oder im Export.

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

pub mod diagnose;
pub mod ezb;
pub use depot_kern::liste;
pub mod nachrichten;
pub mod netz;
pub mod rss;
mod rss_quellen;
pub mod sammeln;
pub mod schluessel;
pub mod termine;

pub use liste::{QUELLE, QUELLE_US, Wert, beobachtungsliste};
pub use nachrichten::{NachrichtenBericht, sammle_nachrichten};
pub use netz::{Ausfall, Https, Netz, NetzFehler, Takt, Verlauf};
pub use sammeln::{ADRESSE_DEVISEN, Bericht, Ergebnis, SammelFehler, einordnen, sammle};
pub use schluessel::{Anbieter, Schluessel, Zugang, zugang_laden};
pub use termine::{TerminBericht, sammle_termine};
