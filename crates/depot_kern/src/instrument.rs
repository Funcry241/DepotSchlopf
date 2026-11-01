//! Wertpapiere. Nur was hier als Typ existiert, kann ueberhaupt gehandelt werden.

use std::fmt;

/// Eine geprueft gueltige ISIN (12 Zeichen, Laendercode, Pruefziffer nach Luhn).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Isin(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IsinFehler {
    Laenge,
    Zeichen,
    Pruefziffer,
}

impl Isin {
    pub fn neu(text: &str) -> Result<Isin, IsinFehler> {
        let t = text.trim().to_ascii_uppercase();
        if t.len() != 12 {
            return Err(IsinFehler::Laenge);
        }
        let zeichen: Vec<char> = t.chars().collect();
        let (land, rest) = zeichen.split_at(2);
        if !land.iter().all(char::is_ascii_uppercase)
            || !rest.iter().all(char::is_ascii_alphanumeric)
            || !rest.last().is_some_and(char::is_ascii_digit)
        {
            return Err(IsinFehler::Zeichen);
        }
        if !luhn_gueltig(&t) {
            return Err(IsinFehler::Pruefziffer);
        }
        Ok(Isin(t))
    }

    pub fn als_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Isin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Buchstaben werden zu Zahlen (A=10 … Z=35), dann Luhn ueber alle Ziffern.
fn luhn_gueltig(isin: &str) -> bool {
    let mut ziffern: Vec<u32> = Vec::with_capacity(24);
    for c in isin.chars() {
        match c.to_digit(36) {
            Some(v) if v >= 10 => {
                ziffern.push(v / 10);
                ziffern.push(v % 10);
            }
            Some(v) => ziffern.push(v),
            None => return false,
        }
    }
    let mut summe: u32 = 0;
    for (i, d) in ziffern.iter().rev().enumerate() {
        let mut w = *d;
        if i % 2 == 1 {
            w = w.saturating_mul(2);
            if w > 9 {
                w = w.saturating_sub(9);
            }
        }
        summe = summe.saturating_add(w);
    }
    summe.is_multiple_of(10)
}

/// Erlaubte Instrumentarten. Derivate, Zertifikate, Hebel- und Short-Produkte gibt es nicht.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InstrumentArt {
    Aktie,
    /// Vorbereitet, aber nur handelbar, wenn die Konfiguration ETFs freigibt.
    EtfPhysischUcits,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instrument {
    pub isin: Isin,
    pub name: String,
    pub art: InstrumentArt,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echte_isins() {
        for s in [
            "DE0007164600",
            "DE0008404005",
            "DE000BASF111",
            "IE00B4L5Y983",
        ] {
            assert!(Isin::neu(s).is_ok(), "{s}");
        }
    }

    #[test]
    fn falsche_isins() {
        assert_eq!(Isin::neu("DE0007164601"), Err(IsinFehler::Pruefziffer));
        assert_eq!(Isin::neu("DE000716460"), Err(IsinFehler::Laenge));
        assert_eq!(Isin::neu("120007164600"), Err(IsinFehler::Zeichen));
    }
}
