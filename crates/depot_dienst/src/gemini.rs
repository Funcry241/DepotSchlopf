use depot_kern::Datum;
use depot_kern::gemini::{DIENST, MODELL, TAGESGRENZE};
use depot_lage::gespraeche;
use depot_lage::html::maskiere;
use depot_lage::{GeminiBudget, SchluesselStand};

use crate::http::{feld, umleitung};
use crate::schluessel::Wer;
use crate::seiten;
use crate::werkbank::Dienst;
use std::path::Path;

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
        Ok(()) => {
            d.sperre().lage_veraltet = true;
            umleitung("/einrichtung#zusatz")
        }
        Err(_) => seiten::fehler(500, "Gemini-Einstellung nicht gespeichert."),
    }
}

pub(crate) fn budget(daten: &Path, tag: Option<Datum>) -> GeminiBudget {
    GeminiBudget {
        an: gespraeche::gemini_aktiv(daten),
        aufrufe: tag.map(|tag| gespraeche::gemini_aufrufe(daten, tag)),
    }
}

pub(crate) fn einrichtung(d: &Dienst, stand: SchluesselStand, tag: Option<Datum>) -> String {
    let daten = d.wurzel.join("daten");
    let budget = budget(&daten, tag);
    let letzter_lauf = tag
        .and_then(|tag| gespraeche::lesen(&daten, tag).ok())
        .and_then(|(eintraege, _)| eintraege.into_iter().rev().find(|e| e.wer == DIENST))
        .map_or_else(String::new, |e| {
            let ergebnis = e
                .fehler
                .as_deref()
                .unwrap_or_else(|| crate::gespraeche::status(&e.ergebnis));
            format!(
                "<p class=\"leise\">Letzter Lauf heute ({}) · {}</p>",
                depot_lage::zeit_text(e.zeit),
                maskiere(ergebnis)
            )
        });
    let status = if budget.an {
        "Eingeschaltet"
    } else {
        "Ausgeschaltet"
    };
    let klasse = if budget.an { "gut" } else { "leise" };
    let aufrufe = budget.aufrufe.map_or_else(|| "–".into(), |n| n.to_string());
    let mut schluessel = seiten::SCHLUESSEL_TABELLE.to_owned();
    seiten::schluessel_zeile(&mut schluessel, &d.token, Wer::Gemini, stand, "");
    let token = maskiere(&d.token);
    let einschalten = if budget.an {
        " disabled"
    } else {
        " class=\"primaer\""
    };
    let ausschalten = if budget.an { "" } else { " disabled" };
    let bestaetigung = if budget.an {
        ""
    } else {
        "<label><input type=\"checkbox\" name=\"gratisprojekt\" value=\"ja\" required> Ich verwende ein Google-Gratis-Projekt ohne Abrechnung.</label>"
    };
    format!(
        "<section id=\"zusatz\" class=\"einstellungsgruppe\" aria-labelledby=\"zusatz-titel\">\
         <h2 id=\"zusatz-titel\">Zusatz-KIs (optional)</h2>\
         <p>Optionale Zuarbeiter helfen Dachs ({}) und Maus ({}). Der Papiertag läuft auch ohne sie.</p>\
         <section aria-labelledby=\"gemini-titel\"><h3 id=\"gemini-titel\">Gemini ({MODELL})</h3>\
         <p>Sortiert belegte Nachrichten mit höchster Denkstufe vor.</p>\
         <p><strong class=\"{klasse}\">{status}</strong> · Aufrufe heute {aufrufe} von {TAGESGRENZE} · Gratis-Projekt ohne Abrechnung</p>\
         {letzter_lauf}{schluessel}</table>\
         <form method=\"post\" action=\"/gemini\" data-aenderungen><input type=\"hidden\" name=\"t\" value=\"{token}\">\
         {bestaetigung}<button type=\"submit\" name=\"an\" value=\"ja\"{einschalten}>Gemini einschalten</button> \
         <button type=\"submit\" name=\"an\" value=\"nein\"{ausschalten}>Gemini ausschalten</button>\
         <p data-speicherstand role=\"status\"></p></form></section></section>",
        depot_kern::modelle::MODELL_DACHS,
        depot_kern::modelle::MODELL_MAUS,
    )
}
