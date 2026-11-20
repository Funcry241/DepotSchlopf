//! Lokaler Anzeigestand des lesenden IBKR-Papierabrufs. Kein Buchungsbeleg.

use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write as _};
use std::path::{Path, PathBuf};

use depot_kern::ibkr_papier::kontonummer_gueltig;
use depot_kern::{Cent, Zeitpunkt};

const FORMAT: &str = "ibkr-abruf-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Abrufstand {
    pub erfolg: Option<(Zeitpunkt, Cent)>,
    pub fehlgeschlagen: bool,
}

fn pfad(wurzel: &Path, konto: &str) -> io::Result<PathBuf> {
    if !kontonummer_gueltig(konto) {
        return Err(io::Error::from(ErrorKind::InvalidInput));
    }
    let pfad = wurzel
        .join("daten/ibkr-papier")
        .join(konto)
        .join("abrufstand.txt");
    crate::datei::kein_link_im_pfad(&pfad).map_err(|_| io::Error::from(ErrorKind::InvalidInput))?;
    Ok(pfad)
}

impl Abrufstand {
    fn aus_text(text: &str) -> Option<Self> {
        let mut felder = text.strip_suffix('\n')?.split(';');
        if felder.next()? != FORMAT {
            return None;
        }
        let status = felder.next()?;
        let zeit = felder.next()?;
        let betrag = felder.next()?;
        if felder.next().is_some() {
            return None;
        }
        let fehlgeschlagen = match status {
            "ok" => false,
            "fehler" => true,
            _ => return None,
        };
        let erfolg = if fehlgeschlagen && zeit.is_empty() && betrag.is_empty() {
            None
        } else {
            Some((Zeitpunkt::lies(zeit)?, Cent(betrag.parse().ok()?)))
        };
        Some(Self {
            erfolg,
            fehlgeschlagen,
        })
    }

    fn als_text(self) -> String {
        let status = if self.fehlgeschlagen { "fehler" } else { "ok" };
        match self.erfolg {
            Some((zeit, betrag)) => format!("{FORMAT};{status};{zeit};{}\n", betrag.0),
            None => format!("{FORMAT};{status};;\n"),
        }
    }
}

/// Liest den letzten Stand des angegebenen Papierkontos.
///
/// # Errors
/// Kontonummer ungültig oder vorhandener Stand unlesbar.
pub fn lesen(wurzel: &Path, konto: &str) -> io::Result<Option<Abrufstand>> {
    let pfad = pfad(wurzel, konto)?;
    regulaer_oder_fehlend(&pfad)?;
    match fs::read_to_string(pfad) {
        Ok(text) => Abrufstand::aus_text(&text)
            .map(Some)
            .ok_or_else(|| io::Error::from(ErrorKind::InvalidData)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn regulaer_oder_fehlend(pfad: &Path) -> io::Result<()> {
    crate::datei::datei_da(pfad)
        .map(|_| ())
        .map_err(|_| io::Error::from(ErrorKind::InvalidData))
}

/// Hält einen abgeschlossenen Versuch fest. Ein Fehlschlag bewahrt den letzten Erfolg.
///
/// # Errors
/// Kontonummer ungültig, lokale Sperre belegt oder Stand nicht speicherbar.
pub fn vermerken(wurzel: &Path, konto: &str, abruf: Option<(Zeitpunkt, Cent)>) -> io::Result<()> {
    let ziel = pfad(wurzel, konto)?;
    let ordner = ziel
        .parent()
        .ok_or_else(|| io::Error::from(ErrorKind::InvalidInput))?;
    fs::create_dir_all(ordner)?;
    let sperrpfad = ziel.with_extension("lock");
    regulaer_oder_fehlend(&sperrpfad)?;
    let sperre = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(sperrpfad)?;
    sperre.try_lock().map_err(io::Error::from)?;
    let erfolg = match abruf {
        Some(neu) => Some(neu),
        None => lesen(wurzel, konto)?.and_then(|alt| alt.erfolg),
    };
    let text = Abrufstand {
        erfolg,
        fehlgeschlagen: abruf.is_none(),
    }
    .als_text();
    let neben = ziel.with_extension("neu");
    match fs::remove_file(&neben) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let mut datei = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&neben)?;
    datei.write_all(text.as_bytes())?;
    datei.sync_all()?;
    drop(datei);
    crate::dateiersatz::ersetze(&neben, &ziel)
}
