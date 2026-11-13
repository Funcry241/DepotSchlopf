//! Die Freigabe braucht neben dem Formular eine Bestätigung am Windows-Desktop.

use depot_lage::PapierFreigabeAngebot;

#[cfg(any(windows, test))]
use std::{
    process::Child,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogErgebnis {
    Ja,
    Nein,
    ZeitAbgelaufen,
    NichtVerfuegbar,
}

#[cfg(any(windows, test))]
fn warte_auf_bestaetigung(kind: &mut Child, zeitlimit: Duration) -> DialogErgebnis {
    let start = Instant::now();
    while let Some(rest) = zeitlimit.checked_sub(start.elapsed()) {
        match kind.try_wait() {
            Ok(Some(_)) if start.elapsed() >= zeitlimit => return DialogErgebnis::ZeitAbgelaufen,
            Ok(Some(status)) => {
                return match status.code() {
                    Some(0) => DialogErgebnis::Ja,
                    Some(2) => DialogErgebnis::Nein,
                    _ => DialogErgebnis::NichtVerfuegbar,
                };
            }
            Ok(None) => std::thread::sleep(rest.min(Duration::from_millis(100))),
            Err(_) => {
                beende(kind);
                return DialogErgebnis::NichtVerfuegbar;
            }
        }
    }
    beende(kind);
    DialogErgebnis::ZeitAbgelaufen
}

#[cfg(any(windows, test))]
fn beende(kind: &mut Child) {
    if kind.kill().is_ok() {
        let _ = kind.wait();
    }
}

pub fn bestaetigen(angebot: &PapierFreigabeAngebot) -> bool {
    zeige(&format!(
        "Papierhandel freigeben?\nNeue Verlustmessung ab: {}\nVerlust seit historischem Höchststand: {}",
        angebot.neuer_start_text(),
        angebot.gesamtverlust_text()
    )) == DialogErgebnis::Ja
}

pub(crate) fn sperre_bestaetigen(sperre: crate::Sperre) -> bool {
    let text = match sperre {
        crate::Sperre::Kostenschutz => {
            "Sperre des KI-Kostenschutzes aufheben?\nBestätige nur, wenn du die Kosten beim Anbieter geprüft hast."
        }
        crate::Sperre::Kurse => {
            "Kurssperre aufheben?\nBestätige nur, wenn du den Schlüssel der Kursquelle geprüft hast."
        }
    };
    zeige(text) == DialogErgebnis::Ja
}

pub fn testen() -> DialogErgebnis {
    zeige("TEST – es wird nichts freigegeben.\nPrüfe, ob Nein vorausgewählt ist.")
}

#[cfg(windows)]
fn zeige(text: &str) -> DialogErgebnis {
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DIALOG: &str = "
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
$text = $env:DEPOTSCHLOPF_PAPIER_DIALOG
$wahl = [System.Windows.Forms.MessageBox]::Show(
    $text, 'DepotSchlopf - Freigabe', 'YesNo', 'Warning', 'Button2', 'DefaultDesktopOnly')
if ($wahl -eq [System.Windows.Forms.DialogResult]::Yes) { exit 0 }
exit 2
";
    let Some(systemroot) = std::env::var_os("SystemRoot") else {
        return DialogErgebnis::NichtVerfuegbar;
    };
    let Ok(mut kind) = Command::new(
        PathBuf::from(systemroot)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe"),
    )
    .creation_flags(CREATE_NO_WINDOW)
    .args(["-NoProfile", "-NonInteractive", "-STA", "-Command", DIALOG])
    .env("DEPOTSCHLOPF_PAPIER_DIALOG", text)
    .spawn() else {
        return DialogErgebnis::NichtVerfuegbar;
    };
    warte_auf_bestaetigung(&mut kind, Duration::from_secs(120))
}

#[cfg(not(windows))]
fn zeige(_text: &str) -> DialogErgebnis {
    DialogErgebnis::NichtVerfuegbar
}

#[cfg(all(test, unix))]
mod tests {
    use super::{DialogErgebnis, Duration, warte_auf_bestaetigung};
    use std::process::Command;

    #[test]
    fn dialogantwort_gilt_nur_rechtzeitig_und_bei_ja() -> Result<(), String> {
        for (code, erwartet) in [
            ("exit 0", DialogErgebnis::Ja),
            ("exit 2", DialogErgebnis::Nein),
            ("exit 1", DialogErgebnis::NichtVerfuegbar),
        ] {
            let mut kind = Command::new("sh")
                .args(["-c", code])
                .spawn()
                .map_err(|e| e.to_string())?;
            assert_eq!(
                warte_auf_bestaetigung(&mut kind, Duration::from_secs(2)),
                erwartet
            );
        }
        let mut kind = Command::new("sleep")
            .arg("30")
            .spawn()
            .map_err(|e| e.to_string())?;
        assert_eq!(
            warte_auf_bestaetigung(&mut kind, Duration::from_millis(20)),
            DialogErgebnis::ZeitAbgelaufen
        );
        assert!(kind.try_wait().map_err(|e| e.to_string())?.is_some());
        Ok(())
    }
}
