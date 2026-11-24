//! Wann ein automatischer Papiertag eine Erstmeinung braucht.

use depot_journal::{KiZweck, Stand};
use depot_kern::liste::handelstag;
use depot_kern::{Betroffen, Datum, Isin, Preis, Universum, Zeitpunkt};

use crate::{kursverlauf::Verlauf, szenario::Lage};

pub const RUHETAG: &str = "Ruhetag, kein Anlass";
pub const ANLASS: &str = "Anlass heute: ";
const BEWEGUNG_1T_BP: u64 = 300;
const BEWEGUNG_5T_BP: u64 = 600;
const STOPP_ABSTAND_BP: i64 = 200;
const FREIES_BARGELD_BP: i64 = 5_000;
const PFLICHTBLICK_HANDELSTAGE: usize = 5;

fn letzte_erstmeinung(stand: &Stand, lage: &Lage<'_>) -> Option<Datum> {
    let ki = stand.ki();
    ki.buchungen().iter().enumerate().rev().find_map(|(nr, b)| {
        (b.berater == lage.topf.id().berater()
            && b.datum < lage.datum
            && ki
                .buchungsbeleg(nr)
                .is_some_and(|(zweck, _)| zweck == KiZweck::Erstmeinung))
        .then_some(b.datum)
    })
}

fn naechste_handelstage(tag: Datum) -> impl Iterator<Item = Datum> {
    (1..=31)
        .filter_map(move |n| tag.plus_tage(n))
        .filter(|d| handelstag(*d))
}

fn ereignis(
    stand: &Stand,
    universum: &Universum,
    lage: &Lage<'_>,
    wissen: Zeitpunkt,
    letzte: Option<Datum>,
) -> Option<String> {
    let handelbar = universum.isins();
    let relevant = |isin: &Isin| lage.topf.stueck(isin) != 0 || handelbar.contains(isin);
    let seit = letzte.and_then(|tag| stand.wissensstichtag(tag));
    let werte = depot_kern::liste::beobachtungsliste().unwrap_or_default();
    if let Some(wert) = crate::papiertag::nachrichtenbelege_am(stand, lage.datum, wissen)
        .into_iter()
        .filter(|n| n.zeit() <= wissen && seit.is_none_or(|zeit| n.abruf() > zeit))
        .flat_map(|n| crate::nachrichtenbezug::bezug(n.text(), &werte).werte)
        .find(|w| relevant(&w.isin))
    {
        return Some(format!("neue Meldung zu {}", wert.name));
    }
    let naechster = naechste_handelstage(lage.datum).next();
    let termine: std::collections::BTreeMap<_, _> = stand
        .termine_bis_stichtag(lage.datum)
        .unwrap_or_else(|| stand.termine())
        .iter()
        .filter(|b| b.abruf() <= wissen)
        .map(|b| (b.termin().kennung(), b.termin()))
        .collect();
    termine.values().find_map(|t| {
        let datum = t.zeit().tag()?;
        ((datum == lage.datum || Some(datum) == naechster)
            && t.betroffen()
                .iter()
                .any(|b| matches!(b, Betroffen::Wert(i) if relevant(i))))
        .then(|| format!("Termin am {datum}: {}", t.titel()))
    })
}

pub(crate) fn anlass(stand: &Stand, universum: &Universum, lage: &Lage<'_>) -> Option<String> {
    let letzte = letzte_erstmeinung(stand, lage);
    let wissen = stand
        .wissensstichtag(lage.datum)
        .unwrap_or_else(|| lage.kurse.stand());
    if let Some(grund) = ereignis(stand, universum, lage, wissen, letzte) {
        return Some(grund);
    }
    let frei = Preis::aus_cent(lage.kaufbar)
        .zip(lage.topf.wert_exakt(lage.kurse))
        .and_then(|(bar, wert)| bar.in_bp_von(wert))
        .is_some_and(|bp| bp >= FREIES_BARGELD_BP);
    if let Some((isin, _)) = lage.verlauf.iter().find(|(isin, verlauf)| {
        let gehalten = lage.topf.stueck(isin) != 0;
        let sprung = |tage, grenze| {
            let bp = match verlauf {
                Verlauf::Da { t1_bp, t5_bp, .. } => Some(if tage == 1 { *t1_bp } else { *t5_bp }),
                Verlauf::ZuWenig => lage.kurse.eintrag(isin).and_then(|kurs| {
                    crate::kursverlauf::kurzbewegung(stand, isin, kurs, wissen, tage)
                }),
            };
            bp.is_some_and(|bp| bp.unsigned_abs() >= grenze)
        };
        ((gehalten || frei) && sprung(1, BEWEGUNG_1T_BP)) || (gehalten && sprung(5, BEWEGUNG_5T_BP))
    }) {
        return Some(format!("Kursbewegung bei {isin}"));
    }
    if let Some(isin) = lage.topf.positionen().keys().find(|isin| {
        stand
            .stoppkurs(lage.topf.id(), isin)
            .and_then(Preis::aus_cent)
            .zip(lage.kurse.kurs(isin))
            .and_then(|(stopp, kurs)| kurs.checked_sub(stopp)?.in_bp_von(stopp))
            .is_some_and(|bp| bp < STOPP_ABSTAND_BP)
    }) {
        return Some(format!("{isin} nahe am Schutz-Stopp"));
    }
    if letzte.is_none_or(|tag| {
        naechste_handelstage(tag)
            .take_while(|tag| *tag <= lage.datum)
            .take(PFLICHTBLICK_HANDELSTAGE)
            .count()
            == PFLICHTBLICK_HANDELSTAGE
    }) {
        return Some("Pflichtblick".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pflichtblick_zaehlt_handelstage_statt_kalendertage() -> Result<(), String> {
        let montag = Datum::neu(2030, 1, 7).ok_or("Montag")?;
        let freitag = Datum::neu(2030, 1, 11).ok_or("Freitag")?;
        let naechster_montag = Datum::neu(2030, 1, 14).ok_or("Nächster Montag")?;
        let vor = || naechste_handelstage(montag).take(PFLICHTBLICK_HANDELSTAGE);
        assert_eq!(vor().filter(|tag| *tag <= freitag).count(), 4);
        assert_eq!(vor().last(), Some(naechster_montag));
        Ok(())
    }
}
