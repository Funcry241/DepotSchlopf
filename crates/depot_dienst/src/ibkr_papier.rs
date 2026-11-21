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

pub(crate) fn konto(d: &Dienst) -> Option<String> {
    lies_falls_da(&datei(d))
        .ok()
        .flatten()
        .map(|wert| wert.trim().to_owned())
        .filter(|wert| kontonummer_gueltig(wert))
}

pub(crate) fn gespeichert(d: &Dienst) -> bool {
    konto(d).is_some()
}

pub(crate) fn einstiegslink(d: &Dienst) -> String {
    let mut s = crate::seiten::marke(&d.start);
    let text = match konto(d) {
        None => "IBKR-Papierkonto einrichten",
        Some(konto) => {
            let erfolgreich = depot_journal::ibkr_abruf::lesen(&d.wurzel, &konto)
                .ok()
                .flatten()
                .is_some_and(|stand| stand.erfolg.is_some() && !stand.fehlgeschlagen);
            if erfolgreich {
                return s;
            }
            "IBKR-Papierabruf testen"
        }
    };
    let _ = write!(s, "<p><a href=\"#ibkr-papier\">{text}</a></p>");
    s
}

pub(crate) fn depot(d: &Dienst) -> String {
    use depot_lage::zeit_text;

    let mut s = crate::seiten::marke(&d.start);
    s.push_str("<section id=\"ibkr-depot\"><h2>IBKR-Papierkonto</h2>");
    let stand = konto(d)
        .map(|konto| depot_journal::ibkr_abruf::lesen(&d.wurzel, &konto))
        .transpose()
        .map(Option::flatten);
    match stand {
        Ok(Some(stand)) => {
            if let Some((zeit, betrag)) = stand.erfolg {
                let _ = write!(
                    s,
                    "<p>Bargeld <strong class=\"gross\">{betrag}</strong></p>\
                     <p class=\"leise\">Letzter erfolgreicher Papierabruf {}</p>",
                    zeit_text(zeit)
                );
            } else {
                s.push_str("<p>Noch kein erfolgreicher Papierabruf.</p>");
            }
            if stand.fehlgeschlagen {
                s.push_str("<p class=\"offen\">Der jüngste Papierabruf ist fehlgeschlagen.</p>");
            }
        }
        Ok(None) => s.push_str("<p>Noch kein Papierabruf.</p>"),
        Err(_) => {
            s.push_str("<p class=\"offen\">Der gespeicherte Papierabrufstand ist unlesbar.</p>");
        }
    }
    s.push_str("<p><a href=\"/einrichtung#ibkr-papier\">Zum Papierabruf in den Einstellungen</a></p></section>");
    s
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
        || [Art::IbkrPapier, Art::IbkrJournal, Art::IbkrAbgleich]
            .into_iter()
            .any(|art| z.auftrag_stand(art).is_some())
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
    s.push_str("<p class=\"gut\">✓ Papierkontonummer lokal gespeichert</p>");
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
        let diagnose = a
            .protokoll
            .iter()
            .filter_map(|zeile| zeile.strip_prefix("[IBKR-Diagnose] "))
            .collect::<Vec<_>>()
            .join("\n");
        if !diagnose.is_empty() {
            let _ = write!(
                s,
                "<details id=\"ibkr-gateway-diagnose\"><summary>Gateway-Diagnose</summary>\
                 <pre class=\"konsole\">{}</pre></details>",
                maskiere(&diagnose)
            );
        }
    }
    s.push_str(
        "<p class=\"leise\">Lass das IBKR Gateway im Paper-Trading geöffnet. Empfohlen ist ein \
         automatischer Neustart um <b>03:00 Uhr Ortszeit</b>. Nach dem wöchentlichen IBKR-Reset \
         am Wochenende meldest du dich wieder von Hand an.</p>",
    );
    s.push_str(&crate::seiten::auftrag_knopf(
        &d.token,
        &z,
        Art::IbkrPapier,
        "Papierabruf testen",
        true,
    ));
    s
}

pub(crate) fn einrichtung(d: &Dienst, abfrage: &str, konto_gespeichert: bool) -> String {
    let felder = formular(abfrage.as_bytes());
    let kennung = felder.as_ref().and_then(|f| feld(f, "ibkr"));
    let meldung = match kennung {
        Some("format") => Some(("offen", "Gib die Kontonummer aus dem Client Portal ein. Nach DU oder DUR folgen nur Ziffern.")),
        Some("schreibfehler") => Some(("halt", "Die Papierkontonummer konnte nicht gespeichert werden. Prüfe, ob der DepotSchlopf-Datenordner beschreibbar ist.")),
        Some("belegt") => Some(("offen", "Warte, bis der aktuelle Auftrag oder das Update beendet ist, und versuche es erneut.")),
        _ => None,
    }
    .map_or_else(String::new, |(klasse, text)| {
        format!("<p class=\"{klasse}\" role=\"status\">{text}</p>")
    });
    let hilfe = "<ol><li>Melde dich im <a href=\"https://www.interactivebrokers.com/sso/Login\">IBKR Client Portal</a> mit deinem normalen IBKR-Zugang an. \
         Unter <b>Einstellungen → Kontokonfiguration → Paper Trading Account</b> findest du die Kontonummer und kannst den Papierzugang einrichten.</li>\
         <li>Öffne das IBKR Gateway und wähle <b>IB API</b> und <b>Paper-Trading</b>. Melde dich dort mit dem Papierzugang an. \
         Falls es noch nicht installiert ist, <a href=\"https://ndcdyn.interactivebrokers.com/en/trading/ibgateway-latest.php\">lade das Gateway bei IBKR herunter</a>.</li>\
         <li>Prüfe im Gateway unter <b>Konfigurieren → Einstellungen → API → Einstellungen</b> den Socket-Port <b>4002</b>. \
         Aktiviere <b>Schreibgeschützte API</b> und <b>Nur Verbindungen vom lokalen Host zulassen</b>. Lass das Gateway geöffnet und warte auf <b>verbunden</b>.</li>\
         <li>Wähle im Gateway <b>Konfigurieren → Einstellungen → Sperren und schließen → Automatischer Neustart</b>. \
         Stelle <b>03:00 Uhr Ortszeit</b> ein. Im 12-Stunden-Format wählst du <b>03:00 AM</b>. Klicke auf <b>Übernehmen</b>.</li>\
         <li>Für den Journalstart sind <b>1.000 €</b> vorgesehen. Öffne bei abweichendem Bargeldbestand im Client Portal den <b>Paper-Modus</b> und suche in den <b>Einstellungen</b> nach <b>Paper Trading Account Reset</b>. Prüfe dort, ob IBKR 1.000 € in EUR anbietet.</li></ol>";
    let formular = format!(
        "<form method=\"post\" action=\"/ibkr-papier\" data-aenderungen>\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <div class=\"formularfelder\"><label>Papierkontonummer \
         <input name=\"konto\" required autocomplete=\"off\" spellcheck=\"false\" maxlength=\"23\" pattern=\"DUR?[0-9]{{1,20}}\"></label>\
         <button type=\"submit\" data-speichern-hervorheben>Papierkontonummer speichern</button></div></form>\
         <p class=\"leise\">Die Nummer bleibt lokal. Benutzername und Passwort gibst du im Gateway ein.</p>",
        maskiere(&d.token),
    );
    let status = format!("<div data-teil=\"/teil/ibkr-papier\">{}</div>", status(d));
    let inhalt = if konto_gespeichert {
        let offen = if meldung.is_empty() { "" } else { " open" };
        format!(
            "{status}<details{offen}><summary>Papierkontonummer ändern</summary>{meldung}{formular}</details>\
             <details><summary>Hilfe zur Einrichtung</summary>{hilfe}</details>"
        )
    } else {
        format!("{hilfe}{meldung}{formular}{status}")
    };
    let abgleich = if konto_gespeichert {
        crate::ibkr_abgleich::einrichtung(d, felder.as_deref().unwrap_or_default())
    } else {
        String::new()
    };
    format!("<section id=\"ibkr-papier\"><h2>IBKR-Papierkonto</h2>{inhalt}{abgleich}</section>")
}
