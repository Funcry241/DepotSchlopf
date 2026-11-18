//! Auswertung unabhängiger Relevanzurteile für eine These und ihre Belegliste (#325).

use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Richtung {
    Stuetzt,
    Widerlegt,
    Offen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Urteil {
    KeinBezug,
    Bezug,
    Entscheidend(Richtung),
}

impl Urteil {
    fn gewinn(self) -> u32 {
        match self {
            Self::KeinBezug => 0,
            Self::Bezug => 1,
            Self::Entscheidend(_) => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Beleg {
    pub urteil: Option<Urteil>,
    pub pflicht: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vergleich {
    KeineKandidaten,
    /// Höchstens drei frei sortierbare Belege. Jede Reihenfolge erreicht die Top drei.
    NichtTrennscharf,
    /// Die Rangfolge enthält mindestens einen frei sortierbaren Beleg ohne Urteil.
    Unbewertet,
    OhneRelevantenBeleg,
    /// nDCG@3 in Basispunkten (10.000 entspricht 1,0).
    Bewertet(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Messung {
    pub vergleich: Vergleich,
    pub belege_ohne_urteil: usize,
    pub relevanter_treffer: bool,
    pub entscheidende_unter_drei: usize,
    pub entscheidende_gesamt: usize,
    pub gegenbelege_unter_drei: usize,
    pub gegenbelege_gesamt: usize,
}

/// 1 / log2(Rang + 1), gerundet auf Millionstel. Die Multiplikatoren bilden die
/// nDCG-Gewinne 2^Stufe - 1 unabhängig von der Richtung des Belegs ab.
fn dcg(gewinne: impl IntoIterator<Item = u32>) -> u32 {
    gewinne
        .into_iter()
        .zip([1_000_000_u32, 630_930, 500_000])
        .map(|(gewinn, gewicht)| gewinn.saturating_mul(gewicht))
        .sum()
}

/// Wertet die Rangfolge als vollständige Vertauschung derselben Kandidaten aus.
/// Pflichtbelege sind für alle Sortierer gesetzt und belegen in der Messung keinen
/// Platz. Enthält der Fall offene Urteile, bleiben die belegten Treffer berichtbar.
/// Ein Punktwert wird erst berechnet, wenn für alle frei sortierbaren Belege ein
/// Urteil vorliegt, weil ein unbekanntes Urteil auch den Idealrang ändern kann.
///
/// `None` kennzeichnet fehlerhafte Indizes, fehlende oder doppelte Kandidaten.
pub fn messe(belege: &[Beleg], rang: &[usize]) -> Option<Messung> {
    if rang.len() != belege.len() {
        return None;
    }
    let mut gesehen = BTreeSet::new();
    let mut geordnet = Vec::new();
    for &index in rang {
        if !gesehen.insert(index) {
            return None;
        }
        let beleg = belege.get(index)?;
        if !beleg.pflicht {
            geordnet.push(beleg.urteil);
        }
    }

    let oben = geordnet.iter().take(3).flatten().copied();
    let alle = geordnet.iter().flatten().copied();
    let entscheidend = |u| matches!(u, Urteil::Entscheidend(_));
    let gegenbeleg = |u| u == Urteil::Entscheidend(Richtung::Widerlegt);
    let belege_ohne_urteil = geordnet.len().saturating_sub(alle.clone().count());
    let vergleich = if geordnet.is_empty() {
        Vergleich::KeineKandidaten
    } else if geordnet.len() <= 3 {
        Vergleich::NichtTrennscharf
    } else if belege_ohne_urteil != 0 {
        Vergleich::Unbewertet
    } else {
        let mut ideal: Vec<_> = alle.clone().map(Urteil::gewinn).collect();
        ideal.sort_unstable_by_key(|g| std::cmp::Reverse(*g));
        let norm = u64::from(dcg(ideal));
        let wert = u64::from(dcg(oben.clone().map(Urteil::gewinn)));
        wert.saturating_mul(10_000)
            .saturating_add(norm / 2)
            .checked_div(norm)
            .and_then(|p| u32::try_from(p).ok())
            .map_or(Vergleich::OhneRelevantenBeleg, Vergleich::Bewertet)
    };
    Some(Messung {
        vergleich,
        belege_ohne_urteil,
        relevanter_treffer: oben.clone().any(|u| u.gewinn() > 0),
        entscheidende_unter_drei: oben.clone().filter(|u| entscheidend(*u)).count(),
        entscheidende_gesamt: alle.clone().filter(|u| entscheidend(*u)).count(),
        gegenbelege_unter_drei: oben.filter(|u| gegenbeleg(*u)).count(),
        gegenbelege_gesamt: alle.filter(|u| gegenbeleg(*u)).count(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beleg(urteil: Urteil) -> Beleg {
        Beleg {
            urteil: Some(urteil),
            pflicht: false,
        }
    }

    #[test]
    fn gegenbeleg_zaehlt_gleich_und_verdrängung_bleibt_sichtbar() -> Result<(), String> {
        let belege = [
            beleg(Urteil::Entscheidend(Richtung::Widerlegt)),
            beleg(Urteil::Entscheidend(Richtung::Stuetzt)),
            beleg(Urteil::Bezug),
            beleg(Urteil::Bezug),
            beleg(Urteil::KeinBezug),
        ];
        let referenz = messe(&belege, &[0, 1, 2, 3, 4]).ok_or("Referenz")?;
        let umgekehrt = messe(&belege, &[1, 0, 2, 3, 4]).ok_or("Umgekehrt")?;
        assert_eq!(referenz.vergleich, Vergleich::Bewertet(10_000));
        assert_eq!(referenz, umgekehrt);

        // Der Gesamtwert steigt, obwohl der entscheidende Gegenbeleg aus den Top drei fällt.
        let vorher = messe(&belege, &[4, 2, 0, 3, 1]).ok_or("Vorher")?;
        let nachher = messe(&belege, &[1, 2, 3, 0, 4]).ok_or("Nachher")?;
        assert_eq!(vorher.vergleich, Vergleich::Bewertet(3_951));
        assert_eq!(nachher.vergleich, Vergleich::Bewertet(7_660));
        assert!(vorher.relevanter_treffer && nachher.relevanter_treffer);
        assert_eq!(vorher.gegenbelege_unter_drei, 1);
        assert_eq!(nachher.gegenbelege_unter_drei, 0);
        assert_eq!(nachher.gegenbelege_gesamt, 1);
        Ok(())
    }

    #[test]
    fn offene_urteile_und_kurze_listen_erhalten_keine_quote() -> Result<(), String> {
        let kein_bezug = beleg(Urteil::KeinBezug);
        let offen = Beleg {
            urteil: None,
            pflicht: false,
        };
        let belege = [kein_bezug, offen, beleg(Urteil::Bezug), kein_bezug];
        let m = messe(&belege, &[0, 1, 2, 3]).ok_or("Unbewertet")?;
        assert_eq!(m.vergleich, Vergleich::Unbewertet);
        assert_eq!(m.belege_ohne_urteil, 1);
        assert!(m.relevanter_treffer);

        let leer = messe(&[], &[]).ok_or("Leer")?;
        assert_eq!(leer.vergleich, Vergleich::KeineKandidaten);
        assert!(!leer.relevanter_treffer);
        let kurz = messe(&[kein_bezug, offen], &[0, 1]).ok_or("Kurz")?;
        assert_eq!(kurz.vergleich, Vergleich::NichtTrennscharf);
        let ohne = messe(&[kein_bezug; 4], &[0, 1, 2, 3]).ok_or("Ohne Treffer")?;
        assert_eq!(ohne.vergleich, Vergleich::OhneRelevantenBeleg);
        Ok(())
    }

    #[test]
    fn pflichtbelege_verbrauchen_keinen_rangplatz() -> Result<(), String> {
        let kein_bezug = beleg(Urteil::KeinBezug);
        let belege = [
            Beleg {
                urteil: None,
                pflicht: true,
            },
            kein_bezug,
            kein_bezug,
            beleg(Urteil::Entscheidend(Richtung::Widerlegt)),
            kein_bezug,
        ];
        let m = messe(&belege, &[0, 1, 2, 3, 4]).ok_or("Pflichtbeleg")?;
        assert_eq!(m.vergleich, Vergleich::Bewertet(5_000));
        assert_eq!(m.belege_ohne_urteil, 0);
        assert_eq!(m.gegenbelege_unter_drei, 1);
        assert_eq!(messe(&belege, &[1, 2, 3, 4, 0]), Some(m));
        assert_eq!(messe(&belege, &[0, 1, 1, 3, 4]), None);
        assert_eq!(messe(&belege, &[0, 1, 2, 3, 5]), None);
        assert_eq!(messe(&belege, &[0, 1, 2, 3]), None);
        Ok(())
    }
}
