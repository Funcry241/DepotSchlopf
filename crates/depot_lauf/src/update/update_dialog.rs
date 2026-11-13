//! Bestätigung eines Updates mit geändertem Cargo.lock am Windows-Desktop.

#[cfg(windows)]
pub(super) fn bestaetigen(text: &str) -> bool {
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
exit 2
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
    .env("DEPOTSCHLOPF_UPDATE_DIALOG", text)
    .spawn() else {
        return false;
    };
    let start = Instant::now();
    let zeitlimit = Duration::from_secs(120);
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

#[cfg(not(windows))]
pub(super) fn bestaetigen(_text: &str) -> bool {
    false
}
