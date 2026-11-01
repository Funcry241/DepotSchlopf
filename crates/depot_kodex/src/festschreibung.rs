//! Festschreibung: Was ohne ausdrueckliche Freigabe nie geaendert werden darf.
//! Die Werte werden aus dem aktuellen Baum berechnet und mit `kodex/festschreibung.txt` verglichen.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use depot_kern::{BeraterId, Grenzen};
use depot_ki::{KiDeckel, KiGrenzen};
use sha2::{Digest, Sha256};

use crate::deklaration::Klasse;

fn sha(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn lies(pfad: &Path) -> Result<String, String> {
    fs::read_to_string(pfad).map_err(|e| format!("{}: {e}", pfad.display()))
}

/// Varianten von `InstrumentArt`, textuell gelesen: eine neue Produktart faellt sofort auf.
fn produktarten(wurzel: &Path) -> Result<String, String> {
    let text = lies(&wurzel.join("crates/depot_kern/src/instrument.rs"))?;
    let mut zeilen = text
        .lines()
        .skip_while(|z| !z.contains("pub enum InstrumentArt"));
    if zeilen.next().is_none() {
        return Err("enum InstrumentArt nicht gefunden".into());
    }
    let arten: Vec<String> = zeilen
        .map(str::trim)
        .take_while(|z| !z.starts_with('}'))
        .filter(|z| !z.is_empty() && !z.starts_with("//") && !z.starts_with('#'))
        .map(|z| {
            z.chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect()
        })
        .collect();
    Ok(arten.join(","))
}

/// Namen aller Abhaengigkeiten einer Cargo.toml (normal, dev, build, target).
fn abhaengigkeiten(text: &str) -> String {
    let mut namen = BTreeSet::new();
    let mut in_abschnitt = false;
    for zeile in text.lines().map(str::trim) {
        if zeile.starts_with('[') {
            let kopf = zeile.trim_matches(|c| c == '[' || c == ']');
            in_abschnitt = kopf.ends_with("dependencies");
            if let Some((abschnitt, name)) = kopf.rsplit_once('.')
                && abschnitt.ends_with("dependencies")
            {
                namen.insert(name.to_string());
            }
            continue;
        }
        if in_abschnitt && !zeile.is_empty() && !zeile.starts_with('#') {
            let schluessel = zeile.split(['=', '.']).next().unwrap_or("").trim();
            if !schluessel.is_empty() {
                namen.insert(schluessel.to_string());
            }
        }
    }
    namen.into_iter().collect::<Vec<_>>().join(",")
}

/// Lint- und Profil-Abschnitte der Wurzel-Cargo.toml: wer die harten Lints lockert, faellt auf.
fn lints(text: &str) -> String {
    let mut aus = String::new();
    let mut drin = false;
    for zeile in text.lines().map(str::trim) {
        if zeile.starts_with('[') {
            drin = zeile.starts_with("[workspace.lints") || zeile.starts_with("[profile");
        }
        if drin && !zeile.is_empty() {
            aus.push_str(zeile);
            aus.push('\n');
        }
    }
    sha(&aus)
}

/// Berechnet die Festschreibung des aktuellen Baums.
///
/// # Errors
/// Wenn Dateien fehlen.
pub fn berechne(wurzel: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut f = BTreeMap::new();
    f.insert("grenzen".into(), Grenzen::standard().fingerabdruck());
    f.insert(
        "ki_budget".into(),
        sha(&format!(
            "depotschlopf-ki-v1|{:?}|{:?}|{:?}",
            KiGrenzen::standard(),
            KiDeckel::standard(BeraterId::Dachs),
            KiDeckel::standard(BeraterId::Maus)
        )),
    );
    f.insert("produktarten".into(), produktarten(wurzel)?);
    let wurzel_toml = lies(&wurzel.join("Cargo.toml"))?;
    f.insert("lints".into(), lints(&wurzel_toml));
    f.insert(
        "abhaengigkeiten.workspace".into(),
        abhaengigkeiten(&wurzel_toml),
    );
    let mut crates = Vec::new();
    for eintrag in fs::read_dir(wurzel.join("crates")).map_err(|e| format!("crates/: {e}"))? {
        let eintrag = eintrag.map_err(|e| format!("crates/: {e}"))?;
        let name = eintrag.file_name().to_string_lossy().into_owned();
        let toml = lies(&eintrag.path().join("Cargo.toml"))?;
        f.insert(format!("abhaengigkeiten.{name}"), abhaengigkeiten(&toml));
        crates.push(name);
    }
    crates.sort();
    f.insert("crates".into(), crates.join(","));
    Ok(f)
}

pub fn als_text(f: &BTreeMap<String, String>) -> String {
    let mut s = String::from(
        "# DepotSchlopf – Festschreibung. Aendern nur mit Klasse FestschreibungAenderung\n\
         # und ausdruecklicher Freigabe des Eigentuemers. Neu erzeugen: run.cmd festschreibung\n",
    );
    for (k, v) in f {
        let _ = writeln!(s, "{k}: {v}");
    }
    s
}

fn aus_text(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter(|z| !z.starts_with('#') && !z.trim().is_empty())
        .filter_map(|z| z.split_once(": "))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

pub fn pruefe(wurzel: &Path, klasse: Klasse, diff: &BTreeSet<String>) -> Vec<String> {
    let berechnet = match berechne(wurzel) {
        Ok(b) => b,
        Err(e) => return vec![format!("Festschreibung nicht berechenbar: {e}")],
    };
    let fest = match lies(&wurzel.join(crate::FESTSCHREIBUNG)) {
        Ok(t) => aus_text(&t),
        Err(e) => return vec![format!("{} fehlt: {e}", crate::FESTSCHREIBUNG)],
    };
    let mut fehler = Vec::new();
    let schluessel: BTreeSet<&String> = berechnet.keys().chain(fest.keys()).collect();
    for k in schluessel {
        let (b, s) = (berechnet.get(k), fest.get(k));
        if b != s {
            fehler.push(format!(
                "Festschreibung verletzt bei {k}: festgeschrieben {:?}, im Baum {:?}. \
                 Nur mit Klasse FestschreibungAenderung und Freigabe des Eigentuemers.",
                s.map_or("-", String::as_str),
                b.map_or("-", String::as_str)
            ));
        }
    }
    if klasse == Klasse::FestschreibungAenderung && !diff.contains(crate::FESTSCHREIBUNG) {
        fehler.push("FestschreibungAenderung ohne neue Festschreibung".into());
    }
    fehler
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abhaengigkeiten_lesen() {
        let t = "[package]\nname = \"x\"\n[dependencies]\ndepot_kern.workspace = true\nsha2 = { workspace = true }\n[dev-dependencies.proptest]\nversion = \"1\"\n[lints]\nworkspace = true\n";
        assert_eq!(abhaengigkeiten(t), "depot_kern,proptest,sha2");
    }

    #[test]
    fn gelockerte_lints_fallen_auf() {
        let a = "[workspace.lints.clippy]\nunwrap_used = \"deny\"\n";
        let b = "[workspace.lints.clippy]\nunwrap_used = \"allow\"\n";
        assert_ne!(lints(a), lints(b));
    }
}
