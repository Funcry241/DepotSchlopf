//! Rueckfall nach einem Update (`run.cmd update`, siehe `depot_lauf`).
//!
//! `daten/update_offen.txt` sagt, dass der Projektordner gerade von `vorher` auf `neu`
//! vorgerueckt ist und der neue Stand noch nicht bewiesen hat, dass er laeuft. Zwei Ausgaenge:
//!
//! - **Bestaetigen:** Der neue Dienst laeuft `BESTAETIGEN_NACH` lang; dann loescht er die Datei.
//! - **Zuruecksetzen:** Kommt der neue Dienst nicht hoch (Bau scheitert, er endet mit Fehler,
//!   bevor er bestaetigt hat), setzt der Waechter den Projektordner mit `git reset --keep` auf
//!   `vorher` zurueck und vermerkt `neu` in `daten/update_abgelehnt.txt`.
//!
//! Das braucht nur Git, keinen Code des neuen Stands: Der Waechter ist schon gebaut und laeuft.
//! Steht der Projektordner weder auf `vorher` noch auf `neu`, faesst der Rueckfall nichts an.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Erste Zeile von `daten/update_offen.txt` (wie `depot_lauf::update::KOPF`).
pub const KOPF: &str = "update-v1";
/// So lange muss ein neuer Stand laufen, bis das Update als bestaetigt gilt.
pub const BESTAETIGEN_NACH: Duration = Duration::from_secs(15);
/// Mit diesem Code endet der Dienst nach einem eingespielten Update; der Waechter startet ihn
/// dann neu.
pub const NEUSTART: u8 = 3;

/// Ein offenes, noch nicht bestaetigtes Update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offen {
    pub vorher: String,
    pub neu: String,
}

pub fn offen_pfad(wurzel: &Path) -> PathBuf {
    wurzel.join("daten").join("update_offen.txt")
}

pub fn abgelehnt_pfad(wurzel: &Path) -> PathBuf {
    wurzel.join("daten").join("update_abgelehnt.txt")
}

fn ist_commit(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Liest den Inhalt streng: genau Kopf, `vorher=`, `neu=`, volle Commit-Namen.
///
/// # Errors
/// Jede andere Form.
pub fn lies(text: &str) -> Result<Offen, String> {
    let mut z = text.lines();
    let (Some(kopf), Some(v), Some(n), None) = (z.next(), z.next(), z.next(), z.next()) else {
        return Err("update_offen.txt: unerwartete Zeilenzahl".into());
    };
    let vorher = v.strip_prefix("vorher=").filter(|c| ist_commit(c));
    let neu = n.strip_prefix("neu=").filter(|c| ist_commit(c));
    match (kopf == KOPF, vorher, neu) {
        (true, Some(v), Some(n)) if v != n => Ok(Offen {
            vorher: v.to_string(),
            neu: n.to_string(),
        }),
        _ => Err("update_offen.txt: unbekannte Form".into()),
    }
}

/// Das offene Update, falls eins da ist.
///
/// # Errors
/// Die Datei ist da, aber nicht lesbar oder fremd.
pub fn offen(wurzel: &Path) -> Result<Option<Offen>, String> {
    match crate::werkbank::lies_falls_da(&offen_pfad(wurzel))? {
        Some(t) => lies(&t).map(Some),
        None => Ok(None),
    }
}

fn git(wurzel: &Path, args: &[&str]) -> Result<String, String> {
    let aus = Command::new("git")
        .arg("-C")
        .arg(wurzel)
        .args(args)
        .output()
        .map_err(|e| format!("git nicht startbar: {e}"))?;
    if !aus.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&aus.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&aus.stdout).trim().to_string())
}

fn kopf(wurzel: &Path) -> Result<String, String> {
    git(wurzel, &["rev-parse", "--verify", "HEAD^{commit}"])
}

/// Kurzform eines Commit-Namens fuer Meldungen.
pub fn kurz(c: &str) -> &str {
    c.get(..10).unwrap_or(c)
}

/// Bestaetigt das offene Update auf dem neuen Stand mit lesbaren und abgeglichenen Papierdaten.
///
/// # Errors
/// Die Datei ist fremd, Git antwortet nicht, der Projektordner steht anders, oder die Datei
/// laesst sich nicht loeschen. Dann bleibt sie liegen.
pub fn bestaetige(wurzel: &Path) -> Result<Option<Offen>, String> {
    let Some(o) = offen(wurzel)? else {
        return Ok(None);
    };
    if kopf(wurzel)? != o.neu {
        return Err(format!(
            "Projektordner steht nicht auf dem eingespielten Stand {}; Update nicht bestaetigt.",
            kurz(&o.neu)
        ));
    }
    depot_lage::update_datenprobe(&wurzel.join("daten"))
        .map_err(|e| format!("Update nicht bestätigt: Papierdaten nicht verwendbar ({e})."))?;
    fs::remove_file(offen_pfad(wurzel)).map_err(|e| format!("update_offen.txt: {e}"))?;
    Ok(Some(o))
}

/// Setzt ein offenes Update zurueck. `Ok(None)`: Es gab keins.
///
/// # Errors
/// Die Datei ist fremd, Git scheitert, oder der Projektordner steht weder auf `vorher` noch
/// auf `neu`; dann bleibt alles, wie es ist.
pub fn zuruecksetzen(wurzel: &Path) -> Result<Option<String>, String> {
    let Some(o) = offen(wurzel)? else {
        return Ok(None);
    };
    let ist = kopf(wurzel)?;
    let meldung = if ist == o.vorher {
        // Vorruecken kam nie zustande: nichts zurueckzusetzen.
        format!("Update auf {} war nicht eingespielt.", kurz(&o.neu))
    } else if ist == o.neu {
        git(wurzel, &["reset", "--quiet", "--keep", &o.vorher])?;
        let p = abgelehnt_pfad(wurzel);
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&p)
            .and_then(|mut f| {
                writeln!(f, "{}", o.neu)?;
                f.sync_all()
            })
            .map_err(|e| format!("{}: {e}", p.display()))?;
        format!(
            "Update auf {} zurueckgesetzt, wieder auf {}. Der Stand wird nicht noch einmal \
             eingespielt.",
            kurz(&o.neu),
            kurz(&o.vorher)
        )
    } else {
        return Err(format!(
            "Projektordner steht auf {}, weder vor ({}) noch nach dem Update ({}). Nichts \
             angefasst; bitte von Hand pruefen.",
            kurz(&ist),
            kurz(&o.vorher),
            kurz(&o.neu)
        ));
    };
    fs::remove_file(offen_pfad(wurzel)).map_err(|e| format!("update_offen.txt: {e}"))?;
    Ok(Some(meldung))
}
