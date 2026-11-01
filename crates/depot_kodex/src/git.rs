//! Realer Diff gegen die Basis: Merge-Base bis Arbeitsbaum, inklusive neuer Dateien.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

#[derive(Debug)]
pub struct Diff {
    pub pfade: BTreeSet<String>,
    /// Hatte die Basis schon einen Kodex? Wenn nicht, ist nur `Gruendung` erlaubt.
    pub basis_hat_kodex: bool,
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
    String::from_utf8(aus.stdout).map_err(|e| format!("git-Ausgabe: {e}"))
}

/// # Errors
/// Wenn git fehlt oder die Basis unbekannt ist.
pub fn aenderungen(wurzel: &Path, basis: &str) -> Result<Diff, String> {
    let mb = git(wurzel, &["merge-base", basis, "HEAD"])?;
    let mb = mb.trim();
    let mut pfade: BTreeSet<String> = git(wurzel, &["diff", "--name-only", mb])?
        .lines()
        .map(str::to_string)
        .collect();
    pfade.extend(
        git(wurzel, &["ls-files", "--others", "--exclude-standard"])?
            .lines()
            .map(str::to_string),
    );
    let basis_hat_kodex = git(
        wurzel,
        &["cat-file", "-e", &format!("{mb}:{}", crate::DEKLARATION)],
    )
    .is_ok();
    Ok(Diff {
        pfade,
        basis_hat_kodex,
    })
}
