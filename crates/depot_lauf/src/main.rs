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
mod dateiersatz;
mod export;
mod sicherung;
mod update;
mod updatedaten;

use std::fs;
use std::path::Path;
use std::process::ExitCode;

const HILFE: &str = "Aufruf:\n  run.cmd                  alles: Gate prüfen und Assistenten-Export schreiben\n  \
run.cmd kodex            dasselbe\n  run.cmd festschreibung   Festschreibung neu schreiben (nur für FestschreibungAenderung)\n  \
run.cmd kurse            Schlusskurse und Nachrichten holen, im Journal belegen und prüfen (nur lesend)\n  \
run.cmd kurse einrichten Journal daten/journal.txt einmal anlegen\n  \
run.cmd nachrichten      nur die Nachrichten holen und im Journal belegen (nur lesend)\n  \
run.cmd termine          Termine der EZB holen und im Journal belegen (nur lesend)\n  \
run.cmd papier           Papiertag mit Dachs und Maus und den heute belegten Schlusskursen (nur Papiergeld)\n  \
run.cmd lage             Lagebild nach daten/lage.html: Bereitschaft, KI-Budget, Depot, Gespräch\n  \
run.cmd sichern          Daten am eingestellten Ziel sichern und prüfen\n  \
run.cmd update prüfen   neue Stände auf main nennen (git fetch, ändert nichts)\n  \
run.cmd update           neuen Stand im Prüfordner mit dem Gate prüfen, nur dann vorwärts einspielen\n  \
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
                    "daten/{name} liegt noch: früherer Lauf abgebrochen. KI-Aufrufe bleiben gesperrt, \
                     bis ein Mensch geprüft und die Datei gelöscht hat."
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

/// Startet den Sammler im Projektordner und gibt seinen Ausgang weiter.
fn sammler(wurzel: &Path, args: &[String]) -> ExitCode {
    programm(wurzel, "depot_sammler", args)
}

/// Startet ein Programm des Arbeitsbereichs im Projektordner und gibt seinen Ausgang weiter.
fn programm(wurzel: &Path, paket: &str, args: &[String]) -> ExitCode {
    let mut c = std::process::Command::new("cargo");
    c.current_dir(wurzel)
        .args(["run", "--quiet", "-p", paket, "--"])
        .args(args);
    match c.status() {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("[DepotSchlopf] FEHLER: cargo nicht startbar: {e}");
            ExitCode::FAILURE
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

    // Der Sammler laeuft fuer sich: kein Gate, kein Export. Netz hat nur er (Kodex).
    if befehl == "kurse" || befehl == "nachrichten" || befehl == "termine" {
        return sammler(&wurzel, &args);
    }
    // Der Papiertag laeuft fuer sich: kein Netz, nur Papiergeld.
    if matches!(befehl.as_str(), "papier" | "vorbereiten" | "gemini-vorlauf") {
        return programm(&wurzel, "depot_berater", &args);
    }
    // Das Lagebild liest nur und schreibt nur daten/lage.html: kein Netz, keine Schluessel.
    if befehl == "lage" {
        return programm(&wurzel, "depot_lage", &optionen);
    }
    // Das Update prueft den neuen Stand in einer eigenen Kopie und rueckt nur vorwaerts vor.
    if befehl == "sichern" {
        return sicherung::lauf(&wurzel, &optionen);
    }
    if befehl == "update" {
        return update::lauf(&wurzel, &optionen);
    }

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
        if let Some(a) = lauf
            .abbild
            .as_ref()
            .filter(|_| lauf.befehl != "festschreibung")
        {
            update::vermerke_geprueft(&wurzel, &a.baum);
        }
        println!("[DepotSchlopf] Fertig.");
        ExitCode::SUCCESS
    } else {
        eprintln!("[DepotSchlopf] Lauf mit Fehlern beendet. Details im Export.");
        ExitCode::FAILURE
    }
}
