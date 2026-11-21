//! Anbieterprofile für Dachs und Maus. Eine Auswahl gehört fest zu einem Berater.
use crate::BeraterId;

pub const MODELL_DACHS: &str = "claude-sonnet-5-5";
pub const MODELL_MAUS: &str = "gpt-6.1-terra";
pub const MAX_BERATER_AUFRUFE: usize = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Denktiefe {
    Ohne,
    Niedrig,
    Mittel,
    Hoch,
    SehrHoch,
    Maximal,
}

const MIT_DENKEN: &[Denktiefe] = &[
    Denktiefe::Niedrig,
    Denktiefe::Mittel,
    Denktiefe::Hoch,
    Denktiefe::SehrHoch,
    Denktiefe::Maximal,
];
const ALLE_STUFEN: &[Denktiefe] = &[
    Denktiefe::Ohne,
    Denktiefe::Niedrig,
    Denktiefe::Mittel,
    Denktiefe::Hoch,
    Denktiefe::SehrHoch,
    Denktiefe::Maximal,
];

impl Denktiefe {
    pub fn kennung(self) -> &'static str {
        match self {
            Self::Ohne => "none",
            Self::Niedrig => "low",
            Self::Mittel => "medium",
            Self::Hoch => "high",
            Self::SehrHoch => "xhigh",
            Self::Maximal => "max",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Ohne => "Ohne",
            Self::Niedrig => "Niedrig",
            Self::Mittel => "Mittel",
            Self::Hoch => "Hoch",
            Self::SehrHoch => "Sehr hoch",
            Self::Maximal => "Maximal",
        }
    }

    pub fn lies(kennung: &str) -> Option<Self> {
        ALLE_STUFEN
            .iter()
            .copied()
            .find(|tiefe| tiefe.kennung() == kennung)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modell {
    Sonnet55,
    Opus55,
    Terra61,
    Sol62,
}

impl Modell {
    pub fn fuer_berater(wer: BeraterId) -> &'static [Self] {
        match wer {
            BeraterId::Dachs => &[Self::Sonnet55, Self::Opus55],
            BeraterId::Maus => &[Self::Terra61, Self::Sol62],
        }
    }

    pub fn berater(self) -> BeraterId {
        match self {
            Self::Sonnet55 | Self::Opus55 => BeraterId::Dachs,
            Self::Terra61 | Self::Sol62 => BeraterId::Maus,
        }
    }

    pub fn kennung(self) -> &'static str {
        match self {
            Self::Sonnet55 => MODELL_DACHS,
            Self::Opus55 => "claude-opus-5-5",
            Self::Terra61 => MODELL_MAUS,
            Self::Sol62 => "gpt-6.2-sol",
        }
    }

    /// Akzeptiert die Modellkennung und die datierten Varianten desselben Modells.
    pub fn passt_meldung(self, meldung: &str) -> bool {
        let kennung = self.kennung();
        kennung == meldung
            || meldung
                .strip_prefix(kennung)
                .and_then(|m| m.strip_prefix('-'))
                .is_some_and(|version| {
                    crate::Datum::lies(version).is_some()
                        || (version.len() == 8 && version.bytes().all(|b| b.is_ascii_digit()))
                })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Sonnet55 => "Claude Sonnet 5.5",
            Self::Opus55 => "Claude Opus 5.5",
            Self::Terra61 => "GPT-6.1 Terra",
            Self::Sol62 => "GPT-6.2 Sol",
        }
    }

    pub fn stufen(self) -> &'static [Denktiefe] {
        match self {
            Self::Terra61 => ALLE_STUFEN,
            Self::Sonnet55 | Self::Opus55 | Self::Sol62 => MIT_DENKEN,
        }
    }

    pub fn lies(kennung: &str) -> Option<Self> {
        [Self::Sonnet55, Self::Opus55, Self::Terra61, Self::Sol62]
            .into_iter()
            .find(|modell| modell.kennung() == kennung)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Modellwahl {
    modell: Modell,
    denktiefe: Denktiefe,
}

impl Modellwahl {
    pub fn neu(wer: BeraterId, modell: Modell, denktiefe: Denktiefe) -> Option<Self> {
        (wer == modell.berater() && modell.stufen().contains(&denktiefe))
            .then_some(Self { modell, denktiefe })
    }

    pub fn standard(wer: BeraterId) -> Self {
        let modell = match wer {
            BeraterId::Dachs => Modell::Sonnet55,
            BeraterId::Maus => Modell::Terra61,
        };
        Self {
            modell,
            denktiefe: Denktiefe::Mittel,
        }
    }

    pub fn modell(self) -> Modell {
        self.modell
    }

    pub fn denktiefe(self) -> Denktiefe {
        self.denktiefe
    }
}

/// Die persönliche Tagesgrenze gilt zusätzlich zu den festen Grenzen des Kassenbuchs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BeraterEinstellung {
    pub auswahl: Modellwahl,
    pub max_aufrufe: usize,
}

impl BeraterEinstellung {
    pub fn standard(wer: BeraterId) -> Self {
        Self {
            auswahl: Modellwahl::standard(wer),
            max_aufrufe: MAX_BERATER_AUFRUFE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KiEinstellungen {
    pub dachs: BeraterEinstellung,
    pub maus: BeraterEinstellung,
    pub gemini: usize,
    pub neuroschlopf: usize,
}

impl Default for KiEinstellungen {
    fn default() -> Self {
        Self {
            dachs: BeraterEinstellung::standard(BeraterId::Dachs),
            maus: BeraterEinstellung::standard(BeraterId::Maus),
            gemini: 5,
            neuroschlopf: 8,
        }
    }
}
