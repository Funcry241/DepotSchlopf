//! Tiingo-Import: unbereinigtes `close`, `high` und `low`, mit belegter
//! Normalisierung winziger Dezimalartefakte. Der allgemeine Preisleser bleibt exakt.

use crate::{Datum, Preis, QuellenFehler, Tageskurs};

/// Journalquelle für Kapitalmaßnahmen aus einem Tiingo-Kursabruf.
pub const QUELLE_KAPITAL: &str = "tiingo-kapital";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rundungsbeleg {
    pub datum: Datum,
    pub zeile: usize,
    pub feld: String,
    pub original: String,
    pub preis: Preis,
}

/// Erkennt das bestehende Journalformat für eine belegte Tiingo-Rundung.
#[must_use]
pub fn ist_rundungsbeobachtung(text: &str) -> bool {
    let Some(felder) = text.strip_prefix("Tiingo-Normalisierung: ") else {
        return false;
    };
    let teile: Vec<_> = felder.splitn(8, "; ").collect();
    let [
        isin,
        abruf,
        roh,
        zeile,
        feld,
        original,
        preis,
        "rundung=halb-gerade; abweichung-hoechstens=0.000000001-USD",
    ] = teile.as_slice()
    else {
        return false;
    };
    let Some(original) = original.strip_prefix("original=") else {
        return false;
    };
    let preis = preis
        .strip_prefix("nach-millionstel-usd=")
        .and_then(|wert| wert.parse().ok());
    isin.strip_prefix("isin=")
        .is_some_and(|s| crate::Isin::neu(s).is_ok())
        && abruf
            .strip_prefix("abruf=")
            .and_then(crate::Zeitpunkt::lies)
            .is_some()
        && roh
            .strip_prefix("roh=")
            .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        && zeile
            .strip_prefix("zeile=")
            .and_then(|s| s.parse::<usize>().ok())
            .is_some_and(|n| n >= 2)
        && matches!(*feld, "feld=close" | "feld=high" | "feld=low")
        && crate::lies_preis(original).is_none()
        && normalisiere(original).is_some_and(|gerundet| Some(gerundet.fein()) == preis)
}

/// Optionale Tiingo-Felder zum unbereinigten Tageskurs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marktdaten {
    pub datum: Datum,
    pub eroeffnung: Option<Preis>,
    pub volumen: Option<u64>,
    pub dividende: Option<Preis>,
    /// Positiver Dezimalfaktor mit ursprünglicher Genauigkeit.
    pub split_faktor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tageskurse {
    pub kurse: Vec<Tageskurs>,
    pub rundungen: Vec<Rundungsbeleg>,
    pub marktdaten: Vec<Marktdaten>,
}

/// Rundet nur bei einer absoluten Abweichung von höchstens 0.000000001 Dollar.
/// Kein Fließkomma: auch die Toleranzprüfung liest Dezimalziffern exakt.
fn normalisiere(text: &str) -> Option<Preis> {
    if let Some(preis) = crate::lies_preis(text) {
        return Some(preis);
    }
    let (ganz, bruch) = text.split_once('.')?;
    if ganz.is_empty() || bruch.len() <= 6 || !bruch.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let basis = Preis::lies_euro(&format!("{ganz}.{}", bruch.get(..6)?))?;
    let rest = bruch.get(6..)?;
    let erste = rest.bytes().next()?;
    let auf = erste > b'5'
        || (erste == b'5'
            && (rest.get(1..)?.bytes().any(|b| b != b'0') || basis.fein().checked_rem(2)? != 0));
    let nano: u16 = format!("{:0<3}", rest.get(..rest.len().min(3))?)
        .parse()
        .ok()?;
    let nah = if auf {
        nano == 999
    } else {
        nano == 0 || (nano == 1 && rest.get(rest.len().min(3)..)?.bytes().all(|b| b == b'0'))
    };
    if !nah {
        return None;
    }
    let preis = Preis::aus_fein(basis.fein().checked_add(i64::from(auf))?);
    (preis.fein() > 0).then_some(preis)
}

fn split_faktor(text: &str) -> Option<String> {
    let (ganz, bruch) = text.split_once('.').unwrap_or((text, ""));
    if text.len() > 32
        || ganz.is_empty()
        || !ganz
            .bytes()
            .chain(bruch.bytes())
            .all(|z| z.is_ascii_digit())
        || !ganz.bytes().chain(bruch.bytes()).any(|z| z != b'0')
    {
        return None;
    }
    let ganz = ganz.trim_start_matches('0');
    let ganz = if ganz.is_empty() { "0" } else { ganz };
    let bruch = bruch.trim_end_matches('0');
    Some(if bruch.is_empty() {
        ganz.to_owned()
    } else {
        format!("{ganz}.{bruch}")
    })
}

fn marktdaten(datum: Datum, felder: &[String], spalten: &[String]) -> Marktdaten {
    let feld = |name| {
        spalten
            .iter()
            .position(|s| s == name)
            .and_then(|i| felder.get(i))
            .map(String::as_str)
    };
    Marktdaten {
        datum,
        eroeffnung: feld("open").and_then(normalisiere),
        volumen: feld("volume").and_then(|t| t.parse().ok()),
        dividende: feld("divcash")
            .and_then(Preis::lies_euro)
            .filter(|p| p.fein() >= 0),
        split_faktor: feld("splitfactor").and_then(split_faktor),
    }
}

/// Normalisiert nur die drei unbereinigten Preisfelder. Angepasste Preise (`adjClose`)
/// werden nicht gelesen. Die strengen Kalender-, Spalten- und Spannenprüfungen folgen danach.
pub fn lies_tageskurse(text: &str) -> Result<Tageskurse, QuellenFehler> {
    let mut zeilen = text.lines();
    let kopf = zeilen.next().ok_or(QuellenFehler::Leer)?;
    let spalten: Vec<_> = kopf
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .collect();
    let datum_spalte = spalten
        .iter()
        .position(|s| matches!(s.as_str(), "date" | "timestamp"))
        .ok_or(QuellenFehler::Kopf)?;
    let mut ausgabe = format!("{kopf}\n");
    let mut rundungen = Vec::new();
    let mut extras = Vec::new();
    for (i, zeile) in zeilen.enumerate() {
        let nr = i.saturating_add(2);
        if zeile.trim().is_empty() {
            ausgabe.push('\n');
            continue;
        }
        let mut felder: Vec<_> = zeile.split(',').map(|s| s.trim().to_string()).collect();
        if felder.len() != spalten.len() {
            return Err(QuellenFehler::Felder(nr));
        }
        let datum = felder
            .get(datum_spalte)
            .and_then(|s| Datum::lies(s))
            .ok_or(QuellenFehler::Datum(nr))?;
        extras.push(marktdaten(datum, &felder, &spalten));
        for (feld, name) in felder.iter_mut().zip(&spalten) {
            if !matches!(name.as_str(), "close" | "high" | "low") {
                continue;
            }
            let preis = normalisiere(feld).ok_or(QuellenFehler::Preis(nr))?;
            if crate::lies_preis(feld).is_none() {
                rundungen.push(Rundungsbeleg {
                    datum,
                    zeile: nr,
                    feld: name.clone(),
                    original: feld.clone(),
                    preis,
                });
                *feld = format!(
                    "{}.{:06}",
                    preis.fein().checked_div(1_000_000).unwrap_or(0),
                    preis.fein().checked_rem(1_000_000).unwrap_or(0)
                );
            }
        }
        ausgabe.push_str(&felder.join(","));
        ausgabe.push('\n');
    }
    let kurse = crate::lies_tageskurse(&ausgabe)?;
    Ok(Tageskurse {
        kurse,
        rundungen,
        marktdaten: extras,
    })
}
