//! Gleichmäßige Monatsverteilung aus bestätigten Buchungen, ohne Uhr oder Dateien.
use crate::{Buchung, KiGrenzen, Sperre};
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
        .checked_div(i64::from(handelstage_rest(datum).max(1)))
        .ok_or(Sperre::Rechenfehler)?;
    Ok(Cent(rahmen.saturating_sub(heute).max(0)))
}
