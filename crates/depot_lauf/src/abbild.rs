//! Der gepruefte Stand (R28, 1·R04).
//!
//! Das Gate prueft den Arbeitsbaum, der Export nimmt den Git-Index. Byte-gleich sind beide nur,
//! wenn der Arbeitsbaum dem Index gleicht und sich waehrend der Pruefung nichts aendert. Deshalb
//! haelt der Lauf vor und nach der Pruefung ein Abbild fest: jede gewoehnliche Datei des Index
//! mit ihrem Blob und jede Abweichung des Arbeitsbaums. Der Quellstand des Exports kommt aus
//! den Blobs des Abbilds vor der Pruefung; ein Blob aendert sich nie. Weicht etwas ab, ist der
//! Lauf gescheitert, und der Export nennt den Quellstand ungeprueft. Der Export entsteht
//! trotzdem, damit der Fehler sichtbar wird.
//!
//! **Grenze:** Eine Aenderung, die waehrend der Pruefung entsteht und vor ihrem Ende genau
//! zurueckgenommen wird, sieht kein Vergleich zweier Abbilder.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use crate::export::{Lauf, Schritt, geheim_oder_erzeugt, hat_git};

/// Der Index zu einem Zeitpunkt und wo der Arbeitsbaum davon abweicht.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Abbild {
    /// Die gewoehnlichen Dateien im Index mit ihren Blobs, nach Pfad sortiert.
    pub dateien: Vec<(String, String)>,
    /// Wo der Arbeitsbaum vom Index abweicht: geaenderte, geloeschte und neue, nicht
    /// ignorierte Dateien, und Pfade mit mehreren Eintraegen (offener Konflikt).
    pub abweichungen: Vec<String>,
}

/// NUL-getrennte Pfade aus einem Git-Aufruf. Jeder Fehler ist ein Fehler.
fn pfade(wurzel: &Path, args: &[&str]) -> Result<Vec<String>, String> {
    let aus = Command::new("git")
        .arg("-C")
        .arg(wurzel)
        .args(args)
        .output()
        .map_err(|e| format!("git nicht startbar: {e}"))?;
    if !aus.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&aus.stderr).trim()
        ));
    }
    aus.stdout
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| {
            String::from_utf8(p.to_vec())
                .map_err(|_| format!("Pfad ist kein UTF-8: {}", String::from_utf8_lossy(p)))
        })
        .collect()
}

/// Haelt den Index und die Abweichungen des Arbeitsbaums fest. Ohne Git `None`.
///
/// # Errors
/// Git fehlt, oder Index oder Arbeitsbaum sind nicht lesbar.
pub fn aufnehmen(wurzel: &Path) -> Result<Option<Abbild>, String> {
    if !hat_git(wurzel)? {
        return Ok(None);
    }
    let mut abweichungen = Vec::new();
    let mut dateien: BTreeMap<String, String> = BTreeMap::new();
    for d in depot_kodex::git::index_dateien(wurzel)
        .map_err(|e| format!("Git-Index nicht lesbar: {e}"))?
    {
        if geheim_oder_erzeugt(&d.pfad) {
            continue;
        }
        if dateien.insert(d.pfad.clone(), d.blob).is_some() {
            abweichungen.push(format!("{}: mehrere Eintraege im Index (Konflikt)", d.pfad));
        }
    }
    for p in pfade(wurzel, &["diff", "-z", "--no-renames", "--name-only"])? {
        if !geheim_oder_erzeugt(&p) {
            abweichungen.push(format!("{p}: im Arbeitsbaum anders als im Index"));
        }
    }
    for p in pfade(
        wurzel,
        &["ls-files", "-z", "--others", "--exclude-standard"],
    )? {
        if !geheim_oder_erzeugt(&p) {
            abweichungen.push(format!("{p}: nicht verfolgt"));
        }
    }
    abweichungen.sort();
    abweichungen.dedup();
    Ok(Some(Abbild {
        dateien: dateien.into_iter().collect(),
        abweichungen,
    }))
}

/// Was den geprueften Stand vom Export trennt; leer, wenn beide byte-gleich sind.
fn vergleiche(vorher: &Abbild, nachher: &Abbild) -> Vec<String> {
    let mut probleme: Vec<String> = vorher
        .abweichungen
        .iter()
        .map(|a| format!("vor der Pruefung: {a}"))
        .chain(
            nachher
                .abweichungen
                .iter()
                .map(|a| format!("nach der Pruefung: {a}")),
        )
        .collect();
    let alt: BTreeMap<&String, &String> = vorher.dateien.iter().map(|(p, b)| (p, b)).collect();
    let neu: BTreeMap<&String, &String> = nachher.dateien.iter().map(|(p, b)| (p, b)).collect();
    for p in alt.keys().chain(neu.keys()) {
        if alt.get(p) != neu.get(p) {
            probleme.push(format!("{p}: Index waehrend der Pruefung geaendert"));
        }
    }
    probleme.dedup();
    probleme
}

/// Fuehrt die Pruefung aus und haelt den Stand davor und danach fest. Der Schritt `stand`
/// scheitert, wenn der Export nicht byte-gleich mit dem geprueften Stand waere.
pub fn gepruefter_lauf(
    wurzel: &Path,
    befehl: String,
    mut konsole: String,
    mut warnungen: Vec<String>,
    pruefung: impl FnOnce(&mut String) -> i32,
) -> Lauf {
    let vorher = aufnehmen(wurzel);
    let exit = pruefung(&mut konsole);
    let nachher = aufnehmen(wurzel);
    let probleme = match (&vorher, &nachher) {
        (Ok(Some(v)), Ok(Some(n))) => vergleiche(v, n),
        (Ok(None), Ok(None)) => Vec::new(),
        (Err(e), _) | (_, Err(e)) => vec![format!("Stand nicht lesbar: {e}")],
        _ => vec!["Git ist waehrend der Pruefung erschienen oder verschwunden".into()],
    };
    for p in &probleme {
        let z = format!("[DepotSchlopf] Quellstand ungeprueft: {p}\n");
        eprint!("{z}");
        konsole.push_str(&z);
        warnungen.push(format!("Quellstand ungeprueft: {p}"));
    }
    Lauf {
        befehl,
        schritte: vec![
            Schritt {
                name: "kodex".into(),
                exit,
            },
            Schritt {
                name: "stand".into(),
                exit: i32::from(!probleme.is_empty()),
            },
        ],
        konsole,
        warnungen,
        abbild: vorher.ok().flatten(),
    }
}
