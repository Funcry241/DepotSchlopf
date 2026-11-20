//! Zahlenpaare aus historischer These und belegtem Fragekandidaten für die lokale Modellvorbereitung.

use std::{collections::BTreeSet, fmt::Write as _, ops::RangeInclusive};

use depot_journal::Stand;
use depot_kern::{BeraterId, Datum, abdruck::sha256_hex, liste};
use depot_sim::belegrang::{Grundrang, Sortierer, eindeutige_frageliste, thesen_aus};

const GRENZE: &str = "{\"status\":\"Größenlimit erreicht, kürzeren Exportzeitraum wählen\"}\n";
/// Reihenfolge und ganzzahlige Skala gehören zum Merkmalsabdruck.
const SCHEMA: &str = "grundrang-paar-v1|wertbezug:0/1|termin:0/1|fragenplatz:ab1";

/// Ein Kopf sowie Eingabe- oder Statuszeilen. Die unabhängige Relevanzbewertung steht noch aus.
pub fn eingaben(
    stand: &Stand,
    original: &str,
    zeitraum: RangeInclusive<Datum>,
    limit: usize,
) -> String {
    let kopf = format!(
        "{{\"format\":\"neuro2026-vorbereitung-v1\",\"schema\":\"{SCHEMA}\",\"merkmale_sha\":\"{}\",\"journal_sha\":\"{}\",\"journal_bytes\":{},\"von\":\"{}\",\"bis\":\"{}\"}}\n",
        sha256_hex(SCHEMA.as_bytes()),
        sha256_hex(original.as_bytes()),
        original.len(),
        zeitraum.start(),
        zeitraum.end()
    );
    let mut aus = kopf.clone();
    let Ok(werte) = liste::beobachtungsliste() else {
        return kopf + "{\"status\":\"Werteliste fehlt\"}\n";
    };
    let universum = liste::als_universum(&werte);
    let rang = Grundrang::neu(&werte);
    let tage: BTreeSet<Datum> = stand
        .beobachtungen()
        .iter()
        .map(|(d, _)| *d)
        .chain(stand.tage_mit_wissensstichtag())
        .chain(
            stand
                .begruendungen()
                .iter()
                .filter(|(_, wer, _)| wer == "Frageliste")
                .map(|(d, _, _)| *d),
        )
        .filter(|tag| zeitraum.contains(tag))
        .collect();
    if tage.is_empty() {
        aus.push_str("{\"status\":\"Keine Fragekandidaten im Zeitraum belegt\"}\n");
    }
    for tag in tage {
        if aus.len() > limit {
            return kopf + GRENZE;
        }
        let Some((zeit, vorher)) = stand
            .wissensstichtag(tag)
            .zip(stand.begruendungen_bis_stichtag(tag))
        else {
            let _ = writeln!(
                aus,
                "{{\"tag\":\"{tag}\",\"status\":\"Wissensstichtag fehlt\"}}"
            );
            continue;
        };
        let kandidaten = match eindeutige_frageliste(stand, tag) {
            Ok(liste) if !liste.is_empty() => liste,
            Ok(_) => {
                let _ = writeln!(
                    aus,
                    "{{\"tag\":\"{tag}\",\"status\":\"Frageabschnitt leer\"}}"
                );
                continue;
            }
            Err(grund) => {
                let _ = writeln!(aus, "{{\"tag\":\"{tag}\",\"status\":\"{grund}\"}}");
                continue;
            }
        };
        let referenzen: Vec<_> = kandidaten
            .iter()
            .map(|text| sha256_hex(text.as_bytes()))
            .collect();
        for wer in [BeraterId::Dachs, BeraterId::Maus] {
            let thesen = thesen_aus(vorher, &universum, wer, tag);
            if thesen.is_empty() {
                let _ = writeln!(
                    aus,
                    "{{\"tag\":\"{tag}\",\"berater\":\"{wer:?}\",\"status\":\"Keine belegte Vorschlagsthese\"}}"
                );
            }
            for these in thesen {
                let isin = these.isin.als_str();
                for (rangindex, i) in rang
                    .ordne(&kandidaten, std::slice::from_ref(&these))
                    .into_iter()
                    .enumerate()
                {
                    let Some((text, hash)) = kandidaten.get(i).zip(referenzen.get(i)) else {
                        continue;
                    };
                    let [bezug, termin, platz] =
                        rang.paarmerkmale(text, &these, i.saturating_add(1));
                    let _ = writeln!(
                        aus,
                        "{{\"tag\":\"{tag}\",\"stichtag\":\"{zeit}\",\"berater\":\"{wer:?}\",\"isin\":\"{isin}\",\"ref\":\"{tag}#{platz}:{hash}\",\"x\":[{bezug},{termin},{platz}],\"grundrang\":{}}}",
                        rangindex.saturating_add(1)
                    );
                    if aus.len() > limit {
                        return kopf + GRENZE;
                    }
                }
            }
        }
    }
    if aus.len() > limit {
        kopf + GRENZE
    } else {
        aus
    }
}
