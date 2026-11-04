//! Belegte Kurse (R12, R20, Kap. 6): Ein Kurs aus einer lesenden Quelle steht vor seiner
//! Verwendung im Journal, mit Quelle, Abrufzeit und Abdruck der rohen Antwort. Danach gelten
//! fuer ihn dieselben Pruefungen wie fuer jeden Kurs. Erfundene Kurse in der Form einer Quelle
//! (D12).
//!
//! Jede Vorbereitung prueft ihren Erfolg ausdruecklich und endet sonst mit „Vorbedingung …“.

use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{Art, Journal, JournalFehler, KursBeleg, inhalt_fuer, vermerk_fuer};
use depot_kern::{
    Cent, Datum, Grenzen, Herkunft, Instrument, InstrumentArt, Isin, Kurs, Kurse, Preis, Universum,
    Zeitpunkt, lies_tageskurse, schlusskurse,
};
use depot_regelwerk::{DatenFehler, pruefe_kurse};
use sha2::{Digest, Sha256};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

fn neues_verzeichnis() -> Result<PathBuf, String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_kurse_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
    Ok(dir.join("journal.txt"))
}

fn eingerichtet() -> Result<PathBuf, String> {
    let p = neues_verzeichnis()?;
    vorbedingung(Journal::einrichten(&p), "Einrichtung")?;
    Ok(p)
}

/// Ein Journal mit genau diesen Eintraegen samt passendem Vermerk.
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

fn tag(j: u16, m: u8, t: u8) -> Result<Datum, String> {
    Datum::neu(j, m, t).ok_or_else(|| format!("Vorbedingung: Datum {j}-{m}-{t}"))
}

fn utc(m: u8, t: u8, h: u8, min: u8) -> Result<Zeitpunkt, String> {
    Zeitpunkt::neu(tag(2026, m, t)?, h, min, 0).ok_or_else(|| "Vorbedingung: Zeitpunkt".into())
}

fn isin(text: &str) -> Result<Isin, String> {
    vorbedingung(Isin::neu(text), "ISIN")
}

const A: &str = "DE0007164600";
const B: &str = "DE0007100000";

/// Rohe Antworten in der Form von Stooq. Erfundene Kurse.
const ROH_A: &str = "Date,Open,High,Low,Close,Volume\n\
2026-11-02,101.2,103.4,100.9,103,98000\n\
2026-11-03,103,104.1,102.5,103.75,101000\n";
const ROH_B: &str = "Date,Open,High,Low,Close,Volume\n\
2026-11-02,50,51,49.5,50.5,5000\n\
2026-11-03,50.5,52,50,51.8,6000\n";

/// Liest beide Antworten, bildet die Schlusskurse des Abrufs und die Belege dazu.
fn abruf(zeit: Zeitpunkt) -> Result<(Kurse, Kurse, Vec<KursBeleg>), String> {
    let mut werte = Vec::new();
    for (wert, roh) in [(A, ROH_A), (B, ROH_B)] {
        werte.push((
            isin(wert)?,
            vorbedingung(lies_tageskurse(roh), "Kursdatei")?,
        ));
    }
    let (heute, vortag) = vorbedingung(schlusskurse(zeit, werte), "Schlusskurse")?;
    let mut belege = Vec::new();
    for (wert, roh) in [(A, ROH_A), (B, ROH_B)] {
        let i = isin(wert)?;
        for kurse in [&vortag, &heute] {
            let k = kurse.eintrag(&i).ok_or("Vorbedingung: Kurs")?;
            belege.push(vorbedingung(
                KursBeleg::neu("stooq", zeit, roh.as_bytes(), i.clone(), k.clone()),
                "Beleg",
            )?);
        }
    }
    Ok((heute, vortag, belege))
}

/// Baut aus den Belegen eines Abrufs die Kurse wieder auf: je Wert der spaeteste Kurs als
/// `heute`, der fruehere als `vortag`.
fn aus_belegen(belege: &[KursBeleg], zeit: Zeitpunkt) -> Result<(Kurse, Kurse), String> {
    let mut heute: Vec<(Isin, Kurs)> = Vec::new();
    let mut vortag: Vec<(Isin, Kurs)> = Vec::new();
    for b in belege.iter().filter(|b| b.abruf() == zeit) {
        match heute.iter().position(|(i, _)| i == b.isin()) {
            Some(stelle) => {
                let frueher = heute.remove(stelle);
                vortag.push(frueher);
                heute.push((b.isin().clone(), b.kurs().clone()));
            }
            None => heute.push((b.isin().clone(), b.kurs().clone())),
        }
    }
    Ok((
        vorbedingung(Kurse::neu(zeit, heute), "Kurse heute")?,
        vorbedingung(Kurse::neu(zeit, vortag), "Kurse Vortag")?,
    ))
}

fn universum() -> Result<Universum, String> {
    Ok(Universum::neu(
        [A, B]
            .into_iter()
            .map(|w| {
                Ok(Instrument {
                    isin: isin(w)?,
                    name: w.to_string(),
                    art: InstrumentArt::Aktie,
                })
            })
            .collect::<Result<_, String>>()?,
    ))
}

/// Jeder Kurs steht mit Quelle, Herkunft, Abruf, Abdruck der rohen Antwort, Preis und
/// Zeitpunkt im Journal und kommt nach dem Neustart unveraendert zurueck.
#[test]
fn kurs_mit_herkunft_und_abrufzeit() -> Result<(), String> {
    let p = eingerichtet()?;
    let zeit = utc(11, 3, 16, 45)?;
    let (_, _, belege) = abruf(zeit)?;
    {
        let mut j = oeffnen(&p)?;
        for b in &belege {
            vorbedingung(j.belege_kurs(b), "belegen")?;
        }
    }
    let j = oeffnen(&p)?;
    assert_eq!(j.stand().kurse(), belege.as_slice());
    let erster = j
        .eintraege()
        .iter()
        .find(|(a, _)| *a == Art::Kurs)
        .ok_or("kein Eintrag")?;
    let abdruck = Sha256::digest(ROH_A.as_bytes())
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        });
    assert_eq!(
        erster.1,
        format!("stooq;quelle;2026-11-03T16:45:00Z;{abdruck};{A};10300;2026-11-02T16:35:00Z")
    );
    let b = belege.last().ok_or("kein Beleg")?;
    assert_eq!(b.quelle(), "stooq");
    assert_eq!(b.abruf(), zeit);
    assert_eq!(b.kurs().herkunft(), Herkunft::Quelle);
    assert_eq!(b.kurs().preis_cent(), Some(Cent(5_180)));
    assert_eq!(b.roh().len(), 64);
    assert_ne!(belege.first().map(KursBeleg::roh), Some(b.roh()));
    Ok(())
}

/// Belegte Kurse durchlaufen dieselben Pruefungen wie frische: Kurz nach dem Xetra-Schluss
/// bestehen sie, eine halbe Stunde spaeter sind sie zu alt, am naechsten Morgen nicht vom
/// Handelstag.
#[test]
fn belegte_kurse_bestehen_dieselben_pruefungen() -> Result<(), String> {
    let pfad = eingerichtet()?;
    let grenzen = Grenzen::standard();
    let werte = universum()?;
    let zeit = utc(11, 3, 16, 45)?;
    let (heute, vortag, belege) = abruf(zeit)?;
    {
        let mut j = oeffnen(&pfad)?;
        for b in &belege {
            vorbedingung(j.belege_kurs(b), "belegen")?;
        }
    }
    let j = oeffnen(&pfad)?;
    let (neu_h, neu_v) = aus_belegen(j.stand().kurse(), zeit)?;
    assert_eq!((&neu_h, &neu_v), (&heute, &vortag));
    assert_eq!(
        pruefe_kurse(&grenzen, &werte, tag(2026, 11, 3)?, &neu_h, &neu_v),
        vec![]
    );

    // Dieselben Kurse spaeter verwendet: Um 18:10 Uhr sind sie 35 Minuten alt, am naechsten
    // Morgen nicht vom Handelstag.
    let spaeter = |stand: Zeitpunkt| {
        Kurse::neu(
            stand,
            neu_h.alle().map(|(i, k)| (i.clone(), k.clone())).collect(),
        )
    };
    let abend = vorbedingung(spaeter(utc(11, 3, 17, 10)?), "Abend")?;
    let morgen = vorbedingung(spaeter(utc(11, 4, 8, 0)?), "Morgen")?;
    let mut erwartet_abend = Vec::new();
    let mut erwartet_morgen = Vec::new();
    for w in [B, A] {
        erwartet_abend.push(DatenFehler::KursZuAlt(isin(w)?));
        erwartet_morgen.push(DatenFehler::KursNichtAmHandelstag(isin(w)?));
    }
    assert_eq!(
        pruefe_kurse(&grenzen, &werte, tag(2026, 11, 3)?, &abend, &neu_v),
        erwartet_abend
    );
    assert_eq!(
        pruefe_kurse(&grenzen, &werte, tag(2026, 11, 4)?, &morgen, &neu_v),
        erwartet_morgen
    );
    Ok(())
}

/// Ein Beleg entsteht nur mit gueltigem Quellnamen und nur fuer einen Kurs, der beim Abruf
/// schon vorlag.
#[test]
fn beleg_nur_gueltig() -> Result<(), String> {
    let zeit = utc(11, 3, 16, 45)?;
    let (heute, _, _) = abruf(zeit)?;
    let a = isin(A)?;
    let k = heute.eintrag(&a).ok_or("Vorbedingung: Kurs")?.clone();
    for name in ["", "Stooq", "stooq.com", "st ooq", "stooq;x", "stööq"] {
        assert!(
            KursBeleg::neu(name, zeit, b"", a.clone(), k.clone()).is_err(),
            "{name}"
        );
    }
    assert!(KursBeleg::neu("alpha-vantage2", zeit, b"", a.clone(), k.clone()).is_ok());
    let zu_frueh = utc(11, 3, 16, 34)?;
    assert!(KursBeleg::neu("stooq", zu_frueh, b"", a, k).is_err());
    Ok(())
}

/// Die Abrufe im Journal fallen nie zurueck; ein abgelehnter Kurs aendert nichts.
#[test]
fn abruf_faellt_nie_zurueck() -> Result<(), String> {
    let p = eingerichtet()?;
    let spaet = utc(11, 3, 16, 50)?;
    let frueh = utc(11, 3, 16, 45)?;
    let (_, _, neu) = abruf(spaet)?;
    let (_, _, alt) = abruf(frueh)?;
    let mut j = oeffnen(&p)?;
    let erster = neu.first().ok_or("Vorbedingung: Beleg")?;
    vorbedingung(j.belege_kurs(erster), "belegen")?;
    let vorher = vorbedingung(fs::read(&p), "lesen")?;
    let r = j.belege_kurs(alt.first().ok_or("Vorbedingung: Beleg")?);
    assert!(matches!(r, Err(JournalFehler::Abgelehnt(_))), "{r:?}");
    assert_eq!(vorbedingung(fs::read(&p), "lesen")?, vorher);
    // Derselbe Abruf darf mehrere Kurse belegen.
    vorbedingung(j.belege_kurs(erster), "derselbe Abruf")?;
    Ok(())
}

/// Ein Kurseintrag in der Datei gilt nur in genau der Form, die das Journal selbst schreibt
/// (R20). Alles andere ist beim Oeffnen Halt.
#[test]
fn kurseintrag_in_der_datei_nur_kanonisch() -> Result<(), String> {
    let roh = "0".repeat(64);
    let gut = format!("stooq;quelle;2026-11-03T16:45:00Z;{roh};{A};10375;2026-11-03T16:35:00Z");
    vorbedingung(
        Journal::oeffnen(&mit(&[(Art::Kurs, &gut)])?),
        "gueltiger Eintrag",
    )?;
    let falsch = [
        gut.replace("10375", "010375"),
        gut.replace("10375", "0"),
        gut.replace("10375", "-5"),
        gut.replace("stooq", "Stooq"),
        gut.replace("stooq", ""),
        gut.replace(";quelle;", ";erfunden;"),
        gut.replace(&roh, &"0".repeat(63)),
        gut.replace(&roh, &"A".repeat(64)),
        gut.replace(A, "DE0007164601"),
        gut.replace("T16:35:00Z", "T16:46:00Z"),
        gut.replace("2026-11-03T16:45:00Z", "2026-11-03T16:45:00+01:00"),
        gut.replace("2026-11-03T16:35:00Z", "2026-11-03T16:35:00"),
        format!("{gut};x"),
        gut.replacen(';', ",", 1),
    ];
    for f in &falsch {
        let r = Journal::oeffnen(&mit(&[(Art::Kurs, f)])?);
        assert!(r.is_err(), "angenommen: {f}");
    }
    // Zwei Eintraege, der zweite mit frueherem Abruf.
    let frueher = gut.replace("16:45:00Z", "16:40:00Z");
    let r = Journal::oeffnen(&mit(&[(Art::Kurs, &gut), (Art::Kurs, &frueher)])?);
    assert!(r.is_err(), "zurueckfallender Abruf angenommen");
    Ok(())
}

/// Ein Kurs mit Bruchteil eines Cents steht exakt im Journal: Cent mit Nachkommastellen, ohne
/// Rundung. Ein Kurs in ganzen Cent steht wie bisher.
#[test]
fn kurs_mit_bruchteil_eines_cents_exakt() -> Result<(), String> {
    let roh = "0".repeat(64);
    let gut =
        format!("alphavantage;quelle;2026-11-03T16:45:00Z;{roh};{A};4992.5;2026-11-03T16:35:00Z");
    let j = vorbedingung(
        Journal::oeffnen(&mit(&[(Art::Kurs, &gut)])?),
        "Kurs mit halbem Cent",
    )?;
    let b = j.stand().kurse().first().ok_or("Beleg fehlt")?;
    assert_eq!(b.kurs().preis(), Preis::aus_fein(49_925_000));
    assert_eq!(b.kurs().preis_cent(), None);
    // Nur die eine Form: keine Null am Ende, kein Komma, hoechstens vier Nachkommastellen.
    for falsch in [
        "4992.50",
        "4992,5",
        "4992.",
        "04992.5",
        "4992.00001",
        "4992.0",
    ] {
        let f = gut.replace("4992.5", falsch);
        let r = Journal::oeffnen(&mit(&[(Art::Kurs, &f)])?);
        assert!(r.is_err(), "angenommen: {f}");
    }
    // Geschrieben wird genau diese Form, gelesen kommt derselbe Preis zurueck.
    let p = eingerichtet()?;
    let mut j = oeffnen(&p)?;
    let zeit = utc(11, 3, 16, 45)?;
    let kurs = vorbedingung(
        Kurs::exakt(
            Preis::aus_fein(41_155_000),
            "EUR",
            tag(2026, 11, 3)?,
            utc(11, 3, 16, 35)?,
            Herkunft::Quelle,
        ),
        "Kurs",
    )?;
    let beleg = vorbedingung(
        KursBeleg::neu("alphavantage", zeit, b"roh", isin(B)?, kurs.clone()),
        "Beleg",
    )?;
    vorbedingung(j.belege_kurs(&beleg), "belegen")?;
    drop(j);
    let text = vorbedingung(fs::read_to_string(&p), "Journal lesen")?;
    assert!(text.contains(&format!(";{B};4115.5;")), "{text}");
    let j = oeffnen(&p)?;
    assert_eq!(j.stand().kurse().last().map(KursBeleg::kurs), Some(&kurs));
    Ok(())
}
