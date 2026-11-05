//! Die Schluessel der Quellen. Sie liegen nur in der Windows-Anmeldeinformationsverwaltung, als
//! generische Anmeldeinformation mit der Adresse der Quelle (`Anbieter::adresse`). Wolfgang legt
//! jeden dort einmal an, oder in der Einrichtung:
//!
//! ```text
//! cmdkey /generic:DepotSchlopf/alphavantage /user:depotschlopf /pass
//! cmdkey /generic:DepotSchlopf/tiingo /user:depotschlopf /pass
//! ```
//!
//! `cmdkey` fragt den Schluessel dann verdeckt ab; er erscheint weder auf dem Bildschirm noch in
//! der Befehlsgeschichte.
//!
//! Ein Schluessel verlaesst dieses Modul nur als Teil der Abrufadresse. Er hat keine Textform:
//! `Debug` zeigt `***`, und jede Meldung des Sammlers wird vor der Ausgabe von ihm bereinigt.

use std::fmt;

/// Adresse des Schluessels von Alpha Vantage in der Anmeldeinformationsverwaltung.
pub const ADRESSE: &str = "DepotSchlopf/alphavantage";

/// Eine Quelle mit eigenem Schluessel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Anbieter {
    /// Xetra-Werte. Schluessel: 8 bis 64 Grossbuchstaben und Ziffern.
    AlphaVantage,
    /// Werte der NYSE. Schluessel: 20 bis 64 Kleinbuchstaben und Ziffern (Tiingo vergibt 40
    /// Hexadezimalzeichen).
    Tiingo,
}

impl Anbieter {
    /// Alle Quellen mit Schluessel.
    pub const ALLE: [Anbieter; 2] = [Anbieter::AlphaVantage, Anbieter::Tiingo];

    /// Name im Anbieterprotokoll: je Quelle eigene Zeilen.
    pub fn protokoll(self) -> depot_journal::anbieter::Wer {
        match self {
            Anbieter::AlphaVantage => depot_journal::anbieter::Wer::Kursquelle,
            Anbieter::Tiingo => depot_journal::anbieter::Wer::Tiingo,
        }
    }

    /// Die Quelle fuer die Werte einer Boerse, wie `depot_kern::liste::quelle`.
    pub fn fuer(boerse: depot_kern::Boerse) -> Anbieter {
        match boerse {
            depot_kern::Boerse::Xetra => Anbieter::AlphaVantage,
            depot_kern::Boerse::Nyse => Anbieter::Tiingo,
        }
    }

    /// Name im Speicher und in der Ablage, wie `depot_kern::liste::quelle`.
    pub fn dienst(self) -> &'static str {
        match self {
            Anbieter::AlphaVantage => depot_kern::liste::QUELLE,
            Anbieter::Tiingo => depot_kern::liste::QUELLE_US,
        }
    }

    /// Adresse in der Anmeldeinformationsverwaltung.
    pub fn adresse(self) -> &'static str {
        match self {
            Anbieter::AlphaVantage => ADRESSE,
            Anbieter::Tiingo => "DepotSchlopf/tiingo",
        }
    }

    /// Ist das ein gueltiger Schluessel dieser Quelle? Nur Buchstaben und Ziffern, damit er die
    /// Abrufadresse nicht veraendern kann.
    fn gueltig(self, t: &str) -> Result<(), String> {
        let (laenge, zeichen, art): (std::ops::RangeInclusive<usize>, fn(u8) -> bool, &str) =
            match self {
                Anbieter::AlphaVantage => (
                    8..=64,
                    |b| b.is_ascii_uppercase() || b.is_ascii_digit(),
                    "Grossbuchstaben und Ziffern",
                ),
                Anbieter::Tiingo => (
                    20..=64,
                    |b| b.is_ascii_lowercase() || b.is_ascii_digit(),
                    "Kleinbuchstaben und Ziffern",
                ),
            };
        if !laenge.contains(&t.len()) {
            return Err(format!(
                "Schluessel {}: {} bis {} Zeichen erwartet",
                self.dienst(),
                laenge.start(),
                laenge.end()
            ));
        }
        if !t.bytes().all(zeichen) {
            return Err(format!("Schluessel {}: nur {art} erlaubt", self.dienst()));
        }
        Ok(())
    }
}

/// Ein geprueft gueltiger Schluessel einer Quelle (`Anbieter`). Mehr als Buchstaben und Ziffern
/// braucht keine Quelle, und so kann er die Abrufadresse nicht veraendern.
#[derive(Clone, PartialEq, Eq)]
pub struct Schluessel(String);

impl fmt::Debug for Schluessel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Schluessel(***)")
    }
}

impl Schluessel {
    /// Ein Schluessel von Alpha Vantage. Leerraum am Rand wird entfernt; sonst gilt der Text
    /// genau so.
    ///
    /// # Errors
    /// Laenge oder Zeichen passen nicht. Die Meldung nennt den Schluessel nicht.
    pub fn neu(text: &str) -> Result<Schluessel, String> {
        Schluessel::fuer(Anbieter::AlphaVantage, text)
    }

    /// Ein Schluessel dieser Quelle. Leerraum am Rand wird entfernt; sonst gilt der Text genau
    /// so.
    ///
    /// # Errors
    /// Laenge oder Zeichen passen nicht. Die Meldung nennt den Schluessel nicht.
    pub fn fuer(anbieter: Anbieter, text: &str) -> Result<Schluessel, String> {
        let t = text.trim();
        anbieter.gueltig(t)?;
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
        // Verglichen wird gross mit gross, gleich wie der Schluessel geschrieben ist.
        let gross = text.to_ascii_uppercase();
        let gesucht = self.0.to_ascii_uppercase();
        let mut aus = String::with_capacity(text.len());
        let mut rest = text;
        let mut rest_gross = gross.as_str();
        while let Some(pos) = rest_gross.find(gesucht.as_str()) {
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

    /// Liest den Schluessel einer Quelle aus der Windows-Anmeldeinformationsverwaltung.
    ///
    /// # Errors
    /// Kein Eintrag unter `Anbieter::adresse`, nicht lesbar oder ungueltig.
    #[cfg(windows)]
    pub fn aus_anmeldeverwaltung(anbieter: Anbieter) -> Result<Schluessel, String> {
        use std::collections::HashMap;

        use keyring_core::api::CredentialStoreApi;
        use windows_native_keyring_store::Store;

        let adresse = anbieter.adresse();
        let hinweis = || {
            format!(
                "Schluessel fehlt. In der Einrichtung hinterlegen oder einmal anlegen mit: cmdkey /generic:{adresse} /user:depotschlopf /pass"
            )
        };
        let store = Store::new().map_err(|e| format!("Anmeldeverwaltung: {e}"))?;
        let ziel = HashMap::from([("target", adresse)]);
        let eintrag = store
            .build(anbieter.dienst(), "depotschlopf", Some(&ziel))
            .map_err(|e| format!("Anmeldeverwaltung: {e}"))?;
        match eintrag.get_password() {
            Ok(text) => Schluessel::fuer(anbieter, &text),
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
    pub fn aus_anmeldeverwaltung(_anbieter: Anbieter) -> Result<Schluessel, String> {
        Err("Der Schluessel liegt nur in der Windows-Anmeldeinformationsverwaltung.".into())
    }
}

/// Die Schluessel eines Laufs, je Quelle. Fehlt der Schluessel einer Quelle, werden ihre Werte
/// nicht abgerufen; die anderen schon. Jede Antwort wird gegen alle Schluessel geprueft, und
/// jede Meldung wird von allen bereinigt.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Zugang {
    schluessel: std::collections::BTreeMap<Anbieter, Schluessel>,
}

impl Zugang {
    /// Kein Schluessel.
    pub fn leer() -> Zugang {
        Zugang::default()
    }

    /// Nur der Schluessel von Alpha Vantage.
    pub fn alpha_vantage(s: Schluessel) -> Zugang {
        Zugang::leer().mit(Anbieter::AlphaVantage, s)
    }

    /// Dazu der Schluessel einer weiteren Quelle.
    #[must_use]
    pub fn mit(mut self, anbieter: Anbieter, s: Schluessel) -> Zugang {
        self.schluessel.insert(anbieter, s);
        self
    }

    /// Der Schluessel einer Quelle, wenn es ihn gibt.
    pub fn von(&self, anbieter: Anbieter) -> Option<&Schluessel> {
        self.schluessel.get(&anbieter)
    }

    /// Steht einer der Schluessel in diesen Bytes, gleich in welcher Schreibweise?
    pub fn steht_in(&self, bytes: &[u8]) -> bool {
        self.schluessel.values().any(|s| s.steht_in(bytes))
    }

    /// Ersetzt jeden Schluessel durch `***`.
    pub fn bereinige(&self, text: &str) -> String {
        self.schluessel
            .values()
            .fold(text.to_string(), |t, s| s.bereinige(&t))
    }
}

/// Laedt die Schluessel fuer einen Lauf: **alle** hinterlegten, auch die der Quellen, die dieser
/// Lauf nicht abruft, damit jede Antwort gegen jeden Schluessel geprueft wird (Maus, Befund 1 zu
/// #37). Ein fehlender oder unlesbarer Schluessel wird nur gemeldet, wenn ein Wert des Laufs ihn
/// braucht; dann werden nur dessen Werte nicht abgerufen. Die Meldungen nennen keinen Schluessel.
pub fn zugang_laden(
    werte: &[depot_kern::liste::Wert],
    laden: impl Fn(Anbieter) -> Result<Schluessel, String>,
) -> (Zugang, Vec<String>) {
    let mut zugang = Zugang::leer();
    let mut meldungen = Vec::new();
    for a in Anbieter::ALLE {
        match laden(a) {
            Ok(s) => zugang = zugang.mit(a, s),
            Err(e) => {
                if werte.iter().any(|w| Anbieter::fuer(w.boerse) == a) {
                    meldungen.push(format!("{}: {e}", a.dienst()));
                }
            }
        }
    }
    (zugang, meldungen)
}
