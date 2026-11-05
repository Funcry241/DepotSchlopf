//! Normative Tests des KI-Budgetwaechters im Journal: Was passiert, wenn jemand jede Sekunde
//! fragt? Aus `depot_ki` hierher verlegt, seit das Kassenbuch im Journal steht.
#![allow(
    clippy::many_single_char_names,
    clippy::panic,
    reason = "Testcode: kurze Namen, Abbruch bei fehlender Vorbedingung"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{Art, Journal, JournalFehler, inhalt_fuer, vermerk_fuer};
use depot_kern::{BeraterId, Cent};
use depot_ki::{Datum, KiDeckel, KiGrenzen, Sperre, grenzabdruck};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

/// Eigenes, leeres Verzeichnis je Test; liefert den Pfad des Journals darin.
fn leerer_pfad() -> PathBuf {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_ki_test_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).ok();
    dir.join("journal.txt")
}

/// Wie `leerer_pfad`, mit eingerichtetem Journal.
fn neuer_pfad() -> PathBuf {
    let p = leerer_pfad();
    if let Err(e) = Journal::einrichten(&p) {
        panic!("Einrichtung: {e:?}");
    }
    p
}

fn tag(t: u8) -> Datum {
    Datum::neu(2026, 11, t).unwrap_or_else(|| panic!("Datum {t}.11.2026"))
}

fn journal(p: &Path) -> Journal {
    match Journal::oeffnen(p) {
        Ok(j) => j,
        Err(e) => panic!("Journal nicht offen: {e:?}"),
    }
}

/// Journal mit genau diesen Eintraegen, alle bestaetigt.
fn schreib(p: &Path, eintraege: &[(Art, &str)]) {
    let inhalt = inhalt_fuer(eintraege);
    fs::write(p, &inhalt).ok();
    fs::write(
        p.with_extension("vermerk"),
        vermerk_fuer(&inhalt, 0).unwrap_or_default(),
    )
    .ok();
}

const DACHS: BeraterId = BeraterId::Dachs;

#[test]
fn hoechstkosten_je_aufruf() {
    assert_eq!(KiDeckel::standard(DACHS).hoechstkosten(), Some(Cent(41)));
    assert_eq!(
        KiDeckel::standard(BeraterId::Maus).hoechstkosten(),
        Some(Cent(23))
    );
}

/// Eine Schleife fragt einen ganzen Tag lang jede Sekunde: genau zwei Erlaubnisse.
#[test]
fn jede_sekunde_fragen_bringt_nur_zwei_aufrufe() {
    let p = neuer_pfad();
    let mut j = journal(&p);
    let k = &mut j;
    let mut erlaubt = 0_u32;
    for _sekunde in 0..86_400 {
        match k.reserviere(DACHS, tag(2), 1) {
            Ok(_) => erlaubt = erlaubt.saturating_add(1),
            Err(e) => assert_eq!(e, Sperre::ZyklusVoll),
        }
    }
    assert_eq!(erlaubt, 2);
    assert_eq!(k.stand().ki().buchungen().len(), 2);
}

/// Die Schleife zaehlt den Zyklus hoch, um die Grenze zu umgehen: der Tag ist trotzdem voll.
#[test]
fn neuer_zyklus_am_selben_tag_gesperrt() {
    let p = neuer_pfad();
    let mut j = journal(&p);
    let k = &mut j;
    assert!(k.reserviere(DACHS, tag(2), 1).is_ok());
    for zyklus in 2..1_000 {
        assert_eq!(
            k.reserviere(DACHS, tag(2), zyklus).err(),
            Some(Sperre::TagVoll)
        );
    }
}

/// Jeden Tag ein neuer Zyklus, einen Monat lang: hoechstens 46 Aufrufe, hoechstens 20 €.
#[test]
fn monat_hoechstens_46_aufrufe() {
    let p = neuer_pfad();
    let mut j = journal(&p);
    let k = &mut j;
    let mut zyklus = 0_u32;
    for t in 1..=30 {
        zyklus = zyklus.saturating_add(1);
        for _ in 0..100 {
            let _ = k.reserviere(BeraterId::Maus, tag(t), zyklus);
        }
    }
    let summe: i64 = k
        .stand()
        .ki()
        .buchungen()
        .iter()
        .map(|b| b.reserviert.0)
        .sum();
    assert_eq!(k.stand().ki().buchungen().len(), 46);
    assert!(
        summe <= KiGrenzen::standard().kosten_je_monat().0,
        "{summe}"
    );
}

/// Neustart des Programms setzt nichts zurueck: die Zaehler stehen in der Datei.
#[test]
fn neustart_setzt_nichts_zurueck() {
    let p = neuer_pfad();
    {
        let mut j = journal(&p);
        let k = &mut j;
        assert!(k.reserviere(DACHS, tag(3), 5).is_ok());
        assert!(k.reserviere(DACHS, tag(3), 5).is_ok());
    }
    let mut j = journal(&p);
    let k = &mut j;
    assert_eq!(k.stand().ki().buchungen().len(), 2);
    assert_eq!(
        k.reserviere(DACHS, tag(3), 5).err(),
        Some(Sperre::ZyklusVoll)
    );
}

/// Unlesbare oder unzulaessige KI-Eintraege in einem sonst gueltigen Journal sperren alles.
/// Ebenso eine Buchung ohne vorangehenden Grenzabdruck. Geloeschte Zeilen und Abbrueche prueft
/// das Journal selbst (`depot_journal`).
#[test]
fn kaputtes_kassenbuch_sperrt() {
    let abdruck = grenzabdruck();
    for (fall, eintraege) in [
        (
            "Muell",
            vec![(Art::KiGrenzen, abdruck.as_str()), (Art::Ki, "Muell")],
        ),
        (
            "negativ",
            vec![
                (Art::KiGrenzen, abdruck.as_str()),
                (Art::Ki, "Dachs;2026-11-01;1;-500"),
            ],
        ),
        (
            "Fuchs",
            vec![
                (Art::KiGrenzen, abdruck.as_str()),
                (Art::Ki, "Fuchs;2026-11-01;1;48"),
            ],
        ),
        (
            "nicht kanonisch",
            vec![
                (Art::KiGrenzen, abdruck.as_str()),
                (Art::Ki, "Dachs;2026-11-01;01;48"),
            ],
        ),
        (
            "ohne Grenzabdruck",
            vec![(Art::Ki, "Dachs;2026-11-01;1;48")],
        ),
        ("Abdruck unlesbar", vec![(Art::KiGrenzen, "xyz")]),
    ] {
        let p = leerer_pfad();
        schreib(&p, &eintraege);
        assert!(
            matches!(Journal::oeffnen(&p), Err(JournalFehler::Halt(_))),
            "{fall}"
        );
    }
}

/// Die Uhr wird zurueckgestellt: gesperrt.
#[test]
fn uhr_rueckwaerts_gesperrt() {
    let p = neuer_pfad();
    let mut j = journal(&p);
    let k = &mut j;
    assert!(k.reserviere(DACHS, tag(10), 3).is_ok());
    assert_eq!(
        k.reserviere(DACHS, tag(9), 4).err(),
        Some(Sperre::UhrRueckwaerts)
    );
}

/// Dachs und Maus haben getrennte Konten: Ist Dachs leer, darf Maus noch.
#[test]
fn getrennte_konten() {
    let p = neuer_pfad();
    let mut j = journal(&p);
    let k = &mut j;
    assert!(k.reserviere(DACHS, tag(4), 1).is_ok());
    assert!(k.reserviere(DACHS, tag(4), 1).is_ok());
    assert!(k.reserviere(DACHS, tag(4), 1).is_err());
    assert!(k.reserviere(BeraterId::Maus, tag(4), 1).is_ok());
}

/// Zu lange Eingaben werden vor dem Senden erkannt.
#[test]
fn eingabe_deckel() {
    let p = neuer_pfad();
    let mut j = journal(&p);
    let k = &mut j;
    let erl = k.reserviere(DACHS, tag(6), 1).ok();
    let Some(erl) = erl else {
        panic!("keine Erlaubnis");
    };
    assert!(erl.eingabe_passt(&"a".repeat(100_000)));
    assert!(!erl.eingabe_passt(&"a".repeat(100_001)));
}

fn reservieren(k: &mut Journal, t: Datum, zyklus: u32) {
    if let Err(e) = k.reserviere(DACHS, t, zyklus) {
        panic!("Reservierung {t:?}/{zyklus}: {e:?}");
    }
}

/// Ist das Monatsbudget erschoepft, bleibt die KI bis zum Monatsende gesperrt, auch nach einem
/// Neustart. Im Folgemonat darf sie wieder.
#[test]
fn erschoepfter_monat_bleibt_bis_zum_folgemonat_gesperrt() {
    let p = neuer_pfad();
    {
        let mut j = journal(&p);
        let k = &mut j;
        for t in 1..=23 {
            reservieren(k, tag(t), u32::from(t));
            reservieren(k, tag(t), u32::from(t));
        }
        assert_eq!(
            k.reserviere(DACHS, tag(24), 24).err(),
            Some(Sperre::MonatVoll)
        );
    }
    let mut j = journal(&p);
    let k = &mut j;
    assert_eq!(
        k.reserviere(DACHS, tag(30), 30).err(),
        Some(Sperre::MonatVoll)
    );
    let dezember = Datum::neu(2026, 12, 1).unwrap_or_else(|| panic!("Datum"));
    assert!(k.reserviere(DACHS, dezember, 31).is_ok());
}

/// Uebergangsregel fuer spaetere Budgetaenderungen: Buchungen unter einem frueheren
/// Grenzabdruck werden nicht nach den heutigen Grenzen beurteilt, zaehlen aber weiter mit. Ein
/// voller Monat bleibt also voll, auch wenn sich die Grenzen aendern. Vor der ersten Buchung
/// unter neuen Grenzen steht deren Abdruck im Journal.
#[test]
fn alte_buchungen_zaehlen_nach_einer_grenzaenderung_weiter() {
    let alt = "0".repeat(64);
    let p = leerer_pfad();
    // Unter den alten Grenzen kostete ein Aufruf 600 Cent; heute waere das unzulaessig.
    schreib(
        &p,
        &[
            (Art::KiGrenzen, alt.as_str()),
            (Art::Ki, "Dachs;2026-11-02;1;600"),
            (Art::Ki, "Dachs;2026-11-02;1;600"),
            (Art::Ki, "Dachs;2026-11-03;2;600"),
            (Art::Ki, "Dachs;2026-11-03;2;600"),
        ],
    );
    let mut j = journal(&p);
    let k = &mut j;
    assert_eq!(k.stand().ki().buchungen().len(), 4);
    assert_eq!(
        k.reserviere(DACHS, tag(4), 3).err(),
        Some(Sperre::MonatsbudgetErschoepft)
    );
    let dezember = Datum::neu(2026, 12, 1).unwrap_or_else(|| panic!("Datum"));
    assert!(k.reserviere(DACHS, dezember, 4).is_ok());
    let arten: Vec<Art> = j.eintraege().iter().map(|(a, _)| *a).collect();
    assert_eq!(arten.get(5..), Some([Art::KiGrenzen, Art::Ki].as_slice()));
    assert_eq!(
        j.eintraege().get(5).map(|(_, t)| t.clone()),
        Some(grenzabdruck())
    );
    drop(j);
    // Dieselben 30-Cent-Buchungen unter dem heutigen Abdruck: Halt.
    let q = leerer_pfad();
    let heute = grenzabdruck();
    schreib(
        &q,
        &[
            (Art::KiGrenzen, heute.as_str()),
            (Art::Ki, "Dachs;2026-11-02;1;30"),
        ],
    );
    assert!(matches!(
        Journal::oeffnen(&q),
        Err(JournalFehler::Halt(m)) if m.contains("Reservierung")
    ));
}

/// Der Grenzabdruck haengt an den festgeschriebenen Werten und ist stabil.
#[test]
fn grenzabdruck_ist_stabil() {
    let a = grenzabdruck();
    assert_eq!(a, grenzabdruck());
    assert_eq!(a.len(), 64);
    assert!(
        a.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
}
