//! Plausibilitaetspruefung der Kurse. Bei jedem Fehler faellt der Zyklus aus.

use depot_kern::{Grenzen, Isin, Kurse, Universum};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DatenFehler {
    KursFehlt(Isin),
    KursNichtPositiv(Isin),
    Kurssprung { isin: Isin, sprung_bp: i64 },
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
        let Some(alt) = vortag.kurs(&isin) else {
            continue;
        };
        let sprung_bp = kurs.checked_sub(alt).and_then(|d| d.abs().in_bp_von(alt));
        match sprung_bp {
            Some(bp) if bp <= grenzen.max_kurssprung_bp => {}
            Some(bp) => fehler.push(DatenFehler::Kurssprung {
                isin,
                sprung_bp: bp,
            }),
            None => fehler.push(DatenFehler::KursNichtPositiv(isin)),
        }
    }
    fehler
}
