//! Geprüfter Xetra-Split im Papierdepot, mit nachholbarem Kontoabgleich.

use std::collections::BTreeSet;
use std::path::Path;

use depot_journal::{Journal, Stand, kapitalmassnahme::Kapitalmassnahme, splitverdacht};
use depot_kern::{Kurs, Kurse, Zeitpunkt};

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
