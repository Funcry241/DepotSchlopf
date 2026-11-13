//! Gemeinsame, schreibfreie Auswertung der Papierorders für Anzeige und Beratung.

use depot_journal::{AuftragsLage, AuftragsZustand, Stand};
use depot_kern::{Cent, Datum, Herkunft, Isin, Preis, Rundung, TopfId, Zeitpunkt};
pub use depot_regelwerk::Richtung;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderZeile {
    pub nr: u64,
    pub topf: TopfId,
    pub isin: Isin,
    pub richtung: Richtung,
    pub stueck: u64,
    pub versand: Option<Zeitpunkt>,
    pub frist: Option<Zeitpunkt>,
    pub zustand: AuftragsZustand,
    pub bezug: Option<Preis>,
    pub limit: Cent,
    pub limit_bp: Option<i64>,
    pub fuellpreis: Option<Preis>,
    pub fuellpreis_bp: Option<i64>,
    pub schluss: Option<Preis>,
    pub schluss_bp: Option<i64>,
}

fn abstand(preis: Option<Preis>, bezug: Option<Preis>) -> Option<i64> {
    preis?.in_bp_von(bezug?)?.checked_sub(10_000)
}

fn fuellpreis(order: &AuftragsLage) -> Option<Preis> {
    let (wert, stueck) =
        order
            .ausfuehrungen
            .iter()
            .try_fold((0_i128, 0_i128), |(wert, stueck), aus| {
                let teil = i128::from(aus.kurs.0).checked_mul(i128::from(aus.stueck))?;
                Some((
                    wert.checked_add(teil)?,
                    stueck.checked_add(i128::from(aus.stueck))?,
                ))
            })?;
    let richtung = match order.beleg.richtung {
        Richtung::Kauf => Rundung::Auf,
        Richtung::Verkauf => Rundung::Ab,
    };
    let quotient = wert.checked_div(stueck)?;
    let aufrunden = richtung == Rundung::Auf && wert.checked_rem(stueck)? != 0;
    let preis = quotient.checked_add(i128::from(aufrunden))?;
    Preis::aus_cent(Cent(i64::try_from(preis).ok()?))
}

pub fn ausfuehrungsbuch(stand: &Stand) -> Vec<OrderZeile> {
    use std::collections::BTreeMap;
    let mut je_wert = BTreeMap::<_, Vec<_>>::new();
    for beleg in stand
        .kurse()
        .iter()
        .filter(|beleg| beleg.kurs().herkunft() == Herkunft::Quelle)
    {
        je_wert.entry(beleg.isin()).or_default().push(beleg);
    }
    stand
        .auftraege()
        .iter()
        .map(|order| {
            let belege = je_wert
                .get(&order.beleg.isin)
                .map_or(&[][..], Vec::as_slice);
            orderzeile(order, belege)
        })
        .collect()
}

fn orderzeile(order: &AuftragsLage, belege: &[&depot_journal::KursBeleg]) -> OrderZeile {
    let bezug = order.versand.and_then(|versand| {
        belege
            .iter()
            .filter(|b| b.abruf() <= versand)
            .max_by_key(|b| (b.kurs().zeitpunkt(), b.abruf()))
            .map(|b| b.kurs().preis())
    });
    let schluss = order
        .frist
        .filter(|_| order.zustand == AuftragsZustand::Verfallen)
        .and_then(|frist| {
            belege
                .iter()
                .filter(|b| b.kurs().zeitpunkt() == frist)
                .max_by_key(|b| b.abruf())
                .map(|b| b.kurs().preis())
        });
    let fuellpreis = fuellpreis(order);
    OrderZeile {
        nr: order.beleg.nr,
        topf: order.beleg.topf,
        isin: order.beleg.isin.clone(),
        richtung: order.beleg.richtung,
        stueck: order.beleg.stueck,
        versand: order.versand,
        frist: order.frist,
        zustand: order.zustand,
        bezug,
        limit: order.beleg.limit,
        limit_bp: abstand(Preis::aus_cent(order.beleg.limit), bezug),
        fuellpreis,
        fuellpreis_bp: abstand(fuellpreis, bezug),
        schluss,
        schluss_bp: abstand(schluss, bezug),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Kennzahlen {
    pub gefuellt: usize,
    pub verfallen: usize,
    pub mittlerer_fuellpreis_bp: Option<i64>,
    pub mittlere_bewegung_bp: Option<i64>,
}

pub fn letzte_20_tage(heute: Datum) -> Datum {
    let mut tag = heute;
    for _ in 0..19 {
        while let Some(vorher) = tag.plus_tage(-1) {
            tag = vorher;
            if depot_kern::liste::handelstag(tag) {
                break;
            }
        }
    }
    tag
}

pub(crate) fn kaufbilanz(stand: &Stand, topf: TopfId, heute: Datum) -> Kennzahlen {
    je_ki(
        &ausfuehrungsbuch(stand),
        topf,
        Richtung::Kauf,
        letzte_20_tage(heute),
    )
}

pub fn je_ki(zeilen: &[OrderZeile], topf: TopfId, richtung: Richtung, ab: Datum) -> Kennzahlen {
    let mut k = Kennzahlen::default();
    let mut preise = Vec::new();
    let mut bewegungen = Vec::new();
    for z in zeilen.iter().filter(|z| {
        z.topf == topf
            && z.richtung == richtung
            && z.versand
                .and_then(Zeitpunkt::frankfurt_tag)
                .is_some_and(|tag| tag >= ab)
    }) {
        match z.zustand {
            AuftragsZustand::Gefuellt => {
                k.gefuellt = k.gefuellt.saturating_add(1);
                preise.extend(z.fuellpreis_bp);
            }
            AuftragsZustand::Verfallen => {
                k.verfallen = k.verfallen.saturating_add(1);
                bewegungen.extend(z.schluss_bp);
            }
            _ => {}
        }
    }
    let mittel = |werte: &[i64]| -> Option<i64> {
        let summe = werte.iter().map(|wert| i128::from(*wert)).sum::<i128>();
        i64::try_from(summe.checked_div(i128::try_from(werte.len()).ok()?)?).ok()
    };
    k.mittlerer_fuellpreis_bp = mittel(&preise);
    k.mittlere_bewegung_bp = mittel(&bewegungen);
    k
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_journal::{AuftragsBeleg, Ausfuehrung, KursBeleg};
    use depot_kern::{Boerse, Kurs};
    use depot_regelwerk::Anlass;

    fn order(richtung: Richtung, zustand: AuftragsZustand) -> Result<AuftragsLage, String> {
        let tag = Datum::neu(2026, 11, 10).ok_or("Tag")?;
        let frist = Boerse::Xetra.schluss(tag).ok_or("Schluss")?;
        Ok(AuftragsLage {
            beleg: AuftragsBeleg {
                nr: 1,
                topf: TopfId::A,
                isin: Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?,
                richtung,
                stueck: 10,
                limit: if richtung == Richtung::Kauf {
                    Cent(10_100)
                } else {
                    Cent(9_900)
                },
                anlass: Anlass::Umschichtung,
            },
            zustand,
            ausfuehrungen: if zustand == AuftragsZustand::Gefuellt {
                vec![Ausfuehrung {
                    stueck: 10,
                    kurs: Cent(10_050),
                    gebuehr: Cent(150),
                    kurszeit: Some(frist),
                }]
            } else {
                Vec::new()
            },
            versand: Zeitpunkt::frankfurt(tag, 7, 40, 0),
            frist: Some(frist),
        })
    }

    fn belege(order: &AuftragsLage, schluss: i64) -> Result<[KursBeleg; 2], String> {
        let versand = order.versand.ok_or("Versand")?;
        let tag = versand.frankfurt_tag().ok_or("Tag")?;
        let vortag = Boerse::Xetra.bezugstag(tag).ok_or("Vortag")?;
        let beleg = |datum, zeit, abruf, cent| -> Result<KursBeleg, String> {
            let kurs = Kurs::neu(Cent(cent), "EUR", datum, zeit, Herkunft::Quelle)
                .map_err(|e| format!("{e:?}"))?;
            KursBeleg::neu(
                "alphavantage",
                abruf,
                b"Ausfuehrung",
                order.beleg.isin.clone(),
                kurs,
            )
            .map_err(|e| format!("{e:?}"))
        };
        let heute = order.frist.ok_or("Frist")?;
        Ok([
            beleg(
                vortag,
                Boerse::Xetra.schluss(vortag).ok_or("Vortagsschluss")?,
                versand,
                10_000,
            )?,
            beleg(tag, heute, heute, schluss)?,
        ])
    }

    #[test]
    fn kauf_gefuellt() -> Result<(), String> {
        let o = order(Richtung::Kauf, AuftragsZustand::Gefuellt)?;
        let [bezug, schluss] = belege(&o, 10_050)?;
        let z = orderzeile(&o, &[&bezug, &schluss]);
        assert_eq!(z.fuellpreis, Preis::aus_cent(Cent(10_050)));
        assert_eq!((z.limit_bp, z.fuellpreis_bp), (Some(100), Some(50)));
        Ok(())
    }

    #[test]
    fn nachgeholt_gilt_als_gefuellt() -> Result<(), String> {
        let o = order(Richtung::Kauf, AuftragsZustand::Gefuellt)?;
        let [bezug, schluss] = belege(&o, 10_050)?;
        let frist = o.frist.ok_or("Frist")?;
        let folgetag = frist
            .frankfurt_tag()
            .and_then(|d| d.plus_tage(1))
            .ok_or("Folgetag")?;
        let zeit = Boerse::Xetra.schluss(folgetag).ok_or("Späterer Schluss")?;
        let kurs = Kurs::neu(Cent(10_300), "EUR", folgetag, zeit, Herkunft::Quelle)
            .map_err(|e| format!("{e:?}"))?;
        let spaeter = KursBeleg::neu(
            "alphavantage",
            zeit,
            b"Folgetag",
            o.beleg.isin.clone(),
            kurs,
        )
        .map_err(|e| format!("{e:?}"))?;
        let z = orderzeile(&o, &[&bezug, &schluss, &spaeter]);
        let ab = letzte_20_tage(folgetag);
        let k = je_ki(&[z], TopfId::A, Richtung::Kauf, ab);
        assert_eq!(
            (k.gefuellt, k.verfallen, k.mittlerer_fuellpreis_bp),
            (1, 0, Some(50))
        );
        Ok(())
    }

    #[test]
    fn kauf_verfallen_verpasst() -> Result<(), String> {
        let o = order(Richtung::Kauf, AuftragsZustand::Verfallen)?;
        let [bezug, schluss] = belege(&o, 10_240)?;
        let z = orderzeile(&o, &[&bezug, &schluss]);
        assert_eq!(z.schluss_bp, Some(240));
        assert_eq!(z.fuellpreis, None);
        Ok(())
    }

    #[test]
    fn verkauf_verfallen() -> Result<(), String> {
        let o = order(Richtung::Verkauf, AuftragsZustand::Verfallen)?;
        let [bezug, schluss] = belege(&o, 9_700)?;
        assert_eq!(orderzeile(&o, &[&bezug, &schluss]).schluss_bp, Some(-300));
        Ok(())
    }

    #[test]
    fn ohne_bezugskurs() -> Result<(), String> {
        let o = order(Richtung::Kauf, AuftragsZustand::Gefuellt)?;
        let [_, schluss] = belege(&o, 10_050)?;
        let z = orderzeile(&o, &[&schluss]);
        assert_eq!((z.bezug, z.limit_bp, z.fuellpreis_bp), (None, None, None));
        Ok(())
    }

    #[test]
    fn fuellquote() -> Result<(), String> {
        let o = order(Richtung::Kauf, AuftragsZustand::Gefuellt)?;
        let [bezug, schluss] = belege(&o, 10_050)?;
        let a = orderzeile(&o, &[&bezug, &schluss]);
        let mut b = a.clone();
        b.zustand = AuftragsZustand::Verfallen;
        b.schluss_bp = Some(240);
        let mut andere = a.clone();
        andere.topf = TopfId::B;
        let zeilen = [a.clone(), a.clone(), a, b, andere];
        let tag = Datum::neu(2026, 11, 10).ok_or("Tag")?;
        let k = je_ki(&zeilen, TopfId::A, Richtung::Kauf, tag);
        assert_eq!(
            (
                k.gefuellt,
                k.verfallen,
                k.mittlerer_fuellpreis_bp,
                k.mittlere_bewegung_bp
            ),
            (3, 1, Some(50), Some(240))
        );
        assert_eq!(je_ki(&zeilen, TopfId::B, Richtung::Kauf, tag).gefuellt, 1);
        Ok(())
    }
}
