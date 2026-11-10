//! Typische ASCII-Umschreibungen in sichtbaren Texten melden, ohne Maschinenformate umzuschreiben.
use depot_kodex::lexer::zeichenketten;
use std::{
    fs,
    path::{Path, PathBuf},
};

const STAMM: &[&str] = &[
    "ueber",
    "pruef",
    "schluessel",
    "zurueck",
    "unguelt",
    "laeuft",
    "moeglich",
    "naechst",
    "bestaetig",
    "gepruef",
    "ungepruef",
    "verfueg",
    "auftragszustaend",
    "gespraech",
    "hoechststand",
    "staend",
    "unveraendert",
    "vorwaert",
    "vorrueck",
    "waehrend",
];
const HISTORISCH: &[(&str, &str)] = &[
    (
        "depot_journal/src/stand.rs",
        "Notbremse aktiv: nur offene Verkaeufe weiter abgleichen",
    ),
    (
        "depot_journal/src/stand.rs",
        "Notbremse: 30 % Gesamtverlust erreicht. Neue Kaeufe gesperrt; Freigabe des Eigentuemers noetig",
    ),
    (
        "depot_sim/src/szenario.rs",
        "Alpha Werke kuendigt Quartalszahlen fuer den 26.11. an",
    ),
    (
        "depot_sim/src/szenario.rs",
        "Notenbank kuendigt Zinsentscheid fuer den 27.11. an",
    ),
    (
        "depot_sim/src/szenario.rs",
        "Alpha Werke: Umsatz ueber den Erwartungen, Kurs +7,8 %",
    ),
    (
        "depot_sim/src/szenario.rs",
        "Notenbank laesst den Leitzins unveraendert",
    ),
    (
        "depot_sim/src/szenario.rs",
        "Gamma Netz kuendigt Hauptversammlung fuer den 10.12. an",
    ),
];
fn fund(text: &str) -> Option<String> {
    if text == "Schluessel(***)" {
        return Some(text.into());
    }
    let text = text
        .replace("\\n", " ")
        .replace("\\r", " ")
        .replace("\\t", " ");
    if !text.trim().chars().any(char::is_whitespace) {
        return None;
    }
    for teil in text.split_whitespace() {
        // Feldnamen, URL-/Dateipfade, Formatparameter und zitierte Schlüssel bleiben ASCII.
        if teil.contains(['_', '/', '#', '=', '{', '}', '\\', '"', ';']) {
            continue;
        }
        let wort = teil
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_ascii_lowercase();
        if wort.contains('.') {
            continue;
        }
        if wort == "fuer" || STAMM.iter().any(|s| wort.starts_with(s)) {
            return Some(wort);
        }
    }
    None
}
fn dateien(p: &Path, aus: &mut Vec<PathBuf>) -> Result<(), String> {
    for eintrag in fs::read_dir(p).map_err(|e| e.to_string())? {
        let eintrag = eintrag.map_err(|e| e.to_string())?;
        let typ = eintrag.file_type().map_err(|e| e.to_string())?;
        if typ.is_dir() {
            dateien(&eintrag.path(), aus)?;
        } else if typ.is_file() && eintrag.path().extension().is_some_and(|e| e == "rs") {
            aus.push(eintrag.path());
        }
    }
    Ok(())
}
#[test]
fn sichtbare_quelltexte_verwenden_umlaute() -> Result<(), String> {
    let crates = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut pfade = Vec::new();
    for krate in fs::read_dir(&crates).map_err(|e| e.to_string())? {
        let quelle = krate.map_err(|e| e.to_string())?.path().join("src");
        if quelle.is_dir() {
            dateien(&quelle, &mut pfade)?;
        }
    }
    let mut fehler = Vec::new();
    for pfad in pfade {
        let name = pfad
            .strip_prefix(&crates)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let text = fs::read_to_string(&pfad).map_err(|e| e.to_string())?;
        for literal in zeichenketten(&text)? {
            if HISTORISCH.iter().any(|(p, t)| name == *p && literal == *t) {
                continue;
            }
            if let Some(wort) = fund(&literal) {
                fehler.push(format!("{name}: {wort}: {literal}"));
            }
        }
    }
    assert!(fehler.is_empty(), "{}", fehler.join("\n"));
    Ok(())
}
#[test]
fn sprachregel_erkennt_meldungen_aber_keine_bezeichner_oder_kommentare() -> Result<(), String> {
    assert!(fund("Pruefung fuer den Handelstag").is_some());
    assert!(fund("Schluessel hinterlegt").is_some());
    for text in [
        "Waehrend des Updates",
        "Neue Staende gefunden",
        "Vorruecken gescheitert",
        "Neuer Hoechststand",
        "Gespraech beendet",
    ] {
        assert!(fund(text).is_some(), "{text}");
    }
    assert!(fund("Pause\\nNaechster Versuch").is_some());
    assert!(fund("Prüfung für den Handelstag").is_none());
    for text in [
        "geprueft-v1;{baum}",
        "depotschlopf-waechter-{}.lock",
        "/einrichtung?m=schluessel#schluessel",
        "name=\\\"geprueft\\\" value=\\\"ja\\\"",
        "Feld \\\"fuer\\\" fehlt",
    ] {
        assert!(fund(text).is_none(), "{text}");
    }
    let quelle = "// Pruefung fuer Kommentare\nfn pruefe() { let _ = \"Pruefung fuer morgen\"; let _ = r#\"Schluessel fuer Quelle\"#; let _ = '\\n'; }";
    let texte = zeichenketten(quelle)?;
    assert_eq!(
        texte,
        vec!["Pruefung fuer morgen", "Schluessel fuer Quelle"]
    );
    assert_eq!(
        depot_kodex::lexer::zerlege(quelle)?,
        depot_kodex::lexer::zerlege(&quelle.replace("Pruefung fuer morgen", "Prüfung für morgen"))?
    );
    assert!(zeichenketten("/* nicht abgeschlossen").is_err());
    assert!(zeichenketten("let x = \"nicht abgeschlossen").is_err());
    Ok(())
}
