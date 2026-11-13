//! Belegte Bewegungen über die letzten 21 Handelstage eines Werts.

use std::collections::BTreeMap;

use depot_journal::Stand;
use depot_kern::{Datum, Herkunft, Isin, Zeitpunkt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verlauf {
    ZuWenig,
    Da {
        t1_bp: i64,
        t5_bp: i64,
        t20_bp: i64,
        unter_hoch_bp: i64,
        ueber_tief_bp: i64,
    },
}

pub fn kursverlauf(stand: &Stand, isin: &Isin, bis: Datum, wissen: Zeitpunkt) -> Verlauf {
    berechne(stand, isin, bis, wissen).unwrap_or(Verlauf::ZuWenig)
}

fn berechne(stand: &Stand, isin: &Isin, bis: Datum, wissen: Zeitpunkt) -> Option<Verlauf> {
    let boerse = depot_kern::liste::boerse_von(isin)?;
    let mut tag = bis;
    let mut tage = vec![tag];
    for _ in 0..20 {
        tag = boerse.bezugstag(tag)?;
        tage.push(tag);
    }
    let mut belege = BTreeMap::new();
    for beleg in stand.kurse().iter().filter(|b| {
        b.isin() == isin
            && b.abruf() <= wissen
            && b.kurs().herkunft() == Herkunft::Quelle
            && b.kurs().handelstag() >= tag
            && b.kurs().handelstag() <= bis
    }) {
        let tag = beleg.kurs().handelstag();
        if boerse.handelstag(tag) {
            let eintrag = belege
                .entry(tag)
                .or_insert((beleg.abruf(), beleg.kurs().preis()));
            if beleg.abruf() >= eintrag.0 {
                *eintrag = (beleg.abruf(), beleg.kurs().preis());
            }
        }
    }
    let preise: Vec<_> = tage
        .iter()
        .map(|tag| belege.get(tag).map(|(_, preis)| *preis))
        .collect::<Option<_>>()?;
    let aktuell = *preise.first()?;
    let bewegung = |basis: depot_kern::Preis| {
        if basis.fein() <= 0 {
            return None;
        }
        let differenz = i128::from(aktuell.fein()).checked_sub(i128::from(basis.fein()))?;
        i64::try_from(
            differenz
                .checked_mul(10_000)?
                .checked_div(i128::from(basis.fein()))?,
        )
        .ok()
    };
    let hoch = *preise.iter().max()?;
    let tief = *preise.iter().min()?;
    Some(Verlauf::Da {
        t1_bp: bewegung(*preise.get(1)?)?,
        t5_bp: bewegung(*preise.get(5)?)?,
        t20_bp: bewegung(*preise.get(20)?)?,
        unter_hoch_bp: bewegung(hoch)?.checked_neg()?,
        ueber_tief_bp: bewegung(tief)?,
    })
}

pub(crate) fn fuer_lage(
    stand: &Stand,
    kurse: &depot_kern::Kurse,
    universum: &depot_kern::Universum,
    lauf_tag: Datum,
) -> Vec<(Isin, Verlauf)> {
    let wissen = stand
        .wissensstichtag(lauf_tag)
        .unwrap_or_else(|| kurse.stand());
    universum
        .isins()
        .into_iter()
        .map(|isin| {
            let bis = kurse
                .eintrag(&isin)
                .map(depot_kern::Kurs::handelstag)
                .or_else(|| depot_kern::liste::boerse_von(&isin)?.bezugstag(lauf_tag));
            let verlauf = bis.map_or(Verlauf::ZuWenig, |bis| {
                kursverlauf(stand, &isin, bis, wissen)
            });
            (isin, verlauf)
        })
        .collect()
}

/// Börsentage nach dem Kaufdatum bis einschließlich des Kursstichtags.
pub fn haltedauer(isin: &Isin, kauf: Datum, bis: Datum) -> Option<u32> {
    let boerse = depot_kern::liste::boerse_von(isin)?;
    let mut tag = bis;
    let mut tage = 0u32;
    if tag < kauf {
        return None;
    }
    while tag > kauf {
        if boerse.handelstag(tag) {
            tage = tage.checked_add(1)?;
        }
        tag = tag.plus_tage(-1)?;
    }
    Some(tage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haltedauer_zaehlt_boersentage() -> Result<(), String> {
        let sap = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let freitag = Datum::neu(2026, 11, 6).ok_or("Freitag")?;
        let dienstag = Datum::neu(2026, 11, 10).ok_or("Dienstag")?;
        assert_eq!(haltedauer(&sap, freitag, dienstag), Some(2));
        assert_eq!(haltedauer(&sap, dienstag, freitag), None);
        Ok(())
    }

    #[test]
    fn kursverlauf_ohne_belege() -> Result<(), String> {
        let sap = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let tag = Datum::neu(2026, 11, 13).ok_or("Datum")?;
        let abruf = Zeitpunkt::frankfurt(tag, 18, 0, 0).ok_or("Abruf")?;
        assert_eq!(
            kursverlauf(&Stand::default(), &sap, tag, abruf),
            Verlauf::ZuWenig
        );
        Ok(())
    }
}
