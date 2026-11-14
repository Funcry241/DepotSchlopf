//! Tagesendbestände und die vor dem Abschluss belegten Kurse für den Analyse-Export.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use depot_journal::{KursBeleg, Schlussbeleg, Stand};
use depot_kern::{Cent, Datum, Herkunft, Isin, Kurse, Zeitpunkt};
use depot_lage::datum_text;

use super::{Zeitraum, geld};

pub(super) struct Historie {
    pub(super) ueberblick: String,
    pub(super) belege: String,
    pub(super) depotwerte: BTreeMap<Datum, Cent>,
}

pub(super) fn bericht(zeitraum: Zeitraum, stand: Option<&Stand>, luecken: &mut String) -> Historie {
    let mut ueberblick = String::from(
        "## Historische Tagesübersicht\n\nJe Tag der Stand am belegten Abschluss. Die Bewertung nutzt nur Quellenkurse desselben Handelstags, die bis zum Frankfurter Tagesende vorlagen und vor dem Abschluss im Journal stehen. Fehlende Kurse lassen den Gesamtwert offen.\n\n| Tag | Journalabschluss | Bargeld | Depotwert |\n|---|---|---:|---:|\n",
    );
    let mut belege = String::from(
        "# Journalbelege zur Tagesübersicht\n\nAbschluss und Bestände stammen aus der geprüften Journal-Kette. Der Abschlussabdruck bezeichnet die Vorgeschichte. Die Kursabdrücke sind die im Journal gespeicherten SHA-256-Prüfsummen der Quellenantworten.\n",
    );
    let mut abschluss_belegt = false;
    let mut depotwerte = BTreeMap::new();
    for tag in zeitraum.tage() {
        let datum = datum_text(tag);
        let Some(schluss) = stand.and_then(|s| s.schlussbeleg(tag)) else {
            let status = if stand.is_some() {
                "kein Abschluss belegt"
            } else {
                "Journal nicht bestätigt"
            };
            let _ = writeln!(ueberblick, "| {datum} | {status} | – | – |");
            continue;
        };

        abschluss_belegt = true;
        let toepfe = schluss.toepfe;
        let positionen: BTreeSet<_> = toepfe
            .values()
            .flat_map(|t| t.positionen().keys())
            .collect();
        let ende = Zeitpunkt::frankfurt(tag, 23, 59, 59);
        let mut verwendete = BTreeMap::new();
        for b in schluss.kurse {
            if b.kurs().handelstag() == tag
                && b.kurs().herkunft() == Herkunft::Quelle
                && ende.is_some_and(|ende| b.abruf() <= ende)
                && positionen.contains(b.isin())
            {
                verwendete.insert(b.isin(), b);
            }
        }
        let kurse = ende.and_then(|ende| {
            Kurse::neu(
                ende,
                verwendete
                    .values()
                    .map(|b| (b.isin().clone(), b.kurs().clone()))
                    .collect(),
            )
            .ok()
        });
        let bargeld = (!toepfe.is_empty())
            .then(|| {
                toepfe
                    .values()
                    .try_fold(Cent::NULL, |summe, t| summe.checked_add(t.bargeld()))
            })
            .flatten();
        let depotwert = bargeld.and_then(|_| {
            toepfe.values().try_fold(Cent::NULL, |summe, t| {
                let wert = if t.positionen().is_empty() {
                    t.bargeld()
                } else {
                    t.wert(kurse.as_ref()?)?
                };
                summe.checked_add(wert)
            })
        });
        let _ = writeln!(
            ueberblick,
            "| {datum} | belegt | {} | {} |",
            geld(bargeld),
            geld(depotwert)
        );
        if let Some(wert) = depotwert {
            depotwerte.insert(tag, wert);
        } else {
            let _ = writeln!(
                luecken,
                "- Historischer Depotwert am {datum} nicht belegbar. Siehe Journalbelege."
            );
        }

        schreibe_belege(&mut belege, &datum, &schluss, positionen, &verwendete);
    }
    if stand.is_none() {
        belege.push_str("\nJournal-Kette nicht bestätigt. Tagesbelege fehlen.\n");
    } else if !abschluss_belegt {
        belege.push_str("\nIm Zeitraum ist kein Journalabschluss belegt.\n");
    }
    Historie {
        ueberblick,
        belege,
        depotwerte,
    }
}

fn schreibe_belege(
    belege: &mut String,
    datum: &str,
    schluss: &Schlussbeleg<'_>,
    positionen: BTreeSet<&Isin>,
    verwendete: &BTreeMap<&Isin, &KursBeleg>,
) {
    let _ = writeln!(
        belege,
        "\n## {datum}\n\n- Abschlussabdruck: `{}`\n- Belegter Höchststand: {}",
        schluss.abschluss.abdruck, schluss.abschluss.hoechststand
    );
    if schluss.toepfe.is_empty() {
        belege.push_str("- Keine Topfbestände belegt.\n");
    }
    for (id, topf) in schluss.toepfe {
        let _ = writeln!(belege, "- Topf {id:?}: {} Bargeld", topf.bargeld());
        for (isin, stueck) in topf.positionen() {
            let _ = writeln!(belege, "  - {isin}: {stueck} Stück");
        }
    }
    for isin in positionen {
        if let Some(b) = verwendete.get(isin) {
            let _ = writeln!(
                belege,
                "- {isin}: {} EUR je Stück · Quelle {} · Kurszeit (UTC) {} · Abruf (UTC) {} · Antwortabdruck `{}`",
                b.kurs().preis().euro_text(),
                b.quelle(),
                b.kurs().zeitpunkt(),
                b.abruf(),
                b.roh()
            );
        } else {
            let _ = writeln!(belege, "- {isin}: kein passender Quellenkurs belegt.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_journal::{Art, Journal, inhalt_fuer, vermerk_fuer};
    use std::fs;

    #[test]
    fn tagesuebersicht_verwendet_nur_belege_am_abschluss() -> Result<(), String> {
        let abschlussabdruck = "ab".repeat(32);
        let roh = "bc".repeat(32);
        let spaeter = "cd".repeat(32);
        let erster_kurs = format!(
            "stooq;quelle;2026-11-02T17:00:00Z;{roh};DE0007164600;1234;2026-11-02T16:35:00Z"
        );
        let nachtrag = format!(
            "stooq;quelle;2026-11-02T18:00:00Z;{spaeter};DE0007164600;9000;2026-11-02T16:35:00Z"
        );
        let simuliert = format!(
            "stooq;simulation;2026-11-04T17:00:00Z;{spaeter};DE0007164600;8000;2026-11-04T16:35:00Z"
        );
        let erster_schluss = format!("2026-11-02;200000;{abschlussabdruck}");
        let zweiter_schluss = format!("2026-11-04;200000;{abschlussabdruck}");
        let dritter_schluss = format!("2026-11-05;200000;{abschlussabdruck}");
        let journal = inhalt_fuer(&[
            (Art::Topf, "A;100000;DE0007164600:2"),
            (Art::Kurs, &erster_kurs),
            (Art::Abschluss, &erster_schluss),
            (Art::Kurs, &nachtrag),
            (Art::Topf, "A;110000;DE0007164600:2"),
            (Art::Kurs, &simuliert),
            (Art::Abschluss, &zweiter_schluss),
            (Art::Topf, "A;120000;"),
            (Art::Abschluss, &dritter_schluss),
            (Art::Topf, "A;130000;"),
        ]);
        let ordner =
            std::env::temp_dir().join(format!("depot_analyse_historie_{}", std::process::id()));
        fs::create_dir_all(&ordner).map_err(|e| e.to_string())?;
        let pfad = ordner.join("journal.txt");
        let vermerk = vermerk_fuer(&journal, 0).ok_or("Testvermerk fehlt")?;
        fs::write(&pfad, &journal).map_err(|e| e.to_string())?;
        fs::write(pfad.with_extension("vermerk"), vermerk).map_err(|e| e.to_string())?;
        let zeitraum = Zeitraum {
            von: Datum::lies("2026-11-02").ok_or("Von")?,
            bis: Datum::lies("2026-11-06").ok_or("Bis")?,
        };
        let mut luecken = String::new();
        let gelesen =
            Journal::pruefe_lesend(&pfad, |stand| bericht(zeitraum, Some(stand), &mut luecken));
        fs::remove_dir_all(ordner).map_err(|e| e.to_string())?;
        let Historie {
            ueberblick, belege, ..
        } = gelesen.map_err(|e| format!("{e:?}"))?;
        for (tag, werte) in [
            ("2026-11-02", "belegt | 1.000,00 € | 1.024,68 €"),
            ("2026-11-03", "kein Abschluss belegt | – | –"),
            ("2026-11-04", "belegt | 1.100,00 € | nicht belegt"),
            ("2026-11-05", "belegt | 1.200,00 € | 1.200,00 €"),
            ("2026-11-06", "kein Abschluss belegt | – | –"),
        ] {
            let tag = datum_text(Datum::lies(tag).ok_or("Tag")?);
            assert!(
                ueberblick.contains(&format!("| {tag} | {werte} |")),
                "{ueberblick}"
            );
        }
        assert!(belege.contains(&abschlussabdruck));
        assert!(belege.contains(&roh));
        assert!(belege.contains("kein passender Quellenkurs belegt"));
        assert!(!belege.contains(&spaeter));
        assert!(!belege.contains("1.300,00"));
        assert!(luecken.contains("Historischer Depotwert"));

        let unbestaetigt = bericht(zeitraum, None, &mut String::new());
        assert_eq!(
            unbestaetigt
                .ueberblick
                .matches("Journal nicht bestätigt | – | –")
                .count(),
            5
        );
        assert!(!unbestaetigt.belege.contains(&abschlussabdruck));
        Ok(())
    }
}
