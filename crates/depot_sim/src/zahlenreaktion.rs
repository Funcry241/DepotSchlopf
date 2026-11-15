//! Erste belegte Schlusskursreaktion nach einem datierten Quartalstermin.

use std::collections::BTreeMap;

use depot_journal::{Stand, TerminBeleg};
use depot_kern::termin::{NACH_BOERSENSCHLUSS, QUELLE_ERGEBNISSE, VOR_BOERSENBEGINN};
use depot_kern::{
    Betroffen, Boerse, Datum, Herkunft, Isin, Kurse, Preis, Verlaesslichkeit, Zeitbezug, Zeitpunkt,
};

use crate::positionskauf::ZAHLEN_UEBERRASCHUNG;

const SCHWELLE_BP: u64 = 500;

fn letzter_schluss(wissen: Zeitpunkt) -> Option<Datum> {
    let boerse = Boerse::Nyse;
    let tag = wissen.datum()?;
    if boerse.handelstag(tag) && wissen >= boerse.schluss(tag)? {
        Some(tag)
    } else {
        boerse.bezugstag(tag)
    }
}

fn zeitraum(beleg: &TerminBeleg) -> Option<(Datum, Datum)> {
    let t = beleg.termin();
    let Zeitbezug::Tag(bericht) = t.zeit() else {
        return None;
    };
    let boerse = Boerse::Nyse;
    if !boerse.handelstag(bericht) || t.verlaesslichkeit() == Verlaesslichkeit::Vermutet {
        return None;
    }
    if t.titel().ends_with(VOR_BOERSENBEGINN) {
        Some((boerse.bezugstag(bericht)?, bericht))
    } else if t.titel().ends_with(NACH_BOERSENSCHLUSS) {
        let folgetag = (1..=10)
            .filter_map(|tage| bericht.plus_tage(tage))
            .find(|tag| boerse.handelstag(*tag))?;
        Some((bericht, folgetag))
    } else {
        None
    }
}

fn schlusskurs(stand: &Stand, isin: &Isin, tag: Datum, wissen: Zeitpunkt) -> Option<Preis> {
    let zeit = Boerse::Nyse.schluss(tag)?;
    stand
        .kurse()
        .iter()
        .filter(|b| {
            b.isin() == isin
                && b.abruf() <= wissen
                && b.kurs().zeitpunkt() == zeit
                && b.kurs().herkunft() == Herkunft::Quelle
        })
        .max_by_key(|b| b.abruf())
        .map(|b| b.kurs().preis())
}

fn kapitalereignis(stand: &Stand, isin: &Isin, tag: Datum, wissen: Zeitpunkt) -> bool {
    stand.nachrichten().iter().any(|b| {
        b.quelle() == depot_kern::tiingo::QUELLE_KAPITAL
            && b.abruf() <= wissen
            && b.zeit().datum() == Some(tag)
            && b.text()
                .rsplit_once(") · ")
                .and_then(|(wert, _)| wert.rsplit_once(" ("))
                .is_some_and(|(_, kennung)| kennung == isin.als_str())
    })
}

pub(crate) fn belege(stand: &Stand, kurse: &Kurse, wissen: Zeitpunkt) -> BTreeMap<Isin, String> {
    let Some(ende) = letzter_schluss(wissen) else {
        return BTreeMap::new();
    };
    let termine: BTreeMap<_, _> = stand
        .termine()
        .iter()
        .filter(|b| {
            b.quelle() == QUELLE_ERGEBNISSE
                && b.herkunft() == Herkunft::Quelle
                && b.abruf() <= wissen
        })
        .map(|b| (b.termin().kennung(), b))
        .collect();
    termine
        .into_iter()
        .filter_map(|(kennung, beleg)| {
            let (von, bis) = zeitraum(beleg)?;
            if bis != ende
                || beleg.abruf() > Boerse::Nyse.schluss(bis)?
                || [von, bis].into_iter().any(depot_kern::boerse::target_geschlossen)
            {
                return None;
            }
            let [Betroffen::Wert(isin)] = beleg.termin().betroffen() else {
                return None;
            };
            let kurs = kurse.eintrag(isin)?;
            if kurs.handelstag() != bis
                || kurs.zeitpunkt() != Boerse::Nyse.schluss(bis)?
                || kurs.herkunft() != Herkunft::Quelle
                || depot_kern::liste::boerse_von(isin) != Some(Boerse::Nyse)
                || kapitalereignis(stand, isin, bis, wissen)
            {
                return None;
            }
            let vorher = schlusskurs(stand, isin, von, wissen)?;
            let nachher = schlusskurs(stand, isin, bis, wissen)?;
            if nachher != kurs.preis() {
                return None;
            }
            let bp = nachher.checked_sub(vorher)?.in_bp_von(vorher)?;
            if bp.unsigned_abs() < SCHWELLE_BP {
                return None;
            }
            let betrag = bp.unsigned_abs();
            let zeichen = if bp > 0 { "+" } else { "−" };
            let prozent = format!("{zeichen}{},{:02} %", betrag / 100, betrag % 100);
            Some((
                isin.clone(),
                format!("{ZAHLEN_UEBERRASCHUNG} · {isin} · {prozent} in EUR vom Schluss am {von} bis zum Schluss am {bis} · Kalender {kennung}"),
            ))
        })
        .collect()
}

pub(crate) fn ereignisse(
    stand: &Stand,
    kurse: &Kurse,
    wissen: Zeitpunkt,
    weitere: Vec<String>,
) -> Vec<String> {
    belege(stand, kurse, wissen)
        .into_values()
        .chain(weitere)
        .collect()
}
