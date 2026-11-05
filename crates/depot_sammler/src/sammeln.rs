//! Ein Lauf des Sammlers: je Wert abrufen, roh ablegen, lesen, belegen; danach pruefen.
//!
//! Reihenfolge je Wert (R12): Erst liegt die rohe Antwort unter `daten/roh/`, dann steht jeder
//! Kurs daraus im Journal, erst dann wird er verwendet. Was nicht gelesen werden kann, wird nicht
//! belegt; die rohe Antwort bleibt zur Durchsicht liegen.
//!
//! Enthaelt eine Antwort den Schluessel, gleich in welcher Schreibweise, wird sie weder abgelegt
//! noch gelesen, und der Lauf endet (K19-01).
//!
//! Meldet die Quelle statt Kursen einen Hinweis, gilt: Ein Fehler zu genau diesem Kuerzel
//! (`Error Message`) betrifft nur diesen Wert. Jeder andere Hinweis, etwa das erreichte
//! Tageskontingent, beendet den Lauf, damit kein weiterer Abruf das Kontingent belastet.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use depot_journal::{Journal, JournalFehler, KursBeleg};
use depot_kern::{Datum, Grenzen, Kurs, Kurse, Preis, Zeitpunkt, lies_tageskurse, schlusskurse};
use depot_regelwerk::{DatenFehler, waehle_tageswerte};

use crate::liste::{QUELLE, Wert, als_universum};
use crate::netz::Netz;
use crate::schluessel::Schluessel;

/// Was ein Wert ergeben hat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ergebnis {
    /// Belegt: die Schlusskurse des letzten beendeten Tags und des Tags davor, soweit vorhanden,
    /// exakt.
    Belegt {
        heute: Option<(Datum, Preis)>,
        vortag: Option<(Datum, Preis)>,
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
    /// Der Lauf endete aus einem Schutzgrund, etwa weil die Quelle den Schluessel zurueckgab.
    /// Dann ist nichts aus diesem Lauf verwendbar, auch nicht, was vorher belegt wurde (Maus,
    /// Gegenprobe 1 zu #34).
    pub sicherheitsabbruch: bool,
    /// Der juengste Handelstag unter den belegten Schlusskursen.
    pub handelstag: Option<Datum>,
    /// Befunde, an denen der Papiertag ausfiele (`waehle_tageswerte`): zum Abruf, zu einem
    /// gehaltenen Wert, oder kein Wert handelbar. Leer = in Ordnung.
    pub pruefung: Vec<DatenFehler>,
    /// Kandidaten, die heute nicht handelbar sind, mit Grund. Der Papiertag laeuft ohne sie.
    pub ausgelassen: Vec<DatenFehler>,
}

impl Bericht {
    /// Sind die Kurse dieses Laufs verwendbar? Nie nach einem Sicherheitsabbruch. Sonst nur, wenn
    /// Schlusskurse belegt sind und die
    /// Pruefung wie im Papiertag nichts meldet, was den Zyklus ausfallen liesse (K19-02): Pflicht
    /// sind die gehaltenen Werte; ein Kandidat mit Fehler oder ohne Abruf faellt nur aus der
    /// Auswahl des Tages. Belegt bleibt alles, was abgerufen wurde; verwendbar ist es damit
    /// noch nicht.
    pub fn verwendbar(&self) -> bool {
        !self.sicherheitsabbruch && self.handelstag.is_some() && self.pruefung.is_empty()
    }
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

/// Was ein Lauf ueber den Schluessel und die Quelle sagt, fuer das Anbieterprotokoll. `None`,
/// wenn nichts abgerufen wurde.
pub fn einordnen(b: &Bericht) -> Option<depot_journal::anbieter::Ergebnis> {
    use depot_journal::anbieter::Ergebnis as E;
    let fehler: Vec<String> = b
        .werte
        .iter()
        .filter_map(|(_, e)| match e {
            Ergebnis::Fehler(t) => Some(t.to_ascii_lowercase()),
            _ => None,
        })
        .collect();
    if b.werte
        .iter()
        .any(|(_, e)| matches!(e, Ergebnis::Belegt { .. }))
    {
        return Some(E::Ok);
    }
    let abbruch = b
        .abbruch
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if abbruch.contains("rate limit") || abbruch.contains("requests per") {
        return Some(E::Limit);
    }
    if fehler.is_empty() {
        return None;
    }
    if fehler.iter().all(|t| t.contains("apikey")) {
        return Some(E::Abgelehnt);
    }
    if fehler.iter().all(|t| t.starts_with("abruf:")) {
        return Some(E::Netz);
    }
    Some(E::Fehler)
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

/// Legt eine rohe Antwort als `<ablage>/<quelle>/<abruf><zusatz>` ab, etwa
/// `20261104T164000Z_DE0007164600.csv`. Die Datei wird nur neu angelegt, nie ueberschrieben,
/// und vor der Rueckkehr auf den Datentraeger geschrieben.
///
/// # Errors
/// Ordner oder Datei lassen sich nicht anlegen, oder die Datei gibt es schon.
pub(crate) fn lege_ab(
    ablage: &Path,
    quelle: &str,
    abruf: Zeitpunkt,
    zusatz: &str,
    roh: &[u8],
) -> Result<PathBuf, String> {
    let ordner = ablage.join(quelle);
    fs::create_dir_all(&ordner).map_err(|e| format!("{}: {e}", ordner.display()))?;
    let stempel: String = abruf
        .to_string()
        .chars()
        .filter(|c| *c != '-' && *c != ':')
        .collect();
    let pfad = ordner.join(format!("{stempel}{zusatz}"));
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

fn tag_und_preis(k: Option<&Kurs>) -> Option<(Datum, Preis)> {
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
        sicherheitsabbruch: false,
        handelstag: None,
        pruefung: Vec::new(),
        ausgelassen: Vec::new(),
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
        // Gibt die Quelle den Schluessel zurueck, wird die Antwort weder abgelegt noch gelesen,
        // und der Lauf endet: Keine Datei des Rahmens enthaelt den Schluessel (K19-01).
        if schluessel.steht_in(&roh) {
            let text =
                "Die Antwort enthaelt den Schluessel: nicht abgelegt, nicht belegt, Lauf beendet"
                    .to_string();
            bericht.abbruch = Some(text.clone());
            bericht.sicherheitsabbruch = true;
            bericht.werte.push((wert.clone(), Ergebnis::Fehler(text)));
            continue;
        }
        let abruf = uhr().ok_or(SammelFehler::Uhr)?;
        let rohdatei = lege_ab(ablage, QUELLE, abruf, &format!("_{}.csv", wert.isin), &roh)
            .map_err(SammelFehler::Ablage)?;
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
                schlusskurse(abruf, vec![(wert.isin.clone(), wert.boerse, tage)])
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

    if let Some(stand) = letzter_abruf {
        pruefe(&mut bericht, journal, werte, stand, heute, vortag)?;
    }
    Ok(bericht)
}

/// Prueft wie der Papiertag (`waehle_tageswerte`). Das Alter misst gegen den letzten Abruf;
/// fuer frueher abgerufene Werte ist das strenger, nie milder.
fn pruefe(
    bericht: &mut Bericht,
    journal: &Journal,
    werte: &[Wert],
    stand: Zeitpunkt,
    heute: Vec<(depot_kern::Isin, Kurs)>,
    vortag: Vec<(depot_kern::Isin, Kurs)>,
) -> Result<(), SammelFehler> {
    let handelstag = heute.iter().map(|(_, k)| k.handelstag()).max();
    bericht.handelstag = handelstag;
    let heute = Kurse::neu(stand, heute).map_err(|e| SammelFehler::Kurse(format!("{e:?}")))?;
    let vortag = Kurse::neu(stand, vortag).map_err(|e| SammelFehler::Kurse(format!("{e:?}")))?;
    if let Some(tag) = handelstag {
        let wahl = waehle_tageswerte(
            &Grenzen::standard(),
            &als_universum(werte),
            &journal.stand().gehalten(),
            tag,
            &heute,
            &vortag,
        );
        bericht.pruefung = wahl.fehler;
        bericht.ausgelassen = wahl.ausgelassen;
    }
    Ok(())
}
