//! Kaufkandidaten zum Beratungszeitpunkt, auch wenn die Beratung sie später verwirft.

use std::{collections::BTreeMap, fmt, ops::Range, str::FromStr};

use depot_journal::{Journal, Stand};
use depot_kern::liste::boerse_von;
use depot_kern::{
    BP_GANZ, Boerse, Cent, Datum, Grenzen, Herkunft, Isin, Preis, Rundung, Topf, TopfId, Vorschlag,
    Zeitpunkt,
};

use crate::lauf::{LaufFehler, Zyklus, journal};
use crate::positionskauf::{Ausstiegsplan, Kursziel};

/// Eigener, versionierter Beleg für die simulierte Nachrechnung. Er ist keine Order.
pub const BELEG: &str = "Schattenkandidat";
const BERATUNG: &str = "Schattenberatung";
const PHASE: &str = "Schattenphase";
const SIGNAL: &str = "Schattensignal";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Beratungslauf {
    pub tag: Datum,
    pub lauf: usize,
    pub buchungen: Range<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Phasenbeleg {
    pub tag: Datum,
    pub lauf: usize,
    pub topf: TopfId,
    pub phase: Phase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    Erstmeinung,
    Zweitmeinung,
    Runde(u8),
    Gemeinsam,
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Erstmeinung => f.write_str("Erstmeinung"),
            Self::Zweitmeinung => f.write_str("Zweitmeinung"),
            Self::Runde(nr) => write!(f, "Runde-{nr}"),
            Self::Gemeinsam => f.write_str("Gemeinsam"),
        }
    }
}

impl Phase {
    fn lies(text: &str) -> Option<Self> {
        match text {
            "Erstmeinung" => Some(Self::Erstmeinung),
            "Zweitmeinung" => Some(Self::Zweitmeinung),
            "Gemeinsam" => Some(Self::Gemeinsam),
            _ => text.strip_prefix("Runde-")?.parse().ok().map(Self::Runde),
        }
    }
}

/// Ein Kaufziel mit seinen zum Beratungszeitpunkt belegbaren Modellparametern.
/// `None` heißt unbekannt oder unbrauchbar. `stueck = Some(0)` heißt belegbar nicht stückelbar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kandidat {
    pub tag: Datum,
    pub lauf: Option<usize>,
    pub topf: TopfId,
    pub phase: Phase,
    pub isin: Isin,
    pub ziel_bp: i64,
    pub stueck: Option<u64>,
    pub limit: Option<Cent>,
    pub stopp_bp: Option<i64>,
    pub plan: Option<Ausstiegsplan>,
    pub bereit_ab: Option<Zeitpunkt>,
}

fn feld<T: fmt::Display>(wert: Option<T>) -> String {
    wert.map_or_else(|| "-".into(), |wert| wert.to_string())
}

fn lies_feld<T: FromStr>(text: &str) -> Result<Option<T>, T::Err> {
    if text == "-" {
        Ok(None)
    } else {
        text.parse().map(Some)
    }
}

fn lies_plan(ziel: &str, tage: &str) -> Result<Option<Ausstiegsplan>, ()> {
    if ziel == "-" && tage == "-" {
        return Ok(None);
    }
    let (kursziel, wert) = if let Some(bp) = ziel.strip_prefix('+') {
        let bp = bp.parse().map_err(|_| ())?;
        (Kursziel::GewinnBp(bp), bp)
    } else {
        let cent = ziel.parse().map_err(|_| ())?;
        (Kursziel::Absolut(Cent(cent)), cent)
    };
    let handelstage = tage.parse().map_err(|_| ())?;
    if wert <= 0 || handelstage == 0 {
        return Err(());
    }
    Ok(Some(Ausstiegsplan {
        kursziel,
        handelstage,
    }))
}

fn topf_text(topf: TopfId) -> &'static str {
    match topf {
        TopfId::A => "A",
        TopfId::B => "B",
    }
}

fn lies_topf(text: &str) -> Option<TopfId> {
    match text {
        "A" => Some(TopfId::A),
        "B" => Some(TopfId::B),
        _ => None,
    }
}

impl Kandidat {
    fn text(&self) -> String {
        let version = self
            .lauf
            .map_or_else(|| "v1".into(), |lauf| format!("v2 {lauf}"));
        let topf = topf_text(self.topf);
        let ziel = self.plan.map_or_else(
            || "-".into(),
            |plan| match plan.kursziel {
                Kursziel::Absolut(Cent(cent)) => cent.to_string(),
                Kursziel::GewinnBp(bp) => format!("+{bp}"),
            },
        );
        format!(
            "{version} {topf} {} {} {} {} {} {} {ziel} {} {}",
            self.phase,
            self.isin,
            self.ziel_bp,
            feld(self.stueck),
            feld(self.limit.map(|c| c.0)),
            feld(self.stopp_bp),
            feld(self.plan.map(|p| p.handelstage)),
            feld(self.bereit_ab.map(Zeitpunkt::sekunden))
        )
    }

    fn lies(tag: Datum, text: &str) -> Option<Self> {
        let mut felder = text.split_ascii_whitespace();
        let lauf = match felder.next()? {
            "v1" => None,
            "v2" => Some(felder.next()?.parse().ok()?),
            _ => return None,
        };
        let topf = lies_topf(felder.next()?)?;
        let phase = Phase::lies(felder.next()?)?;
        let isin = Isin::neu(felder.next()?).ok()?;
        let ziel_bp = felder.next()?.parse().ok()?;
        let stueck = lies_feld(felder.next()?).ok()?;
        let limit = lies_feld(felder.next()?).ok()?.map(Cent);
        let stopp_bp = lies_feld(felder.next()?).ok()?;
        let plan = lies_plan(felder.next()?, felder.next()?).ok()?;
        let bereit_ab = match lies_feld(felder.next()?).ok()? {
            Some(sekunden) => Some(Zeitpunkt::aus_sekunden(sekunden)?),
            None => None,
        };
        (ziel_bp > 0
            && limit.is_none_or(|c| c > Cent::NULL)
            && stopp_bp.is_none_or(|bp| bp > 0)
            && felder.next().is_none())
        .then_some(Self {
            tag,
            lauf,
            topf,
            phase,
            isin,
            ziel_bp,
            stueck,
            limit,
            stopp_bp,
            plan,
            bereit_ab,
        })
    }
}

/// Belegte Kandidaten in Journalreihenfolge. Unbekannte Versionen sind keine lesbaren Belege.
pub fn kandidaten(stand: &Stand) -> Vec<Kandidat> {
    stand
        .begruendungen()
        .iter()
        .filter(|(_, wer, _)| wer == BELEG)
        .filter_map(|(tag, _, text)| Kandidat::lies(*tag, text))
        .collect()
}

fn lies_lauf(text: &str) -> Option<(&str, usize, usize)> {
    let mut felder = text.split_ascii_whitespace();
    let ("v1", art, lauf, buchung, None) = (
        felder.next()?,
        felder.next()?,
        felder.next()?.parse().ok()?,
        felder.next()?.parse().ok()?,
        felder.next(),
    ) else {
        return None;
    };
    Some((art, lauf, buchung))
}

/// Abgeschlossene Beratungen. Die Buchungsindizes bleiben über spätere Abrechnungen stabil.
pub fn beratungslaeufe(stand: &Stand) -> Vec<Beratungslauf> {
    let mut beginn = BTreeMap::new();
    let mut ergebnis = Vec::new();
    for (tag, _, text) in stand
        .begruendungen()
        .iter()
        .filter(|(_, wer, _)| wer == BERATUNG)
    {
        match lies_lauf(text) {
            Some(("Beginn", lauf, buchung)) => {
                beginn.insert((*tag, lauf), buchung);
            }
            Some(("Ende", lauf, ende)) => {
                if let Some(anfang) = beginn
                    .remove(&(*tag, lauf))
                    .filter(|&anfang| anfang <= ende)
                    && ende <= stand.ki().buchungen().len()
                {
                    ergebnis.push(Beratungslauf {
                        tag: *tag,
                        lauf,
                        buchungen: anfang..ende,
                    });
                }
            }
            _ => {}
        }
    }
    ergebnis
}

pub fn phasenbelege(stand: &Stand) -> Vec<Phasenbeleg> {
    stand
        .begruendungen()
        .iter()
        .filter(|(_, wer, _)| wer == PHASE)
        .filter_map(|(tag, _, text)| {
            let mut felder = text.split_ascii_whitespace();
            let ("v1", lauf, topf, phase, None) = (
                felder.next()?,
                felder.next()?.parse().ok()?,
                lies_topf(felder.next()?)?,
                Phase::lies(felder.next()?)?,
                felder.next(),
            ) else {
                return None;
            };
            Some(Phasenbeleg {
                tag: *tag,
                lauf,
                topf,
                phase,
            })
        })
        .collect()
}

/// Rangführer aus den Kursen, die für dieselbe Beratung vorlagen.
pub(crate) fn signale(stand: &Stand) -> BTreeMap<(Datum, usize, Boerse), Isin> {
    stand
        .begruendungen()
        .iter()
        .filter(|(_, wer, _)| wer == SIGNAL)
        .filter_map(|(tag, _, text)| {
            let mut felder = text.split_ascii_whitespace();
            let ("v1", lauf, isin, None) = (
                felder.next()?,
                felder.next()?.parse().ok()?,
                Isin::neu(felder.next()?).ok()?,
                felder.next(),
            ) else {
                return None;
            };
            Some(((*tag, lauf, boerse_von(&isin)?), isin))
        })
        .collect()
}

pub(crate) fn beginne(j: &mut Journal, z: &Zyklus<'_>) -> Result<Option<usize>, LaufFehler> {
    if z.uhr.is_none() {
        return Ok(None);
    }
    let lauf = j.stand().begruendungen().len();
    laufbeleg(j, z.tag.datum, lauf, "Beginn")?;
    let rang = crate::relative_staerke::kurzrang(j.stand(), &z.tag.kurse, z.universum, z.tag.datum);
    for boerse in [Boerse::Xetra, Boerse::Nyse] {
        if let Some((isin, _)) = rang.iter().find(|(isin, _)| {
            boerse_von(isin) == Some(boerse)
                && z.universum.handelbar(isin, z.grenzen)
                && schlusskurs(z, isin).is_some()
        }) {
            j.begruende(z.tag.datum, SIGNAL, &format!("v1 {lauf} {isin}"))
                .map_err(journal)?;
        }
    }
    Ok(Some(lauf))
}

pub(crate) fn beende(j: &mut Journal, tag: Datum, lauf: Option<usize>) -> Result<(), LaufFehler> {
    if let Some(lauf) = lauf {
        laufbeleg(j, tag, lauf, "Ende")?;
    }
    Ok(())
}

fn laufbeleg(j: &mut Journal, tag: Datum, lauf: usize, art: &str) -> Result<(), LaufFehler> {
    let index = j.stand().ki().buchungen().len();
    j.begruende(tag, BERATUNG, &format!("v1 {art} {lauf} {index}"))
        .map_err(journal)
}

fn laufkennung(stand: &Stand, datum: Datum) -> Option<usize> {
    let (_, _, text) = stand
        .begruendungen()
        .iter()
        .rev()
        .find(|(tag, wer, _)| *tag == datum && wer == BERATUNG)?;
    let ("Beginn", lauf, _) = lies_lauf(text)? else {
        return None;
    };
    Some(lauf)
}

fn schlusskurs(z: &Zyklus<'_>, isin: &Isin) -> Option<Preis> {
    let kurs = z.tag.kurse.eintrag(isin)?;
    let boerse = boerse_von(isin)?;
    let tag = kurs.handelstag();
    (kurs.herkunft() == Herkunft::Quelle
        && boerse.handelstag(tag)
        && boerse.schluss(tag) == Some(kurs.zeitpunkt())
        && !z.gesperrt.contains(isin))
    .then_some(kurs.preis())
}

pub(crate) fn modell_stueck(
    grenzen: &Grenzen,
    ziel: Preis,
    frei: Cent,
    limit: Cent,
) -> Option<u64> {
    let je_stueck = Preis::aus_cent(limit)?;
    let budget = Preis::aus_cent(frei)?;
    // Anlageziel begrenzt den Gegenwert. Die Gebühr muss zusätzlich in das freie Bargeld passen.
    let maximal = je_stueck.stueck_fuer(ziel.min(budget))?;
    let passt = |stueck| {
        let volumen = limit.mal_stueck(stueck)?;
        Some(volumen.checked_add(grenzen.gebuehren().fuer(volumen)?)? <= frei)
    };
    let (mut unten, mut oben) = (0_u64, maximal);
    if passt(oben)? {
        unten = oben;
    }
    while oben.checked_sub(unten)? > 1 {
        let mitte = unten.checked_add(oben.checked_sub(unten)?.checked_div(2)?)?;
        if passt(mitte)? {
            unten = mitte;
        } else {
            oben = mitte;
        }
    }
    if limit.mal_stueck(unten)? < grenzen.min_order() {
        return Some(0);
    }
    Some(unten)
}

/// Schreibt ausschließlich Belege. Das Größenmodell prüft jeden Kauf einzeln gegen das zur
/// Beratungszeit freie Bargeld, ohne dieses zwischen Modellkäufen aufzuteilen oder zu reservieren.
pub(crate) fn belege(
    j: &mut Journal,
    z: &Zyklus<'_>,
    phase: Phase,
    topf: &Topf,
    vorschlag: Option<&Vorschlag>,
) -> Result<(), LaufFehler> {
    let Some(uhr) = z.uhr else {
        return Ok(());
    };
    let lauf = laufkennung(j.stand(), z.tag.datum);
    if let Some(vorschlag) = vorschlag {
        let bereit_ab = uhr().filter(|zeit| *zeit >= z.tag.kurse.stand());
        belege_kaeufe(j, z, phase, topf, vorschlag, bereit_ab, lauf)?;
    }
    if let Some(lauf) = lauf {
        j.begruende(
            z.tag.datum,
            PHASE,
            &format!("v1 {lauf} {} {phase}", topf_text(topf.id())),
        )
        .map_err(journal)?;
    }
    Ok(())
}

fn belege_kaeufe(
    j: &mut Journal,
    z: &Zyklus<'_>,
    phase: Phase,
    topf: &Topf,
    vorschlag: &Vorschlag,
    bereit_ab: Option<Zeitpunkt>,
    lauf: Option<usize>,
) -> Result<(), LaufFehler> {
    let topfwert = topf.wert_exakt(&z.tag.kurse);
    let grenzen = z.grenzen;
    let frei = depot_journal::konto::verfuegbar(grenzen, j.stand(), topf.id());
    for (isin, &ziel_bp) in vorschlag.zielgewichte_bp.iter().filter(|(_, bp)| **bp > 0) {
        let kurs = schlusskurs(z, isin);
        let ziel = (|| {
            let kurs = kurs?;
            let gesamt = Preis::aus_cent(topfwert?.mal_bp(ziel_bp, Rundung::Ab)?)?;
            gesamt.checked_sub(kurs.mal_stueck_exakt(topf.stueck(isin))?)
        })();
        // Ein bestätigter Bestand ohne Erhöhung ist kein neues Kaufziel.
        if ziel.is_some_and(|ziel| ziel <= Preis::NULL) {
            continue;
        }
        let abstand = vorschlag
            .limit_bp
            .get(isin)
            .copied()
            .filter(|bp| {
                (grenzen.limit_abstand_min_bp()..=grenzen.limit_abstand_max_bp()).contains(bp)
            })
            .unwrap_or(grenzen.limit_abstand_bp());
        let limit = kurs
            .and_then(|kurs| kurs.mal_bp(BP_GANZ.checked_add(abstand)?, Rundung::Ab))
            .filter(|limit| *limit > Cent::NULL);
        let stueck = if z.universum.handelbar(isin, grenzen) {
            ziel.zip(frei)
                .zip(limit)
                .and_then(|((ziel, frei), limit)| modell_stueck(grenzen, ziel, frei, limit))
        } else {
            Some(0)
        };
        let stopp_bp = vorschlag
            .stopp_bp
            .get(isin)
            .copied()
            .filter(|bp| (grenzen.stopp_min_bp()..=grenzen.notbremse_bp()).contains(bp));
        let plan = vorschlag
            .begruendung
            .get(isin)
            .and_then(|text| Ausstiegsplan::lies(text));
        let kandidat = Kandidat {
            tag: z.tag.datum,
            lauf,
            topf: topf.id(),
            phase,
            isin: isin.clone(),
            ziel_bp,
            stueck,
            limit,
            stopp_bp,
            plan,
            bereit_ab,
        };
        j.begruende(z.tag.datum, BELEG, &kandidat.text())
            .map_err(journal)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;

    use depot_kern::{Boerse, Kurs, Kurse, liste};

    use super::*;
    use crate::Tag;

    #[test]
    fn veto_loescht_den_kaufkandidaten_und_seine_daten_nicht() -> Result<(), String> {
        let tag = Datum::neu(2026, 11, 24).ok_or("Tag")?;
        let vortag = tag.plus_tage(-1).ok_or("Vortag")?;
        let schluss = Boerse::Xetra.schluss(vortag).ok_or("Schluss")?;
        let stand = Zeitpunkt::frankfurt(tag, 7, 35, 0).ok_or("Kursstand")?;
        let bereit = Zeitpunkt::frankfurt(tag, 7, 36, 0).ok_or("Kandidat bereit")?;
        let telekom = Isin::neu("DE0005557508").map_err(|e| format!("{e:?}"))?;
        let ohne_kurs = Isin::neu("DE0005190003").map_err(|e| format!("{e:?}"))?;
        let kurs = Kurs::neu(Cent(2_634), "EUR", vortag, schluss, Herkunft::Quelle)
            .map_err(|e| format!("{e:?}"))?;
        let kurse =
            Kurse::neu(stand, vec![(telekom.clone(), kurs)]).map_err(|e| format!("{e:?}"))?;
        let tag = Tag {
            datum: tag,
            kurse,
            ereignisse: Vec::new(),
        };
        let topf = Topf::neu(TopfId::B, Cent(500_000)).map_err(|e| format!("{e:?}"))?;
        let vorschlag = Vorschlag {
            berater: topf.id().berater(),
            zielgewichte_bp: BTreeMap::from([(telekom.clone(), 3500), (ohne_kurs.clone(), 3000)]),
            erwartung_bp: BTreeMap::new(),
            limit_bp: BTreeMap::new(),
            park: None,
            stopp_bp: BTreeMap::from([(telekom.clone(), 300)]),
            begruendung: BTreeMap::from([(
                telekom.clone(),
                "[Kursbewegung][Ausstieg +150bp 5 HANDELSTAGE] These".into(),
            )]),
            belege: BTreeMap::new(),
        };
        let uhr = || Some(bereit);
        let grenzen = Grenzen::standard();
        let universum = liste::als_universum(&liste::beobachtungsliste()?);
        let gesperrt = BTreeSet::new();
        let zyklus = Zyklus {
            anlass_pruefen: false,
            grenzen: &grenzen,
            universum: &universum,
            tag: &tag,
            vortag: &tag.kurse,
            nr: 1,
            abdruck: "Test",
            uhr: Some(&uhr),
            vollstaendige_kurse: false,
            gesperrt: &gesperrt,
            bewertung: &tag.kurse,
        };
        let ordner =
            std::env::temp_dir().join(format!("depot-schattenkandidaten-{}", std::process::id()));
        fs::create_dir(&ordner).map_err(|e| e.to_string())?;
        let test = || -> Result<(), String> {
            let pfad = ordner.join("journal.txt");
            let fehler = |e| format!("{e:?}");
            Journal::einrichten(&pfad).map_err(fehler)?;
            let mut j = Journal::oeffnen(&pfad).map_err(fehler)?;
            j.setze_topf(&topf).map_err(fehler)?;
            let lauf = beginne(&mut j, &zyklus).map_err(|e| format!("{e:?}"))?;
            belege(&mut j, &zyklus, Phase::Erstmeinung, &topf, Some(&vorschlag))
                .map_err(|e| format!("{e:?}"))?;
            belege(&mut j, &zyklus, Phase::Zweitmeinung, &topf, None)
                .map_err(|e| format!("{e:?}"))?;
            belege(&mut j, &zyklus, Phase::Gemeinsam, &topf, None).map_err(|e| format!("{e:?}"))?;
            beende(&mut j, tag.datum, lauf).map_err(|e| format!("{e:?}"))?;
            let wiederholung = beginne(&mut j, &zyklus).map_err(|e| format!("{e:?}"))?;
            assert_ne!(lauf, wiederholung);
            drop(j);
            let j = Journal::oeffnen(&pfad).map_err(fehler)?;
            let belege = kandidaten(j.stand());
            assert_eq!(belege.len(), 2);
            let erster = belege.iter().find(|k| k.isin == telekom).ok_or("Telekom")?;
            assert_eq!(erster.phase, Phase::Erstmeinung);
            assert_eq!(erster.lauf, lauf);
            assert_eq!(erster.limit, Some(Cent(2_660)));
            assert_eq!(erster.stueck, Some(65));
            assert_eq!(erster.stopp_bp, Some(300));
            assert_eq!(erster.plan, None);
            assert_eq!(erster.bereit_ab, Some(bereit));
            let ohne = belege
                .iter()
                .find(|k| k.isin == ohne_kurs)
                .ok_or("Kandidat ohne Kurs")?;
            assert_eq!((ohne.stueck, ohne.limit), (None, None));
            let phasen = phasenbelege(j.stand());
            assert_eq!(phasen.len(), 3);
            assert!(
                phasen
                    .iter()
                    .any(|p| p.phase == Phase::Gemeinsam && Some(p.lauf) == lauf)
            );
            let abgeschlossen = beratungslaeufe(j.stand());
            assert_eq!(abgeschlossen.len(), 1);
            assert_eq!(abgeschlossen.first().map(|l| l.lauf), lauf);
            assert!(j.stand().auftraege().is_empty());
            assert!(j.stand().ki().buchungen().is_empty());
            Ok(())
        };
        let ergebnis = test();
        fs::remove_dir_all(ordner).map_err(|e| e.to_string())?;
        ergebnis
    }

    #[test]
    fn format_erhaelt_rundennummer_und_lesbaren_ausstieg() -> Result<(), String> {
        let tag = Datum::neu(2026, 11, 24).ok_or("Tag")?;
        let text = "v1 A Runde-2 DE0005557508 3000 0 2660 - +150 5 -";
        let kandidat = Kandidat::lies(tag, text).ok_or("Rundenbeleg")?;
        assert_eq!(kandidat.text(), text);
        assert_eq!(
            kandidat.plan,
            Some(Ausstiegsplan {
                kursziel: Kursziel::GewinnBp(150),
                handelstage: 5
            })
        );
        assert!(Kandidat::lies(tag, &text.replace("v1", "v2")).is_none());
        Ok(())
    }
}
