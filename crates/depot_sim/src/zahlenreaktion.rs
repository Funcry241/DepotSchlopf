//! Belegte Schlusskursreaktion nach einem datierten Quartalstermin, nur für NYSE-Werte.

use std::collections::BTreeMap;

use depot_journal::{KursBeleg, NachrichtBeleg, Stand, TerminBeleg};
use depot_kern::termin::{NACH_BOERSENSCHLUSS, QUELLE_ERGEBNISSE, VOR_BOERSENBEGINN};
use depot_kern::{
    Betroffen, Boerse, Datum, Herkunft, Isin, Kurse, Preis, Verlaesslichkeit, Zeitbezug, Zeitpunkt,
};

use crate::positionskauf::ZAHLEN_UEBERRASCHUNG;

const SCHWELLE_BP: u64 = 500;
const SIGNAL_HANDELSTAGE: usize = 5;

pub(crate) struct Reaktion {
    pub(crate) positiv: bool,
    pub(crate) abstand_bp: i64,
    pub(crate) text: String,
}

fn im_fenster(reaktion: Datum, ende: Datum) -> bool {
    std::iter::successors(Some(ende), |tag| Boerse::Nyse.bezugstag(*tag))
        .take(SIGNAL_HANDELSTAGE)
        .any(|tag| tag == reaktion)
}

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

fn schlusskurs(belege: &[KursBeleg], isin: &Isin, tag: Datum, wissen: Zeitpunkt) -> Option<Preis> {
    let boerse = depot_kern::liste::boerse_von(isin)?;
    if !boerse.handelstag(tag) {
        return None;
    }
    let zeit = boerse.schluss(tag)?;
    belege
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

fn kapitalereignis(belege: &[NachrichtBeleg], isin: &Isin, tag: Datum, wissen: Zeitpunkt) -> bool {
    belege.iter().any(|b| {
        b.quelle() == depot_kern::tiingo::QUELLE_KAPITAL
            && b.abruf() <= wissen
            && b.zeit().datum() == Some(tag)
            && b.text()
                .rsplit_once(") · ")
                .and_then(|(wert, _)| wert.rsplit_once(" ("))
                .is_some_and(|(_, kennung)| kennung == isin.als_str())
    })
}

fn rendite(vorher: Preis, nachher: Preis) -> Option<i64> {
    nachher.checked_sub(vorher)?.in_bp_von(vorher)
}

fn korbrendite(belege: &[KursBeleg], von: Datum, bis: Datum, wissen: Zeitpunkt) -> Option<i64> {
    let werte = depot_kern::liste::VERGLEICH_ISIN;
    let gesamt = werte.iter().try_fold(0_i64, |summe, kennung| {
        let isin = Isin::neu(kennung).ok()?;
        let davor = schlusskurs(belege, &isin, von, wissen)?;
        let danach = schlusskurs(belege, &isin, bis, wissen)?;
        summe.checked_add(rendite(davor, danach)?)
    })?;
    gesamt.checked_div(i64::try_from(werte.len()).ok()?)
}

fn mit_vorzeichen(bp: i64) -> String {
    let betrag = bp.unsigned_abs();
    let zeichen = match bp.cmp(&0) {
        std::cmp::Ordering::Greater => "+",
        std::cmp::Ordering::Less => "−",
        std::cmp::Ordering::Equal => "",
    };
    format!("{zeichen}{},{:02}", betrag / 100, betrag % 100)
}

pub(crate) fn belege(stand: &Stand, kurse: &Kurse, wissen: Zeitpunkt) -> BTreeMap<Isin, Reaktion> {
    let Some(ende) = letzter_schluss(wissen) else {
        return BTreeMap::new();
    };
    let tag = wissen
        .frankfurt_tag()
        .filter(|&tag| stand.wissensstichtag(tag) == Some(wissen));
    let kursbelege = tag
        .and_then(|tag| stand.kurse_bis_stichtag(tag))
        .unwrap_or_else(|| stand.kurse());
    let nachrichten = tag
        .and_then(|tag| stand.nachrichten_bis_stichtag(tag))
        .unwrap_or_else(|| stand.nachrichten());
    let termine: BTreeMap<_, _> = tag
        .and_then(|tag| stand.termine_bis_stichtag(tag))
        .unwrap_or_else(|| stand.termine())
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
            if !im_fenster(bis, ende)
                || beleg.abruf() > Boerse::Nyse.schluss(bis)?
                || [von, bis].into_iter().any(depot_kern::boerse::target_geschlossen)
            {
                return None;
            }
            let [Betroffen::Wert(isin)] = beleg.termin().betroffen() else {
                return None;
            };
            let kurs = kurse.eintrag(isin)?;
            if kurs.handelstag() != ende
                || kurs.zeitpunkt() != Boerse::Nyse.schluss(ende)?
                || kurs.herkunft() != Herkunft::Quelle
                || depot_kern::liste::boerse_von(isin) != Some(Boerse::Nyse)
                || std::iter::successors(Some(ende), |tag| Boerse::Nyse.bezugstag(*tag))
                    .take_while(|tag| *tag >= bis)
                    .any(|tag| kapitalereignis(nachrichten, isin, tag, wissen))
            {
                return None;
            }
            let vorher = schlusskurs(kursbelege, isin, von, wissen)?;
            let nachher = schlusskurs(kursbelege, isin, bis, wissen)?;
            let aktuell = schlusskurs(kursbelege, isin, ende, wissen)?;
            if aktuell != kurs.preis() {
                return None;
            }
            let roh = rendite(vorher, nachher)?;
            let abstand = roh.checked_sub(korbrendite(kursbelege, von, bis, wissen)?)?;
            if abstand.unsigned_abs() < SCHWELLE_BP || roh.signum() != abstand.signum() {
                return None;
            }
            let positiv = abstand > 0;
            let art = if positiv {
                format!("{ZAHLEN_UEBERRASCHUNG} (Kaufanlass)")
            } else {
                "Zahlen-Enttäuschung (Bestand prüfen)".into()
            };
            let roh = mit_vorzeichen(roh);
            let abstand_wert = abstand;
            let abstand = mit_vorzeichen(abstand);
            let seitdem = mit_vorzeichen(rendite(nachher, aktuell)?);
            Some((isin.clone(), Reaktion {
                positiv,
                abstand_bp: abstand_wert,
                text: format!("{art} · {isin} · {roh} % in EUR vom Schluss am {von} bis zum Schluss am {bis} · {abstand} Prozentpunkte gegen den Korb · seitdem {seitdem} % in EUR bis {ende} · Kalender {kennung}"),
            }))
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
        .map(|beleg| beleg.text)
        .chain(weitere)
        .collect()
}
