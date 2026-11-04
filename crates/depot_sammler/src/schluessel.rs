//! Der Schluessel der Quelle. Er liegt nur in der Windows-Anmeldeinformationsverwaltung, als
//! generische Anmeldeinformation mit der Adresse `ADRESSE`. Wolfgang legt ihn dort einmal an:
//!
//! ```text
//! cmdkey /generic:DepotSchlopf/alphavantage /user:depotschlopf /pass
//! ```
//!
//! `cmdkey` fragt den Schluessel dann verdeckt ab; er erscheint weder auf dem Bildschirm noch in
//! der Befehlsgeschichte.
//!
//! Der Schluessel verlaesst dieses Modul nur als Teil der Abrufadresse. Er hat keine Textform:
//! `Debug` zeigt `***`, und jede Meldung des Sammlers wird vor der Ausgabe von ihm bereinigt.

use std::fmt;

/// Adresse in der Anmeldeinformationsverwaltung.
pub const ADRESSE: &str = "DepotSchlopf/alphavantage";

/// Ein geprueft gueltiger Schluessel: 8 bis 64 Grossbuchstaben und Ziffern. Mehr braucht die
/// Quelle nicht, und so kann er die Abrufadresse nicht veraendern.
#[derive(Clone, PartialEq, Eq)]
pub struct Schluessel(String);

impl fmt::Debug for Schluessel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Schluessel(***)")
    }
}

impl Schluessel {
    /// Leerraum am Rand wird entfernt; sonst gilt der Text genau so.
    ///
    /// # Errors
    /// Laenge oder Zeichen passen nicht. Die Meldung nennt den Schluessel nicht.
    pub fn neu(text: &str) -> Result<Schluessel, String> {
        let t = text.trim();
        if !(8..=64).contains(&t.len()) {
            return Err("Schluessel: 8 bis 64 Zeichen erwartet".into());
        }
        if !t
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        {
            return Err("Schluessel: nur Grossbuchstaben und Ziffern erlaubt".into());
        }
        Ok(Schluessel(t.to_string()))
    }

    pub(crate) fn als_str(&self) -> &str {
        &self.0
    }

    /// Steht der Schluessel in diesen Bytes, gleich in welcher Schreibweise?
    pub fn steht_in(&self, bytes: &[u8]) -> bool {
        let k = self.0.as_bytes();
        bytes.windows(k.len()).any(|w| w.eq_ignore_ascii_case(k))
    }

    /// Ersetzt jedes Vorkommen des Schluessels durch `***`, gleich in welcher Schreibweise.
    pub fn bereinige(&self, text: &str) -> String {
        // Der Schluessel ist ASCII; Grossschreibung von ASCII laesst jede Byteposition gleich.
        let gross = text.to_ascii_uppercase();
        let mut aus = String::with_capacity(text.len());
        let mut rest = text;
        let mut rest_gross = gross.as_str();
        while let Some(pos) = rest_gross.find(self.0.as_str()) {
            let (Some(vor), Some(nach), Some(nach_gross)) = (
                rest.get(..pos),
                pos.checked_add(self.0.len()).and_then(|e| rest.get(e..)),
                pos.checked_add(self.0.len())
                    .and_then(|e| rest_gross.get(e..)),
            ) else {
                return "***".into();
            };
            aus.push_str(vor);
            aus.push_str("***");
            rest = nach;
            rest_gross = nach_gross;
        }
        aus.push_str(rest);
        aus
    }

    /// Liest den Schluessel aus der Windows-Anmeldeinformationsverwaltung.
    ///
    /// # Errors
    /// Kein Eintrag unter `ADRESSE`, nicht lesbar oder ungueltig.
    #[cfg(windows)]
    pub fn aus_anmeldeverwaltung() -> Result<Schluessel, String> {
        use std::collections::HashMap;

        use keyring_core::api::CredentialStoreApi;
        use windows_native_keyring_store::Store;

        let hinweis = || {
            format!(
                "Schluessel fehlt. Einmal anlegen mit: cmdkey /generic:{ADRESSE} /user:depotschlopf /pass"
            )
        };
        let store = Store::new().map_err(|e| format!("Anmeldeverwaltung: {e}"))?;
        let ziel = HashMap::from([("target", ADRESSE)]);
        let eintrag = store
            .build("alphavantage", "depotschlopf", Some(&ziel))
            .map_err(|e| format!("Anmeldeverwaltung: {e}"))?;
        match eintrag.get_password() {
            Ok(text) => Schluessel::neu(&text),
            Err(keyring_core::Error::NoEntry) => Err(hinweis()),
            Err(_) => Err(format!(
                "Anmeldeverwaltung: Eintrag {ADRESSE} nicht lesbar. {}",
                hinweis()
            )),
        }
    }

    /// Ausserhalb von Windows gibt es keinen Ort fuer den Schluessel.
    ///
    /// # Errors
    /// Immer.
    #[cfg(not(windows))]
    pub fn aus_anmeldeverwaltung() -> Result<Schluessel, String> {
        Err("Der Schluessel liegt nur in der Windows-Anmeldeinformationsverwaltung.".into())
    }
}
