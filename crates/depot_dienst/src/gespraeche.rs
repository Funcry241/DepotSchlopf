//! Lokale Ansicht der protokollierten Aufrufe aller KI-Rollen.
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use depot_kern::json::{Json, erstes_objekt};
use depot_kern::{BeraterId, Cent, Datum};
use depot_lage::gespraeche::{self, Gespraech};
use depot_lage::html::maskiere;

pub(crate) fn beim_start(wurzel: &Path) {
    if let Some((tag, ..)) = crate::jetzt().and_then(depot_kern::Zeitpunkt::in_frankfurt)
        && let Err(e) = gespraeche::aufraeumen(&wurzel.join("daten"), tag)
    {
        eprintln!("[Hinweis] Gesprächsprotokoll nicht aufgeräumt: {e}");
    }
}

pub(crate) fn name(wer: &str) -> &str {
    match wer {
        "dachs" => "Dachs",
        "maus" => "Maus",
        "gemini" => "Gemini",
        wer => wer,
    }
}

pub(crate) fn status(ergebnis: &str) -> &str {
    match ergebnis {
        "ok" => "Antwort erhalten",
        "uebersprungen" => "Nicht gefragt",
        "netz" => "Netzfehler",
        "limit" => "Anbieterlimit",
        "guthaben" => "Guthaben fehlt",
        "kosten" => "Kostengrenze",
        "unklar" => "Antwort unklar",
        "abgelehnt" => "Abgelehnt",
        "ueberlastet" => "Anbieter überlastet",
        "fehler" => "Fehler",
        wert => wert,
    }
}

pub(crate) fn zweck_text(zweck: &str) -> &str {
    match zweck {
        "erstmeinung" => "Erstmeinung",
        "zweitmeinung" => "Zweitmeinung",
        "abstimmung" => "Abstimmung",
        "vorbereitung" => "Vorbereitung",
        "nachrichten-vorsortieren" => "Nachrichten vorsortieren",
        wert => wert,
    }
}

fn kosten(e: &Gespraech, buchungen: &[(BeraterId, Cent)]) -> Option<u64> {
    let cent = e.kosten_cent?;
    let Some(nr) = e.buchung else {
        return Some(cent);
    };
    let (berater, gebucht) = buchungen.get(nr)?;
    let wer = match berater {
        BeraterId::Dachs => "dachs",
        BeraterId::Maus => "maus",
    };
    (wer == e.wer && u64::try_from(gebucht.0).ok() == Some(cent)).then_some(cent)
}

fn absatz(s: &mut String, vorspann: &str, text: &str) {
    let _ = write!(s, "<p>{vorspann}{}</p>", maskiere(text));
}

fn urteil(s: &mut String, obj: &Json, feld: &str, ja: &str, nein: &str) {
    if let Some(zustimmung) = obj.feld(feld).and_then(Json::wahr) {
        absatz(s, "", if zustimmung { ja } else { nein });
    }
}

fn handelsziele(s: &mut String, obj: &Json) {
    match obj.feld("ziele") {
        Some(Json::Null) => absatz(s, "", "Keine neuen Handelsziele."),
        Some(Json::Liste(ziele)) if ziele.is_empty() => {
            absatz(s, "", "Ziel: vollständig in Bargeld wechseln.");
        }
        Some(Json::Liste(ziele)) => {
            for ziel in ziele {
                let isin = ziel.feld("isin").and_then(Json::text);
                let gewicht = ziel.feld("gewicht_bp").and_then(Json::ganz);
                if let (Some(isin), Some(gewicht)) = (isin, gewicht) {
                    let prozent = depot_lage::html::prozent(gewicht);
                    let grund = ziel.feld("begruendung").and_then(Json::text).unwrap_or("");
                    absatz(
                        s,
                        "Ziel: ",
                        &format!("{isin} {}. {grund}", prozent.trim_start_matches(['+', '±'])),
                    );
                }
            }
        }
        _ => {}
    }
}

fn antwort_text(s: &mut String, antwort: &str) {
    let Ok(obj) = erstes_objekt(antwort) else {
        let text = antwort.trim();
        if text.is_empty() || text.contains(['{', '}']) || text.starts_with(['[', '`']) {
            absatz(s, "", "Keine lesbare Antwort belegt.");
        } else {
            absatz(s, "", text);
        }
        return;
    };
    let vorher = s.len();
    if let Some(text) = obj.feld("einschaetzung").and_then(Json::text) {
        absatz(s, "", text);
    }
    urteil(
        s,
        &obj,
        "zustimmung",
        "Stimmt dem Vorschlag zu.",
        "Lehnt den Vorschlag ab.",
    );
    urteil(
        s,
        &obj,
        "zustimmung_fremd",
        "Stimmt dem fremden Vorschlag zu.",
        "Lehnt den fremden Vorschlag ab.",
    );
    if let Some(text) = obj.feld("begruendung").and_then(Json::text) {
        absatz(s, "Begründung: ", text);
    }
    handelsziele(s, &obj);
    if let Some(prognose) = obj.feld("prognose") {
        let isin = prognose.feld("isin").and_then(Json::text);
        let richtung = prognose.feld("richtung").and_then(Json::text);
        let tage = prognose.feld("handelstage").and_then(Json::anzahl);
        if let (Some(isin), Some(richtung), Some(tage)) = (isin, richtung, tage) {
            let richtung = if richtung == "faellt" {
                "fällt"
            } else {
                richtung
            };
            absatz(
                s,
                "Prognose: ",
                &format!("{isin} {richtung} innerhalb von {tage} Handelstagen."),
            );
        }
        if let Some(text) = prognose.feld("begruendung").and_then(Json::text) {
            absatz(s, "Begründung der Prognose: ", text);
        }
        if let Some(text) = prognose.feld("widerlegung").and_then(Json::text) {
            absatz(s, "Woran sie scheitert: ", text);
        }
    }
    for meldung in obj
        .feld("meldungen")
        .and_then(Json::liste)
        .unwrap_or_default()
    {
        if let Some(text) = meldung.feld("kurz").and_then(Json::text) {
            absatz(s, "", text);
        }
    }
    if s.len() == vorher {
        absatz(s, "", "Keine lesbare Antwort belegt.");
    }
}

fn fehler_text(e: &Gespraech) -> &'static str {
    match e.ergebnis.as_str() {
        "guthaben" => "Beim Anbieter fehlt Guthaben. Bitte lade es im Anbieterkonto auf.",
        "abgelehnt" => {
            "Der Anbieter hat den Aufruf abgelehnt. Prüfe die Zugangsdaten in der Einrichtung."
        }
        "netz" => "Der Anbieter war nicht erreichbar.",
        "limit" => {
            "Das Limit des Anbieters ist erreicht. DepotSchlopf wartet auf den nächsten möglichen Aufruf."
        }
        "ueberlastet" => "Der Anbieter war überlastet.",
        "kosten" => {
            "Die Kostengrenze wurde überschritten. Prüfe den Kostenschutz in der Einrichtung."
        }
        "uebersprungen" if e.fehler.as_deref().is_some_and(|f| f.contains("Schlüssel")) => {
            "Kein Zugangsschlüssel verfügbar. Prüfe die Einrichtung."
        }
        "uebersprungen" => "Die KI wurde bei diesem Lauf nicht gefragt.",
        _ => "Keine lesbare Antwort belegt.",
    }
}

fn eintrag(s: &mut String, e: &Gespraech, buchungen: &[(BeraterId, Cent)]) {
    let rolle = if e.rolle == "zusatz" {
        "Zusatz-KI"
    } else {
        "Haupt-KI"
    };
    let _ = write!(
        s,
        "<article><h4>{} ({})</h4><p>{rolle} · {} · {}",
        maskiere(name(&e.wer)),
        maskiere(&e.modell),
        depot_lage::zeit_text(e.zeit),
        maskiere(zweck_text(&e.zweck)),
    );
    if let Some(cent) = kosten(e, buchungen) {
        let _ = write!(s, " · {cent} ct");
    } else if e.buchung.is_some() {
        s.push_str(" · Kosten noch offen");
    }
    s.push_str("</p>");
    if e.ergebnis == "ok"
        && let Some(antwort) = &e.antwort
    {
        antwort_text(s, antwort);
    } else {
        absatz(s, "", fehler_text(e));
    }
    s.push_str("</article>");
}

fn filter(s: &mut String, datum: Option<Datum>, wer: &str, lauf: &str, tage: &[Tag]) {
    let mut namen = BTreeMap::from([("dachs", "Dachs"), ("maus", "Maus")]);
    for e in tage.iter().flat_map(|t| &t.eintraege) {
        namen.insert(&e.wer, name(&e.wer));
    }
    let _ = write!(
        s,
        "<form method=\"get\" action=\"/gespraeche\"><label>Tag <input type=\"date\" name=\"tag\" value=\"{}\"></label>\
         <label>KI <select name=\"wer\"><option value=\"\">Alle KIs</option>",
        datum.map_or_else(String::new, |d| d.to_string())
    );
    for (id, titel) in namen {
        let auswahl = if id == wer { " selected" } else { "" };
        let _ = write!(
            s,
            "<option value=\"{}\"{auswahl}>{}</option>",
            maskiere(id),
            maskiere(titel)
        );
    }
    s.push_str("</select></label>");
    if !lauf.is_empty() {
        let _ = write!(
            s,
            "<input type=\"hidden\" name=\"lauf\" value=\"{}\"> <span>Lauf {}</span>",
            maskiere(lauf),
            maskiere(lauf)
        );
    }
    s.push_str(" <button type=\"submit\">Filtern</button> <a href=\"/gespraeche\">Filter zurücksetzen</a></form>");
}

struct Tag {
    datum: Datum,
    eintraege: Vec<Gespraech>,
    fehler: usize,
    lesefehler: bool,
}

fn tag_lesen(daten: &Path, datum: Datum) -> Tag {
    let (mut eintraege, fehler, lesefehler) = match gespraeche::lesen(daten, datum) {
        Ok((e, f)) => (e, f, false),
        Err(_) => (Vec::new(), 0, true),
    };
    eintraege.sort_by_key(|e| std::cmp::Reverse(e.zeit));
    Tag {
        datum,
        eintraege,
        fehler,
        lesefehler,
    }
}

fn tagesstand(
    s: &mut String,
    tag: &Tag,
    wer: &str,
    lauf: &str,
    buchungen: &[(BeraterId, Cent)],
    leer_zeigen: bool,
) {
    let sichtbar: Vec<_> = tag
        .eintraege
        .iter()
        .filter(|e| (wer.is_empty() || wer == e.wer) && (lauf.is_empty() || lauf == e.lauf))
        .collect();
    if sichtbar.is_empty() && !tag.lesefehler && tag.fehler == 0 && !leer_zeigen {
        return;
    }
    let _ = write!(s, "<h3>{}</h3>", depot_lage::datum_text(tag.datum));
    if tag.lesefehler {
        s.push_str("<p>Gesprächsdatei nicht lesbar.</p>");
        return;
    }
    if sichtbar.is_empty() {
        s.push_str("<p>Keine Gespräche an diesem Tag für die Auswahl.</p>");
    }
    let (ausgelassen, aufrufe): (Vec<_>, Vec<_>) = sichtbar
        .into_iter()
        .partition(|e| e.ergebnis == "uebersprungen");
    for e in aufrufe {
        eintrag(s, e, buchungen);
    }
    if !ausgelassen.is_empty() {
        let _ = write!(
            s,
            "<details><summary>Nicht gefragt ({})</summary>",
            ausgelassen.len()
        );
        for e in ausgelassen {
            eintrag(s, e, buchungen);
        }
        s.push_str("</details>");
    }
    if tag.fehler > 0 {
        let _ = write!(s, "<p>Nicht lesbare Zeilen: {}</p>", tag.fehler);
    }
}

pub(crate) fn inhalt(
    daten: &Path,
    heute: Option<Datum>,
    datum: &str,
    wer: &str,
    lauf: &str,
) -> String {
    let mut s = String::from(
        "<section id=\"gespraeche\"><h2>KI-Gespräche</h2>\
        <p>Antworten und Begründungen von Dachs, Maus und den Zusatz-KIs.</p>",
    );
    let einzel = Datum::lies(datum);
    if !datum.is_empty() && einzel.is_none() {
        s.push_str("<p>Das gewählte Datum ist ungültig.</p></section>");
        return s;
    }
    let tage: Vec<_> = if let Some(d) = einzel {
        vec![tag_lesen(daten, d)]
    } else {
        (0_i64..7)
            .filter_map(|n| heute?.plus_tage(n.saturating_neg()))
            .map(|d| tag_lesen(daten, d))
            .collect()
    };
    let buchungen = if tage
        .iter()
        .flat_map(|tag| &tag.eintraege)
        .any(|e| e.buchung.is_some())
    {
        gespraeche::gebuchte_kosten(daten).unwrap_or_default()
    } else {
        Vec::new()
    };
    filter(&mut s, einzel, wer, lauf, &tage);
    let vor_tagen = s.len();
    for tag in tage {
        tagesstand(&mut s, &tag, wer, lauf, &buchungen, einzel.is_some());
    }
    if s.len() == vor_tagen {
        s.push_str("<p>Keine Gespräche für die Auswahl.</p>");
    }
    s.push_str("</section>");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ungeklärte_kosten_erscheinen_nicht_als_betrag() -> Result<(), &'static str> {
        let zeit = depot_kern::Zeitpunkt::lies("2026-11-10T10:00:00Z").ok_or("Zeit fehlt")?;
        let (datum, ..) = zeit.in_frankfurt().ok_or("Datum fehlt")?;
        let tag = Tag {
            datum,
            fehler: 0,
            lesefehler: false,
            eintraege: vec![Gespraech {
                zeit,
                lauf: "probe".into(),
                wer: "dachs".into(),
                rolle: "haupt".into(),
                modell: "probe".into(),
                zweck: "erstmeinung".into(),
                ergebnis: "unklar".into(),
                http: None,
                token_ein: None,
                token_aus: None,
                buchung: Some(7),
                kosten_cent: Some(3),
                system: None,
                frage: None,
                antwort: None,
                fehler: None,
            }],
        };
        let mut buchungen = vec![(BeraterId::Maus, Cent(1)); 7];
        buchungen.push((BeraterId::Dachs, Cent(20)));
        let mut html = String::new();
        tagesstand(&mut html, &tag, "", "", &buchungen, true);
        assert!(html.contains("Kosten noch offen"));
        assert!(!html.contains("20 ct") && !html.contains("3 ct"));
        let mut html = String::new();
        tagesstand(&mut html, &tag, "", "", &[], true);
        assert!(html.contains("Kosten noch offen") && !html.contains("3 ct"));
        Ok(())
    }
}
