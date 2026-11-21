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

pub mod aktualisierung;
mod analyse;
pub mod autostart;
mod belegpruefung;
mod belegurteile;
mod depotseite;
mod desktopdialog;
mod gemini;
mod gespraeche;
pub mod hinweis;
pub mod http;
mod ibkr_abgleich;
mod ibkr_papier;
mod ki_einstellungen;
pub mod kostenschutz;
mod neuro2026;
mod neuroauftraege;
mod neuroschlopf;
mod otter;
mod papierdialog;
pub use papierdialog::DialogErgebnis;
pub mod programmstand;
pub mod protokoll;
mod prueftag;
pub mod rueckfall;
pub mod schluessel;
mod schmuck;
pub mod seiten;
mod sicherung;
mod splitverdacht;
mod update_angebot;
pub mod verlauf;
pub mod waechter;
pub mod werkbank;
pub mod zeitplan;
mod zeitstrahl_ki;

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use depot_kern::{BeraterId, Zeitpunkt};
use depot_lage::betrieb::schluessel_stand;
use depot_lage::{
    Hinterlegt, Umgebung, anbieter, anbieterprotokoll, grenzwerte, ki_halt,
    lagebild_mit_ki_einstellungen, ortszeit, sperrdateien,
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
    fn autostart(&self, wurzel: &Path) -> autostart::Stand;
    /// Verlangt eine Bestaetigung ausserhalb der lokalen Webseite.
    fn papierfreigabe_bestaetigen(
        &self,
        _angebot: &depot_lage::PapierFreigabeAngebot,
    ) -> Option<Zeitpunkt> {
        None
    }
    /// Verlangt eine Bestätigung am Desktop für die manuelle Aufhebung.
    fn sperre_bestaetigen(&self, _sperre: Sperre) -> Option<Zeitpunkt> {
        None
    }

    fn papierdialog_testen(&self) -> DialogErgebnis {
        DialogErgebnis::NichtVerfuegbar
    }

    /// # Errors
    /// Der native Ordnerdialog ist nicht verfügbar oder konnte nicht geöffnet werden.
    fn sicherungsziel_waehlen(&self) -> Result<Option<String>, String> {
        Err("Der Ordnerdialog ist nur unter Windows verfügbar.".into())
    }

    /// Zeigt einen Windows-Hinweis und wartet nicht darauf. Ab Werk: nichts.
    fn hinweis(&self, _titel: &str, _text: &str) {}

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

    fn autostart(&self, wurzel: &Path) -> autostart::Stand {
        autostart::stand(wurzel)
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

    fn sperre_bestaetigen(&self, sperre: Sperre) -> Option<Zeitpunkt> {
        papierdialog::sperre_bestaetigen(sperre)
            .then(jetzt)
            .flatten()
    }

    fn papierdialog_testen(&self) -> DialogErgebnis {
        papierdialog::testen()
    }

    #[cfg(windows)]
    fn sicherungsziel_waehlen(&self) -> Result<Option<String>, String> {
        papierdialog::sicherungsziel_waehlen()
    }

    fn hinweis(&self, titel: &str, text: &str) {
        hinweis::zeige(titel, text);
    }

    fn autostart_setzen(&self, wurzel: &Path, an: bool) -> Result<(), String> {
        if an {
            autostart::einrichten(wurzel)
        } else {
            autostart::entfernen(wurzel)
        }
    }
}

/// Das Lagebild fuer den Dienst: dieselbe Rechnung wie `run.cmd lage`, dazu der Stand der
/// Schluessel und Knoepfe, die Auftraege einreihen.
pub fn lagerechner(umwelt: std::sync::Arc<dyn Umwelt>) -> Lagerechner {
    let lesecache = std::sync::Mutex::new(depot_journal::Lesecache::default());
    Box::new(move |wurzel: &Path, jetzt: Zeitpunkt| {
        let daten = wurzel.join("daten");
        let sperren = sperrdateien(&daten);
        let (anbieter, anbieter_unlesbar) = anbieterprotokoll(&daten);
        let mut cache = lesecache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut bild = depot_lage::mit_lesecache(&daten, &mut cache, |journal, eintraege| {
            let einstellungen = depot_journal::ki_einstellungen::lesen(&daten);
            let mut bild = lagebild_mit_ki_einstellungen(
                &Umgebung {
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
                },
                einstellungen.as_ref().copied().map_err(String::as_str),
            );
            if let depot_lage::Zugang::Offen(stand) = journal {
                bild.wiederanlauf =
                    depot_lage::wiederanlauf::Diagnose::lies(&daten, stand, eintraege);
            }
            bild
        });
        drop(cache);
        bild.gemini_budget = Some(gemini::budget(&daten, heute(jetzt)));
        bild
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
    println!("[Dienst] Oberfläche: {adresse}");
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

/// Aufruf im einzigen Hintergrundfaden für die optionale lokale Auswertung.
pub fn starte_neuroschlopf(d: &std::sync::Arc<Dienst>) {
    let externes = std::sync::Arc::clone(d);
    std::thread::spawn(move || neuroauftraege::arbeite(&externes));
    neuroschlopf::arbeite(d);
}

fn update_sperre(d: &Dienst, z: &werkbank::Zustand) -> Option<seiten::UpdateSperre> {
    use seiten::UpdateSperre;

    if !d.update_offen() {
        return None;
    }
    let offen = match rueckfall::offen(&d.wurzel) {
        Ok(Some(offen)) => offen,
        Ok(None) if !d.update_offen() => return None,
        Ok(None) | Err(_) => return Some(UpdateSperre::NichtPruefbar),
    };
    let frist_abgelaufen = rueckfall::offen_pfad(&d.wurzel)
        .metadata()
        .and_then(|datei| datei.modified())
        .ok()
        .and_then(|zeit| zeit.elapsed().ok())
        .is_some_and(|dauer| dauer.as_secs() >= 180);
    if frist_abgelaufen {
        return Some(UpdateSperre::BestaetigungFehlgeschlagen(offen));
    }
    if z.neustart || z.laufend.as_ref().is_some_and(|a| a.art == Art::Update) {
        Some(UpdateSperre::Neustart)
    } else if z.update_meldung.as_ref().is_some_and(|(fehler, _)| *fehler) {
        Some(UpdateSperre::BestaetigungFehlgeschlagen(offen))
    } else if d.version.as_deref() == Some(offen.neu.as_str()) {
        Some(UpdateSperre::Probe)
    } else {
        Some(UpdateSperre::Unbestaetigt)
    }
}

fn live_inhalt(d: &Dienst, jetzt: Zeitpunkt) -> String {
    let ki =
        zeitstrahl_ki::Aufrufe::lesen(&d.gespraechscache, &d.wurzel.join("daten"), heute(jetzt));
    let z = d.sperre();
    let sperre = update_sperre(d, &z);
    let mut html = seiten::live_mit_ki(&d.token, &d.start, &z, jetzt, &ki, sperre.as_ref());
    if sicherung::ziel(&d.wurzel).is_empty() {
        html.push_str("<p class=\"leise\">Sicherung: aus</p>");
    } else {
        html.push_str(&sicherungsstand(d, &z, jetzt));
    }
    drop(z);
    html.push_str(&neuroschlopf::live(d));
    html
}

fn sicherungsstand(d: &Dienst, z: &werkbank::Zustand, jetzt: Zeitpunkt) -> String {
    let laeuft = z.laufend.as_ref().is_some_and(|a| a.art == Art::Sicherung);
    let stand = sicherung::lies(&d.wurzel);
    stand.html(jetzt, laeuft || stand.laeuft(&d.wurzel))
}

fn sicherungsinfo(d: &Dienst, jetzt: Zeitpunkt) -> String {
    let (ziel, status) = {
        let z = d.sperre();
        let status = if z.warteschlange.iter().any(|a| a.art == Art::Sicherung) {
            "<p>Sicherung wartet auf den Start.</p>".into()
        } else {
            sicherungsstand(d, &z, jetzt)
        };
        (sicherung::ziel(&d.wurzel), status)
    };
    sicherung::info(&ziel, &status)
}

fn betriebsleiste(d: &Dienst, tag: Option<depot_kern::Datum>) -> String {
    let kurssperre = seiten::kurssperre_status(&d.wurzel.join("daten"), tag);
    let z = d.sperre();
    let sperre = update_sperre(d, &z);
    let update_freigabe_offen = sperre.is_none()
        && z.update_angebot().is_some_and(|angebot| {
            angebot.passt(d.version.as_deref()) && angebot.abdruck.is_some()
        })
        && matches!(
            d.update_freigabe(&z),
            update_angebot::Freigabe::Offen | update_angebot::Freigabe::Wartet
        );
    seiten::betriebsleiste_mit_kurssperre(
        &d.start,
        &z,
        tag,
        kurssperre.as_deref(),
        sperre.as_ref(),
        update_freigabe_offen,
    )
}

fn update_leiste(d: &Dienst) -> String {
    let z = d.sperre();
    seiten::update_leiste(
        &d.start,
        &z,
        d.version.as_deref(),
        update_sperre(d, &z).is_some(),
    )
}

fn live_seite(d: &Dienst, a: &Anfrage, jetzt: Zeitpunkt) -> Vec<u8> {
    let export = analyse::formular(d, heute(jetzt));
    let live = live_inhalt(d, jetzt);
    let inhalt = format!("{export}<div data-teil=\"/teil/live\">{live}</div>");
    let s = seiten::rahmen(
        "/",
        None,
        seiten::meldung(&a.abfrage),
        &inhalt,
        d.version.as_deref(),
        (&betriebsleiste(d, heute(jetzt)), &update_leiste(d)),
        seiten::meldungs_aktion(&a.abfrage),
    );
    antwort(200, HTML, s.as_bytes())
}

fn einrichtungsseite(
    d: &Dienst,
    u: &dyn Umwelt,
    a: &Anfrage,
    jetzt: Zeitpunkt,
    tag: Option<depot_kern::Datum>,
) -> Vec<u8> {
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
        alpha_kontingent: depot_lage::abrufkontingent::stand(&d.wurzel.join("daten"), jetzt).ok(),
        zeitplan_gespeichert: None,
        ki_einstellungen: ki_einstellungen::einrichtung(d, tag),
        entwurf: formular(a.abfrage.as_bytes())
            .map(|f| seiten::Entwurf::aus_formular(&f))
            .unwrap_or_default(),
        autostart: u.autostart(&d.wurzel),
        ibkr_einstiegslink: ibkr_papier::einstiegslink(d),
        wurzel: d.wurzel.display().to_string(),
        grenzen: grenzwerte(),
        abgelehnt: werkbank::lies_falls_da(&rueckfall::abgelehnt_pfad(&d.wurzel)).map(|t| {
            t.unwrap_or_default()
                .lines()
                .map(|z| rueckfall::kurz(z.trim()).to_string())
                .filter(|z| !z.is_empty())
                .collect()
        }),
    };
    let gemini_schluessel = u.schluessel(Wer::Gemini);
    let gemini_stand = if gemini_schluessel == Hinterlegt::Unbekannt {
        depot_lage::SchluesselStand::Unbekannt
    } else {
        schluessel_stand(gemini_schluessel, &protokoll, Wer::Gemini.protokoll())
    };
    let mut zusatz = gemini::einrichtung(d, gemini_stand, tag, &a.abfrage);
    zusatz.push_str(&sicherung::formular(
        &d.token,
        &sicherung::ziel(&d.wurzel),
        &sicherungsinfo(d, jetzt),
    ));
    zusatz.push_str(&mindesteinsatz_formular(d));
    zusatz.push_str(&splitverdacht::formular(d));
    zusatz.push_str(&analyse::massstab::einrichtung(d));
    zusatz.push_str(&ibkr_papier::einrichtung(
        d,
        &a.abfrage,
        ibkr_papier::gespeichert(d),
    ));
    let zustand = d.sperre();
    let gespeichert = seiten::Einrichtung {
        zeitplan_gespeichert: match datei {
            Ok(Some(t)) => Some(t == zustand.zeitplan.als_text()),
            Ok(None) => None,
            Err(_) => Some(false),
        },
        ..gespeichert
    };
    let sperre = update_sperre(d, &zustand);
    let update = seiten::UpdateAnzeige {
        sperre: sperre.as_ref(),
        version: d.version.as_deref(),
        jetzt: Some(jetzt),
        freigabe: d.update_freigabe(&zustand),
    };
    let inhalt = seiten::einrichtung_mit_update(
        &d.token,
        &d.start,
        &zustand,
        tag,
        &gespeichert,
        &zusatz,
        update,
    );
    drop(zustand);
    let seite = seiten::rahmen(
        "/einrichtung",
        None,
        gemini::seiten_meldung(&a.abfrage),
        &inhalt,
        d.version.as_deref(),
        (&betriebsleiste(d, tag), &update_leiste(d)),
        seiten::meldungs_aktion(&a.abfrage),
    );
    antwort(200, HTML, seite.as_bytes())
}

fn depot_version(d: &Dienst) -> Option<u64> {
    let z = d.sperre();
    if z.lage_veraltet || z.laufend.is_some() {
        return None;
    }
    z.lage.as_ref().map(|lage| lage.depot_version)
}

fn depot_seite(d: &Dienst, a: &Anfrage, jetzt: Zeitpunkt) -> Vec<u8> {
    use depot_lage::depotverlauf::Zeitraum;
    let abfrage = formular(a.abfrage.as_bytes()).unwrap_or_default();
    let zeitraum = Zeitraum::aus_abfrage(feld(&abfrage, "zeitraum").unwrap_or_default());
    let version = depot_version(d);
    let teil = a.pfad == "/teil/depot";
    if teil
        && (version.is_none() || feld(&abfrage, "stand").and_then(|s| s.parse().ok()) == version)
    {
        return antwort(200, HTML, b"");
    }
    let (mut inhalt, lesbar) = depotseite::inhalt(d, jetzt, zeitraum);
    if let Some(version) = version.filter(|_| lesbar) {
        use std::fmt::Write as _;
        let _ = write!(
            inhalt,
            "<span hidden data-depot-stand=\"{version}\"></span>"
        );
    }
    if teil {
        let inhalt = if lesbar { inhalt.as_bytes() } else { b"" };
        return antwort(200, HTML, inhalt);
    }
    let url = format!("/teil/depot?zeitraum={}", zeitraum.als_abfrage());
    let inhalt = format!(
        "<div data-teil=\"/teil/ibkr-depot\">{}</div><div data-teil=\"{url}\">{inhalt}</div>",
        ibkr_papier::depot(d),
    );
    let html = seiten::rahmen(
        "/depot",
        None,
        None,
        &inhalt,
        d.version.as_deref(),
        (&betriebsleiste(d, heute(jetzt)), &update_leiste(d)),
        seiten::meldungs_aktion(&a.abfrage),
    );
    antwort(200, HTML, html.as_bytes())
}

fn get(d: &Dienst, u: &dyn Umwelt, a: &Anfrage, jetzt: Zeitpunkt) -> Vec<u8> {
    let tag = heute(jetzt);
    match a.pfad.as_str() {
        "/" => live_seite(d, a, jetzt),
        "/depot" | "/teil/depot" => depot_seite(d, a, jetzt),
        "/gespraeche" => gespraechsseite(d, a, tag),
        "/neuroschlopf/belegpruefung" => {
            let Some(f) = formular(a.abfrage.as_bytes()) else {
                return seiten::fehler(400, "Der gewählte Prüftag ist ungültig.");
            };
            let wahl = match feld(&f, "tag") {
                Some(wert) => match depot_kern::Datum::lies(wert) {
                    Some(datum) => Some(datum),
                    None => return seiten::fehler(400, "Der gewählte Prüftag ist ungültig."),
                },
                None => None,
            };
            belegpruefung::seite(d, tag, wahl, feld(&f, "gespeichert") == Some("ja"))
        }
        "/neuroschlopf" => {
            let ki =
                zeitstrahl_ki::Aufrufe::lesen(&d.gespraechscache, &d.wurzel.join("daten"), tag);
            neuroschlopf::seite(d, &ki, tag)
        }
        "/neuroschlopf/lernlauf" => neuro2026::seite(d, tag),
        "/neuroschlopf/lernlauf.json" => neuro2026::daten(),
        "/gespraeche/rohtext" => gespraechsrohtext(d, a),
        "/export" => seiten::fehler(405, "Verwende das Analyse-Exportformular."),
        "/teil/verbindung" | "/teil/status" => verbindungsstatus(d, tag),
        "/teil/sicherung" => antwort(200, HTML, sicherungsinfo(d, jetzt).as_bytes()),
        "/teil/ibkr-abgleich" => antwort(200, HTML, ibkr_abgleich::status(d).as_bytes()),
        "/teil/ibkr-depot" => antwort(200, HTML, ibkr_papier::depot(d).as_bytes()),
        "/teil/ibkr-einstiegslink" => antwort(200, HTML, ibkr_papier::einstiegslink(d).as_bytes()),
        "/teil/ibkr-papier" => antwort(200, HTML, ibkr_papier::status(d).as_bytes()),
        "/teil/live" => antwort(200, HTML, live_inhalt(d, jetzt).as_bytes()),
        "/teil/neuroschlopf-aktivitaet" => {
            antwort(200, HTML, neuroschlopf::aktivitaetsfragment(d).as_bytes())
        }
        "/teil/erste-schritte" => {
            let autostart = u.autostart(&d.wurzel);
            let html = seiten::erste_schritte(&d.start, &d.sperre(), &autostart);
            antwort(200, HTML, html.as_bytes())
        }
        "/teil/journal" => antwort(
            200,
            HTML,
            seiten::journal_teil(&d.token, &d.start, &d.sperre()).as_bytes(),
        ),
        "/teil/quellen" => antwort(
            200,
            HTML,
            seiten::quellen_teil(
                &d.token,
                &d.start,
                &d.sperre(),
                depot_lage::abrufkontingent::stand(&d.wurzel.join("daten"), jetzt).ok(),
            )
            .as_bytes(),
        ),
        "/teil/kopf" => antwort(200, HTML, betriebsleiste(d, tag).as_bytes()),
        "/teil/update-hinweis" => antwort(200, HTML, update_leiste(d).as_bytes()),
        "/teil/update" => {
            let z = d.sperre();
            let sperre = update_sperre(d, &z);
            let update = seiten::UpdateAnzeige {
                sperre: sperre.as_ref(),
                version: d.version.as_deref(),
                jetzt: Some(jetzt),
                freigabe: d.update_freigabe(&z),
            };
            antwort(
                200,
                HTML,
                seiten::update_aktionen(&d.token, &d.start, &z, update).as_bytes(),
            )
        }
        "/desktop-dialog" | "/teil/desktop-dialog" => desktopdialog_seite(d, a, jetzt),
        "/bestaetigen" => auftrag_bestaetigen(d, a, jetzt),
        "/protokoll" | "/protokoll.txt" => protokoll_lesen(d, a, tag),
        "/einrichtung" => einrichtungsseite(d, u, a, jetzt, tag),
        "/app.js" => {
            // Rust-Quelltext normalisierte im früheren Literal CRLF bereits beim Einlesen.
            // Der ausgelieferte Text bleibt auch aus einem Windows-Checkout bytegleich.
            let skript = seiten::APP_JS;
            if skript.as_bytes().contains(&b'\r') {
                antwort(
                    200,
                    "text/javascript; charset=utf-8",
                    skript.replace("\r\n", "\n").as_bytes(),
                )
            } else {
                antwort(200, "text/javascript; charset=utf-8", skript.as_bytes())
            }
        }
        _ => seiten::fehler(404, "Nicht gefunden"),
    }
}

/// Räumt alte lokale Gesprächsprotokolle beim Start des Dienstes auf.
pub fn gespraeche_beim_start(wurzel: &std::path::Path) {
    gespraeche::beim_start(wurzel);
}

fn gespraechsseite(d: &Dienst, a: &Anfrage, heute: Option<depot_kern::Datum>) -> Vec<u8> {
    let abfrage = formular(a.abfrage.as_bytes()).unwrap_or_default();
    let mut inhalt = analyse::formular(d, heute);
    inhalt.push_str(&gespraeche::inhalt(
        &d.wurzel.join("daten"),
        heute,
        feld(&abfrage, "tag").unwrap_or_default(),
        feld(&abfrage, "wer").unwrap_or_default(),
        feld(&abfrage, "lauf").unwrap_or_default(),
    ));
    let seite = seiten::rahmen(
        "/gespraeche",
        None,
        None,
        &inhalt,
        d.version.as_deref(),
        (&betriebsleiste(d, heute), &update_leiste(d)),
        ("", ""),
    );
    antwort(200, HTML, seite.as_bytes())
}

fn gespraechsrohtext(d: &Dienst, a: &Anfrage) -> Vec<u8> {
    let abfrage = formular(a.abfrage.as_bytes()).unwrap_or_default();
    let nr = feld(&abfrage, "nr").and_then(|nr| nr.parse().ok());
    let art = feld(&abfrage, "art").unwrap_or_default();
    match nr.and_then(|nr| gespraeche::rohtext(&d.wurzel.join("daten"), nr, art)) {
        Some(bytes) => antwort(200, "text/plain; charset=utf-8", &bytes),
        None => seiten::fehler(404, "Der belegte Rohtext ist nicht verfügbar."),
    }
}

/// Der bereits vor diesen Korrekturen vorhandene Statuspfad bleibt der Herzschlag.
fn verbindungsstatus(d: &Dienst, tag: Option<depot_kern::Datum>) -> Vec<u8> {
    let bereit = !d.update_offen();
    let zustand = d.sperre();
    let update_laueft = zustand.auftrag_stand(Art::Update).is_some() || zustand.neustart;
    let text = format!(
        "{}<span hidden data-bereit=\"{}\" data-update-laeuft=\"{}\"></span>",
        seiten::status(&d.token, &d.start, &zustand, tag),
        if bereit { "ja" } else { "nein" },
        if update_laueft { "ja" } else { "nein" },
    );
    antwort(200, HTML, text.as_bytes())
}

fn protokoll_lesen(d: &Dienst, a: &Anfrage, tag: Option<depot_kern::Datum>) -> Vec<u8> {
    let formular = formular(a.abfrage.as_bytes()).unwrap_or_default();
    let Some(kennung) = feld(&formular, "lauf").filter(|k| protokoll::gueltige_kennung(k)) else {
        return seiten::fehler(400, "Wähle ein Protokoll unter Live aus.");
    };
    match protokoll::lesen(&d.wurzel, kennung) {
        Ok(text) => {
            let text = d.protokoll_datei_bereinigen(&text);
            if a.pfad == "/protokoll.txt" {
                return antwort(200, "text/plain; charset=utf-8", text.as_bytes());
            }
            let inhalt = format!(
                "{}{}",
                seiten::marke(&d.start),
                seiten::protokoll_seite(kennung, &text)
            );
            let html = seiten::rahmen(
                "/",
                None,
                None,
                &inhalt,
                d.version.as_deref(),
                (&betriebsleiste(d, tag), &update_leiste(d)),
                seiten::meldungs_aktion(&a.abfrage),
            );
            antwort(200, HTML, html.as_bytes())
        }
        Err(_) => seiten::fehler(
            404,
            "Dieses Protokoll ist nicht verfügbar. Ältere Läufe oder Läufe vor Einführung der Protokolldateien haben keine vollständige Datei. Unter Live steht der noch bekannte Verlauf.",
        ),
    }
}

fn zeitplan_aus_formular(f: &[(String, String)]) -> Option<Zeitplan> {
    let haken = |n: &str| match feld(f, n) {
        None => Some(false),
        Some("ja") => Some(true),
        Some(_) => None,
    };
    Some(Zeitplan {
        an: false,
        termine: lies_uhrzeit(feld(f, "termine")?)?,
        kurse: lies_uhrzeit(feld(f, "kurse")?)?,
        papier_danach: haken("papier_danach")?,
        nachrichten_an: haken("nachrichten_an")?,
        vorbereitung: zeitplan::VorbereitungsPlan::aus_bool(haken("vorbereitung_an")?),
        hinweis: zeitplan::HinweisPlan::aus_bool(haken("hinweis_an")?),
    })
}

fn desktopdialog_seite(d: &Dienst, a: &Anfrage, jetzt: Zeitpunkt) -> Vec<u8> {
    let id = a
        .abfrage
        .strip_prefix("id=")
        .filter(|id| id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()))
        .unwrap_or_default();
    let inhalt = desktopdialog::inhalt(d, id);
    let seite = if a.pfad == "/desktop-dialog" {
        seiten::rahmen(
            "/einrichtung",
            None,
            None,
            &format!(
                "<section><h2>Windows-Dialog</h2><div data-teil=\"/teil/desktop-dialog?id={id}\">{inhalt}</div></section>"
            ),
            d.version.as_deref(),
            (&betriebsleiste(d, heute(jetzt)), &update_leiste(d)),
            ("", ""),
        )
    } else {
        inhalt
    };
    antwort(200, HTML, seite.as_bytes())
}

fn ist_formular(a: &Anfrage) -> bool {
    a.kopf("content-type")
        .and_then(|typ| typ.split(';').next())
        .is_some_and(|t| {
            t.trim()
                .eq_ignore_ascii_case("application/x-www-form-urlencoded")
        })
}

fn post(
    d: &Dienst,
    u: &dyn Umwelt,
    a: &Anfrage,
    jetzt: Zeitpunkt,
    hintergrund: Option<desktopdialog::Umgebung<'_>>,
) -> Vec<u8> {
    if !herkunft_erlaubt(a.kopf("origin")) {
        return seiten::fehler(403, "Abgelehnt: fremde Herkunft");
    }
    if !ist_formular(a) {
        return seiten::fehler(400, "Abgelehnt: nur Formulare");
    }
    let Some(f) = formular(&a.rumpf) else {
        return seiten::fehler(400, "Abgelehnt: Formular unlesbar");
    };
    if !feld(&f, "t").is_some_and(|token| gleich(token, &d.token)) {
        return seiten::fehler(
            403,
            "Diese Seite ist veraltet. Öffne Live oder die Einrichtung erneut und wiederhole die Aktion.",
        );
    }
    if let Some(antwort) = desktopdialog::post(d, u, hintergrund, &a.pfad, &f, jetzt) {
        return antwort;
    }
    match a.pfad.as_str() {
        "/ausfuehren" => auftrag_einreihen(d, &f, jetzt),
        "/update-freigabe" => update_freigabe_starten(d, &f),
        "/export" => analyse::exportieren(d, u, jetzt, &f),
        "/gemini" => gemini::speichern(d, &f),
        "/ki-einstellungen" => ki_einstellungen::speichern(d, &f),
        "/ibkr-abgleich" => ibkr_abgleich::speichern(d, &f),
        "/ibkr-papier" => ibkr_papier::speichern(d, feld(&f, "konto").unwrap_or_default()),
        "/neuroschlopf" => neuroschlopf::speichern(d, &f),
        "/neuroschlopf/belegpruefung" => belegpruefung::speichern(d, &f, heute(jetzt)),
        "/schluessel" => {
            let (Some(wer), Some(wert)) =
                (feld(&f, "wer").and_then(Wer::aus_kurz), feld(&f, "wert"))
            else {
                return seiten::fehler(
                    400,
                    "Schlüssel unbekannt. Wähle den Anbieter erneut in der Einrichtung.",
                );
            };
            let ziel = if wer == Wer::Gemini {
                "&wer=gemini#zusatz"
            } else {
                "#schluessel"
            };
            if wer.pruefe(wert).is_err() {
                return http::umleitung(&format!("/einrichtung?m=schluessel_ungueltig{ziel}"));
            }
            if u.schluessel_ablegen(wer, wert).is_err() {
                return http::umleitung(&format!("/einrichtung?m=schluessel_fehler{ziel}"));
            }
            d.protokoll_schluessel_merken(wert.trim());
            if wer == Wer::Gemini && u.schluessel(wer) != Hinterlegt::Ja {
                return http::umleitung(&format!("/einrichtung?m=schluessel_fehler{ziel}"));
            }
            // Der neue Schlüssel ist hinterlegt, bis ein Aufruf ihn bestätigt.
            let neu = anbieter::Eintrag::neu(jetzt, wer.protokoll(), anbieter::Ergebnis::Neu, None);
            let datei = d.wurzel.join("daten").join(anbieter::DATEI);
            let meldung = if anbieter::anhaengen(&datei, &neu).is_ok() {
                "schluessel"
            } else {
                "schluessel_protokoll"
            };
            d.sperre().lage_veraltet = true;
            http::umleitung(&format!("/einrichtung?m={meldung}{ziel}"))
        }
        "/zeitplan" => zeitplan_aendern(d, &f),
        "/mindesteinsatz" => {
            let Some(bp) = feld(&f, "bp")
                .and_then(|s| s.parse::<u32>().ok())
                .filter(|bp| *bp <= 10_000)
            else {
                return seiten::fehler(400, "Mindesteinsatz: 0 bis 10000 Basispunkte eingeben.");
            };
            let _zustand = d.sperre();
            match werkbank::schreibe_ganz(
                &d.wurzel.join("daten/mindesteinsatz.txt"),
                &format!("{bp}\n"),
            ) {
                Ok(()) => http::umleitung("/einrichtung#mindesteinsatz"),
                Err(_) => seiten::fehler(409, "Mindesteinsatz konnte nicht gespeichert werden."),
            }
        }
        "/sicherung-ziel" => sicherung_einstellen(d, &f, jetzt),
        "/splitverdacht" => splitverdacht::aktion(d, &f, jetzt),
        "/autostart" => {
            let an = match feld(&f, "an") {
                Some("an") => true,
                Some("aus") => false,
                _ => return seiten::fehler(400, "Abgelehnt: an oder aus erwartet"),
            };
            if !an && feld(&f, "geprueft") != Some("ja") {
                return http::umleitung("/einrichtung?m=autostart_bestaetigen#autostart");
            }
            let _z = d.sperre();
            match u.autostart_setzen(&d.wurzel, an) {
                Ok(()) if an => http::umleitung("/einrichtung?m=autostart_an#autostart"),
                Ok(()) => http::umleitung("/einrichtung?m=autostart_aus#autostart"),
                Err(_) => http::umleitung("/einrichtung?m=autostart_fehler#autostart"),
            }
        }
        _ => seiten::fehler(404, "Nicht gefunden"),
    }
}

fn auftrag_bestaetigen(d: &Dienst, a: &Anfrage, jetzt: Zeitpunkt) -> Vec<u8> {
    let f = formular(a.abfrage.as_bytes()).unwrap_or_default();
    let (art, sha, ziel) = match feld(&f, "art").and_then(Art::aus_kurz) {
        Some(Art::Papier) => (Art::Papier, None, "/"),
        Some(Art::Simulation) => {
            if d.sperre().simulation_belegt() {
                return http::umleitung("/?m=simulation_belegt#ki-simulation");
            }
            (Art::Simulation, None, "/")
        }
        Some(Art::Vorbereiten) => {
            if d.update_offen() {
                return http::umleitung("/?m=vorbereitung_update#vorbereitung");
            }
            if let Err(m) = d.sperre().vorbereitung_pruefen(jetzt) {
                return http::umleitung(&format!("/?m={m}#vorbereitung"));
            }
            (Art::Vorbereiten, None, "/")
        }
        Some(Art::Update) => {
            let z = d.sperre();
            let Some(angebot) = z.update_angebot().filter(|angebot| {
                angebot.passt(d.version.as_deref()) && feld(&f, "sha") == Some(angebot.neu.as_str())
            }) else {
                return http::umleitung("/einrichtung?m=update_suche#update");
            };
            if z.update_wartet_auf_ruhe() {
                return http::umleitung("/einrichtung?m=update_belegt#update");
            }
            if d.update_freigabe(&z) != update_angebot::Freigabe::Erteilt {
                return http::umleitung("/einrichtung?m=update_freigabe_fehlt#update");
            }
            (Art::Update, Some(angebot.neu.clone()), "/einrichtung")
        }
        _ => return seiten::fehler(404, "Diese Aktion gibt es nicht."),
    };
    let inhalt = format!(
        "{}{}",
        seiten::marke(&d.start),
        seiten::auftrag_bestaetigen(
            &d.token,
            art,
            sha.as_deref(),
            if art == Art::Simulation {
                depot_journal::ki_einstellungen::lesen(&d.wurzel.join("daten"))
                    .ok()
                    .and_then(|e| {
                        depot_lage::simulationskosten_fuer(e.dachs.auswahl, e.maus.auswahl)
                    })
            } else {
                None
            }
        )
    );
    let seite = seiten::rahmen(
        ziel,
        None,
        None,
        &inhalt,
        d.version.as_deref(),
        (&betriebsleiste(d, heute(jetzt)), &update_leiste(d)),
        seiten::meldungs_aktion(&a.abfrage),
    );
    antwort(200, HTML, seite.as_bytes())
}

fn mindesteinsatz_formular(d: &Dienst) -> String {
    let mindesteinsatz = depot_lage::papier_mindesteinsatz(&d.wurzel.join("daten"))
        .map(|bp| format!("{bp}"))
        .unwrap_or_default();
    format!(
        r#"<section id="mindesteinsatz"><h2>Papier-Mindesteinsatz</h2>
          <form method="post" action="/mindesteinsatz" data-aenderungen><input type="hidden" name="t" value="{}">
          <label>Mindesteinsatz in Basispunkten <input type="number" name="bp" min="0" max="10000" value="{mindesteinsatz}" required></label>
          <button type="submit" data-speichern-hervorheben>Speichern</button></form><p>3.000 bp entsprechen 30 %. Darunter ist eine Begründung erforderlich.</p></section>"#,
        depot_lage::html::maskiere(&d.token)
    )
}

fn sicherung_einstellen(d: &Dienst, f: &[(String, String)], jetzt: Zeitpunkt) -> Vec<u8> {
    let aktion = feld(f, "aktion").unwrap_or("speichern");
    let (ziel, meldung) = match aktion {
        "speichern" | "starten" => {
            let Some(ziel) = feld(f, "ziel") else {
                return seiten::fehler(400, "Sicherungsziel fehlt");
            };
            let meldung = match (aktion, ziel.trim().is_empty()) {
                ("starten", _) => "sicherung_start",
                (_, true) => "sicherung_aus",
                _ => "sicherung_ziel",
            };
            (ziel.to_owned(), meldung)
        }
        _ => return seiten::fehler(400, "Unbekannte Sicherungsaktion"),
    };
    match d.sicherung_einstellen(&ziel, aktion == "starten", jetzt, None) {
        Ok(()) => http::umleitung(&format!("/einrichtung?m={meldung}#sicherung")),
        Err((code, fehler)) => seiten::fehler(code, &fehler),
    }
}

fn auftrag_einreihen(d: &Dienst, f: &[(String, String)], jetzt: Zeitpunkt) -> Vec<u8> {
    let Some(art) = feld(f, "art").and_then(Art::aus_kurz) else {
        return seiten::fehler(400, "Diese Aktion gibt es nicht.");
    };
    let bestaetigt = feld(f, "geprueft") == Some("ja");
    let ort = match art {
        Art::Vorbereiten if d.update_offen() => "/?m=vorbereitung_update",
        Art::Vorbereiten if !bestaetigt => "/?m=vorbereitung_bestaetigen",
        Art::Vorbereiten => {
            let m = match d.von_hand(art, jetzt) {
                Ok(()) => "auftrag",
                Err(m) if m.starts_with("vorbereitung_") => m,
                Err(_) => "auftrag_belegt",
            };
            return http::umleitung(&format!("/?m={m}#vorbereitung"));
        }
        Art::Simulation => {
            let m = if bestaetigt {
                match d.von_hand(art, jetzt) {
                    Ok(()) => "auftrag",
                    Err("simulation_belegt") => "simulation_belegt",
                    Err(_) => "auftrag_belegt",
                }
            } else {
                "simulation_bestaetigen"
            };
            return http::umleitung(&format!("/?m={m}#ki-simulation"));
        }
        Art::Papier if !bestaetigt => "/?m=papier_bestaetigen",
        Art::Update if !bestaetigt => "/einrichtung?m=update_bestaetigen#update",
        Art::Update => {
            let sha = feld(f, "sha").unwrap_or_default();
            let m = d.update_von_hand(sha).err().unwrap_or("update");
            return http::umleitung(&format!("/einrichtung?m={m}#update"));
        }
        Art::Sicherung => {
            if d.von_hand(art, jetzt).is_ok() {
                "/einrichtung?m=auftrag#sicherung"
            } else {
                "/einrichtung?m=auftrag_belegt#sicherung"
            }
        }
        Art::UpdatePruefen => {
            if d.von_hand(art, jetzt).is_ok() {
                "/einrichtung?m=updatesuche_eingereiht#update"
            } else {
                "/einrichtung?m=updatesuche_belegt#update"
            }
        }
        Art::IbkrPapier => return ibkr_papier::testen(d),
        Art::IbkrJournal | Art::IbkrAbgleich => return ibkr_abgleich::starten(d, art, jetzt),
        Art::Einrichten if feld(f, "ziel") == Some("einrichtung") => {
            if d.von_hand(art, jetzt).is_ok() {
                "/einrichtung?m=auftrag#journal"
            } else {
                "/einrichtung?m=auftrag_belegt#journal"
            }
        }
        _ => {
            if d.von_hand(art, jetzt).is_ok() {
                "/?m=auftrag"
            } else {
                "/?m=auftrag_belegt"
            }
        }
    };
    http::umleitung(ort)
}

fn update_freigabe_starten(d: &Dienst, f: &[(String, String)]) -> Vec<u8> {
    let sha = feld(f, "sha").unwrap_or_default();
    let abdruck = feld(f, "abdruck").unwrap_or_default();
    let meldung = d
        .update_dialog_starten(sha, abdruck)
        .err()
        .unwrap_or("update_dialog");
    http::umleitung(&format!("/einrichtung?m={meldung}#update"))
}

fn zeitplan_aendern(d: &Dienst, f: &[(String, String)]) -> Vec<u8> {
    let (meldung, entwurf, ergebnis) = match feld(f, "aktion") {
        Some(aktion @ ("speichern" | "speichern_an")) => {
            let entwurf = seiten::Entwurf::aus_formular(f).als_abfrage();
            let Some(plan) = zeitplan_aus_formular(f) else {
                return http::umleitung(&format!(
                    "/einrichtung?m=zeitplan_ungueltig{entwurf}#zeitplan"
                ));
            };
            if !zeitplan::morgenplan(&plan) {
                return http::umleitung(&format!(
                    "/einrichtung?m=zeitplan_kursfenster{entwurf}#zeitplan"
                ));
            }
            if plan.vorbereitung.aktiv() && !plan.nachrichten_an {
                return http::umleitung(&format!(
                    "/einrichtung?m=vorbereitung_quellen{entwurf}#zeitplan"
                ));
            }
            (
                if aktion == "speichern_an" {
                    "zeitplan_gespeichert_an"
                } else {
                    "zeitplan"
                },
                entwurf,
                d.aendere_zeitplan(|alt| {
                    if feld(f, "planstand").is_some_and(|gesehen| gesehen != alt.abdruck()) {
                        return Err(werkbank::ZeitplanAbgelehnt::StandGeaendert);
                    }
                    Ok(Zeitplan {
                        an: aktion == "speichern_an" || alt.an,
                        ..plan
                    })
                }),
            )
        }
        Some(aktion @ ("an" | "aus")) => {
            let an = aktion == "an";
            (
                if an { "zeitplan_an" } else { "zeitplan_aus" },
                String::new(),
                d.aendere_zeitplan(|alt| Ok(Zeitplan { an, ..alt })),
            )
        }
        _ => return http::umleitung("/einrichtung?m=zeitplan_neu_laden#zeitplan"),
    };
    let fehler = match ergebnis {
        Ok(()) => return http::umleitung(&format!("/einrichtung?m={meldung}#zeitplan")),
        Err(werkbank::ZeitplanAbgelehnt::StandGeaendert) => "zeitplan_stand_geaendert",
        Err(werkbank::ZeitplanAbgelehnt::Einrichtung(_)) => "zeitplan_einrichtung",
        Err(werkbank::ZeitplanAbgelehnt::UpdateOffen) => "vorbereitung_update",
        Err(werkbank::ZeitplanAbgelehnt::LageFehlt) => "zeitplan_lage",
        Err(werkbank::ZeitplanAbgelehnt::Datei(_)) => "zeitplan_fehler",
    };
    http::umleitung(&format!("/einrichtung?m={fehler}{entwurf}#zeitplan"))
}

/// Welche Sperrdatei ein Formular aufhebt.
#[derive(Clone, Copy, Debug)]
pub enum Sperre {
    Kostenschutz,
    Kurse,
}

/// Beantwortet eine gelesene Anfrage. Erst der Host, dann je nach Methode.
pub fn behandle(d: &Dienst, u: &dyn Umwelt, a: &Anfrage, jetzt: Zeitpunkt) -> Vec<u8> {
    behandle_anfrage(d, u, a, jetzt, None)
}

/// Antwortet sofort auf Aufrufe eines Windows-Dialogs. Dessen Ergebnis wird separat angezeigt.
pub fn behandle_am_desktop(
    d: &std::sync::Arc<Dienst>,
    u: &std::sync::Arc<dyn Umwelt>,
    a: &Anfrage,
    jetzt: Zeitpunkt,
) -> Vec<u8> {
    behandle_anfrage(d, u.as_ref(), a, jetzt, Some((d, u)))
}

fn behandle_anfrage(
    d: &Dienst,
    u: &dyn Umwelt,
    a: &Anfrage,
    jetzt: Zeitpunkt,
    hintergrund: Option<desktopdialog::Umgebung<'_>>,
) -> Vec<u8> {
    if !host_erlaubt(a.kopf("host")) {
        return seiten::fehler(403, "Abgelehnt: fremder Host");
    }
    match a.methode.as_str() {
        "GET" => get(d, u, a, jetzt),
        "POST" if a.pfad == http::ESTR_IMPORT => analyse::massstab::importieren(d, a),
        "POST" => post(d, u, a, jetzt, hintergrund),
        _ => seiten::fehler(405, "Nur GET und POST"),
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
