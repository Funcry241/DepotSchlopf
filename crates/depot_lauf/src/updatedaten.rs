//! Unveränderte Laufzeitdaten für Update-Probe und Sicherung.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DATEIEN: [&str; 4] = [
    "journal.txt",
    "journal.vermerk",
    "papierkonto.txt",
    "ki.halt",
];
const MAX_SICHERUNGEN: usize = 10;

pub type Datenstand = BTreeMap<String, Vec<u8>>;

fn verzeichnis(pfad: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(pfad) {
        Ok(m) if m.is_dir() => Ok(true),
        Ok(_) => Err(format!("{} ist kein reguläres Verzeichnis", pfad.display())),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("{}: {e}", pfad.display())),
    }
}

fn privat_anlegen(pfad: &Path) -> std::io::Result<()> {
    let b = DirBuilder::new();
    #[cfg(unix)]
    let b = {
        use std::os::unix::fs::DirBuilderExt as _;
        let mut b = b;
        b.mode(0o700);
        b
    };
    b.create(pfad)
}

pub fn update_sperren(daten: &Path) -> Result<File, String> {
    if !verzeichnis(daten)? {
        privat_anlegen(daten).map_err(|e| format!("Datenordner: {e}"))?;
    }
    let pfad = daten.join("update.lock");
    if fs::symlink_metadata(&pfad).is_ok_and(|m| !m.is_file()) {
        return Err("Update-Sperre ist keine reguläre Datei".into());
    }
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(pfad)
        .map_err(|e| format!("Update-Sperre: {e}"))?;
    f.try_lock().map_err(|e| {
        format!("Ein anderes Update läuft oder die Update-Sperre ist nicht verwendbar: {e}")
    })?;
    Ok(f)
}

pub fn journal_sperren(daten: &Path) -> Result<Option<File>, String> {
    let pfad = daten.join("journal.txt");
    match fs::symlink_metadata(&pfad) {
        Ok(m) if m.is_file() => {}
        Ok(_) => return Err("Journal ist keine reguläre Datei".into()),
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Journal: {e}")),
    }
    let f = File::open(pfad).map_err(|e| format!("Journal: {e}"))?;
    f.try_lock_shared()
        .map_err(|e| format!("Journal für die Update-Sicherung belegt oder nicht sperrbar: {e}"))?;
    Ok(Some(f))
}

pub fn lesen(daten: &Path) -> Result<Datenstand, String> {
    let mut stand = Datenstand::new();
    if !verzeichnis(daten)? {
        return Ok(stand);
    }
    for eintrag in fs::read_dir(daten).map_err(|e| format!("Datenordner: {e}"))? {
        let eintrag = eintrag.map_err(|e| format!("Datenordner: {e}"))?;
        let dateiname = eintrag.file_name();
        let Some(name) = dateiname.to_str() else {
            continue;
        };
        let klein = name.to_ascii_lowercase();
        if !DATEIEN.contains(&klein.as_str()) && !klein.starts_with("kassenbuch") {
            continue;
        }
        let typ = eintrag.file_type().map_err(|e| format!("{name}: {e}"))?;
        if !typ.is_file() {
            return Err(format!("Datendatei {name} ist keine reguläre Datei"));
        }
        let bytes = fs::read(eintrag.path()).map_err(|e| format!("Datendatei {name}: {e}"))?;
        stand.insert(name.into(), bytes);
    }
    Ok(stand)
}

pub fn kopiere(stand: &Datenstand, ziel: &Path) -> Result<(), String> {
    for (name, original) in stand {
        let pfad = ziel.join(name);
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pfad)
            .and_then(|mut f| {
                f.write_all(original)?;
                f.sync_all()
            })
            .map_err(|e| format!("Kopie {name}: {e}"))?;
        let kopie = fs::read(&pfad).map_err(|e| format!("Kopie {name} prüfen: {e}"))?;
        if Sha256::digest(&kopie) != Sha256::digest(original) {
            return Err(format!("SHA-256 der Kopie von {name} stimmt nicht"));
        }
    }
    Ok(())
}

pub struct Probe(pub PathBuf);

impl Probe {
    pub fn neu(ablage: &Path) -> Result<Self, String> {
        let ordner = ablage.join(format!("update-datenprobe-{}", std::process::id()));
        privat_anlegen(&ordner).map_err(|e| format!("Ordner für Datenprobe: {e}"))?;
        Ok(Self(ordner))
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_dir_all(&self.0) {
            eprintln!("[Update] Temporäre Datenprobe konnte nicht entfernt werden: {e}");
        }
    }
}

pub fn sichern(daten: &Path, stand: &Datenstand, alt: &str) -> Result<(), String> {
    if stand.is_empty() {
        return Ok(());
    }
    let wurzel = daten.join("sicherung");
    if !verzeichnis(&wurzel)? {
        privat_anlegen(&wurzel).map_err(|e| format!("Sicherungsordner: {e}"))?;
    }
    let utc = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("Zeit für Datensicherung: {e}"))?;
    let name = format!("utc-{:020}-{:09}_{alt}", utc.as_secs(), utc.subsec_nanos());
    let ziel = wurzel.join(&name);
    let staging = wurzel.join(format!("{name}.tmp"));
    privat_anlegen(&staging).map_err(|e| format!("Sicherungsordner: {e}"))?;
    if let Err(e) = kopiere(stand, &staging)
        .and_then(|()| fs::rename(&staging, &ziel).map_err(|e| format!("Sicherung ablegen: {e}")))
    {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }
    println!("[Update] Geprüfte Sicherung: daten/sicherung/{name}");
    begrenze(&wurzel)
}

fn eigener_name(name: &str) -> bool {
    let Some((uhr, commit)) = name.strip_prefix("utc-").and_then(|s| s.split_once('_')) else {
        return false;
    };
    let Some((sekunden, nanos)) = uhr.split_once('-') else {
        return false;
    };
    sekunden.len() == 20
        && nanos.len() == 9
        && sekunden
            .bytes()
            .chain(nanos.bytes())
            .all(|b| b.is_ascii_digit())
        && crate::update::ist_commit(commit)
}

fn begrenze(wurzel: &Path) -> Result<(), String> {
    let mut namen = Vec::new();
    for e in fs::read_dir(wurzel).map_err(|e| format!("Sicherungen auflisten: {e}"))? {
        let e = e.map_err(|e| format!("Sicherungen auflisten: {e}"))?;
        let name = e.file_name();
        if name.to_str().is_some_and(eigener_name)
            && e.file_type()
                .map_err(|e| format!("Sicherung prüfen: {e}"))?
                .is_dir()
        {
            namen.push(name);
        }
    }
    namen.sort_unstable();
    let zu_viel = namen.len().saturating_sub(MAX_SICHERUNGEN);
    for name in namen.into_iter().take(zu_viel) {
        fs::remove_dir_all(wurzel.join(name))
            .map_err(|e| format!("Alte Sicherung entfernen: {e}"))?;
    }
    Ok(())
}
