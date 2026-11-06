//! Die Freigabe braucht neben dem Formular eine Bestätigung am Windows-Desktop.

use depot_lage::PapierFreigabeAngebot;

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
    Command::new(
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
    .status()
    .is_ok_and(|status| status.success())
}

#[cfg(not(windows))]
pub fn bestaetigen(_angebot: &PapierFreigabeAngebot) -> bool {
    false
}
