//! Kaufthesen kommen erst über ausgeführte Aufträge in die Folgeberatung.

use depot_journal::{Ausfuehrung, Stand};
use depot_kern::{Isin, Topf};
use depot_regelwerk::Richtung;

pub(crate) const KAUFTHESE: &str = "Kaufthese";

/// Letzte belegte Kaufausführung für eine gehaltene Position und die zugehörige These.
#[derive(Clone, Debug)]
pub struct Positionskauf {
    pub isin: Isin,
    pub ausfuehrung: Ausfuehrung,
    pub these: Option<String>,
}

pub(crate) fn lies(stand: &Stand, topf: &Topf) -> Vec<Positionskauf> {
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
