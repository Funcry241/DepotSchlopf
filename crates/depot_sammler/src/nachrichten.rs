//! Ein Lauf ueber die Nachrichten (R40): ein Abruf bei Alpha Vantage, roh ablegen, lesen,
//! belegen (`nachricht` im Journal).
//!
//! Dachs und Maus suchen nicht selbst im Netz (K25-01: was eine Suche einfuegt, laesst sich vor
//! dem Aufruf nicht begrenzen). Stattdessen holt der Sammler einmal am Tag die neuesten
//! Marktnachrichten, und der Papiertag gibt sie den Beratern mit (Wolfgang, 05.11.2026).
//!
//! Reihenfolge wie bei den Kursen (R12): Erst liegt die rohe Antwort unter
//! `daten/roh/alphavantage-news/`, dann steht jede Nachricht daraus im Journal. Enthaelt die
//! Antwort den Schluessel, roh oder in einer dekodierten Zeichenkette, wird sie weder abgelegt
//! noch gelesen (K19-01, K28-01). Der Lauf erfindet
//! nichts: Eine Nachricht ohne Titel oder mit unlesbarer Zeit wird ausgelassen, nicht geraten.
//! Belegt werden hoechstens `MAX_NACHRICHTEN` Nachrichten der letzten `MAX_ALTER_STUNDEN`
//! Stunden, jede mit hoechstens `MAX_ZEICHEN` Zeichen; so bleibt die Frage an die Berater klein.

use std::path::{Path, PathBuf};

use depot_journal::{Journal, JournalFehler, NachrichtBeleg};
use depot_kern::json::{self, Json};
use depot_kern::{Datum, Zeitpunkt};

use crate::netz::Netz;
use crate::sammeln::{SammelFehler, lege_ab};
use crate::schluessel::{Anbieter, Schluessel, Zugang};

/// Name der Quelle; Ordner der rohen Antworten.
pub const QUELLE_NACHRICHTEN: &str = "alphavantage-news";
/// Hoechstens so viele Nachrichten je Lauf, die neuesten zuerst.
pub const MAX_NACHRICHTEN: usize = 15;
/// Hoechstlaenge einer belegten Nachricht in Zeichen.
pub const MAX_ZEICHEN: usize = 400;
/// Nur Nachrichten, die hoechstens so alt sind wie der Abruf.
pub const MAX_ALTER_STUNDEN: i64 = 24;

/// Die Abrufadresse: die 50 neuesten Nachrichten zum Thema Finanzmaerkte.
pub fn adresse_nachrichten(schluessel: &Schluessel) -> String {
    format!(
        "https://www.alphavantage.co/query?function=NEWS_SENTIMENT&topics=financial_markets&sort=LATEST&limit=50&apikey={}",
        schluessel.als_str()
    )
}

/// Eine gelesene Nachricht: Zeit der Veroeffentlichung (UTC) und der Text fuer das Journal:
/// Quelle, betroffene Werte, Titel und Zusammenfassung.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nachricht {
    pub zeit: Zeitpunkt,
    pub text: String,
}

/// Der Bericht eines Laufs ueber die Nachrichten.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NachrichtenBericht {
    pub sicherheitsabbruch: bool,
    /// Die abgelegte rohe Antwort; keine, wenn der Abruf scheiterte oder sie den Schluessel
    /// enthielt.
    pub rohdatei: Option<PathBuf>,
    /// Neu belegte Nachrichten.
    pub neu: usize,
    /// Nachrichten, die schon genau so belegt waren.
    pub bekannt: usize,
    /// Warum nichts belegt wurde. Die Meldung ist vom Schluessel bereinigt.
    pub fehler: Option<String>,
}

/// Ein Text fuer das Journal: Steuerzeichen werden zu Leerzeichen, Leerraum wird zusammengefasst,
/// hoechstens `MAX_ZEICHEN` Zeichen.
fn sauber(text: &str) -> String {
    let flach: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let eins = flach.split_whitespace().collect::<Vec<_>>().join(" ");
    if eins.chars().nth(MAX_ZEICHEN).is_some() {
        let mut kurz: String = eins.chars().take(MAX_ZEICHEN.saturating_sub(2)).collect();
        kurz.push_str(" …");
        kurz
    } else {
        eins
    }
}

/// `20261105T151458` als Zeitpunkt in UTC.
fn lies_zeit(t: &str) -> Option<Zeitpunkt> {
    let (tag, uhr) = t.split_once('T')?;
    if tag.len() != 8 || uhr.len() != 6 || !t.bytes().all(|b| b.is_ascii_digit() || b == b'T') {
        return None;
    }
    let zahl = |s: &str, von: usize, bis: usize| s.get(von..bis)?.parse::<u16>().ok();
    let datum = Datum::neu(
        zahl(tag, 0, 4)?,
        u8::try_from(zahl(tag, 4, 6)?).ok()?,
        u8::try_from(zahl(tag, 6, 8)?).ok()?,
    )?;
    Zeitpunkt::neu(
        datum,
        u8::try_from(zahl(uhr, 0, 2)?).ok()?,
        u8::try_from(zahl(uhr, 2, 4)?).ok()?,
        u8::try_from(zahl(uhr, 4, 6)?).ok()?,
    )
}

fn nachricht(x: &Json) -> Option<Nachricht> {
    let titel = x.feld("title").and_then(Json::text)?.trim();
    if titel.is_empty() {
        return None;
    }
    let zeit = lies_zeit(x.feld("time_published").and_then(Json::text)?)?;
    let quelle = x
        .feld("source")
        .and_then(Json::text)
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .unwrap_or("?");
    let inhalt = x.feld("summary").and_then(Json::text).map_or("", str::trim);
    let werte: Vec<&str> = x
        .feld("ticker_sentiment")
        .and_then(Json::liste)
        .unwrap_or(&[])
        .iter()
        .filter_map(|t| t.feld("ticker").and_then(Json::text))
        .take(5)
        .collect();
    let werte = if werte.is_empty() {
        String::new()
    } else {
        format!(" [Werte: {}]", werte.join(", "))
    };
    let kopf = format!("{quelle}{werte}: {titel}.");
    let text = if inhalt.is_empty() {
        kopf
    } else {
        format!("{kopf} {inhalt}")
    };
    Some(Nachricht {
        zeit,
        text: sauber(&text),
    })
}

/// Liest die Antwort der Quelle: die Nachrichten der letzten `MAX_ALTER_STUNDEN` Stunden vor
/// `abruf`, die neuesten zuerst, hoechstens `MAX_NACHRICHTEN`.
///
/// # Errors
/// Die Antwort ist kein JSON oder hat keine Liste `feed`; dann meist ein Hinweis der Quelle,
/// etwa das erreichte Tageskontingent.
pub fn lies_nachrichten(text: &str, abruf: Zeitpunkt) -> Result<Vec<Nachricht>, String> {
    let w = json::lies(text)?;
    let Some(feed) = w.feld("feed").and_then(Json::liste) else {
        let hinweis = ["Information", "Note", "Error Message"]
            .iter()
            .find_map(|f| w.feld(f).and_then(Json::text))
            .map_or_else(|| "Antwort ohne feed".to_string(), sauber);
        return Err(format!("Hinweis der Quelle: {hinweis}"));
    };
    let fenster = MAX_ALTER_STUNDEN.saturating_mul(3_600);
    let mut aus: Vec<Nachricht> = feed
        .iter()
        .filter_map(nachricht)
        .filter(|n| {
            abruf
                .sekunden_seit(n.zeit)
                .is_some_and(|s| (0..=fenster).contains(&s))
        })
        .collect();
    aus.sort_by(|a, b| b.zeit.cmp(&a.zeit).then_with(|| a.text.cmp(&b.text)));
    aus.dedup_by(|a, b| a.text == b.text);
    aus.truncate(MAX_NACHRICHTEN);
    Ok(aus)
}

/// Ein Lauf ueber die Nachrichten. `uhr` liefert die Zeit des Rechners in UTC.
///
/// # Errors
/// Journal, Ablage oder Uhr versagen. Was bis dahin belegt ist, bleibt belegt.
pub fn sammle_nachrichten(
    journal: &mut Journal,
    netz: &mut dyn Netz,
    zugang: &Zugang,
    uhr: &mut dyn FnMut() -> Option<Zeitpunkt>,
    ablage: &Path,
) -> Result<NachrichtenBericht, SammelFehler> {
    let mut bericht = NachrichtenBericht::default();
    let Some(schluessel) = zugang.von(Anbieter::AlphaVantage) else {
        bericht.fehler = Some("Schluessel fehlt (DepotSchlopf/alphavantage)".into());
        return Ok(bericht);
    };
    let roh = match netz.hole(&adresse_nachrichten(schluessel)) {
        Ok(r) => r,
        Err(e) => {
            bericht.fehler = Some(format!("Abruf: {}", zugang.bereinige(&e.to_string())));
            return Ok(bericht);
        }
    };
    if zugang.steht_in(&roh) {
        bericht.sicherheitsabbruch = true;
        bericht.fehler =
            Some("Die Antwort enthaelt den Schluessel: nicht abgelegt, nicht belegt".into());
        return Ok(bericht);
    }
    let abruf = uhr().ok_or(SammelFehler::Uhr)?;
    let gelesen = std::str::from_utf8(&roh)
        .map_err(|_| "Antwort ist kein UTF-8".to_string())
        .and_then(|t| lies_nachrichten(t, abruf));
    // Auch die fertig aufbereiteten Texte prüfen, bevor Antwort oder Belege gespeichert werden.
    if gelesen.as_ref().is_ok_and(|nachrichten| {
        nachrichten
            .iter()
            .any(|n| zugang.steht_in(n.text.as_bytes()))
    }) {
        bericht.sicherheitsabbruch = true;
        bericht.fehler = Some("Eine Nachricht enthaelt den Schluessel: nichts belegt".into());
        return Ok(bericht);
    }
    bericht.rohdatei = Some(
        lege_ab(ablage, QUELLE_NACHRICHTEN, abruf, ".json", &roh).map_err(SammelFehler::Ablage)?,
    );
    let nachrichten = match gelesen {
        Ok(n) => n,
        Err(e) => {
            bericht.fehler = Some(zugang.bereinige(&e));
            return Ok(bericht);
        }
    };
    // Die aelteste zuerst, damit das Journal in der Reihenfolge der Veroeffentlichung steht.
    for n in nachrichten.iter().rev() {
        let beleg = NachrichtBeleg::neu(QUELLE_NACHRICHTEN, abruf, &roh, n.zeit, &n.text)
            .map_err(|e| SammelFehler::Journal(JournalFehler::Abgelehnt(e)))?;
        if journal
            .belege_nachricht(&beleg)
            .map_err(SammelFehler::Journal)?
        {
            bericht.neu = bericht.neu.saturating_add(1);
        } else {
            bericht.bekannt = bericht.bekannt.saturating_add(1);
        }
    }
    Ok(bericht)
}
