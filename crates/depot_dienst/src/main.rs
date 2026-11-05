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
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use depot_dienst::werkbank::{Dienst, cargo_starter};
use depot_dienst::{System, Umwelt, behandle, http, lagerechner, neues_token};
use depot_kern::Zeitpunkt;

/// So viele Verbindungen werden hoechstens zugleich bedient; weitere werden geschlossen.
const VERBINDUNGEN_MAX: usize = 32;
const ADRESSE: &str = "http://127.0.0.1:47410/";

fn wurzel() -> Option<PathBuf> {
    let start = std::env::current_dir().ok()?;
    start
        .ancestors()
        .find(|d| d.join("kodex").join("festschreibung.txt").is_file())
        .map(Path::to_path_buf)
}

fn jetzt() -> Option<Zeitpunkt> {
    let s = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    Zeitpunkt::aus_sekunden(i64::try_from(s).ok()?)
}

/// Oeffnet die Seite im Standardbrowser.
fn browser() {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", ADRESSE])
            .spawn();
    }
    println!("[Dienst] Oberflaeche: {ADRESSE}");
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
fn arbeiter(d: &Arc<Dienst>) {
    loop {
        if let Some(t) = jetzt()
            && let Some((nr, kind)) = d.schritt(t)
        {
            let d2 = Arc::clone(d);
            thread::spawn(move || d2.verfolge(nr, kind, jetzt));
        }
        thread::sleep(Duration::from_secs(1));
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
    let Some(wurzel) = wurzel() else {
        eprintln!("[Dienst] FEHLER: Projektordner nicht gefunden. DepotSchlopf.cmd dort starten.");
        return ExitCode::FAILURE;
    };
    let lauscher = match TcpListener::bind((::std::net::Ipv4Addr::LOCALHOST, http::PORT)) {
        Ok(l) => l,
        Err(e) if e.kind() == ErrorKind::AddrInUse => {
            println!("[Dienst] Laeuft schon.");
            if !ohne_browser {
                browser();
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
    {
        let d = Arc::clone(&dienst);
        thread::spawn(move || arbeiter(&d));
    }
    println!("[Dienst] Bereit. Fenster offen lassen; Schliessen beendet den Dienst.");
    if !ohne_browser {
        browser();
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
