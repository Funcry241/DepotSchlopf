//! Realisierte Verkäufe und gebührenbereinigter Vergleich für die Depot-Seite.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use depot_journal::Stand;
use depot_kern::liste::Wert;
use depot_kern::{Cent, Datum, Grenzen, TopfId, Zeitpunkt};
use depot_lage::handelsergebnis::{
    Bilanz, OffenePosition, Verkauf, bilanz, handelspositionen, mehrertrag,
    nicht_zugeordnete_kaeufe,
};
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

#[derive(Clone, Copy)]
struct Abstand {
    mittel: Option<i64>,
    anzahl: usize,
}

fn abstand(daten: &[&Verkauf], korbrenditen: &BTreeMap<u64, i64>) -> Abstand {
    let differenzen: Vec<_> = daten
        .iter()
        .filter_map(|v| rendite(v)?.checked_sub(*korbrenditen.get(&v.nr)?))
        .collect();
    let mittel = differenzen
        .iter()
        .try_fold(0_i128, |s, d| s.checked_add(i128::from(*d)))
        .and_then(|s| s.checked_div(i128::try_from(differenzen.len()).ok()?))
        .and_then(|s| i64::try_from(s).ok());
    Abstand {
        mittel,
        anzahl: differenzen.len(),
    }
}

fn zahlen(gesamt: usize, summe: Option<Bilanz>, abstand: Abstand, text: &mut String) {
    let n = summe.map_or(0, |b| b.abgeschlossen);
    let _ = write!(
        text,
        "<td>{n} von {}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>",
        gesamt,
        geld(summe.map(|b| b.netto)),
        quote(summe.and_then(Bilanz::trefferquote)),
        geld(summe.and_then(|b| Bilanz::durchschnitt(b.gewinn_summe, b.gewinne))),
        geld(summe.and_then(|b| Bilanz::durchschnitt(b.verlust_summe, b.verluste))),
    );
    let _ = write!(
        text,
        "<td>{} ({})</td>",
        abstand.mittel.map_or_else(|| "–".into(), prozentpunkte),
        abstand.anzahl
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
    text.push_str("<h5>Letzte abgeschlossene Verkaufsaufträge</h5><div class=\"breit\"><table>        <tr><th>Order</th><th>Wert</th><th>Anlass</th><th>Einstand inkl. Kaufgebühren</th>        <th>Einstieg</th><th>Letzte Ausführung</th><th>Kalendertage</th><th>Verkaufserlös netto</th><th>Ergebnis netto</th>        <th>Rendite netto</th><th>Korb netto</th><th>Differenz in Prozentpunkten</th></tr>");
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
    text.push_str("</table></div>");
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
    grenzen: &Grenzen,
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
    let gesamt = verkettete_rendite(
        phasen
            .iter()
            .filter(|(von, bis)| von < bis)
            .map(|&(von, bis)| vergleich.rendite(von, bis, grenzen)),
    );
    let _ = write!(
        text,
        "<li>Verpasste Rendite durch Bargeld im Zeitraum: {}</li>",
        quote(gesamt)
    );
    for &(von, bis) in phasen.iter().rev().take(12) {
        let korb = (von < bis)
            .then(|| vergleich.rendite(von, bis, grenzen))
            .flatten();
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

fn verkettete_rendite(mut renditen: impl Iterator<Item = Option<i64>>) -> Option<i64> {
    let faktor = i128::from(renditen.next()??).checked_add(10_000)?;
    let skala = 100_000_000_i128;
    let faktor = renditen.try_fold(faktor.checked_mul(skala)?, |summe, bp| {
        summe
            .checked_mul(i128::from(bp?).checked_add(10_000)?)?
            .checked_div(10_000)
    })?;
    i64::try_from(
        faktor
            .checked_sub(10_000_i128.checked_mul(skala)?)?
            .checked_div(skala)?,
    )
    .ok()
}

struct Handelszahlen<'a> {
    topf: TopfId,
    verkaeufe: Vec<&'a Verkauf>,
    korbrenditen: BTreeMap<u64, i64>,
    bilanz: Option<Bilanz>,
    abstand: Abstand,
}

impl<'a> Handelszahlen<'a> {
    fn neu(
        topf: TopfId,
        verkaeufe: &'a [Verkauf],
        vergleich: &Handelsvergleich,
        ab: Option<Datum>,
        grenzen: &Grenzen,
    ) -> Self {
        let mut daten: Vec<_> = verkaeufe
            .iter()
            .filter(|v| v.topf == topf && ab.is_none_or(|ab| v.tag.is_some_and(|tag| tag >= ab)))
            .collect();
        daten.sort_by_key(|v| (v.tag, v.nr));
        let korbrenditen = daten
            .iter()
            .filter_map(|v| {
                let (von, bis) = v.einstieg.zip(v.ausstieg)?;
                Some((v.nr, vergleich.rendite(von, bis, grenzen)?))
            })
            .collect();
        Self {
            topf,
            bilanz: bilanz(daten.iter().copied()),
            abstand: abstand(&daten, &korbrenditen),
            verkaeufe: daten,
            korbrenditen,
        }
    }
}

fn balkenlaenge(bp: i64, skala: u64) -> String {
    let zehntel = u128::from(bp.unsigned_abs())
        .saturating_mul(500)
        .checked_div(u128::from(skala))
        .unwrap_or(0)
        .min(500);
    format!("{}.{:01}%", zehntel.div_euclid(10), zehntel.rem_euclid(10))
}

fn korbabstand(abstand: Abstand, skala: u64, text: &mut String) {
    text.push_str(
        r#"<div class="handelskarte-vergleich"><span>Ø Abstand zum Vergleichskorb</span>"#,
    );
    if let Some(bp) = abstand.mittel {
        let richtung = match bp.cmp(&0) {
            std::cmp::Ordering::Greater => "plus",
            std::cmp::Ordering::Less => "minus",
            std::cmp::Ordering::Equal => "null",
        };
        let _ = write!(
            text,
            r#"<strong>{} Prozentpunkte</strong><div class="handelskarte-skala" aria-hidden="true">
            <span data-richtung="{richtung}" style="--laenge:{}"></span></div><small>{} {}</small>"#,
            prozentpunkte(bp),
            balkenlaenge(bp, skala),
            abstand.anzahl,
            if abstand.anzahl == 1 {
                "Verkauf im Vergleich"
            } else {
                "Verkäufe im Vergleich"
            },
        );
    } else {
        text.push_str(
            r#"<strong class="leise">Nicht belegt</strong><small>0 auswertbare Vergleiche</small>"#,
        );
    }
    text.push_str("</div>");
}

fn kaufanlaesse(zahlen_je_topf: &Handelszahlen<'_>, werte: &[Wert], id: &str, text: &mut String) {
    let daten = &zahlen_je_topf.verkaeufe;
    if daten.is_empty() {
        return;
    }
    let _ = write!(
        text,
        r#"<details id="handelsdetails-{id}"><summary>Kaufanlässe und Verkäufe</summary>
        <div class="breit"><table><tr><th>Kaufanlass oder Stärkerang</th><th>Verkäufe auswertbar</th>
        <th>Ergebnis netto</th><th>Trefferquote</th><th>Ø Gewinn</th><th>Ø Verlust</th>
        <th>Ø Abstand zum Korb in Prozentpunkten (Anzahl)</th></tr><tr><th>Alle</th>"#
    );
    zahlen(
        daten.len(),
        zahlen_je_topf.bilanz,
        zahlen_je_topf.abstand,
        text,
    );
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
        zahlen(
            gruppe.len(),
            bilanz(gruppe.iter().copied()),
            abstand(&gruppe, &zahlen_je_topf.korbrenditen),
            text,
        );
        text.push_str("</tr>");
    }
    for (titel, lang, von, bis) in [
        ("Stärkerang 120 Tage, Rang 1–10", true, 1, 10),
        ("Stärkerang 120 Tage, Rang ab 11", true, 11, usize::MAX),
        ("Stärkerang 20 Tage, Rang 1–10", false, 1, 10),
        ("Stärkerang 20 Tage, Rang ab 11", false, 11, usize::MAX),
    ] {
        let gruppe: Vec<_> = daten
            .iter()
            .copied()
            .filter(|v| {
                let rang = if lang { v.staerkerang } else { v.staerkerang20 };
                rang.is_some_and(|rang| (von..=bis).contains(&rang))
            })
            .collect();
        if !gruppe.is_empty() {
            let _ = write!(text, "<tr><th>{titel}</th>");
            zahlen(
                gruppe.len(),
                bilanz(gruppe.iter().copied()),
                abstand(&gruppe, &zahlen_je_topf.korbrenditen),
                text,
            );
            text.push_str("</tr>");
        }
    }
    text.push_str("</table></div>");
    details(daten, werte, &zahlen_je_topf.korbrenditen, text);
    text.push_str("</details>");
}

fn karte(zahlen: &Handelszahlen<'_>, skala: u64, text: &mut String) -> &'static str {
    let (id, figur) = match zahlen.topf {
        TopfId::A => ("dachs", crate::schmuck::DACHS_FIGUR),
        TopfId::B => ("maus", crate::schmuck::MAUS_FIGUR),
    };
    let _ = write!(
        text,
        r#"<article class="handelskarte" data-berater="{id}" aria-labelledby="handelskarte-{id}">
        <header><span class="handelskarte-figur">{figur}</span><h4 id="handelskarte-{id}">{}</h4></header>
        <div class="handelskarte-netto"><span>Realisiert nach Handelsgebühren</span>"#,
        topf(zahlen.topf)
    );
    if let Some(bilanz) = zahlen.bilanz {
        let klasse = match bilanz.netto.0.cmp(&0) {
            std::cmp::Ordering::Greater => "gut",
            std::cmp::Ordering::Less => "halt",
            std::cmp::Ordering::Equal => "",
        };
        let _ = write!(
            text,
            r#"<strong class="{klasse}">{}</strong><small>{} von {} Verkäufen auswertbar</small>"#,
            geld(Some(bilanz.netto)),
            bilanz.abgeschlossen,
            zahlen.verkaeufe.len()
        );
    } else {
        text.push_str(r#"<strong class="leise">–</strong>"#);
        if zahlen.verkaeufe.is_empty() {
            text.push_str("<small>Keine abgeschlossenen Verkäufe im Zeitraum.</small>");
        } else {
            let _ = write!(
                text,
                "<small>0 von {} Verkäufen auswertbar</small>",
                zahlen.verkaeufe.len()
            );
        }
    }
    text.push_str("</div>");
    korbabstand(zahlen.abstand, skala, text);
    id
}

fn erwartungsabgleich(zahlen: &Handelszahlen<'_>, text: &mut String) {
    let paar: Vec<_> = zahlen
        .verkaeufe
        .iter()
        .filter_map(|v| v.erwartung_bp.zip(rendite(v)))
        .collect();
    let mittel = |wert: fn(&(i64, i64)) -> i64| -> Option<i64> {
        let summe = paar
            .iter()
            .try_fold(0_i64, |summe, x| summe.checked_add(wert(x)))?;
        summe.checked_div(i64::try_from(paar.len()).ok()?)
    };
    if let Some((erwartet, erreicht)) = mittel(|p| p.0).zip(mittel(|p| p.1)) {
        let abstand = erreicht
            .checked_sub(erwartet)
            .map_or_else(|| "–".into(), prozentpunkte);
        let _ = write!(
            text,
            "<p>Kauferwartung · {} Verkäufe: Ø erwartet {} brutto · Ø erreicht {} netto · Differenz {abstand} Prozentpunkte</p>",
            paar.len(),
            prozent(erwartet),
            prozent(erreicht)
        );
    }
}

fn schattenbilanz(stand: &Stand, text: &mut String) {
    let Some(schatten) = depot_lage::schatten::Schattenstand::letzter(stand, None) else {
        return;
    };
    let ergebnisse = schatten.ergebnisse();
    let korb = ergebnisse.last().and_then(|e| e.netto);
    text.push_str("<h4>Schattenstrategien · nur gerechnet</h4><table><tr><th>Strategie</th><th>Ergebnis netto</th><th>Abgeschlossene Trades</th><th>Abstand zum Korb</th></tr>");
    for e in ergebnisse {
        let abstand = e.netto.zip(korb).and_then(|(n, k)| n.checked_sub(k));
        let _ = write!(
            text,
            "<tr><th>{}</th><td>{}</td><td>{}</td><td>{}</td></tr>",
            e.name,
            geld(e.netto),
            e.trades,
            geld(abstand)
        );
    }
    text.push_str("</table>");
}

fn offene_bilanz(
    offen: &[OffenePosition],
    zahlen: &Handelszahlen<'_>,
    werte: &[Wert],
    korb: &Handelsvergleich,
    grenzen: &Grenzen,
    text: &mut String,
) {
    let positionen: Vec<_> = offen.iter().filter(|p| p.topf == zahlen.topf).collect();
    let korbrendite = |p: &OffenePosition| korb.rendite(p.einstieg?, p.bewertungstag?, grenzen);
    let realisiert = zahlen
        .verkaeufe
        .iter()
        .try_fold(Cent::NULL, |summe, v| summe.checked_add(v.netto?));
    let offen_netto = positionen
        .iter()
        .try_fold(Cent::NULL, |summe, p| summe.checked_add(p.netto()?));
    let euro_mehrertrag = zahlen
        .verkaeufe
        .iter()
        .map(|v| mehrertrag(v.netto?, v.einstand?, *zahlen.korbrenditen.get(&v.nr)?))
        .chain(
            positionen
                .iter()
                .map(|p| mehrertrag(p.netto()?, p.einstand?, korbrendite(p)?)),
        )
        .try_fold(Cent::NULL, |summe, betrag| summe.checked_add(betrag?));
    if zahlen.verkaeufe.is_empty() && positionen.is_empty() {
        return;
    }
    let _ = write!(
        text,
        "<p>Realisiert + offen: {} · Mehrertrag zum Vergleichskorb: {}</p>",
        geld(
            realisiert
                .zip(offen_netto)
                .and_then(|(r, o)| r.checked_add(o))
        ),
        geld(euro_mehrertrag),
    );
    if positionen.is_empty() {
        return;
    }
    text.push_str(r#"<details><summary>Positionen offen, bewertet zum letzten Schluss</summary><div class="breit"><table><tr><th>Wert</th><th>Schluss</th><th>Einstand inkl. Kaufgebühren</th><th>Wert nach rechnerischer Verkaufsgebühr</th><th>Ergebnis netto</th><th>Abstand zum Korb in Prozentpunkten</th></tr>"#);
    for p in positionen {
        let name = werte
            .iter()
            .find(|w| w.isin == p.isin)
            .map_or_else(|| p.isin.als_str(), |w| w.name.as_str());
        let abstand = p
            .einstand
            .zip(p.wert_netto)
            .and_then(|(e, w)| rendite_bp(e, w)?.checked_sub(korbrendite(p)?));
        let _ = write!(
            text,
            "<tr><th>{}</th><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            maskiere(name),
            kalenderdatum(p.bewertungstag),
            geld(p.einstand),
            geld(p.wert_netto),
            geld(p.netto()),
            abstand.map_or_else(|| "–".into(), prozentpunkte)
        );
    }
    text.push_str("</table></div></details>");
}

pub(super) fn abschnitt(
    stand: &Stand,
    werte: &[Wert],
    ab: Option<Datum>,
    grenzen: &Grenzen,
    text: &mut String,
) {
    let (verkaeufe, offen) = handelspositionen(stand, grenzen);
    let vergleich = Handelsvergleich::neu(stand);
    let toepfe = [TopfId::A, TopfId::B]
        .map(|id| Handelszahlen::neu(id, &verkaeufe, &vergleich, ab, grenzen));
    let skala = toepfe
        .iter()
        .filter_map(|t| t.abstand.mittel)
        .map(i64::unsigned_abs)
        .max()
        .unwrap_or(0)
        .max(500)
        .div_ceil(500)
        .saturating_mul(500);
    text.push_str(r#"<section id="handelsbilanz"><h3>Handelsbilanz</h3>
        <p class="leise">Für den gewählten Depotzeitraum · Der Füllpreis enthält einen gebuchten Modellspread. KI-Kosten sind hier nicht eingerechnet.</p>
        <div class="handelskarten">"#);
    for zahlen in &toepfe {
        let id = karte(zahlen, skala, text);
        offene_bilanz(&offen, zahlen, werte, &vergleich, grenzen, text);
        let ohne_anlass = nicht_zugeordnete_kaeufe(stand, zahlen.topf, ab);
        let _ = write!(text, "<p>Käufe nicht zugeordnet: {ohne_anlass}</p>");
        kaufanlaesse(zahlen, werte, id, text);
        erwartungsabgleich(zahlen, text);
        bargeld(stand, zahlen.topf, ab, &vergleich, grenzen, text);
        text.push_str("</article>");
    }
    text.push_str("</div>");
    schattenbilanz(stand, text);
    if let Some(gesamt) =
        depot_lage::topfgewichtung::feste_haelfte(stand, &depot_kern::liste::als_universum(werte))
    {
        let _ = write!(
            text,
            "<p>Feste Hälfte · Schattenrechnung anhand der beobachteten Topfrenditen: {gesamt}</p>"
        );
    }

    text.push_str(r#"<details id="handelsbilanz-vergleich">
        <summary>So wird der Vergleich berechnet</summary><p>Der Vergleichskorb startet am belegten Einstiegstag
        mit dem üblichen Vergleichskapital. Die Rendite berücksichtigt Kaufgebühren und einen rechnerischen
        Verkauf zum Ausstiegstag. Mehrere Einstiegs- oder Ausstiegstage können den Vergleich offenlassen.
        Beide Balken nutzen dieselbe Skala. Die Mittellinie steht für 0 Prozentpunkte.</p></details></section>"#);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bargeldrenditen_werden_verkettet() {
        assert_eq!(
            verkettete_rendite([Some(100), Some(200)].into_iter()),
            Some(302)
        );
        assert_eq!(verkettete_rendite([Some(100), None].into_iter()), None);
    }

    #[test]
    fn geldgewichteter_mehrertrag_zaehlt_die_positionsgroesse() {
        let klein = mehrertrag(Cent(1_000), Cent(10_000), 0);
        let gross = mehrertrag(Cent(-2_000), Cent(200_000), 0);
        assert_eq!(
            klein.zip(gross).and_then(|(a, b)| a.checked_add(b)),
            Some(Cent(-1_000))
        );
    }

    #[test]
    fn vergleichsbalken_zeigt_richtung_stichprobe_und_belegstatus() {
        for (bp, richtung, anzahl, beschriftung) in [
            (250, "plus", 2, "2 Verkäufe im Vergleich"),
            (-250, "minus", 1, "1 Verkauf im Vergleich"),
            (0, "null", 1, "1 Verkauf im Vergleich"),
        ] {
            let mut html = String::new();
            korbabstand(
                Abstand {
                    mittel: Some(bp),
                    anzahl,
                },
                500,
                &mut html,
            );
            assert!(html.contains(&format!("data-richtung=\"{richtung}\"")));
            assert!(html.contains(beschriftung));
            let breite = if bp == 0 { "0.0%" } else { "25.0%" };
            assert!(html.contains(&format!("--laenge:{breite}")));
        }
        let mut html = String::new();
        korbabstand(
            Abstand {
                mittel: None,
                anzahl: 0,
            },
            500,
            &mut html,
        );
        assert!(html.contains("Nicht belegt"));
        assert!(!html.contains("handelskarte-skala"));
    }
}
