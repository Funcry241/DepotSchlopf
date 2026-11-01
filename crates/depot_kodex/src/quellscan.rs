//! Quellscan: billige, harte Strukturpruefungen am Text des Produktivcodes.

use std::fs;
use std::path::{Path, PathBuf};

/// Kern und Regelwerk sind rein: keine Dateien, kein Netz, keine Uhr, keine Umgebung.
const REIN: [&str; 2] = ["depot_kern", "depot_regelwerk"];
const UNREIN: [&str; 8] = [
    "std::fs",
    "std::net",
    "std::env",
    "std::time",
    "std::process",
    "std::thread",
    "SystemTime",
    "Instant::",
];
/// Netzzugang nur in diesen Crates. Erweitern nur ueber eine KodexAenderung.
const NETZ_ERLAUBT: [&str; 0] = [];
const NETZ: [&str; 6] = [
    "std::net",
    "reqwest",
    "ureq",
    "hyper",
    "TcpStream",
    "UdpSocket",
];
/// Diese Lints duerfen im Produktivcode nie ausgesetzt werden.
const HARTE_LINTS: [&str; 9] = [
    "unwrap_used",
    "expect_used",
    "panic",
    "indexing_slicing",
    "arithmetic_side_effects",
    "float_arithmetic",
    "cast_possible_truncation",
    "cast_sign_loss",
    "unsafe_code",
];

fn rs_dateien(dir: &Path, aus: &mut Vec<PathBuf>) {
    let Ok(eintraege) = fs::read_dir(dir) else {
        return;
    };
    for e in eintraege.flatten() {
        let p = e.path();
        if p.is_dir() {
            rs_dateien(&p, aus);
        } else if p.extension().is_some_and(|x| x == "rs") {
            aus.push(p);
        }
    }
}

/// Alle `#[allow(..)]`/`#[expect(..)]`-Attribute einer Datei als Text.
fn ausnahmen(text: &str) -> Vec<&str> {
    let mut aus = Vec::new();
    for muster in ["#[allow(", "#![allow(", "#[expect(", "#![expect("] {
        for (pos, _) in text.match_indices(muster) {
            let rest = text.get(pos..).unwrap_or("");
            let ende = rest.find(")]").unwrap_or(rest.len());
            aus.push(rest.get(..ende).unwrap_or(rest));
        }
    }
    aus
}

fn pruefe_datei(krate: &str, rolle_produktiv: bool, pfad: &Path, text: &str) -> Vec<String> {
    let mut fehler = Vec::new();
    let anzeige = pfad.display();
    if rolle_produktiv && REIN.contains(&krate) {
        for t in UNREIN {
            if text.contains(t) {
                fehler.push(format!("{anzeige}: {t} im reinen Crate {krate}"));
            }
        }
    }
    if rolle_produktiv && !NETZ_ERLAUBT.contains(&krate) {
        for t in NETZ {
            if text.contains(t) {
                fehler.push(format!("{anzeige}: Netzzugang {t} ohne Erlaubnis"));
            }
        }
    }
    for a in ausnahmen(text) {
        if !a.contains("reason") {
            fehler.push(format!("{anzeige}: Lint-Ausnahme ohne Grund: {a}"));
        }
        if rolle_produktiv {
            for l in HARTE_LINTS {
                if a.contains(l) {
                    fehler.push(format!("{anzeige}: harter Lint {l} ausgesetzt"));
                }
            }
        }
    }
    fehler
}

pub fn pruefe(wurzel: &Path) -> Vec<String> {
    let mut fehler = Vec::new();
    let Ok(crates) = fs::read_dir(wurzel.join("crates")) else {
        return vec!["crates/ nicht lesbar".into()];
    };
    for c in crates.flatten() {
        let krate = c.file_name().to_string_lossy().into_owned();
        match fs::read_to_string(c.path().join("Cargo.toml")) {
            Ok(t)
                if t.replace("\r\n", "\n")
                    .contains("[lints]\nworkspace = true") => {}
            _ => fehler.push(format!(
                "crates/{krate}/Cargo.toml erbt die Workspace-Lints nicht"
            )),
        }
        if krate == "depot_kodex" {
            continue;
        }
        for (unter, produktiv) in [("src", true), ("tests", false)] {
            let mut dateien = Vec::new();
            rs_dateien(&c.path().join(unter), &mut dateien);
            for d in dateien {
                match fs::read_to_string(&d) {
                    Ok(t) => fehler.extend(pruefe_datei(&krate, produktiv, &d, &t)),
                    Err(e) => fehler.push(format!("{}: {e}", d.display())),
                }
            }
        }
    }
    fehler
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regelwerk_bleibt_rein() {
        let p = Path::new("x.rs");
        assert_eq!(
            pruefe_datei("depot_regelwerk", true, p, "use std::time::SystemTime;").len(),
            2
        );
        assert!(pruefe_datei("depot_ki", true, p, "use std::fs;").is_empty());
    }

    #[test]
    fn kein_netz_ohne_erlaubnis() {
        let p = Path::new("x.rs");
        assert!(!pruefe_datei("depot_ki", true, p, "use reqwest::Client;").is_empty());
    }

    #[test]
    fn lint_ausnahmen() {
        let p = Path::new("x.rs");
        assert!(!pruefe_datei("depot_ki", false, p, "#[allow(dead_code)]").is_empty());
        assert!(
            pruefe_datei("depot_ki", false, p, "#[allow(dead_code, reason = \"x\")]").is_empty()
        );
        assert!(
            !pruefe_datei(
                "depot_ki",
                true,
                p,
                "#[allow(clippy::unwrap_used, reason = \"bequem\")]"
            )
            .is_empty()
        );
    }
}
