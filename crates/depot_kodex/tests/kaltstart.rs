//! Der aktuelle Baum muss die Nicht-Git-Schritte des Kodex bestehen.

use std::path::PathBuf;

fn wurzel() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn essenz_ist_vollstaendig_und_aufloesbar() {
    assert_eq!(depot_kodex::essenz::pruefe(&wurzel()), Vec::<String>::new());
}

#[test]
fn festschreibung_stimmt_mit_baum_ueberein() {
    let fehler = depot_kodex::festschreibung::pruefe(
        &wurzel(),
        depot_kodex::Klasse::Funktion,
        &std::collections::BTreeSet::new(),
    );
    assert_eq!(fehler, Vec::<String>::new());
}

#[test]
fn quellscan_sauber() {
    assert_eq!(
        depot_kodex::quellscan::pruefe(&wurzel()),
        Vec::<String>::new()
    );
}
