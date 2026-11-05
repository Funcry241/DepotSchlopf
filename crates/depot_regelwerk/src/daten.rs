//! Plausibilitaetspruefung der Kurse.
//!
//! `pruefe_kurse` prueft jeden Wert eines Universums; bei jedem Fehler faellt der Zyklus aus.
//! `waehle_tageswerte` teilt das fuer den Papiertag: Pflicht sind nur die gehaltenen Werte,
//! denn ohne ihren Kurs ist das Depot nicht zu bewerten. Ein Kandidat ohne gueltigen Kurs ist an
//! diesem Tag nur nicht handelbar; er faellt aus dem Universum des Tages, und der Grund steht im
//! Journal. Faellt der Abruf selbst durch, faellt der Zyklus aus.
//!
//! Jeder Wert des Universums braucht einen Kurs vom Handelstag des Zyklus, der nicht aelter
//! als das Hoechstalter ist, und einen Vergleichskurs von einem frueheren Handelstag. Auch der
//! Abruf liegt am Handelstag; sonst gaelte ein alter Kurssatz an seinem eigenen Zeitstempel als
//! frisch (K15-02). Fehlt der Vergleichskurs, ist der Sprungtest nicht
//! bestanden, sondern ein eigener Fehler (R20, 1·F08).

use std::collections::BTreeSet;

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
    /// Kein Wert hat einen gueltigen Kurs, und nichts wird gehalten: Es gibt nichts zu
    /// entscheiden, niemand wird gefragt.
    KeinWertHandelbar,
}

impl DatenFehler {
    /// Der Wert, den der Befund betrifft; `None` fuer einen Befund zum ganzen Abruf.
    pub fn isin(&self) -> Option<&Isin> {
        match self {
            DatenFehler::KursFehlt(i)
            | DatenFehler::KursNichtPositiv(i)
            | DatenFehler::Kurssprung { isin: i, .. }
            | DatenFehler::KeinVergleichskurs(i)
            | DatenFehler::KursZuAlt(i)
            | DatenFehler::KursNichtAmHandelstag(i)
            | DatenFehler::VergleichskursNichtVorher(i) => Some(i),
            DatenFehler::AbrufNichtAmHandelstag
            | DatenFehler::VergleichsabrufNachAbruf
            | DatenFehler::KeinWertHandelbar => None,
        }
    }
}

/// Die Werte eines Tages nach der Kurspruefung.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tageswerte {
    /// Die handelbaren Werte des Tages: alle mit gueltigem Kurs, in der Reihenfolge des
    /// Universums.
    pub universum: Universum,
    /// Kandidaten, die heute nicht handelbar sind, mit Grund. Der Zyklus laeuft ohne sie.
    pub ausgelassen: Vec<DatenFehler>,
    /// Befunde, an denen der Zyklus ausfaellt: zum Abruf, zu einem gehaltenen Wert, oder kein
    /// Wert handelbar. Leer = der Zyklus laeuft.
    pub fehler: Vec<DatenFehler>,
}

/// Prueft die Kurse eines Tages wie `pruefe_kurse` und teilt die Befunde: Ein Befund zu einem
/// gehaltenen Wert oder zum Abruf laesst den Zyklus ausfallen; ein Befund zu einem Kandidaten
/// nimmt nur diesen aus dem Universum des Tages. Ein gehaltener Wert ausserhalb des Universums
/// (zwingender Ausstieg) wird wie bisher nicht hier geprueft; ohne seinen Kurs ist das Depot
/// nicht zu bewerten, und der Waechter haelt an.
pub fn waehle_tageswerte(
    grenzen: &Grenzen,
    universum: &Universum,
    gehalten: &BTreeSet<Isin>,
    handelstag: Datum,
    heute: &Kurse,
    vortag: &Kurse,
) -> Tageswerte {
    let mut fehler = Vec::new();
    let mut ausgelassen = Vec::new();
    for befund in pruefe_kurse(grenzen, universum, handelstag, heute, vortag) {
        match befund.isin() {
            Some(i) if !gehalten.contains(i) => ausgelassen.push(befund),
            _ => fehler.push(befund),
        }
    }
    let raus: BTreeSet<&Isin> = ausgelassen.iter().filter_map(DatenFehler::isin).collect();
    let tageswerte = Universum::neu(
        universum
            .instrumente()
            .iter()
            .filter(|i| !raus.contains(&i.isin))
            .cloned()
            .collect(),
    );
    if fehler.is_empty() && tageswerte.instrumente().is_empty() {
        fehler.push(DatenFehler::KeinWertHandelbar);
    }
    Tageswerte {
        universum: tageswerte,
        ausgelassen,
        fehler,
    }
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
        if kurs.fein() <= 0 {
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
