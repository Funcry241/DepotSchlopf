//! Vorab festgelegte Messlatte für den Papierhandel.

use crate::Cent;

pub const MINDEST_TAGE: usize = 60;
pub const MINDEST_VORSPRUNG_BP: i64 = 0;
pub const HOECHSTER_RUECKGANG_BP: i64 = 1_500;
pub const HOECHSTER_AUSFALL_BP: i64 = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ergebnis {
    Traegt,
    NochNicht,
    Verfehlt,
    NichtBewertbar,
}

pub fn bewertung(
    tage: usize,
    vorsprung: Option<i64>,
    rueckgang: Option<i64>,
    ausfall: Option<i64>,
) -> Ergebnis {
    let (Some(v), Some(r), Some(a)) = (vorsprung, rueckgang, ausfall) else {
        return Ergebnis::NichtBewertbar;
    };
    if v < MINDEST_VORSPRUNG_BP || r >= HOECHSTER_RUECKGANG_BP || a > HOECHSTER_AUSFALL_BP {
        Ergebnis::Verfehlt
    } else if tage < MINDEST_TAGE {
        Ergebnis::NochNicht
    } else {
        Ergebnis::Traegt
    }
}

/// Größter Rückgang mit Startgeld als erstem Höchststand und Aufrundung in Basispunkten.
pub fn rueckgang_bp(verlauf: &[Option<Cent>], start: Cent) -> Option<i64> {
    let fehlt = verlauf.iter().filter(|w| w.is_none()).count();
    if verlauf.is_empty() || fehlt.saturating_mul(10) > verlauf.len() || start.0 <= 0 {
        return None;
    }
    let mut hoch = start;
    let mut max = 0;
    for wert in verlauf.iter().flatten().copied() {
        hoch = hoch.max(wert);
        let nenner = i128::from(hoch.0);
        let zaehler = i128::from(hoch.0)
            .checked_sub(i128::from(wert.0))?
            .checked_mul(10_000)?;
        let bp = zaehler
            .checked_add(nenner.checked_sub(1)?)?
            .checked_div(nenner)?;
        max = max.max(i64::try_from(bp).ok()?);
    }
    Some(max)
}

/// Anteil ausgefallener und begonnener Tage an allen Handelstagen, in Basispunkten.
pub fn ausfall_bp(ausfaelle: usize, tage: usize) -> Option<i64> {
    let zaehler = ausfaelle.checked_mul(10_000)?;
    let bp = zaehler
        .checked_add(tage.checked_sub(1)?)?
        .checked_div(tage)?;
    i64::try_from(bp).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m3_rueckgang_1500() {
        let v = [500_000, 520_000, 442_000, 480_000].map(|n| Some(Cent(n)));
        let r = rueckgang_bp(&v, Cent(500_000));
        assert_eq!(r, Some(1_500));
        assert_eq!(bewertung(60, Some(0), r, Some(0)), Ergebnis::Verfehlt);
    }

    #[test]
    fn m3_rueckgang_1499() {
        let v = [500_000, 520_000, 442_100, 480_000].map(|n| Some(Cent(n)));
        let r = rueckgang_bp(&v, Cent(500_000));
        assert_eq!(r, Some(1_499));
        assert_eq!(bewertung(60, Some(0), r, Some(0)), Ergebnis::Traegt);
    }

    #[test]
    fn rueckgang_zaehlt_startgeld_als_hoechststand() {
        assert_eq!(
            rueckgang_bp(&[Some(Cent(400_000))], Cent(500_000)),
            Some(2_000)
        );
    }

    #[test]
    fn zu_viele_unbewertbare_tage() {
        let mut v = vec![Some(Cent(500_000)); 17];
        v.extend([None; 3]);
        assert_eq!(rueckgang_bp(&v, Cent(500_000)), None);
    }

    #[test]
    fn m4_ausfaelle_500() {
        assert_eq!(ausfall_bp(5, 100), Some(500));
        assert_eq!(
            bewertung(100, Some(0), Some(0), ausfall_bp(5, 100)),
            Ergebnis::Traegt
        );
    }

    #[test]
    fn m4_ausfaelle_501() {
        assert_eq!(ausfall_bp(51, 1000), Some(510));
        assert_eq!(ausfall_bp(51, 1019), Some(501));
        assert_eq!(
            bewertung(100, Some(0), Some(0), ausfall_bp(51, 1019)),
            Ergebnis::Verfehlt
        );
    }

    #[test]
    fn m1_zu_frueh() {
        assert_eq!(
            bewertung(59, Some(0), Some(0), Some(0)),
            Ergebnis::NochNicht
        );
    }

    #[test]
    fn m1_genau() {
        assert_eq!(bewertung(60, Some(0), Some(0), Some(0)), Ergebnis::Traegt);
    }

    #[test]
    fn vorsprung_null_reicht() {
        assert_eq!(bewertung(60, Some(0), Some(0), Some(0)), Ergebnis::Traegt);
        assert_eq!(
            bewertung(60, Some(-1), Some(0), Some(0)),
            Ergebnis::Verfehlt
        );
    }
}
