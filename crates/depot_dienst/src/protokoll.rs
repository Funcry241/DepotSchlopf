//! Dauerhafte, begrenzte Diagnoseprotokolle. Geschäftsdaten stehen weiterhin im Journal.

use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use depot_kern::Zeitpunkt;

pub const AUFBEWAHREN: usize = 20;
pub const DATEI_MAX: u64 = 16 * 1024 * 1024;
pub const ZEILE_MAX: usize = 64 * 1024;
const ENDE_RESERVE: u64 = 1024;

/// Geheimnisse werden weder formatiert noch in Fehlermeldungen übernommen.
#[derive(Default)]
pub(crate) struct Schutz {
    schluessel: Vec<String>,
    gesperrt: bool,
}

impl Schutz {
    pub(crate) fn laden() -> Self {
        match crate::schluessel::fuer_protokoll() {
            Ok(schluessel) => Self {
                schluessel,
                gesperrt: false,
            },
            Err(()) => Self {
                schluessel: Vec::new(),
                gesperrt: true,
            },
        }
    }

    pub(crate) fn auffrischen(&mut self) {
        match crate::schluessel::fuer_protokoll() {
            Ok(alle) => {
                for text in alle {
                    self.merken(&text);
                }
                self.gesperrt = false;
            }
            Err(()) => self.gesperrt = true,
        }
    }

    pub(crate) fn merken(&mut self, text: &str) {
        if !self.schluessel.iter().any(|s| s == text) {
            self.schluessel.push(text.to_string());
        }
    }

    pub(crate) fn bereinige(&self, text: &str) -> String {
        if self.gesperrt {
            return "[Ausgabe unterdrückt. Schlüsselschutz nicht verfügbar.]".into();
        }
        let enthaelt = |text: &str| {
            let gross = text.to_ascii_uppercase();
            self.schluessel
                .iter()
                .any(|s| gross.contains(&s.to_ascii_uppercase()))
        };
        if enthaelt(text)
            || depot_kern::json::alle_texte(text)
                .iter()
                .any(|t| enthaelt(t))
        {
            "[Ausgabe mit Zugangsdaten entfernt.]".into()
        } else {
            text.to_string()
        }
    }
}

pub fn gueltige_kennung(kennung: &str) -> bool {
    let Some((zeit, nr)) = kennung.split_once('-') else {
        return false;
    };
    kennung.len() <= 40
        && zeit.parse::<u64>().is_ok_and(|n| n.to_string() == zeit)
        && nr.parse::<u64>().is_ok_and(|n| n.to_string() == nr)
}

fn ordner(wurzel: &Path) -> PathBuf {
    wurzel.join("daten").join("protokolle")
}

fn dateipfad(wurzel: &Path, kennung: &str) -> Result<PathBuf, String> {
    if !gueltige_kennung(kennung) {
        return Err("Unbekannte Laufkennung.".into());
    }
    Ok(ordner(wurzel).join(format!("{kennung}.log")))
}

fn gewoehnlich(pfad: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(pfad).map_err(|_| "Protokoll nicht verfügbar.")?;
    if !meta.is_file() {
        return Err("Protokoll ist keine reguläre Datei.".into());
    }
    Ok(())
}

/// Liest höchstens die zugesagte Dateigröße. Ein fehlendes altes Log wird erklärt.
pub fn lesen(wurzel: &Path, kennung: &str) -> Result<String, String> {
    let pfad = dateipfad(wurzel, kennung)?;
    if fs::symlink_metadata(ordner(wurzel))
        .ok()
        .is_none_or(|m| !m.is_dir())
    {
        return Err("Protokollordner nicht verfügbar.".into());
    }
    gewoehnlich(&pfad)?;
    let mut text = String::new();
    File::open(pfad)
        .and_then(|f| f.take(DATEI_MAX + 1).read_to_string(&mut text))
        .map_err(|_| "Protokoll konnte nicht gelesen werden.")?;
    if u64::try_from(text.len()).unwrap_or(u64::MAX) > DATEI_MAX {
        return Err("Protokoll überschreitet die zulässige Größe.".into());
    }
    Ok(text)
}

fn aufraeumen(wurzel: &Path, aktuell: &str) -> Result<(), String> {
    let mut dateien = Vec::new();
    for eintrag in fs::read_dir(ordner(wurzel)).map_err(|_| "Protokollordner nicht lesbar.")? {
        let e = eintrag.map_err(|_| "Protokollordner nicht lesbar.")?;
        let name = e.file_name();
        let Some(kennung) = name.to_str().and_then(|n| n.strip_suffix(".log")) else {
            continue;
        };
        if gueltige_kennung(kennung) && e.file_type().is_ok_and(|t| t.is_file()) {
            let zeit = kennung
                .split_once('-')
                .and_then(|(t, _)| t.parse::<u64>().ok())
                .unwrap_or(0);
            let nr = kennung
                .split_once('-')
                .and_then(|(_, n)| n.parse::<u64>().ok())
                .unwrap_or(0);
            dateien.push((zeit, nr, kennung.to_string(), e.path()));
        }
    }
    dateien.sort();
    let entfernen = dateien.len().saturating_sub(AUFBEWAHREN);
    for (_, _, kennung, pfad) in dateien
        .into_iter()
        .filter(|(_, _, k, _)| k != aktuell)
        .take(entfernen)
    {
        fs::remove_file(pfad).map_err(|_| format!("Altes Protokoll {kennung} nicht löschbar."))?;
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) struct Datei {
    ausgabe: File,
    bytes: u64,
    voll: bool,
}

impl Datei {
    pub(crate) fn neu(
        wurzel: &Path,
        kennung: &str,
        version: Option<&str>,
        start: Zeitpunkt,
        art: &str,
    ) -> Result<Self, String> {
        let p = dateipfad(wurzel, kennung)?;
        fs::create_dir_all(ordner(wurzel)).map_err(|_| "Protokollordner nicht schreibbar.")?;
        if fs::symlink_metadata(ordner(wurzel))
            .ok()
            .is_none_or(|m| !m.is_dir())
        {
            return Err("Protokollordner ist kein regulärer Ordner.".into());
        }
        let datei = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(p)
            .map_err(|_| "Protokoll konnte nicht neu angelegt werden.")?;
        let mut log = Self {
            ausgabe: datei,
            bytes: 0,
            voll: false,
        };
        log.zeile(
            start,
            &format!(
                "Lauf {kennung} · {art} · Version {}",
                version.unwrap_or("unbekannt")
            ),
        )?;
        aufraeumen(wurzel, kennung)?;
        Ok(log)
    }

    pub(crate) fn zeile(&mut self, zeit: Zeitpunkt, text: &str) -> Result<(), String> {
        if self.voll {
            return Ok(());
        }
        let mut zeile = format!("[{zeit}] {text}\n");
        let n = u64::try_from(zeile.len()).unwrap_or(u64::MAX);
        if self.bytes.saturating_add(n) > DATEI_MAX.saturating_sub(ENDE_RESERVE) {
            zeile = format!(
                "[{zeit}] Protokollgrenze erreicht. Weitere Ausgabe wird nicht gespeichert.\n"
            );
            self.voll = true;
        }
        self.ausgabe
            .write_all(zeile.as_bytes())
            .and_then(|()| self.ausgabe.flush())
            .map_err(|_| "Protokoll konnte nicht geschrieben werden.")?;
        self.bytes = self
            .bytes
            .saturating_add(u64::try_from(zeile.len()).unwrap_or(u64::MAX));
        Ok(())
    }

    pub(crate) fn abschliessen(mut self, zeit: Zeitpunkt, erfolgreich: bool) -> Result<(), String> {
        let ende = if erfolgreich {
            "Lauf erfolgreich beendet."
        } else {
            "Lauf abgebrochen."
        };
        self.ausgabe
            .write_all(format!("[{zeit}] {ende}\n").as_bytes())
            .map_err(|_| "Protokollabschluss nicht schreibbar.")?;
        self.ausgabe
            .sync_all()
            .map_err(|_| "Protokoll konnte nicht dauerhaft gespeichert werden.".into())
    }
}

/// Letzte Fehlermeldung des Laufs, ohne erfolgreiche Testfälle als Fehler auszugeben.
pub fn letzter_fehler(zeilen: &[String]) -> Option<&str> {
    zeilen
        .iter()
        .rev()
        .find(|z| {
            let klein = z.to_ascii_lowercase();
            !klein.ends_with(" ... ok")
                && [
                    "fehler",
                    "error",
                    "failed",
                    "panicked",
                    "fehlgeschlagen",
                    "nicht gestartet",
                ]
                .iter()
                .any(|s| klein.contains(s))
        })
        .map(String::as_str)
}

/// Liest auch fehlerhafte oder überlange Ausgaben mit begrenztem Speicher.
pub(crate) fn zeilen(mut leser: impl std::io::BufRead, mut ausgabe: impl FnMut(String)) {
    let mut zeile = Vec::new();
    let mut zu_lang = false;
    loop {
        let Ok(puffer) = leser.fill_buf() else {
            ausgabe("FEHLER. Prozessausgabe konnte nicht vollständig gelesen werden.".into());
            break;
        };
        if puffer.is_empty() {
            if zu_lang || !zeile.is_empty() {
                ausgabe(zeilentext(&zeile, zu_lang));
            }
            break;
        }
        let ende = puffer
            .iter()
            .position(|b| *b == b'\n')
            .map(|n| n.saturating_add(1));
        let menge = ende.unwrap_or(puffer.len());
        if !zu_lang && zeile.len().saturating_add(menge) <= ZEILE_MAX {
            zeile.extend_from_slice(puffer.get(..menge).unwrap_or_default());
        } else {
            zu_lang = true;
            zeile.clear();
        }
        leser.consume(menge);
        if ende.is_some() {
            ausgabe(zeilentext(&zeile, zu_lang));
            zeile.clear();
            zu_lang = false;
        }
    }
}

fn zeilentext(zeile: &[u8], zu_lang: bool) -> String {
    if zu_lang {
        return "FEHLER. Eine überlange Ausgabezeile wurde aus dem Protokoll ausgelassen.".into();
    }
    String::from_utf8_lossy(zeile)
        .trim_end_matches(['\r', '\n'])
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schluessel_werden_auch_hinter_escapes_entfernt() {
        let mut schutz = Schutz::default();
        schutz.merken(&["GEHEIM", "SCHLUESSEL"].concat());
        for t in ["text GEHEIMSCHLUESSEL", r#"{"x":"\u0047EHEIMSCHLUESSEL"}"#] {
            assert_eq!(schutz.bereinige(t), "[Ausgabe mit Zugangsdaten entfernt.]");
        }
        assert_eq!(
            schutz.bereinige("error: etwas ging schief"),
            "error: etwas ging schief"
        );
    }

    #[test]
    fn kennungen_lassen_keine_pfade_zu() {
        assert!(gueltige_kennung("1794000000-1"));
        for k in ["../1", "-1", "1-../2", "01-1", "1-2-3"] {
            assert!(!gueltige_kennung(k));
        }
    }
    #[test]
    fn unlesbare_schluessel_unterdruecken_ausgabe() {
        let schutz = Schutz {
            schluessel: Vec::new(),
            gesperrt: true,
        };
        assert_eq!(
            schutz.bereinige("irgendeine Ausgabe"),
            "[Ausgabe unterdrückt. Schlüsselschutz nicht verfügbar.]"
        );
    }

    #[test]
    fn ueberlange_zeilen_werden_ganz_ausgelassen() {
        let mut eingabe = vec![b'x'; ZEILE_MAX.saturating_add(100)];
        eingabe.extend_from_slice(b"\nFEHLER: danach\r\n");
        let mut ausgabe = Vec::new();
        zeilen(std::io::Cursor::new(eingabe), |z| ausgabe.push(z));
        assert_eq!(ausgabe.len(), 2);
        assert!(ausgabe.first().is_some_and(|z| z.contains("überlange")));
        assert_eq!(ausgabe.last().map(String::as_str), Some("FEHLER: danach"));
    }

    #[test]
    fn rotation_groessenlimit_und_abschluss() -> Result<(), String> {
        let wurzel =
            std::env::temp_dir().join(format!("depot_log_rotation_{}", std::process::id()));
        let _ = fs::remove_dir_all(&wurzel);
        let zeit = depot_kern::Datum::neu(2026, 11, 5)
            .and_then(|d| Zeitpunkt::frankfurt(d, 12, 0, 0))
            .ok_or("Testzeit fehlt")?;
        for n in 0..AUFBEWAHREN.saturating_add(3) {
            let kennung = format!("1-{n}");
            let mut datei = Datei::neu(&wurzel, &kennung, None, zeit, "Test")?;
            if n == AUFBEWAHREN.saturating_add(2) {
                datei.bytes = DATEI_MAX.saturating_sub(ENDE_RESERVE);
                datei.zeile(zeit, "geheimer, zu großer Rest")?;
                datei.zeile(zeit, "auch nicht gespeichert")?;
            }
            datei.abschliessen(zeit, false)?;
        }
        let anzahl = fs::read_dir(ordner(&wurzel))
            .map_err(|e| e.to_string())?
            .count();
        assert_eq!(anzahl, AUFBEWAHREN);
        assert!(lesen(&wurzel, "1-0").is_err());
        let text = lesen(&wurzel, &format!("1-{}", AUFBEWAHREN.saturating_add(2)))?;
        assert!(text.contains("Protokollgrenze erreicht"));
        assert!(text.contains("Lauf abgebrochen"));
        assert!(!text.contains("geheimer"));
        assert!(!text.contains("auch nicht"));
        fs::remove_dir_all(wurzel).map_err(|e| e.to_string())?;
        Ok(())
    }
}
