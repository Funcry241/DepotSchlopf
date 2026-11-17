//! Gemeinsame, schreibfreie Auswertung der Papierorders für Anzeige und Beratung.

use depot_journal::{AuftragsLage, AuftragsZustand, Stand};
use depot_kern::{Cent, Datum, Herkunft, Isin, Preis, Rundung, TopfId, Zeitpunkt};
pub use depot_regelwerk::{Anlass, Richtung};

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
    pub modellspread_bp: Option<i64>,
    pub schluss: Option<Preis>,
    pub schluss_bp: Option<i64>,
    /// Warum die Order entstand: KI-Vorschlag oder zwingender Ausstieg.
    pub anlass: Anlass,
    /// Handelstag der letzten belegten Ausführung.
    pub ausgefuehrt_am: Option<Datum>,
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

fn modellspread(order: &AuftragsLage, belege: &[&depot_journal::KursBeleg]) -> Option<i64> {
    let boerse = depot_kern::liste::boerse_von(&order.beleg.isin)?;
    let grenzen = depot_kern::Grenzen::standard();
    let rundung = match order.beleg.richtung {
        Richtung::Kauf => Rundung::Auf,
        Richtung::Verkauf => Rundung::Ab,
    };
    let mut teile = order.ausfuehrungen.iter().map(|aus| {
        let zeit = aus.kurszeit?;
        let (ohne, mit) = belege.iter().filter(|b| b.kurs().zeitpunkt() == zeit).fold(
            (false, false),
            |(ohne, mit), b| {
                let schluss = b.kurs().preis();
                (
                    ohne || schluss.mal_stueck(1, rundung) == Some(aus.kurs),
                    mit || grenzen.papierkurs(boerse, schluss, rundung) == Some(aus.kurs),
                )
            },
        );
        match (ohne, mit) {
            (false, true) => Some(grenzen.spread_bp(boerse)),
            (true, false) => Some(0),
            _ => None,
        }
    });
    let spread = teile.next()??;
    teile.all(|teil| teil == Some(spread)).then_some(spread)
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
        modellspread_bp: modellspread(order, belege),
        schluss,
        schluss_bp: abstand(schluss, bezug),
        anlass: order.beleg.anlass,
        ausgefuehrt_am: order
            .ausfuehrungen
            .iter()
            .filter_map(|aus| aus.kurszeit?.frankfurt_tag())
            .max(),
    }
}

/// Stufe des Limitabstands zum Bezugskurs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stufe {
    Bis100,
    Bis200,
    Bis300,
    Unbelegt,
}

impl Stufe {
    pub fn von(limit_bp: Option<i64>) -> Self {
        match limit_bp.map(i64::unsigned_abs) {
            Some(0..=100) => Self::Bis100,
            Some(101..=200) => Self::Bis200,
            Some(201..=300) => Self::Bis300,
            _ => Self::Unbelegt,
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Self::Bis100 => "bis 100 bp",
            Self::Bis200 => "101–200 bp",
            Self::Bis300 => "201–300 bp",
            Self::Unbelegt => "nicht belegt",
        }
    }
}

/// Gefüllte und verfallene Orders; offene und abgelehnte zählen nicht.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Quote {
    pub gefuellt: u32,
    pub verfallen: u32,
}

impl Quote {
    /// Füllquote in Prozent, abgerundet.
    pub fn prozent(&self) -> Option<u32> {
        let summe = self.gefuellt.checked_add(self.verfallen)?;
        self.gefuellt.checked_mul(100)?.checked_div(summe)
    }

    pub fn zaehle(&mut self, zustand: AuftragsZustand) {
        match zustand {
            AuftragsZustand::Gefuellt => self.gefuellt = self.gefuellt.saturating_add(1),
            AuftragsZustand::Verfallen => self.verfallen = self.verfallen.saturating_add(1),
            _ => {}
        }
    }
}

/// Füllquote der KI-Orders je Richtung und Stufe des Limitabstands (#311 H2c). Gezählt
/// werden nur Orders aus KI-Vorschlägen mit belegtem Abstand; erzwungene Ausstiege
/// (Stopp, Notbremse, Frist, Abbau, Positivliste) wählen ihren Abstand nicht selbst.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fuellquoten {
    pub kauf: [Quote; 3],
    pub verkauf: [Quote; 3],
}

impl Fuellquoten {
    /// Die belegten Stufen in Anzeigereihenfolge, passend zu den Feldern `kauf` und `verkauf`.
    pub const STUFEN: [Stufe; 3] = [Stufe::Bis100, Stufe::Bis200, Stufe::Bis300];

    /// Keine einzige entschiedene Order in irgendeiner Stufe.
    pub fn leer(&self) -> bool {
        self.kauf
            .iter()
            .chain(&self.verkauf)
            .all(|q| q.gefuellt == 0 && q.verfallen == 0)
    }

    fn zaehle(&mut self, z: &OrderZeile) {
        let Some(i) = Self::STUFEN
            .iter()
            .position(|s| *s == Stufe::von(z.limit_bp))
        else {
            return;
        };
        let reihe = match z.richtung {
            Richtung::Kauf => &mut self.kauf,
            Richtung::Verkauf => &mut self.verkauf,
        };
        if let Some(q) = reihe.get_mut(i) {
            q.zaehle(z.zustand);
        }
    }
}

/// Füllquoten aller Töpfe seit `ab` (Versandtag). Der Limitabstand wirkt am Markt gleich,
/// egal welcher Topf die Order schickt; beide Töpfe zusammen geben mehr Beobachtungen.
pub fn fuellquoten(zeilen: &[OrderZeile], ab: Datum) -> Fuellquoten {
    let mut f = Fuellquoten::default();
    for z in zeilen.iter().filter(|z| {
        !z.anlass.zwingend()
            && z.versand
                .and_then(Zeitpunkt::frankfurt_tag)
                .is_some_and(|tag| tag >= ab)
    }) {
        f.zaehle(z);
    }
    f
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Kennzahlen {
    pub gefuellt: usize,
    pub verfallen: usize,
    pub mittlerer_fuellpreis_bp: Option<i64>,
    pub mittlere_bewegung_bp: Option<i64>,
    /// Füllquote je Limitabstand über beide Töpfe (#311 H2c). Nur `kaufbilanz` füllt sie.
    pub stufen: Fuellquoten,
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
    let buch = ausfuehrungsbuch(stand);
    let ab = letzte_20_tage(heute);
    Kennzahlen {
        stufen: fuellquoten(&buch, ab),
        ..je_ki(&buch, topf, Richtung::Kauf, ab)
    }
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
                stopp_bp: None,
                nach: Vec::new(),
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
    fn spread_nur_bei_eindeutigem_schlussbeleg() -> Result<(), String> {
        let mut o = order(Richtung::Kauf, AuftragsZustand::Gefuellt)?;
        o.ausfuehrungen.first_mut().ok_or("Ausführung")?.kurs = Cent(10_005);
        let [bezug, modell] = belege(&o, 10_000)?;
        let [_, alt] = belege(&o, 10_005)?;
        assert_eq!(orderzeile(&o, &[&bezug, &modell]).modellspread_bp, Some(5));
        assert_eq!(orderzeile(&o, &[&bezug, &alt]).modellspread_bp, Some(0));
        assert_eq!(
            orderzeile(&o, &[&bezug, &modell, &alt]).modellspread_bp,
            None
        );
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

    /// #311 H2c: je Richtung und Stufe gezählt, beide Töpfe zusammen; erzwungene, alte,
    /// offene und Orders ohne Bezugskurs zählen nicht.
    #[test]
    fn fuellquoten_je_stufe() -> Result<(), String> {
        let o = order(Richtung::Kauf, AuftragsZustand::Gefuellt)?;
        let [bezug, schluss] = belege(&o, 10_050)?;
        let basis = orderzeile(&o, &[&bezug, &schluss]);
        let zeile = |richtung, bp, zustand| OrderZeile {
            richtung,
            limit_bp: bp,
            zustand,
            ..basis.clone()
        };
        let tag = Datum::neu(2026, 11, 10).ok_or("Tag")?;
        let mut topf_b = zeile(Richtung::Kauf, Some(100), AuftragsZustand::Verfallen);
        topf_b.topf = TopfId::B;
        let mut stopp = zeile(Richtung::Verkauf, Some(-300), AuftragsZustand::Gefuellt);
        stopp.anlass = Anlass::Stopp;
        let mut alt = zeile(Richtung::Kauf, Some(100), AuftragsZustand::Gefuellt);
        alt.versand = Zeitpunkt::frankfurt(Datum::neu(2026, 10, 1).ok_or("Alt")?, 7, 40, 0);
        let zeilen = [
            zeile(Richtung::Kauf, Some(100), AuftragsZustand::Gefuellt),
            zeile(Richtung::Kauf, Some(50), AuftragsZustand::Gefuellt),
            topf_b,
            zeile(Richtung::Kauf, Some(150), AuftragsZustand::Verfallen),
            zeile(Richtung::Kauf, Some(250), AuftragsZustand::Angenommen),
            zeile(Richtung::Kauf, None, AuftragsZustand::Gefuellt),
            zeile(Richtung::Verkauf, Some(-280), AuftragsZustand::Gefuellt),
            zeile(Richtung::Verkauf, Some(-280), AuftragsZustand::Verfallen),
            stopp,
            alt,
        ];
        let f = fuellquoten(&zeilen, tag);
        let paare = |reihe: &[Quote; 3]| reihe.map(|q| (q.gefuellt, q.verfallen));
        assert_eq!(paare(&f.kauf), [(2, 1), (0, 1), (0, 0)]);
        assert_eq!(paare(&f.verkauf), [(0, 0), (0, 0), (1, 1)]);
        assert_eq!(f.kauf.first().and_then(Quote::prozent), Some(66));
        assert!(!f.leer());
        // Ohne verwertbare Daten: nur erzwungene, offene oder unbelegte Orders.
        let leer = fuellquoten(&zeilen[4..6], tag);
        assert!(leer.leer());
        assert!(fuellquoten(&[], tag).leer());
        Ok(())
    }
}
