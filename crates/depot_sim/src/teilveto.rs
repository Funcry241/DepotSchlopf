//! Veto je Wert (#311 H4, P43): Die Zweitmeinung kann einzelne Werte eines Vorschlags ablehnen.
//!
//! Vorarbeit bis P35 auf `main` ist: die reine Regel mit ihren Gegenproben. Eingebunden wird sie
//! erst mit P43, zusammen mit dem Feld `abgelehnt` im Urteil.
//!
//! Regel: Nur ein ausdrückliches `true` ist Zustimmung. Ein Nein mit einer gültigen Liste
//! abgelehnter Werte streicht genau diese Werte; alle übrigen bleiben unverändert. Fehlt die
//! Liste, ist sie leer, nennt sie einen Wert, der nicht im Vorschlag steht, oder streicht sie
//! alles, gilt das Veto für den ganzen Vorschlag. Ein Ja mit Ablehnungsliste widerspricht sich
//! und gilt ebenfalls als Veto. Ein halber Kauf trotz Veto ist nicht freigegeben.

use depot_kern::{Isin, Vorschlag};

/// Was aus einem Urteil der Zweitmeinung folgt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Urteilsfolge {
    Zustimmung,
    /// Die genannten Werte fallen heraus, der Rest des Vorschlags gilt.
    Teilveto {
        vorschlag: Box<Vorschlag>,
        abgelehnt: Vec<Isin>,
    },
    Veto,
}

/// Wendet ein verwertbares Urteil auf den Vorschlag an.
pub fn urteilsfolge(v: &Vorschlag, zustimmung: bool, abgelehnt: Option<&[Isin]>) -> Urteilsfolge {
    let liste = abgelehnt.unwrap_or_default();
    if zustimmung {
        return if liste.is_empty() {
            Urteilsfolge::Zustimmung
        } else {
            Urteilsfolge::Veto
        };
    }
    let genannt = |isin: &Isin| {
        v.zielgewichte_bp.contains_key(isin) || v.park.as_ref().is_some_and(|p| &p.isin == isin)
    };
    if liste.is_empty() || !liste.iter().all(genannt) {
        return Urteilsfolge::Veto;
    }
    let mut rest = v.clone();
    for isin in liste {
        rest.zielgewichte_bp.remove(isin);
        rest.limit_bp.remove(isin);
        rest.stopp_bp.remove(isin);
        rest.erwartung_bp.remove(isin);
        rest.begruendung.remove(isin);
        if rest.park.as_ref().is_some_and(|p| &p.isin == isin) {
            rest.park = None;
        }
    }
    if rest.zielgewichte_bp.is_empty() && rest.park.is_none() {
        return Urteilsfolge::Veto;
    }
    let mut abgelehnt = liste.to_vec();
    abgelehnt.sort();
    abgelehnt.dedup();
    Urteilsfolge::Teilveto {
        vorschlag: Box::new(rest),
        abgelehnt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_kern::BeraterId;
    use std::collections::BTreeMap;

    const A: &str = "DE0007164600";
    const B: &str = "DE0008404005";
    const C: &str = "DE000BASF111";
    const FREMD: &str = "DE000BAY0017";

    fn isin(s: &str) -> Result<Isin, String> {
        Isin::neu(s).map_err(|e| format!("{e:?}"))
    }

    fn vorschlag() -> Result<Vorschlag, String> {
        let mut v = Vorschlag {
            erwartung_bp: BTreeMap::new(),
            limit_bp: BTreeMap::new(),
            stopp_bp: BTreeMap::new(),
            berater: BeraterId::Dachs,
            zielgewichte_bp: BTreeMap::new(),
            begruendung: BTreeMap::new(),
            park: None,
        };
        for s in [A, B, C] {
            v.zielgewichte_bp.insert(isin(s)?, 2_000);
            v.stopp_bp.insert(isin(s)?, 500);
            v.begruendung.insert(isin(s)?, "beobachtet".into());
        }
        Ok(v)
    }

    /// Gegenprobe (a): Veto nur gegen C, A und B bleiben unverändert im Vorschlag.
    #[test]
    fn veto_nur_gegen_c() -> Result<(), String> {
        let v = vorschlag()?;
        let folge = urteilsfolge(&v, false, Some(&[isin(C)?]));
        let Urteilsfolge::Teilveto {
            vorschlag: rest,
            abgelehnt,
        } = folge
        else {
            return Err(format!("kein Teilveto: {folge:?}"));
        };
        assert_eq!(abgelehnt, vec![isin(C)?]);
        let mut erwartet = v.clone();
        erwartet.zielgewichte_bp.remove(&isin(C)?);
        erwartet.stopp_bp.remove(&isin(C)?);
        erwartet.begruendung.remove(&isin(C)?);
        assert_eq!(*rest, erwartet);
        Ok(())
    }

    /// Gegenprobe (c): Veto ohne Wertangabe gilt wie bisher für alle.
    #[test]
    fn veto_ohne_liste_gilt_fuer_alle() -> Result<(), String> {
        let v = vorschlag()?;
        assert_eq!(urteilsfolge(&v, false, None), Urteilsfolge::Veto);
        assert_eq!(urteilsfolge(&v, false, Some(&[])), Urteilsfolge::Veto);
        Ok(())
    }

    /// Ein fremder Wert in der Liste: Veto für den ganzen Vorschlag.
    #[test]
    fn fremder_wert_verwirft_alles() -> Result<(), String> {
        let v = vorschlag()?;
        assert_eq!(
            urteilsfolge(&v, false, Some(&[isin(C)?, isin(FREMD)?])),
            Urteilsfolge::Veto
        );
        Ok(())
    }

    /// Alles abgelehnt ist ein ganzes Veto; Ja mit Liste widerspricht sich.
    #[test]
    fn alles_abgelehnt_und_widerspruch() -> Result<(), String> {
        let v = vorschlag()?;
        assert_eq!(
            urteilsfolge(&v, false, Some(&[isin(A)?, isin(B)?, isin(C)?])),
            Urteilsfolge::Veto
        );
        assert_eq!(
            urteilsfolge(&v, true, Some(&[isin(C)?])),
            Urteilsfolge::Veto
        );
        assert_eq!(urteilsfolge(&v, true, None), Urteilsfolge::Zustimmung);
        Ok(())
    }
}
