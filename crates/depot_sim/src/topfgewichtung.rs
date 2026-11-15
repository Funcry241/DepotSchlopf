//! Gewichtung nach belegtem Euro-Mehrertrag mit einer reinen Bargeldumbuchung.

use depot_journal::Stand;
use depot_kern::{Cent, Grenzen, Kurse, Rundung, Topf, TopfId, Universum};

use crate::handelsergebnis::{handelspositionen, mehrertrag};
use crate::handelsvergleich::Handelsvergleich;

pub const BELEG: &str = "Topfgewichtung";
const ABSTAND: usize = 20;
const MIN_TRADES: usize = 10;
const MIN_ANTEIL_BP: i64 = 3_000;
const MAX_ANTEIL_BP: i64 = 7_000;

pub(crate) fn faellig(stand: &Stand) -> bool {
    let tage = stand.tagesschluesse().len();
    tage != 0 && tage.is_multiple_of(ABSTAND)
}

fn ziel(wert: Cent, a: Cent, b: Cent) -> Option<i64> {
    let differenz = a.checked_sub(b)?.in_bp_von(wert)?;
    Some(
        5_000_i64
            .saturating_add(differenz)
            .clamp(MIN_ANTEIL_BP, MAX_ANTEIL_BP),
    )
}

fn freier_anteil(topf: &Topf, kurse: &Kurse, grenzen: &Grenzen, frei: Cent) -> Option<Cent> {
    let wert = topf.wert(kurse)?;
    let groesste = topf
        .positionen()
        .iter()
        .try_fold(Cent::NULL, |groesste, (isin, stueck)| {
            Some(groesste.max(kurse.kurs(isin)?.mal_stueck(*stueck, Rundung::Auf)?))
        })?;
    let zaehler = i128::from(groesste.0).checked_mul(10_000)?;
    let nenner = i128::from(grenzen.max_anteil_bp());
    let mindestwert = zaehler
        .checked_add(nenner.checked_sub(1)?)?
        .checked_div(nenner)?;
    let puffer = wert
        .checked_sub(Cent(i64::try_from(mindestwert).ok()?))?
        .max(Cent::NULL);
    Some(frei.min(puffer))
}

pub(crate) fn umbuchen(
    stand: &Stand,
    kurse: &Kurse,
    grenzen: &Grenzen,
    frei: [Cent; 2],
) -> Result<Cent, &'static str> {
    if stand.offene_auftraege().next().is_some() {
        return Err("offene Aufträge");
    }
    let (verkaeufe, positionen) = handelspositionen(stand, grenzen);
    let korb = Handelsvergleich::neu(stand);
    let score = |id| -> Result<Cent, &'static str> {
        let trades: Vec<_> = verkaeufe
            .iter()
            .filter(|v| v.topf == id && v.netto.is_some())
            .collect();
        if trades.len() < MIN_TRADES {
            return Err("weniger als zehn abgeschlossene Trades je Topf");
        }
        let verkaeufe = trades.into_iter().map(|v| {
            mehrertrag(
                v.netto?,
                v.einstand?,
                korb.rendite(v.einstieg?, v.ausstieg?, grenzen)?,
            )
        });
        let gehalten = positionen.iter().filter(|p| p.topf == id).map(|p| {
            mehrertrag(
                p.netto()?,
                p.einstand?,
                korb.rendite(p.einstieg?, p.bewertungstag?, grenzen)?,
            )
        });
        verkaeufe
            .chain(gehalten)
            .try_fold(Cent::NULL, |summe, s| summe.checked_add(s?))
            .ok_or("vollständiger Euro-Mehrertrag nicht belegt")
    };
    let a = stand.toepfe().get(&TopfId::A).ok_or("Topf Dachs fehlt")?;
    let b = stand.toepfe().get(&TopfId::B).ok_or("Topf Maus fehlt")?;
    berechne(
        a,
        b,
        kurse,
        grenzen,
        frei,
        [score(TopfId::A)?, score(TopfId::B)?],
    )
    .ok_or("nicht genug freies Bargeld innerhalb der Positionsgrenzen")
}

fn berechne(
    a: &Topf,
    b: &Topf,
    kurse: &Kurse,
    grenzen: &Grenzen,
    frei: [Cent; 2],
    [score_a, score_b]: [Cent; 2],
) -> Option<Cent> {
    if score_a == score_b {
        return Some(Cent::NULL);
    }
    let wert_a = a.wert(kurse)?;
    let summe = wert_a.checked_add(b.wert(kurse)?)?;
    let ziel_bp = ziel(summe, score_a, score_b)?;
    let zielbetrag = if ziel_bp == MIN_ANTEIL_BP {
        summe.anteil_bp_auf(ziel_bp)?
    } else {
        summe.anteil_bp_ab(ziel_bp)?
    };
    let gewuenscht = zielbetrag.checked_sub(wert_a)?;
    let [frei_a, frei_b] = frei;
    let grenze = match gewuenscht.0.cmp(&0) {
        std::cmp::Ordering::Less => {
            Cent(freier_anteil(a, kurse, grenzen, frei_a)?.0.checked_neg()?)
        }
        std::cmp::Ordering::Equal => return Some(Cent::NULL),
        std::cmp::Ordering::Greater => freier_anteil(b, kurse, grenzen, frei_b)?,
    };
    let bewegt = if gewuenscht.0 < 0 {
        gewuenscht.max(grenze)
    } else {
        gewuenscht.min(grenze)
    };
    let neuer_wert = wert_a.checked_add(bewegt)?;
    let anteil = i128::from(neuer_wert.0).checked_mul(10_000)?;
    let gesamt = i128::from(summe.0);
    let minimum = gesamt.checked_mul(i128::from(MIN_ANTEIL_BP))?;
    let maximum = gesamt.checked_mul(i128::from(MAX_ANTEIL_BP))?;
    (bewegt != Cent::NULL && (minimum..=maximum).contains(&anteil)).then_some(bewegt)
}

/// Vergleich mit täglich wieder gleicher Kapitalzuordnung und den beobachteten Topfrenditen.
pub fn feste_haelfte(stand: &Stand, universum: &Universum) -> Option<Cent> {
    let start_a = stand.startgeld_je_topf(TopfId::A)?;
    let start_b = stand.startgeld_je_topf(TopfId::B)?;
    let start = start_a.checked_add(start_b)?;
    let (_, _, ende) = stand.tagesschluesse().iter().try_fold(
        (start_a, start_b, start),
        |(vor_a, vor_b, schatten), (tag, toepfe)| {
            let kurse = crate::papiertag_wiederabspielen(stand, universum, *tag)
                .ok()?
                .kurse;
            let wert_a = toepfe.get(&TopfId::A)?.wert(&kurse)?;
            let wert_b = toepfe.get(&TopfId::B)?.wert(&kurse)?;
            let umbuchung = stand
                .topf_umbuchungen()
                .get(tag)
                .copied()
                .unwrap_or(Cent::NULL);
            let a = wert_a
                .checked_sub(umbuchung)?
                .checked_sub(vor_a)?
                .in_bp_von(vor_a)?;
            let b = wert_b
                .checked_add(umbuchung)?
                .checked_sub(vor_b)?
                .in_bp_von(vor_b)?;
            let mittel = a.checked_add(b)?.checked_div_euclid(2)?;
            let faktor = 10_000_i64.checked_add(mittel)?;
            Some((wert_a, wert_b, schatten.anteil_bp_ab(faktor)?))
        },
    )?;
    ende.checked_sub(start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topfgewichtung_bleibt_bei_dreissig_bis_siebzig_prozent_und_nimmt_nur_bargeld()
    -> Result<(), String> {
        let dachs = Topf::neu(TopfId::A, Cent(500_000)).map_err(|e| format!("{e:?}"))?;
        let maus = Topf::neu(TopfId::B, Cent(500_000)).map_err(|e| format!("{e:?}"))?;
        let zeit = depot_kern::Zeitpunkt::lies("2026-11-09T20:00:00Z").ok_or("Zeit")?;
        let kurse = Kurse::neu(zeit, Vec::new()).map_err(|e| format!("{e:?}"))?;
        let grenzen = Grenzen::standard();
        let stark = [Cent(500_000), Cent::NULL];
        assert_eq!(
            berechne(
                &dachs,
                &maus,
                &kurse,
                &grenzen,
                [dachs.bargeld(), maus.bargeld()],
                stark
            ),
            Some(Cent(200_000))
        );
        assert_eq!(
            berechne(
                &dachs,
                &maus,
                &kurse,
                &grenzen,
                [Cent::NULL, Cent::NULL],
                stark
            ),
            None
        );
        assert_eq!(
            umbuchen(
                &Stand::default(),
                &kurse,
                &grenzen,
                [Cent::NULL, Cent::NULL]
            ),
            Err("Topf Dachs fehlt")
        );
        Ok(())
    }
}

#[cfg(test)]
mod anzahl_tests {
    use super::*;
    use depot_journal::{Art, Journal};

    #[test]
    fn neun_abgeschlossene_trades_loesen_keine_umbuchung_aus() -> Result<(), String> {
        let ordner = std::env::temp_dir().join(format!("depot-neun-trades-{}", std::process::id()));
        std::fs::create_dir(&ordner).map_err(|e| e.to_string())?;
        let pruefung = || -> Result<(), String> {
            let mut eintraege = vec![
                (Art::Topf, "A;500000;".into()),
                (Art::Topf, "B;500000;".into()),
            ];
            for nr in (1_u64..=18).step_by(2) {
                let verkauf = nr.saturating_add(1_u64);
                for (nr, richtung) in [(nr, "Kauf"), (verkauf, "Verkauf")] {
                    eintraege.extend([
                        (
                            Art::Auftrag,
                            format!("{nr};A;DE0007164600;{richtung};1;10000;Umschichtung"),
                        ),
                        (Art::Zustand, format!("{nr};gesendet")),
                        (Art::Zustand, format!("{nr};angenommen")),
                        (Art::Zustand, format!("{nr};ausfuehrung;1;10000;150")),
                    ]);
                }
            }
            let refs: Vec<_> = eintraege
                .iter()
                .map(|(art, t)| (*art, t.as_str()))
                .collect();
            let inhalt = depot_journal::inhalt_fuer(&refs);
            let vermerk = depot_journal::vermerk_fuer(&inhalt, 0).ok_or("Vermerk")?;
            let pfad = ordner.join("journal.txt");
            std::fs::write(&pfad, inhalt).map_err(|e| e.to_string())?;
            std::fs::write(ordner.join("journal.vermerk"), vermerk).map_err(|e| e.to_string())?;
            let journal = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
            let zeit = depot_kern::Zeitpunkt::lies("2026-11-09T20:00:00Z").ok_or("Zeit")?;
            let kurse = Kurse::neu(zeit, Vec::new()).map_err(|e| format!("{e:?}"))?;
            assert_eq!(crate::handelsergebnis::verkaeufe(journal.stand()).len(), 9);
            assert_eq!(
                umbuchen(
                    journal.stand(),
                    &kurse,
                    &Grenzen::standard(),
                    [Cent(400_000); 2]
                ),
                Err("weniger als zehn abgeschlossene Trades je Topf")
            );
            assert!(journal.stand().topf_umbuchungen().is_empty());
            Ok(())
        }();
        std::fs::remove_dir_all(ordner).map_err(|e| e.to_string())?;
        pruefung
    }
}
