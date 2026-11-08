//! Die Deklaration einer Aenderung und ihr Abgleich mit dem realen Diff.

use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Klasse {
    /// Nur erlaubt, solange die Basis noch keinen Kodex hat.
    Gruendung,
    DokuOnly,
    TestOnly,
    /// Code umbauen ohne Verhaltensaenderung; Festschreibung bleibt gleich.
    RefactorOnly,
    /// Neue oder geaenderte Funktion; Festschreibung bleibt gleich.
    Funktion,
    /// Das Gate selbst aendern; nie zusammen mit Produktivcode.
    KodexAenderung,
    /// Grenzen, KI-Budget, Produktarten, Abhaengigkeiten oder Netzzugang aendern.
    /// Braucht die ausdrueckliche Freigabe des Eigentuemers beim Zusammenfuehren.
    FestschreibungAenderung,
}

impl Klasse {
    pub fn lies(text: &str) -> Option<Klasse> {
        Some(match text {
            "Gruendung" => Klasse::Gruendung,
            "DokuOnly" => Klasse::DokuOnly,
            "TestOnly" => Klasse::TestOnly,
            "RefactorOnly" => Klasse::RefactorOnly,
            "Funktion" => Klasse::Funktion,
            "KodexAenderung" => Klasse::KodexAenderung,
            "FestschreibungAenderung" => Klasse::FestschreibungAenderung,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deklaration {
    pub klasse: Klasse,
    pub zweck: String,
    /// Exakte Pfade oder Verzeichnisse mit abschliessendem `/`.
    pub pfade: Vec<String>,
}

/// Format:
/// ```text
/// klasse: Funktion
/// zweck: Was und warum, in einem Satz
/// pfade:
/// crates/depot_ki/src/
/// docs/essenz.md
/// ```
///
/// # Errors
/// Fehlende oder unbekannte Felder.
pub fn lies(text: &str) -> Result<Deklaration, String> {
    let mut klasse = None;
    let mut zweck = None;
    let mut pfade = Vec::new();
    let mut in_pfaden = false;
    for zeile in text.lines().map(str::trim) {
        if zeile.is_empty() || zeile.starts_with('#') {
            continue;
        }
        if in_pfaden {
            pfade.push(zeile.to_string());
        } else if zeile == "pfade:" {
            in_pfaden = true;
        } else if let Some(wert) = zeile.strip_prefix("klasse:") {
            klasse = Some(
                Klasse::lies(wert.trim())
                    .ok_or_else(|| format!("unbekannte Klasse {:?}", wert.trim()))?,
            );
        } else if let Some(wert) = zeile.strip_prefix("zweck:") {
            zweck = Some(wert.trim().to_string());
        } else {
            return Err(format!("unbekannte Zeile {zeile:?}"));
        }
    }
    let klasse = klasse.ok_or("klasse fehlt")?;
    let zweck = zweck.filter(|z| !z.is_empty()).ok_or("zweck fehlt")?;
    if pfade.is_empty() {
        return Err("keine pfade".into());
    }
    Ok(Deklaration {
        klasse,
        zweck,
        pfade,
    })
}

fn deckt(eintrag: &str, pfad: &str) -> bool {
    if eintrag.ends_with('/') {
        pfad.starts_with(eintrag)
    } else {
        pfad == eintrag
    }
}

/// Undeklariert = 0 und deklariert-aber-unveraendert = 0.
pub fn vergleiche(deklaration: &Deklaration, diff: &BTreeSet<String>) -> Vec<String> {
    let mut fehler = Vec::new();
    if !diff.contains(crate::DEKLARATION) {
        fehler.push(format!(
            "{} wurde nicht erneuert: jede Änderung braucht ihre eigene Deklaration",
            crate::DEKLARATION
        ));
    }
    for pfad in diff {
        if pfad != crate::DEKLARATION && !deklaration.pfade.iter().any(|e| deckt(e, pfad)) {
            fehler.push(format!("undeklarierte Änderung: {pfad}"));
        }
    }
    for eintrag in &deklaration.pfade {
        if !diff.iter().any(|p| deckt(eintrag, p)) {
            fehler.push(format!("deklariert, aber unveraendert: {eintrag}"));
        }
    }
    fehler
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menge(p: &[&str]) -> BTreeSet<String> {
        p.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn lesen_und_vergleichen() {
        let d = lies("klasse: Funktion\nzweck: Test\npfade:\ncrates/depot_ki/src/\n");
        assert!(d.is_ok());
        let Ok(d) = d else { return };
        assert_eq!(d.klasse, Klasse::Funktion);
        let gut = menge(&["kodex/aenderung.txt", "crates/depot_ki/src/budget.rs"]);
        assert!(vergleiche(&d, &gut).is_empty());
        let heimlich = menge(&[
            "kodex/aenderung.txt",
            "crates/depot_ki/src/budget.rs",
            "crates/depot_kern/src/grenzen.rs",
        ]);
        assert_eq!(vergleiche(&d, &heimlich).len(), 1);
        let leer = menge(&["kodex/aenderung.txt"]);
        assert_eq!(vergleiche(&d, &leer).len(), 1);
    }

    #[test]
    fn unvollstaendige_deklaration() {
        assert!(lies("klasse: Funktion\npfade:\nx\n").is_err());
        assert!(lies("klasse: Alles\nzweck: x\npfade:\nx\n").is_err());
        assert!(lies("klasse: Funktion\nzweck: x\n").is_err());
    }
}
