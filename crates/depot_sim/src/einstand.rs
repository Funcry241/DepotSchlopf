//! Gebuchter Einstand eines Papierbestands.

use depot_kern::Cent;
use depot_regelwerk::Richtung;

#[derive(Debug, Default)]
pub struct Einstand {
    pub stueck: u64,
    pub kosten: Cent,
}

impl Einstand {
    /// Bucht eine Ausführung und liefert für Verkäufe den Nettoertrag nach Einstand und Gebühren.
    /// Der Resteinstand rundet auf volle Cent auf. Je Teilverkauf kann das Netto dadurch
    /// um weniger als einen Cent höher liegen. Beim vollständigen Verkauf ist der Einstand
    /// vollständig verrechnet.
    pub fn buche(&mut self, richtung: Richtung, a: &depot_journal::Ausfuehrung) -> Option<Cent> {
        let verbindlich = a.kurs.mal_stueck(a.stueck)?;
        match richtung {
            Richtung::Kauf => {
                self.stueck = self.stueck.checked_add(a.stueck)?;
                self.kosten = self
                    .kosten
                    .checked_add(verbindlich)?
                    .checked_add(a.gebuehr)?;
                Some(Cent::NULL)
            }
            Richtung::Verkauf => {
                let rest = self.stueck.checked_sub(a.stueck)?;
                let stueck_vorher = i128::from(self.stueck);
                let zaehler = i128::from(self.kosten.0).checked_mul(i128::from(rest))?;
                let ab = zaehler.checked_div(stueck_vorher)?;
                let auf = i128::from(zaehler.checked_rem(stueck_vorher)? != 0);
                let restkosten = Cent(i64::try_from(ab.checked_add(auf)?).ok()?);
                let netto = verbindlich
                    .checked_sub(a.gebuehr)?
                    .checked_sub(self.kosten.checked_sub(restkosten)?)?;
                self.kosten = restkosten;
                self.stueck = rest;
                Some(netto)
            }
        }
    }
}
