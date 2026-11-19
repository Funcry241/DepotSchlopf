//! Historische Vergleichsmaßstäbe für den Analyse-Export.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use depot_journal::Stand;
use depot_kern::messlatte::ibkr::LANGFRIST;
use depot_kern::{Cent, Datum};
use depot_lage::vergleich::{eunl, rendite_bp};

use super::{geld, prozent, textdatei};

const ESTR_SERIE: &str = "EST.B.EU000A2QQF08.CI";

fn indexwert(text: &str) -> Option<i128> {
    let (ganz, nachkomma) = text.split_once('.').unwrap_or((text, ""));
    if ganz.is_empty()
        || nachkomma.len() > 8
        || !ganz.bytes().chain(nachkomma.bytes()).all(|b| b.is_ascii_digit())
    {
        return None;
    }
    format!("{ganz}{nachkomma:0<8}")
        .parse::<i128>()
        .ok()
        .filter(|n| *n > 0)
}

fn indexreihe(csv: &str) -> Option<BTreeMap<Datum, i128>> {
    let mut zeilen = csv.trim_start_matches('\u{feff}').lines();
    let felder: Vec<_> = zeilen.next()?.split(',').collect();
    let spalte = |name| {
        let mut treffer = felder
            .iter()
            .enumerate()
            .filter(|(_, s)| **s == name);
        let (nr, _) = treffer.next()?;
        treffer.next().is_none().then_some(nr)
    };
    let serie = spalte("KEY")?;
    let tag = spalte("TIME_PERIOD")?;
    let wert = spalte("OBS_VALUE")?;
    let mut reihe = BTreeMap::new();
    for zeile in zeilen.filter(|z| !z.trim().is_empty()) {
        let felder: Vec<_> = zeile.split(',').collect();
        if *felder.get(serie)? != ESTR_SERIE {
            return None;
        }
        let d = Datum::lies(felder.get(tag)?)?;
        let n = indexwert(felder.get(wert)?)?;
        if reihe.insert(d, n).is_some() {
            return None;
        }
    }
    Some(reihe)
}

fn estr(csv: &str, von: Datum, bis: Datum) -> Option<Cent> {
    let reihe = indexreihe(csv)?;
    let vorher = *reihe.get(&von)?;
    let nachher = *reihe.get(&bis)?;
    let cent = i128::from(LANGFRIST.startkapital.0)
        .checked_mul(nachher)?
        .checked_div(vorher)?;
    i64::try_from(cent).ok().map(Cent)
}

pub(super) fn bericht(daten: &Path, stand: &Stand, werte: &BTreeMap<Datum, Cent>) -> String {
    let mut text = String::from("\n### EUNL und €STR\n\n");
    let Some((&von, &bis)) = werte
        .first_key_value()
        .zip(werte.last_key_value())
        .map(|((von, _), (bis, _))| (von, bis))
        .filter(|(von, bis)| von < bis)
    else {
        text.push_str("Zwei belegte Depotabschlüsse für den Vergleich fehlen.\n");
        return text;
    };
    let etf = eunl(stand, von, bis);
    let geldmarkt = textdatei(&daten.join("estr-index.csv"))
        .ok()
        .flatten()
        .and_then(|csv| estr(&csv, von, bis));
    let render = |wert: Option<Cent>| {
        format!(
            "{} | {}",
            geld(wert),
            prozent(wert.and_then(|w| rendite_bp(LANGFRIST.startkapital, w)))
        )
    };
    let _ = write!(
        text,
        "Modellrechnung mit 1.000 € Startkapital vom ersten bis zum letzten belegten Depotabschluss im Exportzeitraum ({von} bis {bis}). Alle Werte vor Steuern.\n\n| Maßstab | Modell-Endwert | Rendite |\n|---|---:|---:|\n| EUNL kaufen und halten | {} |\n| €STR (EZB-Zinsindex) | {} |\n\n",
        render(etf),
        render(geldmarkt)
    );
    text.push_str("EUNL verwendet belegte Kurse zu den Depotstichtagen, ganze Stücke, Restbargeld, Xetra-Modellspread und die IBKR-Höchstgebühr beim Kauf und beim modellierten Verkauf zum Enddatum. €STR verwendet die Indexstände an beiden Daten. Fehlende Werte bleiben unbelegt.\n");
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ezb_index_vergleicht_exakte_stichtage() -> Result<(), String> {
        let von = Datum::lies("2026-11-16").ok_or("Starttag")?;
        let bis = Datum::lies("2026-11-19").ok_or("Endtag")?;
        let csv = "KEY,TIME_PERIOD,OBS_VALUE\nEST.B.EU000A2QQF08.CI,2026-11-16,110.00000000\nEST.B.EU000A2QQF08.CI,2026-11-19,110.01100000\n";
        assert_eq!(estr(csv, von, bis), Some(Cent(100_010)));
        assert_eq!(estr(csv, von, bis.plus_tage(1).ok_or("Folgetag")?), None);
        assert!(indexreihe(&csv.replace(ESTR_SERIE, "EST.B.EU000A2X2A25.WT")).is_none());
        Ok(())
    }
}
