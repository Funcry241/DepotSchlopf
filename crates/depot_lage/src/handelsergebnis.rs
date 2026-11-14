//! Realisierte Ergebnisse abgeschlossener Verkaufsorders, aus der gebuchten Ausführungsfolge.

use std::collections::BTreeMap;

use depot_journal::Stand;
use depot_kern::{Cent, Datum, Isin, TopfId, Zeitpunkt};
use depot_regelwerk::Richtung;

#[derive(Debug, PartialEq, Eq)]
pub struct Verkauf {
    pub nr: u64,
    pub topf: TopfId,
    pub isin: Isin,
    pub tag: Option<Datum>,
    pub netto: Option<Cent>,
}

/// Ein Verkaufsauftrag zählt einmal, auch bei mehreren Füllungen. Ein stornierter oder
/// verfallener Rest ändert das Ergebnis des bereits ausgeführten Teils nicht.
pub fn verkaeufe(stand: &Stand) -> Vec<Verkauf> {
    let mut einstaende = BTreeMap::new();
    let mut netto = BTreeMap::<u64, Option<Cent>>::new();
    for (auftrag, ausfuehrung) in stand.ausfuehrungsfolge() {
        let einstand = einstaende
            .entry((auftrag.topf, &auftrag.isin))
            .or_insert_with(|| Some(crate::depot::Einstand::default()));
        let ergebnis = einstand
            .as_mut()
            .and_then(|e| e.buche(auftrag.richtung, ausfuehrung));
        if ergebnis.is_none() {
            *einstand = None;
        }
        if auftrag.richtung == Richtung::Verkauf {
            let summe = netto.entry(auftrag.nr).or_insert(Some(Cent::NULL));
            *summe = summe.zip(ergebnis).and_then(|(s, e)| s.checked_add(e));
        }
    }
    stand
        .auftraege()
        .iter()
        .filter(|a| !a.zustand.offen())
        .filter_map(|a| {
            let netto = netto.remove(&a.beleg.nr)?;
            let tag = a
                .ausfuehrungen
                .iter()
                .map(|f| f.kurszeit.and_then(Zeitpunkt::frankfurt_tag))
                .try_fold(None, |letzter, tag| Some(letzter.max(Some(tag?))))
                .flatten();
            Some(Verkauf {
                nr: a.beleg.nr,
                topf: a.beleg.topf,
                isin: a.beleg.isin.clone(),
                tag,
                netto,
            })
        })
        .collect()
}
