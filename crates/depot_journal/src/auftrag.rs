//! Zustaende eines Auftrags (R37, Kap. 10). Jeder Schritt ist ein eigener Eintrag im Journal;
//! der Zustand nach einem Neustart folgt allein aus diesen Eintraegen.
//!
//! **Feste Zustandsmaschine:**
//!
//! | Zustand | weiter nach |
//! | --- | --- |
//! | reserviert (der Eintrag `auftrag`) | gesendet, verworfen |
//! | gesendet: Versand im Journal begonnen | angenommen, abgelehnt, unbekannt |
//! | unbekannt: keine Antwort | angenommen, abgelehnt; nur ueber den Abgleich, nie neu gesendet |
//! | angenommen, teilgefuellt | Ausfuehrung, storniert, verfallen |
//! | gefuellt, abgelehnt, storniert, verfallen, verworfen | Ende |
//!
//! `gesendet` gibt es nur nach `reserviert`: Ein zweiter Versand desselben Auftrags laesst sich
//! nicht belegen. Gefuellt ist ein Auftrag, wenn seine Ausfuehrungen genau die beauftragte Menge
//! ergeben; mehr nimmt das Journal nicht an. Eine Ausfuehrung haelt das Limit ein.

use depot_kern::Cent;
use depot_regelwerk::Richtung;

use crate::stand::AuftragsBeleg;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuftragsZustand {
    Reserviert,
    Gesendet,
    /// Sendestatus unbekannt: Klaerung nur ueber den Abgleich, nie durch neuen Versand.
    Unbekannt,
    Angenommen,
    Teilgefuellt,
    Gefuellt,
    Abgelehnt,
    Storniert,
    Verfallen,
    /// Reserviert und nie gesendet; die Reservierung ist frei.
    Verworfen,
}

impl AuftragsZustand {
    /// Noch kein Endzustand: Die Reservierung gilt.
    pub fn offen(self) -> bool {
        matches!(
            self,
            AuftragsZustand::Reserviert
                | AuftragsZustand::Gesendet
                | AuftragsZustand::Unbekannt
                | AuftragsZustand::Angenommen
                | AuftragsZustand::Teilgefuellt
        )
    }

    /// Der Versand ist begonnen und der Auftrag hat keinen Endzustand: Er kann beim Broker
    /// liegen.
    pub fn beim_broker(self) -> bool {
        self.offen() && self != AuftragsZustand::Reserviert
    }
}

/// Eine Ausfuehrung laut Broker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ausfuehrung {
    pub stueck: u64,
    pub kurs: Cent,
    pub gebuehr: Cent,
}

/// Was zu einem Auftrag gemeldet wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Meldung {
    Gesendet,
    Unbekannt,
    Angenommen,
    Abgelehnt,
    Ausfuehrung(Ausfuehrung),
    Storniert,
    Verfallen,
    Verworfen,
}

impl Meldung {
    fn name(self) -> &'static str {
        match self {
            Meldung::Gesendet => "gesendet",
            Meldung::Unbekannt => "unbekannt",
            Meldung::Angenommen => "angenommen",
            Meldung::Abgelehnt => "abgelehnt",
            Meldung::Ausfuehrung(_) => "ausfuehrung",
            Meldung::Storniert => "storniert",
            Meldung::Verfallen => "verfallen",
            Meldung::Verworfen => "verworfen",
        }
    }
}

/// Ein Auftrag mit seinem Zustand und seinen Ausfuehrungen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuftragsLage {
    pub beleg: AuftragsBeleg,
    pub zustand: AuftragsZustand,
    pub ausfuehrungen: Vec<Ausfuehrung>,
}

impl AuftragsLage {
    pub(crate) fn reserviert(beleg: AuftragsBeleg) -> AuftragsLage {
        AuftragsLage {
            beleg,
            zustand: AuftragsZustand::Reserviert,
            ausfuehrungen: Vec::new(),
        }
    }

    /// Ausgefuehrte Stueck. Nie mehr als beauftragt; das prueft jede Meldung.
    pub fn gefuellt(&self) -> u64 {
        self.ausfuehrungen
            .iter()
            .fold(0_u64, |s, a| s.saturating_add(a.stueck))
    }

    /// Stueck, fuer die die Reservierung noch gilt: der offene Rest, im Endzustand null.
    pub fn offen_stueck(&self) -> u64 {
        if self.zustand.offen() {
            self.beleg.stueck.saturating_sub(self.gefuellt())
        } else {
            0
        }
    }

    /// Die Lage nach der Meldung, oder warum die Meldung hier unzulaessig ist.
    pub(crate) fn nach(&self, m: &Meldung) -> Result<AuftragsLage, String> {
        use AuftragsZustand as Z;
        let nr = self.beleg.nr;
        let mut neu = self.clone();
        neu.zustand = match (self.zustand, m) {
            (Z::Reserviert, Meldung::Gesendet) => Z::Gesendet,
            (Z::Reserviert, Meldung::Verworfen) => Z::Verworfen,
            (Z::Gesendet, Meldung::Unbekannt) => Z::Unbekannt,
            (Z::Gesendet | Z::Unbekannt, Meldung::Angenommen) => Z::Angenommen,
            (Z::Gesendet | Z::Unbekannt, Meldung::Abgelehnt) => Z::Abgelehnt,
            (Z::Angenommen | Z::Teilgefuellt, Meldung::Storniert) => Z::Storniert,
            (Z::Angenommen | Z::Teilgefuellt, Meldung::Verfallen) => Z::Verfallen,
            (Z::Angenommen | Z::Teilgefuellt, Meldung::Ausfuehrung(a)) => {
                self.pruefe(a)?;
                neu.ausfuehrungen.push(*a);
                if neu.gefuellt() == self.beleg.stueck {
                    Z::Gefuellt
                } else {
                    Z::Teilgefuellt
                }
            }
            (z, m) => {
                return Err(format!(
                    "Auftrag {nr}: `{}` nach {z:?} unzulaessig",
                    m.name()
                ));
            }
        };
        Ok(neu)
    }

    fn pruefe(&self, a: &Ausfuehrung) -> Result<(), String> {
        let b = &self.beleg;
        let nr = b.nr;
        if a.stueck == 0 || a.kurs.0 <= 0 || a.gebuehr.0 < 0 {
            return Err(format!("Auftrag {nr}: Ausfuehrung unplausibel"));
        }
        let gesamt = self
            .gefuellt()
            .checked_add(a.stueck)
            .ok_or("Ausfuehrung: Ueberlauf")?;
        if gesamt > b.stueck {
            return Err(format!(
                "Auftrag {nr}: {gesamt} Stueck ausgefuehrt, {} beauftragt",
                b.stueck
            ));
        }
        let im_limit = match b.richtung {
            Richtung::Kauf => a.kurs <= b.limit,
            Richtung::Verkauf => a.kurs >= b.limit,
        };
        if !im_limit {
            return Err(format!(
                "Auftrag {nr}: Kurs {} ausserhalb des Limits {}",
                a.kurs.0, b.limit.0
            ));
        }
        Ok(())
    }
}

pub(crate) fn meldung_text(nr: u64, m: &Meldung) -> String {
    match m {
        Meldung::Ausfuehrung(a) => format!(
            "{nr};{};{};{};{}",
            m.name(),
            a.stueck,
            a.kurs.0,
            a.gebuehr.0
        ),
        _ => format!("{nr};{}", m.name()),
    }
}

pub(crate) fn lies_meldung(inhalt: &str) -> Option<(u64, Meldung)> {
    let teile: Vec<&str> = inhalt.split(';').collect();
    let (nr, m) = match teile.as_slice() {
        [nr, name] => {
            let m = [
                Meldung::Gesendet,
                Meldung::Unbekannt,
                Meldung::Angenommen,
                Meldung::Abgelehnt,
                Meldung::Storniert,
                Meldung::Verfallen,
                Meldung::Verworfen,
            ]
            .into_iter()
            .find(|m| m.name() == *name)?;
            (nr, m)
        }
        [nr, "ausfuehrung", stueck, kurs, gebuehr] => (
            nr,
            Meldung::Ausfuehrung(Ausfuehrung {
                stueck: stueck.parse().ok()?,
                kurs: Cent(kurs.parse().ok()?),
                gebuehr: Cent(gebuehr.parse().ok()?),
            }),
        ),
        _ => return None,
    };
    Some((nr.parse().ok()?, m))
}
