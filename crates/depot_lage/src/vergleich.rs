//! Vergleich seit Start (#118): Dachs und Maus gegen ein Vergleichsdepot, das das Startgeld
//! gleich gewichtet in die festen Werte `liste::VERGLEICH_ISIN` steckt und danach nur haelt,
//! und gegen reines Bargeld. Je Topf auch nach KI-Kosten: 20 € im Monat auf 5.000 € sind rund
//! 4,8 % im Jahr, die ein Topf erst verdienen muss.
//!
//! Alles kommt aus dem Journal: die Toepfe je Abschluss (`Stand::tagesschluesse`), die Kurse
//! eines Tages (`tageskurse_am`) und die gebuchten KI-Kosten (`Buchung::reserviert`, wie die
//! Budgetanzeige). Nichts davon entscheidet etwas.
//!
//! **Vereinfachung:** Das Vergleichsdepot kauft am ersten Papiertag zu Schlusskursen plus Modellspread.
//! Die Toepfe kaufen erst zum naechsten geprueften Schluss nach Versand.
//!
//! **Rundung:** Kauf zum aufgerundeten Cent plus Gebuehr, Bewertung zum abgerundeten Cent,
//! Rendite Richtung minus unendlich. Ein Verlust erscheint so nie kleiner.

use std::collections::BTreeMap;

use depot_journal::Stand;
use depot_kern::liste::{VERGLEICH_ISIN, Wert, als_universum};
use depot_kern::{BeraterId, Cent, Datum, Grenzen, Isin, Kurse, Preis, Rundung, TopfId};
use depot_ki::Buchung;
use depot_sim::papierlauf::START_JE_TOPF;
use depot_sim::{LaufFehler, tageskurse_am};

pub use depot_sim::vergleichsrechnung::{Einstieg, kaufe_gleich, rendite_bp};

/// Wert zum abgerundeten Cent; `None`, wenn ein gehaltener Wert keinen Kurs hat.
pub fn bewerte(e: &Einstieg, kurs: impl Fn(&Isin) -> Option<Preis>) -> Option<Cent> {
    e.stueck.iter().try_fold(e.bargeld, |summe, (isin, s)| {
        summe.checked_add(kurs(isin)?.mal_stueck(*s, Rundung::Ab)?)
    })
}

/// Eine Zeile des Vergleichs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VergleichZeile {
    pub name: String,
    /// Wert zum Bewertungstag; `None` heisst nicht bewertbar (Grund in `grund`).
    pub wert: Option<Cent>,
    pub rendite_bp: Option<i64>,
    /// Nur fuer Toepfe: Rendite nach den bis zum Bewertungstag gebuchten KI-Kosten.
    pub nach_ki_bp: Option<i64>,
    /// Gebuchte KI-Kosten bis zum Bewertungstag (nur Toepfe).
    pub ki_kosten: Option<Cent>,
    pub grund: Option<String>,
}

/// Der Vergleich seit Start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VergleichLage {
    /// Erster Papiertag: Einstieg des Vergleichsdepots.
    pub start: Datum,
    /// Letzter abgeschlossener Papiertag: Bewertungstag.
    pub bewertet: Datum,
    /// Abgeschlossene Papiertage.
    pub tage: usize,
    pub startgeld: Cent,
    /// Dachs, Maus, Vergleich, Bargeld.
    pub zeilen: Vec<VergleichZeile>,
    /// Vergleichswerte ohne Kurs am Einstiegstag, mit Namen.
    pub ohne: Vec<String>,
}

fn text(e: LaufFehler) -> String {
    match e {
        LaufFehler::Halt(t) => t,
    }
}

fn name(werte: &[Wert], isin: &Isin) -> String {
    werte
        .iter()
        .find(|w| w.isin == *isin)
        .map_or_else(|| isin.als_str().to_string(), |w| w.name.clone())
}

fn berater_name(b: BeraterId) -> &'static str {
    match b {
        BeraterId::Dachs => "Dachs (Topf A)",
        BeraterId::Maus => "Maus (Topf B)",
    }
}

/// Gebuchte KI-Kosten eines Beraters bis einschliesslich `tag`, wie die Budgetanzeige
/// (`reserviert`; eine nicht abgerechnete Reservierung zaehlt voll).
pub fn ki_kosten(buchungen: &[Buchung], b: BeraterId, tag: Datum) -> Option<Cent> {
    buchungen
        .iter()
        .filter(|x| x.berater == b && x.datum <= tag)
        .try_fold(Cent::NULL, |summe, x| summe.checked_add(x.reserviert))
}

fn topf_zeile(
    stand: &Stand,
    topf: TopfId,
    toepfe: &BTreeMap<TopfId, depot_kern::Topf>,
    kurse: Result<&Kurse, &str>,
    tag: Datum,
) -> VergleichZeile {
    let berater = topf.berater();
    let ki = ki_kosten(stand.ki().buchungen(), berater, tag);
    let (wert, grund) = match (toepfe.get(&topf), kurse) {
        (None, _) => (None, Some("Topf fehlt am Bewertungstag".to_string())),
        (Some(_), Err(e)) => (None, Some(e.to_string())),
        (Some(t), Ok(k)) => match t.wert_gerundet(k, Rundung::Ab) {
            Some(betrag) => (Some(betrag), None),
            None => (None, Some("ein gehaltener Wert hat keinen Kurs".into())),
        },
    };
    let rendite = wert.and_then(|w| rendite_bp(START_JE_TOPF, w));
    let nach_ki = wert
        .zip(ki)
        .and_then(|(w, k)| w.checked_sub(k))
        .and_then(|w| rendite_bp(START_JE_TOPF, w));
    VergleichZeile {
        name: berater_name(berater).into(),
        wert,
        rendite_bp: rendite,
        nach_ki_bp: nach_ki,
        ki_kosten: ki,
        grund,
    }
}

/// Der Vergleich aus dem Journal; `None` vor dem ersten abgeschlossenen Papiertag.
pub(crate) fn vergleich_lage(s: &Stand, werte: &[Wert]) -> Option<VergleichLage> {
    vergleich_bis(s, werte, None)
}

/// Vergleich seit dem ersten Papiertag, höchstens bis zum angegebenen Abschlussdatum.
pub fn vergleich_bis(s: &Stand, werte: &[Wert], bis: Option<Datum>) -> Option<VergleichLage> {
    let schluesse = s.tagesschluesse();
    let anzahl = schluesse.partition_point(|(tag, _)| bis.is_none_or(|bis| *tag <= bis));
    let schluesse = schluesse.get(..anzahl)?;
    let (start, _) = schluesse.first()?;
    let (bewertet, toepfe) = schluesse.last()?;
    let universum = als_universum(werte);
    let vergleich: Vec<Isin> = VERGLEICH_ISIN
        .iter()
        .filter_map(|i| Isin::neu(i).ok())
        .collect();
    let kurse_start = tageskurse_am(s, &universum, *start).map_err(text);
    let kurse_jetzt = tageskurse_am(s, &universum, *bewertet).map_err(text);
    let jetzt = kurse_jetzt
        .as_ref()
        .map(|k| &k.heute)
        .map_err(String::as_str);

    let mut zeilen: Vec<VergleichZeile> = [TopfId::A, TopfId::B]
        .into_iter()
        .map(|t| topf_zeile(s, t, toepfe, jetzt, *bewertet))
        .collect();

    let grenzen = Grenzen::standard();
    let einstieg = kurse_start.as_ref().map(|k| {
        let mit_kurs: Vec<(Isin, Option<Preis>)> = vergleich
            .iter()
            .map(|i| {
                let preis = k
                    .heute
                    .kurs(i)
                    .zip(depot_kern::liste::boerse_von(i))
                    .and_then(|(kurs, boerse)| grenzen.papierkurs(boerse, kurs, Rundung::Auf))
                    .and_then(Preis::aus_cent);
                (i.clone(), preis)
            })
            .collect();
        kaufe_gleich(START_JE_TOPF, &mit_kurs, |v| grenzen.gebuehren().fuer(v))
    });
    let (wert, grund, ohne) = match (&einstieg, jetzt) {
        (Err(e), _) => (None, Some(format!("Einstieg: {e}")), Vec::new()),
        (Ok(None), _) => (None, Some("Einstieg nicht berechenbar".into()), Vec::new()),
        (Ok(Some(e)), Err(g)) => (
            None,
            Some(g.to_string()),
            e.ohne.iter().map(|i| name(werte, i)).collect(),
        ),
        (Ok(Some(e)), Ok(k)) => {
            let ohne = e.ohne.iter().map(|i| name(werte, i)).collect();
            if let Some(w) = bewerte(e, |i| k.kurs(i)) {
                (Some(w), None, ohne)
            } else {
                let fehlt: Vec<String> = e
                    .stueck
                    .keys()
                    .filter(|i| k.kurs(i).is_none())
                    .map(|i| name(werte, i))
                    .collect();
                (
                    None,
                    Some(format!("kein Kurs für {}", fehlt.join(", "))),
                    ohne,
                )
            }
        }
    };
    zeilen.push(VergleichZeile {
        name: format!(
            "Vergleich: {} Werte gleich gewichtet halten",
            VERGLEICH_ISIN.len()
        ),
        rendite_bp: wert.and_then(|w| rendite_bp(START_JE_TOPF, w)),
        wert,
        nach_ki_bp: None,
        ki_kosten: None,
        grund,
    });
    zeilen.push(VergleichZeile {
        name: "Nur Bargeld".into(),
        wert: Some(START_JE_TOPF),
        rendite_bp: Some(0),
        nach_ki_bp: None,
        ki_kosten: None,
        grund: None,
    });
    Some(VergleichLage {
        start: *start,
        bewertet: *bewertet,
        tage: schluesse.len(),
        startgeld: START_JE_TOPF,
        zeilen,
        ohne,
    })
}

/// Modellierter Wert bei Verkauf am Ende des Zeitraums, mit ganzen Stücken und Restbargeld.
pub fn eunl(stand: &Stand, von: Datum, bis: Datum) -> Option<Cent> {
    let profil = depot_kern::messlatte::ibkr::LANGFRIST;
    let isin = Isin::neu(profil.etf_isin).ok()?;
    let mut liste = depot_kern::liste::beobachtungsliste().ok()?;
    liste.retain(|wert| wert.isin == isin);
    let universum = als_universum(&liste);
    let kurs = |tag| {
        tageskurse_am(stand, &universum, tag)
            .ok()?
            .heute
            .kurs(&isin)
    };
    eunl_modell(kurs(von)?, kurs(bis)?)
}

fn eunl_modell(einstieg: Preis, ausstieg: Preis) -> Option<Cent> {
    use depot_kern::Boerse;
    let profil = depot_kern::messlatte::ibkr::LANGFRIST;
    let grenzen = Grenzen::standard();
    let kaufkurs = grenzen.papierkurs(Boerse::Xetra, einstieg, Rundung::Auf)?;
    let verkaufskurs = grenzen.papierkurs(Boerse::Xetra, ausstieg, Rundung::Ab)?;
    let frei = profil.startkapital.checked_sub(profil.ordergebuehr_max)?;
    let stueck = Preis::aus_cent(kaufkurs)?.stueck_fuer(Preis::aus_cent(frei)?)?;
    if stueck == 0 {
        return Some(profil.startkapital);
    }
    frei.checked_sub(kaufkurs.mal_stueck(stueck)?)?
        .checked_add(verkaufskurs.mal_stueck(stueck)?)?
        .checked_sub(profil.ordergebuehr_max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eunl_modell_behaelt_restbargeld_und_zieht_beide_orderkosten_ab() -> Result<(), String> {
        let start = Preis::aus_cent(Cent(12_000)).ok_or("Startkurs")?;
        let ende = Preis::aus_cent(Cent(12_000)).ok_or("Endkurs")?;
        // 8 Stück, Spread 6 Cent je Seite, zwei Gebühren zu 218 Cent.
        assert_eq!(eunl_modell(start, ende), Some(Cent(99_468)));
        Ok(())
    }

    fn isin(s: &str) -> Result<Isin, String> {
        Isin::neu(s).map_err(|e| format!("{e:?}"))
    }
    fn p(cent: i64) -> Option<Preis> {
        Preis::aus_cent(Cent(cent))
    }
    fn werte() -> Result<(Isin, Isin, Isin), String> {
        Ok((
            isin("DE0007164600")?,
            isin("DE0007236101")?,
            isin("DE0008404005")?,
        ))
    }
    fn einstieg_abc() -> Result<(Einstieg, Isin, Isin, Isin), String> {
        let (a, b, c) = werte()?;
        let e = kaufe_gleich(
            Cent(100_000),
            &[
                (a.clone(), p(10_000)),
                (b.clone(), p(3_333)),
                (c.clone(), p(25_000)),
            ],
            |_| Some(Cent(150)),
        )
        .ok_or("Einstieg")?;
        Ok((e, a, b, c))
    }

    #[test]
    fn kaufe_gleich_teilt_und_zieht_gebuehr_ab() -> Result<(), String> {
        let (e, a, b, c) = einstieg_abc()?;
        assert_eq!(e.stueck.get(&a), Some(&3));
        assert_eq!(e.stueck.get(&b), Some(&9));
        assert_eq!(e.stueck.get(&c), Some(&1));
        // 30.000 + 29.997 + 25.000 + 3 × 150 = 85.447 Cent Kosten
        assert_eq!(e.bargeld, Cent(14_553));
        assert!(e.ohne.is_empty());
        Ok(())
    }

    #[test]
    fn bewerte_zum_abrundenden_cent() -> Result<(), String> {
        let (e, a, b, _) = einstieg_abc()?;
        let neu = |i: &Isin| {
            if *i == a {
                p(11_000)
            } else if *i == b {
                p(3_000)
            } else {
                p(26_000)
            }
        };
        let w = bewerte(&e, neu).ok_or("Wert")?;
        assert_eq!(w, Cent(100_553));
        assert_eq!(rendite_bp(Cent(100_000), w), Some(55));
        Ok(())
    }

    #[test]
    fn rendite_rundet_verlust_nie_klein() {
        assert_eq!(rendite_bp(Cent(100_000), Cent(99_999)), Some(-1));
        assert_eq!(rendite_bp(Cent(100_000), Cent(100_000)), Some(0));
        assert_eq!(rendite_bp(Cent(100_000), Cent(100_010)), Some(1));
        assert_eq!(rendite_bp(Cent(0), Cent(1)), None);
    }

    #[test]
    fn fehlender_kurs_am_einstieg_wird_genannt() -> Result<(), String> {
        let (a, b, c) = werte()?;
        let e = kaufe_gleich(
            Cent(100_000),
            &[(a, p(10_000)), (b.clone(), None), (c, p(25_000))],
            |_| Some(Cent(150)),
        )
        .ok_or("Einstieg")?;
        assert_eq!(e.ohne, vec![b]);
        // je Wert 500,00 €: 4 Stück zu 100 € und 1 Stück zu 250 €, je 1,50 € Gebühr
        assert_eq!(e.bargeld, Cent(34_700));
        Ok(())
    }

    #[test]
    fn fehlender_kurs_spaeter_macht_tag_nicht_bewertbar() -> Result<(), String> {
        let (e, a, _, _) = einstieg_abc()?;
        assert_eq!(bewerte(&e, |i| if *i == a { None } else { p(1) }), None);
        Ok(())
    }

    #[test]
    fn zu_teurer_wert_bleibt_bargeld_ohne_gebuehr() -> Result<(), String> {
        let (a, _, _) = werte()?;
        let e =
            kaufe_gleich(Cent(10_000), &[(a, p(20_000))], |_| Some(Cent(150))).ok_or("Einstieg")?;
        assert!(e.stueck.is_empty());
        assert_eq!(e.bargeld, Cent(10_000));
        Ok(())
    }

    #[test]
    fn ki_kosten_zaehlen_bis_zum_tag() -> Result<(), String> {
        let d = |t| Datum::neu(2026, 11, t).ok_or("Datum");
        let b = |berater, t, cent| -> Result<Buchung, String> {
            Ok(Buchung {
                berater,
                datum: d(t)?,
                zyklus: 0,
                reserviert: Cent(cent),
            })
        };
        let liste = vec![
            b(BeraterId::Dachs, 1, 24)?,
            b(BeraterId::Maus, 1, 12)?,
            b(BeraterId::Dachs, 3, 24)?,
        ];
        assert_eq!(ki_kosten(&liste, BeraterId::Dachs, d(2)?), Some(Cent(24)));
        assert_eq!(ki_kosten(&liste, BeraterId::Dachs, d(3)?), Some(Cent(48)));
        assert_eq!(ki_kosten(&liste, BeraterId::Maus, d(3)?), Some(Cent(12)));
        Ok(())
    }

    #[test]
    fn ohne_jeden_kurs_kein_einstieg() -> Result<(), String> {
        let (a, _, _) = werte()?;
        assert_eq!(
            kaufe_gleich(Cent(10_000), &[(a, None)], |_| Some(Cent(150))),
            None
        );
        Ok(())
    }
}
