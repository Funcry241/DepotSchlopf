//! Kurse. Ein Kurs ist nur als geprueftes Objekt zu haben: positiv, in Euro, mit Handelstag,
//! Zeitpunkt und Herkunft (R20, 1·R06). Kurse eines Abrufs kennen ihren Stand und damit das
//! Alter jedes Kurses.

use std::collections::BTreeMap;

use crate::geld::Cent;
use crate::instrument::Isin;
use crate::zeit::{Datum, Zeitpunkt};

/// Die Waehrung eines Kurses. Gehandelt wird nur in Euro (Positivliste: Xetra).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Waehrung {
    Eur,
}

/// Woher ein Kurs stammt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Herkunft {
    /// Vom Broker abgerufen.
    Broker,
    /// Aus einer Aufzeichnung wieder eingespielt.
    Aufzeichnung,
    /// In einer Simulation oder einem Test erfunden.
    Simulation,
    /// Lesend von einer freigegebenen Kursquelle abgerufen (Kap. 6); welcher, steht im Beleg.
    Quelle,
}

impl Herkunft {
    /// Der Name im Journal.
    pub fn name(self) -> &'static str {
        match self {
            Herkunft::Broker => "broker",
            Herkunft::Aufzeichnung => "aufzeichnung",
            Herkunft::Simulation => "simulation",
            Herkunft::Quelle => "quelle",
        }
    }

    pub fn aus_name(name: &str) -> Option<Herkunft> {
        [
            Herkunft::Broker,
            Herkunft::Aufzeichnung,
            Herkunft::Simulation,
            Herkunft::Quelle,
        ]
        .into_iter()
        .find(|h| h.name() == name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KursFehler {
    /// Preis null oder negativ.
    NichtPositiv,
    /// Waehrung fehlt oder ist nicht `EUR`.
    Waehrung,
    /// Der Zeitpunkt des Kurses liegt nicht an seinem Handelstag.
    Handelstag,
    /// Der Kurs ist juenger als der Stand der Kurse.
    Zukunft(Isin),
    /// Fuer denselben Wert liegen zwei Kurse vor.
    Doppelt(Isin),
}

/// Ein geprueft gueltiger Kurs je Stueck.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kurs {
    preis: Cent,
    waehrung: Waehrung,
    handelstag: Datum,
    zeitpunkt: Zeitpunkt,
    herkunft: Herkunft,
}

impl Kurs {
    /// Die Waehrung kommt als Code, wie die Quelle sie liefert; nur genau `EUR` gilt.
    ///
    /// # Errors
    /// Preis nicht positiv, Waehrung nicht `EUR`, Zeitpunkt nicht am Handelstag (K15-03): Ein
    /// Kurs liegt an seinem Handelstag, nie davor und nie danach.
    pub fn neu(
        preis: Cent,
        waehrung: &str,
        handelstag: Datum,
        zeitpunkt: Zeitpunkt,
        herkunft: Herkunft,
    ) -> Result<Kurs, KursFehler> {
        if preis.0 <= 0 {
            return Err(KursFehler::NichtPositiv);
        }
        if waehrung != "EUR" {
            return Err(KursFehler::Waehrung);
        }
        if !zeitpunkt.am_tag(handelstag) {
            return Err(KursFehler::Handelstag);
        }
        Ok(Kurs {
            preis,
            waehrung: Waehrung::Eur,
            handelstag,
            zeitpunkt,
            herkunft,
        })
    }

    pub fn preis(&self) -> Cent {
        self.preis
    }

    pub fn waehrung(&self) -> Waehrung {
        self.waehrung
    }

    pub fn handelstag(&self) -> Datum {
        self.handelstag
    }

    pub fn zeitpunkt(&self) -> Zeitpunkt {
        self.zeitpunkt
    }

    pub fn herkunft(&self) -> Herkunft {
        self.herkunft
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Bestand {
    stand: Zeitpunkt,
    kurse: BTreeMap<Isin, Kurs>,
}

/// Die Kurse eines Abrufs. Nur ueber `Kurse::neu` zu bauen und danach unveraenderlich.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kurse(Bestand);

impl Kurse {
    /// `stand` ist der Zeitpunkt des Abrufs; kein Kurs darf juenger sein.
    ///
    /// # Errors
    /// Ein Kurs liegt nach dem Stand, oder ein Wert kommt doppelt vor.
    pub fn neu(stand: Zeitpunkt, eintraege: Vec<(Isin, Kurs)>) -> Result<Kurse, KursFehler> {
        let mut kurse = BTreeMap::new();
        for (isin, kurs) in eintraege {
            if kurs.zeitpunkt > stand {
                return Err(KursFehler::Zukunft(isin));
            }
            if kurse.contains_key(&isin) {
                return Err(KursFehler::Doppelt(isin));
            }
            kurse.insert(isin, kurs);
        }
        Ok(Kurse(Bestand { stand, kurse }))
    }

    pub fn stand(&self) -> Zeitpunkt {
        self.0.stand
    }

    /// Preis je Stueck; `None`, wenn kein Kurs vorliegt.
    pub fn kurs(&self, isin: &Isin) -> Option<Cent> {
        self.0.kurse.get(isin).map(Kurs::preis)
    }

    pub fn eintrag(&self, isin: &Isin) -> Option<&Kurs> {
        self.0.kurse.get(isin)
    }

    /// Alle Kurse, nach ISIN geordnet.
    pub fn alle(&self) -> impl Iterator<Item = (&Isin, &Kurs)> {
        self.0.kurse.iter()
    }

    /// Alter des Kurses in Sekunden, gemessen am Stand.
    pub fn alter_s(&self, isin: &Isin) -> Option<i64> {
        self.0
            .kurse
            .get(isin)
            .and_then(|k| self.0.stand.sekunden_seit(k.zeitpunkt))
    }
}
