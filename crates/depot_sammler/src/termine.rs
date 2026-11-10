//! Ein Lauf ueber die Termine (R39): die Seite der EZB abrufen, roh ablegen, lesen, belegen.
//!
//! Reihenfolge wie bei den Kursen (R12): Erst liegt die rohe Seite unter `daten/roh/ezb/`, dann
//! steht jeder Termin daraus im Journal. Was nicht gelesen werden kann, wird nicht belegt; die
//! rohe Seite bleibt zur Durchsicht liegen. Ein Termin, der schon genau so belegt ist, wird nicht
//! noch einmal geschrieben; ein geaenderter wird unter seiner Kennung fortgeschrieben.
//!
//! Der Lauf erfindet nichts und loescht nichts. Ein kuenftiger Termin, der im Journal steht, aber
//! nicht mehr auf der Seite, wird nur gemeldet (`fehlen`); was das bedeutet, entscheidet nicht
//! der Sammler.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use depot_journal::{Fortschreibung, Journal, JournalFehler, TerminBeleg};
use depot_kern::{Herkunft, Termin, Zeitpunkt};

use crate::ezb::{ADRESSE_EZB, QUELLE_EZB, lies_ezb_seite, ordne_zu};
use crate::netz::Netz;
use crate::sammeln::{SammelFehler, lege_ab};

/// Der Bericht eines Laufs ueber die Termine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminBericht {
    /// Die abgelegte rohe Seite; keine, wenn der Abruf scheiterte.
    pub rohdatei: Option<PathBuf>,
    /// Termine mit neuer Kennung.
    pub neu: usize,
    /// Termine, die sich geaendert haben und fortgeschrieben sind.
    pub geaendert: usize,
    /// Termine, die schon genau so belegt waren.
    pub unveraendert: usize,
    /// Kennungen kuenftiger Termine dieser Quelle, die im Journal stehen, aber nicht mehr auf
    /// der Seite, aufsteigend.
    pub fehlen: Vec<String>,
    /// Warum nichts belegt wurde.
    pub fehler: Option<String>,
}

/// Ein Lauf ueber die Seite der EZB. `uhr` liefert die Zeit des Rechners in UTC.
///
/// # Errors
/// Journal, Ablage oder Uhr versagen. Was bis dahin belegt ist, bleibt belegt.
pub fn sammle_termine(
    journal: &mut Journal,
    netz: &mut dyn Netz,
    uhr: &mut dyn FnMut() -> Option<Zeitpunkt>,
    ablage: &Path,
) -> Result<TerminBericht, SammelFehler> {
    sammle_termine_geschuetzt(journal, netz, uhr, ablage, &crate::Zugang::leer())
}

/// Wie [`sammle_termine`], mit Prüfung auf hinterlegte Zugangsdaten vor der Rohablage.
///
/// # Errors
/// Journal, Ablage oder Uhr versagen.
pub fn sammle_termine_geschuetzt(
    journal: &mut Journal,
    netz: &mut dyn Netz,
    uhr: &mut dyn FnMut() -> Option<Zeitpunkt>,
    ablage: &Path,
    zugang: &crate::Zugang,
) -> Result<TerminBericht, SammelFehler> {
    let mut bericht = TerminBericht::default();
    let roh = match netz.hole(ADRESSE_EZB) {
        Ok(r) => r,
        Err(e) => {
            bericht.fehler = Some(format!("Abruf: {e}"));
            return Ok(bericht);
        }
    };
    if let Some(text) = crate::diagnose::sicherheitsabbruch(zugang, QUELLE_EZB, &roh) {
        bericht.fehler = Some(text);
        return Ok(bericht);
    }
    let abruf = uhr().ok_or(SammelFehler::Uhr)?;
    bericht.rohdatei =
        Some(lege_ab(ablage, QUELLE_EZB, abruf, ".html", &roh).map_err(SammelFehler::Ablage)?);
    // Die bekannten Termine dieser Quelle, je Kennung die letzte Fassung.
    let bekannt: Vec<Termin> = journal
        .stand()
        .aktuelle_termine()
        .into_values()
        .filter(|b| b.quelle() == QUELLE_EZB)
        .map(|b| b.termin().clone())
        .collect();
    let bekannt: Vec<&Termin> = bekannt.iter().collect();
    let termine = match std::str::from_utf8(&roh)
        .map_err(|_| "Seite ist kein UTF-8".to_string())
        .and_then(|t| lies_ezb_seite(t).map_err(|e| format!("Seite unlesbar: {e:?}")))
        .and_then(|e| ordne_zu(e, &bekannt).map_err(|e| format!("Zuordnung: {e:?}")))
    {
        Ok(t) => t,
        Err(text) => {
            bericht.fehler = Some(text);
            return Ok(bericht);
        }
    };

    let heute = abruf.datum().ok_or(SammelFehler::Uhr)?;
    let auf_der_seite: BTreeSet<&str> = termine.iter().map(Termin::kennung).collect();
    bericht.fehlen = journal
        .stand()
        .aktuelle_termine()
        .into_iter()
        .filter(|(kennung, b)| {
            b.quelle() == QUELLE_EZB
                && !auf_der_seite.contains(kennung)
                && b.termin().zeit().tag().is_some_and(|tag| tag >= heute)
        })
        .map(|(kennung, _)| kennung.to_string())
        .collect();

    for termin in termine {
        let beleg = TerminBeleg::neu(QUELLE_EZB, Herkunft::Quelle, abruf, &roh, termin)
            .map_err(|e| SammelFehler::Journal(JournalFehler::Abgelehnt(e)))?;
        match journal
            .belege_termin(&beleg)
            .map_err(SammelFehler::Journal)?
        {
            Fortschreibung::Neu => bericht.neu = bericht.neu.saturating_add(1),
            Fortschreibung::Geaendert => bericht.geaendert = bericht.geaendert.saturating_add(1),
            Fortschreibung::Unveraendert => {
                bericht.unveraendert = bericht.unveraendert.saturating_add(1);
            }
        }
    }
    Ok(bericht)
}
