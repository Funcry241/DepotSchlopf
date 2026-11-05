//! Der Sammler (Kap. 6, R12) mit einer erfundenen Quelle statt des Netzes. Die Antworten haben
//! die echte Form von Alpha Vantage (CSV, CRLF, vier Nachkommastellen, neuester Tag zuerst), die
//! Kurse sind erfunden (D12).
//!
//! Jede Vorbereitung prueft ihren Erfolg ausdruecklich und endet sonst mit „Vorbedingung …“.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use depot_journal::{Journal, JournalFehler};
use depot_kern::{Cent, Datum, Herkunft, Preis, Zeitpunkt};
use depot_regelwerk::DatenFehler;
use depot_sammler::liste::pruefe_liste;
use depot_sammler::sammeln::adresse;
use depot_sammler::{
    Ergebnis, Netz, QUELLE, SammelFehler, Schluessel, Wert, beobachtungsliste, sammle,
};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

/// Ein leerer Ordner mit eingerichtetem Journal.
fn ordner() -> Result<(PathBuf, PathBuf), String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_sammler_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
    let journal = dir.join("journal.txt");
    vorbedingung(Journal::einrichten(&journal), "Einrichtung")?;
    Ok((journal, dir.join("roh")))
}

fn tag(m: u8, t: u8) -> Result<Datum, String> {
    Datum::neu(2026, m, t).ok_or_else(|| "Vorbedingung: Datum".into())
}

fn utc(m: u8, t: u8, h: u8, min: u8) -> Result<Zeitpunkt, String> {
    Zeitpunkt::neu(tag(m, t)?, h, min, 0).ok_or_else(|| "Vorbedingung: Zeitpunkt".into())
}

/// Erfundener Schluessel in der Form der Quelle.
fn cent(c: i64) -> Result<Preis, String> {
    Preis::aus_cent(Cent(c)).ok_or_else(|| "Vorbedingung: Preis".into())
}

const SCHLUESSEL: &str = "ERFUNDEN0123TEST";

fn schluessel() -> Result<Schluessel, String> {
    vorbedingung(Schluessel::neu(SCHLUESSEL), "Schluessel")
}

fn zwei_werte() -> Result<Vec<Wert>, String> {
    vorbedingung(
        pruefe_liste(&[
            ("DE0007164600", "SAP.DEX", "SAP"),
            ("DE0007100000", "MBG.DEX", "Mercedes-Benz Group"),
        ]),
        "Liste",
    )
}

/// Erfundene Kurse in der echten Form von Alpha Vantage.
const ROH_SAP: &str = "timestamp,open,high,low,close,volume\r\n\
2026-11-04,210.0000,212.4500,209.1050,211.8000,1200000\r\n\
2026-11-03,208.5000,210.2000,207.9000,209.9500,1100000\r\n\
2026-11-02,207.0000,209.0000,206.5000,208.4000,1000000\r\n";
const ROH_MBG: &str = "timestamp,open,high,low,close,volume\r\n\
2026-11-04,60.1000,61.0000,59.8000,60.7500,900000\r\n\
2026-11-03,59.5000,60.4000,59.2000,60.0500,850000\r\n";
/// Mercedes-Benz mit dem echten Vortagsschluss auf einen halben Cent (41,155 €) aus dem ersten
/// Lauf am 04.11.2026. Die Zeile vom 03.11. ist erfunden (D12).
const ROH_MBG_HALB: &str = "timestamp,open,high,low,close,volume\r\n\
2026-11-03,41.3000,41.9000,41.0500,41.4800,3100000\r\n\
2026-11-02,40.6250,41.3650,40.2600,41.1550,2443353\r\n";
const NOTIZ: &str = "{\n    \"Note\": \"Thank you for using Alpha Vantage! Our standard API rate limit is 25 requests per day.\"\n}";
const FALSCHES_KUERZEL: &str = "{\n    \"Error Message\": \"Invalid API call. Please retry or visit the documentation for TIME_SERIES_DAILY.\"\n}";

/// Erfundene Quelle: Antwort je Kuerzel, merkt sich jede Adresse.
struct Erfunden {
    antworten: BTreeMap<String, Result<Vec<u8>, String>>,
    abgerufen: Vec<String>,
}

impl Erfunden {
    fn neu(antworten: &[(&str, Result<&str, &str>)]) -> Erfunden {
        Erfunden {
            antworten: antworten
                .iter()
                .map(|(k, a)| {
                    (
                        (*k).to_string(),
                        a.map(|t| t.as_bytes().to_vec()).map_err(str::to_string),
                    )
                })
                .collect(),
            abgerufen: Vec::new(),
        }
    }
}

impl Netz for Erfunden {
    fn hole(&mut self, adresse: &str) -> Result<Vec<u8>, String> {
        self.abgerufen.push(adresse.to_string());
        let kuerzel = adresse
            .split('&')
            .find_map(|t| t.strip_prefix("symbol="))
            .unwrap_or_default();
        self.antworten
            .get(kuerzel)
            .cloned()
            .unwrap_or_else(|| Err("unbekannt".into()))
    }
}

/// Eine Uhr, die nacheinander diese Zeitpunkte liefert.
fn uhr(zeiten: Vec<Zeitpunkt>) -> impl FnMut() -> Option<Zeitpunkt> {
    let mut i = zeiten.into_iter();
    move || i.next()
}

fn lauf(
    journal: &Path,
    roh: &Path,
    netz: &mut Erfunden,
    zeiten: Vec<Zeitpunkt>,
    werte: &[Wert],
) -> Result<Result<depot_sammler::Bericht, SammelFehler>, String> {
    let mut j = vorbedingung(Journal::oeffnen(journal), "Journal oeffnen")?;
    let mut u = uhr(zeiten);
    Ok(sammle(
        &mut j,
        netz,
        &schluessel()?,
        &mut u,
        roh,
        Duration::ZERO,
        werte,
    ))
}

fn sha256_hex(b: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;
    Sha256::digest(b).iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

/// Die festgelegte Liste ist gueltig und passt in das Tageskontingent der kostenlosen Stufe.
/// Fehlerhafte Listen werden abgelehnt.
#[test]
fn beobachtungsliste_gueltig() -> Result<(), String> {
    let l = beobachtungsliste()?;
    assert!(!l.is_empty() && l.len() <= 25, "{}", l.len());
    assert!(pruefe_liste(&[("DE0007164601", "SAP.DEX", "x")]).is_err());
    assert!(pruefe_liste(&[("DE0007164600", "SAP.DE", "x")]).is_err());
    assert!(pruefe_liste(&[("DE0007164600", "SAP&X.DEX", "x")]).is_err());
    assert!(pruefe_liste(&[("DE0007164600", "sap.DEX", "x")]).is_err());
    assert!(
        pruefe_liste(&[
            ("DE0007164600", "SAP.DEX", "x"),
            ("DE0007100000", "SAP.DEX", "y")
        ])
        .is_err()
    );
    assert!(
        pruefe_liste(&[
            ("DE0007164600", "SAP.DEX", "x"),
            ("DE0007164600", "MBG.DEX", "y")
        ])
        .is_err()
    );
    assert!(pruefe_liste(&[]).is_err());
    Ok(())
}

/// Der Schluessel ist kurz gefasst und hat keine Textform.
#[test]
fn schluessel_ohne_textform() -> Result<(), String> {
    let s = schluessel()?;
    assert_eq!(format!("{s:?}"), "Schluessel(***)");
    assert_eq!(
        s.bereinige(&format!("x apikey={SCHLUESSEL} y")),
        "x apikey=*** y"
    );
    assert_eq!(Schluessel::neu(&format!(" {SCHLUESSEL}\r\n")), Ok(s));
    for falsch in [
        "",
        "KURZ",
        "ERFUNDEN0123TEST&x=1",
        "erfunden0123test",
        "ERFUNDEN 0123TEST",
    ] {
        let e = Schluessel::neu(falsch);
        assert!(e.is_err(), "{falsch:?}");
        assert!(!format!("{e:?}").contains("ERFUNDEN"), "{e:?}");
    }
    Ok(())
}

/// Abgerufen wird nur die Tagesreihe als CSV bei Alpha Vantage, ueber HTTPS.
#[test]
fn abrufadresse() -> Result<(), String> {
    let werte = zwei_werte()?;
    let w = werte.first().ok_or("Vorbedingung: Wert")?;
    let a = adresse(w, &schluessel()?);
    assert!(a.starts_with("https://www.alphavantage.co/query?"), "{a}");
    for teil in [
        "function=TIME_SERIES_DAILY",
        "symbol=SAP.DEX",
        "datatype=csv",
        "outputsize=compact",
    ] {
        assert!(
            a.split(['?', '&']).any(|t| t == teil),
            "{teil} fehlt in der Adresse"
        );
    }
    Ok(())
}

/// Kurz nach dem Xetra-Schluss (17:35 Uhr MEZ = 16:35 Uhr UTC): Jede Antwort liegt roh in der
/// Ablage, jeder Kurs steht mit Quelle, Abrufzeit und Abdruck genau dieser Antwort im Journal,
/// und die Pruefung besteht.
#[test]
fn normaler_lauf_belegt_und_prueft() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let werte = zwei_werte()?;
    let mut netz = Erfunden::neu(&[("SAP.DEX", Ok(ROH_SAP)), ("MBG.DEX", Ok(ROH_MBG))]);
    let (a1, a2) = (utc(11, 4, 16, 40)?, utc(11, 4, 16, 41)?);
    let b = lauf(&journal, &roh, &mut netz, vec![a1, a2], &werte)?
        .map_err(|e| format!("Lauf: {e:?}"))?;
    assert_eq!(netz.abgerufen.len(), 2);
    assert_eq!(b.abbruch, None);
    assert_eq!(b.handelstag, Some(tag(11, 4)?));
    assert_eq!(b.pruefung, Vec::<DatenFehler>::new());
    assert!(b.verwendbar());
    let mut rohdateien = Vec::new();
    for ((_, e), (antwort, heute, vortag)) in b
        .werte
        .iter()
        .zip([(ROH_SAP, 21_180, 20_995), (ROH_MBG, 6_075, 6_005)])
    {
        let Ergebnis::Belegt {
            heute: h,
            vortag: v,
            rohdatei,
        } = e
        else {
            return Err(format!("nicht belegt: {e:?}"));
        };
        assert_eq!(*h, Some((tag(11, 4)?, cent(heute)?)));
        assert_eq!(*v, Some((tag(11, 3)?, cent(vortag)?)));
        assert!(
            rohdatei.starts_with(roh.join(QUELLE)),
            "{}",
            rohdatei.display()
        );
        assert_eq!(
            vorbedingung(fs::read(rohdatei), "Rohdatei")?,
            antwort.as_bytes()
        );
        rohdateien.push(rohdatei.clone());
    }
    // Nach einem Neustart: vier Belege, je Wert Vortag und heute, alle aus ihrer Antwort.
    let j = vorbedingung(Journal::oeffnen(&journal), "erneut oeffnen")?;
    let k = j.stand().kurse();
    assert_eq!(k.len(), 4);
    for (beleg, (antwort, abruf)) in
        k.iter()
            .zip([(ROH_SAP, a1), (ROH_SAP, a1), (ROH_MBG, a2), (ROH_MBG, a2)])
    {
        assert_eq!(beleg.quelle(), QUELLE);
        assert_eq!(beleg.abruf(), abruf);
        assert_eq!(beleg.roh(), sha256_hex(antwort.as_bytes()));
        assert_eq!(beleg.kurs().herkunft(), Herkunft::Quelle);
    }
    Ok(())
}

/// Am naechsten Morgen sind dieselben Schlusskurse nicht mehr frisch: Die Pruefung meldet das,
/// belegt wird trotzdem, was abgerufen wurde.
#[test]
fn am_naechsten_morgen_zu_alt() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let werte = zwei_werte()?;
    let mut netz = Erfunden::neu(&[("SAP.DEX", Ok(ROH_SAP)), ("MBG.DEX", Ok(ROH_MBG))]);
    let b = lauf(
        &journal,
        &roh,
        &mut netz,
        vec![utc(11, 5, 7, 0)?, utc(11, 5, 7, 1)?],
        &werte,
    )?
    .map_err(|e| format!("Lauf: {e:?}"))?;
    assert!(b.pruefung.contains(&DatenFehler::AbrufNichtAmHandelstag));
    // Zu alte Kurse sind belegt, aber nicht verwendbar: Der Lauf meldet keinen Erfolg (K19-02).
    assert!(!b.verwendbar());
    assert!(
        b.pruefung
            .iter()
            .any(|f| matches!(f, DatenFehler::KursZuAlt(_)))
    );
    let j = vorbedingung(Journal::oeffnen(&journal), "erneut oeffnen")?;
    assert_eq!(j.stand().kurse().len(), 4);
    Ok(())
}

/// Ein Hinweis der Quelle (etwa das erreichte Tageskontingent) beendet den Lauf: kein weiterer
/// Abruf, nichts belegt.
#[test]
fn hinweis_beendet_den_lauf() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let werte = zwei_werte()?;
    let mut netz = Erfunden::neu(&[("SAP.DEX", Ok(NOTIZ)), ("MBG.DEX", Ok(ROH_MBG))]);
    let b = lauf(
        &journal,
        &roh,
        &mut netz,
        vec![utc(11, 4, 16, 40)?, utc(11, 4, 16, 41)?],
        &werte,
    )?
    .map_err(|e| format!("Lauf: {e:?}"))?;
    assert_eq!(netz.abgerufen.len(), 1);
    assert!(
        b.abbruch
            .as_deref()
            .is_some_and(|a| a.contains("25 requests"))
    );
    assert!(matches!(
        b.werte.get(1),
        Some((_, Ergebnis::NichtAbgerufen))
    ));
    assert_eq!(b.handelstag, None);
    assert!(!b.verwendbar());
    let j = vorbedingung(Journal::oeffnen(&journal), "erneut oeffnen")?;
    assert!(j.stand().kurse().is_empty());
    Ok(())
}

/// Ein falsches Kuerzel betrifft nur seinen Wert; die anderen werden weiter abgerufen. Die
/// Pruefung nennt den fehlenden Kurs.
#[test]
fn falsches_kuerzel_nur_fuer_seinen_wert() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let werte = zwei_werte()?;
    let mut netz = Erfunden::neu(&[("SAP.DEX", Ok(FALSCHES_KUERZEL)), ("MBG.DEX", Ok(ROH_MBG))]);
    let b = lauf(
        &journal,
        &roh,
        &mut netz,
        vec![utc(11, 4, 16, 40)?, utc(11, 4, 16, 41)?],
        &werte,
    )?
    .map_err(|e| format!("Lauf: {e:?}"))?;
    assert_eq!(netz.abgerufen.len(), 2);
    assert_eq!(b.abbruch, None);
    assert!(
        matches!(b.werte.first(), Some((_, Ergebnis::Fehler(t))) if t.contains("Invalid API call"))
    );
    assert!(matches!(b.werte.get(1), Some((_, Ergebnis::Belegt { .. }))));
    let sap = werte
        .first()
        .map(|w| w.isin.clone())
        .ok_or("Vorbedingung")?;
    assert!(b.pruefung.contains(&DatenFehler::KursFehlt(sap)));
    assert!(!b.verwendbar());
    let j = vorbedingung(Journal::oeffnen(&journal), "erneut oeffnen")?;
    assert_eq!(j.stand().kurse().len(), 2);
    Ok(())
}

/// Eine unlesbare Antwort wird nicht belegt, liegt aber roh zur Durchsicht in der Ablage.
#[test]
fn unlesbare_antwort_nicht_belegt() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let werte = zwei_werte()?;
    let kaputt = "timestamp,open,high,low,close,volume\r\n2026-11-04,1,1,1,1.0050,1\r\n";
    let mut netz = Erfunden::neu(&[("SAP.DEX", Ok(kaputt)), ("MBG.DEX", Ok("<html>"))]);
    let b = lauf(
        &journal,
        &roh,
        &mut netz,
        vec![utc(11, 4, 16, 40)?, utc(11, 4, 16, 41)?],
        &werte,
    )?
    .map_err(|e| format!("Lauf: {e:?}"))?;
    assert!(
        b.werte
            .iter()
            .all(|(_, e)| matches!(e, Ergebnis::Fehler(t) if t.contains("unlesbar")))
    );
    let abgelegt = vorbedingung(fs::read_dir(roh.join(QUELLE)), "Ablage")?.count();
    assert_eq!(abgelegt, 2);
    let j = vorbedingung(Journal::oeffnen(&journal), "erneut oeffnen")?;
    assert!(j.stand().kurse().is_empty());
    Ok(())
}

/// Alle Dateien unter `ordner`, samt Inhalt; ein fehlender Ordner ist leer.
fn dateien(ordner: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut aus = Vec::new();
    if !ordner.exists() {
        return Ok(aus);
    }
    for d in vorbedingung(fs::read_dir(ordner), "Ordner")? {
        let p = vorbedingung(d, "Eintrag")?.path();
        if p.is_dir() {
            aus.extend(dateien(&p)?);
        } else {
            aus.push((
                p.to_string_lossy().into_owned(),
                vorbedingung(fs::read(&p), "Datei")?,
            ));
        }
    }
    Ok(aus)
}

/// Enthaelt der Text den Schluessel, gleich in welcher Schreibweise, auch hinter einem Escape?
fn mit_schluessel(b: &[u8]) -> bool {
    depot_kern::json::alle_texte(&String::from_utf8_lossy(b))
        .iter()
        .any(|t| t.to_ascii_uppercase().contains(SCHLUESSEL))
}

/// Der Schluessel als JSON-Escapes, Zeichen fuer Zeichen (`\u0045…`).
fn maskiert() -> String {
    SCHLUESSEL.chars().fold(String::new(), |mut m, c| {
        let _ = write!(m, "\\u{:04x}", u32::from(c));
        m
    })
}

/// Der Schluessel erscheint nirgends: nicht in einer Meldung, auch wenn der Abruf die Adresse
/// nennt oder die Quelle ihn zurueckgibt, nicht im Journal, nicht im Namen und nicht im Inhalt
/// einer abgelegten Datei, in keiner Schreibweise und auch nicht hinter JSON-Escapes (K19-01,
/// K28-01). Gibt die Quelle den Schluessel
/// zurueck, wird die Antwort nicht abgelegt, und der Lauf endet.
#[test]
fn schluessel_erscheint_nirgends() -> Result<(), String> {
    let s = schluessel()?;
    let werte = zwei_werte()?;
    let w = werte.first().ok_or("Vorbedingung: Wert")?;
    let a = adresse(w, &s);
    let netzfehler = format!(
        "Verbindung zu {a} fehlgeschlagen ({})",
        a.to_ascii_lowercase()
    );
    let gross = format!("{{\"Information\": \"key {SCHLUESSEL} unknown\"}}");
    let klein = format!(
        "{{\"Error Message\": \"key {} unknown\"}}",
        SCHLUESSEL.to_ascii_lowercase()
    );
    let in_csv = format!("{ROH_MBG}# {SCHLUESSEL}\r\n");
    // K28-01: hinter Unicode-Escapes verborgen.
    let escapes = format!("{{\"Information\": \"key {} unknown\"}}", maskiert());
    for (fall, antwort) in [
        ("gross", &gross),
        ("klein", &klein),
        ("in CSV", &in_csv),
        ("Escapes", &escapes),
    ] {
        let (journal, roh) = ordner()?;
        let mut netz = Erfunden::neu(&[("SAP.DEX", Err(&netzfehler)), ("MBG.DEX", Ok(antwort))]);
        let b = lauf(&journal, &roh, &mut netz, vec![utc(11, 4, 16, 40)?], &werte)?
            .map_err(|e| format!("{fall}: Lauf: {e:?}"))?;
        let bericht = format!("{b:?}");
        assert!(bericht.contains("***"), "{fall}: {bericht}");
        assert!(!mit_schluessel(bericht.as_bytes()), "{fall}: {bericht}");
        assert!(
            !mit_schluessel(&vorbedingung(fs::read(&journal), "Journal")?),
            "{fall}: Journal"
        );
        for (name, inhalt) in dateien(&roh)? {
            assert!(!mit_schluessel(name.as_bytes()), "{fall}: Name {name}");
            assert!(!mit_schluessel(&inhalt), "{fall}: Inhalt von {name}");
        }
        assert!(b.abbruch.is_some(), "{fall}: Lauf nicht beendet");
        assert!(!b.verwendbar(), "{fall}");
    }
    Ok(())
}

/// Geht die Uhr zurueck, nimmt das Journal den Beleg nicht an, und der Lauf haelt. Ohne Uhr
/// wird nichts abgelegt und nichts belegt.
#[test]
fn uhr_haelt_den_lauf() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let werte = zwei_werte()?;
    let mut netz = Erfunden::neu(&[("SAP.DEX", Ok(ROH_SAP)), ("MBG.DEX", Ok(ROH_MBG))]);
    let r = lauf(
        &journal,
        &roh,
        &mut netz,
        vec![utc(11, 4, 16, 41)?, utc(11, 4, 16, 40)?],
        &werte,
    )?;
    assert!(
        matches!(r, Err(SammelFehler::Journal(JournalFehler::Abgelehnt(_)))),
        "{r:?}"
    );
    let j = vorbedingung(Journal::oeffnen(&journal), "erneut oeffnen")?;
    assert_eq!(j.stand().kurse().len(), 2);
    drop(j);

    let (journal, roh) = ordner()?;
    let mut netz = Erfunden::neu(&[("SAP.DEX", Ok(ROH_SAP))]);
    let r = lauf(&journal, &roh, &mut netz, Vec::new(), &werte)?;
    assert_eq!(r, Err(SammelFehler::Uhr));
    assert!(!roh.exists());
    Ok(())
}

/// Ein Schluss auf einen Bruchteil eines Cents wird belegt und geprueft wie jeder Kurs: Der Lauf
/// ist verwendbar, und das Journal haelt den Preis exakt.
#[test]
fn halber_cent_wird_belegt() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let werte = zwei_werte()?;
    let sap = ROH_SAP.replacen(
        "2026-11-04,210.0000,212.4500,209.1050,211.8000,1200000\r\n",
        "",
        1,
    );
    let mut netz = Erfunden::neu(&[("SAP.DEX", Ok(&sap)), ("MBG.DEX", Ok(ROH_MBG_HALB))]);
    let (a1, a2) = (utc(11, 3, 16, 40)?, utc(11, 3, 16, 41)?);
    let b = lauf(&journal, &roh, &mut netz, vec![a1, a2], &werte)?
        .map_err(|e| format!("Lauf: {e:?}"))?;
    assert_eq!(b.pruefung, Vec::<DatenFehler>::new());
    assert!(b.verwendbar(), "{b:?}");
    let mbg = b.werte.get(1).map(|(_, e)| e.clone());
    let Some(Ergebnis::Belegt { heute, vortag, .. }) = mbg else {
        return Err(format!("nicht belegt: {mbg:?}"));
    };
    assert_eq!(heute, Some((tag(11, 3)?, Preis::aus_fein(41_480_000))));
    assert_eq!(vortag, Some((tag(11, 2)?, Preis::aus_fein(41_155_000))));
    let text = vorbedingung(fs::read_to_string(&journal), "Journal lesen")?;
    assert!(text.contains(";4115.5;"), "{text}");
    assert!(text.contains(";4148;"), "{text}");
    Ok(())
}
