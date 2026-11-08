//! Belegte Kurse (R12, R20, Kap. 6): Jeder Kurs, der von aussen kommt, steht vor seiner
//! Verwendung im Journal, mit Quelle, Abrufzeit und dem Abdruck der rohen Antwort.
//!
//! Ein Eintrag `kurs` steht als `quelle;herkunft;abruf;roh;isin;preis;zeitpunkt`:
//!
//! - `quelle`: Name der Quelle, Kleinbuchstaben, Ziffern und `-`.
//! - `herkunft`: `Herkunft::name` des Kurses.
//! - `abruf`: wann der Rahmen die Antwort erhalten hat, in UTC (`Zeitpunkt`).
//! - `roh`: SHA-256 der rohen Antwort in Kleinbuchstaben-Hex. Wer die Antwort aufbewahrt, kann
//!   sie damit dem Kurs zuordnen; das Journal haelt nur den Abdruck.
//! - `isin`, `preis` in Cent, `zeitpunkt` des Kurses in UTC; sein Handelstag ist der Tag dieses
//!   Zeitpunkts. Ein Preis feiner als ein Cent steht exakt mit Nachkommastellen (`4115.5`,
//!   hoechstens vier), ohne Null am Ende (`Preis::cent_text`); ganze Cent stehen wie bisher ohne
//!   Punkt. Nichts wird gerundet.
//!
//! Der Kurs liegt nie nach dem Abruf, und die Abrufe im Journal fallen nie zurueck.

use depot_kern::{Herkunft, Isin, Kurs, Preis, Zeitpunkt};
use sha2::{Digest, Sha256};

use crate::datei::hex;

/// Ein belegter Kurs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KursBeleg {
    quelle: String,
    abruf: Zeitpunkt,
    roh: String,
    isin: Isin,
    kurs: Kurs,
}

pub(crate) fn quelle_gueltig(quelle: &str) -> bool {
    !quelle.is_empty()
        && quelle
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub(crate) fn abdruck_gueltig(roh: &str) -> bool {
    roh.len() == 64
        && roh
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl KursBeleg {
    /// Ein Kurs aus der rohen Antwort `roh` der Quelle `quelle`, erhalten um `abruf`.
    ///
    /// # Errors
    /// Ungueltiger Name der Quelle, oder der Kurs liegt nach dem Abruf.
    pub fn neu(
        quelle: &str,
        abruf: Zeitpunkt,
        roh: &[u8],
        isin: Isin,
        kurs: Kurs,
    ) -> Result<KursBeleg, String> {
        KursBeleg::pruefe(quelle, abruf, hex(&Sha256::digest(roh)), isin, kurs)
    }

    fn pruefe(
        quelle: &str,
        abruf: Zeitpunkt,
        roh: String,
        isin: Isin,
        kurs: Kurs,
    ) -> Result<KursBeleg, String> {
        if !quelle_gueltig(quelle) {
            return Err(format!("Quelle `{quelle}` ungültig"));
        }
        if !abdruck_gueltig(&roh) {
            return Err("Abdruck der Antwort ungültig".into());
        }
        if kurs.zeitpunkt() > abruf {
            return Err(format!("Kurs von {isin} nach seinem Abruf"));
        }
        Ok(KursBeleg {
            quelle: quelle.to_string(),
            abruf,
            roh,
            isin,
            kurs,
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

    pub fn isin(&self) -> &Isin {
        &self.isin
    }

    pub fn kurs(&self) -> &Kurs {
        &self.kurs
    }
}

pub(crate) fn kursbeleg_text(b: &KursBeleg) -> String {
    format!(
        "{};{};{};{};{};{};{}",
        b.quelle,
        b.kurs.herkunft().name(),
        b.abruf,
        b.roh,
        b.isin,
        b.kurs.preis().cent_text(),
        b.kurs.zeitpunkt()
    )
}

pub(crate) fn lies_kursbeleg(inhalt: &str) -> Result<KursBeleg, String> {
    let teile: Vec<&str> = inhalt.split(';').collect();
    let [quelle, herkunft, abruf, roh, isin, preis, zeitpunkt] = teile.as_slice() else {
        return Err("Kurs: sieben Felder erwartet".into());
    };
    let herkunft = Herkunft::aus_name(herkunft).ok_or("Kurs: Herkunft unbekannt")?;
    let abruf = Zeitpunkt::lies(abruf).ok_or("Kurs: Abruf unlesbar")?;
    let isin = Isin::neu(isin).map_err(|e| format!("Kurs: ISIN {e:?}"))?;
    let preis = Preis::lies_cent_text(preis).ok_or("Kurs: Preis unlesbar")?;
    let zeitpunkt = Zeitpunkt::lies(zeitpunkt).ok_or("Kurs: Zeitpunkt unlesbar")?;
    let tag = zeitpunkt.datum().ok_or("Kurs: Zeitpunkt ohne Tag")?;
    let kurs = Kurs::exakt(preis, "EUR", tag, zeitpunkt, herkunft)
        .map_err(|e| format!("Kurs von {isin}: {e:?}"))?;
    KursBeleg::pruefe(quelle, abruf, (*roh).to_string(), isin, kurs)
}
