//! Ein Lauf des Sammlers: je Wert abrufen, roh ablegen, lesen, belegen; danach pruefen.
//!
//! Reihenfolge je Wert (R12): Erst liegt die rohe Antwort unter `daten/roh/`, dann steht jeder
//! Kurs daraus im Journal, erst dann wird er verwendet. Was nicht gelesen werden kann, wird nicht
//! belegt; die rohe Antwort bleibt zur Durchsicht liegen.
//!
//! Meldet die Quelle statt Kursen einen Hinweis, gilt: Ein Fehler zu genau diesem Kuerzel
//! (`Error Message`) betrifft nur diesen Wert. Jeder andere Hinweis, etwa das erreichte
//! Tageskontingent, beendet den Lauf, damit kein weiterer Abruf das Kontingent belastet.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use depot_journal::{Journal, JournalFehler, KursBeleg};
use depot_kern::{Cent, Datum, Grenzen, Kurs, Kurse, Zeitpunkt, lies_tageskurse, schlusskurse};
use depot_regelwerk::{DatenFehler, pruefe_kurse};

use crate::liste::{QUELLE, Wert, als_universum};
use crate::netz::Netz;
use crate::schluessel::Schluessel;

/// Was ein Wert ergeben hat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ergebnis {
    /// Belegt: die Schlusskurse des letzten beendeten Tags und des Tags davor, soweit vorhanden.
    Belegt {
        heute: Option<(Datum, Cent)>,
        vortag: Option<(Datum, Cent)>,
        rohdatei: PathBuf,
    },
    /// Nichts belegt. Die Meldung ist vom Schluessel bereinigt.
    Fehler(String),
    /// Nicht abgerufen, weil der Lauf vorher beendet wurde.
    NichtAbgerufen,
}

/// Der Bericht eines Laufs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bericht {
    pub werte: Vec<(Wert, Ergebnis)>,
    /// Warum der Lauf vor dem letzten Wert endete.
    pub abbruch: Option<String>,
    /// Der juengste Handelstag unter den belegten Schlusskursen.
    pub handelstag: Option<Datum>,
    /// Ergebnis von `pruefe_kurse` fuer alle Werte der Liste; leer = alles in Ordnung.
    pub pruefung: Vec<DatenFehler>,
}

/// Was den Lauf anhaelt, bevor alles abgearbeitet ist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SammelFehler {
    /// Das Journal hat den Beleg nicht angenommen oder ist im Halt.
    Journal(JournalFehler),
    /// Die rohe Antwort liess sich nicht ablegen.
    Ablage(String),
    /// Die Uhr lieferte keinen gueltigen Zeitpunkt.
    Uhr,
    /// Kurse eines Abrufs liessen sich nicht zusammenfassen.
    Kurse(String),
}

/// Die Abrufadresse eines Werts: Tageskurse der letzten 100 Handelstage als CSV.
pub fn adresse(wert: &Wert, schluessel: &Schluessel) -> String {
    format!(
        "https://www.alphavantage.co/query?function=TIME_SERIES_DAILY&symbol={}&outputsize=compact&datatype=csv&apikey={}",
        wert.kuerzel,
        schluessel.als_str()
    )
}

/// Ein Hinweis der Quelle statt Kursen: eine JSON-Antwort. `Some((betrifft_nur_den_wert, text))`.
fn hinweis(roh: &[u8]) -> Option<(bool, String)> {
    let text = String::from_utf8_lossy(roh);
    let t = text.trim_start();
    if !t.starts_with('{') {
        return None;
    }
    let nur_wert = t.contains("\"Error Message\"");
    let kurz: String = t
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(300)
        .collect();
    Some((nur_wert, kurz))
}

/// Ablage der rohen Antwort: `<ablage>/<quelle>/<abruf>_<isin>.csv`, nur neu angelegt.
fn lege_ab(ablage: &Path, abruf: Zeitpunkt, wert: &Wert, roh: &[u8]) -> Result<PathBuf, String> {
    let ordner = ablage.join(QUELLE);
    fs::create_dir_all(&ordner).map_err(|e| format!("{}: {e}", ordner.display()))?;
    let stempel: String = abruf
        .to_string()
        .chars()
        .filter(|c| *c != '-' && *c != ':')
        .collect();
    let pfad = ordner.join(format!("{stempel}_{}.csv", wert.isin));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pfad)
        .and_then(|mut f| {
            f.write_all(roh)?;
            f.sync_all()
        })
        .map_err(|e| format!("{}: {e}", pfad.display()))?;
    Ok(pfad)
}

fn tag_und_preis(k: Option<&Kurs>) -> Option<(Datum, Cent)> {
    k.map(|k| (k.handelstag(), k.preis()))
}

/// Ein Lauf ueber alle Werte. `uhr` liefert die Zeit des Rechners in UTC; `pause` liegt
/// zwischen zwei Abrufen.
///
/// # Errors
/// Journal, Ablage oder Uhr versagen. Was bis dahin belegt ist, bleibt belegt.
pub fn sammle(
    journal: &mut Journal,
    netz: &mut dyn Netz,
    schluessel: &Schluessel,
    uhr: &mut dyn FnMut() -> Option<Zeitpunkt>,
    ablage: &Path,
    pause: Duration,
    werte: &[Wert],
) -> Result<Bericht, SammelFehler> {
    let mut bericht = Bericht {
        werte: Vec::new(),
        abbruch: None,
        handelstag: None,
        pruefung: Vec::new(),
    };
    let mut heute: Vec<(depot_kern::Isin, Kurs)> = Vec::new();
    let mut vortag: Vec<(depot_kern::Isin, Kurs)> = Vec::new();
    let mut letzter_abruf: Option<Zeitpunkt> = None;

    for (i, wert) in werte.iter().enumerate() {
        if bericht.abbruch.is_some() {
            bericht.werte.push((wert.clone(), Ergebnis::NichtAbgerufen));
            continue;
        }
        if i > 0 && !pause.is_zero() {
            std::thread::sleep(pause);
        }
        let roh = match netz.hole(&adresse(wert, schluessel)) {
            Ok(r) => r,
            Err(e) => {
                let text = format!("Abruf: {}", schluessel.bereinige(&e));
                bericht.werte.push((wert.clone(), Ergebnis::Fehler(text)));
                continue;
            }
        };
        let abruf = uhr().ok_or(SammelFehler::Uhr)?;
        let rohdatei = lege_ab(ablage, abruf, wert, &roh).map_err(SammelFehler::Ablage)?;
        if let Some((nur_wert, text)) = hinweis(&roh) {
            let text = format!("Hinweis der Quelle: {}", schluessel.bereinige(&text));
            if !nur_wert {
                bericht.abbruch = Some(text.clone());
            }
            bericht.werte.push((wert.clone(), Ergebnis::Fehler(text)));
            continue;
        }
        let gelesen = String::from_utf8(roh.clone())
            .map_err(|_| "Antwort ist kein UTF-8".to_string())
            .and_then(|t| lies_tageskurse(&t).map_err(|e| format!("Antwort unlesbar: {e:?}")))
            .and_then(|tage| {
                schlusskurse(abruf, vec![(wert.isin.clone(), tage)])
                    .map_err(|e| format!("Schlusskurse: {e:?}"))
            });
        let (h, v) = match gelesen {
            Ok(k) => k,
            Err(text) => {
                bericht.werte.push((wert.clone(), Ergebnis::Fehler(text)));
                continue;
            }
        };
        // Belegen, bevor irgendetwas die Kurse verwendet (R12): erst der Vortag, dann heute.
        for kurs in [v.eintrag(&wert.isin), h.eintrag(&wert.isin)]
            .into_iter()
            .flatten()
        {
            let beleg = KursBeleg::neu(QUELLE, abruf, &roh, wert.isin.clone(), kurs.clone())
                .map_err(|e| SammelFehler::Journal(JournalFehler::Abgelehnt(e)))?;
            journal.belege_kurs(&beleg).map_err(SammelFehler::Journal)?;
        }
        if let Some(k) = h.eintrag(&wert.isin) {
            heute.push((wert.isin.clone(), k.clone()));
        }
        if let Some(k) = v.eintrag(&wert.isin) {
            vortag.push((wert.isin.clone(), k.clone()));
        }
        letzter_abruf = Some(abruf);
        bericht.werte.push((
            wert.clone(),
            Ergebnis::Belegt {
                heute: tag_und_preis(h.eintrag(&wert.isin)),
                vortag: tag_und_preis(v.eintrag(&wert.isin)),
                rohdatei,
            },
        ));
    }

    // Pruefen wie jeden Kurs. Das Alter misst gegen den letzten Abruf; fuer frueher abgerufene
    // Werte ist das strenger, nie milder.
    if let Some(stand) = letzter_abruf {
        let handelstag = heute.iter().map(|(_, k)| k.handelstag()).max();
        bericht.handelstag = handelstag;
        let heute = Kurse::neu(stand, heute).map_err(|e| SammelFehler::Kurse(format!("{e:?}")))?;
        let vortag =
            Kurse::neu(stand, vortag).map_err(|e| SammelFehler::Kurse(format!("{e:?}")))?;
        if let Some(tag) = handelstag {
            bericht.pruefung = pruefe_kurse(
                &Grenzen::standard(),
                &als_universum(werte),
                tag,
                &heute,
                &vortag,
            );
        }
    }
    Ok(bericht)
}
