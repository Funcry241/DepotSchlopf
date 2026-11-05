//! Boersen: wann sie handeln und wann ihr Handel schliesst.
//!
//! Jeder Wert der Beobachtungsliste gehoert zu genau einer Boerse. Ein Schlusskurs gilt erst ab
//! dem Schluss seiner Boerse; ein Tag, an dem keine Boerse der Liste handelt, ist geschlossen und
//! nicht verpasst. Reine Kalenderrechnung ohne Netz; die Feiertage folgen den veroeffentlichten
//! Regeln der Boersen, nicht einer abgerufenen Liste.
//!
//! **Xetra** (Frankfurt): Handel bis 17:35 Uhr Ortszeit. Geschlossen an Neujahr, Karfreitag,
//! Ostermontag, 1. Mai, 24., 25., 26. und 31. Dezember.
//!
//! **NYSE** (New York): Handel bis 16:00 Uhr Ortszeit, an verkuerzten Tagen bis 13:00 Uhr (am
//! 3. Juli, am Tag nach Thanksgiving und am 24. Dezember, wenn dort gehandelt wird).
//! Geschlossen an New Year's Day, Martin Luther King Jr. Day, Washington's Birthday, Good
//! Friday, Memorial Day, Juneteenth (seit 2022), Independence Day, Labor Day, Thanksgiving und
//! Christmas Day. Faellt ein fester Feiertag auf einen Samstag, ist der Freitag davor frei, auf
//! einen Sonntag der Montag danach. Ausnahme: Faellt Neujahr auf einen Samstag, bleibt der
//! 31. Dezember ein Handelstag.

use std::fmt;

use crate::zeit::{Datum, Zeitpunkt};

/// Eine Boerse, an der ein Wert der Liste gehandelt wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Boerse {
    Xetra,
    Nyse,
}

/// Stunde und Minute des Xetra-Schlusses in Frankfurt.
pub const XETRA_SCHLUSS: (u8, u8) = (17, 35);
/// Stunde und Minute des NYSE-Schlusses in New York.
pub const NYSE_SCHLUSS: (u8, u8) = (16, 0);
/// Stunde und Minute des NYSE-Schlusses an verkuerzten Tagen.
pub const NYSE_SCHLUSS_KURZ: (u8, u8) = (13, 0);

impl Boerse {
    pub const ALLE: [Boerse; 2] = [Boerse::Xetra, Boerse::Nyse];

    pub fn name(self) -> &'static str {
        match self {
            Boerse::Xetra => "Xetra",
            Boerse::Nyse => "NYSE",
        }
    }

    /// Die Waehrung, in der die Boerse notiert. Ein Kurs gilt nur in Euro (`Kurs::exakt`):
    /// Ein Wert in Dollar bekommt erst mit einem geprueften Umrechnungskurs einen Kurs.
    pub fn waehrung(self) -> &'static str {
        match self {
            Boerse::Xetra => "EUR",
            Boerse::Nyse => "USD",
        }
    }

    /// Handelt die Boerse an diesem Tag? Montag bis Freitag ohne Feiertag.
    pub fn handelstag(self, datum: Datum) -> bool {
        datum.wochentag().is_some_and(|w| (1..=5).contains(&w)) && self.feiertag(datum).is_none()
    }

    /// Der Name des Feiertags, wenn die Boerse an einem Werktag deswegen geschlossen ist.
    pub fn feiertag(self, datum: Datum) -> Option<&'static str> {
        match self {
            Boerse::Xetra => xetra_feiertag(datum),
            Boerse::Nyse => nyse_feiertag(datum),
        }
    }

    /// Der Schluss des Handels an diesem Tag, auch an verkuerzten Tagen. Fuer einen Tag ohne
    /// Handel steht der uebliche Schluss: Ob gehandelt wurde, sagt `handelstag`, nicht die Uhr.
    /// `None` nur fuer eine Uhrzeit, die es nicht gibt.
    pub fn schluss(self, datum: Datum) -> Option<Zeitpunkt> {
        match self {
            Boerse::Xetra => {
                let (h, m) = XETRA_SCHLUSS;
                Zeitpunkt::frankfurt(datum, h, m, 0)
            }
            Boerse::Nyse => {
                let (h, m) = if nyse_verkuerzt(datum) {
                    NYSE_SCHLUSS_KURZ
                } else {
                    NYSE_SCHLUSS
                };
                Zeitpunkt::new_york(datum, h, m, 0)
            }
        }
    }
}

impl fmt::Display for Boerse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

fn ostern(datum: Datum, abstand: i64) -> bool {
    Datum::ostersonntag(datum.jahr())
        .and_then(|o| o.plus_tage(abstand))
        .is_some_and(|d| d == datum)
}

fn xetra_feiertag(d: Datum) -> Option<&'static str> {
    match (d.monat(), d.tag()) {
        (1, 1) => return Some("Neujahr"),
        (5, 1) => return Some("Tag der Arbeit"),
        (12, 24) => return Some("Heiligabend"),
        (12, 25) => return Some("1. Weihnachtstag"),
        (12, 26) => return Some("2. Weihnachtstag"),
        (12, 31) => return Some("Silvester"),
        _ => {}
    }
    if ostern(d, -2) {
        return Some("Karfreitag");
    }
    if ostern(d, 1) {
        return Some("Ostermontag");
    }
    None
}

/// Der freie Tag zu einem festen Feiertag: Samstag auf Freitag, Sonntag auf Montag.
fn beobachtet(feiertag: Option<Datum>) -> Option<Datum> {
    let f = feiertag?;
    match f.wochentag()? {
        6 => f.plus_tage(-1),
        7 => f.plus_tage(1),
        _ => Some(f),
    }
}

fn nyse_feiertag(d: Datum) -> Option<&'static str> {
    let j = d.jahr();
    let ist = |tag: Option<Datum>| tag.is_some_and(|t| t == d);
    // Neujahr: Sonntag auf Montag; ein Samstag wird nicht nachgeholt.
    let neujahr = Datum::neu(j, 1, 1).filter(|n| n.wochentag() != Some(6));
    if ist(beobachtet(neujahr)) {
        return Some("New Year's Day");
    }
    let fest: [(u8, u8, &'static str, u16); 3] = [
        (6, 19, "Juneteenth", 2022),
        (7, 4, "Independence Day", 0),
        (12, 25, "Christmas Day", 0),
    ];
    for (monat, tag, name, seit) in fest {
        if j >= seit && ist(beobachtet(Datum::neu(j, monat, tag))) {
            return Some(name);
        }
    }
    let beweglich: [(Option<Datum>, &'static str); 5] = [
        (
            Datum::nter_wochentag(j, 1, 1, 3),
            "Martin Luther King Jr. Day",
        ),
        (Datum::nter_wochentag(j, 2, 1, 3), "Washington's Birthday"),
        (Datum::letzter_wochentag(j, 5, 1), "Memorial Day"),
        (Datum::nter_wochentag(j, 9, 1, 1), "Labor Day"),
        (Datum::nter_wochentag(j, 11, 4, 4), "Thanksgiving Day"),
    ];
    for (tag, name) in beweglich {
        if ist(tag) {
            return Some(name);
        }
    }
    if ostern(d, -2) {
        return Some("Good Friday");
    }
    None
}

/// Schliesst die NYSE an diesem Tag schon um 13:00 Uhr?
fn nyse_verkuerzt(d: Datum) -> bool {
    let j = d.jahr();
    let tag_nach_thanksgiving = Datum::nter_wochentag(j, 11, 4, 4).and_then(|t| t.plus_tage(1));
    let kandidat = (d.monat(), d.tag()) == (7, 3)
        || (d.monat(), d.tag()) == (12, 24)
        || tag_nach_thanksgiving == Some(d);
    kandidat && Boerse::Nyse.handelstag(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(j: u16, m: u8, t: u8) -> Result<Datum, String> {
        Datum::neu(j, m, t).ok_or_else(|| format!("{j}-{m}-{t}"))
    }

    #[test]
    fn xetra_kalender_2026() -> Result<(), String> {
        let frei = [
            d(2026, 1, 1)?,
            d(2026, 4, 3)?,
            d(2026, 4, 6)?,
            d(2026, 5, 1)?,
            d(2026, 12, 24)?,
            d(2026, 12, 25)?,
            d(2026, 12, 31)?,
        ];
        for f in frei {
            assert!(!Boerse::Xetra.handelstag(f), "{f}");
            assert!(Boerse::Xetra.feiertag(f).is_some(), "{f}");
        }
        // Pfingstmontag und 3. Oktober handelt Xetra.
        assert!(Boerse::Xetra.handelstag(d(2026, 5, 25)?));
        assert!(Boerse::Xetra.handelstag(d(2026, 11, 5)?));
        assert!(!Boerse::Xetra.handelstag(d(2026, 11, 7)?));
        // Ein Feiertag am Wochenende ist kein Feiertag-Werktag.
        assert!(Boerse::Xetra.feiertag(d(2026, 12, 26)?).is_some());
        assert!(!Boerse::Xetra.handelstag(d(2026, 12, 26)?));
        Ok(())
    }

    #[test]
    fn nyse_kalender_2026_und_verschiebungen() -> Result<(), String> {
        let frei = [
            d(2026, 1, 1)?,
            d(2026, 1, 19)?,
            d(2026, 2, 16)?,
            d(2026, 4, 3)?,
            d(2026, 5, 25)?,
            d(2026, 6, 19)?,
            d(2026, 7, 3)?, // 4. Juli ist ein Samstag
            d(2026, 9, 7)?,
            d(2026, 11, 26)?,
            d(2026, 12, 25)?,
            d(2027, 7, 5)?,   // 4. Juli ist ein Sonntag
            d(2027, 12, 24)?, // Weihnachten ist ein Samstag
            d(2023, 1, 2)?,   // Neujahr war ein Sonntag
        ];
        for f in frei {
            assert!(!Boerse::Nyse.handelstag(f), "{f}");
        }
        // Neujahr 2028 ist ein Samstag: Der 31.12.2027 bleibt Handelstag.
        assert!(Boerse::Nyse.handelstag(d(2027, 12, 31)?));
        // Juneteenth erst seit 2022; Ostermontag handelt die NYSE.
        assert!(Boerse::Nyse.handelstag(d(2021, 6, 18)?));
        assert!(Boerse::Nyse.handelstag(d(2026, 4, 6)?));
        // Der 26.12. und der 31.12. sind in New York Handelstage.
        assert!(Boerse::Nyse.handelstag(d(2028, 12, 26)?));
        assert!(Boerse::Nyse.handelstag(d(2026, 12, 31)?));
        Ok(())
    }

    #[test]
    fn schluss_je_boerse() -> Result<(), String> {
        let utc = |j, m, t, h, mi| d(j, m, t).ok().and_then(|x| Zeitpunkt::neu(x, h, mi, 0));
        // Xetra 17:35 Frankfurt: im Winter 16:35 UTC.
        assert_eq!(
            Boerse::Xetra.schluss(d(2026, 11, 5)?),
            utc(2026, 11, 5, 16, 35)
        );
        // NYSE 16:00 New York: im Winter 21:00 UTC, im Sommer 20:00 UTC.
        assert_eq!(
            Boerse::Nyse.schluss(d(2026, 11, 5)?),
            utc(2026, 11, 5, 21, 0)
        );
        assert_eq!(Boerse::Nyse.schluss(d(2026, 7, 2)?), utc(2026, 7, 2, 20, 0));
        // Verkuerzt: Tag nach Thanksgiving, 24.12., 3.7. (wenn gehandelt).
        assert_eq!(
            Boerse::Nyse.schluss(d(2026, 11, 27)?),
            utc(2026, 11, 27, 18, 0)
        );
        assert_eq!(
            Boerse::Nyse.schluss(d(2026, 12, 24)?),
            utc(2026, 12, 24, 18, 0)
        );
        assert_eq!(Boerse::Nyse.schluss(d(2025, 7, 3)?), utc(2025, 7, 3, 17, 0));
        assert_eq!(Boerse::Nyse.schluss(d(2026, 7, 3)?), utc(2026, 7, 3, 20, 0));
        Ok(())
    }
}
