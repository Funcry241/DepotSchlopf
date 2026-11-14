//! Abgeschlossene Positionen nach Börsengebühren und belegten KI-Kosten.

use std::collections::BTreeMap;

use depot_journal::Stand;
use depot_kern::{Cent, Datum, Isin, TopfId};
use depot_sim::ausfuehrung::{Richtung, ausfuehrungsbuch};

#[derive(Default)]
struct OffenerTrade {
    stueck: u64,
    saldo: Option<Cent>,
    ab: Option<Datum>,
    mit_vorbestand: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Bilanz {
    pub abgeschlossen: usize,
    pub gewinne: usize,
    pub verluste: usize,
    pub netto: Cent,
    pub gewinn_summe: Cent,
    pub verlust_summe: Cent,
}

fn ki_kosten(stand: &Stand, topf: TopfId, ab: Datum, bis: Datum) -> Option<Cent> {
    stand
        .ki()
        .abgerechnete_buchungen()
        .filter(|b| b.berater == topf.berater() && (ab..=bis).contains(&b.datum))
        .try_fold(Cent::NULL, |summe, b| summe.checked_add(b.reserviert))
}

pub(crate) fn bilanzen(stand: &Stand) -> Option<BTreeMap<TopfId, Bilanz>> {
    let buch: BTreeMap<_, _> = ausfuehrungsbuch(stand)
        .into_iter()
        .map(|a| (a.nr, a))
        .collect();
    let mut offen = BTreeMap::<(TopfId, Isin), OffenerTrade>::new();
    let mut bilanzen = BTreeMap::<TopfId, Bilanz>::new();
    for (auftrag, aus) in stand.ausfuehrungsfolge() {
        let tag = aus.kurszeit.and_then(depot_kern::Zeitpunkt::frankfurt_tag);
        let zeile = buch.get(&auftrag.nr)?;
        let schluessel = (auftrag.topf, auftrag.isin.clone());
        let netto = aus.kurs.mal_stueck(aus.stueck)?;
        match zeile.richtung {
            Richtung::Kauf => {
                let o = offen.entry(schluessel).or_default();
                o.ab = o.ab.or(tag);
                o.mit_vorbestand |= tag.is_none();
                o.stueck = o.stueck.checked_add(aus.stueck)?;
                o.saldo = Some(
                    o.saldo
                        .unwrap_or(Cent::NULL)
                        .checked_sub(netto.checked_add(aus.gebuehr)?)?,
                );
            }
            Richtung::Verkauf => {
                let Some(o) = offen.get_mut(&schluessel) else {
                    continue;
                };
                let Some(stueck) = o.stueck.checked_sub(aus.stueck) else {
                    offen.remove(&schluessel);
                    continue;
                };
                o.stueck = stueck;
                let saldo = o.saldo?.checked_add(netto.checked_sub(aus.gebuehr)?)?;
                if o.stueck == 0 {
                    if !o.mit_vorbestand
                        && let Some((ab, bis)) = o.ab.zip(tag)
                    {
                        let netto = saldo.checked_sub(ki_kosten(stand, auftrag.topf, ab, bis)?)?;
                        bilanzen.entry(auftrag.topf).or_default().aufnehmen(netto)?;
                    }
                    offen.remove(&schluessel);
                } else {
                    o.saldo = Some(saldo);
                }
            }
        }
    }
    Some(bilanzen)
}

impl Bilanz {
    fn aufnehmen(&mut self, netto: Cent) -> Option<()> {
        self.abgeschlossen = self.abgeschlossen.saturating_add(1);
        self.netto = self.netto.checked_add(netto)?;
        if netto > Cent::NULL {
            self.gewinne = self.gewinne.saturating_add(1);
            self.gewinn_summe = self.gewinn_summe.checked_add(netto)?;
        } else if netto < Cent::NULL {
            self.verluste = self.verluste.saturating_add(1);
            self.verlust_summe = self.verlust_summe.checked_add(netto)?;
        }
        Some(())
    }

    pub fn durchschnitt(summe: Cent, anzahl: usize) -> Option<Cent> {
        summe.0.checked_div(i64::try_from(anzahl).ok()?).map(Cent)
    }

    pub fn trefferquote(self) -> Option<usize> {
        self.gewinne
            .checked_mul(100)?
            .checked_div(self.abgeschlossen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netto_einfach() -> Result<(), String> {
        let kauf = Cent(1_000)
            .mal_stueck(10)
            .and_then(|p| p.checked_add(Cent(150)))
            .ok_or("Kauf")?;
        let verkauf = Cent(1_100)
            .mal_stueck(10)
            .and_then(|p| p.checked_sub(Cent(150)))
            .ok_or("Verkauf")?;
        let mut b = Bilanz::default();
        b.aufnehmen(
            verkauf
                .checked_sub(kauf)
                .and_then(|p| p.checked_sub(Cent(40)))
                .ok_or("Netto")?,
        )
        .ok_or("Bilanz")?;
        assert_eq!(b.netto, Cent(660));
        Ok(())
    }

    #[test]
    fn trefferquote() -> Result<(), String> {
        let mut b = Bilanz::default();
        for ergebnis in [200, 400, 600, -300] {
            b.aufnehmen(Cent(ergebnis)).ok_or("Bilanz")?;
        }
        assert_eq!(b.trefferquote(), Some(75));
        assert_eq!(
            Bilanz::durchschnitt(b.gewinn_summe, b.gewinne),
            Some(Cent(400))
        );
        assert_eq!(
            Bilanz::durchschnitt(b.verlust_summe, b.verluste),
            Some(Cent(-300))
        );
        Ok(())
    }
}
