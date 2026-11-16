//! Der veröffentlichte FOMC-Kalender der Federal Reserve.

use std::collections::BTreeSet;
use std::path::Path;

use depot_journal::Journal;
use depot_kern::{Betroffen, Datum, Termin, Verlaesslichkeit, Zeitbezug, Zeitpunkt};

use crate::ergebnisse::{KalenderBericht, belege_kalender, merke_abruf};
use crate::sammeln::{SammelFehler, lege_ab};
use crate::{Netz, Zugang};

pub const QUELLE_FED: &str = "fed";
pub const ADRESSE_FED: &str = "https://www.federalreserve.gov/monetarypolicy/fomccalendars.htm";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FedFehler {
    Seite,
    /// Zeile ab 1 in den ausgewerteten Kalenderblöcken.
    Zeile(usize),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FedKalender {
    pub termine: Vec<Termin>,
    pub ausgelassen: Vec<String>,
}

fn hat_klasse(knoten: &str, klasse: &str) -> bool {
    knoten
        .split_once('>')
        .and_then(|(tag, _)| tag.split_once(" class="))
        .and_then(|(_, wert)| {
            let anfuehrung = wert.chars().next().filter(|c| matches!(c, '"' | '\''))?;
            wert.strip_prefix(anfuehrung)?.split_once(anfuehrung)
        })
        .is_some_and(|(wert, _)| wert.split_whitespace().any(|k| k == klasse))
}

fn abschnitte<'a>(html: &'a str, klasse: &'a str) -> impl Iterator<Item = &'a str> {
    let mut starts = html
        .match_indices("<div")
        .filter_map(move |(nr, _)| hat_klasse(html.get(nr..)?, klasse).then_some(nr))
        .peekable();
    std::iter::from_fn(move || {
        let anfang = starts.next()?;
        html.get(anfang..starts.peek().copied().unwrap_or(html.len()))
    })
}

fn kalenderjahr(panel: &str) -> Option<u16> {
    let (_, kopf) = panel.split_once("<h4>")?;
    let (kopf, _) = kopf.split_once("</h4>")?;
    let (_, text) = kopf.trim_start().strip_prefix("<a ")?.split_once('>')?;
    text.strip_suffix("</a>")?
        .strip_suffix(" FOMC Meetings")?
        .parse()
        .ok()
}

fn feld<'a>(zeile: &'a str, klasse: &str) -> Option<&'a str> {
    let mut felder = zeile.match_indices("<div").filter_map(|(nr, _)| {
        let knoten = zeile.get(nr..)?;
        hat_klasse(knoten, klasse).then_some(knoten)
    });
    let knoten = felder.next()?;
    if felder.next().is_some() {
        return None;
    }
    let (_, danach) = knoten.split_once('>')?;
    let (text, _) = danach.split_once("</div>")?;
    Some(text.trim())
}

fn monat(name: &str) -> Option<u8> {
    Some(match name.trim() {
        "January" | "Jan" => 1,
        "February" | "Feb" => 2,
        "March" | "Mar" => 3,
        "April" | "Apr" => 4,
        "May" => 5,
        "June" | "Jun" => 6,
        "July" | "Jul" => 7,
        "August" | "Aug" => 8,
        "September" | "Sep" => 9,
        "October" | "Oct" => 10,
        "November" | "Nov" => 11,
        "December" | "Dec" => 12,
        _ => return None,
    })
}

fn datumspaar(jahr: u16, monate: &str, tage: &str) -> Option<(Datum, Datum)> {
    let (erster_monat, letzter_monat) = monate.split_once('/').unwrap_or((monate, monate));
    let (erster_monat, letzter_monat) = (monat(erster_monat)?, monat(letzter_monat)?);
    let (erster, letzter) = tage.split_once('-').or_else(|| tage.split_once('–'))?;
    let start = Datum::neu(jahr, erster_monat, erster.trim().parse().ok()?)?;
    let endjahr = if (erster_monat, letzter_monat) == (12, 1) {
        jahr.checked_add(1)?
    } else {
        jahr
    };
    let ende = Datum::neu(endjahr, letzter_monat, letzter.trim().parse().ok()?)?;
    (start.plus_tage(1) == Some(ende)).then_some((start, ende))
}

fn termin(start: Datum, ende: Datum, projektionen: bool) -> Option<Termin> {
    let zusatz = if projektionen {
        ", mit Projektionen"
    } else {
        ""
    };
    let beginn = if (start.monat(), start.jahr()) == (ende.monat(), ende.jahr()) {
        format!("{:02}.", start.tag())
    } else {
        format!("{:02}.{:02}.", start.tag(), start.monat())
    };
    let titel = format!(
        "Fed-Sitzung {beginn}–{:02}.{:02}.{}, Zinsentscheid am {:02}.{:02}.{zusatz}",
        ende.tag(),
        ende.monat(),
        ende.jahr(),
        ende.tag(),
        ende.monat()
    );
    Termin::neu(
        &format!("fed-{ende}"),
        &titel,
        Zeitbezug::Tag(ende),
        vec![Betroffen::groesse("usd-leitzins")?],
        Verlaesslichkeit::Bestaetigt,
    )
    .ok()
}

fn zeile_lesen(kalender: &mut FedKalender, zeile: &str, jahr: u16) -> Option<()> {
    let monate = feld(zeile, "fomc-meeting__month")?
        .strip_prefix("<strong>")?
        .strip_suffix("</strong>")?;
    let tage = feld(zeile, "fomc-meeting__date")?;
    if let Some(tag) = tage.strip_suffix("(unscheduled)") {
        // Die Fed kennzeichnet nicht angesetzte Sitzungen ausdrücklich, z. B. „15 (unscheduled)“.
        Datum::neu(jahr, monat(monate)?, tag.trim().parse().ok()?)?;
        kalender.ausgelassen.push(format!("{jahr} {monate} {tage}"));
        return Some(());
    }
    let projektionen = tage.ends_with('*');
    let (start, ende) = datumspaar(jahr, monate, tage.strip_suffix('*').unwrap_or(tage))?;
    // Die Fed kennzeichnet solche Strategiesitzungen im Kalender und setzt keinen Zinsentscheid an.
    if zeile.contains("<p>Monetary Policy Strategy Meeting</p>") {
        kalender
            .ausgelassen
            .push(format!("{ende}: Strategiesitzung"));
    } else {
        kalender.termine.push(termin(start, ende, projektionen)?);
    }
    Some(())
}

/// Liest den Kalender für dieses und, sobald veröffentlicht, das nächste Jahr vollständig.
pub fn lies_fed_seite(html: &str, heute: Datum) -> Result<FedKalender, FedFehler> {
    let mut kalender = FedKalender::default();
    let mut jahre = BTreeSet::new();
    let mut nr = 0_usize;
    for panel in abschnitte(html, "panel-default") {
        let Some(jahr) = kalenderjahr(panel) else {
            continue;
        };
        if jahr != heute.jahr() && Some(jahr) != heute.jahr().checked_add(1) {
            continue;
        }
        if !jahre.insert(jahr) {
            return Err(FedFehler::Seite);
        }
        let vorher = kalender.termine.len();
        for zeile in abschnitte(panel, "fomc-meeting") {
            nr = nr.saturating_add(1);
            zeile_lesen(&mut kalender, zeile, jahr).ok_or(FedFehler::Zeile(nr))?;
        }
        let felder = panel
            .match_indices("<div")
            .filter(|(i, _)| {
                panel
                    .get(*i..)
                    .is_some_and(|k| hat_klasse(k, "fomc-meeting__date"))
            })
            .count();
        let zeilen = abschnitte(panel, "fomc-meeting").count();
        if kalender.termine.len() == vorher || felder != zeilen {
            return Err(FedFehler::Seite);
        }
    }
    let mut kennungen = BTreeSet::new();
    if !jahre.contains(&heute.jahr())
        || !kalender
            .termine
            .iter()
            .all(|t| kennungen.insert(t.kennung()))
    {
        return Err(FedFehler::Seite);
    }
    Ok(kalender)
}

/// Ruft die Seite einmal je Frankfurter Tag ab. Ein Lesefehler erzeugt keine Termine.
pub fn sammle_fed(
    journal: &mut Journal,
    netz: &mut dyn Netz,
    uhr: &mut dyn FnMut() -> Option<Zeitpunkt>,
    ablage: &Path,
    zugang: &Zugang,
) -> Result<KalenderBericht, SammelFehler> {
    let mut bericht = KalenderBericht::default();
    let tag = uhr()
        .and_then(Zeitpunkt::frankfurt_tag)
        .ok_or(SammelFehler::Uhr)?;
    if !merke_abruf(ablage, QUELLE_FED, tag).map_err(SammelFehler::Ablage)? {
        bericht.uebersprungen = Some("heute schon abgerufen".into());
        return Ok(bericht);
    }
    let roh = match netz.hole(ADRESSE_FED) {
        Ok(roh) => roh,
        Err(e) => {
            bericht.fehler = Some(zugang.bereinige(&format!("Abruf: {e}")));
            return Ok(bericht);
        }
    };
    if let Some(text) = crate::diagnose::sicherheitsabbruch(zugang, QUELLE_FED, &roh) {
        bericht.sicherheitsabbruch = true;
        bericht.fehler = Some(text);
        return Ok(bericht);
    }
    let abruf = uhr().ok_or(SammelFehler::Uhr)?;
    bericht.rohdatei =
        Some(lege_ab(ablage, QUELLE_FED, abruf, ".html", &roh).map_err(SammelFehler::Ablage)?);
    match std::str::from_utf8(&roh)
        .map_err(|_| "Seite ist kein UTF-8".to_owned())
        .and_then(|html| lies_fed_seite(html, tag).map_err(|e| format!("Kalender unlesbar: {e:?}")))
    {
        Ok(kalender) => {
            bericht.ausgelassen = kalender.ausgelassen;
            belege_kalender(
                &mut bericht,
                journal,
                QUELLE_FED,
                abruf,
                &roh,
                kalender.termine,
            )?;
        }
        Err(e) => bericht.fehler = Some(e),
    }
    Ok(bericht)
}
