//! Negativtests zu den Befunden aus `docs/BEFUNDE.md` (Stufe 2, Kern).
//!
//! Ein Test mit `#[ignore = "offen: …"]` zeigt einen Befund, der im heutigen Code noch besteht:
//! `cargo test -p depot_kern --test befunde -- --ignored` ist am Stand dieses Commits rot.
//! Der Umbau entfernt das `ignore`; dann laeuft der Test im Kodex.
//!
//! Die Vertraege werden ueber die Sonde (`tests/sonde/mod.rs`) als fremdes Crate gebaut und
//! ausgefuehrt. Ein Vertrag, der sich nicht bauen laesst, gilt als nicht erfuellt; ein Verbot
//! gilt nur mit dem erwarteten Fehlercode als bewiesen.

mod sonde;

const KERN: [&str; 1] = ["depot_kern"];

// ---------------------------------------------------------------------------------------------
// 1·A01 – Geldoperationen saettigen still (R13)
// ---------------------------------------------------------------------------------------------

/// `Cent::euro` meldet einen Ueberlauf als `None`. Normale Betraege und die Randwerte direkt
/// unter dem Ueberlauf bleiben exakt.
#[test]
fn befund_1_a01_euro_meldet_ueberlauf_als_fehler() -> Result<(), String> {
    sonde::muss_laufen(
        &KERN,
        r#"
use depot_kern::Cent;
fn main() {
    let gross = i64::MAX / 100;
    let klein = i64::MIN / 100;
    let r: Option<Cent> = Cent::euro(gross + 1);
    assert!(r.is_none(), "Ueberlauf nach oben: {r:?}");
    assert!(Cent::euro(klein - 1).is_none(), "Ueberlauf nach unten");
    assert!(Cent::euro(i64::MAX).is_none());
    assert!(Cent::euro(i64::MIN).is_none());
    assert_eq!(Cent::euro(gross).map(|c| c.0), Some(gross * 100));
    assert_eq!(Cent::euro(klein).map(|c| c.0), Some(klein * 100));
    assert_eq!(Cent::euro(12).map(|c| c.to_string()), Some("12,00 €".to_string()));
    assert_eq!(Cent::euro(-12).map(|c| c.to_string()), Some("-12,00 €".to_string()));
    assert_eq!(Cent::euro(0).map(|c| c.0), Some(0));
}
"#,
    )
}

/// `Cent::abs` meldet `i64::MIN` als `None` statt still auf `i64::MAX` zu kappen.
#[test]
fn befund_1_a01_abs_meldet_ueberlauf_als_fehler() -> Result<(), String> {
    sonde::muss_laufen(
        &KERN,
        r#"
use depot_kern::Cent;
fn main() {
    let r: Option<Cent> = Cent(i64::MIN).abs();
    assert!(r.is_none(), "abs(i64::MIN) ergibt {r:?}");
    assert_eq!(Cent(i64::MIN + 1).abs(), Some(Cent(i64::MAX)));
    assert_eq!(Cent(-5).abs(), Some(Cent(5)));
    assert_eq!(Cent(5).abs(), Some(Cent(5)));
    assert_eq!(Cent(0).abs(), Some(Cent(0)));
}
"#,
    )
}

// ---------------------------------------------------------------------------------------------
// 1·F01 – freie Laufzeitgrenzen umgehen die Festschreibung (R19)
// ---------------------------------------------------------------------------------------------

/// Gegenstueck: Eine vorhandene Grenze weitergeben und kopieren ist erlaubt.
const GRENZEN_ERLAUBT: &str =
    "pub fn weiter(g: &depot_kern::Grenzen) -> depot_kern::Grenzen { g.clone() }\n";

/// Ausserhalb von Tests laesst sich keine Grenze im Literal bauen. Der Test nutzt bewusst
/// keinen Konstruktor: Eine Umbenennung von `standard` macht ihn nicht gruen.
#[test]
fn befund_1_f01_grenze_im_literal_laesst_sich_nicht_bauen() -> Result<(), String> {
    sonde::erlaubt(&KERN, GRENZEN_ERLAUBT)?;
    sonde::verboten(
        &KERN,
        "pub fn locker(g: &depot_kern::Grenzen) -> depot_kern::Grenzen {\n    \
         depot_kern::Grenzen { max_anteil_bp: 10_000, ..g.clone() }\n}\n",
        &sonde::PRIVAT,
        &["max_anteil_bp"],
    )
}

/// Eine geladene Grenze laesst sich nicht nachtraeglich lockern.
#[test]
fn befund_1_f01_geladene_grenze_laesst_sich_nicht_lockern() -> Result<(), String> {
    sonde::erlaubt(&KERN, GRENZEN_ERLAUBT)?;
    sonde::verboten(
        &KERN,
        "pub fn locker(mut g: depot_kern::Grenzen) -> depot_kern::Grenzen {\n    \
         g.notbremse_bp = 9_000;\n    g\n}\n",
        &sonde::PRIVAT,
        &["notbremse_bp"],
    )
}

/// K8-01 (Pruefung von PR #8): Ein fremdes Programm schaltet jedes Feature von `depot_kern`
/// und `depot_regelwerk` ein und baut im Release. Auch dann laesst sich keine Grenze und keine
/// Gebuehr lockern. Das Programm ist Maus' Gegenprobe, unveraendert bis auf die Pfade.
#[test]
fn befund_k8_01_grenzen_auch_mit_allen_features_im_release_nicht_lockerbar() -> Result<(), String> {
    const BEIDE: [&str; 2] = ["depot_kern", "depot_regelwerk"];
    sonde::erlaubt_mit_allen_features(
        &BEIDE,
        r"
use depot_kern::{Cent, Grenzen};
fn main() {
    let original = Grenzen::standard();
    assert_eq!(original.max_anteil_bp(), 2_500);
    let befund = depot_regelwerk::waechter(
        &original, Cent(800_000), Cent(800_000), Cent(1_000_000));
    assert!(matches!(befund, depot_regelwerk::WaechterBefund::Notbremse { .. }));
}
",
    )?;
    sonde::verboten_mit_allen_features(
        &BEIDE,
        r"
use depot_kern::{Cent, Grenzen};

fn main() {
    let original = Grenzen::standard();
    let locker = original.clone()
        .mit_max_anteil_bp(10_000)
        .mit_notbremse_bp(9_999)
        .mit_gebuehren(original.gebuehren()
            .mit_minimum(Cent(0))
            .mit_boersengebuehr(Cent(0))
            .mit_prozent_bp(0));

    assert_eq!(locker.max_anteil_bp(), 10_000);
    assert_eq!(locker.notbremse_bp(), 9_999);
    assert_eq!(locker.gebuehren().fuer(Cent(50_000)), Some(Cent(0)));

    let standard_befund = depot_regelwerk::waechter(
        &original, Cent(800_000), Cent(800_000), Cent(1_000_000));
    let gelockerter_befund = depot_regelwerk::waechter(
        &locker, Cent(800_000), Cent(800_000), Cent(1_000_000));
    assert!(matches!(standard_befund,
        depot_regelwerk::WaechterBefund::Notbremse { .. }));
    assert_eq!(gelockerter_befund,
        depot_regelwerk::WaechterBefund::InOrdnung);
}
",
        &["E0599"],
        &["mit_max_anteil_bp"],
    )
}

// ---------------------------------------------------------------------------------------------
// 1·R06 – Kurse ohne Datum, rohe Zustaende (R20)
// ---------------------------------------------------------------------------------------------

/// Gegenstueck: Kurse weitergeben und nachschlagen ist erlaubt.
const KURSE_ERLAUBT: &str = "pub fn nachschlagen(k: &depot_kern::Kurse, i: &depot_kern::Isin) -> bool {\n    \
                             k.kurs(i).is_some()\n}\n";

/// Aus einem rohen Centbetrag laesst sich kein Kurs bauen: Ein Kurs ist nur als geprueftes
/// Objekt mit Handelstag, Waehrung, Herkunft und Alter zu haben.
#[test]
fn befund_1_r06_kurs_aus_rohem_betrag_laesst_sich_nicht_bauen() -> Result<(), String> {
    sonde::erlaubt(&KERN, KURSE_ERLAUBT)?;
    sonde::verboten(
        &KERN,
        "pub fn roh(i: depot_kern::Isin, c: depot_kern::Cent) -> depot_kern::Kurse {\n    \
         depot_kern::Kurse([(i, c)].into_iter().collect())\n}\n",
        &sonde::PRIVAT,
        &["private"],
    )
}

/// Geladene Kurse lassen sich nicht nachtraeglich roh veraendern.
#[test]
fn befund_1_r06_geladene_kurse_lassen_sich_nicht_roh_aendern() -> Result<(), String> {
    sonde::erlaubt(&KERN, KURSE_ERLAUBT)?;
    sonde::verboten(
        &KERN,
        "pub fn roh(mut k: depot_kern::Kurse, i: depot_kern::Isin) -> depot_kern::Kurse {\n    \
         k.0.insert(i, depot_kern::Cent(0));\n    k\n}\n",
        &sonde::PRIVAT,
        &["Kurse"],
    )
}

/// Bargeld ist nie negativ: Ein Topf mit negativem Bargeld laesst sich nicht anlegen, und das
/// Bargeld eines Topfes laesst sich nicht roh setzen.
#[test]
fn befund_1_r06_negatives_bargeld_laesst_sich_nicht_anlegen() -> Result<(), String> {
    sonde::erlaubt(
        &KERN,
        "pub fn wert(t: &depot_kern::Topf, k: &depot_kern::Kurse) -> Option<depot_kern::Cent> {\n    \
         t.wert(k)\n}\n",
    )?;
    sonde::muss_laufen(
        &KERN,
        r#"
use depot_kern::{Cent, Topf, TopfId};
fn main() {
    let minus: Option<Topf> = Topf::neu(TopfId::A, Cent(-100)).ok();
    assert!(minus.is_none(), "negatives Bargeld angenommen");
    assert!(Topf::neu(TopfId::A, Cent(0)).is_ok());
}
"#,
    )?;
    sonde::verboten(
        &KERN,
        "pub fn roh(t: &mut depot_kern::Topf) {\n    t.bargeld = depot_kern::Cent(-1);\n}\n",
        &sonde::PRIVAT,
        &["bargeld"],
    )
}

/// Ein Kurs ist nur in Euro zu haben (Positivliste: Xetra). Jede andere oder fehlende Waehrung
/// wird beim Eintritt abgelehnt, ebenso ein Preis, der nicht positiv ist.
#[test]
fn befund_1_r06_kurs_nur_in_euro_und_positiv() -> Result<(), String> {
    sonde::muss_laufen(
        &KERN,
        r#"
use depot_kern::{Cent, Datum, Herkunft, Kurs, KursFehler, Zeitpunkt};
fn main() {
    let tag = Datum::neu(2026, 11, 2).expect("gueltiges Datum");
    let zeit = Zeitpunkt::neu(tag, 10, 0, 0).expect("gueltige Uhrzeit");
    let neu = |preis: i64, w: &str| Kurs::neu(Cent(preis), w, tag, zeit, Herkunft::Broker);
    assert_eq!(neu(10_000, "EUR").map(|k| k.preis_cent()), Ok(Some(Cent(10_000))));
    for w in ["USD", "eur", "", "EUR "] {
        assert_eq!(neu(10_000, w).err(), Some(KursFehler::Waehrung), "Waehrung {w:?} angenommen");
    }
    assert_eq!(neu(0, "EUR").err(), Some(KursFehler::NichtPositiv));
    assert_eq!(neu(-1, "EUR").err(), Some(KursFehler::NichtPositiv));
}
"#,
    )
}

/// Ein Kurs traegt Handelstag, Zeitpunkt und Herkunft. Kalenderdaten und Uhrzeiten werden
/// geprueft; ein Handelstag nach dem Zeitpunkt des Kurses wird abgelehnt.
#[test]
fn befund_1_r06_kurs_traegt_handelstag_und_herkunft() -> Result<(), String> {
    sonde::muss_laufen(
        &KERN,
        r#"
use depot_kern::{Cent, Datum, Herkunft, Kurs, KursFehler, Zeitpunkt};
fn main() {
    assert!(Datum::neu(2026, 2, 31).is_none(), "31.02. angenommen");
    assert!(Datum::neu(2026, 2, 29).is_none(), "29.02.2026 angenommen");
    assert!(Datum::neu(2024, 2, 29).is_some());
    assert!(Datum::neu(2026, 13, 1).is_none());
    assert!(Datum::neu(2026, 4, 31).is_none());
    assert!(Datum::neu(2026, 1, 0).is_none());
    let tag = Datum::neu(2026, 11, 2).expect("gueltiges Datum");
    let morgen = Datum::neu(2026, 11, 3).expect("gueltiges Datum");
    assert!(Zeitpunkt::neu(tag, 24, 0, 0).is_none());
    assert!(Zeitpunkt::neu(tag, 10, 60, 0).is_none());
    assert!(Zeitpunkt::neu(tag, 10, 0, 60).is_none());
    let zeit = Zeitpunkt::neu(tag, 10, 0, 0).expect("gueltige Uhrzeit");
    let k = Kurs::neu(Cent(10_000), "EUR", tag, zeit, Herkunft::Aufzeichnung).expect("Kurs");
    assert_eq!(k.handelstag(), tag);
    assert_eq!(k.zeitpunkt(), zeit);
    assert_eq!(k.herkunft(), Herkunft::Aufzeichnung);
    assert_eq!(
        Kurs::neu(Cent(10_000), "EUR", morgen, zeit, Herkunft::Broker).err(),
        Some(KursFehler::Handelstag)
    );
}
"#,
    )
}

/// K15-03 (Maus, PR #15): Ein Kurs liegt an seinem Handelstag. Ein Zeitpunkt nach dem
/// Handelstag (aus Sicht des Handelstags: aus der Zukunft) wird schon beim Bau abgelehnt; der
/// letzte und der erste Augenblick des Tages gelten.
#[test]
fn befund_k15_03_kurszeitpunkt_liegt_am_handelstag() -> Result<(), String> {
    sonde::muss_laufen(
        &KERN,
        r#"
use depot_kern::{Cent, Datum, Herkunft, Kurs, KursFehler, Zeitpunkt};
fn main() {
    let tag = Datum::neu(2026, 11, 24).expect("gueltiges Datum");
    let spaeter = Datum::neu(2026, 11, 26).expect("gueltiges Datum");
    let morgen = Datum::neu(2026, 11, 25).expect("gueltiges Datum");
    for z in [
        Zeitpunkt::neu(spaeter, 17, 30, 0).expect("Zeit"),
        Zeitpunkt::neu(morgen, 0, 0, 0).expect("Zeit"),
    ] {
        assert_eq!(
            Kurs::neu(Cent(5_000), "EUR", tag, z, Herkunft::Simulation).err(),
            Some(KursFehler::Handelstag),
            "Zeitpunkt nach dem Handelstag angenommen"
        );
    }
    for z in [
        Zeitpunkt::neu(tag, 0, 0, 0).expect("Zeit"),
        Zeitpunkt::neu(tag, 23, 59, 59).expect("Zeit"),
    ] {
        assert!(Kurs::neu(Cent(5_000), "EUR", tag, z, Herkunft::Simulation).is_ok());
    }
}
"#,
    )
}

/// Kurse kennen ihren Stand und das Alter jedes Kurses. Ein Kurs nach dem Stand (aus der
/// Zukunft) und ein doppelter Kurs werden abgelehnt.
#[test]
fn befund_1_r06_kurse_kennen_ihr_alter() -> Result<(), String> {
    sonde::muss_laufen(
        &KERN,
        r#"
use depot_kern::{Cent, Datum, Herkunft, Isin, Kurs, KursFehler, Kurse, Zeitpunkt};
fn main() {
    let tag = Datum::neu(2026, 11, 2).expect("gueltiges Datum");
    let um = |h: u8, m: u8| Zeitpunkt::neu(tag, h, m, 0).expect("gueltige Uhrzeit");
    let isin = Isin::neu("DE0007164600").expect("ISIN");
    let k = Kurs::neu(Cent(10_000), "EUR", tag, um(10, 0), Herkunft::Broker).expect("Kurs");
    let kurse = Kurse::neu(um(10, 20), vec![(isin.clone(), k.clone())]).expect("Kurse");
    assert_eq!(kurse.stand(), um(10, 20));
    assert_eq!(kurse.kurs_cent(&isin), Some(Cent(10_000)));
    assert_eq!(kurse.alter_s(&isin), Some(1_200));
    assert_eq!(
        Kurse::neu(um(9, 59), vec![(isin.clone(), k.clone())]).err(),
        Some(KursFehler::Zukunft(isin.clone()))
    );
    assert_eq!(
        Kurse::neu(um(10, 20), vec![(isin.clone(), k.clone()), (isin.clone(), k)]).err(),
        Some(KursFehler::Doppelt(isin))
    );
}
"#,
    )
}
