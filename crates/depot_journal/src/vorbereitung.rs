//! Beobachten ohne Handel. Start, Reservierungen und Antworten sind eigene Journaleintraege
//! und beginnen keinen Papiertag. Ein Start wird nach einem Absturz nie erneut ausgefuehrt.
use depot_kern::json::{self, Json, zitiere};
use depot_kern::{BeraterId, Datum, Isin, Zeitpunkt};
use depot_ki::Buchung;

use crate::kasse::{KiStand, buchung_text, lies_buchung};
use crate::{Prognoserichtung, Weltprognose};

/// Feste Speichergrenze der eingefrorenen Auswahl.
pub const MAX_EREIGNISSE: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VorbereitungsAntwort {
    pub zeit: Zeitpunkt,
    pub berater: BeraterId,
    pub gefragt: bool,
    pub verwertbar: bool,
    pub text: String,
    pub prognose: Option<Weltprognose>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vorbereitung {
    pub zeit: Zeitpunkt,
    pub fuer: Datum,
    /// Genau der vor beiden Antworten eingefrorene Wissensstand.
    pub ereignisse: Vec<String>,
    pub reserviert: Vec<BeraterId>,
    pub antworten: Vec<VorbereitungsAntwort>,
}

impl Vorbereitung {
    pub fn abgeschlossen(&self) -> bool {
        self.antworten.len() == 2
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Eintrag {
    Start {
        zeit: Zeitpunkt,
        fuer: Datum,
        ereignisse: Vec<String>,
    },
    Budget {
        start: Zeitpunkt,
        abdruck: String,
        buchung: Buchung,
    },
    Antwort {
        start: Zeitpunkt,
        antwort: VorbereitungsAntwort,
    },
}

fn wer(b: BeraterId) -> &'static str {
    match b {
        BeraterId::Dachs => "Dachs",
        BeraterId::Maus => "Maus",
    }
}

fn lies_wer(s: &str) -> Option<BeraterId> {
    match s {
        "Dachs" => Some(BeraterId::Dachs),
        "Maus" => Some(BeraterId::Maus),
        _ => None,
    }
}

fn text_ok(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.chars().count() <= max && !s.chars().any(char::is_control)
}

fn prognose_text(p: &Weltprognose) -> String {
    format!(
        "{{\"isin\":{},\"richtung\":{},\"tage\":{},\"nr\":{},\"grund\":{},\"widerlegung\":{}}}",
        zitiere(p.isin.als_str()),
        zitiere(match p.richtung {
            Prognoserichtung::Steigt => "steigt",
            Prognoserichtung::Faellt => "faellt",
        }),
        p.handelstage,
        p.ereignis_nr,
        zitiere(&p.begruendung),
        zitiere(&p.widerlegung)
    )
}

fn prognose_lies(p: &Json) -> Option<Weltprognose> {
    let p = Weltprognose {
        isin: Isin::neu(p.feld("isin")?.text()?).ok()?,
        richtung: Prognoserichtung::lies(p.feld("richtung")?.text()?)?,
        handelstage: u8::try_from(p.feld("tage")?.ganz()?).ok()?,
        ereignis_nr: usize::try_from(p.feld("nr")?.ganz()?).ok()?,
        begruendung: p.feld("grund")?.text()?.into(),
        widerlegung: p.feld("widerlegung")?.text()?.into(),
    };
    p.gueltig().then_some(p)
}

impl Eintrag {
    pub(crate) fn text(&self) -> String {
        match self {
            Self::Start {
                zeit,
                fuer,
                ereignisse,
            } => format!(
                "{{\"art\":\"start\",\"zeit\":{},\"fuer\":{},\"ereignisse\":[{}]}}",
                zeit.sekunden(),
                zitiere(&fuer.to_string()),
                ereignisse
                    .iter()
                    .map(|e| zitiere(e))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::Budget {
                start,
                abdruck,
                buchung,
            } => format!(
                "{{\"art\":\"budget\",\"start\":{},\"abdruck\":{},\"buchung\":{}}}",
                start.sekunden(),
                zitiere(abdruck),
                zitiere(&buchung_text(buchung))
            ),
            Self::Antwort { start, antwort: a } => format!(
                "{{\"art\":\"antwort\",\"start\":{},\"zeit\":{},\"berater\":{},\"gefragt\":{},\"verwertbar\":{},\"text\":{},\"prognose\":{}}}",
                start.sekunden(),
                a.zeit.sekunden(),
                zitiere(wer(a.berater)),
                a.gefragt,
                a.verwertbar,
                zitiere(&a.text),
                a.prognose
                    .as_ref()
                    .map_or_else(|| "null".into(), prognose_text)
            ),
        }
    }

    pub(crate) fn lies(text: &str) -> Option<Self> {
        let j = json::lies(text).ok()?;
        let zeit = |n| Zeitpunkt::aus_sekunden(j.feld(n)?.ganz()?);
        let e = match j.feld("art")?.text()? {
            "start" => Self::Start {
                zeit: zeit("zeit")?,
                fuer: Datum::lies(j.feld("fuer")?.text()?)?,
                ereignisse: j
                    .feld("ereignisse")?
                    .liste()?
                    .iter()
                    .map(|e| e.text().map(str::to_string))
                    .collect::<Option<_>>()?,
            },
            "budget" => Self::Budget {
                start: zeit("start")?,
                abdruck: j.feld("abdruck")?.text()?.into(),
                buchung: lies_buchung(j.feld("buchung")?.text()?)?,
            },
            "antwort" => {
                let p = j.feld("prognose")?;
                Self::Antwort {
                    start: zeit("start")?,
                    antwort: VorbereitungsAntwort {
                        zeit: zeit("zeit")?,
                        berater: lies_wer(j.feld("berater")?.text()?)?,
                        gefragt: j.feld("gefragt")?.wahr()?,
                        verwertbar: j.feld("verwertbar")?.wahr()?,
                        text: j.feld("text")?.text()?.into(),
                        prognose: if *p == Json::Null {
                            None
                        } else {
                            Some(prognose_lies(p)?)
                        },
                    },
                }
            }
            _ => return None,
        };
        (e.text() == text).then_some(e)
    }
}

/// Prueft auf einer Kopie, damit ein fehlerhafter Eintrag nichts veraendert.
pub(crate) fn anwenden(
    alle: &mut Vec<Vorbereitung>,
    ki: &mut KiStand,
    text: &str,
) -> Result<(), String> {
    let e = Eintrag::lies(text).ok_or("Vorbereitung unlesbar")?;
    match e {
        Eintrag::Start {
            zeit,
            fuer,
            ereignisse,
        } => {
            let tag = zeit.datum().ok_or("Datum fehlt")?;
            let naechster = (1..=31)
                .filter_map(|n| tag.plus_tage(n))
                .find(|d| depot_kern::liste::handelstag(*d));
            if depot_kern::liste::handelstag(tag)
                || naechster != Some(fuer)
                || alle
                    .last()
                    .is_some_and(|a| a.zeit.datum().is_none_or(|d| d >= tag))
                || ereignisse.is_empty()
                || ereignisse.len() > MAX_EREIGNISSE
                || ereignisse.iter().any(|e| !text_ok(e, 8_000))
                || ereignisse.iter().map(String::len).sum::<usize>() > 64_000
            {
                return Err("Vorbereitungsstart unzulaessig".into());
            }
            alle.push(Vorbereitung {
                zeit,
                fuer,
                ereignisse,
                reserviert: Vec::new(),
                antworten: Vec::new(),
            });
        }
        Eintrag::Budget {
            start,
            abdruck,
            buchung,
        } => {
            let p = alle
                .last_mut()
                .filter(|p| p.zeit == start)
                .ok_or("Vorbereitungsstart fehlt")?;
            let tag = start.datum().ok_or("Datum fehlt")?;
            let zyklus =
                u32::try_from(start.sekunden().div_euclid(86_400)).map_err(|_| "Zyklus")?;
            if buchung.datum != tag
                || buchung.zyklus != zyklus
                || p.reserviert.contains(&buchung.berater)
                || p.antworten.iter().any(|a| a.berater == buchung.berater)
            {
                return Err("Vorbereitungsreservierung unzulaessig".into());
            }
            let mut neu = ki.clone();
            neu.abdruck(&abdruck)?;
            neu.buche_fuer(&buchung_text(&buchung), crate::KiZweck::Vorbereitung)?;
            *ki = neu;
            p.reserviert.push(buchung.berater);
        }
        Eintrag::Antwort { start, antwort: a } => {
            let p = alle
                .last_mut()
                .filter(|p| p.zeit == start)
                .ok_or("Vorbereitungsstart fehlt")?;
            if a.zeit < start
                || a.gefragt != p.reserviert.contains(&a.berater)
                || (!a.gefragt && a.verwertbar)
                || (!a.verwertbar && a.prognose.is_some())
                || !text_ok(&a.text, 1_200)
                || p.antworten.iter().any(|alt| alt.berater == a.berater)
                || a.prognose
                    .as_ref()
                    .is_some_and(|w| !w.gueltig() || w.ereignis_nr > p.ereignisse.len())
            {
                return Err("Vorbereitungsantwort unzulaessig".into());
            }
            p.antworten.push(a);
        }
    }
    Ok(())
}
