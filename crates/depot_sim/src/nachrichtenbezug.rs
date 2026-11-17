//! Ordnet Schlagzeilen anhand der belegten Beobachtungsliste ein.
use depot_kern::Isin;
use depot_kern::liste::Wert;

pub(crate) struct Bezug<'a> {
    pub werte: Vec<&'a Wert>,
    pub makro: bool,
}

impl Bezug<'_> {
    pub fn markierung(&self) -> String {
        if self.werte.is_empty() {
            return if self.makro {
                " [Makro]".into()
            } else {
                String::new()
            };
        }
        let namen = self
            .werte
            .iter()
            .map(|w| w.name.as_str())
            .collect::<Vec<_>>();
        format!(" [betrifft: {}]", namen.join(", "))
    }

    pub fn rang(&self, bestand: &std::collections::BTreeSet<Isin>) -> usize {
        if self.werte.iter().any(|w| bestand.contains(&w.isin)) {
            0
        } else if !self.werte.is_empty() {
            1
        } else if self.makro {
            2
        } else {
            3
        }
    }
}

pub(crate) struct Wortsuche<'a> {
    woerter: Vec<&'a str>,
}

impl<'a> Wortsuche<'a> {
    /// Der Text ist bereits kleingeschrieben.
    pub(crate) fn neu(text: &'a str) -> Self {
        Self {
            woerter: text
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
                .collect(),
        }
    }

    pub(crate) fn trifft(&self, ausdruck: &str) -> bool {
        let ausdruck = ausdruck.to_lowercase();
        let teile: Vec<_> = ausdruck
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();
        ausdruck.chars().count() >= 3
            && !teile.is_empty()
            && self.woerter.windows(teile.len()).any(|w| w == teile)
    }

    pub(crate) fn wert(&self, w: &Wert) -> bool {
        self.trifft(w.isin.als_str())
            || self.trifft(&w.name)
            || self.trifft(w.kuerzel.strip_suffix(".DEX").unwrap_or(&w.kuerzel))
    }
}

pub(crate) fn bezug<'a>(text: &str, werte: &'a [Wert]) -> Bezug<'a> {
    let text = text.to_lowercase();
    let suche = Wortsuche::neu(&text);
    let makro = [
        "Zins",
        "Leitzins",
        "Inflation",
        "Fed",
        "EZB",
        "Öl",
        "Zoll",
        "Zölle",
        "Sanktion",
        "rate",
        "tariff",
        "oil",
    ]
    .into_iter()
    .any(|wort| suche.woerter.contains(&wort.to_lowercase().as_str()));
    let werte = werte.iter().filter(|w| suche.wert(w)).collect();
    Bezug { werte, makro }
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_kern::liste::beobachtungsliste;

    #[test]
    fn ganzes_wort() -> Result<(), String> {
        let w = beobachtungsliste()?;
        assert!(bezug("Sapporo", &w).werte.is_empty());
        assert_eq!(bezug("sap-Aktie", &w).markierung(), " [betrifft: SAP]");
        Ok(())
    }

    #[test]
    fn isin_trifft() -> Result<(), String> {
        assert_eq!(
            bezug("DE0007164600", &beobachtungsliste()?).markierung(),
            " [betrifft: SAP]"
        );
        Ok(())
    }
}
