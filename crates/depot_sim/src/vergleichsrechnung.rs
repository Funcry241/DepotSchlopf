//! Rechenregeln des festen Vergleichskorbs.

use depot_kern::{Cent, Isin, Preis, Rundung};
use std::collections::BTreeMap;

/// Der Einstieg des Vergleichsdepots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Einstieg {
    pub stueck: BTreeMap<Isin, u64>,
    pub bargeld: Cent,
    /// Werte ohne Kurs am Einstiegstag, weggelassen.
    pub ohne: Vec<Isin>,
}

/// Teilt `start` gleich auf alle Werte mit Kurs; je Wert geht die Gebuehr ab. Ein Wert, von
/// dem fuer seinen Anteil kein ganzes Stueck zu haben ist, bleibt Bargeld, ohne Gebuehr.
/// `None` bei Ueberlauf oder wenn kein Wert einen Kurs hat.
pub fn kaufe_gleich(
    start: Cent,
    kurse: &[(Isin, Option<Preis>)],
    gebuehr: impl Fn(Cent) -> Option<Cent>,
) -> Option<Einstieg> {
    let mit: Vec<(&Isin, Preis)> = kurse
        .iter()
        .filter_map(|(i, p)| p.map(|p| (i, p)))
        .collect();
    let ohne = kurse
        .iter()
        .filter(|(_, p)| p.is_none())
        .map(|(i, _)| i.clone())
        .collect();
    let n = i64::try_from(mit.len()).ok().filter(|n| *n > 0)?;
    let je_wert = Cent(start.0.checked_div_euclid(n)?);
    let mut bargeld = start;
    let mut stueck = BTreeMap::new();
    for (isin, preis) in mit {
        let g = gebuehr(je_wert)?;
        let Some(verfuegbar) = je_wert.checked_sub(g).filter(|v| v.0 > 0) else {
            continue;
        };
        let s = preis.stueck_fuer(Preis::aus_cent(verfuegbar)?)?;
        if s == 0 {
            continue;
        }
        let kosten = preis.mal_stueck(s, Rundung::Auf)?.checked_add(g)?;
        bargeld = bargeld.checked_sub(kosten)?;
        stueck.insert(isin.clone(), s);
    }
    Some(Einstieg {
        stueck,
        bargeld,
        ohne,
    })
}

/// `(wert - start) * 10.000 / start` in Basispunkten, Richtung minus unendlich gerundet.
pub fn rendite_bp(start: Cent, wert: Cent) -> Option<i64> {
    if start.0 <= 0 {
        return None;
    }
    let diff = i128::from(wert.0).checked_sub(i128::from(start.0))?;
    i64::try_from(
        diff.checked_mul(10_000)?
            .checked_div_euclid(i128::from(start.0))?,
    )
    .ok()
}
