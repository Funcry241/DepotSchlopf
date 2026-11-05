//! `depot_sammler` – holt Xetra-Schlusskurse und Termine, belegt sie im Journal und prueft sie.
//!
//! Aufruf ueber `run.cmd kurse` (oder `run.cmd kurse einrichten` beim ersten Mal) und
//! `run.cmd termine`. Das Journal
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

use depot_journal::{Journal, anbieter, kurssperre};
use depot_kern::{Datum, Preis, Zeitpunkt};
use depot_sammler::{
    Ergebnis, Https, Schluessel, beobachtungsliste, einordnen, ezb::ADRESSE_EZB, sammle,
    sammle_termine,
};

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

/// Exakt, ohne Rundung: `49,925 EUR`.
fn euro(p: Preis) -> String {
    format!("{} EUR", p.euro_text())
}

fn datum(d: Datum) -> String {
    format!("{:02}.{:02}.{}", d.tag(), d.monat(), d.jahr())
}

fn tag_preis(t: Option<(Datum, Preis)>) -> String {
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

/// Haelt im Anbieterprotokoll fest, was der Lauf ueber Schluessel und Quelle sagt.
fn vermerke(wurzel: &Path, bericht: &depot_sammler::Bericht) {
    if let (Some(ergebnis), Some(zeit)) = (einordnen(bericht), jetzt()) {
        let e = anbieter::Eintrag::neu(zeit, anbieter::Wer::Kursquelle, ergebnis, None);
        if let Err(f) = anbieter::anhaengen(&wurzel.join("daten").join(anbieter::DATEI), &e) {
            eprintln!("[Sammler] Anbieterprotokoll nicht geschrieben: {f}");
        }
    }
}

/// Liegt die Kurssperre, wird nichts abgerufen.
fn gesperrt(wurzel: &Path) -> bool {
    let Some(grund) = kurssperre::kurssperre(&wurzel.join("daten")) else {
        return false;
    };
    eprintln!(
        "[Sammler] HALT: daten/{} liegt ({grund}). Kein Abruf und kein Papiertag, bis ein Mensch \
         den Schluessel geprueft und die Sperre in der Einrichtung aufgehoben hat.",
        kurssperre::KURSSPERRE
    );
    true
}

/// Nach einem Sicherheitsabbruch: Kurssperre anlegen, damit auch ein Papiertag von Hand die
/// Teilkurse dieses Abrufs nie verwendet.
fn sperre(wurzel: &Path) {
    let grund = "Sicherheitsabbruch: Die Kursquelle gab den Schluessel zurueck. Teilkurse dieses \
                 Abrufs nicht verwenden; Schluessel pruefen und bei Bedarf erneuern";
    match kurssperre::sperre_kurse(&wurzel.join("daten"), grund) {
        Ok(()) => eprintln!(
            "[Sammler] Kurssperre daten/{} angelegt.",
            kurssperre::KURSSPERRE
        ),
        Err(e) => eprintln!(
            "[Sammler] FEHLER: Kurssperre nicht angelegt ({e}). KEINEN Papiertag starten."
        ),
    }
}

fn kurse(wurzel: &Path, journal_pfad: &Path) -> ExitCode {
    if gesperrt(wurzel) {
        return ExitCode::FAILURE;
    }
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
    let verwendbar = bericht.verwendbar();
    if bericht.sicherheitsabbruch {
        sperre(wurzel);
    }
    vermerke(wurzel, &bericht);
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
                println!("  {:<20} {}  FEHLER: {t}", w.name, w.isin);
            }
            Ergebnis::NichtAbgerufen => {
                println!("  {:<20} {}  nicht abgerufen", w.name, w.isin);
            }
        }
    }
    if let Some(a) = &bericht.abbruch {
        println!("[Sammler] Lauf vorzeitig beendet: {a}");
    }
    zeige_pruefung(&bericht);
    println!(
        "[Sammler] Belegt im Journal {}, rohe Antworten unter daten/roh/.",
        journal_pfad.display()
    );
    // Erfolg nur mit verwendbaren Kursen: Ein spaeterer Lauf darf sich auf den Ausgang
    // verlassen (K19-02).
    if verwendbar {
        println!("[Sammler] Kurse verwendbar.");
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "[Sammler] Kurse NICHT verwendbar (Fehler, Abbruch oder Pruefung nicht bestanden)."
        );
        ExitCode::FAILURE
    }
}

/// Was die Pruefung sagt, wie der Papiertag sie ansetzt.
fn zeige_pruefung(bericht: &depot_sammler::Bericht) {
    match bericht.handelstag {
        None => println!("[Sammler] Keine Schlusskurse belegt."),
        Some(t) if bericht.pruefung.is_empty() => {
            println!(
                "[Sammler] Pruefung fuer den Handelstag {}: in Ordnung.",
                datum(t)
            );
            for f in &bericht.ausgelassen {
                println!("  heute nicht handelbar: {f:?}");
            }
        }
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
}

fn termine(wurzel: &Path, journal_pfad: &Path) -> ExitCode {
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
    println!("[Sammler] Hole die Termine der EZB von {ADRESSE_EZB} ...");
    let mut netz = Https::neu();
    let mut uhr = jetzt;
    let bericht = match sammle_termine(
        &mut journal,
        &mut netz,
        &mut uhr,
        &wurzel.join("daten").join("roh"),
    ) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[Sammler] FEHLER, Lauf angehalten: {e:?}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(d) = &bericht.rohdatei {
        println!("[Sammler] Rohe Seite: {}", d.display());
    }
    if let Some(f) = &bericht.fehler {
        eprintln!("[Sammler] FEHLER, nichts belegt: {f}");
        return ExitCode::FAILURE;
    }
    println!(
        "[Sammler] Termine: {} neu, {} geaendert, {} unveraendert. Belegt im Journal {}.",
        bericht.neu,
        bericht.geaendert,
        bericht.unveraendert,
        journal_pfad.display()
    );
    for k in &bericht.fehlen {
        println!("  {k}: steht im Journal, aber nicht mehr auf der Seite (nur gemeldet)");
    }
    ExitCode::SUCCESS
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
        ["termine"] => termine(&wurzel, &journal),
        _ => {
            eprintln!("Aufruf: run.cmd kurse | run.cmd kurse einrichten | run.cmd termine");
            ExitCode::from(2)
        }
    }
}
