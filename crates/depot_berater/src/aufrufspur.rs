//! Kurzlebige Spur des Versands und dauerhafte Rohdaten ohne Kopfzeilen.

use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use depot_journal::KiAufrufErgebnis;

pub(crate) struct Spur {
    pub frage: Option<String>,
    pub antwort: Option<Vec<u8>>,
    pub verbrauch: Option<crate::anbieter::Verbrauch>,
    pub ergebnis: KiAufrufErgebnis,
}

impl Default for Spur {
    fn default() -> Self {
        Self {
            frage: None,
            antwort: None,
            verbrauch: None,
            ergebnis: KiAufrufErgebnis::NichtGesendet,
        }
    }
}

pub(crate) fn enthaelt_geheimnis(bytes: &[u8], geheim: &impl Fn(&str) -> bool) -> bool {
    use depot_kern::json::Json;
    fn innen(wert: &Json, geheim: &impl Fn(&str) -> bool) -> bool {
        match wert {
            Json::Text(text) => geheim(text),
            Json::Liste(werte) => werte.iter().any(|wert| innen(wert, geheim)),
            Json::Objekt(felder) => felder
                .iter()
                .any(|(name, wert)| geheim(name) || innen(wert, geheim)),
            _ => false,
        }
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return true;
    };
    match depot_kern::json::lies(text) {
        Ok(wert) => innen(&wert, geheim),
        Err(_) => text.len() > 4096 || geheim(text),
    }
}

pub(crate) fn ablege(ordner: &Path, sha: &str, bytes: &[u8]) -> io::Result<()> {
    static NUMMER: AtomicU64 = AtomicU64::new(0);
    fs::create_dir_all(ordner)?;
    let ziel = ordner.join(format!("{sha}.txt"));
    match fs::read(&ziel) {
        Ok(alt) if alt == bytes => return Ok(()),
        Ok(_) => return Err(io::Error::other("Rohdatei hat abweichenden Inhalt")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let nr = NUMMER.fetch_add(1, Ordering::Relaxed);
    let neben = ordner.join(format!(".{sha}-{}-{nr}.tmp", std::process::id()));
    let mut datei = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&neben)?;
    let ausgang = datei.write_all(bytes).and_then(|()| datei.sync_all());
    drop(datei);
    let ausgang = ausgang.and_then(|()| depot_journal::dateiersatz::ersetze(&neben, &ziel));
    if ausgang.is_err() {
        let _ = fs::remove_file(&neben);
    }
    ausgang
}
