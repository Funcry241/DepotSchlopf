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

use depot_journal::{
    Abrechnungen, Ausfuehrung, Konto, KontoOrder, KontoStatus, Kontoauszug, papier_valuta,
};
use depot_kern::liste::papierorder_schluss;
use depot_kern::{Cent, Datum, Gebuehren, Grenzen, Isin, Kurse, Rundung, Topf, Zeitpunkt};
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
    /// Versand mit belegter Zeit fuer einen Auftrag im Papiertag.
    fn sende_am(&mut self, _auftrag: &GenehmigterAuftrag, _zeit: Zeitpunkt) -> Sendeantwort {
        Sendeantwort::Abgelehnt("Konto unterstuetzt keinen zeitbelegten Versand".into())
    }
    /// Storniert im Halt offene Kaeufe und bestaetigt, dass keine mehr offen sind.
    fn stoppe_kaeufe(&mut self) -> bool {
        false
    }
    /// Der Markt laeuft bis zum Schluss dieses Handelstags.
    fn markt(&mut self, kurse: &Kurse);
    /// Gibt faellige Verkaufserloese zum Kauf frei.
    fn rechne_ab(&mut self, _tag: Datum) {}
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
    versand: Option<(Zeitpunkt, Zeitpunkt)>,
}

/// Das erfundene Gesamtkonto.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Papierkonto {
    bargeld: Cent,
    bestand: BTreeMap<Isin, u64>,
    orders: BTreeMap<u64, PapierOrder>,
    abrechnungen: Abrechnungen,
    abrechnungstag: Option<Datum>,
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
            abrechnungen: Abrechnungen::new(),
            abrechnungstag: None,
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
        self.abrechnungen
            .values()
            .filter(|(valuta, _)| self.abrechnungstag.is_none_or(|tag| *valuta > tag))
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
        let mut eintrag = kurse.eintrag(&a.isin);
        if let Some((_, schluss)) = o.versand {
            let stand = kurse.stand();
            if stand < schluss {
                return;
            }
            eintrag =
                eintrag.filter(|k| stand.datum() == schluss.datum() && k.zeitpunkt() == schluss);
        }
        let ausfuehrung = eintrag.and_then(|eintrag| {
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
                Some((papier_valuta(eintrag.handelstag())?, erloes))
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
                    kurszeit: o.versand.map(|_| eintrag.zeitpunkt()),
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
                    self.abrechnungen.insert(nr, erloes);
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
            abrechnungen: Some(self.abrechnungen.clone()),
        }
    }

    fn order(&self, nr: u64) -> Option<KontoOrder> {
        self.orders.get(&nr).map(|o| KontoOrder {
            status: o.status,
            ausfuehrungen: o.ausfuehrungen.clone(),
            versand: o.versand,
        })
    }
}

impl Papierkonto {
    fn nehme_an(&mut self, g: &GenehmigterAuftrag, versand: Option<Zeitpunkt>) -> Sendeantwort {
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
        let versand = if let Some(zeit) = versand {
            let Some(frist) = papierorder_schluss(&o.isin, zeit) else {
                return Sendeantwort::Abgelehnt("keine Frist der Börse".into());
            };
            Some((zeit, frist))
        } else {
            None
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
                versand,
            },
        );
        match status {
            KontoStatus::Offen => Sendeantwort::Angenommen,
            _ => Sendeantwort::Abgelehnt("keine Deckung im Konto".into()),
        }
    }
}

impl Broker for Papierkonto {
    fn sende(&mut self, g: &GenehmigterAuftrag) -> Sendeantwort {
        self.nehme_an(g, None)
    }

    fn sende_am(&mut self, g: &GenehmigterAuftrag, zeit: Zeitpunkt) -> Sendeantwort {
        self.nehme_an(g, Some(zeit))
    }

    fn stoppe_kaeufe(&mut self) -> bool {
        for o in self.orders.values_mut() {
            if o.status == KontoStatus::Offen && o.auftrag.richtung == Richtung::Kauf {
                o.status = KontoStatus::Storniert;
            }
        }
        true
    }

    fn rechne_ab(&mut self, tag: Datum) {
        self.abrechnungstag = self.abrechnungstag.max(Some(tag));
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

const KOPF_ALT: &str = "papierkonto-v1";
const KOPF: &str = "papierkonto-v2";

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
        let kopf = if self.orders.values().any(|o| o.versand.is_some()) {
            KOPF
        } else {
            KOPF_ALT
        };
        let mut t = format!(
            "{kopf}\nbargeld;{}\nsendungen;{}\n",
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
            if let Some((zeit, frist)) = o.versand {
                let _ = writeln!(t, "versand;{nr};{zeit};{frist}");
            }
            for x in &o.ausfuehrungen {
                let _ = write!(
                    t,
                    "ausfuehrung;{nr};{};{};{}",
                    x.stueck, x.kurs.0, x.gebuehr.0
                );
                if let Some(zeit) = x.kurszeit {
                    let _ = write!(t, ";{zeit}");
                }
                t.push('\n');
            }
        }
        for (nr, (datum, erloes)) in &self.abrechnungen {
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
        if !matches!(zeilen.next(), Some(KOPF | KOPF_ALT)) {
            return Err(format!("Papierkonto: Kopf {KOPF} fehlt"));
        }
        let mut k = Papierkonto {
            bargeld: Cent::NULL,
            bestand: BTreeMap::new(),
            orders: BTreeMap::new(),
            abrechnungen: Abrechnungen::new(),
            abrechnungstag: None,
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
                        versand: None,
                    };
                    k.orders.insert(nr.parse().map_err(|_| fehler())?, o);
                }
                ["versand", nr, zeit, frist] => {
                    let nr = nr.parse().map_err(|_| fehler())?;
                    let o = k.orders.get_mut(&nr).ok_or_else(fehler)?;
                    let zeit = Zeitpunkt::lies(zeit).ok_or_else(fehler)?;
                    let frist = Zeitpunkt::lies(frist)
                        .filter(|f| *f > zeit)
                        .ok_or_else(fehler)?;
                    o.versand = Some((zeit, frist));
                }
                ["ausfuehrung", nr, stueck, kurs, gebuehr, rest @ ..] => {
                    let nr: u64 = nr.parse().map_err(|_| fehler())?;
                    let o = k.orders.get_mut(&nr).ok_or_else(fehler)?;
                    let kurszeit = match (o.versand, rest) {
                        (None, []) => None,
                        (Some((_, frist)), [zeit]) if Zeitpunkt::lies(zeit) == Some(frist) => {
                            Some(frist)
                        }
                        _ => return Err(fehler()),
                    };
                    o.ausfuehrungen.push(Ausfuehrung {
                        stueck: stueck.parse().map_err(|_| fehler())?,
                        kurs: Cent(kurs.parse().map_err(|_| fehler())?),
                        gebuehr: Cent(gebuehr.parse().map_err(|_| fehler())?),
                        kurszeit,
                    });
                }
                ["abrechnung", nr, datum, netto] => {
                    let nr: u64 = nr.parse().map_err(|_| fehler())?;
                    let datum = Datum::lies(datum).ok_or_else(fehler)?;
                    let netto = Cent(netto.parse().map_err(|_| fehler())?);
                    let order = k.orders.get(&nr).ok_or_else(fehler)?;
                    let erloes = order.ausfuehrungen.iter().try_fold(Cent::NULL, |summe, a| {
                        let netto = a.kurs.mal_stueck(a.stueck)?.checked_sub(a.gebuehr)?;
                        summe.checked_add(netto.max(Cent::NULL))
                    });
                    if order.auftrag.richtung != Richtung::Verkauf
                        || order.status != KontoStatus::Gefuellt
                        || netto <= Cent::NULL
                        || erloes != Some(netto)
                    {
                        return Err(fehler());
                    }
                    k.abrechnungen.insert(nr, (datum, netto));
                }
                _ => return Err(fehler()),
            }
        }
        if k.als_text() != text {
            return Err("Papierkonto: nicht in der eigenen Form geschrieben".into());
        }
        Ok(k)
    }
}
