//! Mindesteinsatz ohne Nachfrage (#312-3, P37). Vorarbeit für Maus, noch nicht eingebaut.
//!
//! Die Frage nennt `unter_mindesteinsatz` als Pflichtfeld, eine zweite Anfrage gibt es nicht.
//! Fehlt der Grund, trennt der Vermerk, wer den Plan unter das Minimum gebracht hat:
//! - „fehlt“: Die KI wollte selbst weniger einsetzen. Das bleibt ein Versäumnis der KI
//!   (`analyse::pruefbericht::pflichtgrund_fehlt` zählt nur Zeilen, die auf „ · fehlt“ enden).
//! - „fehlt, Regelwerk kürzte“: Die Zielgewichte der KI erreichen das Minimum, das Regelwerk hat
//!   gekürzt (Budget, Anteil, Mindestorder). Das zählt nicht als Versäumnis.
//!
//! Einbau in `lauf::mindesteinsatz`: Nachfrage samt Reservierung `KiZweck::Abstimmung` entfernen,
//! `status` mit [`vermerk`] bilden; `gehalten` über `entscheidung::ist_gewicht_bp`.

use std::collections::BTreeMap;

use depot_kern::{Isin, Vorschlag};

/// Einsatz, den die KI selbst geplant hat, in bp vom Topfwert: genannte Zielgewichte plus die
/// heutigen Gewichte gehaltener Werte, die sie nicht nennt (die bleiben unverändert). Der
/// Parkwert zählt nicht zum Mindesteinsatz (#311 H3, Festlegung 2).
#[must_use]
pub fn ki_einsatz_bp(gehalten: &BTreeMap<Isin, i64>, v: Option<&Vorschlag>) -> i64 {
    let park = v.and_then(|v| v.park.as_ref()).map(|p| &p.isin);
    let genannt = v.map(|v| &v.zielgewichte_bp);
    let mut summe = 0_i64;
    for (isin, ist) in gehalten {
        if Some(isin) != park && !genannt.is_some_and(|g| g.contains_key(isin)) {
            summe = summe.saturating_add(*ist);
        }
    }
    if let Some(g) = genannt {
        for (isin, ziel) in g {
            if Some(isin) != park {
                summe = summe.saturating_add(*ziel);
            }
        }
    }
    summe
}

/// Statusteil der Journalzeile „unter_mindesteinsatz … · <status>“.
#[must_use]
pub fn vermerk(grund: Option<&str>, ki_einsatz_bp: i64, minimum_bp: i64) -> String {
    match grund.map(str::trim).filter(|g| !g.is_empty()) {
        Some(g) => format!("begründet: {g}"),
        None if ki_einsatz_bp >= minimum_bp => "fehlt, Regelwerk kürzte".into(),
        None => "fehlt".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_kern::{BeraterId, Parkwunsch};

    fn isin(s: &str) -> Result<Isin, String> {
        Isin::neu(s).map_err(|e| format!("{e:?}"))
    }

    fn vorschlag(ziele: &[(&str, i64)]) -> Result<Vorschlag, String> {
        let mut v = Vorschlag {
            erwartung_bp: BTreeMap::new(),
            limit_bp: BTreeMap::new(),
            stopp_bp: BTreeMap::new(),
            berater: BeraterId::Dachs,
            zielgewichte_bp: BTreeMap::new(),
            begruendung: BTreeMap::new(),
            park: None,
        };
        for (s, bp) in ziele {
            v.zielgewichte_bp.insert(isin(s)?, *bp);
        }
        Ok(v)
    }

    /// `mindesteinsatz_ohne_feld_fragt_nicht`: Plan unter 30 %, Feld fehlt, KI wollte selbst
    /// weniger. Die Zeile endet auf „ · fehlt“ wie heute.
    #[test]
    fn ohne_feld_fehlt() -> Result<(), String> {
        let v = vorschlag(&[("DE0007164600", 1_000)])?;
        let ki = ki_einsatz_bp(&BTreeMap::new(), Some(&v));
        assert_eq!(ki, 1_000);
        assert_eq!(vermerk(None, ki, 3_000), "fehlt");
        assert_eq!(vermerk(Some("  "), ki, 3_000), "fehlt");
        Ok(())
    }

    /// `mindesteinsatz_mit_feld`: Text übernommen.
    #[test]
    fn mit_feld() {
        assert_eq!(
            vermerk(Some("NVIDIA-Zahlen abwarten"), 0, 3_000),
            "begründet: NVIDIA-Zahlen abwarten"
        );
    }

    /// `mindesteinsatz_regelwerk_kuerzte`: Ziele 40 %, das Regelwerk plant nur 20 %.
    #[test]
    fn regelwerk_kuerzte() -> Result<(), String> {
        let v = vorschlag(&[("DE0007164600", 2_000), ("DE0008404005", 2_000)])?;
        let ki = ki_einsatz_bp(&BTreeMap::new(), Some(&v));
        assert_eq!(ki, 4_000);
        assert_eq!(vermerk(None, ki, 3_000), "fehlt, Regelwerk kürzte");
        Ok(())
    }

    /// Ungenannte gehaltene Werte zählen mit ihrem heutigen Gewicht, genannte mit dem Ziel.
    #[test]
    fn gehaltene_werte_zaehlen() -> Result<(), String> {
        let gehalten = BTreeMap::from([
            (isin("DE0007164600")?, 2_500),
            (isin("DE0008404005")?, 1_000),
        ]);
        let v = vorschlag(&[("DE0008404005", 0)])?;
        assert_eq!(ki_einsatz_bp(&gehalten, Some(&v)), 2_500);
        assert_eq!(ki_einsatz_bp(&gehalten, None), 3_500);
        Ok(())
    }

    /// Der Parkwert füllt den Mindesteinsatz nicht (P46, Festlegung 2).
    #[test]
    fn parkwert_zaehlt_nicht() -> Result<(), String> {
        let etf = isin("IE00B4L5Y983")?;
        let mut v = vorschlag(&[("DE0007164600", 1_000)])?;
        v.park = Some(Parkwunsch {
            isin: etf.clone(),
            reserve_bp: 1_000,
            naechster_kauf_tage: 5,
            begruendung: "parken".into(),
        });
        let gehalten = BTreeMap::from([(etf, 4_000)]);
        let ki = ki_einsatz_bp(&gehalten, Some(&v));
        assert_eq!(ki, 1_000);
        assert_eq!(vermerk(None, ki, 3_000), "fehlt");
        Ok(())
    }
}
