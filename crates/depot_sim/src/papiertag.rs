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
//! der zuletzt belegte Kurs des juengsten Tages davor. Das Alter eines Kurses misst sich an
//! seinem eigenen Abruf, denn Werte verschiedener Boersen werden je kurz nach ihrem Schluss
//! abgerufen (Wolfgang, 05.11.2026). Geprueft wie im Sammler
//! (`waehle_tageswerte`): Pflicht sind nur die gehaltenen Werte. Ein Kandidat ohne gueltigen
//! Kurs ist heute nicht handelbar; er faellt aus dem Universum des Tages, und der Grund steht
//! unter `Auswahl` im Journal. Faellt die Pruefung eines gehaltenen Werts oder des Abrufs durch,
//! oder bleibt kein Wert handelbar, faellt der Zyklus aus, und niemand wird gefragt.
//!
//! **Einmal je Tag:** Vor allem anderen gleicht der Kontowaechter Journal und Papierkonto ab;
//! jede Abweichung ist Halt, auch fuer einen schon abgeschlossenen Tag (K23-02). Ist der
//! Handelstag danach schon abgeschlossen, geschieht nichts. Ein angefangener Tag haelt an wie in
//! der Simulation; niemand wird neu gefragt.
//!
//! **Ausfuehrung:** Bestaetigte Orders bleiben offen bis zum ersten im Papiertag verwendbaren
//! Schlusskurs nach Versand. Fehlt an diesem Boersentag ein gepruefter Kurs oder liegt er
//! ausserhalb des Limits, verfallen sie.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use depot_journal::{Journal, KursBeleg, NachrichtBeleg, PapierHalt, Stand};
use depot_kern::{
    Cent, Datum, Grenzen, Herkunft, Isin, Kurs, Kurse, Rundung, Topf, Universum, Zeitpunkt,
};
use depot_regelwerk::waehle_tageswerte;
use sha2::{Digest, Sha256};

use crate::lauf::{
    Beteiligte, LaufFehler, Zyklus, depotwert, ein_tag, halt, journal, pruefe_stoerung, rechnung,
};
use crate::papier::Broker;
use crate::szenario::{Berater, Tag};

const FASSUNG: &str = "depot_sim-papiertag-v3";
const PAPIERTAG: &str = "Papiertag";
/// Unter diesem Namen stehen Kandidaten, die heute nicht handelbar sind.
pub const AUSWAHL: &str = "Auswahl";
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

/// Nummer des Tages seit 1970: steigt von Tag zu Tag, wie der Kostenschutz es verlangt. Der
/// Papiertag fragt die Berater im Zyklus mit dieser Nummer; die Anzeige des KI-Budgets prueft
/// mit derselben.
pub fn tagnummer(d: Datum) -> Option<u32> {
    let s = tagesbeginn(d)?;
    u32::try_from(s.checked_div(SEKUNDEN_JE_TAG)?).ok()
}

/// Baut aus den Kursbelegen des Journals die Kurse des juengsten Handelstags und seines
/// Vortags, nur fuer Werte des Universums und nur mit Herkunft `Quelle`.
///
/// # Errors
/// `Halt`, wenn kein Kurs belegt ist oder die Kurse sich nicht zusammenfassen lassen.
pub fn tageskurse(stand: &Stand, universum: &Universum) -> Result<Tageskurse, LaufFehler> {
    tageskurse_bis(stand, universum, None)
}

/// Kurse am belegten Stichtag. Fuer einen Tag ohne Papiertag dient der Tagesabschluss als
/// Obergrenze der Kursdiagnose. Fuer ein historisches Wiederabspielen gilt `papiertag_wiederabspielen`.
///
/// # Errors
/// `Halt`, wenn es fuer `tag` keine Kurse gab; sonst wie `tageskurse`.
pub fn tageskurse_am(
    stand: &Stand,
    universum: &Universum,
    tag: Datum,
) -> Result<Tageskurse, LaufFehler> {
    let ende = stand
        .wissensstichtag(tag)
        .or_else(|| Zeitpunkt::frankfurt(tag, 23, 59, 59))
        .ok_or_else(|| halt(format!("Kein Tagesende für den {}", datum_text(tag))))?;
    let k = tageskurse_bis(stand, universum, Some(ende))?;
    if k.handelstag != tag {
        return Err(halt(format!(
            "Am {} lagen keine Schlusskurse dieses Tages vor",
            datum_text(tag)
        )));
    }
    Ok(k)
}

fn tageskurse_bis(
    stand: &Stand,
    universum: &Universum,
    bis: Option<Zeitpunkt>,
) -> Result<Tageskurse, LaufFehler> {
    let isins: Vec<Isin> = universum.isins().into_iter().collect();
    let belege: Vec<&KursBeleg> = stand
        .kurse()
        .iter()
        .filter(|b| b.kurs().herkunft() == Herkunft::Quelle && isins.contains(b.isin()))
        .filter(|b| bis.is_none_or(|bis| b.abruf() <= bis))
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
    // Je Wert mit seinem eigenen Abruf: Das Alter misst sich daran (Abruf je Boerse).
    let kurse = |m: &BTreeMap<&Isin, &KursBeleg>| -> Result<Kurse, LaufFehler> {
        let stand = m
            .values()
            .map(|b| b.abruf())
            .max()
            .ok_or_else(|| halt("Kein Kurs für den Vergleich belegt"))?;
        let eintraege: Vec<(Isin, Kurs, Zeitpunkt)> = m
            .iter()
            .map(|(i, b)| ((*i).clone(), b.kurs().clone(), b.abruf()))
            .collect();
        Kurse::mit_abrufen(stand, eintraege).map_err(|e| halt(format!("Kurse: {e:?}")))
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

/// Die zum Wissensstichtag bekannten Termine und Nachrichten eines Papiertags.
pub fn weltbelege_am(stand: &Stand, handelstag: Datum, wissen_bis: Zeitpunkt) -> Vec<String> {
    let Some(von) = tagesbeginn(handelstag) else {
        return Vec::new();
    };
    let bis = TERMINE_VORAUS_TAGE
        .checked_mul(SEKUNDEN_JE_TAG)
        .and_then(|d| von.checked_add(d));
    let termine: BTreeMap<_, _> = stand
        .termine()
        .iter()
        .filter(|b| b.abruf() <= wissen_bis)
        .map(|b| (b.termin().kennung(), b))
        .collect();
    let mut aus: Vec<(Datum, String)> = termine
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
    aus.extend(nachrichten(stand, handelstag, wissen_bis));
    aus
}

/// Quellenbelege plus vorher fixierte Hypothesen; diese sind ausdruecklich keine Fakten.
pub fn weltlage_am(stand: &Stand, handelstag: Datum, wissen_bis: Zeitpunkt) -> Vec<String> {
    let mut aus = weltbelege_am(stand, handelstag, wissen_bis);
    for p in stand
        .vorbereitungen()
        .iter()
        .filter(|p| p.zeit < wissen_bis && p.fuer <= handelstag)
        .rev()
    {
        let alter = (0..=64)
            .filter_map(|n| p.fuer.plus_tage(n))
            .take_while(|d| *d <= handelstag)
            .filter(|d| depot_kern::liste::handelstag(*d))
            .count();
        for a in p
            .antworten
            .iter()
            .filter(|a| a.verwertbar && a.zeit <= wissen_bis)
        {
            let gueltig = a.prognose.as_ref().map_or(p.fuer == handelstag, |w| {
                alter <= usize::from(w.handelstage)
            });
            if !gueltig {
                continue;
            }
            aus.push(format!(
                "Frühere KI-Hypothese {:?} für {} (kein Quellenbeleg, neu prüfen): {}",
                a.berater, p.fuer, a.text
            ));
            if let Some(w) = &a.prognose
                && let Some(e) = w
                    .ereignis_nr
                    .checked_sub(1)
                    .and_then(|n| p.ereignisse.get(n))
            {
                aus.push(format!("Frühere Hypothese {:?}: {} {:?}; Wirkung: {}; Widerlegung/Beobachtung: {}. Ursprünglicher Beleg: {}", a.berater, w.isin, w.richtung, w.begruendung, w.widerlegung, e));
            }
        }
    }
    aus
}

/// Die belegten Nachrichten (R40), seit Beginn des vorherigen Handelstags bis zum Ende des
/// Handelstags (UTC): hoechstens `MAX_NACHRICHTEN_LAGE`, die neueste zuerst. Wird es dem
/// Berater zu viel, faellt so die aelteste weg.
fn nachrichten(stand: &Stand, handelstag: Datum, wissen_bis: Zeitpunkt) -> Vec<String> {
    let Some(beginn) = tagesbeginn(handelstag) else {
        return Vec::new();
    };
    // Seit dem vorigen echten Handelstag: Samstag und Feiertage duerfen nicht am Montag
    // verschwinden. Veroeffentlichung UND Abruf bleiben am Wissensstichtag begrenzt.
    let von = (1..=31)
        .filter_map(|n: i64| n.checked_neg().and_then(|n| handelstag.plus_tage(n)))
        .find(|d| depot_kern::liste::handelstag(*d))
        .and_then(tagesbeginn);
    let bis = beginn.checked_add(SEKUNDEN_JE_TAG);
    let mut aus: Vec<&NachrichtBeleg> = stand
        .nachrichten()
        .iter()
        .filter(|n| n.abruf() <= wissen_bis)
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
fn abdruck(vorher: Option<&str>, tag: &Tag, vortag: &Kurse, wissen_bis: Zeitpunkt) -> String {
    let mut h = Sha256::new();
    feld(&mut h, FASSUNG);
    feld(&mut h, vorher.unwrap_or("anfang"));
    feld(&mut h, &datum_text(tag.datum));
    feld(&mut h, &wissen_bis.to_string());
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

/// Die Kurse eines Papiertags: die belegten Schlusskurse, nur wenn ihr Handelstag `heute` ist.
///
/// # Errors
/// `Halt` ohne Kurse von heute; sonst wie `tageskurse`.
pub fn kurse_von_heute(
    stand: &Stand,
    universum: &Universum,
    heute: Datum,
) -> Result<Tageskurse, LaufFehler> {
    pruefe_kurstag(tageskurse(stand, universum)?, heute)
}

fn pruefe_kurstag(k: Tageskurse, heute: Datum) -> Result<Tageskurse, LaufFehler> {
    if k.handelstag != heute {
        return Err(halt(format!(
            "Die neuesten Schlusskurse sind vom {}, heute ist der {}: Ein Tag wird nur mit \
             seinen eigenen Kursen entschieden. Zuerst run.cmd kurse nach 17:35 Uhr",
            datum_text(k.handelstag),
            datum_text(heute)
        )));
    }
    Ok(k)
}

/// Wo das Journal zu einem Handelstag steht, ohne Konto und ohne etwas zu schreiben.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tageslage {
    /// Der Tag kann beginnen.
    Offen,
    /// Notbremse: Nur bereits offene Verkaeufe koennen ausgefuehrt werden.
    NurSchutzverkaeufe,
    /// Schutzverkaeufe sind erledigt; der Papiertag kann den Wiederanlauf pruefen.
    WartetAufWiederanlauf,
    /// Der Tag ist schon abgeschlossen.
    Abgeschlossen(Datum),
}

/// Prueft, ob ein Papiertag fuer `handelstag` beginnen kann. Der Papiertag selbst prueft genau
/// so, nach dem Abgleich mit dem Konto; die Anzeige zeigt damit dieselben Gruende.
///
/// # Errors
/// `Halt`, wenn der letzte Abschluss nach dem Handelstag liegt oder ein Tag angefangen ist.
pub fn tageslage(stand: &Stand, handelstag: Datum) -> Result<Tageslage, LaufFehler> {
    if let Some(a) = stand.abschluss() {
        if a.datum == handelstag {
            if stand.papier_verlustsperre_aktiv() {
                return Err(halt(PapierHalt::Gesamtverlust.meldung()));
            }
            if stand.notbremse_aktiv() {
                return Err(halt("Notbremse aktiv; keine neuen Auftraege"));
            }
            return Ok(Tageslage::Abgeschlossen(a.datum));
        }
        if a.datum > handelstag {
            return Err(halt(format!(
                "Letzter Abschluss am {} liegt nach dem Handelstag {}",
                datum_text(a.datum),
                datum_text(handelstag)
            )));
        }
    }
    if stand.seit_abschluss() > 0 {
        return Err(halt(format!(
            "{} Eintraege nach dem letzten Abschluss: ein Tag ist angefangen und nicht \
             abgeschlossen. Halt bis zur Prüfung; niemand wird neu gefragt",
            stand.seit_abschluss()
        )));
    }
    if stand.notbremse_aktiv() {
        if stand.offene_auftraege().next().is_none() && stand.gehalten().is_empty() {
            if stand.papier_verlustsperre_aktiv() {
                return Err(halt(PapierHalt::Gesamtverlust.meldung()));
            }
            return Ok(Tageslage::WartetAufWiederanlauf);
        }
        return Ok(Tageslage::NurSchutzverkaeufe);
    }
    Ok(Tageslage::Offen)
}

/// Rekonstruiert die Quellenlage eines Papiertags zum damals belegten Stichtag, ohne
/// Kontobewegung oder Beratung. Bei aelteren Tagen ohne Stichtag liefert der Aufruf Halt.
pub fn papiertag_wiederabspielen(
    stand: &Stand,
    universum: &Universum,
    tag: Datum,
) -> Result<Tag, LaufFehler> {
    let wissen_bis = stand
        .wissensstichtag(tag)
        .ok_or_else(|| halt(format!("Kein belegter Wissensstichtag für {tag}")))?;
    let kurse = pruefe_kurstag(tageskurse_bis(stand, universum, Some(wissen_bis))?, tag)?;
    Ok(Tag {
        datum: tag,
        kurse: kurse.heute,
        ereignisse: weltlage_am(stand, tag, wissen_bis),
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
    let mut beteiligte = Beteiligte { dachs, maus, konto };
    papiertag_mit_uhr(j, heute, universum, start, &mut beteiligte, None)
}

fn belegte_lage(
    stand: &Stand,
    universum: &Universum,
    heute: Datum,
    uhr: Option<&dyn Fn() -> Option<Zeitpunkt>>,
) -> Result<(Tageskurse, Zeitpunkt), LaufFehler> {
    let jetzt = match uhr {
        Some(uhr) => uhr().ok_or_else(|| halt("Keine verlässliche Zeit für den Wissensstand"))?,
        // Ein neuer Papiertag ohne reale Uhr dient Szenarien. Seine simulierte Uhr steht
        // beim letzten an diesem Tag bezeugten Abruf.
        None => {
            letzter_abruf_am(stand, heute).ok_or_else(|| halt("Keine Abrufe für den Papiertag"))?
        }
    };
    let wissen_bis = stand.wissensstichtag(heute).unwrap_or(jetzt);
    if wissen_bis > jetzt {
        return Err(halt("Wissensstichtag liegt nach der aktuellen Uhrzeit"));
    }
    let kurse = pruefe_kurstag(tageskurse_bis(stand, universum, Some(wissen_bis))?, heute)?;
    Ok((kurse, wissen_bis))
}

fn letzter_abruf_am(stand: &Stand, tag: Datum) -> Option<Zeitpunkt> {
    stand
        .kurse()
        .iter()
        .map(KursBeleg::abruf)
        .chain(
            stand
                .termine()
                .iter()
                .map(depot_journal::TerminBeleg::abruf),
        )
        .chain(stand.nachrichten().iter().map(NachrichtBeleg::abruf))
        .filter(|zeit| zeit.in_frankfurt().is_some_and(|(d, ..)| d == tag))
        .max()
}

/// Papiertag mit einer Uhr fuer den Zeitpunkt des Versands.
pub(crate) fn papiertag_mit_uhr(
    j: &mut Journal,
    heute: Datum,
    universum: &Universum,
    start: &[Topf],
    beteiligte: &mut Beteiligte<'_, '_, '_>,
    uhr: Option<&dyn Fn() -> Option<Zeitpunkt>>,
) -> Result<Ausgang, LaufFehler> {
    let (k, wissen_bis) = belegte_lage(j.stand(), universum, heute, uhr)?;
    let neu = j.stand().toepfe().is_empty();
    if !neu {
        // Wiederanlauf: zuerst der Abgleich, auch wenn der Tag schon abgeschlossen ist
        // (K23-02). Er belegt, was aus offenen Auftraegen geworden ist, und sendet nichts.
        pruefe_stoerung(&*beteiligte.konto)?;
        let abweichungen = j.gleiche_ab(&*beteiligte.konto).map_err(journal)?;
        if !abweichungen.is_empty() {
            return Err(halt(format!(
                "Kontowaechter: {abweichungen:?}. Halt bis zur Prüfung; niemand wird gefragt, \
                 nichts gesendet"
            )));
        }
    }
    if let Tageslage::Abgeschlossen(d) = tageslage(j.stand(), k.handelstag)? {
        return Ok(Ausgang::SchonAbgeschlossen(d));
    }
    let letzter = j.stand().abschluss().cloned();
    j.stelle_um().map_err(journal)?;
    j.belege_wissensstichtag(heute, wissen_bis)
        .map_err(journal)?;
    let hoch = if neu {
        for t in start {
            j.setze_topf(t).map_err(journal)?;
        }
        depotwert(start.iter(), &k.vortag, Rundung::Auf).ok_or_else(|| rechnung("Startwert"))?
    } else {
        j.stand()
            .papier_waechterreferenz()
            .ok_or_else(|| halt("Töpfe ohne Abschluss im Journal"))?
    };
    let grenzen = Grenzen::standard();
    let tag = Tag {
        datum: k.handelstag,
        kurse: k.heute,
        ereignisse: weltlage_am(j.stand(), k.handelstag, wissen_bis),
    };
    j.begruende(
        tag.datum,
        PAPIERTAG,
        &format!(
            "Papiergeld; Ausführung erst zu einem geprüften Schlusskurs nach Versand. Weltmodell: Wissensstand {wissen_bis}."
        ),
    )
    .map_err(journal)?;
    let wahl = waehle_tageswerte(
        &grenzen,
        universum,
        &j.stand().gehalten(),
        tag.datum,
        &tag.kurse,
        &k.vortag,
    );
    for f in &wahl.ausgelassen {
        j.begruende(tag.datum, AUSWAHL, &format!("heute nicht handelbar: {f:?}"))
            .map_err(journal)?;
    }
    let ab = abdruck(
        letzter.as_ref().map(|a| a.abdruck.as_str()),
        &tag,
        &k.vortag,
        wissen_bis,
    );
    let nr = tagnummer(tag.datum).ok_or_else(|| rechnung("Tagnummer"))?;
    let belegte_zeit = || match uhr {
        Some(uhr) => uhr().filter(|zeit| *zeit >= wissen_bis),
        None => Some(tag.kurse.stand()),
    };
    let z = Zyklus {
        grenzen: &grenzen,
        universum: &wahl.universum,
        tag: &tag,
        vortag: &k.vortag,
        nr,
        abdruck: &ab,
        uhr: Some(&belegte_zeit),
        vollstaendige_kurse: wahl.ausgelassen.is_empty(),
    };
    let hoechststand = ein_tag(j, &z, hoch, &wahl.fehler, beteiligte)?;
    Ok(Ausgang::Abgeschlossen {
        datum: tag.datum,
        hoechststand,
    })
}
