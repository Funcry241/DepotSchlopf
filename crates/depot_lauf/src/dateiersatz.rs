//! Atomarer Dateiersatz mit kurzer Wiederholung bei Windows-Dateisperren.

use std::io;
use std::path::Path;

/// Ersetzt das Ziel. Unter Windows bekommt ein vorübergehend gesperrtes Ziel bis zu
/// fünf weitere Versuche im Abstand von 200 Millisekunden.
///
/// # Errors
/// Der Dateiersatz ist nach den Versuchen weiterhin fehlgeschlagen.
pub fn ersetze(quelle: impl AsRef<Path>, ziel: impl AsRef<Path>) -> io::Result<()> {
    let mut wiederholungen = 0_u8;
    loop {
        match std::fs::rename(quelle.as_ref(), ziel.as_ref()) {
            Err(e)
                if cfg!(windows)
                    && wiederholungen < 5
                    && (e.kind() == io::ErrorKind::PermissionDenied
                        || matches!(e.raw_os_error(), Some(32 | 33))) =>
            {
                wiederholungen = wiederholungen.saturating_add(1);
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            ausgang => return ausgang,
        }
    }
}
