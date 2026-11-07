//! `depot_waechter` – startet den Dienst und haelt ihn nach einem Update am Laufen.
//!
//! `DepotSchlopf.cmd` ruft nur ihn. Er baut den Dienst (`cargo build`), startet ihn und wartet:
//!
//! - Endet der Dienst mit `NEUSTART` (Update eingespielt), baut und startet er ihn neu, ohne
//!   Browser; die offene Seite verbindet sich von selbst wieder.
//! - Liegt beim Start `daten/update_offen.txt`, ist der neue Stand auf Probe (`waechter`):
//!   Scheitert der Bau, endet der Dienst vor seiner Bestaetigung (mit jedem Code, auch 0) oder
//!   bestaetigt er sich nicht binnen `FRIST`, beendet der Waechter ihn, setzt den Projektordner
//!   auf den alten Stand zurueck und startet diesen (`rueckfall`). Dafuer braucht er keinen
//!   Code des neuen Stands.
//! - Sonst endet er mit dem Ergebnis des Dienstes.
//!
//! - **Ohne Cargo** (Maus, 05.11.2026): Ist ein gesicherter Programmstand startklar
//!   (`programmstand`), startet er dessen Dienst direkt, ohne zu bauen. Sonst baut er mit Cargo
//!   und sagt dem Dienst, aus welchem Baum; laeuft der eine Weile, sichert er sich als Stand.
//!
//! Der Waechter selbst wird erst beim naechsten Start von `DepotSchlopf.cmd` neu gebaut.

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

use std::io::ErrorKind;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use depot_dienst::programmstand;
use depot_dienst::rueckfall::{self, NEUSTART};
use depot_dienst::waechter::{Ausgang, FRIST, TAKT, begleite, programm_aus_bau};
use depot_dienst::{finde_wurzel, http, oeffne_browser};

/// So oft startet der Waechter den Dienst hoechstens neu, bevor er aufgibt.
const NEUSTARTS_MAX: u32 = 20;

/// Baut den Dienst und nennt seine Programmdatei. Der Waechter startet sie direkt, nicht ueber
/// `cargo run`: So beendet er bei abgelaufener Frist wirklich den Dienst und nicht nur Cargo.
fn baue(wurzel: &Path) -> Option<PathBuf> {
    let aus = depot_dienst::waechter::baubefehl(wurzel)
        .stderr(Stdio::inherit())
        .output();
    match aus {
        Ok(a) if a.status.success() => {
            let p = programm_aus_bau(&String::from_utf8_lossy(&a.stdout));
            if p.is_none() {
                eprintln!("[Waechter] FEHLER: Cargo nennt keine Programmdatei des Dienstes.");
            }
            p
        }
        Ok(_) => None,
        Err(e) => {
            eprintln!("[Waechter] FEHLER: cargo nicht startbar: {e}");
            None
        }
    }
}

/// Wie der Dienst gestartet wird: aus dem gesicherten Stand oder frisch gebaut aus einem Baum.
enum Quelle {
    Stand,
    Gebaut(Option<String>),
}

fn starte(wurzel: &Path, programm: &Path, ohne_browser: bool, quelle: &Quelle) -> Ausgang {
    let mut c = Command::new(programm);
    c.current_dir(wurzel);
    if ohne_browser {
        c.arg("--ohne-browser");
    }
    match quelle {
        Quelle::Stand => {
            c.arg("--stand");
        }
        Quelle::Gebaut(Some(baum)) => {
            c.args(["--baum", baum]);
        }
        Quelle::Gebaut(None) => {}
    }
    match c.spawn() {
        Ok(mut kind) => begleite(&mut kind, &rueckfall::offen_pfad(wurzel), FRIST, TAKT),
        Err(e) if rueckfall::offen_pfad(wurzel).exists() => {
            Ausgang::Rueckfall(format!("Neuer Stand nicht startbar: {e}"))
        }
        Err(e) => {
            eprintln!("[Waechter] FEHLER: Dienst nicht startbar: {e}");
            Ausgang::Ende(None)
        }
    }
}

/// Setzt ein offenes Update zurueck. `true`: Es gab eins und es ist zurueckgesetzt.
fn zuruecksetzen(wurzel: &Path, warum: &str) -> bool {
    match rueckfall::zuruecksetzen_mit_grund(wurzel, warum) {
        Ok(Some(m)) => {
            eprintln!("[Waechter] {warum}. {m}");
            true
        }
        Ok(None) => false,
        Err(e) => {
            eprintln!("[Waechter] {warum}. Rueckfall nicht moeglich: {e}");
            false
        }
    }
}

/// Laeuft schon ein Dienst? Dann belegt er den Port. Der Waechter haelt ihn nur kurz.
fn laeuft_schon() -> bool {
    match TcpListener::bind((::std::net::Ipv4Addr::LOCALHOST, http::PORT)) {
        Ok(_) => false,
        Err(e) => e.kind() == ErrorKind::AddrInUse,
    }
}

fn main() -> ExitCode {
    let mut ohne_browser = false;
    for a in std::env::args().skip(1) {
        if a == "--ohne-browser" {
            ohne_browser = true;
        } else {
            eprintln!("[Waechter] FEHLER: unbekannte Angabe {a:?}. Erlaubt: --ohne-browser");
            return ExitCode::FAILURE;
        }
    }
    let Some(wurzel) = finde_wurzel() else {
        eprintln!(
            "[Waechter] FEHLER: Projektordner nicht gefunden. DepotSchlopf.cmd dort starten."
        );
        return ExitCode::FAILURE;
    };
    // Ein zweiter Start baut nichts (die laufende Datei ist unter Windows gesperrt) und setzt
    // nichts zurueck; er oeffnet nur die Seite.
    if laeuft_schon() {
        println!("[Waechter] Dienst laeuft schon.");
        if !ohne_browser {
            oeffne_browser();
        }
        return ExitCode::SUCCESS;
    }
    let mut neustarts = 0_u32;
    loop {
        // Erst der gesicherte Stand: Er braucht kein Cargo.
        match programmstand::startklar(&wurzel) {
            Ok(stand) => {
                println!(
                    "[Waechter] Starte den gesicherten Programmstand {} ohne Cargo.",
                    stand.kurz()
                );
                match starte(
                    &wurzel,
                    &stand.programm("depot_dienst"),
                    ohne_browser,
                    &Quelle::Stand,
                ) {
                    Ausgang::Ende(Some(0)) => return ExitCode::SUCCESS,
                    Ausgang::Ende(Some(c))
                        if c == i32::from(NEUSTART) && rueckfall::offen_pfad(&wurzel).exists() =>
                    {
                        println!("[Waechter] Update eingespielt, Dienst startet neu.");
                        ohne_browser = true;
                        continue;
                    }
                    Ausgang::Ende(code) => {
                        eprintln!("[Waechter] Dienst endete mit Fehler ({code:?}).");
                        return ExitCode::FAILURE;
                    }
                    Ausgang::Rueckfall(warum) => {
                        eprintln!("[Waechter] {warum}");
                        return ExitCode::FAILURE;
                    }
                }
            }
            Err(grund) => println!("[Waechter] Kein Start ohne Cargo: {grund}. Baue mit Cargo."),
        }
        // Der Baum vor dem Bau: Nur wenn er danach noch derselbe ist, wird gesichert.
        let baum = programmstand::sauberer_baum(&wurzel).ok();
        let gebaut = baue(&wurzel).or_else(|| {
            zuruecksetzen(&wurzel, "Bau des neuen Stands gescheitert")
                .then(|| baue(&wurzel))
                .flatten()
        });
        let Some(programm) = gebaut else {
            eprintln!("[Waechter] FEHLER: Dienst laesst sich nicht bauen.");
            return ExitCode::FAILURE;
        };
        match starte(&wurzel, &programm, ohne_browser, &Quelle::Gebaut(baum)) {
            Ausgang::Rueckfall(warum) => {
                if !zuruecksetzen(&wurzel, &warum) {
                    return ExitCode::FAILURE;
                }
            }
            Ausgang::Ende(Some(0)) => return ExitCode::SUCCESS,
            // Code 3 mit neuer Markierung: ein Update ist eben eingespielt.
            Ausgang::Ende(Some(c))
                if c == i32::from(NEUSTART) && rueckfall::offen_pfad(&wurzel).exists() =>
            {
                println!("[Waechter] Update eingespielt, Dienst startet neu.");
            }
            Ausgang::Ende(code) => {
                eprintln!("[Waechter] Dienst endete mit Fehler ({code:?}).");
                return ExitCode::FAILURE;
            }
        }
        neustarts = neustarts.saturating_add(1);
        if neustarts > NEUSTARTS_MAX {
            eprintln!("[Waechter] FEHLER: zu viele Neustarts, gebe auf.");
            return ExitCode::FAILURE;
        }
        ohne_browser = true;
    }
}
