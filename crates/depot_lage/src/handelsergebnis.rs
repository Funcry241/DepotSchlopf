//! Realisierte Ergebnisse abgeschlossener Verkaufsorders, aus der gebuchten Ausführungsfolge.

use std::collections::BTreeMap;

use depot_journal::{Stand, kapitalmassnahme::Bewertungsschritt};
use depot_kern::{Cent, Datum, Grenzen, Herkunft, Isin, Rundung, TopfId, Zeitpunkt};
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
    pub staerkerang: Option<usize>,
}

#[derive(Clone, Copy, Default)]
struct Kaufbezug {
    tag: Option<Datum>,
    anlass: Option<&'static str>,
    staerkerang: Option<usize>,
}

impl Kaufbezug {
    fn gemeinsam(self, anderer: Self) -> Self {
        Self {
            tag: self.tag.filter(|tag| Some(*tag) == anderer.tag),
            anlass: self.anlass.filter(|anlass| Some(*anlass) == anderer.anlass),
            staerkerang: self
                .staerkerang
                .filter(|rang| Some(*rang) == anderer.staerkerang),
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

fn staerkerang(stand: &Stand) -> BTreeMap<u64, usize> {
    stand
        .begruendungen()
        .iter()
        .filter_map(|(_, wer, text)| {
            if wer != depot_sim::relative_staerke::RANG_BELEG {
                return None;
            }
            let (nr, rang) = text.split_once(' ')?;
            let rang = rang.parse::<usize>().ok().filter(|n| *n > 0)?;
            Some((nr.parse::<u64>().ok()?, rang))
        })
        .collect()
}

type Positionen<'a> = BTreeMap<(TopfId, &'a Isin), Option<Position>>;

fn anlaesse(stand: &Stand) -> BTreeMap<u64, Option<&'static str>> {
    stand
        .begruendungen()
        .iter()
        .filter_map(|(_, wer, text)| {
            if wer != depot_sim::KAUFTHESE {
                return None;
            }
            let (nr, these) = text.split_once(' ')?;
            Some((nr.parse::<u64>().ok()?, depot_sim::kaufanlass(these)))
        })
        .collect()
}

fn ausfuehrungen(stand: &Stand) -> (Positionen<'_>, BTreeMap<u64, Erloes>) {
    let anlaesse = anlaesse(stand);
    let staerkerang = staerkerang(stand);
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
                    staerkerang: staerkerang.get(&auftrag.nr).copied(),
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
    (positionen, erloese)
}

/// Ein Verkaufsauftrag zählt einmal, auch bei mehreren Füllungen. Ein stornierter oder
/// verfallener Rest ändert das Ergebnis des bereits ausgeführten Teils nicht.
pub fn verkaeufe(stand: &Stand) -> Vec<Verkauf> {
    verkaeufe_aus(stand, ausfuehrungen(stand).1)
}

fn verkaeufe_aus(stand: &Stand, mut erloese: BTreeMap<u64, Erloes>) -> Vec<Verkauf> {
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
                staerkerang: erloes.kauf.staerkerang,
            })
        })
        .collect()
}

#[derive(Debug)]
pub struct OffenePosition {
    pub topf: TopfId,
    pub isin: Isin,
    pub einstand: Option<Cent>,
    pub wert_netto: Option<Cent>,
    pub einstieg: Option<Datum>,
    pub bewertungstag: Option<Datum>,
}

impl OffenePosition {
    pub fn netto(&self) -> Option<Cent> {
        self.wert_netto?.checked_sub(self.einstand?)
    }
}

pub fn handelspositionen(stand: &Stand, grenzen: &Grenzen) -> (Vec<Verkauf>, Vec<OffenePosition>) {
    let (positionen, erloese) = ausfuehrungen(stand);
    let gebuehren = grenzen.gebuehren();
    let schlusstag = stand.abschluss().map(|a| a.datum);
    let verdacht = depot_journal::splitverdacht::aktiv(stand, None);
    let mut offen = Vec::new();
    for topf in stand.toepfe().values() {
        for (isin, stueck) in topf.positionen() {
            let position = positionen
                .get(&(topf.id(), isin))
                .and_then(Option::as_ref)
                .filter(|p| p.einstand.stueck == *stueck);
            let einstieg = position.and_then(|p| p.kauf.tag);
            let letzter_split = stand
                .kapitalmassnahmen()
                .iter()
                .filter(|b| b.massnahme.topf == topf.id() && b.massnahme.isin == *isin)
                .map(|b| b.massnahme.wirksam)
                .max();
            let belegt = stand
                .kurse()
                .iter()
                .filter(|b| {
                    let k = b.kurs();
                    b.isin() == isin
                        && !verdacht.contains_key(isin)
                        && k.herkunft() == Herkunft::Quelle
                        && schlusstag.is_some_and(|bis| k.handelstag() <= bis)
                        && einstieg
                            .max(letzter_split)
                            .is_none_or(|von| von <= k.handelstag())
                })
                .max_by_key(|b| (b.kurs().handelstag(), b.abruf()));
            let wert_netto = belegt.and_then(|b| {
                let wert = b.kurs().preis().mal_stueck(*stueck, Rundung::Ab)?;
                wert.checked_sub(gebuehren.fuer(wert)?)
            });
            offen.push(OffenePosition {
                topf: topf.id(),
                isin: isin.clone(),
                einstand: position.map(|p| p.einstand.kosten),
                wert_netto,
                einstieg,
                bewertungstag: belegt.map(|b| b.kurs().handelstag()),
            });
        }
    }
    (verkaeufe_aus(stand, erloese), offen)
}

pub fn nicht_zugeordnete_kaeufe(stand: &Stand, topf: TopfId, ab: Option<Datum>) -> usize {
    let anlaesse = anlaesse(stand);
    stand
        .auftraege()
        .iter()
        .filter(|a| {
            a.beleg.topf == topf
                && a.beleg.richtung == Richtung::Kauf
                && a.ausfuehrungen.iter().any(|f| {
                    ab.is_none_or(|von| {
                        f.kurszeit
                            .and_then(Zeitpunkt::frankfurt_tag)
                            .is_some_and(|tag| tag >= von)
                    })
                })
                && anlaesse.get(&a.beleg.nr).copied().flatten().is_none()
        })
        .count()
}

pub fn mehrertrag(netto: Cent, einstand: Cent, korb_bp: i64) -> Option<Cent> {
    let differenz = i128::from(einstand.0)
        .checked_mul(i128::from(korb_bp))?
        .checked_div(10_000)?;
    netto.checked_sub(Cent(i64::try_from(differenz).ok()?))
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
