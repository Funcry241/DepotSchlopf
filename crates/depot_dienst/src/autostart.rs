//! Autostart mit Windows: eine kleine Datei `DepotSchlopf.cmd` im Autostart-Ordner des
//! Benutzers (`%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup`). Sie ruft nur den
//! Starter `DepotSchlopf.cmd` im Projektordner, ohne Browser und mit kleinem Fenster.
//! Kein Dienst, keine Registry, keine Adminrechte; entfernen loescht genau diese Datei.

use std::fs;
use std::io::{ErrorKind, Read as _, Write as _};
use std::path::{Path, PathBuf};

const DATEI: &str = "DepotSchlopf.cmd";
const KOPF: &str = "rem Autostart von DepotSchlopf. Entfernen über die Einrichtung.";
const KOPF_ASCII: &str = "rem Autostart von DepotSchlopf. Entfernen ueber die Einrichtung.";
const START: &str = "start \"DepotSchlopf\" /min \"";
const ENDE: &str = "\\DepotSchlopf.cmd\" --ohne-browser";
const MAX_DATEI: u64 = 4096;

/// Der Autostart-Ordner des Benutzers, wenn `APPDATA` gesetzt ist.
pub fn ordner() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup"),
    )
}

/// Inhalt der Autostart-Datei fuer diesen Projektordner.
///
/// # Errors
/// Der Pfad enthaelt Zeichen, die in einer Befehlsdatei etwas anderes bedeuten koennten.
pub fn inhalt(wurzel: &Path) -> Result<String, String> {
    let w = sicherer_pfad(wurzel)?;
    Ok(format!("@echo off\r\n{KOPF}\r\n{START}{w}{ENDE}\r\n"))
}

/// Ein Pfad als Text, der in einer Befehlsdatei zwischen Anfuehrungszeichen nichts anderes
/// bedeuten kann.
///
/// # Errors
/// Kein gueltiger Text, leer, oder mit Sonderzeichen.
pub fn sicherer_pfad(p: &Path) -> Result<&str, String> {
    let w = p.to_str().ok_or("Pfad ist kein gültiger Text")?;
    if w.is_empty()
        || w.chars()
            .any(|c| c.is_control() || matches!(c, '"' | '%' | '!' | '^' | '&' | '|' | '<' | '>'))
    {
        return Err("Pfad enthält Sonderzeichen (\" % ! ^ & | < >)".into());
    }
    Ok(w)
}

/// Zu welchem Projektordner der Autostart gehört.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stand {
    Aus,
    An,
    Veraltet { ziel: PathBuf },
    Fremd,
    NichtVerfuegbar,
}

impl Stand {
    /// Status ohne lokalen Pfad für den Analyse-Export.
    pub(crate) fn exporttext(&self) -> &'static str {
        match self {
            Self::Aus => "Aus",
            Self::An => "An",
            Self::Veraltet { .. } => "Veraltet",
            Self::Fremd => "Fremd",
            Self::NichtVerfuegbar => "NichtVerfuegbar",
        }
    }
}

fn eigenes_ziel(text: &str) -> Option<&str> {
    let mut zeilen = text.lines();
    if zeilen.next()? != "@echo off" || !matches!(zeilen.next()?, KOPF | KOPF_ASCII) {
        return None;
    }
    let ziel = zeilen.next()?.strip_prefix(START)?.strip_suffix(ENDE)?;
    if zeilen.next().is_some() {
        return None;
    }
    sicherer_pfad(Path::new(ziel)).ok()
}

fn datei_stand(p: &Path, soll: &str) -> Stand {
    match fs::symlink_metadata(p) {
        Err(e) if e.kind() == ErrorKind::NotFound => Stand::Aus,
        Err(_) => Stand::NichtVerfuegbar,
        Ok(m) if !m.is_file() || m.len() > MAX_DATEI => Stand::Fremd,
        Ok(_) => {
            let mut text = String::new();
            if let Err(e) = fs::File::open(p).and_then(|f| {
                f.take(MAX_DATEI.saturating_add(1))
                    .read_to_string(&mut text)
            }) {
                return if e.kind() == ErrorKind::InvalidData {
                    Stand::Fremd
                } else {
                    Stand::NichtVerfuegbar
                };
            }
            if u64::try_from(text.len()).unwrap_or(u64::MAX) > MAX_DATEI {
                return Stand::Fremd;
            }
            match eigenes_ziel(&text) {
                Some(ziel) if eigenes_ziel(soll) == Some(ziel) => Stand::An,
                Some(ziel) if Path::new(ziel).is_absolute() || laufwerkspfad(ziel) => {
                    Stand::Veraltet {
                        ziel: PathBuf::from(ziel),
                    }
                }
                _ => Stand::Fremd,
            }
        }
    }
}

fn laufwerkspfad(ziel: &str) -> bool {
    matches!(ziel.as_bytes(), [laufwerk, b':', b'\\', _, ..] if laufwerk.is_ascii_alphabetic())
}

/// Stand fuer den aktuellen Projektordner.
pub fn stand(wurzel: &Path) -> Stand {
    let (Some(o), Ok(soll)) = (ordner().filter(|o| o.is_dir()), inhalt(wurzel)) else {
        return Stand::NichtVerfuegbar;
    };
    datei_stand(&o.join(DATEI), &soll)
}

/// Richtet den Autostart ein und erhaelt fremde Eintraege.
///
/// # Errors
/// Ordner oder Pfad ungeeignet, Eintrag belegt oder Schreibfehler.
pub fn einrichten(wurzel: &Path) -> Result<(), String> {
    let o = ordner().ok_or("Autostart-Ordner unbekannt (APPDATA fehlt)")?;
    eintrag_aendern(&o.join(DATEI), &inhalt(wurzel)?, true)
}

/// Entfernt den eigenen Eintrag, auch wenn er auf einen alten Projektordner zeigt.
///
/// # Errors
/// Ordner oder Pfad ungeeignet, fremder Eintrag oder Löschen fehlgeschlagen.
pub fn entfernen(wurzel: &Path) -> Result<(), String> {
    let o = ordner().ok_or("Autostart-Ordner unbekannt (APPDATA fehlt)")?;
    eintrag_aendern(&o.join(DATEI), &inhalt(wurzel)?, false)
}

fn eintrag_aendern(p: &Path, soll: &str, an: bool) -> Result<(), String> {
    let fehler = |e| format!("{}: {e}", p.display());
    let stand = datei_stand(p, soll);
    match (&stand, an) {
        (Stand::Fremd, _) => Err("Ein anderer Autostart-Eintrag ist vorhanden".into()),
        (Stand::NichtVerfuegbar, _) => Err("Autostart-Eintrag nicht lesbar".into()),
        (Stand::Aus, false) | (Stand::An, true) => Ok(()),
        (Stand::An | Stand::Veraltet { .. }, false) => fs::remove_file(p).map_err(fehler),
        (Stand::Veraltet { .. }, true) => {
            let neben = p.with_extension(format!("{}.neu", crate::neues_token()));
            let mut datei = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&neben)
                .map_err(fehler)?;
            let geschrieben = datei
                .write_all(soll.as_bytes())
                .and_then(|()| datei.sync_all());
            drop(datei);
            let ergebnis = geschrieben.map_err(fehler).and_then(|()| {
                if datei_stand(p, soll) != stand {
                    return Err("Der Autostart-Eintrag wurde inzwischen geändert".into());
                }
                depot_journal::dateiersatz::ersetze(&neben, p).map_err(fehler)
            });
            if ergebnis.is_err() {
                let _ = fs::remove_file(neben);
            }
            ergebnis
        }
        (Stand::Aus, true) => {
            let mut datei = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(p)
                .map_err(fehler)?;
            if let Err(e) = datei
                .write_all(soll.as_bytes())
                .and_then(|()| datei.sync_all())
            {
                drop(datei);
                let _ = fs::remove_file(p);
                return Err(fehler(e));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALT: &str = r"C:\Test\Alt";
    const NEU: &str = r"C:\Test\Neu";

    fn mit_eintrag(
        pruefung: impl FnOnce(&Path, &str, &str) -> Result<(), String>,
    ) -> Result<(), String> {
        let ordner = std::env::temp_dir().join(format!("depot-autostart-{}", crate::neues_token()));
        fs::create_dir(&ordner).map_err(|e| e.to_string())?;
        let ergebnis = pruefung(
            &ordner.join(DATEI),
            &inhalt(Path::new(ALT))?,
            &inhalt(Path::new(NEU))?,
        );
        let entfernt = fs::remove_dir_all(ordner).map_err(|e| e.to_string());
        ergebnis.and(entfernt)
    }

    #[test]
    fn veralteter_eigener_eintrag_laesst_sich_umstellen_und_entfernen() -> Result<(), String> {
        mit_eintrag(|p, alt, neu| {
            assert_eq!(datei_stand(p, neu), Stand::Aus);
            for text in [alt.to_owned(), alt.replace(KOPF, KOPF_ASCII)] {
                fs::write(p, text).map_err(|e| e.to_string())?;
                assert_eq!(datei_stand(p, alt), Stand::An);
                assert_eq!(
                    datei_stand(p, neu),
                    Stand::Veraltet {
                        ziel: PathBuf::from(ALT)
                    }
                );
                eintrag_aendern(p, neu, true)?;
                assert_eq!(datei_stand(p, neu), Stand::An);
                assert_eq!(fs::read_to_string(p).map_err(|e| e.to_string())?, neu);
            }
            eintrag_aendern(p, alt, false)?;
            assert_eq!(datei_stand(p, neu), Stand::Aus);
            Ok(())
        })
    }

    #[test]
    fn fremde_eintraege_bleiben_unberuehrt() -> Result<(), String> {
        mit_eintrag(|p, alt, neu| {
            for text in [
                format!("{alt}echo fremd\r\n"),
                alt.replace(DATEI, "Anderes.cmd"),
                alt.replace("--ohne-browser", "--anderes"),
                alt.replace(ALT, r"C:\Test\%ALT%"),
                alt.replace(ALT, r"C:\Test\A&B"),
                alt.replace(ALT, "Alt"),
            ] {
                fs::write(p, &text).map_err(|e| e.to_string())?;
                assert_eq!(datei_stand(p, neu), Stand::Fremd);
                assert!(eintrag_aendern(p, neu, true).is_err());
                assert!(eintrag_aendern(p, neu, false).is_err());
                assert_eq!(fs::read_to_string(p).map_err(|e| e.to_string())?, text);
            }
            Ok(())
        })
    }
}
