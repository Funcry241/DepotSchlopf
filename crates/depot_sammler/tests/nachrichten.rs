//! RSS mit erfundenen Antworten: Quellenlimit, Wiederanlauf, Schluesselschutz und Journal.
use depot_journal::Journal;
use depot_kern::{Datum, Zeitpunkt};
use depot_sammler::nachrichten::MAX_NACHRICHTEN;
use depot_sammler::{Netz, NetzFehler, Schluessel, Zugang, sammle_nachrichten};
use std::fmt::Write as _;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};

static NR: AtomicU32 = AtomicU32::new(0);
const KEY: &str = "TESTSCHLUESSEL42";
fn um(h: u8, m: u8) -> Result<Zeitpunkt, String> {
    Datum::neu(2026, 11, 7)
        .and_then(|d| Zeitpunkt::neu(d, h, m, 0))
        .ok_or("Zeit fehlt".into())
}
fn ordner() -> Result<PathBuf, String> {
    let p = std::env::temp_dir().join(format!(
        "depot_rss_{}_{}",
        std::process::id(),
        NR.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Journal::einrichten(&p.join("journal.txt")).map_err(|e| format!("Vorbedingung: {e:?}"))?;
    Ok(p)
}
fn item(titel: &str, zeit: &str, quelle: &str) -> String {
    let host = if quelle == "tagesschau" {
        "www.tagesschau.de"
    } else {
        "www.bbc.com"
    };
    format!(
        "<item><title>{titel}</title><link>https://{host}/artikel</link><pubDate>{zeit}</pubDate><description>Nur Titel speichern</description></item>"
    )
}
fn feed(items: &str) -> String {
    format!("<rss version=\"2.0\"><channel>{items}</channel></rss>")
}
fn normal(quelle: &str) -> String {
    feed(&item(
        "Politik &amp; Wirtschaft",
        "Sat, 07 Nov 2026 12:00:00 GMT",
        quelle,
    ))
}
struct Fake {
    antworten: Vec<Result<Vec<u8>, NetzFehler>>,
    adressen: Vec<String>,
}
impl Netz for Fake {
    fn hole(&mut self, adresse: &str) -> Result<Vec<u8>, NetzFehler> {
        self.adressen.push(adresse.into());
        if self.antworten.is_empty() {
            return Err(NetzFehler::endgueltig("Unerwarteter Abruf"));
        }
        self.antworten.remove(0)
    }
}
fn netz(a: &str, b: &str) -> Fake {
    Fake {
        antworten: vec![Ok(a.as_bytes().to_vec()), Ok(b.as_bytes().to_vec())],
        adressen: Vec::new(),
    }
}
fn zugang() -> Result<Zugang, String> {
    Schluessel::neu(KEY).map(Zugang::alpha_vantage)
}
fn oeffnen(p: &std::path::Path) -> Result<Journal, String> {
    Journal::oeffnen(&p.join("journal.txt")).map_err(|e| format!("Vorbedingung: {e:?}"))
}

#[test]
fn kostenlos_ohne_schluessel_mit_quelle_link_zeit_und_rohhash() -> Result<(), String> {
    let p = ordner()?;
    let mut j = oeffnen(&p)?;
    let mut n = netz(&normal("tagesschau"), &normal("bbc-world"));
    let b = sammle_nachrichten(
        &mut j,
        &mut n,
        &Zugang::default(),
        &mut || um(14, 0).ok(),
        &p.join("roh"),
    )
    .map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.neu, 2);
    assert_eq!(n.adressen.len(), 2);
    assert_eq!(
        j.stand().seit_abschluss(),
        0,
        "RSS-Abrufe beginnen keinen Papiertag"
    );
    assert!(j.stand().rss_abruf("tagesschau").is_some());
    assert!(j.stand().rss_abruf("bbc-world").is_some());
    assert!(
        n.adressen
            .iter()
            .all(|a| !a.contains("apikey") && !a.contains("alphavantage"))
    );
    for x in j.stand().nachrichten() {
        assert!(x.text().contains("Politik & Wirtschaft"));
        assert!(x.text().contains("https://"));
        assert!(!x.text().contains("Nur Titel speichern"));
        assert_eq!(x.zeit(), um(12, 0)?);
        assert_eq!(x.abruf(), um(14, 0)?);
        assert_eq!(x.roh().len(), 64);
    }
    Ok(())
}

#[test]
fn abrufpause_ueberlebt_neustart_und_uhrsprung_nach_hinten() -> Result<(), String> {
    let p = ordner()?;
    for (h, m, erwartet) in [(14, 0, 2), (14, 1, 0), (13, 0, 0), (15, 29, 0), (15, 30, 2)] {
        let mut j = oeffnen(&p)?;
        let mut n = netz(&normal("tagesschau"), &normal("bbc-world"));
        let b = sammle_nachrichten(
            &mut j,
            &mut n,
            &zugang()?,
            &mut || um(h, m).ok(),
            &p.join("roh"),
        )
        .map_err(|e| format!("{e:?}"))?;
        assert_eq!(n.adressen.len(), erwartet);
        if h == 15 && m == 30 {
            assert_eq!(b.bekannt, 2);
            assert_eq!(b.neu, 0);
        }
    }
    Ok(())
}

#[test]
fn sicherheitsabbruch_auch_im_zweiten_feed_und_hinter_xml_entities() -> Result<(), String> {
    let kodiert = KEY.chars().fold(String::new(), |mut s, c| {
        let _ = write!(s, "&#{};", u32::from(c));
        s
    });
    for text in [
        KEY.to_string(),
        kodiert.clone(),
        kodiert.replace('&', "&amp;"),
    ] {
        for mehrdeutig in [false, true] {
            let p = ordner()?;
            let mut j = oeffnen(&p)?;
            // Auch ein ignoriertes Feld wird dekodiert geprueft, nicht nur die Schlagzeile.
            let mut boese = normal("bbc-world").replace("Nur Titel speichern", &text);
            if mehrdeutig {
                boese = boese.replace("</title>", "</title><title>Mehrdeutiger Titel</title>");
            }
            let mut n = netz(&normal("tagesschau"), &boese);
            let b = sammle_nachrichten(
                &mut j,
                &mut n,
                &zugang()?,
                &mut || um(14, 0).ok(),
                &p.join("roh"),
            )
            .map_err(|e| format!("{e:?}"))?;
            assert!(b.sicherheitsabbruch);
            assert!(j.stand().nachrichten().is_empty());
            assert!(!p.join("roh").exists());
            assert!(!format!("{b:?}").contains(KEY));
        }
    }
    Ok(())
}

#[test]
fn eine_quelle_faellt_aus_und_fehler_werden_maskiert() -> Result<(), String> {
    let p = ordner()?;
    let mut j = oeffnen(&p)?;
    let mut n = netz("", &normal("bbc-world"));
    if let Some(a) = n.antworten.first_mut() {
        *a = Err(NetzFehler::endgueltig(format!("apikey={KEY}")));
    }
    let b = sammle_nachrichten(
        &mut j,
        &mut n,
        &zugang()?,
        &mut || um(14, 0).ok(),
        &p.join("roh"),
    )
    .map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.neu, 1);
    assert!(b.fehler.is_none());
    assert!(!format!("{b:?}").contains(KEY));
    assert!(b.quellen.iter().any(|s| s.starts_with("tagesschau:")));
    Ok(())
}

#[test]
fn beide_quellen_kommen_unter_dem_gemeinsamen_limit_vor() -> Result<(), String> {
    let p = ordner()?;
    let mut j = oeffnen(&p)?;
    let items = |q: &str| {
        (0..30)
            .map(|i| item(&format!("Titel {i}"), "Sat, 07 Nov 2026 12:00:00 GMT", q))
            .collect::<String>()
    };
    let mut n = netz(&feed(&items("tagesschau")), &feed(&items("bbc-world")));
    let b = sammle_nachrichten(
        &mut j,
        &mut n,
        &zugang()?,
        &mut || um(14, 0).ok(),
        &p.join("roh"),
    )
    .map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.neu, MAX_NACHRICHTEN);
    for q in ["tagesschau", "bbc-world"] {
        assert!(j.stand().nachrichten().iter().any(|n| n.quelle() == q));
    }
    Ok(())
}

#[test]
fn kaputter_feed_wird_als_ganzes_verworfen() -> Result<(), String> {
    let p = ordner()?;
    let mut j = oeffnen(&p)?;
    let mut n = netz(&normal("tagesschau").replace("</rss>", ""), "kein XML");
    let b = sammle_nachrichten(
        &mut j,
        &mut n,
        &zugang()?,
        &mut || um(14, 0).ok(),
        &p.join("roh"),
    )
    .map_err(|e| format!("{e:?}"))?;
    assert!(b.fehler.is_some());
    assert_eq!(b.neu, 0);
    assert!(!p.join("roh").exists());
    Ok(())
}

#[test]
fn verschiedene_abrufzeiten_fallen_im_journal_nicht_zurueck() -> Result<(), String> {
    let p = ordner()?;
    let mut j = oeffnen(&p)?;
    let mut n = netz(
        &normal("tagesschau"),
        &normal("bbc-world").replace("12:00:00", "11:00:00"),
    );
    let mut zeiten = [um(14, 0)?, um(14, 1)?, um(14, 2)?, um(14, 3)?].into_iter();
    let b = sammle_nachrichten(
        &mut j,
        &mut n,
        &zugang()?,
        &mut || zeiten.next(),
        &p.join("roh"),
    )
    .map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.neu, 2);
    let abrufe: Vec<_> = j
        .stand()
        .nachrichten()
        .iter()
        .map(depot_journal::NachrichtBeleg::abruf)
        .collect();
    assert_eq!(abrufe, vec![um(14, 1)?, um(14, 3)?]);
    drop(j);
    assert_eq!(oeffnen(&p)?.stand().nachrichten().len(), 2);
    Ok(())
}

#[test]
fn ferner_zukunftsbeleg_erklaert_die_pause_ohne_netzabruf() -> Result<(), String> {
    let p = ordner()?;
    let mut j = oeffnen(&p)?;
    let heute = um(14, 0)?;
    let zukunft =
        Zeitpunkt::aus_sekunden(heute.sekunden().saturating_add(172_800)).ok_or("Zeit")?;
    for q in ["tagesschau", "bbc-world"] {
        j.belege_rss_abruf(q, zukunft)
            .map_err(|e| format!("{e:?}"))?;
    }
    let vorher = j.bestaetigt();
    let mut n = netz("", "");
    let b = sammle_nachrichten(
        &mut j,
        &mut n,
        &Zugang::default(),
        &mut || Some(heute),
        &p.join("roh"),
    )
    .map_err(|e| format!("{e:?}"))?;
    assert!(n.adressen.is_empty());
    assert_eq!(b.quellen.len(), 2);
    assert!(
        b.quellen
            .iter()
            .all(|t| t.contains("Uhr oder Journal prüfen"))
    );
    assert_eq!(j.bestaetigt(), vorher);
    Ok(())
}
