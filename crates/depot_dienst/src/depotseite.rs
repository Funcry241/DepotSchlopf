//! Lesbare Momentaufnahme des Papierdepots.

use std::fmt::Write as _;
use std::path::Path;

use depot_kern::{Cent, Preis, TopfId, Zeitpunkt};
use depot_lage::depotverlauf::{Tagespunkt, Zeitraum, depotverlauf, im_zeitraum};
use depot_lage::html::{maskiere, prozent};
use depot_lage::{DepotLage, Tagesergebnis, Zugang, datum_text, zeit_text};

fn geld(wert: Option<Cent>) -> String {
    wert.and_then(Preis::aus_cent)
        .map_or_else(|| "–".into(), |p| format!("{} €", p.euro_text()))
}

fn topf(id: TopfId) -> &'static str {
    match id {
        TopfId::A => "Dachs",
        TopfId::B => "Maus",
    }
}

fn anteil(wert: Option<Cent>, gesamt: Option<Cent>) -> String {
    let Some(bp) = wert
        .zip(gesamt.filter(|g| g.0 > 0))
        .and_then(|(wert, gesamt)| {
            u64::try_from(
                i128::from(wert.0)
                    .checked_mul(10_000)?
                    .checked_div(i128::from(gesamt.0))?,
            )
            .ok()
        })
    else {
        return "–".into();
    };
    format!(
        "{},{:02} % <meter min=\"0\" max=\"10000\" value=\"{}\" aria-label=\"Anteil am Depot\"></meter>",
        bp.checked_div(100).unwrap_or(0),
        bp.checked_rem(100).unwrap_or(0),
        bp.min(10_000)
    )
}

fn kennzahlen(d: &DepotLage, jetzt: Zeitpunkt, text: &mut String) {
    let tag = match d.tagesergebnis {
        Tagesergebnis::OhneKurstag => "Kursdatum fehlt".into(),
        Tagesergebnis::OhneVortag => "noch kein Vortag".into(),
        Tagesergebnis::NichtBewertbar(tag) => format!("am {} nicht bewertbar", datum_text(tag)),
        Tagesergebnis::Belegt { tag, betrag, bp } => format!(
            "am {}: {} ({})",
            datum_text(tag),
            geld(Some(betrag)),
            bp.map_or_else(|| "–".into(), prozent)
        ),
    };
    let basis = d
        .vergleich
        .as_ref()
        .and_then(|v| v.startgeld.checked_add(v.startgeld));
    let seit_start = d.gesamt.zip(basis).map_or_else(
        || "–".into(),
        |(wert, start)| {
            format!(
                "{} ({})",
                geld(wert.checked_sub(start)),
                depot_lage::vergleich::rendite_bp(start, wert).map_or_else(|| "–".into(), prozent)
            )
        },
    );
    let kurstag = d.kurse_vom.map_or_else(
        || "Schlusskurse nicht belegt".into(),
        |datum| {
            let alt = jetzt
                .frankfurt_tag()
                .and_then(|tag| crate::analyse::letzter_boersenschluss(jetzt, tag))
                .is_some_and(|tag| datum < tag);
            format!(
                "Bewertet mit Schlusskursen vom {}{}",
                datum_text(datum),
                if alt { " (veraltet)" } else { "" }
            )
        },
    );
    let _ = write!(
        text,
        "<section><h2>Auf einen Blick</h2><p>Gesamtwert <strong class=\"gross\">{}</strong></p>\
        <p>Tagesergebnis {tag} · Seit Start {seit_start}</p><p class=\"leise\">{kurstag}</p></section>",
        geld(d.gesamt)
    );
}

fn cash(d: &DepotLage, text: &mut String) -> Option<Cent> {
    let summe = |feld: fn(&depot_lage::TopfLage) -> Option<Cent>| {
        d.toepfe
            .iter()
            .try_fold(Cent::NULL, |summe, topf| summe.checked_add(feld(topf)?))
    };
    let bar = summe(|topf| Some(topf.bargeld));
    let valuta = d
        .toepfe
        .iter()
        .filter_map(|topf| topf.naechste_valuta)
        .min()
        .map_or_else(String::new, |tag| format!(" (frei ab {})", datum_text(tag)));
    let _ = write!(
        text,
        "<section><h2>Cash-Konto</h2><dl><dt>Bargeld gesamt</dt><dd>{}</dd>\
        <dt>Frei für Käufe</dt><dd>{}</dd><dt>Vorgemerkt für offene Käufe</dt><dd>{}</dd>\
        <dt>Verkaufserlös noch nicht gutgeschrieben</dt><dd>{}{valuta}</dd></dl></section>",
        geld(bar),
        geld(summe(|topf| topf.frei)),
        geld(summe(|topf| topf.vorgemerkt)),
        geld(summe(|topf| topf.nicht_gutgeschrieben)),
    );
    bar
}

fn positionen(d: &DepotLage, bar: Option<Cent>, text: &mut String) {
    let mut zeilen: Vec<_> = d
        .toepfe
        .iter()
        .flat_map(|t| t.positionen.iter().map(|p| (t.topf, p)))
        .collect();
    zeilen.sort_by_key(|(_, p)| std::cmp::Reverse(p.wert));
    text.push_str(
        "<section><h2>Wo bin ich investiert?</h2><div class=\"breit\"><table><thead><tr>\
        <th>Wertpapier</th><th>Topf</th><th>Stück</th><th>Kurs</th><th>Wert</th><th>Anteil</th>\
        <th>Einstand</th><th>Gewinn/Verlust</th></tr></thead><tbody>",
    );
    for (id, p) in zeilen {
        let kurs = p
            .kurs
            .map_or_else(|| "–".into(), |kurs| format!("{} €", kurs.euro_text()));
        let einstand = p
            .einstand
            .map_or_else(|| "nicht belegt".into(), |einstand| geld(Some(einstand)));
        let gv = p.gv.map_or_else(
            || "–".into(),
            |wert| {
                p.gv_bp.map_or_else(
                    || geld(Some(wert)),
                    |bp| format!("{} ({})", geld(Some(wert)), prozent(bp)),
                )
            },
        );
        let _ = write!(
            text,
            "<tr><td title=\"{}\">{}</td><td>{}</td><td>{}</td><td>{kurs}</td>\
            <td>{}</td><td>{}</td><td>{einstand}</td><td>{gv}</td></tr>",
            maskiere(&p.isin),
            maskiere(&p.name),
            topf(id),
            p.stueck,
            geld(p.wert),
            anteil(p.wert, d.gesamt)
        );
    }
    let _ = write!(
        text,
        "<tr><th scope=\"row\" colspan=\"4\">Bargeld</th><td>{}</td><td>{}</td><td colspan=\"2\"></td></tr>\
        </tbody></table></div></section>",
        geld(bar),
        anteil(bar, d.gesamt)
    );
}

fn orders(stand: &depot_journal::Stand, werte: &[depot_kern::liste::Wert], text: &mut String) {
    text.push_str("<section><h2>Offene Orders</h2>");
    if stand.offene_auftraege().next().is_none() {
        text.push_str("<p>Keine offenen Orders.</p></section>");
        return;
    }
    text.push_str(
        "<div class=\"breit\"><table><tr><th>Wertpapier</th><th>Richtung</th><th>Stück offen</th>\
        <th>Limit</th><th>Topf</th><th>Zustand</th></tr>",
    );
    for auftrag in stand.offene_auftraege() {
        let zeile = depot_lage::auftrag_zeile(auftrag, werte);
        let _ = write!(
            text,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            maskiere(&zeile.name),
            zeile.richtung,
            auftrag.offen_stueck(),
            geld(Some(zeile.limit)),
            topf(zeile.topf),
            maskiere(&zeile.zustand)
        );
    }
    text.push_str("</table></div></section>");
}

fn zahlen_je_tag(punkte: &[Tagespunkt], text: &mut String) {
    text.push_str("<details><summary>Zahlen je Tag</summary><div class=\"breit\"><table><thead><tr><th>Tag</th><th>Wertpapiere</th><th>Bargeld</th><th>Gesamt</th><th>Zum Vortag</th></tr></thead><tbody>");
    let mut vorher: Option<Cent> = None;
    for punkt in punkte {
        let gesamt = punkt.grund.as_ref().map_or_else(
            || geld(punkt.gesamt),
            |grund| format!("nicht bewertbar: {}", maskiere(grund)),
        );
        let aenderung = punkt
            .gesamt
            .zip(vorher)
            .and_then(|(wert, basis)| wert.checked_sub(basis));
        let _ = write!(
            text,
            "<tr><th scope=\"row\">{}</th><td>{}</td><td>{}</td><td>{gesamt}</td><td>{}</td></tr>",
            datum_text(punkt.tag),
            geld(punkt.wertpapiere),
            geld(punkt.bargeld),
            geld(aenderung)
        );
        vorher = punkt.gesamt;
    }
    text.push_str("</tbody></table></div></details>");
}

fn entwicklung(
    punkte: &[Tagespunkt],
    startgeld: Option<Cent>,
    zeitraum: Zeitraum,
    text: &mut String,
) {
    text.push_str("<section><h2>Entwicklung</h2><p aria-label=\"Zeitraum\">");
    for (auswahl, label) in [
        (Zeitraum::Monat, "1 Monat"),
        (Zeitraum::DreiMonate, "3 Monate"),
        (Zeitraum::Jahr, "1 Jahr"),
        (Zeitraum::Alles, "Alles"),
    ] {
        if auswahl != Zeitraum::Monat {
            text.push_str(" · ");
        }
        let aktiv = if auswahl == zeitraum {
            " aria-current=\"true\""
        } else {
            ""
        };
        let _ = write!(
            text,
            "<a href=\"/depot?zeitraum={}\"{aktiv}>{label}</a>",
            auswahl.als_abfrage()
        );
    }
    text.push_str("</p>");
    if punkte.is_empty() {
        text.push_str(
            "<p>Im gewählten Zeitraum gibt es noch keinen abgeschlossenen Papiertag.</p></section>",
        );
        return;
    }
    text.push_str(
        "<p>Linien: Gesamt (durchgezogen) · Wertpapiere (gestrichelt) · Bargeld (gepunktet)",
    );
    if startgeld.is_some() {
        text.push_str(" · Startgeld (dünn gestrichelt)");
    } else {
        text.push_str(" · Startgeld nicht belegbar");
    }
    text.push_str("</p>");
    text.push_str(&depot_lage::diagramm::svg_mit_startgeld(punkte, startgeld));
    zahlen_je_tag(punkte, text);
    text.push_str("</section>");
}

pub fn inhalt(daten: &Path, jetzt: Zeitpunkt, zeitraum: Zeitraum) -> String {
    let mut text = format!(
        "<section><h2>Depot</h2><p class=\"leise\">Stand {} · Seite neu laden für den neuesten Stand</p></section>",
        zeit_text(jetzt)
    );
    depot_lage::mit_journal(daten, |zugang| {
        let stand = match zugang {
            Zugang::Offen(stand) if !stand.toepfe().is_empty() => stand,
            Zugang::NichtEingerichtet | Zugang::Offen(_) => {
                text.push_str("<p>Noch kein Depot: Es entsteht mit dem ersten Papiertag.</p>");
                return;
            }
            Zugang::Belegt => {
                text.push_str("<p class=\"offen\">Journal belegt. Lade die Seite erneut, sobald der Lauf beendet ist.</p>");
                return;
            }
            Zugang::Fehler(grund) => {
                let _ = write!(text, "<p class=\"halt\">Halt: {}</p>", maskiere(grund));
                return;
            }
        };
        let Ok(werte) = depot_kern::liste::beobachtungsliste() else {
            text.push_str("<p class=\"halt\">Die Beobachtungsliste ist unlesbar.</p>");
            return;
        };
        let lage = depot_lage::depotstand(stand, &werte);
        kennzahlen(&lage, jetzt, &mut text);
        let bar = cash(&lage, &mut text);
        positionen(&lage, bar, &mut text);
        orders(stand, &werte, &mut text);
        depot_lage::html::ausfuehrungsbilanz(&lage, &mut text);
        let verlauf = depotverlauf(stand, &werte);
        let ab = jetzt.frankfurt_tag().and_then(|heute| zeitraum.ab(heute));
        let startgeld = verlauf.first().and_then(|punkt| {
            punkt.gesamt.or_else(|| {
                let (_, toepfe) = stand.tagesschluesse().first()?;
                toepfe
                    .values()
                    .all(|topf| topf.positionen().is_empty())
                    .then_some(punkt.bargeld)
                    .flatten()
            })
        });
        entwicklung(im_zeitraum(&verlauf, ab), startgeld, zeitraum, &mut text);
        text.push_str("<p class=\"leise\">Nur Papiergeld. Werte zum abgerundeten Cent, Einstand aufgerundet.</p>");
    });
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_lage::{Position, TopfLage};

    fn lage() -> DepotLage {
        DepotLage {
            kurse_vom: None,
            toepfe: vec![TopfLage {
                topf: TopfId::A,
                bargeld: Cent(500_000),
                frei: Some(Cent(499_000)),
                vorgemerkt: Some(Cent::NULL),
                nicht_gutgeschrieben: Some(Cent(1_000)),
                naechste_valuta: depot_kern::Datum::neu(2026, 11, 17),
                wert: None,
                positionen: vec![Position {
                    isin: "DE0007164600".into(),
                    name: "<b>X&Y</b>".into(),
                    stueck: 1,
                    wert: None,
                    kurs: None,
                    einstand: None,
                    gv: None,
                    gv_bp: None,
                }],
            }],
            gesamt: None,
            tagesergebnis: Tagesergebnis::OhneKurstag,
            letzter_abschluss: None,
            hoechststand: None,
            auftraege: Vec::new(),
            ausfuehrungen: Vec::new(),
            vergleich: None,
        }
    }

    #[test]
    fn depot_maskiert() {
        let mut html = String::new();
        positionen(&lage(), None, &mut html);
        assert!(html.contains("&lt;b&gt;X&amp;Y&lt;/b&gt;"));
        assert!(!html.contains("<b>X&Y</b>"));
        assert_eq!(html.matches("DE0007164600").count(), 1);
        assert!(html.contains("title=\"DE0007164600\""));
    }

    #[test]
    fn depot_cash_valuta() {
        let mut html = String::new();
        assert_eq!(cash(&lage(), &mut html), Some(Cent(500_000)));
        assert!(html.contains("10,00 € (frei ab 17.11.2026)"), "{html}");
    }

    #[test]
    fn zahlen_je_tag_markiert_fehlende_bewertung() -> Result<(), String> {
        let mut html = String::new();
        let punkt = Tagespunkt {
            tag: depot_kern::Datum::neu(2026, 11, 13).ok_or("Datum")?,
            wertpapiere: None,
            bargeld: Some(Cent(900_000)),
            gesamt: None,
            grund: Some("Kurs fehlt für <Siemens>".into()),
            kurstage: Vec::new(),
            kursbezug: std::collections::BTreeMap::new(),
            ersatz: Vec::new(),
            topfwerte: std::collections::BTreeMap::new(),
        };
        zahlen_je_tag(&[punkt], &mut html);
        assert!(html.contains("nicht bewertbar: Kurs fehlt für &lt;Siemens&gt;"));
        assert!(html.contains("<td>9.000,00 €</td>"));
        Ok(())
    }
}
