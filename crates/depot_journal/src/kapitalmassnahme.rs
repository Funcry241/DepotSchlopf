//! Wertneutrale Split-Umbuchung und ihre Reihenfolge zwischen Ausführungen.
use depot_kern::{Boerse, Cent, Datum, Herkunft, Isin, Preis, TopfId, abdruck::sha256_hex};

use crate::{AuftragsBeleg, Ausfuehrung, NachrichtBeleg, Stand};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Splitfaktor {
    zaehler: u64,
    nenner: u64,
}

impl Splitfaktor {
    pub fn neu(zaehler: u64, nenner: u64) -> Option<Self> {
        (zaehler != 0 && nenner != 0 && zaehler != nenner).then_some(Self { zaehler, nenner })
    }

    fn dezimal(text: &str) -> Option<Self> {
        let (ganz, rest) = text.split_once('.').unwrap_or((text, ""));
        if ganz.is_empty() || !ganz.bytes().chain(rest.bytes()).all(|b| b.is_ascii_digit()) {
            return None;
        }
        let nenner = 10_u64.checked_pow(u32::try_from(rest.len()).ok()?)?;
        let ganz: u64 = ganz.parse().ok()?;
        let rest = if rest.is_empty() {
            0
        } else {
            rest.parse().ok()?
        };
        let zaehler = ganz.checked_mul(nenner)?.checked_add(rest)?;
        let (mut a, mut b) = (zaehler, nenner);
        while b != 0 {
            (a, b) = (b, a.checked_rem(b)?);
        }
        Self::neu(zaehler.checked_div(a)?, nenner.checked_div(a)?)
    }

    pub fn stueck(self, vorher: u64) -> Option<u64> {
        let produkt = u128::from(vorher).checked_mul(u128::from(self.zaehler))?;
        let nenner = u128::from(self.nenner);
        if produkt.checked_rem(nenner)? != 0 {
            return None;
        }
        u64::try_from(produkt.checked_div(nenner)?).ok()
    }

    pub fn vergleichskurs(self, vorher: Preis) -> Option<Preis> {
        let produkt = i128::from(vorher.fein()).checked_mul(i128::from(self.nenner))?;
        let danach = i64::try_from(produkt.checked_div(i128::from(self.zaehler))?).ok()?;
        (danach > 0).then_some(Preis::aus_fein(danach))
    }

    pub(crate) fn stopp(self, vorher: Cent) -> Option<Cent> {
        let produkt = i128::from(vorher.0).checked_mul(i128::from(self.nenner))?;
        let zaehler = i128::from(self.zaehler);
        let danach = produkt
            .checked_div(zaehler)?
            .checked_add(i128::from(produkt.checked_rem(zaehler)? != 0))?;
        let danach = i64::try_from(danach).ok()?;
        (danach > 0).then_some(Cent(danach))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kapitalmassnahme {
    pub topf: TopfId,
    pub isin: Isin,
    pub faktor: Splitfaktor,
    pub wirksam: Datum,
    /// Kennung des geprüften Quellenbelegs.
    pub quelle: String,
}

impl Kapitalmassnahme {
    pub(crate) fn text(&self) -> String {
        format!(
            "{};{};art=split;{};{};{};{}",
            crate::stand::topf_name(self.topf),
            self.isin,
            self.faktor.zaehler,
            self.faktor.nenner,
            self.wirksam,
            self.quelle
        )
    }

    pub(crate) fn lies(text: &str) -> Option<Self> {
        let teile: Vec<_> = text.split(';').collect();
        let [topf, isin, "art=split", zaehler, nenner, wirksam, quelle] = teile.as_slice() else {
            return None;
        };
        if !crate::kursbeleg::abdruck_gueltig(quelle) {
            return None;
        }
        Some(Self {
            topf: crate::stand::lies_topf_name(topf)?,
            isin: Isin::neu(isin).ok()?,
            faktor: Splitfaktor::neu(zaehler.parse().ok()?, nenner.parse().ok()?)?,
            wirksam: Datum::lies(wirksam)?,
            quelle: (*quelle).into(),
        })
    }

    pub fn kennung(&self) -> String {
        sha256_hex(self.text().as_bytes())
    }

    /// Geprüfter Xetra-Split für den noch offenen, angezeigten Verdacht.
    pub fn xetra(stand: &Stand, topf: TopfId, kennung: &str) -> Option<Self> {
        let (isin, beleg) = crate::splitverdacht::aktiv(stand, None)
            .into_iter()
            .find(|(_, b)| crate::splitverdacht::kennung(b) == kennung)?;
        let (faktor, _) = beleg
            .text()
            .strip_prefix("Möglicher Split ")?
            .split_once(":1 bei ")?;
        let zaehler = faktor.parse().ok()?;
        if ![2, 3, 4, 5, 10].contains(&zaehler) {
            return None;
        }
        Some(Self {
            topf,
            isin,
            faktor: Splitfaktor::neu(zaehler, 1)?,
            wirksam: beleg.zeit().frankfurt_tag()?,
            quelle: kennung.to_owned(),
        })
    }
}

/// Liest die ISIN und den Wirktag aus einer vom Tiingo-Sammler belegten Splitmeldung.
pub fn tiingo_split(beleg: &NachrichtBeleg) -> Option<(Isin, Datum)> {
    if beleg.quelle() != depot_kern::tiingo::QUELLE_KAPITAL {
        return None;
    }
    let (wert, zusatz) = beleg
        .text()
        .strip_prefix("Tiingo: ")?
        .rsplit_once(") · Split-Faktor ")?;
    let (_, isin) = wert.rsplit_once(" (")?;
    let (_, tag) = zusatz.split_once(" am ")?;
    Some((Isin::neu(isin).ok()?, Datum::lies(tag)?))
}

impl Kapitalmassnahme {
    /// Ein Tiingo-Split braucht den Kurs desselben Werts und Handelstags aus derselben Rohantwort.
    pub fn tiingo(stand: &Stand, topf: TopfId, beleg: &NachrichtBeleg) -> Option<Self> {
        let (isin, wirksam) = tiingo_split(beleg)?;
        if Boerse::Nyse.schluss(wirksam)? != beleg.zeit()
            || !stand.kurse().iter().any(|b| {
                b.quelle() == depot_kern::liste::QUELLE_US
                    && b.isin() == &isin
                    && b.kurs().herkunft() == Herkunft::Quelle
                    && b.kurs().handelstag() == wirksam
                    && b.kurs().zeitpunkt() == beleg.zeit()
                    && b.roh() == beleg.roh()
                    && b.abruf() <= beleg.abruf()
            })
        {
            return None;
        }
        let (_, faktor) = beleg.text().rsplit_once(") · Split-Faktor ")?;
        let (faktor, _) = faktor.split_once(" am ")?;
        Some(Self {
            topf,
            isin,
            wirksam,
            faktor: Splitfaktor::dezimal(faktor)?,
            quelle: crate::splitverdacht::kennung(beleg),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GebuchteKapitalmassnahme {
    pub massnahme: Kapitalmassnahme,
    pub vorher_gesamt: u64,
    pub danach_gesamt: u64,
    pub(crate) ausfuehrungen_vorher: usize,
}

#[derive(Debug)]
pub enum Bewertungsschritt<'a> {
    Ausfuehrung(&'a AuftragsBeleg, &'a Ausfuehrung),
    Split(&'a Kapitalmassnahme),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiingo_faktor_skaliert_stopp_und_lehnt_gebrochene_stuecke_ab() -> Result<(), String> {
        let split = Splitfaktor::dezimal("4").ok_or("Splitfaktor")?;
        assert_eq!(split.stopp(Cent(39_601)), Some(Cent(9_901)));
        let reverse = Splitfaktor::dezimal("0.25").ok_or("Reverse-Splitfaktor")?;
        assert_eq!(reverse.stueck(40), Some(10));
        assert_eq!(reverse.stueck(41), None);
        assert_eq!(reverse.stopp(Cent(9_900)), Some(Cent(39_600)));
        assert!(Splitfaktor::dezimal("1.0").is_none());
        Ok(())
    }
}
