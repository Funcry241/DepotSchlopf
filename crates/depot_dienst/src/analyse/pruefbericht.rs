//! Kurzbefund über die belegten Daten des gewählten Exportzeitraums.

use std::fmt::Write as _;

use depot_journal::Stand;

use super::Zeitraum;

pub(super) fn bericht(stand: Option<&Stand>, zeitraum: Zeitraum) -> String {
    let mut text = format!("# Prüfbericht · {}\n\n", zeitraum.titel());
    let in_zeitraum = |tag| (zeitraum.von..=zeitraum.bis).contains(&tag);
    let handelstage = zeitraum
        .tage()
        .filter(|tag| depot_kern::liste::handelstag(*tag))
        .count();
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
            text.push_str("**Auffällig:** Im Zeitraum ist kein Orderversand belegt. Die Ursachen sind noch nicht ausgewertet.\n\n");
        }
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
            "02_ki_gespraeche.md enthält die im Zeitraum belegten Gespräche.",
        ),
        (
            "Nicht-Handeln",
            "03_journal.md enthält vorhandene Entscheidungsbelege. Die Zuordnung je Topf und Richtung fehlt noch.",
        ),
        (
            "Orders",
            "04_orders_und_depot.md und ausfuehrungen.csv enthalten bekannte Orders und Füllungen.",
        ),
        (
            "Depot und Leistung",
            "01_ueberblick.md enthält den Depotstand und belegte Tageswerte.",
        ),
        (
            "Kosten",
            "02_ki_gespraeche.md und 03_journal.md enthalten KI-Buchungen und verfügbare Abrechnungen.",
        ),
        (
            "Nachher-Blick",
            "Die Auswertung späterer Kurse steht noch aus.",
        ),
    ] {
        let ampel = match frage {
            "Nicht-Handeln" | "Nachher-Blick" => "Rot (noch nicht ausgewertet)",
            "Ablauf" | "Fehler" => "Gelb",
            _ if stand.is_none() => "Rot",
            _ => "Gelb",
        };
        let _ = writeln!(text, "| {frage} | {ampel} | {belege} |");
    }
    text
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
        assert!(text.contains("| Nicht-Handeln | Rot (noch nicht ausgewertet) |"));
        assert!(text.contains("| Nachher-Blick | Rot (noch nicht ausgewertet) |"));
        assert!(text.contains("| Orders | Gelb |"));
        Ok(())
    }
}
