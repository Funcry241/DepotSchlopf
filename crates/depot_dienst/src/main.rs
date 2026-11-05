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
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use depot_dienst::rueckfall::{self, NEUSTART};
use depot_dienst::werkbank::{Dienst, cargo_starter};
use depot_dienst::{
    System, Umwelt, behandle, finde_wurzel, http, lagerechner, neues_token, oeffne_browser,
};
use depot_kern::Zeitpunkt;

/// So viele Verbindungen werden hoechstens zugleich bedient; weitere werden geschlossen.
const VERBINDUNGEN_MAX: usize = 32;
fn jetzt() -> Option<Zeitpunkt> {
    let s = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    Zeitpunkt::aus_sekunden(i64::try_from(s).ok()?)
}

fn bediene(d: &Dienst, u: &dyn Umwelt, mut strom: TcpStream) {
    let _ = strom.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = strom.set_write_timeout(Some(Duration::from_secs(10)));
    let antwort = match http::lies(&mut strom) {
        Ok(anfrage) => match jetzt() {
            Some(t) => behandle(d, u, &anfrage, t),
            None => http::text(500, "Uhr liefert keinen Zeitpunkt"),
        },
        Err(http::LeseFehler::ZuGross) => http::text(413, "Anfrage zu gross"),
        Err(http::LeseFehler::Kaputt) => http::text(400, "Anfrage unlesbar"),
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
            d.sperre().update_meldung = Some((
                false,
                format!("Update auf {} läuft zur Probe.", rueckfall::kurz(&o.neu)),
            ));
            let d = Arc::clone(d);
            thread::spawn(move || {
                thread::sleep(rueckfall::BESTAETIGEN_NACH);
                let m = match rueckfall::bestaetige(&d.wurzel) {
                    Ok(Some(o)) => (
                        false,
                        format!("Update auf {} bestätigt.", rueckfall::kurz(&o.neu)),
                    ),
                    Ok(None) => (false, "Kein Update offen.".to_string()),
                    Err(e) => (true, e),
                };
                d.sperre().update_meldung = Some(m);
            });
        }
        Err(e) => d.sperre().update_meldung = Some((true, e)),
    }
}

fn main() -> ExitCode {
    let mut ohne_browser = false;
    for a in std::env::args().skip(1) {
        if a == "--ohne-browser" {
            ohne_browser = true;
        } else {
            eprintln!("[Dienst] FEHLER: unbekannte Angabe {a:?}. Erlaubt: --ohne-browser");
            return ExitCode::FAILURE;
        }
    }
    let Some(wurzel) = finde_wurzel() else {
        eprintln!("[Dienst] FEHLER: Projektordner nicht gefunden. DepotSchlopf.cmd dort starten.");
        return ExitCode::FAILURE;
    };
    let lauscher = match TcpListener::bind((::std::net::Ipv4Addr::LOCALHOST, http::PORT)) {
        Ok(l) => l,
        Err(e) if e.kind() == ErrorKind::AddrInUse => {
            println!("[Dienst] Laeuft schon.");
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
    let dienst = Arc::new(Dienst::neu(
        wurzel,
        token.clone(),
        cargo_starter(),
        lagerechner(token, Arc::clone(&umwelt)),
    ));
    bestaetigen(&dienst);
    {
        let d = Arc::clone(&dienst);
        thread::spawn(move || arbeiter(&d));
    }
    println!("[Dienst] Bereit. Fenster offen lassen; Schliessen beendet den Dienst.");
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
