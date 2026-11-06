//! Das Papierkonto: ein erfundenes Konto beim erfundenen Broker. Es kennt keine Toepfe, nur
//! das Gesamtkonto, und fuehrt seinen Stand selbst, unabhaengig vom Journal. So hat der
//! Kontowaechter etwas, womit er abgleichen kann.
//!
//! **Senden:** Die Auftragsstelle sendet nur `GenehmigterAuftrag` (R3). Das Konto erkennt eine
//! Kennung, die es schon kennt, und lehnt sie ab; ein Auftrag ohne Deckung im Gesamtkonto wird
//! abgelehnt.
//!
//! **Markt:** Tagesorders werden zu den Schlusskursen des Tages ausgefuehrt, wenn der Kurs das
//! Limit einhaelt, sonst verfallen sie. Jede Ausfuehrung kostet die Gebuehr nach Tarif. Ein
//! Kurs feiner als ein Cent wird zur sicheren Seite gerundet: ein Kauf zum aufgerundeten, ein
//! Verkauf zum abgerundeten Cent. Das Limit gilt am exakten Kurs.
//!
//! **Datei:** `als_text` und `aus_text` halten das Konto ueber einen Neustart, unabhaengig vom
//! Journal. Nur so hat der Kontowaechter beim Papiertag etwas, womit er abgleichen kann.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use depot_journal::{Ausfuehrung, Konto, KontoOrder, KontoStatus, Kontoauszug};
use depot_kern::{Boerse, Cent, Datum, Gebuehren, Grenzen, Isin, Kurse, Rundung, Topf};
use depot_regelwerk::{GenehmigterAuftrag, Richtung};

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
    /// Gibt faellige Verkaufserloese zum Kauf frei.
    fn rechne_ab(&mut self, _tag: Datum) {}
    /// Noch nicht abgerechnete Nettoerloese je Auftragskennung.
    fn offene_erloese(&self) -> Option<&BTreeMap<u64, (Datum, Cent)>> {
        None
    }
    /// Eine Stoerung des Kontos: Sein Stand ist nicht gesichert. Dann haelt der Lauf an, bevor
    /// das Journal noch etwas vom Konto bestaetigt (K23-01).
    fn stoerung(&self) -> Option<String> {
        None
    }
}

/// Was das Konto von einem Auftrag festhaelt.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Order {
    isin: Isin,
    richtung: Richtung,
    stueck: u64,
    limit: Cent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PapierOrder {
    auftrag: Order,
    status: KontoStatus,
    ausfuehrungen: Vec<Ausfuehrung>,
}

/// Das erfundene Gesamtkonto.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Papierkonto {
    bargeld: Cent,
    bestand: BTreeMap<Isin, u64>,
    orders: BTreeMap<u64, PapierOrder>,
    offene_erloese: BTreeMap<u64, (Datum, Cent)>,
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
            offene_erloese: BTreeMap::new(),
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

    fn abgerechnetes_bargeld(&self) -> Option<Cent> {
        self.offene_erloese
            .values()
            .try_fold(self.bargeld, |saldo, (_, erloes)| {
                saldo.checked_sub(*erloes)
            })
    }

    fn gedeckt(&self, a: &Order) -> Option<bool> {
        match a.richtung {
            Richtung::Kauf => {
                let mut frei = self.abgerechnetes_bargeld()?;
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

    fn bedarf(&self, a: &Order) -> Option<Cent> {
        let v = a.limit.mal_stueck(a.stueck)?;
        v.checked_add(self.gebuehren.fuer(v)?)
    }

    /// Fuehrt eine offene Order ganz aus, wenn der Kurs das Limit einhaelt; sonst verfaellt sie.
    fn schluss(&mut self, nr: u64, kurse: &Kurse) {
        let Some(o) = self.orders.get(&nr) else {
            return;
        };
        let a = &o.auftrag;
        let ausfuehrung = kurse.eintrag(&a.isin).and_then(|eintrag| {
            let exakt = eintrag.preis();
            // Limit in ganzen Cent: Aufgerundet liegt ein Kurs genau dann nicht ueber dem Limit,
            // wenn er es exakt nicht tut; abgerundet ebenso nicht darunter.
            let (kurs, im_limit) = match a.richtung {
                Richtung::Kauf => {
                    let k = exakt.mal_stueck(1, Rundung::Auf)?;
                    (k, k <= a.limit)
                }
                Richtung::Verkauf => {
                    let k = exakt.mal_stueck(1, Rundung::Ab)?;
                    (k, k >= a.limit)
                }
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
            let erloes = volumen.checked_sub(gebuehr)?;
            let abrechnung = if a.richtung == Richtung::Verkauf && erloes > Cent::NULL {
                Some((abrechnungstag(eintrag.handelstag())?, erloes))
            } else {
                None
            };
            Some((
                bargeld,
                abrechnung,
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
            Some((bargeld, abrechnung, stueck, aus)) => {
                self.bargeld = bargeld;
                if let Some(erloes) = abrechnung {
                    self.offene_erloese.insert(nr, erloes);
                }
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

// Einheitliche Regel des EUR-Papierkontos: zwei folgende Xetra-Handelstage.
fn abrechnungstag(mut tag: Datum) -> Option<Datum> {
    for _ in 0..2 {
        loop {
            tag = tag.plus_tage(1)?;
            if Boerse::Xetra.handelstag(tag) {
                break;
            }
        }
    }
    Some(tag)
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
        let o = Order {
            isin: a.isin.clone(),
            richtung: a.richtung,
            stueck: a.stueck,
            limit: a.limit,
        };
        let status = if self.gedeckt(&o) == Some(true) {
            KontoStatus::Offen
        } else {
            KontoStatus::Abgelehnt
        };
        self.orders.insert(
            nr,
            PapierOrder {
                auftrag: o,
                status,
                ausfuehrungen: Vec::new(),
            },
        );
        match status {
            KontoStatus::Offen => Sendeantwort::Angenommen,
            _ => Sendeantwort::Abgelehnt("keine Deckung im Konto".into()),
        }
    }

    fn rechne_ab(&mut self, tag: Datum) {
        self.offene_erloese.retain(|_, (faellig, _)| *faellig > tag);
    }

    fn offene_erloese(&self) -> Option<&BTreeMap<u64, (Datum, Cent)>> {
        Some(&self.offene_erloese)
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

const KOPF: &str = "papierkonto-v1";

fn richtung_text(r: Richtung) -> &'static str {
    match r {
        Richtung::Kauf => "kauf",
        Richtung::Verkauf => "verkauf",
    }
}

fn status_text(s: KontoStatus) -> &'static str {
    match s {
        KontoStatus::Offen => "offen",
        KontoStatus::Gefuellt => "gefuellt",
        KontoStatus::Abgelehnt => "abgelehnt",
        KontoStatus::Storniert => "storniert",
        KontoStatus::Verfallen => "verfallen",
    }
}

fn lies_status(t: &str) -> Option<KontoStatus> {
    [
        KontoStatus::Offen,
        KontoStatus::Gefuellt,
        KontoStatus::Abgelehnt,
        KontoStatus::Storniert,
        KontoStatus::Verfallen,
    ]
    .into_iter()
    .find(|s| status_text(*s) == t)
}

fn lies_richtung(t: &str) -> Option<Richtung> {
    [Richtung::Kauf, Richtung::Verkauf]
        .into_iter()
        .find(|r| richtung_text(*r) == t)
}

impl Papierkonto {
    /// Das Konto als Text, eine Angabe je Zeile. `aus_text` liest genau diese Form.
    pub fn als_text(&self) -> String {
        let mut t = format!(
            "{KOPF}\nbargeld;{}\nsendungen;{}\n",
            self.bargeld.0, self.sendungen
        );
        for (isin, n) in &self.bestand {
            let _ = writeln!(t, "bestand;{isin};{n}");
        }
        for (nr, o) in &self.orders {
            let a = &o.auftrag;
            let _ = writeln!(
                t,
                "order;{nr};{};{};{};{};{}",
                a.isin,
                richtung_text(a.richtung),
                a.stueck,
                a.limit.0,
                status_text(o.status)
            );
            for x in &o.ausfuehrungen {
                let _ = writeln!(
                    t,
                    "ausfuehrung;{nr};{};{};{}",
                    x.stueck, x.kurs.0, x.gebuehr.0
                );
            }
        }
        for (nr, (datum, erloes)) in &self.offene_erloese {
            let _ = writeln!(t, "abrechnung;{nr};{datum};{}", erloes.0);
        }
        t
    }

    /// Liest ein Konto aus `als_text`. Nur genau diese Form gilt: Was sich nicht Zeichen fuer
    /// Zeichen wieder so schreiben laesst, wird abgelehnt (fail-closed).
    ///
    /// # Errors
    /// Nennt die erste unlesbare Zeile.
    pub fn aus_text(text: &str) -> Result<Papierkonto, String> {
        let mut zeilen = text.lines();
        if zeilen.next() != Some(KOPF) {
            return Err(format!("Papierkonto: Kopf {KOPF} fehlt"));
        }
        let mut k = Papierkonto {
            bargeld: Cent::NULL,
            bestand: BTreeMap::new(),
            orders: BTreeMap::new(),
            offene_erloese: BTreeMap::new(),
            gebuehren: Grenzen::standard().gebuehren(),
            sendungen: 0,
        };
        for z in zeilen {
            let f: Vec<&str> = z.split(';').collect();
            let fehler = || format!("Papierkonto: Zeile unlesbar: {z}");
            match f.as_slice() {
                ["bargeld", b] => k.bargeld = Cent(b.parse().map_err(|_| fehler())?),
                ["sendungen", n] => k.sendungen = n.parse().map_err(|_| fehler())?,
                ["bestand", isin, n] => {
                    let isin = Isin::neu(isin).map_err(|_| fehler())?;
                    k.bestand.insert(isin, n.parse().map_err(|_| fehler())?);
                }
                ["order", nr, isin, r, stueck, limit, status] => {
                    let o = PapierOrder {
                        auftrag: Order {
                            isin: Isin::neu(isin).map_err(|_| fehler())?,
                            richtung: lies_richtung(r).ok_or_else(fehler)?,
                            stueck: stueck.parse().map_err(|_| fehler())?,
                            limit: Cent(limit.parse().map_err(|_| fehler())?),
                        },
                        status: lies_status(status).ok_or_else(fehler)?,
                        ausfuehrungen: Vec::new(),
                    };
                    k.orders.insert(nr.parse().map_err(|_| fehler())?, o);
                }
                ["ausfuehrung", nr, stueck, kurs, gebuehr] => {
                    let nr: u64 = nr.parse().map_err(|_| fehler())?;
                    let o = k.orders.get_mut(&nr).ok_or_else(fehler)?;
                    o.ausfuehrungen.push(Ausfuehrung {
                        stueck: stueck.parse().map_err(|_| fehler())?,
                        kurs: Cent(kurs.parse().map_err(|_| fehler())?),
                        gebuehr: Cent(gebuehr.parse().map_err(|_| fehler())?),
                    });
                }
                ["abrechnung", nr, datum, netto] => {
                    let nr: u64 = nr.parse().map_err(|_| fehler())?;
                    let datum = Datum::lies(datum).ok_or_else(fehler)?;
                    let netto = Cent(netto.parse().map_err(|_| fehler())?);
                    let order = k.orders.get(&nr).ok_or_else(fehler)?;
                    let letzte = order.ausfuehrungen.last().ok_or_else(fehler)?;
                    let erloes = letzte
                        .kurs
                        .mal_stueck(letzte.stueck)
                        .and_then(|brutto| brutto.checked_sub(letzte.gebuehr));
                    if order.auftrag.richtung != Richtung::Verkauf
                        || order.status != KontoStatus::Gefuellt
                        || netto <= Cent::NULL
                        || erloes != Some(netto)
                    {
                        return Err(fehler());
                    }
                    k.offene_erloese.insert(nr, (datum, netto));
                }
                _ => return Err(fehler()),
            }
        }
        if k.abgerechnetes_bargeld().is_none() || k.als_text() != text {
            return Err("Papierkonto: nicht in der eigenen Form geschrieben".into());
        }
        Ok(k)
    }
}
