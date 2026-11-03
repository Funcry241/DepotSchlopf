//! DepotSchlopf – Journal.
//!
//! Das Journal ist die einzige dauerhafte Datei des Rahmens. Darin stehen Beobachtungen, der
//! Stand des Papierdepots, jeder genehmigte Auftrag und das Kassenbuch der KI-Aufrufe. Es wird
//! nur angehaengt und ist eine Hash-Kette mit Anhangsvermerk (R25, Kap. 9). Weil jeder Vorgang
//! genau ein Eintrag in genau einer Datei ist, gibt es keinen Stand zwischen zwei Dateien, den
//! ein Absturz zerreissen koennte (R38).
//!
//! **Einrichtung (R25):** Das Journal entsteht nur durch `einrichten`, nie beim Oeffnen.
//!
//! **Wiederanlauf (Kap. 11):** Der Stand wird nur aus dem Journal nachgerechnet: Toepfe,
//! Auftraege mit der letzten Kennung, Beobachtungen. Das Kassenbuch rechnet `depot_ki` aus
//! denselben Eintraegen nach.
//!
//! **Sperre (R24):** Gesperrt wird die geoeffnete Datei selbst ueber das Betriebssystem. Ein
//! Link in irgendeinem Teil des Pfads ist verboten. Zwischendateien werden nur exklusiv neu
//! angelegt.
//!
//! **Fehler sperren (R29):** Nach einem Schreib- oder Synchronisationsfehler schreibt das
//! Journal nichts mehr, bis es neu geoeffnet und dabei geprueft wird.
//!
//! **Grenze:** Gegen das gemeinsame Zuruecksetzen von Journal und Vermerk auf einen alten Stand
//! hilft keine Datei auf dem Rechner. Dafuer steht der bestaetigte Stand im Abendbericht
//! (`Journal::bestaetigt`), ausserhalb des Rechners.

// Die harten Lints stehen auf `forbid`: Der Compiler lehnt jede lokale Ausnahme ab, auch eine
// hinter `cfg_attr` (R26, Befund 1·G01). Die Liste haelt der Kodex fest.
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

pub mod datei;
pub mod stand;

pub use datei::{Journal, JournalFehler, inhalt_fuer, vermerk_fuer};
pub use stand::{Art, AuftragsBeleg, Stand};

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use depot_kern::Datum;

    use crate::{Journal, JournalFehler};

    thread_local! {
        /// 0: aus. n: die n-te folgende Synchronisation scheitert.
        static SYNC_FEHLER: Cell<u32> = const { Cell::new(0) };
    }

    pub(crate) fn sync_scheitert() -> bool {
        SYNC_FEHLER.with(|f| match f.get() {
            0 => false,
            1 => {
                f.set(0);
                true
            }
            n => {
                f.set(n.saturating_sub(1));
                false
            }
        })
    }

    static NR: AtomicU32 = AtomicU32::new(0);

    fn eingerichtet() -> Result<PathBuf, String> {
        let n = NR.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("depot_journal_sync_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let p = dir.join("journal.txt");
        Journal::einrichten(&p).map_err(|e| format!("{e:?}"))?;
        Ok(p)
    }

    /// 1·R02: Ein eingespeister Synchronisationsfehler beim Vermerk des Plans, bei der Zeile und
    /// beim Vorruecken des Stands sperrt das Journal. Danach schreibt es nichts mehr. Nach dem
    /// Neustart zaehlt ein Eintrag genau dann, wenn seine Zeile vollstaendig auf der Platte
    /// stand: Ein Abbruch gibt nichts zurueck, was schon gebucht war.
    #[test]
    fn befund_1_r02_synchronisationsfehler_sperrt_das_journal() -> Result<(), String> {
        let tag = Datum::neu(2026, 11, 2).ok_or("Datum")?;
        for (fall, n, zaehlt) in [("Plan", 1_u32, 0_usize), ("Zeile", 2, 1), ("Stand", 3, 1)] {
            let p = eingerichtet()?;
            let mut j = Journal::oeffnen(&p).map_err(|e| format!("{e:?}"))?;
            SYNC_FEHLER.with(|f| f.set(n));
            let erster = j.beobachte(tag, "erste");
            SYNC_FEHLER.with(|f| f.set(0));
            assert!(erster.is_err(), "{fall}: Fehler nicht eingetreten");
            let vorher = fs::read(&p).map_err(|e| e.to_string())?;
            let zweiter = j.beobachte(tag, "zweite");
            let nachher = fs::read(&p).map_err(|e| e.to_string())?;
            assert!(
                matches!(&zweiter, Err(JournalFehler::Halt(m)) if m.contains("gesperrt")),
                "{fall}: {zweiter:?}"
            );
            assert_eq!(vorher, nachher, "{fall}: weitergeschrieben");
            assert!(j.eintraege().is_empty(), "{fall}: Eintrag im Speicher");
            drop(j);
            let neu = Journal::oeffnen(&p).map_err(|e| format!("{fall}: {e:?}"))?;
            assert_eq!(neu.eintraege().len(), zaehlt, "{fall}: nach dem Neustart");
        }
        Ok(())
    }
}
