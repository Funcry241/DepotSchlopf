//! DepotSchlopf – harter Budgetwaechter fuer Dachs (Claude) und Maus (GPT).
//!
//! Kein KI-Aufruf ohne `AufrufErlaubnis`. Eine Erlaubnis entsteht nur, wenn
//! 1. das Kassenbuch exklusiv geoeffnet ist (kein zweiter Prozess),
//! 2. alle Grenzen fuer Zyklus, Tag, Monat und Jahr eingehalten sind,
//! 3. die Hoechstkosten des Aufrufs vorab gebucht und auf die Platte geschrieben sind.
//!
//! Jeder Fehler sperrt (fail-closed). Eine Endlosschleife, die jede Sekunde fragt,
//! bekommt pro Zyklus hoechstens zwei Erlaubnisse und danach nur noch Sperren.

pub mod budget;
pub mod kassenbuch;

pub use budget::{AufrufErlaubnis, Buchung, Datum, KiDeckel, KiGrenzen, Preise, Sperre, pruefe};
pub use kassenbuch::Kassenbuch;
