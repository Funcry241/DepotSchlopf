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

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs::{File, OpenOptions};
    use std::os::windows::fs::OpenOptionsExt;
    use std::time::{Duration, Instant};

    fn gesperrtes_ziel(name: &str) -> io::Result<(PathBuf, PathBuf, File)> {
        let ordner =
            std::env::temp_dir().join(format!("depot-ersetzen-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&ordner)?;
        let quelle = ordner.join("neu");
        let ziel = ordner.join("alt");
        std::fs::write(&quelle, b"neu")?;
        std::fs::write(&ziel, b"alt")?;
        let sperre = OpenOptions::new().read(true).share_mode(3).open(&ziel)?;
        Ok((quelle, ziel, sperre))
    }

    use std::path::PathBuf;

    #[test]
    fn ersetze_wartet_auf_freigabe() -> io::Result<()> {
        let (quelle, ziel, sperre) = gesperrtes_ziel("freigabe")?;
        std::thread::scope(|s| {
            s.spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                drop(sperre);
            });
            ersetze(&quelle, &ziel)
        })?;
        assert_eq!(std::fs::read(&ziel)?, b"neu");
        std::fs::remove_dir_all(quelle.with_file_name(""))
    }

    #[test]
    fn ersetze_gibt_nach_5_versuchen_auf() -> io::Result<()> {
        let (quelle, ziel, sperre) = gesperrtes_ziel("timeout")?;
        let anfang = Instant::now();
        assert!(ersetze(&quelle, &ziel).is_err());
        assert!(anfang.elapsed() >= Duration::from_secs(1));
        assert_eq!(std::fs::read(&quelle)?, b"neu");
        assert_eq!(std::fs::read(&ziel)?, b"alt");
        drop(sperre);
        std::fs::remove_dir_all(quelle.with_file_name(""))
    }
}
