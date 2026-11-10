//! Lokale Ansicht der protokollierten Aufrufe aller KI-Rollen.
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

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

fn name(wer: &str) -> &str {
    match wer {
        "dachs" => "Dachs",
        "maus" => "Maus",
        "gemini" => "Gemini",
        wer => wer,
    }
}

fn status(ergebnis: &str) -> &str {
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

fn zweck_text(zweck: &str) -> &str {
    match zweck {
        "erstmeinung" => "Erstmeinung",
        "zweitmeinung" => "Zweitmeinung",
        "abstimmung" => "Abstimmung",
        "vorbereitung" => "Vorbereitung",
        wert => wert,
    }
}

fn zahl(wert: Option<impl ToString>) -> String {
    wert.map_or_else(|| "–".into(), |n| n.to_string())
}

fn kosten(e: &Gespraech, buchungen: &[(BeraterId, Cent)]) -> (Option<u64>, bool) {
    let Some(nr) = e.buchung else {
        return (e.kosten_cent, false);
    };
    let betrag = buchungen.get(nr).and_then(|(berater, cent)| {
        let wer = match berater {
            BeraterId::Dachs => "dachs",
            BeraterId::Maus => "maus",
        };
        (wer == e.wer).then(|| u64::try_from(cent.0).ok()).flatten()
    });
    (betrag, true)
}

fn eintrag(s: &mut String, e: &Gespraech, buchungen: &[(BeraterId, Cent)]) {
    let rolle = if e.rolle == "zusatz" {
        "Zusatz-KI"
    } else {
        "Haupt-KI"
    };
    let kosten = match kosten(e, buchungen) {
        (Some(cent), true) => format!("{cent} ct (gebucht)"),
        (Some(cent), false) => format!("{cent} ct (laut Antwort)"),
        (None, true) => "Buchung nicht lesbar".into(),
        (None, false) => "–".into(),
    };
    let _ = write!(
        s,
        "<article><h3>{} ({})</h3><p>{rolle} · {} · {} · {}</p>\
         <p>HTTP {} · Token ein/aus {}/{} · Kosten {kosten}</p>",
        maskiere(name(&e.wer)),
        maskiere(&e.modell),
        depot_lage::zeit_text(e.zeit),
        maskiere(zweck_text(&e.zweck)),
        maskiere(status(&e.ergebnis)),
        zahl(e.http),
        zahl(e.token_ein),
        zahl(e.token_aus),
    );
    if let Some(grund) = &e.fehler {
        let _ = write!(s, "<p>{}</p>", maskiere(grund));
    }
    for (titel, text) in [
        ("Systemtext", &e.system),
        ("Frage", &e.frage),
        ("Antwort", &e.antwort),
    ] {
        if let Some(t) = text {
            let _ = write!(
                s,
                "<details><summary>{titel}</summary><pre style=\"white-space:pre-wrap;overflow-wrap:anywhere\">{}</pre></details>",
                maskiere(t)
            );
        }
    }
    s.push_str("</article>");
}

fn filter(s: &mut String, datum: Option<Datum>, wer: &str, tage: &[Tag]) {
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
    s.push_str("</select></label> <button type=\"submit\">Filtern</button> <a href=\"/gespraeche\">Filter zurücksetzen</a></form>");
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

fn tagesstand(s: &mut String, tag: &Tag, wer: &str, buchungen: &[(BeraterId, Cent)]) {
    let _ = write!(s, "<h3>{}</h3>", depot_lage::datum_text(tag.datum));
    if tag.lesefehler {
        s.push_str("<p>Gesprächsdatei nicht lesbar.</p>");
        return;
    }
    let sichtbar: Vec<_> = tag
        .eintraege
        .iter()
        .filter(|e| wer.is_empty() || wer == e.wer)
        .collect();
    let mut nach_ki = BTreeMap::<&str, usize>::new();
    let mut anzahl = 0_usize;
    let mut cent = 0_u64;
    let mut gebucht = 0_u64;
    let mut unbekannt = false;
    for e in &sichtbar {
        if e.ergebnis != "uebersprungen" {
            let n = nach_ki.entry(name(&e.wer)).or_default();
            *n = n.saturating_add(1);
            anzahl = anzahl.saturating_add(1);
        }
        if e.ergebnis != "uebersprungen" || e.buchung.is_some() {
            let (betrag, buchung) = kosten(e, buchungen);
            cent = cent.saturating_add(betrag.unwrap_or_default());
            if buchung {
                gebucht = gebucht.saturating_add(betrag.unwrap_or_default());
            }
            unbekannt |= betrag.is_none();
        }
    }
    if sichtbar.is_empty() {
        s.push_str("<p>Keine Gespräche an diesem Tag für die Auswahl.</p>");
    } else {
        let _ = write!(
            s,
            "<p>{anzahl} Aufrufe · bekannte Kosten {cent} ct · davon gebucht {gebucht} ct"
        );
        if unbekannt {
            s.push_str(" · Kosten teils unbekannt");
        }
        for (name, n) in nach_ki {
            let _ = write!(s, " · {} {n}", maskiere(name));
        }
        s.push_str("</p>");
        for e in sichtbar {
            eintrag(s, e, buchungen);
        }
    }
    if tag.fehler > 0 {
        let _ = write!(s, "<p>Nicht lesbare Zeilen: {}</p>", tag.fehler);
    }
}

pub(crate) fn inhalt(daten: &Path, heute: Option<Datum>, datum: &str, wer: &str) -> String {
    let mut s = String::from(
        "<section id=\"gespraeche\"><h2>KI-Gespräche</h2>\
        <p>Lokales Protokoll für Haupt- und Zusatz-KIs. Übersprungene Anfragen stehen separat in der Liste.</p>",
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
    filter(&mut s, einzel, wer, &tage);
    for tag in tage {
        tagesstand(&mut s, &tag, wer, &buchungen);
    }
    s.push_str("</section>");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buchung_hat_vorrang_vor_kosten_laut_antwort() -> Result<(), &'static str> {
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
        tagesstand(&mut html, &tag, "", &buchungen);
        assert!(html.contains("Kosten 20 ct (gebucht)"));
        assert!(html.contains("bekannte Kosten 20 ct · davon gebucht 20 ct"));
        let mut html = String::new();
        tagesstand(&mut html, &tag, "", &[]);
        assert!(html.contains("Buchung nicht lesbar") && !html.contains("3 ct"));
        Ok(())
    }
}
