//! Negativtests zu den Kodex-Befunden aus `docs/BEFUNDE.md` (Stufe 2, Kodex-Schritt).
//!
//! Der Testprozess erfasst den Arbeitsbaum einmal als Git-Bundle. Jeder Test klont daraus
//! ein eigenes Git-Repository mit der Basis `basis` und führt dort seine Verstöße ein. Der Kodex muss ihn ablehnen, und zwar aus dem richtigen Grund: Die Ablehnung muss ein
//! erwartetes Stichwort nennen. Vor jedem Verstoss prueft eine Kontrolle, dass dieselbe Kopie
//! mit einer harmlosen, sauber deklarierten Aenderung das Gate besteht. Eine kaputte Kopie macht
//! einen Test also nie gruen.
//!
//! Die Tests nutzen nur `depot_kodex::pruefe`, `pruefe_ohne_diff` und die Festschreibung
//! (`berechne`, `als_text`). Diese Schnittstelle bleibt beim Umbau gleich.
//!
//! Dieses Modul enthält die gemeinsame Prüfkopie (`Baum`) und ihre Hilfen. Die Tests stehen in
//! `befunde.rs`, `befunde_regeln.rs` und `befunde_compiler.rs`, damit das Gate sie als eigene
//! Testprogramme parallel startet (C1, #190). Jedes Programm erfasst den Arbeitsbaum einmal.

#![allow(dead_code, reason = "jede Testdatei nutzt nur einen Teil der Hilfen")]

use std::cell::RefCell;
use std::collections::{BTreeMap, btree_map::Entry};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use sha2::{Digest, Sha256};

pub static ZAEHLER: AtomicU32 = AtomicU32::new(0);
pub static BASIS: OnceLock<Result<Vec<u8>, String>> = OnceLock::new();

pub fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let aus = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=Kodex-Probe",
            "-c",
            "user.email=probe@depotschlopf.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .map_err(|e| format!("git nicht startbar: {e}"))?;
    if !aus.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&aus.stderr)
        ));
    }
    Ok(aus.stdout)
}

pub fn sha_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Reserviert einen eigenen Ordner atomisch. Ein gleicher Name (auch bei wiederverwendeter
/// PID oder getrennten PID-Namensraeumen) darf keinen fremden Pruefbaum loeschen.
pub fn reserviere_ordner(basis: &Path, kennung: &str) -> Result<PathBuf, String> {
    fs::create_dir_all(basis).map_err(|e| format!("{}: {e}", basis.display()))?;
    for versuch in 0..1000 {
        let pfad = basis.join(format!("{kennung}_{versuch}"));
        match fs::create_dir(&pfad) {
            Ok(()) => return Ok(pfad),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("Pruefbaum {} reservieren: {e}", pfad.display())),
        }
    }
    Err(format!("Keine freie Pruefbaumkennung fuer {kennung}"))
}

/// Eine Kopie des Arbeitsbaums mit eigenem Git-Repository; die Basis heisst `basis`.
pub struct Baum {
    pub ordner: PathBuf,
    pub pfad: PathBuf,
    pub vorher: RefCell<BTreeMap<String, Option<Vec<u8>>>>,
}

impl Drop for Baum {
    fn drop(&mut self) {
        // Nur die exklusiv reservierte eigene Kopie entfernen, nie einen fremden Namen.
        let _ = fs::remove_dir_all(&self.ordner);
    }
}

impl Baum {
    pub fn reserviere() -> Result<Self, String> {
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let pfad = reserviere_ordner(
            &std::env::temp_dir().join("depotschlopf-kodex-befunde"),
            &format!("baum_{}_{n}", std::process::id()),
        )?;
        Ok(Self {
            ordner: pfad.clone(),
            pfad,
            vorher: RefCell::default(),
        })
    }

    pub fn neu() -> Result<Self, String> {
        let basis = BASIS
            .get_or_init(|| Self::basis_bundle(&workspace()))
            .as_ref()
            .map_err(Clone::clone)?;
        Self::aus_bundle(basis)
    }

    pub fn aus_bundle(basis: &[u8]) -> Result<Self, String> {
        let mut baum = Self::reserviere()?;
        fs::write(baum.pfad.join("basis.bundle"), basis)
            .map_err(|e| format!("Prüfbasis schreiben: {e}"))?;
        git(
            &baum.pfad,
            &[
                "-c",
                "protocol.file.allow=always",
                "clone",
                "-q",
                "--branch",
                "basis",
                "-c",
                "core.autocrlf=false",
                "-c",
                "core.eol=lf",
                "--",
                "basis.bundle",
                "repo",
            ],
        )?;
        baum.pfad.push("repo");
        Ok(baum)
    }

    pub fn basis_bundle(ws: &Path) -> Result<Vec<u8>, String> {
        let baum = Self::reserviere()?;
        let pfad = &baum.pfad;
        let liste = git(
            ws,
            &[
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
            ],
        )?;
        for roh in liste.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            let rel = String::from_utf8(roh.to_vec()).map_err(|e| format!("Pfad: {e}"))?;
            let quelle = ws.join(&rel);
            if !quelle.is_file() {
                continue; // im Arbeitsbaum geloescht
            }
            let ziel = pfad.join(&rel);
            if let Some(eltern) = ziel.parent() {
                fs::create_dir_all(eltern).map_err(|e| format!("{rel}: {e}"))?;
            }
            fs::copy(&quelle, &ziel).map_err(|e| format!("{rel}: {e}"))?;
        }
        git(pfad, &["init", "-q", "-b", "basis"])?;
        // Die Gegenproben arbeiten wie die Prüfkopie mit unveränderten LF-Dateien.
        git(pfad, &["config", "--local", "core.autocrlf", "false"])?;
        git(pfad, &["config", "--local", "core.eol", "lf"])?;
        git(pfad, &["add", "-A"])?;
        git(pfad, &["commit", "-q", "-m", "basis"])?;
        git(pfad, &["bundle", "create", "-", "refs/heads/basis"])
    }

    pub fn datei(&self, rel: &str) -> PathBuf {
        self.pfad.join(rel)
    }

    pub fn lies(&self, rel: &str) -> Result<String, String> {
        fs::read_to_string(self.datei(rel)).map_err(|e| format!("{rel}: {e}"))
    }

    pub fn schreibe(&self, rel: &str, inhalt: &str) -> Result<(), String> {
        self.schreibe_bytes(rel, inhalt.as_bytes())
    }

    /// Die Testdateien liegen in vorhandenen Ordnern. Pro Fall merken wir ihren ersten Stand.
    pub fn schreibe_bytes(&self, rel: &str, inhalt: &[u8]) -> Result<(), String> {
        let ziel = self.datei(rel);
        if let Entry::Vacant(eintrag) = self.vorher.borrow_mut().entry(rel.to_owned()) {
            let alt = match fs::read(&ziel) {
                Ok(bytes) => Some(bytes),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(format!("{rel} vor der Änderung lesen: {e}")),
            };
            eintrag.insert(alt);
        }
        fs::write(ziel, inhalt).map_err(|e| format!("{rel}: {e}"))
    }

    /// Ersetzt genau ein Vorkommen von `alt`.
    pub fn ersetze(&self, rel: &str, alt: &str, neu: &str) -> Result<(), String> {
        let text = self.lies(rel)?;
        if text.matches(alt).count() != 1 {
            return Err(format!("Vorbedingung: {alt:?} nicht genau einmal in {rel}"));
        }
        self.schreibe(rel, &text.replacen(alt, neu, 1))
    }

    pub fn haenge_an(&self, rel: &str, code: &str) -> Result<(), String> {
        let text = self.lies(rel)?;
        self.schreibe(rel, &format!("{text}\n{code}"))
    }

    pub fn deklariere(&self, klasse: &str, pfade: &[&str]) -> Result<(), String> {
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let mut t = format!("klasse: {klasse}\nzweck: Kodex-Probe {n}\npfade:\n");
        for p in pfade {
            let _ = writeln!(t, "{p}");
        }
        self.schreibe("kodex/aenderung.txt", &t)
    }

    /// Stellt nur die Dateien wieder her, die der Fall über `schreibe` geändert hat.
    pub fn zuruecksetzen(&self) -> Result<(), String> {
        for (rel, alt) in self.vorher.take() {
            let ziel = self.datei(&rel);
            let ergebnis = match alt {
                Some(bytes) => fs::write(ziel, bytes),
                None => fs::remove_file(ziel).or_else(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        Ok(())
                    } else {
                        Err(e)
                    }
                }),
            };
            ergebnis.map_err(|e| format!("{rel} zurücksetzen: {e}"))?;
        }
        Ok(())
    }

    pub fn gate(&self) -> Vec<String> {
        depot_kodex::pruefe(&self.pfad, "basis")
    }

    /// Kontrolle: eine harmlose, sauber deklarierte Aenderung besteht das Gate.
    pub fn kontrolle(&self) -> Result<(), String> {
        self.deklariere("DokuOnly", &["docs/kontrolle.md"])?;
        self.schreibe("docs/kontrolle.md", "Kontrolle\n")?;
        let fehler = self.gate();
        self.zuruecksetzen()?;
        if fehler.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "Vorbedingung: die unveraenderte Kopie besteht das Gate nicht: {fehler:#?}"
            ))
        }
    }
}

/// Der Kodex muss ablehnen und dabei `stichwort` nennen.
pub fn abgelehnt(fall: &str, fehler: &[String], stichwort: &str) -> Result<(), String> {
    if fehler.iter().any(|f| f.contains(stichwort)) {
        return Ok(());
    }
    Err(format!(
        "{fall}: nicht aus dem erwarteten Grund abgelehnt (Stichwort {stichwort:?}): {fehler:#?}"
    ))
}

/// Ein Verstoss: aendert die Kopie und liefert das Urteil des Kodex.
pub type Verstoss<'a> = dyn Fn(&Baum) -> Result<Vec<String>, String> + 'a;

/// Fuehrt jeden Fall auf derselben Kopie aus und sammelt alle Fehlschlaege.
pub fn alle_faelle(baum: &Baum, faelle: &[(&str, &Verstoss<'_>, &str)]) -> Result<(), String> {
    let mut offen = Vec::new();
    for (fall, verstoss, stichwort) in faelle {
        let fehler = verstoss(baum)?;
        if let Err(e) = abgelehnt(fall, &fehler, stichwort) {
            offen.push(e);
        }
        baum.zuruecksetzen()?;
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(offen.join("\n\n"))
    }
}

// ---------------------------------------------------------------------------------------------
// 1·G01 – harte Lints aussetzbar (R26)
// ---------------------------------------------------------------------------------------------

pub const TOML: &str = "crates/depot_kern/Cargo.toml";
pub const ESSENZ: &str = "docs/essenz.md";
pub const UNICODE: &str = "docs/prüfung mit leer.md";
pub const GELD: &str = "crates/depot_kern/src/geld.rs";
pub const KERN_LIB: &str = "crates/depot_kern/src/lib.rs";

pub fn funktion_mit(baum: &Baum, code: &str) -> Result<Vec<String>, String> {
    baum.deklariere("Funktion", &[GELD])?;
    baum.haenge_an(GELD, code)?;
    Ok(baum.gate())
}
