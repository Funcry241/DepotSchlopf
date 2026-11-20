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
        || !ganz
            .bytes()
            .chain(nachkomma.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    format!("{ganz}{nachkomma:0<8}")
        .parse::<i128>()
        .ok()
        .filter(|n| *n > 0)
}

fn indexreihe(csv: &str) -> Result<BTreeMap<Datum, i128>, &'static str> {
    const SPALTEN: &str = "Es fehlen eindeutige Spalten KEY, TIME_PERIOD oder OBS_VALUE.";
    const DATUM: &str = "Ein Stichtag ist ungültig.";
    const INDEX: &str = "Ein Indexstand ist ungültig.";
    let mut zeilen = csv.trim_start_matches('\u{feff}').lines();
    let felder: Vec<_> = zeilen
        .next()
        .ok_or("Die Datei ist leer.")?
        .split(',')
        .collect();
    let spalte = |name| {
        let mut treffer = felder.iter().enumerate().filter(|(_, s)| **s == name);
        let (nr, _) = treffer.next()?;
        treffer.next().is_none().then_some(nr)
    };
    let serie = spalte("KEY").ok_or(SPALTEN)?;
    let tag = spalte("TIME_PERIOD").ok_or(SPALTEN)?;
    let wert = spalte("OBS_VALUE").ok_or(SPALTEN)?;
    let mut reihe = BTreeMap::new();
    for zeile in zeilen.filter(|z| !z.trim().is_empty()) {
        let felder: Vec<_> = zeile.split(',').collect();
        if felder.get(serie) != Some(&ESTR_SERIE) {
            return Err("Eine Zeile enthält eine andere Indexreihe oder keinen Serienschlüssel.");
        }
        let d = Datum::lies(felder.get(tag).ok_or(DATUM)?).ok_or(DATUM)?;
        let n = indexwert(felder.get(wert).ok_or(INDEX)?).ok_or(INDEX)?;
        if reihe.insert(d, n).is_some() {
            return Err("Ein Stichtag kommt mehrfach vor.");
        }
    }
    Ok(reihe)
}

fn estr(csv: &str, von: Datum, bis: Datum) -> Result<Cent, String> {
    let reihe =
        indexreihe(csv).map_err(|grund| format!("Fehler in daten/estr-index.csv. {grund}"))?;
    let index_am = |tag| {
        reihe
            .get(&tag)
            .copied()
            .ok_or_else(|| format!("In daten/estr-index.csv fehlt der Indexstand für {tag}."))
    };
    let vorher = index_am(von)?;
    let nachher = index_am(bis)?;
    let cent = i128::from(LANGFRIST.startkapital.0)
        .checked_mul(nachher)
        .and_then(|n| n.checked_div(vorher))
        .and_then(|n| i64::try_from(n).ok())
        .ok_or("Der €STR-Modellwert kann nicht berechnet werden.")?;
    Ok(Cent(cent))
}

fn estr_aus_datei(daten: &Path, von: Datum, bis: Datum) -> Result<Cent, String> {
    let csv = textdatei(&daten.join("estr-index.csv"))
        .map_err(|grund| format!("daten/estr-index.csv konnte nicht gelesen werden ({grund})."))?
        .ok_or("Die lokale Datei daten/estr-index.csv fehlt.")?;
    estr(&csv, von, bis)
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
    let geldmarkt = estr_aus_datei(daten, von, bis);
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
        render(etf.as_ref().ok().copied()),
        render(geldmarkt.as_ref().ok().copied())
    );
    text.push_str("EUNL verwendet belegte Kurse zu den Depotstichtagen, ganze Stücke, Restbargeld, Xetra-Modellspread und die IBKR-Höchstgebühr beim Kauf und beim modellierten Verkauf zum Enddatum. €STR verwendet die Indexstände an beiden Daten. Fehlende Werte bleiben unbelegt.\n");
    for (name, ergebnis) in [("EUNL", etf), ("€STR", geldmarkt)] {
        if let Err(grund) = ergebnis {
            let _ = writeln!(text, "\n**{name}** · {grund}");
        }
    }
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
        assert_eq!(estr(csv, von, bis), Ok(Cent(100_010)));
        assert_eq!(
            estr(csv, von, bis.plus_tage(1).ok_or("Folgetag")?),
            Err("In daten/estr-index.csv fehlt der Indexstand für 2026-11-20.".into())
        );
        assert!(
            estr(&csv.replace(ESTR_SERIE, "EST.B.EU000A2X2A25.WT"), von, bis)
                .is_err_and(|grund| grund.contains("andere Indexreihe"))
        );
        Ok(())
    }

    #[test]
    fn bericht_nennt_fehlenden_eunl_kurs_und_fehlende_estr_datei() -> Result<(), String> {
        let von = Datum::lies("2026-11-16").ok_or("Starttag")?;
        let bis = Datum::lies("2026-11-20").ok_or("Endtag")?;
        let werte = BTreeMap::from([(von, Cent(1_000_000)), (bis, Cent(1_000_000))]);
        let daten = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ohne-estr-index");
        let aus = bericht(&daten, &Stand::default(), &werte);
        assert!(aus.contains("| EUNL kaufen und halten | nicht belegt | nicht belegt |"));
        assert!(aus.contains("Zum Depotabschluss am 2026-11-16 fehlt ein belegter EUNL-Kurs."));
        assert!(aus.contains("Die lokale Datei daten/estr-index.csv fehlt."));
        Ok(())
    }
}
