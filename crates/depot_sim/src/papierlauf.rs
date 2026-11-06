//! Der Papiertag als Programmschritt: Journal und Papierkonto aus dem Ordner `daten/` des
//! Projekts, Ausgabe als Zeilen. Aufgerufen von `run.cmd papier` (`depot_berater`), nach
//! `run.cmd kurse`. Beides bleibt auf dem eigenen Rechner (`.gitignore`, Export ohne `daten/`).

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use depot_journal::Journal;
use depot_kern::{Cent, Datum, Rundung, Topf, TopfId, Universum, Zeitpunkt};

use crate::lauf::Beteiligte;
use crate::papierdatei::Papierdatei;
use crate::papiertag::{Ausgang, papiertag_mit_uhr, tageskurse};
use crate::szenario::Berater;

/// Papiergeld je Topf beim ersten Papiertag: 5.000 Euro fuer Dachs (Topf A) und Maus (Topf B).
pub const START_JE_TOPF: Cent = Cent(500_000);

fn euro(c: Cent) -> String {
    depot_kern::Preis::aus_cent(c)
        .map_or_else(|| format!("{} Cent", c.0), depot_kern::Preis::euro_text)
}

/// Ein Papiertag mit den heute belegten Schlusskursen: Journal `daten/journal.txt`,
/// Papierkonto `daten/papierkonto.txt`. Liefert die Zeilen fuer die Ausgabe.
///
/// # Errors
/// Halt oder Fehler, als Text fuer die Ausgabe.
pub fn papier(
    wurzel: &Path,
    heute: Datum,
    universum: &Universum,
    dachs: &mut dyn Berater,
    maus: &mut dyn Berater,
) -> Result<Vec<String>, String> {
    let mut zeilen = Vec::new();
    // Nach einem Sicherheitsabbruch des Sammlers beginnt kein Papiertag, auch nicht von Hand:
    // Seine Teilkurse werden nie verwendet (Maus, Restbefund zu #34).
    if let Some(grund) = depot_journal::kurssperre::kurssperre(&wurzel.join("daten")) {
        return Err(format!(
            "daten/{} liegt ({grund}). Kein Papiertag, bis ein Mensch geprueft und die Sperre \
             in der Einrichtung aufgehoben hat",
            depot_journal::kurssperre::KURSSPERRE
        ));
    }
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
    let uhr = || {
        let sekunden = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
        Zeitpunkt::aus_sekunden(i64::try_from(sekunden).ok()?)
    };
    let mut beteiligte = Beteiligte {
        dachs,
        maus,
        konto: &mut konto,
    };
    let ausgang = papiertag_mit_uhr(&mut j, heute, universum, &start, &mut beteiligte, Some(&uhr));
    if let Some(f) = konto.fehler() {
        return Err(format!(
            "Papierkonto nicht geschrieben: {f}. Papierkonto und Journal weichen ab; der naechste Lauf haelt an"
        ));
    }
    let ausgang = ausgang.map_err(|e| format!("{e:?}"))?;
    let datum = match &ausgang {
        Ausgang::SchonAbgeschlossen(d) => {
            zeilen.push(format!(
                "Der Handelstag {d} ist schon abgeschlossen; nichts zu tun."
            ));
            *d
        }
        Ausgang::Abgeschlossen {
            datum,
            hoechststand,
        } => {
            zeilen.push(format!(
                "Papiertag {datum} abgeschlossen. Hoechststand {} EUR.",
                euro(*hoechststand)
            ));
            *datum
        }
    };
    for (d, wer, text) in j.stand().begruendungen() {
        if *d == datum {
            zeilen.push(format!("  {wer}: {text}"));
        }
    }
    let kurse = tageskurse(j.stand(), universum).map_err(|e| format!("{e:?}"))?;
    for t in j.stand().toepfe().values() {
        let wert = t
            .wert_gerundet(&kurse.heute, Rundung::Ab)
            .map_or_else(|| "nicht bewertbar".into(), |w| format!("{} EUR", euro(w)));
        zeilen.push(format!(
            "  {:?}: Wert {wert}, Bargeld {} EUR, {} Positionen",
            t.id(),
            euro(t.bargeld()),
            t.positionen().len()
        ));
    }
    Ok(zeilen)
}
