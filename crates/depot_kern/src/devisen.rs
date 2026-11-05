//! Umrechnung von Dollar in Euro ueber die Referenzkurse der EZB (Abruf je Boerse, Schritt 2).
//!
//! Ein Kurs gilt nur in Euro (`Kurs::exakt`). Ein Wert der NYSE notiert in Dollar; er bekommt
//! einen Kurs erst, wenn sein Schluss mit dem Referenzkurs der EZB **desselben Tages** in Euro
//! umgerechnet ist. Die EZB veroeffentlicht ihre Referenzkurse an Geschaeftstagen gegen 16:00
//! Uhr in Frankfurt, also vor dem Schluss der NYSE. Fehlt der Kurs des Tages, gibt es keinen
//! Euro-Kurs; ein aelterer Tag ersetzt ihn nie (an den Feiertagen des Zahlungssystems TARGET,
//! an denen die NYSE handelt, etwa am 1. Mai und 26. Dezember, faellt ein Wert der NYSE damit
//! aus und wird als ausgelassen gefuehrt).
//!
//! Gelesen wird der Text, wie die EZB ihn liefert (`eurofxref-hist-90d.xml`): je Tag ein
//! `<Cube time="JJJJ-MM-TT">` mit einem `<Cube currency="USD" rate="1.1224"/>` darin. Netz kennt
//! der Kern nicht; der Aufrufer reicht den Text herein.
//!
//! **Rundung:** Der Referenzkurs nennt Dollar je Euro. Der Euro-Preis ist Dollar-Preis durch
//! Referenzkurs, exakt gerechnet und auf sechs Nachkommastellen abgeschnitten (abgerundet). Der
//! Fehler liegt unter 0,000001 Euro je Stueck, weit unter dem ganzen Cent, zu dem das Papierkonto
//! ausfuehrt.

use std::collections::BTreeMap;

use crate::preis::{Preis, STELLEN_EURO};
use crate::zeit::Datum;

/// Warum ein Text keine Referenzkurse ergibt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DevisenFehler {
    /// Kein Tag mit einem Dollarkurs im Text.
    Leer,
    /// Ein `time` ist kein Datum der Form `JJJJ-MM-TT`.
    Datum(String),
    /// Ein Tag kommt zweimal vor, oder ein Tag nennt den Dollar zweimal.
    Doppelt(Datum),
    /// Ein Dollarkurs steht vor dem ersten Tag.
    OhneTag,
    /// Ein Dollarkurs ist kein positiver Betrag mit hoechstens sechs Nachkommastellen.
    Kurs(Datum),
}

/// Die Referenzkurse der EZB: Dollar je Euro an jedem Tag, den die EZB nennt.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Devisen {
    usd_je_eur: BTreeMap<Datum, Preis>,
}

/// Zehn hoch `STELLEN_EURO`: so viele feine Einheiten hat ein ganzer Betrag.
fn skala() -> i128 {
    let mut s: i128 = 1;
    for _ in 0..STELLEN_EURO {
        s = s.saturating_mul(10);
    }
    s
}

/// Der Wert eines Attributs in einem Tag (`name="wert"`), wenn es genau einmal vorkommt.
fn attribut<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let muster = format!(" {name}=\"");
    let mut teile = tag.split(muster.as_str());
    teile.next()?;
    let wert = teile.next()?;
    if teile.next().is_some() {
        return None;
    }
    wert.split_once('"').map(|(w, _)| w)
}

impl Devisen {
    /// Keine Referenzkurse: Jeder Wert in Dollar bleibt ohne Euro-Kurs.
    pub fn keine() -> Devisen {
        Devisen::default()
    }

    /// Liest die Referenzkurse der EZB. Andere Waehrungen werden nicht gelesen. Ein Tag ohne
    /// Dollarkurs fehlt, er wird nicht aus Nachbartagen ergaenzt.
    ///
    /// # Errors
    /// Kein Tag mit Dollarkurs, ein ungueltiges Datum, ein Tag doppelt, ein Dollarkurs ohne Tag
    /// oder ein ungueltiger Dollarkurs. Ein einzelner kaputter Eintrag verwirft den ganzen
    /// Text, statt still zu fehlen.
    pub fn lies_ezb(text: &str) -> Result<Devisen, DevisenFehler> {
        let mut usd_je_eur = BTreeMap::new();
        let mut tage = std::collections::BTreeSet::new();
        let mut aktuell: Option<Datum> = None;
        for stueck in text.split('<').skip(1) {
            let tag = stueck.split_once('>').map_or(stueck, |(t, _)| t);
            if !(tag.starts_with("Cube ") || tag.starts_with("Cube\t")) {
                continue;
            }
            if let Some(zeit) = attribut(tag, "time") {
                let datum = Datum::lies(zeit).ok_or_else(|| DevisenFehler::Datum(zeit.into()))?;
                if !tage.insert(datum) {
                    return Err(DevisenFehler::Doppelt(datum));
                }
                aktuell = Some(datum);
            }
            if attribut(tag, "currency") == Some("USD") {
                let datum = aktuell.ok_or(DevisenFehler::OhneTag)?;
                let kurs = attribut(tag, "rate")
                    .and_then(Preis::lies_euro)
                    .filter(|p| p.fein() > 0)
                    .ok_or(DevisenFehler::Kurs(datum))?;
                if usd_je_eur.insert(datum, kurs).is_some() {
                    return Err(DevisenFehler::Doppelt(datum));
                }
            }
        }
        if usd_je_eur.is_empty() {
            return Err(DevisenFehler::Leer);
        }
        Ok(Devisen { usd_je_eur })
    }

    /// Dollar je Euro an genau diesem Tag.
    pub fn usd_je_eur(&self, datum: Datum) -> Option<Preis> {
        self.usd_je_eur.get(&datum).copied()
    }

    /// Der juengste Tag mit Dollarkurs.
    pub fn letzter_tag(&self) -> Option<Datum> {
        self.usd_je_eur.keys().next_back().copied()
    }

    /// Ein Dollar-Preis dieses Tages in Euro, abgerundet auf sechs Nachkommastellen. `None`,
    /// wenn die EZB fuer den Tag keinen Kurs nennt oder das Ergebnis nicht positiv ist.
    pub fn in_euro(&self, usd: Preis, datum: Datum) -> Option<Preis> {
        let kurs = i128::from(self.usd_je_eur(datum)?.fein());
        let euro = i128::from(usd.fein())
            .checked_mul(skala())?
            .checked_div(kurs)?;
        i64::try_from(euro)
            .ok()
            .filter(|f| *f > 0)
            .map(Preis::aus_fein)
    }
}
