//! Autostart mit Windows: eine kleine Datei `DepotSchlopf.cmd` im Autostart-Ordner des
//! Benutzers (`%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup`). Sie ruft nur den
//! Starter `DepotSchlopf.cmd` im Projektordner, ohne Browser und mit kleinem Fenster.
//! Kein Dienst, keine Registry, keine Adminrechte; entfernen loescht genau diese Datei.

use std::fs;
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

/// Ist der Autostart fuer diesen Projektordner eingerichtet?
pub fn an(wurzel: &Path) -> bool {
    let (Some(o), Ok(soll)) = (ordner(), inhalt(wurzel)) else {
        return false;
    };
    fs::read_to_string(o.join(DATEI)).is_ok_and(|ist| ist == soll)
}

/// Richtet den Autostart ein.
///
/// # Errors
/// Kein Autostart-Ordner, Pfad ungeeignet oder Schreibfehler.
pub fn einrichten(wurzel: &Path) -> Result<(), String> {
    let o = ordner().ok_or("Autostart-Ordner unbekannt (APPDATA fehlt)")?;
    let text = inhalt(wurzel)?;
    crate::werkbank::schreibe_ganz(&o.join(DATEI), &text)
}

/// Entfernt den Autostart. Nichts da ist auch gut.
///
/// # Errors
/// Kein Autostart-Ordner oder die Datei laesst sich nicht loeschen.
pub fn entfernen() -> Result<(), String> {
    let o = ordner().ok_or("Autostart-Ordner unbekannt (APPDATA fehlt)")?;
    let p = o.join(DATEI);
    match fs::remove_file(&p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}
