//! Modell und Ergebnis eines KI-Schritts, dem tatsächlichen Lauf zugeordnet.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::SystemTime;

use depot_kern::modelle::{MODELL_DACHS, MODELL_MAUS};
use depot_kern::{Datum, Zeitpunkt};
use depot_lage::gespraeche::{self, Gespraech};
use depot_lage::html::maskiere;

use crate::gespraeche::{name, status};

#[derive(Default)]
pub(crate) struct Aufrufe {
    je_lauf: BTreeMap<String, BTreeMap<Beteiligung, BTreeSet<String>>>,
    handzeiten: BTreeMap<String, Vec<Zeitpunkt>>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Beteiligung {
    tag: Datum,
    wer: String,
    modell: String,
    optional: bool,
    gefragt: bool,
}

struct Kurz {
    zeit: Zeitpunkt,
    lauf: String,
    wer: String,
    modell: String,
    optional: bool,
    ergebnis: String,
}

impl From<Gespraech> for Kurz {
    fn from(e: Gespraech) -> Self {
        Self {
            zeit: e.zeit,
            lauf: e.lauf,
            wer: e.wer,
            modell: e.modell,
            optional: e.rolle == "zusatz",
            ergebnis: e.ergebnis,
        }
    }
}

type Dateistand = (u64, SystemTime);
type Tagescache = BTreeMap<Datum, (Dateistand, Arc<Vec<Kurz>>)>;

#[derive(Default)]
pub(crate) struct Cache(Mutex<Tagescache>);

fn dateistand(pfad: &Path) -> Option<Dateistand> {
    let meta = fs::symlink_metadata(pfad).ok()?;
    if !meta.is_file() {
        return None;
    }
    Some((meta.len(), meta.modified().ok()?))
}

impl Cache {
    fn tag(
        &self,
        daten: &Path,
        tag: Datum,
        lesen: impl FnOnce() -> io::Result<Vec<Kurz>>,
    ) -> Option<Arc<Vec<Kurz>>> {
        let mut cache = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let pfad = daten.join("gespraeche").join(format!("{tag}.jsonl"));
        let vorher = dateistand(&pfad);
        if let Some((stand, eintraege)) = cache.get(&tag)
            && vorher == Some(*stand)
        {
            return Some(Arc::clone(eintraege));
        }
        cache.remove(&tag);
        let eintraege = Arc::new(lesen().ok()?);
        if let Some(stand) = vorher
            && Some(stand) == dateistand(&pfad)
        {
            cache.insert(tag, (stand, Arc::clone(&eintraege)));
        }
        Some(eintraege)
    }
}

impl Aufrufe {
    pub(crate) fn lesen(cache: &Cache, daten: &Path, heute: Option<Datum>) -> Self {
        let gestern = heute.and_then(|d| d.plus_tage(-1));
        cache
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|tag, _| Some(*tag) == heute || Some(*tag) == gestern);
        let mut aus = Self::default();
        for tag in heute.into_iter().chain(gestern) {
            let Some(eintraege) = cache.tag(daten, tag, || {
                gespraeche::lesen(daten, tag).map(|(e, _)| e.into_iter().map(Kurz::from).collect())
            }) else {
                continue;
            };
            for e in eintraege.iter() {
                if e.lauf.starts_with("hand-") {
                    aus.handzeiten
                        .entry(e.lauf.clone())
                        .or_default()
                        .push(e.zeit);
                }
                let modell = Beteiligung {
                    tag,
                    wer: e.wer.clone(),
                    modell: e.modell.clone(),
                    optional: e.optional,
                    gefragt: e.ergebnis != "uebersprungen",
                };
                aus.je_lauf
                    .entry(e.lauf.clone())
                    .or_default()
                    .entry(modell)
                    .or_default()
                    .insert(e.ergebnis.clone());
            }
        }
        aus
    }

    pub(crate) fn hand_kennung(
        &self,
        dienstlauf: &str,
        von: Zeitpunkt,
        bis: Zeitpunkt,
    ) -> Option<&str> {
        if self.je_lauf.contains_key(dienstlauf) {
            return None;
        }
        if !bis
            .sekunden_seit(von)
            .is_some_and(|dauer| (0..=7_200).contains(&dauer))
        {
            return None;
        }
        let mut passend = self
            .handzeiten
            .iter()
            .filter(|(_, zeiten)| zeiten.iter().all(|zeit| (von..=bis).contains(zeit)));
        let (lauf, _) = passend.next()?;
        passend.next().is_none().then_some(lauf.as_str())
    }

    pub(crate) fn gelaufen(&self, s: &mut String, lauf: Option<&str>, hand: Option<&str>) {
        let exakt = lauf.and_then(|id| self.je_lauf.get(id).map(|m| (id, m)));
        let gefunden = exakt.or_else(|| hand.and_then(|id| self.je_lauf.get(id).map(|m| (id, m))));
        let Some((lauf, modelle)) = gefunden else {
            s.push_str(" <span class=\"leise\">Modell nicht protokolliert</span>");
            return;
        };
        if exakt.is_none() {
            s.push_str(" <span class=\"leise\">Handlauf, nach Uhrzeit zugeordnet</span>");
        }
        for (m, ergebnisse) in modelle {
            let ergebnis = ergebnisse
                .iter()
                .map(|e| status(e))
                .collect::<Vec<_>>()
                .join(", ");
            let modell = if m.gefragt {
                m.modell.as_str()
            } else {
                "kein Modell aufgerufen"
            };
            let optional = if m.optional { " · optional" } else { "" };
            let klasse = if m.optional {
                " class=\"optional\""
            } else {
                ""
            };
            let _ = write!(
                s,
                " <span{klasse}><a href=\"/gespraeche?tag={}&amp;wer={}&amp;lauf={}\">{} ({})</a> · {}{optional}</span>",
                m.tag,
                abfragewert(&m.wer),
                abfragewert(lauf),
                maskiere(name(&m.wer)),
                maskiere(modell),
                maskiere(&ergebnis),
            );
        }
    }

    pub(crate) fn konsolenlaeufe(&self, s: &mut String, tag: Datum, zugeordnet: &BTreeSet<&str>) {
        for (lauf, zeiten) in &self.handzeiten {
            if !zugeordnet.contains(lauf.as_str())
                && zeiten
                    .iter()
                    .any(|z| z.in_frankfurt().is_some_and(|(d, ..)| d == tag))
            {
                s.push_str("<p><b>Konsolenlauf</b>");
                self.gelaufen(s, Some(lauf), None);
                s.push_str("</p>");
            }
        }
    }
}

fn abfragewert(wert: &str) -> String {
    let mut aus = String::new();
    for b in wert.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
            aus.push(char::from(b));
        } else {
            let _ = write!(aus, "%{b:02X}");
        }
    }
    aus
}

pub(crate) fn geplant(s: &mut String, tag: Datum) {
    for (wer, modell) in [("dachs", MODELL_DACHS), ("maus", MODELL_MAUS)] {
        let _ = write!(
            s,
            " <span><a href=\"/gespraeche?tag={tag}&amp;wer={wer}\">{} ({modell})</a></span>",
            name(wer)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn zeit(s: &str) -> io::Result<Zeitpunkt> {
        Zeitpunkt::lies(s).ok_or_else(|| io::Error::other("Zeit fehlt"))
    }

    #[test]
    fn unveraenderte_datei_wird_nur_einmal_gelesen() -> io::Result<()> {
        let daten =
            std::env::temp_dir().join(format!("depot_zeitstrahl_cache_{}", std::process::id()));
        let ordner = daten.join("gespraeche");
        fs::create_dir_all(&ordner)?;
        let ergebnis = (|| {
            let tag = Datum::lies("2026-11-10").ok_or_else(|| io::Error::other("Datum fehlt"))?;
            let pfad = ordner.join(format!("{tag}.jsonl"));
            fs::write(&pfad, "vorher")?;
            let cache = Cache::default();
            let aufrufe = Cell::new(0_u32);
            let lesen = || {
                aufrufe.set(aufrufe.get().saturating_add(1));
                Ok(Vec::new())
            };
            assert!(cache.tag(&daten, tag, lesen).is_some());
            assert!(cache.tag(&daten, tag, lesen).is_some());
            assert_eq!(aufrufe.get(), 1);
            fs::write(&pfad, "nachher: neuer Eintrag")?;
            assert!(cache.tag(&daten, tag, lesen).is_some());
            assert_eq!(aufrufe.get(), 2);
            Ok(())
        })();
        fs::remove_dir_all(daten)?;
        ergebnis
    }

    #[test]
    fn uhrzeit_ordnen_nur_eindeutige_handlaeufe_zu() -> io::Result<()> {
        let von = zeit("2026-11-10T06:31:00Z")?;
        let gesendet = zeit("2026-11-10T06:32:00Z")?;
        let bis = zeit("2026-11-10T06:34:00Z")?;
        let (tag, ..) = von
            .in_frankfurt()
            .ok_or_else(|| io::Error::other("Datum fehlt"))?;
        let mut aufrufe = Aufrufe::default();
        for (lauf, modell) in [("hand-123", "Handmodell"), ("123-1", "Dienstmodell")] {
            aufrufe.je_lauf.entry(lauf.into()).or_default().insert(
                Beteiligung {
                    tag,
                    wer: "dachs".into(),
                    modell: modell.into(),
                    optional: false,
                    gefragt: true,
                },
                BTreeSet::from(["ok".into()]),
            );
        }
        aufrufe.handzeiten.insert("hand-123".into(), vec![gesendet]);
        let mut s = String::new();
        aufrufe.gelaufen(
            &mut s,
            Some("unbekannt"),
            aufrufe.hand_kennung("unbekannt", von, bis),
        );
        assert!(s.contains("Handmodell") && s.contains("nach Uhrzeit zugeordnet"));
        assert!(!s.contains("Dienstmodell"));
        assert_eq!(aufrufe.hand_kennung("123-1", von, bis), None);
        aufrufe.handzeiten.insert("hand-456".into(), vec![gesendet]);
        assert_eq!(aufrufe.hand_kennung("unbekannt", von, bis), None);
        Ok(())
    }
}
