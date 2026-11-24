//! Hält den Befund fest, bevor der Papiertag eine Kaufentscheidung trifft.

use depot_journal::{Journal, Stand};
use depot_kern::{Boerse, Datum, Isin, Preis, Zeitpunkt};

use crate::aufwaertsdruck::{Kursindex, Signal};
use crate::lauf::{LaufFehler, journal};

const BELEG: &str = "Aufwärtsdruck";

pub(crate) struct Beobachtung {
    pub tag: Datum,
    pub boerse: Boerse,
    pub bereit: Zeitpunkt,
    pub bezugstag: Datum,
    pub wissen: Zeitpunkt,
    pub befund: Befund,
}

pub(crate) enum Befund {
    Luecke,
    KeinSignal { bewertet: usize, fehlend: usize },
    Signal(Idee),
}

pub(crate) struct Idee {
    pub isin: Isin,
    pub referenz: Preis,
    pub gruppe: String,
    pub markt_rueckgang_bp: i64,
    pub markt_erholung_bp: i64,
    pub gruppe_rueckgang_bp: i64,
    pub gruppe_erholung_bp: i64,
    pub breite_rueckgang: usize,
    pub breite_erholung: usize,
    pub titel: usize,
    pub referenz_abruf: Zeitpunkt,
}

fn signaltext(signal: &Signal) -> String {
    format!(
        "Signal;{};{};{};{};{};{};{};{};{};{};{}",
        signal.gruppe,
        signal.isin,
        signal.referenz.cent_text(),
        signal.referenz_abruf,
        signal.markt_rueckgang_bp,
        signal.markt_erholung_bp,
        signal.gruppe_rueckgang_bp,
        signal.gruppe_erholung_bp,
        signal.breite_rueckgang,
        signal.breite_erholung,
        signal.titel,
    )
}

fn lies_idee(felder: &mut std::str::Split<'_, char>) -> Option<Idee> {
    let gruppe = felder.next()?;
    if gruppe.is_empty()
        || !gruppe
            .chars()
            .all(|c| c.is_alphabetic() || c == ' ' || c == '-')
    {
        return None;
    }
    Some(Idee {
        gruppe: gruppe.to_string(),
        isin: Isin::neu(felder.next()?).ok()?,
        referenz: Preis::lies_cent_text(felder.next()?)?,
        referenz_abruf: Zeitpunkt::lies(felder.next()?)?,
        markt_rueckgang_bp: felder.next()?.parse().ok()?,
        markt_erholung_bp: felder.next()?.parse().ok()?,
        gruppe_rueckgang_bp: felder.next()?.parse().ok()?,
        gruppe_erholung_bp: felder.next()?.parse().ok()?,
        breite_rueckgang: felder.next()?.parse().ok()?,
        breite_erholung: felder.next()?.parse().ok()?,
        titel: felder.next()?.parse().ok()?,
    })
}

fn lies(tag: Datum, text: &str) -> Option<Beobachtung> {
    let mut felder = text.split(';');
    if felder.next()? != "v1" {
        return None;
    }
    let boerse = match felder.next()? {
        "Xetra" => Boerse::Xetra,
        "NYSE" => Boerse::Nyse,
        _ => return None,
    };
    let wissen = Zeitpunkt::lies(felder.next()?)?;
    let bereit = Zeitpunkt::lies(felder.next()?)?;
    let bezugstag = Datum::lies(felder.next()?)?;
    if bereit < wissen || wissen.frankfurt_tag() != Some(tag) || bezugstag > tag {
        return None;
    }
    let befund = match felder.next()? {
        "Lücke" => Befund::Luecke,
        "KeinSignal" => Befund::KeinSignal {
            bewertet: felder.next()?.parse().ok()?,
            fehlend: felder.next()?.parse().ok()?,
        },
        "Signal" => Befund::Signal(lies_idee(&mut felder)?),
        _ => return None,
    };
    if let Befund::Signal(idee) = &befund
        && (idee.referenz_abruf > wissen
            || idee.referenz <= Preis::NULL
            || depot_kern::liste::boerse_von(&idee.isin) != Some(boerse))
    {
        return None;
    }
    felder.next().is_none().then_some(Beobachtung {
        tag,
        boerse,
        bereit,
        bezugstag,
        wissen,
        befund,
    })
}

pub(crate) fn beobachtungen(stand: &Stand) -> Vec<Beobachtung> {
    stand
        .begruendungen()
        .iter()
        .filter(|(_, wer, _)| wer == BELEG)
        .filter_map(|(tag, _, text)| lies(*tag, text))
        .filter(|beleg| stand.wissensstichtag(beleg.tag) == Some(beleg.wissen))
        .collect()
}

pub(crate) fn belege(
    j: &mut Journal,
    tag: Datum,
    wissen: Zeitpunkt,
    uhr: &dyn Fn() -> Option<Zeitpunkt>,
) -> Result<(), LaufFehler> {
    let index = Kursindex::zum_stichtag(j.stand(), tag);
    let mut texte = Vec::new();
    for (boerse, name) in [(Boerse::Xetra, "Xetra"), (Boerse::Nyse, "NYSE")] {
        if !boerse.handelstag(tag) {
            continue;
        }
        let bezugstag = if boerse.schluss(tag).is_some_and(|schluss| schluss <= wissen) {
            Some(tag)
        } else {
            boerse.bezugstag(tag)
        };
        let Some(bezugstag) = bezugstag else { continue };
        let befund = index.signale(boerse, bezugstag, wissen);
        let Some(bereit) = uhr().filter(|zeit| *zeit >= wissen) else {
            continue;
        };
        let kopf = format!("v1;{name};{wissen};{bereit};{bezugstag}");
        match befund {
            None => texte.push(format!("{kopf};Lücke")),
            Some(b) if b.signale.is_empty() => texte.push(format!(
                "{kopf};KeinSignal;{};{}",
                b.gruppen_bewertet, b.gruppen_fehlend
            )),
            Some(b) => texte.extend(
                b.signale
                    .iter()
                    .map(|s| format!("{kopf};{}", signaltext(s))),
            ),
        }
    }
    for text in texte {
        j.begruende(tag, BELEG, &text).map_err(journal)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn befund_erhaelt_wissensstand_und_lehnt_zukuenftigen_bezugskurs_ab() -> Result<(), String> {
        let tag = Datum::lies("2026-11-24").ok_or("Datum")?;
        let text = concat!(
            "v1;Xetra;2026-11-24T07:30:00Z;2026-11-24T07:30:01Z;2026-11-23;",
            "Signal;Versicherer;DE0008404005;36400.5;2026-11-23T17:00:00Z;-100;30;-10;90;2;2;2"
        );
        let beobachtung = lies(tag, text).ok_or("Beobachtung")?;
        let Befund::Signal(idee) = beobachtung.befund else {
            return Err("Signal fehlt".into());
        };
        assert_eq!(idee.referenz.cent_text(), "36400.5");
        assert_eq!(idee.markt_rueckgang_bp, -100);
        assert_eq!(idee.breite_erholung, 2);
        assert_eq!(beobachtung.bereit.to_string(), "2026-11-24T07:30:01Z");
        assert!(
            lies(
                tag,
                &text.replace("2026-11-23T17:00:00Z", "2026-11-24T08:00:00Z")
            )
            .is_none()
        );
        Ok(())
    }
}
