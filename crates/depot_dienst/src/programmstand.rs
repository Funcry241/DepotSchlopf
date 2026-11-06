//! Der gesicherte Programmstand: Start ohne Cargo nach einem Neustart (Maus, 05.11.2026).
//!
//! **Sichern:** Hat der Waechter den Dienst mit Cargo gebaut und laeuft dieser
//! `rueckfall::BESTAETIGEN_NACH` lang, kopiert der Dienst die gebauten Programme (`PROGRAMME`)
//! nach `daten/programm/<baum>/`, mit einem Abdruck (SHA-256) je Datei in `stand.txt`. Nur unter
//! drei Bedingungen: Der Projektordner ist sauber (`git status` ohne Eintrag), sein Baum ist
//! der, aus dem gebaut wurde, und genau dieser Baum hat das Gate bestanden
//! (`daten/geprueft.txt`, geschrieben von `run.cmd` und `run.cmd update`). Danach zeigt
//! `daten/programm/aktuell.txt` auf den neuen Stand.
//!
//! **Starten:** `DepotSchlopf.cmd` (und damit der Autostart) prueft den Stand vor dem Start.
//! Der Waechter nimmt den gesicherten Stand nur, wenn er `startklar` ist: Der Projektordner ist
//! sauber und steht auf genau diesem Baum, kein Update ist auf Probe, und jede Programmdatei hat
//! ihren Abdruck. Sonst baut er wie bisher mit Cargo. Die Auftraege des Dienstes (Kurse,
//! Termine, Papiertag) starten dann die gesicherten Programme direkt, ebenfalls ohne Cargo; nur
//! das Update braucht Cargo, denn es prueft mit dem ganzen Gate.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use depot_kern::abdruck::sha256_hex;

use crate::werkbank::{Art, Starter, cargo_starter, lies_falls_da, schreibe_ganz};

/// Die Programme eines Stands.
pub const PROGRAMME: [&str; 4] = [
    "depot_waechter",
    "depot_dienst",
    "depot_sammler",
    "depot_berater",
];

const KOPF: &str = "programm-v1";
/// Eine Zeile in `daten/geprueft.txt` (wie in `depot_lauf`): `geprueft-v1;<baum>`.
const GEPRUEFT_KOPF: &str = "geprueft-v1;";

pub fn ordner(wurzel: &Path) -> PathBuf {
    wurzel.join("daten").join("programm")
}

fn aktuell_pfad(wurzel: &Path) -> PathBuf {
    ordner(wurzel).join("aktuell.txt")
}

pub fn geprueft_pfad(wurzel: &Path) -> PathBuf {
    wurzel.join("daten").join("geprueft.txt")
}

/// Der Dateiname eines Programms auf diesem System (`.exe` unter Windows).
pub fn datei(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

fn ist_baum(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Ein gesicherter Stand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stand {
    pub baum: String,
    pub ordner: PathBuf,
    /// Je Programm sein Abdruck.
    pub abdruecke: Vec<(String, String)>,
}

impl Stand {
    pub fn programm(&self, name: &str) -> PathBuf {
        self.ordner.join(datei(name))
    }

    pub fn kurz(&self) -> &str {
        self.baum.get(..10).unwrap_or(&self.baum)
    }

    fn als_text(&self) -> String {
        let mut t = format!("{KOPF}\nbaum={}\n", self.baum);
        for (n, a) in &self.abdruecke {
            let _ = writeln!(t, "{n}={a}");
        }
        t
    }
}

/// Liest `stand.txt` streng: Kopf, Baum, je Programm genau ein Abdruck in fester Reihenfolge.
///
/// # Errors
/// Jede andere Form.
pub fn lies(text: &str, ordner: PathBuf) -> Result<Stand, String> {
    let mut z = text.lines();
    if z.next() != Some(KOPF) {
        return Err("stand.txt: Kopf fehlt".into());
    }
    let baum = z
        .next()
        .and_then(|l| l.strip_prefix("baum="))
        .filter(|b| ist_baum(b))
        .ok_or("stand.txt: Baum fehlt")?
        .to_string();
    let mut abdruecke = Vec::new();
    for name in PROGRAMME {
        let a = z
            .next()
            .and_then(|l| l.strip_prefix(name))
            .and_then(|l| l.strip_prefix('='))
            .filter(|a| a.len() == 64 && a.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| format!("stand.txt: Abdruck von {name} fehlt"))?;
        abdruecke.push((name.to_string(), a.to_string()));
    }
    let s = Stand {
        baum,
        ordner,
        abdruecke,
    };
    if z.next().is_some() || s.als_text() != text {
        return Err("stand.txt: nicht in eigener Form".into());
    }
    Ok(s)
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

/// Der Baum, auf dem der Projektordner steht, wenn er sauber ist.
///
/// # Errors
/// Git antwortet nicht, oder es gibt Aenderungen oder unbekannte Dateien ausserhalb von
/// `.gitignore`: Dann ist nicht sicher, woraus gebaut wird.
pub fn sauberer_baum(wurzel: &Path) -> Result<String, String> {
    let status = git(wurzel, &["status", "--porcelain"])?;
    if !status.is_empty() {
        return Err("Projektordner hat lokale Änderungen".into());
    }
    let baum = git(wurzel, &["rev-parse", "--verify", "HEAD^{tree}"])?;
    if !ist_baum(&baum) {
        return Err("Git nennt keinen Baum".into());
    }
    Ok(baum)
}

/// Nennt den Commit nur, wenn HEAD denselben Quellbaum wie das Programm hat.
pub fn commit_fuer_baum(wurzel: &Path, baum: &str) -> Option<String> {
    let git = git(wurzel, &["rev-parse", "HEAD^{commit}", "HEAD^{tree}"]).ok()?;
    let mut zeilen = git.lines();
    let commit = zeilen.next()?;
    (ist_baum(commit) && zeilen.next() == Some(baum) && zeilen.next().is_none())
        .then(|| commit.to_owned())
}

/// Hat genau dieser Baum das Gate bestanden?
pub fn geprueft(wurzel: &Path, baum: &str) -> bool {
    lies_falls_da(&geprueft_pfad(wurzel)).is_ok_and(|t| {
        t.unwrap_or_default()
            .lines()
            .any(|z| z.strip_prefix(GEPRUEFT_KOPF) == Some(baum))
    })
}

/// Der zuletzt gesicherte Stand, mit geprueften Abdruecken.
///
/// # Errors
/// Datei unlesbar oder fremd, oder ein Programm fehlt oder weicht ab.
pub fn aktuell(wurzel: &Path) -> Result<Option<Stand>, String> {
    let Some(name) = lies_falls_da(&aktuell_pfad(wurzel))? else {
        return Ok(None);
    };
    let name = name.trim();
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("aktuell.txt: unbekannte Form".into());
    }
    let dir = ordner(wurzel).join(name);
    let text = lies_falls_da(&dir.join("stand.txt"))?.ok_or("stand.txt fehlt")?;
    let s = lies(&text, dir)?;
    for (n, soll) in &s.abdruecke {
        let ist = fs::read(s.programm(n))
            .map(|b| sha256_hex(&b))
            .map_err(|e| format!("{n}: {e}"))?;
        if &ist != soll {
            return Err(format!("{n}: Abdruck weicht ab"));
        }
    }
    Ok(Some(s))
}

/// Der gesicherte Stand, wenn der Waechter ihn ohne Cargo starten darf.
///
/// # Errors
/// Warum nicht, in Worten.
pub fn startklar(wurzel: &Path) -> Result<Stand, String> {
    if crate::rueckfall::offen_pfad(wurzel).exists() {
        return Err("Ein Update läuft zur Probe".into());
    }
    let s = aktuell(wurzel)?.ok_or("Kein Programmstand gesichert")?;
    let baum = sauberer_baum(wurzel)?;
    if baum != s.baum {
        return Err(format!(
            "Der Projektordner steht auf einem anderen Stand als der gesicherte ({})",
            s.kurz()
        ));
    }
    if !geprueft(wurzel, &baum) {
        return Err("Dieser Stand hat das Gate hier noch nicht bestanden".into());
    }
    Ok(s)
}

/// Sichert die Programme aus `von` als Stand des Baums `baum`.
///
/// # Errors
/// Der Baum ist nicht geprueft, der Projektordner nicht sauber oder auf anderem Stand, eine
/// Datei fehlt oder laesst sich nicht schreiben. Dann bleibt der alte Stand.
pub fn sichere(wurzel: &Path, baum: &str, von: &Path) -> Result<Stand, String> {
    if !ist_baum(baum) {
        return Err("Baum unbekannter Form".into());
    }
    if !geprueft(wurzel, baum) {
        return Err(
            "Dieser Stand hat das Gate hier noch nicht bestanden. Einmal run.cmd ausführen \
             oder ein Update einspielen."
                .into(),
        );
    }
    if sauberer_baum(wurzel)? != baum {
        return Err("Der Projektordner hat sich seit dem Bau bewegt".into());
    }
    let name: String = baum.chars().take(16).collect();
    let ziel = ordner(wurzel).join(&name);
    let stand = match aktuell(wurzel) {
        Ok(Some(s)) if s.baum == baum => s,
        _ => kopiere(baum, von, &ziel)?,
    };
    let alt = lies_falls_da(&aktuell_pfad(wurzel)).ok().flatten();
    schreibe_ganz(&aktuell_pfad(wurzel), &format!("{name}\n"))?;
    aufraeumen(wurzel, &name, alt.as_deref().map(str::trim));
    Ok(stand)
}

fn kopiere(baum: &str, von: &Path, ziel: &Path) -> Result<Stand, String> {
    let neu = ziel.with_extension("neu");
    let _ = fs::remove_dir_all(&neu);
    fs::create_dir_all(&neu).map_err(|e| format!("{}: {e}", neu.display()))?;
    let mut abdruecke = Vec::new();
    for n in PROGRAMME {
        let q = von.join(datei(n));
        let bytes = fs::read(&q).map_err(|e| format!("{}: {e}", q.display()))?;
        let a = neu.join(datei(n));
        fs::write(&a, &bytes).map_err(|e| format!("{}: {e}", a.display()))?;
        // Unter Unix bleibt die Datei ausfuehrbar wie das Original.
        if let Ok(m) = fs::metadata(&q) {
            let _ = fs::set_permissions(&a, m.permissions());
        }
        abdruecke.push((n.to_string(), sha256_hex(&bytes)));
    }
    let stand = Stand {
        baum: baum.to_string(),
        ordner: ziel.to_path_buf(),
        abdruecke,
    };
    schreibe_ganz(&neu.join("stand.txt"), &stand.als_text())?;
    let _ = fs::remove_dir_all(ziel);
    fs::rename(&neu, ziel).map_err(|e| format!("{}: {e}", ziel.display()))?;
    Ok(stand)
}

/// Loescht alte Staende ausser dem neuen und dem vorigen. Was in Gebrauch ist (der laufende
/// Waechter unter Windows), bleibt einfach liegen.
fn aufraeumen(wurzel: &Path, neu: &str, vorher: Option<&str>) {
    let Ok(eintraege) = fs::read_dir(ordner(wurzel)) else {
        return;
    };
    for e in eintraege.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        let behalten = n == neu || Some(n.as_str()) == vorher;
        if !behalten && e.path().is_dir() {
            let _ = fs::remove_dir_all(e.path());
        }
    }
}

/// Der Starter fuer Auftraege aus einem gesicherten Stand: Kurse, Termine, Papiertag und das
/// Einrichten des Journals direkt mit den Programmen des Stands; das Update ueber Cargo.
pub fn starter(stand: &Stand) -> Starter {
    let sammler = stand.programm("depot_sammler");
    let berater = stand.programm("depot_berater");
    let cargo = cargo_starter();
    Box::new(move |art: Art, wurzel: &Path| {
        let (programm, args): (&Path, &[&str]) = match art {
            Art::Kurse | Art::KurseVon(_) | Art::Termine | Art::Einrichten => {
                (&sammler, art.argumente())
            }
            Art::Papier => (&berater, art.argumente()),
            Art::UpdatePruefen | Art::Update => return cargo(art, wurzel),
        };
        let mut c = Command::new(programm);
        c.current_dir(wurzel).args(args);
        c
    })
}
