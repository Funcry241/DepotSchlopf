//! Messbare Kriterien für die Papierhandelsreife.

use depot_journal::Stand;
use depot_kern::liste::Wert;
use depot_kern::{Cent, Datum, Grenzen, Zeitpunkt};

use crate::handelsergebnis::verkaeufe;
use crate::handelsvergleich::Handelsvergleich;
use crate::vergleich::{rendite_bp, vergleich_bis};

#[derive(Clone, Copy, Debug, Default)]
pub struct Reife {
    pub handelstage: usize,
    pub trades: usize,
    pub netto_bp: Option<(i64, i64)>,
    pub rueckgang_bp: Option<(i64, i64)>,
}

impl Reife {
    pub fn genug_tage(self) -> bool {
        self.handelstage >= 40
    }
    pub fn genug_trades(self) -> bool {
        self.trades >= 30
    }
    pub fn netto_erfuellt(self) -> Option<bool> {
        self.netto_bp.map(|(depot, korb)| depot > korb)
    }
    pub fn rueckgang_erfuellt(self) -> Option<bool> {
        self.rueckgang_bp.map(|(depot, korb)| depot <= korb)
    }
}

fn modellierte_spreadkosten(stand: &Stand, tag: Datum) -> Option<Cent> {
    stand
        .ausfuehrungsfolge()
        .try_fold(Cent::NULL, |summe, (auftrag, f)| {
            if f.kurszeit.and_then(Zeitpunkt::frankfurt_tag)? > tag {
                return Some(summe);
            }
            let boerse = depot_kern::liste::boerse_von(&auftrag.isin)?;
            let volumen = f.kurs.mal_stueck(f.stueck)?;
            summe.checked_add(
                volumen.anteil_bp_auf(depot_sim::kostenhuerde::spread_je_seite_bp(boerse))?,
            )
        })
}

fn papier_rendite(stand: &Stand, werte: &[Wert], tag: Datum) -> Option<i64> {
    let vergleich = vergleich_bis(stand, werte, Some(tag))?;
    let [dachs, maus, ..] = vergleich.zeilen.as_slice() else {
        return None;
    };
    let start = stand
        .startgeld_je_topf(depot_kern::TopfId::A)?
        .checked_add(stand.startgeld_je_topf(depot_kern::TopfId::B)?)?;
    let netto = dachs
        .wert?
        .checked_add(maus.wert?)?
        .checked_sub(dachs.ki_kosten?)?
        .checked_sub(maus.ki_kosten?)?
        .checked_sub(modellierte_spreadkosten(stand, tag)?)?;
    rendite_bp(start, netto)
}

fn rueckgang(mut reihe: impl Iterator<Item = i64>) -> Option<i64> {
    let (_, groesster) = reihe.try_fold((10_000_i64, 0_i64), |(hoch, groesster), bp| {
        let wert = 10_000_i64.checked_add(bp)?;
        let hoch = hoch.max(wert);
        let weg = Cent(hoch.checked_sub(wert)?).in_bp_von(Cent(hoch))?;
        Some((hoch, groesster.max(weg)))
    })?;
    Some(groesster)
}

pub fn ermittle(stand: &Stand, werte: &[Wert], grenzen: &Grenzen) -> Reife {
    let tage: Vec<_> = stand
        .tagesschluesse()
        .iter()
        .map(|(tag, _)| *tag)
        .filter(|tag| depot_kern::liste::handelstag(*tag))
        .collect();
    let mut reife = Reife {
        handelstage: tage.len(),
        trades: verkaeufe(stand)
            .iter()
            .filter(|v| v.netto.is_some())
            .count(),
        ..Reife::default()
    };
    let Some(&erster) = tage.first() else {
        return reife;
    };
    let korb = Handelsvergleich::neu(stand);
    let werte_je_tag =
        |tag| papier_rendite(stand, werte, tag).zip(korb.rendite_mit_spread(erster, tag, grenzen));
    reife.netto_bp = tage.last().and_then(|t| werte_je_tag(*t));
    if let Some(paare) = tage
        .into_iter()
        .map(werte_je_tag)
        .collect::<Option<Vec<_>>>()
    {
        reife.rueckgang_bp =
            rueckgang(paare.iter().map(|(p, _)| *p)).zip(rueckgang(paare.iter().map(|(_, k)| *k)));
    }
    reife
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reifegrenzen_und_fehlende_belege() {
        let reife = Reife {
            handelstage: 39,
            trades: 29,
            netto_bp: Some((50, 50)),
            rueckgang_bp: None,
        };
        assert!(!reife.genug_tage() && !reife.genug_trades());
        assert_eq!(reife.netto_erfuellt(), Some(false));
        assert_eq!(reife.rueckgang_erfuellt(), None);
        let reife = Reife {
            handelstage: 40,
            trades: 30,
            netto_bp: Some((51, 50)),
            rueckgang_bp: Some((100, 100)),
        };
        assert!(reife.genug_tage() && reife.genug_trades());
        assert_eq!(reife.netto_erfuellt(), Some(true));
        assert_eq!(reife.rueckgang_erfuellt(), Some(true));
        assert_eq!(rueckgang([100, -1, 100, -1_000].into_iter()), Some(1_089));
    }
}
