//! `depotschlopf` – der Ein-Befehl-Einstieg hinter `run.cmd`.
//!
//! `run.cmd` ohne Argument macht alles: Wiederherstellungs-Pruefung, Gate (Struktur,
//! Festschreibung, Quellscan, Essenz, Format, Clippy, Tests) und am Ende immer den
//! Assistenten-Export `assistant_export_latest.zip`, auch wenn ein Schritt scheitert.

// Die harten Lints stehen auf `forbid`: Der Compiler lehnt jede lokale Ausnahme ab, auch eine
// hinter `cfg_attr` (R26, Befund 1·G01). Die Liste haelt der Kodex fest.
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

mod abbild;
mod export;
mod zip;

use std::fs;
use std::path::Path;
use std::process::ExitCode;

const HILFE: &str = "Aufruf:\n  run.cmd                  alles: Gate pruefen und Assistenten-Export schreiben\n  \
run.cmd kodex            dasselbe\n  run.cmd festschreibung   Festschreibung neu schreiben (nur fuer FestschreibungAenderung)\n  \
run.cmd hilfe            diese Hilfe\nOptionen: --basis <ref> (Standard origin/main)";

/// Offene Reste frueherer Laeufe: Eine liegengebliebene Sperrdatei bedeutet einen Absturz
/// waehrend eines Laufs. Sie wird nicht automatisch geloescht (fail-closed).
fn wiederherstellung(wurzel: &Path) -> Vec<String> {
    let mut warnungen = Vec::new();
    if let Ok(eintraege) = fs::read_dir(wurzel.join("daten")) {
        for e in eintraege.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(".sperre") {
                warnungen.push(format!(
                    "daten/{name} liegt noch: frueherer Lauf abgebrochen. KI-Aufrufe bleiben gesperrt, \
                     bis ein Mensch geprueft und die Datei geloescht hat."
                ));
            }
        }
    }
    warnungen
}

/// Startet das Gate im Ordner `dir` mit dem Build-Ordner `bau`.
fn kodex(dir: &Path, bau: &Path, konsole: &mut String, args: &[String]) -> i32 {
    let zeile = format!(
        "[DepotSchlopf] cargo run -p depot_kodex -- {}",
        args.join(" ")
    );
    println!("{zeile}");
    konsole.push_str(&zeile);
    konsole.push('\n');
    match abbild::gate_befehl(dir, bau, args).output() {
        Ok(aus) => {
            let o = String::from_utf8_lossy(&aus.stdout);
            let e = String::from_utf8_lossy(&aus.stderr);
            print!("{o}");
            eprint!("{e}");
            konsole.push_str(&o);
            konsole.push_str(&e);
            aus.status.code().unwrap_or(1)
        }
        Err(err) => {
            let z = format!("[DepotSchlopf] FEHLER: cargo nicht startbar: {err}\n");
            eprint!("{z}");
            konsole.push_str(&z);
            1
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let befehl = args
        .first()
        .filter(|a| !a.starts_with("--"))
        .map_or("alles", String::as_str)
        .to_string();
    let optionen: Vec<String> = args.iter().filter(|a| **a != befehl).cloned().collect();
    let Some(wurzel) = depot_kodex::finde_wurzel() else {
        eprintln!(
            "[DepotSchlopf] FEHLER: Projektordner nicht gefunden. run.cmd im Projektordner starten."
        );
        return ExitCode::FAILURE;
    };

    let mut kodex_args: Vec<String> = match befehl.as_str() {
        "alles" | "kodex" => vec!["--voll".into()],
        "festschreibung" => vec!["festschreibung".into()],
        "hilfe" | "help" => {
            println!("{HILFE}");
            return ExitCode::SUCCESS;
        }
        anders => {
            eprintln!("[DepotSchlopf] unbekannter Befehl {anders:?}\n{HILFE}");
            return ExitCode::from(2);
        }
    };
    kodex_args.extend(optionen);

    let mut konsole = format!("[DepotSchlopf] Befehl: {befehl}\n");
    let warnungen = wiederherstellung(&wurzel);
    for w in &warnungen {
        let z = format!("[DepotSchlopf] WARNUNG: {w}\n");
        eprint!("{z}");
        konsole.push_str(&z);
    }

    // Gate und Export nehmen denselben festgehaltenen Stand (1·R04, K14-01). Nur das
    // Schreiben der Festschreibung arbeitet im Projektordner selbst; es oeffnet nie das Gate.
    let lauf = if befehl == "festschreibung" {
        let exit = kodex(&wurzel, &wurzel.join("target"), &mut konsole, &kodex_args);
        export::Lauf {
            befehl,
            schritte: vec![export::Schritt {
                name: "kodex".into(),
                exit,
            }],
            konsole,
            warnungen,
            abbild: None,
        }
    } else {
        abbild::gepruefter_lauf(&wurzel, befehl, konsole, warnungen, |dir, k| {
            kodex(dir, &abbild::bau_ordner(&wurzel), k, &kodex_args)
        })
    };
    match export::schreibe(&wurzel, &lauf) {
        Ok(p) => println!("[DepotSchlopf] Assistenten-Export: {}", p.display()),
        Err(e) => {
            eprintln!("[DepotSchlopf] FEHLER beim Assistenten-Export: {e}");
            return ExitCode::FAILURE;
        }
    }
    if lauf.erfolgreich() {
        println!("[DepotSchlopf] Fertig.");
        ExitCode::SUCCESS
    } else {
        eprintln!("[DepotSchlopf] Lauf mit Fehlern beendet. Details im Export.");
        ExitCode::FAILURE
    }
}
