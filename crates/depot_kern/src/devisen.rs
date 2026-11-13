//! Umrechnung von Dollar in Euro ueber die Referenzkurse der EZB (Abruf je Boerse, Schritt 2).
//!
//! Ein Kurs gilt nur in Euro (`Kurs::exakt`). Ein Wert der NYSE notiert in Dollar; er bekommt
//! einen Kurs erst, wenn sein Schluss mit dem Referenzkurs der EZB **desselben Tages** in Euro
//! umgerechnet ist. Die EZB veroeffentlicht ihre Referenzkurse an Geschaeftstagen gegen 16:00
//! Uhr in Frankfurt, also vor dem Schluss der NYSE. Fehlt der Kurs des Tages, gibt es keinen
//! Euro-Kurs; ein aelterer Tag ersetzt ihn nie (an den Feiertagen des Zahlungssystems TARGET,
//! an denen die NYSE handelt, etwa am Ostermontag, 1. Mai und 26. Dezember, faellt ein Wert der NYSE damit
//! aus und wird als ausgelassen gefuehrt). Für den Kursvergleich wird der jüngste
//! frühere Schluss mit eigenem Referenzkurs verwendet.
//!
//! Gelesen wird der Text, wie die EZB ihn liefert (`eurofxref-hist-90d.xml`): je Tag ein
//! `<Cube time="JJJJ-MM-TT">` mit einem `<Cube currency="USD" rate="1.1224"/>` darin. Netz kennt
//! der Kern nicht; der Aufrufer reicht den Text herein.
//!
//! **Aufbau:** Geprueft wird der ganze Text, bevor ein Kurs gilt: genau eine Huelle
//! `gesmes:Envelope`, jedes Element geschlossen, und zwar in der Reihenfolge, in der es geoeffnet
//! wurde; nach der Huelle nichts mehr. Darin genau ein `Cube` ohne Attribute, darin nur Tage
//! (`Cube` mit genau `time`), darin nur Waehrungen (`Cube` mit genau `currency` und `rate`, ohne
//! Inhalt). Attribute werden vollstaendig gelesen; ein Name zweimal, gleich in welcher
//! Schreibweise, oder ein Rest, der kein Attribut ist, verwirft den Text. Ein abgeschnittener
//! Text oder ein Dollarkurs ausserhalb seines Tagesblocks verwirft den ganzen Text; ein Kurs
//! wird nie einem anderen Tag zugeschlagen.
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
    /// Ein Dollarkurs steht ausserhalb eines Tagesblocks.
    OhneTag,
    /// Der Text hat nicht den Aufbau der EZB-Datei: abgeschnitten, ein Element nicht oder falsch
    /// geschlossen, ein `Cube` an falscher Stelle oder mit anderen Attributen.
    Aufbau,
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

/// Ein Element, wie es im Tag steht: Name und alle Attribute in ihrer Reihenfolge.
struct Element<'a> {
    name: &'a str,
    attribute: Vec<(&'a str, &'a str)>,
}

impl<'a> Element<'a> {
    /// Liest den Inhalt eines Tags vollstaendig: Name, dann jedes Attribut als
    /// `name="wert"` oder `name='wert'`, mit Leerraum davor und erlaubtem Leerraum um `=`.
    /// Ein Rest, der kein Attribut ist, oder ein Name zweimal (ohne Ruecksicht auf Gross- und
    /// Kleinschreibung) ist ein Fehler im Aufbau. Werte mit `<` oder `&` werden nicht gelesen.
    fn lies(tag: &'a str) -> Result<Element<'a>, DevisenFehler> {
        let leer = |c: char| matches!(c, ' ' | '\t' | '\r' | '\n');
        let namenszeichen =
            |c: char| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-' | '.');
        let name_von = |t: &'a str| -> Result<(&'a str, &'a str), DevisenFehler> {
            let ende = t.find(|c: char| !namenszeichen(c)).unwrap_or(t.len());
            let (name, rest) = t.split_at_checked(ende).ok_or(DevisenFehler::Aufbau)?;
            if name.is_empty() {
                return Err(DevisenFehler::Aufbau);
            }
            Ok((name, rest))
        };
        let (name, mut rest) = name_von(tag)?;
        let mut attribute: Vec<(&'a str, &'a str)> = Vec::new();
        loop {
            let ohne = rest.trim_start_matches(leer);
            if ohne.is_empty() {
                break;
            }
            if ohne.len() == rest.len() {
                return Err(DevisenFehler::Aufbau);
            }
            let (a, nach_name) = name_von(ohne)?;
            let nach_gleich = nach_name
                .trim_start_matches(leer)
                .strip_prefix('=')
                .ok_or(DevisenFehler::Aufbau)?
                .trim_start_matches(leer);
            let zeichen = nach_gleich.chars().next().ok_or(DevisenFehler::Aufbau)?;
            if !matches!(zeichen, '"' | '\'') {
                return Err(DevisenFehler::Aufbau);
            }
            let (wert, danach) = nach_gleich
                .get(1..)
                .and_then(|w| w.split_once(zeichen))
                .ok_or(DevisenFehler::Aufbau)?;
            if wert.contains(['<', '&']) || attribute.iter().any(|(n, _)| n.eq_ignore_ascii_case(a))
            {
                return Err(DevisenFehler::Aufbau);
            }
            attribute.push((a, wert));
            rest = danach;
        }
        Ok(Element { name, attribute })
    }

    /// Der Wert eines Attributs.
    fn wert(&self, name: &str) -> Option<&'a str> {
        self.attribute
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, w)| *w)
    }

    /// Hat das Element genau diese Attribute, keines mehr und keines weniger?
    fn hat_genau(&self, namen: &[&str]) -> bool {
        self.attribute.len() == namen.len() && namen.iter().all(|n| self.wert(n).is_some())
    }
}

/// Ein offenes Element und, fuer `Cube`, seine Stelle im Aufbau.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ebene<'a> {
    Huelle,
    Wuerfel,
    Tag(Datum),
    Sonst(&'a str),
}

impl Ebene<'_> {
    fn name(self) -> &'static str {
        match self {
            Ebene::Huelle => "gesmes:Envelope",
            Ebene::Wuerfel | Ebene::Tag(_) => "Cube",
            Ebene::Sonst(_) => "",
        }
    }
}

/// Der Stand beim Lesen: offene Elemente, gesehene Tage, gelesene Kurse.
#[derive(Default)]
struct Leser<'a> {
    offen: Vec<Ebene<'a>>,
    huelle_gesehen: bool,
    wuerfel_gesehen: bool,
    tage: std::collections::BTreeSet<Datum>,
    usd_je_eur: BTreeMap<Datum, Preis>,
}

impl<'a> Leser<'a> {
    fn oben(&self) -> Option<Ebene<'a>> {
        self.offen.last().copied()
    }

    /// Ein schliessendes Element: Es muss das zuletzt geoeffnete sein.
    fn schliesse(&mut self, name: &str) -> Result<(), DevisenFehler> {
        let oben = self.offen.pop().ok_or(DevisenFehler::Aufbau)?;
        let erwartet = match oben {
            Ebene::Sonst(n) => n,
            e => e.name(),
        };
        if name == erwartet {
            Ok(())
        } else {
            Err(DevisenFehler::Aufbau)
        }
    }

    /// Ein `Cube`: Huelle, Tag oder Waehrung, je nach Attributen, nur an seiner Stelle und nur
    /// mit genau seinen Attributen.
    fn wuerfel(&mut self, e: &Element<'a>, leer: bool) -> Result<(), DevisenFehler> {
        let oben = self.oben();
        if let Some(waehrung) = e.wert("currency") {
            let Some(Ebene::Tag(datum)) = oben else {
                return Err(if waehrung == "USD" {
                    DevisenFehler::OhneTag
                } else {
                    DevisenFehler::Aufbau
                });
            };
            if !e.hat_genau(&["currency", "rate"]) || !leer {
                return Err(DevisenFehler::Aufbau);
            }
            if waehrung == "USD" {
                let kurs = e
                    .wert("rate")
                    .and_then(Preis::lies_euro)
                    .filter(|p| p.fein() > 0)
                    .ok_or(DevisenFehler::Kurs(datum))?;
                if self.usd_je_eur.insert(datum, kurs).is_some() {
                    return Err(DevisenFehler::Doppelt(datum));
                }
            }
            return Ok(());
        }
        if let Some(zeit) = e.wert("time") {
            if oben != Some(Ebene::Wuerfel) || !e.hat_genau(&["time"]) || leer {
                return Err(DevisenFehler::Aufbau);
            }
            let datum = Datum::lies(zeit).ok_or_else(|| DevisenFehler::Datum(zeit.into()))?;
            if !self.tage.insert(datum) {
                return Err(DevisenFehler::Doppelt(datum));
            }
            self.offen.push(Ebene::Tag(datum));
            return Ok(());
        }
        if !e.hat_genau(&[]) || oben != Some(Ebene::Huelle) || self.wuerfel_gesehen {
            return Err(DevisenFehler::Aufbau);
        }
        self.wuerfel_gesehen = true;
        if !leer {
            self.offen.push(Ebene::Wuerfel);
        }
        Ok(())
    }

    /// Ein oeffnendes oder leeres Element ausser `Cube`.
    fn element(&mut self, e: &Element<'a>, leer: bool) -> Result<(), DevisenFehler> {
        if e.name == "gesmes:Envelope" {
            if !self.offen.is_empty() || self.huelle_gesehen || leer {
                return Err(DevisenFehler::Aufbau);
            }
            self.huelle_gesehen = true;
            self.offen.push(Ebene::Huelle);
            return Ok(());
        }
        // Andere Elemente nur in der Huelle und nie zwischen Tagen und Waehrungen.
        if !matches!(self.oben(), Some(Ebene::Huelle | Ebene::Sonst(_))) {
            return Err(DevisenFehler::Aufbau);
        }
        if !leer {
            self.offen.push(Ebene::Sonst(e.name));
        }
        Ok(())
    }
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
        if text.trim().is_empty() {
            return Err(DevisenFehler::Leer);
        }
        let mut stand = Leser::default();
        let mut stuecke = text.split('<');
        // Vor dem ersten Element nur Leerraum.
        if !stuecke.next().unwrap_or("").trim().is_empty() {
            return Err(DevisenFehler::Aufbau);
        }
        for (i, stueck) in stuecke.enumerate() {
            let (tag, danach) = stueck.split_once('>').ok_or(DevisenFehler::Aufbau)?;
            let huelle_zu = stand.huelle_gesehen && stand.offen.is_empty();
            if huelle_zu || (danach.contains('>')) {
                return Err(DevisenFehler::Aufbau);
            }
            if let Some(kopf) = tag.strip_prefix('?') {
                if i != 0 || !kopf.starts_with("xml ") || !kopf.ends_with('?') {
                    return Err(DevisenFehler::Aufbau);
                }
            } else if let Some(name) = tag.strip_prefix('/') {
                stand.schliesse(name)?;
            } else {
                let (inhalt, leer) = tag.strip_suffix('/').map_or((tag, false), |t| (t, true));
                let e = Element::lies(inhalt)?;
                if e.name == "Cube" {
                    stand.wuerfel(&e, leer)?;
                } else {
                    stand.element(&e, leer)?;
                }
            }
            // Text zwischen Elementen nur in der Huelle, nie in Tagen und Waehrungen.
            if !danach.trim().is_empty() && !matches!(stand.oben(), Some(Ebene::Sonst(_))) {
                return Err(DevisenFehler::Aufbau);
            }
        }
        if !stand.huelle_gesehen || !stand.offen.is_empty() || !stand.wuerfel_gesehen {
            return Err(DevisenFehler::Aufbau);
        }
        if stand.usd_je_eur.is_empty() {
            return Err(DevisenFehler::Leer);
        }
        Ok(Devisen {
            usd_je_eur: stand.usd_je_eur,
        })
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
