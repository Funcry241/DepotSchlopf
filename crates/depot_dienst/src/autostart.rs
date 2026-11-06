//! Autostart mit Windows: eine kleine Datei `DepotSchlopf.cmd` im Autostart-Ordner des
//! Benutzers (`%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup`). Sie ruft nur den
//! Starter `DepotSchlopf.cmd` im Projektordner, ohne Browser und mit kleinem Fenster.
//! Kein Dienst, keine Registry, keine Adminrechte; entfernen loescht genau diese Datei.

use std::fs;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};

const DATEI: &str = "DepotSchlopf.cmd";

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
    Ok(format!(
        "@echo off\r\nrem Autostart von DepotSchlopf. Entfernen ueber die Einrichtung.\r\n\
         start \"DepotSchlopf\" /min \"{w}\\{DATEI}\" --ohne-browser\r\n"
    ))
}

/// Ein Pfad als Text, der in einer Befehlsdatei zwischen Anfuehrungszeichen nichts anderes
/// bedeuten kann.
///
/// # Errors
/// Kein gueltiger Text, leer, oder mit Sonderzeichen.
pub fn sicherer_pfad(p: &Path) -> Result<&str, String> {
    let w = p.to_str().ok_or("Pfad ist kein gueltiger Text")?;
    if w.is_empty()
        || w.chars()
            .any(|c| c.is_control() || matches!(c, '"' | '%' | '!' | '^' | '&' | '|' | '<' | '>'))
    {
        return Err("Pfad enthaelt Sonderzeichen (\" % ! ^ & | < >)".into());
    }
    Ok(w)
}

/// Zu welchem Projektordner der Autostart gehört.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stand {
    Aus,
    An,
    Fremd,
    NichtVerfuegbar,
}

fn datei_stand(p: &Path, soll: &str) -> Stand {
    match fs::symlink_metadata(p) {
        Err(e) if e.kind() == ErrorKind::NotFound => Stand::Aus,
        Err(_) => Stand::NichtVerfuegbar,
        Ok(m) if !m.is_file() => Stand::Fremd,
        Ok(_) => match fs::read_to_string(p) {
            Ok(ist) if ist == soll => Stand::An,
            Ok(_) => Stand::Fremd,
            Err(_) => Stand::NichtVerfuegbar,
        },
    }
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

/// Entfernt ausschliesslich den Eintrag fuer diesen Projektordner.
///
/// # Errors
/// Ordner oder Pfad ungeeignet, Eintrag gehoert einem anderen Ordner oder Loeschen fehlgeschlagen.
pub fn entfernen(wurzel: &Path) -> Result<(), String> {
    let o = ordner().ok_or("Autostart-Ordner unbekannt (APPDATA fehlt)")?;
    eintrag_aendern(&o.join(DATEI), &inhalt(wurzel)?, false)
}

fn eintrag_aendern(p: &Path, soll: &str, an: bool) -> Result<(), String> {
    let fehler = |e| format!("{}: {e}", p.display());
    match (datei_stand(p, soll), an) {
        (Stand::Fremd, _) => Err("Ein anderer Autostart-Eintrag ist vorhanden".into()),
        (Stand::NichtVerfuegbar, _) => Err("Autostart-Eintrag nicht lesbar".into()),
        (Stand::Aus, false) | (Stand::An, true) => Ok(()),
        (Stand::An, false) => fs::remove_file(p).map_err(fehler),
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

    #[test]
    fn pruefordner_kann_fremden_autostart_nicht_aendern() -> Result<(), String> {
        let o = std::env::temp_dir().join(format!("depot-autostart-{}", crate::neues_token()));
        fs::create_dir(&o).map_err(|e| e.to_string())?;
        let p = o.join(DATEI);
        let echt = inhalt(Path::new("C:\\DepotSchlopf"))?;
        let probe = inhalt(Path::new("C:\\Pruefordner"))?;
        assert_eq!(datei_stand(&p, &echt), Stand::Aus);
        eintrag_aendern(&p, &echt, true)?;
        assert_eq!(datei_stand(&p, &echt), Stand::An);
        assert_eq!(datei_stand(&p, &probe), Stand::Fremd);
        assert!(eintrag_aendern(&p, &probe, true).is_err());
        assert!(eintrag_aendern(&p, &probe, false).is_err());
        assert_eq!(datei_stand(&p, &echt), Stand::An);
        eintrag_aendern(&p, &echt, false)?;
        assert_eq!(datei_stand(&p, &echt), Stand::Aus);
        fs::remove_dir(o).map_err(|e| e.to_string())
    }
}
