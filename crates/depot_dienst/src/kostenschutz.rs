//! Die Sperre des Kostenschutzes (`daten/ki.halt`) gefuehrt aufheben.
//!
//! Der Berater schreibt die Sperre, wenn ein Aufruf mehr gekostet hat als reserviert oder die
//! Kosten unbekannt sind. Aufheben heisst nur: die Datei nach einer Pruefung durch den Menschen
//! entfernen. Es aendert nichts am Kassenbuch (die Reservierung bleibt gebucht und zaehlt im
//! Budget) und nichts an der Kostenpruefung, die jeder weitere Aufruf wieder durchlaeuft und die
//! bei einer Abweichung wieder sperrt. Der Grund bleibt in `daten/ki_halt_aufgehoben.txt`.
//!
//! Aufgehoben wird nur die Fassung, die der Mensch gesehen hat (Abdruck), nur mit ausdruecklicher
//! Bestaetigung und nur, wenn kein Auftrag laeuft oder wartet.
//!
//! Ebenso die Kurssperre (`daten/kurse.halt`, `depot_journal::kurssperre`): Der Sammler legt sie
//! nach einem Sicherheitsabbruch an. Aufheben entfernt sie nach der Pruefung; der Grund bleibt in
//! `daten/kurse_halt_aufgehoben.txt`. Die Teilkurse des abgebrochenen Abrufs bleiben belegt, aber
//! ein Papiertag nimmt sie nur, wenn sie vom selben Tag sind und die Pruefung besteht.

use std::fs::{self, OpenOptions};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use depot_kern::Zeitpunkt;

pub fn pfad(wurzel: &Path) -> PathBuf {
    wurzel.join("daten").join("ki.halt")
}

pub fn protokoll_pfad(wurzel: &Path) -> PathBuf {
    wurzel.join("daten").join("ki_halt_aufgehoben.txt")
}

/// Die Kurssperre nach einem Sicherheitsabbruch des Sammlers.
pub fn kurse_pfad(wurzel: &Path) -> PathBuf {
    wurzel
        .join("daten")
        .join(depot_lage::kurssperre::KURSSPERRE)
}

pub fn kurse_protokoll_pfad(wurzel: &Path) -> PathBuf {
    wurzel.join("daten").join("kurse_halt_aufgehoben.txt")
}

/// Ein Abdruck des Inhalts, damit nur die gesehene Fassung aufgehoben wird. Kein Schutz gegen
/// Angreifer (dafuer sorgen Herkunft und Token), nur gegen eine neue Sperre in der Zwischenzeit.
pub fn abdruck(inhalt: &str) -> String {
    let mut h = DefaultHasher::new();
    inhalt.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Wie das Aufheben ausging.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aufheben {
    /// Es lag keine Sperre (mehr).
    Frei,
    /// Die Sperre hat sich seit dem Anzeigen geaendert; nichts angefasst.
    Geaendert,
    Aufgehoben,
}

/// Hebt die Sperre auf, wenn sie noch die gesehene ist. Erst wird der Grund festgehalten, dann
/// die Datei entfernt.
///
/// # Errors
/// Lesen, Festhalten oder Entfernen scheitert; dann bleibt die Sperre.
pub fn aufheben(wurzel: &Path, gesehen: &str, jetzt: Zeitpunkt) -> Result<Aufheben, String> {
    hebe_auf(&pfad(wurzel), &protokoll_pfad(wurzel), gesehen, jetzt)
}

/// Hebt die Kurssperre auf, wie `aufheben` die Sperre des Kostenschutzes.
///
/// # Errors
/// Wie `aufheben`.
pub fn kurse_aufheben(wurzel: &Path, gesehen: &str, jetzt: Zeitpunkt) -> Result<Aufheben, String> {
    hebe_auf(
        &kurse_pfad(wurzel),
        &kurse_protokoll_pfad(wurzel),
        gesehen,
        jetzt,
    )
}

fn hebe_auf(p: &Path, q: &Path, gesehen: &str, jetzt: Zeitpunkt) -> Result<Aufheben, String> {
    let inhalt = match fs::read_to_string(p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Aufheben::Frei),
        Err(e) => return Err(format!("{}: {e}", p.display())),
    };
    if abdruck(&inhalt) != gesehen {
        return Ok(Aufheben::Geaendert);
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(q)
        .and_then(|mut f| {
            writeln!(
                f,
                "aufgehoben {} ({}) nach Pruefung ueber die Einrichtung:",
                jetzt.sekunden(),
                depot_lage::zeit_text(jetzt)
            )?;
            f.write_all(inhalt.as_bytes())?;
            writeln!(f, "---")?;
            f.sync_all()
        })
        .map_err(|e| format!("{}: {e}", q.display()))?;
    fs::remove_file(p).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(Aufheben::Aufgehoben)
}
