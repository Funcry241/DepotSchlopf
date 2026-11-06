//! Strukturierte Vorhersagen fuer die spaetere Auswertung des Weltmodells.
use depot_kern::{BeraterId, Datum, Isin, Preis};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prognoserichtung {
    Steigt,
    Faellt,
}

impl Prognoserichtung {
    pub fn lies(text: &str) -> Option<Self> {
        match text {
            "steigt" => Some(Self::Steigt),
            "faellt" => Some(Self::Faellt),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Steigt => "steigt",
            Self::Faellt => "faellt",
        }
    }
}

/// Ein unabhaengiger, optionaler Befund. Die Ereignisnummer bezieht sich auf die Frage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Weltprognose {
    pub isin: Isin,
    pub richtung: Prognoserichtung,
    pub handelstage: u8,
    pub ereignis_nr: usize,
    pub begruendung: String,
    pub widerlegung: String,
}

impl Weltprognose {
    pub fn gueltig(&self) -> bool {
        (1..=20).contains(&self.handelstage)
            && self.ereignis_nr > 0
            && text_ok(&self.begruendung, 600)
            && text_ok(&self.widerlegung, 600)
    }
}

/// Eine im Journal fixierte Vorhersage mit Ereignis und beobachtetem Ausgangskurs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrognoseBeleg {
    pub datum: Datum,
    pub berater: BeraterId,
    pub prognose: Weltprognose,
    pub ausgangskurs: Preis,
    pub ereignis: String,
}

fn text_ok(text: &str, max: usize) -> bool {
    !text.trim().is_empty() && text.chars().count() <= max && !text.chars().any(char::is_control)
}

fn kodiere(text: &str) -> String {
    text.replace('%', "%25").replace(';', "%3B")
}

fn dekodiere(text: &str) -> String {
    text.replace("%3B", ";").replace("%25", "%")
}

impl PrognoseBeleg {
    pub(crate) fn als_text(&self) -> String {
        let p = &self.prognose;
        let berater = match self.berater {
            BeraterId::Dachs => "Dachs",
            BeraterId::Maus => "Maus",
        };
        format!(
            "{};{berater};{};{};{};{};{};{};{};{}",
            self.datum,
            p.isin,
            p.richtung.name(),
            p.handelstage,
            p.ereignis_nr,
            self.ausgangskurs.cent_text(),
            kodiere(&self.ereignis),
            kodiere(&p.begruendung),
            kodiere(&p.widerlegung)
        )
    }

    pub(crate) fn lies(text: &str) -> Option<Self> {
        let teile: Vec<_> = text.split(';').collect();
        let [
            datum,
            berater,
            isin,
            richtung,
            tage,
            nr,
            preis,
            ereignis,
            grund,
            widerlegung,
        ] = teile.as_slice()
        else {
            return None;
        };
        let b = Self {
            datum: Datum::lies(datum)?,
            berater: match *berater {
                "Dachs" => BeraterId::Dachs,
                "Maus" => BeraterId::Maus,
                _ => return None,
            },
            prognose: Weltprognose {
                isin: Isin::neu(isin).ok()?,
                richtung: Prognoserichtung::lies(richtung)?,
                handelstage: tage.parse().ok()?,
                ereignis_nr: nr.parse().ok()?,
                begruendung: dekodiere(grund),
                widerlegung: dekodiere(widerlegung),
            },
            ausgangskurs: Preis::lies_cent_text(preis)?,
            ereignis: dekodiere(ereignis),
        };
        (b.prognose.gueltig()
            && b.ausgangskurs.fein() > 0
            && text_ok(&b.ereignis, 4_000)
            && b.als_text() == text)
            .then_some(b)
    }
}
