//! Depotentwicklung und realisierte Ergebnisse im gewählten Exportzeitraum.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use depot_journal::Stand;
use depot_kern::{Cent, Datum};
use depot_lage::vergleich::{rendite_bp, vergleich_bis};

use super::{Zeitraum, geld, prozent};

pub(super) fn bericht(stand: &Stand, zeitraum: Zeitraum, werte: &BTreeMap<Datum, Cent>) -> String {
    let mut text = String::from("\n## Depot und Leistung\n\n");
    text.push_str("| Tag | Depotwert zum belegten Abschluss |\n|---|---:|\n");
    let mut luecken = false;
    for tag in zeitraum
        .tage()
        .filter(|tag| depot_kern::liste::handelstag(*tag) || werte.contains_key(tag))
    {
        let wert = werte.get(&tag).copied();
        luecken |= wert.is_none();
        let _ = writeln!(text, "| {tag} | {} |", geld(wert));
    }
    let rendite = werte
        .first_key_value()
        .zip(werte.last_key_value())
        .filter(|((anfang, _), (ende, _))| anfang < ende)
        .and_then(|((_, start), (_, schluss))| rendite_bp(*start, *schluss));
    let _ = writeln!(
        text,
        "\n- Veränderung zwischen erstem und letztem belegten Depotwert: {}.",
        prozent(rendite)
    );
    let _ = writeln!(
        text,
        "- Maximaler Rückgang vom bisherigen Höchstwert im gewählten Zeitraum: {}.",
        prozent(rueckgang(werte.values().copied()))
    );
    if luecken {
        text.push_str("- Beleglücken können einen größeren Rückgang verdecken.\n");
    }

    if let Some(v) = depot_kern::liste::beobachtungsliste()
        .ok()
        .and_then(|liste| vergleich_bis(stand, &liste, Some(zeitraum.bis)))
        .filter(|v| v.bewertet >= zeitraum.von)
    {
        let _ = writeln!(
            text,
            "\n### Vergleich seit {} bis {}\n\n| Depot | Rendite | Nach KI-Kosten |\n|---|---:|---:|",
            v.start, v.bewertet
        );
        for zeile in v.zeilen {
            let _ = writeln!(
                text,
                "| {} | {} | {} |",
                zeile.name,
                prozent(zeile.rendite_bp),
                prozent(zeile.nach_ki_bp)
            );
        }
    } else {
        text.push_str("\nVergleichsdepot im Zeitraum nicht belegt.\n");
    }
    text.push_str("\nMesslatte: Auswertung folgt mit #233.\n");
    verkaeufe(&mut text, stand, zeitraum);
    text
}

fn rueckgang(werte: impl IntoIterator<Item = Cent>) -> Option<i64> {
    let mut werte = werte.into_iter();
    let mut hoch = werte.next()?;
    let erster_folgepunkt = werte.next()?;
    std::iter::once(erster_folgepunkt)
        .chain(werte)
        .try_fold(0, |tief, wert| {
            hoch = hoch.max(wert);
            Some(tief.min(rendite_bp(hoch, wert)?))
        })
}

fn verkaeufe(text: &mut String, stand: &Stand, zeitraum: Zeitraum) {
    text.push_str("\n### Abgeschlossene Verkäufe nach Gebühren\n\nEine Order zählt einmal zum Datum ihrer letzten belegten Ausführung. Das Netto verrechnet anteiligen Einstand und Kauf- und Verkaufsgebühren. KI-Kosten stehen im Depotvergleich.\n\n");
    let verkaeufe = depot_lage::handelsergebnis::verkaeufe(stand);
    if verkaeufe.iter().any(|v| v.tag.is_none()) {
        text.push_str(
            "Verkäufe ohne belegtes Ausführungsdatum fehlen in der Zeitraum-Auswertung.\n\n",
        );
    }
    let mut anzahl = 0_u32;
    let mut gewinne = 0_u32;
    let mut tabelle = String::new();
    for v in verkaeufe {
        let Some(tag) = v
            .tag
            .filter(|tag| (zeitraum.von..=zeitraum.bis).contains(tag))
        else {
            continue;
        };
        let _ = writeln!(
            tabelle,
            "| {tag} | {} | {:?} | {} | {} |",
            v.nr,
            v.topf,
            v.isin,
            geld(v.netto)
        );
        if let Some(netto) = v.netto {
            anzahl = anzahl.saturating_add(1);
            gewinne = gewinne.saturating_add(u32::from(netto > Cent::NULL));
        }
    }
    if tabelle.is_empty() {
        text.push_str(
            "Im Zeitraum ist kein abgeschlossener Verkauf mit Ausführungsdatum belegt.\n",
        );
    } else {
        text.push_str(
            "| Letzte Ausführung | Order | Topf | ISIN | Netto |\n|---|---:|---|---|---:|\n",
        );
        text.push_str(&tabelle);
    }
    let quote = i64::from(gewinne)
        .checked_mul(10_000)
        .and_then(|n| n.checked_div(i64::from(anzahl)));
    let _ = writeln!(
        text,
        "\nTrefferquote unter Verkäufen mit belegtem Einstand: {} ({gewinne} von {anzahl} mit Gewinn).",
        prozent(quote)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rueckgang_verwendet_nur_fruehere_hoechstwerte() {
        assert_eq!(
            rueckgang([Cent(100), Cent(80), Cent(120), Cent(108)]),
            Some(-2000)
        );
        assert_eq!(rueckgang([Cent(100)]), None);
    }
}
