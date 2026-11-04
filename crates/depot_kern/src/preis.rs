//! Preise je Stueck, exakt bis auf ein Millionstel Euro (ein Zehntausendstel Cent).
//!
//! Ein Kurs kann feiner sein als ein Cent: Xetra notiert Werte unter 50 € oder in bestimmten
//! Preisklassen auf einen halben Cent, und Kursquellen liefern bis zu sechs Nachkommastellen.
//! Ein solcher Preis wird exakt gefuehrt und nie still gerundet (R4). Geldbetraege bleiben in
//! ganzen Cent (`Cent`); wo aus einem Preis ein Geldbetrag wird, entscheidet die Stelle, die das
//! tut, ausdruecklich (`Preis::als_cent` gilt nur fuer ganze Cent).

use std::fmt;

use crate::geld::{BP_GANZ, Cent};

/// Feinste Einheit je Cent: sechs Nachkommastellen in Euro.
pub const FEIN_JE_CENT: i64 = 10_000;
/// Nachkommastellen in Euro, die ein Preis hoechstens hat.
pub const STELLEN_EURO: usize = 6;
/// Nachkommastellen in Cent, die ein Preis hoechstens hat.
const STELLEN_CENT: usize = 4;

/// In welche Richtung ein Betrag auf ganze Cent gerundet wird. Die Stelle, die rundet, nennt sie
/// ausdruecklich: Bewertung und Kauflimit `Ab`, Verkaufslimit und hoechstmoegliche Belastung
/// `Auf` (Maus, 04.11.2026).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rundung {
    /// Zum naechsten ganzen Cent darunter (Richtung minus unendlich).
    Ab,
    /// Zum naechsten ganzen Cent darueber (Richtung plus unendlich).
    Auf,
}

/// Ein Preis in Zehntausendstel Cent. Darf als Differenz negativ sein; ein Kurs ist es nie.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Preis(i64);

/// Liest `ganz[.bruch]` aus Ziffern, mit hoechstens `stellen` Nachkommastellen ungleich null,
/// als ganze Zahl in der feinsten Einheit. Kein Vorzeichen, kein Exponent, kein Komma.
fn lies_dezimal(text: &str, stellen: usize) -> Option<i64> {
    let (ganz, bruch) = text.split_once('.').unwrap_or((text, ""));
    let ziffern = |t: &str| t.bytes().all(|b| b.is_ascii_digit());
    if ganz.is_empty()
        || !ziffern(ganz)
        || !ziffern(bruch)
        || (text.contains('.') && bruch.is_empty())
    {
        return None;
    }
    let (teil, rest) = bruch.split_at(bruch.len().min(stellen));
    if rest.bytes().any(|b| b != b'0') {
        return None;
    }
    let skala = 10_i64.checked_pow(u32::try_from(stellen).ok()?)?;
    let teil: i64 = if teil.is_empty() {
        0
    } else {
        format!("{teil:0<stellen$}").parse().ok()?
    };
    let ganz: i64 = ganz.parse().ok()?;
    ganz.checked_mul(skala)?.checked_add(teil)
}

impl Preis {
    pub const NULL: Preis = Preis(0);

    /// Ein Preis aus Zehntausendstel Cent.
    pub const fn aus_fein(fein: i64) -> Preis {
        Preis(fein)
    }

    /// Ein Preis aus ganzen Cent. `None` bei Ueberlauf.
    pub fn aus_cent(cent: Cent) -> Option<Preis> {
        cent.0.checked_mul(FEIN_JE_CENT).map(Preis)
    }

    /// Der Preis in Zehntausendstel Cent.
    pub const fn fein(self) -> i64 {
        self.0
    }

    /// Der Preis in ganzen Cent, nur wenn er genau auf einem Cent liegt. Nie gerundet.
    pub fn als_cent(self) -> Option<Cent> {
        (self.0.checked_rem(FEIN_JE_CENT)? == 0)
            .then(|| self.0.checked_div(FEIN_JE_CENT).map(Cent))
            .flatten()
    }

    /// Ein Betrag in Euro mit Punkt, wie ihn eine Kursquelle liefert (`49.925`, `49.9250`,
    /// `215`), mit hoechstens sechs Nachkommastellen ungleich null. Weitere Stellen gelten nur,
    /// wenn sie null sind. Kein Vorzeichen.
    pub fn lies_euro(text: &str) -> Option<Preis> {
        lies_dezimal(text, STELLEN_EURO).map(Preis)
    }

    /// Der Preis in Cent als Text, wie das Journal ihn fuehrt: ganze Cent ohne Punkt (`4148`),
    /// sonst mit den noetigen Nachkommastellen ohne Null am Ende (`4115.5`, `4992.0001`).
    pub fn cent_text(self) -> String {
        let vorzeichen = if self.0 < 0 { "-" } else { "" };
        let betrag = self.0.unsigned_abs();
        let fein_je_cent = FEIN_JE_CENT.unsigned_abs();
        let cent = betrag.checked_div(fein_je_cent).unwrap_or(0);
        let rest = betrag.checked_rem(fein_je_cent).unwrap_or(0);
        if rest == 0 {
            return format!("{vorzeichen}{cent}");
        }
        let bruch = format!("{rest:0STELLEN_CENT$}");
        format!("{vorzeichen}{cent}.{}", bruch.trim_end_matches('0'))
    }

    /// Liest genau die Form von `cent_text` fuer einen Preis ab null: keine fuehrende Null,
    /// keine Null am Ende der Nachkommastellen, hoechstens vier davon. Alles andere ist `None`.
    pub fn lies_cent_text(text: &str) -> Option<Preis> {
        let preis = lies_dezimal(text, STELLEN_CENT).map(Preis)?;
        (preis.cent_text() == text).then_some(preis)
    }

    pub fn checked_sub(self, other: Preis) -> Option<Preis> {
        self.0.checked_sub(other.0).map(Preis)
    }

    /// Betrag ohne Vorzeichen. `None` fuer den kleinsten Wert.
    pub fn abs(self) -> Option<Preis> {
        self.0.checked_abs().map(Preis)
    }

    /// `self × faktor / teiler` in ganzen Cent, in der genannten Richtung gerundet. Gerechnet
    /// wird exakt in `i128`; `None` bei Teiler <= 0 oder wenn das Ergebnis nicht in `Cent` passt.
    fn in_cent(self, faktor: i128, teiler: i128, richtung: Rundung) -> Option<Cent> {
        if teiler <= 0 {
            return None;
        }
        let zaehler = i128::from(self.0).checked_mul(faktor)?;
        let nenner = teiler.checked_mul(i128::from(FEIN_JE_CENT))?;
        let ab = zaehler.checked_div_euclid(nenner)?;
        let cent = match richtung {
            Rundung::Ab => ab,
            Rundung::Auf if zaehler.checked_rem_euclid(nenner)? == 0 => ab,
            Rundung::Auf => ab.checked_add(1)?,
        };
        i64::try_from(cent).ok().map(Cent)
    }

    /// Der Wert von `stueck` Stueck in ganzen Cent, in der genannten Richtung gerundet. Fuer einen
    /// Preis in ganzen Cent dasselbe wie `Cent::mal_stueck`.
    pub fn mal_stueck(self, stueck: u64, richtung: Rundung) -> Option<Cent> {
        self.in_cent(i128::from(stueck), 1, richtung)
    }

    /// `self × bp / 10.000` in ganzen Cent, in der genannten Richtung gerundet; etwa ein Limit
    /// mit `BP_GANZ + abstand` oder `BP_GANZ - abstand`.
    pub fn mal_bp(self, bp: i64, richtung: Rundung) -> Option<Cent> {
        self.in_cent(i128::from(bp), i128::from(BP_GANZ), richtung)
    }

    /// Wie viele ganze Stuecke kosten hoechstens `betrag`? Abgerundet; 0 bei nicht positivem
    /// Betrag oder Preis.
    pub fn stueck_fuer(self, betrag: Cent) -> Option<u64> {
        if self.0 <= 0 || betrag.0 <= 0 {
            return Some(0);
        }
        let zaehler = i128::from(betrag.0).checked_mul(i128::from(FEIN_JE_CENT))?;
        u64::try_from(zaehler.checked_div(i128::from(self.0))?).ok()
    }

    /// Wie viele Basispunkte ist `self` von `basis`? Abgerundet wie `Cent::in_bp_von`, fuer
    /// Preise in ganzen Cent mit demselben Ergebnis; `None` bei Basis <= 0.
    pub fn in_bp_von(self, basis: Preis) -> Option<i64> {
        if basis.0 <= 0 {
            return None;
        }
        let bp = i128::from(self.0)
            .checked_mul(i128::from(BP_GANZ))?
            .checked_div(i128::from(basis.0))?;
        i64::try_from(bp).ok()
    }

    /// Der Preis in Euro mit Komma, ohne Waehrung: mindestens zwei Nachkommastellen, weitere nur,
    /// wenn sie nicht null sind (`49,925`, `41,48`, `1.234,50`).
    pub fn euro_text(self) -> String {
        let vorzeichen = if self.0 < 0 { "-" } else { "" };
        let betrag = self.0.unsigned_abs();
        let je_euro = FEIN_JE_CENT.unsigned_abs().saturating_mul(100);
        let euro = betrag.checked_div(je_euro).unwrap_or(0);
        let rest = betrag.checked_rem(je_euro).unwrap_or(0);
        let bruch = format!("{rest:0STELLEN_EURO$}");
        let bruch = bruch.trim_end_matches('0');
        let bruch = format!("{bruch:0<2}");
        let ziffern = euro.to_string();
        let mut gruppiert = String::new();
        for (i, z) in ziffern.chars().enumerate() {
            let rest = ziffern.len().saturating_sub(i);
            if i > 0 && rest.checked_rem(3) == Some(0) {
                gruppiert.push('.');
            }
            gruppiert.push(z);
        }
        format!("{vorzeichen}{gruppiert},{bruch}")
    }
}

impl fmt::Display for Preis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} €", self.euro_text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(fein: i64) -> Preis {
        Preis::aus_fein(fein)
    }

    #[test]
    fn euro_lesen() {
        assert_eq!(Preis::lies_euro("49.9250"), Some(p(49_925_000)));
        assert_eq!(Preis::lies_euro("49.925"), Some(p(49_925_000)));
        assert_eq!(Preis::lies_euro("254.7105"), Some(p(254_710_500)));
        assert_eq!(Preis::lies_euro("1.000001"), Some(p(1_000_001)));
        assert_eq!(Preis::lies_euro("1.0000010"), Some(p(1_000_001)));
        assert_eq!(Preis::lies_euro("215"), Some(p(215_000_000)));
        assert_eq!(Preis::lies_euro("0"), Some(Preis::NULL));
        for falsch in [
            "1.0000001",
            "-1",
            "+1",
            "1e3",
            "",
            ".5",
            "5.",
            "1,5",
            " 1",
            "1.5.0",
            "N/D",
            "99999999999999999999",
        ] {
            assert_eq!(Preis::lies_euro(falsch), None, "{falsch}");
        }
    }

    #[test]
    fn cent_text_hin_und_zurueck() {
        for (fein, text) in [
            (49_925_000, "4992.5"),
            (41_480_000, "4148"),
            (1_000_001, "100.0001"),
            (10_000, "1"),
            (5_000, "0.5"),
            (1, "0.0001"),
        ] {
            assert_eq!(p(fein).cent_text(), text);
            assert_eq!(Preis::lies_cent_text(text), Some(p(fein)), "{text}");
        }
        for falsch in [
            "4992.50",
            "04992.5",
            "4992.",
            ".5",
            "4992,5",
            "4992.00001",
            "-5",
            "+5",
            "4992.0",
            "",
            "1e3",
        ] {
            assert_eq!(Preis::lies_cent_text(falsch), None, "{falsch}");
        }
    }

    #[test]
    fn nur_ganze_cent_sind_cent() {
        assert_eq!(p(41_480_000).als_cent(), Some(Cent(4_148)));
        assert_eq!(p(49_925_000).als_cent(), None);
        assert_eq!(Preis::aus_cent(Cent(4_148)), Some(p(41_480_000)));
        assert_eq!(Preis::aus_cent(Cent(i64::MAX)), None);
    }

    /// Exakt statt gerundet: 1,005 € gegen 1,00 € sind 50 bp. Auf Cent gerundet waeren es 0 oder
    /// 100 bp. Fuer ganze Cent gilt dasselbe Ergebnis wie mit `Cent::in_bp_von`.
    #[test]
    fn basispunkte_exakt() {
        let basis = p(1_000_000);
        assert_eq!(p(5_000).in_bp_von(basis), Some(50));
        for (d, b) in [(7, 4_115), (1, 3), (12_345, 99_999), (0, 1)] {
            let dp = Preis::aus_cent(Cent(d))
                .and_then(|x| Preis::aus_cent(Cent(b)).and_then(|y| x.in_bp_von(y)));
            assert_eq!(dp, Cent(d).in_bp_von(Cent(b)), "{d}/{b}");
        }
        assert_eq!(p(1).in_bp_von(Preis::NULL), None);
    }

    /// Gerichtet gerundet, exakt fuer ganze Cent.
    #[test]
    fn auf_ganze_cent() {
        use Rundung::{Ab, Auf};
        assert_eq!(p(49_925_000).mal_stueck(3, Ab), Some(Cent(14_977)));
        assert_eq!(p(49_925_000).mal_stueck(3, Auf), Some(Cent(14_978)));
        assert_eq!(p(49_925_000).mal_stueck(2, Ab), Some(Cent(9_985)));
        assert_eq!(p(49_925_000).mal_stueck(2, Auf), Some(Cent(9_985)));
        assert_eq!(p(49_925_000).mal_bp(10_100, Ab), Some(Cent(5_042)));
        assert_eq!(p(41_155_000).mal_bp(9_900, Auf), Some(Cent(4_075)));
        assert_eq!(p(-5_000).mal_stueck(1, Ab), Some(Cent(-1)));
        assert_eq!(p(-5_000).mal_stueck(1, Auf), Some(Cent(0)));
        for (c, s) in [(4_148, 7), (1, 1), (99_999, 3)] {
            let preis = Preis::aus_cent(Cent(c));
            for r in [Ab, Auf] {
                assert_eq!(
                    preis.and_then(|x| x.mal_stueck(s, r)),
                    Cent(c).mal_stueck(s)
                );
            }
        }
        assert_eq!(p(i64::MAX).mal_stueck(u64::MAX, Ab), None);
        assert_eq!(p(49_925_000).stueck_fuer(Cent(100_000)), Some(20));
        assert_eq!(p(50_000_000).stueck_fuer(Cent(100_000)), Some(20));
        assert_eq!(p(50_000_001).stueck_fuer(Cent(100_000)), Some(19));
        assert_eq!(p(1).stueck_fuer(Cent(0)), Some(0));
    }

    #[test]
    fn anzeige() {
        assert_eq!(p(49_925_000).to_string(), "49,925 €");
        assert_eq!(p(41_480_000).to_string(), "41,48 €");
        assert_eq!(p(1_234_500_000).to_string(), "1.234,50 €");
        assert_eq!(p(1).to_string(), "0,000001 €");
    }
}
