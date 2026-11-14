//! Rückblick auf belegten Orderversand und festgeschriebene Kursprognosen.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use depot_journal::{KursBeleg, Stand, aufloesung::zieltag, weltprognose::Prognoserichtung};
use depot_kern::{Datum, Herkunft, Isin, Preis, Zeitpunkt};
use depot_lage::ausfuehrung::{OrderZeile, Richtung};

use super::{Zeitraum, prozent};

const HORIZONTE: [u8; 3] = [1, 3, 5];

#[derive(Default)]
pub(super) struct Nachher {
    pub bericht: String,
    zeilen: usize,
    fehlend: usize,
    ausstehend: usize,
}

impl Nachher {
    pub(super) fn ampel(&self) -> &'static str {
        if self.zeilen == 0 {
            "Rot (keine auswertbaren Belege)"
        } else if self.fehlend > 0 {
            "Rot (Kursbelege fehlen)"
        } else if self.ausstehend > 0 {
            "Gelb (Zieltage stehen aus)"
        } else {
            "Grün"
        }
    }

    fn zeile(
        &mut self,
        entscheidung: &Entscheidung<'_>,
        kurse: &Schlusskurse<'_>,
        jetzt: Zeitpunkt,
    ) {
        self.zeilen = self.zeilen.saturating_add(1);
        let basis = entscheidung
            .basis
            .map_or_else(|| "nicht belegt".into(), preis);
        if entscheidung.basis.is_none() {
            self.fehlend = self.fehlend.saturating_add(1);
        }
        let werte = HORIZONTE.map(|tage| {
            let Some((tag, schluss)) =
                zieltag(entscheidung.isin, entscheidung.tag, tage).and_then(|tag| {
                    depot_kern::liste::boerse_von(entscheidung.isin)
                        .and_then(|b| b.schluss(tag))
                        .map(|schluss| (tag, schluss))
                })
            else {
                self.fehlend = self.fehlend.saturating_add(1);
                return "Zieltag nicht bestimmbar".into();
            };
            if schluss > jetzt {
                self.ausstehend = self.ausstehend.saturating_add(1);
                return format!("{tag} · ausstehend");
            }
            let Some(beleg) = kurse.get(&(entscheidung.isin, schluss)) else {
                self.fehlend = self.fehlend.saturating_add(1);
                return format!("{tag} · Schlusskurs nicht belegt");
            };
            let kurs = beleg.kurs().preis();
            let aenderung = entscheidung
                .basis
                .and_then(|basis| kurs.in_bp_von(basis))
                .and_then(|bp| bp.checked_sub(10_000));
            format!("{tag} · {} · {}", preis(kurs), prozent(aenderung))
        });
        let [eins, drei, fuenf] = werte;
        let _ = writeln!(
            self.bericht,
            "| {} | {} | {} | {basis} | {eins} | {drei} | {fuenf} |",
            entscheidung.tag, entscheidung.beleg, entscheidung.isin
        );
    }
}

struct Entscheidung<'a> {
    tag: Datum,
    beleg: String,
    isin: &'a Isin,
    basis: Option<Preis>,
}

type Schlusskurse<'a> = BTreeMap<(&'a Isin, Zeitpunkt), &'a KursBeleg>;

fn preis(preis: Preis) -> String {
    format!("{} €", preis.euro_text())
}

pub(super) fn auswertung(
    stand: &Stand,
    orders: &[OrderZeile],
    zeitraum: Zeitraum,
    jetzt: Zeitpunkt,
) -> Nachher {
    let mut aus = Nachher {
        bericht: String::from(concat!(
            "\n## Nachher-Blick\n\n",
            "Die Auswahl richtet sich nach dem Frankfurter Datum des Orderversands oder dem Prognosetag. ",
            "Die Zielkurse stammen vom 1., 3. und 5. folgenden Handelstag an der Börse des Werts, ",
            "auch nach Ende des Exportzeitraums. Angezeigt wird die Kursänderung zum belegten Bezugskurs ",
            "beim Versand oder zum fixierten Prognosekurs. Die Änderung enthält keine Ausführungskosten. ",
            "Ein Verzicht auf Orders ist in „Nicht-Handeln“ belegt. Konkrete Kursverläufe lassen sich ",
            "einem Verzicht nur bei einer gesondert belegten Kursprognose zuordnen.\n\n",
            "| Entscheidungstag | Beleg | ISIN | Bezugskurs | +1 Handelstag | +3 Handelstage | +5 Handelstage |\n",
            "|---|---|---|---|---|---|---|\n",
        )),
        ..Nachher::default()
    };
    let mut entscheidungen = Vec::new();
    for order in orders {
        if let Some(versand) = order
            .versand
            .filter(|z| *z <= jetzt && zeitraum.enthaelt(*z))
            && let Some(tag) = versand.frankfurt_tag()
        {
            let richtung = match order.richtung {
                Richtung::Kauf => "Kauf",
                Richtung::Verkauf => "Verkauf",
            };
            entscheidungen.push(Entscheidung {
                tag,
                beleg: format!("Order #{} · {richtung} · Topf {:?}", order.nr, order.topf),
                isin: &order.isin,
                basis: order.bezug,
            });
        }
    }
    for p in stand.weltprognosen() {
        if (zeitraum.von..=zeitraum.bis).contains(&p.datum) {
            let richtung = match p.prognose.richtung {
                Prognoserichtung::Steigt => "steigt",
                Prognoserichtung::Faellt => "fällt",
            };
            entscheidungen.push(Entscheidung {
                tag: p.datum,
                beleg: format!(
                    "Prognose {:?} · {richtung} über {} Handelstage · Ereignis {}",
                    p.berater, p.prognose.handelstage, p.prognose.ereignis_nr
                ),
                isin: &p.prognose.isin,
                basis: Some(p.ausgangskurs),
            });
        }
    }
    if entscheidungen.is_empty() {
        aus.bericht.push_str("| – | Kein datierter Orderversand und keine Kursprognose im Zeitraum belegt | – | – | – | – | – |\n");
        return aus;
    }

    let mut kurse = Schlusskurse::new();
    for beleg in stand
        .kurse()
        .iter()
        .filter(|b| b.abruf() <= jetzt && b.kurs().herkunft() == Herkunft::Quelle)
    {
        let schluessel = (beleg.isin(), beleg.kurs().zeitpunkt());
        if kurse
            .get(&schluessel)
            .is_none_or(|alt| alt.abruf() <= beleg.abruf())
        {
            kurse.insert(schluessel, beleg);
        }
    }
    entscheidungen.sort_by_key(|e| e.tag);
    for entscheidung in entscheidungen {
        aus.zeile(&entscheidung, &kurse, jetzt);
    }
    aus
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_kern::{Boerse, Cent, Kurs};

    #[test]
    fn handelstage_und_kursluecken_bleiben_unterscheidbar() -> Result<(), String> {
        let freitag = Datum::neu(2030, 1, 11).ok_or("Freitag")?;
        let montag = Datum::neu(2030, 1, 14).ok_or("Montag")?;
        let mittwoch = Datum::neu(2030, 1, 16).ok_or("Mittwoch")?;
        let zielschluss = Boerse::Xetra.schluss(montag).ok_or("Schluss")?;
        let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let kurs = Kurs::neu(Cent(11_000), "EUR", montag, zielschluss, Herkunft::Quelle)
            .map_err(|e| format!("{e:?}"))?;
        let beleg = KursBeleg::neu("beispielquelle", zielschluss, b"test", isin.clone(), kurs)?;
        let kurse = Schlusskurse::from([((&isin, zielschluss), &beleg)]);
        let entscheidung = Entscheidung {
            tag: freitag,
            beleg: "Beispielentscheidung".into(),
            isin: &isin,
            basis: Preis::aus_cent(Cent(10_000)),
        };
        let mut nachher = Nachher::default();
        nachher.zeile(&entscheidung, &kurse, zielschluss);
        assert!(nachher.bericht.contains("2030-01-14 · 110,00 € · 10,00 %"));
        assert!(nachher.bericht.contains("2030-01-16 · ausstehend"));
        assert_eq!(nachher.ampel(), "Gelb (Zieltage stehen aus)");

        let mut nachher = Nachher::default();
        nachher.zeile(
            &entscheidung,
            &kurse,
            Boerse::Xetra.schluss(mittwoch).ok_or("Schluss")?,
        );
        assert!(
            nachher
                .bericht
                .contains("2030-01-16 · Schlusskurs nicht belegt")
        );
        assert!(nachher.bericht.contains("2030-01-18 · ausstehend"));
        assert_eq!(nachher.ampel(), "Rot (Kursbelege fehlen)");
        Ok(())
    }
}
