use depot_kern::Datum;
use depot_kern::gemini::{DIENST, MODELL, TAGESGRENZE};
use depot_lage::gespraeche;
use depot_lage::html::maskiere;

use crate::http::{feld, umleitung};
use crate::schluessel::Wer;
use crate::werkbank::Dienst;
use crate::{Umwelt, seiten};

pub(crate) fn speichern(d: &Dienst, formular: &[(String, String)]) -> Vec<u8> {
    let wert = match (feld(formular, "an"), feld(formular, "gratisprojekt")) {
        (Some("ja"), Some("ja")) => "gratisprojekt=ja\n",
        (Some("nein"), _) => "gratisprojekt=nein\n",
        _ => {
            return seiten::fehler(
                400,
                "Bestätige für Gemini ein Gratis-Projekt ohne Abrechnung.",
            );
        }
    };
    match crate::werkbank::schreibe_ganz(&gespraeche::gemini_pfad(&d.wurzel.join("daten")), wert) {
        Ok(()) => umleitung("/einrichtung#zusatz"),
        Err(_) => seiten::fehler(500, "Gemini-Einstellung nicht gespeichert."),
    }
}

pub(crate) fn einrichtung(d: &Dienst, u: &dyn Umwelt, tag: Option<Datum>) -> String {
    let daten = d.wurzel.join("daten");
    let an = gespraeche::gemini_aktiv(&daten);
    let hat_schluessel = u.schluessel(Wer::Gemini) == depot_lage::Hinterlegt::Ja;
    let eintraege = tag
        .and_then(|tag| gespraeche::lesen(&daten, tag).ok())
        .unwrap_or_default()
        .0;
    let letzte = eintraege.iter().rev().find(|e| e.wer == DIENST);
    let aufrufe = tag.map_or(0, |tag| gespraeche::gemini_aufrufe(&daten, tag));
    let stand = if !an {
        "Aus"
    } else if !hat_schluessel {
        "Schlüssel fehlt"
    } else if let Some(e) = letzte {
        e.fehler
            .as_deref()
            .unwrap_or_else(|| crate::gespraeche::status(&e.ergebnis))
    } else {
        "Bereit"
    };
    let auswahl = if an { " checked" } else { "" };
    let token = maskiere(&d.token);
    format!(
        "<section id=\"zusatz\" class=\"einstellungsgruppe\" aria-labelledby=\"zusatz-titel\">\
         <h2 id=\"zusatz-titel\">Zusatz-KIs (optional)</h2>\
         <p>Optionale Zuarbeiter helfen Dachs ({}) und Maus ({}). Der Papiertag läuft auch ohne sie.</p>\
         <section aria-labelledby=\"gemini-titel\"><h3 id=\"gemini-titel\">Gemini ({MODELL})</h3>\
         <p>Sortiert belegte Nachrichten mit höchster Denkstufe vor.</p>\
         <p><strong>{}</strong> · Aufrufe heute {aufrufe} von {TAGESGRENZE} · Gratis-Projekt ohne Abrechnung</p>\
         <form method=\"post\" action=\"/schluessel\" data-aenderungen>\
         <input type=\"hidden\" name=\"t\" value=\"{token}\">\
         <input type=\"hidden\" name=\"wer\" value=\"{DIENST}\">\
         <label>Gemini-Schlüssel <input type=\"password\" name=\"wert\" autocomplete=\"new-password\" required></label>\
         <button type=\"submit\">Schlüssel speichern</button>\
         <p data-speicherstand role=\"status\">Noch keinen neuen Schlüssel eingegeben.</p></form>\
         <form method=\"post\" action=\"/gemini\" data-aenderungen><input type=\"hidden\" name=\"t\" value=\"{token}\">\
         <label><input type=\"checkbox\" name=\"gratisprojekt\" value=\"ja\"{auswahl}> Ich verwende ein Google-Gratis-Projekt ohne Abrechnung.</label>\
         <button type=\"submit\" name=\"an\" value=\"ja\">Gemini einschalten</button> \
         <button type=\"submit\" name=\"an\" value=\"nein\">Gemini ausschalten</button>\
         <p data-speicherstand role=\"status\"></p></form></section></section>",
        depot_kern::modelle::MODELL_DACHS,
        depot_kern::modelle::MODELL_MAUS,
        maskiere(stand),
    )
}
