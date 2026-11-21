//! Schätzung aus bestätigten Abrechnungen des KI-Kassenbuchs.
use depot_journal::{KiAufrufErgebnis, KiStand, KiZweck};
use depot_kern::modelle::{Denktiefe, Modellwahl};
use depot_kern::{BeraterId, Cent};

/// Messwerte aus bestätigten Abrechnungen für dasselbe Anbieterprofil.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AufrufkostenSchaetzung {
    pub anzahl: usize,
    pub gesamt: Cent,
}

impl AufrufkostenSchaetzung {
    /// Rundet erst den Betrag für alle geplanten Aufrufe auf ganze Cent.
    pub fn fuer_aufrufe(self, anzahl: u64) -> Option<Cent> {
        let stichprobe = u128::try_from(self.anzahl).ok()?;
        let gesamt = u128::try_from(self.gesamt.0).ok()?;
        let produkt = gesamt.checked_mul(u128::from(anzahl))?;
        let gerundet = produkt.checked_add(stichprobe.checked_div(2)?)?;
        i64::try_from(gerundet.checked_div(stichprobe)?)
            .ok()
            .map(Cent)
    }
}

/// Verwendet je Buchung den bestätigten Betrag. Eine leere Zweckliste
/// berücksichtigt alle Einsätze des Beraters. Ohne angegebene Denktiefe
/// fließen ausschließlich ältere Belege mit unbelegter Denkstufe ein.
pub fn schaetze(
    stand: &KiStand,
    berater: BeraterId,
    anbieter: &str,
    modell: &str,
    denktiefe: Option<Denktiefe>,
    zwecke: &[KiZweck],
) -> Option<AufrufkostenSchaetzung> {
    let mut anzahl = 0_usize;
    let mut gesamt = Cent::NULL;
    for (nr, buchung) in stand.buchungen().iter().enumerate() {
        if buchung.berater != berater {
            continue;
        }
        let wahl = stand.modellwahl(nr);
        if wahl.map(Modellwahl::denktiefe) != denktiefe {
            continue;
        }
        let Some((zweck, true)) = stand.buchungsbeleg(nr) else {
            continue;
        };
        if !zwecke.is_empty() && !zwecke.contains(&zweck) {
            continue;
        }
        if stand.aufruf(nr).is_some_and(|a| {
            a.anbieter == anbieter
                && wahl.map_or_else(
                    || a.modell == modell,
                    |wahl| {
                        wahl.modell().kennung() == modell && wahl.modell().passt_meldung(&a.modell)
                    },
                )
                && a.ergebnis != KiAufrufErgebnis::NichtGesendet
        }) {
            anzahl = anzahl.checked_add(1)?;
            gesamt = gesamt.checked_add(buchung.reserviert)?;
        }
    }
    (anzahl > 0).then_some(AufrufkostenSchaetzung { anzahl, gesamt })
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_journal::{Art, Journal, inhalt_fuer, vermerk_fuer};
    use std::{fs, path::Path};

    #[test]
    fn mittelt_belegte_zentbetraege_und_rundet_nur_einmal() {
        let schaetzung = AufrufkostenSchaetzung {
            anzahl: 2,
            gesamt: Cent(5),
        };
        assert_eq!(schaetzung.fuer_aufrufe(0), Some(Cent::NULL));
        assert_eq!(schaetzung.fuer_aufrufe(1), Some(Cent(3)));
        assert_eq!(schaetzung.fuer_aufrufe(8), Some(Cent(20)));
        assert!(schaetzung.fuer_aufrufe(u64::MAX).is_none());
    }

    fn aufruf(nr: usize, modell: &str, gesendet: bool) -> String {
        let vorlage = "a".repeat(64);
        if gesendet {
            let frage = "b".repeat(64);
            let antwort = "c".repeat(64);
            format!("{nr};openai;{modell};{vorlage};{frage};{antwort};20;10;beantwortet")
        } else {
            format!("{nr};openai;{modell};{vorlage};-;-;-;-;nicht_gesendet")
        }
    }

    fn testjournal(pfad: &Path) -> Result<Journal, String> {
        let bytes = inhalt_fuer(&[
            (Art::KiGrenzen, &"0".repeat(64)),
            (Art::Ki, "Maus;2026-11-20;1;25"),
            (Art::KiAufruf, &aufruf(0, "modell-a", true)),
            (Art::KiAbrechnung, "0;25;2"),
            (Art::Ki, "Maus;2026-11-20;2;31;vorbereitung"),
            (Art::KiAufruf, &aufruf(1, "modell-a", true)),
            (Art::KiAbrechnung, "1;31;3"),
            (Art::Ki, "Maus;2026-11-20;3;99"),
            (Art::KiAufruf, &aufruf(2, "modell-a", true)),
            (Art::Ki, "Maus;2026-11-20;4;99"),
            (Art::KiAufruf, &aufruf(3, "modell-a", false)),
            (Art::KiAbrechnung, "3;99;0"),
            (Art::Ki, "Dachs;2026-11-20;5;99"),
            (Art::KiAufruf, &aufruf(4, "modell-a", true)),
            (Art::KiAbrechnung, "4;99;5"),
            (Art::Ki, "Maus;2026-11-20;6;99"),
            (Art::KiAufruf, &aufruf(5, "modell-b", true)),
            (Art::KiAbrechnung, "5;99;7"),
            (Art::Ki, "Maus;2026-11-20;7;99"),
            (Art::KiAbrechnung, "6;99;9"),
        ]);
        fs::write(pfad, &bytes).map_err(|e| e.to_string())?;
        let vermerk = vermerk_fuer(&bytes, 0).ok_or("Vermerk fehlt")?;
        fs::write(pfad.with_extension("vermerk"), vermerk).map_err(|e| e.to_string())?;
        Journal::oeffnen(pfad).map_err(|e| format!("{e:?}"))
    }

    #[test]
    fn verwendet_nur_zuordenbare_abrechnungen_des_gewaehlten_modells() -> Result<(), String> {
        let ordner = std::env::temp_dir().join(format!("depot-prognose-{}", std::process::id()));
        fs::create_dir(&ordner).map_err(|e| e.to_string())?;
        let ergebnis = (|| {
            let journal = testjournal(&ordner.join("journal.txt"))?;
            let ki = journal.stand().ki();
            let berater = BeraterId::Maus;
            assert_eq!(
                schaetze(ki, berater, "openai", "modell-a", None, &[]),
                Some(AufrufkostenSchaetzung {
                    anzahl: 2,
                    gesamt: Cent(5),
                })
            );
            assert_eq!(
                schaetze(
                    ki,
                    berater,
                    "openai",
                    "modell-a",
                    None,
                    &[KiZweck::Vorbereitung]
                ),
                Some(AufrufkostenSchaetzung {
                    anzahl: 1,
                    gesamt: Cent(3),
                })
            );
            assert_eq!(schaetze(ki, berater, "openai", "modell-c", None, &[]), None);
            assert_eq!(
                schaetze(ki, berater, "anthropic", "modell-a", None, &[]),
                None
            );
            Ok(())
        })();
        let _ = fs::remove_dir_all(ordner);
        ergebnis
    }
}
