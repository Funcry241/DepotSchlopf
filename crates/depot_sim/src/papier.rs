//! Das Papierkonto: ein erfundenes Konto beim erfundenen Broker. Es kennt keine Toepfe, nur
//! das Gesamtkonto, und fuehrt seinen Stand selbst, unabhaengig vom Journal. So hat der
//! Kontowaechter etwas, womit er abgleichen kann.
//!
//! **Senden:** Die Auftragsstelle sendet nur `GenehmigterAuftrag` (R3). Das Konto erkennt eine
//! Kennung, die es schon kennt, und lehnt sie ab; ein Auftrag ohne Deckung im Gesamtkonto wird
//! abgelehnt.
//!
//! **Markt:** Tagesorders werden zum aus dem Schlusskurs abgeleiteten Brief- oder Geldkurs
//! ausgeführt. Das Limit gilt gegen diesen Preis. Käufe werden auf Cent aufgerundet,
//! Verkäufe abgerundet. Jede Ausführung kostet zusätzlich die Gebühr nach Tarif.
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
        Sendeantwort::Abgelehnt("Konto unterstützt keinen zeitbelegten Versand".into())
    }
    /// Storniert im Halt offene Kaeufe und bestaetigt, dass keine mehr offen sind.
    fn stoppe_kaeufe(&mut self) -> bool {
        false
    }
    /// Kann das Konto einen Kauf an Verkäufe desselben Schlusses binden (#311 H1b)? Nur das
    /// Papierkonto; ein echter Broker braucht dafür eine bedingte Order.
    fn tausch_moeglich(&self) -> bool {
        false
    }
    /// Der Markt laeuft bis zum Schluss dieses Handelstags.
    fn markt(&mut self, kurse: &Kurse);
    fn markt_fuer(&mut self, kurse: &Kurse, nummern: &std::collections::BTreeSet<u64>);
    /// Beendet offene Aufträge, deren Schlusskurs nicht mehr nachholbar ist.
    fn verfalle(&mut self, nummern: &std::collections::BTreeSet<u64>);
    /// Gibt faellige Verkaufserloese zum Kauf frei.
    fn rechne_ab(&mut self, _tag: Datum) {}
    /// Holt belegte Split-Umbuchungen vor dem Abgleich nach.
    fn kapitalmassnahmen(
        &mut self,
        massnahmen: &[depot_journal::kapitalmassnahme::GebuchteKapitalmassnahme],
    ) -> Result<(), String> {
        if massnahmen.is_empty() {
            Ok(())
        } else {
            Err("Konto unterstützt keine Split-Umbuchungen".into())
        }
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
    versand: Option<(Zeitpunkt, Zeitpunkt)>,
    /// Tauschkauf (#311 H1b): füllt nur, wenn diese Verkäufe vorher gefüllt sind.
    nach: Vec<u64>,
}

/// Das erfundene Gesamtkonto.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Papierkonto {
    bargeld: Cent,
    bestand: BTreeMap<Isin, u64>,
    kapitalbelege: Vec<String>,
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
            kapitalbelege: Vec::new(),
            orders: BTreeMap::new(),
            abrechnungen: Abrechnungen::new(),
            abrechnungstag: None,
            gebuehren: Grenzen::standard().gebuehren(),
            sendungen: 0,
        })
    }

    pub(crate) fn hole_kapitalmassnahmen(
        &mut self,
        massnahmen: &[depot_journal::kapitalmassnahme::GebuchteKapitalmassnahme],
    ) -> Result<bool, String> {
        if self.kapitalbelege.len() > massnahmen.len()
            || self
                .kapitalbelege
                .iter()
                .zip(massnahmen)
                .any(|(kennung, m)| *kennung != m.massnahme.kennung())
        {
            return Err("Papierkonto: Split-Belege passen nicht zum Journal".into());
        }
        let fehlend = massnahmen.iter().skip(self.kapitalbelege.len());
        if fehlend.len() == 0 {
            return Ok(false);
        }
        let mut bestand = self.bestand.clone();
        let mut kennungen = Vec::new();
        for m in fehlend {
            let isin = &m.massnahme.isin;
            if self.offen().any(|o| &o.auftrag.isin == isin)
                || bestand.get(isin).copied() != Some(m.vorher_gesamt)
            {
                return Err(format!(
                    "Papierkonto: Bestand oder offene Order verhindert Split für {isin}"
                ));
            }
            bestand.insert(isin.clone(), m.danach_gesamt);
            kennungen.push(m.massnahme.kennung());
        }
        self.bestand = bestand;
        self.kapitalbelege.extend(kennungen);
        Ok(true)
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

    /// Ein Tauschkauf, dessen Verkauf ungefüllt endete, kann nicht mehr füllen.
    fn tausch_unmoeglich(&self, o: &PapierOrder) -> bool {
        o.nach.iter().any(|v| {
            self.orders
                .get(v)
                .is_none_or(|s| !matches!(s.status, KontoStatus::Offen | KontoStatus::Gefuellt))
        })
    }

    /// `tausch;NR;3+4`: Ein Kauf hängt an aufsteigenden, verschiedenen Verkaufskennungen.
    fn lies_tausch(&mut self, nr: &str, liste: &str) -> Option<()> {
        let nr: u64 = nr.parse().ok()?;
        let nach: Vec<u64> = liste
            .split('+')
            .map(|v| v.parse().ok())
            .collect::<Option<_>>()?;
        let o = self.orders.get_mut(&nr)?;
        (o.auftrag.richtung == Richtung::Kauf
            && o.nach.is_empty()
            && !nach.is_empty()
            && nach.windows(2).all(|w| w.first() < w.get(1)))
        .then(|| o.nach = nach)
    }

    fn gedeckt(&self, a: &Order, nach: &[u64]) -> Option<bool> {
        match a.richtung {
            Richtung::Kauf => {
                // Gefüllte Erlöse vor Valuta decken Käufe wie beim Bargeldkonto (#311 H1a).
                let mut frei = self.bargeld;
                let mut verkaeufe: std::collections::BTreeSet<u64> = nach.iter().copied().collect();
                for o in self
                    .offen()
                    .filter(|o| o.auftrag.richtung == Richtung::Kauf && !self.tausch_unmoeglich(o))
                {
                    frei = frei.checked_sub(self.bedarf(&o.auftrag)?)?;
                    verkaeufe.extend(o.nach.iter().copied());
                }
                // Tauschkäufe rechnen den schlechtesten Erlös ihrer offenen Verkäufe ein (#311 H1b).
                for v in verkaeufe {
                    if let Some(s) = self.orders.get(&v).filter(|s| {
                        s.status == KontoStatus::Offen && s.auftrag.richtung == Richtung::Verkauf
                    }) {
                        let volumen = s.auftrag.limit.mal_stueck(s.auftrag.stueck)?;
                        let netto = volumen.checked_sub(self.gebuehren.fuer(volumen)?)?;
                        frei = frei.checked_add(netto.max(Cent::NULL))?;
                    }
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

    /// Führt eine offene Order am belegten Schluss aus, wenn der Kurs das Limit einhält.
    /// Sie wartet auf den Schlusskurs und verfällt bei einem späteren Kurs ohne passenden Beleg.
    fn schluss(&mut self, nr: u64, kurse: &Kurse) {
        let Some(o) = self.orders.get(&nr) else {
            return;
        };
        let a = &o.auftrag;
        if !o.nach.is_empty()
            && o.nach
                .iter()
                .any(|v| self.orders.get(v).map(|s| s.status) != Some(KontoStatus::Gefuellt))
        {
            // Der Tauschkauf füllt nur nach seinen Verkäufen; sonst verfällt er an seinem Schluss.
            let faellig = o.versand.is_none_or(|(_, schluss)| {
                kurse.stand() >= schluss
                    && kurse
                        .eintrag(&a.isin)
                        .is_some_and(|k| k.zeitpunkt() >= schluss)
            });
            if faellig && let Some(o) = self.orders.get_mut(&nr) {
                o.status = KontoStatus::Verfallen;
            }
            return;
        }
        let mut eintrag = kurse.eintrag(&a.isin);
        if let Some((_, schluss)) = o.versand {
            if kurse.stand() < schluss || eintrag.is_none_or(|k| k.zeitpunkt() < schluss) {
                return;
            }
            eintrag = eintrag.filter(|k| k.zeitpunkt() == schluss);
        }
        let ausfuehrung = eintrag.and_then(|eintrag| {
            let rundung = match a.richtung {
                Richtung::Kauf => Rundung::Auf,
                Richtung::Verkauf => Rundung::Ab,
            };
            let kurs = match depot_kern::liste::boerse_von(&a.isin) {
                Some(boerse) => Grenzen::standard().papierkurs(boerse, eintrag.preis(), rundung)?,
                None if o.versand.is_none() => eintrag.preis().mal_stueck(1, rundung)?,
                None => return None,
            };
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
        let nach = g.nach().to_vec();
        let tausch_ok = nach.iter().all(|v| {
            self.orders.get(v).is_some_and(|s| {
                s.status == KontoStatus::Offen
                    && s.auftrag.richtung == Richtung::Verkauf
                    && s.versand.map(|(_, f)| f) == versand.map(|(_, f)| f)
            })
        });
        let status = if tausch_ok && self.gedeckt(&o, &nach) == Some(true) {
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
                nach,
            },
        );
        match status {
            KontoStatus::Offen => Sendeantwort::Angenommen,
            _ => Sendeantwort::Abgelehnt("keine Deckung im Konto".into()),
        }
    }
}

impl Broker for Papierkonto {
    fn tausch_moeglich(&self) -> bool {
        true
    }

    fn kapitalmassnahmen(
        &mut self,
        massnahmen: &[depot_journal::kapitalmassnahme::GebuchteKapitalmassnahme],
    ) -> Result<(), String> {
        self.hole_kapitalmassnahmen(massnahmen).map(|_| ())
    }

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
        let offen = self.orders.keys().copied().collect();
        self.markt_fuer(kurse, &offen);
    }

    fn markt_fuer(&mut self, kurse: &Kurse, nummern: &std::collections::BTreeSet<u64>) {
        // Erst die Verkäufe, dann die Käufe: So hat ein Tauschkauf den Erlös (#311 H1b).
        let mut folge: Vec<u64> = nummern.iter().copied().collect();
        folge.sort_by_key(|nr| {
            self.orders
                .get(nr)
                .is_some_and(|o| o.auftrag.richtung == Richtung::Kauf)
        });
        for nr in &folge {
            if self
                .orders
                .get(nr)
                .is_some_and(|o| o.status == KontoStatus::Offen)
            {
                self.schluss(*nr, kurse);
            }
        }
    }

    fn verfalle(&mut self, nummern: &std::collections::BTreeSet<u64>) {
        for nr in nummern {
            if let Some(order) = self.orders.get_mut(nr)
                && order.status == KontoStatus::Offen
            {
                order.status = KontoStatus::Verfallen;
            }
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
        for kennung in &self.kapitalbelege {
            let _ = writeln!(t, "kapitalmassnahme;{kennung}");
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
            if !o.nach.is_empty() {
                let nach: Vec<String> = o.nach.iter().map(u64::to_string).collect();
                let _ = writeln!(t, "tausch;{nr};{}", nach.join("+"));
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
            kapitalbelege: Vec::new(),
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
                ["kapitalmassnahme", kennung]
                    if kennung.len() == 64
                        && kennung
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                        && !k.kapitalbelege.iter().any(|b| b == kennung) =>
                {
                    k.kapitalbelege.push((*kennung).to_string());
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
                        nach: Vec::new(),
                    };
                    k.orders.insert(nr.parse().map_err(|_| fehler())?, o);
                }
                ["tausch", nr, liste] => k.lies_tausch(nr, liste).ok_or_else(fehler)?,
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

#[cfg(test)]
mod tests {
    use super::*;
    use depot_kern::{Boerse, Herkunft, Kurs};
    use std::collections::BTreeSet;

    fn zwei_orders() -> Result<(Papierkonto, Isin, Zeitpunkt, Zeitpunkt), String> {
        let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let mut konto = Papierkonto::eroeffne(&[
            Topf::neu(depot_kern::TopfId::A, Cent(100_000)).map_err(|e| format!("{e:?}"))?
        ])
        .ok_or("Konto")?;
        let mut fristen = Vec::new();
        for (nr, tag) in [(1, 9), (2, 10)] {
            let datum = Datum::neu(2026, 11, tag).ok_or("Datum")?;
            let versand = Zeitpunkt::frankfurt(datum, 7, 40, 0).ok_or("Versand")?;
            let frist = Boerse::Xetra.schluss(datum).ok_or("Schluss")?;
            konto.orders.insert(
                nr,
                PapierOrder {
                    auftrag: Order {
                        isin: isin.clone(),
                        richtung: Richtung::Kauf,
                        stueck: 1,
                        limit: Cent(6_000),
                    },
                    status: KontoStatus::Offen,
                    ausfuehrungen: Vec::new(),
                    versand: Some((versand, frist)),
                    nach: Vec::new(),
                },
            );
            fristen.push(frist);
        }
        Ok((
            konto,
            isin,
            *fristen.first().ok_or("Erste Frist")?,
            *fristen.get(1).ok_or("Zweite Frist")?,
        ))
    }

    fn schlusskurs(isin: &Isin, schluss: Zeitpunkt, cent: i64) -> Result<Kurse, String> {
        let datum = schluss.frankfurt_tag().ok_or("Datum")?;
        let kurs = Kurs::neu(Cent(cent), "EUR", datum, schluss, Herkunft::Quelle)
            .map_err(|e| format!("{e:?}"))?;
        Kurse::neu(schluss, vec![(isin.clone(), kurs)]).map_err(|e| format!("{e:?}"))
    }

    #[test]
    fn kauf_mit_abschlag() -> Result<(), String> {
        let (mut konto, isin, frist, _) = zwei_orders()?;
        konto.orders.get_mut(&1).ok_or("Order")?.auftrag.limit = Cent(10_100);
        konto.markt_fuer(&schlusskurs(&isin, frist, 10_000)?, &BTreeSet::from([1]));
        let auftrag = konto.order(1).ok_or("Order")?;
        assert_eq!(auftrag.status, KontoStatus::Gefuellt);
        assert_eq!(
            auftrag.ausfuehrungen.first().map(|f| f.kurs),
            Some(Cent(10_005))
        );
        Ok(())
    }

    #[test]
    fn verkauf_mit_abschlag() -> Result<(), String> {
        let (mut konto, _, versandfrist, _) = zwei_orders()?;
        let isin = Isin::neu("US67066G1040").map_err(|e| format!("{e:?}"))?;
        let tag = versandfrist.frankfurt_tag().ok_or("Tag")?;
        let frist = Boerse::Nyse.schluss(tag).ok_or("NYSE-Schluss")?;
        let versand = Zeitpunkt::frankfurt(tag, 7, 40, 0).ok_or("Versand")?;
        let order = konto.orders.get_mut(&1).ok_or("Order")?;
        order.auftrag.isin = isin.clone();
        order.auftrag.richtung = Richtung::Verkauf;
        order.auftrag.limit = Cent(9_900);
        order.versand = Some((versand, frist));
        konto.bestand.insert(isin.clone(), 1);
        konto.markt_fuer(&schlusskurs(&isin, frist, 10_000)?, &BTreeSet::from([1]));
        let auftrag = konto.order(1).ok_or("Order")?;
        assert_eq!(auftrag.status, KontoStatus::Gefuellt);
        assert_eq!(
            auftrag.ausfuehrungen.first().map(|f| f.kurs),
            Some(Cent(9_985))
        );
        Ok(())
    }

    #[test]
    fn limit_gegen_abschlag() -> Result<(), String> {
        let (mut konto, isin, frist, _) = zwei_orders()?;
        konto.orders.get_mut(&1).ok_or("Order")?.auftrag.limit = Cent(10_100);
        konto.markt_fuer(&schlusskurs(&isin, frist, 10_098)?, &BTreeSet::from([1]));
        let auftrag = konto.order(1).ok_or("Order")?;
        assert_eq!(auftrag.status, KontoStatus::Verfallen);
        assert!(auftrag.ausfuehrungen.is_empty());
        Ok(())
    }

    #[test]
    fn zwei_orders_gleiche_isin() -> Result<(), String> {
        let (mut konto, isin, erste, zweite) = zwei_orders()?;
        konto.markt_fuer(&schlusskurs(&isin, erste, 5_000)?, &BTreeSet::from([1]));
        assert_eq!(
            konto.order(1).map(|o| o.status),
            Some(KontoStatus::Gefuellt)
        );
        assert_eq!(konto.order(2).map(|o| o.status), Some(KontoStatus::Offen));
        konto.markt_fuer(&schlusskurs(&isin, zweite, 7_000)?, &BTreeSet::from([2]));
        assert_eq!(
            konto.order(2).map(|o| o.status),
            Some(KontoStatus::Verfallen)
        );
        Ok(())
    }

    #[test]
    fn zwei_schlusszeiten() -> Result<(), String> {
        let (mut konto, isin, erste, zweite) = zwei_orders()?;
        let allianz = Isin::neu("DE0008404005").map_err(|e| format!("{e:?}"))?;
        konto.orders.get_mut(&2).ok_or("Zweite Order")?.auftrag.isin = allianz.clone();
        konto.markt_fuer(&schlusskurs(&isin, erste, 5_000)?, &BTreeSet::from([1]));
        let offen = konto.order(2).ok_or("Zweite Order")?;
        assert_eq!(offen.status, KontoStatus::Offen);
        let datum = Datum::neu(2026, 11, 11).ok_or("Folgetag")?;
        let spaeter = Boerse::Xetra.schluss(datum).ok_or("Späterer Schluss")?;
        let belegt = schlusskurs(&allianz, zweite, 5_500)?;
        let spaeterer_kurs = schlusskurs(&allianz, spaeter, 7_500)?;
        konto.markt_fuer(&belegt, &BTreeSet::from([2]));
        konto.markt(&spaeterer_kurs);
        let gefuellt = konto.order(2).ok_or("Zweite Order")?;
        assert_eq!(gefuellt.status, KontoStatus::Gefuellt);
        assert_eq!(gefuellt.versand, offen.versand);
        let aus = gefuellt.ausfuehrungen.first().ok_or("Ausführung")?;
        assert_eq!((aus.kurs, aus.kurszeit), (Cent(5_503), Some(zweite)));
        Ok(())
    }

    #[test]
    fn eintrag_vor_schluss_bleibt_offen() -> Result<(), String> {
        let (mut konto, isin, erste, zweite) = zwei_orders()?;
        let alter_kurs = schlusskurs(&isin, erste, 5_000)?;
        let beleg = alter_kurs.eintrag(&isin).cloned().ok_or("Kurs")?;
        let kurse = Kurse::neu(zweite, vec![(isin, beleg)]).map_err(|e| format!("{e:?}"))?;
        konto.markt_fuer(&kurse, &BTreeSet::from([2]));
        let order = konto.order(2).ok_or("Order")?;
        assert_eq!(order.status, KontoStatus::Offen);
        assert_eq!(order.versand.map(|(_, frist)| frist), Some(zweite));
        Ok(())
    }
}

#[cfg(test)]
mod tausch_tests {
    use super::*;
    use depot_kern::{Boerse, Herkunft, Kurs};
    use std::collections::BTreeSet;

    const SAP: &str = "DE0007164600";
    const ALV: &str = "DE0008404005";

    /// Konto mit `bargeld`, 10 SAP; Order 1 verkauft 10 SAP zu mindestens 99 €, Order 2 kauft
    /// `stueck` Allianz zu höchstens 101 € und hängt an Order 1.
    fn tausch(bargeld: i64, stueck: u64) -> Result<(Papierkonto, Zeitpunkt), String> {
        let sap = Isin::neu(SAP).map_err(|e| format!("{e:?}"))?;
        let alv = Isin::neu(ALV).map_err(|e| format!("{e:?}"))?;
        let mut topf =
            Topf::neu(depot_kern::TopfId::A, Cent(bargeld)).map_err(|e| format!("{e:?}"))?;
        topf.setze_bestand(sap.clone(), 10);
        let mut konto = Papierkonto::eroeffne(&[topf]).ok_or("Konto")?;
        let datum = Datum::neu(2026, 11, 17).ok_or("Datum")?;
        let versand = Zeitpunkt::frankfurt(datum, 7, 40, 0).ok_or("Versand")?;
        let frist = Boerse::Xetra.schluss(datum).ok_or("Schluss")?;
        for (nr, isin, richtung, stueck, limit, nach) in [
            (1, sap, Richtung::Verkauf, 10, 9_900, Vec::new()),
            (2, alv, Richtung::Kauf, stueck, 10_100, vec![1]),
        ] {
            konto.orders.insert(
                nr,
                PapierOrder {
                    auftrag: Order {
                        isin,
                        richtung,
                        stueck,
                        limit: Cent(limit),
                    },
                    status: KontoStatus::Offen,
                    ausfuehrungen: Vec::new(),
                    versand: Some((versand, frist)),
                    nach,
                },
            );
        }
        Ok((konto, frist))
    }

    fn schluss(frist: Zeitpunkt, sap: i64, alv: i64) -> Result<Kurse, String> {
        let datum = frist.frankfurt_tag().ok_or("Datum")?;
        let kurs = |cent| {
            Kurs::neu(Cent(cent), "EUR", datum, frist, Herkunft::Quelle)
                .map_err(|e| format!("{e:?}"))
        };
        Kurse::neu(
            frist,
            vec![
                (Isin::neu(SAP).map_err(|e| format!("{e:?}"))?, kurs(sap)?),
                (Isin::neu(ALV).map_err(|e| format!("{e:?}"))?, kurs(alv)?),
            ],
        )
        .map_err(|e| format!("{e:?}"))
    }

    /// Deckung: 9 Allianz zu 101 € plus Gebühr passen nur mit dem Verkaufserlös zum Limit.
    #[test]
    fn tauschkauf_rechnet_verkaufserloes_zum_limit_ein() -> Result<(), String> {
        // Geprüft wird der neue Kauf vor seiner Annahme, also ohne ihn im Buch.
        let neu = |stueck| -> Result<_, String> {
            let (mut konto, _) = tausch(0, stueck)?;
            let kauf = konto.orders.remove(&2).ok_or("Kauf")?.auftrag;
            Ok((konto, kauf))
        };
        let (konto, kauf) = neu(9)?;
        assert_eq!(konto.gedeckt(&kauf, &[1]), Some(true));
        assert_eq!(konto.gedeckt(&kauf, &[]), Some(false));
        // 10 Stück übersteigen den schlechtesten Erlös von 990 € minus Gebühr.
        let (konto, kauf) = neu(10)?;
        assert_eq!(konto.gedeckt(&kauf, &[1]), Some(false));
        Ok(())
    }

    /// #311 H1b Gegenprobe 3: Füllt der Verkauf nicht, verfällt der Tauschkauf, und das Bargeld
    /// wird nie negativ. Geprüft über ein Raster von Schlusskursen und Startbargeld.
    #[test]
    fn tauschkauf_nur_nach_seinem_verkauf_bargeld_nie_negativ() -> Result<(), String> {
        for bargeld in [0, 5_000, 50_000] {
            for sap in (9_700..=10_300).step_by(50) {
                for alv in (9_800..=10_300).step_by(50) {
                    let (mut konto, frist) = tausch(bargeld, 9)?;
                    // Der Kauf steht vorne: Das Konto ordnet die Verkäufe trotzdem zuerst.
                    konto.markt_fuer(&schluss(frist, sap, alv)?, &BTreeSet::from([2, 1]));
                    let status = |nr| konto.orders.get(&nr).map(|o| o.status);
                    assert!(konto.bargeld >= Cent::NULL, "{bargeld} {sap} {alv}");
                    if status(2) == Some(KontoStatus::Gefuellt) {
                        assert_eq!(status(1), Some(KontoStatus::Gefuellt));
                    }
                    if status(1) != Some(KontoStatus::Gefuellt) {
                        assert_eq!(status(2), Some(KontoStatus::Verfallen), "{sap} {alv}");
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn tausch_ueberdauert_die_datei() -> Result<(), String> {
        let (konto, _) = tausch(0, 9)?;
        let text = konto.als_text();
        assert!(text.contains("\ntausch;2;1\n"), "{text}");
        assert_eq!(Papierkonto::aus_text(&text)?, konto);
        assert!(Papierkonto::aus_text(&text.replace("tausch;2;1", "tausch;1;2")).is_err());
        Ok(())
    }
}
