//! Wie das Lagebild zum Journal kommt.
//!
//! **Neu ist nur, was keine Spur hat (K26-01):** „Nicht eingerichtet“ heisst, dass weder
//! Journal noch Vermerk noch Papierkonto da sind. Ob das Journal eingerichtet ist, entscheidet
//! `Journal::eingerichtet`, dieselbe Pruefung, mit der `Journal::oeffnen` beginnt. Fehlt nur ein
//! Teil, ist der Datenbestand unvollstaendig: Halt, nie „neu einrichten“. Sonst oeffnet das
//! Lagebild das Journal wie jeder Lauf (geprueft, exklusiv).

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use depot_journal::{Journal, JournalFehler};

use crate::Zugang;

/// Name des Papierkontos im Datenordner, wie `run.cmd papier` es fuehrt.
pub const PAPIERKONTO: &str = "papierkonto.txt";
/// Name des Journals im Datenordner.
pub const JOURNAL: &str = "journal.txt";

fn fehler_text(e: &JournalFehler) -> String {
    match e {
        JournalFehler::Belegt => "belegt".into(),
        JournalFehler::Halt(t) | JournalFehler::Abgelehnt(t) => t.clone(),
    }
}

/// Liegt unter diesem Namen irgendetwas? Ein Fehler beim Nachsehen gilt als Spur.
fn spur(pfad: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(pfad) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("{}: nicht lesbar: {e}", pfad.display())),
    }
}

/// Oeffnet das Journal im Datenordner `daten` und ruft `f` mit dem Zugang auf. Das Journal ist
/// nur waehrend `f` offen.
pub fn mit_journal<R>(daten: &Path, f: impl FnOnce(Zugang<'_>) -> R) -> R {
    let pfad = daten.join(JOURNAL);
    let eingerichtet = match Journal::eingerichtet(&pfad) {
        Ok(e) => e,
        Err(e) => return f(Zugang::Fehler(&fehler_text(&e))),
    };
    if !eingerichtet {
        return match spur(&daten.join(PAPIERKONTO)) {
            Ok(false) => f(Zugang::NichtEingerichtet),
            Ok(true) => f(Zugang::Fehler(
                "Papierkonto ohne Journal und Vermerk: Datenbestand unvollstaendig",
            )),
            Err(t) => f(Zugang::Fehler(&t)),
        };
    }
    match Journal::oeffnen(&pfad) {
        Ok(j) => f(Zugang::Offen(j.stand())),
        Err(JournalFehler::Belegt) => f(Zugang::Belegt),
        Err(e) => f(Zugang::Fehler(&fehler_text(&e))),
    }
}
