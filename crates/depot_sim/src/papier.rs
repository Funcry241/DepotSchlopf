//! Das Papierkonto: ein erfundenes Konto beim erfundenen Broker. Es kennt keine Toepfe, nur
//! das Gesamtkonto, und fuehrt seinen Stand selbst, unabhaengig vom Journal. So hat der
//! Kontowaechter etwas, womit er abgleichen kann.
//!
//! **Senden:** Die Auftragsstelle sendet nur `GenehmigterAuftrag` (R3). Das Konto erkennt eine
//! Kennung, die es schon kennt, und lehnt sie ab; ein Auftrag ohne Deckung im Gesamtkonto wird
//! abgelehnt.
//!
//! **Markt:** Tagesorders werden zu den Schlusskursen des Tages ausgefuehrt, wenn der Kurs das
//! Limit einhaelt, sonst verfallen sie. Jede Ausfuehrung kostet die Gebuehr nach Tarif.

use std::collections::BTreeMap;

use depot_journal::{Ausfuehrung, Konto, KontoOrder, KontoStatus, Kontoauszug};
use depot_kern::{Cent, Gebuehren, Grenzen, Isin, Kurse, Topf};
use depot_regelwerk::{Auftrag, GenehmigterAuftrag, Richtung};

/// Antwort des Brokers auf einen Versand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sendeantwort {
    Angenommen,
    Abgelehnt(String),
    /// Keine Antwort: Ob der Auftrag angekommen ist, klaert nur der Abgleich.
    KeineAntwort,
}

/// Der Broker, wie die Auftragsstelle ihn sieht: lesen wie der Kontowaechter, dazu senden.
pub trait Broker: Konto {
    fn sende(&mut self, auftrag: &GenehmigterAuftrag) -> Sendeantwort;
    /// Der Markt laeuft bis zum Schluss dieses Handelstags.
    fn markt(&mut self, kurse: &Kurse);
}

#[derive(Clone, Debug)]
struct PapierOrder {
    auftrag: Auftrag,
    status: KontoStatus,
    ausfuehrungen: Vec<Ausfuehrung>,
}

/// Das erfundene Gesamtkonto.
#[derive(Clone, Debug)]
pub struct Papierkonto {
    bargeld: Cent,
    bestand: BTreeMap<Isin, u64>,
    orders: BTreeMap<u64, PapierOrder>,
    gebuehren: Gebuehren,
    sendungen: usize,
}

impl Papierkonto {
    /// Eroeffnet das Konto mit der Summe der Starttoepfe. `None` bei Ueberlauf.
    pub fn eroeffne(start: &[Topf]) -> Option<Papierkonto> {
        let mut bargeld = Cent::NULL;
        let mut bestand: BTreeMap<Isin, u64> = BTreeMap::new();
        for t in start {
            bargeld = bargeld.checked_add(t.bargeld())?;
            for (isin, n) in t.positionen() {
                let s = bestand.entry(isin.clone()).or_insert(0);
                *s = s.checked_add(*n)?;
            }
        }
        Some(Papierkonto {
            bargeld,
            bestand,
            orders: BTreeMap::new(),
            gebuehren: Grenzen::standard().gebuehren(),
            sendungen: 0,
        })
    }

    /// Wie oft gesendet wurde, auch abgelehnt.
    pub fn sendungen(&self) -> usize {
        self.sendungen
    }

    /// Bucht eine fremde Belastung oder Gutschrift, etwa eine Gebuehr des Brokers.
    pub fn buche_fremd(&mut self, betrag: Cent) -> Option<()> {
        self.bargeld = self.bargeld.checked_add(betrag)?;
        Some(())
    }

    fn offen(&self) -> impl Iterator<Item = &PapierOrder> {
        self.orders
            .values()
            .filter(|o| o.status == KontoStatus::Offen)
    }

    fn gedeckt(&self, a: &Auftrag) -> Option<bool> {
        match a.richtung {
            Richtung::Kauf => {
                let mut frei = self.bargeld;
                for o in self
                    .offen()
                    .filter(|o| o.auftrag.richtung == Richtung::Kauf)
                {
                    frei = frei.checked_sub(self.bedarf(&o.auftrag)?)?;
                }
                Some(self.bedarf(a)? <= frei)
            }
            Richtung::Verkauf => {
                let gebunden = self
                    .offen()
                    .filter(|o| o.auftrag.richtung == Richtung::Verkauf && o.auftrag.isin == a.isin)
                    .try_fold(0_u64, |s, o| s.checked_add(o.auftrag.stueck))?;
                let frei = self.bestand.get(&a.isin).copied().unwrap_or(0);
                Some(gebunden.checked_add(a.stueck)? <= frei)
            }
        }
    }

    fn bedarf(&self, a: &Auftrag) -> Option<Cent> {
        let v = a.limit.mal_stueck(a.stueck)?;
        v.checked_add(self.gebuehren.fuer(v)?)
    }

    /// Fuehrt eine offene Order ganz aus, wenn der Kurs das Limit einhaelt; sonst verfaellt sie.
    fn schluss(&mut self, nr: u64, kurse: &Kurse) {
        let Some(o) = self.orders.get(&nr) else {
            return;
        };
        let a = &o.auftrag;
        let ausfuehrung = kurse.kurs_cent(&a.isin).and_then(|kurs| {
            let im_limit = match a.richtung {
                Richtung::Kauf => kurs <= a.limit,
                Richtung::Verkauf => kurs >= a.limit,
            };
            im_limit.then_some(())?;
            let volumen = kurs.mal_stueck(a.stueck)?;
            let gebuehr = self.gebuehren.fuer(volumen)?;
            let gehalten = self.bestand.get(&a.isin).copied().unwrap_or(0);
            let (bargeld, stueck) = match a.richtung {
                Richtung::Kauf => (
                    self.bargeld.checked_sub(volumen)?.checked_sub(gebuehr)?,
                    gehalten.checked_add(a.stueck)?,
                ),
                Richtung::Verkauf => (
                    self.bargeld.checked_add(volumen)?.checked_sub(gebuehr)?,
                    gehalten.checked_sub(a.stueck)?,
                ),
            };
            Some((
                bargeld,
                stueck,
                Ausfuehrung {
                    stueck: a.stueck,
                    kurs,
                    gebuehr,
                },
            ))
        });
        let isin = a.isin.clone();
        let Some(o) = self.orders.get_mut(&nr) else {
            return;
        };
        match ausfuehrung {
            Some((bargeld, stueck, aus)) => {
                self.bargeld = bargeld;
                if stueck == 0 {
                    self.bestand.remove(&isin);
                } else {
                    self.bestand.insert(isin, stueck);
                }
                o.ausfuehrungen.push(aus);
                o.status = KontoStatus::Gefuellt;
            }
            None => o.status = KontoStatus::Verfallen,
        }
    }
}

impl Konto for Papierkonto {
    fn auszug(&self) -> Kontoauszug {
        Kontoauszug {
            bargeld: self.bargeld,
            bestand: self.bestand.clone(),
            offene: self
                .orders
                .iter()
                .filter(|(_, o)| o.status == KontoStatus::Offen)
                .map(|(nr, _)| *nr)
                .collect(),
            fremde_waehrungen: std::collections::BTreeSet::new(),
        }
    }

    fn order(&self, nr: u64) -> Option<KontoOrder> {
        self.orders.get(&nr).map(|o| KontoOrder {
            status: o.status,
            ausfuehrungen: o.ausfuehrungen.clone(),
        })
    }
}

impl Broker for Papierkonto {
    fn sende(&mut self, g: &GenehmigterAuftrag) -> Sendeantwort {
        self.sendungen = self.sendungen.saturating_add(1);
        let a = g.auftrag();
        let nr = a.kennung.nr();
        if self.orders.contains_key(&nr) {
            // Dieselbe Kennung zweimal: Der zweite Versand wird abgelehnt, die erste Order
            // bleibt, wie sie ist (R24, R37).
            return Sendeantwort::Abgelehnt(format!("Kennung {nr} schon vergeben"));
        }
        let status = if self.gedeckt(a) == Some(true) {
            KontoStatus::Offen
        } else {
            KontoStatus::Abgelehnt
        };
        self.orders.insert(
            nr,
            PapierOrder {
                auftrag: a.clone(),
                status,
                ausfuehrungen: Vec::new(),
            },
        );
        match status {
            KontoStatus::Offen => Sendeantwort::Angenommen,
            _ => Sendeantwort::Abgelehnt("keine Deckung im Konto".into()),
        }
    }

    fn markt(&mut self, kurse: &Kurse) {
        let offen: Vec<u64> = self
            .orders
            .iter()
            .filter(|(_, o)| o.status == KontoStatus::Offen)
            .map(|(nr, _)| *nr)
            .collect();
        for nr in offen {
            self.schluss(nr, kurse);
        }
    }
}
