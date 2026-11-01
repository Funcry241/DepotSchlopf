//! Grenzfreigabe (R30, Befund 1·R08): Jede Aenderung der Festschreibung braucht eine Freigabe,
//! die an genau diese Aenderung gebunden ist.
//!
//! `kodex/freigabe.txt`:
//!
//! ```text
//! typ: Grenzfreigabe
//! diff: <sha256 ueber die geaenderten Pfade und ihren neuen Inhalt>
//! festschreibung: <sha256 der neuen kodex/festschreibung.txt>
//! erteilt: Wolfgang
//! ```
//!
//! Der Diff-Hash entsteht aus je einer Zeile `pfad TAB sha256(inhalt)` (oder `pfad TAB
//! geloescht`) fuer jeden geaenderten Pfad ausser der Freigabe selbst, nach Pfad sortiert;
//! Windows-Zeilenenden zaehlen nicht. Aendert sich danach noch etwas, erlischt die Freigabe.
//! Eine Freigabe wird nie geerbt: Sie muss mit der Aenderung neu im Diff stehen.
//!
//! Grenze: Der Kodex prueft die Bindung, nicht die Urheberschaft. Dass Wolfgang die Freigabe
//! erteilt hat, sichern die Regeln auf GitHub (nur Wolfgang fuehrt zusammen). Eine
//! maschinenpruefbare Unterschrift waere ein eigener spaeterer Schritt.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use crate::festschreibung::{normalisiert, sha_bytes};

pub const FREIGABE: &str = "kodex/freigabe.txt";
const FELDER: [&str; 4] = ["typ", "diff", "festschreibung", "erteilt"];

/// Diff-Hash ueber die geaenderten Pfade ohne die Freigabe.
pub fn diff_hash(wurzel: &Path, diff: &BTreeSet<String>) -> String {
    let mut text = String::new();
    for p in diff.iter().filter(|p| p.as_str() != FREIGABE) {
        match fs::read(wurzel.join(p)) {
            Ok(inhalt) => {
                let _ = writeln!(text, "{p}\t{}", sha_bytes(&normalisiert(&inhalt)));
            }
            Err(_) => {
                let _ = writeln!(text, "{p}\tgeloescht");
            }
        }
    }
    sha_bytes(text.as_bytes())
}

fn lies(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut f = BTreeMap::new();
    for zeile in text.lines().map(str::trim).filter(|z| !z.is_empty()) {
        let Some((k, v)) = zeile.split_once(':') else {
            return Err(format!("Zeile ohne Feld: {zeile:?}"));
        };
        let k = k.trim();
        if !FELDER.contains(&k) {
            return Err(format!("unbekanntes Feld {k:?}"));
        }
        if f.insert(k.to_string(), v.trim().to_string()).is_some() {
            return Err(format!("Feld {k} doppelt"));
        }
    }
    for k in FELDER {
        if !f.contains_key(k) {
            return Err(format!("Feld {k} fehlt"));
        }
    }
    Ok(f)
}

pub fn pruefe(wurzel: &Path, diff: &BTreeSet<String>) -> Vec<String> {
    let festschreibung = diff.contains(crate::FESTSCHREIBUNG);
    let freigabe = diff.contains(FREIGABE);
    if !festschreibung {
        return if freigabe {
            vec![format!(
                "Grenzfreigabe {FREIGABE} geaendert, aber die Festschreibung nicht: eine Freigabe gilt nur fuer ihre eigene Aenderung"
            )]
        } else {
            Vec::new()
        };
    }
    if !freigabe {
        return vec![format!(
            "Grenzfreigabe fehlt: {} geaendert ohne neue {FREIGABE} (R30)",
            crate::FESTSCHREIBUNG
        )];
    }
    let text = match fs::read(wurzel.join(FREIGABE)) {
        Ok(t) => String::from_utf8_lossy(&t).into_owned(),
        Err(e) => return vec![format!("Grenzfreigabe {FREIGABE} nicht lesbar: {e}")],
    };
    let f = match lies(&text) {
        Ok(f) => f,
        Err(e) => return vec![format!("Grenzfreigabe {FREIGABE}: {e}")],
    };
    let mut fehler = Vec::new();
    let feld = |k: &str| f.get(k).map_or("", String::as_str);
    if feld("typ") != "Grenzfreigabe" {
        fehler.push(format!("Grenzfreigabe: typ ist {:?}", feld("typ")));
    }
    if feld("erteilt") != "Wolfgang" {
        fehler.push(format!("Grenzfreigabe: erteilt ist {:?}", feld("erteilt")));
    }
    let neu = match fs::read(wurzel.join(crate::FESTSCHREIBUNG)) {
        Ok(t) => sha_bytes(&normalisiert(&t)),
        Err(e) => return vec![format!("Grenzfreigabe: Festschreibung nicht lesbar: {e}")],
    };
    if feld("festschreibung") != neu {
        fehler.push(format!(
            "Grenzfreigabe passt nicht zur Festschreibung: freigegeben {}, im Baum {neu}",
            feld("festschreibung")
        ));
    }
    let d = diff_hash(wurzel, diff);
    if feld("diff") != d {
        fehler.push(format!(
            "Grenzfreigabe passt nicht zum Diff: freigegeben {}, im Baum {d}. \
             Nach der Freigabe wurde etwas geaendert",
            feld("diff")
        ));
    }
    fehler
}

/// Text einer passenden Freigabe fuer den aktuellen Stand (Vorlage fuer Wolfgang).
pub fn vorlage(wurzel: &Path, diff: &BTreeSet<String>) -> Result<String, String> {
    let neu = fs::read(wurzel.join(crate::FESTSCHREIBUNG))
        .map_err(|e| format!("Festschreibung nicht lesbar: {e}"))?;
    Ok(format!(
        "typ: Grenzfreigabe\ndiff: {}\nfestschreibung: {}\nerteilt: Wolfgang\n",
        diff_hash(wurzel, diff),
        sha_bytes(&normalisiert(&neu))
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn felder_streng() {
        assert!(
            lies("typ: Grenzfreigabe\ndiff: a\nfestschreibung: b\nerteilt: Wolfgang\n").is_ok()
        );
        assert!(lies("typ: Grenzfreigabe\ndiff: a\nfestschreibung: b\n").is_err());
        assert!(
            lies("typ: Grenzfreigabe\ndiff: a\ndiff: a\nfestschreibung: b\nerteilt: W\n").is_err()
        );
        assert!(lies("typ: x\ndiff: a\nfestschreibung: b\nerteilt: W\nnachsatz: y\n").is_err());
    }
}
