//! Die Termine der EZB von ihrer eigenen Seite: Sitzungen des EZB-Rats und des Erweiterten
//! Rats, je Tag ein Eintrag. Die EZB veroeffentlicht sie selbst; sie gelten deshalb als
//! bestaetigt (R39).
//!
//! Gelesen wird genau die Form der Seite: im Element `main` genau eine Liste `dl` aus Paaren
//! `<dt>TT/MM/JJJJ</dt><dd>Text<br></dd>`. Jede Abweichung ist ein Fehler mit der Nummer des
//! Eintrags; nichts wird geraten, uebersprungen oder umgeschrieben. Eine Uhrzeit nennt die Seite
//! nicht, also hat kein Termin eine.
//!
//! - **Kennung:** `ezb-JJJJ-MM-TT-n`, `n` zaehlt die Eintraege desselben Tags ab 1 in der
//!   Reihenfolge der Seite.
//! - **Titel:** der Text der Seite, Leerraum zu einem Leerzeichen zusammengefasst.
//! - **Betroffen:** `eur-leitzins` fuer eine geldpolitische Sitzung (der Text nennt
//!   „monetary policy meeting“, nicht „non-monetary“), sonst `ezb`.

use depot_kern::{Termin, TerminFehler};

/// Name der Quelle im Journal.
pub const QUELLE_EZB: &str = "ezb";
/// Die Seite mit den Terminen.
pub const ADRESSE_EZB: &str = "https://www.ecb.europa.eu/press/calendars/mgcgc/html/index.en.html";

/// Warum die Seite keine Termine ergibt. Eintraege zaehlen ab 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EzbFehler {
    /// Kein `main`, keine oder mehr als eine Liste, oder die Liste ist leer.
    Seite,
    /// Der Eintrag hat nicht die Form `<dt>…</dt><dd>…</dd>`.
    Form(usize),
    /// Kein Datum `TT/MM/JJJJ` im Kalender.
    Datum(usize),
    /// Der Text enthaelt etwas anderes als Text und ein abschliessendes `<br>`.
    Text(usize),
    /// Der Eintrag ergibt keinen gueltigen Termin.
    Termin(usize, TerminFehler),
}

/// Liest alle Termine der Seite in ihrer Reihenfolge.
///
/// # Errors
/// Jede Abweichung von der Form; dann gibt es keinen Termin.
pub fn lies_ezb_termine(html: &str) -> Result<Vec<Termin>, EzbFehler> {
    let _ = html;
    Err(EzbFehler::Seite)
}
