//! Anzeige der lokal erzeugten Neuro2026-Rangvergleiche.
use std::fmt::Write as _;
use std::path::Path;

use depot_kern::Datum;
use depot_lage::{datum_text, html::maskiere};

use crate::neuromodell::{Lernkurve, ModellBericht, Rangbeleg, Ranggruppe, ansicht, lernstand};

pub(super) fn html(html: &mut String, daten: &Path, heute: Option<Datum>) {
    html.push_str("<section aria-labelledby=\"depotmodell-titel\"><h3 id=\"depotmodell-titel\">Neuro2026 · Depotmodell</h3>");
    html.push_str("<p>Neuro2026 bewertet, wie gut Nachrichten und Termine zu einer Depotthese passen. Im lokalen Modelllauf ordnet es historische Belege nach ihrem Modellwert und vergleicht die Rangfolge mit der bisherigen Sortierung.</p>");
    if let Some(heute) = heute {
        lernstand_html(html, daten, heute);
    }
    match ansicht(daten) {
        Ok(Some(bericht)) => bericht_html(html, &bericht),
        Ok(None) => html.push_str("<p>Wartet auf die erste lokale Modellauswertung.</p>"),
        Err(_) => html.push_str("<p>Der lokale Modellbericht ist unlesbar.</p>"),
    }
    html.push_str("</section>");
}

fn lernstand_html(html: &mut String, daten: &Path, heute: Datum) {
    let stand = match lernstand(daten, heute) {
        Ok(Some(stand)) => stand,
        Ok(None) => {
            html.push_str("<p>Für den ersten Modelllauf fehlen bisher belegte historische These-Beleg-Paare.</p>");
            return;
        }
        Err(fehler) => {
            let _ = write!(
                html,
                "<p>Trainingsstand gerade nicht verfügbar. {}</p>",
                maskiere(&fehler)
            );
            return;
        }
    };
    let _ = write!(
        html,
        "<p><strong>KI-Übereinstimmungen fürs Training bis {}</strong><br>{} mit Belegbezug · {} ohne Bezug</p>",
        datum_text(stand.train_bis),
        stand.mit_bezug,
        stand.ohne_bezug,
    );
    let hinweis = match (stand.mit_bezug, stand.ohne_bezug, stand.prueffaelle) {
        (0, 0, _) => "Es fehlen KI-Übereinstimmungen mit und ohne Belegbezug.",
        (0, _, _) => "Es fehlt eine KI-Übereinstimmung mit Belegbezug.",
        (_, 0, _) => "Es fehlt eine KI-Übereinstimmung ohne Belegbezug.",
        (_, _, 0) => {
            "Es fehlen spätere Prüffälle mit drei vollen Handelstagen Abstand zum Training."
        }
        _ => "Die Fälle für den lokalen Modelllauf liegen vor.",
    };
    let _ = write!(html, "<p>{hinweis}</p>");
    if stand.prueffaelle > 0 {
        let _ = write!(
            html,
            "<p class=\"leise\">Zeitraum für den lokalen Lauf {} bis {} · {} spätere Prüffälle, davon {} mit KI-Übereinstimmung</p>",
            datum_text(stand.von),
            datum_text(stand.bis),
            stand.prueffaelle,
            stand.pruefurteile,
        );
    }
}

fn bericht_html(html: &mut String, bericht: &ModellBericht) {
    let _ = write!(
        html,
        "<p>{}</p><p class=\"leise\">{} These-Beleg-Paare · {} {} · {} Trainingstage · {} Prüftage</p>",
        maskiere(&bericht.stand),
        bericht.paare,
        bericht.urteile,
        if bericht.ki_konsens {
            "KI-Übereinstimmungen"
        } else {
            "Blindurteile"
        },
        bericht.trainingstage,
        bericht.prueftage,
    );
    let Some(version) = &bericht.modellversion else {
        return;
    };
    let _ = write!(
        html,
        "<p><strong>Neuro2026-Kern {}</strong> · Beobachtungsbetrieb</p>",
        maskiere(version)
    );
    if let Some(kurve) = &bericht.lernkurve {
        lernkurve_html(html, kurve);
    } else {
        html.push_str("<p>Für diesen Modellbericht liegt keine Lernkurve vor. Ein neuer lokaler Modelllauf erfasst sie.</p>");
    }
    if let Some(abdruck) = &bericht.modellabdruck {
        let _ = write!(
            html,
            "<details><summary>Modellabdruck</summary><pre>{}</pre></details>",
            maskiere(abdruck),
        );
    }
    for (nr, gruppe) in bericht.gruppen.iter().enumerate() {
        gruppe_html(html, gruppe, nr == 0);
    }
}

fn lernkurve_html(html: &mut String, kurve: &Lernkurve) {
    let _ = write!(
        html,
        r#"<div class="neuro-uebersicht" data-depotmodell="{}">
        <section class="neuro-topologie"><h4>Netz des Depotmodells</h4>
        <p data-schichten></p>
        <svg viewBox="0 0 480 215" role="img" aria-label="Schichten des trainierten Depotmodells"></svg>
        <p>Vier Eingaben erfassen den Wertbezug, den Terminbezug, gemeinsame Thesenwörter und die Aktualität. Der Ausgang liefert einen Sortierwert pro Beleg.</p></section>
        <section class="neuro-kurve"><h4>Lernkurve des Depotmodells</h4>
        <p data-trainingsfehler></p>
        <svg viewBox="0 0 600 210" role="img" aria-label="Trainingsfehler über aufeinanderfolgende Epochen"></svg>
        <p>Die Linie zeigt den mittleren quadratischen Trainingsfehler je Epoche. Der Rangvergleich mit späteren Belegen steht im Modellbericht.</p></section></div>"#,
        maskiere(&kurve.json())
    );
}

fn gruppe_html(html: &mut String, gruppe: &Ranggruppe, aufgeklappt: bool) {
    let offen = if aufgeklappt { " open" } else { "" };
    let datum = Datum::lies(&gruppe.tag);
    let tag = datum.map_or_else(|| gruppe.tag.clone(), datum_text);
    let _ = write!(
        html,
        "<details{offen}><summary>Prüftag {} · {} · {} · {} angezeigte Belege</summary>\
         <p><strong>Damals offene These</strong><br>{}</p>",
        maskiere(&tag),
        maskiere(&gruppe.berater),
        maskiere(&gruppe.isin),
        gruppe.belege.len(),
        maskiere(&gruppe.these),
    );
    if let Some(datum) = datum.filter(|_| gruppe.belege.len() > 3) {
        let _ = write!(
            html,
            "<p><a href=\"/neuroschlopf/belegpruefung?tag={datum}&amp;vergleich=ja\">Prüftag blind vergleichen</a></p>",
        );
    }
    html.push_str(
        "<div class=\"breit\"><table>\
         <thead><tr><th scope=\"col\">Beleg</th><th class=\"z\" scope=\"col\">Neuro2026-Rang</th>\
         <th class=\"z\" scope=\"col\">Grundrang</th><th class=\"z\" scope=\"col\">Modellwert</th></tr></thead>\
         <tbody>",
    );
    for beleg in &gruppe.belege {
        beleg_html(html, beleg);
    }
    html.push_str("</tbody></table></div></details>");
}

fn beleg_html(html: &mut String, beleg: &Rangbeleg) {
    let [wertbezug, termin, ueberschneidung, aktualitaet] = beleg.merkmale;
    let _ = write!(
        html,
        "<tr><td>{}<br><span class=\"leise\">{}</span>\
         <details><summary>Modelleingaben</summary>\
         <dl><dt>Wertbezug im Beleg</dt><dd>{}</dd>\
         <dt>Beleg ist ein Termin</dt><dd>{}</dd>\
         <dt>Wortüberschneidung mit der These</dt><dd>{}</dd>\
         <dt>Aktualität am Wissensstichtag</dt><dd>{}</dd>\
         <dt>Belegreferenz</dt><dd><code>{}</code></dd></dl>\
         </details></td><td class=\"z\">{}</td><td class=\"z\">{}</td><td class=\"z\">{}</td></tr>",
        maskiere(&beleg.titel),
        maskiere(&beleg.quelle),
        ja_nein(wertbezug),
        ja_nein(termin),
        prozent(ueberschneidung),
        prozent(aktualitaet),
        maskiere(&beleg.referenz),
        beleg.modellrang,
        beleg.grundrang,
        maskiere(&beleg.wert.replace('.', ",")),
    );
}

fn ja_nein(wert: u16) -> &'static str {
    if wert == 0 { "Nein" } else { "Ja" }
}

fn prozent(wert: u16) -> String {
    format!("{},{} %", wert / 10, wert % 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echte_rangwerte_zeigen_beide_raenge_und_maskieren_belegtexte() {
        let bericht = ModellBericht {
            modellversion: Some("v0.1.16-ab12".into()),
            lernkurve: None,
            modellabdruck: Some("9".repeat(64)),
            stand: "Lokaler Beobachtungslauf abgeschlossen.".into(),
            paare: 9,
            urteile: 5,
            ki_konsens: false,
            trainingstage: 2,
            prueftage: 1,
            gruppen: vec![Ranggruppe {
                tag: "2026-11-20".into(),
                berater: "Maus".into(),
                isin: "DE0008430026".into(),
                these: "Rendite < 3 %".into(),
                belege: [7, 3, 2, 1]
                    .into_iter()
                    .enumerate()
                    .map(|(index, grundrang)| Rangbeleg {
                        fall_id: format!("{index:064}"),
                        referenz: format!("20#{index}:beleg"),
                        titel: "A < B".into(),
                        quelle: "Quelle & Titel".into(),
                        grundrang,
                        modellrang: index.saturating_add(1),
                        wert: "0.789".into(),
                        merkmale: [1000, 0, 125, 999],
                    })
                    .collect(),
            }],
        };
        let mut html = String::new();
        bericht_html(&mut html, &bericht);
        assert!(html.contains("Neuro2026-Kern v0.1.16-ab12"));
        assert!(html.contains("Prüftag 20.11.2026"));
        assert!(html.contains("tag=2026-11-20&amp;vergleich=ja"));
        assert!(html.contains(&"9".repeat(64)));
        assert!(html.contains("Rendite &lt; 3 %"));
        assert!(html.contains("A &lt; B"));
        assert!(html.contains("Quelle &amp; Titel"));
        assert!(html.contains("<td class=\"z\">1</td><td class=\"z\">7</td>"));
        assert!(html.contains("0,789"));
        assert!(html.contains("12,5 %"));
    }

    #[test]
    fn bericht_ohne_modell_zeigt_fallbestand_und_keine_rangwerte() {
        let bericht = ModellBericht {
            modellversion: None,
            modellabdruck: None,
            lernkurve: None,
            stand: "Zu wenig unabhängige Urteile.".into(),
            paare: 105,
            urteile: 1,
            ki_konsens: true,
            trainingstage: 0,
            prueftage: 1,
            gruppen: Vec::new(),
        };
        let mut html = String::new();
        bericht_html(&mut html, &bericht);
        assert!(html.contains("105 These-Beleg-Paare"));
        assert!(html.contains("Zu wenig unabhängige Urteile."));
        assert!(!html.contains("Modellwert"));
    }
}
