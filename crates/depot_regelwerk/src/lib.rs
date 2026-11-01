//! DepotSchlopf – Regelwerk.
//!
//! Das Regelwerk entscheidet. Es ist eine reine Funktion ohne Netz- und Dateizugriff und der
//! einzige Ort, an dem ein [`GenehmigterAuftrag`] entstehen kann. Die KIs liefern nur Zielgewichte.

pub mod auftrag;
pub mod daten;
pub mod entscheidung;
pub mod waechter;

pub use auftrag::{Anlass, Auftrag, AuftragsKennung, GenehmigterAuftrag, RegelBeleg, Richtung};
pub use daten::{DatenFehler, pruefe_kurse};
pub use entscheidung::{Befund, Eingabe, Entscheidung, VerwerfGrund, entscheide};
pub use waechter::{WaechterBefund, notbremse_auftraege, waechter};
