//! Semantische Pruefung der Cargo-Manifeste (R26, Befunde 1·G03, 1·G04, 2·A11).
//!
//! `cargo metadata` liefert keine wirksamen Lints. Deshalb liest der Kodex die Manifeste selbst
//! (`toml.rs`) und prueft die Vererbung der Workspace-Lints, die Lage der Produktiv-Targets und
//! die Identitaet jeder Abhaengigkeit. Den Beweis, dass der Compiler die harten Lints wirklich
//! durchsetzt, liefert der Negativtest `befund_1_g01_compiler_lehnt_ausnahme_in_jedem_target_ab`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::toml::{self, Wert};

/// Die harten Lints. Sie stehen im Workspace auf `deny` oder `forbid` und im Kopf jedes
/// Produktiv-Targets auf `forbid`. Aendern nur ueber eine KodexAenderung.
pub const HARTE_LINTS: [&str; 9] = [
    "unsafe_code",
    "clippy::unwrap_used",
    "clippy::expect_used",
    "clippy::panic",
    "clippy::indexing_slicing",
    "clippy::arithmetic_side_effects",
    "clippy::float_arithmetic",
    "clippy::cast_possible_truncation",
    "clippy::cast_sign_loss",
];

/// Endet der Pfad genau auf `.rs`?
fn ist_rs(p: &str) -> bool {
    Path::new(p).extension().is_some_and(|x| x == "rs")
}

/// Liest ein Manifest.
///
/// # Errors
/// Lesefehler oder TOML, das der Leser nicht versteht.
pub fn lies(pfad: &Path) -> Result<Wert, String> {
    let text =
        fs::read_to_string(pfad).map_err(|e| format!("{}: nicht lesbar: {e}", pfad.display()))?;
    toml::lies(&text).map_err(|e| format!("{}: {e}", pfad.display()))
}

/// Stufe eines Lints im Workspace: `"deny"` aus `x = "deny"` oder `x = { level = "deny" }`.
fn stufe(w: &Wert) -> Option<&str> {
    w.text().or_else(|| w.feld("level").and_then(Wert::text))
}

/// Der Workspace setzt jeden harten Lint auf `deny` oder `forbid`.
pub fn pruefe_workspace(wurzel: &Wert) -> Vec<String> {
    let mut fehler = Vec::new();
    for lint in HARTE_LINTS {
        let (werkzeug, name) = lint.split_once("::").unwrap_or(("rust", lint));
        let s = wurzel
            .pfad(&["workspace", "lints", werkzeug, name])
            .and_then(stufe);
        if !matches!(s, Some("deny" | "forbid")) {
            fehler.push(format!(
                "Cargo.toml: harter Lint {lint} steht in den Workspace-Lints nicht auf deny oder forbid"
            ));
        }
    }
    fehler
}

/// Produktiv-Targets eines Crates (Bibliothek und Programme) als Pfade relativ zum Crate.
///
/// # Errors
/// Ein Target ausserhalb von `src/` oder ein Ordner, der sich nicht lesen laesst.
pub fn produktiv_wurzeln(crate_dir: &Path, m: &Wert) -> Result<Vec<String>, String> {
    let mut aus: Vec<String> = Vec::new();
    let nimm = |p: String, aus: &mut Vec<String>| -> Result<(), String> {
        if !p.starts_with("src/") || p.contains("..") || !ist_rs(&p) {
            return Err(format!(
                "Target {p} muss eine .rs-Datei unter src/ sein (K7-03)"
            ));
        }
        if !aus.contains(&p) {
            aus.push(p);
        }
        Ok(())
    };
    match m.pfad(&["lib", "path"]) {
        Some(Wert::Text(p)) => nimm(p.clone(), &mut aus)?,
        Some(_) => return Err("lib.path ist kein Text".into()),
        None if crate_dir.join("src/lib.rs").is_file() => nimm("src/lib.rs".into(), &mut aus)?,
        None => {}
    }
    if let Some(bins) = m.feld("bin") {
        let Wert::Liste(bins) = bins else {
            return Err("bin ist keine Tabellen-Liste".into());
        };
        for b in bins {
            match (b.feld("path"), b.feld("name").and_then(Wert::text)) {
                (Some(Wert::Text(p)), _) => nimm(p.clone(), &mut aus)?,
                (None, Some(name)) => {
                    let eigen = format!("src/bin/{name}.rs");
                    if crate_dir.join(&eigen).is_file() {
                        nimm(eigen, &mut aus)?;
                    } else {
                        nimm("src/main.rs".into(), &mut aus)?;
                    }
                }
                _ => return Err("bin ohne Namen oder mit ungueltigem Pfad".into()),
            }
        }
    }
    if crate_dir.join("src/main.rs").is_file() {
        nimm("src/main.rs".into(), &mut aus)?;
    }
    let bin_dir = crate_dir.join("src/bin");
    if bin_dir.exists() {
        let eintraege = fs::read_dir(&bin_dir)
            .map_err(|e| format!("{}: nicht lesbar: {e}", bin_dir.display()))?;
        for e in eintraege {
            let e = e.map_err(|e| format!("{}: nicht lesbar: {e}", bin_dir.display()))?;
            let name = e.file_name().to_string_lossy().into_owned();
            if e.path().is_dir() {
                nimm(format!("src/bin/{name}/main.rs"), &mut aus)?;
            } else if Path::new(&name)
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("rs"))
            {
                nimm(format!("src/bin/{name}"), &mut aus)?;
            }
        }
    }
    aus.sort();
    Ok(aus)
}

/// Strukturpruefung eines Crate-Manifests.
pub fn pruefe_crate(krate: &str, crate_dir: &Path, m: &Wert) -> Vec<String> {
    let mut fehler = Vec::new();
    let erwartet = toml::lies("workspace = true").ok();
    if m.feld("lints") != erwartet.as_ref() {
        fehler.push(format!(
            "crates/{krate}/Cargo.toml erbt die Workspace-Lints nicht: [lints] muss genau `workspace = true` sein"
        ));
    }
    if m.pfad(&["package", "build"]).is_some() || crate_dir.join("build.rs").exists() {
        fehler.push(format!(
            "crates/{krate}: Build-Skripte sind nicht erlaubt (Code ausserhalb von src/)"
        ));
    }
    if let Err(e) = produktiv_wurzeln(crate_dir, m) {
        fehler.push(format!("crates/{krate}: {e}"));
    }
    // Test-, Bench- und Beispiel-Targets: ebenfalls nur .rs-Dateien unter tests/.
    for art in ["test", "bench", "example"] {
        if let Some(Wert::Liste(ziele)) = m.feld(art) {
            for z in ziele {
                match z.feld("path") {
                    None => {}
                    Some(Wert::Text(p))
                        if p.starts_with("tests/") && ist_rs(p) && !p.contains("..") => {}
                    Some(p) => fehler.push(format!(
                        "crates/{krate}: [[{art}]] mit Pfad {} muss eine .rs-Datei unter tests/ sein",
                        p.kanonisch()
                    )),
                }
            }
        } else if m.feld(art).is_some() {
            fehler.push(format!("crates/{krate}: {art} ist keine Tabellen-Liste"));
        }
    }
    fehler
}

const ABSCHNITTE: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

/// Abhaengigkeiten mit voller Identitaet: Abschnitt, Alias und die kanonische Form des
/// Eintrags (Version, Paket, Quelle, Features, Workspace-Bezug, …).
pub fn abhaengigkeiten(m: &Wert) -> String {
    let mut teile: Vec<String> = Vec::new();
    let mut abschnitt = |vor: &str, t: Option<&Wert>| {
        if let Some(Wert::Tabelle(t)) = t {
            for (alias, w) in t {
                teile.push(format!("{vor}{alias}={}", w.kanonisch()));
            }
        }
    };
    for a in ABSCHNITTE {
        abschnitt(&format!("{a}."), m.feld(a));
    }
    abschnitt(
        "workspace.dependencies.",
        m.pfad(&["workspace", "dependencies"]),
    );
    if let Some(Wert::Tabelle(ziele)) = m.feld("target") {
        for (ziel, w) in ziele {
            for a in ABSCHNITTE {
                abschnitt(&format!("target.{ziel}.{a}."), w.feld(a));
            }
        }
    }
    teile.sort();
    teile.join(";")
}

/// Namen der direkten Abhaengigkeiten (Alias und, falls umbenannt, Paketname).
pub fn abhaengigkeits_namen(m: &Wert) -> Vec<String> {
    let mut aus = Vec::new();
    for a in ABSCHNITTE {
        if let Some(Wert::Tabelle(t)) = m.feld(a) {
            for (alias, w) in t {
                aus.push(alias.clone());
                if let Some(p) = w.feld("package").and_then(Wert::text) {
                    aus.push(p.to_string());
                }
            }
        }
    }
    if let Some(Wert::Tabelle(ziele)) = m.feld("target") {
        for w in ziele.values() {
            for a in ABSCHNITTE {
                if let Some(Wert::Tabelle(t)) = w.feld(a) {
                    aus.extend(t.keys().cloned());
                }
            }
        }
    }
    aus
}

/// Pfad eines Crate-Ordners relativ zur Wurzel.
pub fn crate_ordner(wurzel: &Path, krate: &str) -> PathBuf {
    wurzel.join("crates").join(krate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identitaet_umfasst_features_und_paket() {
        let a = toml::lies("[dependencies]\nsha2 = { workspace = true }\n")
            .unwrap_or(Wert::Roh(String::new()));
        let b =
            toml::lies("[dependencies]\nsha2 = { version = \"0.10\", default-features = false }\n")
                .unwrap_or(Wert::Roh(String::new()));
        let c = toml::lies("[dependencies]\nsha2 = { package = \"sha3\", version = \"0.10\" }\n")
            .unwrap_or(Wert::Roh(String::new()));
        assert_ne!(abhaengigkeiten(&a), abhaengigkeiten(&b));
        assert_ne!(abhaengigkeiten(&b), abhaengigkeiten(&c));
        assert!(abhaengigkeits_namen(&c).contains(&"sha3".to_string()));
    }

    #[test]
    fn vererbung_nur_echt() {
        let d = Path::new("/nirgends");
        let gut = toml::lies("[lints]\nworkspace = true\n").unwrap_or(Wert::Roh(String::new()));
        let gepunktet = toml::lies("lints.workspace = true\n").unwrap_or(Wert::Roh(String::new()));
        let text =
            toml::lies("[package.metadata]\nx = \"\"\"\n[lints]\nworkspace = true\n\"\"\"\n")
                .unwrap_or(Wert::Roh(String::new()));
        let mehr =
            toml::lies("[lints]\nworkspace = true\n[lints.clippy]\nunwrap_used = \"allow\"\n")
                .unwrap_or(Wert::Roh(String::new()));
        assert!(pruefe_crate("x", d, &gut).is_empty());
        assert!(pruefe_crate("x", d, &gepunktet).is_empty());
        assert!(!pruefe_crate("x", d, &text).is_empty());
        assert!(!pruefe_crate("x", d, &mehr).is_empty());
    }
}
