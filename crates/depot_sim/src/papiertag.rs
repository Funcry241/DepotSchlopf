//! Der Papiertag: ein echter Handelstag mit Papiergeld.
//!
//! Gleicher Ablauf wie ein Tag der Simulation (`lauf`), aber mit den Schlusskursen, die der
//! Sammler von der freigegebenen Quelle geholt und im Journal belegt hat, und mit den belegten
//! Terminen als Ereignissen. Gehandelt wird nur auf dem Papierkonto; einen anderen Broker gibt
//! es nicht. Echtes Geld erreicht dieser Ablauf nie.
//!
//! **Kurse:** Nur Belege mit Herkunft `Quelle` zaehlen. Der Handelstag ist der juengste belegte
//! Tag; er muss der heutige sein, sonst Halt: Ein Tag wird nie mit Kursen eines anderen Tages
//! entschieden. Je Wert gilt der zuletzt belegte Kurs des Handelstags; der Vergleichskurs ist
//! der zuletzt belegte Kurs des juengsten Tages davor. Gepruefte wie im Sammler
//! (`pruefe_kurse`): Faellt die Pruefung durch, faellt der Zyklus aus, und niemand wird gefragt.
//!
//! **Einmal je Tag:** Vor allem anderen gleicht der Kontowaechter Journal und Papierkonto ab;
//! jede Abweichung ist Halt, auch fuer einen schon abgeschlossenen Tag (K23-02). Ist der
//! Handelstag danach schon abgeschlossen, geschieht nichts. Ein angefangener Tag haelt an wie in
//! der Simulation; niemand wird neu gefragt.
//!
//! **Ausfuehrung:** Das Papierkonto fuehrt zum Schlusskurs des Entscheidungstags aus (feine
//! Kurse zur sicheren Seite gerundet). Das ist eine Vereinfachung: Wirklich ginge ein Auftrag
//! erst nach dem Schluss an den Markt. Sie steht im Journal jedes Tages.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use depot_journal::{Journal, KursBeleg, NachrichtBeleg, Stand};
use depot_kern::{
    Cent, Datum, Grenzen, Herkunft, Isin, Kurs, Kurse, Rundung, Topf, Universum, Zeitpunkt,
};
use depot_regelwerk::pruefe_kurse;
use sha2::{Digest, Sha256};

use crate::lauf::{
    Beteiligte, LaufFehler, Zyklus, depotwert, ein_tag, halt, journal, pruefe_stoerung, rechnung,
};
use crate::papier::Broker;
use crate::szenario::{Berater, Tag};

const FASSUNG: &str = "depot_sim-papiertag-v1";
const PAPIERTAG: &str = "Papiertag";
/// Termine so viele Tage voraus gehen als Ereignisse an die Berater.
const TERMINE_VORAUS_TAGE: i64 = 30;
const SEKUNDEN_JE_TAG: i64 = 86_400;
/// Hoechstens so viele belegte Nachrichten gehen in die Lage, die neuesten. So bleibt die Frage
/// an die Berater unter ihrem Deckel.
pub const MAX_NACHRICHTEN_LAGE: usize = 30;

/// Wie der Papiertag ausging.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ausgang {
    /// Der Tag ist abgeschlossen.
    Abgeschlossen { datum: Datum, hoechststand: Cent },
    /// Der Handelstag war schon abgeschlossen; nichts geschah.
    SchonAbgeschlossen(Datum),
}

/// Die Kurse fuer einen Papiertag, aus den Belegen des Journals.
#[derive(Clone, Debug)]
pub struct Tageskurse {
    pub handelstag: Datum,
    pub heute: Kurse,
    pub vortag: Kurse,
}

fn tagesbeginn(d: Datum) -> Option<i64> {
    Zeitpunkt::neu(d, 0, 0, 0).map(Zeitpunkt::sekunden)
}

/// Nummer des Tages seit 1970: steigt von Tag zu Tag, wie der Kostenschutz es verlangt.
fn tagnummer(d: Datum) -> Option<u32> {
    let s = tagesbeginn(d)?;
    u32::try_from(s.checked_div(SEKUNDEN_JE_TAG)?).ok()
}

/// Baut aus den Kursbelegen des Journals die Kurse des juengsten Handelstags und seines
/// Vortags, nur fuer Werte des Universums und nur mit Herkunft `Quelle`.
///
/// # Errors
/// `Halt`, wenn kein Kurs belegt ist oder die Kurse sich nicht zusammenfassen lassen.
pub fn tageskurse(stand: &Stand, universum: &Universum) -> Result<Tageskurse, LaufFehler> {
    let isins: Vec<Isin> = universum.isins().into_iter().collect();
    let belege: Vec<&KursBeleg> = stand
        .kurse()
        .iter()
        .filter(|b| b.kurs().herkunft() == Herkunft::Quelle && isins.contains(b.isin()))
        .collect();
    let handelstag = belege
        .iter()
        .map(|b| b.kurs().handelstag())
        .max()
        .ok_or_else(|| halt("Keine Schlusskurse im Journal: zuerst run.cmd kurse"))?;
    // Je Wert der zuletzt belegte Kurs; die Belege stehen in der Reihenfolge ihres Abrufs.
    let mut heute: BTreeMap<&Isin, &KursBeleg> = BTreeMap::new();
    let mut vortag: BTreeMap<&Isin, &KursBeleg> = BTreeMap::new();
    for b in &belege {
        let tag = b.kurs().handelstag();
        if tag == handelstag {
            heute.insert(b.isin(), b);
        } else if vortag
            .get(b.isin())
            .is_none_or(|alt| alt.kurs().handelstag() <= tag)
        {
            vortag.insert(b.isin(), b);
        }
    }
    let kurse = |m: &BTreeMap<&Isin, &KursBeleg>| -> Result<Kurse, LaufFehler> {
        let stand = m
            .values()
            .map(|b| b.abruf())
            .max()
            .ok_or_else(|| halt("Kein Kurs fuer den Vergleich belegt"))?;
        let eintraege: Vec<(Isin, Kurs)> = m
            .iter()
            .map(|(i, b)| ((*i).clone(), b.kurs().clone()))
            .collect();
        Kurse::neu(stand, eintraege).map_err(|e| halt(format!("Kurse: {e:?}")))
    };
    Ok(Tageskurse {
        handelstag,
        heute: kurse(&heute)?,
        vortag: kurse(&vortag)?,
    })
}

fn datum_text(d: Datum) -> String {
    format!("{:02}.{:02}.{}", d.tag(), d.monat(), d.jahr())
}

/// Die belegten Termine vom Handelstag bis `TERMINE_VORAUS_TAGE` voraus, nach Datum; danach die
/// belegten Nachrichten (`nachrichten`).
fn ereignisse(stand: &Stand, handelstag: Datum) -> Vec<String> {
    let Some(von) = tagesbeginn(handelstag) else {
        return Vec::new();
    };
    let bis = TERMINE_VORAUS_TAGE
        .checked_mul(SEKUNDEN_JE_TAG)
        .and_then(|d| von.checked_add(d));
    let mut aus: Vec<(Datum, String)> = stand
        .aktuelle_termine()
        .values()
        .filter_map(|b| {
            let t = b.termin();
            let tag = t.zeit().tag()?;
            let s = tagesbeginn(tag)?;
            (s >= von && bis.is_some_and(|bis| s <= bis)).then(|| {
                (
                    tag,
                    format!(
                        "Termin {}: {} ({})",
                        datum_text(tag),
                        t.titel(),
                        t.verlaesslichkeit().name()
                    ),
                )
            })
        })
        .collect();
    aus.sort();
    let mut aus: Vec<String> = aus.into_iter().map(|(_, t)| t).collect();
    aus.extend(nachrichten(stand, handelstag));
    aus
}

/// Die belegten Nachrichten (R40), veroeffentlicht vom Beginn des Vortags bis zum Ende des
/// Handelstags (UTC): hoechstens `MAX_NACHRICHTEN_LAGE`, die neueste zuerst. Wird es dem
/// Berater zu viel, faellt so die aelteste weg.
fn nachrichten(stand: &Stand, handelstag: Datum) -> Vec<String> {
    let Some(beginn) = tagesbeginn(handelstag) else {
        return Vec::new();
    };
    let von = beginn.checked_sub(SEKUNDEN_JE_TAG);
    let bis = beginn.checked_add(SEKUNDEN_JE_TAG);
    let mut aus: Vec<&NachrichtBeleg> = stand
        .nachrichten()
        .iter()
        .filter(|n| {
            let s = n.zeit().sekunden();
            von.is_some_and(|von| s >= von) && bis.is_some_and(|bis| s < bis)
        })
        .collect();
    aus.sort_by(|a, b| b.zeit().cmp(&a.zeit()).then_with(|| a.text().cmp(b.text())));
    aus.truncate(MAX_NACHRICHTEN_LAGE);
    aus.into_iter()
        .filter_map(|n| {
            let (h, m, _) = n.zeit().uhrzeit()?;
            let d = n.zeit().datum()?;
            Some(format!(
                "Nachricht {} {h:02}:{m:02} UTC ({}): {}",
                datum_text(d),
                n.quelle(),
                n.text()
            ))
        })
        .collect()
}

fn feld(h: &mut Sha256, text: &str) {
    h.update(text.len().to_string().as_bytes());
    h.update(b":");
    h.update(text.as_bytes());
    h.update(b";");
}

/// Abdruck des Tages: haengt am Abdruck des letzten Abschlusses und an allen Kursen und
/// Ereignissen, mit denen entschieden wird.
fn abdruck(vorher: Option<&str>, tag: &Tag, vortag: &Kurse) -> String {
    let mut h = Sha256::new();
    feld(&mut h, FASSUNG);
    feld(&mut h, vorher.unwrap_or("anfang"));
    feld(&mut h, &datum_text(tag.datum));
    for k in [&tag.kurse, vortag] {
        feld(&mut h, &k.stand().sekunden().to_string());
        for (isin, kurs) in k.alle() {
            feld(&mut h, isin.als_str());
            feld(&mut h, &kurs.preis().cent_text());
            feld(&mut h, &kurs.zeitpunkt().sekunden().to_string());
        }
        feld(&mut h, "ende-kurse");
    }
    for e in &tag.ereignisse {
        feld(&mut h, e);
    }
    h.finalize()
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Ein Papiertag. `heute` ist das Datum der Uhr; `start` sind die Toepfe, mit denen ein neues
/// Papierdepot beginnt. `konto` ist das Papierkonto, das unabhaengig vom Journal weiterlebt.
///
/// # Errors
/// `Halt` ohne Kurse von heute, bei einem angefangenen Tag, bei jeder Abweichung zum Konto,
/// bei einem Befund des Waechters und bei jedem Fehler des Journals.
pub fn papiertag(
    j: &mut Journal,
    heute: Datum,
    universum: &Universum,
    start: &[Topf],
    dachs: &mut dyn Berater,
    maus: &mut dyn Berater,
    konto: &mut dyn Broker,
) -> Result<Ausgang, LaufFehler> {
    let k = tageskurse(j.stand(), universum)?;
    if k.handelstag != heute {
        return Err(halt(format!(
            "Die neuesten Schlusskurse sind vom {}, heute ist der {}: Ein Tag wird nur mit \
             seinen eigenen Kursen entschieden. Zuerst run.cmd kurse nach 17:35 Uhr",
            datum_text(k.handelstag),
            datum_text(heute)
        )));
    }
    let neu = j.stand().toepfe().is_empty();
    if !neu {
        // Wiederanlauf: zuerst der Abgleich, auch wenn der Tag schon abgeschlossen ist
        // (K23-02). Er belegt, was aus offenen Auftraegen geworden ist, und sendet nichts.
        pruefe_stoerung(&*konto)?;
        let abweichungen = j.gleiche_ab(&*konto).map_err(journal)?;
        if !abweichungen.is_empty() {
            return Err(halt(format!(
                "Kontowaechter: {abweichungen:?}. Halt bis zur Pruefung; niemand wird gefragt, \
                 nichts gesendet"
            )));
        }
    }
    let letzter = j.stand().abschluss().cloned();
    if let Some(a) = &letzter {
        if a.datum == k.handelstag {
            return Ok(Ausgang::SchonAbgeschlossen(a.datum));
        }
        if a.datum > k.handelstag {
            return Err(halt(format!(
                "Letzter Abschluss am {} liegt nach dem Handelstag {}",
                datum_text(a.datum),
                datum_text(k.handelstag)
            )));
        }
    }
    if j.stand().seit_abschluss() > 0 {
        return Err(halt(format!(
            "{} Eintraege nach dem letzten Abschluss: ein Tag ist angefangen und nicht \
             abgeschlossen. Halt bis zur Pruefung; niemand wird neu gefragt",
            j.stand().seit_abschluss()
        )));
    }
    j.stelle_um().map_err(journal)?;
    let hoch = if neu {
        for t in start {
            j.setze_topf(t).map_err(journal)?;
        }
        depotwert(start.iter(), &k.vortag, Rundung::Auf).ok_or_else(|| rechnung("Startwert"))?
    } else {
        letzter
            .as_ref()
            .map(|a| a.hoechststand)
            .ok_or_else(|| halt("Toepfe ohne Abschluss im Journal"))?
    };
    let grenzen = Grenzen::standard();
    let tag = Tag {
        datum: k.handelstag,
        kurse: k.heute,
        ereignisse: ereignisse(j.stand(), k.handelstag),
    };
    j.begruende(
        tag.datum,
        PAPIERTAG,
        "Papiergeld; Ausfuehrung zum Schlusskurs des Entscheidungstags (Vereinfachung)",
    )
    .map_err(journal)?;
    let fehler = pruefe_kurse(&grenzen, universum, tag.datum, &tag.kurse, &k.vortag);
    let ab = abdruck(
        letzter.as_ref().map(|a| a.abdruck.as_str()),
        &tag,
        &k.vortag,
    );
    let nr = tagnummer(tag.datum).ok_or_else(|| rechnung("Tagnummer"))?;
    let z = Zyklus {
        grenzen: &grenzen,
        universum,
        tag: &tag,
        vortag: &k.vortag,
        nr,
        abdruck: &ab,
    };
    let mut beteiligte = Beteiligte { dachs, maus, konto };
    let hoechststand = ein_tag(j, &z, hoch, &fehler, &mut beteiligte)?;
    Ok(Ausgang::Abgeschlossen {
        datum: tag.datum,
        hoechststand,
    })
}
