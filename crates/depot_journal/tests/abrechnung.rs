//! Kosten werden nur durch einen bestätigten, zur Reservierung passenden Beleg vermindert.
use depot_journal::{Art, Journal, JournalFehler, KiZweck, inhalt_fuer, vermerk_fuer};
use depot_kern::{BeraterId, Cent, Datum};
use depot_ki::{KiDeckel, KiGrenzen, Sperre};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};
static NR: AtomicU32 = AtomicU32::new(0);
fn ort() -> Result<PathBuf, String> {
    let pfad = std::env::temp_dir().join(format!(
        "depot-abrechnung-{}-{}",
        std::process::id(),
        NR.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&pfad).map_err(|erlaubnis| erlaubnis.to_string())?;
    Ok(pfad.join("journal.txt"))
}
fn tag(t: u8) -> Result<Datum, String> {
    Datum::neu(2026, 11, t).ok_or("Datum".into())
}
fn offen(pfad: &std::path::Path) -> Result<Journal, String> {
    Journal::oeffnen(pfad).map_err(|erlaubnis| format!("{erlaubnis:?}"))
}
fn schreibe(pfad: &std::path::Path, eintraege: &[(Art, &str)]) -> Result<(), String> {
    let bytes = inhalt_fuer(eintraege);
    fs::write(pfad, &bytes).map_err(|erlaubnis| erlaubnis.to_string())?;
    fs::write(
        pfad.with_extension("vermerk"),
        vermerk_fuer(&bytes, 0).ok_or("Vermerk")?,
    )
    .map_err(|erlaubnis| erlaubnis.to_string())
}
#[test]
fn bestaetigte_minderkosten_bleiben_ueber_neustart_erhalten() -> Result<(), String> {
    let pfad = ort()?;
    Journal::einrichten(&pfad).map_err(|erlaubnis| format!("{erlaubnis:?}"))?;
    let mut buch = offen(&pfad)?;
    let erlaubnis = buch
        .reserviere(BeraterId::Dachs, tag(8)?, 1)
        .map_err(|erlaubnis| format!("{erlaubnis:?}"))?;
    assert_eq!(erlaubnis.buchung().reserviert, Cent(24));
    let abrechnung = erlaubnis.abrechnung(100, 100).ok_or("Abrechnung")?;
    assert_eq!(abrechnung.kosten(), Cent(1));
    buch.rechne_ki_ab(&abrechnung)
        .map_err(|erlaubnis| format!("{erlaubnis:?}"))?;
    drop(buch);
    let buch = offen(&pfad)?;
    assert_eq!(buch.stand().ki().buchungen().len(), 1);
    assert_eq!(
        buch.stand().ki().buchungen().first().map(|b| b.reserviert),
        Some(Cent(1))
    );
    assert!(
        buch.eintraege()
            .iter()
            .any(|erlaubnis| erlaubnis.0 == Art::KiAbrechnung)
    );
    Ok(())
}
#[test]
fn fehlende_oder_unplausible_abrechnung_bleibt_voll_gebucht() -> Result<(), String> {
    let pfad = ort()?;
    Journal::einrichten(&pfad).map_err(|erlaubnis| format!("{erlaubnis:?}"))?;
    let mut buch = offen(&pfad)?;
    let erlaubnis = buch
        .reserviere(BeraterId::Dachs, tag(8)?, 1)
        .map_err(|erlaubnis| format!("{erlaubnis:?}"))?;
    assert!(erlaubnis.abrechnung(40_001, 0).is_none());
    assert!(erlaubnis.abrechnung(0, 16_001).is_none());
    assert!(erlaubnis.abrechnung(u64::MAX, u64::MAX).is_none());
    drop(buch);
    let buch = offen(&pfad)?;
    assert_eq!(
        buch.stand().ki().buchungen().first().map(|b| b.reserviert),
        Some(Cent(24))
    );
    Ok(())
}
#[test]
fn journal_lehnt_falsche_und_doppelte_abrechnung_ab() -> Result<(), String> {
    let hash = depot_ki::grenzabdruck();
    for text in ["0;24;-1", "0;24;25", "1;24;1", "0;12;1", "+0;24;1"] {
        let pfad = ort()?;
        schreibe(
            &pfad,
            &[
                (Art::KiGrenzen, &hash),
                (Art::Ki, "Dachs;2026-11-08;1;24"),
                (Art::KiAbrechnung, text),
            ],
        )?;
        assert!(
            matches!(Journal::oeffnen(&pfad), Err(JournalFehler::Halt(_))),
            "{text}"
        );
    }
    let pfad = ort()?;
    schreibe(
        &pfad,
        &[
            (Art::KiGrenzen, &hash),
            (Art::Ki, "Dachs;2026-11-08;1;24"),
            (Art::KiAbrechnung, "0;24;1"),
            (Art::KiAbrechnung, "0;24;1"),
        ],
    )?;
    assert!(matches!(
        Journal::oeffnen(&pfad),
        Err(JournalFehler::Halt(_))
    ));
    Ok(())
}
#[test]
fn tagesrahmen_wird_durch_bestaetigte_kosten_nur_um_diese_verringert() -> Result<(), String> {
    let pfad = ort()?;
    Journal::einrichten(&pfad).map_err(|erlaubnis| format!("{erlaubnis:?}"))?;
    let mut buch = offen(&pfad)?;
    let grenzen = KiGrenzen::standard();
    let datum = tag(8)?;
    let rahmen = depot_ki::tagesrahmen(
        &grenzen,
        buch.stand().ki().buchungen(),
        BeraterId::Dachs,
        datum,
    )
    .map_err(|erlaubnis| format!("{erlaubnis:?}"))?;
    let erlaubnis = buch
        .reserviere(BeraterId::Dachs, datum, 1)
        .map_err(|erlaubnis| format!("{erlaubnis:?}"))?;
    buch.rechne_ki_ab(&erlaubnis.abrechnung(100, 100).ok_or("Kosten")?)
        .map_err(|erlaubnis| format!("{erlaubnis:?}"))?;
    assert_eq!(
        depot_ki::tagesrahmen(
            &grenzen,
            buch.stand().ki().buchungen(),
            BeraterId::Dachs,
            datum
        ),
        Some(Cent(rahmen.0.saturating_sub(1))).ok_or(Sperre::Rechenfehler)
    );
    assert_eq!(
        depot_ki::tagesrahmen(
            &grenzen,
            buch.stand().ki().buchungen(),
            BeraterId::Maus,
            datum
        ),
        Ok(rahmen)
    );
    assert!(
        KiDeckel::kurz(BeraterId::Dachs).max_token_aus()
            < KiDeckel::standard(BeraterId::Dachs).max_token_aus()
    );
    Ok(())
}
#[test]
fn zusatzfragen_lassen_die_naechste_erstmeinung_frei() -> Result<(), String> {
    let pfad = ort()?;
    schreibe(
        &pfad,
        &[
            (Art::KiGrenzen, &"0".repeat(64)),
            (Art::Ki, "Dachs;2026-11-01;1;1970"),
        ],
    )?;
    let buch = offen(&pfad)?;
    assert_eq!(
        buch.pruefe_ki(BeraterId::Dachs, tag(29)?, 2, KiZweck::Vorbereitung),
        Err(Sperre::ErstmeinungReserviert)
    );
    assert_eq!(
        buch.pruefe_ki(BeraterId::Dachs, tag(29)?, 2, KiZweck::Erstmeinung),
        Ok(())
    );
    assert_eq!(
        buch.pruefe_ki(BeraterId::Maus, tag(29)?, 2, KiZweck::Zweitmeinung),
        Err(Sperre::ErstmeinungReserviert)
    );
    Ok(())
}

#[test]
fn jahresrest_wird_fuer_die_naechste_erstmeinung_geschuetzt() -> Result<(), String> {
    let pfad = ort()?;
    schreibe(
        &pfad,
        &[
            (Art::KiGrenzen, &"0".repeat(64)),
            (Art::Ki, "Dachs;2026-01-01;1;23970"),
        ],
    )?;
    let buch = offen(&pfad)?;
    assert_eq!(
        buch.pruefe_ki(BeraterId::Dachs, tag(29)?, 2, KiZweck::Vorbereitung),
        Err(Sperre::ErstmeinungReserviert)
    );
    assert_eq!(
        buch.pruefe_ki(BeraterId::Dachs, tag(29)?, 2, KiZweck::Erstmeinung),
        Ok(())
    );
    Ok(())
}
