//! Die Regeln des Waechters (`depot_waechter`), hier testbar.
//!
//! **Probezeit:** Liegt beim Start `daten/update_offen.txt`, ist der Dienst auf Probe. Er gilt
//! erst als hochgekommen, wenn er die Markierung selbst loescht (`rueckfall::bestaetige`). Endet
//! er vorher, gleich mit welchem Code (auch 0), oder vergeht die Frist, faellt der Waechter
//! zurueck (Maus, Befund zu #30). Bei der Frist beendet er den Dienst zuerst.

use std::path::{Path, PathBuf};
use std::process::Child;
use std::thread;
use std::time::{Duration, Instant};

/// So lange hat ein neuer Stand hoechstens Zeit, sich zu bestaetigen (er tut es nach
/// `rueckfall::BESTAETIGEN_NACH`).
pub const FRIST: Duration = Duration::from_secs(120);
/// So oft sieht der Waechter waehrend der Probezeit nach.
pub const TAKT: Duration = Duration::from_millis(500);

/// Wie ein Lauf des Dienstes ausging.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ausgang {
    /// Der Dienst endete (nach einer Probezeit erst nach der Bestaetigung). `None`: ohne Code.
    Ende(Option<i32>),
    /// Auf Probe nicht hochgekommen: zuruecksetzen. Der Text sagt warum.
    Rueckfall(String),
}

/// Wartet auf den Dienst. Lag `offen` beim Start, gilt die Probezeit mit `frist`.
pub fn begleite(kind: &mut Child, offen: &Path, frist: Duration, takt: Duration) -> Ausgang {
    if offen.exists() {
        let start = Instant::now();
        loop {
            if !offen.exists() {
                break;
            }
            match kind.try_wait() {
                Ok(Some(s)) => {
                    // Bestaetigung und Ende koennen zusammenfallen: Erst nach dem Ende zaehlt,
                    // ob die Markierung noch liegt.
                    if !offen.exists() {
                        return Ausgang::Ende(s.code());
                    }
                    return Ausgang::Rueckfall(format!(
                        "Neuer Stand endete vor der Bestätigung (Code {:?})",
                        s.code()
                    ));
                }
                Ok(None) => {}
                Err(e) => {
                    let _ = kind.kill();
                    let _ = kind.wait();
                    return Ausgang::Rueckfall(format!("Dienst nicht beobachtbar: {e}"));
                }
            }
            if start.elapsed() >= frist {
                let _ = kind.kill();
                let _ = kind.wait();
                if !offen.exists() {
                    // Im letzten Augenblick bestaetigt, aber schon beendet: neu starten.
                    return Ausgang::Ende(None);
                }
                return Ausgang::Rueckfall(format!(
                    "Neuer Stand hat sich nicht binnen {} s bestätigt; Dienst beendet",
                    frist.as_secs()
                ));
            }
            thread::sleep(takt);
        }
    }
    match kind.wait() {
        Ok(s) => Ausgang::Ende(s.code()),
        Err(_) => Ausgang::Ende(None),
    }
}

/// Der Pfad des gebauten Dienstes aus der Ausgabe von `cargo build --message-format=json`:
/// das `executable` des Ziels `depot_dienst`.
pub fn programm_aus_bau(ausgabe: &str) -> Option<PathBuf> {
    ausgabe
        .lines()
        .filter(|z| z.contains("\"reason\":\"compiler-artifact\""))
        .filter(|z| z.contains("\"name\":\"depot_dienst\"") && z.contains("\"kind\":[\"bin\"]"))
        .find_map(|z| json_text_nach(z, "\"executable\":\""))
        .map(PathBuf::from)
}

/// Liest einen JSON-Text ab dem Schluessel bis zum schliessenden Anfuehrungszeichen.
fn json_text_nach(zeile: &str, schluessel: &str) -> Option<String> {
    let (_, rest) = zeile.split_once(schluessel)?;
    let mut aus = String::new();
    let mut z = rest.chars();
    loop {
        match z.next()? {
            '"' => return Some(aus),
            '\\' => match z.next()? {
                '"' => aus.push('"'),
                '\\' => aus.push('\\'),
                '/' => aus.push('/'),
                'u' => {
                    let hex: String = z.by_ref().take(4).collect();
                    aus.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                _ => return None,
            },
            c => aus.push(c),
        }
    }
}

/// Wiederverwendbarer Bau getrennt vom laufenden Wächter. Zwei feste Plätze verhindern
/// sowohl das Überschreiben einer laufenden EXE als auch neue Baukopien bei jedem Update.
pub fn bau_ordner(wurzel: &Path) -> PathBuf {
    let a = wurzel.join("target/dienst-bau-a");
    let aktuell = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok());
    let belegt = a.canonicalize().ok().zip(aktuell).is_some_and(|(a, exe)| {
        if cfg!(windows) {
            exe.to_string_lossy()
                .to_lowercase()
                .starts_with(&format!("{}\\", a.to_string_lossy().to_lowercase()))
        } else {
            exe.starts_with(a)
        }
    });
    if belegt {
        wurzel.join("target/dienst-bau-b")
    } else {
        a
    }
}

/// Genau dieser Befehl baut beim Erststart und nach einem Dienstwechsel. Der laufende
/// Wächter kann noch in target/debug liegen und bleibt unangetastet.
pub fn baubefehl(wurzel: &Path) -> std::process::Command {
    let mut c = std::process::Command::new("cargo");
    c.current_dir(wurzel)
        .env("CARGO_TARGET_DIR", bau_ordner(wurzel))
        .args([
            "build",
            "--quiet",
            "-p",
            "depot_dienst",
            "-p",
            "depot_sammler",
            "-p",
            "depot_berater",
            "--bins",
            "--message-format=json-render-diagnostics",
        ]);
    c
}

/// Prozessübergreifend, auch über verschiedene Projektordner hinweg. Die Datei bleibt
/// liegen; nur die vom Betriebssystem gehaltene Sperre zählt und endet beim Prozessende.
pub fn prozess_sperre() -> Result<Option<std::fs::File>, String> {
    let p = std::env::temp_dir().join(format!("depotschlopf-waechter-{}.lock", crate::http::PORT));
    if std::fs::symlink_metadata(&p).is_ok_and(|m| !m.is_file()) {
        return Err("Wächtersperre ist keine reguläre Datei.".into());
    }
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(p)
        .map_err(|e| format!("Wächtersperre nicht verfügbar: {e}"))?;
    match f.try_lock() {
        Ok(()) => Ok(Some(f)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(e)) => Err(format!("Wächtersperre nicht verwendbar: {e}")),
    }
}
