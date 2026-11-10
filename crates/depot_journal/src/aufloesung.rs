//! Auflösung und Auswertung der Weltprognosen (Issue #114, docs/MATHEMATIK.md).
//!
//! Eine Prognose wird nach ihrem Horizont aufgelöst, deterministisch und ohne KI: Zieltag ist
//! der `handelstage`-te Handelstag nach dem Prognosetag im Kalender der Börse des Werts. Es
//! zählt nur der belegte Schlusskurs genau dieses Tages; ein anderer Tag ersetzt ihn nie.
//! Gleichstand zählt als verfehlt. Ohne Kurs zählt die Prognose in keiner Quote.
//!
//! Die Auswertung ist ein Hinweis, nie eine Regel: Trefferquote mit Wilson-Intervall (95 %)
//! gegen die Quote, die „steigt immer“ auf denselben Fällen erreicht hätte. Gerechnet wird
//! ganzzahlig; die Untergrenze wird ab-, die Obergrenze aufgerundet.

use depot_kern::{BeraterId, Datum, Isin, Preis};

use crate::weltprognose::{PrognoseBeleg, Prognoserichtung};

/// Wie eine Prognose ausging.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ergebnis {
    Eingetroffen,
    Verfehlt,
    /// Am Zieltag liegt kein belegter Kurs vor; zählt in keiner Quote.
    OhneKurs,
}

impl Ergebnis {
    pub fn name(self) -> &'static str {
        match self {
            Self::Eingetroffen => "eingetroffen",
            Self::Verfehlt => "verfehlt",
            Self::OhneKurs => "ohne_kurs",
        }
    }

    fn lies(text: &str) -> Option<Self> {
        [Self::Eingetroffen, Self::Verfehlt, Self::OhneKurs]
            .into_iter()
            .find(|e| e.name() == text)
    }
}

/// Die Auflösung einer Prognose. Schlüssel ist `(datum, berater, isin, ereignis_nr)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Aufloesung {
    pub datum: Datum,
    pub berater: BeraterId,
    pub isin: Isin,
    pub ereignis_nr: usize,
    pub zieltag: Datum,
    pub schluss: Option<Preis>,
    pub ergebnis: Ergebnis,
}

fn berater_name(b: BeraterId) -> &'static str {
    match b {
        BeraterId::Dachs => "Dachs",
        BeraterId::Maus => "Maus",
    }
}

impl Aufloesung {
    /// Gehört diese Auflösung zu dieser Prognose?
    pub fn zu(&self, p: &PrognoseBeleg) -> bool {
        self.datum == p.datum
            && self.berater == p.berater
            && self.isin == p.prognose.isin
            && self.ereignis_nr == p.prognose.ereignis_nr
    }

    pub(crate) fn text(&self) -> String {
        format!(
            "{};{};{};{};{};{};{}",
            self.datum,
            berater_name(self.berater),
            self.isin,
            self.ereignis_nr,
            self.zieltag,
            self.schluss
                .map_or_else(|| "-".to_string(), Preis::cent_text),
            self.ergebnis.name()
        )
    }

    pub(crate) fn lies(text: &str) -> Option<Self> {
        let teile: Vec<&str> = text.split(';').collect();
        let [datum, berater, isin, nr, zieltag, schluss, ergebnis] = teile.as_slice() else {
            return None;
        };
        let a = Self {
            datum: Datum::lies(datum)?,
            berater: match *berater {
                "Dachs" => BeraterId::Dachs,
                "Maus" => BeraterId::Maus,
                _ => return None,
            },
            isin: Isin::neu(isin).ok()?,
            ereignis_nr: nr.parse().ok()?,
            zieltag: Datum::lies(zieltag)?,
            schluss: match *schluss {
                "-" => None,
                s => Some(Preis::lies_cent_text(s)?),
            },
            ergebnis: Ergebnis::lies(ergebnis)?,
        };
        (a.text() == text).then_some(a)
    }
}

/// Ist der Tag ein Handelstag der Börse des Werts? Ohne bekannte Börse zählt jeder Tag, an
/// dem eine Börse der Liste offen ist.
fn handelstag_fuer(isin: &Isin, d: Datum) -> bool {
    match depot_kern::liste::boerse_von(isin) {
        Some(b) => b.handelstag(d),
        None => depot_kern::liste::handelstag(d),
    }
}

/// Der `handelstage`-te Handelstag nach `datum` an der Börse des Werts.
pub fn zieltag(isin: &Isin, datum: Datum, handelstage: u8) -> Option<Datum> {
    let mut d = datum;
    let mut rest = handelstage;
    // Mehr als 400 Kalendertage für höchstens 20 Handelstage gibt es nicht.
    for _ in 0..400 {
        if rest == 0 {
            return Some(d);
        }
        d = d.plus_tage(1)?;
        if handelstag_fuer(isin, d) {
            rest = rest.checked_sub(1)?;
        }
    }
    None
}

/// Das Ergebnis einer Prognose bei diesem Schlusskurs am Zieltag.
pub fn bewerte(p: &PrognoseBeleg, schluss: Option<Preis>) -> Ergebnis {
    match (schluss, p.prognose.richtung) {
        (None, _) => Ergebnis::OhneKurs,
        (Some(s), Prognoserichtung::Steigt) if s > p.ausgangskurs => Ergebnis::Eingetroffen,
        (Some(s), Prognoserichtung::Faellt) if s < p.ausgangskurs => Ergebnis::Eingetroffen,
        (Some(_), _) => Ergebnis::Verfehlt,
    }
}

/// Wilson-Intervall (95 %, $z^2 = 3{,}8416$) für `k` Treffer in `n` Fällen in Basispunkten.
/// Die Untergrenze ist ab-, die Obergrenze aufgerundet. `None` ohne Fälle.
pub fn wilson_bp(n: u64, k: u64) -> Option<(u32, u32)> {
    if n == 0 || k > n {
        return None;
    }
    let (n, k) = (u128::from(n), u128::from(k));
    // Alles mal 10.000: z² = 38.416 / 10.000.
    let mitte = k.checked_mul(10_000)?.checked_add(19_208)?;
    let nenner = n.checked_mul(10_000)?.checked_add(38_416)?;
    // (10.000 · z · √(k(n−k)/n + z²/4))² = (384.160.000 · k(n−k) + 368.947.264 · n) / n
    let zaehler = k
        .checked_mul(n.checked_sub(k)?)?
        .checked_mul(384_160_000)?
        .checked_add(n.checked_mul(368_947_264)?)?;
    let quadrat = zaehler.div_ceil(n);
    let mut wurzel = quadrat.isqrt();
    if wurzel.checked_mul(wurzel)? < quadrat {
        wurzel = wurzel.checked_add(1)?;
    }
    let unten = mitte
        .checked_sub(wurzel)
        .and_then(|d| d.checked_mul(10_000))
        .and_then(|d| d.checked_div(nenner))
        .unwrap_or(0);
    let oben = mitte
        .checked_add(wurzel)?
        .checked_mul(10_000)?
        .div_ceil(nenner)
        .min(10_000);
    Some((u32::try_from(unten).ok()?, u32::try_from(oben).ok()?))
}

/// Die Auswertung einer Menge aufgelöster Prognosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Auswertung {
    /// Fälle mit Ergebnis eingetroffen oder verfehlt.
    pub faelle: u64,
    pub treffer: u64,
    pub quote_bp: u32,
    pub wilson_unten_bp: u32,
    pub wilson_oben_bp: u32,
    /// Die Quote von „steigt immer“ auf denselben Fällen.
    pub naiv_bp: u32,
}

impl Auswertung {
    /// Nur dann besser als naiv, wenn die Untergrenze über der naiven Quote liegt.
    pub fn besser_als_naiv(&self) -> bool {
        self.wilson_unten_bp > self.naiv_bp
    }

    /// Der Hinweistext für Lagebild und Live.
    pub fn hinweis(&self) -> String {
        let prozent = |bp: u32| {
            let (ganz, rest) = (
                bp.checked_div(100).unwrap_or(0),
                bp.checked_rem(100).unwrap_or(0),
            );
            format!("{ganz},{rest:02} %")
        };
        let urteil = if self.besser_als_naiv() {
            "besser als naiv".to_string()
        } else {
            format!("noch nicht unterscheidbar ({} Fälle)", self.faelle)
        };
        format!(
            "{} von {} eingetroffen ({}, 95 %: {} bis {}); „steigt immer“: {}; {urteil}",
            self.treffer,
            self.faelle,
            prozent(self.quote_bp),
            prozent(self.wilson_unten_bp),
            prozent(self.wilson_oben_bp),
            prozent(self.naiv_bp)
        )
    }
}

/// Wertet die Auflösungen eines Beraters aus (`None`: beide zusammen). Die Richtung jeder
/// Prognose steht in `prognosen`; eine Auflösung ohne Prognose zählt nicht.
pub fn werte_aus(
    prognosen: &[PrognoseBeleg],
    aufloesungen: &[Aufloesung],
    berater: Option<BeraterId>,
) -> Option<Auswertung> {
    let mut faelle = 0_u64;
    let mut treffer = 0_u64;
    let mut naiv = 0_u64;
    for a in aufloesungen {
        if berater.is_some_and(|b| b != a.berater) || a.ergebnis == Ergebnis::OhneKurs {
            continue;
        }
        let Some(p) = prognosen.iter().find(|p| a.zu(p)) else {
            continue;
        };
        let Some(schluss) = a.schluss else { continue };
        faelle = faelle.checked_add(1)?;
        if a.ergebnis == Ergebnis::Eingetroffen {
            treffer = treffer.checked_add(1)?;
        }
        if schluss > p.ausgangskurs {
            naiv = naiv.checked_add(1)?;
        }
    }
    let quote = |x: u64| u32::try_from(x.checked_mul(10_000)?.checked_div(faelle)?).ok();
    let (unten, oben) = wilson_bp(faelle, treffer)?;
    Some(Auswertung {
        faelle,
        treffer,
        quote_bp: quote(treffer)?,
        wilson_unten_bp: unten,
        wilson_oben_bp: oben,
        naiv_bp: quote(naiv)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prüfwerte aus Issue #114. Die Untergrenze ist nie zu hoch, die Obergrenze nie zu
    /// niedrig.
    #[test]
    fn wilson_trifft_die_pruefwerte() {
        assert_eq!(wilson_bp(10, 7), Some((3967, 8923)));
        assert_eq!(wilson_bp(20, 10), Some((2992, 7008)));
        assert_eq!(wilson_bp(100, 60), Some((5020, 6907)));
        assert_eq!(wilson_bp(1, 1), Some((2065, 10_000)));
        assert_eq!(wilson_bp(1, 0), Some((0, 7935)));
        assert_eq!(wilson_bp(0, 0), None);
        assert_eq!(wilson_bp(3, 4), None);
    }

    /// Das Intervall enthält immer die Quote, liegt in 0 bis 10.000 und wird mit mehr Fällen
    /// bei gleicher Quote enger.
    #[test]
    fn wilson_enthaelt_die_quote_und_wird_enger() -> Result<(), String> {
        for n in 1..=200_u64 {
            for k in 0..=n {
                let (u, o) = wilson_bp(n, k).ok_or(format!("{n} {k}"))?;
                let quote = k
                    .checked_mul(10_000)
                    .and_then(|x| x.checked_div(n))
                    .ok_or("Rechnung")?;
                if u64::from(u) > quote || quote > u64::from(o) || o > 10_000 {
                    return Err(format!("{n} {k}: {u} bis {o}, Quote {quote}"));
                }
            }
        }
        let breite = |n: u64| wilson_bp(n, n.checked_div(2)?).and_then(|(u, o)| o.checked_sub(u));
        if !(breite(400) < breite(100) && breite(100) < breite(20)) {
            return Err("wird nicht enger".into());
        }
        Ok(())
    }

    #[test]
    fn aufloesung_liest_sich_wie_geschrieben() -> Result<(), String> {
        let datum = Datum::neu(2026, 11, 2).ok_or("Datum")?;
        let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        for (schluss, ergebnis) in [
            (Some(Preis::aus_fein(2_300_001)), Ergebnis::Eingetroffen),
            (Some(Preis::aus_fein(1)), Ergebnis::Verfehlt),
            (None, Ergebnis::OhneKurs),
        ] {
            let a = Aufloesung {
                datum,
                berater: BeraterId::Maus,
                isin: isin.clone(),
                ereignis_nr: 3,
                zieltag: datum,
                schluss,
                ergebnis,
            };
            if Aufloesung::lies(&a.text()).as_ref() != Some(&a) {
                return Err(format!("{a:?}"));
            }
        }
        if Aufloesung::lies("2026-11-02;Maus;DE0007164600;3;2026-11-02;-;anders").is_some() {
            return Err("unbekanntes Ergebnis gelesen".into());
        }
        Ok(())
    }
}
