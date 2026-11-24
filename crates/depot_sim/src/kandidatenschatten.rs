//! Lesende Haltefenster für protokollierte Kaufideen im Papiertag (#457).

use std::collections::{BTreeMap, BTreeSet};

use depot_journal::Stand;
use depot_kern::liste::{VERGLEICH_ISIN, boerse_von, papierorder_schluss};
use depot_kern::{Boerse, Cent, Datum, Grenzen, Herkunft, Isin, Preis, Rundung, Zeitpunkt};

use crate::positionskauf::Kursziel;
use crate::schattenkandidaten::{Kandidat, kandidaten, modell_stueck, signale};

const MODELLTAGE: u16 = 5;
type Schlusskurse<'a> = BTreeMap<(&'a Isin, Datum), (Zeitpunkt, Preis)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fuellung {
    pub datum: Datum,
    pub kurs: Cent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ausstieg {
    Frist,
    Stopp,
    Ziel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Abschluss {
    pub fuellung: Fuellung,
    pub grund: Ausstieg,
    pub netto: Cent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signalkontrolle {
    pub isin: Isin,
    pub idee_netto: Cent,
    pub signal_netto: Cent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StandImSchatten {
    FehlendeKaufdaten,
    KeineStuecke,
    EinstiegOffen,
    LimitVerfehlt(Fuellung),
    Kursluecke(Datum),
    Kapitalmassnahme(Datum),
    Haelt(Fuellung),
    Abgeschlossen {
        einstieg: Fuellung,
        frist: Datum,
        mit_stopp: Abschluss,
        mit_ziel: Option<Abschluss>,
        korbnetto: Option<Cent>,
        signalkontrolle: Option<Signalkontrolle>,
    },
}

#[derive(Debug)]
pub struct Zeile {
    pub kandidat: Kandidat,
    pub stand: StandImSchatten,
}

fn schlusskurse(stand: &Stand, bis: Datum, jetzt: Zeitpunkt) -> Schlusskurse<'_> {
    let mut kurse = BTreeMap::new();
    for beleg in stand.kurse() {
        let kurs = beleg.kurs();
        let datum = kurs.handelstag();
        if beleg.abruf() > jetzt
            || datum > bis
            || kurs.herkunft() != Herkunft::Quelle
            || !boerse_von(beleg.isin())
                .is_some_and(|b| b.handelstag(datum) && b.schluss(datum) == Some(kurs.zeitpunkt()))
        {
            continue;
        }
        let wert = (beleg.abruf(), kurs.preis());
        let bisher = kurse.entry((beleg.isin(), datum)).or_insert(wert);
        if wert.0 < bisher.0 {
            *bisher = wert;
        }
    }
    kurse
}

fn kapitaltage(stand: &Stand, jetzt: Zeitpunkt) -> BTreeMap<Isin, BTreeSet<Datum>> {
    let mut tage = BTreeMap::<Isin, BTreeSet<Datum>>::new();
    for b in stand.kapitalmassnahmen() {
        tage.entry(b.massnahme.isin.clone())
            .or_default()
            .insert(b.massnahme.wirksam);
    }
    for (isin, tag) in stand
        .nachrichten()
        .iter()
        .filter(|b| b.abruf() <= jetzt)
        .filter_map(depot_journal::kapitalmassnahme::tiingo_split)
    {
        tage.entry(isin).or_default().insert(tag);
    }
    for (isin, beleg) in depot_journal::splitverdacht::aktiv(stand, Some(jetzt)) {
        if let Some(tag) = beleg.zeit().frankfurt_tag() {
            tage.entry(isin).or_default().insert(tag);
        }
    }
    tage
}

fn kaufpreis(g: &Grenzen, kurs: Cent, stueck: u64) -> Option<Cent> {
    let volumen = kurs.mal_stueck(stueck)?;
    volumen.checked_add(g.gebuehren().fuer(volumen)?)
}

fn erloes(g: &Grenzen, kurs: Cent, stueck: u64) -> Option<Cent> {
    let volumen = kurs.mal_stueck(stueck)?;
    volumen.checked_sub(g.gebuehren().fuer(volumen)?)
}

fn zielkurs(ziel: Kursziel, einstieg: Cent) -> Option<Preis> {
    Preis::aus_cent(match ziel {
        Kursziel::Absolut(kurs) => kurs,
        Kursziel::GewinnBp(bp) => einstieg.checked_add(einstieg.anteil_bp_auf(bp)?)?,
    })
}

struct Markt<'a> {
    g: Grenzen,
    kurse: Schlusskurse<'a>,
    kapitaltage: BTreeMap<Isin, BTreeSet<Datum>>,
    korb: Vec<Isin>,
    signale: BTreeMap<(Datum, usize, Boerse), Isin>,
    bis: Datum,
    jetzt: Zeitpunkt,
}

impl Markt<'_> {
    fn kurs(&self, isin: &Isin, tag: Datum) -> Option<Preis> {
        self.kurse.get(&(isin, tag)).map(|(_, preis)| *preis)
    }

    fn kapitalwechsel(&self, isin: &Isin, von: Datum, bis: Datum) -> Option<Datum> {
        use std::ops::Bound::{Excluded, Included};
        self.kapitaltage
            .get(isin)?
            .range((Excluded(von), Included(bis)))
            .next()
            .copied()
    }

    fn korbnetto(&self, boerse: Boerse, von: Datum, bis: Datum, start: Cent) -> Option<Cent> {
        let werte: Vec<_> = self
            .korb
            .iter()
            .filter(|isin| boerse_von(isin) == Some(boerse))
            .collect();
        let anteil = Cent(start.0.checked_div(i64::try_from(werte.len()).ok()?)?);
        let frei = anteil.checked_sub(self.g.gebuehren().fuer(anteil)?)?;
        werte.into_iter().try_fold(Cent::NULL, |netto, isin| {
            if self.kapitalwechsel(isin, von, bis).is_some() {
                return None;
            }
            let kauf = self
                .g
                .papierkurs(boerse, self.kurs(isin, von)?, Rundung::Auf)?;
            let verkauf = self
                .g
                .papierkurs(boerse, self.kurs(isin, bis)?, Rundung::Ab)?;
            let stueck = Preis::aus_cent(kauf)?.stueck_fuer(Preis::aus_cent(frei)?)?;
            if kauf.mal_stueck(stueck)? < self.g.min_order() {
                return Some(netto);
            }
            netto.checked_add(
                erloes(&self.g, verkauf, stueck)?.checked_sub(kaufpreis(&self.g, kauf, stueck)?)?,
            )
        })
    }

    fn werte_aus(&self, k: &Kandidat) -> Option<StandImSchatten> {
        use StandImSchatten::{EinstiegOffen, FehlendeKaufdaten, Haelt, KeineStuecke, Kursluecke};
        let (Some(stueck), Some(limit), Some(bereit)) = (k.stueck, k.limit, k.bereit_ab) else {
            return Some(FehlendeKaufdaten);
        };
        if stueck == 0 {
            return Some(KeineStuecke);
        }
        let boerse = boerse_von(&k.isin)?;
        let zeitziel = papierorder_schluss(&k.isin, bereit)?;
        let datum = zeitziel.frankfurt_tag()?;
        if datum > self.bis || zeitziel > self.jetzt {
            return Some(EinstiegOffen);
        }
        let Some(preis) = self.kurs(&k.isin, datum) else {
            return Some(Kursluecke(datum));
        };
        let kurs = self.g.papierkurs(boerse, preis, Rundung::Auf)?;
        let einstieg = Fuellung { datum, kurs };
        if let Some(tag) = self.kapitalwechsel(&k.isin, k.tag, datum) {
            return Some(StandImSchatten::Kapitalmassnahme(tag));
        }
        if kurs > limit {
            return Some(StandImSchatten::LimitVerfehlt(einstieg));
        }
        let dauer = usize::from(k.plan.map_or(MODELLTAGE, |plan| plan.handelstage));
        let tage: Vec<_> = std::iter::successors(datum.plus_tage(1), |d| d.plus_tage(1))
            .take_while(|&d| d <= self.bis)
            .filter(|&d| boerse.handelstag(d) && boerse.schluss(d).is_some_and(|s| s <= self.jetzt))
            .take(dauer)
            .collect();
        if tage.len() < dauer {
            return Some(Haelt(einstieg));
        }
        let frist = *tage.last()?;
        if let Some(tag) = self.kapitalwechsel(&k.isin, datum, frist) {
            return Some(StandImSchatten::Kapitalmassnahme(tag));
        }
        let mut schluesse = Vec::with_capacity(tage.len());
        for tag in tage {
            let Some(preis) = self.kurs(&k.isin, tag) else {
                return Some(Kursluecke(tag));
            };
            schluesse.push((tag, preis));
        }
        self.abschluss(k, boerse, stueck, einstieg, frist, &schluesse)
    }

    fn abschluss(
        &self,
        k: &Kandidat,
        boerse: Boerse,
        stueck: u64,
        einstieg: Fuellung,
        frist: Datum,
        schluesse: &[(Datum, Preis)],
    ) -> Option<StandImSchatten> {
        let basis = kaufpreis(&self.g, einstieg.kurs, stueck)?;
        let stopp = k
            .stopp_bp
            .and_then(|bp| einstieg.kurs.anteil_bp_auf(bp))
            .and_then(|abstand| einstieg.kurs.checked_sub(abstand))
            .and_then(Preis::aus_cent);
        let ziel = k
            .plan
            .and_then(|plan| zielkurs(plan.kursziel, einstieg.kurs));
        let rechne = |ziel| {
            let ((datum, preis), grund) = ausstieg(schluesse, stopp, ziel)?;
            let kurs = self.g.papierkurs(boerse, preis, Rundung::Ab)?;
            Some(Abschluss {
                fuellung: Fuellung { datum, kurs },
                grund,
                netto: erloes(&self.g, kurs, stueck)?.checked_sub(basis)?,
            })
        };
        let signalkontrolle = self.signalkontrolle(k, boerse, einstieg.datum, frist, basis, stueck);
        Some(StandImSchatten::Abgeschlossen {
            einstieg,
            frist,
            mit_stopp: rechne(None)?,
            mit_ziel: ziel.and_then(|ziel| rechne(Some(ziel))),
            korbnetto: self.korbnetto(boerse, einstieg.datum, frist, basis),
            signalkontrolle,
        })
    }

    fn signalkontrolle(
        &self,
        k: &Kandidat,
        boerse: Boerse,
        von: Datum,
        bis: Datum,
        basis: Cent,
        stueck: u64,
    ) -> Option<Signalkontrolle> {
        let isin = self.signale.get(&(k.tag, k.lauf?, boerse))?;
        // Beide Ideen verwenden die ganze Haltefrist und dieselbe Kaufkraft. Fehlende Kurse
        // bleiben Lücken. Bei einer bekannten Größenhürde bleibt das Vergleichsgeld bar.
        if self.kapitalwechsel(isin, von, bis).is_some() {
            return None;
        }
        let kauf = self
            .g
            .papierkurs(boerse, self.kurs(isin, von)?, Rundung::Auf)?;
        let verkauf = self
            .g
            .papierkurs(boerse, self.kurs(isin, bis)?, Rundung::Ab)?;
        let rangstueck = modell_stueck(&self.g, Preis::aus_cent(basis)?, basis, kauf)?;
        let signal_netto = if rangstueck == 0 {
            Cent::NULL
        } else {
            erloes(&self.g, verkauf, rangstueck)?
                .checked_sub(kaufpreis(&self.g, kauf, rangstueck)?)?
        };
        let ende = self
            .g
            .papierkurs(boerse, self.kurs(&k.isin, bis)?, Rundung::Ab)?;
        Some(Signalkontrolle {
            isin: isin.clone(),
            idee_netto: erloes(&self.g, ende, stueck)?.checked_sub(basis)?,
            signal_netto,
        })
    }
}

fn ausstieg(
    kurse: &[(Datum, Preis)],
    stopp: Option<Preis>,
    ziel: Option<Preis>,
) -> Option<((Datum, Preis), Ausstieg)> {
    for (&(_, kurs), &folge) in kurse.iter().zip(kurse.iter().skip(1)) {
        if stopp.is_some_and(|stopp| kurs <= stopp) {
            return Some((folge, Ausstieg::Stopp));
        }
        if ziel.is_some_and(|ziel| kurs >= ziel) {
            return Some((folge, Ausstieg::Ziel));
        }
    }
    Some((*kurse.last()?, Ausstieg::Frist))
}

/// Wählt Kaufkandidaten aus dem Intervall und verfolgt sie bis zum Stand des Exports.
/// Offene und unvollständige Haltefenster bleiben ohne Nettowert, auch nach einer frühen Schwelle.
pub fn auswertung(stand: &Stand, von: Datum, bis: Datum, jetzt: Zeitpunkt) -> Vec<Zeile> {
    let kandidaten = kandidaten(stand)
        .into_iter()
        .filter(|k| (von..=bis).contains(&k.tag));
    let kursende = jetzt.frankfurt_tag().unwrap_or(bis);
    let markt = Markt {
        g: Grenzen::standard(),
        kurse: schlusskurse(stand, kursende, jetzt),
        kapitaltage: kapitaltage(stand, jetzt),
        korb: VERGLEICH_ISIN
            .iter()
            .filter_map(|i| Isin::neu(i).ok())
            .collect(),
        signale: signale(stand),
        bis: kursende,
        jetzt,
    };
    kandidaten
        .map(|kandidat| Zeile {
            stand: markt
                .werte_aus(&kandidat)
                .unwrap_or(StandImSchatten::FehlendeKaufdaten),
            kandidat,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::positionskauf::Ausstiegsplan;
    use crate::schattenkandidaten::Phase;
    use depot_kern::TopfId;

    #[test]
    fn spaeter_stopp_und_ziel_folgen_dem_ersten_ausfuehrbaren_schluss() -> Result<(), String> {
        let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let tag = Datum::neu(2026, 11, 24).ok_or("Kaufdatum")?;
        let bis = Datum::neu(2026, 12, 1).ok_or("Fristdatum")?;
        let zeit = |d, stunde| Zeitpunkt::frankfurt(d, stunde, 0, 0).ok_or("Zeit");
        let jetzt = zeit(bis, 23)?;
        let mut kurse = BTreeMap::new();
        for (datum, cent) in std::iter::successors(Some(tag), |d| d.plus_tage(1))
            .filter(|&d| Boerse::Xetra.handelstag(d))
            .zip([10_000, 10_400, 9_600, 9_500, 10_700, 11_000])
        {
            kurse.insert(
                (&isin, datum),
                (
                    zeit(datum, 23)?,
                    Preis::aus_cent(Cent(cent)).ok_or("Preis")?,
                ),
            );
        }
        let mut markt = Markt {
            g: Grenzen::standard(),
            kurse,
            kapitaltage: BTreeMap::new(),
            korb: vec![isin.clone()],
            signale: BTreeMap::from([((tag, 1, Boerse::Xetra), isin.clone())]),
            bis,
            jetzt,
        };
        let mut kandidat = Kandidat {
            tag,
            lauf: Some(1),
            topf: TopfId::A,
            phase: Phase::Erstmeinung,
            isin: isin.clone(),
            ziel_bp: 3500,
            stueck: Some(10),
            limit: Some(Cent(10_100)),
            stopp_bp: Some(300),
            plan: Some(Ausstiegsplan {
                kursziel: Kursziel::GewinnBp(200),
                handelstage: 5,
            }),
            bereit_ab: Some(zeit(tag, 7)?),
        };
        let Some(StandImSchatten::Abgeschlossen {
            einstieg,
            frist,
            mit_stopp,
            mit_ziel: Some(mit_ziel),
            korbnetto: Some(_),
            signalkontrolle: Some(signal),
        }) = markt.werte_aus(&kandidat)
        else {
            return Err("Der vollständige Fall fehlt".into());
        };
        assert_eq!(einstieg.kurs, Cent(10_005));
        assert_eq!(frist, bis);
        assert_eq!(signal.isin, kandidat.isin);
        assert_eq!(signal.idee_netto, signal.signal_netto);
        assert_eq!(
            (mit_stopp.grund, mit_stopp.netto),
            (Ausstieg::Stopp, Cent(-5_400))
        );
        assert_eq!(
            (mit_ziel.grund, mit_ziel.netto),
            (Ausstieg::Ziel, Cent(-4_400))
        );
        assert_eq!(
            mit_ziel.fuellung.datum,
            Datum::neu(2026, 11, 26).ok_or("Zielausstieg")?
        );
        assert_eq!(
            mit_stopp.fuellung.datum,
            Datum::neu(2026, 11, 27).ok_or("Stoppausstieg")?
        );

        markt.jetzt = zeit(Datum::neu(2026, 11, 30).ok_or("Tag davor")?, 23)?;
        assert_eq!(
            markt.werte_aus(&kandidat),
            Some(StandImSchatten::Haelt(einstieg))
        );
        markt.jetzt = jetzt;
        kandidat.limit = Some(Cent(10_000));
        assert_eq!(
            markt.werte_aus(&kandidat),
            Some(StandImSchatten::LimitVerfehlt(einstieg))
        );
        kandidat.limit = Some(Cent(10_100));
        markt.kurse.remove(&(&isin, bis));
        assert_eq!(
            markt.werte_aus(&kandidat),
            Some(StandImSchatten::Kursluecke(bis))
        );
        Ok(())
    }
}
