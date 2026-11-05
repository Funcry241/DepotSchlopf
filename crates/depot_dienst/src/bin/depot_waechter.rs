//! `depot_waechter` – startet den Dienst und haelt ihn nach einem Update am Laufen.
//!
//! `DepotSchlopf.cmd` ruft nur ihn. Er baut den Dienst (`cargo build`), startet ihn und wartet:
//!
//! - Endet der Dienst mit `NEUSTART` (Update eingespielt), baut und startet er ihn neu, ohne
//!   Browser; die offene Seite verbindet sich von selbst wieder.
//! - Scheitert der Bau oder endet der Dienst mit Fehler, solange ein Update offen ist
//!   (`daten/update_offen.txt`), setzt er den Projektordner auf den alten Stand zurueck und
//!   startet diesen (`rueckfall`). Dafuer braucht er keinen Code des neuen Stands.
//! - Sonst endet er mit dem Ergebnis des Dienstes.
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
use std::path::Path;
use std::process::{Command, ExitCode};

use depot_dienst::rueckfall::{self, NEUSTART};
use depot_dienst::{finde_wurzel, http, oeffne_browser};

/// So oft startet der Waechter den Dienst hoechstens neu, bevor er aufgibt.
const NEUSTARTS_MAX: u32 = 20;

fn cargo(wurzel: &Path, args: &[&str]) -> Option<i32> {
    match Command::new("cargo")
        .current_dir(wurzel)
        .args(args)
        .status()
    {
        Ok(s) => Some(s.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("[Waechter] FEHLER: cargo nicht startbar: {e}");
            None
        }
    }
}

fn baue(wurzel: &Path) -> bool {
    cargo(
        wurzel,
        &[
            "build",
            "--quiet",
            "-p",
            "depot_dienst",
            "--bin",
            "depot_dienst",
        ],
    ) == Some(0)
}

fn starte(wurzel: &Path, ohne_browser: bool) -> Option<i32> {
    let mut args = vec![
        "run",
        "--quiet",
        "-p",
        "depot_dienst",
        "--bin",
        "depot_dienst",
        "--",
    ];
    if ohne_browser {
        args.push("--ohne-browser");
    }
    cargo(wurzel, &args)
}

/// Setzt ein offenes Update zurueck. `true`: Es gab eins und es ist zurueckgesetzt.
fn zuruecksetzen(wurzel: &Path, warum: &str) -> bool {
    match rueckfall::zuruecksetzen(wurzel) {
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
        if !baue(&wurzel) {
            if zuruecksetzen(&wurzel, "Bau des neuen Stands gescheitert") && baue(&wurzel) {
                // weiter mit dem alten Stand
            } else {
                eprintln!("[Waechter] FEHLER: Dienst laesst sich nicht bauen.");
                return ExitCode::FAILURE;
            }
        }
        let code = starte(&wurzel, ohne_browser);
        let offen = rueckfall::offen_pfad(&wurzel).exists();
        match code {
            Some(0) => return ExitCode::SUCCESS,
            Some(c) if c == i32::from(NEUSTART) && offen => {
                println!("[Waechter] Update eingespielt, Dienst startet neu.");
            }
            _ if offen => {
                if !zuruecksetzen(&wurzel, "Neuer Stand kam nicht hoch") {
                    return ExitCode::FAILURE;
                }
            }
            _ => {
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
