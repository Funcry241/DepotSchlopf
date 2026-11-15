//! Realisierte Verkäufe und gebührenbereinigter Vergleich für die Depot-Seite.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use depot_journal::Stand;
use depot_kern::liste::Wert;
use depot_kern::{Datum, TopfId, Zeitpunkt};
use depot_lage::handelsergebnis::{Bilanz, Verkauf, bilanz, verkaeufe};
use depot_lage::handelsvergleich::Handelsvergleich;
use depot_lage::vergleich::rendite_bp;
use depot_lage::{
    datum_text,
    html::{maskiere, prozent},
};

use super::{geld, topf};

fn quote(bp: Option<i64>) -> String {
    bp.map_or_else(|| "nicht belegt".into(), prozent)
}

fn rendite(v: &Verkauf) -> Option<i64> {
    let (einstand, netto) = v.einstand.zip(v.netto)?;
    rendite_bp(einstand, einstand.checked_add(netto)?)
}

fn prozentpunkte(bp: i64) -> String {
    prozent(bp).replace(" %", "")
}

fn zahlen(daten: &[&Verkauf], korbrenditen: &BTreeMap<u64, i64>, text: &mut String) {
    let summe = bilanz(daten.iter().copied());
    let n = summe.map_or(0, |b| b.abgeschlossen);
    let _ = write!(
        text,
        "<td>{n} von {}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>",
        daten.len(),
        geld(summe.map(|b| b.netto)),
        quote(summe.and_then(Bilanz::trefferquote)),
        geld(summe.and_then(|b| Bilanz::durchschnitt(b.gewinn_summe, b.gewinne))),
        geld(summe.and_then(|b| Bilanz::durchschnitt(b.verlust_summe, b.verluste))),
    );
    let differenzen: Vec<_> = daten
        .iter()
        .filter_map(|v| rendite(v)?.checked_sub(*korbrenditen.get(&v.nr)?))
        .collect();
    let mittel = differenzen
        .iter()
        .try_fold(0_i64, |s, d| s.checked_add(*d))
        .and_then(|s| s.checked_div(i64::try_from(differenzen.len()).ok()?));
    let _ = write!(
        text,
        "<td>{} ({})</td>",
        mittel.map_or_else(|| "–".into(), prozentpunkte),
        differenzen.len()
    );
}

fn kalenderdatum(datum: Option<Datum>) -> String {
    datum.map_or_else(|| "–".into(), datum_text)
}

fn haltedauer(von: Option<Datum>, bis: Option<Datum>) -> Option<i64> {
    let von = Zeitpunkt::neu(von?, 0, 0, 0)?;
    Zeitpunkt::neu(bis?, 0, 0, 0)?
        .sekunden_seit(von)?
        .checked_div(86_400)
}

fn details(
    verkaeufe: &[&Verkauf],
    werte: &[Wert],
    korbrenditen: &BTreeMap<u64, i64>,
    text: &mut String,
) {
    if verkaeufe.is_empty() {
        return;
    }
    text.push_str("<details><summary>Letzte abgeschlossene Verkaufsaufträge</summary><div class=\"breit\"><table>        <tr><th>Order</th><th>Wert</th><th>Anlass</th><th>Einstand inkl. Kaufgebühren</th>        <th>Einstieg</th><th>Letzte Ausführung</th><th>Kalendertage</th><th>Verkaufserlös netto</th><th>Ergebnis netto</th>        <th>Rendite netto</th><th>Korb netto</th><th>Differenz in Prozentpunkten</th></tr>");
    for v in verkaeufe.iter().rev().take(30) {
        let netto = rendite(v);
        let korb = korbrenditen.get(&v.nr).copied();
        let differenz = netto.zip(korb).and_then(|(n, k)| n.checked_sub(k));
        let name = werte
            .iter()
            .find(|w| w.isin == v.isin)
            .map_or_else(|| v.isin.als_str(), |w| w.name.as_str());
        let _ = write!(
            text,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>             <td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            v.nr,
            maskiere(name),
            v.anlass.unwrap_or("nicht zugeordnet"),
            geld(v.einstand),
            kalenderdatum(v.einstieg),
            kalenderdatum(v.tag),
            haltedauer(v.einstieg, v.ausstieg).map_or_else(|| "–".into(), |t| t.to_string()),
            geld(v.einstand.zip(v.netto).and_then(|(e, n)| e.checked_add(n))),
            geld(v.netto),
            quote(netto),
            quote(korb),
            differenz.map_or_else(|| "–".into(), prozentpunkte),
        );
    }
    text.push_str("</table></div></details>");
}

fn bargeldphasen(stand: &Stand, id: TopfId, ab: Option<Datum>) -> Vec<(Datum, Datum)> {
    let mut phasen = Vec::new();
    let mut von_bis = None;
    for (tag, toepfe) in stand
        .tagesschluesse()
        .iter()
        .filter(|(tag, _)| ab.is_none_or(|ab| *tag >= ab))
    {
        if toepfe.get(&id).is_some_and(|t| t.positionen().is_empty()) {
            let (von, _) = von_bis.unwrap_or((*tag, *tag));
            von_bis = Some((von, *tag));
        } else if let Some(phase) = von_bis.take() {
            phasen.push(phase);
        }
    }
    phasen.extend(von_bis);
    phasen
}

fn bargeld(
    stand: &Stand,
    id: TopfId,
    ab: Option<Datum>,
    vergleich: &Handelsvergleich,
    text: &mut String,
) {
    let phasen = bargeldphasen(stand, id, ab);
    if phasen.is_empty() {
        return;
    }
    let _ = write!(
        text,
        "<details><summary>Bargeldphasen am Tagesabschluss ({})</summary><ul>",
        phasen.len()
    );
    for &(von, bis) in phasen.iter().rev().take(12) {
        let korb = (von < bis).then(|| vergleich.rendite(von, bis)).flatten();
        let _ = write!(
            text,
            "<li>{} bis {} · Vergleichskorb nach Kauf- und Verkaufsgebühren {}</li>",
            datum_text(von),
            datum_text(bis),
            quote(korb),
        );
    }
    text.push_str("</ul></details>");
}

pub(super) fn abschnitt(stand: &Stand, werte: &[Wert], ab: Option<Datum>, text: &mut String) {
    let verkaeufe = verkaeufe(stand);
    let vergleich = Handelsvergleich::neu(stand);
    text.push_str("<section id=\"handelsbilanz\"><h3>Handelsbilanz</h3>        <p>Zeitraum wie im Depotverlauf. Realisierte Ergebnisse nach gebuchten Handelsgebühren.         Spread und KI-Kosten sind hier nicht eingerechnet.</p>        <p>Der Vergleichskorb startet am belegten Einstiegstag mit dem üblichen Vergleichskapital.         Seine Rendite berücksichtigt Kaufgebühren und einen rechnerischen Verkauf zum Ausstiegstag.         Mehrere Einstiegs- oder Ausstiegstage können den Vergleich offenlassen.</p>");
    for id in [TopfId::A, TopfId::B] {
        let mut daten: Vec<_> = verkaeufe
            .iter()
            .filter(|v| v.topf == id && ab.is_none_or(|ab| v.tag.is_some_and(|tag| tag >= ab)))
            .collect();
        daten.sort_by_key(|v| (v.tag, v.nr));
        let _ = write!(text, "<h4>{}</h4>", topf(id));
        if daten.is_empty() {
            text.push_str("<p>Keine abgeschlossenen Verkäufe im Zeitraum.</p>");
        } else {
            text.push_str("<div class=\"breit\"><table><tr><th>Kaufanlass</th><th>Verkäufe auswertbar</th>                <th>Ergebnis netto</th><th>Trefferquote</th><th>Ø Gewinn</th><th>Ø Verlust</th>\
                <th>Ø Abstand zum Korb in Prozentpunkten (Anzahl)</th></tr>                <tr><th>Alle</th>");
            let korbrenditen = daten
                .iter()
                .filter_map(|v| {
                    let (von, bis) = v.einstieg.zip(v.ausstieg)?;
                    Some((v.nr, vergleich.rendite(von, bis)?))
                })
                .collect();
            zahlen(&daten, &korbrenditen, text);
            text.push_str("</tr>");
            let anlaesse: BTreeSet<_> = daten.iter().map(|v| v.anlass).collect();
            for anlass in anlaesse {
                let gruppe: Vec<_> = daten
                    .iter()
                    .copied()
                    .filter(|v| v.anlass == anlass)
                    .collect();
                let _ = write!(
                    text,
                    "<tr><th>{}</th>",
                    anlass.unwrap_or("nicht zugeordnet")
                );
                zahlen(&gruppe, &korbrenditen, text);
                text.push_str("</tr>");
            }
            text.push_str("</table></div>");
            details(&daten, werte, &korbrenditen, text);
        }
        bargeld(stand, id, ab, &vergleich, text);
    }
    text.push_str("</section>");
}
