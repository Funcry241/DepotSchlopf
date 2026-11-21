//! Samstägliche Rückschau der beiden Haupt-KIs außerhalb des Handels.

use std::path::Path;

use depot_journal::{Journal, KiZweck, Stand};
use depot_kern::{BeraterId, Cent, Datum, Grenzen, Zeitpunkt};

use crate::Berater;
use crate::handelsergebnis::{bilanz, handelspositionen, mehrertrag};
use crate::handelsvergleich::Handelsvergleich;
use crate::vergleichsrechnung::rendite_bp;

fn zusammenfassung(stand: &Stand, wer: BeraterId, tag: Datum) -> String {
    let grenzen = Grenzen::standard();
    let (verkaeufe, offen) = handelspositionen(stand, &grenzen);
    let topf = |id: depot_kern::TopfId| id.berater() == wer;
    let ab = tag.plus_tage(-5);
    let eigene: Vec<_> = verkaeufe
        .iter()
        .filter(|v| {
            topf(v.topf)
                && v.tag
                    .is_some_and(|datum| ab.is_some_and(|ab| datum >= ab) && datum < tag)
        })
        .collect();
    let korb = Handelsvergleich::neu(stand);
    let reale_bilanz = bilanz(eigene.iter().copied());
    let gegen_korb = eigene
        .iter()
        .map(|v| {
            mehrertrag(
                v.netto?,
                v.einstand?,
                korb.rendite(v.einstieg?, v.ausstieg?, &grenzen)?,
            )
        })
        .try_fold(Cent::NULL, |summe, n| summe.checked_add(n?));
    let offen_netto = offen
        .iter()
        .filter(|p| topf(p.topf))
        .try_fold(Cent::NULL, |summe, p| summe.checked_add(p.netto()?));
    let mut erwartung = (0_i64, 0_i64, 0_i64);
    for v in eigene {
        if let Some((soll, ist)) = v.erwartung_bp.zip(
            v.netto
                .zip(v.einstand)
                .and_then(|(n, e)| rendite_bp(e, e.checked_add(n)?)),
        ) {
            erwartung.0 = erwartung.0.saturating_add(soll);
            erwartung.1 = erwartung.1.saturating_add(ist);
            erwartung.2 = erwartung.2.saturating_add(1);
        }
    }
    let geld = |wert: Option<Cent>| wert.map_or_else(|| "unbelegt".to_owned(), |c| c.to_string());
    let bp = |wert: i64| {
        wert.checked_div(erwartung.2)
            .map_or_else(|| "unbelegt".into(), |v| v.to_string())
    };
    format!(
        "Eigene Papierbilanz {wer:?} vor dem {tag}. Abgeschlossene Verkäufe {}, realisiert netto {}, offen netto {}, Euro-Mehrertrag realisiert {}. In {} Paaren Ø erwartet {} Basispunkte brutto, Ø erreicht {} Basispunkte netto.",
        reale_bilanz.map_or(0, |b| b.abgeschlossen),
        geld(reale_bilanz.map(|b| b.netto)),
        geld(offen_netto),
        geld(gegen_korb),
        erwartung.2,
        bp(erwartung.0),
        bp(erwartung.1)
    )
}

pub fn lauf<'a>(
    wurzel: &Path,
    jetzt: Zeitpunkt,
    dachs: &'a mut dyn Berater,
    maus: &'a mut dyn Berater,
) -> Result<(), String> {
    let tag = jetzt.frankfurt_tag().ok_or("Datum der Rückschau fehlt")?;
    if tag.wochentag() != Some(6) {
        return Ok(());
    }
    let mut j =
        Journal::oeffnen(&wurzel.join("daten/journal.txt")).map_err(|e| format!("{e:?}"))?;
    if j.stand().seit_abschluss() != 0 {
        return Ok(());
    }
    let zyklus = crate::tagnummer(tag).ok_or("Ungültige Tageskennung")?;
    for (wer, name, berater) in [
        (BeraterId::Dachs, "Dachs", dachs),
        (BeraterId::Maus, "Maus", maus),
    ] {
        if j.stand()
            .lehren()
            .iter()
            .any(|(datum, ki, _)| *datum == tag && ki == name)
        {
            continue;
        }
        let frage = zusammenfassung(j.stand(), wer, tag);
        j.belege_lehre(tag, name, "begonnen")
            .map_err(|e| format!("{e:?}"))?;
        let text = if berater.angebunden() {
            match j.reserviere_mit_einstellung(
                wer,
                tag,
                zyklus,
                KiZweck::Wochenrueckschau,
                berater.einstellung(),
            ) {
                Ok(erlaubnis) => {
                    let antwort = berater.wochenrat(erlaubnis, &frage);
                    crate::lauf::belege_ki(&mut j, berater).map_err(|e| format!("{e:?}"))?;
                    antwort.map_or_else(
                        |_| "übersprungen: keine verwertbaren Lehren".into(),
                        |lehren| formatiere_lehren(&lehren),
                    )
                }
                Err(sperre) => format!("übersprungen: KI-Kostenrahmen ({sperre:?})"),
            }
        } else {
            "übersprungen: KI nicht angebunden".into()
        };
        j.belege_lehre(tag, name, &text)
            .map_err(|e| format!("{e:?}"))?;
    }
    Ok(())
}

fn formatiere_lehren(lehren: &[String]) -> String {
    let mut zeilen = lehren
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .take(5)
        .peekable();
    if zeilen.peek().is_none() {
        return "übersprungen: keine verwertbaren Lehren".into();
    }
    let lehrsaetze = zeilen
        .map(|l| {
            l.chars()
                .take(120)
                .map(|c| if c.is_control() || c == '|' { ' ' } else { c })
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    format!("Lehren: {}", lehrsaetze.join("|"))
}

pub(crate) fn sichtbare_lehren(stand: &Stand, tag: Datum) -> Vec<String> {
    let Some(samstag) = (1..=7)
        .filter_map(|n: i64| tag.plus_tage(n.checked_neg()?))
        .find(|d| d.wochentag() == Some(6))
    else {
        return Vec::new();
    };
    let tage: Vec<_> = stand
        .lehren()
        .iter()
        .filter_map(|(datum, wer, inhalt)| {
            (*datum == samstag)
                .then(|| (wer, inhalt.strip_prefix("Lehren: ")))
                .and_then(|(wer, l)| Some((wer, l?)))
        })
        .collect();
    (0..5)
        .flat_map(|nr| {
            tage.iter().filter_map(move |(wer, lehren)| {
                let lehre = lehren.split('|').nth(nr)?;
                Some(format!(
                    "Frühere Wochenlehre {wer} (KI-Auswertung, kein Quellenbeleg): {lehre}"
                ))
            })
        })
        .take(5)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Antwort, Lage, Urteil};
    use depot_journal::AufrufErlaubnis;

    #[derive(Default)]
    struct Probe(usize);

    impl Berater for Probe {
        fn wochenrat(&mut self, _: AufrufErlaubnis, bilanz: &str) -> Result<Vec<String>, String> {
            self.0 = self.0.saturating_add(1);
            assert!(bilanz.contains("Eigene Papierbilanz"));
            Ok((1..=7).map(|n| format!("Lehre {n}")).collect())
        }

        fn vorschlagen(&mut self, _: AufrufErlaubnis, _: &Lage<'_>) -> Antwort {
            unreachable!("Wochenrückschau fragt keine Handelsziele")
        }

        fn pruefen(
            &mut self,
            _: AufrufErlaubnis,
            _: &Lage<'_>,
            _: &depot_kern::Vorschlag,
        ) -> Urteil {
            unreachable!("Wochenrückschau fragt keine Zweitmeinung")
        }
    }

    #[test]
    fn rueckschau_einmal_am_samstag_maximal_fuenf_lehren_und_budget_geprueft() -> Result<(), String>
    {
        let ordner = std::env::temp_dir().join(format!("depot-rueckschau-{}", std::process::id()));
        let daten = ordner.join("daten");
        std::fs::create_dir_all(&daten).map_err(|e| e.to_string())?;
        let probe = || -> Result<(), String> {
            let pfad = daten.join("journal.txt");
            Journal::einrichten(&pfad).map_err(|e| format!("{e:?}"))?;
            let samstag = Datum::neu(2026, 11, 7).ok_or("Samstag")?;
            let freitag = samstag.plus_tage(-1).ok_or("Freitag")?;
            let zeit = |tag| Zeitpunkt::frankfurt(tag, 12, 0, 0).ok_or("Zeit");
            let zyklus = crate::tagnummer(samstag).ok_or("Zyklus")?;
            let mut j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
            while j
                .reserviere_fuer(BeraterId::Maus, samstag, zyklus, KiZweck::Wochenrueckschau)
                .is_ok()
            {}
            drop(j);
            let (mut dachs, mut maus) = (Probe::default(), Probe::default());
            lauf(&ordner, zeit(freitag)?, &mut dachs, &mut maus)?;
            assert_eq!((dachs.0, maus.0), (0, 0));
            lauf(&ordner, zeit(samstag)?, &mut dachs, &mut maus)?;
            lauf(&ordner, zeit(samstag)?, &mut dachs, &mut maus)?;
            assert_eq!((dachs.0, maus.0), (1, 0));
            let j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
            assert_eq!(j.stand().seit_abschluss(), 0, "kein Papiertag eröffnet");
            assert!(
                j.stand()
                    .lehren()
                    .iter()
                    .any(|(_, wer, text)| wer == "Maus" && text.contains("KI-Kostenrahmen"))
            );
            let montag = samstag.plus_tage(2).ok_or("Montag")?;
            let lehren = sichtbare_lehren(j.stand(), montag);
            assert_eq!(lehren.len(), 5);
            Ok(())
        }();
        std::fs::remove_dir_all(ordner).map_err(|e| e.to_string())?;
        probe
    }
}
