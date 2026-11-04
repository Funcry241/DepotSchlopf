//! Plausibilitaetspruefung der Kurse. Bei jedem Fehler faellt der Zyklus aus.
//!
//! Jeder Wert des Universums braucht einen Kurs vom Handelstag des Zyklus, der nicht aelter
//! als das Hoechstalter ist, und einen Vergleichskurs von einem frueheren Handelstag. Auch der
//! Abruf liegt am Handelstag; sonst gaelte ein alter Kurssatz an seinem eigenen Zeitstempel als
//! frisch (K15-02). Fehlt der Vergleichskurs, ist der Sprungtest nicht
//! bestanden, sondern ein eigener Fehler (R20, 1·F08).

use depot_kern::{Datum, Grenzen, Isin, Kurse, Universum};

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
    /// Der Abruf liegt nicht am Handelstag des Zyklus (K15-02).
    AbrufNichtAmHandelstag,
    /// Handelstag oder Zeitpunkt des Kurses ist nicht der Handelstag des Zyklus (K15-02).
    KursNichtAmHandelstag(Isin),
    /// Der Vergleichskurs stammt nicht von einem frueheren Handelstag (K15-02).
    VergleichskursNichtVorher(Isin),
    /// Der Vergleichsabruf liegt nach dem Abruf des Zyklus: Seine Kurse lagen damals noch
    /// nicht vor (K15-03).
    VergleichsabrufNachAbruf,
}

/// Prueft alle Werte des Universums. Leere Liste = alles in Ordnung.
pub fn pruefe_kurse(
    grenzen: &Grenzen,
    universum: &Universum,
    handelstag: Datum,
    heute: &Kurse,
    vortag: &Kurse,
) -> Vec<DatenFehler> {
    let mut fehler = Vec::new();
    // Alter misst gegen den Abruf; der Abruf selbst muss am Handelstag liegen (K15-02).
    if !heute.stand().am_tag(handelstag) {
        fehler.push(DatenFehler::AbrufNichtAmHandelstag);
    }
    // Nichts zum Vergleich, was es zum Abruf noch nicht gab (K15-03). Jeder Vergleichskurs liegt
    // an seinem Handelstag (`Kurs::neu`) und damit vor dem Handelstag des Zyklus.
    if vortag.stand() > heute.stand() {
        fehler.push(DatenFehler::VergleichsabrufNachAbruf);
    }
    for isin in universum.isins() {
        let Some(eintrag) = heute.eintrag(&isin) else {
            fehler.push(DatenFehler::KursFehlt(isin));
            continue;
        };
        let kurs = eintrag.preis();
        if kurs.0 <= 0 {
            fehler.push(DatenFehler::KursNichtPositiv(isin));
            continue;
        }
        if eintrag.handelstag() != handelstag || !eintrag.zeitpunkt().am_tag(handelstag) {
            fehler.push(DatenFehler::KursNichtAmHandelstag(isin));
            continue;
        }
        match heute.alter_s(&isin) {
            Some(alter) if alter <= grenzen.kurs_hoechstalter_s() => {}
            _ => {
                fehler.push(DatenFehler::KursZuAlt(isin));
                continue;
            }
        }
        let Some(alt) = vortag.eintrag(&isin) else {
            fehler.push(DatenFehler::KeinVergleichskurs(isin));
            continue;
        };
        if alt.handelstag() >= handelstag {
            fehler.push(DatenFehler::VergleichskursNichtVorher(isin));
            continue;
        }
        let alt = alt.preis();
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
