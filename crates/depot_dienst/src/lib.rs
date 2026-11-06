//! `depot_dienst` – der Dienst mit lokaler Weboberflaeche.
//!
//! Er lauscht nur auf 127.0.0.1 (Kodex `NETZ_LOKAL`, Wolfgang 05.11.2026 „Ja, nur 127.0.0.1“),
//! zeigt live, was DepotSchlopf tut, und buendelt die ganze Einrichtung auf einer Seite. Er
//! startet Arbeiten als eigene Laeufe. Einzige Ausnahme ist die vor Ort bestaetigte Papierfreigabe.
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
pub mod kostenschutz;
mod papierdialog;
pub use papierdialog::DialogErgebnis;
pub mod programmstand;
pub mod rueckfall;
pub mod schluessel;
pub mod seiten;
pub mod verlauf;
pub mod waechter;
pub mod werkbank;
pub mod zeitplan;

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use depot_kern::{BeraterId, Zeitpunkt};
use depot_lage::betrieb::schluessel_stand;
use depot_lage::{
    Hinterlegt, Umgebung, anbieter, anbieterprotokoll, grenzwerte, ki_halt, lagebild, mit_journal,
    ortszeit, sperrdateien,
};

use crate::http::{Anfrage, antwort, feld, formular, gleich, herkunft_erlaubt, host_erlaubt};
use crate::schluessel::Wer;
use crate::werkbank::{Art, Dienst, Lagerechner};
use crate::zeitplan::{Zeitplan, lies_uhrzeit};

/// Was der Dienst ausserhalb des Projektordners anfasst: Schluesselspeicher und Autostart.
/// Im Betrieb `System`, in Tests eine Attrappe.
pub trait Umwelt: Send + Sync {
    fn schluessel(&self, wer: Wer) -> Hinterlegt;
    /// # Errors
    /// Ungueltig oder abgelehnt; die Meldung nennt den Schluessel nie.
    fn schluessel_ablegen(&self, wer: Wer, text: &str) -> Result<(), String>;
    fn autostart(&self, wurzel: &Path) -> bool;
    /// Verlangt eine Bestaetigung ausserhalb der lokalen Webseite.
    fn papierfreigabe_bestaetigen(
        &self,
        _angebot: &depot_lage::PapierFreigabeAngebot,
    ) -> Option<Zeitpunkt> {
        None
    }
    fn papierdialog_testen(&self) -> DialogErgebnis {
        DialogErgebnis::NichtVerfuegbar
    }

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

    fn papierfreigabe_bestaetigen(
        &self,
        angebot: &depot_lage::PapierFreigabeAngebot,
    ) -> Option<Zeitpunkt> {
        if papierdialog::bestaetigen(angebot) {
            jetzt()
        } else {
            None
        }
    }

    fn papierdialog_testen(&self) -> DialogErgebnis {
        papierdialog::testen()
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
        let (anbieter, anbieter_unlesbar) = anbieterprotokoll(&daten);
        let l = mit_journal(&daten, |journal| {
            lagebild(&Umgebung {
                jetzt,
                journal,
                kursquelle: umwelt.schluessel(Wer::Kursquelle),
                kursquelle_us: umwelt.schluessel(Wer::KursquelleUs),
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
                anbieter,
                anbieter_unlesbar,
                ki_halt: ki_halt(&daten),
            })
        });
        let html = depot_lage::html::inhalt_mit(&l, &|a| seiten::abhilfe_knopf(&token, a), false);
        (l, html)
    })
}

/// Der Projektordner: der erste Ordner ab dem aktuellen, der `kodex/festschreibung.txt` hat.
pub fn finde_wurzel() -> Option<std::path::PathBuf> {
    let start = std::env::current_dir().ok()?;
    start
        .ancestors()
        .find(|d| d.join("kodex").join("festschreibung.txt").is_file())
        .map(Path::to_path_buf)
}

/// Oeffnet die Seite im Standardbrowser (nur unter Windows) und nennt die Adresse.
pub fn oeffne_browser() {
    let adresse = format!("http://127.0.0.1:{}/", http::PORT);
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", &adresse])
            .spawn();
    }
    println!("[Dienst] Oberflaeche: {adresse}");
}

/// Aktuelle UTC-Zeit des Dienstes.
pub fn jetzt() -> Option<Zeitpunkt> {
    let s = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    Zeitpunkt::aus_sekunden(i64::try_from(s).ok()?)
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
            let inhalt = seiten::live(&d.start, &d.sperre(), tag);
            let s = seiten::rahmen("/", Some("/teil/live"), meldung, &inhalt);
            antwort(200, HTML, s.as_bytes())
        }
        "/teil/live" => antwort(
            200,
            HTML,
            seiten::live(&d.start, &d.sperre(), tag).as_bytes(),
        ),
        "/teil/status" => antwort(
            200,
            HTML,
            seiten::status(&d.start, &d.sperre(), tag).as_bytes(),
        ),
        "/einrichtung" => {
            // Erst ausserhalb der Sperre nachsehen, was gespeichert ist.
            let datei = werkbank::lies_falls_da(&d.wurzel.join("daten").join("zeitplan.txt"));
            let (protokoll, _) = anbieterprotokoll(&d.wurzel.join("daten"));
            let gespeichert = seiten::Einrichtung {
                schluessel: schluessel::ALLE
                    .into_iter()
                    .map(|w| {
                        (
                            w,
                            schluessel_stand(u.schluessel(w), &protokoll, w.protokoll()),
                        )
                    })
                    .collect(),
                ki_halt: werkbank::lies_falls_da(&kostenschutz::pfad(&d.wurzel)),
                kurse_halt: werkbank::lies_falls_da(&kostenschutz::kurse_pfad(&d.wurzel)),
                papier_freigabe: depot_lage::papier_freigabe_angebot(&d.wurzel),
                zeitplan_gespeichert: None,
                entwurf: formular(a.abfrage.as_bytes())
                    .map(|f| seiten::Entwurf::aus_formular(&f))
                    .unwrap_or_default(),
                autostart: u.autostart(&d.wurzel),
                wurzel: d.wurzel.display().to_string(),
                grenzen: grenzwerte(),
                abgelehnt: werkbank::lies_falls_da(&rueckfall::abgelehnt_pfad(&d.wurzel)).map(
                    |t| {
                        t.unwrap_or_default()
                            .lines()
                            .map(|z| rueckfall::kurz(z.trim()).to_string())
                            .filter(|z| !z.is_empty())
                            .collect()
                    },
                ),
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
            let inhalt = seiten::einrichtung(&d.token, &d.start, &zustand, tag, &gespeichert);
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

fn post(d: &Dienst, u: &dyn Umwelt, a: &Anfrage, jetzt: Zeitpunkt) -> Vec<u8> {
    if !herkunft_erlaubt(a.kopf("origin")) {
        return http::text(403, "Abgelehnt: fremde Herkunft");
    }
    let typ_passt = a.kopf("content-type").is_some_and(|typ| {
        typ.split(';').next().is_some_and(|teil| {
            teil.trim()
                .eq_ignore_ascii_case("application/x-www-form-urlencoded")
        })
    });
    if !typ_passt {
        return http::text(400, "Abgelehnt: nur Formulare");
    }
    let Some(f) = formular(&a.rumpf) else {
        return http::text(400, "Abgelehnt: Formular unlesbar");
    };
    if !feld(&f, "t").is_some_and(|token| gleich(token, &d.token)) {
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
            let update = matches!(art, Art::Update | Art::UpdatePruefen);
            match (d.von_hand(art, jetzt), update) {
                (Ok(()), false) => http::umleitung("/?m=auftrag"),
                (Err(_), false) => http::umleitung("/?m=auftrag_belegt"),
                (Ok(()), true) => http::umleitung("/einrichtung?m=update#update"),
                (Err(_), true) => http::umleitung("/einrichtung?m=update_belegt#update"),
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
                    // Der neue Schluessel ist nur hinterlegt, bis ein Aufruf ihn bestaetigt.
                    let neu = anbieter::Eintrag::neu(
                        jetzt,
                        wer.protokoll(),
                        anbieter::Ergebnis::Neu,
                        None,
                    );
                    let datei = d.wurzel.join("daten").join(anbieter::DATEI);
                    let vermerkt = anbieter::anhaengen(&datei, &neu).is_ok();
                    d.sperre().lage_veraltet = true;
                    if vermerkt {
                        http::umleitung("/einrichtung?m=schluessel#schluessel")
                    } else {
                        http::umleitung("/einrichtung?m=schluessel_protokoll#schluessel")
                    }
                }
                Err(_) => http::umleitung("/einrichtung?m=schluessel_fehler#schluessel"),
            }
        }
        "/zeitplan" => {
            // Bei jeder Ablehnung bleiben die gueltigen Eingaben im Formular stehen.
            let zurueck = |m: &str| {
                http::umleitung(&format!(
                    "/einrichtung?m={m}{}#zeitplan",
                    seiten::Entwurf::aus_formular(&f).als_abfrage()
                ))
            };
            let Some(plan) = zeitplan_aus_formular(&f) else {
                return zurueck("zeitplan_ungueltig");
            };
            match d.setze_zeitplan(plan) {
                Ok(()) => http::umleitung("/einrichtung?m=zeitplan#zeitplan"),
                Err(werkbank::ZeitplanAbgelehnt::Einrichtung(_)) => zurueck("zeitplan_einrichtung"),
                Err(werkbank::ZeitplanAbgelehnt::LageFehlt) => zurueck("zeitplan_lage"),
                Err(werkbank::ZeitplanAbgelehnt::Datei(_)) => zurueck("zeitplan_fehler"),
            }
        }
        "/kostenschutz" => kostenschutz_aufheben(d, &f, jetzt, Sperre::Kostenschutz),
        "/kurssperre" => kostenschutz_aufheben(d, &f, jetzt, Sperre::Kurse),
        "/papierfreigabe" => papier_freigeben(d, u, &f),
        "/papierdialog-test" => papierdialog_testen(d, u),
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

fn papierdialog_testen(d: &Dienst, u: &dyn Umwelt) -> Vec<u8> {
    let meldung = if let Ok(_dialog) = d.papierdialog.try_lock() {
        match u.papierdialog_testen() {
            DialogErgebnis::Ja => "dialogtest_ja",
            DialogErgebnis::Nein => "dialogtest_nein",
            DialogErgebnis::ZeitAbgelaufen => "dialogtest_zeit",
            DialogErgebnis::NichtVerfuegbar => "dialogtest_fehler",
        }
    } else {
        "dialogtest_belegt"
    };
    http::umleitung(&format!("/einrichtung?m={meldung}#papierdialog-test"))
}

fn papier_freigeben(d: &Dienst, u: &dyn Umwelt, f: &[(String, String)]) -> Vec<u8> {
    let zurueck = |code| http::umleitung(&format!("/einrichtung?m={code}#papierfreigabe"));
    if feld(f, "geprueft") != Some("ja") {
        return zurueck("papierfreigabe_unbestaetigt");
    }
    let Some(gesehen) =
        feld(f, "abdruck").filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    else {
        return http::text(400, "Abgelehnt: Journalabdruck fehlt oder ist ungueltig");
    };
    let Ok(_dialog) = d.papierdialog.try_lock() else {
        return zurueck("papierfreigabe_belegt");
    };
    let Some(angebot) =
        depot_lage::papier_freigabe_angebot(&d.wurzel).filter(|angebot| angebot.abdruck == gesehen)
    else {
        return zurueck("papierfreigabe_geaendert");
    };
    let Some(bestaetigt_am) = u.papierfreigabe_bestaetigen(&angebot) else {
        return zurueck("papierfreigabe_unbestaetigt");
    };
    zurueck(d.papier_freigeben(gesehen, bestaetigt_am))
}

/// Welche Sperrdatei ein Formular aufhebt.
#[derive(Clone, Copy)]
enum Sperre {
    Kostenschutz,
    Kurse,
}

/// Hebt eine Sperre gefuehrt auf (siehe `kostenschutz`): die des Kostenschutzes oder die
/// Kurssperre.
fn kostenschutz_aufheben(
    d: &Dienst,
    f: &[(String, String)],
    jetzt: Zeitpunkt,
    sperre: Sperre,
) -> Vec<u8> {
    let (name, anker) = match sperre {
        Sperre::Kostenschutz => ("kostenschutz", "kostenschutz"),
        Sperre::Kurse => ("kurssperre", "kurssperre"),
    };
    let ziel = |m: &str| http::umleitung(&format!("/einrichtung?m={name}_{m}#{anker}"));
    if feld(f, "geprueft") != Some("ja") {
        return ziel("unbestaetigt");
    }
    let Some(gesehen) = feld(f, "abdruck") else {
        return http::text(400, "Abgelehnt: Abdruck fehlt");
    };
    // Unter der Sperre des Zustands: Waehrenddessen startet kein Auftrag.
    let mut z = d.sperre();
    if z.laufend.is_some() || !z.warteschlange.is_empty() {
        return ziel("belegt");
    }
    let ausgang = match sperre {
        Sperre::Kostenschutz => kostenschutz::aufheben(&d.wurzel, gesehen, jetzt),
        Sperre::Kurse => kostenschutz::kurse_aufheben(&d.wurzel, gesehen, jetzt),
    };
    match ausgang {
        Ok(kostenschutz::Aufheben::Aufgehoben) => {
            z.lage_veraltet = true;
            ziel("aufgehoben")
        }
        Ok(kostenschutz::Aufheben::Frei) => ziel("frei"),
        Ok(kostenschutz::Aufheben::Geaendert) => ziel("geaendert"),
        Err(_) => ziel("fehler"),
    }
}

/// Beantwortet eine gelesene Anfrage. Erst der Host, dann je nach Methode.
pub fn behandle(d: &Dienst, u: &dyn Umwelt, a: &Anfrage, jetzt: Zeitpunkt) -> Vec<u8> {
    if !host_erlaubt(a.kopf("host")) {
        return http::text(403, "Abgelehnt: fremder Host");
    }
    match a.methode.as_str() {
        "GET" => get(d, u, a, jetzt),
        "POST" => post(d, u, a, jetzt),
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
