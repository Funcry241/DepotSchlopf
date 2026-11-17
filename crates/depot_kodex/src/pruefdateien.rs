//! Wählt genau die Prüfdateien des vorliegenden Diffs aus.

use std::collections::BTreeSet;

use crate::{DEKLARATION, freigabe::FREIGABE};

const DEKLARATIONEN: &str = "kodex/aenderungen/";
const FREIGABEN: &str = "kodex/freigaben/";

pub(crate) fn ist_deklaration(pfad: &str) -> bool {
    ist_datei(pfad, DEKLARATIONEN)
}

pub(crate) fn ist_freigabe(pfad: &str) -> bool {
    ist_datei(pfad, FREIGABEN)
}

fn ist_datei(pfad: &str, ordner: &str) -> bool {
    pfad.strip_prefix(ordner)
        .and_then(|name| name.strip_suffix(".txt"))
        .is_some_and(|name| !name.is_empty() && !name.contains('/'))
}

pub(crate) struct Pruefdateien<'a> {
    pub deklaration: &'a str,
    pub freigabe: Option<&'a str>,
}

pub(crate) fn waehle<'a>(
    diff: &'a BTreeSet<String>,
    basis: &BTreeSet<String>,
) -> Result<Pruefdateien<'a>, String> {
    let deklaration = neu(diff, basis, DEKLARATIONEN, ist_deklaration)?;
    let freigabe = neu(diff, basis, FREIGABEN, ist_freigabe)?;
    if diff.contains(DEKLARATION) {
        if deklaration.is_some() || freigabe.is_some() {
            return Err("Alte und neue Prüfdateien dürfen nicht gemischt werden".into());
        }
        return Ok(Pruefdateien {
            deklaration: DEKLARATION,
            freigabe: diff.contains(FREIGABE).then_some(FREIGABE),
        });
    }
    if diff.contains(FREIGABE) {
        return Err("Die alte Grenzfreigabe braucht die alte Deklaration".into());
    }

    Ok(Pruefdateien {
        deklaration: deklaration.ok_or("Jede Änderung braucht eine eigene Deklaration")?,
        freigabe,
    })
}

fn neu<'a>(
    diff: &'a BTreeSet<String>,
    basis: &BTreeSet<String>,
    ordner: &str,
    ist_datei: fn(&str) -> bool,
) -> Result<Option<&'a str>, String> {
    let mut gefunden = None;
    for pfad in diff.iter().filter(|pfad| pfad.starts_with(ordner)) {
        if !ist_datei(pfad) {
            return Err(format!(
                "{pfad}: erwartet wird eine einzelne Textdatei in {ordner}"
            ));
        }
        if basis.contains(pfad) {
            return Err(format!(
                "{pfad}: Prüfdatei aus der Basis geändert oder gelöscht"
            ));
        }
        if gefunden.replace(pfad.as_str()).is_some() {
            return Err(format!("Mehr als eine neue Prüfdatei in {ordner}"));
        }
    }
    Ok(gefunden)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "kodex/aenderungen/a.txt";
    const B: &str = "kodex/aenderungen/b.txt";
    const F: &str = "kodex/freigaben/a.txt";

    fn menge(pfade: &[&str]) -> BTreeSet<String> {
        pfade.iter().map(|pfad| (*pfad).to_owned()).collect()
    }

    #[test]
    fn zwei_deklarationen_halt() {
        assert!(waehle(&menge(&[A, B]), &menge(&[])).is_err());
    }

    #[test]
    fn alte_deklaration_geaendert_halt() {
        assert!(waehle(&menge(&[A]), &menge(&[A])).is_err());
        assert!(waehle(&menge(&[B, F]), &menge(&[F])).is_err());
    }

    #[test]
    fn alter_und_neuer_weg_halt() {
        assert!(waehle(&menge(&[DEKLARATION, A]), &menge(&[])).is_err());
        assert!(waehle(&menge(&[A, FREIGABE]), &menge(&[])).is_err());
    }

    #[test]
    fn dateien_liegen_je_diff_getrennt() -> Result<(), String> {
        let diff = menge(&[B, F]);
        let dateien = waehle(&diff, &menge(&[A]))?;
        assert_eq!(dateien.deklaration, B);
        assert_eq!(dateien.freigabe, Some(F));
        Ok(())
    }
}
