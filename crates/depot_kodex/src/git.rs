//! Realer Diff gegen die Basis: Merge-Base bis Arbeitsbaum, inklusive neuer Dateien.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

#[derive(Debug)]
pub struct Diff {
    pub pfade: BTreeSet<String>,
    /// Merge-Base, gegen die verglichen wird.
    pub basis: String,
    /// Hatte die Basis schon einen Kodex? Wenn nicht, ist nur `Gruendung` erlaubt.
    pub basis_hat_kodex: bool,
}

fn git(wurzel: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
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
    Ok(aus.stdout)
}

/// NUL-getrennte Pfadliste (R24): keine Quotierung, Umlaute und Leerzeichen bleiben erhalten.
/// Ein Pfad, der kein UTF-8 ist, ist ein Fehler.
fn pfadliste(roh: &[u8]) -> Result<Vec<String>, String> {
    roh.split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| {
            String::from_utf8(p.to_vec())
                .map_err(|_| format!("Pfad ist kein UTF-8: {}", String::from_utf8_lossy(p)))
        })
        .collect()
}

/// Alle Dateien, die Git verfolgt, und alle neuen, nicht ignorierten: alles, was ein Commit
/// mitnehmen koennte.
///
/// # Errors
/// Wenn git fehlt oder der Ordner kein Repository ist.
pub fn projektdateien(wurzel: &Path) -> Result<Vec<String>, String> {
    pfadliste(&git(
        wurzel,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )?)
}

/// Eine gewoehnliche Datei im Index: Pfad und Blob. Links und Untermodule fehlen hier.
#[derive(Debug)]
pub struct IndexDatei {
    pub pfad: String,
    pub blob: String,
}

/// Die gewoehnlichen Dateien im Index, also das, was ein Commit jetzt aufnehmen wuerde.
///
/// # Errors
/// Wenn git fehlt, der Ordner kein Repository ist oder eine Zeile nicht lesbar ist.
pub fn index_dateien(wurzel: &Path) -> Result<Vec<IndexDatei>, String> {
    let mut aus = Vec::new();
    for zeile in pfadliste(&git(wurzel, &["ls-files", "-z", "--stage"])?)? {
        // "<modus> <blob> <stufe>\t<pfad>"
        let (kopf, pfad) = zeile
            .split_once('\t')
            .ok_or_else(|| format!("Index-Zeile nicht lesbar: {zeile}"))?;
        let mut teile = kopf.split(' ');
        let (Some(modus), Some(blob)) = (teile.next(), teile.next()) else {
            return Err(format!("Index-Zeile nicht lesbar: {zeile}"));
        };
        if modus == "100644" || modus == "100755" {
            aus.push(IndexDatei {
                pfad: pfad.to_string(),
                blob: blob.to_string(),
            });
        }
    }
    Ok(aus)
}

/// Inhalt eines Blobs.
///
/// # Errors
/// Wenn git fehlt oder der Blob unbekannt ist.
pub fn blob(wurzel: &Path, blob: &str) -> Result<Vec<u8>, String> {
    git(wurzel, &["cat-file", "blob", blob])
}

/// Nur die Dateien, die Git verfolgt.
///
/// # Errors
/// Wenn git fehlt oder der Ordner kein Repository ist.
pub fn verfolgte_dateien(wurzel: &Path) -> Result<Vec<String>, String> {
    pfadliste(&git(wurzel, &["ls-files", "-z", "--cached"])?)
}

/// Merge-Base von `basis` und `HEAD`.
///
/// # Errors
/// Wenn git fehlt oder die Basis unbekannt ist.
pub fn merge_base(wurzel: &Path, basis: &str) -> Result<String, String> {
    let mb = git(wurzel, &["merge-base", basis, "HEAD"])?;
    Ok(String::from_utf8_lossy(&mb).trim().to_string())
}

/// Inhalt einer Datei im Stand `rev`; `None`, wenn es sie dort nicht gibt.
pub fn datei_im_stand(wurzel: &Path, rev: &str, pfad: &str) -> Option<Vec<u8>> {
    git(wurzel, &["show", &format!("{rev}:{pfad}")]).ok()
}

/// # Errors
/// Wenn git fehlt oder die Basis unbekannt ist.
pub fn aenderungen(wurzel: &Path, basis: &str) -> Result<Diff, String> {
    let mb = merge_base(wurzel, basis)?;
    // --no-renames: Eine Umbenennung zeigt beide Pfade, den alten und den neuen.
    let mut pfade: BTreeSet<String> = pfadliste(&git(
        wurzel,
        &["diff", "-z", "--no-renames", "--name-only", &mb],
    )?)?
    .into_iter()
    .collect();
    pfade.extend(pfadliste(&git(
        wurzel,
        &["ls-files", "-z", "--others", "--exclude-standard"],
    )?)?);
    let basis_hat_kodex = git(
        wurzel,
        &["cat-file", "-e", &format!("{mb}:{}", crate::DEKLARATION)],
    )
    .is_ok();
    Ok(Diff {
        pfade,
        basis: mb,
        basis_hat_kodex,
    })
}
