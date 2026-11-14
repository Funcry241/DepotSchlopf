//! Dauerhafte Kurssperre. Änderungen und automatische Prüfversuche sind serialisiert.
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use depot_kern::{Datum, Zeitpunkt, abdruck::sha256_hex};

pub const KURSSPERRE: &str = "kurse.halt";
pub const AUFGEHOBEN: &str = "kurse_halt_aufgehoben.txt";
const KOPF: &str = "kurssperre-v2";
const PRUEFVERMERK: &str = "kurssperre_pruefdatum.txt";
const AUTOMATISCH: &str = "automatisch: Fehlalarm, Prüfabruf";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    Sicherheit,
    Unklar,
}
impl Art {
    pub fn text(self) -> &'static str {
        match self {
            Self::Sicherheit => "sicherheit",
            Self::Unklar => "unklar",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quelle {
    AlphaVantage,
    Tiingo,
    Ezb,
    Rss,
}
impl Quelle {
    fn text(self) -> &'static str {
        match self {
            Self::AlphaVantage => "alphavantage",
            Self::Tiingo => "tiingo",
            Self::Ezb => "ezb",
            Self::Rss => "rss",
        }
    }

    fn lies(s: &str) -> Option<Self> {
        [Self::AlphaVantage, Self::Tiingo, Self::Ezb, Self::Rss]
            .into_iter()
            .find(|q| q.text() == s)
    }

    fn aus_alt(text: &str) -> Option<Self> {
        if text.lines().count() != 1 {
            return None;
        }
        if let Some(quelle) = text.split_once("Quelle der Antwort: ").map(|(_, q)| q) {
            if !quelle.contains("Antwortart: JSON-Hinweis des Anbieters") {
                return None;
            }
            return [
                ("Alpha Vantage.", Self::AlphaVantage),
                ("Tiingo.", Self::Tiingo),
                ("EZB.", Self::Ezb),
            ]
            .into_iter()
            .find_map(|(name, q)| quelle.starts_with(name).then_some(q));
        }
        // Der alte Standardtext stammt auch vom bekannten Alpha-Vantage-Fehlalarm.
        text.starts_with("Sicherheitsabbruch: Die Kursquelle gab den Schlüssel zurück.")
            .then_some(Self::AlphaVantage)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sperre {
    pub art: Art,
    pub seit: Option<Zeitpunkt>,
    pub quelle: Option<Quelle>,
    pub grund: String,
    inhalt: String,
}

fn altzeit(pfad: &Path) -> Option<Zeitpunkt> {
    let zeit = pfad
        .metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?;
    Zeitpunkt::aus_sekunden(i64::try_from(zeit.as_secs()).ok()?)
}

pub fn lies(daten: &Path) -> Result<Option<Sperre>, String> {
    let pfad = daten.join(KURSSPERRE);
    let inhalt = match fs::read_to_string(&pfad) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Kurssperre unlesbar: {e}")),
    };
    let mut zeilen = inhalt.lines();
    let kopf = zeilen.next().unwrap_or_default();
    if !kopf.starts_with("kurssperre-") {
        let grund = inhalt.trim().to_owned();
        return Ok(Some(Sperre {
            art: Art::Unklar,
            seit: altzeit(&pfad),
            quelle: Quelle::aus_alt(&grund),
            grund,
            inhalt,
        }));
    }
    let unlesbar = "Kurssperre hat ein ungültiges Format";
    let art = match zeilen.next() {
        Some("art=sicherheit") => Art::Sicherheit,
        Some("art=unklar") => Art::Unklar,
        _ => return Err(unlesbar.into()),
    };
    let seit = zeilen
        .next()
        .and_then(|s| s.strip_prefix("seit="))
        .ok_or(unlesbar)?;
    let seit = if art == Art::Sicherheit && seit == "unbekannt" {
        None
    } else {
        Some(
            seit.parse::<i64>()
                .ok()
                .and_then(Zeitpunkt::aus_sekunden)
                .ok_or(unlesbar)?,
        )
    };
    let quelle = zeilen
        .next()
        .and_then(|s| s.strip_prefix("quelle="))
        .and_then(Quelle::lies)
        .ok_or(unlesbar)?;
    let grund = zeilen
        .next()
        .and_then(|s| s.strip_prefix("grund="))
        .filter(|s| !s.is_empty())
        .ok_or(unlesbar)?
        .to_owned();
    if kopf != KOPF || zeilen.next().is_some() {
        return Err(unlesbar.into());
    }
    Ok(Some(Sperre {
        art,
        seit,
        quelle: Some(quelle),
        grund,
        inhalt,
    }))
}

pub fn kurssperre(daten: &Path) -> Option<String> {
    match lies(daten) {
        Ok(s) => s.map(|s| s.grund),
        Err(e) => Some(e),
    }
}

fn schreibe(p: &Path, text: &str) -> io::Result<()> {
    let neben = p.with_extension("kurssperre-neu");
    let mut f = File::create(&neben)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    drop(f);
    crate::dateiersatz::ersetze(neben, p)
}

#[derive(Debug)]
pub struct Gesperrt {
    daten: PathBuf,
    sperre: File,
}
impl Drop for Gesperrt {
    fn drop(&mut self) {
        let _ = self.sperre.unlock();
    }
}

/// Gemeinsam für Sicherheitsabbruch, Prüfabruf und die Freigabe in der Einrichtung.
pub fn sperren(daten: &Path) -> Result<Gesperrt, String> {
    fs::create_dir_all(daten).map_err(|e| e.to_string())?;
    let pfad = daten.join("kurssperre.lock");
    if fs::symlink_metadata(&pfad).is_ok_and(|m| !m.is_file()) {
        return Err("Kurssperre: Sperrdatei ist keine reguläre Datei".into());
    }
    let sperre = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(pfad)
        .map_err(|e| e.to_string())?;
    sperre.lock().map_err(|e| e.to_string())?;
    Ok(Gesperrt {
        daten: daten.to_owned(),
        sperre,
    })
}

impl Gesperrt {
    pub fn sicherheit(
        &self,
        quelle: Quelle,
        jetzt: Option<Zeitpunkt>,
        grund: &str,
    ) -> Result<(), String> {
        let p = self.daten.join(KURSSPERRE);
        let grund = grund.replace(['\n', '\r'], " ");
        schreibe(
            &p,
            &format!(
                "{KOPF}\nart=sicherheit\nseit={}\nquelle={}\ngrund={grund}\n",
                jetzt.map_or_else(|| "unbekannt".into(), |z| z.sekunden().to_string()),
                quelle.text()
            ),
        )
        .map_err(|e| e.to_string())
    }

    /// Reserviert höchstens einen Prüfversuch pro Frankfurter Kalendertag.
    pub fn pruefung_vermerken(&self, tag: Datum) -> Result<bool, String> {
        let p = self.daten.join(PRUEFVERMERK);
        match fs::read_to_string(&p) {
            Ok(s) => {
                let bisher = Datum::lies(s.trim()).ok_or("Datum der letzten Prüfung unlesbar")?;
                if bisher >= tag {
                    return Ok(false);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
        schreibe(&p, &format!("{tag}\n")).map_err(|e| e.to_string())?;
        Ok(true)
    }

    pub fn fehlalarm_aufheben(&self, sperre: &Sperre, jetzt: Zeitpunkt) -> Result<(), String> {
        if sperre.art != Art::Unklar || lies(&self.daten)?.as_ref() != Some(sperre) {
            return Err("Kurssperre wurde zwischenzeitlich geändert".into());
        }
        let q = self.daten.join(AUFGEHOBEN);
        let beleg = sha256_hex(format!("{:?}\n{}", sperre.seit, sperre.inhalt).as_bytes());
        let ende = format!(";beleg={beleg}");
        let vorher = match fs::read_to_string(&q) {
            Ok(s) => s,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e.to_string()),
        };
        if !vorher
            .lines()
            .any(|l| l.starts_with(AUTOMATISCH) && l.ends_with(&ende))
        {
            let trennung = if vorher.is_empty() || vorher.ends_with('\n') {
                ""
            } else {
                "\n"
            };
            let vermerk = format!(
                "{vorher}{trennung}{AUTOMATISCH} {} sauber{ende}\n",
                jetzt.sekunden()
            );
            schreibe(&q, &vermerk).map_err(|e| e.to_string())?;
        }
        fs::remove_file(self.daten.join(KURSSPERRE)).map_err(|e| e.to_string())
    }
}

/// Letzter für die automatische Prüfung vorgemerkter Frankfurter Tag.
pub fn pruefdatum(daten: &Path) -> Option<Datum> {
    Datum::lies(fs::read_to_string(daten.join(PRUEFVERMERK)).ok()?.trim())
}

/// Zeitpunkt des letzten automatisch aufgehobenen Fehlalarms.
pub fn letzter_fehlalarm(daten: &Path) -> Option<Zeitpunkt> {
    fs::read_to_string(daten.join(AUFGEHOBEN))
        .ok()?
        .lines()
        .rev()
        .find_map(|l| {
            let (zeit, _beleg) = l
                .strip_prefix(AUTOMATISCH)?
                .trim_start()
                .split_once(" sauber;beleg=")?;
            Zeitpunkt::aus_sekunden(zeit.parse().ok()?)
        })
}
