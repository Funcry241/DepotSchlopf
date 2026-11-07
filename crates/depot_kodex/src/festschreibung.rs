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
use crate::manifest;
use crate::toml::Wert;

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

/// Text ohne Windows-Zeilenenden: Ein Checkout mit CRLF aendert die Festschreibung nicht.
pub fn normalisiert(bytes: &[u8]) -> Vec<u8> {
    let mut aus = Vec::with_capacity(bytes.len());
    let mut it = bytes.iter().peekable();
    while let Some(b) = it.next() {
        if *b == b'\r' && it.peek() == Some(&&b'\n') {
            continue;
        }
        aus.push(*b);
    }
    aus
}

pub fn sha_bytes(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Lints und Profile der Wurzel-Cargo.toml, semantisch gelesen: wer die harten Lints lockert,
/// faellt auf, gleich in welcher Schreibweise.
fn lints(wurzel_toml: &Wert) -> String {
    let leer = Wert::Tabelle(BTreeMap::new());
    sha(&format!(
        "depotschlopf-lints-v2|{}|{}",
        wurzel_toml
            .pfad(&["workspace", "lints"])
            .unwrap_or(&leer)
            .kanonisch(),
        wurzel_toml.feld("profile").unwrap_or(&leer).kanonisch()
    ))
}

/// Berechnet die Festschreibung des aktuellen Baums.
///
/// # Errors
/// Wenn Dateien fehlen oder sich nicht lesen lassen.
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
    // Vor der ersten RSS-Einrichtung gibt es keine Liste. Danach werden auch Entfernung
    // und jede Aenderung durch den Vergleich mit der Festschreibung erkannt.
    match fs::read(wurzel.join("crates/depot_sammler/src/rss_quellen.txt")) {
        Ok(bytes) => {
            f.insert("rss_quellen".into(), sha_bytes(&normalisiert(&bytes)));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("RSS-Quellenliste nicht lesbar: {e}")),
    }
    let wurzel_toml = manifest::lies(&wurzel.join("Cargo.toml"))?;
    f.insert("lints".into(), lints(&wurzel_toml));
    f.insert(
        "abhaengigkeiten.workspace".into(),
        manifest::abhaengigkeiten(&wurzel_toml),
    );
    let lock = fs::read(wurzel.join("Cargo.lock"))
        .map_err(|e| format!("Cargo.lock: nicht lesbar: {e}"))?;
    f.insert("cargo_lock".into(), sha_bytes(&normalisiert(&lock)));
    let mut crates = Vec::new();
    for eintrag in
        fs::read_dir(wurzel.join("crates")).map_err(|e| format!("crates/: nicht lesbar: {e}"))?
    {
        let eintrag = eintrag.map_err(|e| format!("crates/: nicht lesbar: {e}"))?;
        let name = eintrag.file_name().to_string_lossy().into_owned();
        let toml = manifest::lies(&eintrag.path().join("Cargo.toml"))?;
        f.insert(
            format!("abhaengigkeiten.{name}"),
            manifest::abhaengigkeiten(&toml),
        );
        crates.push(name);
    }
    crates.sort();
    f.insert("crates".into(), crates.join(","));
    Ok(f)
}

pub fn als_text(f: &BTreeMap<String, String>) -> String {
    let mut s = String::from(
        "# DepotSchlopf – Festschreibung. Aendern nur mit Grenzfreigabe des Eigentuemers in\n\
         # kodex/freigabe.txt (R30). Neu erzeugen: run.cmd festschreibung\n",
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
    use crate::toml;

    #[test]
    fn gelockerte_lints_fallen_auf() {
        let a = toml::lies("[workspace.lints.clippy]\nunwrap_used = \"deny\"\n");
        let b = toml::lies("[workspace.lints.clippy]\nunwrap_used = \"allow\"\n");
        let c = toml::lies("workspace.lints.clippy.unwrap_used = \"deny\"\n");
        let (Ok(a), Ok(b), Ok(c)) = (a, b, c) else {
            return;
        };
        assert_ne!(lints(&a), lints(&b));
        assert_eq!(lints(&a), lints(&c));
    }

    #[test]
    fn zeilenenden_aendern_nichts() {
        assert_eq!(normalisiert(b"a\r\nb\n"), b"a\nb\n".to_vec());
        assert_eq!(normalisiert(b"a\rb"), b"a\rb".to_vec());
    }
}
