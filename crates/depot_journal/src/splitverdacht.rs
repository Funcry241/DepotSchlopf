//! Dauerhafte Xetra-Splitverdachte und geprüfte Entwarnungen im bestehenden Journalformat.
use std::collections::{BTreeMap, BTreeSet};

use depot_kern::{Herkunft, Isin, Kurse, Zeitpunkt, abdruck::sha256_hex};

use crate::{Journal, JournalFehler, NachrichtBeleg, Stand};

pub const ENTWARNUNG: &str = "xetra-split-entwarnung";

/// Der Bezug gilt genau für diesen Verdacht samt seiner Erfassung.
pub fn kennung(beleg: &NachrichtBeleg) -> String {
    sha256_hex(format!("{};{};{}", beleg.zeit(), beleg.abruf(), beleg.text()).as_bytes())
}

fn isin(beleg: &NachrichtBeleg) -> Option<Isin> {
    let (links, _) = beleg.text().rsplit_once(") am ")?;
    let (_, isin) = links.rsplit_once('(')?;
    Isin::neu(isin).ok()
}

/// Ungeklärte Verdachte pro Wert. Bei mehreren Belegen bleibt der erste der Bewertungsanker.
pub fn aktiv(stand: &Stand, bis: Option<Zeitpunkt>) -> BTreeMap<Isin, &NachrichtBeleg> {
    let mut offen: BTreeMap<Isin, Vec<&NachrichtBeleg>> = BTreeMap::new();
    for b in stand
        .nachrichten()
        .iter()
        .filter(|b| bis.is_none_or(|bis| b.abruf() <= bis))
    {
        match b.quelle() {
            depot_kern::quelle::QUELLE_XETRA_SPLITVERDACHT => {
                if let Some(isin) = isin(b) {
                    offen.entry(isin).or_default().push(b);
                }
            }
            ENTWARNUNG => offen.retain(|_, verdachte| {
                verdachte.retain(|verdacht| kennung(verdacht) != b.text());
                !verdachte.is_empty()
            }),
            _ => {}
        }
    }
    offen
        .into_iter()
        .filter_map(|(isin, belege)| Some((isin, *belege.first()?)))
        .collect()
}

/// Entwarnt nur den vor der Prüfung angezeigten Verdacht.
pub fn entwarne(
    journal: &mut Journal,
    gesehen: &str,
    jetzt: Zeitpunkt,
) -> Result<bool, JournalFehler> {
    if journal
        .stand()
        .kapitalmassnahmen()
        .iter()
        .any(|b| b.massnahme.quelle == gesehen)
    {
        return Ok(false);
    }
    schliesse(journal, gesehen, jetzt)
}

/// Schließt den Verdacht, nachdem der Papierbestand mit dem Journal übereinstimmt.
pub fn schliesse_umbuchung(
    journal: &mut Journal,
    gesehen: &str,
    jetzt: Zeitpunkt,
    konto: &dyn crate::Konto,
) -> Result<bool, JournalFehler> {
    let Some((isin, _)) = aktiv(journal.stand(), None)
        .into_iter()
        .find(|(_, verdacht)| kennung(verdacht) == gesehen)
    else {
        return Ok(false);
    };
    let mut betroffen = false;
    for topf in journal
        .stand()
        .toepfe()
        .values()
        .filter(|t| t.stueck(&isin) != 0)
    {
        betroffen = true;
        if !journal.stand().kapitalmassnahmen().iter().any(|b| {
            b.massnahme.topf == topf.id()
                && b.massnahme.isin == isin
                && b.massnahme.quelle == gesehen
        }) {
            return Ok(false);
        }
    }
    if !betroffen || !journal.gleiche_ab(konto)?.is_empty() {
        return Ok(false);
    }
    schliesse(journal, gesehen, jetzt)
}

fn schliesse(
    journal: &mut Journal,
    gesehen: &str,
    jetzt: Zeitpunkt,
) -> Result<bool, JournalFehler> {
    if !aktiv(journal.stand(), None)
        .values()
        .any(|b| kennung(b) == gesehen)
    {
        return Ok(false);
    }
    let beleg = NachrichtBeleg::neu(ENTWARNUNG, jetzt, gesehen.as_bytes(), jetzt, gesehen)
        .map_err(JournalFehler::Abgelehnt)?;
    journal.belege_nachricht(&beleg)
}

/// Ersetzt je gesperrter Position den Kurs durch den vor dem Verdacht belegten Schluss.
/// Fehlt der Beleg, bleibt der Kurs offen und der Bestand unbewertbar.
pub fn ersetze_kurse(
    stand: &Stand,
    kurse: &Kurse,
    offen: &BTreeMap<Isin, &NachrichtBeleg>,
    positionen: &BTreeSet<Isin>,
) -> Result<Kurse, depot_kern::KursFehler> {
    if offen.keys().all(|i| !positionen.contains(i)) {
        return Ok(kurse.clone());
    }
    let mut eintraege: Vec<_> = kurse
        .alle()
        .filter(|(i, _)| !positionen.contains(*i) || !offen.contains_key(*i))
        .filter_map(|(i, k)| Some((i.clone(), k.clone(), kurse.abruf(i)?)))
        .collect();
    for (isin, verdacht) in offen.iter().filter(|(i, _)| positionen.contains(*i)) {
        let Some(tag) = verdacht.zeit().datum() else {
            continue;
        };
        if let Some(b) = stand
            .kurse()
            .iter()
            .filter(|b| {
                b.isin() == isin
                    && b.kurs().handelstag() < tag
                    && b.abruf() <= verdacht.abruf()
                    && b.abruf() <= kurse.stand()
                    && b.kurs().herkunft() == Herkunft::Quelle
            })
            .max_by_key(|b| (b.kurs().handelstag(), b.abruf()))
        {
            eintraege.push((isin.clone(), b.kurs().clone(), b.abruf()));
        }
    }
    Kurse::mit_abrufen(kurse.stand(), eintraege)
}
