//! Das vom Updater gespeicherte Angebot. Der Updater prüft die Freigabe vor dem Bauen erneut.

use std::fs::{self, File, TryLockError};
use std::path::Path;

use crate::rueckfall::ist_commit;

#[derive(Debug)]
pub(crate) struct Angebot {
    pub alt: String,
    pub neu: String,
    pub abdruck: Option<String>,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Freigabe {
    #[default]
    Offen,
    Wartet,
    Erteilt,
    Fehler,
}

impl Angebot {
    pub fn laden(wurzel: &Path) -> Option<Self> {
        let text = fs::read_to_string(wurzel.join("daten/update_angebot.txt")).ok()?;
        let (kopf, text) = text.split_once('\n')?;
        let mut felder = kopf.split(';');
        let alt = felder
            .next()
            .filter(|f| *f == "update-angebot-v1")
            .and(felder.next())?;
        let neu = felder.next()?;
        let abdruck = felder.next()?;
        if felder.next().is_some() || !ist_commit(alt) || !ist_commit(neu) || alt == neu {
            return None;
        }
        let abdruck = match abdruck {
            "-" => None,
            a if a.len() == 64 && ist_commit(a) && !text.trim().is_empty() => Some(a.into()),
            _ => return None,
        };
        Some(Self {
            alt: alt.into(),
            neu: neu.into(),
            abdruck,
            text: text.into(),
        })
    }

    pub fn passt(&self, version: Option<&str>) -> bool {
        version.is_none_or(|stand| stand == self.alt)
    }

    pub fn freigabe(&self, wurzel: &Path) -> Freigabe {
        let Some(abdruck) = &self.abdruck else {
            return Freigabe::Erteilt;
        };
        let zeile = format!("update-lock-v1;{};{abdruck}", self.neu);
        match fs::read_to_string(wurzel.join("daten/update_abhaengigkeiten.txt")) {
            Ok(text) if text.lines().any(|z| z == zeile) => return Freigabe::Erteilt,
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Freigabe::Fehler,
            _ => {}
        }
        let pfad = wurzel.join(format!("daten/update-dialog-{}.lock", self.neu));
        match File::options().read(true).write(true).open(pfad) {
            Ok(f) => match f.try_lock() {
                Ok(()) => Freigabe::Offen,
                Err(TryLockError::WouldBlock) => Freigabe::Wartet,
                Err(TryLockError::Error(_)) => Freigabe::Fehler,
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Freigabe::Offen,
            Err(_) => Freigabe::Fehler,
        }
    }
}
