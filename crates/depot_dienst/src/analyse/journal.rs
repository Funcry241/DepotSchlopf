//! Zeitliche Zuordnung für den lesenden Export des bestätigten Journals.

use std::fmt::Write as _;

use depot_journal::{Art, Stand};
use depot_kern::{Datum, Zeitpunkt, json};

use super::Zeitraum;

const TEXTGRENZE: usize = 6 * 1024 * 1024;

#[derive(Default)]
pub(super) struct Auszug {
    pub(super) bericht: String,
}

fn datum(art: Art, text: &str, abrechnungstag: Option<Datum>, stand: &Stand) -> Option<Datum> {
    let feld = |n| text.split(';').nth(n);
    let tag = |n| Datum::lies(feld(n)?);
    let zeit = |n| Zeitpunkt::lies(feld(n)?);
    let frankfurt = |zeit: Zeitpunkt| zeit.in_frankfurt().map(|(tag, ..)| tag);
    match art {
        Art::Beobachtung
        | Art::Weltprognose
        | Art::Aufloesung
        | Art::Begruendung
        | Art::Abschluss
        | Art::NotbremseFreigabe
        | Art::Abrechnungstag
        | Art::PapierHalt
        | Art::Wissensstichtag => tag(0),
        Art::Ki => tag(1),
        Art::KiAbrechnung => stand
            .ki()
            .buchungen()
            .get(feld(0)?.parse::<usize>().ok()?)
            .map(|b| b.datum),
        Art::Kurs | Art::Termin => frankfurt(zeit(2)?),
        Art::Nachricht => frankfurt(zeit(1)?),
        Art::RssAbruf => frankfurt(Zeitpunkt::aus_sekunden(feld(1)?.parse().ok()?)?),
        Art::PapierFreigabe => frankfurt(zeit(4)?),
        Art::Vorbereitung => {
            let obj = json::lies(text).ok()?;
            let sekunden = obj.feld("zeit").or_else(|| obj.feld("start"))?.ganz()?;
            frankfurt(Zeitpunkt::aus_sekunden(sekunden)?)
        }
        Art::Auftrag | Art::Zustand => orderdatum(art, text, abrechnungstag),
        Art::Topf => abrechnungstag,
        Art::Format | Art::KiGrenzen => None,
    }
}

pub(super) fn orderdatum(art: Art, text: &str, abrechnungstag: Option<Datum>) -> Option<Datum> {
    let mut felder = text.split(';').skip(1);
    let zeit = match (art, felder.next()) {
        (Art::Zustand, Some("gesendet")) => felder.next(),
        (Art::Zustand, Some("ausfuehrung")) => felder.nth(3),
        _ => None,
    };
    zeit.and_then(Zeitpunkt::lies)
        .and_then(Zeitpunkt::frankfurt_tag)
        .or(abrechnungstag)
}

pub(super) fn auszug(zeitraum: Zeitraum, stand: &Stand, eintraege: &[(Art, String)]) -> Auszug {
    let mut aus = Auszug {
        bericht: String::from(
            "\n## Weitere bestätigte Journaleinträge\n\nBelege von externen Quellen sind nach ihrem Abrufdatum eingeordnet. Ausführungen ohne eigene Zeitangabe verwenden den belegten Abrechnungstag. Zeilennummern beziehen sich auf das Originaljournal.\n",
        ),
    };
    let mut undatiert = 0_usize;
    let mut abrechnungstag = None;
    for (index, (art, text)) in eintraege.iter().enumerate() {
        if *art == Art::Abrechnungstag {
            abrechnungstag = Datum::lies(text);
        }
        let tag = datum(*art, text, abrechnungstag, stand);
        if matches!(art, Art::Abschluss | Art::NotbremseFreigabe) {
            abrechnungstag = None;
        }
        let Some(tag) = tag else {
            undatiert = undatiert.saturating_add(1);
            continue;
        };
        if !(zeitraum.von..=zeitraum.bis).contains(&tag) {
            continue;
        }
        if aus
            .bericht
            .len()
            .saturating_add(text.len().saturating_add(150))
            > TEXTGRENZE
        {
            aus.bericht
                .push_str("\nWeitere Einträge fehlen wegen der Größenbegrenzung.\n");
            break;
        }
        // Ein eingerückter Block zeigt den Journaleintrag als Text, auch wenn er Markdown enthält.
        let _ = writeln!(
            aus.bericht,
            "\n**{tag} · {} · Journalzeile {}**\n\n    {text}",
            art.name(),
            index.saturating_add(2)
        );
    }
    if undatiert != 0 {
        let _ = writeln!(
            aus.bericht,
            "\nEinträge ohne belegbares Datum im geprüften Journal: {undatiert}. Diese Einträge fehlen im Auszug."
        );
    }
    aus
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordnet_abrufe_und_buchungen_ihrem_belegten_tag_zu() -> Result<(), String> {
        let tag = Datum::lies("2026-03-29").ok_or("Tag")?;
        let zeitraum = Zeitraum { von: tag, bis: tag };
        let werte = [
            (
                Art::Nachricht,
                "rss;2026-03-28T22:59:59Z;ab;zeit;Nachricht vor Mitternacht",
            ),
            (
                Art::Nachricht,
                "rss;2026-03-28T23:00:00Z;ab;zeit;Nachricht ab Mitternacht",
            ),
            (Art::Topf, "A;99999;"),
            (Art::Abrechnungstag, "2026-03-29"),
            (Art::Topf, "A;12345;"),
            (Art::Begruendung, "2026-03-30;Dachs;Nächster Tag"),
        ];
        let eintraege: Vec<_> = werte
            .iter()
            .map(|(art, text)| (*art, (*text).to_owned()))
            .collect();
        let aus = auszug(zeitraum, &Stand::default(), &eintraege);
        assert!(aus.bericht.contains("Nachricht ab Mitternacht"));
        assert!(aus.bericht.contains("Journalzeile 3"));
        assert!(aus.bericht.contains("A;12345;"));
        assert!(
            aus.bericht
                .contains("belegbares Datum im geprüften Journal: 1")
        );
        assert!(!aus.bericht.contains("A;99999;"));
        assert!(!aus.bericht.contains("vor Mitternacht"));
        assert!(!aus.bericht.contains("Nächster Tag"));
        Ok(())
    }
}
