//! Der gepruefte Stand (R28, 1·R04). Leere Schnittstelle fuer die roten Tests.

use std::path::Path;

use crate::export::Lauf;

/// Der Index zu einem Zeitpunkt und wo der Arbeitsbaum davon abweicht.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Abbild {
    /// Die gewoehnlichen Dateien im Index mit ihren Blobs, nach Pfad sortiert.
    pub dateien: Vec<(String, String)>,
    /// Wo der Arbeitsbaum vom Index abweicht.
    pub abweichungen: Vec<String>,
}

/// # Errors
/// Git nicht lesbar.
pub fn aufnehmen(_wurzel: &Path) -> Result<Option<Abbild>, String> {
    Ok(None)
}

/// Fuehrt die Pruefung aus und haelt den Stand davor und danach fest.
pub fn gepruefter_lauf(
    wurzel: &Path,
    befehl: String,
    mut konsole: String,
    warnungen: Vec<String>,
    pruefung: impl FnOnce(&mut String) -> i32,
) -> Lauf {
    let exit = pruefung(&mut konsole);
    Lauf {
        befehl,
        schritte: vec![crate::export::Schritt {
            name: "kodex".into(),
            exit,
        }],
        konsole,
        warnungen,
        abbild: aufnehmen(wurzel).ok().flatten(),
    }
}
