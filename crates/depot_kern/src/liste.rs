//! Die Beobachtungsliste: welche Werte der Sammler unter welchem Kuerzel bei der Quelle abruft.
//!
//! Die Liste steht im Code und aendert sich nur mit einer geprueften Aenderung. Der Papiertag
//! handelt nur Werte dieser Liste (`als_universum`): Was keinen geprueften Schlusskurs hat, kann
//! nicht gehandelt werden. Reine Daten ohne Netz; den Abruf macht `depot_sammler`. Alpha Vantage
//! fuehrt Xetra-Werte mit der Endung `.DEX`. Die kostenlose Stufe erlaubt 25 Abrufe am Tag; ein
//! Lauf braucht einen Abruf je Wert.

use crate::grenzen::Universum;
use crate::instrument::{Instrument, InstrumentArt, Isin};

/// Name der Quelle im Journal (`kurs;quelle;…`).
pub const QUELLE: &str = "alphavantage";

/// ISIN, Kuerzel bei der Quelle, Name.
const WERTE: [(&str, &str, &str); 10] = [
    ("DE0007164600", "SAP.DEX", "SAP"),
    ("DE0007236101", "SIE.DEX", "Siemens"),
    ("DE0008404005", "ALV.DEX", "Allianz"),
    ("DE0005557508", "DTE.DEX", "Deutsche Telekom"),
    ("DE000BASF111", "BAS.DEX", "BASF"),
    ("DE0007100000", "MBG.DEX", "Mercedes-Benz Group"),
    ("DE0008430026", "MUV2.DEX", "Muenchener Rueck"),
    ("DE0005810055", "DB1.DEX", "Deutsche Boerse"),
    ("DE0006231004", "IFX.DEX", "Infineon"),
    ("DE0005190003", "BMW.DEX", "BMW"),
];

/// Ein beobachteter Wert.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wert {
    pub isin: Isin,
    /// Kuerzel bei der Quelle: Grossbuchstaben und Ziffern, dann `.DEX`.
    pub kuerzel: String,
    pub name: String,
}

fn kuerzel_gueltig(k: &str) -> bool {
    k.strip_suffix(".DEX").is_some_and(|stamm| {
        !stamm.is_empty()
            && stamm
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    })
}

/// Prueft eine Liste: gueltige ISIN, Kuerzel in der Form der Quelle, keine ISIN und kein
/// Kuerzel doppelt.
///
/// # Errors
/// Nennt den ersten fehlerhaften Eintrag.
pub fn pruefe_liste(eintraege: &[(&str, &str, &str)]) -> Result<Vec<Wert>, String> {
    let mut werte: Vec<Wert> = Vec::new();
    for (isin, kuerzel, name) in eintraege {
        let isin = Isin::neu(isin).map_err(|e| format!("Liste: ISIN {isin}: {e:?}"))?;
        if !kuerzel_gueltig(kuerzel) {
            return Err(format!("Liste: Kuerzel {kuerzel:?} ungueltig"));
        }
        if werte
            .iter()
            .any(|w| w.isin == isin || w.kuerzel == *kuerzel)
        {
            return Err(format!("Liste: {isin} oder {kuerzel} doppelt"));
        }
        werte.push(Wert {
            isin,
            kuerzel: (*kuerzel).to_string(),
            name: (*name).to_string(),
        });
    }
    if werte.is_empty() {
        return Err("Liste: leer".into());
    }
    Ok(werte)
}

/// Die festgelegte Beobachtungsliste.
///
/// # Errors
/// Wie `pruefe_liste`.
pub fn beobachtungsliste() -> Result<Vec<Wert>, String> {
    pruefe_liste(&WERTE)
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
