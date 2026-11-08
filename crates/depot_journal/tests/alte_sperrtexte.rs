//! Historische Sperrtexte bleiben trotz neuer Anzeigesprache lesbar und unverändert.
use depot_journal::{Art, Journal, PapierHalt, inhalt_fuer, vermerk_fuer};
use std::{
    fs,
    sync::atomic::{AtomicU32, Ordering},
};
static NR: AtomicU32 = AtomicU32::new(0);
#[test]
fn alte_ascii_sperrtexte_behalten_ihre_wirkung_und_bytes() -> Result<(), String> {
    for (text, erwartet) in [
        (
            "Notbremse aktiv: nur offene Verkaeufe weiter abgleichen",
            PapierHalt::Notbremse,
        ),
        (
            "Notbremse: 30 % Gesamtverlust erreicht. Neue Kaeufe gesperrt; Freigabe des Eigentuemers noetig",
            PapierHalt::Gesamtverlust,
        ),
    ] {
        let ordner = std::env::temp_dir().join(format!(
            "depot-alte-sprache-{}-{}",
            std::process::id(),
            NR.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&ordner).map_err(|e| e.to_string())?;
        let pfad = ordner.join("journal.txt");
        let payload = format!("2026-11-08;Waechter;{text}");
        let bytes = inhalt_fuer(&[(Art::Begruendung, &payload)]);
        let vermerk = vermerk_fuer(&bytes, 0).ok_or("Vermerk")?;
        fs::write(&pfad, &bytes).map_err(|e| e.to_string())?;
        fs::write(pfad.with_extension("vermerk"), &vermerk).map_err(|e| e.to_string())?;
        let j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
        assert_eq!(j.stand().papier_halt(), Some(erwartet));
        drop(j);
        assert_eq!(fs::read(&pfad).map_err(|e| e.to_string())?, bytes);
        assert_eq!(
            fs::read_to_string(pfad.with_extension("vermerk")).map_err(|e| e.to_string())?,
            vermerk
        );
        fs::remove_dir_all(ordner).map_err(|e| e.to_string())?;
    }
    Ok(())
}
