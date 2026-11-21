use depot_journal::{Journal, KiStand, KiZweck, ki_einstellungen};
use depot_kern::gemini::TAGESGRENZE;
use depot_kern::modelle::{
    BeraterEinstellung, KiEinstellungen, MAX_BERATER_AUFRUFE, Modell, Modellwahl, NeuroEmpfaenger,
};
use depot_kern::{BeraterId, Cent, Datum, Preis};
use depot_lage::html::maskiere;
use depot_lage::ki_kostenschaetzung::{AufrufkostenSchaetzung, schaetze};
use std::fmt::Write;

use crate::http::{feld, umleitung};
use crate::werkbank::{Dienst, schreibe_ganz};
use crate::{gemini, seiten};

const KEINE_SCHAETZUNG: &str = "Noch keine belastbare Schätzung";
const TAGE: [u64; 3] = [1, 30, 365];
const BERATUNG: &[KiZweck] = &[
    KiZweck::Erstmeinung,
    KiZweck::Zweitmeinung,
    KiZweck::Abstimmung,
    KiZweck::Vorbereitung,
    KiZweck::Simulation,
    KiZweck::Wochenrueckschau,
];

type Schaetzung = Option<(AufrufkostenSchaetzung, bool)>;

#[derive(Default)]
struct Schaubild {
    dachs: Schaetzung,
    maus: Schaetzung,
    neuro: NeuroKosten,
}

#[derive(Default)]
struct NeuroKosten {
    heute: Option<usize>,
    dachs: Option<AufrufkostenSchaetzung>,
    maus: Option<AufrufkostenSchaetzung>,
}

impl NeuroKosten {
    fn projektion(&self, e: KiEinstellungen, aufrufe: u64) -> Option<Cent> {
        if aufrufe == 0 || e.neuro_berater().is_empty() {
            return Some(Cent::NULL);
        }
        match e.neuro_empfaenger {
            NeuroEmpfaenger::Aus => Some(Cent::NULL),
            NeuroEmpfaenger::Dachs => self.dachs?.fuer_aufrufe(aufrufe),
            NeuroEmpfaenger::Maus => self.maus?.fuer_aufrufe(aufrufe),
            NeuroEmpfaenger::Beide => {
                let maus = aufrufe.div_ceil(2);
                let dachs = aufrufe.checked_div(2)?;
                self.maus?
                    .fuer_aufrufe(maus)?
                    .checked_add(self.dachs?.fuer_aufrufe(dachs)?)
            }
        }
    }
}

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
    let neuro_empfaenger = feld(formular, "neuro-empfaenger").map(NeuroEmpfaenger::lies);
    if matches!(neuro_empfaenger, Some(None)) {
        return seiten::fehler(400, "Die Empfängerauswahl für NeuroSchlopf ist ungültig.");
    }
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
        neuro_empfaenger: neuro_empfaenger.flatten().unwrap_or_else(|| {
            ki_einstellungen::lesen(&daten)
                .unwrap_or_default()
                .neuro_empfaenger
        }),
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
    stand: &KiStand,
    anbieter: &str,
    auswahl: Modellwahl,
) -> Option<(AufrufkostenSchaetzung, bool)> {
    let modell = auswahl.modell();
    let wer = modell.berater();
    let vergleich = |tiefe| schaetze(stand, wer, anbieter, modell.kennung(), tiefe, BERATUNG);
    vergleich(Some(auswahl.denktiefe()))
        .map(|wert| (wert, false))
        .or_else(|| {
            (auswahl == Modellwahl::standard(wer))
                .then(|| vergleich(None).map(|wert| (wert, true)))
                .flatten()
        })
}

fn auswerten(stand: &KiStand, e: KiEinstellungen, tag: Option<Datum>) -> Schaubild {
    let neuro_schaetzung = |wer, anbieter, auswahl: Modellwahl| {
        schaetze(
            stand,
            wer,
            anbieter,
            auswahl.modell().kennung(),
            Some(auswahl.denktiefe()),
            &[KiZweck::Neurofrage],
        )
    };
    Schaubild {
        dachs: schaetzung(stand, "anthropic", e.dachs.auswahl),
        maus: schaetzung(stand, "openai", e.maus.auswahl),
        neuro: NeuroKosten {
            heute: tag.map(|tag| {
                stand
                    .neurofragen()
                    .iter()
                    .filter(|v| v.datum == tag)
                    .count()
            }),
            dachs: neuro_schaetzung(BeraterId::Dachs, "anthropic", e.dachs.auswahl),
            maus: neuro_schaetzung(BeraterId::Maus, "openai", e.maus.auswahl),
        },
    }
}

fn zahlenfeld(s: &mut String, id: &str, name: &str, anzahl: usize, maximum: usize) {
    let _ = write!(
        s,
        "<input aria-label=\"{name} Aufrufe pro Tag\" type=\"number\" name=\"{id}\" min=\"0\" max=\"{maximum}\" step=\"1\" value=\"{anzahl}\" required>"
    );
}

fn geldzeile(s: &mut String, aufrufe: usize, kosten: impl Fn(u64) -> Option<Cent>) {
    if aufrufe == 0 {
        s.push_str("<td>0 €</td><td>0 €</td><td>0 €</td>");
    } else {
        let werte = TAGE.map(|tage| {
            let gesamt = u64::try_from(aufrufe).ok()?.checked_mul(tage)?;
            Preis::aus_cent(kosten(gesamt)?)
        });
        if let [Some(tag), Some(monat), Some(jahr)] = werte {
            let _ = write!(
                s,
                "<td>{} €</td><td>{} €</td><td>{} €</td>",
                tag.euro_text(),
                monat.euro_text(),
                jahr.euro_text()
            );
        } else {
            let _ = write!(s, "<td colspan=\"3\">{KEINE_SCHAETZUNG}</td>");
        }
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
    geldzeile(s, p.max_aufrufe, |n| schaetzung?.0.fuer_aufrufe(n));
    s.push_str("</tr>");
}

fn neurozeile(s: &mut String, e: KiEinstellungen, neuro: &NeuroKosten) {
    s.push_str(
        "<tr><th scope=\"row\">NeuroSchlopf</th><td>Lokaler Vergleich · 0 €<br>\
         <label>Antwortet auf externe Fragen <select name=\"neuro-empfaenger\">",
    );
    for wahl in NeuroEmpfaenger::ALLE {
        let gewaehlt = if wahl == e.neuro_empfaenger {
            " selected"
        } else {
            ""
        };
        let _ = write!(
            s,
            "<option value=\"{}\"{gewaehlt}>{}</option>",
            wahl.kennung(),
            wahl.name()
        );
    }
    s.push_str(
        "</select></label><br><small>Ein Aufruf je Empfänger. Rückfragen zählen ebenfalls.</small></td><td>",
    );
    zahlenfeld(
        s,
        "neuro-aufrufe",
        "NeuroSchlopf",
        e.neuroschlopf,
        ki_einstellungen::MAX_NEURO_AUFRUFE,
    );
    if let Some(heute) = neuro.heute {
        let _ = write!(
            s,
            "<br><small>Heute {heute} von {} reserviert</small>",
            e.neuroschlopf
        );
    }
    s.push_str("</td>");
    let externe_aufrufe = if e.neuro_berater().is_empty() {
        0
    } else {
        e.neuroschlopf
    };
    geldzeile(s, externe_aufrufe, |n| neuro.projektion(e, n));
    s.push_str("</tr>");
}

pub(crate) fn einrichtung(d: &Dienst, tag: Option<depot_kern::Datum>) -> String {
    let daten = d.wurzel.join("daten");
    let gespeichert = ki_einstellungen::lesen(&daten);
    let mut s = String::from(
        "<section id=\"ki-einstellungen\"><h2>KI-Einstellungen</h2>\
         <p>Die Kosten sind eine Schätzung für die gespeicherten Einstellungen bei voller Nutzung. \
         Ein Monat zählt 30 Kalendertage, ein Jahr 365. Bei beiden Empfängern rechnet NeuroSchlopf mit abwechselnden Aufrufen.</p>",
    );
    if let Err(grund) = &gespeichert {
        let _ = write!(
            s,
            "<p class=\"halt\">{}. Neue Aufrufe sind pausiert. Speichere gültige Einstellungen.</p>",
            maskiere(grund)
        );
    }
    let e = gespeichert.as_ref().copied().unwrap_or_default();
    let auswertung = if gespeichert.is_ok() {
        Journal::pruefe_im_hintergrund(
            &daten.join("journal.txt"),
            32 * 1024 * 1024,
            &mut || Ok(()),
            |stand, _, _, _, _| auswerten(stand.ki(), e, tag),
        )
        .unwrap_or_default()
    } else {
        Schaubild::default()
    };
    let token = maskiere(&d.token);
    let _ = write!(
        s,
        "<form method=\"post\" action=\"/ki-einstellungen\" data-aenderungen>\
         <input type=\"hidden\" name=\"t\" value=\"{token}\">\
         <table><thead><tr><th scope=\"col\">KI</th><th scope=\"col\">Modell, Denktiefe oder Empfänger</th>\
         <th scope=\"col\">Aufrufe pro Tag</th><th scope=\"col\">Kosten pro Tag</th>\
         <th scope=\"col\">pro Monat</th><th scope=\"col\">pro Jahr</th></tr></thead><tbody>"
    );
    beraterzeile(&mut s, BeraterId::Dachs, e.dachs, auswertung.dachs);
    beraterzeile(&mut s, BeraterId::Maus, e.maus, auswertung.maus);
    let gemini = gemini::budget(&daten, tag);
    let _ = write!(
        s,
        "<tr><th scope=\"row\">Gemini</th><td>{}<br>{}</td><td>",
        depot_kern::gemini::MODELL,
        gemini.kontingent_text()
    );
    zahlenfeld(&mut s, "gemini-aufrufe", "Gemini", e.gemini, TAGESGRENZE);
    s.push_str("</td><td>0 €</td><td>0 €</td><td>0 €</td></tr>");
    neurozeile(&mut s, e, &auswertung.neuro);
    s.push_str(
        "</tbody></table>\
         <p>Ein Tageslimit von 0 pausiert die jeweiligen externen Aufrufe. \
         Der Kostenschutz begrenzt Ausgaben zusätzlich. \
         Gemini kann im Gratis-Projekt weitere Anfragen ablehnen.</p>\
         <button type=\"submit\">KI-Einstellungen speichern</button>\
         <p data-speicherstand role=\"status\"></p></form></section>",
    );
    s
}
