//! Kursbasierte Rückschau. Diese Rechnung behandelt belegte Schlusskurse als
//! ab Börsenschluss bekannt. Sie erzeugt weder einen Wissensstichtag noch einen Papierauftrag.

mod aufwaertsdruck;
mod ausstiegsvergleich;
mod ranggruppen;
mod rotation;
mod schattenkonten;
pub use rotation::rotationsbericht;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use depot_journal::Stand;
use depot_kern::liste::VERGLEICH_ISIN;
use depot_kern::{Boerse, Cent, Datum, Herkunft, Isin, Preis, Rundung, Zeitpunkt};

use crate::vergleichsrechnung::{kaufe_gleich, rendite_bp};

const BLICK_ZURUECK: usize = 20;
const HALTEDAUERN: [usize; 5] = [5, 10, 20, 60, 120];

struct Kurstag {
    datum: Datum,
    kurse: BTreeMap<Isin, Preis>,
}

struct Fall {
    start: Cent,
    gebuehr: Cent,
    mindestens: Cent,
}

const FAELLE: [Fall; 2] = [
    Fall {
        start: Cent(1_000_000),
        gebuehr: Cent(150),
        mindestens: Cent(10_000),
    },
    Fall {
        start: Cent(100_000),
        gebuehr: Cent(218),
        mindestens: Cent(22_000),
    },
];

fn gemeinsamer_tag(datum: Datum) -> bool {
    Boerse::Xetra.handelstag(datum)
        && Boerse::Nyse.handelstag(datum)
        && !depot_kern::boerse::target_geschlossen(datum)
}

fn kurstage(stand: &Stand, jetzt: Zeitpunkt, werte: &[Isin]) -> Vec<Kurstag> {
    let mut reihe = BTreeMap::<Datum, BTreeMap<Isin, Preis>>::new();
    for beleg in stand.kurse() {
        let k = beleg.kurs();
        let datum = k.handelstag();
        if beleg.abruf() <= jetzt
            && k.herkunft() == Herkunft::Quelle
            && gemeinsamer_tag(datum)
            && werte.contains(beleg.isin())
        {
            // Die erste belegte Fassung verwenden, keine späteren Kurskorrekturen.
            reihe
                .entry(datum)
                .or_default()
                .entry(beleg.isin().clone())
                .or_insert(k.preis());
        }
    }
    let letzter_split = stand
        .kapitalmassnahmen()
        .iter()
        .filter(|b| werte.contains(&b.massnahme.isin))
        .map(|b| b.massnahme.wirksam)
        .chain(
            depot_journal::splitverdacht::aktiv(stand, Some(jetzt))
                .values()
                .filter_map(|b| b.zeit().frankfurt_tag()),
        )
        .max();
    let mut tage = Vec::<Kurstag>::new();
    for (datum, kurse) in reihe {
        if letzter_split.is_some_and(|grenze| datum < grenze) || kurse.len() != werte.len() {
            tage.clear();
            continue;
        }
        if let Some(vorher) = tage.last()
            && (naechster_gemeinsamer_tag(vorher.datum) != Some(datum)
                || grober_preissprung(&vorher.kurse, &kurse))
        {
            tage.clear();
        }
        tage.push(Kurstag { datum, kurse });
    }
    tage
}

fn naechster_gemeinsamer_tag(mut datum: Datum) -> Option<Datum> {
    loop {
        datum = datum.plus_tage(1)?;
        if gemeinsamer_tag(datum) {
            return Some(datum);
        }
    }
}

fn grober_preissprung(vorher: &BTreeMap<Isin, Preis>, heute: &BTreeMap<Isin, Preis>) -> bool {
    heute
        .iter()
        .any(|(isin, kurs)| vorher.get(isin).is_some_and(|alt| preissprung(*alt, *kurs)))
}

fn preissprung(vorher: Preis, heute: Preis) -> bool {
    let kleiner = i128::from(heute.min(vorher).fein());
    let groesser = i128::from(heute.max(vorher).fein());
    kleiner
        .checked_mul(3)
        .zip(groesser.checked_mul(2))
        .is_some_and(|(klein, gross)| klein <= gross)
}

fn kurs(preis: Preis, richtung: Rundung) -> Option<Preis> {
    let bp = match richtung {
        Rundung::Auf => 10_005,
        Rundung::Ab => 9_995,
    };
    Preis::aus_cent(preis.mal_bp(bp, richtung)?)
}

fn erloes(preis: Preis, stueck: u64, fall: &Fall) -> Option<Cent> {
    kurs(preis, Rundung::Ab)?
        .mal_stueck(stueck, Rundung::Ab)?
        .checked_sub(fall.gebuehr)
}

fn korb(von: &Kurstag, bis: &Kurstag, fall: &Fall) -> Option<Cent> {
    let preise: Vec<_> = von
        .kurse
        .iter()
        .map(|(isin, p)| (isin.clone(), kurs(*p, Rundung::Auf)))
        .collect();
    let einkauf = kaufe_gleich(fall.start, &preise, |_| Some(fall.gebuehr))?;
    einkauf
        .stueck
        .iter()
        .try_fold(einkauf.bargeld, |summe, (isin, stueck)| {
            summe.checked_add(erloes(*bis.kurse.get(isin)?, *stueck, fall)?)
        })
}

struct Bestand {
    isin: Isin,
    stueck: u64,
    faellig: usize,
}

struct Rechnung {
    bargeld: Cent,
    bestand: Vec<Bestand>,
    kaeufer: usize,
    spitze: Cent,
    rueckgang: i64,
}

impl Rechnung {
    fn wert(&self, tag: &Kurstag, fall: &Fall) -> Option<Cent> {
        self.bestand.iter().try_fold(self.bargeld, |summe, b| {
            summe.checked_add(erloes(*tag.kurse.get(&b.isin)?, b.stueck, fall)?)
        })
    }

    fn tag(&mut self, tage: &[Kurstag], index: usize, dauer: usize, fall: &Fall) -> Option<()> {
        let heute = tage.get(index)?;
        let nicht_faellig = std::mem::take(&mut self.bestand);
        for b in nicht_faellig {
            if b.faellig > index {
                self.bestand.push(b);
            } else {
                self.bargeld = self.bargeld.checked_add(erloes(
                    *heute.kurse.get(&b.isin)?,
                    b.stueck,
                    fall,
                )?)?;
            }
        }
        let wert = self.wert(heute, fall)?;
        // Die Signalkurse enden vor dem Ausführungstag.
        let signal = tage.get(index.checked_sub(1)?)?;
        let basis = tage.get(index.checked_sub(BLICK_ZURUECK.saturating_add(1))?)?;
        let mut rang: Vec<_> = signal
            .kurse
            .iter()
            .filter_map(|(isin, nachher)| {
                let vorher = *basis.kurse.get(isin)?;
                Some((isin, nachher.checked_sub(vorher)?.in_bp_von(vorher)?))
            })
            .collect();
        rang.sort_by(|(a, x), (b, y)| y.cmp(x).then_with(|| a.cmp(b)));

        let anteil = Cent(wert.0.checked_mul(30)?.checked_div(100)?);
        for (isin, bp) in rang.into_iter().take(2) {
            if bp <= 0 || self.bestand.len() >= 2 {
                break;
            }
            if !self.bestand.iter().any(|b| &b.isin == isin) {
                self.kaufen(isin, heute, anteil, fall, index.checked_add(dauer)?)?;
            }
        }
        let wert = self.wert(heute, fall)?;
        self.spitze = self.spitze.max(wert);
        self.rueckgang = self
            .rueckgang
            .max(rendite_bp(self.spitze, wert)?.checked_neg()?);
        Some(())
    }

    fn kaufen(
        &mut self,
        isin: &Isin,
        tag: &Kurstag,
        anteil: Cent,
        fall: &Fall,
        faellig: usize,
    ) -> Option<()> {
        let kurs = kurs(*tag.kurse.get(isin)?, Rundung::Auf)?;
        let deckung = self.bargeld.min(anteil).checked_sub(fall.gebuehr)?;
        let stueck = kurs.stueck_fuer(Preis::aus_cent(deckung)?)?;
        let volumen = kurs.mal_stueck(stueck, Rundung::Auf)?;
        if volumen < fall.mindestens {
            return Some(());
        }
        self.bargeld = self
            .bargeld
            .checked_sub(volumen.checked_add(fall.gebuehr)?)?;
        self.kaeufer = self.kaeufer.saturating_add(1);
        self.bestand.push(Bestand {
            isin: isin.clone(),
            stueck,
            faellig,
        });
        Some(())
    }
}

struct Ergebnis {
    kaeufer: usize,
    netto: Cent,
    abstand: Cent,
    rueckgang: i64,
}

fn rechne(tage: &[Kurstag], beginn: usize, dauer: usize, fall: &Fall) -> Option<Ergebnis> {
    let erster = tage.get(beginn)?;
    let letzter = tage.last()?;
    let mut r = Rechnung {
        bargeld: fall.start,
        bestand: Vec::new(),
        kaeufer: 0,
        spitze: fall.start,
        rueckgang: 0,
    };
    for index in beginn..tage.len() {
        r.tag(tage, index, dauer, fall)?;
    }
    let wert = r.wert(letzter, fall)?;
    Some(Ergebnis {
        kaeufer: r.kaeufer,
        netto: wert.checked_sub(fall.start)?,
        abstand: wert.checked_sub(korb(erster, letzter, fall)?)?,
        rueckgang: r.rueckgang,
    })
}

fn zeile(text: &mut String, name: &str, fall: &Fall, dauer: usize, ergebnis: Option<Ergebnis>) {
    let _ = write!(text, "| {name} | {} | {dauer} | ", fall.start);
    if let Some(e) = ergebnis {
        let _ = writeln!(
            text,
            "{} | {} | {} | {} bp |",
            e.kaeufer, e.netto, e.abstand, e.rueckgang
        );
    } else {
        text.push_str("– | – | – | – |\n");
    }
}

/// Schattenkonten des gewählten Exportfensters, ohne Buchung oder weitere KI-Aufrufe.
pub fn schattenkontenbericht(stand: &Stand, jetzt: Zeitpunkt, von: Datum, bis: Datum) -> String {
    schattenkonten::bericht(
        stand,
        &ausstiegsvergleich::Historie::laden(stand, jetzt),
        von,
        bis,
        jetzt,
    )
}

/// Separater Exportabschnitt, unabhängig vom Zeitraum der echten Tage.
pub fn bericht(stand: &Stand, jetzt: Zeitpunkt) -> String {
    let werte: Vec<_> = VERGLEICH_ISIN
        .into_iter()
        .filter_map(|isin| Isin::neu(isin).ok())
        .collect();
    let tage = kurstage(stand, jetzt, &werte);
    let beginn = BLICK_ZURUECK.saturating_add(1);
    let halb = beginn.saturating_add(tage.len().saturating_sub(beginn).div_euclid(2));
    let mut text = String::from(
        "\n## Historische Kurs-Rückschau\n\nReines Kursmodell ohne KI und Nachrichten. Schlusskurse gelten zum Börsenschluss als bekannt. Die zwei stärksten Werte über 20 gemeinsame Börsentage kommen bei positiver Stärke für einen Kauf am folgenden Schluss infrage. Je Wert höchstens 30 %, ganze Stücke, 5 bp Spread je Seite. Je Order 1,50 € (10.000 €, Mindestorder 100 €) oder 2,18 € (1.000 €, Mindestorder 220 €). Der feste Vergleichskorb beginnt am gleichen Tag, verteilt gleich und rechnet mit denselben Gebühren und Spreads. Offene Positionen werden zum Endschluss nach modellierten Verkaufskosten bewertet. Die zweite Hälfte startet erneut mit Bargeld.\n\n",
    );
    let _ = writeln!(
        text,
        "{} lückenlos belegte gemeinsame Handelstage. Fehlende Kurse oder grobe Preissprünge unterbrechen die Reihe.\n",
        tage.len()
    );
    text.push_str("| Abschnitt | Start | Haltedauer | Trades (Einstiege) | Netto | Abstand zum Korb | Größter Rückgang |\n|---|---:|---:|---:|---:|---:|---:|\n");
    for fall in &FAELLE {
        for dauer in HALTEDAUERN {
            let voll = (tage.len().saturating_sub(beginn) > dauer)
                .then(|| rechne(&tage, beginn, dauer, fall))
                .flatten();
            let spaet = (tage.len().saturating_sub(halb) > dauer)
                .then(|| rechne(&tage, halb, dauer, fall))
                .flatten();
            let geeignet = voll.as_ref().zip(spaet.as_ref()).is_some_and(|(a, b)| {
                a.abstand > Cent::NULL && b.abstand > Cent::NULL && b.kaeufer > 0
            });
            zeile(&mut text, "Gesamt", fall, dauer, voll);
            zeile(
                &mut text,
                if geeignet {
                    "Zweite Hälfte · über Korb"
                } else {
                    "Zweite Hälfte"
                },
                fall,
                dauer,
                spaet,
            );
        }
    }
    let historie = ausstiegsvergleich::Historie::laden(stand, jetzt);
    let beobachtungsliste = depot_kern::liste::beobachtungsliste().ok();
    ausstiegsvergleich::bericht(
        &mut text,
        &historie,
        jetzt,
        &werte,
        beobachtungsliste.as_deref(),
    );
    ranggruppen::bericht(
        &mut text,
        &historie,
        jetzt,
        &werte,
        beobachtungsliste.as_deref(),
    );
    aufwaertsdruck::bericht(&mut text, stand, &historie, jetzt, &werte);
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ausfuehrungskurs_aendert_das_signal_nicht() -> Result<(), String> {
        let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let datum = Datum::neu(2030, 1, 11).ok_or("Datum")?;
        let erzeugen = |cents: i64| -> Result<Kurstag, String> {
            Ok(Kurstag {
                datum,
                kurse: BTreeMap::from([(
                    isin.clone(),
                    Preis::aus_cent(Cent(cents)).ok_or("Preis")?,
                )]),
            })
        };
        let mut tage = (0..20)
            .map(|_| erzeugen(10_000))
            .collect::<Result<Vec<_>, _>>()?;
        tage.push(erzeugen(9_900)?);
        tage.push(erzeugen(12_000)?);
        tage.push(erzeugen(12_100)?);
        let fall = FAELLE.first().ok_or("Fall")?;
        let mut rechnung = Rechnung {
            bargeld: fall.start,
            bestand: Vec::new(),
            kaeufer: 0,
            spitze: fall.start,
            rueckgang: 0,
        };
        rechnung.tag(&tage, 21, 5, fall).ok_or("Tag 21")?;
        assert_eq!(rechnung.kaeufer, 0);
        rechnung.tag(&tage, 22, 5, fall).ok_or("Tag 22")?;
        assert_eq!(rechnung.kaeufer, 1);
        assert!(!rechnung.bestand.is_empty());
        Ok(())
    }
}
