//! Feste Wertgruppen der Beobachtungsliste in drei aufeinanderfolgenden Börsensitzungen.

use std::collections::BTreeMap;

use depot_journal::{KursBeleg, NachrichtBeleg, Stand};
use depot_kern::liste::{VERGLEICH_ISIN, boerse_von};
use depot_kern::{Boerse, Datum, Grenzen, Herkunft, Isin, Preis, Zeitpunkt};

const MARKT_RUECKGANG_BP: i64 = -50;

#[derive(Debug)]
pub(crate) struct Gruppe {
    pub(crate) name: &'static str,
    pub(crate) isins: &'static [&'static str],
}

const XETRA: &[Gruppe] = &[
    Gruppe {
        name: "Versicherer",
        isins: &["DE0008404005", "DE0008430026"],
    },
    Gruppe {
        name: "Autohersteller",
        isins: &["DE0005190003", "DE0007100000"],
    },
    Gruppe {
        name: "Software und Halbleiter",
        isins: &["DE0007164600", "DE0006231004"],
    },
];

const US: &[Gruppe] = &[
    Gruppe {
        name: "Technologiewerte",
        isins: &[
            "US0378331005",
            "US02079K3059",
            "US5949181045",
            "US67066G1040",
        ],
    },
    Gruppe {
        name: "Finanzwerte",
        isins: &["US46625H1005", "US92826C8394"],
    },
];

pub(crate) fn gruppen(boerse: Boerse) -> &'static [Gruppe] {
    match boerse {
        Boerse::Xetra => XETRA,
        Boerse::Nyse => US,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Signal {
    pub(crate) gruppe: &'static str,
    pub(crate) isin: Isin,
    pub(crate) referenz: Preis,
    pub(crate) referenz_abruf: Zeitpunkt,
    pub(crate) tag: Datum,
    pub(crate) markt_rueckgang_bp: i64,
    pub(crate) markt_erholung_bp: i64,
    pub(crate) gruppe_rueckgang_bp: i64,
    pub(crate) gruppe_erholung_bp: i64,
    pub(crate) breite_rueckgang: usize,
    pub(crate) breite_erholung: usize,
    pub(crate) titel: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Befund {
    pub(crate) signale: Vec<Signal>,
    pub(crate) gruppen_bewertet: usize,
    pub(crate) gruppen_fehlend: usize,
}

#[derive(Debug, Default)]
pub(crate) struct Kursindex<'a> {
    kurse: BTreeMap<&'a Isin, BTreeMap<Datum, Vec<&'a KursBeleg>>>,
    splits: BTreeMap<Isin, Vec<(Datum, Zeitpunkt)>>,
}

fn split(beleg: &NachrichtBeleg) -> Option<(Isin, Datum)> {
    if beleg.quelle() == depot_kern::quelle::QUELLE_XETRA_SPLITVERDACHT {
        let (wert, _) = beleg.text().rsplit_once(") am ")?;
        let (_, isin) = wert.rsplit_once('(')?;
        return Some((Isin::neu(isin).ok()?, beleg.zeit().frankfurt_tag()?));
    }
    depot_journal::kapitalmassnahme::tiingo_split(beleg)
}

impl<'a> Kursindex<'a> {
    pub(crate) fn historie(stand: &'a Stand, jetzt: Zeitpunkt) -> Self {
        Self::laden(stand.kurse(), stand.nachrichten(), Some(jetzt))
    }

    pub(crate) fn erster_kurstag(&self, boerse: Boerse) -> Option<Datum> {
        self.kurse
            .iter()
            .filter(|(isin, _)| boerse_von(isin) == Some(boerse))
            .filter_map(|(_, tage)| tage.first_key_value().map(|(&tag, _)| tag))
            .min()
    }

    pub(crate) fn zum_stichtag(stand: &'a Stand, lauf_tag: Datum) -> Self {
        Self::laden(
            stand
                .kurse_bis_stichtag(lauf_tag)
                .unwrap_or_else(|| stand.kurse()),
            stand
                .nachrichten_bis_stichtag(lauf_tag)
                .unwrap_or_else(|| stand.nachrichten()),
            stand.wissensstichtag(lauf_tag),
        )
    }

    fn laden(
        belege: &'a [KursBeleg],
        nachrichten: &[NachrichtBeleg],
        bis: Option<Zeitpunkt>,
    ) -> Self {
        let mut index = Self::default();
        for beleg in belege {
            let kurs = beleg.kurs();
            let tag = kurs.handelstag();
            if bis.is_none_or(|zeit| beleg.abruf() <= zeit)
                && kurs.herkunft() == Herkunft::Quelle
                && kurs.preis().fein() > 0
                && boerse_von(beleg.isin()).is_some_and(|boerse| {
                    boerse.handelstag(tag) && boerse.schluss(tag) == Some(kurs.zeitpunkt())
                })
            {
                // Das Journal liefert aufsteigende Abrufzeiten, bei Gleichstand in Zeilenfolge.
                index
                    .kurse
                    .entry(beleg.isin())
                    .or_default()
                    .entry(tag)
                    .or_default()
                    .push(beleg);
            }
        }
        for beleg in nachrichten
            .iter()
            .filter(|b| bis.is_none_or(|zeit| b.abruf() <= zeit))
        {
            if let Some((isin, tag)) = split(beleg) {
                index
                    .splits
                    .entry(isin)
                    .or_default()
                    .push((tag, beleg.abruf()));
            }
        }
        index
    }

    /// Letzte Fassung aus der Quelle bis zur Abfragezeit. Bei gleicher Abrufzeit gilt die
    /// spätere Journalzeile, sofern sie bereits beim Wissensstichtag im Journal stand.
    pub(crate) fn schluss(
        &self,
        isin: &Isin,
        tag: Datum,
        wissen: Zeitpunkt,
    ) -> Option<&'a KursBeleg> {
        let belege = self.kurse.get(isin)?.get(&tag)?;
        let anzahl = belege.partition_point(|beleg| beleg.abruf() <= wissen);
        belege.get(anzahl.checked_sub(1)?).copied()
    }

    fn bewegung(&self, isin: &Isin, tage: [Datum; 3], wissen: Zeitpunkt) -> Option<Bewegung<'a>> {
        let [vorher, rueckgang, erholung] = tage;
        if self.splits.get(isin).is_some_and(|splits| {
            splits
                .iter()
                .any(|(tag, abruf)| vorher < *tag && *tag <= erholung && *abruf <= wissen)
        }) {
            return None;
        }
        let basis = self.schluss(isin, vorher, wissen)?.kurs().preis();
        let tief = self.schluss(isin, rueckgang, wissen)?.kurs().preis();
        let referenz = self.schluss(isin, erholung, wissen)?;
        let ende = referenz.kurs().preis();
        let rueckgang = tief.checked_sub(basis)?.in_bp_von(basis)?;
        let erholung = ende.checked_sub(tief)?.in_bp_von(tief)?;
        let sprung = Grenzen::standard().max_kurssprung_bp().unsigned_abs();
        (rueckgang.unsigned_abs() <= sprung && erholung.unsigned_abs() <= sprung).then_some(
            Bewegung {
                referenz,
                rueckgang,
                erholung,
            },
        )
    }

    /// `None` bezeichnet eine nicht vollständig belegte Marktgrundlage. Eine vollständige
    /// Marktgrundlage kann kein Signal ergeben, auch bei vollständig belegten Gruppen.
    pub(crate) fn signale(
        &self,
        boerse: Boerse,
        bezugstag: Datum,
        wissen: Zeitpunkt,
    ) -> Option<Befund> {
        if !boerse.handelstag(bezugstag) {
            return None;
        }
        let rueckgang = boerse.bezugstag(bezugstag)?;
        let tage = [boerse.bezugstag(rueckgang)?, rueckgang, bezugstag];
        let vergleich = VERGLEICH_ISIN
            .iter()
            .map(|isin| Isin::neu(isin).ok())
            .collect::<Option<Vec<_>>>()?;
        let marktwerte = vergleich
            .iter()
            .filter(|isin| boerse_von(isin) == Some(boerse))
            .map(|isin| self.bewegung(isin, tage, wissen))
            .collect::<Option<Vec<_>>>()?;
        let markt = Mittel::aus_bewegungen(&marktwerte)?;
        let marktlage = markt.rueckgang
            <= i128::from(MARKT_RUECKGANG_BP).checked_mul(markt.anzahl)?
            && markt.erholung > 0;
        let mut befund = Befund::default();
        for gruppe in gruppen(boerse) {
            let titel = gruppe
                .isins
                .iter()
                .map(|isin| {
                    let isin = Isin::neu(isin).ok()?;
                    (boerse_von(&isin) == Some(boerse)).then_some(())?;
                    self.bewegung(&isin, tage, wissen)
                })
                .collect::<Option<Vec<_>>>();
            let Some(titel) = titel.filter(|titel| titel.len() >= 2) else {
                befund.gruppen_fehlend = befund.gruppen_fehlend.saturating_add(1);
                continue;
            };
            befund.gruppen_bewertet = befund.gruppen_bewertet.saturating_add(1);
            if marktlage && let Some(signal) = bewertung(gruppe.name, &titel, &markt) {
                befund.signale.push(signal);
            }
        }
        Some(befund)
    }
}

#[derive(Debug)]
struct Bewegung<'a> {
    referenz: &'a KursBeleg,
    rueckgang: i64,
    erholung: i64,
}

#[derive(Debug)]
struct Mittel {
    rueckgang: i128,
    erholung: i128,
    anzahl: i128,
}

impl Mittel {
    fn aus_bewegungen(werte: &[Bewegung<'_>]) -> Option<Self> {
        if werte.is_empty() {
            return None;
        }
        let (rueckgang, erholung) = werte.iter().try_fold((0_i128, 0_i128), |summen, wert| {
            Some((
                summen.0.checked_add(i128::from(wert.rueckgang))?,
                summen.1.checked_add(i128::from(wert.erholung))?,
            ))
        })?;
        Some(Self {
            rueckgang,
            erholung,
            anzahl: i128::try_from(werte.len()).ok()?,
        })
    }

    fn uebertrifft(&self, markt: &Self) -> Option<bool> {
        Some(
            self.rueckgang.checked_mul(markt.anzahl)? > markt.rueckgang.checked_mul(self.anzahl)?
                && self.erholung.checked_mul(markt.anzahl)?
                    > markt.erholung.checked_mul(self.anzahl)?,
        )
    }
}

fn bewertung(name: &'static str, titel: &[Bewegung<'_>], markt: &Mittel) -> Option<Signal> {
    let gruppe = Mittel::aus_bewegungen(titel)?;
    if !gruppe.uebertrifft(markt)? {
        return None;
    }
    let mut breite_rueckgang = 0_usize;
    let mut breite_erholung = 0_usize;
    let mut spitze = None::<(i64, &KursBeleg)>;
    for wert in titel {
        let besser_im_rueckgang =
            i128::from(wert.rueckgang).checked_mul(markt.anzahl)? > markt.rueckgang;
        let besser_in_erholung =
            i128::from(wert.erholung).checked_mul(markt.anzahl)? > markt.erholung;
        breite_rueckgang = breite_rueckgang.saturating_add(usize::from(besser_im_rueckgang));
        breite_erholung = breite_erholung.saturating_add(usize::from(besser_in_erholung));
        if besser_im_rueckgang && besser_in_erholung {
            // Gleiche Marktbewegung je Titel: Die Summe der Tagesrenditen ordnet sie
            // genauso wie die Summe ihrer Überschussrenditen. Bei Gleichstand gilt die ISIN.
            let summe = wert.rueckgang.checked_add(wert.erholung)?;
            if spitze.is_none_or(|(bisher, beleg)| {
                summe > bisher || (summe == bisher && wert.referenz.isin() < beleg.isin())
            }) {
                spitze = Some((summe, wert.referenz));
            }
        }
    }
    let haelfte = titel.len().checked_div(2)?;
    if breite_rueckgang <= haelfte || breite_erholung <= haelfte {
        return None;
    }
    let (_, referenz) = spitze?;
    Some(Signal {
        gruppe: name,
        isin: referenz.isin().clone(),
        referenz: referenz.kurs().preis(),
        referenz_abruf: referenz.abruf(),
        tag: referenz.kurs().handelstag(),
        markt_rueckgang_bp: i64::try_from(markt.rueckgang.checked_div(markt.anzahl)?).ok()?,
        markt_erholung_bp: i64::try_from(markt.erholung.checked_div(markt.anzahl)?).ok()?,
        gruppe_rueckgang_bp: i64::try_from(gruppe.rueckgang.checked_div(gruppe.anzahl)?).ok()?,
        gruppe_erholung_bp: i64::try_from(gruppe.erholung.checked_div(gruppe.anzahl)?).ok()?,
        breite_rueckgang,
        breite_erholung,
        titel: titel.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_journal::Journal;
    use depot_kern::{Cent, Kurs};

    fn kursbeleg(
        isin: &Isin,
        boerse: Boerse,
        tag: Datum,
        abruf: Zeitpunkt,
        cent: i64,
    ) -> Result<KursBeleg, String> {
        let schluss = boerse.schluss(tag).ok_or("Handelsschluss fehlt")?;
        let kurs = Kurs::neu(Cent(cent), "EUR", tag, schluss, Herkunft::Quelle)
            .map_err(|e| format!("{e:?}"))?;
        KursBeleg::neu(
            depot_kern::liste::quelle(boerse),
            abruf,
            b"Test",
            isin.clone(),
            kurs,
        )
    }

    #[test]
    fn verlangt_den_vollstaendigen_markt_und_nutzt_den_belegten_wissensstand() -> Result<(), String>
    {
        let ordner =
            std::env::temp_dir().join(format!("depot-aufwaertsdruck-{}", std::process::id()));
        std::fs::create_dir(&ordner).map_err(|e| e.to_string())?;
        let probe = (|| -> Result<(), String> {
            let pfad = ordner.join("journal.txt");
            Journal::einrichten(&pfad).map_err(|e| format!("{e:?}"))?;
            let mut journal = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
            let erholung = Datum::neu(2026, 11, 23).ok_or("Datum fehlt")?;
            let abruf = Zeitpunkt::frankfurt(erholung, 23, 0, 0).ok_or("Abrufzeit fehlt")?;
            let boerse = Boerse::Xetra;
            let rueckgang = boerse.bezugstag(erholung).ok_or("Vortag fehlt")?;
            let vorher = boerse.bezugstag(rueckgang).ok_or("Basis fehlt")?;
            let allianz = Isin::neu("DE0008404005").map_err(|e| format!("{e:?}"))?;
            for isin in VERGLEICH_ISIN {
                let isin = Isin::neu(isin).map_err(|e| format!("{e:?}"))?;
                let sitzungen = match boerse_von(&isin) {
                    Some(Boerse::Xetra)
                        if XETRA
                            .first()
                            .is_some_and(|g| g.isins.contains(&isin.als_str())) =>
                    {
                        [10_000, 9_950, 10_150]
                    }
                    _ => [10_000, 9_800, 9_900],
                };
                let boerse = boerse_von(&isin).ok_or("Börse fehlt")?;
                for (tag, preis) in [vorher, rueckgang, erholung].into_iter().zip(sitzungen) {
                    let beleg = kursbeleg(&isin, boerse, tag, abruf, preis)?;
                    journal.belege_kurs(&beleg).map_err(|e| format!("{e:?}"))?;
                }
            }
            let frueher =
                Zeitpunkt::frankfurt(erholung, 22, 59, 59).ok_or("Frühere Abrufzeit fehlt")?;
            let historie = Kursindex::historie(journal.stand(), abruf);
            assert_eq!(historie.signale(boerse, erholung, frueher), None);
            let us_befund = historie
                .signale(Boerse::Nyse, erholung, abruf)
                .ok_or("US-Vergleich fehlt")?;
            assert_eq!(
                (us_befund.gruppen_bewertet, us_befund.gruppen_fehlend),
                (1, 1)
            );
            assert!(us_befund.signale.is_empty());

            journal
                .belege_wissensstichtag(erholung, abruf)
                .map_err(|e| format!("{e:?}"))?;
            let revision = kursbeleg(&allianz, boerse, erholung, abruf, 9_900)?;
            journal
                .belege_kurs(&revision)
                .map_err(|e| format!("{e:?}"))?;
            let fest = Kursindex::zum_stichtag(journal.stand(), erholung);
            let befund = fest
                .signale(boerse, erholung, abruf)
                .ok_or("Xetra-Vergleich fehlt")?;
            assert_eq!((befund.gruppen_bewertet, befund.gruppen_fehlend), (3, 0));
            assert_eq!(befund.signale.len(), 1);
            let signal = befund.signale.first().ok_or("Signal fehlt")?;
            assert_eq!(signal.isin, allianz);
            assert_eq!((signal.breite_rueckgang, signal.breite_erholung), (2, 2));
            assert_eq!(
                signal.referenz,
                Preis::aus_cent(Cent(10_150)).ok_or("Referenz fehlt")?
            );
            assert_eq!(signal.referenz_abruf, abruf);

            let frei = Kursindex::historie(journal.stand(), abruf);
            let befund = frei
                .signale(boerse, erholung, abruf)
                .ok_or("Revidierter Xetra-Vergleich fehlt")?;
            // Die Revision reduziert die Breite der Erholung auf einen von zwei Titeln.
            assert!(befund.signale.is_empty());
            Ok(())
        })();
        std::fs::remove_dir_all(&ordner).map_err(|e| e.to_string())?;
        probe
    }
}
