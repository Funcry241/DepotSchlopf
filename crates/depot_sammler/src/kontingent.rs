//! Zählt auch Wiederholungen und Diagnoseversuche über den gemeinsamen Netzweg.

use std::path::Path;

use depot_journal::abrufkontingent::Abrufe;
use depot_kern::Zeitpunkt;

use crate::{Netz, NetzFehler};

#[derive(Debug)]
pub struct KontingentNetz<'a, N> {
    netz: N,
    daten: &'a Path,
    uhr: fn() -> Option<Zeitpunkt>,
    abrufe: Option<Abrufe>,
}

impl<'a, N: Netz> KontingentNetz<'a, N> {
    pub fn neu(netz: N, daten: &'a Path, uhr: fn() -> Option<Zeitpunkt>) -> Self {
        Self {
            netz,
            daten,
            uhr,
            abrufe: None,
        }
    }

    fn alpha(&mut self) -> Result<(&Abrufe, Zeitpunkt), String> {
        let jetzt = (self.uhr)().ok_or("Uhr für das Alpha-Vantage-Kontingent nicht verfügbar")?;
        if self.abrufe.is_none() {
            self.abrufe = Some(Abrufe::sperren(self.daten)?);
        }
        Ok((
            self.abrufe
                .as_ref()
                .ok_or("Alpha-Vantage-Kontingent fehlt")?,
            jetzt,
        ))
    }
}

impl<N: Netz> Netz for KontingentNetz<'_, N> {
    fn pruefe_alpha(&mut self, benoetigt: usize) -> Result<(), String> {
        let (abrufe, jetzt) = self.alpha()?;
        abrufe.pruefe(jetzt, benoetigt)
    }

    fn hole(&mut self, adresse: &str) -> Result<Vec<u8>, NetzFehler> {
        if let Some(query) = adresse.strip_prefix("https://www.alphavantage.co/query?") {
            let kuerzel = query
                .split('&')
                .find_map(|p| p.strip_prefix("symbol="))
                .unwrap_or("kalender");
            let (abrufe, jetzt) = self.alpha().map_err(NetzFehler::endgueltig)?;
            abrufe
                .vermerke(jetzt, kuerzel)
                .map_err(NetzFehler::endgueltig)?;
        }
        self.netz.hole(adresse)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::netz::{Takt, hole_mit_wiederholung};
    use depot_kern::Datum;

    fn zeit() -> Option<Zeitpunkt> {
        Zeitpunkt::frankfurt(Datum::neu(2026, 11, 10)?, 10, 0, 0)
    }

    #[derive(Debug, Default)]
    struct Ersatz {
        aufrufe: usize,
    }

    impl Netz for Ersatz {
        fn hole(&mut self, _: &str) -> Result<Vec<u8>, NetzFehler> {
            self.aufrufe = self.aufrufe.saturating_add(1);
            if self.aufrufe == 1 {
                Err(NetzFehler::voruebergehend("Netzprobe"))
            } else {
                Ok(Vec::new())
            }
        }
    }

    #[test]
    fn fehler_wiederholung_und_kalender_zaehlen_aber_tiingo_nicht() -> Result<(), String> {
        let daten =
            std::env::temp_dir().join(format!("depot-netzkontingent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&daten);
        std::fs::create_dir(&daten).map_err(|e| e.to_string())?;
        let mut netz = KontingentNetz::neu(Ersatz::default(), &daten, zeit);
        let jetzt = zeit().ok_or("Uhr fehlt")?;
        let mut takt = Takt {
            uhr: &mut zeit,
            schlafe: &mut |_| (),
            melde: &mut |_| (),
        };
        let aktie = "https://www.alphavantage.co/query?function=TIME_SERIES_DAILY&symbol=SAP.DEX";
        hole_mit_wiederholung(
            &mut netz,
            aktie,
            &|j| {
                j.sekunden()
                    .checked_add(300)
                    .and_then(Zeitpunkt::aus_sekunden)
            },
            &mut takt,
        )
        .map_err(|e| format!("{e:?}"))?;
        netz.hole("https://www.alphavantage.co/query?function=EARNINGS_CALENDAR")
            .map_err(|e| e.to_string())?;
        netz.hole("https://api.tiingo.com/tiingo/daily/JPM/prices")
            .map_err(|e| e.to_string())?;
        assert_eq!(
            depot_journal::abrufkontingent::stand(&daten, jetzt)?,
            (3, 3)
        );
        assert_eq!(netz.netz.aufrufe, 4);
        assert!(netz.pruefe_alpha(18).is_err());
        for _ in 0..17 {
            netz.hole(aktie).map_err(|e| e.to_string())?;
        }
        assert!(netz.hole(aktie).is_err());
        assert_eq!(netz.netz.aufrufe, 21);
        drop(netz);
        std::fs::remove_dir_all(daten).map_err(|e| e.to_string())?;
        Ok(())
    }
}
