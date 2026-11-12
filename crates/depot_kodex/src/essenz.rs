//! Kaltstart-Readback der Essenz. Prueft nur, dass die beratende Schicht vollstaendig und
//! aufloesbar bleibt. Ob eine Essenz-Regel fachlich stimmt, entscheidet nicht dieser Test.

use std::fs;
use std::path::Path;

pub const ESSENZ: &str = "docs/essenz.md";
pub const KALTSTART: &str = "docs/kaltstart.md";
/// Festes Register: eine Zeile `Dnn – Titel` je Regel. Aendern nur mit KodexAenderung.
pub const REGISTER: &str = "kodex/essenz_register.txt";

/// Register als (Nummer, Titel).
///
/// # Errors
/// Zeilen, die nicht `Dnn – Titel` sind.
pub fn register(text: &str) -> Result<Vec<(u32, String)>, String> {
    text.lines()
        .map(str::trim)
        .filter(|z| !z.is_empty() && !z.starts_with('#'))
        .map(|z| {
            z.strip_prefix('D')
                .and_then(|r| r.split_once(" – "))
                .and_then(|(nr, titel)| Some((nr.parse::<u32>().ok()?, titel.trim().to_string())))
                .ok_or_else(|| format!("{REGISTER}: Zeile {z:?} ist nicht `Dnn – Titel`"))
        })
        .collect()
}

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
        fehler.push(format!("{ESSENZ}: Autoritätsgrenze (advisory only) fehlt"));
    }
    let r = regeln(&essenz);
    let reg = match fs::read_to_string(wurzel.join(REGISTER)) {
        Ok(t) => match register(&t) {
            Ok(r) if !r.is_empty() => r,
            Ok(_) => return vec![format!("{REGISTER}: leer")],
            Err(e) => return vec![e],
        },
        Err(e) => return vec![format!("{REGISTER}: nicht lesbar: {e}")],
    };
    for (nr, titel) in &reg {
        match r.iter().find(|(n, _, _)| n == nr) {
            None => fehler.push(format!(
                "{ESSENZ}: D{nr:02} {titel} fehlt (Register {REGISTER})"
            )),
            Some((_, t, _)) if t != titel => fehler.push(format!(
                "{ESSENZ}: D{nr:02} heisst {t:?}, im Register {titel:?}"
            )),
            Some(_) => {}
        }
    }
    for (nr, titel, _) in &r {
        if !reg.iter().any(|(n, _)| n == nr) {
            fehler.push(format!(
                "{ESSENZ}: D{nr:02} {titel} steht nicht im Register {REGISTER}"
            ));
        }
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
            if let (Some((erste, _)), Some((letzte, _))) = (reg.first(), reg.last()) {
                let bereich = format!("D{erste:02} bis D{letzte:02}");
                if !k.contains(&bereich) {
                    fehler.push(format!(
                        "{KALTSTART}: nennt nicht den Bereich {bereich} aus dem Register"
                    ));
                }
            }
        }
        Err(_) => fehler.push(format!("{KALTSTART} fehlt")),
    }
    fehler
}
