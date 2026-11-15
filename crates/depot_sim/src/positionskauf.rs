//! Kaufthesen kommen erst über ausgeführte Aufträge in die Folgeberatung.

use depot_journal::{Ausfuehrung, Stand};
use depot_kern::{Cent, Datum, Isin, Topf, Zeitpunkt};
use depot_regelwerk::Richtung;

pub const KAUFTHESE: &str = "Kaufthese";

/// Kontrollierte Anlassmarke zu Beginn einer Kaufbegründung.
pub fn kaufanlass(text: &str) -> Option<&'static str> {
    let (marke, _) = text.trim_start().strip_prefix('[')?.split_once(']')?;
    [
        "Zahlen",
        "Notenbank",
        "Meldung",
        "Kursbewegung",
        "Sonstiges",
    ]
    .into_iter()
    .find(|anlass| anlass.to_lowercase() == marke.to_lowercase())
}

/// Letzte belegte Kaufausführung für eine gehaltene Position und die zugehörige These.
#[derive(Clone, Debug)]
pub struct Positionskauf {
    pub isin: Isin,
    pub ausfuehrung: Ausfuehrung,
    pub these: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ausstiegsplan {
    pub kursziel: Cent,
    pub handelstage: u16,
}

impl Ausstiegsplan {
    /// Versionsverträgliche Markierung direkt hinter dem Kaufanlass in der Kaufthese.
    pub fn lies(these: &str) -> Option<Self> {
        kaufanlass(these)?;
        let (_, rest) = these.split_once(']')?;
        let rest = rest.trim_start();
        let rest = rest.strip_prefix(':').unwrap_or(rest).trim_start();
        let (felder, _) = rest.strip_prefix("[Ausstieg ")?.split_once("] ")?;
        let (ziel, frist) = felder.split_once(' ')?;
        let kursziel = Cent(ziel.parse::<i64>().ok()?);
        let handelstage = frist.parse::<u16>().ok()?;
        (kursziel.0 > 0 && handelstage != 0).then_some(Self {
            kursziel,
            handelstage,
        })
    }
}

impl Positionskauf {
    pub fn ausstiegsplan(&self) -> Option<Ausstiegsplan> {
        Ausstiegsplan::lies(self.these.as_deref()?)
    }

    pub fn handelstage_seit_kauf(&self, bis: Datum) -> Option<u32> {
        let kauf = self
            .ausfuehrung
            .kurszeit
            .and_then(Zeitpunkt::frankfurt_tag)?;
        crate::kursverlauf::haltedauer(&self.isin, kauf, bis)
    }
}

pub fn lies(stand: &Stand, topf: &Topf) -> Vec<Positionskauf> {
    topf.positionen()
        .keys()
        .filter_map(|isin| {
            let (nr, ausfuehrung) = stand
                .auftraege()
                .iter()
                .filter(|a| {
                    a.beleg.topf == topf.id()
                        && a.beleg.isin == *isin
                        && a.beleg.richtung == Richtung::Kauf
                })
                .flat_map(|a| a.ausfuehrungen.iter().map(|f| (a.beleg.nr, *f)))
                .max_by_key(|(nr, f)| (f.kurszeit, *nr))?;
            let these = stand
                .begruendungen()
                .iter()
                .rev()
                .filter(|(_, wer, _)| wer == KAUFTHESE)
                .find_map(|(_, _, text)| {
                    let (kennung, these) = text.split_once(' ')?;
                    (kennung.parse().ok() == Some(nr)).then(|| these.to_owned())
                });
            Some(Positionskauf {
                isin: isin.clone(),
                ausfuehrung,
                these,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn kaufanlass_toleriert_schreibvarianten() {
        for text in [" [zahlen]: x", "[Zahlen]x", "[ZAHLEN] x"] {
            assert_eq!(super::kaufanlass(text), Some("Zahlen"));
        }
        assert_eq!(super::kaufanlass("[Quatsch] x"), None);
    }
}
