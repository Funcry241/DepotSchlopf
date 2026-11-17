//! Der Anzeige-Cache nutzt bestätigte Anhänge. Die volle Prüfung bleibt der Maßstab.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use depot_journal::{Journal, JournalFehler, Lesecache};
use depot_kern::Datum;

struct Ort(PathBuf);
impl Drop for Ort {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn bestaetigter_anhang_und_veraenderte_alte_zeile_erkannt() -> Result<(), String> {
    let ort = Ort(std::env::temp_dir().join(format!("ds-lesecache-{}", std::process::id())));
    fs::create_dir(&ort.0).map_err(|e| e.to_string())?;
    let pfad = ort.0.join("journal.txt");
    let tag = Datum::neu(2026, 11, 17).ok_or("Datum")?;
    let journalfehler = |e: JournalFehler| format!("{e:?}");
    Journal::einrichten(&pfad).map_err(journalfehler)?;
    let mut journal = Journal::oeffnen(&pfad).map_err(journalfehler)?;
    journal.beobachte(tag, "Vorher").map_err(journalfehler)?;
    let mut cache = Lesecache::default();
    assert!(matches!(
        cache.lesen(&pfad, 4096),
        Err(JournalFehler::Belegt)
    ));
    drop(journal);

    let (_, zuerst) = cache.lesen(&pfad, 4096).map_err(journalfehler)?;
    assert_eq!(zuerst.len(), 1);
    assert_eq!(cache.lesen(&pfad, 4096).map_err(journalfehler)?.1.len(), 1);
    let mut journal = Journal::oeffnen(&pfad).map_err(journalfehler)?;
    journal.beobachte(tag, "Anhang").map_err(journalfehler)?;
    let erwartet = (journal.stand().clone(), journal.eintraege().to_vec());
    drop(journal);

    let (stand, eintraege) = cache.lesen(&pfad, 4096).map_err(journalfehler)?;
    assert_eq!(stand, &erwartet.0);
    assert_eq!(eintraege, erwartet.1);

    let original = fs::read_to_string(&pfad).map_err(|e| e.to_string())?;
    let anders = original.replacen("Vorher", "Anders", 1);
    assert_ne!(original, anders);
    assert_eq!(original.len(), anders.len());
    let geaendert = fs::metadata(&pfad)
        .and_then(|metadaten| metadaten.modified())
        .map_err(|e| e.to_string())?;
    fs::write(&pfad, anders).map_err(|e| e.to_string())?;
    fs::OpenOptions::new()
        .write(true)
        .open(&pfad)
        .and_then(|datei| datei.set_modified(geaendert + Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    assert!(matches!(
        cache.lesen(&pfad, 4096),
        Err(JournalFehler::Halt(_))
    ));
    assert!(matches!(
        Journal::oeffnen(&pfad),
        Err(JournalFehler::Halt(_))
    ));
    assert!(matches!(
        Journal::pruefe_lesend(&pfad, |_| ()),
        Err(JournalFehler::Halt(_))
    ));
    assert!(matches!(
        Lesecache::default().lesen(&pfad, 4096),
        Err(JournalFehler::Halt(_))
    ));
    Ok(())
}
