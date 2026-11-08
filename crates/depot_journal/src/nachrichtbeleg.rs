//! Belegte Nachrichten (R40, R12): Jede Nachricht von aussen steht vor ihrer Verwendung im
//! Journal, mit Quelle, Abrufzeit, Abdruck der rohen Antwort und Zeit der Veroeffentlichung.
//!
//! Ein Eintrag `nachricht` steht als `quelle;abruf;roh;zeit;text`. Wie Kurs- und Terminbelege
//! beginnt er keinen Tag: Der Sammler schreibt ihn vor dem Papiertag ins selbe Journal. Eine
//! Nachricht, die schon genau so belegt ist (gleiche Zeit, gleicher Text), wird nicht noch
//! einmal geschrieben. Die Abrufe fallen nie zurueck.

use depot_kern::Zeitpunkt;
use sha2::{Digest, Sha256};

use crate::datei::hex;
use crate::kursbeleg::{abdruck_gueltig, quelle_gueltig};

/// Hoechstlaenge des Texts einer Nachricht in Zeichen.
pub const MAX_NACHRICHT: usize = 1_000;

/// Eine belegte Nachricht.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NachrichtBeleg {
    quelle: String,
    abruf: Zeitpunkt,
    roh: String,
    zeit: Zeitpunkt,
    text: String,
}

impl NachrichtBeleg {
    /// Eine Nachricht aus der rohen Antwort `roh` der Quelle `quelle`, erhalten um `abruf`,
    /// veroeffentlicht um `zeit`.
    ///
    /// # Errors
    /// Ungueltiger Name der Quelle; Text leer, mit Rand aus Leerraum, mit Steuerzeichen oder
    /// laenger als `MAX_NACHRICHT` Zeichen; Veroeffentlichung nach dem Abruf.
    pub fn neu(
        quelle: &str,
        abruf: Zeitpunkt,
        roh: &[u8],
        zeit: Zeitpunkt,
        text: &str,
    ) -> Result<NachrichtBeleg, String> {
        NachrichtBeleg::pruefe(quelle, abruf, hex(&Sha256::digest(roh)), zeit, text)
    }

    fn pruefe(
        quelle: &str,
        abruf: Zeitpunkt,
        roh: String,
        zeit: Zeitpunkt,
        text: &str,
    ) -> Result<NachrichtBeleg, String> {
        if !quelle_gueltig(quelle) {
            return Err(format!("Quelle `{quelle}` ungültig"));
        }
        if !abdruck_gueltig(&roh) {
            return Err("Abdruck der Antwort ungültig".into());
        }
        if text.is_empty() || text.trim() != text {
            return Err("Nachricht leer oder mit Leerraum am Rand".into());
        }
        if text.chars().any(char::is_control) {
            return Err("Steuerzeichen in der Nachricht".into());
        }
        if text.chars().nth(MAX_NACHRICHT).is_some() {
            return Err(format!("Nachricht länger als {MAX_NACHRICHT} Zeichen"));
        }
        if zeit > abruf {
            return Err(format!("Nachricht von {zeit} nach dem Abruf {abruf}"));
        }
        Ok(NachrichtBeleg {
            quelle: quelle.to_string(),
            abruf,
            roh,
            zeit,
            text: text.to_string(),
        })
    }

    pub fn quelle(&self) -> &str {
        &self.quelle
    }

    pub fn abruf(&self) -> Zeitpunkt {
        self.abruf
    }

    /// SHA-256 der rohen Antwort, Kleinbuchstaben-Hex.
    pub fn roh(&self) -> &str {
        &self.roh
    }

    /// Zeit der Veroeffentlichung (UTC).
    pub fn zeit(&self) -> Zeitpunkt {
        self.zeit
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Dieselbe Nachricht: gleiche Zeit, gleicher Text.
    pub fn gleich(&self, andere: &NachrichtBeleg) -> bool {
        self.zeit == andere.zeit && self.text == andere.text
    }
}

pub(crate) fn nachrichtbeleg_text(b: &NachrichtBeleg) -> String {
    format!("{};{};{};{};{}", b.quelle, b.abruf, b.roh, b.zeit, b.text)
}

pub(crate) fn lies_nachrichtbeleg(inhalt: &str) -> Result<NachrichtBeleg, String> {
    let teile: Vec<&str> = inhalt.splitn(5, ';').collect();
    let [quelle, abruf, roh, zeit, text] = teile.as_slice() else {
        return Err("Nachricht: fünf Felder erwartet".into());
    };
    let abruf = Zeitpunkt::lies(abruf).ok_or("Nachricht: Abruf unlesbar")?;
    let zeit = Zeitpunkt::lies(zeit).ok_or("Nachricht: Zeit unlesbar")?;
    NachrichtBeleg::pruefe(quelle, abruf, (*roh).to_string(), zeit, text)
}
