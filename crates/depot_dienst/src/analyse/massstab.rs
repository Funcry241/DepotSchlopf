//! Historische Vergleichsmaßstäbe für den Analyse-Export.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use depot_journal::Stand;
use depot_kern::messlatte::ibkr::LANGFRIST;
use depot_kern::{Cent, Datum};
use depot_lage::{
    html::maskiere,
    vergleich::{eunl, rendite_bp},
};

use super::{geld, prozent, textdatei};
use crate::http::{self, Anfrage};
use crate::werkbank::{Art, Dienst, schreibe_ganz};

const ESTR_SERIE: &str = "EST.B.EU000A2QQF08.CI";
const ESTR_DATEI: &str = "estr-index.csv";
const EZB_CSV: &str = "https://data-api.ecb.europa.eu/service/data/EST/B.EU000A2QQF08.CI?format=csvdata&amp;detail=dataonly";

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
    if reihe.is_empty() {
        return Err("Die Datei enthält keine Indexstände.");
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

fn estr_datei(daten: &Path) -> Result<String, String> {
    textdatei(&daten.join(ESTR_DATEI))
        .map_err(|grund| format!("daten/estr-index.csv konnte nicht gelesen werden ({grund})."))?
        .ok_or_else(|| "Die lokale Datei daten/estr-index.csv fehlt.".into())
}

fn zeitraum(reihe: &BTreeMap<Datum, i128>) -> Option<String> {
    let ((von, _), (bis, _)) = reihe.first_key_value().zip(reihe.last_key_value())?;
    Some(format!(
        "{} Indexstände vom {von} bis {bis} gespeichert.",
        reihe.len()
    ))
}

pub(crate) fn einrichtung(d: &Dienst) -> String {
    let stand = estr_datei(&d.wurzel.join("daten")).and_then(|csv| {
        let reihe = indexreihe(&csv)?;
        zeitraum(&reihe).ok_or_else(|| "Die Datei enthält keine Indexstände.".into())
    });
    let (klasse, stand) = match stand {
        Ok(stand) => ("gut", stand),
        Err(grund) => ("leise", grund),
    };
    format!(
        "<section id=\"estr-index\"><h2>€STR für den Analyse-Export</h2>\
         <p>Der Analyse-Export ruft die EZB-Indexreihe ab, wenn Indexstände fehlen. Du kannst die <a href=\"{EZB_CSV}\" target=\"_blank\" rel=\"noopener\">EZB-Indexreihe als CSV herunterladen</a> und hier importieren.</p>\
         <form action=\"{}\" method=\"post\" data-estr-import=\"{}\">\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <label>EZB-CSV <input type=\"file\" accept=\".csv,text/csv\" required></label>\
         <button type=\"submit\">€STR-CSV importieren</button>\
         <p class=\"{klasse}\" role=\"status\">{}</p></form></section>",
        http::ESTR_IMPORT,
        http::ESTR_MAX,
        maskiere(&d.token),
        maskiere(&stand),
    )
}

pub(crate) fn importieren(d: &Dienst, a: &Anfrage) -> Vec<u8> {
    let antwort =
        |code, text: &str| http::antwort(code, "text/plain; charset=utf-8", text.as_bytes());
    if !http::herkunft_erlaubt(a.kopf("origin")) {
        return antwort(403, "Abgelehnt: fremde Herkunft.");
    }
    if !a
        .kopf("x-depotschlopf-token")
        .is_some_and(|t| http::gleich(t, &d.token))
    {
        return antwort(
            403,
            "Diese Seite ist veraltet. Öffne die Einrichtung erneut.",
        );
    }
    if a.rumpf.len() > http::ESTR_MAX {
        return antwort(413, "Die CSV ist größer als 2 MiB.");
    }
    if a.kopf("content-type") != Some("text/csv") {
        return antwort(400, "Wähle die EZB-CSV in der Einrichtung aus.");
    }
    let Ok(csv) = std::str::from_utf8(&a.rumpf) else {
        return antwort(400, "Die CSV ist nicht als UTF-8 lesbar.");
    };
    let reihe = match indexreihe(csv) {
        Ok(reihe) => reihe,
        Err(grund) => return antwort(400, grund),
    };
    let Some(stand) = zeitraum(&reihe) else {
        return antwort(400, "Die Datei enthält keine Indexstände.");
    };
    let ziel = d.wurzel.join("daten").join(ESTR_DATEI);
    let zustand = d.sperre();
    if zustand.neustart || zustand.auftrag_stand(Art::Update).is_some() || d.update_offen() {
        return antwort(
            409,
            "Warte, bis das Update abgeschlossen ist, und importiere die CSV erneut.",
        );
    }
    if schreibe_ganz(&ziel, csv).is_err() {
        return antwort(409, "Die €STR-CSV konnte nicht gespeichert werden.");
    }
    antwort(200, &stand)
}

pub(super) struct Vergleich {
    von: Datum,
    bis: Datum,
    etf: Result<Cent, String>,
}

pub(super) fn vergleich(stand: &Stand, werte: &BTreeMap<Datum, Cent>) -> Option<Vergleich> {
    let (&von, _) = werte.first_key_value()?;
    let (&bis, _) = werte.last_key_value()?;
    (von < bis).then(|| Vergleich {
        von,
        bis,
        etf: eunl(stand, von, bis),
    })
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Geldmarkt {
    wert: Cent,
    quelle: Quelle,
}

#[derive(Debug, PartialEq, Eq)]
enum Quelle {
    Datei,
    Ezb,
    DateiNachAbruf,
}

fn index_oder_abruf(
    daten: &Path,
    von: Datum,
    bis: Datum,
    abrufen: impl FnOnce() -> Result<Geldmarkt, String>,
) -> Result<Geldmarkt, String> {
    match estr_datei(daten).and_then(|csv| estr(&csv, von, bis)) {
        Ok(wert) => Ok(Geldmarkt {
            wert,
            quelle: Quelle::Datei,
        }),
        Err(lokal) => abrufen().map_err(|abruf| format!("{lokal} EZB-Abruf: {abruf}")),
    }
}

pub(super) fn geldmarkt(
    d: &Dienst,
    daten: &Path,
    von: Datum,
    bis: Datum,
) -> Result<Geldmarkt, String> {
    index_oder_abruf(daten, von, bis, || {
        let csv = d.estr_index_abrufen()?;
        let wert = estr(&csv, von, bis)?;
        let zustand = d.sperre();
        if let Ok(lokal) = estr_datei(daten).and_then(|lokal| estr(&lokal, von, bis)) {
            return Ok(Geldmarkt {
                wert: lokal,
                quelle: Quelle::DateiNachAbruf,
            });
        }
        if zustand.neustart || zustand.auftrag_stand(Art::Update).is_some() || d.update_offen() {
            return Err("Während des Updates kann die EZB-CSV nicht gespeichert werden.".into());
        }
        schreibe_ganz(&daten.join(ESTR_DATEI), &csv)
            .map_err(|_| "Die EZB-CSV konnte nicht gespeichert werden.")?;
        Ok(Geldmarkt {
            wert,
            quelle: Quelle::Ezb,
        })
    })
}

pub(super) fn bericht(
    vergleich: Option<Vergleich>,
    geldmarkt: impl FnOnce(Datum, Datum) -> Result<Geldmarkt, String>,
) -> String {
    let mut text = String::from("\n### EUNL und €STR\n\n");
    let Some(Vergleich { von, bis, etf }) = vergleich else {
        text.push_str("Zwei belegte Depotabschlüsse für den Vergleich fehlen.\n");
        return text;
    };
    let geldmarkt = geldmarkt(von, bis);
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
        render(geldmarkt.as_ref().ok().map(|beleg| beleg.wert))
    );
    text.push_str("EUNL verwendet belegte Kurse zu den Depotstichtagen, ganze Stücke, Restbargeld, Xetra-Modellspread und die IBKR-Höchstgebühr beim Kauf und beim modellierten Verkauf zum Enddatum. €STR verwendet die Indexstände an beiden Daten. Fehlende Werte bleiben unbelegt.\n");
    if let Err(grund) = etf {
        let _ = writeln!(text, "\n**EUNL** · {grund}");
    }
    let quelle = match geldmarkt {
        Ok(Geldmarkt {
            quelle: Quelle::Datei,
            ..
        }) => "Vorhandene Datei daten/estr-index.csv verwendet.",
        Ok(Geldmarkt {
            quelle: Quelle::Ezb,
            ..
        }) => "In diesem Export von der EZB abgerufen, geprüft und gespeichert.",
        Ok(Geldmarkt {
            quelle: Quelle::DateiNachAbruf,
            ..
        }) => "EZB-Abruf erfolgreich. Inzwischen verfügbare lokale Datei verwendet.",
        Err(grund) => {
            let _ = writeln!(text, "\n**€STR** · Stichtage {von} und {bis} · {grund}");
            return text;
        }
    };
    let _ = writeln!(text, "\n**€STR** · Stichtage {von} und {bis} · {quelle}");
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
    fn vorhandene_stichtage_kommen_ohne_abruf_aus() -> Result<(), String> {
        let von = Datum::lies("2026-11-16").ok_or("Starttag")?;
        let bis = Datum::lies("2026-11-19").ok_or("Endtag")?;
        let daten = std::env::temp_dir().join(format!("depot-estr-{}", crate::neues_token()));
        let cache = daten.join(ESTR_DATEI);
        let csv = "KEY,TIME_PERIOD,OBS_VALUE\nEST.B.EU000A2QQF08.CI,2026-11-16,110.00000000\nEST.B.EU000A2QQF08.CI,2026-11-19,110.01100000\n";
        schreibe_ganz(&cache, csv)?;
        let treffer = index_oder_abruf(&daten, von, bis, || Err("Unerwarteter Abruf.".into()));
        let fehlt = index_oder_abruf(&daten, von, bis.plus_tage(1).ok_or("Folgetag")?, || {
            Err("Abruf ausgefallen.".into())
        });
        let gespeichert = estr_datei(&daten);
        std::fs::remove_dir_all(&daten).map_err(|e| e.to_string())?;
        assert_eq!(
            treffer,
            Ok(Geldmarkt {
                wert: Cent(100_010),
                quelle: Quelle::Datei
            })
        );
        assert!(fehlt.is_err_and(|grund| {
            grund.contains("2026-11-20") && grund.contains("Abruf ausgefallen.")
        }));
        assert_eq!(gespeichert.as_deref(), Ok(csv));
        Ok(())
    }

    #[test]
    fn bericht_nennt_fehlenden_eunl_kurs_und_fehlende_estr_datei() -> Result<(), String> {
        let von = Datum::lies("2026-11-16").ok_or("Starttag")?;
        let bis = Datum::lies("2026-11-20").ok_or("Endtag")?;
        let werte = BTreeMap::from([(von, Cent(1_000_000)), (bis, Cent(1_000_000))]);
        let daten = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ohne-estr-index");
        let aus = bericht(vergleich(&Stand::default(), &werte), |von, bis| {
            index_oder_abruf(&daten, von, bis, || Err("Nicht verfügbar.".into()))
        });
        assert!(aus.contains("| EUNL kaufen und halten | nicht belegt | nicht belegt |"));
        assert!(aus.contains("Zum Depotabschluss am 2026-11-16 fehlt ein belegter EUNL-Kurs."));
        assert!(aus.contains("Stichtage 2026-11-16 und 2026-11-20"));
        assert!(aus.contains("Die lokale Datei daten/estr-index.csv fehlt."));
        assert!(aus.contains("EZB-Abruf: Nicht verfügbar."));
        for (quelle, erwartet) in [
            (
                Quelle::Datei,
                "Vorhandene Datei daten/estr-index.csv verwendet.",
            ),
            (
                Quelle::Ezb,
                "In diesem Export von der EZB abgerufen, geprüft und gespeichert.",
            ),
            (
                Quelle::DateiNachAbruf,
                "EZB-Abruf erfolgreich. Inzwischen verfügbare lokale Datei verwendet.",
            ),
        ] {
            let aus = bericht(vergleich(&Stand::default(), &werte), |_, _| {
                Ok(Geldmarkt {
                    wert: Cent(100_010),
                    quelle,
                })
            });
            assert!(aus.contains(erwartet));
            assert!(aus.contains("Stichtage 2026-11-16 und 2026-11-20"));
        }
        Ok(())
    }
}
