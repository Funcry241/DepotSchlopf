//! Durchstich NeuroSchlopf (#325, Fall `durchstich-v1`): These, belegte Kandidaten und
//! Kaufbeleg im Journal bis zum Abschnitt „Belegauswahl“ des Prüfberichts im Analyse-Export
//! (`depot_dienst/src/analyse.rs`, `belegauswahl::bericht`). Läuft mit dem Grundrang, ohne Modell.

use depot_journal::Journal;
use depot_kern::liste::{als_universum, beobachtungsliste};
use depot_kern::{BeraterId, Datum};
use depot_lage::belegauswahl::bericht;
use depot_sim::belegrang::{Grundrang, Heute, KAUFBELEG, Sortierer, kandidaten_am, thesen_vor};

/// Version des Falls. Bei jeder Änderung an Kandidaten oder Erwartung hochzählen.
const FALL: &str = "durchstich-v1";

const KANDIDATEN: [&str; 5] = [
    "Nachricht: Ölpreis steigt",
    "Nachricht: Autobauer im Minus",
    "Nachricht: Gold fester",
    "Termin 18.11.: Zinsentscheid (bestätigt)",
    "Nachricht: SAP hebt Prognose an",
];

fn tag(t: u8) -> Result<Datum, String> {
    Datum::neu(2026, 11, t).ok_or_else(|| format!("Tag {t}"))
}

#[test]
fn durchstich_v1_vom_journal_bis_zum_pruefbericht() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("depot_{FALL}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let ergebnis = (|| {
        let f = |e| format!("{e:?}");
        let pfad = dir.join("journal.txt");
        Journal::einrichten(&pfad).map_err(f)?;
        let mut j = Journal::oeffnen(&pfad).map_err(f)?;
        // These von Dachs vor dem Stichtag (Format wie lauf.rs: "Topf: ISIN bp: Grund").
        j.begruende(
            tag(13)?,
            "Dachs",
            "Topf A: DE0007164600 2000 bp: Auftragslage",
        )
        .map_err(f)?;
        for k in KANDIDATEN {
            j.beobachte(tag(16)?, k).map_err(f)?;
        }
        // Dachs stützt Auftrag 7 auf Kandidat 5, Maus Auftrag 8 auf Kandidat 1.
        j.begruende(tag(16)?, KAUFBELEG, "7 Dachs 5").map_err(f)?;
        j.begruende(tag(16)?, KAUFBELEG, "8 Maus 1").map_err(f)?;

        let stand = j.stand();
        let universum = als_universum(&beobachtungsliste()?);
        let kandidaten = kandidaten_am(stand, tag(16)?);
        let thesen = thesen_vor(stand, &universum, BeraterId::Dachs, tag(16)?);
        let eins = |v: Vec<usize>| v.into_iter().map(|i| i + 1).collect::<Vec<_>>();
        assert_eq!(eins(Heute.ordne(&kandidaten, &thesen)), vec![1, 2, 3, 4, 5]);
        assert_eq!(
            eins(Grundrang.ordne(&kandidaten, &thesen)),
            vec![5, 4, 1, 2, 3]
        );

        let t = bericht(stand, tag(16)?, tag(20)?);
        println!("{FALL}\n{t}");
        // Dachs-Kauf: heute Platz 5, Grundrang Platz 1. Maus-Kauf: in beiden Platz ≤ 3.
        assert!(
            t.contains("| heutige Reihenfolge | 0 | 0 | – | 2 | 1 | 50 % |"),
            "{t}"
        );
        assert!(
            t.contains("| Grundrang | 0 | 0 | – | 2 | 2 | 100 % |"),
            "{t}"
        );
        assert!(t.contains("Tage mit verknüpften Belegen: 1 von 1"), "{t}");
        assert!(t.contains("| 2026-11-16 | 5 | 0 | 2 |"), "{t}");
        // Im Export kommen nur Zählungen an, keine Belegreferenz (Tag, Nummer, Text).
        assert!(!t.contains("SAP hebt Prognose an"), "{t}");
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    ergebnis
}
