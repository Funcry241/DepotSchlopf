//! Taeglicher Waechter: Tagesverlust und Notbremse, und die Verkaufsauftraege der Notbremse.

use depot_kern::{BP_GANZ, Cent, Grenzen, Kurse, Rundung, Topf};

use crate::auftrag::{Anlass, Auftrag, GenehmigterAuftrag, Kennungsvergabe, RegelBeleg, Richtung};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaechterBefund {
    InOrdnung,
    /// Halt bis zur Freigabe; Positionen bleiben.
    Tagesverlust {
        verlust_bp: i64,
    },
    /// Alles verkaufen, dann Halt.
    Notbremse {
        rueckgang_bp: i64,
    },
    /// Werte nicht auswertbar: wie Halt behandeln.
    Unplausibel,
}

/// Bewertet das Gesamtdepot. Die Notbremse hat Vorrang vor dem Tagesverlust.
pub fn waechter(
    grenzen: &Grenzen,
    wert_heute: Cent,
    wert_gestern: Cent,
    hoechststand: Cent,
) -> WaechterBefund {
    if wert_heute.0 < 0 || wert_gestern.0 <= 0 || hoechststand.0 <= 0 {
        return WaechterBefund::Unplausibel;
    }
    let rueckgang = hoechststand
        .checked_sub(wert_heute)
        .and_then(|d| d.in_bp_von(hoechststand));
    let verlust = wert_gestern
        .checked_sub(wert_heute)
        .and_then(|d| d.in_bp_von(wert_gestern));
    let (Some(rueckgang_bp), Some(verlust_bp)) = (rueckgang, verlust) else {
        return WaechterBefund::Unplausibel;
    };
    if rueckgang_bp >= grenzen.notbremse_bp() {
        WaechterBefund::Notbremse { rueckgang_bp }
    } else if verlust_bp >= grenzen.tagesverlust_bp() {
        WaechterBefund::Tagesverlust { verlust_bp }
    } else {
        WaechterBefund::InOrdnung
    }
}

/// Verkauft alle Positionen eines Topfes mit Limit. Mindestorder, Orderzahl und Umschlag gelten
/// hier nicht (R36). Die Kennungen kommen aus der gemeinsamen Vergabe (R24).
/// `None`, wenn ein Kurs fehlt, aelter als das Hoechstalter ist oder eine Rechnung ueberlaeuft:
/// Dann bleibt es beim Halt, ohne Verkauf.
pub fn notbremse_auftraege(
    grenzen: &Grenzen,
    topf: &Topf,
    kurse: &Kurse,
    vergabe: &mut Kennungsvergabe,
) -> Option<Vec<GenehmigterAuftrag>> {
    let abdruck = grenzen.fingerabdruck();
    // Erst alle Kurse und Limits pruefen, dann alle Kennungen auf einmal vergeben: Ein Abbruch
    // verbraucht keine Nummer (K9-03).
    let mut geplant = Vec::new();
    for (isin, stueck) in topf.positionen() {
        if *stueck == 0 {
            continue;
        }
        let kurs = crate::entscheidung::frischer_kurs(grenzen, kurse, isin)?;
        // Mindestpreis, aufgerundet: nie billiger als Kurs und Limit-Abstand vorsehen.
        let limit = kurs.mal_bp(
            BP_GANZ.checked_sub(grenzen.limit_abstand_bp())?,
            Rundung::Auf,
        )?;
        geplant.push((isin.clone(), *stueck, limit));
    }
    let kennungen = vergabe.vergib(geplant.len())?;
    let mut auftraege = Vec::new();
    for ((isin, stueck, limit), kennung) in geplant.into_iter().zip(kennungen) {
        auftraege.push(GenehmigterAuftrag::neu(
            Auftrag {
                kennung,
                isin,
                richtung: Richtung::Verkauf,
                stueck,
                limit,
            },
            RegelBeleg {
                grenzen_abdruck: abdruck.clone(),
                anlass: Anlass::Notbremse,
            },
        ));
    }
    Some(auftraege)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g() -> Grenzen {
        Grenzen::standard()
    }

    #[test]
    fn ruhiger_tag() {
        assert_eq!(
            waechter(&g(), Cent(990_000), Cent(1_000_000), Cent(1_020_000)),
            WaechterBefund::InOrdnung
        );
    }

    #[test]
    fn tagesverlust_ab_5_prozent() {
        assert_eq!(
            waechter(&g(), Cent(950_000), Cent(1_000_000), Cent(1_000_000)),
            WaechterBefund::Tagesverlust { verlust_bp: 500 }
        );
    }

    #[test]
    fn notbremse_ab_20_prozent_vom_hoch() {
        assert_eq!(
            waechter(&g(), Cent(800_000), Cent(810_000), Cent(1_000_000)),
            WaechterBefund::Notbremse {
                rueckgang_bp: 2_000
            }
        );
    }

    #[test]
    fn notbremse_vor_tagesverlust() {
        assert!(matches!(
            waechter(&g(), Cent(700_000), Cent(1_000_000), Cent(1_000_000)),
            WaechterBefund::Notbremse { .. }
        ));
    }

    #[test]
    fn unsinnige_werte() {
        assert_eq!(
            waechter(&g(), Cent(100), Cent::NULL, Cent(100)),
            WaechterBefund::Unplausibel
        );
    }
}
