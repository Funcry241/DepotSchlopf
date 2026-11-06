//! Wie das Lagebild zum Journal kommt.
//!
//! **Neu ist nur, was keine Spur hat (K26-01):** „Nicht eingerichtet“ heisst, dass weder
//! Journal noch Vermerk noch Papierkonto da sind. Ob das Journal eingerichtet ist, entscheidet
//! `Journal::eingerichtet`, dieselbe Pruefung, mit der `Journal::oeffnen` beginnt. Fehlt nur ein
//! Teil, ist der Datenbestand unvollstaendig: Halt, nie „neu einrichten“. Sonst oeffnet das
//! Lagebild das Journal wie jeder Lauf (geprueft, exklusiv).

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use depot_journal::{Journal, JournalFehler};

use crate::Zugang;

/// Name des Papierkontos im Datenordner, wie `run.cmd papier` es fuehrt.
pub const PAPIERKONTO: &str = "papierkonto.txt";
/// Name des Journals im Datenordner.
pub const JOURNAL: &str = "journal.txt";

fn fehler_text(e: &JournalFehler) -> String {
    match e {
        JournalFehler::Belegt => "belegt".into(),
        JournalFehler::Halt(t) | JournalFehler::Abgelehnt(t) => t.clone(),
    }
}

/// Liegt unter diesem Namen irgendetwas? Ein Fehler beim Nachsehen gilt als Spur.
fn spur(pfad: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(pfad) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("{}: nicht lesbar: {e}", pfad.display())),
    }
}

/// Oeffnet das Journal im Datenordner `daten` und ruft `f` mit dem Zugang auf. Das Journal ist
/// nur waehrend `f` offen.
pub fn mit_journal<R>(daten: &Path, f: impl FnOnce(Zugang<'_>) -> R) -> R {
    let pfad = daten.join(JOURNAL);
    let eingerichtet = match Journal::eingerichtet(&pfad) {
        Ok(e) => e,
        Err(e) => return f(Zugang::Fehler(&fehler_text(&e))),
    };
    if !eingerichtet {
        return match spur(&daten.join(PAPIERKONTO)) {
            Ok(false) => f(Zugang::NichtEingerichtet),
            Ok(true) => f(Zugang::Fehler(
                "Papierkonto ohne Journal und Vermerk: Datenbestand unvollstaendig",
            )),
            Err(t) => f(Zugang::Fehler(&t)),
        };
    }
    match Journal::oeffnen(&pfad) {
        Ok(j) => f(Zugang::Offen(j.stand())),
        Err(JournalFehler::Belegt) => f(Zugang::Belegt),
        Err(e) => f(Zugang::Fehler(&fehler_text(&e))),
    }
}

/// Eine belegte Journalsperre ist vorübergehend. Andere Fehler verlangen einen Eingriff.
#[derive(Debug)]
pub enum DatenprobeFehler {
    Belegt,
    Fehler(String),
}

impl From<String> for DatenprobeFehler {
    fn from(grund: String) -> Self {
        Self::Fehler(grund)
    }
}

impl std::fmt::Display for DatenprobeFehler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Belegt => f.write_str("Journal belegt"),
            Self::Fehler(grund) => f.write_str(grund),
        }
    }
}

/// Prüft die Daten für einen Update-Kandidaten ohne Schreibzugriff oder Netz.
pub fn update_datenprobe(daten: &Path) -> Result<(), DatenprobeFehler> {
    use depot_journal::Konto as _;
    use depot_kern::{Topf, TopfId};
    use depot_sim::papierlauf::START_JE_TOPF;

    let journal = daten.join(JOURNAL);
    let konto = daten.join(PAPIERKONTO);
    let konto_da = match fs::symlink_metadata(&konto) {
        Ok(m) if m.is_file() => true,
        Ok(_) => {
            return Err(DatenprobeFehler::Fehler(
                "Papierkonto ist keine reguläre Datei".into(),
            ));
        }
        Err(e) if e.kind() == ErrorKind::NotFound => false,
        Err(e) => return Err(format!("Papierkonto: {e}").into()),
    };
    if !Journal::eingerichtet(&journal).map_err(|e| fehler_text(&e))? {
        return if konto_da {
            Err(DatenprobeFehler::Fehler(
                "Papierkonto ohne Journal und Vermerk".into(),
            ))
        } else {
            Ok(())
        };
    }
    Journal::pruefe_lesend(&journal, |stand| {
        let depot = !stand.toepfe().is_empty();
        if !depot && !konto_da {
            return Ok(());
        }
        // Nur lesen, selbst wenn das Konto nach der Existenzprüfung verschwindet.
        let papier = fs::read_to_string(&konto).map_err(|e| format!("Papierkonto: {e}"))?;
        let papier = depot_sim::Papierkonto::aus_text(&papier)?;
        if !depot {
            let [a, b] = [TopfId::A, TopfId::B].map(|id| Topf::neu(id, START_JE_TOPF));
            let start = [
                a.map_err(|e| format!("{e:?}"))?,
                b.map_err(|e| format!("{e:?}"))?,
            ];
            return if depot_sim::Papierkonto::eroeffne(&start).as_ref() == Some(&papier) {
                Ok(())
            } else {
                Err("Papierkonto mit Bewegungen, aber ohne Depot im Journal".into())
            };
        }
        let abweichungen = depot_journal::konto::abweichungen(stand, &papier.auszug());
        if abweichungen.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "Journal und Papierkonto weichen ab: {abweichungen:?}"
            ))
        }
    })
    .map_err(|e| match e {
        JournalFehler::Belegt => DatenprobeFehler::Belegt,
        e => fehler_text(&e).into(),
    })?
    .map_err(DatenprobeFehler::Fehler)
}

/// Sperrdateien im Datenordner: `*.sperre` (abgebrochener Lauf) und `*.halt` (etwa
/// `ki.halt` nach einer Kostenueberschreitung). Jede ist Halt, bis ein Mensch geprueft und sie
/// geloescht hat. Ein nicht lesbarer Ordner zaehlt nicht; das Journal meldet ihn ohnehin.
pub fn sperrdateien(daten: &Path) -> Vec<String> {
    let mut aus: Vec<String> = fs::read_dir(daten)
        .map(|e| {
            e.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| {
                    Path::new(n).extension().is_some_and(|x| {
                        x.eq_ignore_ascii_case("sperre") || x.eq_ignore_ascii_case("halt")
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    aus.sort();
    aus
}

/// Das Anbieterprotokoll im Datenordner: lesbare Eintraege und Zahl unlesbarer Zeilen. Fehlt es,
/// ist es leer; ist es unlesbar, zaehlt es als eine unlesbare Zeile.
pub fn anbieterprotokoll(daten: &Path) -> (Vec<depot_journal::anbieter::Eintrag>, usize) {
    match fs::read_to_string(daten.join(depot_journal::anbieter::DATEI)) {
        Ok(t) => depot_journal::anbieter::lies_alle(&t),
        Err(e) if e.kind() == ErrorKind::NotFound => (Vec::new(), 0),
        Err(_) => (Vec::new(), 1),
    }
}

/// Der Inhalt von `daten/ki.halt`, wenn die Datei liegt und lesbar ist.
pub fn ki_halt(daten: &Path) -> Option<String> {
    fs::read_to_string(daten.join("ki.halt")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_journal::{Art, inhalt_fuer, vermerk_fuer};

    #[test]
    fn datenprobe_lehnt_alte_freigabe_mit_gueltiger_hashkette_ab() -> Result<(), String> {
        let daten = std::env::temp_dir().join(format!("depot-datenprobe-{}", std::process::id()));
        fs::create_dir(&daten).map_err(|e| e.to_string())?;
        let test = (|| {
            update_datenprobe(&daten).map_err(|e| e.to_string())?;
            let journal =
                include_bytes!("../../../docs/pruefberichte/papierfreigabe/daten/journal.txt");
            let vermerk =
                include_bytes!("../../../docs/pruefberichte/papierfreigabe/daten/journal.vermerk");
            let konto =
                include_bytes!("../../../docs/pruefberichte/papierfreigabe/daten/papierkonto.txt");
            fs::write(daten.join(JOURNAL), journal).map_err(|e| e.to_string())?;
            fs::write(daten.join("journal.vermerk"), vermerk).map_err(|e| e.to_string())?;
            fs::write(daten.join(PAPIERKONTO), konto).map_err(|e| e.to_string())?;
            update_datenprobe(&daten).map_err(|e| e.to_string())?;
            let abschluss = format!("2026-11-04;1500000;{}", "a".repeat(64));
            let alt = inhalt_fuer(&[
                (Art::Topf, "A;500000;"),
                (Art::Topf, "B;500000;"),
                (Art::Abrechnungstag, "2026-11-04"),
                (Art::PapierHalt, "2026-11-04;gesamtverlust"),
                (Art::Abschluss, &abschluss),
                (Art::PapierFreigabe, "2026-11-04;1000000;einrichtung;dialog"),
            ]);
            let vermerk = vermerk_fuer(&alt, 0).ok_or("Vermerk")?;
            fs::write(daten.join(JOURNAL), &alt).map_err(|e| e.to_string())?;
            fs::write(daten.join("journal.vermerk"), &vermerk).map_err(|e| e.to_string())?;
            assert!(
                update_datenprobe(&daten)
                    .is_err_and(|e| e.to_string().contains("UTC-Zeit der Papierfreigabe"))
            );
            assert_eq!(
                fs::read(daten.join(JOURNAL)).map_err(|e| e.to_string())?,
                alt
            );
            assert_eq!(
                fs::read_to_string(daten.join("journal.vermerk")).map_err(|e| e.to_string())?,
                vermerk
            );
            assert_eq!(
                fs::read(daten.join(PAPIERKONTO)).map_err(|e| e.to_string())?,
                konto
            );
            Ok::<_, String>(())
        })();
        fs::remove_dir_all(&daten).map_err(|e| e.to_string())?;
        test
    }
}
