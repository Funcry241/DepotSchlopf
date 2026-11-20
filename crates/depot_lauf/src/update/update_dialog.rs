//! Bestätigung eines Updates mit geändertem Cargo.lock am Windows-Desktop.

#[cfg_attr(
    not(any(windows, test)),
    allow(dead_code, reason = "Nur der Windows-Dialog erzeugt Ja und Nein")
)]
#[derive(Debug)]
pub(super) enum Freigabe {
    Ja,
    Nein,
    NichtVerfuegbar,
}

#[cfg(windows)]
pub(super) fn bestaetigen(text: &str) -> Freigabe {
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DIALOG: &str = "
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
$wahl = [System.Windows.Forms.MessageBox]::Show(
    $env:DEPOTSCHLOPF_UPDATE_DIALOG, 'DepotSchlopf - Update', 'YesNoCancel', 'Question', 'Button2', 'DefaultDesktopOnly')
if ($wahl -eq [System.Windows.Forms.DialogResult]::Yes) { exit 0 }
if ($wahl -eq [System.Windows.Forms.DialogResult]::No -or $wahl -eq [System.Windows.Forms.DialogResult]::Cancel) { exit 2 }
exit 1
";
    let Some(systemroot) = std::env::var_os("SystemRoot") else {
        return Freigabe::NichtVerfuegbar;
    };
    let status = Command::new(
        PathBuf::from(systemroot)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe"),
    )
    .creation_flags(CREATE_NO_WINDOW)
    .args(["-NoProfile", "-NonInteractive", "-STA", "-Command", DIALOG])
    .env("DEPOTSCHLOPF_UPDATE_DIALOG", text)
    .status();
    match status.ok().and_then(|status| status.code()) {
        Some(0) => Freigabe::Ja,
        Some(2) => Freigabe::Nein,
        _ => Freigabe::NichtVerfuegbar,
    }
}

#[cfg(not(windows))]
pub(super) fn bestaetigen(_text: &str) -> Freigabe {
    Freigabe::NichtVerfuegbar
}
