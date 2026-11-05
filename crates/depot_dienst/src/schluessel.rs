//! Schluessel in der Windows-Anmeldeinformationsverwaltung: nur ablegen und fragen, ob einer da
//! ist. Der Dienst liest nie einen Schluessel aus, zeigt ihn nie und schreibt ihn nirgends hin.
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
            Wer::Kursquelle => "alphavantage",
            Wer::KursquelleUs => "tiingo",
            Wer::Berater(BeraterId::Dachs) => "anthropic",
            Wer::Berater(BeraterId::Maus) => "openai",
        }
    }

    /// Adresse in der Anmeldeinformationsverwaltung.
    pub fn adresse(self) -> &'static str {
        match self {
            Wer::Kursquelle => "DepotSchlopf/alphavantage",
            Wer::KursquelleUs => "DepotSchlopf/tiingo",
            Wer::Berater(BeraterId::Dachs) => "DepotSchlopf/anthropic",
            Wer::Berater(BeraterId::Maus) => "DepotSchlopf/openai",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Wer::Kursquelle => "Kursquelle (Alpha Vantage)",
            Wer::KursquelleUs => "Kursquelle NYSE (Tiingo)",
            Wer::Berater(BeraterId::Dachs) => "Dachs (Claude, Anthropic)",
            Wer::Berater(BeraterId::Maus) => "Maus (GPT, OpenAI)",
        }
    }

    /// Name im Anbieterprotokoll.
    pub fn protokoll(self) -> depot_lage::anbieter::Wer {
        match self {
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
        ALLE.into_iter().find(|w| w.kurz() == k)
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
                    return Err("nur Grossbuchstaben und Ziffern erlaubt");
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
fn eintrag(wer: Wer) -> Result<keyring_core::Entry, String> {
    use std::collections::HashMap;

    use keyring_core::api::CredentialStoreApi;
    use windows_native_keyring_store::Store;

    let store = Store::new().map_err(|e| format!("Anmeldeverwaltung: {e}"))?;
    let ziel = HashMap::from([("target", wer.adresse())]);
    store
        .build(wer.dienst(), BENUTZER, Some(&ziel))
        .map_err(|e| format!("Anmeldeverwaltung: {e}"))
}

/// Ist ein Schluessel hinterlegt? Liest nur die Merkmale des Eintrags, nicht den Schluessel.
#[cfg(windows)]
pub fn status(wer: Wer) -> Hinterlegt {
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
    eintrag(wer)?
        .set_password(&s)
        .map_err(|_| format!("Anmeldeverwaltung: {} nicht gespeichert", wer.adresse()))
}

/// Ausserhalb von Windows gibt es keinen Ort fuer Schluessel.
///
/// # Errors
/// Immer (nach der Pruefung).
#[cfg(not(windows))]
pub fn ablegen(wer: Wer, text: &str) -> Result<(), String> {
    wer.pruefe(text)?;
    let _ = BENUTZER;
    Err("Schluessel liegen nur in der Windows-Anmeldeinformationsverwaltung.".into())
}
