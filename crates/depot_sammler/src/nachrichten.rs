//! Zwei feste kostenlose RSS-Quellen. Keine Artikelseiten, keine KI-Aufrufe, keine API-Schluessel.
//! Alle Antworten werden vor Speicherung auf hinterlegte Schluessel geprueft, auch XML-dekodiert.
use std::path::{Path, PathBuf};

use depot_journal::{Journal, JournalFehler, NachrichtBeleg};
use depot_kern::Zeitpunkt;

use crate::netz::Netz;
use crate::sammeln::{SammelFehler, lege_ab};
use crate::schluessel::Zugang;

pub const MAX_NACHRICHTEN: usize = 15;
pub const MAX_ZEICHEN: usize = 400;
pub const MAX_ALTER_STUNDEN: i64 = 24;
/// tagesschau nennt im Feed TTL 90 Minuten. Auch manuelle Klicks und Neustarts umgehen sie nicht.
pub const ABSTAND_SEKUNDEN: i64 = 90 * 60;
pub const QUELLEN: &str = crate::rss_quellen::QUELLEN;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nachricht {
    pub zeit: Zeitpunkt,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NachrichtenBericht {
    pub sicherheitsabbruch: bool,
    pub rohdatei: Option<PathBuf>,
    pub neu: usize,
    pub bekannt: usize,
    pub fehler: Option<String>,
    pub quellen: Vec<String>,
}

struct Gelesen {
    quelle: String,
    abruf: Zeitpunkt,
    roh: Vec<u8>,
    nachrichten: Vec<Nachricht>,
}

fn versuch(journal: &mut Journal, quelle: &str, jetzt: Zeitpunkt) -> Result<bool, SammelFehler> {
    let anfang = format!("rss-abruf: {quelle};");
    let letzter = journal
        .stand()
        .beobachtungen()
        .iter()
        .rev()
        .find_map(|(_, text)| {
            text.strip_prefix(&anfang)
                .and_then(|s| s.parse::<i64>().ok())
        });
    if letzter.is_some_and(|s| {
        jetzt
            .sekunden()
            .checked_sub(s)
            .is_none_or(|d| d < ABSTAND_SEKUNDEN)
    }) {
        return Ok(false);
    }
    let datum = jetzt.datum().ok_or(SammelFehler::Uhr)?;
    // Vor dem Netz dauerhaft belegen: auch nach Absturz kein sofortiger zweiter Abruf.
    journal
        .beobachte(datum, &format!("{anfang}{}", jetzt.sekunden()))
        .map_err(SammelFehler::Journal)?;
    Ok(true)
}

/// Auch mehrfach als XML-Referenz geschriebene Zugangsdaten erkennen, bevor Rohdaten bleiben.
fn enthaelt_schluessel(text: &str, zugang: &Zugang) -> bool {
    let mut text = text.to_string();
    for _ in 0..5 {
        if zugang.steht_in(text.as_bytes()) {
            return true;
        }
        let mut aus = String::new();
        let mut rest = text.as_str();
        while let Some((vor, nach)) = rest.split_once('&') {
            aus.push_str(vor);
            if let Some((referenz, danach)) = nach.split_once(';')
                && referenz.len() <= 16
            {
                let roh = format!("&{referenz};");
                if let Ok(wert) = quick_xml::escape::unescape(&roh) {
                    aus.push_str(&wert);
                    rest = danach;
                    continue;
                }
            }
            aus.push('&');
            rest = nach;
        }
        aus.push_str(rest);
        if aus == text {
            break;
        }
        text = aus;
    }
    false
}

/// Ein GET je Quelle, mindestens 90 Minuten Abstand. Erst nach allen Sicherheitspruefungen
/// werden Rohantworten abgelegt und hoechstens 15 Nachrichten belegt; keine Teilnachrichten
/// nach einem Sicherheitsabbruch. Der Ausfall einer Quelle laesst die andere verwendbar.
///
/// # Errors
/// Fehler beim dauerhaften Journal, der Uhr oder der Rohablage.
pub fn sammle_nachrichten(
    journal: &mut Journal,
    netz: &mut dyn Netz,
    zugang: &Zugang,
    uhr: &mut dyn FnMut() -> Option<Zeitpunkt>,
    ablage: &Path,
) -> Result<NachrichtenBericht, SammelFehler> {
    let mut b = NachrichtenBericht::default();
    let mut gelesen = Vec::new();
    let mut fehler = Vec::new();
    for zeile in QUELLEN.lines() {
        let (quelle, adresse) = zeile
            .split_once('|')
            .ok_or_else(|| SammelFehler::Ablage("RSS-Quellenliste ungueltig".into()))?;
        let start = uhr().ok_or(SammelFehler::Uhr)?;
        if !versuch(journal, quelle, start)? {
            b.quellen.push(format!("{quelle}: Abrufpause (mindestens 90 Minuten), vorhandene Nachrichten bleiben erhalten."));
            continue;
        }
        let roh = match netz.hole(adresse) {
            Ok(r) => r,
            Err(e) => {
                fehler.push(format!("{quelle}: {}", zugang.bereinige(&e.to_string())));
                continue;
            }
        };
        if zugang.steht_in(&roh) {
            b.sicherheitsabbruch = true;
            break;
        }
        let abruf = uhr().ok_or(SammelFehler::Uhr)?;
        if abruf < start {
            return Err(SammelFehler::Uhr);
        }
        let antwort = std::str::from_utf8(&roh)
            .map_err(|_| "RSS ist kein UTF-8".to_string())
            .and_then(|t| crate::rss::lies(t, quelle, abruf));
        let (nachrichten, ausgelassen, dekodiert) = match antwort {
            Ok(n) => n,
            Err(e) => {
                fehler.push(format!("{quelle}: {e}"));
                continue;
            }
        };
        if enthaelt_schluessel(&dekodiert, zugang)
            || nachrichten
                .iter()
                .any(|n| zugang.steht_in(n.text.as_bytes()))
        {
            b.sicherheitsabbruch = true;
            break;
        }
        b.quellen.push(format!("{quelle}: {} aktuelle Schlagzeilen; {ausgelassen} ungueltige, kuenftige oder zu alte Eintraege ausgelassen.", nachrichten.len()));
        gelesen.push(Gelesen {
            quelle: quelle.into(),
            abruf,
            roh,
            nachrichten,
        });
    }
    if b.sicherheitsabbruch {
        b.fehler = Some("Sicherheitsabbruch: Eine RSS-Antwort enthaelt hinterlegte Zugangsdaten. Keine Antwort und keine Nachricht dieses Laufs gespeichert.".into());
        return Ok(b);
    }
    b.quellen.extend(fehler.iter().cloned());
    if gelesen.is_empty() && !fehler.is_empty() {
        b.fehler = Some(fehler.join("; "));
    }
    belege(journal, ablage, &mut b, gelesen)?;
    Ok(b)
}

fn belege(
    journal: &mut Journal,
    ablage: &Path,
    b: &mut NachrichtenBericht,
    mut gelesen: Vec<Gelesen>,
) -> Result<(), SammelFehler> {
    // Jede Quelle darf beitragen. Die neuesten zuerst, im Wechsel, insgesamt unveraendert 15.
    for g in &mut gelesen {
        g.nachrichten
            .sort_by(|a, b| b.zeit.cmp(&a.zeit).then_with(|| a.text.cmp(&b.text)));
        g.nachrichten
            .dedup_by(|a, b| a.text == b.text && a.zeit == b.zeit);
        let pfad =
            lege_ab(ablage, &g.quelle, g.abruf, ".xml", &g.roh).map_err(SammelFehler::Ablage)?;
        b.rohdatei = Some(pfad);
    }
    let mut auswahl = Vec::new();
    for i in 0..MAX_NACHRICHTEN {
        for g in &gelesen {
            if let Some(n) = g.nachrichten.get(i)
                && auswahl.len() < MAX_NACHRICHTEN
            {
                auswahl.push((g, n));
            }
        }
    }
    auswahl.sort_by_key(|(g, n)| (g.abruf, n.zeit));
    for (g, n) in auswahl {
        let beleg = NachrichtBeleg::neu(&g.quelle, g.abruf, &g.roh, n.zeit, &n.text)
            .map_err(|e| SammelFehler::Journal(JournalFehler::Abgelehnt(e)))?;
        if journal
            .belege_nachricht(&beleg)
            .map_err(SammelFehler::Journal)?
        {
            b.neu = b.neu.saturating_add(1);
        } else {
            b.bekannt = b.bekannt.saturating_add(1);
        }
    }
    Ok(())
}
