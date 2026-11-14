//! Kurzbefund über die belegten Daten des gewählten Exportzeitraums.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use depot_journal::Stand;
use depot_kern::{Datum, TopfId};

use super::Zeitraum;

pub(super) fn bericht(stand: Option<&Stand>, zeitraum: Zeitraum) -> String {
    let mut text = format!("# Prüfbericht · {}\n\n", zeitraum.titel());
    let in_zeitraum = |tag| (zeitraum.von..=zeitraum.bis).contains(&tag);
    let handelstage = zeitraum
        .tage()
        .filter(|tag| depot_kern::liste::handelstag(*tag))
        .count();
    let mut nichthandeln = String::new();
    let mut vollstaendig = false;
    let mut entscheidungen = String::new();
    if let Some(stand) = stand {
        let abschluesse = stand
            .tagesschluesse()
            .iter()
            .filter(|(tag, _)| in_zeitraum(*tag))
            .count();
        let buchungen = stand
            .ki()
            .buchungen()
            .iter()
            .filter(|b| in_zeitraum(b.datum))
            .count();
        let versendet = stand
            .auftraege()
            .iter()
            .filter(|a| a.versand.is_some_and(|t| zeitraum.enthaelt(t)))
            .count();
        let fuellungen = stand
            .ausfuehrungsfolge()
            .filter(|(_, f)| f.kurszeit.is_some_and(|t| zeitraum.enthaelt(t)))
            .count();
        let _ = writeln!(
            text,
            "- Belegte Tagesabschlüsse: {abschluesse} bei {handelstage} Kalendertagen mit Börsenhandel im Zeitraum.\n- KI-Buchungen: {buchungen}.\n- Orderversand: {versendet}. Zeitlich zugeordnete Ausführungen: {fuellungen}.\n"
        );
        if versendet == 0 && handelstage != 0 {
            text.push_str("**Auffällig:** Im Zeitraum ist kein Orderversand belegt. Gründe und Beleglücken stehen unter Nicht-Handeln.\n\n");
        }
        (nichthandeln, vollstaendig, entscheidungen) = entscheidungsbericht(stand, zeitraum);
    } else {
        text.push_str(
            "**Rot: Journal nicht bestätigt.** Kennzahlen zu Handel, Kosten und Depot fehlen.\n\n",
        );
    }
    text.push_str("| Prüffrage | Ampel | Belege und Grenzen |\n|---|---|---|\n");
    for (frage, belege) in [
        (
            "Ablauf",
            "05_betrieb_und_fehler.md enthält den vorhandenen Dienstverlauf.",
        ),
        (
            "Fehler",
            "05_betrieb_und_fehler.md enthält vorhandene Fehler, Hinweise und Anbieterstatus.",
        ),
        (
            "Entscheidungen",
            "Der Abschnitt Entscheidungen zeigt beide Runden und den Orderversand. Entscheidungsgrundlagen und Gemini-Beiträge stehen in 02_ki_gespraeche.md.",
        ),
        (
            "Nicht-Handeln",
            "Die Tabelle darunter ordnet Entscheidungsbelege aus 03_journal.md nach Tag und Topf zu.",
        ),
        (
            "Orders",
            "04_orders_und_depot.md und ausfuehrungen.csv enthalten bekannte Orders und Füllungen.",
        ),
        (
            "Depot und Leistung",
            "Tageswerte, Vergleich und realisierte Ergebnisse stehen im Abschnitt Depot und Leistung. Die Messlatte steht aus.",
        ),
        (
            "Kosten",
            "Die Auswertung darunter zeigt gebuchte Kosten, Token und Lücken in den Belegen.",
        ),
        (
            "Nachher-Blick",
            "Die Auswertung späterer Kurse steht noch aus.",
        ),
    ] {
        let ampel = match frage {
            "Nicht-Handeln" if vollstaendig => "Grün",
            "Nicht-Handeln" => "Rot (Belege fehlen)",
            "Entscheidungen" if stand.is_none() => "Rot (Journal fehlt)",
            "Nachher-Blick" => "Rot (noch nicht ausgewertet)",
            "Ablauf" | "Fehler" => "Gelb",
            _ if stand.is_none() => "Rot",
            _ => "Gelb",
        };
        let _ = writeln!(text, "| {frage} | {ampel} | {belege} |");
    }
    text.push_str(&entscheidungen);
    text.push_str(&nichthandeln);
    text
}

#[derive(Default)]
struct Entscheidung<'a> {
    vorschlag: Vec<&'a str>,
    zweitmeinung: Vec<&'a str>,
    runde_eins: Vec<&'a str>,
    runde_zwei: Vec<&'a str>,
    regelwerk: Vec<&'a str>,
    zweitmeinung_ausgefallen: bool,
    versendet: Vec<u64>,
}

fn zelle(belege: &[&str]) -> String {
    if belege.is_empty() {
        return "Kein Beleg".into();
    }
    belege
        .join(" · ")
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "&#124;")
}

fn entscheidungsbelege(
    stand: &Stand,
    zeitraum: Zeitraum,
) -> BTreeMap<(Datum, TopfId), Entscheidung<'_>> {
    let mut tage = BTreeMap::<(Datum, TopfId), Entscheidung<'_>>::new();
    for (tag, wer, grund) in stand.begruendungen() {
        if !(zeitraum.von..=zeitraum.bis).contains(tag) {
            continue;
        }
        let (topf, grund) = if let Some(grund) = grund.strip_prefix("Topf A: ") {
            (TopfId::A, grund)
        } else if let Some(grund) = grund.strip_prefix("Topf B: ") {
            (TopfId::B, grund)
        } else if grund.starts_with("Abstimmung Runde ") && (wer == "Dachs" || wer == "Maus") {
            let topf = if wer == "Dachs" { TopfId::A } else { TopfId::B };
            let eintrag = tage.entry((*tag, topf)).or_default();
            if let Some(text) = grund.strip_prefix("Abstimmung Runde 1: ") {
                eintrag.runde_eins.push(text);
            } else if let Some(text) = grund.strip_prefix("Abstimmung Runde 2: ") {
                eintrag.runde_zwei.push(text);
            }
            continue;
        } else {
            continue;
        };
        let eintrag = tage.entry((*tag, topf)).or_default();
        if grund.starts_with("Antwort verworfen, kein Urteil:") {
            eintrag.zweitmeinung_ausgefallen = true;
            eintrag.zweitmeinung.push(grund);
        } else if grund.starts_with("Zustimmung:") || grund.starts_with("Veto:") {
            eintrag.zweitmeinung.push(grund);
        } else if wer == "Regelwerk" {
            eintrag.regelwerk.push(grund);
        } else {
            eintrag.vorschlag.push(grund);
        }
    }
    for auftrag in stand.auftraege() {
        if let Some((tag, ..)) = auftrag
            .versand
            .and_then(depot_kern::Zeitpunkt::in_frankfurt)
            && (zeitraum.von..=zeitraum.bis).contains(&tag)
        {
            let eintrag = tage.entry((tag, auftrag.beleg.topf)).or_default();
            eintrag.versendet.push(auftrag.beleg.nr);
        }
    }

    tage
}

fn entscheidungsbericht(stand: &Stand, zeitraum: Zeitraum) -> (String, bool, String) {
    let mut tage = entscheidungsbelege(stand, zeitraum);
    let mut text = String::from(
        "\n## Nicht-Handeln\n\n| Tag | Topf | Vorschlag | Abstimmung | Regelwerk | Order |\n|---|---|---|---|---|---|\n",
    );
    let mut entscheidungen = String::from(
        "\n## Entscheidungen\n\nDie Zweitmeinung und die beiden Abstimmungsrunden sind getrennt belegt. Die letzte Spalte nennt datierte Versandbelege. Ausführungen stehen unter Orders. Fehlende Runden bleiben als Beleglücke sichtbar.\n\n| Tag | Topf | Vorschlag | Zweitmeinung | Runde 1 | Runde 2 | Orderversand |\n|---|---|---|---|---|---|---|\n",
    );
    let mut vollstaendig = true;
    for tag in zeitraum.tage() {
        if !depot_kern::liste::handelstag(tag) {
            let _ = writeln!(text, "| {tag} | – | Börsenfrei | – | – | – |");
            continue;
        }
        for (topf, name) in [(TopfId::A, "A"), (TopfId::B, "B")] {
            let mut eintrag = tage.remove(&(tag, topf)).unwrap_or_default();
            if eintrag.zweitmeinung_ausgefallen {
                for grund in &mut eintrag.regelwerk {
                    if *grund == "KeinVorschlag" {
                        *grund = "ZweitmeinungAusgefallen";
                    }
                }
            }
            let abgeschlossen = stand.schlussbeleg(tag).is_some();
            let abstimmung: Vec<_> = eintrag
                .zweitmeinung
                .iter()
                .copied()
                .chain(eintrag.runde_eins.iter().copied())
                .chain(eintrag.runde_zwei.iter().copied())
                .collect();
            let erklaert = !eintrag.vorschlag.is_empty()
                && (!abstimmung.is_empty() || !eintrag.regelwerk.is_empty())
                && abgeschlossen;
            vollstaendig &= erklaert;
            let order = match (eintrag.versendet.len(), abgeschlossen) {
                (0, true) => "Kein Versand".into(),
                (0, false) => "Kein Versand belegt, Tagesabschluss fehlt".into(),
                (n, _) => format!("{n} versendet"),
            };
            let orderbelege = if eintrag.versendet.is_empty() {
                "Kein datierter Versand belegt".into()
            } else {
                let nummern = eintrag
                    .versendet
                    .iter()
                    .map(|nr| format!("#{nr}"))
                    .collect::<Vec<_>>();
                format!("Versendet: {}", nummern.join(", "))
            };
            let _ = writeln!(
                entscheidungen,
                "| {tag} | {name} | {} | {} | {} | {} | {orderbelege} |",
                zelle(&eintrag.vorschlag),
                zelle(&eintrag.zweitmeinung),
                zelle(&eintrag.runde_eins),
                zelle(&eintrag.runde_zwei)
            );
            let _ = writeln!(
                text,
                "| {tag} | {name} | {} | {} | {} | {order} |",
                zelle(&eintrag.vorschlag),
                zelle(&abstimmung),
                zelle(&eintrag.regelwerk),
            );
        }
    }
    (text, vollstaendig, entscheidungen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nicht_bestaetigtes_journal_ist_rot() -> Result<(), String> {
        let tag = depot_kern::Datum::neu(2026, 11, 12).ok_or("Datum")?;
        let text = bericht(None, Zeitraum { von: tag, bis: tag });
        assert!(text.contains("Journal nicht bestätigt"));
        assert!(text.contains("| Orders | Rot |"));
        assert!(!text.contains("KI-Buchungen: 0"));
        Ok(())
    }

    #[test]
    fn wochenbericht_unterscheidet_ausfall_und_bargeld() -> Result<(), String> {
        use depot_journal::Journal;
        use depot_kern::Cent;

        let ordner =
            std::env::temp_dir().join(format!("depot-pruefbericht-{}", crate::neues_token()));
        std::fs::create_dir(&ordner).map_err(|e| e.to_string())?;
        let pfad = ordner.join("journal.txt");
        let fehler = |e| format!("{e:?}");
        Journal::einrichten(&pfad).map_err(fehler)?;
        let mut journal = Journal::oeffnen(&pfad).map_err(fehler)?;
        let tag = |t| Datum::neu(2030, 1, t).ok_or("Datum");
        let von = tag(7)?;
        let bis = tag(9)?;
        for t in [von, tag(8)?, bis] {
            let gruende = if t == von {
                [
                    (
                        "Dachs",
                        "Topf A: Antwort verwendet: Beispielwert: Zielgewicht 35 %.",
                    ),
                    (
                        "Maus",
                        "Topf A: Antwort verworfen, kein Urteil: Testanbieter: simuliertes HTTP 429",
                    ),
                    ("Regelwerk", "Topf A: KeinVorschlag"),
                    (
                        "Maus",
                        "Topf B: Antwort verworfen: Testanbieter: simuliertes HTTP 429",
                    ),
                    ("Regelwerk", "Topf B: KeinVorschlag"),
                ]
            } else {
                [
                    (
                        "Dachs",
                        "Topf A: Antwort verwendet: Beispielentscheidung: Bargeld behalten.",
                    ),
                    (
                        "Maus",
                        "Topf B: Antwort verwendet: Beispielentscheidung: Bargeld behalten.",
                    ),
                    ("Regelwerk", "Topf A: KeinVorschlag"),
                    (
                        "Dachs",
                        "Abstimmung Runde 2: Grund: Beispielentscheidung: Bargeld behalten.",
                    ),
                    (
                        "Maus",
                        "Abstimmung Runde 2: Grund: Beispielentscheidung: Bargeld behalten.",
                    ),
                ]
            };
            if t != von {
                journal
                    .begruende(
                        t,
                        "Dachs",
                        "Abstimmung Runde 1: Grund: Erfundene Erstabwägung.",
                    )
                    .map_err(fehler)?;
            }
            for (wer, grund) in gruende {
                journal.begruende(t, wer, grund).map_err(fehler)?;
            }
            journal
                .schliesse_ab(t, Cent(500_000), &"0".repeat(64))
                .map_err(fehler)?;
        }
        let (bytes, vermerk) = journal_mit_versand(&journal)?;
        drop(journal);
        std::fs::write(&pfad, bytes).map_err(|e| e.to_string())?;
        std::fs::write(pfad.with_extension("vermerk"), vermerk).map_err(|e| e.to_string())?;
        let journal = Journal::oeffnen(&pfad).map_err(fehler)?;
        let text = bericht(Some(journal.stand()), Zeitraum { von, bis });
        pruefe_entscheidungen(&text)?;
        let mit_luecke = bericht(Some(journal.stand()), Zeitraum { von: tag(4)?, bis });
        drop(journal);
        std::fs::remove_dir_all(ordner).map_err(|e| e.to_string())?;

        assert!(text.contains("| Nicht-Handeln | Grün |"));
        assert!(mit_luecke.contains("| Nicht-Handeln | Rot (Belege fehlen) |"));
        assert!(text.lines().any(|zeile| {
            zeile.starts_with("| 2030-01-07 | A |")
                && zeile.contains("35 %")
                && zeile.contains("ZweitmeinungAusgefallen | Kein Versand")
        }));
        for (t, topf) in [(tag(8)?, "A"), (tag(8)?, "B"), (bis, "B")] {
            assert!(text.lines().any(|zeile| {
                zeile.starts_with(&format!("| {t} | {topf} |"))
                    && zeile.contains("Bargeld behalten")
                    && zeile.ends_with("| Kein Versand |")
            }));
        }
        Ok(())
    }

    fn journal_mit_versand(journal: &depot_journal::Journal) -> Result<(Vec<u8>, String), String> {
        // Eine spätere Ausführung ist hier nicht belegt; Versand ist ein eigener Nachweis.
        let eintraege = [
            (depot_journal::Art::Topf, "A;500000;"),
            (
                depot_journal::Art::Auftrag,
                "1;A;DE0007164600;Kauf;1;1000;Umschichtung",
            ),
            (
                depot_journal::Art::Zustand,
                "1;gesendet;2030-01-09T10:00:00Z;2030-01-09T16:30:00Z",
            ),
        ];
        let mut testjournal: Vec<_> = journal
            .eintraege()
            .iter()
            .map(|(art, text)| (*art, text.as_str()))
            .collect();
        testjournal.extend(eintraege);
        let bytes = depot_journal::inhalt_fuer(&testjournal);
        let vermerk = depot_journal::vermerk_fuer(&bytes, 0).ok_or("Testvermerk")?;
        Ok((bytes, vermerk))
    }

    fn pruefe_entscheidungen(text: &str) -> Result<(), String> {
        let entscheidungen = text
            .split_once("## Entscheidungen")
            .ok_or("Entscheidungen fehlen")?
            .1
            .split_once("## Nicht-Handeln")
            .ok_or("Abschnittsgrenze fehlt")?
            .0;
        assert!(
            entscheidungen
                .lines()
                .any(|zeile| zeile.starts_with("| 2030-01-08 | A |")
                    && zeile.contains("Erfundene Erstabwägung. | Grund: Beispielentscheidung:")
                    && zeile.ends_with("Kein datierter Versand belegt |"))
        );
        assert!(
            entscheidungen
                .lines()
                .any(|zeile| zeile.starts_with("| 2030-01-08 | B |")
                    && zeile.contains("| Kein Beleg | Grund: Beispielentscheidung:"))
        );
        assert!(
            entscheidungen
                .lines()
                .any(|zeile| zeile.starts_with("| 2030-01-09 | A |")
                    && zeile.ends_with("| Versendet: #1 |"))
        );
        Ok(())
    }

    #[test]
    fn pruefbericht_meldet_kein_orderversand() -> Result<(), String> {
        use depot_journal::Journal;
        use depot_kern::{BeraterId, Datum};

        let ordner =
            std::env::temp_dir().join(format!("depot-pruefbericht-{}", crate::neues_token()));
        std::fs::create_dir(&ordner).map_err(|e| e.to_string())?;
        let pfad = ordner.join("journal.txt");
        let journalfehler = |e| format!("{e:?}");
        let von = Datum::neu(2026, 11, 2).ok_or("Beginn der Woche")?;
        let bis = Datum::neu(2026, 11, 6).ok_or("Ende des Prüfzeitraums")?;
        let davor = Datum::neu(2026, 10, 30).ok_or("Tag vor dem Prüfzeitraum")?;

        Journal::einrichten(&pfad).map_err(journalfehler)?;
        let mut journal = Journal::oeffnen(&pfad).map_err(journalfehler)?;
        for datum in [davor, von] {
            journal
                .reserviere(BeraterId::Dachs, datum, 1)
                .map_err(|e| format!("{e:?}"))?;
        }
        drop(journal);
        let text =
            Journal::pruefe_lesend(&pfad, |stand| bericht(Some(stand), Zeitraum { von, bis }))
                .map_err(journalfehler)?;
        std::fs::remove_dir_all(ordner).map_err(|e| e.to_string())?;

        assert!(text.contains("**Auffällig:** Im Zeitraum ist kein Orderversand belegt."));
        assert!(text.contains("KI-Buchungen: 1."));
        assert!(text.contains("Orderversand: 0."));
        assert!(text.contains("| Nicht-Handeln | Rot (Belege fehlen) |"));
        assert!(text.contains("| Nachher-Blick | Rot (noch nicht ausgewertet) |"));
        assert!(text.contains("| Orders | Gelb |"));
        Ok(())
    }
}
