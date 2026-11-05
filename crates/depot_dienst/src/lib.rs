//! `depot_dienst` – der Dienst mit lokaler Weboberflaeche.
//!
//! Er lauscht nur auf 127.0.0.1 (Kodex `NETZ_LOKAL`, Wolfgang 05.11.2026 „Ja, nur 127.0.0.1“),
//! zeigt live, was DepotSchlopf tut, und buendelt die ganze Einrichtung auf einer Seite. Er
//! schreibt nie ins Journal: Jede Arbeit ist ein eigener Lauf wie `run.cmd kurse` (Werkbank).
//!
//! Schutz gegen fremde Webseiten: siehe `http`. Jede Anfrage braucht den eigenen Host; jede
//! aendernde Anfrage (POST) die eigene Herkunft und den Zufallswert dieses Dienststarts.

#![forbid(
    unsafe_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::float_arithmetic,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

pub mod autostart;
pub mod http;
pub mod schluessel;
pub mod seiten;
pub mod werkbank;
pub mod zeitplan;

use std::path::Path;

use depot_kern::{BeraterId, Zeitpunkt};
use depot_lage::{Hinterlegt, Umgebung, grenzwerte, lagebild, mit_journal, ortszeit, sperrdateien};

use crate::http::{Anfrage, antwort, feld, formular, gleich, herkunft_erlaubt, host_erlaubt};
use crate::schluessel::Wer;
use crate::werkbank::{Anlass, Art, Dienst, Lagerechner};
use crate::zeitplan::{Zeitplan, lies_uhrzeit};

/// Was der Dienst ausserhalb des Projektordners anfasst: Schluesselspeicher und Autostart.
/// Im Betrieb `System`, in Tests eine Attrappe.
pub trait Umwelt: Send + Sync {
    fn schluessel(&self, wer: Wer) -> Hinterlegt;
    /// # Errors
    /// Ungueltig oder abgelehnt; die Meldung nennt den Schluessel nie.
    fn schluessel_ablegen(&self, wer: Wer, text: &str) -> Result<(), String>;
    fn autostart(&self, wurzel: &Path) -> bool;
    /// # Errors
    /// Ordner oder Pfad ungeeignet, Schreibfehler.
    fn autostart_setzen(&self, wurzel: &Path, an: bool) -> Result<(), String>;
}

/// Die echte Umwelt: Windows-Anmeldeinformationsverwaltung und Autostart-Ordner.
#[derive(Clone, Copy, Debug, Default)]
pub struct System;

impl Umwelt for System {
    fn schluessel(&self, wer: Wer) -> Hinterlegt {
        schluessel::status(wer)
    }

    fn schluessel_ablegen(&self, wer: Wer, text: &str) -> Result<(), String> {
        schluessel::ablegen(wer, text)
    }

    fn autostart(&self, wurzel: &Path) -> bool {
        autostart::an(wurzel)
    }

    fn autostart_setzen(&self, wurzel: &Path, an: bool) -> Result<(), String> {
        if an {
            autostart::einrichten(wurzel)
        } else {
            autostart::entfernen()
        }
    }
}

/// Das Lagebild fuer den Dienst: dieselbe Rechnung wie `run.cmd lage`, dazu der Stand der
/// Schluessel und Knoepfe, die Auftraege einreihen.
pub fn lagerechner(token: String, umwelt: std::sync::Arc<dyn Umwelt>) -> Lagerechner {
    Box::new(move |wurzel: &Path, jetzt: Zeitpunkt| {
        let daten = wurzel.join("daten");
        let sperren = sperrdateien(&daten);
        let l = mit_journal(&daten, |journal| {
            lagebild(&Umgebung {
                jetzt,
                journal,
                kursquelle: umwelt.schluessel(Wer::Kursquelle),
                berater: [
                    (
                        BeraterId::Dachs,
                        umwelt.schluessel(Wer::Berater(BeraterId::Dachs)),
                    ),
                    (
                        BeraterId::Maus,
                        umwelt.schluessel(Wer::Berater(BeraterId::Maus)),
                    ),
                ],
                sperren,
            })
        });
        let html = depot_lage::html::inhalt(&l, &|a| seiten::abhilfe_knopf(&token, a));
        (l, html)
    })
}

fn heute(jetzt: Zeitpunkt) -> Option<depot_kern::Datum> {
    ortszeit(jetzt).map(|(d, ..)| d)
}

const HTML: &str = "text/html; charset=utf-8";

fn get(d: &Dienst, u: &dyn Umwelt, a: &Anfrage, jetzt: Zeitpunkt) -> Vec<u8> {
    let tag = heute(jetzt);
    let meldung = seiten::meldung(&a.abfrage);
    match a.pfad.as_str() {
        "/" => {
            let inhalt = seiten::live(&d.sperre(), tag);
            let s = seiten::rahmen("/", Some("/teil/live"), meldung, &inhalt);
            antwort(200, HTML, s.as_bytes())
        }
        "/teil/live" => antwort(200, HTML, seiten::live(&d.sperre(), tag).as_bytes()),
        "/teil/status" => antwort(200, HTML, seiten::status(&d.sperre(), tag).as_bytes()),
        "/einrichtung" => {
            // Erst ausserhalb der Sperre nachsehen, was gespeichert ist.
            let datei = werkbank::lies_falls_da(&d.wurzel.join("daten").join("zeitplan.txt"));
            let gespeichert = seiten::Einrichtung {
                schluessel: schluessel::ALLE
                    .into_iter()
                    .map(|w| (w, u.schluessel(w)))
                    .collect(),
                zeitplan_gespeichert: None,
                autostart: u.autostart(&d.wurzel),
                wurzel: d.wurzel.display().to_string(),
                grenzen: grenzwerte(),
            };
            let zustand = d.sperre();
            let gespeichert = seiten::Einrichtung {
                zeitplan_gespeichert: match datei {
                    Ok(Some(t)) => Some(t == zustand.zeitplan.als_text()),
                    Ok(None) => None,
                    Err(_) => Some(false),
                },
                ..gespeichert
            };
            let inhalt = seiten::einrichtung(&d.token, &zustand, tag, &gespeichert);
            drop(zustand);
            let seite = seiten::rahmen("/einrichtung", None, meldung, &inhalt);
            antwort(200, HTML, seite.as_bytes())
        }
        "/app.js" => antwort(
            200,
            "text/javascript; charset=utf-8",
            seiten::APP_JS.as_bytes(),
        ),
        _ => http::text(404, "Nicht gefunden"),
    }
}

fn zeitplan_aus_formular(f: &[(String, String)]) -> Option<Zeitplan> {
    let haken = |n: &str| match feld(f, n) {
        None => Some(false),
        Some("ja") => Some(true),
        Some(_) => None,
    };
    Some(Zeitplan {
        an: haken("an")?,
        termine: lies_uhrzeit(feld(f, "termine")?)?,
        kurse: lies_uhrzeit(feld(f, "kurse")?)?,
        papier_danach: haken("papier_danach")?,
    })
}

fn post(d: &Dienst, u: &dyn Umwelt, a: &Anfrage) -> Vec<u8> {
    if !herkunft_erlaubt(a.kopf("origin")) {
        return http::text(403, "Abgelehnt: fremde Herkunft");
    }
    let typ_passt = a.kopf("content-type").is_some_and(|t| {
        t.split(';').next().is_some_and(|t| {
            t.trim()
                .eq_ignore_ascii_case("application/x-www-form-urlencoded")
        })
    });
    if !typ_passt {
        return http::text(400, "Abgelehnt: nur Formulare");
    }
    let Some(f) = formular(&a.rumpf) else {
        return http::text(400, "Abgelehnt: Formular unlesbar");
    };
    if !feld(&f, "t").is_some_and(|t| gleich(t, &d.token)) {
        return http::text(
            403,
            "Abgelehnt: Seite veraltet oder fremd. Bitte neu laden.",
        );
    }
    match a.pfad.as_str() {
        "/ausfuehren" => {
            let Some(art) = feld(&f, "art").and_then(Art::aus_kurz) else {
                return http::text(400, "Abgelehnt: Auftrag unbekannt");
            };
            match d.einreihen(art, Anlass::Hand) {
                Ok(()) => http::umleitung("/?m=auftrag"),
                Err(_) => http::umleitung("/?m=auftrag_belegt"),
            }
        }
        "/schluessel" => {
            let (Some(wer), Some(wert)) =
                (feld(&f, "wer").and_then(Wer::aus_kurz), feld(&f, "wert"))
            else {
                return http::text(400, "Abgelehnt: Schluessel unbekannt");
            };
            if wer.pruefe(wert).is_err() {
                return http::umleitung("/einrichtung?m=schluessel_ungueltig#schluessel");
            }
            match u.schluessel_ablegen(wer, wert) {
                Ok(()) => {
                    d.sperre().lage_veraltet = true;
                    http::umleitung("/einrichtung?m=schluessel#schluessel")
                }
                Err(_) => http::umleitung("/einrichtung?m=schluessel_fehler#schluessel"),
            }
        }
        "/zeitplan" => {
            let Some(plan) = zeitplan_aus_formular(&f) else {
                return http::umleitung("/einrichtung?m=zeitplan_ungueltig#zeitplan");
            };
            match d.setze_zeitplan(plan) {
                Ok(()) => http::umleitung("/einrichtung?m=zeitplan#zeitplan"),
                Err(_) => http::umleitung("/einrichtung?m=zeitplan_fehler#zeitplan"),
            }
        }
        "/autostart" => {
            let an = match feld(&f, "an") {
                Some("an") => true,
                Some("aus") => false,
                _ => return http::text(400, "Abgelehnt: an oder aus erwartet"),
            };
            match u.autostart_setzen(&d.wurzel, an) {
                Ok(()) if an => http::umleitung("/einrichtung?m=autostart_an#autostart"),
                Ok(()) => http::umleitung("/einrichtung?m=autostart_aus#autostart"),
                Err(_) => http::umleitung("/einrichtung?m=autostart_fehler#autostart"),
            }
        }
        _ => http::text(404, "Nicht gefunden"),
    }
}

/// Beantwortet eine gelesene Anfrage. Erst der Host, dann je nach Methode.
pub fn behandle(d: &Dienst, u: &dyn Umwelt, a: &Anfrage, jetzt: Zeitpunkt) -> Vec<u8> {
    if !host_erlaubt(a.kopf("host")) {
        return http::text(403, "Abgelehnt: fremder Host");
    }
    match a.methode.as_str() {
        "GET" => get(d, u, a, jetzt),
        "POST" => post(d, u, a),
        _ => http::text(405, "Nur GET und POST"),
    }
}

/// Ein Zufallswert fuer diesen Dienststart, als Hex. Quelle ist `RandomState`: Die Standard-
/// bibliothek saet ihn mit 128 Bit aus dem Zufall des Betriebssystems.
pub fn neues_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::fmt::Write as _;
    use std::hash::BuildHasher;

    let mut t = String::with_capacity(64);
    for i in 0_u8..4 {
        let w = RandomState::new().hash_one((i, std::process::id()));
        let _ = write!(t, "{w:016x}");
    }
    t
}
