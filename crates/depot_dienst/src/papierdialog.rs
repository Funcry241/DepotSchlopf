//! Die Freigabe braucht neben dem Formular eine Bestätigung am Windows-Desktop.

use depot_lage::PapierFreigabeAngebot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogErgebnis {
    Ja,
    Nein,
    NichtVerfuegbar,
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
    const DIALOG: &str = "
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
$text = $env:DEPOTSCHLOPF_PAPIER_DIALOG
$wahl = [System.Windows.Forms.MessageBox]::Show(
    $text, 'DepotSchlopf - Freigabe', 'YesNoCancel', 'Question', 'Button2', 'DefaultDesktopOnly')
if ($wahl -eq [System.Windows.Forms.DialogResult]::Yes) { exit 0 }
exit 2
";
    match powershell_dialog(DIALOG)
        .and_then(|mut befehl| befehl.env("DEPOTSCHLOPF_PAPIER_DIALOG", text).status())
        .ok()
        .and_then(|status| status.code())
    {
        Some(0) => DialogErgebnis::Ja,
        Some(2) => DialogErgebnis::Nein,
        _ => DialogErgebnis::NichtVerfuegbar,
    }
}

#[cfg(windows)]
fn powershell_dialog(skript: &str) -> std::io::Result<std::process::Command> {
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let systemroot =
        std::env::var_os("SystemRoot").ok_or_else(|| std::io::Error::other("SystemRoot fehlt"))?;
    let mut befehl = Command::new(
        PathBuf::from(systemroot)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe"),
    );
    befehl.creation_flags(CREATE_NO_WINDOW).args([
        "-NoProfile",
        "-NonInteractive",
        "-STA",
        "-Command",
        skript,
    ]);
    Ok(befehl)
}

#[cfg(windows)]
pub(crate) fn sicherungsziel_waehlen() -> Result<Option<String>, String> {
    use std::process::Stdio;

    const DIALOG: &str = "
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
Add-Type -AssemblyName System.Windows.Forms
$dialog = [System.Windows.Forms.FolderBrowserDialog]::new()
$fenster = [System.Windows.Forms.Form]::new()
$fenster.TopMost = $true
try {
    $dialog.Description = 'DepotSchlopf: USB-Laufwerk oder Ordner für die Sicherung auswählen'
    $dialog.RootFolder = [System.Environment+SpecialFolder]::MyComputer
    $dialog.ShowNewFolderButton = $false
    if ($dialog.ShowDialog($fenster) -ne [System.Windows.Forms.DialogResult]::OK) { exit 2 }
    [Console]::Write($dialog.SelectedPath)
    exit 0
} finally {
    $dialog.Dispose()
    $fenster.Dispose()
}
";
    let fehler = || "Der Windows-Ordnerdialog konnte nicht geöffnet werden.".to_owned();
    let ergebnis = powershell_dialog(DIALOG)
        .and_then(|mut befehl| befehl.stderr(Stdio::null()).output())
        .map_err(|_| fehler())?;
    match ergebnis.status.code() {
        Some(0) => String::from_utf8(ergebnis.stdout)
            .map(Some)
            .map_err(|_| fehler()),
        Some(2) => Ok(None),
        _ => Err(fehler()),
    }
}

#[cfg(not(windows))]
fn zeige(_text: &str) -> DialogErgebnis {
    DialogErgebnis::NichtVerfuegbar
}
