//! Plausibilitaetspruefung der Kurse. Bei jedem Fehler faellt der Zyklus aus.
//!
//! Jeder Wert des Universums braucht einen Kurs, der nicht aelter als das Hoechstalter ist,
//! und einen Vergleichskurs vom Vortag. Fehlt der Vergleichskurs, ist der Sprungtest nicht
//! bestanden, sondern ein eigener Fehler (R20, 1·F08).

use depot_kern::{Grenzen, Isin, Kurse, Universum};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DatenFehler {
    KursFehlt(Isin),
    KursNichtPositiv(Isin),
    Kurssprung {
        isin: Isin,
        sprung_bp: i64,
    },
    /// Kein Vortagskurs: Der Sprung laesst sich nicht pruefen.
    KeinVergleichskurs(Isin),
    /// Kurs aelter als das Hoechstalter (R06), oder sein Alter ist nicht bestimmbar.
    KursZuAlt(Isin),
}

/// Prueft alle Werte des Universums. Leere Liste = alles in Ordnung.
pub fn pruefe_kurse(
    grenzen: &Grenzen,
    universum: &Universum,
    heute: &Kurse,
    vortag: &Kurse,
) -> Vec<DatenFehler> {
    let mut fehler = Vec::new();
    for isin in universum.isins() {
        let Some(kurs) = heute.kurs(&isin) else {
            fehler.push(DatenFehler::KursFehlt(isin));
            continue;
        };
        if kurs.0 <= 0 {
            fehler.push(DatenFehler::KursNichtPositiv(isin));
            continue;
        }
        match heute.alter_s(&isin) {
            Some(alter) if alter <= grenzen.kurs_hoechstalter_s() => {}
            _ => {
                fehler.push(DatenFehler::KursZuAlt(isin));
                continue;
            }
        }
        let Some(alt) = vortag.kurs(&isin) else {
            fehler.push(DatenFehler::KeinVergleichskurs(isin));
            continue;
        };
        let sprung_bp = kurs.checked_sub(alt).and_then(|d| d.abs()?.in_bp_von(alt));
        match sprung_bp {
            Some(bp) if bp <= grenzen.max_kurssprung_bp() => {}
            Some(bp) => fehler.push(DatenFehler::Kurssprung {
                isin,
                sprung_bp: bp,
            }),
            None => fehler.push(DatenFehler::KursNichtPositiv(isin)),
        }
    }
    fehler
}
