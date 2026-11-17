//! Leerlauf im Exportzeitraum (#311 M0): Bargeld, Füllquoten und Verzug je Topf und Woche.

use std::fmt::Write as _;

use depot_journal::Stand;
use depot_lage::ausfuehrung::OrderZeile;
use depot_lage::depotverlauf::Tagespunkt;
use depot_lage::leerlauf::{
    bargeld_je_woche, bargeldtage, quote_je_anlass, quote_je_stufe, verzuege, wochenbeginn,
};

use super::{Zeitraum, prozent};

pub(super) fn bericht(
    stand: &Stand,
    buch: &[OrderZeile],
    punkte: &[Tagespunkt],
    zeitraum: Zeitraum,
) -> String {
    let mut text = String::from(
        "\n## Leerlauf je Topf und Woche\n\nNur lesend aus Journal und Ausführungsbuch. Woche = Montag der Kalenderwoche. Gezählt werden nur entschiedene Orders (gefüllt oder verfallen).\n",
    );
    let wochen_im_zeitraum = |woche: depot_kern::Datum| {
        wochenbeginn(zeitraum.von).is_some_and(|von| woche >= von) && woche <= zeitraum.bis
    };

    text.push_str("\n### Bargeld\n\n| Topf | Woche | Handelstage | Euro-Tage in Bargeld | mittlere Bargeldquote |\n|---|---|---:|---:|---:|\n");
    let tage: Vec<_> = bargeldtage(stand, punkte)
        .into_iter()
        .filter(|t| (zeitraum.von..=zeitraum.bis).contains(&t.tag))
        .collect();
    let bargeld = bargeld_je_woche(&tage);
    if bargeld.is_empty() {
        text.push_str("| – | – | – | – | – |\n");
    }
    for ((topf, woche), w) in &bargeld {
        let _ = writeln!(
            text,
            "| {topf:?} | {woche} | {} | {} | {} |",
            w.handelstage,
            w.euro_tage(),
            prozent(w.quote_bp)
        );
    }

    text.push_str("\n### Füllquote je Anlass\n\n| Topf | Woche | Richtung | Anlass | gefüllt | verfallen | Quote |\n|---|---|---|---|---:|---:|---:|\n");
    let mut leer = true;
    for ((topf, woche, richtung, anlass), q) in quote_je_anlass(buch)
        .into_iter()
        .filter(|((_, woche, _, _), _)| wochen_im_zeitraum(*woche))
    {
        leer = false;
        let _ = writeln!(
            text,
            "| {topf:?} | {woche} | {richtung} | {anlass} | {} | {} | {} |",
            q.gefuellt,
            q.verfallen,
            quote(q.prozent())
        );
    }
    if leer {
        text.push_str("| – | – | – | – | – | – | – |\n");
    }

    text.push_str("\n### Füllquote je Limitabstand\n\n| Topf | Woche | Richtung | Abstand | gefüllt | verfallen | Quote |\n|---|---|---|---|---:|---:|---:|\n");
    let mut leer = true;
    for ((topf, woche, richtung, stufe), q) in quote_je_stufe(buch)
        .into_iter()
        .filter(|((_, woche, _, _), _)| wochen_im_zeitraum(*woche))
    {
        leer = false;
        let _ = writeln!(
            text,
            "| {topf:?} | {woche} | {richtung} | {} | {} | {} | {} |",
            stufe.text(),
            q.gefuellt,
            q.verfallen,
            quote(q.prozent())
        );
    }
    if leer {
        text.push_str("| – | – | – | – | – | – | – |\n");
    }

    text.push_str("\n### Verzug vom Verkauf bis zum nächsten Kauf\n\nHandelstage nach dem gefüllten Verkauf bis zum nächsten gefüllten Kauf im selben Topf. Ohne Folgekauf zählt bis zum Ende des Zeitraums.\n\n| Topf | Verkauf | gefüllt am | Art | nächster Kauf | Handelstage |\n|---|---:|---|---|---|---:|\n");
    let mut leer = true;
    for v in verzuege(buch, zeitraum.bis)
        .into_iter()
        .filter(|v| (zeitraum.von..=zeitraum.bis).contains(&v.verkauft_am))
    {
        leer = false;
        let kauf = v.kauf.map_or_else(
            || "noch keiner".into(),
            |(nr, tag)| format!("#{nr} am {tag}"),
        );
        let _ = writeln!(
            text,
            "| {:?} | #{} | {} | {} | {kauf} | {} |",
            v.topf,
            v.verkauf,
            v.verkauft_am,
            v.herkunft.text(),
            v.handelstage
        );
    }
    if leer {
        text.push_str("| – | – | – | – | – | – |\n");
    }
    vetoschatten(stand, punkte, zeitraum, &mut text);
    text
}

/// Punkt 5: Was ein abgelehnter Kauf bis zum Ende des Zeitraums netto gebracht hätte.
fn vetoschatten(stand: &Stand, punkte: &[Tagespunkt], zeitraum: Zeitraum, text: &mut String) {
    text.push_str("\n### Schatten der Vetos\n\nAbgelehnte Käufe der Erstmeinung, gerechnet wie im Papierkonto: Einstieg zum Schluss des Vetotags mit Modellspread, Ausstieg zum jüngsten belegten Schluss im Zeitraum mit Modellspread, beide Gebühren. Datengrundlage für die offene Frage zum halben Kauf; gebaut ist diese Regel nicht.\n\n| Vetotag | Topf | ISIN | Zielgewicht | Stück | Einstieg | Ausstieg | netto | bei halber Größe |\n|---|---|---|---:|---:|---:|---|---:|---:|\n");
    let zeilen: Vec<_> = depot_lage::vetoschatten::schatten(stand, punkte, zeitraum.bis)
        .into_iter()
        .filter(|s| (zeitraum.von..=zeitraum.bis).contains(&s.veto.tag))
        .collect();
    if zeilen.is_empty() {
        text.push_str("| – | – | – | – | – | – | – | – | – |\n");
    }
    let geld = |c: Option<depot_kern::Cent>| c.map_or_else(|| "–".into(), |c| c.to_string());
    for s in zeilen {
        let ausstieg = s
            .ausstieg
            .map_or_else(|| "–".into(), |(tag, kurs)| format!("{kurs} am {tag}"));
        let _ = writeln!(
            text,
            "| {} | {:?} | {} | {} | {} | {} | {ausstieg} | {} | {} |",
            s.veto.tag,
            s.veto.topf,
            s.veto.isin,
            prozent(Some(s.veto.ziel_bp)),
            s.stueck,
            s.einstieg,
            geld(s.netto),
            geld(s.netto_halb)
        );
    }
}

fn quote(prozent: Option<u32>) -> String {
    prozent.map_or_else(|| "–".into(), |p| format!("{p} %"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_journal::AuftragsZustand;
    use depot_kern::{Cent, Datum, Isin, Preis, TopfId, Zeitpunkt};
    use depot_lage::ausfuehrung::{Anlass, Richtung};

    fn zeile(
        nr: u64,
        richtung: Richtung,
        anlass: Anlass,
        zustand: AuftragsZustand,
        tag: Datum,
    ) -> Result<OrderZeile, String> {
        Ok(OrderZeile {
            nr,
            topf: TopfId::B,
            isin: Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?,
            richtung,
            stueck: 1,
            versand: Zeitpunkt::frankfurt(tag, 7, 30, 0),
            frist: None,
            zustand,
            bezug: Preis::aus_cent(Cent(10_000)),
            limit: Cent(9_700),
            limit_bp: Some(-300),
            fuellpreis: None,
            fuellpreis_bp: None,
            modellspread_bp: None,
            schluss: None,
            schluss_bp: None,
            anlass,
            ausgefuehrt_am: (zustand == AuftragsZustand::Gefuellt).then_some(tag),
        })
    }

    #[test]
    fn bericht_zeigt_quoten_und_verzug_im_zeitraum() -> Result<(), String> {
        let montag = Datum::neu(2026, 11, 16).ok_or("Montag")?;
        let mittwoch = Datum::neu(2026, 11, 18).ok_or("Mittwoch")?;
        let buch = [
            zeile(
                7,
                Richtung::Verkauf,
                Anlass::Stopp,
                AuftragsZustand::Gefuellt,
                montag,
            )?,
            zeile(
                8,
                Richtung::Kauf,
                Anlass::Umschichtung,
                AuftragsZustand::Gefuellt,
                mittwoch,
            )?,
        ];
        let zeitraum = Zeitraum {
            von: montag,
            bis: mittwoch,
        };
        let text = bericht(&Stand::default(), &buch, &[], zeitraum);
        assert!(text.contains("## Leerlauf je Topf und Woche"));
        assert!(text.contains("### Schatten der Vetos"));
        assert!(text.contains("| B | 2026-11-16 | Verkauf | Stopp | 1 | 0 | 100 % |"));
        assert!(text.contains("| B | 2026-11-16 | Verkauf | 201–300 bp | 1 | 0 | 100 % |"));
        assert!(text.contains("| B | #7 | 2026-11-16 | erzwungen | #8 am 2026-11-18 | 2 |"));
        // Ausserhalb des Zeitraums erscheint nichts.
        let spaeter = Zeitraum {
            von: Datum::neu(2026, 11, 30).ok_or("Tag")?,
            bis: Datum::neu(2026, 12, 4).ok_or("Tag")?,
        };
        let text = bericht(&Stand::default(), &buch, &[], spaeter);
        assert!(!text.contains("#7"));
        Ok(())
    }
}
