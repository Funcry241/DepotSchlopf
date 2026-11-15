//! Historische Gegenprobe der Schattenstrategien mit eingefrorenem Wissensstand.

use std::collections::BTreeSet;

use depot_journal::Stand;
use depot_kern::{Datum, Grenzen, Universum, Zeitpunkt};

use crate::schatten::{Ergebnis, Schattenstand, Variante};

#[derive(Clone, Debug)]
pub struct Vergleich {
    pub tage: usize,
    pub reihen: Vec<(Variante, [Ergebnis; 3])>,
}

fn stichtag(stand: &Stand, tag: Datum) -> Option<Zeitpunkt> {
    stand
        .wissensstichtag(tag)
        .or_else(|| Zeitpunkt::frankfurt(tag, 23, 59, 59))
}

fn historische_lage(
    stand: &Stand,
    universum: &Universum,
    datum: Datum,
    wissen: Zeitpunkt,
) -> Option<crate::Tag> {
    if stand.wissensstichtag(datum).is_some() {
        crate::papiertag_wiederabspielen(stand, universum, datum).ok()
    } else {
        crate::papiertag::tageskurse_bis(stand, universum, Some(wissen))
            .ok()
            .filter(|k| k.handelstag == datum)
            .map(|k| crate::Tag {
                datum,
                kurse: k.heute,
                ereignisse: Vec::new(),
            })
    }
}

pub fn rechne(stand: &Stand, grenzen: &Grenzen, universum: &Universum) -> Vergleich {
    let tage: BTreeSet<_> = stand
        .kurse()
        .iter()
        .filter_map(|b| {
            let tag = b.abruf().frankfurt_tag()?;
            depot_kern::liste::handelstag(tag).then_some(tag)
        })
        .chain(stand.tagesschluesse().iter().map(|(tag, _)| *tag))
        .collect();
    let mut varianten: Vec<_> = [5, 10, 20]
        .into_iter()
        .flat_map(|haltedauer| {
            [(false, 500), (true, 800)]
                .into_iter()
                .map(move |(kurzrang, zahlenschwelle_bp)| {
                    (
                        Variante {
                            haltedauer,
                            zahlenschwelle_bp,
                            kurzrang,
                        },
                        Schattenstand::default(),
                    )
                })
        })
        .collect();
    let mut anzahl = 0usize;
    for datum in tage {
        let Some(wissen) = stichtag(stand, datum) else {
            continue;
        };
        let Some(tag) = historische_lage(stand, universum, datum, wissen) else {
            continue;
        };
        anzahl = anzahl.saturating_add(1);
        for (variante, schatten) in &mut varianten {
            schatten.fortschreiben(stand, grenzen, universum, &tag, wissen, *variante);
        }
    }
    Vergleich {
        tage: anzahl,
        reihen: varianten
            .into_iter()
            .map(|(v, s)| (v, s.ergebnisse()))
            .collect(),
    }
}

pub fn drucke(aus: &Vergleich) {
    let geld = |wert: Option<depot_kern::Cent>| {
        wert.map_or_else(|| "unbelegt".to_owned(), |c| c.to_string())
    };
    println!("Rücktest · {} auswertbare Tage · nur gerechnet", aus.tage);
    println!(
        "Haltedauer | Stärkerang | Zahlenschwelle (bp) | Strategie | Trades | Ergebnis netto | Abstand zum Korb | Größter Rückgang (bp)"
    );
    for (v, reihen) in &aus.reihen {
        let korb = reihen.last().and_then(|r| r.netto);
        let rang = if v.kurzrang {
            "20 Tage"
        } else {
            "120 Tage ohne die letzten 20"
        };
        for reihe in reihen {
            println!(
                "{} | {rang} | {} | {} | {} | {} | {} | {}",
                v.haltedauer,
                v.zahlenschwelle_bp,
                reihe.name,
                reihe.trades,
                geld(reihe.netto),
                geld(reihe.netto.zip(korb).and_then(|(n, k)| n.checked_sub(k))),
                reihe
                    .rueckgang_bp
                    .map_or_else(|| "unbelegt".to_owned(), |bp| bp.to_string()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_journal::{Journal, KursBeleg};
    use depot_kern::{Boerse, Cent, Herkunft, Instrument, InstrumentArt, Isin, Kurs};

    #[test]
    fn ruecktest_laesst_spaeter_abgerufene_kurskorrektur_weg() -> Result<(), String> {
        let ordner = std::env::temp_dir().join(format!("depot-ruecktest-{}", std::process::id()));
        std::fs::create_dir(&ordner).map_err(|e| e.to_string())?;
        let probe = || -> Result<(), String> {
            let pfad = ordner.join("journal.txt");
            Journal::einrichten(&pfad).map_err(|e| format!("{e:?}"))?;
            let mut j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
            let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
            let u = Universum::neu(vec![Instrument {
                isin: isin.clone(),
                name: "Probe".into(),
                art: InstrumentArt::Aktie,
            }]);
            let montag = Datum::neu(2026, 11, 9).ok_or("Montag")?;
            let dienstag = montag.plus_tage(1).ok_or("Dienstag")?;
            let schluss = Boerse::Xetra.schluss(montag).ok_or("Schluss")?;
            let wissen = Zeitpunkt::frankfurt(dienstag, 8, 0, 0).ok_or("Uhr")?;
            let tag = |d, h| Zeitpunkt::frankfurt(d, h, 0, 0).ok_or("Abruf");
            let mut buche = |abruf, betrag| -> Result<(), String> {
                let kurs = Kurs::neu(Cent(betrag), "EUR", montag, schluss, Herkunft::Quelle)
                    .map_err(|e| format!("{e:?}"))?;
                let beleg = KursBeleg::neu("probe", abruf, b"Schlusskurs", isin.clone(), kurs)?;
                j.belege_kurs(&beleg).map_err(|e| format!("{e:?}"))
            };
            buche(tag(montag, 18)?, 10_000)?;
            buche(tag(dienstag, 18)?, 20_000)?;
            j.belege_wissensstichtag(dienstag, wissen)
                .map_err(|e| format!("{e:?}"))?;
            let lage = historische_lage(j.stand(), &u, dienstag, wissen).ok_or("Lage")?;
            assert_eq!(lage.kurse.kurs_cent(&isin), Some(Cent(10_000)));
            Ok(())
        }();
        std::fs::remove_dir_all(&ordner).map_err(|e| e.to_string())?;
        probe
    }
}
