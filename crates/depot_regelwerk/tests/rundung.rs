//! Geld aus exakten Kursen (Kap. 6, R4, R21): Ein Kurs bleibt exakt, auch feiner als ein Cent.
//! Gerundet wird erst, wo ganze Cent noetig sind, und immer zur sicheren Seite (Maus,
//! 04.11.2026):
//!
//! - Depotbewertung abgerundet: Das Depot rechnet sich nie reicher.
//! - Kauflimit ist ein Hoechstpreis und wird abgerundet: nie teurer als vorgesehen.
//! - Verkaufslimit ist ein Mindestpreis und wird aufgerundet: nie billiger als vorgesehen.
//!
//! Die Kurse 49,925 € (BASF) und 41,155 € (Mercedes-Benz) sind Schlusskurse aus Wolfgangs
//! erstem Lauf; Bestand, Bargeld und Zeitpunkte sind erfunden (D12).

use std::collections::BTreeMap;

use depot_kern::{
    BeraterId, Cent, Datum, Grenzen, Herkunft, Instrument, InstrumentArt, Isin, Kurs, Kurse, Preis,
    Topf, TopfId, Universum, Vorschlag, Zeitpunkt,
};
use depot_regelwerk::{
    Eingabe, Entscheidung, Kennungsvergabe, Richtung, entscheide, notbremse_auftraege,
};

const BASF: &str = "DE000BASF111";
const MBG: &str = "DE0007100000";

fn isin(text: &str) -> Isin {
    Isin::neu(text).unwrap_or_else(|e| unreachable!("Test-ISIN {text}: {e:?}"))
}

fn universum() -> Universum {
    Universum::neu(
        [BASF, MBG]
            .into_iter()
            .map(|i| Instrument {
                isin: isin(i),
                name: i.to_string(),
                art: InstrumentArt::Aktie,
            })
            .collect(),
    )
}

fn stand() -> Zeitpunkt {
    Datum::neu(2026, 11, 3)
        .and_then(|d| Zeitpunkt::neu(d, 16, 40, 0))
        .unwrap_or_else(|| unreachable!("Teststand ungueltig"))
}

/// Kurse in Zehntausendstel Cent, alle zum Stand und damit frisch.
fn kurse(preise: &[(&str, i64)]) -> Kurse {
    let t = stand();
    let tag = t.datum().unwrap_or_else(|| unreachable!("Testtag"));
    let eintraege = preise
        .iter()
        .map(|(i, fein)| {
            let k = Kurs::exakt(Preis::aus_fein(*fein), "EUR", tag, t, Herkunft::Simulation)
                .unwrap_or_else(|e| unreachable!("Testkurs {i}: {e:?}"));
            (isin(i), k)
        })
        .collect();
    Kurse::neu(t, eintraege).unwrap_or_else(|e| unreachable!("Testkurse: {e:?}"))
}

fn topf(bargeld: i64, bestand: &[(&str, u64)]) -> Topf {
    let mut t =
        Topf::neu(TopfId::A, Cent(bargeld)).unwrap_or_else(|e| unreachable!("Testtopf: {e:?}"));
    for (i, s) in bestand {
        t.setze_bestand(isin(i), *s);
    }
    t
}

fn lauf(topf: &Topf, kurse: &Kurse, ziele: &[(&str, i64)]) -> Entscheidung {
    let v = Vorschlag {
        berater: BeraterId::Dachs,
        zielgewichte_bp: ziele.iter().map(|(i, g)| (isin(i), *g)).collect(),
        begruendung: BTreeMap::new(),
    };
    entscheide(
        &Eingabe {
            grenzen: &Grenzen::standard(),
            universum: &universum(),
            topf,
            kurse,
            vorschlag: Some(&v),
            halt: false,
        },
        &mut Kennungsvergabe::nach(0),
    )
}

/// 3 × 49,925 € = 149,775 € zaehlen als 149,77 €. In ganzen Cent bleibt alles wie bisher.
#[test]
fn bewertung_abgerundet() {
    let t = topf(100_000, &[(BASF, 3), (MBG, 2)]);
    let k = kurse(&[(BASF, 49_925_000), (MBG, 41_480_000)]);
    assert_eq!(t.wert(&k), Some(Cent(100_000 + 14_977 + 8_296)));
}

/// Kauflimit bei 1 % Abstand: 49,925 € × 1,01 = 50,42425 € ergibt 50,42 €, nicht 50,43 €. Auch
/// fuer ganze Cent gilt der Hoechstpreis: 99,99 € × 1,01 = 100,9899 € ergibt 100,98 €.
#[test]
fn kauflimit_hoechstpreis_abgerundet() {
    for (fein, erwartet) in [(99_990_000, 10_098), (49_925_000, 5_042)] {
        let k = kurse(&[(BASF, fein), (MBG, 41_480_000)]);
        let ent = lauf(&topf(1_000_000, &[]), &k, &[(BASF, 2_000)]);
        let kaeufe: Vec<_> = ent
            .kaeufe
            .iter()
            .map(|a| (a.auftrag().richtung, a.auftrag().limit))
            .collect();
        assert_eq!(kaeufe, vec![(Richtung::Kauf, Cent(erwartet))], "{ent:?}");
    }
}

/// Verkaufslimit bei 1 % Abstand: 41,155 € × 0,99 = 40,74345 € ergibt 40,75 €, nicht 40,74 €.
#[test]
fn verkaufslimit_mindestpreis_aufgerundet() {
    let k = kurse(&[(BASF, 49_925_000), (MBG, 41_155_000)]);
    let ent = lauf(&topf(100_000, &[(MBG, 100)]), &k, &[]);
    let verkaeufe: Vec<_> = ent
        .verkaeufe
        .iter()
        .map(|a| (a.auftrag().stueck, a.auftrag().limit))
        .collect();
    assert_eq!(verkaeufe, vec![(100, Cent(4_075))], "{ent:?}");
}

/// Die Notbremse verkauft auch mit einem Kurs feiner als ein Cent, zum aufgerundeten Mindestpreis.
#[test]
fn notbremse_mit_feinem_kurs() {
    let k = kurse(&[(BASF, 49_925_000), (MBG, 41_155_000)]);
    let auftraege = notbremse_auftraege(
        &Grenzen::standard(),
        &topf(0, &[(BASF, 10), (MBG, 100)]),
        &k,
        &mut Kennungsvergabe::nach(0),
    );
    let limits: Option<Vec<_>> = auftraege.map(|a| {
        a.iter()
            .map(|x| (x.auftrag().isin.to_string(), x.auftrag().limit))
            .collect()
    });
    // 49,925 € × 0,99 = 49,42575 € ergibt 49,43 €.
    assert_eq!(
        limits,
        Some(vec![
            (MBG.to_string(), Cent(4_075)),
            (BASF.to_string(), Cent(4_943)),
        ])
    );
}
