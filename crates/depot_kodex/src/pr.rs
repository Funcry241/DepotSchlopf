//! Prüfung eines Kandidaten aus Git-Objekten, ohne Ausführung seines Codes.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

use crate::{
    DEKLARATION, FESTSCHREIBUNG, Rolle, deklaration, festschreibung, freigabe, git, pfade,
    pruefdateien,
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
    let merge_basis = git::ausgabe(wurzel, &["merge-base", basis, kandidat])?;
    let merge_basis = String::from_utf8(merge_basis).map_err(|e| e.to_string())?;
    let basis_pfade = git::baum_pfade(wurzel, merge_basis.trim())?;
    let dateien = pruefdateien::waehle(&pfade, &basis_pfade)?;
    let datei = |pfad: &str| git::ausgabe(wurzel, &["show", &format!("{kandidat}:{pfad}")]);
    let text = String::from_utf8(datei(dateien.deklaration)?).map_err(|e| e.to_string())?;
    let dekl = deklaration::lies(&text)?;
    let mut fehler = deklaration::vergleiche_mit_pfad(&dekl, &pfade, dateien.deklaration);
    fehler.extend(pfade::pruefe_rollen(dekl.klasse, &pfade, true));
    let braucht =
        pfade.contains(FESTSCHREIBUNG) || pfade.iter().any(|p| pfade::rolle(p) == Rolle::Kodex);
    if braucht {
        let Some(freigabepfad) = dateien.freigabe else {
            return Err("Kodex geändert ohne Grenzfreigabe".into());
        };
        let freigabe = String::from_utf8(datei(freigabepfad)?).map_err(|e| e.to_string())?;
        let fest =
            festschreibung::sha_bytes(&festschreibung::normalisiert(&datei(FESTSCHREIBUNG)?));
        let mut inhalt = String::new();
        for p in pfade.iter().filter(|p| p.as_str() != freigabepfad) {
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
    } else if dateien.freigabe.is_some() {
        fehler.push("Grenzfreigabe ohne Änderung der Festschreibung".into());
    }
    if fehler.is_empty() {
        Ok(())
    } else {
        Err(fehler.join("\n"))
    }
}
