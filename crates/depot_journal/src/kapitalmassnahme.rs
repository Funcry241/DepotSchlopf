//! Wertneutrale Split-Umbuchung und ihre Reihenfolge zwischen Ausführungen.
use depot_kern::{Cent, Datum, Isin, Preis, TopfId, abdruck::sha256_hex};

use crate::{AuftragsBeleg, Ausfuehrung, Stand};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Splitfaktor {
    zaehler: u64,
    nenner: u64,
}

impl Splitfaktor {
    pub fn neu(zaehler: u64, nenner: u64) -> Option<Self> {
        (zaehler != 0 && nenner != 0 && zaehler != nenner).then_some(Self { zaehler, nenner })
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
