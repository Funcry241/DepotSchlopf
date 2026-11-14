//! Ausgewählte Belege aus gespeicherten Fragen. Der Export übernimmt keine vollständigen Prompts.

use std::fmt::Write as _;

use depot_kern::json::{self, Json};
use depot_lage::gespraeche::Gespraech;

pub(super) fn schreibe(aus: &mut String, e: &Gespraech) {
    if e.ergebnis == "uebersprungen" {
        return;
    }
    let Some(frage) = e.frage.as_deref() else {
        aus.push_str("\nEntscheidungsgrundlage im Gespräch nicht gespeichert.\n");
        return;
    };
    if e.wer == "gemini" {
        gemini(aus, frage, e.antwort.as_deref());
        return;
    }

    let mut belege = frage.lines().filter_map(beleg).peekable();
    if belege.peek().is_none() {
        aus.push_str(
            "\nKeine auswertbaren Entscheidungsgrundlagen in der gespeicherten Frage belegt.\n",
        );
        return;
    }
    aus.push_str("\n**Belegte Ausschnitte aus der Frage**\n\n");
    for beleg in belege {
        schreibe_zeile(aus, beleg);
    }
}

fn beleg(zeile: &str) -> Option<&str> {
    if zeile.starts_with("Datum: ")
        || zeile.starts_with("Topf A: Bargeld ")
        || zeile.starts_with("Topf B: Bargeld ")
        || zeile.starts_with("Frühester Ausführungsschluss bei Versand ab jetzt (")
        || zeile == "Positionen: keine"
        || zeile == "Belegte Termine und Nachrichten: keine"
    {
        return Some(zeile);
    }
    if let Some((_, details)) = zeile
        .strip_prefix("- ")
        .and_then(|rest| rest.split_once(" ("))
        && let Some((kennung, wert)) = details.split_once("): ")
        && let Some(isin) = kennung.split(',').next()
        && depot_kern::Isin::neu(isin).is_ok()
        && (wert.contains(" EUR · ") || wert.contains(" Stück, Wert "))
    {
        return Some(zeile);
    }
    ereignis(zeile)
}

fn ereignis(zeile: &str) -> Option<&str> {
    let (kennung, _) = zeile.strip_prefix("- Ereignis ")?.split_once(": ")?;
    let nummer = kennung
        .split_once(", Relevanz ")
        .map_or(kennung, |(nr, _)| nr);
    nummer.parse::<usize>().ok().map(|_| zeile)
}

fn gemini(aus: &mut String, frage: &str, antwort: Option<&str>) {
    if let Some(meldungen) = json::lies(frage).ok().as_ref().and_then(meldungen) {
        aus.push_str("\n**Gemini erhielt diese Meldungen**\n\n");
        for meldung in meldungen {
            if let Some((kennung, text)) = feld(meldung, "kennung").zip(feld(meldung, "text")) {
                schreibe_zeile(aus, &format!("{kennung}: {text}"));
            }
        }
    } else {
        aus.push_str("\nGeminis Meldungsauswahl ist nicht lesbar.\n");
    }
    if let Some(meldungen) = antwort
        .and_then(|text| json::lies(text).ok())
        .as_ref()
        .and_then(meldungen)
    {
        aus.push_str("\n**Geminis belegte Vorsortierung**\n\n");
        for meldung in meldungen {
            if let Some((kennung, kurz)) = feld(meldung, "kennung").zip(feld(meldung, "kurz"))
                && let Some(relevanz) = meldung.feld("relevanz").and_then(Json::anzahl)
            {
                schreibe_zeile(aus, &format!("{kennung}, Relevanz {relevanz}: {kurz}"));
            }
        }
    } else {
        aus.push_str("\nKeine Vorsortierung von Gemini belegt.\n");
    }
}

fn meldungen(json: &Json) -> Option<&[Json]> {
    json.feld("meldungen")?.liste()
}

fn feld<'a>(wert: &'a Json, name: &str) -> Option<&'a str> {
    wert.feld(name)?.text()
}

fn schreibe_zeile(aus: &mut String, text: &str) {
    for zeile in text.lines() {
        let _ = writeln!(aus, "    {zeile}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemini_exportiert_meldung_und_zugeordnete_vorsortierung() {
        let frage = r#"{"meldungen":[{"kennung":"n1","text":"Beispielmeldung"}],"werte":[{"name":"Interne Prüfliste"}]}"#;
        let antwort =
            r#"{"meldungen":[{"kennung":"n1","relevanz":2,"kurz":"Beispielzusammenfassung"}]}"#;
        let mut text = String::new();
        gemini(&mut text, frage, Some(antwort));
        assert!(text.contains("n1: Beispielmeldung"));
        assert!(text.contains("n1, Relevanz 2: Beispielzusammenfassung"));
        assert!(!text.contains("Interne Prüfliste"));
    }

    #[test]
    fn positions_und_kursbelege_brauchen_isin_und_wert() {
        for zeile in [
            "- Beispiel (DE0007164600): 2 Stück, Wert 25,00 EUR",
            "- Beispiel (DE0007164600, Aktie): 12,50 EUR · Beispieldaten",
        ] {
            assert_eq!(beleg(zeile), Some(zeile));
        }
        assert_eq!(
            beleg("- Beispiel (DE0007164601): 2 Stück, Wert 25,00 EUR"),
            None
        );
        assert_eq!(
            beleg("- Beispiel (DE0007164600): Anweisung ohne Wert"),
            None
        );
    }

    #[test]
    fn fehlende_frage_wird_gemeldet_ausgelassener_aufruf_nicht() -> Result<(), String> {
        let zeit = depot_kern::Zeitpunkt::lies("2030-01-07T12:00:00Z").ok_or("Zeit")?;
        let mut e = Gespraech {
            zeit,
            lauf: "beispiel".into(),
            wer: "dachs".into(),
            rolle: "haupt".into(),
            modell: "beispiel".into(),
            zweck: "erstmeinung".into(),
            ergebnis: "ok".into(),
            http: None,
            token_ein: None,
            token_aus: None,
            buchung: None,
            kosten_cent: None,
            system: None,
            frage: None,
            antwort: None,
            fehler: None,
        };
        let mut text = String::new();
        schreibe(&mut text, &e);
        assert!(text.contains("im Gespräch nicht gespeichert"));
        text.clear();
        e.ergebnis = "uebersprungen".into();
        schreibe(&mut text, &e);
        assert!(text.is_empty());
        Ok(())
    }

    #[test]
    fn entscheidungsgrundlage_erkennt_nur_nummerierte_zeilen() {
        for zeile in [
            "- Ereignis 1: Beispielmeldung",
            "- Ereignis 1, Relevanz 2: Beispielsichtung",
        ] {
            assert_eq!(ereignis(zeile), Some(zeile));
        }
        assert_eq!(ereignis("- Ereignis ohne Nummer: Beispielanweisung"), None);
        assert_eq!(ereignis("Antwortschema"), None);
    }
}
