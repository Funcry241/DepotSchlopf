//! Die Kurssperre `daten/kurse.halt`.
//!
//! Endet ein Abruf aus einem Schutzgrund, etwa weil die Quelle den Schluessel zurueckgab, legt
//! der Sammler diese Datei an (Maus, Restbefund zu #34). Solange sie liegt, ruft der Sammler
//! nichts ab und beginnt kein Papiertag, gleich ob der Zeitplan oder ein Mensch ihn startet: Die
//! schon belegten Teilkurse dieses Abrufs werden nie verwendet. Aufheben heisst: pruefen, den
//! Schluessel bei Bedarf erneuern, dann die Datei loeschen. Sie enthaelt nur einen festen
//! Grund, nie den Schluessel.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Name der Datei im Datenordner.
pub const KURSSPERRE: &str = "kurse.halt";

fn pfad(daten: &Path) -> PathBuf {
    daten.join(KURSSPERRE)
}

/// Legt die Sperre an oder ergaenzt sie um eine Zeile, und schreibt sie auf den Datentraeger.
///
/// # Errors
/// Ordner oder Datei lassen sich nicht schreiben.
pub fn sperre_kurse(daten: &Path, grund: &str) -> Result<(), String> {
    let p = pfad(daten);
    std::fs::create_dir_all(daten).map_err(|e| format!("{}: {e}", daten.display()))?;
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
        .and_then(|mut f| {
            writeln!(f, "{}", grund.replace(['\n', '\r'], " "))?;
            f.sync_all()
        })
        .map_err(|e| format!("{}: {e}", p.display()))
}

/// Der Grund der Sperre, wenn sie liegt. Liegt sie, ist aber unlesbar, gilt sie trotzdem.
pub fn kurssperre(daten: &Path) -> Option<String> {
    let p = pfad(daten);
    if !p.exists() {
        return None;
    }
    Some(
        std::fs::read_to_string(&p)
            .map_or_else(|e| format!("unlesbar: {e}"), |t| t.trim().to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sperre_liegt_bis_sie_geloescht_wird() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("depot_kurssperre_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(kurssperre(&dir), None);
        sperre_kurse(&dir, "Schluessel\\nin der Antwort")?;
        assert!(kurssperre(&dir).is_some_and(|g| g.contains("Schluessel")));
        std::fs::remove_file(dir.join(KURSSPERRE)).map_err(|e| e.to_string())?;
        assert_eq!(kurssperre(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
