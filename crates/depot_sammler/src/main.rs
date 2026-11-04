//! `depot_sammler` – holt Xetra-Schlusskurse, belegt sie im Journal und prueft sie.
//!
//! Aufruf ueber `run.cmd kurse` (oder `run.cmd kurse einrichten` beim ersten Mal). Das Journal
//! liegt unter `daten/journal.txt`, die rohen Antworten unter `daten/roh/`; beides bleibt auf dem
//! eigenen Rechner (`.gitignore`, Export ohne `daten/`).

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

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use depot_journal::Journal;
use depot_kern::{Cent, Datum, Zeitpunkt};
use depot_sammler::{Ergebnis, Https, Schluessel, beobachtungsliste, sammle};

/// Abstand zwischen zwei Abrufen: hoechstens vier Abrufe je Minute.
const PAUSE: Duration = Duration::from_secs(15);

/// Der Projektordner: der naechste Ordner ab dem aktuellen, der `kodex/festschreibung.txt`
/// enthaelt.
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

fn euro(c: Cent) -> String {
    let (e, r) = (c.0.div_euclid(100), c.0.rem_euclid(100));
    format!("{e},{r:02} EUR")
}

fn datum(d: Datum) -> String {
    format!("{:02}.{:02}.{}", d.tag(), d.monat(), d.jahr())
}

fn tag_preis(t: Option<(Datum, Cent)>) -> String {
    t.map_or_else(|| "-".into(), |(d, c)| format!("{} {}", datum(d), euro(c)))
}

fn einrichten(journal: &Path) -> ExitCode {
    if let Some(ordner) = journal.parent()
        && let Err(e) = std::fs::create_dir_all(ordner)
    {
        eprintln!("[Sammler] FEHLER: {}: {e}", ordner.display());
        return ExitCode::FAILURE;
    }
    match Journal::einrichten(journal) {
        Ok(()) => {
            println!("[Sammler] Journal eingerichtet: {}", journal.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("[Sammler] FEHLER: {e:?}");
            ExitCode::FAILURE
        }
    }
}

fn kurse(wurzel: &Path, journal_pfad: &Path) -> ExitCode {
    let werte = match beobachtungsliste() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("[Sammler] FEHLER: {e}");
            return ExitCode::FAILURE;
        }
    };
    let schluessel = match Schluessel::aus_anmeldeverwaltung() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[Sammler] FEHLER: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut journal = match Journal::oeffnen(journal_pfad) {
        Ok(j) => j,
        Err(e) => {
            eprintln!(
                "[Sammler] FEHLER: Journal {}: {e:?}. Beim ersten Mal: run.cmd kurse einrichten",
                journal_pfad.display()
            );
            return ExitCode::FAILURE;
        }
    };
    println!(
        "[Sammler] Hole {} Werte von Alpha Vantage, {} Sekunden Abstand ...",
        werte.len(),
        PAUSE.as_secs()
    );
    let mut netz = Https::neu();
    let mut uhr = jetzt;
    let bericht = match sammle(
        &mut journal,
        &mut netz,
        &schluessel,
        &mut uhr,
        &wurzel.join("daten").join("roh"),
        PAUSE,
        &werte,
    ) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[Sammler] FEHLER, Lauf angehalten: {e:?}");
            return ExitCode::FAILURE;
        }
    };
    let mut fehler = false;
    for (w, e) in &bericht.werte {
        match e {
            Ergebnis::Belegt { heute, vortag, .. } => println!(
                "  {:<20} {}  Schluss {}  Vortag {}",
                w.name,
                w.isin,
                tag_preis(*heute),
                tag_preis(*vortag)
            ),
            Ergebnis::Fehler(t) => {
                fehler = true;
                println!("  {:<20} {}  FEHLER: {t}", w.name, w.isin);
            }
            Ergebnis::NichtAbgerufen => {
                println!("  {:<20} {}  nicht abgerufen", w.name, w.isin);
            }
        }
    }
    if let Some(a) = &bericht.abbruch {
        fehler = true;
        println!("[Sammler] Lauf vorzeitig beendet: {a}");
    }
    match bericht.handelstag {
        None => println!("[Sammler] Keine Schlusskurse belegt."),
        Some(t) if bericht.pruefung.is_empty() => println!(
            "[Sammler] Pruefung fuer den Handelstag {}: in Ordnung.",
            datum(t)
        ),
        Some(t) => {
            println!(
                "[Sammler] Pruefung fuer den Handelstag {} (Hoechstalter 20 Minuten: Schlusskurse sind nur kurz nach 17:35 Uhr frisch):",
                datum(t)
            );
            for f in &bericht.pruefung {
                println!("  {f:?}");
            }
        }
    }
    println!(
        "[Sammler] Belegt im Journal {}, rohe Antworten unter daten/roh/.",
        journal_pfad.display()
    );
    if fehler {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn main() -> ExitCode {
    let Some(wurzel) = wurzel() else {
        eprintln!(
            "[Sammler] FEHLER: Projektordner nicht gefunden. run.cmd im Projektordner starten."
        );
        return ExitCode::FAILURE;
    };
    let journal = wurzel.join("daten").join("journal.txt");
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["kurse"] => kurse(&wurzel, &journal),
        ["einrichten"] | ["kurse", "einrichten"] => einrichten(&journal),
        _ => {
            eprintln!("Aufruf: run.cmd kurse | run.cmd kurse einrichten");
            ExitCode::from(2)
        }
    }
}
