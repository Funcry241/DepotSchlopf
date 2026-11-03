//! Auftraege. `GenehmigterAuftrag` kann ausserhalb dieses Crates niemand bauen:
//! Das Feld `_siegel` hat einen Typ, dessen Konstruktor privat ist. Der Broker nimmt nur diesen Typ an.

use depot_kern::{Cent, Isin};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Richtung {
    Kauf,
    Verkauf,
}

/// Eindeutige Kennung je Auftrag, damit ein doppeltes Senden nach einem Absturz erkennbar ist.
/// Entsteht nur in der zentralen [`Kennungsvergabe`] (R24).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AuftragsKennung {
    nr: u64,
}

impl AuftragsKennung {
    pub fn nr(self) -> u64 {
        self.nr
    }
}

/// Die zentrale Vergabe der Auftragskennungen (R24). Alle Planer eines Laufs teilen sich eine
/// Vergabe; jede Kennung ist um eins groesser als die vorige, keine kommt zweimal vor. Den
/// Startwert, die zuletzt vergebene Nummer, liefert das Journal. Bis zur Persistenzbasis
/// reicht ihn der Aufrufer herein.
#[derive(Debug, PartialEq, Eq)]
pub struct Kennungsvergabe {
    letzte: u64,
}

impl Kennungsvergabe {
    /// Setzt die Vergabe hinter die zuletzt vergebene Nummer.
    pub fn nach(letzte: u64) -> Kennungsvergabe {
        Kennungsvergabe { letzte }
    }

    /// Die zuletzt vergebene Nummer (fuer das Journal).
    pub fn letzte(&self) -> u64 {
        self.letzte
    }

    /// Vergibt `anzahl` fortlaufende Kennungen auf einmal, oder keine: `None`, wenn die Nummern
    /// nicht fuer alle reichen. Dann bleibt die Vergabe unveraendert (K9-03).
    pub(crate) fn vergib(&mut self, anzahl: usize) -> Option<Vec<AuftragsKennung>> {
        let anzahl = u64::try_from(anzahl).ok()?;
        let letzte = self.letzte.checked_add(anzahl)?;
        let erste = self.letzte.checked_add(1)?;
        let kennungen = (erste..=letzte).map(|nr| AuftragsKennung { nr }).collect();
        self.letzte = letzte;
        Some(kennungen)
    }
}

/// Ein Limit-Auftrag. Andere Auftragsarten gibt es nicht.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Auftrag {
    pub kennung: AuftragsKennung,
    pub isin: Isin,
    pub richtung: Richtung,
    pub stueck: u64,
    pub limit: Cent,
}

/// Warum ein Auftrag entsteht. Alles ausser `Umschichtung` ist ein zwingender Ausstieg (R36):
/// Halt, Orderzahl, Umschlag und Mindestorder sperren ihn nicht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anlass {
    /// Freiwillig, aus dem Vorschlag.
    Umschichtung,
    /// Zwingend: Der Wert hat die Positivliste verlassen.
    AusstiegPositivliste,
    /// Zwingend: Abbau ueber der Bestandsgrenze (Anteil ueber Toleranz oder zu viele Werte).
    Abbau,
    /// Zwingend: Notbremse.
    Notbremse,
}

impl Anlass {
    pub fn zwingend(self) -> bool {
        self != Anlass::Umschichtung
    }
}

/// Wofuer und unter welchen Grenzen der Auftrag genehmigt wurde.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegelBeleg {
    pub grenzen_abdruck: String,
    pub anlass: Anlass,
}

mod siegel {
    /// Nur innerhalb von `depot_regelwerk` erzeugbar.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Siegel(());

    impl Siegel {
        pub(crate) fn neu() -> Siegel {
            Siegel(())
        }
    }
}

/// Der einzige Auftragstyp, den die Auftragsstelle an den Broker senden kann.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenehmigterAuftrag {
    auftrag: Auftrag,
    beleg: RegelBeleg,
    _siegel: siegel::Siegel,
}

impl GenehmigterAuftrag {
    pub(crate) fn neu(auftrag: Auftrag, beleg: RegelBeleg) -> GenehmigterAuftrag {
        GenehmigterAuftrag {
            auftrag,
            beleg,
            _siegel: siegel::Siegel::neu(),
        }
    }

    pub fn auftrag(&self) -> &Auftrag {
        &self.auftrag
    }

    pub fn beleg(&self) -> &RegelBeleg {
        &self.beleg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vergabe_zaehlt_fortlaufend_und_endet_sauber() {
        let nummern = |k: Option<Vec<AuftragsKennung>>| {
            k.map(|k| k.into_iter().map(AuftragsKennung::nr).collect::<Vec<_>>())
        };
        let mut v = Kennungsvergabe::nach(41);
        assert_eq!(nummern(v.vergib(1)), Some(vec![42]));
        assert_eq!(nummern(v.vergib(2)), Some(vec![43, 44]));
        assert_eq!(nummern(v.vergib(0)), Some(vec![]));
        assert_eq!(v.letzte(), 44);
        let mut knapp = Kennungsvergabe::nach(u64::MAX - 1);
        assert_eq!(knapp.vergib(2), None);
        assert_eq!(knapp.letzte(), u64::MAX - 1);
        assert_eq!(nummern(knapp.vergib(1)), Some(vec![u64::MAX]));
        assert_eq!(knapp.vergib(1), None);
        assert_eq!(knapp.letzte(), u64::MAX);
    }
}
