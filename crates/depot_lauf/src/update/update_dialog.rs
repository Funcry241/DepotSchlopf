//! Bestätigung eines Updates mit geändertem Cargo.lock am Windows-Desktop.

#[cfg_attr(
    not(any(windows, test)),
    allow(dead_code, reason = "Nur der Windows-Dialog erzeugt die drei Bedienergebnisse")
)]
#[derive(Debug)]
pub(super) enum Freigabe {
    Ja,
    Nein,
    Abgelaufen,
    NichtVerfuegbar,
}

#[cfg(windows)]
pub(super) fn bestaetigen(text: &str) -> Freigabe {
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{Duration, Instant};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DIALOG: &str = "
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
$wahl = [System.Windows.Forms.MessageBox]::Show(
    $env:DEPOTSCHLOPF_UPDATE_DIALOG, 'DepotSchlopf - Update', 'YesNo', 'Warning', 'Button2', 'DefaultDesktopOnly')
if ($wahl -eq [System.Windows.Forms.DialogResult]::Yes) { exit 0 }
if ($wahl -eq [System.Windows.Forms.DialogResult]::No) { exit 2 }
exit 1
";
    let Some(systemroot) = std::env::var_os("SystemRoot") else {
        return Freigabe::NichtVerfuegbar;
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
    .env("DEPOTSCHLOPF_UPDATE_DIALOG", text)
    .spawn() else {
        return Freigabe::NichtVerfuegbar;
    };
    let start = Instant::now();
    let zeitlimit = Duration::from_secs(120);
    let ausgang = loop {
        let Some(rest) = zeitlimit.checked_sub(start.elapsed()) else {
            break Freigabe::Abgelaufen;
        };
        match kind.try_wait() {
            Ok(Some(_)) if start.elapsed() >= zeitlimit => return Freigabe::Abgelaufen,
            Ok(Some(status)) => match status.code() {
                Some(0) => return Freigabe::Ja,
                Some(2) => return Freigabe::Nein,
                _ => return Freigabe::NichtVerfuegbar,
            },
            Ok(None) => std::thread::sleep(rest.min(Duration::from_millis(100))),
            Err(_) => break Freigabe::NichtVerfuegbar,
        }
    };
    if kind.kill().is_ok() {
        let _ = kind.wait();
    }
    ausgang
}

#[cfg(not(windows))]
pub(super) fn bestaetigen(_text: &str) -> Freigabe {
    Freigabe::NichtVerfuegbar
}
