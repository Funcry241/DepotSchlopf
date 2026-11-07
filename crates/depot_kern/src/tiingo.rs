//! Tiingo-Import: unbereinigtes `close`, `high` und `low`, mit belegter
//! Normalisierung winziger Dezimalartefakte. Der allgemeine Preisleser bleibt exakt.

use crate::{Datum, Preis, QuellenFehler, Tageskurs};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rundungsbeleg {
    pub datum: Datum,
    pub zeile: usize,
    pub feld: String,
    pub original: String,
    pub preis: Preis,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tageskurse {
    pub kurse: Vec<Tageskurs>,
    pub rundungen: Vec<Rundungsbeleg>,
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
    Ok(Tageskurse { kurse, rundungen })
}
