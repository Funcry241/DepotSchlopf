//! Kaufthesen kommen erst über ausgeführte Aufträge in die Folgeberatung.

use depot_journal::{Ausfuehrung, Stand};
use depot_kern::{Cent, Datum, Isin, Preis, Topf, Zeitpunkt};
use depot_regelwerk::Richtung;

pub const KAUFTHESE: &str = "Kaufthese";
pub(crate) const ZAHLEN_UEBERRASCHUNG: &str = "Zahlen-Überraschung";

/// Kontrollierte Anlassmarke zu Beginn einer Kaufbegründung.
pub fn kaufanlass(text: &str) -> Option<&'static str> {
    let (marke, _) = text.trim_start().strip_prefix('[')?.split_once(']')?;
    [
        "Zahlen",
        ZAHLEN_UEBERRASCHUNG,
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
pub enum Kursziel {
    Absolut(Cent),
    GewinnBp(i64),
}

impl std::fmt::Display for Kursziel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Absolut(kurs) => write!(f, "{kurs}"),
            Self::GewinnBp(bp) => write!(f, "+{bp} Basispunkte ab Füllpreis"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ausstiegsplan {
    pub kursziel: Kursziel,
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
        let kursziel = if let Some(bp) = ziel
            .strip_prefix('+')
            .and_then(|ziel| ziel.strip_suffix("bp"))
        {
            let bp: i64 = bp.parse().ok()?;
            (bp > 0).then_some(Kursziel::GewinnBp(bp))?
        } else {
            let cent: i64 = ziel.parse().ok()?;
            (cent > 0).then_some(Kursziel::Absolut(Cent(cent)))?
        };
        let handelstage = frist.parse::<u16>().ok()?;
        (handelstage != 0).then_some(Self {
            kursziel,
            handelstage,
        })
    }
}

impl Positionskauf {
    pub fn ausstiegsplan(&self) -> Option<Ausstiegsplan> {
        Ausstiegsplan::lies(self.these.as_deref()?)
    }

    pub fn kursziel_erreicht(&self, kurs: Preis) -> bool {
        let ziel = self.ausstiegsplan().and_then(|plan| match plan.kursziel {
            Kursziel::Absolut(ziel) => Some(ziel),
            Kursziel::GewinnBp(bp) => self
                .ausfuehrung
                .kurs
                .checked_add(self.ausfuehrung.kurs.anteil_bp_auf(bp)?),
        });
        ziel.and_then(Preis::aus_cent)
            .is_some_and(|ziel| kurs >= ziel)
    }

    pub fn frist_abgelaufen(&self, heute: Datum) -> bool {
        self.ausstiegsplan()
            .zip(self.handelstage_seit_kauf(heute))
            .is_some_and(|(plan, seit)| seit >= u32::from(plan.handelstage))
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

#[cfg(test)]
mod ziel_tests {
    use super::*;

    #[test]
    fn altes_centziel_und_relatives_ziel_am_fuellpreis() -> Result<(), String> {
        let mut kauf = Positionskauf {
            isin: Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?,
            ausfuehrung: Ausfuehrung {
                stueck: 1,
                kurs: Cent(10_000),
                gebuehr: Cent(150),
                kurszeit: None,
            },
            these: Some("[Zahlen] [Ausstieg 10800 10] Alt".into()),
        };
        let unter = Preis::aus_cent(Cent(10_799)).ok_or("Kurs")?;
        let ziel = Preis::aus_cent(Cent(10_800)).ok_or("Kurs")?;
        assert!(!kauf.kursziel_erreicht(unter));
        assert!(kauf.kursziel_erreicht(ziel));
        kauf.these = Some("[Zahlen] [Ausstieg +800bp 10] Neu".into());
        assert!(!kauf.kursziel_erreicht(unter));
        assert!(kauf.kursziel_erreicht(ziel));
        Ok(())
    }
}
