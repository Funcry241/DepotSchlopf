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
    const DIALOG: &str = "
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
$text = $env:DEPOTSCHLOPF_PAPIER_DIALOG
$wahl = [System.Windows.Forms.MessageBox]::Show(
    $text, 'DepotSchlopf - Freigabe', 'YesNo', 'Warning', 'Button2', 'DefaultDesktopOnly')
if ($wahl -eq [System.Windows.Forms.DialogResult]::Yes) { exit 0 }
exit 2
";
    let Ok(mut kind) = powershell_dialog(DIALOG)
        .and_then(|mut befehl| befehl.env("DEPOTSCHLOPF_PAPIER_DIALOG", text).spawn())
    else {
        return DialogErgebnis::NichtVerfuegbar;
    };
    warte_auf_bestaetigung(&mut kind, Duration::from_secs(120))
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
    use std::io::Read as _;
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
    let mut kind = powershell_dialog(DIALOG)
        .and_then(|mut befehl| befehl.stdout(Stdio::piped()).stderr(Stdio::null()).spawn())
        .map_err(|_| fehler())?;
    let mut ausgabe = kind.stdout.take().ok_or_else(fehler)?;
    // Die Pipe parallel lesen, damit auch lange Pfade den Dialogprozess nicht blockieren.
    let leser = std::thread::spawn(move || {
        let mut pfad = String::new();
        ausgabe.read_to_string(&mut pfad).map(|_| pfad)
    });
    match warte_auf_bestaetigung(&mut kind, Duration::from_secs(120)) {
        DialogErgebnis::Ja => leser
            .join()
            .map_err(|_| fehler())?
            .map(Some)
            .map_err(|_| fehler()),
        DialogErgebnis::Nein => Ok(None),
        DialogErgebnis::ZeitAbgelaufen => {
            Err("Der Ordnerdialog wurde nach zwei Minuten geschlossen.".into())
        }
        DialogErgebnis::NichtVerfuegbar => Err(fehler()),
    }
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
