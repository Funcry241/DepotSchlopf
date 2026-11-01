//! Normative Tests des KI-Budgetwaechters: Was passiert, wenn jemand jede Sekunde fragt?
#![allow(
    clippy::many_single_char_names,
    clippy::panic,
    reason = "Testcode: kurze Namen, Abbruch bei fehlender Vorbedingung"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use depot_kern::{BeraterId, Cent};
use depot_ki::{Datum, Kassenbuch, KiDeckel, KiGrenzen, Sperre};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

/// Eigenes, leeres Verzeichnis je Test.
fn neuer_pfad() -> PathBuf {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_ki_test_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).ok();
    dir.join("kassenbuch.txt")
}

fn tag(t: u8) -> Datum {
    Datum::neu(2026, 11, t).unwrap_or(Datum {
        jahr: 2026,
        monat: 11,
        tag: 1,
    })
}

fn oeffnen(p: &Path) -> Kassenbuch {
    match Kassenbuch::oeffnen(p) {
        Ok(k) => k,
        Err(e) => panic!("Kassenbuch nicht offen: {e:?}"),
    }
}

const DACHS: BeraterId = BeraterId::Dachs;

#[test]
fn hoechstkosten_je_aufruf() {
    assert_eq!(KiDeckel::standard(DACHS).hoechstkosten(), Some(Cent(48)));
    assert_eq!(
        KiDeckel::standard(BeraterId::Maus).hoechstkosten(),
        Some(Cent(90))
    );
}

/// Eine Schleife fragt einen ganzen Tag lang jede Sekunde: genau zwei Erlaubnisse.
#[test]
fn jede_sekunde_fragen_bringt_nur_zwei_aufrufe() {
    let p = neuer_pfad();
    let mut k = oeffnen(&p);
    let g = KiGrenzen::standard();
    let d = KiDeckel::standard(DACHS);
    let mut erlaubt = 0_u32;
    for _sekunde in 0..86_400 {
        match k.reserviere(&g, &d, DACHS, tag(2), 1) {
            Ok(_) => erlaubt = erlaubt.saturating_add(1),
            Err(e) => assert_eq!(e, Sperre::ZyklusVoll),
        }
    }
    assert_eq!(erlaubt, 2);
    assert_eq!(k.buchungen().len(), 2);
}

/// Die Schleife zaehlt den Zyklus hoch, um die Grenze zu umgehen: der Tag ist trotzdem voll.
#[test]
fn neuer_zyklus_am_selben_tag_gesperrt() {
    let p = neuer_pfad();
    let mut k = oeffnen(&p);
    let g = KiGrenzen::standard();
    let d = KiDeckel::standard(DACHS);
    assert!(k.reserviere(&g, &d, DACHS, tag(2), 1).is_ok());
    for zyklus in 2..1_000 {
        assert_eq!(
            k.reserviere(&g, &d, DACHS, tag(2), zyklus).err(),
            Some(Sperre::TagVoll)
        );
    }
}

/// Jeden Tag ein neuer Zyklus, einen Monat lang: hoechstens 4 Aufrufe, hoechstens 4 €.
#[test]
fn monat_hoechstens_vier_aufrufe() {
    let p = neuer_pfad();
    let mut k = oeffnen(&p);
    let g = KiGrenzen::standard();
    let d = KiDeckel::standard(BeraterId::Maus);
    let mut zyklus = 0_u32;
    for t in 1..=30 {
        zyklus = zyklus.saturating_add(1);
        for _ in 0..100 {
            let _ = k.reserviere(&g, &d, BeraterId::Maus, tag(t), zyklus);
        }
    }
    let summe: i64 = k.buchungen().iter().map(|b| b.reserviert.0).sum();
    assert_eq!(k.buchungen().len(), 4);
    assert!(summe <= g.kosten_je_monat.0, "{summe}");
}

/// Neustart des Programms setzt nichts zurueck: die Zaehler stehen in der Datei.
#[test]
fn neustart_setzt_nichts_zurueck() {
    let p = neuer_pfad();
    let g = KiGrenzen::standard();
    let d = KiDeckel::standard(DACHS);
    {
        let mut k = oeffnen(&p);
        assert!(k.reserviere(&g, &d, DACHS, tag(3), 5).is_ok());
        assert!(k.reserviere(&g, &d, DACHS, tag(3), 5).is_ok());
    }
    let mut k = oeffnen(&p);
    assert_eq!(k.buchungen().len(), 2);
    assert_eq!(
        k.reserviere(&g, &d, DACHS, tag(3), 5).err(),
        Some(Sperre::ZyklusVoll)
    );
}

/// Zwei Laeufe gleichzeitig: der zweite bekommt das Kassenbuch nicht.
#[test]
fn zweiter_lauf_gesperrt() {
    let p = neuer_pfad();
    let _erster = oeffnen(&p);
    assert_eq!(
        Kassenbuch::oeffnen(&p).err(),
        Some(Sperre::KassenbuchBelegt)
    );
}

/// Eine manipulierte oder abgeschnittene Datei sperrt alles, statt bei null anzufangen.
#[test]
fn kaputtes_kassenbuch_sperrt() {
    for inhalt in [
        "v1;Dachs;2026-11-01;1;48\nMuell\n",
        "v1;Dachs;2026-11-01;1;48\nv1;Dachs;2026-11-0",
        "v1;Dachs;2026-11-01;1;-500\n",
        "v1;Fuchs;2026-11-01;1;48\n",
    ] {
        let p = neuer_pfad();
        fs::write(&p, inhalt).ok();
        assert!(
            matches!(Kassenbuch::oeffnen(&p), Err(Sperre::Kassenbuch(_))),
            "{inhalt:?}"
        );
    }
}

/// Die Uhr wird zurueckgestellt: gesperrt.
#[test]
fn uhr_rueckwaerts_gesperrt() {
    let p = neuer_pfad();
    let mut k = oeffnen(&p);
    let g = KiGrenzen::standard();
    let d = KiDeckel::standard(DACHS);
    assert!(k.reserviere(&g, &d, DACHS, tag(10), 3).is_ok());
    assert_eq!(
        k.reserviere(&g, &d, DACHS, tag(9), 4).err(),
        Some(Sperre::UhrRueckwaerts)
    );
}

/// Dachs und Maus haben getrennte Konten: Ist Dachs leer, darf Maus noch.
#[test]
fn getrennte_konten() {
    let p = neuer_pfad();
    let mut k = oeffnen(&p);
    let g = KiGrenzen::standard();
    let dd = KiDeckel::standard(DACHS);
    let dm = KiDeckel::standard(BeraterId::Maus);
    assert!(k.reserviere(&g, &dd, DACHS, tag(4), 1).is_ok());
    assert!(k.reserviere(&g, &dd, DACHS, tag(4), 1).is_ok());
    assert!(k.reserviere(&g, &dd, DACHS, tag(4), 1).is_err());
    assert!(k.reserviere(&g, &dm, BeraterId::Maus, tag(4), 1).is_ok());
}

/// Ein teurerer Deckel reisst das Monatsbudget, bevor gefragt wird.
#[test]
fn budget_vor_dem_aufruf_geprueft() {
    let p = neuer_pfad();
    let mut k = oeffnen(&p);
    let g = KiGrenzen::standard();
    let mut d = KiDeckel::standard(BeraterId::Maus);
    d.max_token_aus = 100_000; // 0,40 € + 5,00 € > 4 €
    assert_eq!(
        k.reserviere(&g, &d, BeraterId::Maus, tag(5), 1).err(),
        Some(Sperre::MonatsbudgetErschoepft)
    );
    assert!(k.buchungen().is_empty());
    let mut klein = KiGrenzen::standard();
    klein.kosten_je_jahr = Cent(100);
    let d = KiDeckel::standard(BeraterId::Maus);
    assert!(k.reserviere(&klein, &d, BeraterId::Maus, tag(5), 1).is_ok());
    assert_eq!(
        k.reserviere(&klein, &d, BeraterId::Maus, tag(5), 1).err(),
        Some(Sperre::JahresbudgetErschoepft)
    );
}

/// Zu lange Eingaben werden vor dem Senden erkannt.
#[test]
fn eingabe_deckel() {
    let p = neuer_pfad();
    let mut k = oeffnen(&p);
    let erl = k
        .reserviere(
            &KiGrenzen::standard(),
            &KiDeckel::standard(DACHS),
            DACHS,
            tag(6),
            1,
        )
        .ok();
    let Some(erl) = erl else {
        panic!("keine Erlaubnis");
    };
    assert!(erl.eingabe_passt(&"a".repeat(40_000)));
    assert!(!erl.eingabe_passt(&"a".repeat(40_001)));
}
