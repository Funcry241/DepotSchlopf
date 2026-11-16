//! Belegte Splits im Papierdepot, mit nachholbarem Kontoabgleich.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use depot_journal::{
    Journal, Stand,
    kapitalmassnahme::{Kapitalmassnahme, tiingo_split},
    splitverdacht,
};
use depot_kern::{Datum, Isin, Kurs, Kurse, Topf, Zeitpunkt};
use depot_regelwerk::Richtung;

use crate::{papier::Broker, papierdatei::Papierdatei};

/// Bestätigung in der Einrichtung. Der geprüfte Beleg bestimmt ISIN, Tag und Splitfaktor.
pub fn xetra_bestaetigen(daten: &Path, kennung: &str, jetzt: Zeitpunkt) -> Result<(), String> {
    if depot_journal::kurssperre::kurssperre(daten).is_some()
        || daten
            .join("update_offen.txt")
            .try_exists()
            .map_err(|e| e.to_string())?
    {
        return Err("Split-Umbuchung während Kurssperre oder Update gesperrt".into());
    }
    let mut journal = Journal::oeffnen(&daten.join("journal.txt")).map_err(|e| format!("{e:?}"))?;
    if journal.stand().toepfe().is_empty() {
        return Err("Kein Papierdepot für die Split-Umbuchung vorhanden".into());
    }
    let mut konto = Papierdatei::oeffnen(&daten.join("papierkonto.txt"), true, &[])?;
    buche(&mut journal, &mut konto, kennung, jetzt)
}

fn buche(
    journal: &mut Journal,
    konto: &mut dyn Broker,
    kennung: &str,
    jetzt: Zeitpunkt,
) -> Result<(), String> {
    konto.kapitalmassnahmen(journal.stand().kapitalmassnahmen())?;
    if !journal
        .gleiche_ab(konto)
        .map_err(|e| format!("{e:?}"))?
        .is_empty()
    {
        return Err("Papierkonto und Journal stimmen vor der Split-Umbuchung nicht überein".into());
    }
    let (isin, _) = splitverdacht::aktiv(journal.stand(), None)
        .into_iter()
        .find(|(_, b)| splitverdacht::kennung(b) == kennung)
        .ok_or("Der geprüfte Splitverdacht ist nicht mehr offen")?;
    let toepfe = journal
        .stand()
        .toepfe()
        .values()
        .filter(|t| t.stueck(&isin) > 0);
    let massnahmen = toepfe
        .filter(|t| {
            !journal.stand().kapitalmassnahmen().iter().any(|m| {
                m.massnahme.quelle == kennung
                    && m.massnahme.isin == isin
                    && m.massnahme.topf == t.id()
            })
        })
        .map(|t| {
            Kapitalmassnahme::xetra(journal.stand(), t.id(), kennung)
                .ok_or("Der angezeigte Splitfaktor ist nicht lesbar")
        })
        .collect::<Result<Vec<_>, _>>()?;
    journal
        .buche_kapitalmassnahmen(&massnahmen)
        .map_err(|e| format!("{e:?}"))?;
    konto.kapitalmassnahmen(journal.stand().kapitalmassnahmen())?;
    if !splitverdacht::schliesse_umbuchung(journal, kennung, jetzt, konto)
        .map_err(|e| format!("{e:?}"))?
    {
        return Err(
            "Split konnte nicht abgeschlossen werden. Die Wertpapier-Sperre bleibt bestehen."
                .into(),
        );
    }
    Ok(())
}

/// Setzt eine nach der Journalbuchung unterbrochene Split-Umbuchung vor dem Handel fort.
pub(crate) fn wiederanlauf(
    journal: &mut Journal,
    konto: &mut dyn Broker,
    jetzt: Zeitpunkt,
) -> Result<(), String> {
    let offen: Vec<_> = splitverdacht::aktiv(journal.stand(), None)
        .values()
        .map(|b| splitverdacht::kennung(b))
        .filter(|kennung| {
            journal
                .stand()
                .kapitalmassnahmen()
                .iter()
                .any(|m| &m.massnahme.quelle == kennung)
        })
        .collect();
    for kennung in offen {
        buche(journal, konto, &kennung, jetzt)?;
    }
    konto.kapitalmassnahmen(journal.stand().kapitalmassnahmen())
}

fn vor_split_gehalten(stand: &Stand, topf: &Topf, isin: &Isin, tag: Datum) -> bool {
    let bestand = i128::from(topf.stueck(isin));
    // Buchungen ohne verlässliche Zeit oder spätere Splits lassen sich nicht sicher
    // auf die damalige Stückbasis zurückrechnen. Die Journalprüfung sperrt solche Fälle.
    if stand.kapitalmassnahmen().iter().any(|b| {
        let m = &b.massnahme;
        m.topf == topf.id() && &m.isin == isin && m.wirksam > tag
    }) {
        return true;
    }
    let bewegung = stand
        .ausfuehrungsfolge()
        .filter(|(a, _)| a.topf == topf.id() && &a.isin == isin)
        .try_fold(0_i128, |summe, (a, f)| {
            let tag_ausfuehrung = f.kurszeit.and_then(Zeitpunkt::frankfurt_tag)?;
            if tag_ausfuehrung < tag {
                return Some(summe);
            }
            let stueck = i128::from(f.stueck);
            match a.richtung {
                Richtung::Kauf => summe.checked_add(stueck),
                Richtung::Verkauf => summe.checked_sub(stueck),
            }
        });
    bewegung != Some(bestand)
}

/// Übernimmt bestätigte US-Splits vor der Handelsbewertung in Journal und Papierkonto.
pub(crate) fn tiingo_umbuchen(
    journal: &mut Journal,
    konto: &mut dyn Broker,
    kurse: &Kurse,
    wissen_bis: Zeitpunkt,
) -> Result<(), String> {
    let stand = journal.stand();
    let mut offen: BTreeMap<_, Kapitalmassnahme> = BTreeMap::new();
    let Some(heute) = wissen_bis.frankfurt_tag() else {
        return Err("Datum für Tiingo-Splits fehlt".into());
    };
    for b in stand
        .nachrichten()
        .iter()
        .filter(|b| b.abruf() <= wissen_bis)
    {
        let Some((isin, tag)) = tiingo_split(b).filter(|(_, tag)| *tag <= heute) else {
            continue;
        };
        for topf in stand.toepfe().values().filter(|t| t.stueck(&isin) > 0) {
            let gebucht = stand.kapitalmassnahmen().iter().find(|b| {
                let m = &b.massnahme;
                m.topf == topf.id() && m.isin == isin && m.wirksam == tag
            });
            if gebucht.is_none() && !vor_split_gehalten(stand, topf, &isin, tag) {
                continue;
            }
            let massnahme = Kapitalmassnahme::tiingo(stand, topf.id(), b).ok_or_else(|| {
                format!("{isin}: Tiingo-Split am {tag} ohne übereinstimmenden Quellenbeleg")
            })?;
            let schluessel = (tag, isin.clone(), topf.id());
            if let Some(vorher) = gebucht
                .map(|b| &b.massnahme)
                .or_else(|| offen.get(&schluessel))
            {
                if vorher.faktor != massnahme.faktor {
                    return Err(format!(
                        "{isin}: widersprüchliche Tiingo-Splitfaktoren am {tag}"
                    ));
                }
                continue;
            }
            if kurse.eintrag(&isin).is_none_or(|k| k.handelstag() < tag) {
                return Err(format!(
                    "{isin}: kein aktueller Schlusskurs für Tiingo-Split am {tag}"
                ));
            }
            offen.insert(schluessel, massnahme);
        }
    }
    if offen.is_empty() {
        return Ok(());
    }
    let abweichungen = journal.gleiche_ab(konto).map_err(|e| format!("{e:?}"))?;
    if !abweichungen.is_empty() {
        return Err(format!(
            "Papierkonto vor Tiingo-Umbuchung nicht abgeglichen: {abweichungen:?}"
        ));
    }
    journal
        .buche_kapitalmassnahmen(&offen.into_values().collect::<Vec<_>>())
        .map_err(|e| format!("{e:?}"))?;
    konto.kapitalmassnahmen(journal.stand().kapitalmassnahmen())
}

/// Ein alter Schlusskurs darf nach einer Umbuchung nicht als aktueller Kurs gelten.
pub(crate) fn pruefe_kursbasis(stand: &Stand, kurse: &Kurse) -> Result<(), String> {
    for b in stand.kapitalmassnahmen() {
        let m = &b.massnahme;
        if kurse
            .eintrag(&m.isin)
            .is_some_and(|k| k.handelstag() < m.wirksam)
        {
            return Err(format!(
                "{}: Schlusskurs liegt vor der Split-Umbuchung",
                m.isin
            ));
        }
    }
    Ok(())
}

/// Alte Vergleichsschlüsse werden auf die heutige Stückbasis umgerechnet.
/// Rohkurse und die Kurse für die Auftragsausführung bleiben unverändert.
pub(crate) fn vergleichskurse(stand: &Stand, kurse: &Kurse) -> Result<Kurse, String> {
    if stand.kapitalmassnahmen().is_empty() {
        return Ok(kurse.clone());
    }
    let mut eintraege = Vec::new();
    for (isin, kurs) in kurse.alle() {
        let mut tage = BTreeSet::new();
        let mut preis = kurs.preis();
        for b in stand.kapitalmassnahmen() {
            let m = &b.massnahme;
            if m.isin == *isin && kurs.handelstag() < m.wirksam && tage.insert(m.wirksam) {
                preis = m
                    .faktor
                    .vergleichskurs(preis)
                    .ok_or_else(|| format!("Split-Vergleichskurs für {isin} unberechenbar"))?;
            }
        }
        let kurs = Kurs::exakt(
            preis,
            "EUR",
            kurs.handelstag(),
            kurs.zeitpunkt(),
            kurs.herkunft(),
        )
        .map_err(|e| format!("Split-Vergleichskurs: {e:?}"))?;
        let abruf = kurse
            .abruf(isin)
            .ok_or("Abruf des Vergleichskurses fehlt")?;
        eintraege.push((isin.clone(), kurs, abruf));
    }
    Kurse::mit_abrufen(kurse.stand(), eintraege)
        .map_err(|e| format!("Split-Vergleichskurse: {e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use depot_journal::{KursBeleg, NachrichtBeleg};
    use depot_kern::{Boerse, Cent, Herkunft, TopfId};

    struct Probe {
        ordner: PathBuf,
        isin: Isin,
        tag: Datum,
        jetzt: Zeitpunkt,
        kurse: Kurse,
    }

    impl Probe {
        fn neu() -> Result<(Self, Journal, Papierdatei), String> {
            static NR: AtomicU32 = AtomicU32::new(0);
            let nr = NR.fetch_add(1, Ordering::Relaxed);
            let ordner =
                std::env::temp_dir().join(format!("depot-tiingo-{}-{nr}", std::process::id()));
            fs::create_dir(&ordner).map_err(|e| e.to_string())?;
            let isin = Isin::neu("US5949181045").map_err(|e| format!("{e:?}"))?;
            let tag = Datum::neu(2026, 11, 9).ok_or("Splittag")?;
            let schluss = Boerse::Nyse.schluss(tag).ok_or("NYSE-Schluss")?;
            let jetzt = Zeitpunkt::frankfurt(tag, 22, 30, 0).ok_or("Abrufzeit")?;
            let kurs = Kurs::neu(Cent(10_000), "EUR", tag, schluss, Herkunft::Quelle)
                .map_err(|e| format!("{e:?}"))?;
            let kurse = Kurse::neu(jetzt, vec![(isin.clone(), kurs.clone())])
                .map_err(|e| format!("{e:?}"))?;
            let p = Self {
                ordner,
                isin,
                tag,
                jetzt,
                kurse,
            };
            let pfad = p.ordner.join("journal.txt");
            Journal::einrichten(&pfad).map_err(|e| format!("{e:?}"))?;
            let mut j = p.journal()?;
            let mut a = Topf::neu(TopfId::A, Cent(500_000)).map_err(|e| format!("{e:?}"))?;
            let mut b = Topf::neu(TopfId::B, Cent(500_000)).map_err(|e| format!("{e:?}"))?;
            a.setze_bestand(p.isin.clone(), 10);
            b.setze_bestand(p.isin.clone(), 5);
            for t in [&a, &b] {
                j.setze_topf(t).map_err(|e| format!("{e:?}"))?;
            }
            let konto = Papierdatei::oeffnen(&p.ordner.join("papierkonto.txt"), false, &[a, b])?;
            let freitag = Datum::neu(2026, 11, 6).ok_or("Vortag")?;
            j.schliesse_ab(freitag, Cent(1_600_000), &"a".repeat(64))
                .map_err(|e| format!("{e:?}"))?;
            j.belege_kurs(&KursBeleg::neu(
                depot_kern::liste::QUELLE_US,
                p.jetzt,
                b"Tiingo-Kursantwort",
                p.isin.clone(),
                kurs,
            )?)
            .map_err(|e| format!("{e:?}"))?;
            Ok((p, j, konto))
        }

        fn journal(&self) -> Result<Journal, String> {
            Journal::oeffnen(&self.ordner.join("journal.txt")).map_err(|e| format!("{e:?}"))
        }

        fn meldung(
            &self,
            j: &mut Journal,
            faktor: &str,
            roh: &[u8],
        ) -> Result<NachrichtBeleg, String> {
            let zeit = Boerse::Nyse.schluss(self.tag).ok_or("Schlusszeit")?;
            let text = format!(
                "Tiingo: Wert ({}) · Split-Faktor {faktor} am {}",
                self.isin, self.tag
            );
            let b = NachrichtBeleg::neu(
                depot_kern::tiingo::QUELLE_KAPITAL,
                self.jetzt,
                roh,
                zeit,
                &text,
            )?;
            j.belege_nachricht(&b).map_err(|e| format!("{e:?}"))?;
            Ok(b)
        }
    }

    impl Drop for Probe {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.ordner);
        }
    }

    #[test]
    fn tiingo_split_holt_papierkonto_nach_und_bucht_jeden_topf_einmal() -> Result<(), String> {
        let (p, mut j, alt) = Probe::neu()?;
        let beleg = p.meldung(&mut j, "4", b"Tiingo-Kursantwort")?;
        let erster = Kapitalmassnahme::tiingo(j.stand(), TopfId::A, &beleg).ok_or("Splitbeleg")?;
        j.buche_kapitalmassnahmen(&[erster])
            .map_err(|e| format!("{e:?}"))?;
        drop(j);
        drop(alt);
        let mut j = p.journal()?;
        let mut konto = Papierdatei::oeffnen(&p.ordner.join("papierkonto.txt"), true, &[])?;
        wiederanlauf(&mut j, &mut konto, p.jetzt)?;
        tiingo_umbuchen(&mut j, &mut konto, &p.kurse, p.jetzt)?;
        tiingo_umbuchen(&mut j, &mut konto, &p.kurse, p.jetzt)?;
        assert_eq!(j.stand().kapitalmassnahmen().len(), 2);
        let stueck = |id| j.stand().toepfe().get(&id).map(|t| t.stueck(&p.isin));
        assert_eq!(stueck(TopfId::A), Some(40));
        assert_eq!(stueck(TopfId::B), Some(20));
        let split = j.stand().kapitalmassnahmen().first().ok_or("Split fehlt")?;
        assert_eq!(split.vorher_gesamt, 15);
        assert_eq!(split.danach_gesamt, 45);
        let alter_kurs = depot_kern::Preis::aus_cent(Cent(40_000)).ok_or("Alter Preis")?;
        assert_eq!(
            split.massnahme.faktor.vergleichskurs(alter_kurs),
            p.kurse.kurs(&p.isin)
        );
        assert_eq!(j.stand().historischer_hoechststand(), Some(Cent(1_600_000)));
        assert!(
            j.gleiche_ab(&konto)
                .map_err(|e| format!("{e:?}"))?
                .is_empty()
        );
        drop(j);
        assert_eq!(p.journal()?.stand().kapitalmassnahmen().len(), 2);
        Ok(())
    }

    #[test]
    fn tiingo_split_bucht_ohne_passenden_oder_bei_widerspruechlichem_beleg_nichts()
    -> Result<(), String> {
        for antwort in [b"andere Antwort".as_slice(), b"Tiingo-Kursantwort"] {
            let (p, mut j, mut konto) = Probe::neu()?;
            p.meldung(&mut j, "4", b"Tiingo-Kursantwort")?;
            p.meldung(&mut j, "2", antwort)?;
            assert!(tiingo_umbuchen(&mut j, &mut konto, &p.kurse, p.jetzt).is_err());
            assert!(j.stand().kapitalmassnahmen().is_empty());
            assert!(
                j.gleiche_ab(&konto)
                    .map_err(|e| format!("{e:?}"))?
                    .is_empty()
            );
        }
        Ok(())
    }
}
