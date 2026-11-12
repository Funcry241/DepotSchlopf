//! Typische ASCII-Umschreibungen in sichtbaren Texten melden, ohne Maschinenformate umzuschreiben.
use depot_kodex::lexer::zeichenketten;
use std::{
    fs,
    path::{Path, PathBuf},
};

const ERLAUBT: &[&str] = &[
    "aktuell",
    "eventuell",
    "manuell",
    "virtuell",
    "individuell",
    "zuerst",
    "zueinander",
    "true",
    "value",
    "continue",
    "queue",
    "issue",
    "due",
    "does",
    "goes",
    "segoe",
];
const AUSNAHMEN: &[(&str, &str, &str)] = &[
    (
        "depot_journal/src/stand.rs",
        "Notbremse aktiv: nur offene Verkaeufe weiter abgleichen",
        "Historischer Journaltext",
    ),
    (
        "depot_journal/src/stand.rs",
        "Notbremse: 30 % Gesamtverlust erreicht. Neue Kaeufe gesperrt; Freigabe des Eigentuemers noetig",
        "Historischer Journaltext",
    ),
    (
        "depot_sim/src/szenario.rs",
        "Alpha Werke kuendigt Quartalszahlen fuer den 26.11. an",
        "Festgeschriebene Szenariodaten",
    ),
    (
        "depot_sim/src/szenario.rs",
        "Notenbank kuendigt Zinsentscheid fuer den 27.11. an",
        "Festgeschriebene Szenariodaten",
    ),
    (
        "depot_sim/src/szenario.rs",
        "Alpha Werke: Umsatz ueber den Erwartungen, Kurs +7,8 %",
        "Festgeschriebene Szenariodaten",
    ),
    (
        "depot_sim/src/szenario.rs",
        "Notenbank laesst den Leitzins unveraendert",
        "Festgeschriebene Szenariodaten",
    ),
    (
        "depot_sim/src/szenario.rs",
        "Gamma Netz kuendigt Hauptversammlung fuer den 10.12. an",
        "Festgeschriebene Szenariodaten",
    ),
    (
        "depot_sim/src/szenario.rs",
        "Datenlieferant meldet Stoerung",
        "Festgeschriebene Szenariodaten",
    ),
    (
        "depot_sim/src/lauf.rs",
        "Zyklus faellt aus: niemand wird gefragt, kein Auftrag",
        "Historischer Journaltext",
    ),
    (
        "depot_lage/src/tage.rs",
        "Zyklus faellt aus",
        "Erkennung alter Journaltexte",
    ),
    (
        "depot_dienst/src/seiten.rs",
        "aktion primaer",
        "CSS-Klassen",
    ),
    (
        "depot_sim/src/szenario.rs",
        "Neuer Monat; Gamma nach der Stoerung wieder plausibel, nichts zu tun",
        "Festgeschriebene Szenariodaten",
    ),
    (
        "depot_kodex/src/freigabe.rs",
        r"{p}\tgeloescht",
        "Marker im Freigabe-Hash",
    ),
];
/// Entfernt Tag und Attribute, behält aber den Text zwischen den Tags.
fn ohne_html_tags(mut text: &str) -> String {
    let mut aus = String::with_capacity(text.len());
    while let Some((vorher, rest)) = text.split_once('<') {
        aus.push_str(vorher);
        let tag = rest.split_once('>').filter(|(innen, _)| {
            innen
                .as_bytes()
                .first()
                .is_some_and(|b| b.is_ascii_alphabetic() || b"/!".contains(b))
                && !innen.contains('<')
        });
        if let Some((_, danach)) = tag {
            aus.push(' ');
            text = danach;
        } else {
            aus.push('<');
            text = rest;
        }
    }
    aus.push_str(text);
    aus
}

fn ist_umschreibung(wort: &str) -> bool {
    let mut vorher = None;
    for (i, b) in wort.bytes().enumerate() {
        if b"aou".contains(&b)
            && wort.as_bytes().get(i.saturating_add(1)) == Some(&b'e')
            && !vorher.is_some_and(|p| b"qae".contains(&p))
        {
            return true;
        }
        vorher = Some(b);
    }
    false
}

fn fund(text: &str) -> Option<String> {
    if text == "Schluessel(***)" {
        return Some(text.into());
    }
    let text = ohne_html_tags(
        &text
            .replace("\\n", " ")
            .replace("\\r", " ")
            .replace("\\t", " "),
    );
    // Bei HTML kann auch ein einzelnes Wort zwischen den Tags sichtbar sein.
    if !text.chars().any(char::is_whitespace) {
        return None;
    }
    for teil in text.split_whitespace() {
        // Feldnamen, URL-/Dateipfade, Formatparameter und zitierte Schlüssel bleiben ASCII.
        if teil.contains(['_', '/', '#', '=', '{', '}', '\\', '"', ';']) {
            continue;
        }
        let wort = teil.trim_matches(|c: char| !c.is_alphanumeric());
        if wort.contains('.') {
            continue;
        }
        // Reine Großschreibung und Binnenmajuskel können Maschinenkennungen sein.
        if !wort.chars().any(char::is_lowercase)
            || wort.split('-').any(|teil| {
                teil.chars().any(char::is_lowercase) && teil.chars().skip(1).any(char::is_uppercase)
            })
        {
            continue;
        }
        let klein = wort.to_ascii_lowercase();
        if ist_umschreibung(&klein) && !ERLAUBT.iter().any(|s| klein.starts_with(s)) {
            return Some(klein);
        }
    }
    None
}
fn ausnahme_index(datei: &str, text: &str) -> Option<usize> {
    AUSNAHMEN
        .iter()
        .position(|(p, t, _)| datei == *p && text == *t)
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
    let mut treffer = vec![0_usize; AUSNAHMEN.len()];
    pfade.sort();
    for pfad in pfade {
        let name = pfad
            .strip_prefix(&crates)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let text = fs::read_to_string(&pfad).map_err(|e| e.to_string())?;
        for literal in zeichenketten(&text)? {
            if let Some(wort) = fund(&literal) {
                if let Some(index) = AUSNAHMEN
                    .iter()
                    .position(|(p, t, _)| name == *p && literal == *t)
                    && let Some(zahl) = treffer.get_mut(index)
                {
                    *zahl = zahl.saturating_add(1);
                } else {
                    fehler.push(format!("{name}: {wort}: {literal}"));
                }
            }
        }
    }
    for ((datei, text, grund), anzahl) in AUSNAHMEN.iter().zip(treffer) {
        if anzahl != 1 {
            fehler.push(format!(
                "{datei}: Ausnahme ({grund}) hat {anzahl} Treffer: {text}"
            ));
        }
    }
    assert!(fehler.is_empty(), "{}", fehler.join("\n"));
    Ok(())
}
#[test]
fn sprachregel_erkennt_meldungen_aber_keine_bezeichner_oder_kommentare() -> Result<(), String> {
    for (text, erwartet) in [
        (
            "Journal fehlt, der Vermerk ist da: Datenbestand unvollstaendig",
            "unvollstaendig",
        ),
        ("Abruf (unvollstaendig) beendet", "unvollstaendig"),
        ("laut Konto gefuellt, laut Journal {zustand:?}", "gefuellt"),
        ("Muell im Anhang", "muell"),
        ("KI-Eintraege entstehen nur über reserviere", "ki-eintraege"),
        ("Uebersicht der Aufträge", "uebersicht"),
        (r"Pause\nNaechster Versuch", "naechster"),
        ("<h2>Aenderungen</h2><p>Text</p>", "aenderungen"),
        ("<h2>Aenderungen</h2>", "aenderungen"),
        (
            "Kontowaechter beim Wiederanlauf: {abweichungen:?}. Halt",
            "kontowaechter",
        ),
        ("Antworte mit hoechstens drei Sätzen", "hoechstens"),
        ("Feld begruendung fehlt", "begruendung"),
        ("Pruefung fuer den Handelstag", "pruefung"),
        ("Schluessel(***)", "Schluessel(***)"),
        ("Waehrend des Updates", "waehrend"),
        ("Neue Staende gefunden", "staende"),
        ("Vorruecken gescheitert", "vorruecken"),
        ("Neuer Hoechststand", "hoechststand"),
        ("Gespraech beendet", "gespraech"),
    ] {
        assert_eq!(fund(text).as_deref(), Some(erwartet), "{text}");
    }
    for text in [
        "Prüfung für den Handelstag",
        "Quelle nicht erreichbar",
        "Die Kursquelle gab den Schlüssel zurück.",
        "Quellstand und Quellenbeleg",
        "neue Version gefunden",
        "Steuerung und Steuerzeichen",
        "Das dauert ein paar Minuten",
        "Mauer, Feuer, Abenteuer",
        "[Update] Schon aktuell: nichts Neues",
        "eventuell später, manuell prüfen",
        "zuerst synchronisiert: {protokoll:?}",
        "Wert true oder false",
        "font-family: system-ui, Segoe UI",
        "bad request",
        "requests per day",
        "Siehe kodex/aenderung.txt",
        "Siehe aenderung.txt im Kodex",
        "daten/update_abgelehnt.txt fehlt",
        "geprueft-v1;{baum}",
        "depotschlopf-waechter-{}.lock",
        "/einrichtung?m=schluessel#schluessel",
        r#"name=\"geprueft\" value=\"ja\""#,
        r#"Feld \"fuer\" fehlt"#,
        r#"Feld "begruendung" fehlt"#,
        r#"Antworte mit {\"einschaetzung\":\"…\",\"begruendung\":\"…\"}"#,
        r#"<section id="taetigkeit" class="x"><h2>Tätigkeit</h2>"#,
        "<div data-aenderungen><input name=x>",
        "FestschreibungAenderung ohne neue Festschreibung",
        "text GEHEIMSCHLUESSEL",
        "Präfix GEHEIMSCHLUE…",
        "UEBERGABE gefunden",
    ] {
        assert!(fund(text).is_none(), "{text}");
    }
    let alttext = "Notbremse aktiv: nur offene Verkaeufe weiter abgleichen";
    assert_eq!(fund(alttext).as_deref(), Some("verkaeufe"));
    assert!(ausnahme_index("depot_journal/src/stand.rs", alttext).is_some());
    assert!(ausnahme_index("depot_dienst/src/seiten.rs", alttext).is_none());
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
