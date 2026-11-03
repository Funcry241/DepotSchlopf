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
use depot_ki::{Datum, Kassenbuch, KiDeckel, KiGrenzen, Sperre, anker_fuer};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

/// Eigenes, leeres Verzeichnis je Test; liefert den Pfad des Kassenbuchs darin.
fn leerer_pfad() -> PathBuf {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_ki_test_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).ok();
    dir.join("kassenbuch.txt")
}

/// Wie `leerer_pfad`, mit eingerichtetem Kassenbuch.
fn neuer_pfad() -> PathBuf {
    let p = leerer_pfad();
    if let Err(e) = Kassenbuch::einrichten(&p) {
        panic!("Einrichtung: {e:?}");
    }
    p
}

fn tag(t: u8) -> Datum {
    Datum::neu(2026, 11, t).unwrap_or_else(|| panic!("Datum {t}.11.2026"))
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
    let mut erlaubt = 0_u32;
    for _sekunde in 0..86_400 {
        match k.reserviere(DACHS, tag(2), 1) {
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
    assert!(k.reserviere(DACHS, tag(2), 1).is_ok());
    for zyklus in 2..1_000 {
        assert_eq!(
            k.reserviere(DACHS, tag(2), zyklus).err(),
            Some(Sperre::TagVoll)
        );
    }
}

/// Jeden Tag ein neuer Zyklus, einen Monat lang: hoechstens 4 Aufrufe, hoechstens 4 €.
#[test]
fn monat_hoechstens_vier_aufrufe() {
    let p = neuer_pfad();
    let mut k = oeffnen(&p);
    let mut zyklus = 0_u32;
    for t in 1..=30 {
        zyklus = zyklus.saturating_add(1);
        for _ in 0..100 {
            let _ = k.reserviere(BeraterId::Maus, tag(t), zyklus);
        }
    }
    let summe: i64 = k.buchungen().iter().map(|b| b.reserviert.0).sum();
    assert_eq!(k.buchungen().len(), 4);
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
        let mut k = oeffnen(&p);
        assert!(k.reserviere(DACHS, tag(3), 5).is_ok());
        assert!(k.reserviere(DACHS, tag(3), 5).is_ok());
    }
    let mut k = oeffnen(&p);
    assert_eq!(k.buchungen().len(), 2);
    assert_eq!(
        k.reserviere(DACHS, tag(3), 5).err(),
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

/// Eine unlesbare Zeile oder ein abgebrochener letzter Eintrag sperrt alles, auch wenn der
/// Anker genau zu diesem Inhalt passt. Geloeschte Zeilen prueft `befunde.rs` (1·K04).
#[test]
fn kaputtes_kassenbuch_sperrt() {
    for inhalt in [
        "v1;Dachs;2026-11-01;1;48\nMuell\n",
        "v1;Dachs;2026-11-01;1;48\nv1;Dachs;2026-11-0",
        "v1;Dachs;2026-11-01;1;-500\n",
        "v1;Fuchs;2026-11-01;1;48\n",
    ] {
        let p = leerer_pfad();
        fs::write(&p, inhalt).ok();
        fs::write(p.with_extension("anker"), anker_fuer(inhalt.as_bytes())).ok();
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
    let mut k = oeffnen(&p);
    assert!(k.reserviere(DACHS, tag(4), 1).is_ok());
    assert!(k.reserviere(DACHS, tag(4), 1).is_ok());
    assert!(k.reserviere(DACHS, tag(4), 1).is_err());
    assert!(k.reserviere(BeraterId::Maus, tag(4), 1).is_ok());
}

/// Zu lange Eingaben werden vor dem Senden erkannt.
#[test]
fn eingabe_deckel() {
    let p = neuer_pfad();
    let mut k = oeffnen(&p);
    let erl = k.reserviere(DACHS, tag(6), 1).ok();
    let Some(erl) = erl else {
        panic!("keine Erlaubnis");
    };
    assert!(erl.eingabe_passt(&"a".repeat(40_000)));
    assert!(!erl.eingabe_passt(&"a".repeat(40_001)));
}

fn reservieren(k: &mut Kassenbuch, t: Datum, zyklus: u32) {
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
        let mut k = oeffnen(&p);
        reservieren(&mut k, tag(2), 1);
        reservieren(&mut k, tag(2), 1);
        reservieren(&mut k, tag(3), 2);
        reservieren(&mut k, tag(3), 2);
        assert_eq!(
            k.reserviere(DACHS, tag(4), 3).err(),
            Some(Sperre::MonatVoll)
        );
    }
    let mut k = oeffnen(&p);
    assert_eq!(
        k.reserviere(DACHS, tag(30), 4).err(),
        Some(Sperre::MonatVoll)
    );
    let dezember = Datum::neu(2026, 12, 1).unwrap_or_else(|| panic!("Datum"));
    assert!(k.reserviere(DACHS, dezember, 5).is_ok());
}

/// Eine bestaetigte Zeile wird bei gleicher Zeilenzahl geaendert, und die Historie bleibt
/// gueltig (die Buchung rutscht in den Vormonat und gibt im November Budget frei): Halt.
#[test]
fn geaenderte_bestaetigte_zeile_fuehrt_zu_halt() {
    let p = neuer_pfad();
    {
        let mut k = oeffnen(&p);
        reservieren(&mut k, tag(2), 1);
    }
    let alt = fs::read_to_string(&p).unwrap_or_default();
    let neu = alt.replace("2026-11-02", "2026-10-02");
    assert_ne!(alt, neu, "Vorbedingung: Zeile geaendert");
    fs::write(&p, neu).ok();
    assert!(
        matches!(Kassenbuch::oeffnen(&p), Err(Sperre::Kassenbuch(m)) if m.contains("bestaetigten Stand")),
    );
}

/// Hoechstens eine Zeile darf ueber den Anker hinausgehen; zwei sind kein Absturz mehr.
#[test]
fn zwei_unbestaetigte_zeilen_fuehren_zu_halt() {
    let p = neuer_pfad();
    fs::write(&p, "v1;Dachs;2026-11-02;1;48\nv1;Dachs;2026-11-02;1;48\n").ok();
    assert!(
        matches!(Kassenbuch::oeffnen(&p), Err(Sperre::Kassenbuch(m)) if m.contains("unbestaetigter")),
    );
}

/// Eine unbestaetigte, aber gueltige Zeile (Absturz nach dem Buchen, vor dem Anker) zaehlt und
/// wird bestaetigt; ist sie ungueltig, gilt Halt.
#[test]
fn eine_unbestaetigte_zeile_wird_geprueft_und_bestaetigt() {
    let p = neuer_pfad();
    fs::write(&p, "v1;Dachs;2026-11-02;1;48\n").ok();
    let k = oeffnen(&p);
    assert_eq!(k.buchungen().len(), 1);
    drop(k);
    assert_eq!(
        fs::read_to_string(p.with_extension("anker")).unwrap_or_default(),
        anker_fuer(b"v1;Dachs;2026-11-02;1;48\n")
    );
    let q = neuer_pfad();
    fs::write(&q, "v1;Dachs;2026-11-02;1;1\n").ok();
    assert!(matches!(
        Kassenbuch::oeffnen(&q),
        Err(Sperre::Kassenbuch(m)) if m.contains("Reservierung")
    ));
}

/// Einrichten legt nur neu an und laesst ein vorhandenes Kassenbuch unberuehrt.
#[test]
fn einrichten_ueberschreibt_nichts() {
    let p = neuer_pfad();
    {
        let mut k = oeffnen(&p);
        reservieren(&mut k, tag(2), 1);
    }
    let vorher = fs::read(&p).unwrap_or_default();
    assert!(Kassenbuch::einrichten(&p).is_err());
    assert_eq!(fs::read(&p).unwrap_or_default(), vorher);
    assert_eq!(oeffnen(&p).buchungen().len(), 1);
}
