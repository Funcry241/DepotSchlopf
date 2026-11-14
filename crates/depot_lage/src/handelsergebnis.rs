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

#[derive(Clone, Copy, Debug, Default)]
pub struct Bilanz {
    pub abgeschlossen: usize,
    pub gewinne: usize,
    pub verluste: usize,
    pub netto: Cent,
    pub gewinn_summe: Cent,
    pub verlust_summe: Cent,
}

/// Gemeinsame Auswertung der datierten Verkaufsorders mit belegtem Einstand.
pub fn bilanz<'a>(verkaeufe: impl Iterator<Item = &'a Verkauf>) -> Option<Bilanz> {
    let mut bilanz = Bilanz::default();
    for netto in verkaeufe.filter_map(|v| v.netto) {
        bilanz.aufnehmen(netto)?;
    }
    (bilanz.abgeschlossen != 0).then_some(bilanz)
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

    pub fn trefferquote(self) -> Option<i64> {
        let gewinne = i64::try_from(self.gewinne).ok()?;
        gewinne
            .checked_mul(10_000)?
            .checked_div(i64::try_from(self.abgeschlossen).ok()?)
    }
}
