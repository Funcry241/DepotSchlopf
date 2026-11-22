//! Lokale Blindprüfung mit einem historischen These-Beleg-Paar pro Seite.
use std::fmt::Write as _;

use depot_kern::Datum;
use depot_lage::blindpruefung::Prueffall;
use depot_lage::{datum_text, html::maskiere, zeit_text};

use crate::belegurteile::{self, Speicherfehler};
use crate::http::{antwort, feld, umleitung};
use crate::prueftag::{self, Prueftag};
use crate::werkbank::Dienst;

const PFAD: &str = "/neuroschlopf/belegpruefung";
const VERGLEICHSFELD: &str = "<input type=\"hidden\" name=\"vergleich\" value=\"ja\">";
const ANTWORTEN: [(&str, &str); 6] = [
    ("0", "0 · Kein Bezug zur These"),
    (
        "1",
        "1 · Bezug zur These, liefert keinen entscheidenden Hinweis",
    ),
    ("2-stützt", "2 · Entscheidender Hinweis, stützt die These"),
    (
        "2-widerlegt",
        "2 · Entscheidender Hinweis, widerlegt die These",
    ),
    ("2-offen", "2 · Entscheidender Hinweis, Richtung offen"),
    (
        "unbewertet",
        "Unbewertet · Mit diesen Angaben kann ich das nicht beurteilen",
    ),
];

fn rahmen(d: &Dienst, heute: Option<Datum>, status: u16, inhalt: &str) -> Vec<u8> {
    let seite = crate::seiten::rahmen(
        PFAD,
        None,
        None,
        inhalt,
        d.version.as_deref(),
        (&crate::betriebsleiste(d, heute), &crate::update_leiste(d)),
        ("", ""),
    );
    antwort(status, crate::HTML, seite.as_bytes())
}

fn meldung(
    d: &Dienst,
    heute: Option<Datum>,
    tag: Option<Datum>,
    vergleich: bool,
    code: u16,
    text: &str,
) -> Vec<u8> {
    let zusatz = match (tag, vergleich) {
        (Some(t), true) => format!("?tag={t}&amp;vergleich=ja"),
        (Some(t), false) => format!("?tag={t}"),
        (None, true) => "?vergleich=ja".into(),
        (None, false) => String::new(),
    };
    let html = format!(
        "<section><h2>Blindprüfung</h2><p role=\"alert\">{}</p><p><a href=\"{PFAD}{zusatz}\">Blindprüfung erneut öffnen</a></p></section>",
        maskiere(text)
    );
    rahmen(d, heute, code, &html)
}

fn tagwahl(s: &mut String, gruppe: &Prueftag, vergleich: bool) {
    if gruppe.tage.is_empty() {
        return;
    }
    let _ = write!(
        s,
        "<form method=\"get\" action=\"{PFAD}\"><label>Prüftag <select name=\"tag\">"
    );
    for datum in &gruppe.tage {
        let gewaehlt = if Some(*datum) == gruppe.tag {
            " selected"
        } else {
            ""
        };
        let _ = write!(
            s,
            "<option value=\"{datum}\"{gewaehlt}>{}</option>",
            datum_text(*datum)
        );
    }
    s.push_str("</select></label>");
    if vergleich {
        s.push_str(VERGLEICHSFELD);
    }
    s.push_str("<button type=\"submit\">Tag anzeigen</button></form>");
}

fn formular(s: &mut String, fall: &Prueffall, d: &Dienst, tag: Datum, vergleich: bool) {
    let quelle = fall
        .quelle
        .as_deref()
        .map_or_else(|| "Nicht belegt".into(), maskiere);
    let zeit = fall
        .veroeffentlicht
        .map_or_else(|| "Nicht belegt".into(), zeit_text);
    let modus = if vergleich { VERGLEICHSFELD } else { "" };
    let _ = write!(
        s,
        "<section class=\"blindpruefung-fall\"><div class=\"blindpruefung-paar\">\
        <div><h3>These</h3><p><strong>{}</strong></p><p>{}</p><p>Wissensstichtag: {}</p></div>\
        <div><h3>Beleg</h3><p>{}</p><p>Quelle: {quelle} · Veröffentlicht: {zeit}</p></div></div>\
        <form method=\"post\" action=\"{PFAD}\" data-blindentscheidung><input type=\"hidden\" name=\"t\" value=\"{}\">\
        <input type=\"hidden\" name=\"id\" value=\"{}\"><input type=\"hidden\" name=\"tag\" value=\"{tag}\">{modus}\
        <fieldset><legend>Welche Bedeutung hatte dieser Beleg für die These am Wissensstichtag?</legend>\
        <div class=\"blindpruefung-urteile\">",
        maskiere(&fall.wert),
        maskiere(&fall.these),
        zeit_text(fall.stichtag),
        maskiere(&fall.beleg),
        maskiere(&d.token),
        maskiere(&fall.kennung)
    );
    for (wert, text) in ANTWORTEN {
        let _ = write!(
            s,
            "<label><input type=\"radio\" name=\"urteil\" value=\"{wert}\" required> <span>{text}</span></label>"
        );
    }
    s.push_str(
        "</div></fieldset><button type=\"submit\">Urteil speichern und weiter</button></form></section>",
    );
}

fn vergleichsfaelle<'a>(
    d: &Dienst,
    gruppe: &'a Prueftag,
    tag: Datum,
) -> Result<Vec<&'a Prueffall>, String> {
    let pool = crate::neuromodell::pruefpool(&d.wurzel.join("daten"), tag)?;
    let mut faelle: Vec<_> = gruppe
        .faelle
        .iter()
        .filter(|fall| pool.contains(&fall.kennung))
        .collect();
    faelle.sort_unstable_by(|a, b| a.kennung.cmp(&b.kennung));
    Ok(faelle)
}

pub(super) fn get(d: &Dienst, heute: Option<Datum>, abfrage: &str) -> Vec<u8> {
    let Some(f) = crate::http::formular(abfrage.as_bytes()) else {
        return crate::seiten::fehler(400, "Der gewählte Prüftag ist ungültig.");
    };
    let wahl = match feld(&f, "tag") {
        Some(wert) => match Datum::lies(wert) {
            Some(datum) => Some(datum),
            None => return crate::seiten::fehler(400, "Der gewählte Prüftag ist ungültig."),
        },
        None => None,
    };
    let vergleich = match feld(&f, "vergleich") {
        Some("ja") => true,
        None => false,
        Some(_) => return crate::seiten::fehler(400, "Die Auswahl der Blindprüfung ist ungültig."),
    };
    seite(
        d,
        heute,
        wahl,
        feld(&f, "gespeichert") == Some("ja"),
        vergleich,
    )
}

fn seite(
    d: &Dienst,
    heute: Option<Datum>,
    wahl: Option<Datum>,
    gespeichert: bool,
    vergleich: bool,
) -> Vec<u8> {
    let gruppe = match prueftag::laden(d, heute, wahl) {
        Ok(gruppe) => gruppe,
        Err(grund) => return meldung(d, heute, wahl, vergleich, 500, &grund),
    };
    let titel = if vergleich {
        "Blindprüfung zum Rangvergleich"
    } else {
        "Blindprüfung der Belegauswahl"
    };
    let mut s = format!(
        "<section class=\"blindpruefung-kopf\"><div class=\"blindpruefung-zeile\">\
        <h2>{titel}</h2><a href=\"/neuroschlopf\">Zurück zu NeuroSchlopf</a></div>\
        <p>Beurteile jeden Beleg anhand der damaligen These. DepotSchlopf speichert deine Urteile lokal.</p>",
    );
    if gespeichert {
        s.push_str("<p role=\"status\">Dein Urteil wurde gespeichert.</p>");
    }
    if let Some(grund) = gruppe.hinweis.as_deref() {
        let _ = write!(s, "<p>{}</p>", maskiere(grund));
    }
    let Some(tag) = gruppe.tag.filter(|_| !gruppe.faelle.is_empty()) else {
        tagwahl(&mut s, &gruppe, vergleich);
        if gruppe.hinweis.is_none() {
            s.push_str("<p>Keine Bewertung offen.</p>");
        }
        s.push_str("</section>");
        return rahmen(d, heute, 200, &s);
    };
    let stichprobe = if vergleich {
        match vergleichsfaelle(d, &gruppe, tag) {
            Ok(faelle) if faelle.is_empty() => {
                tagwahl(&mut s, &gruppe, vergleich);
                s.push_str("<p>Für diesen Prüftag liegt keine Stichprobe aus dem Rangvergleich vor.</p></section>");
                return rahmen(d, heute, 200, &s);
            }
            Ok(faelle) => Some(faelle),
            Err(grund) => return meldung(d, heute, Some(tag), vergleich, 500, &grund),
        }
    } else {
        None
    };
    let Ok(urteile) = belegurteile::lese(&d.wurzel.join("daten")) else {
        return meldung(
            d,
            heute,
            Some(tag),
            vergleich,
            500,
            "Die gespeicherten Urteile sind nicht lesbar.",
        );
    };
    let alle_faelle = stichprobe.unwrap_or_else(|| gruppe.faelle.iter().collect());
    let bearbeitet = alle_faelle
        .iter()
        .filter_map(|fall| urteile.get(&fall.kennung));
    let unbewertet = bearbeitet.clone().filter(|urteil| urteil.is_none()).count();
    let bearbeitet = bearbeitet.count();
    let offene_faelle = if vergleich {
        alle_faelle
    } else {
        gruppe.offene_faelle().collect()
    };
    let offen = offene_faelle
        .into_iter()
        .filter(|fall| !urteile.contains_key(&fall.kennung));
    s.push_str("<div class=\"blindpruefung-zeile\">");
    tagwahl(&mut s, &gruppe, vergleich);
    let _ = write!(
        s,
        "<p>Bearbeitet <b>{} von {}</b>",
        bearbeitet,
        bearbeitet.saturating_add(offen.clone().count())
    );
    if unbewertet > 0 {
        let _ = write!(s, " · davon ausdrücklich unbewertet: {unbewertet}");
    }
    s.push_str("</p></div></section>");
    if let Some(fall) = offen.into_iter().next() {
        formular(&mut s, fall, d, tag, vergleich);
    } else {
        s.push_str(
            "<section><h3>Prüftag abgeschlossen</h3><p>Keine Bewertung offen.</p></section>",
        );
    }
    rahmen(d, heute, 200, &s)
}

pub(super) fn speichern(d: &Dienst, f: &[(String, String)], heute: Option<Datum>) -> Vec<u8> {
    let tag = feld(f, "tag").and_then(Datum::lies);
    let id = feld(f, "id").unwrap_or_default();
    let auswahl = feld(f, "urteil").unwrap_or_default();
    let modus = feld(f, "vergleich");
    let vergleich = modus == Some("ja");
    let feldzahl = if vergleich { 5 } else { 4 };
    let Some(tag) = tag.filter(|_| {
        f.len() == feldzahl
            && (modus.is_none() || vergleich)
            && belegurteile::kennung_gueltig(id)
            && ANTWORTEN.iter().any(|(wert, _)| *wert == auswahl)
            && feld(f, "t").is_some()
    }) else {
        return meldung(
            d,
            heute,
            tag,
            vergleich,
            400,
            "Das Formular ist unvollständig oder ungültig. Wähle eine der sechs Antworten.",
        );
    };
    let (stufe, richtung) = auswahl.split_once('-').unwrap_or((auswahl, ""));
    let Ok(urteil) = belegurteile::aus_form(stufe, richtung) else {
        return meldung(
            d,
            heute,
            Some(tag),
            vergleich,
            400,
            "Diese Bewertung ist ungültig.",
        );
    };
    let gruppe = match prueftag::laden(d, heute, Some(tag)) {
        Ok(gruppe) => gruppe,
        Err(grund) => return meldung(d, heute, Some(tag), vergleich, 500, &grund),
    };
    let im_vergleich = if vergleich {
        match crate::neuromodell::pruefpool(&d.wurzel.join("daten"), tag) {
            Ok(pool) => pool.contains(id),
            Err(grund) => return meldung(d, heute, Some(tag), vergleich, 500, &grund),
        }
    } else {
        true
    };
    let Some(fall) = gruppe
        .faelle
        .iter()
        .find(|fall| im_vergleich && fall.kennung == id && gruppe.tag == Some(tag))
    else {
        return meldung(
            d,
            heute,
            Some(tag),
            vergleich,
            409,
            "Dieses Paar ist für den gewählten Prüftag nicht mehr belegt.",
        );
    };
    let modus = if vergleich { "&vergleich=ja" } else { "" };
    match belegurteile::speichere(&d.wurzel.join("daten"), fall, urteil) {
        Ok(()) => umleitung(&format!("{PFAD}?tag={tag}&gespeichert=ja{modus}")),
        Err(Speicherfehler::Konflikt) => meldung(
            d,
            heute,
            Some(tag),
            vergleich,
            409,
            "Dieses Paar wurde bereits anders bewertet. Das gespeicherte Urteil gilt weiterhin.",
        ),
        Err(Speicherfehler::Ungueltig) => meldung(
            d,
            heute,
            Some(tag),
            vergleich,
            400,
            "Die Bewertung ist ungültig.",
        ),
        Err(Speicherfehler::Speicher) => meldung(
            d,
            heute,
            Some(tag),
            vergleich,
            500,
            "Die Bewertung konnte nicht lokal gespeichert werden.",
        ),
    }
}
