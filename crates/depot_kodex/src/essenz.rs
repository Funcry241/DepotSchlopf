//! Kaltstart-Readback der Essenz. Prueft nur, dass die beratende Schicht vollstaendig und
//! aufloesbar bleibt. Ob eine Essenz-Regel fachlich stimmt, entscheidet nicht dieser Test.

use std::fs;
use std::path::Path;

pub const ESSENZ: &str = "docs/essenz.md";
pub const KALTSTART: &str = "docs/kaltstart.md";

const ABSCHNITTE: [&str; 6] = [
    "**Aussage**",
    "**Warum**",
    "**Typische Fehlform**",
    "**Lösungsraum**",
    "**Vor der Änderung prüfen**",
    "**Harter Bezug**",
];

/// Regeln als (Nummer, Titel, Text).
pub fn regeln(text: &str) -> Vec<(u32, String, String)> {
    let mut aus: Vec<(u32, String, String)> = Vec::new();
    for zeile in text.lines() {
        if let Some(rest) = zeile.strip_prefix("# D")
            && let Some((nr, titel)) = rest.split_once(" – ")
            && let Ok(n) = nr.parse::<u32>()
        {
            aus.push((n, titel.trim().to_string(), String::new()));
            continue;
        }
        if let Some(letzte) = aus.last_mut() {
            letzte.2.push_str(zeile);
            letzte.2.push('\n');
        }
    }
    aus
}

pub fn pruefe(wurzel: &Path) -> Vec<String> {
    let mut fehler = Vec::new();
    let Ok(essenz) = fs::read_to_string(wurzel.join(ESSENZ)) else {
        return vec![format!("{ESSENZ} fehlt")];
    };
    if !essenz.contains("advisory only") {
        fehler.push(format!("{ESSENZ}: Autoritaetsgrenze (advisory only) fehlt"));
    }
    let r = regeln(&essenz);
    if r.is_empty() {
        fehler.push(format!("{ESSENZ}: keine Regeln D00 ..."));
    }
    for (erwartet, (nr, titel, text)) in (0_u32..).zip(&r) {
        if *nr != erwartet {
            fehler.push(format!("{ESSENZ}: D{nr:02} statt D{erwartet:02}"));
        }
        for a in ABSCHNITTE {
            if !text.contains(a) {
                fehler.push(format!("{ESSENZ}: D{nr:02} {titel}: {a} fehlt"));
            }
        }
    }
    for (i, teil) in essenz.split('`').enumerate() {
        let ist_code = i % 2 == 1;
        let ist_pfad = ["crates/", "kodex/", "docs/"]
            .iter()
            .any(|p| teil.starts_with(p))
            && !teil.contains(' ');
        if ist_code && ist_pfad && !wurzel.join(teil).exists() {
            fehler.push(format!("{ESSENZ}: Verweis {teil} existiert nicht"));
        }
    }
    match fs::read_to_string(wurzel.join(KALTSTART)) {
        Ok(k) => {
            for muss in [ESSENZ, crate::DEKLARATION, crate::FESTSCHREIBUNG] {
                if !k.contains(muss) {
                    fehler.push(format!("{KALTSTART}: Verweis auf {muss} fehlt"));
                }
            }
        }
        Err(_) => fehler.push(format!("{KALTSTART} fehlt")),
    }
    fehler
}
