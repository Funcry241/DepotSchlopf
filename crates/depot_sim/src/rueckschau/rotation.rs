//! Website-unabhängige Referenz für ein handelbares, tagesbasiertes Rotationssignal.
use std::{collections::BTreeMap, fmt::Write as _};

use depot_journal::KursBeleg;
use depot_kern::liste::ETF_ISIN;
use depot_kern::{Boerse, Cent, Datum, Herkunft, Isin, Preis, Rundung, Zeitpunkt};

use super::{Fall, erloes, grober_preissprung, kurs};
use crate::vergleichsrechnung::kaufe_gleich;

const FALL: Fall = Fall {
    start: Cent(100_000),
    gebuehr: Cent(218),
    mindestens: Cent(22_000),
};
const RUECKBLICK: usize = 5;
type Preise = BTreeMap<Isin, Preis>;
type Tagesbelege<'a> = BTreeMap<Datum, BTreeMap<Isin, &'a KursBeleg>>;

fn handelstage_ab(datum: Datum) -> impl Iterator<Item = Datum> {
    std::iter::successors(datum.plus_tage(1), |d| d.plus_tage(1))
        .filter(|&d| Boerse::Xetra.handelstag(d))
}

fn revisionsstand<'a>(
    belege: &'a [KursBeleg],
    werte: &[Isin],
    jetzt: Zeitpunkt,
) -> Tagesbelege<'a> {
    let mut aus = Tagesbelege::new();
    for b in belege.iter().filter(|b| {
        b.abruf() <= jetzt && b.kurs().herkunft() == Herkunft::Quelle && werte.contains(b.isin())
    }) {
        let alt = aus
            .entry(b.kurs().handelstag())
            .or_default()
            .entry(b.isin().clone())
            .or_insert(b);
        if b.abruf() < alt.abruf() {
            *alt = b;
        }
    }
    aus
}

fn preise(belege: &Tagesbelege<'_>, tag: Datum, stichtag: Zeitpunkt) -> Option<Preise> {
    let werte = belege.get(&tag)?;
    (werte.len() == ETF_ISIN.len() && werte.values().all(|b| b.abruf() <= stichtag)).then(|| {
        werte
            .iter()
            .map(|(isin, b)| (isin.clone(), b.kurs().preis()))
            .collect()
    })
}

fn staerke(fenster: &[Preise], isin: &Isin) -> Option<i64> {
    let alt = *fenster.first()?.get(isin)?;
    fenster.last()?.get(isin)?.checked_sub(alt)?.in_bp_von(alt)
}

#[derive(Debug, PartialEq, Eq)]
enum Signal<'a> {
    KeinVorsprung,
    Rotation(&'a Isin),
}

fn signal<'a>(
    belege: &Tagesbelege<'_>,
    werte: &'a [Isin],
    datum: Datum,
    stichtag: Zeitpunkt,
) -> Option<Signal<'a>> {
    let tage = std::iter::successors(datum.plus_tage(-1), |d| d.plus_tage(-1))
        .filter(|&d| Boerse::Xetra.handelstag(d))
        .take(RUECKBLICK.saturating_add(1))
        .collect::<Vec<_>>();
    if tage.len() != RUECKBLICK.saturating_add(1) {
        return None;
    }
    let fenster = tage
        .into_iter()
        .rev()
        .map(|tag| preise(belege, tag, stichtag))
        .collect::<Option<Vec<_>>>()?;
    if fenster.windows(2).any(|p| {
        p.first()
            .zip(p.last())
            .is_some_and(|(a, b)| grober_preissprung(a, b))
    }) {
        return None;
    }
    let massstab = staerke(&fenster, werte.first()?)?;
    let mut rang = werte
        .iter()
        .skip(1)
        .map(|isin| Some((isin, staerke(&fenster, isin)?)))
        .collect::<Option<Vec<_>>>()?;
    rang.sort_by(|(a, x), (b, y)| y.cmp(x).then_with(|| a.cmp(b)));
    Some(match rang.first() {
        Some((isin, bp)) if *bp > massstab => Signal::Rotation(isin),
        _ => Signal::KeinVorsprung,
    })
}

fn netto(ein: Preis, aus: Preis, isin: &Isin) -> Option<Cent> {
    let kauf = kaufe_gleich(
        FALL.start,
        &[(isin.clone(), Some(kurs(ein, Rundung::Auf)?))],
        |_| Some(FALL.gebuehr),
    )?;
    let stueck = *kauf.stueck.get(isin)?;
    if kurs(ein, Rundung::Auf)?.mal_stueck(stueck, Rundung::Auf)? < FALL.mindestens {
        return None;
    }
    kauf.bargeld
        .checked_add(erloes(aus, stueck, &FALL)?)?
        .checked_sub(FALL.start)
}

fn vergleich(
    belege: &Tagesbelege<'_>,
    tag: Datum,
    jetzt: Zeitpunkt,
    isin: &Isin,
    welt: &Isin,
) -> Option<(Datum, Cent, Cent)> {
    let bis = handelstage_ab(tag).next()?;
    let ein = preise(belege, tag, jetzt)?;
    let aus = preise(belege, bis, jetzt)?;
    if grober_preissprung(&ein, &aus) {
        return None;
    }
    Some((
        bis,
        netto(*ein.get(isin)?, *aus.get(isin)?, isin)?,
        netto(*ein.get(welt)?, *aus.get(welt)?, welt)?,
    ))
}

/// Lesender Vergleich auf Basis der tatsächlichen Abrufzeiten der ausgewählten Schlusskurse.
pub fn rotationsbericht(belege: &[KursBeleg], jetzt: Zeitpunkt, von: Datum, bis: Datum) -> String {
    let mut text = String::from(
        "\n## Kapitalrotation · Historische Referenz\n\nUm 09:00 Uhr Frankfurter Zeit vergleicht das Signal fünf Xetra-Handelstage relative Stärke von S&P 500 und EURO STOXX 50 gegenüber MSCI World. Stichtag und tatsächliche Abrufzeit begrenzen die Eingaben. Der Einstieg wird zum belegten Schlusskurs desselben Tags modelliert, der Ausstieg zum folgenden Xetra-Schluss. Je Fall 1.000 € Startbetrag, ganze Stücke, 5 bp Spread und 2,18 € je Seite. MSCI World wird mit denselben Kosten und denselben Ausführungstagen gerechnet. Die Fälle sind Einzelvergleiche ohne fortgeschriebenen Depotwert.\n\n",
    );
    let werte: Vec<_> = ETF_ISIN
        .into_iter()
        .filter_map(|i| Isin::neu(i).ok())
        .collect();
    let Some(welt) = werte.first() else {
        return text;
    };
    let reihe = revisionsstand(belege, &werte, jetzt);
    let vollstaendig = reihe.values().filter(|k| k.len() == werte.len()).count();
    let mut fehlend = 0_usize;
    let mut ohne = 0_usize;
    let mut offen = 0_usize;
    let mut abgeschlossen = 0_usize;
    let mut zeilen = String::new();
    for datum in std::iter::successors(Some(von), |d| d.plus_tage(1))
        .take_while(|&d| d <= bis)
        .filter(|&d| Boerse::Xetra.handelstag(d))
    {
        let Some(stichtag) = Zeitpunkt::frankfurt(datum, 9, 0, 0).filter(|t| *t <= jetzt) else {
            continue;
        };
        match signal(&reihe, &werte, datum, stichtag) {
            None => fehlend = fehlend.saturating_add(1),
            Some(Signal::KeinVorsprung) => ohne = ohne.saturating_add(1),
            Some(Signal::Rotation(isin)) => {
                if let Some((bis, rotation, massstab)) = vergleich(&reihe, datum, jetzt, isin, welt)
                {
                    abgeschlossen = abgeschlossen.saturating_add(1);
                    let _ = writeln!(
                        zeilen,
                        "| {datum} | {isin} | {bis} | {rotation} | {massstab} |"
                    );
                } else {
                    offen = offen.saturating_add(1);
                    let _ = writeln!(zeilen, "| {datum} | {isin} | – | – | – |");
                }
            }
        }
    }
    let _ = writeln!(
        text,
        "{vollstaendig} vollständige Kurstage im Kursbestand. Im Entscheidungszeitraum fehlen an {fehlend} Tagen rechtzeitig verfügbare, lückenlose Kurse. An {ohne} Tagen liegt kein positiver relativer Vorsprung vor. {abgeschlossen} vollständige Ausführungspaare. Bei {offen} Signalen fehlt die vollständige Ausführungsbasis.\n"
    );
    if zeilen.is_empty() {
        text.push_str("Für diesen Zeitraum liegt kein ausführbarer Vergleich vor.\n");
    } else {
        text.push_str("| Signal und geplanter Einstieg | ISIN | Ausstieg | Rotation nach Kosten | MSCI World nach Kosten |\n|---|---|---|---:|---:|\n");
        text.push_str(&zeilen);
    }
    text
}

#[cfg(test)]
mod tests {
    use depot_kern::Kurs;

    use super::*;

    #[test]
    fn spaet_abgerufener_schlusskurs_darf_keine_rotation_ausloesen() -> Result<(), String> {
        let datum = Datum::lies("2026-11-20").ok_or("Datum")?;
        let stichtag = Zeitpunkt::frankfurt(datum, 9, 0, 0).ok_or("Stichtag")?;
        let spaet = Zeitpunkt::frankfurt(datum, 9, 1, 0).ok_or("Verspäteter Abruf")?;
        let werte = ETF_ISIN
            .into_iter()
            .map(|i| Isin::neu(i).map_err(|e| format!("{e:?}")))
            .collect::<Result<Vec<_>, _>>()?;
        let mut tage = std::iter::successors(datum.plus_tage(-1), |d| d.plus_tage(-1))
            .filter(|&d| Boerse::Xetra.handelstag(d))
            .take(6)
            .collect::<Vec<_>>();
        tage.reverse();
        let mut belege = Vec::new();
        for (nr, tag) in tage.into_iter().enumerate() {
            let schluss = Zeitpunkt::frankfurt(tag, 17, 35, 0).ok_or("Schluss")?;
            for (i, isin) in werte.iter().enumerate() {
                let preis = if i == 1 {
                    10_000_i64
                        .saturating_add(i64::try_from(nr).unwrap_or_default().saturating_mul(100))
                } else {
                    10_000
                };
                let kurs = Kurs::neu(Cent(preis), "EUR", tag, schluss, Herkunft::Quelle)
                    .map_err(|e| format!("{e:?}"))?;
                let abruf = if nr == 0 && i == 1 { spaet } else { schluss };
                belege.push(KursBeleg::neu("test", abruf, b"Test", isin.clone(), kurs)?);
            }
        }
        let aus = revisionsstand(&belege, &werte, spaet);
        assert_eq!(signal(&aus, &werte, datum, stichtag), None);
        let erwartet = werte.get(1).ok_or("ETF")?;
        assert_eq!(
            signal(&aus, &werte, datum, spaet),
            Some(Signal::Rotation(erwartet))
        );
        Ok(())
    }

    #[test]
    fn ausfuehrung_braucht_den_unmittelbar_folgenden_xetra_tag() -> Result<(), String> {
        let freitag = Datum::lies("2026-11-20").ok_or("Freitag")?;
        let montag = Datum::lies("2026-11-23").ok_or("Montag")?;
        assert_eq!(handelstage_ab(freitag).next(), Some(montag));
        let dienstag = Datum::lies("2026-11-24").ok_or("Dienstag")?;
        let jetzt = Zeitpunkt::frankfurt(dienstag, 18, 0, 0).ok_or("Stichtag")?;
        let werte = ETF_ISIN
            .into_iter()
            .map(|i| Isin::neu(i).map_err(|e| format!("{e:?}")))
            .collect::<Result<Vec<_>, _>>()?;
        let welt = werte.first().ok_or("Welt")?;
        let kandidaten = werte.get(1).ok_or("ETF")?;
        let mut belege = Vec::new();
        for tag in [freitag, dienstag, montag] {
            let schluss = Zeitpunkt::frankfurt(tag, 17, 35, 0).ok_or("Schluss")?;
            for isin in &werte {
                let preis = if tag == montag && isin == kandidaten {
                    10_500
                } else {
                    10_000
                };
                let kurs = Kurs::neu(Cent(preis), "EUR", tag, schluss, Herkunft::Quelle)
                    .map_err(|e| format!("{e:?}"))?;
                belege.push(KursBeleg::neu(
                    "test",
                    schluss,
                    b"Test",
                    isin.clone(),
                    kurs,
                )?);
            }
        }
        let ohne_montag = revisionsstand(belege.get(..6).ok_or("Belege")?, &werte, jetzt);
        assert_eq!(
            vergleich(&ohne_montag, freitag, jetzt, kandidaten, welt),
            None
        );
        let mit_montag = revisionsstand(&belege, &werte, jetzt);
        let (ausstieg, rotation, massstab) =
            vergleich(&mit_montag, freitag, jetzt, kandidaten, welt)
                .ok_or("Vollständiger Vergleich")?;
        assert_eq!(ausstieg, montag);
        assert!(rotation > Cent::NULL && massstab < Cent::NULL);
        Ok(())
    }
}
