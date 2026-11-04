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
    Rundung, Topf, TopfId, Universum, Vorschlag, Zeitpunkt,
};
use depot_regelwerk::{
    Anlass, Eingabe, Entscheidung, Kennungsvergabe, Richtung, WaechterBefund, entscheide,
    notbremse_auftraege, waechter,
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

/// Ohne Vorschlag: nur die zwingenden Schritte.
fn lauf_ohne(topf: &Topf, kurse: &Kurse) -> Entscheidung {
    entscheide(
        &Eingabe {
            grenzen: &Grenzen::standard(),
            universum: &universum(),
            topf,
            kurse,
            vorschlag: None,
            halt: false,
        },
        &mut Kennungsvergabe::nach(0),
    )
}

/// Maus' Gegenprobe zu PR #21: 1 BASF zu 49,925 € und 134,98 € Bargeld sind exakt 27,00035 %
/// Anteil, also ueber 25 % plus 2 Punkte Toleranz. Mit abgerundeten Werten (49,92 € von
/// 184,90 €) waeren es 26,99838 %, und der Abbau bliebe aus. Grenzen pruefen exakt.
#[test]
fn abbau_prueft_den_anteil_exakt() {
    let k = kurse(&[(BASF, 49_925_000), (MBG, 41_480_000)]);
    let ent = lauf_ohne(&topf(13_498, &[(BASF, 1)]), &k);
    let abbau: Vec<_> = ent
        .verkaeufe
        .iter()
        .map(|a| (a.auftrag().stueck, a.beleg().anlass))
        .collect();
    assert_eq!(abbau, vec![(1, Anlass::Abbau)], "{ent:?}");
    // Knapp darunter (134,99 € Bargeld: exakt 26,99889 %) bleibt der Bestand.
    let ent = lauf_ohne(&topf(13_499, &[(BASF, 1)]), &k);
    assert!(ent.verkaeufe.is_empty(), "{ent:?}");
    // Auch der Topfwert zaehlt exakt: 3 BASF und 404,95 € sind exakt 26,99999 %, kein Abbau.
    // Mit abgerundetem Topfwert (554,72 € statt 554,725 €) waeren es 27,00024 %.
    let ent = lauf_ohne(&topf(40_495, &[(BASF, 3)]), &k);
    assert!(ent.verkaeufe.is_empty(), "{ent:?}");
}

/// Abbau genau dann, wenn der exakte Anteil ueber Hoechstanteil plus Toleranz liegt, und danach
/// liegt der Rest exakt hoechstens beim Hoechstanteil des exakten Topfwerts nach der eigenen
/// Minderung (Abstand zum Limit plus Gebuehr). Viele Bestaende und Bargeldbetraege mit einem Kurs
/// auf halbem Cent.
#[test]
fn abbau_genau_dann_und_rest_exakt_im_rahmen() {
    let g = Grenzen::standard();
    let fein: i64 = 49_925_000;
    let k = kurse(&[(BASF, fein), (MBG, 41_480_000)]);
    let schwelle = i128::from(g.max_anteil_bp() + g.bestand_toleranz_bp());
    let mut faelle = (0, 0);
    for stueck in 1..=12_u64 {
        for bargeld in (0..40_000_i64).step_by(3) {
            let ent = lauf_ohne(&topf(bargeld, &[(BASF, stueck)]), &k);
            let abbau: Vec<_> = ent
                .verkaeufe
                .iter()
                .filter(|a| a.beleg().anlass == Anlass::Abbau)
                .collect();
            let wert = i128::from(stueck) * i128::from(fein);
            let gesamt = i128::from(bargeld) * 10_000 + wert;
            let fall = format!("{stueck} Stueck, {bargeld} Cent");
            if wert * 10_000 <= gesamt * schwelle {
                faelle.0 += 1;
                assert!(abbau.is_empty(), "{fall}: Abbau ohne Grenzuebertritt");
                continue;
            }
            faelle.1 += 1;
            assert_eq!(abbau.len(), 1, "{fall}: kein einzelner Abbau: {ent:?}");
            let Some(a) = abbau.first() else {
                continue;
            };
            let (verkauft, limit) = (a.auftrag().stueck, a.auftrag().limit);
            let volumen = limit.mal_stueck(verkauft).unwrap_or(Cent(i64::MAX));
            let gebuehr = g.gebuehren().fuer(volumen).unwrap_or(Cent(i64::MAX));
            let minderung = (i128::from(fein) - i128::from(limit.0) * 10_000)
                * i128::from(verkauft)
                + i128::from(gebuehr.0) * 10_000;
            let rest = i128::from(stueck - verkauft) * i128::from(fein);
            assert!(
                rest * 10_000 <= (gesamt - minderung) * i128::from(g.max_anteil_bp()),
                "{fall}: Rest {rest} ueber dem Hoechstanteil"
            );
        }
    }
    assert!(faelle.0 > 100 && faelle.1 > 100, "Vorbedingung: {faelle:?}");
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
    let ent = lauf(&topf(1_600_000, &[(MBG, 100)]), &k, &[]);
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

/// Waechter: Gestern 49,929 €, heute 47,43255 € sind exakt 5 % Tagesverlust. Beide abgerundet
/// (49,92 € und 47,43 €) waeren es 4,988 %, und der Halt bliebe aus. Der Vergleichswert wird
/// deshalb aufgerundet, der heutige Wert abgerundet; dann sind es 5,007 %.
#[test]
fn waechter_rundung_verdeckt_keinen_verlust() {
    let g = Grenzen::standard();
    let t = topf(0, &[(BASF, 1)]);
    let gestern = kurse(&[(BASF, 49_929_000), (MBG, 41_480_000)]);
    let heute = kurse(&[(BASF, 47_432_550), (MBG, 41_480_000)]);
    let w = t.wert_gerundet(&heute, Rundung::Ab);
    let v = t.wert_gerundet(&gestern, Rundung::Auf);
    assert_eq!((w, v), (Some(Cent(4_743)), Some(Cent(4_993))));
    let befund = w.zip(v).map(|(w, v)| waechter(&g, w, v, v));
    assert_eq!(
        befund,
        Some(WaechterBefund::Tagesverlust { verlust_bp: 500 })
    );
    // Gegenprobe der Regel: beide abgerundet verdecken den Verlust.
    let v_ab = t.wert_gerundet(&gestern, Rundung::Ab);
    let verdeckt = w.zip(v_ab).map(|(w, v)| waechter(&g, w, v, v));
    assert_eq!(verdeckt, Some(WaechterBefund::InOrdnung));
}
