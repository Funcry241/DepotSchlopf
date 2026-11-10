//! `depot_berater papier` – ein Papiertag mit den heute belegten Schlusskursen, mit Dachs
//! (Claude) und Maus (GPT) als Beratern.
//!
//! Aufruf ueber `run.cmd papier`, nach `run.cmd kurse`. Fehlt ein Schluessel oder liegt die
//! Sperrdatei `daten/ki.halt`, ist der Berater nicht angebunden: Er wird nicht gefragt, und es
//! entstehen nur zwingende Auftraege des Regelwerks.

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
use std::time::{SystemTime, UNIX_EPOCH};

use depot_berater::{Anbieter, Https, KiBerater, SPERRDATEI, Schluessel};
use depot_kern::liste::{als_universum, beobachtungsliste};
use depot_kern::{BeraterId, Datum, Zeitpunkt};
use depot_sim::papierlauf::papier;

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

fn heute() -> Option<Datum> {
    jetzt()?.datum()
}

fn schluessel(wer: BeraterId) -> Option<Schluessel> {
    match Schluessel::aus_anmeldeverwaltung(Anbieter::fuer(wer)) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("[Papier] {wer:?} nicht angebunden: {e}");
            None
        }
    }
}

fn laufkennung() -> String {
    std::env::var(depot_journal::gespraeche::LAUF_ENV)
        .ok()
        .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit() || b == b'-'))
        .unwrap_or_else(|| {
            format!(
                "hand-{}-{}",
                jetzt().map_or(0, Zeitpunkt::sekunden),
                std::process::id()
            )
        })
}

fn lauf(wurzel: &Path, vorbereitung: bool) -> Result<(), String> {
    let werte = beobachtungsliste()?;
    let universum = als_universum(&werte);
    let heute = heute().ok_or("Uhr liefert kein Datum")?;
    let sperre = wurzel.join("daten").join(SPERRDATEI);
    let protokoll = wurzel.join("daten").join(depot_journal::anbieter::DATEI);
    if sperre.exists() {
        eprintln!(
            "[Papier] {} liegt: KIs gesperrt, niemand wird gefragt. Grund in der Datei; nach der Prüfung löschen.",
            sperre.display()
        );
    }
    let lauf = laufkennung();
    let zuarbeit = jetzt()
        .and_then(Zeitpunkt::in_frankfurt)
        .map_or_else(depot_berater::zusatz::Zuarbeit::default, |(tag, ..)| {
            depot_berater::zusatz::Zuarbeit::lesen(&wurzel.join("daten"), tag, &universum)
        });
    let dachs_schluessel = schluessel(BeraterId::Dachs);
    let maus_schluessel = schluessel(BeraterId::Maus);
    let mut dachs = KiBerater::neu(
        BeraterId::Dachs,
        dachs_schluessel.clone(),
        Https::neu(),
        universum.clone(),
        sperre.clone(),
    )
    .mit_protokoll(protokoll.clone(), jetzt)
    .mit_gespraechen(lauf.clone(), maus_schluessel.clone())
    .mit_zuarbeit(zuarbeit.clone());
    let mut maus = KiBerater::neu(
        BeraterId::Maus,
        maus_schluessel,
        Https::neu(),
        universum.clone(),
        sperre.clone(),
    )
    .mit_protokoll(protokoll, jetzt)
    .mit_gespraechen(lauf, dachs_schluessel)
    .mit_zuarbeit(zuarbeit);
    if vorbereitung {
        let zeit = jetzt().ok_or("Uhr liefert keine Zeit")?;
        dachs = dachs.fuer_vorbereitung(zeit);
        maus = maus.fuer_vorbereitung(zeit);
        let text =
            depot_sim::vorbereitung::lauf(wurzel, zeit, &universum, &mut dachs, &mut maus, jetzt)?;
        println!("[Vorbereitung] {text}");
        return Ok(());
    }
    for (name, b) in [("Dachs", &dachs), ("Maus", &maus)] {
        if let Some(p) = b.pause() {
            eprintln!("[Papier] {name} pausiert ({p}): wird nicht gefragt, nichts reserviert.");
        }
    }
    let ergebnis = papier(wurzel, heute, &universum, &mut dachs, &mut maus);
    if dachs.gesperrt() || maus.gesperrt() {
        eprintln!(
            "[Papier] KIs gesperrt am Kostenschutz: {}. Prüfen, dann löschen.",
            sperre.display()
        );
    }
    for z in ergebnis? {
        println!("[Papier] {z}");
    }
    Ok(())
}

fn main() -> ExitCode {
    let Some(wurzel) = wurzel() else {
        eprintln!(
            "[Papier] FEHLER: Projektordner nicht gefunden. run.cmd im Projektordner starten."
        );
        return ExitCode::FAILURE;
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(args.as_slice(), [befehl] if befehl == "gemini-vorlauf")
        || matches!(args.as_slice(), [befehl, modus] if befehl == "gemini-vorlauf" && modus == "papiertag")
    {
        if let (Some(zeit), Ok(werte)) = (jetzt(), beobachtungsliste()) {
            depot_berater::zusatz::vorlauf(
                &wurzel.join("daten"),
                zeit,
                &laufkennung(),
                &als_universum(&werte),
                args.len() == 2,
            );
        }
        return ExitCode::SUCCESS;
    }
    if args.as_slice() != ["papier"] && args.as_slice() != ["vorbereiten"] {
        eprintln!("[Papier] Aufruf: run.cmd papier oder run.cmd vorbereiten");
        return ExitCode::from(2);
    }
    match lauf(&wurzel, args.as_slice() == ["vorbereiten"]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("[Papier] HALT: {e}");
            ExitCode::FAILURE
        }
    }
}
