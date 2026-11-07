//! Dauerhafte Anzeige des Update-Abschlusses, getrennt vom erfolgreichen Einspielauftrag.
//! Diese Belege entscheiden weder über Handel noch über den Rückfallmechanismus.
use crate::{
    rueckfall::Offen,
    werkbank::{Art, schreibe_ganz},
};
use depot_kern::{Zeitpunkt, json};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Probe,
    Bestaetigt,
    Rueckfall,
}
impl Phase {
    fn kurz(self) -> &'static str {
        match self {
            Self::Probe => "probe",
            Self::Bestaetigt => "bestaetigt",
            Self::Rueckfall => "rueckfall",
        }
    }
    pub fn ergebnis(self) -> (&'static str, &'static str) {
        match self {
            Self::Probe => ("offen", "Neustart noch nicht bestätigt"),
            Self::Bestaetigt => ("gut", "Update bestätigt"),
            Self::Rueckfall => ("halt", "zurückgesetzt"),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ergebnis {
    pub vorher: String,
    pub neu: String,
    pub lauf: String,
    pub phase: Phase,
    pub zeit: Zeitpunkt,
    pub grund: String,
}
impl Ergebnis {
    pub fn meldung(&self) -> (bool, String) {
        let status = match self.phase {
            Phase::Probe => "Neustart läuft; noch nicht bestätigt",
            Phase::Bestaetigt => "bestätigt",
            Phase::Rueckfall => "zurückgesetzt; alter Stand läuft",
        };
        (
            self.phase == Phase::Rueckfall,
            format!(
                "Update {} → {}: {status}. {}",
                crate::rueckfall::kurz(&self.vorher),
                crate::rueckfall::kurz(&self.neu),
                self.grund
            ),
        )
    }
    fn text(&self) -> String {
        format!(
            "{{\"typ\":\"update-ergebnis-v1\",\"vorher\":{},\"neu\":{},\"lauf\":{},\"phase\":{},\"zeit\":{},\"grund\":{}}}",
            json::zitiere(&self.vorher),
            json::zitiere(&self.neu),
            json::zitiere(&self.lauf),
            json::zitiere(self.phase.kurz()),
            self.zeit.sekunden(),
            json::zitiere(&self.grund)
        )
    }
}
pub fn pfad(wurzel: &Path) -> PathBuf {
    wurzel.join("daten/update_ergebnisse.txt")
}
fn lies_zeile(text: &str) -> Option<Ergebnis> {
    let j = json::lies(text).ok()?;
    if j.doppelte_felder() || j.feld("typ")?.text()? != "update-ergebnis-v1" {
        return None;
    }
    let vorher = j.feld("vorher")?.text()?;
    let neu = j.feld("neu")?.text()?;
    let lauf = j.feld("lauf")?.text()?;
    if !crate::rueckfall::ist_commit(vorher)
        || !crate::rueckfall::ist_commit(neu)
        || vorher == neu
        || (lauf != "-" && !crate::protokoll::gueltige_kennung(lauf))
    {
        return None;
    }
    let phase = match j.feld("phase")?.text()? {
        "probe" => Phase::Probe,
        "bestaetigt" => Phase::Bestaetigt,
        "rueckfall" => Phase::Rueckfall,
        _ => return None,
    };
    Some(Ergebnis {
        vorher: vorher.into(),
        neu: neu.into(),
        lauf: lauf.into(),
        phase,
        zeit: Zeitpunkt::aus_sekunden(j.feld("zeit")?.ganz()?)?,
        grund: j.feld("grund")?.text()?.into(),
    })
}
/// Höchstens zwanzig Abschlussbelege. Unlesbare oder fremde Daten werden nicht als Erfolg gewertet.
pub fn laden(wurzel: &Path) -> Result<Vec<Ergebnis>, String> {
    let p = pfad(wurzel);
    if p.metadata().is_ok_and(|m| m.len() > 128 * 1024) {
        return Err("Update-Abschlussbelege sind zu groß.".into());
    }
    let Some(text) = crate::werkbank::lies_falls_da(&p)? else {
        return Ok(Vec::new());
    };
    let schutz = crate::protokoll::Schutz::laden();
    text.lines()
        .map(|s| {
            let mut r = lies_zeile(s).ok_or("Update-Abschlussbelege sind unlesbar.")?;
            r.grund = schutz.bereinige_zeilen(&r.grund);
            Ok(r)
        })
        .collect()
}
/// Schreibt und synchronisiert den Beleg, bevor die offene Update-Markierung verschwinden darf.
pub fn vermerke(
    wurzel: &Path,
    o: &Offen,
    phase: Phase,
    grund: &str,
    lauf: Option<&str>,
) -> Result<Ergebnis, String> {
    let mut alle = laden(wurzel)?;
    let vorhanden = alle.iter().find(|e| e.vorher == o.vorher && e.neu == o.neu);
    let lauf = lauf
        .map(str::to_string)
        .or_else(|| vorhanden.map(|e| e.lauf.clone()))
        .unwrap_or_else(|| {
            std::fs::read_to_string(crate::verlauf::pfad(wurzel))
                .ok()
                .and_then(|t| {
                    crate::verlauf::lies(&t)
                        .auftraege
                        .into_iter()
                        .find(|a| a.art == Art::Update)
                        .map(|a| a.kennung)
                })
                .unwrap_or_else(|| "-".into())
        });
    let grund = crate::protokoll::Schutz::laden().bereinige_zeilen(grund);
    let e = Ergebnis {
        vorher: o.vorher.clone(),
        neu: o.neu.clone(),
        lauf,
        phase,
        zeit: crate::jetzt().ok_or("Update-Abschluss: Uhr nicht verfügbar.")?,
        grund: grund.chars().take(2048).collect(),
    };
    if lies_zeile(&e.text()).is_none() {
        return Err("Update-Abschlussbeleg ist ungültig.".into());
    }
    verlauf_abschliessen(wurzel, &e)?;
    alle.retain(|r| r.neu != o.neu || r.vorher != o.vorher);
    alle.push(e.clone());
    let text = alle
        .iter()
        .skip(alle.len().saturating_sub(20))
        .map(Ergebnis::text)
        .collect::<Vec<_>>()
        .join("\n");
    schreibe_ganz(&pfad(wurzel), &format!("{text}\n"))?;
    Ok(e)
}

/// Auch ältere Rückfallstände lesen den bestehenden Verlauf und das bestehende Protokoll.
/// Der endgültige Ausgang muss dort den vorläufigen Erfolg des Einspielens berichtigen.
fn verlauf_abschliessen(wurzel: &Path, e: &Ergebnis) -> Result<(), String> {
    if e.phase == Phase::Probe || e.lauf == "-" {
        return Ok(());
    }
    let text = std::fs::read_to_string(crate::verlauf::pfad(wurzel))
        .map_err(|_| "Update-Verlauf nicht lesbar.")?;
    let Some(mut a) = crate::verlauf::lies(&text)
        .auftraege
        .into_iter()
        .find(|a| a.kennung == e.lauf && a.art == Art::Update)
    else {
        return Ok(());
    };
    a.erfolg = Some(e.phase == Phase::Bestaetigt);
    a.ende = Some(e.zeit);
    crate::verlauf::vermerke(wurzel, &crate::verlauf::ende_zeile(&a))?;
    let p = crate::protokoll::ordner(wurzel).join(format!("{}.log", e.lauf));
    if p.try_exists()
        .map_err(|_| "Update-Protokoll nicht prüfbar.")?
    {
        use std::io::Write as _;
        if p.metadata()
            .map_err(|_| "Update-Protokoll nicht prüfbar.")?
            .len()
            > crate::protokoll::DATEI_MAX.saturating_sub(4096)
        {
            return Err("Update-Protokoll ist zu groß für den Abschlussbeleg.".into());
        }
        let (_, text) = e.meldung();
        let text = crate::protokoll::Schutz::laden().bereinige_zeilen(&text);
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(p)
            .map_err(|_| "Update-Protokoll nicht schreibbar.")?;
        writeln!(f, "[{}] Endgültiger Update-Abschluss: {text}", e.zeit)
            .map_err(|_| "Update-Protokollabschluss nicht schreibbar.")?;
        f.sync_all()
            .map_err(|_| "Update-Protokollabschluss nicht dauerhaft gespeichert.")?;
    }
    Ok(())
}
