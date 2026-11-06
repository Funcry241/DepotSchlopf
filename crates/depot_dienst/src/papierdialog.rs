//! Die Freigabe braucht neben dem Formular eine Bestätigung am Windows-Desktop.

use depot_lage::PapierFreigabeAngebot;

#[cfg(any(windows, test))]
use std::{
    process::Child,
    time::{Duration, Instant},
};

#[cfg(any(windows, test))]
fn warte_auf_bestaetigung(kind: &mut Child, zeitlimit: Duration) -> bool {
    let start = Instant::now();
    while let Some(rest) = zeitlimit.checked_sub(start.elapsed()) {
        match kind.try_wait() {
            Ok(Some(status)) => return status.success() && start.elapsed() < zeitlimit,
            Ok(None) => std::thread::sleep(rest.min(Duration::from_millis(100))),
            Err(_) => break,
        }
    }
    if kind.kill().is_ok() {
        let _ = kind.wait();
    }
    false
}

#[cfg(windows)]
pub fn bestaetigen(angebot: &PapierFreigabeAngebot) -> bool {
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DIALOG: &str = "
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
$text = 'Papierhandel freigeben? ' + [Environment]::NewLine +
    'Neue Verlustmessung ab: ' + $env:DEPOTSCHLOPF_PAPIER_START + [Environment]::NewLine +
    'Verlust seit historischem Hoechststand: ' + $env:DEPOTSCHLOPF_PAPIER_VERLUST
$wahl = [System.Windows.Forms.MessageBox]::Show(
    $text, 'DepotSchlopf - Papierfreigabe', 'YesNo', 'Warning', 'Button2', 'DefaultDesktopOnly')
if ($wahl -eq [System.Windows.Forms.DialogResult]::Yes) { exit 0 }
exit 1
";
    let Some(systemroot) = std::env::var_os("SystemRoot") else {
        return false;
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
    .env("DEPOTSCHLOPF_PAPIER_START", angebot.neuer_start_text())
    .env("DEPOTSCHLOPF_PAPIER_VERLUST", angebot.gesamtverlust_text())
    .spawn() else {
        return false;
    };
    warte_auf_bestaetigung(&mut kind, Duration::from_secs(120))
}

#[cfg(not(windows))]
pub fn bestaetigen(_angebot: &PapierFreigabeAngebot) -> bool {
    false
}

#[cfg(all(test, unix))]
mod tests {
    use super::{Duration, warte_auf_bestaetigung};
    use std::process::Command;

    #[test]
    fn dialogantwort_gilt_nur_rechtzeitig_und_bei_ja() -> Result<(), String> {
        for (code, erwartet) in [("exit 0", true), ("exit 1", false)] {
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
        assert!(!warte_auf_bestaetigung(
            &mut kind,
            Duration::from_millis(20)
        ));
        assert!(kind.try_wait().map_err(|e| e.to_string())?.is_some());
        Ok(())
    }
}
