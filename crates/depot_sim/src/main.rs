//! `depot_sim papier` – ein Papiertag mit den heute belegten Schlusskursen.
//!
//! Aufruf ueber `run.cmd papier`, nach `run.cmd kurse`. Journal `daten/journal.txt` (dasselbe
//! wie beim Sammler), Papierkonto `daten/papierkonto.txt`; beides bleibt auf dem eigenen Rechner
//! (`.gitignore`, Export ohne `daten/`). Kein Netz, kein Broker ausser dem Papierkonto.

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

use depot_journal::Journal;
use depot_kern::liste::{als_universum, beobachtungsliste};
use depot_kern::{Cent, Datum, Rundung, Topf, TopfId, Zeitpunkt};
use depot_sim::{Ausgang, Papierdatei, Stumm, papiertag, tageskurse};

/// Papiergeld je Topf beim ersten Papiertag: 5.000 Euro fuer Dachs (Topf A) und Maus (Topf B).
const START_JE_TOPF: Cent = Cent(500_000);

fn wurzel() -> Option<PathBuf> {
    let start = std::env::current_dir().ok()?;
    start
        .ancestors()
        .find(|d| d.join("kodex").join("festschreibung.txt").is_file())
        .map(Path::to_path_buf)
}

fn heute() -> Option<Datum> {
    let s = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    Zeitpunkt::aus_sekunden(i64::try_from(s).ok()?)?.datum()
}

fn euro(c: Cent) -> String {
    depot_kern::Preis::aus_cent(c)
        .map_or_else(|| format!("{} Cent", c.0), depot_kern::Preis::euro_text)
}

fn papier(wurzel: &Path) -> Result<(), String> {
    let werte = beobachtungsliste()?;
    let universum = als_universum(&werte);
    let heute = heute().ok_or("Uhr liefert kein Datum")?;
    let pfad = wurzel.join("daten").join("journal.txt");
    let mut j = Journal::oeffnen(&pfad).map_err(|e| {
        format!(
            "Journal {}: {e:?}. Beim ersten Mal: run.cmd kurse einrichten",
            pfad.display()
        )
    })?;
    let start = [
        Topf::neu(TopfId::A, START_JE_TOPF).map_err(|e| format!("{e:?}"))?,
        Topf::neu(TopfId::B, START_JE_TOPF).map_err(|e| format!("{e:?}"))?,
    ];
    let depot = !j.stand().toepfe().is_empty();
    let mut konto =
        Papierdatei::oeffnen(&wurzel.join("daten").join("papierkonto.txt"), depot, &start)?;
    let mut dachs = Stumm;
    let mut maus = Stumm;
    let ausgang = papiertag(
        &mut j, heute, &universum, &start, &mut dachs, &mut maus, &mut konto,
    );
    if let Some(f) = konto.fehler() {
        eprintln!("[Papier] FEHLER: Papierkonto nicht geschrieben: {f}");
    }
    let ausgang = ausgang.map_err(|e| format!("{e:?}"))?;
    let datum = match &ausgang {
        Ausgang::SchonAbgeschlossen(d) => {
            println!("[Papier] Der Handelstag {d} ist schon abgeschlossen; nichts zu tun.");
            *d
        }
        Ausgang::Abgeschlossen {
            datum,
            hoechststand,
        } => {
            println!(
                "[Papier] Papiertag {datum} abgeschlossen. Hoechststand {} EUR.",
                euro(*hoechststand)
            );
            *datum
        }
    };
    for (d, wer, text) in j.stand().begruendungen() {
        if *d == datum {
            println!("  {wer}: {text}");
        }
    }
    let kurse = tageskurse(j.stand(), &universum).map_err(|e| format!("{e:?}"))?;
    for t in j.stand().toepfe().values() {
        let wert = t
            .wert_gerundet(&kurse.heute, Rundung::Ab)
            .map_or_else(|| "nicht bewertbar".into(), |w| format!("{} EUR", euro(w)));
        println!(
            "  {:?}: Wert {wert}, Bargeld {} EUR, {} Positionen",
            t.id(),
            euro(t.bargeld()),
            t.positionen().len()
        );
    }
    if konto.fehler().is_some() {
        return Err("Papierkonto und Journal weichen ab; der naechste Lauf haelt an".into());
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
    if args.as_slice() != ["papier"] {
        eprintln!("[Papier] Aufruf: run.cmd papier");
        return ExitCode::from(2);
    }
    match papier(&wurzel) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("[Papier] HALT: {e}");
            ExitCode::FAILURE
        }
    }
}
