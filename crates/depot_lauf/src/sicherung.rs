//! Geprüfte Sicherung der freigegebenen Laufzeitdateien auf ein vorhandenes Ziel.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::updatedaten::{journal_sperren_fuer, lies_sicherung, sichern_nach, sicherung_sperren};

const KOPF: &str = "sicherung-stand-v1";
const STANDDATEI: &str = "sicherung_stand.txt";

#[derive(Default)]
struct Stand {
    auto_start: String,
    start: String,
    erfolg: String,
    dateien: String,
    gleicher_datentraeger: String,
    fehler: String,
}

impl Stand {
    fn lies(daten: &Path) -> Self {
        let Ok(text) = fs::read_to_string(daten.join(STANDDATEI)) else {
            return Self::default();
        };
        let mut zeilen = text.lines();
        if zeilen.next() != Some(KOPF) {
            return Self::default();
        }
        let mut stand = Self::default();
        for (name, wert) in zeilen.filter_map(|z| z.split_once('=')) {
            let feld = match name {
                "auto_start" => &mut stand.auto_start,
                "start" => &mut stand.start,
                "erfolg" => &mut stand.erfolg,
                "dateien" => &mut stand.dateien,
                "gleicher_datentraeger" => &mut stand.gleicher_datentraeger,
                "fehler" => &mut stand.fehler,
                _ => continue,
            };
            wert.clone_into(feld);
        }
        stand
    }

    fn schreibe(&self, daten: &Path, ergebnis: &str) -> Result<(), String> {
        let text = format!(
            "{KOPF}\nauto_start={}\nstart={}\nergebnis={ergebnis}\nerfolg={}\ndateien={}\ngleicher_datentraeger={}\nfehler={}\n",
            self.auto_start,
            self.start,
            self.erfolg,
            self.dateien,
            self.gleicher_datentraeger,
            self.fehler,
        );
        let tmp = daten.join("sicherung_stand.txt.tmp");
        match fs::remove_file(&tmp) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Sicherungsstand: {e}")),
        }
        let mut ausgabe = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| format!("Sicherungsstand: {e}"))?;
        ausgabe
            .write_all(text.as_bytes())
            .and_then(|()| ausgabe.sync_all())
            .map_err(|e| format!("Sicherungsstand: {e}"))?;
        drop(ausgabe);
        crate::dateiersatz::ersetze(tmp, daten.join(STANDDATEI))
            .map_err(|e| format!("Sicherungsstand: {e}"))
    }
}

fn zielordner(wurzel: &Path, ziel: &Path) -> Result<(PathBuf, bool), String> {
    if !ziel.is_absolute() {
        return Err("Zielordner muss ein absoluter Pfad sein".into());
    }
    let meta = match fs::symlink_metadata(ziel) {
        Ok(m) if m.is_dir() => m,
        Ok(_) => return Err("Zielordner ist kein reguläres Verzeichnis".into()),
        Err(e) if e.kind() == ErrorKind::NotFound => return Err("Zielordner fehlt".into()),
        Err(e) => return Err(format!("Zielordner: {e}")),
    };
    let ziel = fs::canonicalize(ziel).map_err(|e| format!("Zielordner: {e}"))?;
    let wurzel = fs::canonicalize(wurzel).map_err(|e| format!("Projektordner: {e}"))?;
    if ziel.starts_with(&wurzel) {
        return Err("Zielordner liegt im Projekt".into());
    }
    #[cfg(unix)]
    let gleich = {
        use std::os::unix::fs::MetadataExt as _;
        fs::metadata(&wurzel)
            .map_err(|e| format!("Projektordner: {e}"))?
            .dev()
            == meta.dev()
    };
    #[cfg(not(unix))]
    let gleich = {
        let _ = meta;
        wurzel.components().next() == ziel.components().next()
    };
    Ok((ziel, gleich))
}

pub(crate) fn sichern(wurzel: &Path, auto: bool) -> Result<(), String> {
    let daten = wurzel.join("daten");
    let sperre = sicherung_sperren(&daten, "Sicherung oder Update läuft gerade")?;
    let ergebnis = sichere_mit_sperre(wurzel, &daten, auto);
    let _ = sperre.unlock();
    ergebnis
}

fn sichere_mit_sperre(wurzel: &Path, daten: &Path, auto: bool) -> Result<(), String> {
    let zeit = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("Zeit der Sicherung: {e}"))?
        .as_secs()
        .to_string();
    let mut stand = Stand::lies(daten);
    let anlass = if auto { "auto" } else { "hand" };
    if auto {
        stand.auto_start.clone_from(&zeit);
    }
    stand.start = format!("{zeit};{anlass}");
    stand.fehler.clear();
    stand.schreibe(daten, "laeuft")?;
    let ergebnis = (|| -> Result<_, String> {
        let text = fs::read_to_string(daten.join("sicherung_ziel.txt"))
            .map_err(|e| format!("Sicherungsziel: {e}"))?;
        let text = text.trim_end_matches(['\r', '\n']);
        if text.is_empty() || text.contains(['\r', '\n', '\0']) {
            return Err("Sicherungsziel fehlt oder ist ungültig".into());
        }
        let (ziel, gleich) = zielordner(wurzel, Path::new(text))?;
        let commit = crate::abbild::git_text(wurzel, &["rev-parse", "HEAD"])?;
        let _journal = journal_sperren_fuer(daten, "Sicherung")?;
        let inhalt = lies_sicherung(daten)?;
        if inhalt.is_empty() {
            return Err("Keine Laufzeitdaten für die Sicherung vorhanden".into());
        }
        let name = sichern_nach(&ziel.join("DepotSchlopf"), &inhalt, &commit, 30)?;
        Ok((name, inhalt.len(), gleich))
    })();
    match ergebnis {
        Ok((name, dateien, gleich)) => {
            stand.erfolg = zeit;
            stand.dateien = dateien.to_string();
            stand.gleicher_datentraeger = if gleich { "ja" } else { "nein" }.into();
            stand.schreibe(daten, "ok")?;
            println!("Geprüfte Sicherung: {dateien} Dateien in {name}");
            if gleich {
                println!("Gleicher Datenträger: schützt nicht vor Plattenausfall");
            }
            Ok(())
        }
        Err(e) => {
            e.lines()
                .next()
                .unwrap_or_default()
                .clone_into(&mut stand.fehler);
            stand.schreibe(daten, "fehler")?;
            Err(e)
        }
    }
}

pub fn lauf(wurzel: &Path, optionen: &[String]) -> ExitCode {
    let auto = match optionen {
        [] => false,
        [option] if option == "--auto" => true,
        _ => {
            eprintln!("Aufruf: run.cmd sichern");
            return ExitCode::FAILURE;
        }
    };
    match sichern(wurzel, auto) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("[Sicherung] {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NR: AtomicUsize = AtomicUsize::new(0);
    pub(super) struct Ort(pub(super) PathBuf);

    impl Ort {
        pub(super) fn neu() -> Result<Self, String> {
            let ort = Self(std::env::temp_dir().join(format!(
                "depot-sicherung-{}-{}",
                std::process::id(),
                NR.fetch_add(1, Ordering::Relaxed)
            )));
            fs::create_dir_all(ort.0.join("projekt/daten")).map_err(|e| e.to_string())?;
            fs::create_dir(ort.0.join("ziel")).map_err(|e| e.to_string())?;
            let projekt = ort.0.join("projekt");
            crate::abbild::git_text(&projekt, &["init", "-q"])?;
            crate::abbild::git_text(
                &projekt,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "commit",
                    "-q",
                    "--allow-empty",
                    "-m",
                    "Sicherungsprobe",
                ],
            )?;
            fs::write(
                projekt.join("daten/sicherung_ziel.txt"),
                ort.0.join("ziel").display().to_string(),
            )
            .map_err(|e| e.to_string())?;
            fs::write(projekt.join("daten/journal.txt"), b"Journal").map_err(|e| e.to_string())?;
            Ok(ort)
        }

        pub(super) fn projekt(&self) -> PathBuf {
            self.0.join("projekt")
        }
    }

    impl Drop for Ort {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn sichert_und_prueft() -> Result<(), String> {
        let ort = Ort::neu()?;
        fs::write(ort.projekt().join("daten/papierkonto.txt"), b"Papier")
            .map_err(|e| e.to_string())?;
        fs::write(ort.projekt().join("daten/nicht_freigegeben.txt"), b"Privat")
            .map_err(|e| e.to_string())?;
        sichern(&ort.projekt(), false)?;
        let mut kopien =
            fs::read_dir(ort.0.join("ziel/DepotSchlopf")).map_err(|e| e.to_string())?;
        let kopie = kopien
            .next()
            .ok_or("Keine Sicherung")?
            .map_err(|e| e.to_string())?
            .path();
        assert!(kopien.next().is_none());
        for datei in ["journal.txt", "papierkonto.txt"] {
            assert_eq!(
                fs::read(kopie.join(datei)).map_err(|e| e.to_string())?,
                fs::read(ort.projekt().join("daten").join(datei)).map_err(|e| e.to_string())?
            );
        }
        assert_eq!(fs::read_dir(kopie).map_err(|e| e.to_string())?.count(), 2);
        Ok(())
    }

    #[test]
    fn ziel_fehlt() -> Result<(), String> {
        let ort = Ort::neu()?;
        let ziel = ort.0.join("ziel");
        fs::remove_dir(&ziel).map_err(|e| e.to_string())?;
        assert_eq!(
            sichern(&ort.projekt(), false),
            Err("Zielordner fehlt".into())
        );
        assert!(!ziel.exists());
        Ok(())
    }

    #[test]
    fn ziel_ist_datei() -> Result<(), String> {
        let ort = Ort::neu()?;
        let ziel = ort.0.join("ziel");
        fs::remove_dir(&ziel).map_err(|e| e.to_string())?;
        fs::write(&ziel, b"Datei").map_err(|e| e.to_string())?;
        assert!(sichern(&ort.projekt(), false).is_err());
        assert!(ziel.is_file());
        Ok(())
    }

    #[test]
    fn ziel_im_projekt() -> Result<(), String> {
        let ort = Ort::neu()?;
        let daten = ort.projekt().join("daten");
        fs::write(
            daten.join("sicherung_ziel.txt"),
            daten.display().to_string(),
        )
        .map_err(|e| e.to_string())?;
        assert_eq!(
            sichern(&ort.projekt(), false),
            Err("Zielordner liegt im Projekt".into())
        );
        assert!(!daten.join("DepotSchlopf").exists());
        Ok(())
    }

    #[test]
    fn behaelt_30() -> Result<(), String> {
        let ort = Ort::neu()?;
        for _ in 0..32 {
            sichern(&ort.projekt(), false)?;
        }
        assert_eq!(
            fs::read_dir(ort.0.join("ziel/DepotSchlopf"))
                .map_err(|e| e.to_string())?
                .count(),
            30
        );
        Ok(())
    }

    #[test]
    fn aufraeumen_nur_eigene_tmp() -> Result<(), String> {
        let ort = Ort::neu()?;
        let ziel = ort.0.join("ziel/DepotSchlopf");
        let eigen = ziel.join(format!("utc-{:020}-{:09}_{}.tmp", 1, 0, "a".repeat(40)));
        let fremd = ziel.join("fremd.tmp");
        fs::create_dir_all(&eigen)
            .and_then(|()| fs::create_dir(&fremd))
            .map_err(|e| e.to_string())?;
        sichern(&ort.projekt(), false)?;
        assert!(!eigen.exists());
        assert!(fremd.is_dir());
        Ok(())
    }

    #[test]
    fn journal_gesperrt() -> Result<(), String> {
        let ort = Ort::neu()?;
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .open(ort.projekt().join("daten/journal.txt"))
            .map_err(|e| e.to_string())?;
        f.try_lock().map_err(|e| e.to_string())?;
        assert!(sichern(&ort.projekt(), false).is_err_and(|e| e.contains("belegt")));
        assert!(!ort.0.join("ziel/DepotSchlopf").exists());
        Ok(())
    }

    #[test]
    fn zwei_sicherungen_gleichzeitig() -> Result<(), String> {
        let ort = Ort::neu()?;
        let daten = ort.projekt().join("daten");
        let sperre = sicherung_sperren(&daten, "Sicherung belegt")?;
        let stand = daten.join(STANDDATEI);
        fs::write(&stand, "unverändert".as_bytes()).map_err(|e| e.to_string())?;
        assert_eq!(
            sichern(&ort.projekt(), false),
            Err("Sicherung oder Update läuft gerade".into())
        );
        assert_eq!(
            fs::read(&stand).map_err(|e| e.to_string())?,
            "unverändert".as_bytes()
        );
        sperre.unlock().map_err(|e| e.to_string())?;
        drop(sperre);
        sichern(&ort.projekt(), false)
    }

    #[test]
    fn sicherung_schreibt_keine_sperre_datei() -> Result<(), String> {
        let ort = Ort::neu()?;
        sichern(&ort.projekt(), false)?;
        fs::remove_dir_all(ort.0.join("ziel")).map_err(|e| e.to_string())?;
        assert!(sichern(&ort.projekt(), false).is_err());
        assert!(
            fs::read_dir(ort.projekt().join("daten"))
                .map_err(|e| e.to_string())?
                .all(|e| e.is_ok_and(|e| !e.file_name().to_string_lossy().ends_with(".sperre")))
        );
        Ok(())
    }
}

#[cfg(test)]
mod status_tests {
    use super::tests::Ort;
    use super::*;

    #[test]
    fn hand_am_selben_tag() -> Result<(), String> {
        let ort = Ort::neu()?;
        let wurzel = ort.projekt();
        let ziel = ort.0.join("ziel");
        let daten = wurzel.join("daten");
        fs::remove_dir(&ziel).map_err(|e| e.to_string())?;
        assert!(sichern(&wurzel, true).is_err());
        let auto_start = Stand::lies(&daten).auto_start;
        assert!(!auto_start.is_empty());
        fs::create_dir(&ziel).map_err(|e| e.to_string())?;
        sichern(&wurzel, false)?;
        let stand = Stand::lies(&daten);
        assert_eq!(stand.auto_start, auto_start);
        assert!(stand.start.ends_with(";hand"));
        assert!(!stand.erfolg.is_empty());
        Ok(())
    }

    #[test]
    fn stand_enthaelt_start_und_erfolg() -> Result<(), String> {
        let ort = Ort::neu()?;
        let wurzel = ort.projekt();
        sichern(&wurzel, true)?;
        let daten = wurzel.join("daten");
        let stand = Stand::lies(&daten);
        assert!(stand.start.ends_with(";auto"));
        assert_eq!(stand.erfolg, stand.auto_start);
        let text = fs::read_to_string(daten.join(STANDDATEI)).map_err(|e| e.to_string())?;
        assert!(text.lines().any(|z| z == "ergebnis=ok"));
        assert_eq!(stand.dateien, "1");
        #[cfg(windows)]
        assert_eq!(stand.gleicher_datentraeger, "ja");
        Ok(())
    }

    #[test]
    fn fehlversuche_behalten_letzten_erfolg() -> Result<(), String> {
        let ort = Ort::neu()?;
        let wurzel = ort.projekt();
        let daten = wurzel.join("daten");
        sichern(&wurzel, false)?;
        let erfolg = Stand::lies(&daten).erfolg;
        fs::remove_dir_all(ort.0.join("ziel")).map_err(|e| e.to_string())?;
        for _ in 0..3 {
            assert!(sichern(&wurzel, false).is_err());
            let stand = Stand::lies(&daten);
            assert_eq!(stand.erfolg, erfolg);
            assert_eq!(stand.fehler, "Zielordner fehlt");
        }
        Ok(())
    }

    #[test]
    fn sichert_bekannte_dateien() -> Result<(), String> {
        let ort = Ort::neu()?;
        let daten = ort.projekt().join("daten");
        for datei in [
            "kurse.halt",
            "gemini.txt",
            "anbieter.txt",
            "zeitplan.txt",
            "sec_kontakt.txt",
            "update_abhaengigkeiten.txt",
            "fremd.txt",
        ] {
            fs::write(daten.join(datei), b"Probe").map_err(|e| e.to_string())?;
        }
        let inhalt = lies_sicherung(&daten)?;
        assert_eq!(
            inhalt.keys().map(String::as_str).collect::<Vec<_>>(),
            [
                "anbieter.txt",
                "gemini.txt",
                "journal.txt",
                "kurse.halt",
                "zeitplan.txt"
            ]
        );
        assert_eq!(crate::updatedaten::lesen(&daten)?.len(), 1);
        Ok(())
    }

    #[test]
    fn schreibt_nur_stand() -> Result<(), String> {
        let ort = Ort::neu()?;
        let wurzel = ort.projekt();
        let daten = wurzel.join("daten");
        sicherung_sperren(&daten, "Belegt")?
            .unlock()
            .map_err(|e| e.to_string())?;
        let snapshot = || -> Result<std::collections::BTreeMap<_, _>, String> {
            fs::read_dir(&daten)
                .map_err(|e| e.to_string())?
                .filter_map(|e| {
                    let e = match e {
                        Ok(e) => e,
                        Err(e) => return Some(Err(e.to_string())),
                    };
                    (e.file_name() != STANDDATEI).then(|| {
                        fs::read(e.path())
                            .map(|inhalt| (e.file_name(), inhalt))
                            .map_err(|e| e.to_string())
                    })
                })
                .collect()
        };
        let vorher = snapshot()?;
        sichern(&wurzel, false)?;
        assert_eq!(snapshot()?, vorher);
        Ok(())
    }

    #[test]
    fn ziel_ist_symlink() -> Result<(), String> {
        let ort = Ort::neu()?;
        let ziel = ort.0.join("ziel");
        let link = ort.0.join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&ziel, &link).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        if let Err(e) = std::os::windows::fs::symlink_dir(&ziel, &link) {
            if e.kind() == ErrorKind::PermissionDenied || e.raw_os_error() == Some(1314) {
                eprintln!("Symlink-Test: Windows erlaubt das Anlegen nicht ({e})");
                return Ok(());
            }
            return Err(e.to_string());
        }
        assert!(zielordner(&ort.projekt(), &link).is_err());
        Ok(())
    }
}
