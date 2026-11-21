use depot_journal::{Journal, KiStand, KiZweck, ki_einstellungen};
use depot_kern::gemini::TAGESGRENZE;
use depot_kern::json::zitiere;
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
    fn projektion(&self, empfaenger: NeuroEmpfaenger, aufrufe: u64) -> Option<Cent> {
        match empfaenger {
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

fn geldwerte(aufrufe: usize, kosten: impl Fn(u64) -> Option<Cent>) -> Option<[String; 3]> {
    let [tag, monat, jahr] = TAGE.map(|tage| {
        let gesamt = u64::try_from(aufrufe).ok()?.checked_mul(tage)?;
        let cent = if gesamt == 0 {
            Cent::NULL
        } else {
            kosten(gesamt)?
        };
        if cent == Cent::NULL {
            Some("0 €".to_owned())
        } else {
            Some(format!("{} €", Preis::aus_cent(cent)?.euro_text()))
        }
    });
    Some([tag?, monat?, jahr?])
}

fn geldzeile(s: &mut String, werte: Option<[String; 3]>) {
    let unbekannt = werte.is_none();
    for wert in werte.unwrap_or_default() {
        let verborgen = if unbekannt { " hidden" } else { "" };
        let _ = write!(s, "<td data-kosten-zeit{verborgen}>{wert}</td>");
    }
    let verborgen = if unbekannt { "" } else { " hidden" };
    let _ = write!(
        s,
        "<td colspan=\"3\" data-kosten-unbekannt{verborgen}>{KEINE_SCHAETZUNG}</td>"
    );
}

// Vorschau und gespeicherte Ansicht verwenden dieselbe Rundung und Formatierung.
fn kostendaten(s: &mut String, modus: &str, maximum: usize, kosten: impl Fn(u64) -> Option<Cent>) {
    let _ = write!(s, " data-kosten-{modus}=\"[");
    for aufrufe in 0..=maximum {
        if aufrufe > 0 {
            s.push(',');
        }
        if let Some([tag, monat, jahr]) = geldwerte(aufrufe, &kosten) {
            let _ = write!(
                s,
                "[{},{},{}]",
                maskiere(&zitiere(&tag)),
                maskiere(&zitiere(&monat)),
                maskiere(&zitiere(&jahr))
            );
        } else {
            s.push_str("null");
        }
    }
    s.push_str("]\"");
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
    let kosten = |n| schaetzung?.0.fuer_aufrufe(n);
    let _ = write!(s, "<tr data-kostenlimit=\"{kurz}-aufrufe\"");
    kostendaten(s, "standard", MAX_BERATER_AUFRUFE, kosten);
    let _ = write!(
        s,
        "><th scope=\"row\">{name}</th><td><select name=\"{kurz}-modell\" aria-label=\"{name} Modell und Denktiefe\">"
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
            "<br><small data-kostenbasis>{} abgerechnete Aufrufe{zusatz}</small>",
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
    geldzeile(s, geldwerte(p.max_aufrufe, kosten));
    s.push_str("</tr>");
}

fn neurozeile(s: &mut String, e: KiEinstellungen, neuro: &NeuroKosten) {
    s.push_str("<tr data-kostenlimit=\"neuro-aufrufe\"");
    for wahl in NeuroEmpfaenger::ALLE {
        kostendaten(
            s,
            wahl.kennung(),
            ki_einstellungen::MAX_NEURO_AUFRUFE,
            |n| neuro.projektion(wahl, n),
        );
    }
    s.push_str(
        "><th scope=\"row\">NeuroSchlopf</th><td>Lokaler Vergleich · 0 €<br>\
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
    geldzeile(
        s,
        geldwerte(e.neuroschlopf, |n| neuro.projektion(e.neuro_empfaenger, n)),
    );
    s.push_str("</tr>");
}

pub(crate) fn einrichtung(d: &Dienst, tag: Option<depot_kern::Datum>) -> String {
    let daten = d.wurzel.join("daten");
    let gespeichert = ki_einstellungen::lesen(&daten);
    let mut s = String::from(
        "<section id=\"ki-einstellungen\"><h2>KI-Einstellungen</h2>\
         <p>Die Schätzung geht von voller Nutzung aus. \
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
        "<form method=\"post\" action=\"/ki-einstellungen\" data-aenderungen data-ki-einstellungen>\
         <input type=\"hidden\" name=\"t\" value=\"{token}\">\
         <p data-kostenstand role=\"status\">Kostenschätzung für gespeicherte Einstellungen.</p>\
         <table><thead><tr><th scope=\"col\">KI</th><th scope=\"col\">Modell, Denktiefe oder Empfänger</th>\
         <th scope=\"col\">Aufrufe pro Tag</th><th scope=\"col\">Kosten pro Tag</th>\
         <th scope=\"col\">pro Monat</th><th scope=\"col\">pro Jahr</th></tr></thead><tbody>"
    );
    beraterzeile(&mut s, BeraterId::Dachs, e.dachs, auswertung.dachs);
    beraterzeile(&mut s, BeraterId::Maus, e.maus, auswertung.maus);
    let gemini = gemini::budget(&daten, tag);
    let gratis = |_| Some(Cent::NULL);
    s.push_str("<tr data-kostenlimit=\"gemini-aufrufe\"");
    kostendaten(&mut s, "standard", TAGESGRENZE, gratis);
    let _ = write!(
        s,
        "><th scope=\"row\">Gemini</th><td>{}<br>{}</td><td>",
        depot_kern::gemini::MODELL,
        gemini.kontingent_text()
    );
    zahlenfeld(&mut s, "gemini-aufrufe", "Gemini", e.gemini, TAGESGRENZE);
    s.push_str("</td>");
    geldzeile(&mut s, geldwerte(e.gemini, gratis));
    s.push_str("</tr>");
    neurozeile(&mut s, e, &auswertung.neuro);
    s.push_str(
        "</tbody></table>\
         <p>Ein Tageslimit von 0 pausiert die jeweiligen externen Aufrufe. \
         Der Kostenschutz begrenzt Ausgaben zusätzlich. \
         Gemini kann im Gratis-Projekt weitere Anfragen ablehnen.</p>\
         <button type=\"submit\" data-speichern-hervorheben>KI-Einstellungen speichern</button>\
         <p data-speicherstand role=\"status\"></p></form></section>",
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vorschau_verwendet_dieselben_belegten_kosten_und_rundet_je_zeitraum() {
        let schaetzung = AufrufkostenSchaetzung {
            anzahl: 2,
            gesamt: Cent(5),
        };
        let kosten = |n| schaetzung.fuer_aufrufe(n);
        assert_eq!(
            geldwerte(1, kosten),
            Some(["0,03 €", "0,75 €", "9,13 €"].map(str::to_owned))
        );
        let mut daten = String::new();
        kostendaten(&mut daten, "standard", 1, kosten);
        assert!(daten.contains("[&quot;0,03 €&quot;,&quot;0,75 €&quot;,&quot;9,13 €&quot;]"));
        assert_eq!(geldwerte(1, |_| None), None);
        assert_eq!(geldwerte(0, |_| None), Some(["0 €"; 3].map(str::to_owned)));
    }

    #[test]
    fn neuro_vorschau_berechnet_nur_die_gewaehlten_empfaenger() {
        let kosten = NeuroKosten {
            heute: None,
            dachs: None,
            maus: Some(AufrufkostenSchaetzung {
                anzahl: 1,
                gesamt: Cent(4),
            }),
        };
        let vorschau = |wahl| geldwerte(3, |n| kosten.projektion(wahl, n));
        assert_eq!(vorschau(NeuroEmpfaenger::Beide), None);
        assert_eq!(
            vorschau(NeuroEmpfaenger::Aus),
            Some(["0 €"; 3].map(str::to_owned))
        );
        assert_eq!(
            vorschau(NeuroEmpfaenger::Maus),
            Some(["0,12 €", "3,60 €", "43,80 €"].map(str::to_owned))
        );
    }
}
