//! `kodex` – das Gate. Aufruf: `kodex [--basis <ref>] [--voll]`, `kodex festschreibung` oder
//! `kodex freigabe [--basis <ref>]` (Vorlage der Grenzfreigabe fuer den aktuellen Diff).

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

mod testumgebung;

use std::process::{Command, ExitCode};
use std::time::Instant;

fn cargo(args: &[&str]) -> bool {
    let schritt = args.join(" ");
    let start = Instant::now();
    println!("[Kodex] cargo {schritt}");
    let mut befehl = Command::new("cargo");
    befehl.args(args);
    let _testumgebung = if args.first() == Some(&"test") {
        match testumgebung::Testumgebung::neu() {
            Ok(umgebung) => {
                umgebung.anwenden(&mut befehl);
                println!(
                    "[Kodex] Tests laufen mit eigenen temporären Dateien und eigener Git-Konfiguration."
                );
                Some(umgebung)
            }
            Err(e) => {
                eprintln!("[Kodex] {e}");
                return false;
            }
        }
    } else {
        None
    };
    let erfolgreich = befehl.status().is_ok_and(|s| s.success());
    println!(
        "[Kodex] cargo {schritt} · Dauer: {} ms",
        start.elapsed().as_millis()
    );
    erfolgreich
}

fn basis_aus(args: &[String]) -> &str {
    args.iter()
        .position(|a| a == "--basis")
        .and_then(|i| args.get(i.saturating_add(1)))
        .map_or("origin/main", String::as_str)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(wurzel) = depot_kodex::finde_wurzel() else {
        eprintln!(
            "[Kodex] FEHLER: Projektordner nicht gefunden (Cargo.toml mit kodex/festschreibung.txt)"
        );
        return ExitCode::FAILURE;
    };

    if args.first().map(String::as_str) == Some("festschreibung") {
        return match depot_kodex::festschreibung::berechne(&wurzel) {
            Ok(f) => {
                let ziel = wurzel.join(depot_kodex::FESTSCHREIBUNG);
                match std::fs::write(&ziel, depot_kodex::festschreibung::als_text(&f)) {
                    Ok(()) => {
                        println!("[Kodex] {} geschrieben", ziel.display());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("[Kodex] FEHLER: {e}");
                        ExitCode::FAILURE
                    }
                }
            }
            Err(e) => {
                eprintln!("[Kodex] FEHLER: {e}");
                ExitCode::FAILURE
            }
        };
    }

    let basis = basis_aus(&args);

    if args.first().map(String::as_str) == Some("freigabe") {
        // Schreibt nichts und meldet nie ein offenes Gate (R27): nur die Vorlage.
        let vorlage = depot_kodex::git::aenderungen(&wurzel, basis)
            .and_then(|d| depot_kodex::freigabe::vorlage(&wurzel, &d.pfade));
        return match vorlage {
            Ok(v) => {
                println!("[Kodex] Vorlage für {}:", depot_kodex::freigabe::FREIGABE);
                print!("{v}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("[Kodex] FEHLER: {e}");
                ExitCode::FAILURE
            }
        };
    }

    let voll = args.iter().any(|a| a == "--voll");

    let fehler = if wurzel.join(".git").exists() {
        println!("[Kodex] Basis: {basis}");
        depot_kodex::pruefe(&wurzel, basis)
    } else {
        println!(
            "[Kodex] HINWEIS: kein Git-Arbeitsbaum. Deklaration und Pfadrollen nicht prüfbar; \
             massgeblich bleibt das Gate auf GitHub."
        );
        depot_kodex::pruefe_ohne_diff(&wurzel)
    };
    if !fehler.is_empty() {
        for f in &fehler {
            eprintln!("[Kodex] ABGELEHNT: {f}");
        }
        eprintln!("[Kodex] {} Verstoss(e). Gate geschlossen.", fehler.len());
        return ExitCode::FAILURE;
    }
    println!("[Kodex] Struktur, Festschreibung, Quellscan, Essenz: in Ordnung");

    if voll {
        let schritte: [&[&str]; 3] = [
            &["fmt", "--all", "--check"],
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
            &["test", "--workspace", "--no-fail-fast"],
        ];
        for s in schritte {
            if !cargo(s) {
                eprintln!("[Kodex] ABGELEHNT: cargo {} fehlgeschlagen", s.join(" "));
                return ExitCode::FAILURE;
            }
        }
    }
    println!("[Kodex] Gate offen.");
    ExitCode::SUCCESS
}
