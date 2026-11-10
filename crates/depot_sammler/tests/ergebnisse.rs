//! Quartalszahlen der US-Werte aus dem Ergebniskalender von Alpha Vantage (#120). Die Vorlage hat
//! die echte Kopfzeile und echte Zeilen vom 10.11.2026 (Demo-Schluessel), dazu eine Zeile mit
//! Komma im Namen, wie sie in der echten Antwort vorkommt.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::Journal;
use depot_kern::{Datum, Zeitpunkt};
use depot_sammler::ergebnisse::{
    KalenderBericht, KalenderFehler, QUELLE_ERGEBNISSE, lies_kalender, sammle_ergebnisse,
};
use depot_sammler::{
    Anbieter, Netz, NetzFehler, SammelFehler, Schluessel, Zugang, beobachtungsliste,
};

const KALENDER: &str = r#"symbol,name,reportDate,fiscalDateEnding,estimate,currency,timeOfTheDay
ACCS,ACCESS NEWSWIRE INCORPORATED,2026-11-10,2026-09-30,0.09,USD,pre-market
JPM,JPMORGAN CHASE & COMPANY,2027-01-14,2026-12-31,,USD,
KO,COCA-COLA CO,2027-02-10,2026-12-31,0.55,USD,pre-market
NVDA,NVIDIA CORPORATION,2026-11-17,2026-10-31,2.49,USD,post-market
SGU,"STAR GROUP, L.P.",2026-12-14,2026-09-30,,USD,
"#;

const SCHLUESSEL: &str = "ABCDEFGH1234";

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

fn ordner() -> Result<(PathBuf, PathBuf), String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_ergebnisse_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
    let journal = dir.join("journal.txt");
    vorbedingung(Journal::einrichten(&journal), "Einrichtung")?;
    Ok((journal, dir.join("roh")))
}

fn utc(t: u8, h: u8) -> Result<Zeitpunkt, String> {
    Datum::neu(2026, 11, t)
        .and_then(|d| Zeitpunkt::neu(d, h, 0, 0))
        .ok_or_else(|| "Vorbedingung: Zeitpunkt".into())
}

fn zugang() -> Result<Zugang, String> {
    Ok(Zugang::alpha_vantage(vorbedingung(
        Schluessel::fuer(Anbieter::AlphaVantage, SCHLUESSEL),
        "Schlüssel",
    )?))
}

struct Erfunden {
    antwort: Result<Vec<u8>, String>,
    abgerufen: Vec<String>,
}

impl Netz for Erfunden {
    fn hole(&mut self, adresse: &str) -> Result<Vec<u8>, NetzFehler> {
        self.abgerufen.push(adresse.to_string());
        self.antwort.clone().map_err(NetzFehler::endgueltig)
    }
}

/// Ein Lauf; gibt Bericht und Zahl der Abrufe zurück.
fn lauf(
    journal: &Path,
    roh: &Path,
    antwort: &str,
    um: Zeitpunkt,
    zugang: &Zugang,
) -> Result<(Result<KalenderBericht, SammelFehler>, usize), String> {
    let mut j = vorbedingung(Journal::oeffnen(journal), "Journal öffnen")?;
    let mut netz = Erfunden {
        antwort: Ok(antwort.as_bytes().to_vec()),
        abgerufen: Vec::new(),
    };
    let mut zeiten = vec![um, um].into_iter();
    let mut uhr = move || zeiten.next();
    let werte = beobachtungsliste()?;
    let b = sammle_ergebnisse(&mut j, &mut netz, &mut uhr, roh, zugang, &werte);
    for a in &netz.abgerufen {
        assert!(a.starts_with(
            "https://www.alphavantage.co/query?function=EARNINGS_CALENDAR&horizon=3month&apikey="
        ));
    }
    Ok((b, netz.abgerufen.len()))
}

#[test]
fn nur_werte_der_liste_werden_belegt() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let (b, abrufe) = lauf(&journal, &roh, KALENDER, utc(10, 22)?, &zugang()?)?;
    let b = b.map_err(|e| format!("{e:?}"))?;
    assert_eq!(abrufe, 1);
    assert_eq!(b.fehler, None);
    assert_eq!((b.neu, b.geaendert, b.unveraendert), (2, 0, 0));
    let j = vorbedingung(Journal::oeffnen(&journal), "Journal öffnen")?;
    let mut k: Vec<String> = j
        .stand()
        .termine()
        .iter()
        .filter(|t| t.quelle() == QUELLE_ERGEBNISSE)
        .map(|t| t.termin().kennung().to_string())
        .collect();
    k.sort();
    assert_eq!(k, vec!["ergebnis-jpm-2026-12-31", "ergebnis-ko-2026-12-31"]);
    let datei = b.rohdatei.ok_or("keine Rohdatei")?;
    assert_eq!(
        datei.file_name().and_then(|n| n.to_str()),
        Some("20261110T220000Z.csv")
    );
    Ok(())
}

#[test]
fn verschoben_wird_fortgeschrieben_und_einmal_am_tag() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    lauf(&journal, &roh, KALENDER, utc(10, 22)?, &zugang()?)?
        .0
        .map_err(|e| format!("{e:?}"))?;
    // Am selben Tag kein zweiter Abruf.
    let (b, abrufe) = lauf(&journal, &roh, KALENDER, utc(10, 22)?, &zugang()?)?;
    assert_eq!(abrufe, 0);
    assert_eq!(
        b.map_err(|e| format!("{e:?}"))?.uebersprungen.as_deref(),
        Some("heute schon abgerufen")
    );
    // Am nächsten Tag: JPM einen Tag später, KO fehlt.
    let neu = KALENDER.replace("2027-01-14", "2027-01-15").replace(
        "KO,COCA-COLA CO,2027-02-10,2026-12-31,0.55,USD,pre-market\n",
        "",
    );
    let (b, abrufe) = lauf(&journal, &roh, &neu, utc(11, 22)?, &zugang()?)?;
    let b = b.map_err(|e| format!("{e:?}"))?;
    assert_eq!(abrufe, 1);
    assert_eq!((b.neu, b.geaendert, b.unveraendert), (0, 1, 0));
    assert_eq!(b.fehlen, vec!["ergebnis-ko-2026-12-31".to_string()]);
    Ok(())
}

#[test]
fn ohne_schluessel_kein_abruf() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let (b, abrufe) = lauf(&journal, &roh, KALENDER, utc(10, 22)?, &Zugang::leer())?;
    assert_eq!(abrufe, 0);
    assert!(b.map_err(|e| format!("{e:?}"))?.uebersprungen.is_some());
    Ok(())
}

#[test]
fn hinweis_der_quelle_belegt_nichts_ohne_sperre() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let (b, _) = lauf(
        &journal,
        &roh,
        &format!(r#"{{"Information": "rate limit reached {SCHLUESSEL}"}}"#),
        utc(10, 22)?,
        &zugang()?,
    )?;
    let b = b.map_err(|e| format!("{e:?}"))?;
    assert!(!b.sicherheitsabbruch);
    assert!(b.rohdatei.is_none());
    assert!(!b.fehler.as_deref().unwrap_or_default().contains(SCHLUESSEL));
    assert!(b.fehler.is_some_and(|f| f.contains("Hinweis der Quelle")));
    assert_eq!(b.neu, 0);
    Ok(())
}

#[test]
fn schluessel_in_der_antwort_ist_sicherheitsabbruch() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let antwort = format!("{KALENDER}{SCHLUESSEL}\n");
    let (b, _) = lauf(&journal, &roh, &antwort, utc(10, 22)?, &zugang()?)?;
    let b = b.map_err(|e| format!("{e:?}"))?;
    assert!(b.sicherheitsabbruch);
    assert_eq!(b.rohdatei, None);
    let (zweiter, abrufe) = lauf(&journal, &roh, KALENDER, utc(10, 22)?, &zugang()?)?;
    assert_eq!(abrufe, 0);
    assert!(
        zweiter
            .map_err(|e| format!("{e:?}"))?
            .uebersprungen
            .is_some()
    );
    assert!(b.fehler.is_some_and(|f| !f.contains(SCHLUESSEL)));
    Ok(())
}

#[test]
fn falsche_zeile_verwirft_alles() -> Result<(), String> {
    let werte = beobachtungsliste()?;
    let kaputt = KALENDER.replace("pre-market\nJPM", "during-market\nJPM");
    assert_eq!(
        lies_kalender(&kaputt, &werte),
        Err(KalenderFehler::Zeile(2))
    );
    let (journal, roh) = ordner()?;
    let (b, _) = lauf(&journal, &roh, &kaputt, utc(10, 22)?, &zugang()?)?;
    let b = b.map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.neu, 0);
    assert!(b.fehler.is_some_and(|f| f.contains("Kalender unlesbar")));
    Ok(())
}

#[test]
fn netzfehler_verbraucht_den_tagesversuch_bis_frankfurter_mitternacht() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let mut j = vorbedingung(Journal::oeffnen(&journal), "Journal öffnen")?;
    let mut netz = Erfunden {
        antwort: Err("Netz ausgefallen".into()),
        abgerufen: Vec::new(),
    };
    let um = utc(10, 22)?;
    let bericht = vorbedingung(
        sammle_ergebnisse(
            &mut j,
            &mut netz,
            &mut || Some(um),
            &roh,
            &zugang()?,
            &beobachtungsliste()?,
        ),
        "Abruf",
    )?;
    assert!(bericht.fehler.is_some());
    assert!(bericht.rohdatei.is_none());
    drop(j);
    assert_eq!(lauf(&journal, &roh, KALENDER, um, &zugang()?)?.1, 0);
    assert_eq!(
        lauf(&journal, &roh, KALENDER, utc(10, 23)?, &zugang()?)?.1,
        1
    );
    Ok(())
}

#[test]
fn doppeltes_quartal_verwirft_den_kalender() -> Result<(), String> {
    let doppelt = format!("{KALENDER}JPM,JPMORGAN,2027-01-15,2026-12-31,,USD,\n");
    assert_eq!(
        lies_kalender(&doppelt, &beobachtungsliste()?),
        Err(KalenderFehler::Zeile(7))
    );
    Ok(())
}
