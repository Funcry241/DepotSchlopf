//! Schluessel in der Windows-Anmeldeinformationsverwaltung: nur ablegen und fragen, ob einer da
//! ist. Für den Protokollschutz liest der Dienst Schlüssel kurz zum Hashen; nur Prüfsummen bleiben im Speicher.
//!
//! Adressen und Regeln sind dieselben wie in den Laeufen, die die Schluessel benutzen:
//! - Kursquelle `DepotSchlopf/alphavantage`: 8 bis 64 Grossbuchstaben und Ziffern
//!   (wie `depot_sammler::Schluessel::fuer`).
//! - Kursquelle der NYSE `DepotSchlopf/tiingo`: 20 bis 64 Kleinbuchstaben und Ziffern
//!   (ebenso).
//! - Dachs `DepotSchlopf/anthropic`, Maus `DepotSchlopf/openai`: 20 bis 256 Zeichen aus
//!   `A-Z a-z 0-9 _ -` (wie der Berater sie liest).
//!
//! Benutzer ist immer `depotschlopf`; so legt auch `cmdkey /generic:… /user:depotschlopf` an.

use depot_kern::BeraterId;
use depot_lage::Hinterlegt;

/// Welcher Schluessel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wer {
    Kursquelle,
    /// Kursquelle der NYSE (Tiingo).
    KursquelleUs,
    Berater(BeraterId),
    Gemini,
}

pub const ALLE: [Wer; 4] = [
    Wer::Kursquelle,
    Wer::KursquelleUs,
    Wer::Berater(BeraterId::Dachs),
    Wer::Berater(BeraterId::Maus),
];

const BENUTZER: &str = "depotschlopf";

impl Wer {
    /// Dienstname im Speicher.
    pub fn dienst(self) -> &'static str {
        match self {
            Wer::Gemini => depot_kern::gemini::DIENST,
            Wer::Kursquelle => "alphavantage",
            Wer::KursquelleUs => "tiingo",
            Wer::Berater(BeraterId::Dachs) => "anthropic",
            Wer::Berater(BeraterId::Maus) => "openai",
        }
    }

    /// Adresse in der Anmeldeinformationsverwaltung.
    pub fn adresse(self) -> &'static str {
        match self {
            Wer::Gemini => depot_kern::gemini::ADRESSE,
            Wer::Kursquelle => "DepotSchlopf/alphavantage",
            Wer::KursquelleUs => "DepotSchlopf/tiingo",
            Wer::Berater(BeraterId::Dachs) => "DepotSchlopf/anthropic",
            Wer::Berater(BeraterId::Maus) => "DepotSchlopf/openai",
        }
    }

    pub fn name(self) -> &'static str {
        self.protokoll().anzeigename()
    }

    /// Name im Anbieterprotokoll.
    pub fn protokoll(self) -> depot_lage::anbieter::Wer {
        match self {
            Wer::Gemini => depot_lage::anbieter::Wer::Gemini,
            Wer::Kursquelle => depot_lage::anbieter::Wer::Kursquelle,
            Wer::KursquelleUs => depot_lage::anbieter::Wer::Tiingo,
            Wer::Berater(BeraterId::Dachs) => depot_lage::anbieter::Wer::Dachs,
            Wer::Berater(BeraterId::Maus) => depot_lage::anbieter::Wer::Maus,
        }
    }

    /// Kurzname fuer Formulare.
    pub fn kurz(self) -> &'static str {
        self.dienst()
    }

    pub fn aus_kurz(k: &str) -> Option<Wer> {
        ALLE.into_iter()
            .chain([Wer::Gemini])
            .find(|w| w.kurz() == k)
    }

    /// Prueft einen eingegebenen Schluessel. Leerraum am Rand wird entfernt. Die Meldung nennt
    /// den Schluessel nie.
    ///
    /// # Errors
    /// Laenge oder Zeichen passen nicht.
    pub fn pruefe(self, text: &str) -> Result<String, &'static str> {
        let t = text.trim();
        match self {
            Wer::Kursquelle => {
                if !(8..=64).contains(&t.len()) {
                    return Err("8 bis 64 Zeichen erwartet");
                }
                if !t
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                {
                    return Err("nur Großbuchstaben und Ziffern erlaubt");
                }
            }
            Wer::KursquelleUs => {
                if !(20..=64).contains(&t.len()) {
                    return Err("20 bis 64 Zeichen erwartet");
                }
                if !t
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                {
                    return Err("nur Kleinbuchstaben und Ziffern erlaubt");
                }
            }
            Wer::Gemini => return depot_kern::gemini::pruefe_schluessel(t).map(str::to_owned),
            Wer::Berater(_) => {
                if !(20..=256).contains(&t.len()) {
                    return Err("20 bis 256 Zeichen erwartet");
                }
                if !t
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                {
                    return Err("nur A-Z, a-z, 0-9, _ und - erlaubt");
                }
            }
        }
        Ok(t.to_string())
    }
}

#[cfg(windows)]
static ANMELDEVERWALTUNG: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(windows)]
fn eintrag(wer: Wer) -> Result<keyring_core::Entry, String> {
    windows_eintrag(wer, wer.adresse())
}

#[cfg(windows)]
fn windows_eintrag(wer: Wer, adresse: &str) -> Result<keyring_core::Entry, String> {
    use std::collections::HashMap;

    use keyring_core::api::CredentialStoreApi;
    use windows_native_keyring_store::Store;

    let store = Store::new().map_err(|e| format!("Anmeldeverwaltung: {e}"))?;
    let mut ziel = HashMap::from([("target", adresse)]);
    if wer == Wer::Gemini {
        ziel.insert("persistence", "Local");
    }
    store
        .build(wer.dienst(), BENUTZER, Some(&ziel))
        .map_err(|e| format!("Anmeldeverwaltung: {e}"))
}

/// Ist ein Schluessel hinterlegt? Liest nur die Merkmale des Eintrags, nicht den Schluessel.
#[cfg(windows)]
pub fn status(wer: Wer) -> Hinterlegt {
    let Ok(_sperre) = ANMELDEVERWALTUNG.lock() else {
        return Hinterlegt::Unbekannt;
    };
    match eintrag(wer).map(|e| e.get_attributes()) {
        Ok(Ok(_)) => Hinterlegt::Ja,
        Ok(Err(keyring_core::Error::NoEntry)) => Hinterlegt::Nein,
        _ => Hinterlegt::Unbekannt,
    }
}

/// Ausserhalb von Windows gibt es keinen Ort fuer Schluessel.
#[cfg(not(windows))]
pub fn status(_wer: Wer) -> Hinterlegt {
    Hinterlegt::Unbekannt
}

/// Legt einen geprueften Schluessel ab (ersetzt einen alten). Er gilt ab dem naechsten Lauf.
///
/// # Errors
/// Ungueltig oder der Speicher lehnt ab. Keine Meldung nennt den Schluessel.
#[cfg(windows)]
pub fn ablegen(wer: Wer, text: &str) -> Result<(), String> {
    let s = wer.pruefe(text)?;
    let _sperre = ANMELDEVERWALTUNG
        .lock()
        .map_err(|_| "Anmeldeinformationsverwaltung nicht verfügbar")?;
    let eintrag = eintrag(wer)?;
    if wer == Wer::Gemini {
        return gemini_ablegen(&eintrag, &s);
    }
    eintrag
        .set_password(&s)
        .map_err(|_| format!("Anmeldeverwaltung: {} nicht gespeichert", wer.adresse()))?;
    Ok(())
}

#[cfg(windows)]
fn gemini_ablegen(eintrag: &keyring_core::Entry, wert: &str) -> Result<(), String> {
    eintrag
        .set_password(wert)
        .map_err(|_| "Windows konnte den Gemini-Schlüssel nicht speichern.")?;
    if eintrag.get_password().is_ok_and(|gelesen| gelesen == wert) {
        Ok(())
    } else {
        Err("Der gespeicherte Gemini-Schlüssel konnte nicht zurückgelesen werden.".into())
    }
}

/// Ausserhalb von Windows gibt es keinen Ort fuer Schluessel.
///
/// # Errors
/// Immer (nach der Pruefung).
#[cfg(not(windows))]
pub fn ablegen(wer: Wer, text: &str) -> Result<(), String> {
    wer.pruefe(text)?;
    let _ = BENUTZER;
    Err("Schlüssel liegen nur in der Windows-Anmeldeinformationsverwaltung.".into())
}

/// Lädt ausschließlich zum Entfernen von Geheimnissen aus Diagnoseausgaben.
#[cfg(windows)]
pub(crate) fn fuer_protokoll() -> Result<Vec<String>, ()> {
    let _sperre = ANMELDEVERWALTUNG.lock().map_err(|_| ())?;
    let mut alle = Vec::new();
    for wer in ALLE.into_iter().chain([Wer::Gemini]) {
        match eintrag(wer).map_err(|_| ())?.get_password() {
            Ok(text) => alle.push(wer.pruefe(&text).map_err(|_| ())?),
            Err(keyring_core::Error::NoEntry) => {}
            Err(_) => return Err(()),
        }
    }
    Ok(alle)
}

#[cfg(not(windows))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "Gleiche fehlbare Schnittstelle wie unter Windows"
)]
pub(crate) fn fuer_protokoll() -> Result<Vec<String>, ()> {
    Ok(Vec::new())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Benötigt eine interaktive Windows-Anmeldung mit Anmeldeinformationsverwaltung"]
    fn gemini_wird_lokal_gespeichert_und_ist_neu_lesbar() -> Result<(), String> {
        let jetzt = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?;
        let ziel = format!(
            "DepotSchlopf/Test/gemini/{}-{}",
            std::process::id(),
            jetzt.as_nanos()
        );
        let eintrag = windows_eintrag(Wer::Gemini, &ziel)?;
        let probe = "AQ.Nur_Ein_Erfundener_Testwert";
        let ergebnis = (|| -> Result<(), String> {
            gemini_ablegen(&eintrag, probe)?;
            let leser = windows_eintrag(Wer::Gemini, &ziel)?;
            if !leser.get_password().is_ok_and(|wert| wert == probe) {
                return Err("Der neue Eintrag kann nicht zurückgelesen werden.".into());
            }
            let merkmale = leser.get_attributes().map_err(|e| e.to_string())?;
            if merkmale.get("persistence").map(String::as_str) != Some("Local") {
                return Err("Die Anmeldeinformation wurde nicht lokal gespeichert.".into());
            }
            Ok(())
        })();
        let geloescht = eintrag.delete_credential().map_err(|e| e.to_string());
        ergebnis.and(geloescht)
    }
}
