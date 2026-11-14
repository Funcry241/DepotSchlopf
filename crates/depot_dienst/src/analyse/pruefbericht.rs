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
            text.push_str("**Auffällig:** Im Zeitraum ist kein Orderversand belegt. Die Ampel zum Nicht-Handeln zeigt, wie weit die Ursachen belegt sind.\n\n");
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
        let ampel = if stand.is_none() && !matches!(frage, "Ablauf" | "Fehler") {
            "Rot"
        } else {
            "Gelb"
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
        assert!(text.contains("| Nicht-Handeln | Rot |"));
        assert!(!text.contains("KI-Buchungen: 0"));
        Ok(())
    }
}
