//! Die Schluessel der Anbieter. Sie liegen nur in der Windows-Anmeldeinformationsverwaltung,
//! als generische Anmeldeinformationen. Wolfgang legt sie einmal an:
//!
//! ```text
//! cmdkey /generic:DepotSchlopf/anthropic /user:depotschlopf /pass
//! cmdkey /generic:DepotSchlopf/openai /user:depotschlopf /pass
//! ```
//!
//! `cmdkey` fragt den Schluessel verdeckt ab; er erscheint weder auf dem Bildschirm noch in der
//! Befehlsgeschichte. Ein Schluessel verlaesst dieses Crate nur im Kopf der Anfrage an seinen
//! Anbieter. Er hat keine Textform: `Debug` zeigt `***`, und jede Meldung wird vor der Ausgabe
//! von ihm bereinigt.

use std::fmt;

use crate::anbieter::Anbieter;

/// Ein geprueft gueltiger Schluessel: 20 bis 256 Zeichen aus Buchstaben, Ziffern, `-` und `_`.
/// So kann er den Kopf der Anfrage nicht veraendern.
#[derive(Clone, PartialEq, Eq)]
pub struct Schluessel(String);

impl fmt::Debug for Schluessel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Schlüssel(***)")
    }
}

impl Schluessel {
    /// Leerraum am Rand wird entfernt; sonst gilt der Text genau so.
    ///
    /// # Errors
    /// Laenge oder Zeichen passen nicht. Die Meldung nennt den Schluessel nicht.
    pub fn neu(text: &str) -> Result<Schluessel, String> {
        let t = text.trim();
        if !(20..=256).contains(&t.len()) {
            return Err("Schlüssel: 20 bis 256 Zeichen erwartet".into());
        }
        if !t
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err("Schlüssel: nur Buchstaben, Ziffern, - und _ erlaubt".into());
        }
        Ok(Schluessel(t.to_string()))
    }

    pub(crate) fn als_str(&self) -> &str {
        &self.0
    }

    /// Ersetzt jedes Vorkommen des Schluessels durch `***`.
    pub fn bereinige(&self, text: &str) -> String {
        depot_kern::geheimnis::mit_schluessel(text, &self.0)
    }

    pub fn aus_anmeldeverwaltung(anbieter: Anbieter) -> Result<Self, String> {
        Self::fuer_dienst(anbieter.dienst(), anbieter.adresse())
    }

    /// Liest den Schluessel eines Anbieters aus der Windows-Anmeldeinformationsverwaltung.
    ///
    /// # Errors
    /// Kein Eintrag, nicht lesbar oder ungueltig.
    #[cfg(windows)]
    pub(crate) fn fuer_dienst(dienst: &str, adresse: &str) -> Result<Schluessel, String> {
        use std::collections::HashMap;

        use keyring_core::api::CredentialStoreApi;
        use windows_native_keyring_store::Store;

        let hinweis = || {
            format!(
                "Schlüssel fehlt. Einmal anlegen mit: cmdkey /generic:{adresse} /user:depotschlopf /pass"
            )
        };
        let store = Store::new().map_err(|e| format!("Anmeldeverwaltung: {e}"))?;
        let ziel = HashMap::from([("target", adresse)]);
        let eintrag = store
            .build(dienst, "depotschlopf", Some(&ziel))
            .map_err(|e| format!("Anmeldeverwaltung: {e}"))?;
        match eintrag.get_password() {
            Ok(text) => Schluessel::neu(&text),
            Err(keyring_core::Error::NoEntry) => Err(hinweis()),
            Err(_) => Err(format!(
                "Anmeldeverwaltung: Eintrag {adresse} nicht lesbar. {}",
                hinweis()
            )),
        }
    }

    /// Ausserhalb von Windows gibt es keinen Ort fuer den Schluessel.
    ///
    /// # Errors
    /// Immer.
    #[cfg(not(windows))]
    pub(crate) fn fuer_dienst(_dienst: &str, adresse: &str) -> Result<Schluessel, String> {
        Err(format!(
            "Der Schlüssel {adresse} liegt nur in der Windows-Anmeldeinformationsverwaltung."
        ))
    }
}
