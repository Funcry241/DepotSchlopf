//! Negativtests zu den Befunden aus `docs/BEFUNDE.md` (Stufe 2, Kern).
//!
//! Ein Test mit `#[ignore = "offen: …"]` zeigt einen Befund, der im heutigen Code noch besteht:
//! `cargo test -p depot_kern --test befunde -- --ignored` ist am Stand dieses Commits rot.
//! Der Umbau entfernt das `ignore`; dann laeuft der Test im Kodex.

mod sonde;

use depot_kern::Cent;

// ---------------------------------------------------------------------------------------------
// 1·A01 – Geldoperationen saettigen still
// ---------------------------------------------------------------------------------------------

/// Ein Ueberlauf ergibt einen Fehler, nie einen gekappten Wert.
#[test]
#[ignore = "offen: 1·A01 Cent::euro und Cent::abs saettigen still"]
fn befund_1_a01_ueberlauf_ergibt_keinen_gekappten_wert() {
    let gross = i64::MAX / 10;
    assert_ne!(
        Cent::euro(gross),
        Cent(i64::MAX),
        "Cent::euro kappt einen Ueberlauf auf i64::MAX"
    );
    assert_ne!(
        Cent(i64::MIN).abs(),
        Cent(i64::MAX),
        "Cent::abs kappt i64::MIN auf i64::MAX"
    );
}

// ---------------------------------------------------------------------------------------------
// 1·F01 – freie Laufzeitgrenzen umgehen die Festschreibung (R19)
// ---------------------------------------------------------------------------------------------

/// Ausserhalb von Tests laesst sich keine gelockerte Grenze bauen. Grenzen entstehen nur beim
/// Laden der hash-geprueften Festschreibung.
#[test]
#[ignore = "offen: 1·F01 Grenzen sind frei baubar"]
fn befund_1_f01_gelockerte_grenze_laesst_sich_nicht_bauen() {
    assert_eq!(sonde::umgebung_baut(&["depot_kern"]), Ok(()));
    let locker = "pub fn locker() -> depot_kern::Grenzen {\n    \
                  depot_kern::Grenzen { max_anteil_bp: 10_000, ..depot_kern::Grenzen::standard() }\n}\n";
    assert!(
        sonde::baut(&["depot_kern"], locker).is_err(),
        "ein fremdes Crate kann Grenzen mit 100 % Hoechstanteil bauen"
    );
    let veraendert = "pub fn locker() -> depot_kern::Grenzen {\n    \
                      let mut g = depot_kern::Grenzen::standard();\n    g.notbremse_bp = 9_000;\n    g\n}\n";
    assert!(
        sonde::baut(&["depot_kern"], veraendert).is_err(),
        "ein fremdes Crate kann eine geladene Grenze nachtraeglich lockern"
    );
}

// ---------------------------------------------------------------------------------------------
// 1·R06 – Kurse ohne Datum, rohe Zustaende (R20)
// ---------------------------------------------------------------------------------------------

/// Ein Kurs ist nur als geprueftes Objekt mit Handelstag, Waehrung, Herkunft und Alter zu haben.
/// Aus einem rohen Centbetrag laesst sich keiner bauen.
#[test]
#[ignore = "offen: 1·R06 Kurse sind rohe Centbetraege ohne Datum"]
fn befund_1_r06_kurs_ohne_handelstag_laesst_sich_nicht_bauen() {
    assert_eq!(sonde::umgebung_baut(&["depot_kern"]), Ok(()));
    let roh = "pub fn roh(i: depot_kern::Isin) -> depot_kern::Kurse {\n    \
               depot_kern::Kurse([(i, depot_kern::Cent(0))].into_iter().collect())\n}\n";
    assert!(
        sonde::baut(&["depot_kern"], roh).is_err(),
        "Kurse lassen sich aus rohen Centbetraegen ohne Handelstag bauen"
    );
}
