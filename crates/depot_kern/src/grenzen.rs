//! Die festgeschriebenen Grenzen. Ihr Fingerabdruck steht in jedem Beleg und in der Echtgeld-Freigabe.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use crate::geld::Cent;
use crate::instrument::{Instrument, InstrumentArt, Isin};

/// Erzeugt eine Struktur mit privaten Feldern, je Feld einen Leser und, nur mit dem Feature
/// `pruefstand`, je Feld einen Setzer. Ausserhalb von Tests laesst sich damit kein Wert bauen
/// oder lockern (R19, 1·F01); das Feature schaltet allein eine Entwicklungsabhaengigkeit ein,
/// und jede Abhaengigkeit steht in der Festschreibung.
macro_rules! versiegelt {
    (
        $(#[$kopf:meta])*
        $name:ident {
            $( $(#[$doc:meta])* $feld:ident : $typ:ty => $setzer:ident ),* $(,)?
        }
    ) => {
        $(#[$kopf])*
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct $name {
            $( $(#[$doc])* $feld: $typ, )*
        }

        impl $name {
            $( $(#[$doc])* pub fn $feld(&self) -> $typ { self.$feld.clone() } )*
        }

        #[cfg(feature = "pruefstand")]
        impl $name {
            $(
                /// Nur im Pruefstand: setzt den Wert.
                #[must_use]
                pub fn $setzer(mut self, wert: $typ) -> $name {
                    self.$feld = wert;
                    self
                }
            )*
        }
    };
}

versiegelt! {
    /// IBKR-Tarif "Tiered" fuer deutsche Aktien plus pauschale Boersengebuehr (Annahme).
    Gebuehren {
        prozent_bp: i64 => mit_prozent_bp,
        minimum: Cent => mit_minimum,
        boersengebuehr: Cent => mit_boersengebuehr,
    }
}

impl Gebuehren {
    /// Gebuehr fuer ein Ordervolumen, aufgerundet.
    pub fn fuer(&self, volumen: Cent) -> Option<Cent> {
        let prozent = volumen.abs()?.anteil_bp_auf(self.prozent_bp)?;
        let provision = if prozent > self.minimum {
            prozent
        } else {
            self.minimum
        };
        provision.checked_add(self.boersengebuehr)
    }
}

versiegelt! {
    /// Alle harten Grenzen. Werte in Basispunkten (bp), 100 bp = 1 %.
    Grenzen {
        /// Hoechstanteil eines Werts am Topf.
        max_anteil_bp: i64 => mit_max_anteil_bp,
        /// Hoechstens so viele Werte mit Zielgewicht > 0.
        max_werte: usize => mit_max_werte,
        /// Hoechstens so viele Orders je Topf und Zyklus.
        max_orders_je_topf: usize => mit_max_orders_je_topf,
        /// Kleinste erlaubte Order (Ausnahme: kompletter Verkauf einer Position).
        min_order: Cent => mit_min_order,
        /// Umschichten erst ab dieser Aenderung des Gewichts.
        min_aenderung_bp: i64 => mit_min_aenderung_bp,
        /// Hoechstens dieser Anteil des Topfes wird je Zyklus umgeschichtet.
        max_umschlag_bp: i64 => mit_max_umschlag_bp,
        /// Limit hoechstens so weit vom letzten Schlusskurs.
        limit_abstand_bp: i64 => mit_limit_abstand_bp,
        /// Kurs springt staerker als das zum Vortag: Daten gelten als unplausibel.
        max_kurssprung_bp: i64 => mit_max_kurssprung_bp,
        /// Tagesverlust des Gesamtdepots, ab dem angehalten wird.
        tagesverlust_bp: i64 => mit_tagesverlust_bp,
        /// Rueckgang vom Hoechststand, ab dem die Notbremse alles verkauft.
        notbremse_bp: i64 => mit_notbremse_bp,
        etf_freigegeben: bool => mit_etf_freigegeben,
        gebuehren: Gebuehren => mit_gebuehren,
    }
}

impl Grenzen {
    /// Die Startwerte aus dem Architekturentwurf v1. Ihr Fingerabdruck steht in der
    /// Festschreibung; jede Aenderung braucht die Grenzfreigabe des Eigentuemers (R30).
    pub fn standard() -> Grenzen {
        Grenzen {
            max_anteil_bp: 2_500,
            max_werte: 8,
            max_orders_je_topf: 4,
            min_order: Cent(50_000),
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
        assert_eq!(g.fuer(Cent(50_000)), Some(Cent(150)));
        assert_eq!(g.fuer(Cent(1_000_000)), Some(Cent(525)));
        assert_eq!(g.fuer(Cent(i64::MIN)), None);
    }

    #[test]
    fn fingerabdruck_aendert_sich() {
        let a = Grenzen::standard();
        let mut b = Grenzen::standard();
        b.notbremse_bp = 2_100;
        assert_eq!(a.min_order(), Cent(50_000));
        assert_eq!(a.fingerabdruck().len(), 64);
        assert_ne!(a.fingerabdruck(), b.fingerabdruck());
        assert_eq!(a.fingerabdruck(), Grenzen::standard().fingerabdruck());
    }
}
