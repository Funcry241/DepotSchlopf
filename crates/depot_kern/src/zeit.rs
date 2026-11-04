//! Kalenderdaten und Zeitpunkte. Beide werden beim Eintritt geprueft (R20); eine rohe Zahl ist
//! kein Datum. Die Uhr liest der Kern nie, den Zeitpunkt "jetzt" reicht der Aufrufer herein.

/// Ein gueltiges Datum des gregorianischen Kalenders zwischen 1970 und 9999.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Datum {
    jahr: u16,
    monat: u8,
    tag: u8,
}

fn schaltjahr(jahr: u16) -> bool {
    jahr.is_multiple_of(4) && (!jahr.is_multiple_of(100) || jahr.is_multiple_of(400))
}

fn tage_im_monat(jahr: u16, monat: u8) -> u8 {
    match monat {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if schaltjahr(jahr) => 29,
        2 => 28,
        _ => 0,
    }
}

impl Datum {
    /// `None` fuer jeden Tag, den es im Kalender nicht gibt (etwa den 31.02.).
    pub fn neu(jahr: u16, monat: u8, tag: u8) -> Option<Datum> {
        ((1970..=9999).contains(&jahr) && tag >= 1 && tag <= tage_im_monat(jahr, monat))
            .then_some(Datum { jahr, monat, tag })
    }

    pub fn jahr(self) -> u16 {
        self.jahr
    }

    pub fn monat(self) -> u8 {
        self.monat
    }

    pub fn tag(self) -> u8 {
        self.tag
    }

    /// Tage seit dem 01.01.1970 (Verfahren nach H. Hinnant, nur geprueft gerechnet).
    fn tage_seit_1970(self) -> Option<i64> {
        let m = i64::from(self.monat);
        let y = i64::from(self.jahr).checked_sub(i64::from(m <= 2))?;
        let era = y.checked_div(400)?;
        let yoe = y.checked_sub(era.checked_mul(400)?)?;
        let mp = if m > 2 {
            m.checked_sub(3)?
        } else {
            m.checked_add(9)?
        };
        let doy = mp
            .checked_mul(153)?
            .checked_add(2)?
            .checked_div(5)?
            .checked_add(i64::from(self.tag))?
            .checked_sub(1)?;
        let doe = yoe
            .checked_mul(365)?
            .checked_add(yoe.checked_div(4)?)?
            .checked_sub(yoe.checked_div(100)?)?
            .checked_add(doy)?;
        era.checked_mul(146_097)?
            .checked_add(doe)?
            .checked_sub(719_468)
    }
}

/// Ein Zeitpunkt in Sekunden seit 01.01.1970, 00:00 Uhr UTC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Zeitpunkt {
    sekunden: i64,
}

const SEKUNDEN_JE_TAG: i64 = 86_400;

impl Zeitpunkt {
    /// Uhrzeit in UTC. `None` fuer jede Uhrzeit, die es nicht gibt (Schaltsekunden zaehlen nicht).
    pub fn neu(datum: Datum, stunde: u8, minute: u8, sekunde: u8) -> Option<Zeitpunkt> {
        if stunde > 23 || minute > 59 || sekunde > 59 {
            return None;
        }
        let tag = datum.tage_seit_1970()?.checked_mul(SEKUNDEN_JE_TAG)?;
        let uhr = i64::from(stunde)
            .checked_mul(3_600)?
            .checked_add(i64::from(minute).checked_mul(60)?)?
            .checked_add(i64::from(sekunde))?;
        Some(Zeitpunkt {
            sekunden: tag.checked_add(uhr)?,
        })
    }

    /// Sekunden seit `frueher`; `None`, wenn `frueher` nach diesem Zeitpunkt liegt.
    pub fn sekunden_seit(self, frueher: Zeitpunkt) -> Option<i64> {
        self.sekunden
            .checked_sub(frueher.sekunden)
            .filter(|d| *d >= 0)
    }

    /// Liegt der Zeitpunkt an genau diesem Tag (UTC)?
    pub fn am_tag(self, datum: Datum) -> bool {
        datum
            .tage_seit_1970()
            .and_then(|t| t.checked_mul(SEKUNDEN_JE_TAG))
            .and_then(|beginn| Some((beginn, beginn.checked_add(SEKUNDEN_JE_TAG)?)))
            .is_some_and(|(beginn, ende)| self.sekunden >= beginn && self.sekunden < ende)
    }

    /// Sekunden seit 01.01.1970, 00:00 Uhr UTC.
    pub fn sekunden(self) -> i64 {
        self.sekunden
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kalender() {
        assert!(Datum::neu(2026, 2, 31).is_none());
        assert!(Datum::neu(2000, 2, 29).is_some());
        assert!(Datum::neu(1900, 2, 29).is_none());
        assert!(Datum::neu(2100, 2, 29).is_none());
        assert!(Datum::neu(1969, 12, 31).is_none());
        assert!(Datum::neu(2026, 0, 1).is_none());
    }

    #[test]
    fn tageszaehlung() {
        let t = |j, m, d| Datum::neu(j, m, d).and_then(Datum::tage_seit_1970);
        assert_eq!(t(1970, 1, 1), Some(0));
        assert_eq!(t(2000, 3, 1), Some(11_017));
        assert_eq!(t(2026, 11, 2), Some(20_759));
        assert_eq!(t(9999, 12, 31), Some(2_932_896));
    }

    #[test]
    fn zeitpunkte() {
        let d = Datum::neu(2026, 11, 2);
        let z = |h, m, s| d.and_then(|d| Zeitpunkt::neu(d, h, m, s));
        assert_eq!(z(0, 0, 0).map(|x| x.sekunden), Some(20_759 * 86_400));
        assert!(z(23, 59, 59).is_some());
        assert!(z(24, 0, 0).is_none());
        let (a, b) = (z(10, 0, 0), z(10, 20, 0));
        assert_eq!(a.zip(b).and_then(|(a, b)| b.sekunden_seit(a)), Some(1_200));
        assert_eq!(a.zip(b).and_then(|(a, b)| a.sekunden_seit(b)), None);
    }
}
