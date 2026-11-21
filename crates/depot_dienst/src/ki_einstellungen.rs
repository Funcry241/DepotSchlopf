use depot_journal::{Journal, ki_einstellungen};
use depot_kern::gemini::TAGESGRENZE;
use depot_kern::modelle::{
    BeraterEinstellung, KiEinstellungen, MAX_BERATER_AUFRUFE, Modell, Modellwahl,
};
use depot_kern::{BeraterId, Preis};
use depot_lage::html::maskiere;
use depot_lage::ki_kostenschaetzung::{AufrufkostenSchaetzung, schaetze};
use std::fmt::Write;

use crate::http::{feld, umleitung};
use crate::werkbank::{Dienst, schreibe_ganz};
use crate::{gemini, seiten};

const KEINE_SCHAETZUNG: &str = "Noch keine belastbare Schätzung";
const TAGE: [u64; 3] = [1, 30, 365];

pub(crate) fn speichern(d: &Dienst, formular: &[(String, String)]) -> Vec<u8> {
    let profil = |wer, modell, aufrufe| {
        ki_einstellungen::profil(wer, feld(formular, modell)?, feld(formular, aufrufe)?)
    };
    let limit = |name, maximum| ki_einstellungen::tagesgrenze(feld(formular, name)?, maximum);
    let (Some(dachs), Some(maus), Some(gemini), Some(neuroschlopf)) = (
        profil(BeraterId::Dachs, "dachs-modell", "dachs-aufrufe"),
        profil(BeraterId::Maus, "maus-modell", "maus-aufrufe"),
        limit("gemini-aufrufe", TAGESGRENZE),
        limit("neuro-aufrufe", ki_einstellungen::MAX_NEURO_AUFRUFE),
    ) else {
        return seiten::fehler(400, "Modell, Denktiefe oder Tageslimit ist ungültig.");
    };
    let mut zustand = d.sperre();
    if zustand.neustart || zustand.auftrag_stand(crate::Art::Update).is_some() || d.update_offen() {
        return seiten::fehler(409, "Speichere die KI-Einstellungen nach dem Update.");
    }
    let daten = d.wurzel.join("daten");
    let e = KiEinstellungen {
        dachs,
        maus,
        gemini,
        neuroschlopf,
    };
    if schreibe_ganz(
        &ki_einstellungen::pfad(&daten),
        &ki_einstellungen::als_text(e),
    )
    .is_err()
    {
        return seiten::fehler(500, "KI-Einstellungen konnten nicht gespeichert werden.");
    }
    zustand.lage_veraltet = true;
    umleitung("/einrichtung?m=ki_einstellungen_gespeichert#ki-einstellungen")
}

fn schaetzung(
    stand: &depot_journal::KiStand,
    anbieter: &str,
    auswahl: Modellwahl,
) -> Option<(AufrufkostenSchaetzung, bool)> {
    let modell = auswahl.modell();
    let wer = modell.berater();
    let vergleich = |tiefe| schaetze(stand, wer, anbieter, modell.kennung(), tiefe, &[]);
    vergleich(Some(auswahl.denktiefe()))
        .map(|wert| (wert, false))
        .or_else(|| {
            (auswahl == Modellwahl::standard(wer))
                .then(|| vergleich(None).map(|wert| (wert, true)))
                .flatten()
        })
}

fn zahlenfeld(s: &mut String, id: &str, name: &str, anzahl: usize, maximum: usize) {
    let _ = write!(
        s,
        "<input aria-label=\"{name} Aufrufe pro Tag\" type=\"number\" name=\"{id}\" min=\"0\" max=\"{maximum}\" step=\"1\" value=\"{anzahl}\" required>"
    );
}

fn geldzeile(s: &mut String, aufrufe: usize, schaetzung: Option<AufrufkostenSchaetzung>) {
    if aufrufe == 0 {
        s.push_str("<td>0 €</td><td>0 €</td><td>0 €</td>");
    } else if let Some(schaetzung) = schaetzung {
        for tage in TAGE {
            let wert = u64::try_from(aufrufe)
                .ok()
                .and_then(|zahl| zahl.checked_mul(tage))
                .and_then(|zahl| schaetzung.fuer_aufrufe(zahl))
                .and_then(Preis::aus_cent)
                .map_or_else(
                    || KEINE_SCHAETZUNG.into(),
                    |preis| format!("{} €", preis.euro_text()),
                );
            let _ = write!(s, "<td>{wert}</td>");
        }
    } else {
        let _ = write!(s, "<td colspan=\"3\">{KEINE_SCHAETZUNG}</td>");
    }
}

fn beraterzeile(
    s: &mut String,
    wer: BeraterId,
    p: BeraterEinstellung,
    schaetzung: Option<(AufrufkostenSchaetzung, bool)>,
) {
    let (name, kurz) = match wer {
        BeraterId::Dachs => ("Dachs", "dachs"),
        BeraterId::Maus => ("Maus", "maus"),
    };
    let _ = write!(
        s,
        "<tr><th scope=\"row\">{name}</th><td><select name=\"{kurz}-modell\" aria-label=\"{name} Modell und Denktiefe\">"
    );
    for &modell in Modell::fuer_berater(wer) {
        let _ = write!(s, "<optgroup label=\"{}\">", modell.name());
        for &tiefe in modell.stufen() {
            let gewaehlt = p.auswahl.modell() == modell && p.auswahl.denktiefe() == tiefe;
            let _ = write!(
                s,
                "<option value=\"{}:{}\"{}>{} · {}</option>",
                modell.kennung(),
                tiefe.kennung(),
                if gewaehlt { " selected" } else { "" },
                modell.name(),
                tiefe.name()
            );
        }
        s.push_str("</optgroup>");
    }
    s.push_str("</select>");
    if let Some((wert, vorlaeufig)) = schaetzung {
        let zusatz = if vorlaeufig {
            " · vorläufig, Denktiefe unbelegt"
        } else {
            ""
        };
        let _ = write!(
            s,
            "<br><small>{} abgerechnete Aufrufe{zusatz}</small>",
            wert.anzahl
        );
    }
    s.push_str("</td><td>");
    zahlenfeld(
        s,
        &format!("{kurz}-aufrufe"),
        name,
        p.max_aufrufe,
        MAX_BERATER_AUFRUFE,
    );
    s.push_str("</td>");
    geldzeile(s, p.max_aufrufe, schaetzung.map(|(wert, _)| wert));
    s.push_str("</tr>");
}

pub(crate) fn einrichtung(d: &Dienst, tag: Option<depot_kern::Datum>) -> String {
    let daten = d.wurzel.join("daten");
    let gespeichert = ki_einstellungen::lesen(&daten);
    let mut s = String::from(
        "<section id=\"ki-einstellungen\"><h2>KI-Einstellungen</h2>\
         <p>Die Kosten sind eine Schätzung für die gespeicherten Einstellungen bei voller Nutzung. \
         Ein Monat zählt 30 Kalendertage, ein Jahr 365.</p>",
    );
    if let Err(grund) = &gespeichert {
        let _ = write!(
            s,
            "<p class=\"halt\">{}. Neue Aufrufe sind pausiert. Speichere gültige Einstellungen.</p>",
            maskiere(grund)
        );
    }
    let e = gespeichert.as_ref().copied().unwrap_or_default();
    let (dachs, maus) = if gespeichert.is_ok() {
        Journal::pruefe_im_hintergrund(
            &daten.join("journal.txt"),
            32 * 1024 * 1024,
            &mut || Ok(()),
            |stand, _, _, _, _| {
                (
                    schaetzung(stand.ki(), "anthropic", e.dachs.auswahl),
                    schaetzung(stand.ki(), "openai", e.maus.auswahl),
                )
            },
        )
        .unwrap_or_default()
    } else {
        (None, None)
    };
    let token = maskiere(&d.token);
    let _ = write!(
        s,
        "<form method=\"post\" action=\"/ki-einstellungen\" data-aenderungen>\
         <input type=\"hidden\" name=\"t\" value=\"{token}\">\
         <table><thead><tr><th scope=\"col\">KI</th><th scope=\"col\">Modell und Denktiefe</th>\
         <th scope=\"col\">Aufrufe pro Tag</th><th scope=\"col\">Kosten pro Tag</th>\
         <th scope=\"col\">pro Monat</th><th scope=\"col\">pro Jahr</th></tr></thead><tbody>"
    );
    beraterzeile(&mut s, BeraterId::Dachs, e.dachs, dachs);
    beraterzeile(&mut s, BeraterId::Maus, e.maus, maus);
    let gemini = gemini::budget(&daten, tag);
    let _ = write!(
        s,
        "<tr><th scope=\"row\">Gemini</th><td>{}<br>{}</td><td>",
        depot_kern::gemini::MODELL,
        gemini.kontingent_text()
    );
    zahlenfeld(&mut s, "gemini-aufrufe", "Gemini", e.gemini, TAGESGRENZE);
    s.push_str(
        "</td><td>0 €</td><td>0 €</td><td>0 €</td></tr>\
                <tr><th scope=\"row\">NeuroSchlopf</th>\
                <td>Lokaler Vergleich · 0 €<br>Externe Fragen sind noch nicht verfügbar.</td><td>",
    );
    zahlenfeld(
        &mut s,
        "neuro-aufrufe",
        "NeuroSchlopf",
        e.neuroschlopf,
        ki_einstellungen::MAX_NEURO_AUFRUFE,
    );
    s.push_str("</td>");
    geldzeile(&mut s, e.neuroschlopf, None);
    s.push_str(
        "</tr></tbody></table>\
         <p>Ein Tageslimit von 0 pausiert die jeweiligen externen Aufrufe. \
         Der Kostenschutz begrenzt Ausgaben zusätzlich. \
         Gemini kann im Gratis-Projekt weitere Anfragen ablehnen.</p>\
         <button type=\"submit\">KI-Einstellungen speichern</button>\
         <p data-speicherstand role=\"status\"></p></form></section>",
    );
    s
}
