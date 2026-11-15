//! Gebührenbereinigter Vergleich für belegte Haltedauern.

use std::collections::BTreeMap;

use depot_journal::Stand;
use depot_journal::kapitalmassnahme::Splitfaktor;
use depot_kern::liste::VERGLEICH_ISIN;
use depot_kern::{Datum, Grenzen, Herkunft, Isin, Preis, Rundung};
use depot_sim::papierlauf::START_JE_TOPF;

use crate::vergleich::{kaufe_gleich, rendite_bp};

#[derive(Debug)]
pub struct Handelsvergleich {
    kurse: BTreeMap<(Datum, Isin), Preis>,
    werte: Vec<Isin>,
    splits: BTreeMap<Isin, BTreeMap<Datum, Option<Splitfaktor>>>,
    splitverdachte: BTreeMap<Isin, Option<Datum>>,
}

impl Handelsvergleich {
    pub fn neu(stand: &Stand) -> Self {
        let werte: Vec<_> = VERGLEICH_ISIN
            .iter()
            .filter_map(|i| Isin::neu(i).ok())
            .collect();
        let kurse = stand
            .kurse()
            .iter()
            .filter(|b| b.kurs().herkunft() == Herkunft::Quelle && werte.contains(b.isin()))
            .map(|b| ((b.kurs().handelstag(), b.isin().clone()), b.kurs().preis()))
            .collect();
        let mut splits = BTreeMap::<_, BTreeMap<_, _>>::new();
        for beleg in stand.kapitalmassnahmen() {
            let m = &beleg.massnahme;
            let faktor = splits
                .entry(m.isin.clone())
                .or_default()
                .entry(m.wirksam)
                .or_insert(Some(m.faktor));
            if *faktor != Some(m.faktor) {
                *faktor = None;
            }
        }
        let splitverdachte = depot_journal::splitverdacht::aktiv(stand, None)
            .into_iter()
            .map(|(isin, beleg)| (isin, beleg.zeit().frankfurt_tag()))
            .collect();
        Self {
            kurse,
            werte,
            splits,
            splitverdachte,
        }
    }

    fn stueck_am_ausstieg(
        &self,
        isin: &Isin,
        stueck: u64,
        einstieg: Datum,
        ausstieg: Datum,
    ) -> Option<u64> {
        if self
            .splitverdachte
            .get(isin)
            .is_some_and(|tag| tag.is_none_or(|tag| tag <= ausstieg))
        {
            return None;
        }
        self.splits.get(isin).map_or(Some(stueck), |splits| {
            splits
                .range((
                    std::ops::Bound::Excluded(einstieg),
                    std::ops::Bound::Included(ausstieg),
                ))
                .try_fold(stueck, |bestand, (_, faktor)| (*faktor)?.stueck(bestand))
        })
    }

    pub fn rendite(&self, einstieg: Datum, ausstieg: Datum) -> Option<i64> {
        if einstieg > ausstieg || self.werte.len() != VERGLEICH_ISIN.len() {
            return None;
        }
        let grenzen = Grenzen::standard();
        let gebuehren = grenzen.gebuehren();
        let kaufkurse: Vec<_> = self
            .werte
            .iter()
            .map(|isin| {
                Some((
                    isin.clone(),
                    Some(*self.kurse.get(&(einstieg, isin.clone()))?),
                ))
            })
            .collect::<Option<_>>()?;
        let korb = kaufe_gleich(START_JE_TOPF, &kaufkurse, |v| gebuehren.fuer(v))?;
        if korb.stueck.is_empty() {
            return None;
        }
        let erloes = korb
            .stueck
            .iter()
            .try_fold(korb.bargeld, |summe, (isin, stueck)| {
                let kurs = self.kurse.get(&(ausstieg, isin.clone()))?;
                let bestand = self.stueck_am_ausstieg(isin, *stueck, einstieg, ausstieg)?;
                let volumen = kurs.mal_stueck(bestand, Rundung::Ab)?;
                summe.checked_add(volumen.checked_sub(gebuehren.fuer(volumen)?)?)
            })?;
        rendite_bp(START_JE_TOPF, erloes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_kern::Cent;

    #[test]
    fn unveraenderte_kurse_mit_kauf_und_verkaufsgebuehren() -> Result<(), &'static str> {
        let von = Datum::neu(2026, 11, 12).ok_or("Einstieg")?;
        let bis = von.plus_tage(1).ok_or("Ausstieg")?;
        let preis = Preis::aus_cent(Cent(10_000)).ok_or("Kurs")?;
        let werte: Vec<_> = VERGLEICH_ISIN
            .iter()
            .filter_map(|i| Isin::neu(i).ok())
            .collect();
        let kurse = werte
            .iter()
            .flat_map(|isin| [(von, isin.clone()), (bis, isin.clone())])
            .map(|schluessel| (schluessel, preis))
            .collect();
        let mut vergleich = Handelsvergleich {
            kurse,
            werte,
            splits: BTreeMap::new(),
            splitverdachte: BTreeMap::new(),
        };
        let gebuehr = Cent(300)
            .mal_stueck(u64::try_from(VERGLEICH_ISIN.len()).map_err(|_| "Anzahl")?)
            .ok_or("Gebühr")?;
        let erloes = START_JE_TOPF.checked_sub(gebuehr).ok_or("Erlös")?;
        assert_eq!(
            vergleich.rendite(von, bis),
            rendite_bp(START_JE_TOPF, erloes)
        );
        let isin = vergleich.werte.first().ok_or("Vergleichswert")?.clone();
        let halbiert = Preis::aus_cent(Cent(5_000)).ok_or("Kurs nach Split")?;
        vergleich.kurse.insert((bis, isin.clone()), halbiert);
        let faktor = Splitfaktor::neu(2, 1).ok_or("Splitfaktor")?;
        vergleich
            .splits
            .insert(isin.clone(), BTreeMap::from([(bis, Some(faktor))]));
        assert_eq!(
            vergleich.rendite(von, bis),
            rendite_bp(START_JE_TOPF, erloes)
        );
        vergleich.splitverdachte.insert(isin, Some(bis));
        assert_eq!(vergleich.rendite(von, bis), None);
        vergleich.splitverdachte.clear();
        vergleich.kurse.retain(|(tag, _), _| *tag != bis);
        assert_eq!(vergleich.rendite(von, bis), None);
        Ok(())
    }
}
