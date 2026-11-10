//! Die festgeschriebenen Grenzen. Ihr Fingerabdruck steht in jedem Beleg und in der Echtgeld-Freigabe.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use crate::geld::Cent;
use crate::instrument::{Instrument, InstrumentArt, Isin};

/// Erzeugt eine Struktur mit privaten Feldern und je Feld einem Leser. Einen Setzer gibt es
/// nicht, auch nicht hinter einem Feature: Cargo-Features kann jeder Aufrufer einschalten, auch
/// im Release (R19, 1·F01, K8-01). Gebaut wird nur ueber `Grenzen::standard()`.
macro_rules! versiegelt {
    (
        $(#[$kopf:meta])*
        $name:ident {
            $( $(#[$doc:meta])* $feld:ident : $typ:ty ),* $(,)?
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
    };
}

versiegelt! {
    /// IBKR-Tarif "Tiered" fuer deutsche Aktien plus pauschale Boersengebuehr (Annahme).
    Gebuehren {
        prozent_bp: i64,
        minimum: Cent,
        boersengebuehr: Cent,
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
        max_anteil_bp: i64,
        /// Hoechstens so viele Werte mit Zielgewicht > 0.
        max_werte: usize,
        /// Hoechstens so viele Orders je Topf und Zyklus.
        max_orders_je_topf: usize,
        /// Kleinste erlaubte Order (Ausnahme: kompletter Verkauf einer Position).
        min_order: Cent,
        /// Umschichten erst ab dieser Aenderung des Gewichts.
        min_aenderung_bp: i64,
        /// Hoechstens dieser Anteil des Topfes wird je Zyklus umgeschichtet.
        max_umschlag_bp: i64,
        /// Limit hoechstens so weit vom letzten Schlusskurs.
        limit_abstand_bp: i64,
        /// Kurs springt staerker als das zum Vortag: Daten gelten als unplausibel.
        max_kurssprung_bp: i64,
        /// Tagesverlust des Gesamtdepots, ab dem angehalten wird.
        tagesverlust_bp: i64,
        /// Rueckgang vom Hoechststand, ab dem die Notbremse alles verkauft.
        notbremse_bp: i64,
        /// Gesamtverlust vom historischen Hoechststand, der die Papierfreigabe sperrt.
        papier_gesamtverlust_bp: i64,
        /// Ausdrueckliche Papierfreigabe beginnt eine neue Gesamtverlustmessung.
        papier_freigabe_neue_messung: bool,
        etf_freigegeben: bool,
        gebuehren: Gebuehren,
        /// Toleranz der Bestandsgrenze: Liegt ein Wert mehr als so viel ueber dem
        /// Hoechstanteil, sind Zukaeufe gesperrt, und er wird abgebaut (R22).
        bestand_toleranz_bp: i64,
        /// Hoechstalter eines Kurses fuer Entscheidungen, Auftraege und Notbremse, in Sekunden.
        kurs_hoechstalter_s: i64,
    }
}

impl Grenzen {
    /// Die Werte fuer den Papierhandel ab 05.11.2026: wenige, konzentrierte Positionen
    /// (hoechstens 4 Werte, je Wert hoechstens 40 %), Umschlag bis 100 % am Tag, ETFs frei.
    /// Alle anderen Werte aus dem Architekturentwurf v1. Ihr Fingerabdruck steht in der
    /// Festschreibung; jede Aenderung braucht die Grenzfreigabe des Eigentuemers (R30).
    pub fn standard() -> Grenzen {
        Grenzen {
            max_anteil_bp: 4_000,
            max_werte: 4,
            max_orders_je_topf: 4,
            min_order: Cent(10_000),
            min_aenderung_bp: 500,
            max_umschlag_bp: 10_000,
            limit_abstand_bp: 100,
            max_kurssprung_bp: 1_500,
            tagesverlust_bp: 500,
            notbremse_bp: 2_000,
            papier_gesamtverlust_bp: 3_000,
            papier_freigabe_neue_messung: true,
            etf_freigegeben: true,
            gebuehren: Gebuehren {
                prozent_bp: 5,
                minimum: Cent(125),
                boersengebuehr: Cent(25),
            },
            bestand_toleranz_bp: 200,
            kurs_hoechstalter_s: 1_200,
        }
    }

    /// Ob der belegte Depotwert die Papier-Gesamtverlustgrenze erreicht oder unterschreitet.
    pub fn papier_gesamtverlust_erreicht(&self, wert: Cent, hoch: Cent) -> bool {
        let rest_bp = 10_000_i128.saturating_sub(i128::from(self.papier_gesamtverlust_bp));
        hoch > Cent::NULL
            && i128::from(wert.0).saturating_mul(10_000)
                <= i128::from(hoch.0).saturating_mul(rest_bp)
    }

    /// SHA-256 ueber eine feste Textform aller Werte. Jede Aenderung ergibt einen anderen Abdruck.
    pub fn fingerabdruck(&self) -> String {
        let text = format!(
            "depotschlopf-grenzen-v3|morgen-alter-seit-abruf-bis-erster-handelsschluss|{self:?}"
        );
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

    /// Alle Instrumente der Liste, in ihrer Reihenfolge.
    pub fn instrumente(&self) -> &[Instrument] {
        &self.instrumente
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
        assert_eq!(g.fuer(Cent(10_000)), Some(Cent(150)));
        assert_eq!(g.fuer(Cent(1_000_000)), Some(Cent(525)));
        assert_eq!(g.fuer(Cent(i64::MIN)), None);
    }

    #[test]
    fn fingerabdruck_aendert_sich() {
        let a = Grenzen::standard();
        let mut b = Grenzen::standard();
        b.notbremse_bp = 2_100;
        assert_eq!(a.min_order(), Cent(10_000));
        assert_eq!(a.fingerabdruck().len(), 64);
        assert_ne!(a.fingerabdruck(), b.fingerabdruck());
        assert_eq!(a.fingerabdruck(), Grenzen::standard().fingerabdruck());
    }
}
