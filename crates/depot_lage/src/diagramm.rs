//! Inline-SVG aus belegten Tagespunkten. Fehlende Werte unterbrechen die Linie.

use std::fmt::Write as _;

use depot_kern::{Cent, Preis};

use crate::depotverlauf::Tagespunkt;
use crate::{datum_text, html::maskiere};

const SCHRITTE: [i128; 7] = [10_000, 20_000, 50_000, 100_000, 200_000, 500_000, 1_000_000];

struct Achse {
    unten: i128,
    oben: i128,
    schritt: i128,
}

impl Achse {
    fn aus_punkten(punkte: &[Tagespunkt], startgeld: Cent) -> Self {
        let start = i128::from(startgeld.0);
        let (min, max) = punkte
            .iter()
            .flat_map(|p| [p.gesamt, p.wertpapiere, p.bargeld])
            .flatten()
            .map(|wert| i128::from(wert.0))
            .fold((start, start), |(min, max), wert| {
                (min.min(wert), max.max(wert))
            });
        let runde = |schritt: i128| {
            let unten = min.div_euclid(schritt).saturating_mul(schritt);
            let oben = max.div_euclid(schritt).saturating_mul(schritt);
            let oben = if oben < max {
                oben.saturating_add(schritt)
            } else {
                oben
            };
            if unten == oben {
                Self {
                    unten: unten.saturating_sub(schritt),
                    oben: oben.saturating_add(schritt),
                    schritt,
                }
            } else {
                Self {
                    unten,
                    oben,
                    schritt,
                }
            }
        };
        for schritt in SCHRITTE {
            let achse = runde(schritt);
            if achse.oben.saturating_sub(achse.unten) <= schritt.saturating_mul(6) {
                return achse;
            }
        }
        // Auch lange Reihen mit großen Ausschlägen behalten höchstens sechs Schritte.
        let mut faktor = 10_i128;
        loop {
            for grund in [10_000_i128, 20_000, 50_000] {
                let schritt = grund.saturating_mul(faktor);
                let achse = runde(schritt);
                if achse.oben.saturating_sub(achse.unten) <= schritt.saturating_mul(6) {
                    return achse;
                }
            }
            faktor = faktor.saturating_mul(10);
        }
    }

    fn y(&self, wert: i128) -> i128 {
        let hoehe = wert.saturating_sub(self.unten).saturating_mul(240);
        260_i128.saturating_sub(hoehe.div_euclid(self.oben.saturating_sub(self.unten)))
    }
}

fn x(index: usize, anzahl: usize) -> usize {
    index
        .saturating_mul(640)
        .checked_div(anzahl.saturating_sub(1))
        .map_or(380, |abstand| 60_usize.saturating_add(abstand))
}

fn geld(wert: Cent) -> String {
    Preis::aus_cent(wert).map_or_else(|| "–".into(), |p| format!("{} €", p.euro_text()))
}

fn linie(
    text: &mut String,
    punkte: &[Tagespunkt],
    achse: &Achse,
    art: &str,
    stil: &str,
    wert: impl Fn(&Tagespunkt) -> Option<Cent>,
) {
    let mut offen = false;
    for (index, punkt) in punkte.iter().enumerate() {
        if let Some(wert) = wert(punkt) {
            if !offen {
                let _ = write!(text, "<polyline class=\"{art}\" {stil} points=\"");
                offen = true;
            }
            let _ = write!(
                text,
                "{},{} ",
                x(index, punkte.len()),
                achse.y(i128::from(wert.0))
            );
        } else if offen {
            text.push_str("\"/>");
            offen = false;
        }
    }
    if offen {
        text.push_str("\"/>");
    }
}

fn achsenbeschriftung(text: &mut String, punkte: &[Tagespunkt], achse: &Achse) {
    let mut wert = achse.unten;
    while wert <= achse.oben {
        let y = achse.y(wert);
        let _ = write!(
            text,
            "<line x1=\"60\" x2=\"700\" y1=\"{y}\" y2=\"{y}\" stroke=\"currentColor\" opacity=\"0.12\"/>"
        );
        if let Ok(cent) = i64::try_from(wert) {
            let preis = Preis::aus_cent(Cent(cent)).map_or_else(|| "–".into(), Preis::euro_text);
            let _ = write!(
                text,
                "<text x=\"56\" y=\"{y}\" fill=\"currentColor\" text-anchor=\"end\" font-size=\"10\">{} €</text>",
                maskiere(preis.trim_end_matches(",00"))
            );
        }
        wert = wert.saturating_add(achse.schritt);
    }
    let letzter = punkte.len().saturating_sub(1);
    let beschriftungen = letzter.min(5);
    for schritt in 0..=beschriftungen {
        let index = schritt
            .saturating_mul(letzter)
            .checked_div(beschriftungen)
            .unwrap_or(0);
        if let Some(punkt) = punkte.get(index) {
            let datum = format!("{:02}.{:02}.", punkt.tag.tag(), punkt.tag.monat());
            let _ = write!(
                text,
                "<text x=\"{}\" y=\"280\" fill=\"currentColor\" text-anchor=\"middle\" font-size=\"11\">{}</text>",
                x(index, punkte.len()),
                maskiere(&datum)
            );
        }
    }
}

/// Zeichnet Gesamtwert, Wertpapiere, Bargeld und das belegte Startgeld.
pub fn svg(punkte: &[Tagespunkt], startgeld: Cent) -> String {
    svg_mit_startgeld(punkte, Some(startgeld))
}

/// Zeichnet auch dann den belegten Verlauf, wenn das Startgeld fehlt.
pub fn svg_mit_startgeld(punkte: &[Tagespunkt], startgeld: Option<Cent>) -> String {
    if punkte.is_empty() {
        return String::new();
    }
    let achsenbasis = startgeld.or_else(|| {
        punkte
            .iter()
            .find_map(|p| p.gesamt.or(p.wertpapiere).or(p.bargeld))
    });
    let Some(achsenbasis) = achsenbasis else {
        return String::new();
    };
    let achse = Achse::aus_punkten(punkte, achsenbasis);
    let mut werte = punkte
        .iter()
        .filter_map(|punkt| Some((punkt.tag, punkt.gesamt?)));
    let beschreibung = werte.next().map_or_else(
        || "Kein belegter Gesamtwert.".into(),
        |(erst, beginn)| {
            let (letzt, ende) = werte.next_back().unwrap_or((erst, beginn));
            format!(
                "Gesamtwert von {} am {} auf {} am {}.",
                geld(beginn),
                datum_text(erst),
                geld(ende),
                datum_text(letzt)
            )
        },
    );
    let mut text = format!(
        "<svg viewBox=\"0 0 720 300\" width=\"100%\" role=\"img\" aria-labelledby=\"depot-diagramm-titel depot-diagramm-beschreibung\">\
        <title id=\"depot-diagramm-titel\">Entwicklung des Papierdepots</title>\
        <desc id=\"depot-diagramm-beschreibung\">{}</desc>",
        maskiere(&beschreibung)
    );
    achsenbeschriftung(&mut text, punkte, &achse);
    if let Some(startgeld) = startgeld {
        let start_y = achse.y(i128::from(startgeld.0));
        let _ = write!(
            text,
            "<line x1=\"60\" x2=\"700\" y1=\"{start_y}\" y2=\"{start_y}\" style=\"stroke:var(--leise)\" stroke-dasharray=\"8 5\"/>"
        );
    }
    linie(
        &mut text,
        punkte,
        &achse,
        "wertpapiere",
        "fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-dasharray=\"6 4\"",
        |p| p.wertpapiere,
    );
    linie(
        &mut text,
        punkte,
        &achse,
        "bargeld",
        "fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-dasharray=\"2 4\"",
        |p| p.bargeld,
    );
    linie(
        &mut text,
        punkte,
        &achse,
        "gesamt",
        "fill=\"none\" style=\"stroke:var(--nav-akzent)\" stroke-width=\"3\"",
        |p| p.gesamt,
    );
    if punkte.len() <= 120 {
        for (index, punkt) in punkte.iter().enumerate() {
            if let Some(wert) = punkt.gesamt {
                let _ = write!(
                    text,
                    "<circle cx=\"{}\" cy=\"{}\" r=\"3\" style=\"fill:var(--nav-akzent)\"><title>{}: {}</title></circle>",
                    x(index, punkte.len()),
                    achse.y(i128::from(wert.0)),
                    maskiere(&datum_text(punkt.tag)),
                    maskiere(&geld(wert))
                );
            }
        }
    }
    text.push_str("</svg>");
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_kern::Datum;

    fn punkte(werte: &[Option<i64>]) -> Result<Vec<Tagespunkt>, String> {
        let beginn = Datum::neu(2026, 11, 3).ok_or("Datum")?;
        werte
            .iter()
            .enumerate()
            .map(|(index, wert)| {
                let tage = i64::try_from(index).map_err(|e| e.to_string())?;
                Ok(Tagespunkt {
                    tag: beginn.plus_tage(tage).ok_or("Tag")?,
                    wertpapiere: None,
                    bargeld: None,
                    gesamt: wert.map(|euro| Cent(euro.saturating_mul(100))),
                    grund: None,
                    kurstage: Vec::new(),
                    kursbezug: std::collections::BTreeMap::new(),
                    ersatz: Vec::new(),
                    topfwerte: std::collections::BTreeMap::new(),
                })
            })
            .collect()
    }

    #[test]
    fn achse_schritt() -> Result<(), String> {
        let punkte = punkte(&[Some(10_000), Some(10_500), Some(9_500)])?;
        let achse = Achse::aus_punkten(&punkte, Cent(1_000_000));
        assert_eq!(
            (achse.unten, achse.oben, achse.schritt),
            (940_000, 1_060_000, 20_000)
        );
        let svg = svg(&punkte, Cent(1_000_000));
        assert_eq!(svg.matches("text-anchor=\"end\"").count(), 7);
        Ok(())
    }

    #[test]
    fn achse_flach() -> Result<(), String> {
        let achse = Achse::aus_punkten(&punkte(&[Some(10_000); 3])?, Cent(1_000_000));
        assert_eq!((achse.unten, achse.oben), (990_000, 1_010_000));
        Ok(())
    }

    #[test]
    fn y_lage() {
        let achse = Achse {
            unten: 940_000,
            oben: 1_060_000,
            schritt: 20_000,
        };
        assert_eq!(
            [achse.y(1_060_000), achse.y(940_000), achse.y(1_000_000)],
            [20, 260, 140]
        );
    }

    #[test]
    fn x_lage() {
        assert_eq!([x(0, 3), x(1, 3), x(2, 3), x(0, 1)], [60, 380, 700, 380]);
    }

    #[test]
    fn luecke_trennt_linie() -> Result<(), String> {
        let mut punkte = punkte(&[Some(10_000), Some(10_020), None, Some(10_050), Some(10_100)])?;
        for punkt in &mut punkte {
            punkt.bargeld = Some(Cent(500_000));
        }
        let svg = svg(&punkte, Cent(1_000_000));
        assert_eq!(svg.matches("<polyline class=\"gesamt\"").count(), 2);
        assert_eq!(svg.matches("<polyline class=\"bargeld\"").count(), 1);
        Ok(())
    }

    #[test]
    fn tooltip_bis_120() -> Result<(), String> {
        for (anzahl, erwartet) in [(120, 120), (121, 0)] {
            let svg = svg(&punkte(&vec![Some(10_000); anzahl])?, Cent(1_000_000));
            assert_eq!(svg.matches("<circle").count(), erwartet);
            assert_eq!(svg.matches("<title>").count(), erwartet);
        }
        Ok(())
    }

    #[test]
    fn svg_ohne_skript() -> Result<(), String> {
        let svg = svg(&punkte(&[Some(10_000)])?, Cent(1_000_000));
        assert!(!svg.contains("<script") && !svg.contains(" on") && !svg.contains("href"));
        Ok(())
    }

    #[test]
    fn svg_hat_titel() -> Result<(), String> {
        let svg = svg(&punkte(&[Some(10_000), Some(10_231)])?, Cent(1_000_000));
        for text in [
            "role=\"img\"",
            "Entwicklung des Papierdepots</title>",
            "10.000,00 € am 03.11.2026 auf 10.231,00 € am 04.11.2026.",
        ] {
            assert!(svg.contains(text), "{text}");
        }
        Ok(())
    }
}
