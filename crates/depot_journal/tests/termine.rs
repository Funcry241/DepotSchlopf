//! Belegte Termine (R39, R12): Jeder Termin von aussen steht vor seiner Verwendung im Journal,
//! mit Quelle, Herkunft, Abrufzeit und Abdruck der rohen Antwort. Ein geaenderter Termin wird
//! fortgeschrieben, die alte Fassung bleibt. Alle Termine hier sind erfunden (D12).
//!
//! Jede Vorbereitung prueft ihren Erfolg ausdruecklich und endet sonst mit „Vorbedingung …“.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{Art, Fortschreibung, Journal, TerminBeleg, inhalt_fuer, vermerk_fuer};
use depot_kern::{Betroffen, Datum, Herkunft, Termin, Verlaesslichkeit, Zeitbezug, Zeitpunkt};
use sha2::{Digest, Sha256};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

fn neues_verzeichnis() -> Result<PathBuf, String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_termine_{}_{n}", std::process::id()));
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

fn tag(m: u8, t: u8) -> Result<Datum, String> {
    Datum::neu(2026, m, t).ok_or_else(|| format!("Vorbedingung: Datum {m}-{t}"))
}

fn utc(m: u8, t: u8, h: u8, min: u8) -> Result<Zeitpunkt, String> {
    Zeitpunkt::neu(tag(m, t)?, h, min, 0).ok_or_else(|| "Vorbedingung: Zeitpunkt".into())
}

/// Rohe Antwort einer erfundenen Quelle; nur ihr Abdruck zaehlt.
const ROH: &[u8] = b"erfundene Terminseite";

fn termin(kennung: &str, zeit: Zeitbezug) -> Result<Termin, String> {
    vorbedingung(
        Termin::neu(
            kennung,
            "EZB: geldpolitische Sitzung",
            zeit,
            vec![Betroffen::groesse("eur-leitzins").ok_or("Vorbedingung: Groesse")?],
            Verlaesslichkeit::Bestaetigt,
        ),
        "Termin",
    )
}

fn beleg(t: Termin, abruf: Zeitpunkt) -> Result<TerminBeleg, String> {
    vorbedingung(
        TerminBeleg::neu("ezb", Herkunft::Quelle, abruf, ROH, t),
        "Beleg",
    )
}

fn sha256_hex(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .fold(String::new(), |s, x| format!("{s}{x:02x}"))
}

/// Ein Termin steht mit Quelle, Herkunft, Abruf und Abdruck im Journal und kommt nach dem
/// Neustart unveraendert zurueck.
#[test]
fn termin_mit_quelle_und_abrufzeit() -> Result<(), String> {
    let p = eingerichtet()?;
    let mut j = oeffnen(&p)?;
    let t = termin("ezb-2026-12-17", Zeitbezug::Tag(tag(12, 17)?))?;
    let b = beleg(t.clone(), utc(11, 5, 17, 0)?)?;
    assert_eq!(b.roh(), sha256_hex(ROH));
    assert_eq!(j.belege_termin(&b), Ok(Fortschreibung::Neu));
    drop(j);
    let text = vorbedingung(fs::read_to_string(&p), "Journal lesen")?;
    assert!(
        text.contains(&format!(
            "termin;ezb;quelle;2026-11-05T17:00:00Z;{};ezb-2026-12-17;bestaetigt;2026-12-17;eur-leitzins;EZB: geldpolitische Sitzung",
            sha256_hex(ROH)
        )),
        "{text}"
    );
    let j = oeffnen(&p)?;
    assert_eq!(j.stand().termine(), std::slice::from_ref(&b));
    let aktuell = j.stand().aktuelle_termine();
    assert_eq!(aktuell.get("ezb-2026-12-17"), Some(&&b));
    assert_eq!(b.termin(), &t);
    Ok(())
}

/// Ein verschobener Termin wird fortgeschrieben, die alte Fassung bleibt. Ein unveraenderter
/// Termin wird nicht noch einmal geschrieben.
#[test]
fn verschiebung_wird_fortgeschrieben() -> Result<(), String> {
    let p = eingerichtet()?;
    let mut j = oeffnen(&p)?;
    let alt = beleg(
        termin("ezb-2026-12-17", Zeitbezug::Tag(tag(12, 17)?))?,
        utc(11, 5, 17, 0)?,
    )?;
    let anderer = beleg(
        termin("ezb-2027-02-04", Zeitbezug::Tag(tag(12, 30)?))?,
        utc(11, 5, 17, 0)?,
    )?;
    assert_eq!(j.belege_termin(&alt), Ok(Fortschreibung::Neu));
    assert_eq!(j.belege_termin(&anderer), Ok(Fortschreibung::Neu));
    let gleich = beleg(
        termin("ezb-2026-12-17", Zeitbezug::Tag(tag(12, 17)?))?,
        utc(11, 6, 17, 0)?,
    )?;
    let laenge = vorbedingung(fs::metadata(&p), "Laenge")?.len();
    assert_eq!(j.belege_termin(&gleich), Ok(Fortschreibung::Unveraendert));
    assert_eq!(vorbedingung(fs::metadata(&p), "Laenge")?.len(), laenge);
    let zeit = Zeitbezug::Zeitpunkt(utc(12, 18, 13, 15)?);
    let neu = beleg(termin("ezb-2026-12-17", zeit)?, utc(11, 7, 17, 0)?)?;
    assert_eq!(j.belege_termin(&neu), Ok(Fortschreibung::Geaendert));
    drop(j);
    let j = oeffnen(&p)?;
    assert_eq!(j.stand().termine(), [alt, anderer.clone(), neu.clone()]);
    let aktuell = j.stand().aktuelle_termine();
    assert_eq!(aktuell.len(), 2);
    assert_eq!(aktuell.get("ezb-2026-12-17"), Some(&&neu));
    assert_eq!(aktuell.get("ezb-2027-02-04"), Some(&&anderer));
    Ok(())
}

/// Die Abrufe fallen nie zurueck, auch nicht gegenueber einem Kurs.
#[test]
fn abruf_faellt_nie_zurueck() -> Result<(), String> {
    let p = eingerichtet()?;
    let mut j = oeffnen(&p)?;
    let spaet = beleg(
        termin("ezb-a", Zeitbezug::Tag(tag(12, 17)?))?,
        utc(11, 5, 17, 0)?,
    )?;
    let frueh = beleg(
        termin("ezb-b", Zeitbezug::Tag(tag(12, 17)?))?,
        utc(11, 5, 16, 59)?,
    )?;
    assert_eq!(j.belege_termin(&spaet), Ok(Fortschreibung::Neu));
    let laenge = vorbedingung(fs::metadata(&p), "Laenge")?.len();
    assert!(j.belege_termin(&frueh).is_err(), "zurueckfallender Abruf");
    assert_eq!(vorbedingung(fs::metadata(&p), "Laenge")?.len(), laenge);
    Ok(())
}

/// Der Name der Quelle ist Kleinbuchstaben, Ziffern und `-`.
#[test]
fn beleg_nur_gueltig() -> Result<(), String> {
    let t = termin("ezb-a", Zeitbezug::Tag(tag(12, 17)?))?;
    let abruf = utc(11, 5, 17, 0)?;
    for q in ["", "EZB", "ezb;x", "ezb quelle"] {
        assert!(
            TerminBeleg::neu(q, Herkunft::Quelle, abruf, ROH, t.clone()).is_err(),
            "{q:?}"
        );
    }
    Ok(())
}

/// Ein Termineintrag in der Datei gilt nur in genau der Form, die das Journal selbst schreibt.
#[test]
fn termineintrag_in_der_datei_nur_kanonisch() -> Result<(), String> {
    let roh = "0".repeat(64);
    let gut = format!(
        "ezb;quelle;2026-11-05T17:00:00Z;{roh};ezb-2026-12-17;bestaetigt;2026-12-17;DE0007164600,eur-leitzins;EZB: Sitzung"
    );
    vorbedingung(
        Journal::oeffnen(&mit(&[(Art::Termin, &gut)])?),
        "gueltiger Eintrag",
    )?;
    let falsch = [
        gut.replace(";ezb;", ";EZB;").replacen("ezb;", "EZB;", 1),
        gut.replace(";quelle;", ";erfunden;"),
        gut.replace(&roh, &"0".repeat(63)),
        gut.replace("ezb-2026-12-17", "EZB-2026"),
        gut.replace(";bestaetigt;", ";sicher;"),
        gut.replace(";2026-12-17;", ";17.12.2026;"),
        gut.replace("DE0007164600,eur-leitzins", "eur-leitzins,DE0007164600"),
        gut.replace("DE0007164600,eur-leitzins", "eur-leitzins,eur-leitzins"),
        gut.replace("DE0007164600,eur-leitzins", ""),
        gut.replace("DE0007164600", "DE0007164601"),
        gut.replace("EZB: Sitzung", ""),
        gut.replace("2026-11-05T17:00:00Z", "2026-11-05T17:00:00"),
        gut.replacen(';', ",", 1),
        format!("{gut};x"),
    ];
    for f in &falsch {
        let r = Journal::oeffnen(&mit(&[(Art::Termin, f)])?);
        assert!(r.is_err(), "angenommen: {f}");
    }
    let frueher = gut.replace("17:00:00Z", "16:00:00Z");
    let r = Journal::oeffnen(&mit(&[(Art::Termin, &gut), (Art::Termin, &frueher)])?);
    assert!(r.is_err(), "zurueckfallender Abruf angenommen");
    Ok(())
}
