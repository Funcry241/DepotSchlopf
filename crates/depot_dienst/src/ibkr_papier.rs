//! Einrichtung und lesender Verbindungstest für das lokale IBKR Gateway.

use std::fmt::Write as _;
use std::path::PathBuf;

use depot_kern::ibkr_papier::{KONTO_DATEI, kontonummer_gueltig};
use depot_lage::html::maskiere;

use crate::http::{feld, formular, umleitung};
use crate::werkbank::{Anlass, Art, Dienst, Frueher, lies_falls_da, schreibe_ganz};

fn datei(d: &Dienst) -> PathBuf {
    d.wurzel.join("daten").join(KONTO_DATEI)
}

fn gespeichert(d: &Dienst) -> bool {
    lies_falls_da(&datei(d))
        .ok()
        .flatten()
        .is_some_and(|wert| kontonummer_gueltig(wert.trim()))
}

fn zurueck(meldung: &str) -> Vec<u8> {
    umleitung(&format!("/einrichtung?ibkr={meldung}#ibkr-papier"))
}

pub(crate) fn speichern(d: &Dienst, wert: &str) -> Vec<u8> {
    let konto = wert.trim();
    if !kontonummer_gueltig(konto) {
        return zurueck("format");
    }
    if d.update_offen() {
        return zurueck("belegt");
    }
    let mut z = d.sperre();
    if z.neustart
        || z.auftrag_stand(Art::Update).is_some()
        || z.auftrag_stand(Art::IbkrPapier).is_some()
    {
        return zurueck("belegt");
    }
    if schreibe_ganz(&datei(d), &format!("{konto}\n")).is_err() {
        return zurueck("schreibfehler");
    }
    z.ibkr_pruefung_ab_nr = z.naechste_nr;
    zurueck("gespeichert")
}

pub(crate) fn testen(d: &Dienst) -> Vec<u8> {
    if !gespeichert(d) {
        return zurueck("format");
    }
    if d.update_offen() || d.einreihen(Art::IbkrPapier, Anlass::Hand).is_err() {
        return zurueck("belegt");
    }
    umleitung("/einrichtung#ibkr-papier")
}

pub(crate) fn status(d: &Dienst) -> String {
    let mut s = crate::seiten::marke(&d.start);
    let z = d.sperre();
    if !gespeichert(d) {
        s.push_str("<p>Speichere zuerst die Nummer des Papierkontos.</p>");
        return s;
    }
    s.push_str("<p>Papierkontonummer lokal gespeichert.</p>");
    if let Some(stand) = z.auftrag_stand(Art::IbkrPapier) {
        let _ = write!(s, "<p role=\"status\">Papierabruf {stand}</p>");
        return s;
    }
    if let Some(a) = z.verlauf.iter().find(|a| {
        a.art == Art::IbkrPapier && a.frueher == Frueher::Nein && a.nr >= z.ibkr_pruefung_ab_nr
    }) {
        let (klasse, text) = if a.erfolg == Some(true) {
            ("gut", "Papierkonto lesbar.")
        } else {
            ("offen", "Papierkonto konnte noch nicht gelesen werden.")
        };
        let _ = write!(
            s,
            "<p class=\"{klasse}\" role=\"status\">{}</p>",
            maskiere(a.hinweis.as_deref().unwrap_or(text))
        );
    }
    s.push_str(&crate::seiten::auftrag_knopf(
        &d.token,
        &z,
        Art::IbkrPapier,
        "Papierabruf testen",
        true,
    ));
    s
}

pub(crate) fn einrichtung(d: &Dienst, abfrage: &str) -> String {
    let meldung = formular(abfrage.as_bytes())
        .and_then(|f| {
            Some(match feld(&f, "ibkr")? {
                "gespeichert" => ("gut", "Gespeichert. Jetzt kannst du den Papierabruf testen."),
                "format" => ("offen", "Gib die Kontonummer aus dem Client Portal ein. Nach DU oder DUR folgen nur Ziffern."),
                "schreibfehler" => ("halt", "Die Papierkontonummer konnte nicht gespeichert werden. Prüfe, ob der DepotSchlopf-Datenordner beschreibbar ist."),
                "belegt" => ("offen", "Warte, bis der aktuelle Auftrag oder das Update beendet ist, und versuche es erneut."),
                _ => return None,
            })
        })
        .map_or_else(String::new, |(klasse, text)| {
            format!("<p class=\"{klasse}\" role=\"status\">{text}</p>")
        });
    format!(
        "<section id=\"ibkr-papier\"><h2>IBKR-Papierkonto</h2>\
         <p>Prüft auf diesem Rechner den lesenden Abruf des Papierkontos.</p>\
         <ol><li>Melde dich im <a href=\"https://www.interactivebrokers.com/sso/Login\">IBKR Client Portal</a> mit deinem normalen IBKR-Zugang an. \
         Unter <b>Einstellungen → Kontokonfiguration → Paper Trading Account</b> findest du die Kontonummer und kannst den Papierzugang einrichten.</li>\
         <li>Öffne das IBKR Gateway und wähle <b>IB API</b> und <b>Paper-Trading</b>. Melde dich dort mit dem Papierzugang an. \
         Falls es noch nicht installiert ist, <a href=\"https://ndcdyn.interactivebrokers.com/en/trading/ibgateway-latest.php\">lade das Gateway bei IBKR herunter</a>.</li>\
         <li>Prüfe im Gateway unter <b>Konfigurieren → Einstellungen → API → Einstellungen</b> den Socket-Port <b>4002</b>. \
         Aktiviere <b>Schreibgeschützte API</b> und <b>Nur Verbindungen vom lokalen Host zulassen</b>. Lass das Gateway geöffnet und warte auf <b>verbunden</b>.</li>\
         <li>Speichere unten die Kontonummer aus dem Client Portal und klicke auf <b>Papierabruf testen</b>.</li></ol>\
         {meldung}<form method=\"post\" action=\"/ibkr-papier\">\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <div class=\"formularfelder\"><label>Papierkontonummer \
         <input name=\"konto\" required autocomplete=\"off\" spellcheck=\"false\" maxlength=\"23\" pattern=\"DUR?[0-9]{{1,20}}\"></label>\
         <button type=\"submit\">Papierkontonummer speichern</button></div></form>\
         <p class=\"leise\">Die Nummer bleibt lokal. Benutzername und Passwort gibst du im Gateway ein.</p>\
         <div data-teil=\"/teil/ibkr-papier\">{}</div></section>",
        maskiere(&d.token),
        status(d)
    )
}
