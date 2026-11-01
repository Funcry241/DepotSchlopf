//! Taeglicher Waechter: Tagesverlust und Notbremse, und die Verkaufsauftraege der Notbremse.

use depot_kern::{Cent, Grenzen, Kurse, Topf};

use crate::auftrag::{Anlass, Auftrag, AuftragsKennung, GenehmigterAuftrag, RegelBeleg, Richtung};

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
    if rueckgang_bp >= grenzen.notbremse_bp {
        WaechterBefund::Notbremse { rueckgang_bp }
    } else if verlust_bp >= grenzen.tagesverlust_bp {
        WaechterBefund::Tagesverlust { verlust_bp }
    } else {
        WaechterBefund::InOrdnung
    }
}

/// Verkauft alle Positionen eines Topfes mit Limit. Mindestorder gilt hier nicht.
/// `None`, wenn ein Kurs fehlt oder eine Rechnung ueberlaeuft (dann bleibt es beim Halt).
pub fn notbremse_auftraege(
    grenzen: &Grenzen,
    topf: &Topf,
    kurse: &Kurse,
    zyklus: u32,
) -> Option<Vec<GenehmigterAuftrag>> {
    let abdruck = grenzen.fingerabdruck();
    let mut auftraege = Vec::new();
    let mut nr: u8 = 0;
    for (isin, stueck) in &topf.positionen {
        if *stueck == 0 {
            continue;
        }
        let kurs = kurse.kurs(isin)?;
        let limit = kurs.checked_sub(kurs.anteil_bp_ab(grenzen.limit_abstand_bp)?)?;
        nr = nr.checked_add(1)?;
        auftraege.push(GenehmigterAuftrag::neu(
            Auftrag {
                kennung: AuftragsKennung {
                    zyklus,
                    topf: topf.id,
                    nr,
                },
                isin: isin.clone(),
                richtung: Richtung::Verkauf,
                stueck: *stueck,
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
            waechter(
                &g(),
                Cent::euro(9_900),
                Cent::euro(10_000),
                Cent::euro(10_200)
            ),
            WaechterBefund::InOrdnung
        );
    }

    #[test]
    fn tagesverlust_ab_5_prozent() {
        assert_eq!(
            waechter(
                &g(),
                Cent::euro(9_500),
                Cent::euro(10_000),
                Cent::euro(10_000)
            ),
            WaechterBefund::Tagesverlust { verlust_bp: 500 }
        );
    }

    #[test]
    fn notbremse_ab_20_prozent_vom_hoch() {
        assert_eq!(
            waechter(
                &g(),
                Cent::euro(8_000),
                Cent::euro(8_100),
                Cent::euro(10_000)
            ),
            WaechterBefund::Notbremse {
                rueckgang_bp: 2_000
            }
        );
    }

    #[test]
    fn notbremse_vor_tagesverlust() {
        assert!(matches!(
            waechter(
                &g(),
                Cent::euro(7_000),
                Cent::euro(10_000),
                Cent::euro(10_000)
            ),
            WaechterBefund::Notbremse { .. }
        ));
    }

    #[test]
    fn unsinnige_werte() {
        assert_eq!(
            waechter(&g(), Cent::euro(1), Cent::NULL, Cent::euro(1)),
            WaechterBefund::Unplausibel
        );
    }
}
