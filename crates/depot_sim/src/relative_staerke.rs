//! Kursstärke auf denselben belegten Xetra- und NYSE-Handelstagen.

use std::collections::{BTreeMap, BTreeSet};

use depot_journal::Stand;
use depot_kern::liste::VERGLEICH_ISIN;
use depot_kern::{Boerse, Datum, Herkunft, Isin, Kurse, Preis, Universum, Zeitpunkt};

pub const HANDELSTAGE: u8 = 120;
pub const AUSLASSEN: u8 = 20;
pub const RANG_BELEG: &str = "Kursstärke";
pub const RANG20_BELEG: &str = "Kursstärke20";

fn gemeinsamer_tag(tag: Datum) -> bool {
    Boerse::Xetra.handelstag(tag)
        && Boerse::Nyse.handelstag(tag)
        && !depot_kern::boerse::target_geschlossen(tag)
}

fn tage(bis: Datum, dauer: u8, auslassen: u8) -> Option<(Datum, Datum)> {
    let mut ende = bis;
    while !gemeinsamer_tag(ende) {
        ende = ende.plus_tage(-1)?;
    }
    let zurueck = |mut tag: Datum, anzahl| {
        for _ in 0..anzahl {
            tag = tag.plus_tage(-1)?;
            while !gemeinsamer_tag(tag) {
                tag = tag.plus_tage(-1)?;
            }
        }
        Some(tag)
    };
    let ende = zurueck(ende, auslassen)?;
    Some((zurueck(ende, dauer)?, ende))
}

/// Rankt positive und negative Abstände gegen einen gleich gewichteten Kursertrag
/// der festen Vergleichswerte. Fehlende Belege und Splitperioden ergeben keinen Rang.
pub(crate) fn fuer_lage(
    stand: &Stand,
    kurse: &Kurse,
    universum: &Universum,
    lauf_tag: Datum,
) -> Vec<(Isin, i64)> {
    rangfolge(stand, kurse, universum, lauf_tag, HANDELSTAGE, AUSLASSEN)
}

pub(crate) fn kurzrang(
    stand: &Stand,
    kurse: &Kurse,
    universum: &Universum,
    lauf_tag: Datum,
) -> Vec<(Isin, i64)> {
    rangfolge(stand, kurse, universum, lauf_tag, AUSLASSEN, 0)
}

fn rangfolge(
    stand: &Stand,
    kurse: &Kurse,
    universum: &Universum,
    lauf_tag: Datum,
    dauer: u8,
    auslassen: u8,
) -> Vec<(Isin, i64)> {
    let wissen = stand
        .wissensstichtag(lauf_tag)
        .unwrap_or_else(|| kurse.stand());
    let Some((von, bis)) = kurse
        .alle()
        .map(|(_, k)| k.handelstag())
        .max()
        .and_then(|tag| tage(tag, dauer, auslassen))
    else {
        return Vec::new();
    };
    let mut gesperrt: BTreeSet<_> = stand
        .kapitalmassnahmen()
        .iter()
        .filter(|b| (von < b.massnahme.wirksam) && (b.massnahme.wirksam <= bis))
        .map(|b| b.massnahme.isin.clone())
        .collect();
    gesperrt.extend(depot_journal::splitverdacht::aktiv(stand, Some(wissen)).into_keys());
    let mut preise = BTreeMap::<(&Isin, Datum), (Zeitpunkt, Preis)>::new();
    for beleg in stand.kurse().iter().filter(|b| {
        let k = b.kurs();
        b.abruf() <= wissen
            && k.herkunft() == Herkunft::Quelle
            && (k.handelstag() == von || k.handelstag() == bis)
            && !gesperrt.contains(b.isin())
    }) {
        let k = beleg.kurs();
        let eintrag = preise
            .entry((beleg.isin(), k.handelstag()))
            .or_insert((beleg.abruf(), k.preis()));
        if beleg.abruf() >= eintrag.0 {
            *eintrag = (beleg.abruf(), k.preis());
        }
    }
    let rendite = |isin: &Isin| {
        let (_, vorher) = preise.get(&(isin, von))?;
        let (_, nachher) = preise.get(&(isin, bis))?;
        nachher.checked_sub(*vorher)?.in_bp_von(*vorher)
    };
    let vergleichswerte = VERGLEICH_ISIN.iter().map(|i| Isin::neu(i).ok());
    let Some(korb) = vergleichswerte
        .map(|isin| rendite(&isin?))
        .try_fold(0_i64, |summe, bp| summe.checked_add(bp?))
        .and_then(|summe| summe.checked_div(i64::try_from(VERGLEICH_ISIN.len()).ok()?))
    else {
        return Vec::new();
    };
    let mut rang: Vec<_> = universum
        .isins()
        .into_iter()
        .filter_map(|isin| {
            // Ohne einen für die Frage belegten Kurs entsteht kein Kandidatenrang.
            kurse.eintrag(&isin)?;
            Some((isin.clone(), rendite(&isin)?.checked_sub(korb)?))
        })
        .collect();
    rang.sort_by(|(a, x), (b, y)| y.cmp(x).then_with(|| a.cmp(b)));
    rang
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_journal::KursBeleg;
    use depot_kern::{Cent, Kurs};

    #[test]
    fn rang_nutzt_gemeinsame_stichtage_und_blendet_unbelegte_werte_aus() -> Result<(), String> {
        let datum = Datum::neu(2026, 11, 13).ok_or("Datum")?;
        let (von, bis) = tage(datum, HANDELSTAGE, AUSLASSEN).ok_or("Zeitraum")?;
        let erst =
            Isin::neu(VERGLEICH_ISIN.first().ok_or("Korb")?).map_err(|e| format!("{e:?}"))?;
        let nvda = Isin::neu("US67066G1040").map_err(|e| format!("{e:?}"))?;
        let werte = depot_kern::liste::beobachtungsliste()?;
        let universum = depot_kern::liste::als_universum(&werte);
        let ordner = std::env::temp_dir().join(format!("depot-rang-{}", std::process::id()));
        std::fs::create_dir(&ordner).map_err(|e| e.to_string())?;
        let probe = (|| -> Result<(), String> {
            let pfad = ordner.join("journal.txt");
            depot_journal::Journal::einrichten(&pfad).map_err(|e| format!("{e:?}"))?;
            let mut journal =
                depot_journal::Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
            let ab = Zeitpunkt::neu(von, 22, 0, 0).ok_or("Startzeit")?;
            let ende = Zeitpunkt::neu(bis, 22, 0, 0).ok_or("Endzeit")?;
            let mut aktuell = Vec::new();
            let heute = Zeitpunkt::neu(datum, 22, 0, 0).ok_or("Heute")?;
            for (tag, zeit) in [(von, ab), (bis, ende), (datum, heute)] {
                for roh_isin in VERGLEICH_ISIN.into_iter().chain(["US67066G1040"]) {
                    let isin = Isin::neu(roh_isin).map_err(|e| format!("{e:?}"))?;
                    if isin == nvda && tag == von {
                        continue;
                    }
                    let preis = if tag == bis && isin == erst {
                        12_000
                    } else if tag == datum && isin == erst {
                        9_000
                    } else {
                        10_000
                    };
                    let kurs = Kurs::neu(Cent(preis), "EUR", tag, zeit, Herkunft::Quelle)
                        .map_err(|e| format!("{e:?}"))?;
                    let beleg = KursBeleg::neu(
                        "kursprobe",
                        zeit,
                        b"Kursprobe",
                        isin.clone(),
                        kurs.clone(),
                    )?;
                    journal.belege_kurs(&beleg).map_err(|e| format!("{e:?}"))?;
                    if tag == datum {
                        aktuell.push((isin, kurs));
                    }
                }
            }
            let kurse = Kurse::neu(heute, aktuell).map_err(|e| format!("{e:?}"))?;
            let rang = fuer_lage(journal.stand(), &kurse, &universum, datum);
            assert_eq!(rang.first(), Some(&(erst.clone(), 1867)));
            let kurz = kurzrang(journal.stand(), &kurse, &universum, datum);
            assert_eq!(kurz.last().map(|(isin, _)| isin), Some(&erst));
            assert!(!rang.iter().any(|(i, _)| *i == nvda));
            Ok(())
        })();
        std::fs::remove_dir_all(&ordner).map_err(|e| e.to_string())?;
        probe
    }
}
