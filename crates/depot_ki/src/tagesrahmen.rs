//! Gleichmäßige Monatsverteilung aus bestätigten Buchungen, ohne Uhr oder Dateien.
use crate::{Buchung, KiDeckel, KiGrenzen, Sperre};
use depot_kern::{BeraterId, Cent, Datum};

/// Verbleibende Handelstage einschließlich heute, auf den Börsen der Positivliste.
pub fn handelstage_rest(datum: Datum) -> u32 {
    (0..31)
        .filter_map(|n| datum.plus_tage(n))
        .filter(|d| d.monat() == datum.monat() && d.jahr() == datum.jahr())
        .filter(|d| depot_kern::liste::handelstag(*d))
        .count()
        .try_into()
        .unwrap_or(0)
}

/// Am Tagesanfang verfügbarer Monatsrest / Resthandelstage, minus heutige Kosten.
/// Abgerechnete Minderkosten werden frei; ungeklärte Reservierungen bleiben voll gebucht.
pub fn tagesrahmen(
    g: &KiGrenzen,
    buchungen: &[Buchung],
    wer: BeraterId,
    datum: Datum,
) -> Result<Cent, Sperre> {
    tagesanteil(
        g,
        buchungen,
        wer,
        datum,
        handelstage_rest(datum).max(1),
        None,
    )
}

/// Börsenfreie Vorbereitung teilt den Monatsrest mit den restlichen Handelstagen.
/// Nach dem letzten Handelstag reicht der Tagesrahmen höchstens für einen Kurzaufruf.
pub fn vorbereitungsrahmen(
    g: &KiGrenzen,
    buchungen: &[Buchung],
    wer: BeraterId,
    datum: Datum,
) -> Result<Cent, Sperre> {
    let tage = handelstage_rest(datum);
    let deckel = if tage == 0 {
        Some(
            KiDeckel::kurz(wer)
                .hoechstkosten()
                .ok_or(Sperre::Rechenfehler)?,
        )
    } else {
        None
    };
    tagesanteil(g, buchungen, wer, datum, tage.saturating_add(1), deckel)
}

fn tagesanteil(
    g: &KiGrenzen,
    buchungen: &[Buchung],
    wer: BeraterId,
    datum: Datum,
    tage: u32,
    deckel: Option<Cent>,
) -> Result<Cent, Sperre> {
    let eigene = || {
        buchungen.iter().filter(|b| {
            b.berater == wer && b.datum.jahr() == datum.jahr() && b.datum.monat() == datum.monat()
        })
    };
    let summe = |heute| {
        eigene()
            .filter(|b| (b.datum == datum) == heute)
            .try_fold(0_i64, |s, b| s.checked_add(b.reserviert.0))
            .ok_or(Sperre::Rechenfehler)
    };
    let vorher = summe(false)?;
    let heute = summe(true)?;
    let rest = g
        .kosten_je_monat()
        .0
        .checked_sub(vorher)
        .ok_or(Sperre::Rechenfehler)?
        .max(0);
    let rahmen = rest
        .checked_div(i64::from(tage))
        .ok_or(Sperre::Rechenfehler)?;
    let rahmen = deckel.map_or(rahmen, |d| rahmen.min(d.0));
    Ok(Cent(rahmen.saturating_sub(heute).max(0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vorbereitung_teilt_mit_handelstagen_und_hat_einen_deckel_am_monatsende() -> Result<(), Sperre>
    {
        let wer = BeraterId::Maus;
        let g = KiGrenzen::standard();
        let vor = Datum::neu(2026, 11, 28).ok_or(Sperre::Rechenfehler)?;
        let ende = Datum::neu(2026, 10, 31).ok_or(Sperre::Rechenfehler)?;
        assert_eq!(handelstage_rest(vor), 1);
        assert_eq!(handelstage_rest(ende), 0);
        assert_eq!(
            vorbereitungsrahmen(&g, &[], wer, vor)?,
            Cent(
                g.kosten_je_monat()
                    .0
                    .checked_div(2)
                    .ok_or(Sperre::Rechenfehler)?
            )
        );
        let kurz = KiDeckel::kurz(wer)
            .hoechstkosten()
            .ok_or(Sperre::Rechenfehler)?;
        assert_eq!(vorbereitungsrahmen(&g, &[], wer, ende)?, kurz);
        let heute = Buchung {
            berater: wer,
            datum: ende,
            zyklus: 1,
            reserviert: kurz,
        };
        assert_eq!(vorbereitungsrahmen(&g, &[heute], wer, ende)?, Cent::NULL);
        Ok(())
    }
}
