//! Termine im Ereignisregister (R39): angekuendigte Ereignisse mit Zeitbezug, etwa Sitzungen der
//! Zentralbank oder Quartalszahlen. Jeder Termin hat dasselbe Format, gleich aus welcher Quelle:
//! Kennung, Titel, Zeitbezug, Betroffene und Verlaesslichkeit. Keine Quelle hat Sonderregeln.
//!
//! Ein Termin sagt nichts ueber den Ausgang (R40). Die KI erfindet keine Termine; sie kann nur
//! Eintraege mit Kennung aus dem Register verwenden.
//!
//! Als Text, wie das Journal ihn fuehrt:
//!
//! - `kennung`: Kleinbuchstaben, Ziffern und `-`, hoechstens 80 Zeichen, nicht mit `-` am Anfang
//!   oder Ende. Dieselbe Kennung bezeichnet denselben Termin; verschiebt er sich, wird er unter
//!   seiner Kennung fortgeschrieben.
//! - `zeit`: `JJJJ-MM-TT` fuer einen Tag ohne bekannte Uhrzeit, sonst ein Zeitpunkt in UTC
//!   (`JJJJ-MM-TTThh:mm:ssZ`).
//! - `betroffen`: durch `,` getrennt, aufsteigend und ohne Doppel: ISIN eines Werts oder eine
//!   Groesse aus Kleinbuchstaben, Ziffern und `-` (etwa `eur-leitzins`).
//! - `titel`: hoechstens 200 Zeichen, ohne Steuerzeichen und ohne `;`.

use std::fmt;

use crate::instrument::Isin;
use crate::zeit::{Datum, Zeitpunkt};

/// Wie verlaesslich ein Eintrag ist (R39).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Verlaesslichkeit {
    /// Von der Stelle selbst veroeffentlicht, die das Ereignis verantwortet.
    Bestaetigt,
    /// Von einer freigegebenen Quelle gemeldet, aber nicht von dieser Stelle bestaetigt.
    Angekuendigt,
    /// Nur erschlossen, etwa aus dem ueblichen Rhythmus.
    Vermutet,
}

impl Verlaesslichkeit {
    pub fn name(self) -> &'static str {
        match self {
            Verlaesslichkeit::Bestaetigt => "bestaetigt",
            Verlaesslichkeit::Angekuendigt => "angekuendigt",
            Verlaesslichkeit::Vermutet => "vermutet",
        }
    }

    pub fn aus_name(name: &str) -> Option<Verlaesslichkeit> {
        [
            Verlaesslichkeit::Bestaetigt,
            Verlaesslichkeit::Angekuendigt,
            Verlaesslichkeit::Vermutet,
        ]
        .into_iter()
        .find(|v| v.name() == name)
    }
}

/// Wann ein Termin ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Zeitbezug {
    /// Ein Tag, Uhrzeit nicht bekannt.
    Tag(Datum),
    /// Ein Zeitpunkt mit bekannter Uhrzeit.
    Zeitpunkt(Zeitpunkt),
}

impl Zeitbezug {
    /// Liest `JJJJ-MM-TT` oder `JJJJ-MM-TTThh:mm:ssZ`, nur genau diese Formen.
    pub fn lies(text: &str) -> Option<Zeitbezug> {
        if text.contains('T') {
            Zeitpunkt::lies(text).map(Zeitbezug::Zeitpunkt)
        } else {
            Datum::lies(text).map(Zeitbezug::Tag)
        }
    }

    /// Der Tag des Termins in UTC.
    pub fn tag(self) -> Option<Datum> {
        match self {
            Zeitbezug::Tag(d) => Some(d),
            Zeitbezug::Zeitpunkt(z) => z.datum(),
        }
    }
}

impl fmt::Display for Zeitbezug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Zeitbezug::Tag(d) => write!(f, "{d}"),
            Zeitbezug::Zeitpunkt(z) => write!(f, "{z}"),
        }
    }
}

/// Wen ein Termin betrifft.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Betroffen {
    /// Ein Wert.
    Wert(Isin),
    /// Eine Groesse, die viele Werte erreicht, etwa `eur-leitzins`.
    Groesse(String),
}

impl Betroffen {
    /// Eine Groesse: Kleinbuchstaben, Ziffern und `-`, 1 bis 40 Zeichen.
    pub fn groesse(name: &str) -> Option<Betroffen> {
        name_gueltig(name, 40).then(|| Betroffen::Groesse(name.to_string()))
    }

    /// Liest eine ISIN oder eine Groesse.
    pub fn lies(text: &str) -> Option<Betroffen> {
        match Isin::neu(text) {
            Ok(i) => Some(Betroffen::Wert(i)),
            Err(_) => Betroffen::groesse(text),
        }
    }
}

impl fmt::Display for Betroffen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Betroffen::Wert(i) => write!(f, "{i}"),
            Betroffen::Groesse(g) => write!(f, "{g}"),
        }
    }
}

/// Kleinbuchstaben, Ziffern und `-`, nicht leer, nicht laenger als `max`, kein `-` am Rand.
fn name_gueltig(name: &str, max: usize) -> bool {
    !name.is_empty()
        && name.len() <= max
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Warum kein Termin entsteht.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminFehler {
    Kennung,
    Titel,
    /// Kein Betroffener, oder einer doppelt.
    Betroffen,
}

/// Ein geprueft gueltiger Termin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Termin {
    kennung: String,
    titel: String,
    zeit: Zeitbezug,
    betroffen: Vec<Betroffen>,
    verlaesslichkeit: Verlaesslichkeit,
}

impl Termin {
    /// Die Betroffenen werden aufsteigend geordnet.
    ///
    /// # Errors
    /// Kennung oder Titel ungueltig, kein Betroffener, oder einer doppelt.
    pub fn neu(
        kennung: &str,
        titel: &str,
        zeit: Zeitbezug,
        betroffen: Vec<Betroffen>,
        verlaesslichkeit: Verlaesslichkeit,
    ) -> Result<Termin, TerminFehler> {
        if !name_gueltig(kennung, 80) {
            return Err(TerminFehler::Kennung);
        }
        if titel.is_empty()
            || titel.chars().count() > 200
            || titel.chars().any(|c| c.is_control() || c == ';')
        {
            return Err(TerminFehler::Titel);
        }
        let mut betroffen = betroffen;
        let anzahl = betroffen.len();
        betroffen.sort();
        betroffen.dedup();
        if betroffen.is_empty() || betroffen.len() != anzahl {
            return Err(TerminFehler::Betroffen);
        }
        Ok(Termin {
            kennung: kennung.to_string(),
            titel: titel.to_string(),
            zeit,
            betroffen,
            verlaesslichkeit,
        })
    }

    pub fn kennung(&self) -> &str {
        &self.kennung
    }

    pub fn titel(&self) -> &str {
        &self.titel
    }

    pub fn zeit(&self) -> Zeitbezug {
        self.zeit
    }

    pub fn betroffen(&self) -> &[Betroffen] {
        &self.betroffen
    }

    pub fn verlaesslichkeit(&self) -> Verlaesslichkeit {
        self.verlaesslichkeit
    }
}
