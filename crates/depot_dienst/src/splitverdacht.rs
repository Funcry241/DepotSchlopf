//! Manuelle Entwarnung für genau den angezeigten Xetra-Splitverdacht.
use std::fmt::Write as _;

use depot_journal::{Journal, splitverdacht};
use depot_kern::Zeitpunkt;
use depot_lage::{Zugang, html::maskiere, mit_journal};

use crate::{http, seiten, werkbank::Dienst};

pub(super) fn formular(dienst: &Dienst) -> String {
    mit_journal(&dienst.wurzel.join("daten"), |zugang| {
        let Zugang::Offen(stand) = zugang else {
            return String::new();
        };
        let offen = splitverdacht::aktiv(stand, None);
        if offen.is_empty() {
            return String::new();
        }
        let mut html = String::from(
            "<section id=\"splitverdacht\"><h2>Xetra-Splitverdacht</h2>            <p>Der betroffene Wert bleibt für Aufträge gesperrt. Die Bewertung nutzt den zuletzt davor belegten Schlusskurs.</p>",
        );
        for verdacht in offen.values() {
            let _ = write!(
                html,
                "<form method=\"post\" action=\"/splitverdacht\"><p class=\"halt\">{}</p>                 <input type=\"hidden\" name=\"t\" value=\"{}\">                 <input type=\"hidden\" name=\"kennung\" value=\"{}\">                 <label><input type=\"checkbox\" name=\"geprueft\" value=\"ja\" required>                  Ich habe geprüft, dass dieser Kurssprung kein Split war.</label>                  <button type=\"submit\">Fehlalarm bestätigen</button></form>",
                maskiere(verdacht.text()),
                maskiere(&dienst.token),
                splitverdacht::kennung(verdacht),
            );
        }
        html.push_str("</section>");
        html
    })
}

pub(super) fn entwarnen(dienst: &Dienst, felder: &[(String, String)], jetzt: Zeitpunkt) -> Vec<u8> {
    if http::feld(felder, "geprueft") != Some("ja") {
        return seiten::fehler(400, "Bestätige zuerst den geprüften Fehlalarm.");
    }
    let Some(kennung) = http::feld(felder, "kennung") else {
        return seiten::fehler(400, "Der angezeigte Splitverdacht fehlt.");
    };
    let mut zustand = dienst.sperre();
    if zustand.laufend.is_some() || !zustand.warteschlange.is_empty() {
        return seiten::fehler(
            409,
            "Warte, bis die laufenden und wartenden Aufträge beendet sind.",
        );
    }
    let Ok(mut journal) = Journal::oeffnen(&dienst.wurzel.join("daten/journal.txt")) else {
        return seiten::fehler(409, "Das Journal lässt sich zur Entwarnung nicht öffnen.");
    };
    match splitverdacht::entwarne(&mut journal, kennung, jetzt) {
        Ok(true) => {
            zustand.lage_veraltet = true;
            http::umleitung("/einrichtung?m=splitverdacht_entwarnt")
        }
        Ok(false) => seiten::fehler(
            409,
            "Der Splitverdacht ist inzwischen geändert oder aufgehoben.",
        ),
        Err(_) => seiten::fehler(409, "Die Entwarnung konnte nicht im Journal belegt werden."),
    }
}
