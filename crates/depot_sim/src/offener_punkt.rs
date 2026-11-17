<<<<<<< HEAD
//! Abstimmung nur bei offenem Punkt (#312-1, P35).
=======
//! Abstimmung nur bei offenem Punkt (#312-1, P35). Vorarbeit für Maus, noch nicht eingebaut.
>>>>>>> origin/main
//!
//! Die Abstimmung kostet bis zu vier bezahlte Aufrufe. Sie lohnt nur, wenn nach Erst- und
//! Zweitmeinung noch etwas offen ist: Für einen Topf weicht der gültige Vorschlag vom ersten ab
//! (Veto, Teilveto, Rückzug), oder ein Ersatzkauf nach technischem Ausfall der Zweitmeinung
//! wartet auf Bestätigung. Sonst entfällt sie, auch wenn beide Töpfe Bargeld sagen.
//!
<<<<<<< HEAD
//! Eingebaut in `abstimmung::berate` gleich nach den Frühausstiegen.
=======
//! Einbau: In `abstimmung::berate` nach den Frühausstiegen
//! `if !offener_punkt([(ea.vorschlag.as_ref(), gueltig[0].vorschlag.as_ref(), gueltig[0].ersatzkauf_offen()), …])`
//! journalisiert „Abstimmung entfällt: kein offener Punkt“ und gibt `gueltig` ohne Reservierung zurück.
>>>>>>> origin/main

use depot_kern::Vorschlag;

/// Ein Topf nach der Zweitmeinung: erster Vorschlag, gültiger Vorschlag, Ersatzkauf offen.
pub type TopfNachZweitmeinung<'a> = (Option<&'a Vorschlag>, Option<&'a Vorschlag>, bool);

/// `true`, wenn die Abstimmung für mindestens einen Topf etwas klären muss.
#[must_use]
pub fn offener_punkt(toepfe: [TopfNachZweitmeinung<'_>; 2]) -> bool {
    toepfe.iter().any(|(erste, gueltig, ersatzkauf)| {
        *ersatzkauf || !crate::abstimmung::gleiche_ziele(*erste, *gueltig)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::teilveto::{Urteilsfolge, urteilsfolge};
    use depot_kern::{BeraterId, Isin};
    use std::collections::BTreeMap;

    fn isin(s: &str) -> Result<Isin, String> {
        Isin::neu(s).map_err(|e| format!("{e:?}"))
    }

    fn vorschlag(berater: BeraterId, werte: &[&str]) -> Result<Vorschlag, String> {
        let mut v = Vorschlag {
            erwartung_bp: BTreeMap::new(),
            limit_bp: BTreeMap::new(),
            stopp_bp: BTreeMap::new(),
            berater,
            zielgewichte_bp: BTreeMap::new(),
            begruendung: BTreeMap::new(),
            park: None,
        };
        for s in werte {
            v.zielgewichte_bp.insert(isin(s)?, 2_000);
            v.begruendung.insert(isin(s)?, "beobachtet".into());
        }
        Ok(v)
    }

    /// `abstimmung_entfaellt_bei_zwei_zustimmungen`: beide Zweitmeinungen stimmen zu.
    #[test]
    fn entfaellt_bei_zwei_zustimmungen() -> Result<(), String> {
        let a = vorschlag(BeraterId::Dachs, &["DE0007164600"])?;
        let b = vorschlag(BeraterId::Maus, &["DE0008404005"])?;
        assert!(!offener_punkt([
            (Some(&a), Some(&a), false),
            (Some(&b), Some(&b), false)
        ]));
        Ok(())
    }

    /// `abstimmung_entfaellt_bei_beidseitigem_bargeld`: beide `ziele: null`.
    #[test]
    fn entfaellt_bei_beidseitigem_bargeld() {
        assert!(!offener_punkt([(None, None, false), (None, None, false)]));
    }

    /// Eine Seite Bargeld, die andere mit Zustimmung: ebenfalls nichts offen.
    #[test]
    fn entfaellt_bei_bargeld_und_zustimmung() -> Result<(), String> {
        let b = vorschlag(BeraterId::Maus, &["DE0008404005"])?;
        assert!(!offener_punkt([
            (None, None, false),
            (Some(&b), Some(&b), false)
        ]));
        Ok(())
    }

    /// `abstimmung_laeuft_bei_veto`: Veto gegen Topf B, der gültige Vorschlag ist weg.
    #[test]
    fn laeuft_bei_veto() -> Result<(), String> {
        let a = vorschlag(BeraterId::Dachs, &["DE0007164600"])?;
        let b = vorschlag(BeraterId::Maus, &["DE0008404005"])?;
        assert!(offener_punkt([
            (Some(&a), Some(&a), false),
            (Some(&b), None, false)
        ]));
        Ok(())
    }

    /// `abstimmung_laeuft_bei_ersatzkauf`: Zweitmeinung ausgefallen, Ersatzkauf offen.
    #[test]
    fn laeuft_bei_ersatzkauf() -> Result<(), String> {
        let a = vorschlag(BeraterId::Dachs, &["DE0007164600"])?;
        assert!(offener_punkt([
            (Some(&a), Some(&a), true),
            (None, None, false)
        ]));
        Ok(())
    }

    /// `teilveto_startet_abstimmung` (P43): Der Rest nach dem Teilveto ist ein anderer
    /// Vorschlag als der erste, also bleibt ein Punkt offen.
    #[test]
    fn teilveto_startet_abstimmung() -> Result<(), String> {
        let a = vorschlag(
            BeraterId::Dachs,
            &["DE0007164600", "DE0008404005", "DE000BASF111"],
        )?;
        let Urteilsfolge::Teilveto {
            vorschlag: rest, ..
        } = urteilsfolge(&a, false, Some(&[isin("DE000BASF111")?]))
        else {
            return Err("Teilveto erwartet".into());
        };
        assert!(offener_punkt([
            (Some(&a), Some(&rest), false),
            (None, None, false)
        ]));
        Ok(())
    }

    /// Ein Wechsel nur bei Limit oder Stopp ist ebenfalls offen (wie `gleiche_ziele`).
    #[test]
    fn laeuft_bei_geaendertem_stopp() -> Result<(), String> {
        let a = vorschlag(BeraterId::Dachs, &["DE0007164600"])?;
        let mut g = a.clone();
        g.stopp_bp.insert(isin("DE0007164600")?, 400);
        assert!(offener_punkt([
            (Some(&a), Some(&g), false),
            (None, None, false)
        ]));
        Ok(())
    }
}
