//! Belegte Nachrichten (R40, R12): Quelle, Abruf, Abdruck der rohen Antwort, Zeit der
//! Veroeffentlichung und Text. Sie beginnen keinen Tag, stehen nie doppelt im Journal, und
//! ihre Abrufe fallen nie zurueck. Jede Vorbereitung prueft ihren Erfolg ausdruecklich und endet
//! sonst mit „Vorbedingung …“.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{
    Art, Journal, JournalFehler, MAX_NACHRICHT, NachrichtBeleg, inhalt_fuer, vermerk_fuer,
};
use depot_kern::{Datum, Zeitpunkt};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

fn neues_verzeichnis() -> Result<PathBuf, String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_nachrichten_j_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
    Ok(dir.join("journal.txt"))
}

fn eingerichtet() -> Result<PathBuf, String> {
    let p = neues_verzeichnis()?;
    vorbedingung(Journal::einrichten(&p), "Einrichtung")?;
    Ok(p)
}

fn mit(eintraege: &[(Art, &str)]) -> Result<PathBuf, String> {
    let p = neues_verzeichnis()?;
    let inhalt = inhalt_fuer(eintraege);
    let vermerk = vermerk_fuer(&inhalt, 0).ok_or("Vorbedingung: Vermerk")?;
    vorbedingung(fs::write(&p, &inhalt), "Journal schreiben")?;
    vorbedingung(fs::write(p.with_extension("vermerk"), vermerk), "Vermerk")?;
    Ok(p)
}

fn oeffnen(p: &Path) -> Result<Journal, String> {
    vorbedingung(Journal::oeffnen(p), "Journal oeffnen")
}

fn utc(t: u8, h: u8, min: u8) -> Result<Zeitpunkt, String> {
    Datum::neu(2026, 11, t)
        .and_then(|d| Zeitpunkt::neu(d, h, min, 0))
        .ok_or_else(|| "Vorbedingung: Zeitpunkt".into())
}

fn beleg(abruf: Zeitpunkt, zeit: Zeitpunkt, text: &str) -> Result<NachrichtBeleg, String> {
    vorbedingung(
        NachrichtBeleg::neu("alphavantage-news", abruf, b"roh", zeit, text),
        "Beleg",
    )
}

/// Belegt, beginnt keinen Tag, uebersteht das Wiedereroeffnen und steht nie doppelt da.
#[test]
fn nachricht_mit_quelle_und_abruf() -> Result<(), String> {
    let p = eingerichtet()?;
    let mut j = oeffnen(&p)?;
    let b = beleg(
        utc(5, 17, 0)?,
        utc(5, 15, 14)?,
        "Benzinga: SAP hebt Prognose an.",
    )?;
    assert!(vorbedingung(j.belege_nachricht(&b), "Beleg")?);
    assert_eq!(
        j.stand().seit_abschluss(),
        0,
        "Belege von aussen beginnen keinen Tag"
    );
    // Dieselbe Nachricht, spaeter noch einmal abgerufen: nichts geschrieben.
    let wieder = beleg(
        utc(5, 18, 0)?,
        utc(5, 15, 14)?,
        "Benzinga: SAP hebt Prognose an.",
    )?;
    let laenge = j.eintraege().len();
    assert!(!vorbedingung(j.belege_nachricht(&wieder), "Beleg")?);
    assert_eq!(j.eintraege().len(), laenge);
    drop(j);
    let j = oeffnen(&p)?;
    assert_eq!(j.stand().nachrichten(), &[b]);
    Ok(())
}

/// Kein Abruf faellt hinter den letzten Beleg zurueck, auch nicht hinter einen Kurs- oder
/// Terminbeleg.
#[test]
fn abruf_faellt_nie_zurueck() -> Result<(), String> {
    let p = eingerichtet()?;
    let mut j = oeffnen(&p)?;
    vorbedingung(
        j.belege_nachricht(&beleg(utc(5, 17, 0)?, utc(5, 15, 0)?, "erste")?),
        "Beleg",
    )?;
    let r = j.belege_nachricht(&beleg(utc(5, 16, 0)?, utc(5, 15, 0)?, "zweite")?);
    assert!(matches!(r, Err(JournalFehler::Abgelehnt(_))), "{r:?}");
    Ok(())
}

/// Nur gueltige Belege: Quelle, Text ohne Steuerzeichen und Rand, nicht zu lang,
/// veroeffentlicht nicht nach dem Abruf.
#[test]
fn beleg_nur_gueltig() -> Result<(), String> {
    let abruf = utc(5, 17, 0)?;
    let zeit = utc(5, 15, 0)?;
    let neu = |q: &str, z: Zeitpunkt, t: &str| NachrichtBeleg::neu(q, abruf, b"roh", z, t);
    assert!(neu("alphavantage-news", zeit, "gut").is_ok());
    assert!(neu("Alphavantage", zeit, "x").is_err());
    assert!(neu("alphavantage_news", zeit, "x").is_err());
    assert!(neu("alphavantage-news", zeit, "").is_err());
    assert!(neu("alphavantage-news", zeit, " x").is_err());
    assert!(neu("alphavantage-news", zeit, "a\nb").is_err());
    assert!(neu("alphavantage-news", utc(5, 17, 1)?, "x").is_err());
    assert!(neu("alphavantage-news", zeit, &"x".repeat(MAX_NACHRICHT)).is_ok());
    assert!(neu("alphavantage-news", zeit, &"x".repeat(MAX_NACHRICHT + 1)).is_err());
    Ok(())
}

/// In der Datei gilt nur die kanonische Form.
#[test]
fn nachricht_in_der_datei_nur_kanonisch() -> Result<(), String> {
    let roh = "0".repeat(64);
    let gut = format!(
        "alphavantage-news;2026-11-05T17:00:00Z;{roh};2026-11-05T15:14:58Z;Benzinga: SAP; Prognose"
    );
    let j = vorbedingung(
        Journal::oeffnen(&mit(&[(Art::Nachricht, &gut)])?),
        "gueltiger Eintrag",
    )?;
    assert_eq!(
        j.stand().nachrichten().first().map(NachrichtBeleg::text),
        Some("Benzinga: SAP; Prognose")
    );
    let falsch = [
        gut.replace("alphavantage-news", "AV"),
        gut.replace(&roh, &"0".repeat(63)),
        gut.replace("2026-11-05T17:00:00Z", "2026-11-05T17:00:00"),
        gut.replace("15:14:58Z", "18:00:00Z"),
        gut.replace("Benzinga: SAP; Prognose", ""),
        gut.replace("Benzinga", " Benzinga"),
        "alphavantage-news;2026-11-05T17:00:00Z".to_string(),
    ];
    for f in &falsch {
        let r = Journal::oeffnen(&mit(&[(Art::Nachricht, f)])?);
        assert!(r.is_err(), "angenommen: {f}");
    }
    let zweimal = Journal::oeffnen(&mit(&[(Art::Nachricht, &gut), (Art::Nachricht, &gut)])?);
    assert!(zweimal.is_err(), "dieselbe Nachricht zweimal angenommen");
    Ok(())
}
