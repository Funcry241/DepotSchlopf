//! Kalenderdaten und Zeitpunkte. Beide werden beim Eintritt geprueft (R20); eine rohe Zahl ist
//! kein Datum. Die Uhr liest der Kern nie, den Zeitpunkt "jetzt" reicht der Aufrufer herein.
//!
//! Als Text steht ein Zeitpunkt immer in UTC: `2026-11-04T16:35:00Z`. Gelesen wird nur genau
//! diese Form.

use std::fmt;

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

    /// Liest genau die Form `JJJJ-MM-TT` eines Tags im Kalender.
    pub fn lies(text: &str) -> Option<Datum> {
        let mut teile = text.split('-');
        let mut zahl = |n: usize| {
            let t = teile.next()?;
            (t.len() == n && t.bytes().all(|b| b.is_ascii_digit()))
                .then(|| t.parse::<u16>().ok())
                .flatten()
        };
        let jahr = zahl(4)?;
        let monat = u8::try_from(zahl(2)?).ok()?;
        let tag = u8::try_from(zahl(2)?).ok()?;
        if teile.next().is_some() {
            return None;
        }
        Datum::neu(jahr, monat, tag)
    }

    /// Wochentag nach ISO 8601: 1 Montag bis 7 Sonntag.
    pub fn wochentag(self) -> Option<u8> {
        // Der 01.01.1970 war ein Donnerstag (4).
        let t = self
            .tage_seit_1970()?
            .checked_add(3)?
            .checked_rem_euclid(7)?;
        u8::try_from(t).ok()?.checked_add(1)
    }

    /// Der letzte Sonntag eines Monats.
    fn letzter_sonntag(jahr: u16, monat: u8) -> Option<Datum> {
        let ende = Datum::neu(jahr, monat, tage_im_monat(jahr, monat))?;
        let zurueck = ende.wochentag()?.checked_rem(7)?;
        Datum::neu(jahr, monat, ende.tag.checked_sub(zurueck)?)
    }

    /// Das Datum zu einer Tageszahl seit dem 01.01.1970 (Umkehrung nach H. Hinnant).
    fn aus_tagen(tage: i64) -> Option<Datum> {
        let z = tage.checked_add(719_468)?;
        let era = z.checked_div_euclid(146_097)?;
        let doe = z.checked_sub(era.checked_mul(146_097)?)?;
        let yoe = doe
            .checked_sub(doe.checked_div(1_460)?)?
            .checked_add(doe.checked_div(36_524)?)?
            .checked_sub(doe.checked_div(146_096)?)?
            .checked_div(365)?;
        let doy = doe.checked_sub(
            yoe.checked_mul(365)?
                .checked_add(yoe.checked_div(4)?)?
                .checked_sub(yoe.checked_div(100)?)?,
        )?;
        let mp = doy.checked_mul(5)?.checked_add(2)?.checked_div(153)?;
        let tag = doy
            .checked_sub(mp.checked_mul(153)?.checked_add(2)?.checked_div(5)?)?
            .checked_add(1)?;
        let monat = if mp < 10 {
            mp.checked_add(3)?
        } else {
            mp.checked_sub(9)?
        };
        let jahr = yoe
            .checked_add(era.checked_mul(400)?)?
            .checked_add(i64::from(monat <= 2))?;
        Datum::neu(
            u16::try_from(jahr).ok()?,
            u8::try_from(monat).ok()?,
            u8::try_from(tag).ok()?,
        )
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

    /// Aus Sekunden seit 01.01.1970, 00:00 Uhr UTC, etwa von der Uhr des Rechners, die der
    /// Aufrufer liest. `None` ausserhalb der Jahre 1970 bis 9999.
    pub fn aus_sekunden(sekunden: i64) -> Option<Zeitpunkt> {
        let z = Zeitpunkt { sekunden };
        z.datum().map(|_| z)
    }

    /// Sekunden seit 01.01.1970, 00:00 Uhr UTC.
    pub fn sekunden(self) -> i64 {
        self.sekunden
    }

    /// Der Tag in UTC.
    pub fn datum(self) -> Option<Datum> {
        Datum::aus_tagen(self.sekunden.checked_div_euclid(SEKUNDEN_JE_TAG)?)
    }

    /// Stunde, Minute und Sekunde in UTC.
    pub fn uhrzeit(self) -> Option<(u8, u8, u8)> {
        let s = self.sekunden.checked_rem_euclid(SEKUNDEN_JE_TAG)?;
        let teil = |n: i64| u8::try_from(n).ok();
        Some((
            teil(s.checked_div(3_600)?)?,
            teil(s.checked_rem(3_600)?.checked_div(60)?)?,
            teil(s.checked_rem(60)?)?,
        ))
    }

    /// Uhrzeit in Frankfurt am Main (MEZ, im Sommer MESZ). Umgestellt wird nach EU-Recht am
    /// letzten Sonntag im Maerz und im Oktober um 01:00 Uhr UTC. `None` fuer eine Uhrzeit, die
    /// es dort nicht gibt oder die doppelt vorkommt: Der Kalender raet nie.
    pub fn frankfurt(datum: Datum, stunde: u8, minute: u8, sekunde: u8) -> Option<Zeitpunkt> {
        let ortszeit = Zeitpunkt::neu(datum, stunde, minute, sekunde)?;
        let mut treffer = [1_i64, 2].into_iter().filter_map(|stunden| {
            let utc = Zeitpunkt {
                sekunden: ortszeit.sekunden.checked_sub(stunden.checked_mul(3_600)?)?,
            };
            (utc.sommerzeit()? == (stunden == 2)).then_some(utc)
        });
        let erster = treffer.next()?;
        treffer.next().is_none().then_some(erster)
    }

    /// Gilt in Frankfurt zu diesem Zeitpunkt Sommerzeit?
    fn sommerzeit(self) -> Option<bool> {
        let jahr = self.datum()?.jahr;
        let wechsel = |monat| Zeitpunkt::neu(Datum::letzter_sonntag(jahr, monat)?, 1, 0, 0);
        Some(self >= wechsel(3)? && self < wechsel(10)?)
    }

    /// Liest genau die Form `JJJJ-MM-TTThh:mm:ssZ`.
    pub fn lies(text: &str) -> Option<Zeitpunkt> {
        let (datum, uhr) = text.strip_suffix('Z')?.split_once('T')?;
        let zahl = |t: &str, n: usize| -> Option<u16> {
            (t.len() == n && t.bytes().all(|b| b.is_ascii_digit()))
                .then(|| t.parse().ok())
                .flatten()
        };
        let klein = |t: &str| zahl(t, 2).and_then(|z| u8::try_from(z).ok());
        let mut d = datum.split('-');
        let mut u = uhr.split(':');
        let jahr = zahl(d.next()?, 4)?;
        let monat = klein(d.next()?)?;
        let tag = klein(d.next()?)?;
        let stunde = klein(u.next()?)?;
        let minute = klein(u.next()?)?;
        let sekunde = klein(u.next()?)?;
        if d.next().is_some() || u.next().is_some() {
            return None;
        }
        Zeitpunkt::neu(Datum::neu(jahr, monat, tag)?, stunde, minute, sekunde)
    }
}

impl fmt::Display for Datum {
    /// `JJJJ-MM-TT`, wie `Datum::lies` es liest.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.jahr, self.monat, self.tag)
    }
}

impl fmt::Display for Zeitpunkt {
    /// `JJJJ-MM-TTThh:mm:ssZ`; ein Zeitpunkt ausserhalb des Kalenders steht als Sekundenzahl.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.datum().zip(self.uhrzeit()) {
            Some((tag, (stunde, minute, sekunde))) => write!(
                f,
                "{:04}-{:02}-{:02}T{stunde:02}:{minute:02}:{sekunde:02}Z",
                tag.jahr, tag.monat, tag.tag
            ),
            None => write!(f, "@{}", self.sekunden),
        }
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
