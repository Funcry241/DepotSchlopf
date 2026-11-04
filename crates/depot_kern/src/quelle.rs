//! Tageskurse aus einer lesenden Quelle (Kap. 6, R20). Gelesen wird Text, wie ihn eine
//! kostenlose Quelle fuer Schlusskurse liefert: eine Kopfzeile, dann eine Zeile je Handelstag
//! mit Datum und Schlusskurs. Netz und Datei kennt der Kern nicht; der Aufrufer reicht den Text
//! und den Zeitpunkt des Abrufs herein.
//!
//! **Was als Schlusskurs gilt:** Der Kurs eines Handelstags zum Ende des Xetra-Handels um
//! 17:35 Uhr in Frankfurt (Schlussauktion). Eine Zeile fuer einen Tag, dessen Handel beim Abruf
//! nicht beendet ist, ist kein Schlusskurs und zaehlt nicht.
//!
//! Geprueft wird alles, was die Zeile selbst sagt: Datum im Kalender, Tage streng steigend,
//! Preise positiv und in ganzen Cent, Schluss zwischen Tief und Hoch. Ob ein Kurs frisch genug
//! und plausibel zum Vortag ist, prueft danach das Regelwerk wie bei jedem anderen Kurs.

use crate::geld::Cent;
use crate::instrument::Isin;
use crate::kurs::{Herkunft, Kurs, KursFehler, Kurse};
use crate::zeit::{Datum, Zeitpunkt};

/// Stunde und Minute des Xetra-Schlusses in Frankfurt.
pub const XETRA_SCHLUSS: (u8, u8) = (17, 35);

/// Warum ein Text keine Tageskurse ergibt. Zeilen zaehlen ab 1, die Kopfzeile ist Zeile 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuellenFehler {
    /// Kein Text oder nur eine Kopfzeile.
    Leer,
    /// Die Kopfzeile nennt die Spalten `date` und `close` nicht genau einmal.
    Kopf,
    /// Die Zeile hat nicht so viele Felder wie der Kopf.
    Felder(usize),
    /// Kein Datum der Form `JJJJ-MM-TT` im Kalender.
    Datum(usize),
    /// Ein Preis ist kein positiver Betrag in ganzen Cent.
    Preis(usize),
    /// Der Schluss liegt nicht zwischen Tief und Hoch derselben Zeile.
    Spanne(usize),
    /// Der Tag ist nicht spaeter als der der Zeile davor.
    Reihenfolge(usize),
    /// Ein Wert kommt im Abruf zweimal vor.
    Doppelt(Isin),
    /// Der Kurs ergibt keinen gueltigen `Kurs`.
    Kurs(KursFehler),
}

/// Der Schlusskurs eines Handelstags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tageskurs {
    pub datum: Datum,
    pub schluss: Cent,
}

/// Ein Betrag in Euro mit Punkt als Dezimalzeichen, etwa `215.3` oder `215.35`, in Cent.
/// Weitere Nachkommastellen gelten nur, wenn sie null sind: Ein Kurs feiner als ein Cent wird
/// nicht gerundet (R4).
pub fn lies_cent(text: &str) -> Option<Cent> {
    let (ganz, bruch) = text.split_once('.').unwrap_or((text, ""));
    let ziffern = |t: &str| t.bytes().all(|b| b.is_ascii_digit());
    if ganz.is_empty()
        || !ziffern(ganz)
        || !ziffern(bruch)
        || (text.contains('.') && bruch.is_empty())
    {
        return None;
    }
    let (cent, rest) = bruch.split_at(bruch.len().min(2));
    if rest.bytes().any(|b| b != b'0') {
        return None;
    }
    let cent_teil: i64 = format!("{cent:0<2}").parse().ok()?;
    let euro: i64 = ganz.parse().ok()?;
    let betrag = euro.checked_mul(100)?.checked_add(cent_teil)?;
    (betrag > 0).then_some(Cent(betrag))
}

/// Liest die Tageskurse eines Werts, alle Zeilen der Datei, aeltester Tag zuerst.
///
/// Erwartet wird eine Kopfzeile mit Komma als Trenner, mit genau einer Datumsspalte (`date`
/// oder `timestamp`) und der Spalte `close`, gross oder klein geschrieben. `low` und `high`
/// werden geprueft, wenn es sie gibt; andere Spalten werden nicht gelesen. Das ist die Form von
/// Stooq (`Date,Open,High,Low,Close,Volume`) und von Alpha Vantage mit `datatype=csv`
/// (`timestamp,open,high,low,close,volume`). Die Tage steigen oder fallen streng durch die
/// ganze Datei; Alpha Vantage liefert den neuesten zuerst. Der Schluss ist ein Betrag in
/// ganzen Cent; Tief und Hoch duerfen feiner sein (bis sechs Nachkommastellen) und werden exakt
/// mit dem Schluss verglichen.
pub fn lies_tageskurse(text: &str) -> Result<Vec<Tageskurs>, QuellenFehler> {
    let mut zeilen = text.lines().map(|z| z.strip_suffix('\r').unwrap_or(z));
    let kopf: Vec<String> = zeilen
        .next()
        .ok_or(QuellenFehler::Leer)?
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .collect();
    let spalte = |name: &str| -> Result<Option<usize>, QuellenFehler> {
        let mut treffer = kopf.iter().enumerate().filter(|(_, k)| *k == name);
        let erste = treffer.next().map(|(i, _)| i);
        if treffer.next().is_some() {
            return Err(QuellenFehler::Kopf);
        }
        Ok(erste)
    };
    let ((Some(datum_s), None) | (None, Some(datum_s))) = (spalte("date")?, spalte("timestamp")?)
    else {
        return Err(QuellenFehler::Kopf);
    };
    let schluss_s = spalte("close")?.ok_or(QuellenFehler::Kopf)?;
    let tief_s = spalte("low")?;
    let hoch_s = spalte("high")?;

    let mut kurse: Vec<Tageskurs> = Vec::new();
    for (i, zeile) in zeilen.enumerate() {
        let nr = i.saturating_add(2);
        if zeile.trim().is_empty() {
            continue;
        }
        let felder: Vec<&str> = zeile.split(',').map(str::trim).collect();
        if felder.len() != kopf.len() {
            return Err(QuellenFehler::Felder(nr));
        }
        let feld = |s: usize| felder.get(s).copied().unwrap_or("");
        let datum = lies_datum(feld(datum_s)).ok_or(QuellenFehler::Datum(nr))?;
        let schluss = lies_cent(feld(schluss_s)).ok_or(QuellenFehler::Preis(nr))?;
        // Tief und Hoch dienen nur dem Vergleich und duerfen feiner als ein Cent sein; sie
        // werden exakt verglichen, nie gerundet.
        let fein = |s: usize| lies_fein(feld(s)).ok_or(QuellenFehler::Preis(nr));
        let schluss_fein = schluss
            .0
            .checked_mul(FEIN_JE_CENT)
            .ok_or(QuellenFehler::Preis(nr))?;
        if let Some(s) = tief_s
            && fein(s)? > schluss_fein
        {
            return Err(QuellenFehler::Spanne(nr));
        }
        if let Some(s) = hoch_s
            && fein(s)? < schluss_fein
        {
            return Err(QuellenFehler::Spanne(nr));
        }
        if let Some(vorher) = kurse.last() {
            let richtig = match kurse.first().zip(kurse.get(1)) {
                Some((a, b)) if a.datum > b.datum => vorher.datum > datum,
                Some(_) => vorher.datum < datum,
                None => vorher.datum != datum,
            };
            if !richtig {
                return Err(QuellenFehler::Reihenfolge(nr));
            }
        }
        kurse.push(Tageskurs { datum, schluss });
    }
    if kurse.is_empty() {
        return Err(QuellenFehler::Leer);
    }
    if kurse
        .first()
        .zip(kurse.last())
        .is_some_and(|(a, b)| a.datum > b.datum)
    {
        kurse.reverse();
    }
    Ok(kurse)
}

/// Feinste Einheit fuer Tief und Hoch: ein Zehntausendstel Cent (sechs Nachkommastellen).
const FEIN_JE_CENT: i64 = 10_000;

/// Ein positiver Betrag in Euro mit hoechstens sechs Nachkommastellen, in Zehntausendstel Cent.
/// Weitere Nachkommastellen gelten nur, wenn sie null sind.
fn lies_fein(text: &str) -> Option<i64> {
    let (ganz, bruch) = text.split_once('.').unwrap_or((text, ""));
    let ziffern = |t: &str| t.bytes().all(|b| b.is_ascii_digit());
    if ganz.is_empty()
        || !ziffern(ganz)
        || !ziffern(bruch)
        || (text.contains('.') && bruch.is_empty())
    {
        return None;
    }
    let (stellen, rest) = bruch.split_at(bruch.len().min(6));
    if rest.bytes().any(|b| b != b'0') {
        return None;
    }
    let teil: i64 = format!("{stellen:0<6}").parse().ok()?;
    let euro: i64 = ganz.parse().ok()?;
    let betrag = euro.checked_mul(1_000_000)?.checked_add(teil)?;
    (betrag > 0).then_some(betrag)
}

fn lies_datum(text: &str) -> Option<Datum> {
    let mut teile = text.split('-');
    let mut zahl = |n: usize| {
        let t = teile.next()?;
        (t.len() == n && t.bytes().all(|b| b.is_ascii_digit()))
            .then(|| t.parse::<u16>().ok())
            .flatten()
    };
    let jahr = zahl(4)?;
    let monat = u8::try_from(zahl(2)?).ok()?;
    let tag = u8::try_from(zahl(2)?).ok()?;
    if teile.next().is_some() {
        return None;
    }
    Datum::neu(jahr, monat, tag)
}

/// Zeitpunkt des Xetra-Schlusses an einem Tag.
pub fn xetra_schluss(datum: Datum) -> Option<Zeitpunkt> {
    let (stunde, minute) = XETRA_SCHLUSS;
    Zeitpunkt::frankfurt(datum, stunde, minute, 0)
}

/// Die Schlusskurse eines Abrufs: je Wert der letzte Tag, dessen Handel beim Abruf beendet
/// war (`heute`), und der Tag davor (`vortag`), beide mit Stand `abruf`. Fehlt der Vortag eines
/// Werts, fehlt er in `vortag`; das Regelwerk fuehrt das als eigenen Fall.
///
/// # Errors
/// Ein Wert kommt doppelt vor, seine Tage steigen nicht streng (`Reihenfolge` mit der Stelle in
/// der Liste, ab 1), oder ein Kurs ist ungueltig. Ein Wert ohne beendeten Tag fehlt in beiden
/// Kursen.
pub fn schlusskurse(
    abruf: Zeitpunkt,
    werte: Vec<(Isin, Vec<Tageskurs>)>,
) -> Result<(Kurse, Kurse), QuellenFehler> {
    let mut heute = Vec::new();
    let mut vortag = Vec::new();
    let mut gesehen = std::collections::BTreeSet::new();
    for (isin, tage) in werte {
        if !gesehen.insert(isin.clone()) {
            return Err(QuellenFehler::Doppelt(isin));
        }
        if let Some(i) = tage.windows(2).position(|p| {
            p.first()
                .zip(p.get(1))
                .is_none_or(|(a, b)| a.datum >= b.datum)
        }) {
            return Err(QuellenFehler::Reihenfolge(i.saturating_add(2)));
        }
        let mut beendet = Vec::new();
        for t in tage {
            let zeit = xetra_schluss(t.datum).ok_or(QuellenFehler::Kurs(KursFehler::Handelstag))?;
            if zeit <= abruf {
                let kurs = Kurs::neu(t.schluss, "EUR", t.datum, zeit, Herkunft::Quelle)
                    .map_err(QuellenFehler::Kurs)?;
                beendet.push(kurs);
            }
        }
        let mut letzte = beendet.into_iter().rev();
        if let Some(k) = letzte.next() {
            heute.push((isin.clone(), k));
        }
        if let Some(k) = letzte.next() {
            vortag.push((isin, k));
        }
    }
    Ok((
        Kurse::neu(abruf, heute).map_err(QuellenFehler::Kurs)?,
        Kurse::neu(abruf, vortag).map_err(QuellenFehler::Kurs)?,
    ))
}
