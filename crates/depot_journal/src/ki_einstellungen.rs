//! Lokale Einstellungen für neue KI-Aufrufe. Ein vorhandener unlesbarer Stand sperrt neue Aufrufe.

use depot_kern::BeraterId;
use depot_kern::gemini::TAGESGRENZE;
use depot_kern::modelle::{
    BeraterEinstellung, Denktiefe, KiEinstellungen, MAX_BERATER_AUFRUFE, Modell, Modellwahl,
    NeuroEmpfaenger,
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
    let mit_empfaenger = match zeilen.next()? {
        "ki-einstellungen=1" => false,
        "ki-einstellungen=2" => true,
        _ => return None,
    };
    let dachs = zeilen.next()?.strip_prefix("dachs=")?.split_once(';')?;
    let maus = zeilen.next()?.strip_prefix("maus=")?.split_once(';')?;
    let gemini = zeilen.next()?.strip_prefix("gemini=")?;
    let neuro = zeilen.next()?.strip_prefix("neuroschlopf=")?;
    let neuro_empfaenger = if mit_empfaenger {
        NeuroEmpfaenger::lies(zeilen.next()?.strip_prefix("neuro-empfaenger=")?)?
    } else {
        NeuroEmpfaenger::Aus
    };
    if zeilen.next().is_some() {
        return None;
    }
    Some(KiEinstellungen {
        dachs: profil(BeraterId::Dachs, dachs.0, dachs.1)?,
        maus: profil(BeraterId::Maus, maus.0, maus.1)?,
        gemini: tagesgrenze(gemini, TAGESGRENZE)?,
        neuroschlopf: tagesgrenze(neuro, MAX_NEURO_AUFRUFE)?,
        neuro_empfaenger,
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
        "ki-einstellungen=2\ndachs={}\nmaus={}\ngemini={}\nneuroschlopf={}\nneuro-empfaenger={}\n",
        profil(e.dachs),
        profil(e.maus),
        e.gemini,
        e.neuroschlopf,
        e.neuro_empfaenger.kennung()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alter_stand_behaelt_das_limit_und_schaltet_externe_fragen_aus() -> Result<(), String> {
        let alt = "ki-einstellungen=1\ndachs=claude-sonnet-5-5:medium;3\nmaus=gpt-6.2-sol:medium;10\ngemini=2\nneuroschlopf=6\n";
        let gespeichert = lies(alt).ok_or("Alte KI-Einstellungen fehlen")?;
        assert_eq!(gespeichert.neuroschlopf, 6);
        assert_eq!(gespeichert.neuro_empfaenger, NeuroEmpfaenger::Aus);
        assert!(gespeichert.neuro_berater().is_empty());
        assert_eq!(lies(&als_text(gespeichert)), Some(gespeichert));
        Ok(())
    }

    #[test]
    fn neue_auswahl_laesst_sich_lesen_und_zaehlt_je_empfaenger() {
        let standard = KiEinstellungen::default();
        assert_eq!(standard.neuroschlopf, 8);
        for (wahl, berater) in [
            (NeuroEmpfaenger::Aus, [].as_slice()),
            (NeuroEmpfaenger::Maus, &[BeraterId::Maus]),
            (NeuroEmpfaenger::Dachs, &[BeraterId::Dachs]),
            (NeuroEmpfaenger::Beide, &[BeraterId::Maus, BeraterId::Dachs]),
        ] {
            let gespeichert = KiEinstellungen {
                neuro_empfaenger: wahl,
                ..standard
            };
            let text = als_text(gespeichert);
            assert_eq!(lies(&text), Some(gespeichert));
            assert_eq!(gespeichert.neuro_berater(), berater);
        }
        let pausiert = KiEinstellungen {
            neuroschlopf: 0,
            neuro_empfaenger: NeuroEmpfaenger::Beide,
            ..standard
        };
        assert!(pausiert.neuro_berater().is_empty());
        assert_eq!(lies(&als_text(pausiert)), Some(pausiert));
        assert!(lies(&als_text(pausiert).replace("=beide", "=gemini")).is_none());
    }
}
