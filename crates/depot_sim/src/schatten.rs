//! Reine Schattenrechnung mit belegten Kursen und modellierten Ausführungskosten.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use depot_journal::Stand;
use depot_kern::{
    Cent, Datum, Grenzen, Herkunft, Isin, Kurs, Preis, Rundung, Universum, Zeitpunkt,
};

use crate::{Tag, papierlauf::START_JE_TOPF};

pub const BELEG: &str = "Schatten";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Strategie {
    Staerke,
    Zahlen,
    Korb,
}

impl Strategie {
    fn name(self) -> &'static str {
        match self {
            Self::Staerke => "Stärke",
            Self::Zahlen => "Zahlen",
            Self::Korb => "Korb",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Bestand {
    stueck: u64,
    kauf: Datum,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Rechnung {
    bar: Cent,
    hoch: Cent,
    wert: Option<Cent>,
    rueckgang_bp: i64,
    trades: usize,
    positionen: BTreeMap<Isin, Bestand>,
    geplant: BTreeMap<Isin, Zeitpunkt>,
}

impl Default for Rechnung {
    fn default() -> Self {
        Self {
            bar: START_JE_TOPF,
            hoch: START_JE_TOPF,
            wert: None,
            rueckgang_bp: 0,
            trades: 0,
            positionen: BTreeMap::new(),
            geplant: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Schattenstand {
    rechnungen: [Rechnung; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct Variante {
    pub haltedauer: u8,
    pub zahlenschwelle_bp: u64,
    pub kurzrang: bool,
}

impl Default for Variante {
    fn default() -> Self {
        Self {
            haltedauer: 10,
            zahlenschwelle_bp: 500,
            kurzrang: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ergebnis {
    pub name: &'static str,
    pub netto: Option<Cent>,
    pub trades: usize,
    pub rueckgang_bp: Option<i64>,
}

impl Ergebnis {
    fn neu(strategie: Strategie, rechnung: &Rechnung) -> Self {
        Self {
            name: strategie.name(),
            netto: rechnung.wert.and_then(|w| w.checked_sub(START_JE_TOPF)),
            trades: rechnung.trades,
            rueckgang_bp: rechnung.wert.map(|_| rechnung.rueckgang_bp),
        }
    }
}

impl Schattenstand {
    pub fn ergebnisse(&self) -> [Ergebnis; 3] {
        let [staerke, zahlen, korb] = &self.rechnungen;
        [
            Ergebnis::neu(Strategie::Staerke, staerke),
            Ergebnis::neu(Strategie::Zahlen, zahlen),
            Ergebnis::neu(Strategie::Korb, korb),
        ]
    }

    pub fn text(&self) -> String {
        self.rechnungen
            .iter()
            .map(Rechnung::text)
            .collect::<Vec<_>>()
            .join("~")
    }

    pub fn lies(text: &str) -> Option<Self> {
        let mut teile = text.split('~');
        let rechnungen = [
            Rechnung::lies(teile.next()?)?,
            Rechnung::lies(teile.next()?)?,
            Rechnung::lies(teile.next()?)?,
        ];
        teile.next().is_none().then_some(Self { rechnungen })
    }

    pub fn letzter(stand: &Stand, vor: Option<Datum>) -> Option<Self> {
        stand
            .begruendungen()
            .iter()
            .rev()
            .find_map(|(tag, wer, text)| {
                (wer == BELEG && vor.is_none_or(|vor| *tag < vor))
                    .then(|| Self::lies(text))
                    .flatten()
            })
    }

    pub fn fortschreiben(
        &mut self,
        stand: &Stand,
        grenzen: &Grenzen,
        universum: &Universum,
        tag: &Tag,
        wissen: Zeitpunkt,
        variante: Variante,
    ) {
        let lauf = Lauf {
            stand,
            grenzen,
            tag,
            wissen,
            variante,
        };
        let [staerke, zahlen, korb] = &mut self.rechnungen;
        let rang = if variante.kurzrang {
            crate::relative_staerke::kurzrang(stand, &tag.kurse, universum, tag.datum)
        } else {
            crate::relative_staerke::fuer_lage(stand, &tag.kurse, universum, tag.datum)
        };
        staerke.schritt(
            &lauf,
            3,
            3_333,
            rang.into_iter().take(3).map(|(isin, _)| isin),
        );
        let reaktionen = crate::zahlenreaktion::belege(stand, &tag.kurse, wissen);
        let signale = reaktionen.into_iter().filter_map(|(isin, b)| {
            (b.positiv && b.abstand_bp >= i64::try_from(variante.zahlenschwelle_bp).ok()?)
                .then_some(isin)
        });
        zahlen.schritt(&lauf, grenzen.max_werte().min(4), 2_500, signale);
        korb.korb(&lauf);
    }
}

fn buchung(
    grenzen: &Grenzen,
    isin: &Isin,
    preis: Preis,
    stueck: u64,
    kauf: bool,
) -> Option<(Cent, Cent)> {
    let boerse = depot_kern::liste::boerse_von(isin)?;
    let rundung = if kauf { Rundung::Auf } else { Rundung::Ab };
    let volumen = grenzen
        .papierkurs(boerse, preis, rundung)?
        .mal_stueck(stueck)?;
    let gebuehr = grenzen.gebuehren().fuer(volumen)?;
    let netto = if kauf {
        volumen.checked_add(gebuehr)?
    } else {
        volumen.checked_sub(gebuehr)?
    };
    Some((volumen, netto))
}

struct Lauf<'a> {
    stand: &'a Stand,
    grenzen: &'a Grenzen,
    tag: &'a Tag,
    wissen: Zeitpunkt,
    variante: Variante,
}

impl Lauf<'_> {
    fn kurs(&self, isin: &Isin) -> Option<&Kurs> {
        let kurs = self.tag.kurse.eintrag(isin)?;
        let boerse = depot_kern::liste::boerse_von(isin)?;
        let abruf = self.tag.kurse.abruf(isin)?;
        (kurs.herkunft() == Herkunft::Quelle
            && abruf <= self.wissen
            && boerse.schluss(kurs.handelstag())? == kurs.zeitpunkt())
        .then_some(kurs)
    }

    fn verkaufswert(&self, isin: &Isin, p: &Bestand) -> Option<Cent> {
        let kurs = self.kurs(isin)?;
        if p.kauf > kurs.handelstag()
            || self.stand.kapitalmassnahmen().iter().any(|b| {
                &b.massnahme.isin == isin
                    && p.kauf < b.massnahme.wirksam
                    && b.massnahme.wirksam <= kurs.handelstag()
            })
        {
            return None;
        }
        buchung(self.grenzen, isin, kurs.preis(), p.stueck, false).map(|(_, bar)| bar)
    }
}

impl Rechnung {
    fn bewerten(&mut self, lauf: &Lauf<'_>) {
        self.wert = self.positionen.iter().try_fold(self.bar, |v, (isin, p)| {
            v.checked_add(lauf.verkaufswert(isin, p)?)
        });
        if let Some(wert) = self.wert {
            self.hoch = self.hoch.max(wert);
            if let Some(weg) = self
                .hoch
                .checked_sub(wert)
                .and_then(|d| d.in_bp_von(self.hoch))
            {
                self.rueckgang_bp = self.rueckgang_bp.max(weg);
            }
        }
    }

    fn kaufen(&mut self, lauf: &Lauf<'_>, isin: &Isin, kurs: &Kurs, budget: Cent) -> Option<()> {
        let budget = budget.min(self.bar);
        let mut stueck = kurs.preis().stueck_fuer(Preis::aus_cent(budget)?)?;
        while stueck != 0 {
            let (volumen, kosten) = buchung(lauf.grenzen, isin, kurs.preis(), stueck, true)?;
            if volumen < lauf.grenzen.min_order() {
                return None;
            }
            if kosten <= budget {
                self.bar = self.bar.checked_sub(kosten)?;
                self.positionen.insert(
                    isin.clone(),
                    Bestand {
                        stueck,
                        kauf: kurs.handelstag(),
                    },
                );
                return Some(());
            }
            stueck = stueck.checked_sub(1)?;
        }
        None
    }

    fn schritt(
        &mut self,
        lauf: &Lauf<'_>,
        max: usize,
        anteil: i64,
        signale: impl Iterator<Item = Isin>,
    ) {
        self.schliesse_faellige(lauf);
        self.bewerten(lauf);
        let geplant = std::mem::take(&mut self.geplant);
        for (isin, bekannt) in geplant {
            if self.positionen.len() >= max {
                break;
            }
            if let Some(kurs) = lauf.kurs(&isin).filter(|k| k.zeitpunkt() > bekannt) {
                let anteil = anteil.min(lauf.grenzen.max_anteil_bp());
                if let Some(budget) = self.wert.and_then(|w| w.anteil_bp_ab(anteil)) {
                    self.kaufen(lauf, &isin, kurs, budget);
                    self.bewerten(lauf);
                }
            } else {
                self.geplant.insert(isin, bekannt);
            }
        }
        for isin in signale {
            if self.positionen.len().saturating_add(self.geplant.len()) >= max {
                break;
            }
            if !self.positionen.contains_key(&isin) {
                self.geplant.entry(isin).or_insert(lauf.wissen);
            }
        }
    }

    fn schliesse_faellige(&mut self, lauf: &Lauf<'_>) {
        self.positionen.retain(|isin, p| {
            let erloes = lauf
                .kurs(isin)
                .and_then(|kurs| crate::kursverlauf::haltedauer(isin, p.kauf, kurs.handelstag()))
                .filter(|tage| *tage >= u32::from(lauf.variante.haltedauer))
                .and_then(|_| lauf.verkaufswert(isin, p))
                .and_then(|erloes| self.bar.checked_add(erloes));
            if let Some(bar) = erloes {
                self.bar = bar;
                self.trades = self.trades.saturating_add(1);
                false
            } else {
                true
            }
        });
    }

    fn korb(&mut self, lauf: &Lauf<'_>) {
        let werte = depot_kern::liste::VERGLEICH_ISIN;
        if self.positionen.is_empty() {
            let Some(kurse) = werte
                .iter()
                .map(|kennung| {
                    let isin = Isin::neu(kennung).ok()?;
                    let kurs = lauf.kurs(&isin)?;
                    Some((isin, kurs))
                })
                .collect::<Option<Vec<_>>>()
            else {
                return;
            };
            let datum = kurse.first().map(|(_, k)| k.handelstag());
            if !kurse.iter().all(|(_, k)| Some(k.handelstag()) == datum) {
                return;
            }
            let budget = Cent(
                START_JE_TOPF
                    .0
                    .checked_div(i64::try_from(werte.len()).unwrap_or(1))
                    .unwrap_or(0),
            );
            for (isin, kurs) in kurse {
                self.kaufen(lauf, &isin, kurs, budget);
            }
        }
        if self.positionen.is_empty() {
            self.wert = None;
        } else {
            self.bewerten(lauf);
        }
    }

    fn text(&self) -> String {
        let wert = self
            .wert
            .map_or_else(|| "-".to_owned(), |c| c.0.to_string());
        let mut aus = format!(
            "{}|{}|{wert}|{}|{}|",
            self.bar.0, self.hoch.0, self.rueckgang_bp, self.trades
        );
        for (isin, p) in &self.positionen {
            let _ = write!(aus, "{isin}/{}/{},", p.stueck, p.kauf);
        }
        aus.push('|');
        for (isin, zeit) in &self.geplant {
            let _ = write!(aus, "{isin}/{zeit},");
        }
        aus
    }

    fn lies(text: &str) -> Option<Self> {
        let mut f = text.split('|');
        let bar = Cent(f.next()?.parse().ok()?);
        let hoch = Cent(f.next()?.parse().ok()?);
        let wert = match f.next()? {
            "-" => None,
            c => Some(Cent(c.parse().ok()?)),
        };
        let rueckgang_bp = f.next()?.parse().ok()?;
        let trades = f.next()?.parse().ok()?;
        let positionen = f
            .next()?
            .split_terminator(',')
            .map(|feld| {
                let (isin, rest) = feld.split_once('/')?;
                let (stueck, kauf) = rest.split_once('/')?;
                let stueck = stueck.parse::<u64>().ok().filter(|s| *s > 0)?;
                Some((
                    Isin::neu(isin).ok()?,
                    Bestand {
                        stueck,
                        kauf: Datum::lies(kauf)?,
                    },
                ))
            })
            .collect::<Option<_>>()?;
        let geplant = f
            .next()?
            .split_terminator(',')
            .map(|feld| {
                let (isin, zeit) = feld.split_once('/')?;
                Some((Isin::neu(isin).ok()?, Zeitpunkt::lies(zeit)?))
            })
            .collect::<Option<_>>()?;
        f.next().is_none().then_some(Self {
            bar,
            hoch,
            wert,
            rueckgang_bp,
            trades,
            positionen,
            geplant,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schatten_folgt_belegtem_signal_kauft_spaeter_und_verkauft_nach_zehn_tagen()
    -> Result<(), String> {
        let stand = Stand::default();
        let grenzen = Grenzen::standard();
        let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let mut rechnung = Rechnung::default();
        let mut billig = Rechnung::default();
        for (monat, t, signal, position, trades) in [
            (11, 2, true, false, 0),
            (11, 3, false, true, 0),
            (11, 16, false, true, 0),
            (11, 17, false, false, 1),
        ] {
            let datum = Datum::neu(2026, monat, t).ok_or("Datum")?;
            let zeit = depot_kern::Boerse::Xetra.schluss(datum).ok_or("Schluss")?;
            let wissen = Zeitpunkt::frankfurt(datum, 19, 0, 0).ok_or("Wissen")?;
            let kurs = Kurs::neu(Cent(10_000), "EUR", datum, zeit, Herkunft::Quelle)
                .map_err(|e| format!("{e:?}"))?;
            let kurse = depot_kern::Kurse::neu(wissen, vec![(isin.clone(), kurs)])
                .map_err(|e| format!("{e:?}"))?;
            let tag = Tag {
                datum,
                kurse,
                ereignisse: Vec::new(),
            };
            let lauf = Lauf {
                stand: &stand,
                grenzen: &grenzen,
                tag: &tag,
                wissen,
                variante: Variante::default(),
            };
            rechnung.schritt(&lauf, 4, 2_500, signal.then(|| isin.clone()).into_iter());
            billig.schritt(&lauf, 4, 1, signal.then(|| isin.clone()).into_iter());
            assert_eq!(rechnung.positionen.contains_key(&isin), position);
            assert_eq!(rechnung.trades, trades);
            assert!(billig.positionen.is_empty(), "Mindestorder");
            assert_eq!(Rechnung::lies(&rechnung.text()), Some(rechnung.clone()));
        }
        assert!(stand.auftraege().is_empty());
        assert!(rechnung.bar < START_JE_TOPF, "Gebühren und Spread");
        Ok(())
    }
}
