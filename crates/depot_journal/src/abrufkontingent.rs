//! Dauerhafte Zählung der Alpha-Vantage-Versuche vor dem Netzabruf.

use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};

use depot_kern::{Datum, Zeitpunkt};

pub const AV_TAG: usize = 20;
const PRAEFIX: &str = "alphavantage-";
const ENDE: &str = ".kontingent";

fn name(tag: Datum) -> String {
    format!("{PRAEFIX}{tag}{ENDE}")
}

fn lies(daten: &Path, tag: Datum) -> Result<String, String> {
    let pfad = daten.join(name(tag));
    match fs::symlink_metadata(&pfad) {
        Ok(m) if m.is_file() && m.len() <= 4096 => (),
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(String::new()),
        _ => return Err("Kontingentdatei ist nicht regulär oder zu groß".into()),
    }
    let text = fs::read_to_string(pfad).map_err(|_| "Kontingentdatei ist unlesbar")?;
    if !text.is_empty() && !text.ends_with('\n') {
        return Err("Kontingentdatei endet mit einem unvollständigen Vermerk".into());
    }
    Ok(text)
}

fn kuerzel_gueltig(kuerzel: &str) -> bool {
    !kuerzel.is_empty()
        && kuerzel.len() <= 32
        && kuerzel
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
}

/// Versuche des heutigen Frankfurter Tages und im rollenden 24-Stunden-Fenster.
///
/// # Errors
/// Uhr oder ein relevanter Kontingentvermerk ist unlesbar.
pub fn stand(daten: &Path, jetzt: Zeitpunkt) -> Result<(usize, usize), String> {
    let seit = jetzt
        .sekunden()
        .checked_sub(86_400)
        .ok_or("Ungültige Kontingentuhr")?;
    let (heute, ..) = jetzt.in_frankfurt().ok_or("Ungültige Kontingentuhr")?;
    let (mut tag, ..) = Zeitpunkt::aus_sekunden(seit)
        .and_then(Zeitpunkt::in_frankfurt)
        .ok_or("Ungültige Kontingentuhr")?;
    let mut heute_n = 0_usize;
    let mut gesamt = 0_usize;
    loop {
        for zeile in lies(daten, tag)?.lines() {
            let (sek, kuerzel) = zeile
                .split_once(';')
                .ok_or("Ungültiger Kontingentvermerk")?;
            let zeit = sek
                .parse()
                .ok()
                .and_then(Zeitpunkt::aus_sekunden)
                .ok_or("Ungültiger Zeitpunkt im Kontingent")?;
            if zeit > jetzt
                || zeit.in_frankfurt().map(|(d, ..)| d) != Some(tag)
                || !kuerzel_gueltig(kuerzel)
            {
                return Err("Ungültiger Kontingentvermerk".into());
            }
            if tag == heute {
                heute_n = heute_n.saturating_add(1);
            }
            if zeit.sekunden() > seit || tag == heute {
                gesamt = gesamt.saturating_add(1);
            }
        }
        if tag == heute {
            return Ok((heute_n, gesamt));
        }
        tag = tag.plus_tage(1).ok_or("Ungültiger Kontingenttag")?;
    }
}

#[derive(Debug)]
pub struct Abrufe {
    daten: PathBuf,
    _sperre: File,
}

impl Abrufe {
    /// Sperrt die Vorprüfung und die Abrufserie gegen konkurrierende Aufträge.
    ///
    /// # Errors
    /// Die Sperre ist belegt oder die Datei ist nicht verwendbar.
    pub fn sperren(daten: &Path) -> Result<Self, String> {
        let pfad = daten.join("alphavantage.lock");
        if fs::symlink_metadata(&pfad).is_ok_and(|m| !m.is_file()) {
            return Err("Alpha-Vantage-Sperrdatei ist nicht regulär".into());
        }
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(pfad)
            .map_err(|_| "Alpha-Vantage-Sperrdatei ist nicht verfügbar")?;
        f.try_lock()
            .map_err(|_| "Alpha-Vantage-Kontingent ist belegt oder gesperrt")?;
        Ok(Self {
            daten: daten.to_path_buf(),
            _sperre: f,
        })
    }

    /// # Errors
    /// Das Kontingent ist unlesbar oder reicht nicht für die benötigten Versuche.
    pub fn pruefe(&self, jetzt: Zeitpunkt, benoetigt: usize) -> Result<(), String> {
        let (_, verbraucht) = stand(&self.daten, jetzt)?;
        let frei = AV_TAG.saturating_sub(verbraucht);
        if frei < benoetigt {
            return Err(format!(
                "Tageskontingent Alpha Vantage reicht nicht ({frei} von {AV_TAG} frei, {benoetigt} nötig)."
            ));
        }
        Ok(())
    }

    /// # Errors
    /// Kein freier Versuch oder der Vermerk konnte nicht dauerhaft geschrieben werden.
    pub fn vermerke(&self, jetzt: Zeitpunkt, kuerzel: &str) -> Result<(), String> {
        self.pruefe(jetzt, 1)?;
        let (tag, ..) = jetzt.in_frankfurt().ok_or("Ungültige Kontingentuhr")?;
        if !kuerzel_gueltig(kuerzel) {
            return Err("Ungültiges Kürzel für das Kontingent".into());
        }
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.daten.join(name(tag)))
            .map_err(|_| "Kontingentvermerk konnte nicht geöffnet werden")?;
        writeln!(f, "{};{kuerzel}", jetzt.sekunden())
            .and_then(|()| f.sync_all())
            .map_err(|_| "Kontingentvermerk konnte nicht dauerhaft gespeichert werden".into())
    }

    /// # Errors
    /// Der Kontingentordner kann nicht gelesen oder eine alte Datei nicht entfernt werden.
    pub fn raeume_auf(&self, heute: Datum) -> std::io::Result<()> {
        let Some(grenze) = heute.plus_tage(-7) else {
            return Ok(());
        };
        for e in fs::read_dir(&self.daten)? {
            let e = e?;
            let alt = e
                .file_name()
                .to_str()
                .and_then(|s| s.strip_prefix(PRAEFIX))
                .and_then(|s| s.strip_suffix(ENDE))
                .and_then(Datum::lies)
                .is_some_and(|tag| tag < grenze);
            if alt && e.file_type()?.is_file() {
                fs::remove_file(e.path())?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zeit(tag: u8, stunde: u8) -> Result<Zeitpunkt, String> {
        Datum::neu(2026, 11, tag)
            .and_then(|d| Zeitpunkt::frankfurt(d, stunde, 0, 0))
            .ok_or_else(|| "Testuhr fehlt".into())
    }

    #[test]
    fn zwei_laeufe_erschoepfen_das_rollende_kontingent() -> Result<(), String> {
        let daten = std::env::temp_dir().join(format!("depot-kontingent-{}", std::process::id()));
        let _ = fs::remove_dir_all(&daten);
        fs::create_dir(&daten).map_err(|e| e.to_string())?;
        let abrufe = Abrufe::sperren(&daten)?;
        assert!(Abrufe::sperren(&daten).is_err());
        for um in [zeit(9, 16)?, zeit(9, 18)?] {
            abrufe.pruefe(um, 10)?;
            for _ in 0..10 {
                abrufe.vermerke(um, "SAP.DEX")?;
            }
        }
        assert!(abrufe.pruefe(zeit(9, 22)?, 10).is_err());
        assert!(abrufe.vermerke(zeit(10, 8)?, "kalender").is_err());
        assert_eq!(stand(&daten, zeit(10, 8)?)?, (0, 20));
        assert_eq!(stand(&daten, zeit(10, 16)?)?, (0, 10));
        assert!(abrufe.pruefe(zeit(10, 16)?, 11).is_err());
        abrufe.pruefe(zeit(10, 16)?, 10)?;
        abrufe.vermerke(zeit(10, 16)?, "kalender")?;
        assert_eq!(stand(&daten, zeit(10, 16)?)?, (1, 11));
        let (heute, ..) = zeit(10, 16)?.in_frankfurt().ok_or("Datum fehlt")?;
        fs::write(daten.join(name(heute)), "unvollständig").map_err(|e| e.to_string())?;
        assert!(abrufe.vermerke(zeit(10, 16)?, "SAP.DEX").is_err());
        drop(abrufe);
        fs::remove_dir_all(daten).map_err(|e| e.to_string())?;
        Ok(())
    }
}
