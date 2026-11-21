//! Lokale Einstellungen für neue KI-Aufrufe. Ein vorhandener unlesbarer Stand sperrt neue Aufrufe.

use depot_kern::BeraterId;
use depot_kern::gemini::TAGESGRENZE;
use depot_kern::modelle::{
    BeraterEinstellung, Denktiefe, KiEinstellungen, MAX_BERATER_AUFRUFE, Modell, Modellwahl,
};
use std::path::{Path, PathBuf};

pub const DATEI: &str = "ki-einstellungen.txt";
pub const MAX_NEURO_AUFRUFE: usize = 30;

pub fn pfad(daten: &Path) -> PathBuf {
    daten.join(DATEI)
}

pub fn tagesgrenze(text: &str, maximum: usize) -> Option<usize> {
    let zahl = text.parse().ok()?;
    (zahl <= maximum).then_some(zahl)
}

pub fn profil(wer: BeraterId, wahl: &str, aufrufe: &str) -> Option<BeraterEinstellung> {
    let (modell, tiefe) = wahl.split_once(':')?;
    Some(BeraterEinstellung {
        auswahl: Modellwahl::neu(wer, Modell::lies(modell)?, Denktiefe::lies(tiefe)?)?,
        max_aufrufe: tagesgrenze(aufrufe, MAX_BERATER_AUFRUFE)?,
    })
}

fn lies(text: &str) -> Option<KiEinstellungen> {
    let mut zeilen = text.lines();
    if zeilen.next()? != "ki-einstellungen=1" {
        return None;
    }
    let dachs = zeilen.next()?.strip_prefix("dachs=")?.split_once(';')?;
    let maus = zeilen.next()?.strip_prefix("maus=")?.split_once(';')?;
    let gemini = zeilen.next()?.strip_prefix("gemini=")?;
    let neuro = zeilen.next()?.strip_prefix("neuroschlopf=")?;
    if zeilen.next().is_some() {
        return None;
    }
    Some(KiEinstellungen {
        dachs: profil(BeraterId::Dachs, dachs.0, dachs.1)?,
        maus: profil(BeraterId::Maus, maus.0, maus.1)?,
        gemini: tagesgrenze(gemini, TAGESGRENZE)?,
        neuroschlopf: tagesgrenze(neuro, MAX_NEURO_AUFRUFE)?,
    })
}

pub fn als_text(e: KiEinstellungen) -> String {
    let profil = |p: BeraterEinstellung| {
        format!(
            "{}:{};{}",
            p.auswahl.modell().kennung(),
            p.auswahl.denktiefe().kennung(),
            p.max_aufrufe
        )
    };
    format!(
        "ki-einstellungen=1\ndachs={}\nmaus={}\ngemini={}\nneuroschlopf={}\n",
        profil(e.dachs),
        profil(e.maus),
        e.gemini,
        e.neuroschlopf
    )
}

/// Eine fehlende Datei bedeutet die bisherigen Voreinstellungen.
///
/// # Errors
/// Die gespeicherte Datei lässt sich nicht lesen oder enthält ungültige Einstellungen.
pub fn lesen(daten: &Path) -> Result<KiEinstellungen, String> {
    match std::fs::read_to_string(pfad(daten)) {
        Ok(text) => lies(&text).ok_or_else(|| "KI-Einstellungen ungültig".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(KiEinstellungen::default()),
        Err(_) => Err("KI-Einstellungen nicht lesbar".into()),
    }
}
