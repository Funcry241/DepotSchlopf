//! Berechnet die Messlatte aus belegten Papiertagen.

use depot_journal::Stand;
use depot_kern::liste::{Wert, als_universum};
use depot_kern::messlatte::{self, Ergebnis};
use depot_kern::{Cent, Datum, TopfId};
use depot_sim::papierlauf::START_JE_TOPF;

use crate::tage::{TagStand, tage_bis};
use crate::vergleich::VergleichLage;

#[derive(Clone, Debug)]
pub struct MesslatteTopf {
    pub topf: TopfId,
    pub tage: usize,
    pub vorsprung_bp: Option<i64>,
    pub rueckgang_bp: Option<i64>,
    pub ausfall_bp: Option<i64>,
    pub ergebnis: Ergebnis,
    pub netto: Option<crate::nettotrade::Bilanz>,
}

pub(crate) fn auswerten(
    stand: &Stand,
    werte: &[Wert],
    vergleich: &VergleichLage,
    anbieter: &[depot_journal::anbieter::Eintrag],
    heute: Datum,
) -> Vec<MesslatteTopf> {
    let netto = crate::nettotrade::bilanzen(stand);
    let verlauf = crate::depotverlauf::depotverlauf(stand, werte);
    let tage = tage_bis(stand, &als_universum(werte), anbieter, heute, usize::MAX);
    let handelstage = tage
        .iter()
        .filter(|t| !matches!(t.stand, TagStand::Geschlossen(_) | TagStand::Offen))
        .count();
    let massstab = vergleich.zeilen.get(2).and_then(|z| z.wert);
    [TopfId::A, TopfId::B]
        .into_iter()
        .enumerate()
        .map(|(i, topf)| {
            let vorsprung_bp = vergleich
                .zeilen
                .get(i)
                .and_then(|z| vorsprung_bp(z.wert, z.ki_kosten, massstab));
            let folge: Vec<_> = verlauf
                .iter()
                .map(|t| t.topfwerte.get(&topf).copied())
                .collect();
            let ausfaelle = tage
                .iter()
                .filter(|t| {
                    matches!(t.stand, TagStand::Ausgefallen(_) | TagStand::Angefangen)
                        || (matches!(
                            t.stand,
                            TagStand::Abgeschlossen | TagStand::MitAuslassung(_)
                        ) && verlauf
                            .binary_search_by_key(&t.datum, |punkt| punkt.tag)
                            .ok()
                            .and_then(|i| folge.get(i))
                            .is_some_and(Option::is_none))
                })
                .count();
            let ausfall_bp = messlatte::ausfall_bp(ausfaelle, handelstage);
            let rueckgang_bp = messlatte::rueckgang_bp(&folge, START_JE_TOPF);
            let tage = stand.tagesschluesse().len();
            MesslatteTopf {
                topf,
                netto: netto.as_ref().and_then(|b| b.get(&topf)).copied(),
                tage,
                vorsprung_bp,
                rueckgang_bp,
                ausfall_bp,
                ergebnis: messlatte::bewertung(tage, vorsprung_bp, rueckgang_bp, ausfall_bp),
            }
        })
        .collect()
}

fn vorsprung_bp(wert: Option<Cent>, ki: Option<Cent>, vergleich: Option<Cent>) -> Option<i64> {
    let differenz = wert?.checked_sub(ki?)?.checked_sub(vergleich?)?;
    crate::vergleich::rendite_bp(START_JE_TOPF, START_JE_TOPF.checked_add(differenz)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m2_exakt_gleich() {
        assert_eq!(
            vorsprung_bp(Some(Cent(500_049)), Some(Cent(0)), Some(Cent(500_049))),
            Some(0)
        );
    }

    #[test]
    fn m2_ein_cent_drunter() {
        assert_eq!(
            vorsprung_bp(Some(Cent(500_048)), Some(Cent(0)), Some(Cent(500_049))),
            Some(-1)
        );
    }

    #[test]
    fn m2_ki_kosten_kippen() {
        assert_eq!(
            vorsprung_bp(Some(Cent(500_099)), Some(Cent(51)), Some(Cent(500_049))),
            Some(-1)
        );
    }

    #[test]
    fn fehlender_wert() {
        assert_eq!(vorsprung_bp(Some(Cent(500_000)), Some(Cent(0)), None), None);
        assert_eq!(
            messlatte::bewertung(60, None, Some(0), Some(0)),
            Ergebnis::NichtBewertbar
        );
    }
}
