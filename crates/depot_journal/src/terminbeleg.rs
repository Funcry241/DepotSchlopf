//! Belegte Termine (R39, R12): Jeder Termin von aussen steht vor seiner Verwendung im Journal,
//! mit Quelle, Herkunft, Abrufzeit und Abdruck der rohen Antwort.
//!
//! Ein Eintrag `termin` steht als
//! `quelle;herkunft;abruf;roh;kennung;verlaesslichkeit;zeit;betroffen;titel`; die Felder des
//! Termins wie in `depot_kern::termin`, die uebrigen wie beim Kurs (`kursbeleg`).
//!
//! Verschiebt sich ein Termin oder aendert sich sonst etwas an ihm, wird er unter seiner
//! Kennung fortgeschrieben; die alte Fassung bleibt im Journal. Ein unveraenderter Termin wird
//! nicht noch einmal belegt. Die Abrufe fallen nie zurueck.

use depot_kern::{Betroffen, Herkunft, Termin, Verlaesslichkeit, Zeitbezug, Zeitpunkt};
use sha2::{Digest, Sha256};

use crate::datei::hex;

/// Was ein Beleg im Journal bewirkt hat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fortschreibung {
    /// Die Kennung war neu.
    Neu,
    /// Die Kennung war bekannt, der Termin hat sich geaendert; die alte Fassung bleibt.
    Geaendert,
    /// Der Termin war schon genau so belegt; nichts wurde geschrieben.
    Unveraendert,
}

/// Ein belegter Termin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminBeleg {
    quelle: String,
    herkunft: Herkunft,
    abruf: Zeitpunkt,
    roh: String,
    termin: Termin,
}

impl TerminBeleg {
    /// Ein Termin aus der rohen Antwort `roh` der Quelle `quelle`, erhalten um `abruf`.
    ///
    /// # Errors
    /// Ungueltiger Name der Quelle.
    pub fn neu(
        quelle: &str,
        herkunft: Herkunft,
        abruf: Zeitpunkt,
        roh: &[u8],
        termin: Termin,
    ) -> Result<TerminBeleg, String> {
        let _ = (quelle, herkunft, abruf, hex(&Sha256::digest(roh)), termin);
        Err("noch nicht gebaut".into())
    }

    pub fn quelle(&self) -> &str {
        &self.quelle
    }

    pub fn herkunft(&self) -> Herkunft {
        self.herkunft
    }

    pub fn abruf(&self) -> Zeitpunkt {
        self.abruf
    }

    /// SHA-256 der rohen Antwort, Kleinbuchstaben-Hex.
    pub fn roh(&self) -> &str {
        &self.roh
    }

    pub fn termin(&self) -> &Termin {
        &self.termin
    }
}

pub(crate) fn terminbeleg_text(b: &TerminBeleg) -> String {
    let _ = b;
    String::new()
}

pub(crate) fn lies_terminbeleg(inhalt: &str) -> Result<TerminBeleg, String> {
    let _ = (
        inhalt,
        Betroffen::lies,
        Verlaesslichkeit::aus_name,
        Zeitbezug::lies,
    );
    Err("noch nicht gebaut".into())
}
