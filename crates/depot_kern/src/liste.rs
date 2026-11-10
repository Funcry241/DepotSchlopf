//! Die Beobachtungsliste: welche Werte der Sammler unter welchem Kuerzel bei der Quelle abruft.
//!
//! Die Liste steht im Code und aendert sich nur mit einer geprueften Aenderung. Der Papiertag
//! handelt nur Werte dieser Liste (`als_universum`): Was keinen geprueften Schlusskurs hat, kann
//! nicht gehandelt werden. Reine Daten ohne Netz; den Abruf macht `depot_sammler`.
//! Xetra-Werte kommen von Alpha Vantage mit der Endung `.DEX`, Werte der NYSE von Tiingo, im
//! Kuerzel ohne Endung (`quelle`). Ein Lauf braucht einen Abruf je Wert; je Quelle passt die
//! Liste in das Kontingent der kostenlosen Stufe (`hoechstens_werte`).
//!
//! Jeder Wert gehoert zu einer Boerse; ihr Schluss bestimmt, ab wann sein Schlusskurs gilt, und
//! ihr Kalender, an welchen Tagen er gehandelt wird (`handelstag`).

use crate::boerse::Boerse;
use crate::grenzen::Universum;
use crate::instrument::{Instrument, InstrumentArt, Isin};
use crate::zeit::Zeitpunkt;

/// Name der Quelle fuer Xetra-Werte im Journal (`kurs;quelle;…`).
pub const QUELLE: &str = "alphavantage";

/// Name der Quelle fuer Werte der NYSE im Journal: Tiingo (freigegeben am 05.11.2026). Die
/// kostenlose Stufe von Tiingo hat ein eigenes Kontingent, unabhaengig von Alpha Vantage.
pub const QUELLE_US: &str = "tiingo";

/// Die Quelle, bei der die Werte einer Boerse abgerufen werden.
pub fn quelle(boerse: Boerse) -> &'static str {
    match boerse {
        Boerse::Xetra => QUELLE,
        Boerse::Nyse => QUELLE_US,
    }
}

/// Abrufe am Tag, die Alpha Vantage in der kostenlosen Stufe erlaubt.
pub const ABRUFE_ALPHA_VANTAGE: usize = 25;

/// Abrufe am Tag bei Alpha Vantage, die nicht fuer Kurse sind: der Ergebniskalender, den
/// `run.cmd termine` hoechstens einmal am Tag mit demselben Schluessel holt
/// (`depot_sammler::ergebnisse`, #120). Die Nachrichten kommen seit #81 per RSS.
pub const WEITERE_ABRUFE_ALPHA_VANTAGE: usize = 1;

/// Wie viele Werte eine Quelle je Lauf hoechstens bekommt: ein Abruf je Wert, und ein Lauf muss
/// mit allen weiteren Abrufen derselben Quelle in das Tageskontingent der kostenlosen Stufe
/// passen. Alpha Vantage: 25 Abrufe am Tag, einer davon fuer die Nachrichten. Tiingo: 1000 am
/// Tag, aber nur 500 verschiedene Kuerzel im Monat, das ist die engere Grenze.
pub fn hoechstens_werte(quelle_name: &str) -> usize {
    match quelle_name {
        QUELLE => ABRUFE_ALPHA_VANTAGE.saturating_sub(WEITERE_ABRUFE_ALPHA_VANTAGE),
        QUELLE_US => 500,
        _ => 0,
    }
}

/// Ein Eintrag der Liste: ISIN, Boerse, Kuerzel bei der Quelle, Name.
pub type Eintrag<'a> = (&'a str, Boerse, &'a str, &'a str);

const WERTE: [Eintrag<'static>; 15] = [
    ("DE0007164600", Boerse::Xetra, "SAP.DEX", "SAP"),
    ("DE0007236101", Boerse::Xetra, "SIE.DEX", "Siemens"),
    ("DE0008404005", Boerse::Xetra, "ALV.DEX", "Allianz"),
    ("DE0005557508", Boerse::Xetra, "DTE.DEX", "Deutsche Telekom"),
    ("DE000BASF111", Boerse::Xetra, "BAS.DEX", "BASF"),
    (
        "DE0007100000",
        Boerse::Xetra,
        "MBG.DEX",
        "Mercedes-Benz Group",
    ),
    ("DE0008430026", Boerse::Xetra, "MUV2.DEX", "Münchener Rück"),
    ("DE0005810055", Boerse::Xetra, "DB1.DEX", "Deutsche Börse"),
    ("DE0006231004", Boerse::Xetra, "IFX.DEX", "Infineon"),
    ("DE0005190003", Boerse::Xetra, "BMW.DEX", "BMW"),
    ("US46625H1005", Boerse::Nyse, "JPM", "JPMorgan Chase"),
    ("US1912161007", Boerse::Nyse, "KO", "Coca-Cola"),
    ("US4781601046", Boerse::Nyse, "JNJ", "Johnson & Johnson"),
    ("US30231G1022", Boerse::Nyse, "XOM", "Exxon Mobil"),
    ("US92826C8394", Boerse::Nyse, "V", "Visa"),
];

/// Ein beobachteter Wert.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wert {
    pub isin: Isin,
    pub boerse: Boerse,
    /// Kuerzel bei der Quelle: fuer Xetra Grossbuchstaben und Ziffern, dann `.DEX`; fuer die
    /// NYSE ein bis fuenf Grossbuchstaben, hoechstens mit einer Klasse wie `.B`.
    pub kuerzel: String,
    pub name: String,
}

fn kuerzel_gueltig(boerse: Boerse, k: &str) -> bool {
    let gross = |t: &str, ziffern: bool| {
        !t.is_empty()
            && t.bytes()
                .all(|b| b.is_ascii_uppercase() || (ziffern && b.is_ascii_digit()))
    };
    match boerse {
        Boerse::Xetra => k
            .strip_suffix(".DEX")
            .is_some_and(|stamm| gross(stamm, true)),
        Boerse::Nyse => {
            let (stamm, klasse) = k.split_once('.').unwrap_or((k, "A"));
            stamm.len() <= 5 && gross(stamm, false) && klasse.len() == 1 && gross(klasse, false)
        }
    }
}

/// Prueft eine Liste: gueltige ISIN, Kuerzel in der Form der Quelle fuer die Boerse, keine ISIN
/// und kein Kuerzel doppelt, und je Quelle nicht mehr Werte, als ein Lauf abrufen darf
/// (`hoechstens_werte`).
///
/// # Errors
/// Nennt den ersten fehlerhaften Eintrag.
pub fn pruefe_liste(eintraege: &[Eintrag<'_>]) -> Result<Vec<Wert>, String> {
    let mut werte: Vec<Wert> = Vec::new();
    for (isin, boerse, kuerzel, name) in eintraege {
        let isin = Isin::neu(isin).map_err(|e| format!("Liste: ISIN {isin}: {e:?}"))?;
        if !kuerzel_gueltig(*boerse, kuerzel) {
            return Err(format!("Liste: Kürzel {kuerzel:?} ungültig"));
        }
        if werte
            .iter()
            .any(|w| w.isin == isin || w.kuerzel == *kuerzel)
        {
            return Err(format!("Liste: {isin} oder {kuerzel} doppelt"));
        }
        werte.push(Wert {
            isin,
            boerse: *boerse,
            kuerzel: (*kuerzel).to_string(),
            name: (*name).to_string(),
        });
    }
    if werte.is_empty() {
        return Err("Liste: leer".into());
    }
    for q in [QUELLE, QUELLE_US] {
        let n = werte.iter().filter(|w| quelle(w.boerse) == q).count();
        if n > hoechstens_werte(q) {
            return Err(format!(
                "Liste: {n} Werte für {q}, höchstens {} je Lauf",
                hoechstens_werte(q)
            ));
        }
    }
    Ok(werte)
}

/// Die Werte des Vergleichsdepots (#118): genau die Liste vom 10.11.2026. Sie bleibt fest, auch
/// wenn die Beobachtungsliste waechst; sonst aenderte sich die Vergangenheit des Vergleichs.
pub const VERGLEICH_ISIN: [&str; 15] = [
    "DE0007164600",
    "DE0007236101",
    "DE0008404005",
    "DE0005557508",
    "DE000BASF111",
    "DE0007100000",
    "DE0008430026",
    "DE0005810055",
    "DE0006231004",
    "DE0005190003",
    "US46625H1005",
    "US1912161007",
    "US4781601046",
    "US30231G1022",
    "US92826C8394",
];

/// Die festgelegte Beobachtungsliste.
///
/// # Errors
/// Wie `pruefe_liste`.
pub fn beobachtungsliste() -> Result<Vec<Wert>, String> {
    pruefe_liste(&WERTE)
}

/// Der erste Schluss der Boerse des Werts nach dem Versand. Nur dieser Schluss ist fuer
/// eine zeitbelegte Papierorder erreichbar. Ohne bekannte Boerse gibt es keine Frist.
pub fn papierorder_schluss(isin: &Isin, versand: Zeitpunkt) -> Option<Zeitpunkt> {
    let (_, boerse, _, _) = WERTE.iter().find(|(id, ..)| *id == isin.als_str())?;
    let mut tag = versand.datum()?;
    loop {
        if boerse.handelstag(tag) {
            let schluss = boerse.schluss(tag)?;
            if schluss > versand {
                return Some(schluss);
            }
        }
        tag = tag.plus_tage(1)?;
    }
}

/// Die Boerse eines Werts der festgelegten Liste.
pub fn boerse_von(isin: &Isin) -> Option<Boerse> {
    WERTE
        .iter()
        .find(|(id, ..)| *id == isin.als_str())
        .map(|(_, boerse, _, _)| *boerse)
}

/// Die Boersen der festgelegten Liste.
pub fn boersen() -> Vec<Boerse> {
    let mut b: Vec<Boerse> = WERTE.iter().map(|(_, boerse, _, _)| *boerse).collect();
    b.sort();
    b.dedup();
    b
}

/// Ist an diesem Tag wenigstens eine Boerse der festgelegten Liste offen? Sonst ist der Tag
/// geschlossen: kein Abruf, kein Papiertag, nichts verpasst.
pub fn handelstag(datum: crate::zeit::Datum) -> bool {
    boersen().into_iter().any(|b| b.handelstag(datum))
}

/// Warum die Boersen der festgelegten Liste an einem Werktag geschlossen sind, etwa
/// `Xetra: Heiligabend`. `None`, wenn eine offen ist oder der Tag ein Wochenende ist.
pub fn feiertag(datum: crate::zeit::Datum) -> Option<String> {
    if handelstag(datum) || !datum.wochentag().is_some_and(|w| (1..=5).contains(&w)) {
        return None;
    }
    let namen: Vec<String> = boersen()
        .into_iter()
        .filter_map(|b| b.feiertag(datum).map(|f| format!("{b}: {f}")))
        .collect();
    Some(namen.join(", "))
}

/// Die Werte als Universum, damit `pruefe_kurse` jeden Wert der Liste prueft.
pub fn als_universum(werte: &[Wert]) -> Universum {
    Universum::neu(
        werte
            .iter()
            .map(|w| Instrument {
                isin: w.isin.clone(),
                name: w.name.clone(),
                art: InstrumentArt::Aktie,
            })
            .collect(),
    )
}

#[cfg(test)]
mod papierorder_tests {
    use super::*;

    #[test]
    fn papierorder_wartet_auf_ihre_boerse() -> Result<(), String> {
        let mittwoch = crate::Datum::neu(2026, 11, 25).ok_or("Mittwoch")?;
        let donnerstag = mittwoch.plus_tage(1).ok_or("Donnerstag")?;
        let freitag = donnerstag.plus_tage(1).ok_or("Freitag")?;
        let versand = Zeitpunkt::frankfurt(mittwoch, 22, 15, 0).ok_or("Versand")?;
        let sap = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let coca_cola = Isin::neu("US1912161007").map_err(|e| format!("{e:?}"))?;
        assert_eq!(
            papierorder_schluss(&sap, versand),
            Boerse::Xetra.schluss(donnerstag)
        );
        assert_eq!(
            papierorder_schluss(&coca_cola, versand),
            Boerse::Nyse.schluss(freitag)
        );
        Ok(())
    }

    #[test]
    fn vergleichswerte_sind_gueltig_und_in_der_liste() -> Result<(), String> {
        for isin in VERGLEICH_ISIN {
            Isin::neu(isin).map_err(|e| format!("{isin}: {e:?}"))?;
            if !WERTE.iter().any(|(id, ..)| *id == isin) {
                return Err(format!("{isin} fehlt in der Beobachtungsliste"));
            }
        }
        Ok(())
    }
}
