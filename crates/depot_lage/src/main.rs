//! `depot_lage` – schreibt das Lagebild nach `daten/lage.html`.
//!
//! Aufruf ueber `run.cmd lage`. Liest das Journal `daten/journal.txt` und schreibt nur die Seite;
//! kein Netz, keine Schluessel. Ob die Schluessel hinterlegt sind, prueft erst der Dienst.

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

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use depot_kern::{BeraterId, Zeitpunkt};
use depot_lage::{
    Hinterlegt, Umgebung, anbieterprotokoll, html, ki_halt, lagebild, mit_journal, sperrdateien,
};

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

fn schreibe(pfad: &Path, text: &str) -> Result<(), String> {
    let neben = pfad.with_extension("html.neu");
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&neben)
        .and_then(|mut f| {
            f.write_all(text.as_bytes())?;
            f.sync_all()
        })
        .map_err(|e| format!("{}: {e}", neben.display()))?;
    fs::rename(&neben, pfad).map_err(|e| format!("{}: {e}", pfad.display()))
}

fn lauf(wurzel: &Path) -> Result<PathBuf, String> {
    let daten = wurzel.join("daten");
    let jetzt = jetzt().ok_or("Uhr liefert keinen Zeitpunkt")?;
    let sperren = sperrdateien(&daten);
    let (anbieter, anbieter_unlesbar) = anbieterprotokoll(&daten);
    let l = mit_journal(&daten, |journal| {
        lagebild(&Umgebung {
            jetzt,
            journal,
            kursquelle: Hinterlegt::Unbekannt,
            kursquelle_us: Hinterlegt::Unbekannt,
            berater: [
                (BeraterId::Dachs, Hinterlegt::Unbekannt),
                (BeraterId::Maus, Hinterlegt::Unbekannt),
            ],
            sperren,
            anbieter,
            anbieter_unlesbar,
            ki_halt: ki_halt(&daten),
        })
    });
    fs::create_dir_all(&daten).map_err(|e| format!("{}: {e}", daten.display()))?;
    let ziel = daten.join("lage.html");
    schreibe(&ziel, &html::seite(&l))?;
    println!("[Lage] {}", l.naechster_schritt);
    for p in &l.punkte {
        println!("  [{:?}] {}: {}", p.ampel, p.name, p.text);
    }
    Ok(ziel)
}

fn main() -> ExitCode {
    let Some(wurzel) = wurzel() else {
        eprintln!("[Lage] FEHLER: Projektordner nicht gefunden. run.cmd im Projektordner starten.");
        return ExitCode::FAILURE;
    };
    match lauf(&wurzel) {
        Ok(ziel) => {
            println!("[Lage] Seite: {}", ziel.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("[Lage] FEHLER: {e}");
            ExitCode::FAILURE
        }
    }
}
