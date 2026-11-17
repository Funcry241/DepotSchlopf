//! Kodex-Befund 1·G01, Gegenprobe im Compiler: Die Ausnahme scheitert in jedem Produktiv-Target (E0453).
//!
//! Prüfkopie und Hilfen stehen in `befunde_gemeinsam/mod.rs`; Aufbau wie in `befunde.rs`.
//! Eigenes Testprogramm, damit das Gate es parallel startet (C1, #190).

mod befunde_gemeinsam;

use std::fs;
use std::path::Path;
use std::process::Command;

use befunde_gemeinsam::{Baum, workspace};

/// Minimaler JSON-Leser fuer `cargo metadata` (nur was die Targetliste braucht).
mod json {
    #[derive(Debug, PartialEq)]
    pub enum Wert {
        Text(String),
        Liste(Vec<Wert>),
        Objekt(Vec<(String, Wert)>),
        Sonst,
    }

    impl Wert {
        pub fn feld(&self, k: &str) -> Option<&Wert> {
            match self {
                Wert::Objekt(o) => o.iter().find(|(n, _)| n == k).map(|(_, w)| w),
                _ => None,
            }
        }

        pub fn text(&self) -> Option<&str> {
            match self {
                Wert::Text(t) => Some(t),
                _ => None,
            }
        }

        pub fn liste(&self) -> &[Wert] {
            match self {
                Wert::Liste(l) => l,
                _ => &[],
            }
        }
    }

    pub fn lies(s: &str) -> Option<Wert> {
        let z: Vec<char> = s.chars().collect();
        let mut i = 0_usize;
        let w = wert(&z, &mut i)?;
        Some(w)
    }

    fn leer(z: &[char], i: &mut usize) {
        while z.get(*i).is_some_and(|c| c.is_whitespace()) {
            *i = i.saturating_add(1);
        }
    }

    fn text(z: &[char], i: &mut usize) -> Option<String> {
        *i = i.saturating_add(1);
        let mut aus = String::new();
        loop {
            let zeichen = *z.get(*i)?;
            *i = i.saturating_add(1);
            match zeichen {
                '"' => return Some(aus),
                '\\' => {
                    let esc = *z.get(*i)?;
                    *i = i.saturating_add(1);
                    match esc {
                        'n' => aus.push('\n'),
                        't' => aus.push('\t'),
                        'u' => {
                            let hex: String = z.get(*i..i.saturating_add(4))?.iter().collect();
                            *i = i.saturating_add(4);
                            aus.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                        }
                        anders => aus.push(anders),
                    }
                }
                anders => aus.push(anders),
            }
        }
    }

    fn wert(z: &[char], i: &mut usize) -> Option<Wert> {
        leer(z, i);
        match z.get(*i)? {
            '"' => text(z, i).map(Wert::Text),
            '[' => {
                *i = i.saturating_add(1);
                let mut l = Vec::new();
                loop {
                    leer(z, i);
                    if z.get(*i) == Some(&']') {
                        *i = i.saturating_add(1);
                        return Some(Wert::Liste(l));
                    }
                    l.push(wert(z, i)?);
                    leer(z, i);
                    if z.get(*i) == Some(&',') {
                        *i = i.saturating_add(1);
                    }
                }
            }
            '{' => {
                *i = i.saturating_add(1);
                let mut o = Vec::new();
                loop {
                    leer(z, i);
                    if z.get(*i) == Some(&'}') {
                        *i = i.saturating_add(1);
                        return Some(Wert::Objekt(o));
                    }
                    let k = text(z, i)?;
                    leer(z, i);
                    if z.get(*i) != Some(&':') {
                        return None;
                    }
                    *i = i.saturating_add(1);
                    o.push((k, wert(z, i)?));
                    leer(z, i);
                    if z.get(*i) == Some(&',') {
                        *i = i.saturating_add(1);
                    }
                }
            }
            _ => {
                while z
                    .get(*i)
                    .is_some_and(|c| !matches!(c, ',' | ']' | '}') && !c.is_whitespace())
                {
                    *i = i.saturating_add(1);
                }
                Some(Wert::Sonst)
            }
        }
    }
}

/// Ein Produktiv-Target: Paket, Auswahl fuer cargo und Wurzeldatei relativ zum Workspace.
struct Target {
    paket: String,
    auswahl: Vec<String>,
    wurzel: String,
}

/// Alle Produktiv-Targets laut `cargo metadata` (Bibliotheken jeder Art und Programme), nicht
/// fest codiert: ein neues Target ist automatisch dabei.
fn produktiv_targets(ws: &Path) -> Result<Vec<Target>, String> {
    let aus = Command::new(option_env!("CARGO").unwrap_or("cargo"))
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
            "--locked",
        ])
        .current_dir(ws)
        .output()
        .map_err(|e| format!("cargo metadata nicht startbar: {e}"))?;
    if !aus.status.success() {
        return Err(format!(
            "cargo metadata: {}",
            String::from_utf8_lossy(&aus.stderr)
        ));
    }
    let text = String::from_utf8(aus.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
    let meta = json::lies(&text).ok_or("cargo metadata: JSON nicht lesbar")?;
    let basis = fs::canonicalize(ws).map_err(|e| format!("{}: {e}", ws.display()))?;
    let mut aus = Vec::new();
    for paket in meta
        .feld("packages")
        .map(json::Wert::liste)
        .unwrap_or_default()
    {
        let name = paket
            .feld("name")
            .and_then(json::Wert::text)
            .ok_or("Paket ohne Namen")?;
        for ziel in paket
            .feld("targets")
            .map(json::Wert::liste)
            .unwrap_or_default()
        {
            let arten: Vec<&str> = ziel
                .feld("kind")
                .map(json::Wert::liste)
                .unwrap_or_default()
                .iter()
                .filter_map(json::Wert::text)
                .collect();
            if arten
                .iter()
                .any(|a| matches!(*a, "test" | "bench" | "example" | "custom-build"))
            {
                continue;
            }
            let pfad = ziel
                .feld("src_path")
                .and_then(json::Wert::text)
                .ok_or("Target ohne src_path")?;
            // canonicalize kann unter Windows einen Verbatim-Praefix und lange Namen
            // liefern. Beide Seiten auflösen, bevor Workspace und Cargo verglichen werden.
            let voll = fs::canonicalize(pfad).map_err(|e| format!("Target {pfad}: {e}"))?;
            let rel = voll
                .strip_prefix(&basis)
                .map_err(|_| format!("Target {pfad} liegt nicht im Workspace"))?
                .to_string_lossy()
                .replace('\\', "/");
            let tname = ziel
                .feld("name")
                .and_then(json::Wert::text)
                .ok_or("Target ohne Namen")?;
            let auswahl = if arten.contains(&"bin") {
                vec!["--bin".to_string(), tname.to_string()]
            } else {
                vec!["--lib".to_string()]
            };
            aus.push(Target {
                paket: name.to_string(),
                auswahl,
                wurzel: rel,
            });
        }
    }
    aus.sort_by(|a, b| a.wurzel.cmp(&b.wurzel));
    Ok(aus)
}

const VERSTOSS: &str = "
#[cfg_attr(not(test), allow(clippy::unwrap_used, reason = \"Gegenprobe 1·G01\"))]
#[allow(dead_code, reason = \"Gegenprobe 1·G01\")]
/// # Panics
/// Gegenprobe.
fn gegenprobe_g01_a(x: Option<u8>) -> u8 {
    x.unwrap()
}

#[cfg_attr(all(), expect(clippy::arithmetic_side_effects, reason = \"Gegenprobe 1·G01\"))]
#[allow(dead_code, reason = \"Gegenprobe 1·G01\")]
fn gegenprobe_g01_b(x: u8) -> u8 {
    x + 1
}
";

/// Der Compiler selbst lehnt in jedem Produktiv-Target die Ausnahme ab (E0453). Gilt nur, wenn
/// jeder Fehler E0453 ist und beide Lints genannt werden; ein anderer Fehler beweist nichts.
#[test]
fn befund_1_g01_compiler_lehnt_ausnahme_in_jedem_target_ab() -> Result<(), String> {
    let baum = Baum::neu()?;
    let ziel = workspace().join("target/kodex-sonde");
    let targets = produktiv_targets(&baum.pfad)?;
    // Der Kodex muss genau dieselben Wurzeln kennen wie Cargo, sonst prueft er etwas anderes,
    // als gebaut wird.
    let mut kodex_wurzeln = Vec::new();
    for t in &targets {
        let dir = baum.pfad.join("crates").join(&t.paket);
        let m = depot_kodex::manifest::lies(&dir.join("Cargo.toml"))?;
        for w in depot_kodex::manifest::produktiv_wurzeln(&dir, &m)? {
            let voll = format!("crates/{}/{w}", t.paket);
            if !kodex_wurzeln.contains(&voll) {
                kodex_wurzeln.push(voll);
            }
        }
    }
    kodex_wurzeln.sort();
    let cargo_wurzeln: Vec<String> = targets.iter().map(|t| t.wurzel.clone()).collect();
    if cargo_wurzeln != kodex_wurzeln || cargo_wurzeln.len() < 6 {
        return Err(format!(
            "Targets von Cargo und Kodex verschieden: {cargo_wurzeln:?} / {kodex_wurzeln:?}"
        ));
    }
    let mut offen = Vec::new();
    for Target {
        paket,
        auswahl,
        wurzel,
    } in &targets
    {
        baum.haenge_an(wurzel, VERSTOSS)?;
        let aus = Command::new(option_env!("CARGO").unwrap_or("cargo"))
            .args(["clippy", "--quiet", "--offline", "--locked", "-p", paket])
            .args(auswahl)
            .args(["--message-format=short", "--", "-D", "warnings"])
            .current_dir(&baum.pfad)
            .env("CARGO_TARGET_DIR", &ziel)
            .output()
            .map_err(|e| format!("cargo nicht startbar: {e}"))?;
        baum.zuruecksetzen()?;
        let text = String::from_utf8_lossy(&aus.stderr).into_owned();
        let fehler: Vec<&str> = text
            .lines()
            .filter(|z| z.contains("error"))
            .filter(|z| !z.contains("could not compile") && !z.contains("aborting due to"))
            .collect();
        let nur_e0453 = !fehler.is_empty() && fehler.iter().all(|z| z.contains("E0453"));
        let nennt = text.contains("allow(clippy::unwrap_used) incompatible with previous forbid")
            && text.contains(
                "expect(clippy::arithmetic_side_effects) incompatible with previous forbid",
            );
        if aus.status.success() || !nur_e0453 || !nennt {
            offen.push(format!("{paket} {} ({wurzel}):\n{text}", auswahl.join(" ")));
        }
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Ausnahme nicht vom Compiler abgelehnt:\n{}",
            offen.join("\n")
        ))
    }
}
