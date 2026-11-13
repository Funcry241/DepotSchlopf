use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use depot_kodex::{DEKLARATION, FESTSCHREIBUNG, freigabe, pr};

const QUELLTEXT: &str = "crates/depot_kodex/src/quellscan.rs";
const WORKFLOW: &str = ".github/workflows/kodex.yml";
static NR: AtomicU32 = AtomicU32::new(0);

struct Probe {
    wurzel: PathBuf,
    basis: String,
}

fn git(wurzel: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(wurzel)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
        ])
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().into())
}

impl Probe {
    fn neu() -> Result<Self, String> {
        let wurzel = std::env::temp_dir().join(format!(
            "depot-kodex-pr-{}-{}",
            std::process::id(),
            NR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&wurzel).map_err(|e| e.to_string())?;
        git(&wurzel, &["init", "-q"])?;
        let mut probe = Self {
            wurzel,
            basis: String::new(),
        };
        for (pfad, text) in [
            (QUELLTEXT, "// vorher"),
            (WORKFLOW, "vorher"),
            (FESTSCHREIBUNG, "kodex: vorher"),
            (DEKLARATION, "vorher"),
        ] {
            probe.schreibe(pfad, text)?;
        }
        probe.basis = probe.commit()?;
        Ok(probe)
    }

    fn schreibe(&self, pfad: &str, text: &str) -> Result<(), String> {
        let datei = self.wurzel.join(pfad);
        let eltern = datei.parent().ok_or("Ordner fehlt")?;
        fs::create_dir_all(eltern).map_err(|e| e.to_string())?;
        fs::write(datei, text).map_err(|e| e.to_string())
    }

    fn commit(&self) -> Result<String, String> {
        git(&self.wurzel, &["add", "."])?;
        git(&self.wurzel, &["commit", "-qm", "Probe"])?;
        git(&self.wurzel, &["rev-parse", "HEAD"])
    }

    fn deklariere(&self, pfade: &[&str]) -> Result<(), String> {
        let text = format!(
            "klasse: KodexAenderung\nzweck: Gegenprobe\npfade:\n{}\n",
            pfade.join("\n")
        );
        self.schreibe(DEKLARATION, &text)
    }

    fn freigeben(&self) -> Result<(), String> {
        let diff = depot_kodex::git::aenderungen(&self.wurzel, &self.basis)?;
        self.schreibe(
            freigabe::FREIGABE,
            &freigabe::vorlage(&self.wurzel, &diff.pfade)?,
        )
    }

    fn pruefe(&self) -> Result<(), String> {
        pr::pruefe(&self.wurzel, &self.basis, &self.commit()?)
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.wurzel);
    }
}

#[test]
fn kodex_kommentar_ohne_alles() -> Result<(), String> {
    let p = Probe::neu()?;
    p.schreibe(QUELLTEXT, "// danach")?;
    assert!(p.pruefe().is_err());
    Ok(())
}

#[test]
fn kodex_mit_festschreibung_ohne_freigabe() -> Result<(), String> {
    let p = Probe::neu()?;
    p.schreibe(QUELLTEXT, "// danach")?;
    p.schreibe(FESTSCHREIBUNG, "kodex: danach")?;
    p.deklariere(&[QUELLTEXT, FESTSCHREIBUNG])?;
    assert!(p.pruefe().is_err_and(|e| e.contains("ohne Grenzfreigabe")));
    Ok(())
}

fn passender_diff(p: &Probe) -> Result<(), String> {
    p.schreibe(QUELLTEXT, "// danach")?;
    p.schreibe(FESTSCHREIBUNG, "kodex: danach")?;
    p.deklariere(&[QUELLTEXT, FESTSCHREIBUNG, freigabe::FREIGABE])?;
    p.freigeben()
}

#[test]
fn kodex_mit_freigabe() -> Result<(), String> {
    let p = Probe::neu()?;
    passender_diff(&p)?;
    p.pruefe()
}

#[test]
fn kodex_freigabe_veraltet() -> Result<(), String> {
    let p = Probe::neu()?;
    passender_diff(&p)?;
    p.schreibe(QUELLTEXT, "// erneut geändert")?;
    assert!(
        p.pruefe()
            .is_err_and(|e| e.contains("passt nicht zum Diff"))
    );
    Ok(())
}

#[test]
fn workflow_ohne_freigabe() -> Result<(), String> {
    let p = Probe::neu()?;
    p.schreibe(WORKFLOW, "danach")?;
    p.deklariere(&[WORKFLOW])?;
    assert!(p.pruefe().is_err_and(|e| e.contains("ohne Grenzfreigabe")));
    Ok(())
}

#[test]
fn freigabe_allein() -> Result<(), String> {
    let p = Probe::neu()?;
    p.schreibe(freigabe::FREIGABE, "Probe")?;
    p.deklariere(&[freigabe::FREIGABE])?;
    assert!(p.pruefe().is_err());
    Ok(())
}

#[test]
fn nur_deklaration() -> Result<(), String> {
    let p = Probe::neu()?;
    p.deklariere(&[])?;
    p.pruefe()
}

#[test]
fn pruefe_pr_abschalten_abgelehnt() -> Result<(), String> {
    let p = Probe::neu()?;
    p.schreibe(QUELLTEXT, "// Schutz entfernt")?;
    p.schreibe(FESTSCHREIBUNG, "")?;
    p.deklariere(&[QUELLTEXT, FESTSCHREIBUNG])?;
    assert!(p.pruefe().is_err_and(|e| e.contains("ohne Grenzfreigabe")));
    Ok(())
}
