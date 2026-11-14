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

/// Exklusive Sperre für die externe Sicherung und das Einspielen eines Updates.
pub fn sicherung_sperren(daten: &Path, belegt: &str) -> Result<File, String> {
    let pfad = daten.join("sicherung.lock");
    match fs::symlink_metadata(&pfad) {
        Ok(m) if m.is_file() => {}
        Ok(_) => return Err("Sicherungssperre ist keine reguläre Datei".into()),
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(format!("Sicherungssperre: {e}")),
    }
    let sperre = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(pfad)
        .map_err(|e| format!("Sicherungssperre: {e}"))?;
    sperre.try_lock().map_err(|e| match e {
        std::fs::TryLockError::WouldBlock => belegt.to_owned(),
        std::fs::TryLockError::Error(e) => format!("Sicherungssperre: {e}"),
    })?;
    Ok(sperre)
}

pub fn journal_sperren(daten: &Path) -> Result<Option<File>, String> {
    journal_sperren_fuer(daten, "Update-Sicherung")
}

pub fn journal_sperren_fuer(daten: &Path, zweck: &str) -> Result<Option<File>, String> {
    let pfad = daten.join("journal.txt");
    match fs::symlink_metadata(&pfad) {
        Ok(m) if m.is_file() => {}
        Ok(_) => return Err("Journal ist keine reguläre Datei".into()),
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Journal: {e}")),
    }
    let f = File::open(pfad).map_err(|e| format!("Journal: {e}"))?;
    f.try_lock_shared()
        .map_err(|e| format!("Journal für die {zweck} belegt oder nicht sperrbar: {e}"))?;
    Ok(Some(f))
}

pub fn lesen(daten: &Path) -> Result<Datenstand, String> {
    lies_auswahl(daten, &[])
}

pub fn lies_sicherung(daten: &Path) -> Result<Datenstand, String> {
    lies_auswahl(
        daten,
        &["anbieter.txt", "zeitplan.txt", "kurse.halt", "gemini.txt"],
    )
}

fn lies_auswahl(daten: &Path, weitere: &[&str]) -> Result<Datenstand, String> {
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
        if !DATEIEN.contains(&klein.as_str())
            && !weitere.contains(&klein.as_str())
            && !klein.starts_with("kassenbuch")
        {
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
        pruefe_kopie(name, original, &kopie)?;
    }
    Ok(())
}

fn pruefe_kopie(name: &str, original: &[u8], kopie: &[u8]) -> Result<(), String> {
    if Sha256::digest(kopie) != Sha256::digest(original) {
        return Err(format!("SHA-256 der Kopie von {name} stimmt nicht"));
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn prueft_nach_dem_kopieren() {
    assert!(pruefe_kopie("journal.txt", b"ganz", b"gan").is_err());
    assert!(pruefe_kopie("journal.txt", b"ganz", b"ganz").is_ok());
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
    let name = sichern_nach(&daten.join("sicherung"), stand, alt, MAX_SICHERUNGEN)?;
    println!("[Update] Geprüfte Sicherung: daten/sicherung/{name}");
    Ok(())
}

pub fn sichern_nach(
    wurzel: &Path,
    stand: &Datenstand,
    commit: &str,
    max: usize,
) -> Result<String, String> {
    if !crate::update::ist_commit(commit) {
        return Err("Ungültiger Commit für Sicherung".into());
    }
    if !verzeichnis(wurzel)? {
        privat_anlegen(wurzel).map_err(|e| format!("Sicherungsordner: {e}"))?;
    }
    for eintrag in fs::read_dir(wurzel).map_err(|e| format!("Sicherungen auflisten: {e}"))? {
        let eintrag = eintrag.map_err(|e| format!("Sicherungen auflisten: {e}"))?;
        if eintrag
            .file_name()
            .to_str()
            .and_then(|name| name.strip_suffix(".tmp"))
            .is_some_and(depot_export::sicherung::sicherungsname)
            && eintrag
                .file_type()
                .map_err(|e| format!("Sicherungsordner: {e}"))?
                .is_dir()
        {
            fs::remove_dir_all(eintrag.path())
                .map_err(|e| format!("Unfertige Sicherung aufräumen: {e}"))?;
        }
    }
    let utc = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("Zeit für Datensicherung: {e}"))?;
    let name = format!(
        "utc-{:020}-{:09}_{commit}",
        utc.as_secs(),
        utc.subsec_nanos()
    );
    let ziel = wurzel.join(&name);
    let staging = wurzel.join(format!("{name}.tmp"));
    privat_anlegen(&staging).map_err(|e| format!("Sicherungsordner: {e}"))?;
    if let Err(e) = kopiere(stand, &staging).and_then(|()| {
        crate::dateiersatz::ersetze(&staging, &ziel).map_err(|e| format!("Sicherung ablegen: {e}"))
    }) {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }
    begrenze(wurzel, max)?;
    Ok(name)
}

fn begrenze(wurzel: &Path, max: usize) -> Result<(), String> {
    let mut namen = Vec::new();
    for e in fs::read_dir(wurzel).map_err(|e| format!("Sicherungen auflisten: {e}"))? {
        let e = e.map_err(|e| format!("Sicherungen auflisten: {e}"))?;
        let name = e.file_name();
        if name
            .to_str()
            .is_some_and(depot_export::sicherung::sicherungsname)
            && e.file_type()
                .map_err(|e| format!("Sicherung prüfen: {e}"))?
                .is_dir()
        {
            namen.push(name);
        }
    }
    namen.sort_unstable();
    let zu_viel = namen.len().saturating_sub(max);
    for name in namen.into_iter().take(zu_viel) {
        fs::remove_dir_all(wurzel.join(name))
            .map_err(|e| format!("Alte Sicherung entfernen: {e}"))?;
    }
    Ok(())
}
