//! `kodex` – das Gate. Aufruf: `kodex [--basis <ref>] [--voll]` oder `kodex festschreibung`.

use std::path::PathBuf;
use std::process::{Command, ExitCode};

fn cargo(args: &[&str]) -> bool {
    println!("[Kodex] cargo {}", args.join(" "));
    Command::new("cargo")
        .args(args)
        .status()
        .is_ok_and(|s| s.success())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let wurzel = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");

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

    let basis = args
        .iter()
        .position(|a| a == "--basis")
        .and_then(|i| args.get(i.saturating_add(1)))
        .map_or("origin/main", String::as_str);
    let voll = args.iter().any(|a| a == "--voll");

    let fehler = if wurzel.join(".git").exists() {
        println!("[Kodex] Basis: {basis}");
        depot_kodex::pruefe(&wurzel, basis)
    } else {
        println!(
            "[Kodex] HINWEIS: kein Git-Arbeitsbaum. Deklaration und Pfadrollen nicht pruefbar; \
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
            &["test", "--workspace"],
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
