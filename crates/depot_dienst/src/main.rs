//! `depot_dienst` – startet den Dienst mit Oberflaeche auf http://127.0.0.1:47410/.
//!
//! Aufruf ueber `DepotSchlopf.cmd` (Doppelklick) oder den Autostart. Laeuft der Dienst schon,
//! oeffnet ein zweiter Start nur die Seite und endet. `--ohne-browser` oeffnet keine Seite.

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

use std::io::{ErrorKind, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use depot_dienst::programmstand;
use depot_dienst::rueckfall::{self, NEUSTART};
use depot_dienst::werkbank::{Dienst, cargo_starter};
use depot_dienst::{
    System, Umwelt, behandle, finde_wurzel, http, jetzt, lagerechner, neues_token, oeffne_browser,
};

/// So viele Verbindungen werden hoechstens zugleich bedient; weitere werden geschlossen.
const VERBINDUNGEN_MAX: usize = 32;

fn bediene(d: &Dienst, u: &dyn Umwelt, mut strom: TcpStream) {
    let _ = strom.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = strom.set_write_timeout(Some(Duration::from_secs(10)));
    let antwort = match http::lies(&mut strom) {
        Ok(anfrage) => match jetzt() {
            Some(t) => behandle(d, u, &anfrage, t),
            None => depot_dienst::seiten::fehler(500, "Uhr liefert keinen Zeitpunkt"),
        },
        Err(http::LeseFehler::ZuGross) => depot_dienst::seiten::fehler(413, "Anfrage zu groß"),
        Err(http::LeseFehler::Kaputt) => depot_dienst::seiten::fehler(400, "Anfrage unlesbar"),
        Err(http::LeseFehler::Abgebrochen) => return,
    };
    let _ = strom.write_all(&antwort);
    let _ = strom.flush();
}

/// Der Arbeiter: jede Sekunde ein Schritt; ein gestarteter Auftrag wird in eigenem Faden verfolgt.
/// Nach einem eingespielten Update endet der Dienst, sobald nichts mehr laeuft oder wartet, mit
/// `NEUSTART`; der Waechter startet dann den neuen Stand.
fn arbeiter(d: &Arc<Dienst>) {
    loop {
        if let Some(t) = jetzt()
            && let Some((nr, kind)) = d.schritt(t)
        {
            let d2 = Arc::clone(d);
            thread::spawn(move || d2.verfolge(nr, kind, jetzt));
        }
        if d.neustart_bereit() {
            println!("[Dienst] Update eingespielt: Neustart.");
            // Die Seite soll den Hinweis noch einmal abholen koennen.
            thread::sleep(Duration::from_secs(3));
            std::process::exit(i32::from(NEUSTART));
        }
        thread::sleep(Duration::from_secs(1));
    }
}

/// Ein offenes Update gilt als bestaetigt, wenn dieser Dienst eine Weile laeuft.
fn bestaetigen(d: &Arc<Dienst>) {
    match rueckfall::offen(&d.wurzel) {
        Ok(None) => {}
        Ok(Some(o)) => {
            if let Ok(e) = depot_dienst::aktualisierung::vermerke(
                &d.wurzel,
                &o,
                depot_dienst::aktualisierung::Phase::Probe,
                "Neuer Dienst läuft zur Probe; Abschlussprüfung steht aus.",
                None,
            ) {
                let mut z = d.sperre();
                z.update_ergebnisse.retain(|alt| alt.neu != e.neu);
                z.update_ergebnisse.push(e);
            }
            d.sperre().update_meldung = Some((
                false,
                format!("Update auf {} läuft zur Probe.", rueckfall::kurz(&o.neu)),
            ));
            let d = Arc::clone(d);
            thread::spawn(move || {
                thread::sleep(rueckfall::BESTAETIGEN_NACH);
                let m = match rueckfall::bestaetige_mit_wiederholung(
                    &d.wurzel,
                    rueckfall::BESTAETIGUNG_WIEDERHOLEN_FUER,
                    rueckfall::BESTAETIGUNG_WIEDERHOLEN_ALLE,
                ) {
                    Ok(Some(o)) => (
                        false,
                        format!("Update auf {} bestätigt.", rueckfall::kurz(&o.neu)),
                    ),
                    Ok(None) => (false, "Kein Update offen.".to_string()),
                    Err(e) => {
                        eprintln!("[Update] {e}");
                        if let Ok(Some(o)) = rueckfall::offen(&d.wurzel) {
                            let _ = depot_dienst::aktualisierung::vermerke(
                                &d.wurzel,
                                &o,
                                depot_dienst::aktualisierung::Phase::Probe,
                                &e,
                                None,
                            );
                        }
                        (true, e)
                    }
                };
                if let Ok(e) = depot_dienst::aktualisierung::laden(&d.wurzel) {
                    d.sperre().update_ergebnisse = e;
                }
                d.sperre().update_meldung = Some(m);
            });
        }
        Err(e) => d.sperre().update_meldung = Some((true, e)),
    }
}

/// Woher dieses Programm kommt: aus dem gesicherten Stand, oder mit Cargo aus einem Baum.
enum Herkunft {
    Unbekannt,
    Stand,
    Gebaut(String),
}

/// Der Starter der Auftraege und was die Einrichtung dazu sagt. Aus dem gesicherten Stand nur,
/// wenn er hier noch einmal startklar ist; sonst ueber Cargo wie bisher.
fn starter_fuer(
    wurzel: &std::path::Path,
    h: &Herkunft,
) -> (
    depot_dienst::werkbank::Starter,
    (bool, String),
    Option<String>,
) {
    match h {
        Herkunft::Stand => match programmstand::startklar(wurzel) {
            Ok(s) => (
                programmstand::starter(&s),
                (
                    false,
                    format!(
                        "Gestartet aus dem geprüften Programmstand {} ohne Cargo. Kurse, Termine und Papiertag laufen ohne Cargo; nur das Update braucht es.",
                        s.kurz()
                    ),
                ),
                programmstand::commit_fuer_baum(wurzel, &s.baum),
            ),
            Err(e) => (
                cargo_starter(),
                (true, format!("Programmstand nicht verwendbar ({e}); Aufträge über Cargo.")),
                None,
            ),
        },
        Herkunft::Gebaut(baum) => (
            cargo_starter(),
            (
                false,
                "Mit Cargo gebaut. Läuft der Dienst gleich eine Weile, wird dieser Stand für den nächsten Start gesichert."
                    .into(),
            ),
            programmstand::commit_fuer_baum(wurzel, baum),
        ),
        Herkunft::Unbekannt => (
            cargo_starter(),
            (
                true,
                "Ohne Wächter gestartet: Kein Programmstand für einen Start ohne Cargo.".into(),
            ),
            None,
        ),
    }
}

/// Sichert nach `BESTAETIGEN_NACH` die gebauten Programme als Stand fuer den naechsten Start.
fn sichern(d: &Arc<Dienst>, baum: String) {
    let d = Arc::clone(d);
    thread::spawn(move || {
        thread::sleep(rueckfall::BESTAETIGEN_NACH);
        // Erst nach einer Probezeit: Ein offenes Update muss zuerst bestaetigt sein.
        thread::sleep(Duration::from_secs(2));
        let von = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(std::path::Path::to_path_buf));
        let m = match von {
            _ if rueckfall::offen_pfad(&d.wurzel).exists() => (
                true,
                "Update noch nicht bestätigt; Programmstand nicht gesichert.".into(),
            ),
            None => (true, "Ort des Programms unbekannt; nicht gesichert.".into()),
            Some(von) => match programmstand::sichere(&d.wurzel, &baum, &von) {
                Ok(s) => (
                    false,
                    format!(
                        "Programmstand {} gesichert. Der nächste Start, auch nach einem Windows-Neustart, braucht kein Cargo.",
                        s.kurz()
                    ),
                ),
                Err(e) => (
                    true,
                    format!(
                        "Programmstand nicht gesichert: {e}. Der nächste Start baut mit Cargo."
                    ),
                ),
            },
        };
        d.sperre().programm_meldung = Some(m);
    });
}

fn main() -> ExitCode {
    let mut ohne_browser = false;
    let mut herkunft = Herkunft::Unbekannt;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--ohne-browser" => ohne_browser = true,
            "--stand" => herkunft = Herkunft::Stand,
            "--baum" => {
                let Some(b) = args.next() else {
                    eprintln!("[Dienst] FEHLER: --baum ohne Baum");
                    return ExitCode::FAILURE;
                };
                herkunft = Herkunft::Gebaut(b);
            }
            _ => {
                eprintln!(
                    "[Dienst] FEHLER: unbekannte Angabe {a:?}. Erlaubt: --ohne-browser, \
                     --stand, --baum <baum>"
                );
                return ExitCode::FAILURE;
            }
        }
    }
    let Some(wurzel) = finde_wurzel() else {
        eprintln!("[Dienst] FEHLER: Projektordner nicht gefunden. DepotSchlopf.cmd dort starten.");
        return ExitCode::FAILURE;
    };
    let lauscher = match TcpListener::bind((::std::net::Ipv4Addr::LOCALHOST, http::PORT)) {
        Ok(l) => l,
        Err(e) if e.kind() == ErrorKind::AddrInUse => {
            println!("[Dienst] Läuft schon.");
            if !ohne_browser {
                oeffne_browser();
            }
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!(
                "[Dienst] FEHLER: 127.0.0.1:{} nicht belegbar: {e}",
                http::PORT
            );
            return ExitCode::FAILURE;
        }
    };
    let token = neues_token();
    let umwelt: Arc<dyn Umwelt> = Arc::new(System);
    let (starter, meldung, version) = starter_fuer(&wurzel, &herkunft);
    let mut dienst = Dienst::neu(wurzel, token, starter, lagerechner(Arc::clone(&umwelt)));
    dienst.version = version;
    let dienst = Arc::new(dienst);
    dienst.sperre().programm_meldung = Some(meldung);
    bestaetigen(&dienst);
    if let Herkunft::Gebaut(baum) = herkunft {
        sichern(&dienst, baum);
    }
    {
        let d = Arc::clone(&dienst);
        thread::spawn(move || arbeiter(&d));
    }
    println!("[Dienst] Bereit. Fenster offen lassen; Schließen beendet den Dienst.");
    if !ohne_browser {
        oeffne_browser();
    }
    let offen = Arc::new(AtomicUsize::new(0));
    for verbindung in lauscher.incoming() {
        let Ok(s) = verbindung else {
            continue;
        };
        if offen.fetch_add(1, Ordering::SeqCst) >= VERBINDUNGEN_MAX {
            offen.fetch_sub(1, Ordering::SeqCst);
            drop(s);
            continue;
        }
        let (d, u, o) = (Arc::clone(&dienst), Arc::clone(&umwelt), Arc::clone(&offen));
        thread::spawn(move || {
            bediene(&d, u.as_ref(), s);
            o.fetch_sub(1, Ordering::SeqCst);
        });
    }
    ExitCode::SUCCESS
}
