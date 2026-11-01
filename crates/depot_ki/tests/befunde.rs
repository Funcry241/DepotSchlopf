//! Negativtests zu den Befunden aus `docs/BEFUNDE.md` (Stufe 2, KI-Budget und Kassenbuch).
//!
//! Ein Test mit `#[ignore = "offen: …"]` zeigt einen Befund, der im heutigen Code noch besteht:
//! `cargo test -p depot_ki --test befunde -- --ignored` ist am Stand dieses Commits rot.
//! Der Umbau entfernt das `ignore`; dann laeuft der Test im Kodex.

mod sonde;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use depot_kern::{BeraterId, Cent};
use depot_ki::{Datum, Kassenbuch, KiDeckel, KiGrenzen, Preise, Sperre, pruefe};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

/// Eigenes, leeres Verzeichnis je Test; liefert den Pfad des Kassenbuchs darin.
fn neuer_pfad() -> PathBuf {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_ki_befund_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).ok();
    dir.join("kassenbuch.txt")
}

fn tag(t: u8) -> Datum {
    Datum {
        jahr: 2026,
        monat: 11,
        tag: t,
    }
}

const DACHS: BeraterId = BeraterId::Dachs;

/// Fuellt den Monat von Dachs (zwei Zyklen zu je zwei Aufrufen) und schliesst das Kassenbuch.
fn monat_voll(p: &Path) -> Result<(), Sperre> {
    let g = KiGrenzen::standard();
    let d = KiDeckel::standard(DACHS);
    let mut k = Kassenbuch::oeffnen(p)?;
    for (t, z) in [(2, 1), (2, 1), (3, 2), (3, 2)] {
        k.reserviere(&g, &d, DACHS, tag(t), z)?;
    }
    match k.reserviere(&g, &d, DACHS, tag(4), 3) {
        Err(Sperre::MonatVoll) => Ok(()),
        anders => Err(Sperre::Kassenbuch(format!("Vorbedingung: {anders:?}"))),
    }
}

// ---------------------------------------------------------------------------------------------
// 1·K01 – KI-Grenzen als freie Argumente (R19)
// ---------------------------------------------------------------------------------------------

/// Ausserhalb von Tests laesst sich keine gelockerte KI-Grenze und kein gelockerter Deckel bauen.
#[test]
#[ignore = "offen: 1·K01 KI-Grenzen und Deckel sind frei baubar"]
fn befund_1_k01_gelockerte_ki_grenze_laesst_sich_nicht_bauen() {
    let deps = ["depot_kern", "depot_ki"];
    assert_eq!(sonde::umgebung_baut(&deps), Ok(()));
    let grenzen = "pub fn locker() -> depot_ki::KiGrenzen {\n    \
                   depot_ki::KiGrenzen { kosten_je_monat: depot_kern::Cent(100_000), ..depot_ki::KiGrenzen::standard() }\n}\n";
    assert!(
        sonde::baut(&deps, grenzen).is_err(),
        "ein fremdes Crate kann das Monatsbudget auf 1.000 € setzen"
    );
    let deckel = "pub fn locker() -> depot_ki::KiDeckel {\n    \
                  let mut d = depot_ki::KiDeckel::standard(depot_kern::BeraterId::Dachs);\n    \
                  d.max_token_aus = 1_000_000;\n    d\n}\n";
    assert!(
        sonde::baut(&deps, deckel).is_err(),
        "ein fremdes Crate kann den Token-Deckel lockern"
    );
}

// ---------------------------------------------------------------------------------------------
// 1·K02 – negative Preise, negative Reservierung (R20)
// ---------------------------------------------------------------------------------------------

/// Ein negativer Preis wird abgelehnt und ergibt nie eine negative Reservierung.
#[test]
#[ignore = "offen: 1·K02 negativer Preis ergibt negative Reservierung"]
fn befund_1_k02_negativer_preis_wird_abgelehnt() {
    let mut d = KiDeckel::standard(DACHS);
    d.preise = Preise {
        cent_je_mio_ein: -1_000_000,
        cent_je_mio_aus: 2_000,
    };
    let ergebnis = pruefe(&KiGrenzen::standard(), &d, &[], DACHS, tag(2), 1);
    assert!(
        ergebnis.is_err(),
        "negativer Preis angenommen, Reservierung {ergebnis:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 1·K03 – ungueltige Kalenderdaten (R20)
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "offen: 1·K03 Datum::neu nimmt den 31.02. an"]
fn befund_1_k03_ungueltiges_kalenderdatum_wird_abgelehnt() {
    for (j, m, t) in [(2026, 2, 31), (2026, 2, 29), (2026, 4, 31), (2026, 11, 31)] {
        assert_eq!(Datum::neu(j, m, t), None, "{t:02}.{m:02}.{j} angenommen");
    }
    assert!(Datum::neu(2028, 2, 29).is_some(), "Schaltjahr");
    assert!(Datum::neu(2026, 12, 31).is_some());
}

// ---------------------------------------------------------------------------------------------
// 1·K04 – leeres oder fehlendes Kassenbuch setzt die Grenzen zurueck (R25, R38)
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "offen: 1·K04 geloeschtes Kassenbuch setzt das Budget zurueck"]
fn befund_1_k04_geloeschtes_kassenbuch_fuehrt_zu_halt() {
    let p = neuer_pfad();
    assert_eq!(monat_voll(&p), Ok(()));
    fs::remove_file(&p).ok();
    assert!(
        Kassenbuch::oeffnen(&p).is_err(),
        "nach dem Loeschen beginnt das Budget bei null"
    );
}

#[test]
#[ignore = "offen: 1·K04 geleertes Kassenbuch setzt das Budget zurueck"]
fn befund_1_k04_geleertes_kassenbuch_fuehrt_zu_halt() {
    let p = neuer_pfad();
    assert_eq!(monat_voll(&p), Ok(()));
    fs::write(&p, "").ok();
    assert!(
        Kassenbuch::oeffnen(&p).is_err(),
        "nach dem Leeren beginnt das Budget bei null"
    );
}

#[test]
#[ignore = "offen: 1·K04 um ganze Zeilen gekuerztes Kassenbuch wird angenommen"]
fn befund_1_k04_gekuerztes_kassenbuch_fuehrt_zu_halt() {
    let p = neuer_pfad();
    assert_eq!(monat_voll(&p), Ok(()));
    let inhalt = fs::read_to_string(&p).unwrap_or_default();
    let erste: String = inhalt.split_inclusive('\n').take(2).collect();
    fs::write(&p, erste).ok();
    assert!(
        Kassenbuch::oeffnen(&p).is_err(),
        "nach dem Kuerzen auf zwei Zeilen ist wieder Budget frei"
    );
}

// ---------------------------------------------------------------------------------------------
// 1·K05 – semantisch kaputte Historie (R20, R25)
// ---------------------------------------------------------------------------------------------

/// Jede Zeile ist fuer sich lesbar, die Historie als Ganzes aber unmoeglich.
#[test]
#[ignore = "offen: 1·K05 semantisch kaputte Historie wird angenommen"]
fn befund_1_k05_kaputte_historie_fuehrt_zu_halt() {
    let mut angenommen = Vec::new();
    for (fall, inhalt) in [
        (
            "Datum rueckwaerts",
            "v1;Dachs;2026-11-05;3;48\nv1;Dachs;2026-11-02;4;48\n",
        ),
        (
            "Zyklus rueckwaerts",
            "v1;Dachs;2026-11-02;9;48\nv1;Dachs;2026-11-03;4;48\n",
        ),
        (
            "Reservierung ueber dem Hoechstpreis eines Aufrufs",
            "v1;Dachs;2026-11-02;1;999999\n",
        ),
        (
            "mehr Aufrufe im Zyklus als erlaubt",
            "v1;Dachs;2026-11-02;1;48\nv1;Dachs;2026-11-02;1;48\nv1;Dachs;2026-11-02;1;48\n",
        ),
    ] {
        let p = neuer_pfad();
        fs::write(&p, inhalt).ok();
        if Kassenbuch::oeffnen(&p).is_ok() {
            angenommen.push(fall);
        }
    }
    assert!(angenommen.is_empty(), "angenommen: {angenommen:?}");
}

// ---------------------------------------------------------------------------------------------
// 1·K06 – zwei Pfade umgehen die Sperre (R24)
// ---------------------------------------------------------------------------------------------

/// Ein zweiter Name fuer dieselbe Datei (harter Link) bekommt das Kassenbuch nicht.
#[test]
#[ignore = "offen: 1·K06 Sperre haengt am Pfadnamen, nicht an der Datei"]
fn befund_1_k06_zweiter_pfad_auf_dieselbe_datei_wird_gesperrt() {
    let p = neuer_pfad();
    assert_eq!(monat_voll(&p), Ok(()));
    let zweit = p.with_file_name("zweitname.txt");
    assert!(
        fs::hard_link(&p, &zweit).is_ok(),
        "Vorbedingung: harter Link"
    );
    let _erster = Kassenbuch::oeffnen(&p);
    assert!(
        matches!(Kassenbuch::oeffnen(&zweit), Err(Sperre::KassenbuchBelegt)),
        "dieselbe Datei ist ueber einen zweiten Namen ein zweites Mal offen"
    );
}

/// Ein symbolischer Link im Datenverzeichnis ist verboten.
#[cfg(unix)]
#[test]
#[ignore = "offen: 1·K06 Kassenbuch ueber symbolischen Link wird geoeffnet"]
fn befund_1_k06_symbolischer_link_wird_abgelehnt() {
    let p = neuer_pfad();
    assert_eq!(monat_voll(&p), Ok(()));
    let link = p.with_file_name("link.txt");
    assert!(
        std::os::unix::fs::symlink(&p, &link).is_ok(),
        "Vorbedingung: Link"
    );
    assert!(
        Kassenbuch::oeffnen(&link).is_err(),
        "Kassenbuch ueber einen Link geoeffnet"
    );
}

// ---------------------------------------------------------------------------------------------
// 1·R02 – Weiterschreiben nach Schreibfehler (R29)
// ---------------------------------------------------------------------------------------------

/// Nach einem Schreibfehler ist das Kassenbuch bis zum geprueften Wiederanlauf gesperrt.
#[test]
#[ignore = "offen: 1·R02 nach einem Schreibfehler wird weitergeschrieben"]
fn befund_1_r02_schreibfehler_sperrt_das_kassenbuch() {
    let p = neuer_pfad();
    let g = KiGrenzen::standard();
    let d = KiDeckel::standard(DACHS);
    let Ok(mut k) = Kassenbuch::oeffnen(&p) else {
        unreachable!("Vorbedingung: Kassenbuch offen")
    };
    // An der Stelle des Kassenbuchs liegt ein Verzeichnis: Schreiben scheitert.
    assert!(fs::create_dir(&p).is_ok(), "Vorbedingung");
    assert!(
        k.reserviere(&g, &d, DACHS, tag(2), 1).is_err(),
        "Vorbedingung"
    );
    fs::remove_dir(&p).ok();
    assert!(
        k.reserviere(&g, &d, DACHS, tag(2), 1).is_err(),
        "nach dem Schreibfehler wurde weitergeschrieben"
    );
}

// ---------------------------------------------------------------------------------------------
// 2·A12 – Hoechstpreis wird aufgerundet (R21)
// ---------------------------------------------------------------------------------------------

/// Dachs bei einem Ereignis: 16.000 Token zu 4 $ und 4.000 Token zu 20 $ je Million ergeben
/// 0,144 €. Reserviert werden 15 Cent, nie 14.
#[test]
fn grenzfall_2_a12_hoechstpreis_wird_auf_ganze_cent_aufgerundet() {
    let mut d = KiDeckel::standard(DACHS);
    d.max_token_ein = 16_000;
    d.max_token_aus = 4_000;
    assert_eq!(d.hoechstkosten(), Some(Cent(15)));
    d.max_token_aus = 0;
    d.max_token_ein = 250_000; // genau 1,00 $
    assert_eq!(d.hoechstkosten(), Some(Cent(100)));
    d.max_token_ein = 250_001;
    assert_eq!(d.hoechstkosten(), Some(Cent(101)));
}
