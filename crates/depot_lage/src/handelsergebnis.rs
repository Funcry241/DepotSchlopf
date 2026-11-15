//! Realisierte Ergebnisse abgeschlossener Verkaufsorders, aus der gebuchten Ausführungsfolge.

use std::collections::BTreeMap;

use depot_journal::{Stand, kapitalmassnahme::Bewertungsschritt};
use depot_kern::{Cent, Datum, Isin, TopfId, Zeitpunkt};
use depot_regelwerk::Richtung;

#[derive(Debug, PartialEq, Eq)]
pub struct Verkauf {
    pub nr: u64,
    pub topf: TopfId,
    pub isin: Isin,
    pub tag: Option<Datum>,
    pub netto: Option<Cent>,
    pub einstand: Option<Cent>,
    pub einstieg: Option<Datum>,
    /// Ein Vergleich ist nur bei Ausführungen am selben Ausstiegstag möglich.
    pub ausstieg: Option<Datum>,
    pub anlass: Option<&'static str>,
}

#[derive(Clone, Copy, Default)]
struct Kaufbezug {
    tag: Option<Datum>,
    anlass: Option<&'static str>,
}

impl Kaufbezug {
    fn gemeinsam(self, anderer: Self) -> Self {
        Self {
            tag: self.tag.filter(|tag| Some(*tag) == anderer.tag),
            anlass: self.anlass.filter(|anlass| Some(*anlass) == anderer.anlass),
        }
    }
}

#[derive(Default)]
struct Position {
    einstand: crate::depot::Einstand,
    kauf: Kaufbezug,
}

struct Erloes {
    netto: Option<Cent>,
    einstand: Option<Cent>,
    kauf: Kaufbezug,
}

impl Default for Erloes {
    fn default() -> Self {
        Self {
            netto: Some(Cent::NULL),
            einstand: Some(Cent::NULL),
            kauf: Kaufbezug::default(),
        }
    }
}

/// Ein Verkaufsauftrag zählt einmal, auch bei mehreren Füllungen. Ein stornierter oder
/// verfallener Rest ändert das Ergebnis des bereits ausgeführten Teils nicht.
pub fn verkaeufe(stand: &Stand) -> Vec<Verkauf> {
    let anlaesse = stand
        .begruendungen()
        .iter()
        .filter_map(|(_, wer, text)| {
            if wer != depot_sim::KAUFTHESE {
                return None;
            }
            let (nr, these) = text.split_once(' ')?;
            Some((nr.parse::<u64>().ok()?, depot_sim::kaufanlass(these)))
        })
        .collect::<BTreeMap<_, _>>();
    let mut positionen = BTreeMap::<(TopfId, &Isin), Option<Position>>::new();
    let mut erloese = BTreeMap::<u64, Erloes>::new();
    for schritt in stand.bewertungsfolge() {
        let (auftrag, ausfuehrung) = match schritt {
            Bewertungsschritt::Ausfuehrung(auftrag, ausfuehrung) => (auftrag, ausfuehrung),
            Bewertungsschritt::Split(m) => {
                if let Some(position) = positionen.get_mut(&(m.topf, &m.isin)) {
                    *position = position.take().and_then(|mut p| {
                        p.einstand.stueck = m.faktor.stueck(p.einstand.stueck)?;
                        Some(p)
                    });
                }
                continue;
            }
        };
        let position = positionen
            .entry((auftrag.topf, &auftrag.isin))
            .or_insert_with(|| Some(Position::default()));
        let buchung = position.as_mut().and_then(|p| {
            let vorher = p.einstand.kosten;
            let schon_gehalten = p.einstand.stueck != 0;
            let netto = p.einstand.buche(auftrag.richtung, ausfuehrung)?;
            if auftrag.richtung == Richtung::Kauf {
                let kauf = Kaufbezug {
                    tag: ausfuehrung.kurszeit.and_then(Zeitpunkt::frankfurt_tag),
                    anlass: anlaesse.get(&auftrag.nr).copied().flatten(),
                };
                p.kauf = if schon_gehalten {
                    p.kauf.gemeinsam(kauf)
                } else {
                    kauf
                };
            }
            Some((netto, vorher.checked_sub(p.einstand.kosten)?, p.kauf))
        });
        if buchung.is_none() {
            *position = None;
        }
        if auftrag.richtung == Richtung::Verkauf {
            let erloes = erloese.entry(auftrag.nr).or_insert_with(|| Erloes {
                kauf: buchung.map_or_else(Kaufbezug::default, |(_, _, kauf)| kauf),
                ..Erloes::default()
            });
            erloes.netto = erloes
                .netto
                .zip(buchung)
                .and_then(|(s, (n, _, _))| s.checked_add(n));
            erloes.einstand = erloes
                .einstand
                .zip(buchung)
                .and_then(|(s, (_, e, _))| s.checked_add(e));
            erloes.kauf = erloes
                .kauf
                .gemeinsam(buchung.map_or_else(Kaufbezug::default, |(_, _, k)| k));
        }
    }
    stand
        .auftraege()
        .iter()
        .filter(|a| !a.zustand.offen())
        .filter_map(|a| {
            let erloes = erloese.remove(&a.beleg.nr)?;
            let tage = || {
                a.ausfuehrungen
                    .iter()
                    .map(|f| f.kurszeit.and_then(Zeitpunkt::frankfurt_tag))
            };
            let tag = tage()
                .try_fold(None, |letzter, tag| Some(letzter.max(Some(tag?))))
                .flatten();
            Some(Verkauf {
                nr: a.beleg.nr,
                topf: a.beleg.topf,
                isin: a.beleg.isin.clone(),
                tag,
                netto: erloes.netto,
                einstand: erloes.einstand,
                einstieg: erloes.kauf.tag,
                ausstieg: tag.filter(|tag| tage().all(|t| t == Some(*tag))),
                anlass: erloes.kauf.anlass,
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
