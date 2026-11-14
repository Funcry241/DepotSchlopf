//! Ein Lauf des Sammlers: je Wert abrufen, roh ablegen, lesen, belegen; danach pruefen.
//!
//! Reihenfolge (R12): Erst liegt die rohe Antwort unter `daten/roh/`, dann steht jeder Kurs
//! daraus im Journal, erst dann wird er verwendet. Was nicht gelesen werden kann, wird nicht
//! belegt; die rohe Antwort bleibt zur Durchsicht liegen. Ins Journal kommen die Kurse erst,
//! wenn der Lauf ohne Sicherheitsabbruch zu Ende ist, in der Reihenfolge ihres Abrufs.
//!
//! Enthaelt eine Antwort den Schluessel, gleich in welcher Schreibweise, wird sie weder abgelegt
//! noch gelesen, und der Lauf endet (K19-01). Dann wird auch kein schon gelesener Kurs dieses
//! Laufs belegt: Ein Papiertag, auch von Hand gestartet, findet keine Teilkurse (Maus,
//! Restbefund zu #34), selbst wenn die Kurssperre sich nicht schreiben laesst.
//!
//! **Quelle je Boerse:** Xetra-Werte kommen von Alpha Vantage, Werte der NYSE von Tiingo, je mit
//! eigenem Schluessel (`Zugang`). Fehlt der Schluessel einer Quelle, werden nur ihre Werte nicht
//! abgerufen. Jede Antwort wird gegen alle Schluessel geprueft.
//!
//! **Dollar in Euro:** Enthaelt der Lauf einen Wert, der nicht in Euro notiert, holt der Sammler
//! (und hat er den Schluessel seiner Quelle) vorher einmal die Referenzkurse der EZB (`ADRESSE_DEVISEN`), legt sie roh unter
//! `daten/roh/ezb-referenzkurse/` ab und rechnet jeden Dollar-Schluss mit dem Kurs desselben
//! Tages um (`depot_kern::schlusskurse_in_euro`). Lassen sie sich nicht holen oder lesen, wird
//! kein Wert in Dollar abgerufen; die Werte in Euro bleiben unberuehrt.
//!
//! **Kurze Netzausfaelle:** Jeder Abruf von Kursen und Referenzkursen geht ueber
//! `netz::hole_mit_wiederholung`: Scheitert er voruebergehend, wird er nach kurzer Wartezeit
//! wiederholt, solange seine Kurse noch frisch genug waeren (`netz::frist`). Bleibt die
//! Verbindung zu einer Quelle weg (`Bericht::verbindung`), ruft der Lauf bei ihr nichts mehr ab;
//! die anderen Quellen fragt er weiter. Fallen die Referenzkurse der EZB aus, fehlen nur die
//! Werte in Dollar; Werte in Euro brauchen sie nicht (Maus, Befund 1 zu #38).
//!
//! Meldet die Quelle statt Kursen einen Hinweis, gilt: Ein Fehler zu genau diesem Kuerzel
//! (`Error Message`) betrifft nur diesen Wert. Jeder andere Hinweis, etwa das erreichte
//! Tageskontingent, beendet den Lauf, damit kein weiterer Abruf das Kontingent belastet.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use depot_journal::{Journal, JournalFehler, KursBeleg};
use depot_kern::{
    Boerse, Datum, Devisen, Grenzen, Kurs, Kurse, Preis, Zeitpunkt, lies_tageskurse,
    schlusskurse_in_euro,
};
use depot_regelwerk::{DatenFehler, waehle_tageswerte};

use crate::liste::{Wert, als_universum};
use crate::netz::{Ausfall, Netz, Takt, frist, hole_mit_autorisierung, hole_mit_wiederholung};
use crate::schluessel::{Anbieter, Schluessel, Zugang};

/// Die Referenzkurse der EZB der letzten 90 Tage.
pub const ADRESSE_DEVISEN: &str =
    "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-hist-90d.xml";
/// Ordner der rohen Referenzkurse unter `daten/roh/`.
pub const ABLAGE_DEVISEN: &str = "ezb-referenzkurse";
/// So viele Tage vor dem Abruf beginnt die Reihe bei Tiingo: genug fuer heute und den Vortag
/// auch ueber Feiertage.
const TAGE_ZURUECK: i64 = 14;

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
    /// Ein vorübergehender Verbindungsfehler der Quelle.
    Verbindung(String),
    /// Nicht abgerufen, mit Grund: Der Schluessel seiner Quelle fehlt, oder er notiert nicht in
    /// Euro und die Referenzkurse fehlen. Zaehlt fuer keine Quelle im Anbieterprotokoll.
    Uebersprungen(String),
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
    pub sperrquelle: Option<depot_journal::kurssperre::Quelle>,
    /// Der juengste Handelstag unter den belegten Schlusskursen.
    pub handelstag: Option<Datum>,
    /// Befunde, an denen der Papiertag ausfiele (`waehle_tageswerte`): zum Abruf, zu einem
    /// gehaltenen Wert, oder kein Wert handelbar. Leer = in Ordnung.
    pub pruefung: Vec<DatenFehler>,
    /// Kandidaten, die heute nicht handelbar sind, mit Grund. Der Papiertag laeuft ohne sie.
    pub ausgelassen: Vec<DatenFehler>,
    /// Die Verbindung zu einer Quelle blieb trotz Wiederholung weg (die erste solche). Bei
    /// dieser Quelle wurde danach nichts mehr abgerufen, bei den anderen schon.
    pub verbindung: Option<Ausfall>,
}

impl Bericht {
    fn beende_abruf(&mut self, wert: &Wert, text: String, sicherheit: bool) {
        self.abbruch = Some(text.clone());
        if sicherheit {
            self.sicherheitsabbruch = true;
            self.sperrquelle = Some(crate::diagnose::sperrquelle(Anbieter::fuer(wert.boerse)));
        }
        self.werte.push((wert.clone(), Ergebnis::Fehler(text)));
    }

    /// Sind ausschließlich vorübergehende Verbindungsfehler aufgetreten?
    pub fn nur_verbindungsfehler(&self) -> bool {
        self.verbindung.is_some()
            && self.abbruch.is_none()
            && !self.sicherheitsabbruch
            && self.werte.iter().all(|(_, e)| match e {
                Ergebnis::Fehler(_) => false,
                Ergebnis::Uebersprungen(grund) => {
                    grund.starts_with("Verbindung zu ")
                        || self.verbindung.as_ref().is_some_and(|a| {
                            grund.starts_with("Abruf: ") && grund.ends_with(a.fehler())
                        })
                }
                _ => true,
            })
    }

    /// Sind die Kurse dieses Laufs verwendbar? Nie nach einem Sicherheitsabbruch. Sonst nur, wenn
    /// Schlusskurse belegt sind und die
    /// Pruefung wie im Papiertag nichts meldet, was den Zyklus ausfallen liesse (K19-02): Pflicht
    /// sind die gehaltenen Werte; ein Kandidat mit Fehler oder ohne Abruf faellt nur aus der
    /// Auswahl des Tages. Die Belegung eines Kurses und seine Handelbarkeit sind getrennt.
    /// Ein NYSE-Schluss ohne belegbaren Vergleichskurs bleibt im Rohabruf.
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

/// Die Abrufadresse eines Werts bei seiner Quelle (`Anbieter::fuer`), als CSV:
/// - Alpha Vantage: Tageskurse der letzten 100 Handelstage.
/// - Tiingo: Tageskurse ab `ab` (bei `None` nur der letzte Tag), nur Datum, Schluss, Tief und
///   Hoch. Das Kuerzel steht klein, eine Klasse mit Bindestrich (`BRK.B` wird `brk-b`).
pub fn adresse(wert: &Wert, schluessel: &Schluessel, ab: Option<Datum>) -> String {
    match Anbieter::fuer(wert.boerse) {
        Anbieter::AlphaVantage => format!(
            "https://www.alphavantage.co/query?function=TIME_SERIES_DAILY&symbol={}&outputsize=compact&datatype=csv&apikey={}",
            wert.kuerzel,
            schluessel.als_str()
        ),
        Anbieter::Tiingo => {
            let kuerzel = wert.kuerzel.to_ascii_lowercase().replace('.', "-");
            let start = ab.map(|d| format!("startDate={d}&")).unwrap_or_default();
            format!(
                "https://api.tiingo.com/tiingo/daily/{kuerzel}/prices?{start}format=csv&columns=date,close,high,low"
            )
        }
    }
}

pub(crate) fn autorisierung(anbieter: Anbieter, schluessel: &Schluessel) -> Option<String> {
    (anbieter == Anbieter::Tiingo).then(|| format!("Token {}", schluessel.als_str()))
}

/// Was ein Lauf ueber den Schluessel und eine Quelle sagt, fuer das Anbieterprotokoll. Gezaehlt
/// werden nur die Werte dieser Quelle, die abgerufen wurden (Maus, Befund 2 zu #37): Ein Erfolg
/// der einen Quelle sagt nichts ueber den Schluessel der anderen. `None`, wenn bei ihr nichts
/// abgerufen wurde.
pub fn einordnen(b: &Bericht, quelle: Anbieter) -> Option<depot_journal::anbieter::Ergebnis> {
    use depot_journal::anbieter::Ergebnis as E;
    let eigene: Vec<&Ergebnis> = b
        .werte
        .iter()
        .filter(|(w, _)| Anbieter::fuer(w.boerse) == quelle)
        .map(|(_, e)| e)
        .collect();
    if eigene
        .iter()
        .any(|e| matches!(e, Ergebnis::Fehler(t) if t == crate::diagnose::ALPHA_LIMIT))
    {
        return Some(E::Limit);
    }
    if eigene.iter().any(|e| matches!(e, Ergebnis::Belegt { .. })) {
        return Some(E::Ok);
    }
    let fehler: Vec<String> = eigene
        .iter()
        .filter_map(|e| match e {
            Ergebnis::Fehler(t) | Ergebnis::Verbindung(t) => Some(t.to_ascii_lowercase()),
            _ => None,
        })
        .collect();
    if fehler.is_empty() {
        return None;
    }
    if fehler.iter().any(|t| {
        t == "abruf: http status: 429" || t.contains("rate limit") || t.contains("requests per")
    }) {
        return Some(E::Limit);
    }
    if fehler
        .iter()
        .all(|t| t == "abruf: http status: 401" || t.contains("apikey"))
    {
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

/// Die Referenzkurse eines Laufs: gelesen, oder warum nicht.
struct Referenz {
    devisen: Devisen,
    fehler: Option<String>,
}

/// Holt die Referenzkurse der EZB, wenn ein Wert des Laufs nicht in Euro notiert. `Ok(None)`,
/// wenn keiner das tut. Gibt die Antwort einen Schluessel zurueck, ist das ein
/// Sicherheitsabbruch wie bei jeder Quelle.
fn hole_devisen(
    bericht: &mut Bericht,
    netz: &mut dyn Netz,
    zugang: &Zugang,
    beginn: Zeitpunkt,
    ablage: &Path,
    (werte, takt): (&[Wert], &mut Takt<'_>),
) -> Result<Referenz, SammelFehler> {
    let fehlt = |text: String| Referenz {
        devisen: Devisen::keine(),
        fehler: Some(format!(
            "Referenzkurse der EZB: {text}; Wert in Dollar nicht abgerufen"
        )),
    };
    // Frist der fruehesten Boerse, deren Werte die Referenzkurse brauchen (nicht in Euro):
    // Spaeter nuetzten sie keinem Wert. Werte in Euro verkuerzen die Frist nicht.
    let frist_ab = |jetzt: Zeitpunkt| {
        werte
            .iter()
            .filter(|w| w.boerse.waehrung() != "EUR")
            .filter_map(|w| frist(w.boerse, &Grenzen::standard(), jetzt))
            .min()
    };
    let roh = match hole_mit_wiederholung(netz, ADRESSE_DEVISEN, &frist_ab, takt) {
        Ok(r) => r,
        Err(a) => {
            let text = format!("Abruf: {}", zugang.bereinige(a.fehler()));
            if a.verbindung() && bericht.verbindung.is_none() {
                bericht.verbindung = Some(a);
            }
            return Ok(fehlt(text));
        }
    };
    if let Some(text) = crate::diagnose::sicherheitsabbruch(zugang, "ezb", &roh) {
        bericht.abbruch = Some(text);
        bericht.sicherheitsabbruch = true;
        bericht.sperrquelle = Some(depot_journal::kurssperre::Quelle::Ezb);
        return Ok(fehlt("Sicherheitsabbruch".into()));
    }
    lege_ab(ablage, ABLAGE_DEVISEN, beginn, ".xml", &roh).map_err(SammelFehler::Ablage)?;
    let gelesen = String::from_utf8(roh)
        .map_err(|_| "Antwort ist kein UTF-8".to_string())
        .and_then(|t| Devisen::lies_ezb(&t).map_err(|e| format!("unlesbar: {e:?}")));
    Ok(match gelesen {
        Ok(devisen) => Referenz {
            devisen,
            fehler: None,
        },
        Err(text) => fehlt(text),
    })
}

/// Was vor dem ersten Wert feststeht: Referenzkurse, Beginn der Reihe bei Tiingo, und ob schon
/// abgerufen wurde.
struct Vorbereitung {
    referenz: Referenz,
    ab: Option<Datum>,
    abgerufen: bool,
}

/// Enthaelt der Lauf einen Wert ausserhalb von Xetra, fragt sie die Uhr einmal: fuer den Beginn
/// der Reihe bei Tiingo und, wenn ein Wert in Dollar abgerufen werden kann, fuer die Ablage der
/// Referenzkurse, die sie dann holt.
fn vorbereite(
    bericht: &mut Bericht,
    netz: &mut dyn Netz,
    zugang: &Zugang,
    takt: &mut Takt<'_>,
    ablage: &Path,
    werte: &[Wert],
) -> Result<Vorbereitung, SammelFehler> {
    let mut v = Vorbereitung {
        referenz: Referenz {
            devisen: Devisen::keine(),
            fehler: None,
        },
        ab: None,
        abgerufen: false,
    };
    if werte.iter().all(|w| w.boerse == Boerse::Xetra) {
        return Ok(v);
    }
    let beginn = (takt.uhr)().ok_or(SammelFehler::Uhr)?;
    v.ab = beginn.datum().and_then(|d| d.plus_tage(-TAGE_ZURUECK));
    if werte
        .iter()
        .any(|w| w.boerse.waehrung() != "EUR" && zugang.von(Anbieter::fuer(w.boerse)).is_some())
    {
        v.referenz = hole_devisen(bericht, netz, zugang, beginn, ablage, (werte, takt))?;
        v.abgerufen = true;
    }
    Ok(v)
}

/// Der Schluessel fuer einen Wert, oder warum er nicht abgerufen wird: Es fehlt der Schluessel
/// seiner Quelle, oder er notiert nicht in Euro und die Referenzkurse fehlen.
fn abrufbar<'a>(
    zugang: &'a Zugang,
    r: &Referenz,
    wert: &Wert,
    weg: &[Anbieter],
) -> Result<&'a Schluessel, String> {
    let anbieter = Anbieter::fuer(wert.boerse);
    if weg.contains(&anbieter) {
        return Err(format!(
            "Verbindung zu {} weg: nicht abgerufen",
            anbieter.dienst()
        ));
    }
    let schluessel = zugang.von(anbieter).ok_or_else(|| {
        format!(
            "Kein Schlüssel für {} ({}): nicht abgerufen",
            anbieter.dienst(),
            anbieter.adresse()
        )
    })?;
    match &r.fehler {
        Some(text) if wert.boerse.waehrung() != "EUR" => Err(text.clone()),
        _ => Ok(schluessel),
    }
}

/// Was der Abruf eines Werts ergeben hat.
enum Schritt {
    /// Gelesen: Abrufzeit, rohe Antwort, Ablage und die Schlusskurse.
    Gelesen {
        abruf: Zeitpunkt,
        roh: Vec<u8>,
        rohdatei: PathBuf,
        normalisierungen: Vec<(Datum, String)>,
        heute: Kurse,
        vortag: Kurse,
    },
    /// Nur dieser Wert ergibt nichts.
    Fehler(String),
    AlphaLimit,
    /// Der Lauf endet; `sicherheit`, wenn die Antwort einen Schluessel enthielt.
    Abbruch {
        text: String,
        sicherheit: bool,
    },
    /// Die Verbindung zur Quelle blieb trotz Wiederholung weg; bei ihr wird nichts mehr
    /// abgerufen.
    Verbindung {
        text: String,
        ausfall: Ausfall,
    },
}

/// Holt, prueft, legt ab und liest einen Wert.
///
/// # Errors
/// Uhr oder Ablage versagen.
fn lies_wert(
    netz: &mut dyn Netz,
    zugang: &Zugang,
    schluessel: &Schluessel,
    wert: &Wert,
    v: &Vorbereitung,
    takt: &mut Takt<'_>,
    ablage: &Path,
) -> Result<Schritt, SammelFehler> {
    let devisen = &v.referenz.devisen;
    let frist_ab = |jetzt: Zeitpunkt| frist(wert.boerse, &Grenzen::standard(), jetzt);
    let autorisiert = autorisierung(Anbieter::fuer(wert.boerse), schluessel);
    let roh = match hole_mit_autorisierung(
        netz,
        &adresse(wert, schluessel, v.ab),
        autorisiert.as_deref(),
        &frist_ab,
        takt,
    ) {
        Ok(r) => r,
        Err(a) => {
            let text = format!("Abruf: {}", zugang.bereinige(a.fehler()));
            return Ok(if a.verbindung() {
                Schritt::Verbindung { text, ausfall: a }
            } else {
                Schritt::Fehler(text)
            });
        }
    };
    let anbieter = Anbieter::fuer(wert.boerse);
    if crate::diagnose::alpha_limit(zugang, anbieter, &roh) {
        return Ok(Schritt::AlphaLimit);
    }
    // Gibt die Quelle einen Schluessel zurueck, wird die Antwort weder abgelegt noch gelesen,
    // und der Lauf endet: Keine Datei des Rahmens enthaelt ihn (K19-01).
    if let Some(text) = crate::diagnose::sicherheitsabbruch(zugang, anbieter.dienst(), &roh) {
        return Ok(Schritt::Abbruch {
            text,
            sicherheit: true,
        });
    }
    let abruf = (takt.uhr)().ok_or(SammelFehler::Uhr)?;
    let quelle = anbieter.dienst();
    let rohdatei = lege_ab(ablage, quelle, abruf, &format!("_{}.csv", wert.isin), &roh)
        .map_err(SammelFehler::Ablage)?;
    if let Some((nur_wert, text)) = hinweis(&roh) {
        let text = format!("Hinweis der Quelle: {}", zugang.bereinige(&text));
        return Ok(if nur_wert {
            Schritt::Fehler(text)
        } else {
            Schritt::Abbruch {
                text,
                sicherheit: false,
            }
        });
    }
    let mut normalisierungen = Vec::new();
    let gelesen = String::from_utf8(roh.clone())
        .map_err(|_| "Antwort ist kein UTF-8".to_string())
        .and_then(|t| {
            if wert.boerse != Boerse::Nyse {
                return lies_tageskurse(&t).map_err(|e| format!("Antwort unlesbar: {e:?}"));
            }
            let gelesen = depot_kern::tiingo::lies_tageskurse(&t)
                .map_err(|e| format!("Antwort unlesbar: {e:?}"))?;
            for r in gelesen.rundungen {
                normalisierungen.push((r.datum, format!(
                    "Tiingo-Normalisierung: isin={}; abruf={abruf}; roh={}; zeile={}; feld={}; original={}; nach-millionstel-usd={}; rundung=halb-gerade; abweichung-hoechstens=0.000000001-USD",
                    wert.isin, depot_kern::abdruck::sha256_hex(&roh), r.zeile, r.feld, r.original, r.preis.fein()
                )));
            }
            Ok(gelesen.kurse)
        })
        .and_then(|tage| {
            schlusskurse_in_euro(abruf, vec![(wert.isin.clone(), wert.boerse, tage)], devisen)
                .map_err(|e| format!("Schlusskurse: {e:?}"))
        });
    Ok(match gelesen {
        Ok((heute, vortag)) => {
            // Nur tatsächlich verwendete Tage belegen, nicht die ganze Abrufhistorie.
            normalisierungen.retain(|(tag, _)| {
                [&heute, &vortag].iter().any(|k| {
                    k.eintrag(&wert.isin)
                        .is_some_and(|kurs| kurs.handelstag() == *tag)
                })
            });
            Schritt::Gelesen {
                abruf,
                roh,
                rohdatei,
                normalisierungen,
                heute,
                vortag,
            }
        }
        Err(text) => Schritt::Fehler(text),
    })
}

/// Ein Lauf ueber alle Werte. `takt` liefert die Zeit des Rechners in UTC, wartet und meldet
/// Wiederholungen; `pause` liegt zwischen zwei Abrufen. Enthaelt `werte` einen Wert ausserhalb von Xetra, fragt der Lauf die
/// Uhr einmal vorher: fuer die Ablage der Referenzkurse und den Beginn der Reihe bei Tiingo.
///
/// # Errors
/// Journal, Ablage oder Uhr versagen. Was bis dahin belegt ist, bleibt belegt; vor dem Ende
/// der Abrufe ist das nichts.
pub fn sammle(
    journal: &mut Journal,
    netz: &mut dyn Netz,
    zugang: &Zugang,
    takt: &mut Takt<'_>,
    ablage: &Path,
    pause: Duration,
    werte: &[Wert],
) -> Result<Bericht, SammelFehler> {
    let mut bericht = Bericht {
        werte: Vec::new(),
        abbruch: None,
        sicherheitsabbruch: false,
        sperrquelle: None,
        handelstag: None,
        pruefung: Vec::new(),
        ausgelassen: Vec::new(),
        verbindung: None,
    };
    let mut gelesen = Gelesen::default();

    let vorbereitung = vorbereite(&mut bericht, netz, zugang, takt, ablage, werte)?;
    let mut abgerufen = vorbereitung.abgerufen;
    // Quellen, zu denen die Verbindung trotz Wiederholung wegblieb.
    let mut weg: Vec<Anbieter> = Vec::new();

    let alpha_anzahl = werte.iter().filter(|w| w.boerse == Boerse::Xetra).count();
    let alpha_grund = (alpha_anzahl > 0 && zugang.von(Anbieter::AlphaVantage).is_some())
        .then(|| netz.pruefe_alpha(alpha_anzahl).err())
        .flatten();
    let mut alpha_limit = false;
    for wert in werte {
        if bericht.abbruch.is_some() {
            bericht.werte.push((wert.clone(), Ergebnis::NichtAbgerufen));
            continue;
        }
        if wert.boerse == Boerse::Xetra
            && let Some(grund) = &alpha_grund
        {
            bericht
                .werte
                .push((wert.clone(), Ergebnis::Uebersprungen(grund.clone())));
            continue;
        }
        if alpha_limit && Anbieter::fuer(wert.boerse) == Anbieter::AlphaVantage {
            bericht.werte.push((wert.clone(), Ergebnis::NichtAbgerufen));
            continue;
        }
        let schluessel = match abrufbar(zugang, &vorbereitung.referenz, wert, &weg) {
            Ok(s) => s,
            Err(text) => {
                bericht
                    .werte
                    .push((wert.clone(), Ergebnis::Uebersprungen(text)));
                continue;
            }
        };
        if abgerufen && !pause.is_zero() {
            (takt.schlafe)(pause);
        }
        abgerufen = true;
        let (abruf, roh, rohdatei, h, v, notizen) =
            match lies_wert(netz, zugang, schluessel, wert, &vorbereitung, takt, ablage)? {
                Schritt::Gelesen {
                    abruf,
                    roh,
                    rohdatei,
                    normalisierungen,
                    heute,
                    vortag,
                } => (abruf, roh, rohdatei, heute, vortag, normalisierungen),
                Schritt::Fehler(text) => {
                    bericht.werte.push((wert.clone(), Ergebnis::Fehler(text)));
                    continue;
                }
                Schritt::AlphaLimit => {
                    alpha_limit = true;
                    bericht.werte.push((
                        wert.clone(),
                        Ergebnis::Fehler(crate::diagnose::ALPHA_LIMIT.into()),
                    ));
                    continue;
                }
                Schritt::Abbruch { text, sicherheit } => {
                    bericht.beende_abruf(wert, text, sicherheit);
                    continue;
                }
                Schritt::Verbindung { text, ausfall } => {
                    weg.push(Anbieter::fuer(wert.boerse));
                    if bericht.verbindung.is_none() {
                        bericht.verbindung = Some(ausfall);
                    }
                    bericht
                        .werte
                        .push((wert.clone(), Ergebnis::Verbindung(text)));
                    continue;
                }
            };
        gelesen.normalisierungen.extend(notizen);
        gelesen.belege(wert, abruf, &roh, &h, &v)?;
        let ergebnis = if fehlt_nyse_vergleich(wert, &h, &v) {
            Ergebnis::Fehler("Nicht belegt: kein NYSE-Vergleichskurs mit EZB-Referenzkurs".into())
        } else {
            Ergebnis::Belegt {
                heute: tag_und_preis(h.eintrag(&wert.isin)),
                vortag: tag_und_preis(v.eintrag(&wert.isin)),
                rohdatei,
            }
        };
        bericht.werte.push((wert.clone(), ergebnis));
    }

    abschliessen(bericht, journal, werte, gelesen)
}

fn fehlt_nyse_vergleich(wert: &Wert, heute: &Kurse, vortag: &Kurse) -> bool {
    wert.boerse == Boerse::Nyse
        && heute.eintrag(&wert.isin).is_some()
        && vortag.eintrag(&wert.isin).is_none()
}

/// Was ein Lauf gelesen, aber noch nicht belegt hat.
#[derive(Default)]
struct Gelesen {
    normalisierungen: Vec<(Datum, String)>,
    offen: Vec<KursBeleg>,
    letzter_abruf: Option<Zeitpunkt>,
    heute: Vec<(depot_kern::Isin, Kurs, Zeitpunkt)>,
    vortag: Vec<(depot_kern::Isin, Kurs)>,
}

impl Gelesen {
    fn belege(
        &mut self,
        wert: &Wert,
        abruf: Zeitpunkt,
        roh: &[u8],
        heute: &Kurse,
        vortag: &Kurse,
    ) -> Result<(), SammelFehler> {
        let quelle = Anbieter::fuer(wert.boerse).dienst();
        // Erst den Vortag, dann den aktuellen Kurs zur Belegung vormerken.
        for kurs in [vortag.eintrag(&wert.isin), heute.eintrag(&wert.isin)]
            .into_iter()
            .flatten()
            .filter(|_| !fehlt_nyse_vergleich(wert, heute, vortag))
        {
            let beleg = KursBeleg::neu(quelle, abruf, roh, wert.isin.clone(), kurs.clone())
                .map_err(|e| SammelFehler::Journal(JournalFehler::Abgelehnt(e)))?;
            self.offen.push(beleg);
        }
        if let Some(k) = heute.eintrag(&wert.isin) {
            self.heute.push((wert.isin.clone(), k.clone(), abruf));
        }
        if let Some(k) = vortag.eintrag(&wert.isin) {
            self.vortag.push((wert.isin.clone(), k.clone()));
        }
        self.letzter_abruf = Some(abruf);
        Ok(())
    }
}

/// Nach den Abrufen: ohne Sicherheitsabbruch alles in Abrufreihenfolge belegen und pruefen;
/// nach einem Sicherheitsabbruch nichts belegen.
fn abschliessen(
    mut bericht: Bericht,
    journal: &mut Journal,
    werte: &[Wert],
    g: Gelesen,
) -> Result<Bericht, SammelFehler> {
    if bericht.sicherheitsabbruch {
        for (_, e) in &mut bericht.werte {
            if matches!(e, Ergebnis::Belegt { .. }) {
                *e = Ergebnis::Fehler(
                    "Nicht belegt: Sicherheitsabbruch des Laufs, kein Kurs dieses Abrufs zählt"
                        .into(),
                );
            }
        }
        return Ok(bericht);
    }
    for (datum, text) in &g.normalisierungen {
        journal
            .beobachte(*datum, text)
            .map_err(SammelFehler::Journal)?;
    }
    for beleg in &g.offen {
        journal.belege_kurs(beleg).map_err(SammelFehler::Journal)?;
    }
    if let Some(stand) = g.letzter_abruf {
        pruefe(&mut bericht, journal, werte, stand, g.heute, g.vortag)?;
    }
    Ok(bericht)
}

/// Prueft wie der Papiertag (`waehle_tageswerte`), aber nur die Werte dieses Laufs: Ein Lauf
/// fuer eine Boerse prueft die gehaltenen Werte dieser Boerse; alle zusammen prueft der
/// Papiertag. Das Alter misst je Wert an seinem eigenen Abruf, wie im Papiertag.
fn pruefe(
    bericht: &mut Bericht,
    journal: &Journal,
    werte: &[Wert],
    stand: Zeitpunkt,
    heute: Vec<(depot_kern::Isin, Kurs, Zeitpunkt)>,
    vortag: Vec<(depot_kern::Isin, Kurs)>,
) -> Result<(), SammelFehler> {
    let handelstag = heute.iter().map(|(_, k, _)| k.handelstag()).max();
    bericht.handelstag = handelstag;
    let heute =
        Kurse::mit_abrufen(stand, heute).map_err(|e| SammelFehler::Kurse(format!("{e:?}")))?;
    let vortag = Kurse::neu(stand, vortag).map_err(|e| SammelFehler::Kurse(format!("{e:?}")))?;
    if let Some(tag) = handelstag {
        let universum = als_universum(werte);
        let gehalten = journal
            .stand()
            .gehalten()
            .into_iter()
            .filter(|i| werte.iter().any(|w| w.isin == *i))
            .collect();
        let grenzen = Grenzen::standard();
        let wahl = if depot_kern::liste::morgenlauf(stand) {
            depot_regelwerk::waehle_morgenwerte(
                &grenzen, &universum, &gehalten, stand, &heute, &vortag,
            )
        } else {
            waehle_tageswerte(&grenzen, &universum, &gehalten, tag, &heute, &vortag)
        };
        bericht.pruefung = wahl.fehler;
        // Ohne Vergleichskurs kann der Papiertag den gesperrten Bestand nicht bewerten.
        bericht.pruefung.extend(
            wahl.gesperrt
                .iter()
                .filter(|f| f.isin().is_some_and(|i| vortag.eintrag(i).is_none()))
                .cloned(),
        );
        bericht.ausgelassen = wahl.ausgelassen;
        bericht.ausgelassen.extend(wahl.gesperrt);
    }
    Ok(())
}
