//! Die festgeschriebenen Grenzen. Ihr Fingerabdruck steht in jedem Beleg und in der Echtgeld-Freigabe.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use crate::geld::Cent;
use crate::instrument::{Instrument, InstrumentArt, Isin};

/// IBKR-Tarif "Tiered" fuer deutsche Aktien plus pauschale Boersengebuehr (Annahme).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gebuehren {
    pub prozent_bp: i64,
    pub minimum: Cent,
    pub boersengebuehr: Cent,
}

impl Gebuehren {
    /// Gebuehr fuer ein Ordervolumen, aufgerundet.
    pub fn fuer(&self, volumen: Cent) -> Option<Cent> {
        let prozent = volumen.abs().anteil_bp_auf(self.prozent_bp)?;
        let provision = if prozent > self.minimum {
            prozent
        } else {
            self.minimum
        };
        provision.checked_add(self.boersengebuehr)
    }
}

/// Alle harten Grenzen. Werte in Basispunkten (bp), 100 bp = 1 %.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grenzen {
    /// Hoechstanteil eines Werts am Topf.
    pub max_anteil_bp: i64,
    /// Hoechstens so viele Werte mit Zielgewicht > 0.
    pub max_werte: usize,
    /// Hoechstens so viele Orders je Topf und Zyklus.
    pub max_orders_je_topf: usize,
    /// Kleinste erlaubte Order (Ausnahme: kompletter Verkauf einer Position).
    pub min_order: Cent,
    /// Umschichten erst ab dieser Aenderung des Gewichts.
    pub min_aenderung_bp: i64,
    /// Hoechstens dieser Anteil des Topfes wird je Zyklus umgeschichtet.
    pub max_umschlag_bp: i64,
    /// Limit hoechstens so weit vom letzten Schlusskurs.
    pub limit_abstand_bp: i64,
    /// Kurs springt staerker als das zum Vortag: Daten gelten als unplausibel.
    pub max_kurssprung_bp: i64,
    /// Tagesverlust des Gesamtdepots, ab dem angehalten wird.
    pub tagesverlust_bp: i64,
    /// Rueckgang vom Hoechststand, ab dem die Notbremse alles verkauft.
    pub notbremse_bp: i64,
    pub etf_freigegeben: bool,
    pub gebuehren: Gebuehren,
}

impl Grenzen {
    /// Die Startwerte aus dem Architekturentwurf v1.
    pub fn standard() -> Grenzen {
        Grenzen {
            max_anteil_bp: 2_500,
            max_werte: 8,
            max_orders_je_topf: 4,
            min_order: Cent::euro(500),
            min_aenderung_bp: 500,
            max_umschlag_bp: 5_000,
            limit_abstand_bp: 100,
            max_kurssprung_bp: 1_500,
            tagesverlust_bp: 500,
            notbremse_bp: 2_000,
            etf_freigegeben: false,
            gebuehren: Gebuehren {
                prozent_bp: 5,
                minimum: Cent(125),
                boersengebuehr: Cent(25),
            },
        }
    }

    /// SHA-256 ueber eine feste Textform aller Werte. Jede Aenderung ergibt einen anderen Abdruck.
    pub fn fingerabdruck(&self) -> String {
        let text = format!("depotschlopf-grenzen-v1|{self:?}");
        let hash = Sha256::digest(text.as_bytes());
        hash.iter().fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
    }
}

/// Die Positivliste: nur diese Werte duerfen gehandelt werden.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Universum {
    instrumente: Vec<Instrument>,
}

impl Universum {
    pub fn neu(instrumente: Vec<Instrument>) -> Universum {
        Universum { instrumente }
    }

    /// Handelbar = in der Liste und von erlaubter Art.
    pub fn handelbar(&self, isin: &Isin, grenzen: &Grenzen) -> bool {
        self.instrumente.iter().any(|i| {
            &i.isin == isin
                && match i.art {
                    InstrumentArt::Aktie => true,
                    InstrumentArt::EtfPhysischUcits => grenzen.etf_freigegeben,
                }
        })
    }

    pub fn isins(&self) -> BTreeSet<Isin> {
        self.instrumente.iter().map(|i| i.isin.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mindestgebuehr_greift() {
        let g = Grenzen::standard().gebuehren;
        assert_eq!(g.fuer(Cent::euro(500)), Some(Cent(150)));
        assert_eq!(g.fuer(Cent::euro(10_000)), Some(Cent(525)));
    }

    #[test]
    fn fingerabdruck_aendert_sich() {
        let a = Grenzen::standard();
        let mut b = Grenzen::standard();
        b.notbremse_bp = 2_100;
        assert_eq!(a.fingerabdruck().len(), 64);
        assert_ne!(a.fingerabdruck(), b.fingerabdruck());
        assert_eq!(a.fingerabdruck(), Grenzen::standard().fingerabdruck());
    }
}
