//! Nachrichten ueber den Sammler (R40, Wolfgang 05.11.2026). Die Antwort hat die Form der echten
//! Antwort von `NEWS_SENTIMENT` (abgerufen am 05.11.2026), mit erfundenen Nachrichten. Der Lauf
//! bekommt sie von einer erfundenen Quelle statt aus dem Netz. Jede Vorbereitung prueft ihren
//! Erfolg ausdruecklich und endet sonst mit „Vorbedingung …“.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::Journal;
use depot_kern::json::zitiere;
use depot_kern::{Datum, Zeitpunkt};
use depot_sammler::nachrichten::{
    MAX_NACHRICHTEN, MAX_ZEICHEN, QUELLE_NACHRICHTEN, lies_nachrichten,
};
use depot_sammler::{NachrichtenBericht, Netz, SammelFehler, Schluessel, sammle_nachrichten};

const SCHLUESSEL: &str = "TESTSCHLUESSEL42";

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

fn ordner() -> Result<(PathBuf, PathBuf), String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_nachrichten_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
    let journal = dir.join("journal.txt");
    vorbedingung(Journal::einrichten(&journal), "Einrichtung")?;
    Ok((journal, dir.join("roh")))
}

fn um(t: u8, h: u8, m: u8) -> Result<Zeitpunkt, String> {
    Datum::neu(2026, 11, t)
        .and_then(|d| Zeitpunkt::neu(d, h, m, 0))
        .ok_or_else(|| "Vorbedingung: Zeitpunkt".into())
}

/// Eine Nachricht in der Form der Quelle.
fn eintrag(titel: &str, zeit: &str, inhalt: &str, werte: &[&str]) -> String {
    let werte: Vec<String> = werte
        .iter()
        .map(|w| {
            format!(
                "{{\"ticker\":{},\"relevance_score\":\"0.9\",\"ticker_sentiment_score\":\"0.1\",\"ticker_sentiment_label\":\"Neutral\"}}",
                zitiere(w)
            )
        })
        .collect();
    format!(
        concat!(
            "{{\"title\":{},\"url\":\"https://example.com/a\",\"time_published\":{},",
            "\"authors\":[\"A\"],\"summary\":{},\"source\":\"Benzinga\",",
            "\"topics\":[{{\"topic\":\"financial_markets\",\"relevance_score\":\"0.9\"}}],",
            "\"overall_sentiment_score\":0.28,\"overall_sentiment_label\":\"Somewhat-Bullish\",",
            "\"ticker_sentiment\":[{}]}}"
        ),
        zitiere(titel),
        zitiere(zeit),
        zitiere(inhalt),
        werte.join(",")
    )
}

fn antwort(eintraege: &[String]) -> String {
    format!(
        "{{\"items\":\"{}\",\"sentiment_score_definition\":\"x\",\"relevance_score_definition\":\"y\",\"feed\":[{}]}}",
        eintraege.len(),
        eintraege.join(",")
    )
}

struct Erfunden {
    antwort: Result<Vec<u8>, String>,
    abgerufen: Vec<String>,
}

impl Netz for Erfunden {
    fn hole(&mut self, adresse: &str) -> Result<Vec<u8>, String> {
        self.abgerufen.push(adresse.to_string());
        self.antwort.clone()
    }
}

fn lauf(
    journal: &Path,
    roh: &Path,
    antwort: Result<&str, &str>,
    zeit: Zeitpunkt,
) -> Result<Result<NachrichtenBericht, SammelFehler>, String> {
    let mut j = vorbedingung(Journal::oeffnen(journal), "Journal oeffnen")?;
    let s = vorbedingung(Schluessel::neu(SCHLUESSEL), "Schluessel")?;
    let mut netz = Erfunden {
        antwort: antwort
            .map(|t| t.as_bytes().to_vec())
            .map_err(str::to_string),
        abgerufen: Vec::new(),
    };
    let mut zeiten = vec![zeit].into_iter();
    let mut uhr = move || zeiten.next();
    let b = sammle_nachrichten(&mut j, &mut netz, &s, &mut uhr, roh);
    assert_eq!(netz.abgerufen.len(), 1, "genau ein Abruf");
    let a = netz.abgerufen.first().map_or("", String::as_str);
    assert!(a.starts_with("https://www.alphavantage.co/query?function=NEWS_SENTIMENT&"));
    assert!(a.ends_with(&format!("&apikey={SCHLUESSEL}")));
    Ok(b)
}

/// Die belegten Nachrichten: Zeit der Veroeffentlichung und Text.
fn belegt(journal: &Path) -> Result<Vec<(String, String)>, String> {
    Ok(vorbedingung(Journal::oeffnen(journal), "Journal oeffnen")?
        .stand()
        .nachrichten()
        .iter()
        .map(|n| (n.zeit().to_string(), n.text().to_string()))
        .collect())
}

fn dateien(roh: &Path) -> Vec<PathBuf> {
    fs::read_dir(roh.join(QUELLE_NACHRICHTEN))
        .map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default()
}

/// Erster Lauf: die rohe Antwort liegt unter `roh/alphavantage-news/`, jede Nachricht der
/// letzten 24 Stunden steht als Nachricht im Journal. Ein
/// zweiter Lauf mit derselben Antwort belegt nichts doppelt.
#[test]
fn nachrichten_werden_belegt() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let text = antwort(&[
        eintrag(
            "EZB laesst Zinsen unveraendert",
            "20261105T131500",
            "Die Notenbank haelt den Leitzins.",
            &[],
        ),
        eintrag(
            "SAP hebt Prognose an",
            "20261105T151458",
            "Cloud waechst staerker.",
            &["SAP", "FOREX:USD"],
        ),
        eintrag("Zu alt", "20261103T120000", "x", &[]),
        eintrag("Ohne Zeit", "gestern", "x", &[]),
        eintrag("", "20261105T150000", "ohne Titel", &[]),
        eintrag("Nach dem Abruf", "20261105T190000", "x", &[]),
    ]);
    let zeit = um(5, 17, 40)?;
    let b = lauf(&journal, &roh, Ok(&text), zeit)?.map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.fehler, None);
    assert_eq!((b.neu, b.bekannt), (2, 0));
    let datei = b.rohdatei.ok_or("keine Rohdatei")?;
    assert_eq!(dateien(&roh), vec![datei.clone()]);
    assert_eq!(vorbedingung(fs::read_to_string(&datei), "Rohdatei")?, text);
    // Die aelteste zuerst, mit Abruf und Abdruck der rohen Antwort.
    assert_eq!(
        belegt(&journal)?,
        vec![
            (
                um(5, 13, 15)?.to_string(),
                "Benzinga: EZB laesst Zinsen unveraendert. Die Notenbank haelt den Leitzins."
                    .to_string()
            ),
            (
                "2026-11-05T15:14:58Z".to_string(),
                "Benzinga [Werte: SAP, FOREX:USD]: SAP hebt Prognose an. Cloud waechst staerker."
                    .to_string()
            ),
        ]
    );
    let j = vorbedingung(Journal::oeffnen(&journal), "Journal oeffnen")?;
    for n in j.stand().nachrichten() {
        assert_eq!(n.quelle(), QUELLE_NACHRICHTEN);
        assert_eq!(n.abruf(), zeit);
        assert_eq!(n.roh().len(), 64);
    }
    // Belege von aussen beginnen keinen Tag.
    assert_eq!(j.stand().seit_abschluss(), 0);
    drop(j);
    let b = lauf(&journal, &roh, Ok(&text), um(5, 17, 50)?)?.map_err(|e| format!("{e:?}"))?;
    assert_eq!((b.neu, b.bekannt), (0, 2));
    assert_eq!(belegt(&journal)?.len(), 2);
    Ok(())
}

/// Hoechstens `MAX_NACHRICHTEN`, jede hoechstens `MAX_ZEICHEN` Zeichen, ohne Steuerzeichen.
#[test]
fn nachrichten_bleiben_klein() -> Result<(), String> {
    let lang = "Wort\nmit\tSteuerzeichen ".repeat(100);
    let eintraege: Vec<String> = (0..40)
        .map(|i| {
            eintrag(
                &format!("Titel {i:02}"),
                &format!("20261105T10{i:02}00"),
                &lang,
                &[],
            )
        })
        .collect();
    let n = lies_nachrichten(&antwort(&eintraege), um(5, 17, 40)?)?;
    assert_eq!(n.len(), MAX_NACHRICHTEN);
    for x in &n {
        assert!(x.text.chars().count() <= MAX_ZEICHEN, "{}", x.text);
        assert!(!x.text.chars().any(char::is_control), "{}", x.text);
        assert!(x.text.ends_with(" …"), "{}", x.text);
    }
    // Die neuesten zuerst: 10:39 bis 10:25.
    assert!(n.first().is_some_and(|x| x.text.contains("Titel 39")));
    assert!(n.last().is_some_and(|x| x.text.contains("Titel 25")));
    Ok(())
}

/// Meldet die Quelle statt Nachrichten einen Hinweis, wird nichts belegt; die rohe Antwort
/// bleibt zur Durchsicht liegen.
#[test]
fn hinweis_der_quelle_belegt_nichts() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let text = "{\"Information\": \"We have detected your API key and our standard API rate limit is 25 requests per day.\"}";
    let b = lauf(&journal, &roh, Ok(text), um(5, 17, 40)?)?.map_err(|e| format!("{e:?}"))?;
    assert!(
        b.fehler
            .as_deref()
            .is_some_and(|f| f.starts_with("Hinweis der Quelle: We have detected")),
        "{b:?}"
    );
    assert_eq!(b.neu, 0);
    assert!(b.rohdatei.is_some());
    assert!(belegt(&journal)?.is_empty());
    let b = lauf(&journal, &roh, Ok("kein json"), um(5, 17, 41)?)?.map_err(|e| format!("{e:?}"))?;
    assert!(b.fehler.is_some(), "{b:?}");
    assert!(belegt(&journal)?.is_empty());
    Ok(())
}

/// Gibt die Quelle den Schluessel zurueck, gleich in welcher Schreibweise, wird die Antwort
/// weder abgelegt noch gelesen (K19-01). Ein Fehler beim Abruf nennt ihn nicht.
#[test]
fn schluessel_in_der_antwort_wird_nicht_abgelegt() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let text = antwort(&[eintrag(
        "Leck",
        "20261105T150000",
        &SCHLUESSEL.to_lowercase(),
        &[],
    )]);
    let b = lauf(&journal, &roh, Ok(&text), um(5, 17, 40)?)?.map_err(|e| format!("{e:?}"))?;
    assert!(b.fehler.is_some());
    assert_eq!(b.rohdatei, None);
    assert!(dateien(&roh).is_empty());
    assert!(belegt(&journal)?.is_empty());
    let fehler = format!("https://www.alphavantage.co/query?apikey={SCHLUESSEL}: timeout");
    let b = lauf(&journal, &roh, Err(&fehler), um(5, 17, 41)?)?.map_err(|e| format!("{e:?}"))?;
    assert!(
        b.fehler.as_deref().is_some_and(|f| !f.contains(SCHLUESSEL)),
        "{b:?}"
    );
    assert!(dateien(&roh).is_empty());
    Ok(())
}

/// Gegenprobe von Maus (K28-01): Gibt die Quelle den Schluessel als Unicode-Escapes zurueck,
/// etwa `\u0054` fuer `T`, wird die Antwort weder abgelegt noch belegt. Ebenso, wenn erst eine
/// zweite Schicht Escapes ihn verbirgt.
#[test]
fn schluessel_hinter_escapes_wird_nicht_abgelegt() -> Result<(), String> {
    let maskiert: String = SCHLUESSEL.chars().fold(String::new(), |mut m, c| {
        let _ = write!(m, "\\u{:04x}", u32::from(c));
        m
    });
    let einfach = antwort(&[]).replace("\"feed\":[]", &format!(
        "\"feed\":[{{\"title\":\"Leck {maskiert}\",\"time_published\":\"20261105T150000\",\"source\":\"X\",\"summary\":\"\"}}]"
    ));
    let doppelt = einfach.replace(&maskiert, &maskiert.replace('\\', "\\\\"));
    for (fall, text) in [("einfach", &einfach), ("doppelt", &doppelt)] {
        let (journal, roh) = ordner()?;
        assert!(
            !text.contains(SCHLUESSEL),
            "{fall}: Vorbedingung, roh verborgen"
        );
        let b = lauf(&journal, &roh, Ok(text), um(5, 17, 40)?)?.map_err(|e| format!("{e:?}"))?;
        assert!(b.fehler.is_some(), "{fall}: {b:?}");
        assert_eq!(b.rohdatei, None, "{fall}");
        assert!(dateien(&roh).is_empty(), "{fall}");
        assert!(belegt(&journal)?.is_empty(), "{fall}");
        let j = vorbedingung(fs::read_to_string(&journal), "Journal")?;
        assert!(!j.contains(SCHLUESSEL), "{fall}");
    }
    Ok(())
}
