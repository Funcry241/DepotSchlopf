//! Schatten der Vetos (#311 M0, Punkt 5): Was ein von der Zweitmeinung abgelehnter Kauf bis
//! heute netto gebracht hätte. Nur lesend aus den Journalbegründungen und belegten Kursen.
//!
//! Gerechnet wird wie im Papierkonto: Einstieg zum Schluss des Vetotags mit Modellspread
//! aufgerundet, Ausstieg zum jüngsten belegten Schluss mit Spread abgerundet, beide Gebühren.
//! Das ist die Datengrundlage für die offene Frage, ob bei einem Veto mit halber Größe gekauft
//! werden soll; diese Regel ist nicht gebaut.

use std::collections::BTreeMap;

use depot_journal::Stand;
use depot_kern::{BeraterId, Cent, Datum, Grenzen, Herkunft, Isin, Preis, Rundung, TopfId};

use crate::depotverlauf::Tagespunkt;

/// Ein abgelehnter Kauf: Zielgewicht der Erstmeinung über dem gehaltenen Gewicht.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Veto {
    pub tag: Datum,
    pub topf: TopfId,
    pub isin: Isin,
    pub ziel_bp: i64,
}

fn berater_name(b: BeraterId) -> &'static str {
    match b {
        BeraterId::Dachs => "Dachs",
        BeraterId::Maus => "Maus",
    }
}

/// Vetos mit den Zielgewichten, die die Erstmeinung am selben Tag für diesen Topf genannt hat.
pub fn vetos(begruendungen: &[(Datum, String, String)]) -> Vec<Veto> {
    let mut aus = Vec::new();
    for topf in [TopfId::A, TopfId::B] {
        let praefix = format!("Topf {topf:?}: ");
        let erst = berater_name(topf.berater());
        let zweit = berater_name(match topf.berater() {
            BeraterId::Dachs => BeraterId::Maus,
            BeraterId::Maus => BeraterId::Dachs,
        });
        let mut ziele = BTreeMap::<(Datum, Isin), i64>::new();
        let mut veto_tage = Vec::new();
        for (tag, wer, text) in begruendungen {
            let Some(rest) = text.strip_prefix(&praefix) else {
                continue;
            };
            if wer == zweit && rest.starts_with("Veto: ") {
                veto_tage.push(*tag);
            } else if wer == erst
                && let Some((kopf, _)) = rest.split_once(" bp: ")
                && let Some((isin, bp)) = kopf.split_once(' ')
                && let (Ok(isin), Ok(bp)) = (Isin::neu(isin), bp.parse::<i64>())
            {
                ziele.insert((*tag, isin), bp);
            }
        }
        veto_tage.dedup();
        for tag in veto_tage {
            for ((_, isin), ziel_bp) in ziele.iter().filter(|((t, _), _)| *t == tag) {
                if *ziel_bp > 0 {
                    aus.push(Veto {
                        tag,
                        topf,
                        isin: isin.clone(),
                        ziel_bp: *ziel_bp,
                    });
                }
            }
        }
    }
    aus.sort_by(|a, b| (a.tag, a.topf, &a.isin).cmp(&(b.tag, b.topf, &b.isin)));
    aus
}

/// Netto eines Papierkaufs von `stueck` zu `einstieg` und Verkaufs zu `ausstieg`, nach beiden
/// Gebühren.
pub fn netto(g: &Grenzen, stueck: u64, einstieg: Cent, ausstieg: Cent) -> Option<Cent> {
    let kauf = einstieg.mal_stueck(stueck)?;
    let verkauf = ausstieg.mal_stueck(stueck)?;
    verkauf
        .checked_sub(kauf)?
        .checked_sub(g.gebuehren().fuer(kauf)?)?
        .checked_sub(g.gebuehren().fuer(verkauf)?)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schatten {
    pub veto: Veto,
    pub stueck: u64,
    pub einstieg: Cent,
    pub ausstieg: Option<(Datum, Cent)>,
    pub netto: Option<Cent>,
    pub netto_halb: Option<Cent>,
}

/// Rechnet jeden abgelehnten Kauf bis zum jüngsten belegten Schluss bis einschließlich `bis`.
pub fn schatten(stand: &Stand, punkte: &[Tagespunkt], bis: Datum) -> Vec<Schatten> {
    let g = Grenzen::standard();
    let mut kurse = BTreeMap::<(&Isin, Datum), (depot_kern::Zeitpunkt, Preis)>::new();
    for b in stand
        .kurse()
        .iter()
        .filter(|b| b.kurs().herkunft() == Herkunft::Quelle)
    {
        let eintrag = kurse
            .entry((b.isin(), b.kurs().handelstag()))
            .or_insert((b.abruf(), b.kurs().preis()));
        if b.abruf() >= eintrag.0 {
            *eintrag = (b.abruf(), b.kurs().preis());
        }
    }
    vetos(stand.begruendungen())
        .into_iter()
        .filter_map(|veto| {
            let isin = &veto.isin;
            let boerse = depot_kern::liste::boerse_von(isin)?;
            let vorher = punkte.iter().rev().find(|p| p.tag < veto.tag)?;
            let topfwert = *vorher.topfwerte.get(&veto.topf)?;
            let gehalten = stand
                .tagesschluesse()
                .iter()
                .find(|(t, _)| *t == vorher.tag)
                .and_then(|(_, t)| t.get(&veto.topf))
                .map_or(0, |t| t.stueck(isin));
            let (_, (_, vorkurs)) = kurse
                .range((isin, Datum::neu(1970, 1, 1)?)..=(isin, vorher.tag))
                .next_back()?;
            let ist = vorkurs.mal_stueck(gehalten, Rundung::Ab)?;
            let ziel = Preis::aus_cent(topfwert)?.mal_bp(veto.ziel_bp, Rundung::Ab)?;
            let betrag = ziel.checked_sub(ist).filter(|b| *b > Cent::NULL)?;
            let (_, (_, schluss)) = kurse.range((isin, veto.tag)..=(isin, bis)).next()?;
            let einstieg = g.papierkurs(boerse, *schluss, Rundung::Auf)?;
            let stueck = u64::try_from(betrag.0.checked_div(einstieg.0)?).ok()?;
            let ausstieg = kurse
                .range((isin, veto.tag)..=(isin, bis))
                .next_back()
                .and_then(|((_, tag), (_, kurs))| {
                    Some((*tag, g.papierkurs(boerse, *kurs, Rundung::Ab)?))
                });
            let rechne = |s| ausstieg.and_then(|(_, aus)| netto(&g, s, einstieg, aus));
            Some(Schatten {
                stueck,
                einstieg,
                ausstieg,
                netto: rechne(stueck),
                netto_halb: rechne(stueck.div_euclid(2)),
                veto,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(t: u8) -> Result<Datum, String> {
        Datum::neu(2026, 11, t).ok_or_else(|| format!("Tag {t}"))
    }

    #[test]
    fn veto_nimmt_die_ziele_der_erstmeinung_desselben_tags() -> Result<(), String> {
        let z = |t: u8, wer: &str, text: &str| -> Result<_, String> {
            Ok((tag(t)?, wer.to_string(), text.to_string()))
        };
        let begruendungen = vec![
            z(16, "Dachs", "Topf A: DE0007164600 2000 bp: Auftragslage")?,
            z(16, "Dachs", "Topf A: DE0008404005 0 bp: raus")?,
            z(16, "Maus", "Topf A: Veto: zu teuer")?,
            z(16, "Maus", "Topf B: US0378331005 1500 bp: stark")?,
            z(16, "Dachs", "Topf B: Zustimmung: passt")?,
            z(17, "Dachs", "Topf A: DE0007164600 3000 bp: mehr")?,
        ];
        let v = vetos(&begruendungen);
        assert_eq!(
            v,
            vec![Veto {
                tag: tag(16)?,
                topf: TopfId::A,
                isin: Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?,
                ziel_bp: 2000,
            }]
        );
        Ok(())
    }

    /// 10 Stück zu 100,00 € gekauft und zu 105,00 € verkauft: 50,00 € minus zweimal 1,50 €.
    #[test]
    fn netto_nach_gebuehren() {
        let g = Grenzen::standard();
        assert_eq!(netto(&g, 10, Cent(10_000), Cent(10_500)), Some(Cent(4_700)));
        assert_eq!(netto(&g, 5, Cent(10_000), Cent(10_500)), Some(Cent(2_200)));
    }

    /// Veto am Montag 16.11. gegen 20 % SAP in Topf A (10.000 €): 2.000 € zum Montagsschluss
    /// 100,00 € mit Spread, Ausstieg zum Mittwochsschluss 110,00 € mit Spread, beide Gebühren.
    #[test]
    fn schatten_rechnet_bis_zum_juengsten_schluss() -> Result<(), String> {
        use depot_journal::{Journal, KursBeleg};
        use depot_kern::{Boerse, Kurs, Topf, Zeitpunkt};
        let dir = std::env::temp_dir().join(format!("depot_vetoschatten_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let pfad = dir.join("journal.txt");
        Journal::einrichten(&pfad).map_err(|e| format!("{e:?}"))?;
        let mut j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
        let topf = Topf::neu(TopfId::A, Cent(1_000_000)).map_err(|e| format!("{e:?}"))?;
        j.setze_topf(&topf).map_err(|e| format!("{e:?}"))?;
        j.schliesse_ab(tag(13)?, Cent(1_000_000), &"0".repeat(64))
            .map_err(|e| format!("{e:?}"))?;
        let sap = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        for (t, cent) in [(13, 9_800), (16, 10_000), (18, 11_000)] {
            let d = tag(t)?;
            let schluss = Boerse::Xetra.schluss(d).ok_or("Schluss")?;
            let abruf = Zeitpunkt::frankfurt(d, 23, 0, 0).ok_or("Abruf")?;
            let kurs = Kurs::neu(Cent(cent), "EUR", d, schluss, Herkunft::Quelle)
                .map_err(|e| format!("{e:?}"))?;
            j.belege_kurs(
                &KursBeleg::neu("alphavantage", abruf, b"Probe", sap.clone(), kurs)
                    .map_err(|e| format!("{e:?}"))?,
            )
            .map_err(|e| format!("{e:?}"))?;
        }
        j.begruende(tag(16)?, "Dachs", "Topf A: DE0007164600 2000 bp: Probe")
            .map_err(|e| format!("{e:?}"))?;
        j.begruende(tag(16)?, "Maus", "Topf A: Veto: Probe")
            .map_err(|e| format!("{e:?}"))?;
        let punkt = Tagespunkt {
            tag: tag(13)?,
            wertpapiere: Some(Cent::NULL),
            bargeld: Some(Cent(1_000_000)),
            gesamt: Some(Cent(1_000_000)),
            grund: None,
            kurstage: Vec::new(),
            kursbezug: BTreeMap::new(),
            ersatz: Vec::new(),
            topfwerte: [(TopfId::A, Cent(1_000_000))].into_iter().collect(),
        };
        let s = schatten(j.stand(), &[punkt], tag(18)?);
        let g = Grenzen::standard();
        let [eins] = s.as_slice() else {
            return Err(format!("{s:?}"));
        };
        let einstieg = g
            .papierkurs(
                Boerse::Xetra,
                Preis::aus_cent(Cent(10_000)).ok_or("Kurs")?,
                Rundung::Auf,
            )
            .ok_or("Einstieg")?;
        let ausstieg = g
            .papierkurs(
                Boerse::Xetra,
                Preis::aus_cent(Cent(11_000)).ok_or("Kurs")?,
                Rundung::Ab,
            )
            .ok_or("Ausstieg")?;
        let stueck = u64::try_from(200_000 / einstieg.0).map_err(|e| e.to_string())?;
        assert_eq!(eins.einstieg, einstieg);
        assert_eq!(eins.stueck, stueck);
        assert_eq!(eins.ausstieg, Some((tag(18)?, ausstieg)));
        assert_eq!(eins.netto, netto(&g, stueck, einstieg, ausstieg));
        assert_eq!(eins.netto_halb, netto(&g, stueck / 2, einstieg, ausstieg));
        assert!(eins.netto.is_some_and(|n| n > Cent::NULL));
        drop(j);
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
