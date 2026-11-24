//! Paarweise Auswertung eines vorab festgelegten Rangsignals.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::ops::Bound::{Excluded, Included};

use depot_journal::{KursBeleg, Stand};
use depot_kern::geld::BP_GANZ;
use depot_kern::liste::{Wert, beobachtungsliste, boerse_von};
use depot_kern::{Boerse, Cent, Datum, Grenzen, Herkunft, Isin, Preis, Rundung, Zeitpunkt};

use super::{BLICK_ZURUECK, gemeinsamer_tag, naechster_gemeinsamer_tag, preissprung, rendite_bp};

const HALTETAGE: usize = 5;
const ZIEL_BP: i128 = 200;
const STOPP_BP: i128 = 300;
const POSITION_BUDGETS: [Cent; 2] = [Cent(300_000), Cent(30_000)];

#[derive(Clone, Copy)]
struct Schluss {
    preis: Preis,
    abruf: Zeitpunkt,
}

#[derive(Default)]
struct Historie {
    kurse: BTreeMap<Isin, BTreeMap<Datum, Schluss>>,
    schnitte: BTreeMap<Isin, BTreeSet<Datum>>,
    signaltage: BTreeSet<Datum>,
}

impl Historie {
    fn laden(stand: &Stand, jetzt: Zeitpunkt) -> Self {
        let mut historie = Self::default();
        for b in stand.kurse() {
            historie.nimm_schluss(b, jetzt);
        }
        for b in stand.kapitalmassnahmen() {
            historie.schnitt(&b.massnahme.isin, b.massnahme.wirksam);
        }
        for (isin, tag) in stand
            .nachrichten()
            .iter()
            .filter(|b| b.abruf() <= jetzt)
            .filter_map(depot_journal::kapitalmassnahme::tiingo_split)
        {
            historie.schnitt(&isin, tag);
        }
        for (isin, beleg) in depot_journal::splitverdacht::aktiv(stand, Some(jetzt)) {
            if let Some(tag) = beleg.zeit().frankfurt_tag() {
                historie.schnitt(&isin, tag);
            }
        }
        historie
    }

    fn nimm_schluss(&mut self, beleg: &KursBeleg, jetzt: Zeitpunkt) {
        let kurs = beleg.kurs();
        let datum = kurs.handelstag();
        if beleg.abruf() > jetzt
            || kurs.herkunft() != Herkunft::Quelle
            || !boerse_von(beleg.isin())
                .is_some_and(|b| b.handelstag(datum) && b.schluss(datum) == Some(kurs.zeitpunkt()))
        {
            return;
        }
        let schluss = Schluss {
            preis: kurs.preis(),
            abruf: beleg.abruf(),
        };
        let bisher = self
            .kurse
            .entry(beleg.isin().clone())
            .or_default()
            .entry(datum)
            .or_insert(schluss);
        if schluss.abruf < bisher.abruf {
            *bisher = schluss;
        }
        if gemeinsamer_tag(datum) {
            self.signaltage.insert(datum);
        }
    }

    fn schnitt(&mut self, isin: &Isin, tag: Datum) {
        self.schnitte.entry(isin.clone()).or_default().insert(tag);
    }

    fn fenster(&self, isin: &Isin, tage: &[Datum]) -> Option<Vec<Schluss>> {
        let (&anfang, &ende) = tage.first().zip(tage.last())?;
        if self.schnitte.get(isin).is_some_and(|tage| {
            tage.range((Excluded(anfang), Included(ende)))
                .next()
                .is_some()
        }) {
            return None;
        }
        let kurse = self.kurse.get(isin)?;
        let schluss: Vec<_> = tage
            .iter()
            .map(|d| kurse.get(d).copied())
            .collect::<Option<_>>()?;
        schluss
            .iter()
            .zip(schluss.iter().skip(1))
            .all(|(alt, neu)| !preissprung(alt.preis, neu.preis))
            .then_some(schluss)
    }
}

struct Rang<'a> {
    wert: &'a Wert,
    bezugskurs: Preis,
    rendite: i64,
}

fn kursrendite(schluesse: &[Schluss]) -> Option<i64> {
    let anfang = schluesse.first()?.preis;
    schluesse
        .last()?
        .preis
        .checked_sub(anfang)?
        .in_bp_von(anfang)
}

fn vergleichsrendite(historie: &Historie, tage: &[Datum], korb: &[Isin]) -> Option<i64> {
    korb.iter()
        .try_fold(0_i64, |summe, isin| {
            summe.checked_add(kursrendite(&historie.fenster(isin, tage)?)?)
        })?
        .checked_div(i64::try_from(korb.len()).ok()?)
}

fn rangliste<'a>(historie: &Historie, tage: &[Datum], werte: &'a [Wert]) -> Vec<Rang<'a>> {
    let mut rang: Vec<_> = werte
        .iter()
        .filter_map(|wert| {
            let kurse = historie.fenster(&wert.isin, tage)?;
            Some(Rang {
                wert,
                bezugskurs: kurse.last()?.preis,
                rendite: kursrendite(&kurse)?,
            })
        })
        .collect();
    rang.sort_by(|a, b| {
        b.rendite
            .cmp(&a.rendite)
            .then_with(|| a.wert.isin.cmp(&b.wert.isin))
    });
    rang
}

fn handelstage_ab(boerse: Boerse, tag: Datum) -> impl Iterator<Item = Datum> {
    std::iter::successors(tag.plus_tage(1), |d| d.plus_tage(1))
        .filter(move |&d| boerse.handelstag(d))
}

fn stuecke(budget: Cent, limit: Cent, grenzen: &Grenzen) -> Option<u64> {
    let frei = budget.checked_sub(grenzen.gebuehren().fuer(budget)?)?;
    let stueck = Preis::aus_cent(frei)
        .zip(Preis::aus_cent(limit))
        .and_then(|(frei, limit)| limit.stueck_fuer(frei))
        .unwrap_or(0);
    if limit.mal_stueck(stueck)? < grenzen.min_order() {
        return Some(0);
    }
    Some(stueck)
}

fn kaufbetrag(kurs: Cent, stueck: u64, grenzen: &Grenzen) -> Option<Cent> {
    let volumen = kurs.mal_stueck(stueck)?;
    volumen.checked_add(grenzen.gebuehren().fuer(volumen)?)
}

fn verkauf(boerse: Boerse, kurs: Preis, stueck: u64, grenzen: &Grenzen) -> Option<Cent> {
    let volumen = grenzen
        .papierkurs(boerse, kurs, Rundung::Ab)?
        .mal_stueck(stueck)?;
    volumen.checked_sub(grenzen.gebuehren().fuer(volumen)?)
}

fn schwelle(kurs: Preis, fuellpreis: Cent, mit_ziel: bool) -> bool {
    // Ein Preis wird in Zehntausendstel Cent gespeichert. Die Basispunktfaktoren
    // auf den Füllpreis in Cent liefern Schwellen in derselben Einheit.
    let fuellung = i128::from(fuellpreis.0);
    let kurs = i128::from(kurs.fein());
    fuellung
        .checked_mul(10_000 - STOPP_BP)
        .is_some_and(|stopp| kurs <= stopp)
        || (mit_ziel
            && fuellung
                .checked_mul(10_000 + ZIEL_BP)
                .is_some_and(|ziel| kurs >= ziel))
}

fn ausstieg(haltekurse: &[Schluss], fuellpreis: Cent, mit_ziel: bool) -> usize {
    // Index 0 ist die Füllung. Die erste Reaktion ist nach dem ersten Halteschluss möglich.
    haltekurse
        .iter()
        .enumerate()
        .take(HALTETAGE)
        .skip(1)
        .find_map(|(i, kurs)| {
            schwelle(kurs.preis, fuellpreis, mit_ziel).then(|| i.saturating_add(1))
        })
        .unwrap_or(HALTETAGE)
}

fn korbnetto(
    historie: &Historie,
    korb: &[Isin],
    boerse: Boerse,
    tage: &[Datum],
    budget: Cent,
    grenzen: &Grenzen,
) -> Option<Cent> {
    let lokal: Vec<_> = korb
        .iter()
        .filter(|isin| boerse_von(isin) == Some(boerse))
        .collect();
    let je_wert = Cent(budget.0.checked_div(i64::try_from(lokal.len()).ok()?)?);
    lokal.into_iter().try_fold(Cent::NULL, |netto, isin| {
        let kurse = historie.fenster(isin, tage)?;
        let limit = grenzen.papierkurs(boerse, kurse.first()?.preis, Rundung::Auf)?;
        let stueck = stuecke(je_wert, limit, grenzen)?;
        if stueck == 0 {
            return Some(netto);
        }
        netto.checked_add(
            verkauf(boerse, kurse.last()?.preis, stueck, grenzen)?
                .checked_sub(kaufbetrag(limit, stueck, grenzen)?)?,
        )
    })
}

struct Ausstiegswert {
    netto: Cent,
    frueher: bool,
    rueckgang: i64,
}

impl Ausstiegswert {
    fn neu(budget: Cent, kauf: Cent, erloese: &[Cent], ausstieg: usize) -> Option<Self> {
        let bis_ausstieg = erloese.get(..=ausstieg)?;
        Some(Self {
            netto: bis_ausstieg.last()?.checked_sub(kauf)?,
            frueher: ausstieg < HALTETAGE,
            rueckgang: maximaler_rueckgang(budget, kauf, bis_ausstieg)?,
        })
    }
}

struct Paar {
    fest: Ausstiegswert,
    stopp: Ausstiegswert,
    schwellen: Ausstiegswert,
    korb: Cent,
}

fn maximaler_rueckgang(budget: Cent, kauf: Cent, verkaeufe: &[Cent]) -> Option<i64> {
    let rest = budget.checked_sub(kauf)?;
    let mut spitze = budget;
    let mut tief = 0;
    for erloes in verkaeufe {
        let wert = rest.checked_add(*erloes)?;
        spitze = spitze.max(wert);
        tief = tief.max(rendite_bp(spitze, wert)?.checked_neg()?);
    }
    Some(tief)
}

struct Fenster<'a> {
    historie: &'a Historie,
    korb: &'a [Isin],
    kandidat: &'a Rang<'a>,
    signal: Datum,
    tage: &'a [Datum],
    grenzen: &'a Grenzen,
}

enum Fall {
    Offen,
    NichtGefuellt,
    Luecke,
    Fertig(Paar),
}

impl Fenster<'_> {
    fn paar(&self, budget: Cent, jetzt: Zeitpunkt) -> Option<Fall> {
        let boerse = self.kandidat.wert.boerse;
        let (&einstieg, &ende) = self.tage.first().zip(self.tage.last())?;
        let limit = self.kandidat.bezugskurs.mal_bp(
            BP_GANZ.checked_add(self.grenzen.limit_abstand_bp())?,
            Rundung::Ab,
        )?;
        let stueck = stuecke(budget, limit, self.grenzen)?;
        if stueck == 0 {
            return Some(Fall::NichtGefuellt);
        }
        if boerse.schluss(einstieg)? > jetzt {
            return Some(Fall::Offen);
        }
        let kurse = self
            .historie
            .fenster(&self.kandidat.wert.isin, &[self.signal, einstieg])?;
        let fuellpreis = self
            .grenzen
            .papierkurs(boerse, kurse.last()?.preis, Rundung::Auf)?;
        if fuellpreis > limit {
            return Some(Fall::NichtGefuellt);
        }
        if boerse.schluss(ende)? > jetzt {
            return Some(Fall::Offen);
        }
        let kurse = self.historie.fenster(&self.kandidat.wert.isin, self.tage)?;
        let bezahlt = kaufbetrag(fuellpreis, stueck, self.grenzen)?;
        let erloese: Vec<_> = kurse
            .iter()
            .map(|schluss| verkauf(boerse, schluss.preis, stueck, self.grenzen))
            .collect::<Option<_>>()?;
        Some(Fall::Fertig(Paar {
            fest: Ausstiegswert::neu(budget, bezahlt, &erloese, HALTETAGE)?,
            stopp: Ausstiegswert::neu(
                budget,
                bezahlt,
                &erloese,
                ausstieg(&kurse, fuellpreis, false),
            )?,
            schwellen: Ausstiegswert::neu(
                budget,
                bezahlt,
                &erloese,
                ausstieg(&kurse, fuellpreis, true),
            )?,
            korb: korbnetto(
                self.historie,
                self.korb,
                boerse,
                self.tage,
                budget,
                self.grenzen,
            )?,
        }))
    }
}

#[derive(Default)]
struct Ausstiegsbilanz {
    frueher: u32,
    netto: i128,
    rueckgang: i64,
}

impl Ausstiegsbilanz {
    fn buche(&mut self, ausstieg: &Ausstiegswert) {
        self.frueher = self.frueher.saturating_add(u32::from(ausstieg.frueher));
        self.netto = self.netto.saturating_add(i128::from(ausstieg.netto.0));
        self.rueckgang = self.rueckgang.max(ausstieg.rueckgang);
    }
}

#[derive(Default)]
struct Bilanz {
    vollstaendig: u32,
    offen: u32,
    nicht_gefuellt: u32,
    luecken: u32,
    fest: Ausstiegsbilanz,
    stopp: Ausstiegsbilanz,
    schwellen: Ausstiegsbilanz,
    korb: i128,
}

impl Bilanz {
    fn buche(&mut self, fall: &Fall) {
        match fall {
            Fall::Offen => self.offen = self.offen.saturating_add(1),
            Fall::Luecke => self.luecken = self.luecken.saturating_add(1),
            Fall::NichtGefuellt => self.nicht_gefuellt = self.nicht_gefuellt.saturating_add(1),
            Fall::Fertig(paar) => {
                self.vollstaendig = self.vollstaendig.saturating_add(1);
                self.fest.buche(&paar.fest);
                self.stopp.buche(&paar.stopp);
                self.schwellen.buche(&paar.schwellen);
                self.korb = self.korb.saturating_add(i128::from(paar.korb.0));
            }
        }
    }

    fn zeile(&self, text: &mut String, budget: Cent) {
        let _ = write!(
            text,
            "| {budget} | {} | {} | {} | {} | {} | {} | ",
            self.vollstaendig,
            self.stopp.frueher,
            self.schwellen.frueher,
            self.nicht_gefuellt,
            self.luecken,
            self.offen
        );
        let mittel = |summe: i128| {
            summe
                .checked_div(i128::from(self.vollstaendig))
                .and_then(|n| i64::try_from(n).ok())
                .map_or_else(|| "–".into(), |n| Cent(n).to_string())
        };
        let [fest, stopp, schwellen, korb] = [
            self.fest.netto,
            self.stopp.netto,
            self.schwellen.netto,
            self.korb,
        ];
        let [rueckgang_fest, rueckgang_stopp, rueckgang_schwellen] = [
            self.fest.rueckgang,
            self.stopp.rueckgang,
            self.schwellen.rueckgang,
        ]
        .map(|bp| {
            if self.vollstaendig == 0 {
                "–".into()
            } else {
                format!("{bp} bp")
            }
        });
        let _ = writeln!(
            text,
            "{} | {} | {} | {} | {} | {} | {} | {} | {} | {rueckgang_fest} | {rueckgang_stopp} | {rueckgang_schwellen} |",
            mittel(fest),
            mittel(stopp),
            mittel(schwellen),
            mittel(schwellen.saturating_sub(fest)),
            mittel(schwellen.saturating_sub(stopp)),
            mittel(korb),
            mittel(fest.saturating_sub(korb)),
            mittel(stopp.saturating_sub(korb)),
            mittel(schwellen.saturating_sub(korb))
        );
    }
}

struct Messung {
    bilanzen: [(Cent, Bilanz); POSITION_BUDGETS.len()],
    fehlende_fenster: usize,
    fehlende_raenge: usize,
}

fn messe(historie: &Historie, korb: &[Isin], werte: &[Wert], jetzt: Zeitpunkt) -> Messung {
    let tage: Vec<_> = historie.signaltage.iter().copied().collect();
    let grenzen = Grenzen::standard();
    let mut bilanzen = POSITION_BUDGETS.map(|budget| (budget, Bilanz::default()));
    let mut unvollstaendig = 0_usize;
    let mut titel_luecken = 0_usize;
    for fenster in tage.windows(BLICK_ZURUECK + 1) {
        if !fenster
            .iter()
            .zip(fenster.iter().skip(1))
            .all(|(&alt, &neu)| naechster_gemeinsamer_tag(alt) == Some(neu))
        {
            unvollstaendig = unvollstaendig.saturating_add(1);
            continue;
        }
        let Some(korbrendite) = vergleichsrendite(historie, fenster, korb) else {
            unvollstaendig = unvollstaendig.saturating_add(1);
            continue;
        };
        let rang = rangliste(historie, fenster, werte);
        titel_luecken = titel_luecken.saturating_add(werte.len().saturating_sub(rang.len()));
        let Some(&signal) = fenster.last() else {
            continue;
        };
        for kandidat in rang.iter().take(2).take_while(|r| r.rendite > korbrendite) {
            let haltetage: Vec<_> = handelstage_ab(kandidat.wert.boerse, signal)
                .take(HALTETAGE + 1)
                .collect();
            let fenster = Fenster {
                historie,
                korb,
                kandidat,
                signal,
                tage: &haltetage,
                grenzen: &grenzen,
            };
            for (budget, bilanz) in &mut bilanzen {
                bilanz.buche(&fenster.paar(*budget, jetzt).unwrap_or(Fall::Luecke));
            }
        }
    }
    Messung {
        bilanzen,
        fehlende_fenster: unvollstaendig,
        fehlende_raenge: titel_luecken,
    }
}

pub(super) fn bericht(text: &mut String, stand: &Stand, jetzt: Zeitpunkt, korb: &[Isin]) {
    text.push_str("\n## Paarweise Kurs-Rückschau zum Ausstieg\n\n");
    let Ok(werte) = beobachtungsliste() else {
        text.push_str("Die Beobachtungsliste ist nicht lesbar.\n");
        return;
    };
    let historie = Historie::laden(stand, jetzt);
    let messung = messe(&historie, korb, &werte, jetzt);
    text.push_str(concat!(
        "Kursmodell ohne KI-Aufrufe, KI-Kosten und Nachrichtensignale. Es nutzt alle brauchbaren Abschnitte der gespeicherten Quellkurse, einschließlich später nachgeladener Historie. Das Modell setzt Schlusskurse ab Börsenschluss voraus.\n\n",
        "Die zwei stärksten belegbaren Werte der aktuellen Beobachtungsliste kommen nach 20 gemeinsamen Handelstagen für einen Kauf infrage, wenn sie den festen Vergleichskorb schlagen. Der Einstieg erfolgt am nächsten Schluss des jeweiligen Börsenplatzes mit 1 % Kauflimit ab Bezugskurs. Das Modell verwendet ganze Stücke, die Mindestorder sowie Gebühren und Spread des Papierhandels. Alle drei Varianten nutzen dieselbe Füllung. Die feste Variante verkauft am fünften Handelstag nach dem Kauf. Die Stoppvariante reagiert bei −3 % ab Füllpreis. Die Ziel-und-Stopp-Variante reagiert bei +2 % oder −3 %. In beiden Fällen erfolgt der Verkauf am nächsten Börsenschluss, spätestens am fünften Handelstag. Der Ausführungskurs kann von der Schwelle abweichen. Der Erlös bleibt bis zum Fensterende in Bargeld. Der Nettounterschied zwischen Ziel und Stopp und dem alleinigen Stopp misst den Einfluss der Gewinnschwelle von +2 %.\n\n",
        "Die Nettobeträge sind Durchschnittswerte je vollständigem Einstieg. Bargeld bleibt unverzinst und hat 0 € Netto. Der Korb teilt dasselbe Einsatzbudget gleich auf die Vergleichswerte desselben Börsenplatzes auf und bewertet zum fünften Schluss mit denselben Handelskosten. Zu kleine Anteile bleiben Bargeld. Bei 300 € Einsatzbudget liegt jeder Anteil des aktuellen Korbs unter der Mindestorder von 100 €. Dieser Korb bleibt vollständig in Bargeld. Der maximale Rückgang gilt innerhalb eines einzelnen Fensters. Die Zeitfenster können überlappen.\n\n"
    ));
    if let Some((von, bis)) = historie.signaltage.first().zip(historie.signaltage.last()) {
        let _ = writeln!(text, "Gemeinsame Quellkurstage vom {von} bis {bis}.");
    }
    let _ = writeln!(
        text,
        "{} gemeinsame Kurstage. {} Rangfenster mit fehlenden Kursen oder Unterbrechungen im Korb. {} fehlende Wertränge in den übrigen Fenstern.\n",
        historie.signaltage.len(),
        messung.fehlende_fenster,
        messung.fehlende_raenge
    );
    text.push_str("| Einsatzbudget | Vollständige Einstiege | Früher verkauft mit Stopp | Früher verkauft mit Ziel und Stopp | Kauf ausgeblieben | Kurslücke oder Bruch | Offen | Netto fest | Netto Stopp | Netto Ziel und Stopp | Ziel und Stopp − Fest | Ziel und Stopp − Stopp | Netto Korb | Fest zum Korb | Stopp zum Korb | Ziel und Stopp zum Korb | Max. Rückgang fest | Max. Rückgang Stopp | Max. Rückgang Ziel und Stopp |\n|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n");
    for (budget, bilanz) in &messung.bilanzen {
        bilanz.zeile(text, *budget);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notiere(historie: &mut Historie, isin: &Isin, tag: Datum, cent: i64) -> Result<(), String> {
        let schluss = Schluss {
            preis: Preis::aus_cent(Cent(cent)).ok_or("Preis")?,
            abruf: Zeitpunkt::neu(tag, 23, 0, 0).ok_or("Abruf")?,
        };
        historie
            .kurse
            .entry(isin.clone())
            .or_default()
            .insert(tag, schluss);
        Ok(())
    }

    #[test]
    fn nur_erster_verfuegbarer_boersenschluss_zaehlt() -> Result<(), String> {
        use depot_kern::Kurs;

        let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let tag = Datum::neu(2026, 11, 24).ok_or("Datum")?;
        let intraday = Zeitpunkt::frankfurt(tag, 12, 0, 0).ok_or("Uhrzeit")?;
        let schluss = Boerse::Xetra.schluss(tag).ok_or("Börsenschluss")?;
        let abruf = Zeitpunkt::frankfurt(tag, 18, 0, 0).ok_or("Abruf")?;
        let jetzt = Zeitpunkt::frankfurt(tag, 19, 0, 0).ok_or("Zeit")?;
        let mut historie = Historie::default();
        for (preis, zeit, eingang) in [
            (9_900, intraday, intraday),
            (10_100, schluss, jetzt),
            (10_000, schluss, abruf),
        ] {
            let kurs = Kurs::neu(Cent(preis), "EUR", tag, zeit, Herkunft::Quelle)
                .map_err(|e| format!("{e:?}"))?;
            let beleg = KursBeleg::neu("kursprobe", eingang, b"Testkurs", isin.clone(), kurs)?;
            historie.nimm_schluss(&beleg, jetzt);
        }
        let preis = historie
            .kurse
            .get(&isin)
            .and_then(|kurse| kurse.get(&tag))
            .map(|b| b.preis);
        assert_eq!(preis, Preis::aus_cent(Cent(10_000)));
        Ok(())
    }

    #[test]
    fn gleiche_fuellung_schwelle_wirkt_erst_am_folgeschluss() -> Result<(), String> {
        let wert = beobachtungsliste()?
            .into_iter()
            .find(|w| w.boerse == Boerse::Xetra)
            .ok_or("Wert")?;
        let signal = Datum::neu(2026, 11, 23).ok_or("Signal")?;
        let tage: Vec<_> = handelstage_ab(wert.boerse, signal)
            .take(HALTETAGE + 1)
            .collect();
        let mut historie = Historie::default();
        notiere(&mut historie, &wert.isin, signal, 10_000)?;
        for (&tag, preis) in tage
            .iter()
            .zip([10_000, 10_400, 9_600, 10_200, 10_000, 11_000])
        {
            notiere(&mut historie, &wert.isin, tag, preis)?;
        }
        let korb = [wert.isin.clone()];
        let kandidat = Rang {
            wert: &wert,
            bezugskurs: Preis::aus_cent(Cent(10_000)).ok_or("Preis")?,
            rendite: 400,
        };
        let grenzen = Grenzen::standard();
        let budget = Cent(30_000);
        let pruefe = |historie: &Historie, zeit| {
            Fenster {
                historie,
                korb: &korb,
                kandidat: &kandidat,
                signal,
                tage: &tage,
                grenzen: &grenzen,
            }
            .paar(budget, zeit)
        };
        let jetzt = Zeitpunkt::neu(*tage.last().ok_or("Ende")?, 23, 0, 0).ok_or("Zeit")?;
        let Some(Fall::Fertig(paar)) = pruefe(&historie, jetzt) else {
            return Err("Vergleich fehlt".into());
        };
        assert!(paar.schwellen.frueher);
        assert!(paar.stopp.frueher);
        assert_eq!(paar.schwellen.netto, Cent(-1_120));
        assert_eq!(paar.stopp.netto, Cent(78));
        assert_eq!(paar.fest.netto, Cent(1_678));
        assert_eq!(paar.korb, paar.fest.netto);
        let vor_frist =
            Zeitpunkt::neu(*tage.get(3).ok_or("Dritter Schluss")?, 23, 0, 0).ok_or("Zeit")?;
        assert!(matches!(pruefe(&historie, vor_frist), Some(Fall::Offen)));
        let einstieg = *tage.first().ok_or("Einstieg")?;
        notiere(&mut historie, &wert.isin, einstieg, 10_101)?;
        assert!(matches!(
            pruefe(&historie, jetzt),
            Some(Fall::NichtGefuellt)
        ));
        notiere(&mut historie, &wert.isin, einstieg, 10_000)?;
        let dritter = *tage.get(3).ok_or("Dritter Schluss")?;
        historie
            .kurse
            .get_mut(&wert.isin)
            .ok_or("Kurse")?
            .remove(&dritter);
        assert!(pruefe(&historie, jetzt).is_none());
        notiere(&mut historie, &wert.isin, dritter, 10_200)?;
        historie.schnitt(&wert.isin, dritter);
        assert!(pruefe(&historie, jetzt).is_none());
        Ok(())
    }

    #[test]
    fn schwellen_sind_exakt_und_frist_folgt_der_boerse() -> Result<(), String> {
        let ab = Datum::neu(2026, 11, 24).ok_or("Kauftag")?;
        assert_eq!(
            handelstage_ab(Boerse::Xetra, ab).nth(4),
            Datum::neu(2026, 12, 1)
        );
        assert_eq!(
            handelstage_ab(Boerse::Nyse, ab).nth(4),
            Datum::neu(2026, 12, 2)
        );
        let pruefe = |kurs| Preis::lies_euro(kurs).map(|preis| schwelle(preis, Cent(10_000), true));
        assert_eq!(pruefe("101.999999"), Some(false));
        assert_eq!(pruefe("102"), Some(true));
        assert_eq!(pruefe("97.000001"), Some(false));
        assert_eq!(pruefe("97"), Some(true));
        Ok(())
    }

    #[test]
    fn gewinnschwelle_kappt_nur_die_variante_mit_ziel() -> Result<(), String> {
        let tag = Datum::neu(2026, 11, 24).ok_or("Datum")?;
        let abruf = Zeitpunkt::neu(tag, 23, 0, 0).ok_or("Zeit")?;
        let kurse = [10_000, 10_200, 10_300, 10_400, 10_500, 10_600]
            .into_iter()
            .map(|cent| {
                Some(Schluss {
                    preis: Preis::aus_cent(Cent(cent))?,
                    abruf,
                })
            })
            .collect::<Option<Vec<_>>>()
            .ok_or("Kurse")?;
        assert_eq!(ausstieg(&kurse, Cent(10_000), true), 2);
        assert_eq!(ausstieg(&kurse, Cent(10_000), false), HALTETAGE);
        Ok(())
    }

    #[test]
    fn messung_behaelt_beide_kursabschnitte_und_verwirft_fenster_ueber_luecken()
    -> Result<(), String> {
        let werte: Vec<_> = beobachtungsliste()?
            .into_iter()
            .filter(|w| w.boerse == Boerse::Xetra)
            .take(2)
            .collect();
        let [kandidat, referenz] = werte.as_slice() else {
            return Err("Zwei Werte fehlen".into());
        };
        let korb = [referenz.isin.clone()];
        let mut historie = Historie::default();
        for monat in [1, 4] {
            let anfang = Datum::neu(2026, monat, 1).ok_or("Anfang")?;
            let signalreihe: Vec<_> = handelstage_ab(Boerse::Xetra, anfang)
                .filter(|&d| gemeinsamer_tag(d))
                .take(BLICK_ZURUECK + 1)
                .collect();
            let signal = *signalreihe.last().ok_or("Signal")?;
            for &tag in &signalreihe {
                notiere(
                    &mut historie,
                    &kandidat.isin,
                    tag,
                    if tag == signal { 10_500 } else { 10_000 },
                )?;
                notiere(&mut historie, &referenz.isin, tag, 10_000)?;
                historie.signaltage.insert(tag);
            }
            for tag in handelstage_ab(Boerse::Xetra, signal).take(HALTETAGE + 1) {
                notiere(&mut historie, &kandidat.isin, tag, 10_500)?;
                notiere(&mut historie, &referenz.isin, tag, 10_000)?;
            }
        }
        let jetzt = Datum::neu(2026, 6, 1)
            .and_then(|d| Zeitpunkt::neu(d, 23, 0, 0))
            .ok_or("Zeit")?;
        let messung = messe(&historie, &korb, &werte, jetzt);
        assert_eq!(messung.fehlende_fenster, 20);
        assert_eq!(messung.fehlende_raenge, 0);
        assert!(
            messung
                .bilanzen
                .iter()
                .all(|(_, b)| b.vollstaendig == 2 && b.luecken == 0)
        );
        Ok(())
    }
}
