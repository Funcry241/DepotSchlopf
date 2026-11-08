//! Der Kontowaechter (Kap. 9): Deckung vor jeder Reservierung und Abgleich des Journals mit
//! dem Konto. Reiner Rust-Code ohne KI.
//!
//! **Deckung:** Ein Auftrag wird nur reserviert, wenn der Topf ihn traegt: ein Kauf mit
//! Limit mal Menge plus hoechstmoeglicher Gebuehr aus dem Bargeld, das nicht schon offene
//! Kaeufe samt Gebuehr binden; ein Verkauf aus Stueck, die nicht schon offene Verkaeufe binden.
//! Die Reservierung ist der Eintrag im Journal; so reserviert eine einzige Stelle, nacheinander
//! (R37).
//!
//! **Abgleich:** Zuerst holt der Kontowaechter zu jedem offenen Auftrag den Stand beim Konto
//! und belegt jede neue Meldung im Journal: Annahme, Ablehnung, Ausfuehrungen, Storno, Verfall.
//! Ein nie gesendeter Auftrag wird verworfen. Was sich nicht belegen laesst, ist eine
//! Abweichung. Dann vergleicht er Bargeld, Bestand je Wert und offene Orders des Kontos mit der
//! Summe der Toepfe. Jede Abweichung heisst Halt (R4, R12). Einen Fehlbetrag gleicht kein
//! Automatismus aus, weder durch Verkauf noch durch Nachschuss.

use std::collections::{BTreeMap, BTreeSet};

use depot_kern::{
    Boerse, Cent, Datum, Grenzen, Isin, TopfId, Zeitpunkt, liste::papierorder_schluss,
};
use depot_regelwerk::{GenehmigterAuftrag, Richtung};

use crate::auftrag::{AuftragsZustand, Ausfuehrung, Meldung};
use crate::datei::{Journal, JournalFehler};
use crate::stand::Stand;

/// Wie das Konto einen Auftrag fuehrt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KontoStatus {
    /// Angenommen und noch offen, vielleicht teilweise ausgefuehrt.
    Offen,
    Gefuellt,
    Abgelehnt,
    Storniert,
    Verfallen,
}

/// Ein Auftrag laut Konto, gefunden ueber seine Kennung.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KontoOrder {
    pub status: KontoStatus,
    pub ausfuehrungen: Vec<Ausfuehrung>,
    pub versand: Option<(Zeitpunkt, Zeitpunkt)>,
}

/// Valuta und Nettoerloes je Verkaufsorder im EUR-Papiermodell.
pub type Abrechnungen = BTreeMap<u64, (Datum, Cent)>;

/// Einheitliche Regel des EUR-Papiermodells, auch fuer NYSE-Werte.
pub fn papier_valuta(mut tag: Datum) -> Option<Datum> {
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

/// Was das Konto meldet. Betraege in Euro-Cent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Kontoauszug {
    /// Saldo in Euro; kann laut Konto auch negativ sein.
    pub bargeld: Cent,
    pub bestand: BTreeMap<Isin, u64>,
    /// Kennungen der offenen Orders.
    pub offene: BTreeSet<u64>,
    /// Waehrungen ausser Euro mit einem Saldo ungleich null.
    pub fremde_waehrungen: BTreeSet<String>,
    /// Belege seit Aktivierung der Papierabrechnung, auch wenn sie schon faellig wurden.
    /// `None` bedeutet, dass das Konto keinen Nachweis liefert.
    pub abrechnungen: Option<Abrechnungen>,
}

/// Das Konto, wie der Kontowaechter es liest. Lesen darf jeder; senden nur die Auftragsstelle.
pub trait Konto {
    fn auszug(&self) -> Kontoauszug;
    /// Der Auftrag mit dieser Kennung; `None`, wenn das Konto ihn nicht kennt.
    fn order(&self, nr: u64) -> Option<KontoOrder>;
}

/// Eine Abweichung zwischen Journal und Konto. Jede heisst Halt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Abweichung {
    SaldoNegativ(Cent),
    Fremdwaehrung(String),
    Bargeld {
        journal: Cent,
        konto: Cent,
    },
    Bestand {
        isin: Isin,
        journal: u64,
        konto: u64,
    },
    /// Gesendet laut Journal, offen, aber das Konto kennt den Auftrag nicht.
    OrderFehlt(u64),
    /// Das Konto fuehrt eine offene Order, die im Journal nicht offen beim Broker liegt.
    OrderNurBeimKonto(u64),
    /// Das Journal fuehrt den Auftrag offen beim Broker, der Auszug nicht (K16-01).
    OrderNichtImAuszug(u64),
    /// Das Konto kennt einen Auftrag, dessen Versand das Journal nie begonnen hat.
    OrderOhneVersand(u64),
    /// Was das Konto meldet, laesst sich im Journal nicht belegen.
    Unbuchbar {
        nr: u64,
        grund: String,
    },
    AbrechnungsnachweisFehlt,
    Abrechnung {
        nr: u64,
        journal: Option<(Datum, Cent)>,
        konto: Option<(Datum, Cent)>,
    },
    Rechnung,
}

/// Bargeld eines Topfes, das noch fuer einen Kauf frei ist: Bargeld minus aller offenen Kaeufe
/// am Limit samt Gebuehr und minus noch nicht abgerechneter Erloese.
pub fn verfuegbar(grenzen: &Grenzen, stand: &Stand, topf: TopfId) -> Option<Cent> {
    let mut frei = stand
        .toepfe()
        .get(&topf)?
        .bargeld()
        .checked_sub(stand.gesperrt(topf)?)?;
    for l in stand.offene_auftraege() {
        if l.beleg.topf == topf && l.beleg.richtung == Richtung::Kauf {
            frei = frei.checked_sub(kaufbedarf(grenzen, l.beleg.limit, l.offen_stueck())?)?;
        }
    }
    Some(frei)
}

/// Limit mal Menge plus hoechstmoegliche Gebuehr.
fn kaufbedarf(grenzen: &Grenzen, limit: Cent, stueck: u64) -> Option<Cent> {
    if stueck == 0 {
        return Some(Cent::NULL);
    }
    let volumen = limit.mal_stueck(stueck)?;
    volumen.checked_add(grenzen.gebuehren().fuer(volumen)?)
}

/// Prueft die Deckung eines neuen Auftrags (R37). Die Stueck eines Verkaufs prueft das Journal
/// beim Anwenden fuer jeden Eintrag, auch beim Oeffnen.
pub(crate) fn pruefe_deckung(
    grenzen: &Grenzen,
    stand: &Stand,
    topf: TopfId,
    auftrag: &GenehmigterAuftrag,
) -> Result<(), String> {
    if auftrag.beleg().grenzen_abdruck != grenzen.fingerabdruck() {
        return Err("Auftrag unter anderen Grenzen genehmigt".into());
    }
    let a = auftrag.auftrag();
    if a.richtung == Richtung::Kauf {
        let frei = verfuegbar(grenzen, stand, topf).ok_or("Deckung nicht berechenbar")?;
        let bedarf = kaufbedarf(grenzen, a.limit, a.stueck).ok_or("Deckung: Überlauf")?;
        if bedarf > frei {
            return Err(format!(
                "keine Deckung: Kauf braucht {} Cent mit Gebühr, frei sind {} Cent",
                bedarf.0, frei.0
            ));
        }
    }
    Ok(())
}

/// Vergleicht die Summe der Toepfe und die offenen Auftraege mit dem Auszug.
pub fn abweichungen(stand: &Stand, auszug: &Kontoauszug) -> Vec<Abweichung> {
    let mut aus = Vec::new();
    if auszug.bargeld.0 < 0 {
        aus.push(Abweichung::SaldoNegativ(auszug.bargeld));
    }
    for w in &auszug.fremde_waehrungen {
        aus.push(Abweichung::Fremdwaehrung(w.clone()));
    }
    let mut bargeld = Some(Cent::NULL);
    let mut bestand: BTreeMap<&Isin, u64> = BTreeMap::new();
    let mut rechnung = false;
    for t in stand.toepfe().values() {
        bargeld = bargeld.and_then(|b| b.checked_add(t.bargeld()));
        for (isin, n) in t.positionen() {
            let s = bestand.entry(isin).or_insert(0);
            match s.checked_add(*n) {
                Some(neu) => *s = neu,
                None => rechnung = true,
            }
        }
    }
    match bargeld {
        Some(b) if b != auszug.bargeld => aus.push(Abweichung::Bargeld {
            journal: b,
            konto: auszug.bargeld,
        }),
        Some(_) => {}
        None => rechnung = true,
    }
    let alle: BTreeSet<&Isin> = bestand
        .keys()
        .copied()
        .chain(auszug.bestand.keys())
        .collect();
    for isin in alle {
        let journal = bestand.get(isin).copied().unwrap_or(0);
        let konto = auszug.bestand.get(isin).copied().unwrap_or(0);
        if journal != konto {
            aus.push(Abweichung::Bestand {
                isin: isin.clone(),
                journal,
                konto,
            });
        }
    }
    let beim_broker: BTreeSet<u64> = stand
        .offene_auftraege()
        .filter(|l| l.zustand.beim_broker())
        .map(|l| l.beleg.nr)
        .collect();
    // In beide Richtungen (K16-01): Was das Journal offen beim Broker fuehrt, muss der Auszug
    // offen fuehren, und umgekehrt.
    for nr in auszug.offene.difference(&beim_broker) {
        aus.push(Abweichung::OrderNurBeimKonto(*nr));
    }
    for nr in beim_broker.difference(&auszug.offene) {
        aus.push(Abweichung::OrderNichtImAuszug(*nr));
    }
    match &auszug.abrechnungen {
        Some(konto) => {
            let journal = stand.abrechnungen();
            let nummern: BTreeSet<_> = journal.keys().chain(konto.keys()).copied().collect();
            for nr in nummern {
                let j = journal.get(&nr).copied();
                let k = konto.get(&nr).copied();
                if j != k {
                    aus.push(Abweichung::Abrechnung {
                        nr,
                        journal: j,
                        konto: k,
                    });
                }
            }
        }
        None if stand.abrechnungstag().is_some() => aus.push(Abweichung::AbrechnungsnachweisFehlt),
        None => {}
    }
    if rechnung {
        aus.push(Abweichung::Rechnung);
    }
    aus
}

impl Journal {
    /// Reserviert einen genehmigten Auftrag: Der Kontowaechter prueft die Deckung, dann steht
    /// der Auftrag im Journal, bevor er den Rechner verlaesst (R12, R37). Nur so entsteht ein
    /// Auftrag im Journal.
    ///
    /// # Errors
    /// `Abgelehnt` ohne Deckung, unter anderen Grenzen oder mit einer Kennung, die nicht
    /// groesser als die letzte ist; nichts wird geschrieben. Sonst wie `anhaengen`.
    pub fn reserviere_auftrag(
        &mut self,
        grenzen: &Grenzen,
        topf: TopfId,
        auftrag: &GenehmigterAuftrag,
    ) -> Result<(), JournalFehler> {
        if self.stand().format() != crate::stand::FORMAT {
            return Err(JournalFehler::Abgelehnt(
                "Journal im Format 1: vor dem ersten neuen Auftrag stelle_um".into(),
            ));
        }
        pruefe_deckung(grenzen, self.stand(), topf, auftrag).map_err(JournalFehler::Abgelehnt)?;
        self.schreibe_auftrag(topf, auftrag)
    }

    /// Belegt den naechsten Zustand eines Auftrags.
    ///
    /// # Errors
    /// `Abgelehnt`, wenn der Schritt nach der Zustandsmaschine unzulaessig ist oder die
    /// Ausfuehrung Limit, Menge oder Deckung verletzt. Sonst wie `anhaengen`.
    pub fn melde(&mut self, nr: u64, meldung: &Meldung) -> Result<(), JournalFehler> {
        if let Meldung::GesendetAm(zeit, frist) = meldung
            && self
                .stand()
                .auftrag(nr)
                .and_then(|l| papierorder_schluss(&l.beleg.isin, *zeit))
                != Some(*frist)
        {
            return Err(JournalFehler::Abgelehnt(
                "Versand: ungültige Börsenfrist".into(),
            ));
        }
        self.schreibe_meldung(nr, meldung)
    }

    /// Der Abgleich mit dem Konto. Belegt jede neue Meldung zu offenen Auftraegen und liefert
    /// alle Abweichungen. Leer heisst: Journal und Konto stimmen ueberein.
    ///
    /// # Errors
    /// Nur Fehler des Journals (`Halt`, `Belegt`); jede Unstimmigkeit ist eine `Abweichung`.
    pub fn gleiche_ab(&mut self, konto: &dyn Konto) -> Result<Vec<Abweichung>, JournalFehler> {
        let mut aus = Vec::new();
        let offene: Vec<u64> = self
            .stand()
            .offene_auftraege()
            .map(|l| l.beleg.nr)
            .collect();
        for nr in offene {
            if let Some(a) = self.gleiche_auftrag_ab(nr, konto.order(nr))? {
                aus.push(a);
            }
        }
        aus.extend(abweichungen(self.stand(), &konto.auszug()));
        Ok(aus)
    }

    fn gleiche_auftrag_ab(
        &mut self,
        nr: u64,
        order: Option<KontoOrder>,
    ) -> Result<Option<Abweichung>, JournalFehler> {
        let Some(lage) = self.stand().auftrag(nr).cloned() else {
            return Ok(None);
        };
        let Some(order) = order else {
            return Ok(match lage.zustand {
                // Der Versand wird belegt, bevor gesendet wird: Ohne diesen Eintrag ist der
                // Auftrag nie gesendet worden, und die Reservierung wird frei.
                AuftragsZustand::Reserviert => self.belege_meldung(nr, &Meldung::Verworfen)?,
                _ => Some(Abweichung::OrderFehlt(nr)),
            });
        };
        if lage.versand.zip(lage.frist) != order.versand
            && lage.zustand != AuftragsZustand::Reserviert
        {
            return Ok(Some(Abweichung::Unbuchbar {
                nr,
                grund: "Versandzeit oder Börsenfrist weicht vom Journal ab".into(),
            }));
        }
        let mut folge = Vec::new();
        match lage.zustand {
            AuftragsZustand::Reserviert => return Ok(Some(Abweichung::OrderOhneVersand(nr))),
            AuftragsZustand::Gesendet | AuftragsZustand::Unbekannt => {
                if order.status == KontoStatus::Abgelehnt && order.ausfuehrungen.is_empty() {
                    return self.belege_meldung(nr, &Meldung::Abgelehnt);
                }
                folge.push(Meldung::Angenommen);
            }
            _ => {}
        }
        let neue = order
            .ausfuehrungen
            .strip_prefix(lage.ausfuehrungen.as_slice())
            .ok_or_else(|| Abweichung::Unbuchbar {
                nr,
                grund: "Ausführungen weichen vom Journal ab".into(),
            });
        let neue = match neue {
            Ok(n) => n,
            Err(a) => return Ok(Some(a)),
        };
        folge.extend(neue.iter().copied().map(Meldung::Ausfuehrung));
        match order.status {
            KontoStatus::Offen | KontoStatus::Gefuellt => {}
            KontoStatus::Abgelehnt => folge.push(Meldung::Abgelehnt),
            KontoStatus::Storniert => folge.push(Meldung::Storniert),
            KontoStatus::Verfallen => folge.push(Meldung::Verfallen),
        }
        for m in &folge {
            if let Some(a) = self.belege_meldung(nr, m)? {
                return Ok(Some(a));
            }
        }
        let zustand = self.stand().auftrag(nr).map(|l| l.zustand);
        if order.status == KontoStatus::Gefuellt && zustand != Some(AuftragsZustand::Gefuellt) {
            return Ok(Some(Abweichung::Unbuchbar {
                nr,
                grund: format!("laut Konto gefuellt, laut Journal {zustand:?}"),
            }));
        }
        Ok(None)
    }

    /// Belegt eine Meldung aus dem Abgleich. Laesst das Journal sie nicht zu, ist das eine
    /// Abweichung, kein Fehler des Journals.
    fn belege_meldung(
        &mut self,
        nr: u64,
        m: &Meldung,
    ) -> Result<Option<Abweichung>, JournalFehler> {
        match self.schreibe_meldung(nr, m) {
            Ok(()) => Ok(None),
            Err(JournalFehler::Abgelehnt(grund)) => Ok(Some(Abweichung::Unbuchbar { nr, grund })),
            Err(e) => Err(e),
        }
    }
}
