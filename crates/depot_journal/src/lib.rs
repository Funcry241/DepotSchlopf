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
//! Auftraege mit Zustand und der letzten Kennung, Beobachtungen und das Kassenbuch der
//! KI-Aufrufe nach den Regeln aus `depot_ki`. Eine `AufrufErlaubnis` entsteht nur durch
//! `Journal::reserviere`, ein Auftrag im Journal nur durch `Journal::reserviere_auftrag`.
//!
//! **Kurse (R12, Kap. 6):** Ein Kurs von aussen steht vor seiner Verwendung im Journal, mit
//! Quelle, Abrufzeit und Abdruck der rohen Antwort (`kursbeleg`). Ebenso jeder Termin
//! (`terminbeleg`, R39); ein geaenderter Termin wird fortgeschrieben, die alte Fassung bleibt.
//! Ebenso jede Nachricht (`nachrichtbeleg`, R40).
//!
//! **Kontowaechter (Kap. 9, R37):** Jeder Auftrag hat einen Zustand nach fester
//! Zustandsmaschine (`auftrag`). Reserviert wird nur mit Deckung; der Abgleich mit dem Konto
//! belegt dessen Meldungen und meldet jede Abweichung (`konto`).
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

pub mod abrufkontingent;
pub mod anbieter;
pub mod auftrag;
pub mod datei;
pub mod dateiersatz;
pub mod gespraeche;
pub mod kasse;
mod ki_abrechnung;
mod ki_aufruf;
pub use ki_aufruf::{KiAufruf, KiAufrufErgebnis};
pub mod konto;
pub mod kursbeleg;
pub mod kurssperre;
pub mod nachrichtbeleg;
pub mod rss_abruf;
pub use kasse::KiZweck;
pub use ki_abrechnung::KiAbrechnung;
pub mod aufloesung;
pub mod stand;
pub mod terminbeleg;
pub mod vorbereitung;
pub mod weltprognose;

pub use aufloesung::{Aufloesung, Auswertung, Ergebnis, werte_aus, wilson_bp};
pub use auftrag::{AuftragsLage, AuftragsZustand, Ausfuehrung, Meldung};
pub use datei::{Journal, JournalFehler, inhalt_fuer, vermerk_fuer};
pub use kasse::{AufrufErlaubnis, KiStand};
pub use konto::{
    Abrechnungen, Abweichung, Konto, KontoOrder, KontoStatus, Kontoauszug, papier_valuta,
    verfuegbar,
};
pub use kursbeleg::KursBeleg;
pub use nachrichtbeleg::{MAX_NACHRICHT, NachrichtBeleg};
pub use stand::{
    Abschluss, Art, AuftragsBeleg, FORMAT, FORMAT_ALT, PapierHalt, Schlussbeleg, Stand,
};
pub use terminbeleg::{Fortschreibung, TerminBeleg};
pub use vorbereitung::{Vorbereitung, VorbereitungsAntwort};
pub use weltprognose::{PrognoseBeleg, Prognoserichtung, Weltprognose};

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use depot_kern::Datum;

    use crate::{Journal, JournalFehler};

    thread_local! {
        /// 0: aus. n: die n-te folgende Synchronisation scheitert.
        static SYNC_FEHLER: Cell<u32> = const { Cell::new(0) };
    }

    thread_local! {
        /// Jede Synchronisation dieses Threads, in Reihenfolge.
        static PROTOKOLL: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    }

    pub(crate) fn protokolliere(was: &'static str) {
        PROTOKOLL.with(|p| p.borrow_mut().push(was));
    }

    fn protokoll_neu() -> Vec<&'static str> {
        PROTOKOLL.with(|p| std::mem::take(&mut *p.borrow_mut()))
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

    #[test]
    fn vorbereitung_gibt_bei_schreibfehler_keine_aufruferlaubnis() -> Result<(), String> {
        let datum = Datum::neu(2026, 11, 7).ok_or("Samstag")?;
        let zeit = depot_kern::Zeitpunkt::frankfurt(datum, 12, 0, 0).ok_or("Zeit")?;
        for fehler in 1..=3 {
            let pfad = eingerichtet()?;
            let mut j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
            j.starte_vorbereitung(
                zeit,
                Datum::neu(2026, 11, 9).ok_or("Montag")?,
                vec!["Beleg".into()],
            )
            .map_err(|e| format!("{e:?}"))?;
            SYNC_FEHLER.with(|f| f.set(fehler));
            let e = j.reserviere_vorbereitung(zeit, depot_kern::BeraterId::Dachs);
            SYNC_FEHLER.with(|f| f.set(0));
            assert!(matches!(e, Err(depot_ki::Sperre::Kassenbuch(_))));
            assert!(
                j.reserviere_vorbereitung(zeit, depot_kern::BeraterId::Maus)
                    .is_err()
            );
            drop(j);
            let j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
            assert!(j.stand().ki().buchungen().len() <= 1);
            assert_eq!(j.stand().seit_abschluss(), 0);
        }
        Ok(())
    }

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
            // Windows erlaubt keinen zweiten Zugriff auf die exklusiv gesperrten Bytes.
            // Die Sperre bleibt bestehen; der Test liest über den eigenen Dateizugriff.
            let vorher = j.test_dateiinhalt().map_err(|e| e.to_string())?;
            let zweiter = j.beobachte(tag, "zweite");
            let nachher = j.test_dateiinhalt().map_err(|e| e.to_string())?;
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

    /// K13-01: Steht nach einem Absturz ein vollstaendiger, aber unbestaetigter Eintrag im
    /// Journal, ist er vielleicht nur im Cache des Betriebssystems lesbar. Der Wiederanlauf
    /// synchronisiert deshalb zuerst das Journal und rueckt erst danach den Vermerk vor.
    /// Scheitert diese Synchronisation, bleibt der Vermerk, wie er war.
    #[test]
    fn befund_k13_01_wiederanlauf_synchronisiert_vor_dem_vermerk() -> Result<(), String> {
        let tag = Datum::neu(2026, 11, 2).ok_or("Datum")?;
        let p = eingerichtet()?;
        let mut j = Journal::oeffnen(&p).map_err(|e| format!("{e:?}"))?;
        // Plan vermerkt, Zeile geschrieben, ihre Synchronisation scheitert.
        SYNC_FEHLER.with(|f| f.set(2));
        let erster = j.beobachte(tag, "erste");
        SYNC_FEHLER.with(|f| f.set(0));
        if erster.is_ok() {
            return Err("Vorbedingung: Fehler tritt nicht ein".into());
        }
        drop(j);
        let vermerk = p.with_extension("vermerk");
        let vorher = fs::read(&vermerk).map_err(|e| e.to_string())?;

        // Scheitert die erste Synchronisation des Wiederanlaufs, rueckt nichts vor.
        protokoll_neu();
        SYNC_FEHLER.with(|f| f.set(1));
        let gescheitert = Journal::oeffnen(&p);
        SYNC_FEHLER.with(|f| f.set(0));
        let protokoll = protokoll_neu();
        let nachher = fs::read(&vermerk).map_err(|e| e.to_string())?;
        assert_eq!(
            protokoll.first(),
            Some(&"Journal"),
            "zuerst synchronisiert: {protokoll:?}"
        );
        assert!(
            gescheitert.is_err(),
            "Wiederanlauf trotz Synchronisationsfehler"
        );
        assert_eq!(
            vorher, nachher,
            "Vermerk trotz Synchronisationsfehler vorgerückt"
        );

        // Ohne Fehler: erst das Journal, dann der Vermerk; der Eintrag zaehlt.
        let j = Journal::oeffnen(&p).map_err(|e| format!("{e:?}"))?;
        let protokoll = protokoll_neu();
        assert_eq!(protokoll, vec!["Journal", "Zwischendatei"], "Reihenfolge");
        assert_eq!(j.eintraege().len(), 1);
        Ok(())
    }
}
