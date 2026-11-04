//! Der lesende Abruf ueber HTTPS. Nur `GET`, nur HTTPS, keine Weiterleitung, feste Zeitgrenze
//! und feste Hoechstgroesse der Antwort. Ein Status ausser 200 ist ein Fehler.

use std::time::Duration;

/// Hoechstgroesse einer Antwort. Die taegliche Kursreihe eines Werts hat rund 6 KB.
pub const MAX_ANTWORT: u64 = 1024 * 1024;
/// Zeitgrenze eines Abrufs vom Verbindungsaufbau bis zum letzten Byte.
pub const ZEITGRENZE: Duration = Duration::from_secs(30);

/// Holt die rohe Antwort einer Adresse. In Tests ersetzt eine erfundene Quelle das Netz.
pub trait Netz {
    /// # Errors
    /// Jeder Fehler beim Abruf. Die Meldung darf die Adresse enthalten; der Sammler bereinigt
    /// sie vor jeder Ausgabe vom Schluessel.
    fn hole(&mut self, adresse: &str) -> Result<Vec<u8>, String>;
}

/// Abruf ueber HTTPS.
#[derive(Debug)]
pub struct Https(ureq::Agent);

impl Https {
    pub fn neu() -> Https {
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .timeout_global(Some(ZEITGRENZE))
            .http_status_as_error(true)
            .user_agent("DepotSchlopf")
            .build();
        Https(config.into())
    }
}

impl Default for Https {
    fn default() -> Https {
        Https::neu()
    }
}

impl Netz for Https {
    fn hole(&mut self, adresse: &str) -> Result<Vec<u8>, String> {
        let mut antwort = self.0.get(adresse).call().map_err(|e| e.to_string())?;
        if antwort.status() != 200 {
            return Err(format!("Status {}", antwort.status()));
        }
        antwort
            .body_mut()
            .with_config()
            .limit(MAX_ANTWORT)
            .read_to_vec()
            .map_err(|e| e.to_string())
    }
}
