//! Quellscan: Code syntaktisch, Manifeste semantisch (R26), Lesefehler sind Fehler (R29), Links
//! sind verboten (R24).
//!
//! - Jedes Produktiv-Target (Bibliothek, Programm) verbietet die harten Lints im Crate-Kopf mit
//!   einem unbedingten `#![forbid(..)]`. Das gilt auch fuer den Kodex selbst.
//! - Keine Datei setzt einen harten Lint oder eine Gruppe, die ihn enthaelt, herab, auch nicht
//!   hinter `cfg_attr`. Jede Ausnahme (`allow`, `expect`) nennt einen Grund.
//! - Kern und Regelwerk sind rein: kein Datei-, Netz-, Umgebungs-, Prozess-, Thread- oder
//!   Zeitzugriff ueber `std`, aufgeloest ueber gruppierte, umbenannte und Glob-Importe sowie
//!   `extern crate`. Reine Crates haengen nur von reinen Crates und ruhigen Bibliotheken ab.
//! - Netzzugang nur in den Crates aus `NETZ_ERLAUBT`.
//!
//! - Geprueft wird, was kompiliert werden kann: Im Crate liegen nur `Cargo.toml`, `src/` und
//!   `tests/`; darin nur `.rs`-Dateien, und jede wird vollstaendig geprueft. `#[path]` und
//!   `include!` sind verboten, Target-Pfade enden auf `.rs`. Damit liegt jede kompilierte Datei
//!   im geprueften Quellbaum (K7-03).
//! - In reinen Crates zaehlt jede Bedeutung eines Alias; mehrdeutige Aliasse und das Einbinden
//!   von `std` als Ganzes (Weiterreichung) sind verboten, verbotene Module zaehlen an jeder
//!   Stelle eines Pfads (K7-01, K7-02).
//!
//! Grenze: Makros werden nicht expandiert und Reexporte fremder Crates nicht verfolgt. Dafuer
//! stehen die Crate-Grenzen (reine Crates haengen nur von reinen ab) und das Verbot von
//! `include!` und `#[path]`.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::lexer::{self, Token};
use crate::manifest::{self, HARTE_LINTS};
use crate::rust;

/// Reine Crates: keine Dateien, kein Netz, keine Uhr, keine Umgebung.
const REIN: [&str; 2] = ["depot_kern", "depot_regelwerk"];
/// Bibliotheken ohne Datei- und Netzzugriff, von denen reine Crates abhaengen duerfen.
const RUHIG: [&str; 1] = ["sha2"];
const UNREIN: [&[&str]; 6] = [
    &["std", "fs"],
    &["std", "net"],
    &["std", "env"],
    &["std", "time"],
    &["std", "process"],
    &["std", "thread"],
];
/// Netzzugang nur in diesen Crates. Erweitern nur ueber eine KodexAenderung.
const NETZ_ERLAUBT: [&str; 0] = [];
const NETZ_CRATES: [&str; 5] = ["reqwest", "ureq", "hyper", "tokio", "socket2"];
const NETZ_TYPEN: [&str; 3] = ["TcpStream", "UdpSocket", "TcpListener"];
/// Gruppen, die einen harten Lint enthalten.
const GRUPPEN: [&str; 3] = ["clippy::restriction", "clippy::pedantic", "warnings"];
const HERAB: [&str; 3] = ["allow", "expect", "warn"];

/// Ein Eintrag im Quellbaum, ohne Links zu folgen.
fn eintraege(dir: &Path, fehler: &mut Vec<String>) -> Vec<std::path::PathBuf> {
    let anzeige = dir.display();
    let liste = match fs::read_dir(dir) {
        Ok(l) => l,
        Err(e) => {
            fehler.push(format!("{anzeige}: nicht lesbar: {e}"));
            return Vec::new();
        }
    };
    let mut aus = Vec::new();
    for e in liste {
        match e {
            Ok(e) => aus.push(e.path()),
            Err(e) => fehler.push(format!("{anzeige}: Eintrag nicht lesbar: {e}")),
        }
    }
    aus.sort();
    aus
}

/// Alle `.rs`-Dateien unter `dir`. Links, Sonderdateien und Lesefehler sind Fehler.
fn rs_dateien(dir: &Path, aus: &mut Vec<std::path::PathBuf>, fehler: &mut Vec<String>) {
    for p in eintraege(dir, fehler) {
        match fs::symlink_metadata(&p) {
            Err(e) => fehler.push(format!("{}: nicht lesbar: {e}", p.display())),
            Ok(m) if m.file_type().is_symlink() => {
                fehler.push(format!(
                    "{}: Link im Quellbaum ist verboten (R24)",
                    p.display()
                ));
            }
            Ok(m) if m.is_dir() => rs_dateien(&p, aus, fehler),
            Ok(m) if m.is_file() => {
                if p.extension().is_some_and(|x| x == "rs") {
                    aus.push(p);
                } else {
                    // Cargo baut auch Dateien mit anderer Endung (Targetpfad, `#[path]`). Im
                    // Quellbaum liegt deshalb nur Rust-Code, und der wird vollstaendig geprueft.
                    fehler.push(format!(
                        "{}: keine .rs-Datei im Quellbaum (geprueft wird, was kompiliert werden kann)",
                        p.display()
                    ));
                }
            }
            Ok(_) => fehler.push(format!("{}: weder Datei noch Ordner", p.display())),
        }
    }
}

/// Ein Ordner, der fehlen darf; gibt es ihn, muss er ein lesbarer Ordner sein.
fn optionaler_ordner(dir: &Path, fehler: &mut Vec<String>) -> bool {
    match fs::symlink_metadata(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            fehler.push(format!("{}: nicht lesbar: {e}", dir.display()));
            false
        }
        Ok(m) if m.file_type().is_symlink() => {
            fehler.push(format!(
                "{}: Link im Quellbaum ist verboten (R24)",
                dir.display()
            ));
            false
        }
        Ok(m) if m.is_dir() => true,
        Ok(_) => {
            fehler.push(format!("{}: nicht lesbar: kein Ordner", dir.display()));
            false
        }
    }
}

/// Die fuehrenden inneren Attribute einer Datei (Crate-Kopf).
fn kopf(t: &[Token]) -> &[Token] {
    let mut i = 0_usize;
    while t.get(i).is_some_and(|x| x.ist("#"))
        && t.get(i.saturating_add(1)).is_some_and(|x| x.ist("!"))
        && t.get(i.saturating_add(2)).is_some_and(|x| x.ist("["))
    {
        let mut tiefe = 0_usize;
        let mut j = i.saturating_add(2);
        loop {
            match t.get(j) {
                None => return t,
                Some(x) if x.ist("[") || x.ist("(") || x.ist("{") => {
                    tiefe = tiefe.saturating_add(1);
                }
                Some(x) if x.ist("]") || x.ist(")") || x.ist("}") => {
                    tiefe = tiefe.saturating_sub(1);
                    if tiefe == 0 {
                        break;
                    }
                }
                Some(_) => {}
            }
            j = j.saturating_add(1);
        }
        i = j.saturating_add(1);
    }
    t.get(..i).unwrap_or_default()
}

/// Der Crate-Kopf verbietet jeden harten Lint unbedingt.
pub fn pruefe_kopf(anzeige: &str, text: &str) -> Vec<String> {
    let t = match lexer::zerlege(text) {
        Ok(t) => t,
        Err(e) => return vec![format!("{anzeige}: nicht lesbar: {e}")],
    };
    let verboten: BTreeSet<String> = rust::lint_attribute(kopf(&t))
        .into_iter()
        .filter(|a| a.ebene == "forbid" && a.innen && !a.bedingt)
        .flat_map(|a| a.lints)
        .collect();
    HARTE_LINTS
        .iter()
        .filter(|l| !verboten.contains(**l))
        .map(|l| {
            format!(
                "{anzeige}: harter Lint {l} steht im Crate-Kopf nicht unbedingt auf forbid \
                 (#![forbid(..)] am Dateianfang, nicht hinter cfg_attr)"
            )
        })
        .collect()
}

/// Importe in reinen Crates: kein `std` als Ganzes, keine mehrdeutigen Aliasse, kein Glob aus
/// `std`.
fn rein_importe(krate: &str, anzeige: &str, importe: &[rust::Import]) -> Vec<String> {
    let mut fehler = Vec::new();
    for imp in importe.iter().filter(|i| !i.glob && i.pfad == ["std"]) {
        fehler.push(format!(
            "{anzeige}: std als Ganzes eingebunden als {} im reinen Crate {krate} \
             (Weiterreichung von std, K7-02)",
            imp.alias.as_deref().unwrap_or("_")
        ));
    }
    for a in rust::mehrdeutige_aliasse(importe) {
        fehler.push(format!(
            "{anzeige}: mehrdeutiger Alias {a} im reinen Crate {krate} (K7-01)"
        ));
    }
    for imp in importe.iter().filter(|i| i.glob) {
        if imp.pfad.first().is_some_and(|s| s == "std") && imp.pfad.len() == 1 {
            fehler.push(format!(
                "{anzeige}: Glob-Import aus std im reinen Crate {krate}"
            ));
        }
    }

    fehler
}

/// Prueft eine Quelldatei. `produktiv` heisst: liegt unter `src/`.
pub fn pruefe_datei(krate: &str, produktiv: bool, anzeige: &str, text: &str) -> Vec<String> {
    let mut fehler = Vec::new();
    let t = match lexer::zerlege(text) {
        Ok(t) => t,
        Err(e) => return vec![format!("{anzeige}: nicht lesbar: {e}")],
    };
    for a in rust::lint_attribute(&t) {
        if matches!(a.ebene.as_str(), "allow" | "expect") && !a.grund {
            fehler.push(format!(
                "{anzeige}: Lint-Ausnahme ohne Grund: {}({})",
                a.ebene,
                a.lints.join(", ")
            ));
        }
        if produktiv && HERAB.contains(&a.ebene.as_str()) {
            for l in &a.lints {
                if HARTE_LINTS.contains(&l.as_str()) || GRUPPEN.contains(&l.as_str()) || l == "?" {
                    fehler.push(format!(
                        "{anzeige}: harter Lint {l} ausgesetzt ({}{})",
                        a.ebene,
                        if a.bedingt { " hinter cfg_attr" } else { "" }
                    ));
                }
            }
        }
    }
    // Beide Wege binden Code ein, der nicht im Quellbaum unter seinem eigenen Namen liegt.
    // Ohne sie kompiliert Cargo nur die Target-Wurzeln und Module in Standardlage, und die
    // liegen alle als .rs-Dateien im Quellbaum (K7-03).
    for (i, tok) in t.iter().enumerate() {
        if tok.ist_wort("include") && t.get(i.saturating_add(1)).is_some_and(|x| x.ist("!")) {
            fehler.push(format!(
                "{anzeige}: include! bindet Code ausserhalb des Quellscans ein"
            ));
        }
    }
    if rust::attributnamen(&t).iter().any(|n| n == "path") {
        fehler.push(format!(
            "{anzeige}: #[path] bindet Module ausserhalb der Standardlage ein und ist verboten"
        ));
    }
    if !produktiv {
        return fehler;
    }
    let importe = match rust::importe(&t) {
        Ok(i) => i,
        Err(e) => {
            fehler.push(format!("{anzeige}: nicht lesbar: {e}"));
            return fehler;
        }
    };
    let mut pfade: Vec<Vec<String>> = importe.iter().map(|i| i.pfad.clone()).collect();
    pfade.extend(rust::pfade(&t, &importe));
    let rein = REIN.contains(&krate);
    if rein {
        fehler.extend(rein_importe(krate, anzeige, &importe));
    }
    let mut gemeldet = BTreeSet::new();
    for p in &pfade {
        if rein {
            for u in UNREIN {
                if rust::enthaelt(p, u) && gemeldet.insert(u.join("::")) {
                    fehler.push(format!(
                        "{anzeige}: {} im reinen Crate {krate} ({})",
                        u.join("::"),
                        p.join("::")
                    ));
                }
            }
        }
        if !NETZ_ERLAUBT.contains(&krate) {
            let netz = rust::enthaelt(p, &["std", "net"])
                || p.first().is_some_and(|s| NETZ_CRATES.contains(&s.as_str()));
            if netz && gemeldet.insert(format!("netz {}", p.join("::"))) {
                fehler.push(format!(
                    "{anzeige}: Netzzugang {} ohne Erlaubnis",
                    p.join("::")
                ));
            }
        }
    }
    if !NETZ_ERLAUBT.contains(&krate) {
        for tok in &t {
            if let Token::Wort(w) = tok
                && NETZ_TYPEN.contains(&w.as_str())
                && gemeldet.insert(format!("typ {w}"))
            {
                fehler.push(format!("{anzeige}: Netzzugang {w} ohne Erlaubnis"));
            }
        }
    }
    fehler
}

fn pruefe_crate(wurzel: &Path, krate: &str, fehler: &mut Vec<String>) {
    let dir = manifest::crate_ordner(wurzel, krate);
    let m = match manifest::lies(&dir.join("Cargo.toml")) {
        Ok(m) => m,
        Err(e) => {
            fehler.push(e);
            return;
        }
    };
    fehler.extend(manifest::pruefe_crate(krate, &dir, &m));
    for p in eintraege(&dir, fehler) {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !matches!(name.as_str(), "Cargo.toml" | "src" | "tests") {
            fehler.push(format!(
                "crates/{krate}/{name}: unbekannter Eintrag im Crate (nur Cargo.toml, src/, tests/; \
                 Cargo baut sonst z. B. examples/ und benches/ ungeprueft)"
            ));
        }
    }
    if REIN.contains(&krate) {
        for name in manifest::abhaengigkeits_namen(&m) {
            if !REIN.contains(&name.as_str()) && !RUHIG.contains(&name.as_str()) {
                fehler.push(format!(
                    "crates/{krate}: reines Crate haengt von {name} ab (nur reine Crates und {})",
                    RUHIG.join(", ")
                ));
            }
        }
    }
    match manifest::produktiv_wurzeln(&dir, &m) {
        Ok(wurzeln) => {
            if wurzeln.is_empty() {
                fehler.push(format!("crates/{krate}: kein Produktiv-Target"));
            }
            for w in wurzeln {
                let anzeige = format!("crates/{krate}/{w}");
                match fs::read_to_string(dir.join(&w)) {
                    Ok(t) => fehler.extend(pruefe_kopf(&anzeige, &t)),
                    Err(e) => fehler.push(format!("{anzeige}: nicht lesbar: {e}")),
                }
            }
        }
        Err(e) => fehler.push(format!("crates/{krate}: {e}")),
    }
    for (unter, produktiv) in [("src", true), ("tests", false)] {
        let ordner = dir.join(unter);
        if !optionaler_ordner(&ordner, fehler) {
            if produktiv {
                fehler.push(format!("crates/{krate}/src fehlt"));
            }
            continue;
        }
        let mut dateien = Vec::new();
        rs_dateien(&ordner, &mut dateien, fehler);
        for d in dateien {
            let anzeige = d
                .strip_prefix(wurzel)
                .unwrap_or(&d)
                .to_string_lossy()
                .replace('\\', "/");
            match fs::read_to_string(&d) {
                Ok(t) => fehler.extend(pruefe_datei(krate, produktiv, &anzeige, &t)),
                Err(e) => fehler.push(format!("{anzeige}: nicht lesbar: {e}")),
            }
        }
    }
}

pub fn pruefe(wurzel: &Path) -> Vec<String> {
    let mut fehler = Vec::new();
    match manifest::lies(&wurzel.join("Cargo.toml")) {
        Ok(m) => fehler.extend(manifest::pruefe_workspace(&m)),
        Err(e) => fehler.push(e),
    }
    let crates_dir = wurzel.join("crates");
    for p in eintraege(&crates_dir, &mut fehler) {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        match fs::symlink_metadata(&p) {
            Err(e) => fehler.push(format!("crates/{name}: nicht lesbar: {e}")),
            Ok(m) if m.file_type().is_symlink() => {
                fehler.push(format!("crates/{name}: Link ist verboten (R24)"));
            }
            Ok(m) if m.is_dir() => pruefe_crate(wurzel, &name, &mut fehler),
            Ok(_) => fehler.push(format!("crates/{name}: kein Crate-Ordner")),
        }
    }
    fehler
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regelwerk_bleibt_rein() {
        let p = "x.rs";
        assert!(!pruefe_datei("depot_regelwerk", true, p, "use std::time::SystemTime;").is_empty());
        assert!(!pruefe_datei("depot_kern", true, p, "use std::{env, fs as d};").is_empty());
        assert!(pruefe_datei("depot_ki", true, p, "use std::fs;").is_empty());
        assert!(
            pruefe_datei(
                "depot_kern",
                true,
                p,
                "// std::fs\nconst X: &str = \"std::env\";"
            )
            .is_empty()
        );
    }

    #[test]
    fn kein_netz_ohne_erlaubnis() {
        let p = "x.rs";
        assert!(!pruefe_datei("depot_ki", true, p, "use reqwest::Client;").is_empty());
        assert!(!pruefe_datei("depot_ki", true, p, "use std::{net::TcpStream as T};").is_empty());
    }

    #[test]
    fn lint_ausnahmen() {
        let p = "x.rs";
        assert!(!pruefe_datei("depot_ki", false, p, "#[allow(dead_code)] fn f() {}").is_empty());
        assert!(
            pruefe_datei(
                "depot_ki",
                false,
                p,
                "#[allow(dead_code, reason = \"x\")] fn f() {}"
            )
            .is_empty()
        );
        assert!(
            !pruefe_datei(
                "depot_ki",
                true,
                p,
                "#[allow(clippy::unwrap_used, reason = \"bequem\")] fn f() {}"
            )
            .is_empty()
        );
        assert!(
            !pruefe_datei(
                "depot_ki",
                true,
                p,
                "#[cfg_attr(test, expect(clippy::pedantic, reason = \"x\"))] fn f() {}"
            )
            .is_empty()
        );
        assert!(
            pruefe_datei(
                "depot_ki",
                false,
                p,
                "#[allow(clippy::panic, reason = \"Test\")] fn f() {}"
            )
            .is_empty()
        );
    }

    #[test]
    fn reexport_und_mehrdeutigkeit() {
        let p = "x.rs";
        assert!(
            !pruefe_datei(
                "depot_kern",
                true,
                p,
                "pub mod b { pub use std as platform; }"
            )
            .is_empty()
        );
        assert!(
            !pruefe_datei("depot_kern", true, p, "fn f() { crate::m::std::fs::x(); }").is_empty()
        );
        assert!(
            !pruefe_datei(
                "depot_kern",
                true,
                p,
                "fn a() { use core as b; } fn c() { use alloc as b; }"
            )
            .is_empty()
        );
        assert!(!pruefe_datei("depot_ki", false, p, "#[path = \"x.md\"] mod m;").is_empty());
        assert!(
            pruefe_datei(
                "depot_kern",
                true,
                p,
                "use core::mem; fn f() { mem::size_of::<u8>(); }"
            )
            .is_empty()
        );
    }

    #[test]
    fn kopf_verlangt_forbid() {
        let voll = format!(
            "//! Doku\n#![forbid({})]\nfn main() {{}}\n",
            HARTE_LINTS.join(", ")
        );
        assert!(pruefe_kopf("m.rs", &voll).is_empty());
        assert_eq!(pruefe_kopf("m.rs", "fn main() {}").len(), HARTE_LINTS.len());
        let bedingt = format!(
            "#![cfg_attr(not(test), forbid({}))]",
            HARTE_LINTS.join(", ")
        );
        assert!(!pruefe_kopf("m.rs", &bedingt).is_empty());
        let spaet = format!(
            "fn f() {{}}\nmod m {{ #![forbid({})] }}",
            HARTE_LINTS.join(", ")
        );
        assert!(!pruefe_kopf("m.rs", &spaet).is_empty());
    }
}
