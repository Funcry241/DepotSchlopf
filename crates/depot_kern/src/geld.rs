//! Geldbetraege in ganzen Cent. Kein `f64` fuer Geld, alle Rechnungen pruefen auf Ueberlauf.

use std::fmt;

/// Ein Geldbetrag in Euro-Cent. Darf negativ sein (z. B. Differenzen), Kontostaende sind es nie.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cent(pub i64);

/// Basispunkte: 10.000 bp = 100 %.
pub const BP_GANZ: i64 = 10_000;

impl Cent {
    pub const NULL: Cent = Cent(0);

    pub const fn euro(euro: i64) -> Cent {
        Cent(euro.saturating_mul(100))
    }

    pub fn checked_add(self, other: Cent) -> Option<Cent> {
        self.0.checked_add(other.0).map(Cent)
    }

    pub fn checked_sub(self, other: Cent) -> Option<Cent> {
        self.0.checked_sub(other.0).map(Cent)
    }

    /// Betrag mal Stueckzahl.
    pub fn mal_stueck(self, stueck: u64) -> Option<Cent> {
        let s = i64::try_from(stueck).ok()?;
        self.0.checked_mul(s).map(Cent)
    }

    /// Anteil in Basispunkten, abgerundet (Richtung null).
    pub fn anteil_bp_ab(self, bp: i64) -> Option<Cent> {
        self.0.checked_mul(bp)?.checked_div(BP_GANZ).map(Cent)
    }

    /// Anteil in Basispunkten, aufgerundet (fuer Gebuehren und Reserven).
    pub fn anteil_bp_auf(self, bp: i64) -> Option<Cent> {
        let p = self.0.checked_mul(bp)?;
        let q = p.checked_div(BP_GANZ)?;
        let r = p.checked_rem(BP_GANZ)?;
        if r > 0 {
            q.checked_add(1).map(Cent)
        } else {
            Some(Cent(q))
        }
    }

    /// Wie viele Basispunkte ist `self` von `basis`? Abgerundet; `None` bei Basis <= 0.
    pub fn in_bp_von(self, basis: Cent) -> Option<i64> {
        if basis.0 <= 0 {
            return None;
        }
        self.0.checked_mul(BP_GANZ)?.checked_div(basis.0)
    }

    #[must_use]
    pub fn abs(self) -> Cent {
        Cent(self.0.saturating_abs())
    }
}

impl fmt::Display for Cent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let vorzeichen = if self.0 < 0 { "-" } else { "" };
        let betrag = self.0.unsigned_abs();
        let euro = betrag / 100;
        let cent = betrag % 100;
        let ziffern = euro.to_string();
        let mut gruppiert = String::new();
        for (i, z) in ziffern.chars().enumerate() {
            let rest = ziffern.len().saturating_sub(i);
            if i > 0 && rest % 3 == 0 {
                gruppiert.push('.');
            }
            gruppiert.push(z);
        }
        write!(f, "{vorzeichen}{gruppiert},{cent:02} €")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anzeige_deutsch() {
        assert_eq!(Cent(123_456_789).to_string(), "1.234.567,89 €");
        assert_eq!(Cent(-5).to_string(), "-0,05 €");
        assert_eq!(Cent::euro(10_000).to_string(), "10.000,00 €");
    }

    #[test]
    fn runden() {
        assert_eq!(Cent(1_001).anteil_bp_auf(5), Some(Cent(1)));
        assert_eq!(Cent(1_001).anteil_bp_ab(5), Some(Cent(0)));
        assert_eq!(Cent(250_000).anteil_bp_auf(5), Some(Cent(125)));
    }

    #[test]
    fn ueberlauf_wird_erkannt() {
        assert_eq!(Cent(i64::MAX).checked_add(Cent(1)), None);
        assert_eq!(Cent(i64::MAX).mal_stueck(2), None);
    }
}
