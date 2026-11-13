//! Prüfung eines Kandidaten aus Git-Objekten, ohne Ausführung seines Codes.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

use crate::{
    DEKLARATION, FESTSCHREIBUNG, Rolle, deklaration, festschreibung, freigabe, git, pfade,
};

fn commit(sha: &str) -> bool {
    matches!(sha.len(), 40 | 64) && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn pruefe(wurzel: &Path, basis: &str, kandidat: &str) -> Result<(), String> {
    if !commit(basis) || !commit(kandidat) {
        return Err("Basis und Kandidat müssen volle Commit-SHAs sein".into());
    }
    let vergleich = format!("{basis}...{kandidat}");
    let namen = git::ausgabe(
        wurzel,
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-renames",
            &vergleich,
            "--",
        ],
    )?;
    let pfade: BTreeSet<_> = git::pfadliste(&namen)?.into_iter().collect();
    if pfade.is_empty() || pfade.iter().all(|p| p == DEKLARATION) {
        return Ok(());
    }
    let datei = |pfad: &str| git::ausgabe(wurzel, &["show", &format!("{kandidat}:{pfad}")]);
    let text = String::from_utf8(datei(DEKLARATION)?).map_err(|e| e.to_string())?;
    let dekl = deklaration::lies(&text)?;
    let mut fehler = deklaration::vergleiche(&dekl, &pfade);
    fehler.extend(pfade::pruefe_rollen(dekl.klasse, &pfade, true));
    let braucht =
        pfade.contains(FESTSCHREIBUNG) || pfade.iter().any(|p| pfade::rolle(p) == Rolle::Kodex);
    if braucht {
        if !pfade.contains(freigabe::FREIGABE) {
            return Err("Kodex geändert ohne Grenzfreigabe".into());
        }
        let freigabe = String::from_utf8(datei(freigabe::FREIGABE)?).map_err(|e| e.to_string())?;
        let fest =
            festschreibung::sha_bytes(&festschreibung::normalisiert(&datei(FESTSCHREIBUNG)?));
        let mut inhalt = String::new();
        for p in pfade.iter().filter(|p| p.as_str() != freigabe::FREIGABE) {
            let hash = if let Ok(bytes) = datei(p) {
                festschreibung::sha_bytes(&festschreibung::normalisiert(&bytes))
            } else {
                "geloescht".into()
            };
            let _ = writeln!(inhalt, "{p}\t{hash}");
        }
        fehler.extend(freigabe::pruefe_werte(
            &freigabe,
            &fest,
            &festschreibung::sha_bytes(inhalt.as_bytes()),
        ));
    } else if pfade.contains(freigabe::FREIGABE) {
        fehler.push("Grenzfreigabe ohne Änderung der Festschreibung".into());
    }
    if fehler.is_empty() {
        Ok(())
    } else {
        Err(fehler.join("\n"))
    }
}
