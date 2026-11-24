use std::fmt::Write as _;

use depot_journal::{KursBeleg, Stand};
use depot_kern::{Datum, Zeitpunkt};

use super::Zeitraum;

pub(super) fn bericht(stand: &Stand, zeitraum: Zeitraum) -> String {
    let mut text = String::from(
        "\n## Kursgrundlage des Papiertags\n\nHier stehen die als Kursgrundlage protokollierten Schlusskurse. Die Antwort-SHA-256 identifiziert die ursprüngliche Quellenantwort. Alle Zeitstempel stehen in UTC.\n",
    );
    let mut stichtag_belegt = false;
    for tag in zeitraum.tage() {
        let Some(stichtag) = stand.wissensstichtag(tag) else {
            continue;
        };
        stichtag_belegt = true;
        let belege = stand.kursgrundlage(tag);
        tagesbericht(&mut text, tag, stichtag, belege.as_deref());
    }
    if !stichtag_belegt {
        text.push_str("\nIm Zeitraum ist kein Wissensstichtag belegt.\n");
    }
    text
}

fn tagesbericht(text: &mut String, tag: Datum, stichtag: Zeitpunkt, belege: Option<&[&KursBeleg]>) {
    let _ = writeln!(text, "\n### {tag} · Wissensstichtag {stichtag}\n");
    match belege {
        None => text.push_str("Für diesen Papiertag ist keine Kursgrundlage protokolliert.\n"),
        Some([]) => {
            text.push_str("Für diesen Papiertag ist eine leere Kursgrundlage protokolliert.\n");
        }
        Some(belege) => {
            text.push_str(
                "| ISIN | Preis in EUR | Kurszeit | Abrufzeit | Quelle | Antwort-SHA-256 |\n|---|---:|---|---|---|---|\n",
            );
            for beleg in belege {
                let kurs = beleg.kurs();
                let _ = writeln!(
                    text,
                    "| {} | {} | {} | {} | {} | `{}` |",
                    beleg.isin(),
                    kurs.preis().euro_text(),
                    kurs.zeitpunkt(),
                    beleg.abruf(),
                    beleg.quelle(),
                    beleg.roh(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_journal::Journal;
    use depot_kern::{Herkunft, Isin, Kurs, Kurse, Preis};

    #[test]
    fn bericht_nutzt_protokollierten_kurs_statt_spaeterem_abruf() -> Result<(), String> {
        let tag = Datum::lies("2026-11-24").ok_or("Tag")?;
        let kurstag = tag.plus_tage(-1).ok_or("Kurstag")?;
        let kurszeit = Zeitpunkt::neu(kurstag, 20, 0, 0).ok_or("Kurszeit")?;
        let abruf = Zeitpunkt::neu(tag, 7, 0, 0).ok_or("Abrufzeit")?;
        let stichtag = Zeitpunkt::neu(tag, 8, 0, 0).ok_or("Stichtag")?;
        let spaeter = Zeitpunkt::neu(tag, 9, 0, 0).ok_or("Späterer Abruf")?;
        let isin = Isin::neu("DE0008404005").map_err(|e| format!("ISIN: {e:?}"))?;
        let preis = Preis::lies_cent_text("4115.5").ok_or("Preis")?;
        let kurs = Kurs::exakt(preis, "EUR", kurstag, kurszeit, Herkunft::Quelle)
            .map_err(|e| format!("Kurs: {e:?}"))?;
        let auswahl = Kurse::neu(abruf, vec![(isin.clone(), kurs.clone())])
            .map_err(|e| format!("Kursauswahl: {e:?}"))?;
        let beleg = KursBeleg::neu(
            "tiingo",
            abruf,
            b"Originalantwort",
            isin.clone(),
            kurs.clone(),
        )?;
        let nachtrag = KursBeleg::neu("tiingo", spaeter, b"Neue Antwort", isin, kurs)?;
        let wurzel =
            std::env::temp_dir().join(format!("depot-kursgrundlage-{}", crate::neues_token()));
        std::fs::create_dir(&wurzel).map_err(|e| e.to_string())?;
        let ergebnis = (|| {
            let pfad = wurzel.join("journal.txt");
            let fehler = |e| format!("{e:?}");
            Journal::einrichten(&pfad).map_err(fehler)?;
            let mut journal = Journal::oeffnen(&pfad).map_err(fehler)?;
            journal.belege_kurs(&beleg).map_err(fehler)?;
            journal
                .belege_wissensstichtag(tag, stichtag)
                .map_err(fehler)?;
            let zeitraum = Zeitraum { von: tag, bis: tag };
            let alt = bericht(journal.stand(), zeitraum);
            journal
                .belege_kursgrundlage(tag, &auswahl)
                .map_err(fehler)?;
            journal.belege_kurs(&nachtrag).map_err(fehler)?;
            Ok::<_, String>((alt, bericht(journal.stand(), zeitraum)))
        })();
        std::fs::remove_dir_all(&wurzel).map_err(|e| e.to_string())?;
        let (alt, text) = ergebnis?;
        assert!(alt.contains("keine Kursgrundlage protokolliert"));
        assert!(!alt.contains("| ISIN |"));
        assert!(text.contains("2026-11-24 · Wissensstichtag 2026-11-24T08:00:00Z"));
        assert!(text.contains(&format!(
            "| DE0008404005 | 41,155 | 2026-11-23T20:00:00Z | 2026-11-24T07:00:00Z | tiingo | `{}` |",
            beleg.roh()
        )));
        assert!(!text.contains("Originalantwort"));
        assert!(!text.contains(nachtrag.roh()));
        Ok(())
    }
}
