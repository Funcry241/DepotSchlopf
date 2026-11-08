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
use crate::kursbeleg::{abdruck_gueltig, quelle_gueltig};

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
        TerminBeleg::pruefe(quelle, herkunft, abruf, hex(&Sha256::digest(roh)), termin)
    }

    fn pruefe(
        quelle: &str,
        herkunft: Herkunft,
        abruf: Zeitpunkt,
        roh: String,
        termin: Termin,
    ) -> Result<TerminBeleg, String> {
        if !quelle_gueltig(quelle) {
            return Err(format!("Quelle `{quelle}` ungültig"));
        }
        if !abdruck_gueltig(&roh) {
            return Err("Abdruck der Antwort ungültig".into());
        }
        Ok(TerminBeleg {
            quelle: quelle.to_string(),
            herkunft,
            abruf,
            roh,
            termin,
        })
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
    let t = &b.termin;
    let betroffen: Vec<String> = t.betroffen().iter().map(ToString::to_string).collect();
    format!(
        "{};{};{};{};{};{};{};{};{}",
        b.quelle,
        b.herkunft.name(),
        b.abruf,
        b.roh,
        t.kennung(),
        t.verlaesslichkeit().name(),
        t.zeit(),
        betroffen.join(","),
        t.titel()
    )
}

pub(crate) fn lies_terminbeleg(inhalt: &str) -> Result<TerminBeleg, String> {
    let teile: Vec<&str> = inhalt.splitn(9, ';').collect();
    let [
        quelle,
        herkunft,
        abruf,
        roh,
        kennung,
        verl,
        zeit,
        betroffen,
        titel,
    ] = teile.as_slice()
    else {
        return Err("Termin: neun Felder erwartet".into());
    };
    let herkunft = Herkunft::aus_name(herkunft).ok_or("Termin: Herkunft unbekannt")?;
    let abruf = Zeitpunkt::lies(abruf).ok_or("Termin: Abruf unlesbar")?;
    let verl = Verlaesslichkeit::aus_name(verl).ok_or("Termin: Verlässlichkeit unbekannt")?;
    let zeit = Zeitbezug::lies(zeit).ok_or("Termin: Zeit unlesbar")?;
    let betroffen = betroffen
        .split(',')
        .map(|b| Betroffen::lies(b).ok_or_else(|| format!("Termin: Betroffener `{b}` unlesbar")))
        .collect::<Result<Vec<_>, _>>()?;
    let termin = Termin::neu(kennung, titel, zeit, betroffen, verl)
        .map_err(|e| format!("Termin {kennung}: {e:?}"))?;
    TerminBeleg::pruefe(quelle, herkunft, abruf, (*roh).to_string(), termin)
}
