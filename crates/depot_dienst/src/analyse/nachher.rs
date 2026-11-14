//! Rückblick auf belegten Orderversand und festgeschriebene Kursprognosen.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use depot_journal::{KursBeleg, Stand, aufloesung::zieltag, weltprognose::Prognoserichtung};
use depot_kern::{BeraterId, Datum, Herkunft, Isin, Preis, TopfId, Zeitpunkt};
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
                return if depot_kern::liste::boerse_von(entscheidung.isin).is_none() {
                    "Wert außerhalb der aktuellen Beobachtungsliste, Börsenkalender nicht belegt"
                        .into()
                } else {
                    "Zieltag nicht bestimmbar".into()
                };
            };
            if schluss > jetzt {
                self.ausstehend = self.ausstehend.saturating_add(1);
                return format!("{tag} · ausstehend");
            }
            let Some(beleg) = kurse.get(&(entscheidung.isin, tag)) else {
                self.fehlend = self.fehlend.saturating_add(1);
                return format!("{tag} · Schlusskurs nicht belegt");
            };
            let kurs = beleg.kurs().preis();
            let aenderung = entscheidung
                .basis
                .and_then(|basis| kurs.in_bp_von(basis))
                .and_then(|bp| bp.checked_sub(10_000));
            let richtung = match entscheidung.basis {
                Some(basis) if kurs == basis => " · unverändert",
                Some(basis) if (kurs > basis) == entscheidung.anstieg => " · Richtung bestätigt",
                Some(_) => " · Gegenrichtung",
                None => "",
            };
            format!("{tag} · {} · {}{richtung}", preis(kurs), prozent(aenderung))
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
    anstieg: bool,
}

type Schlusskurse<'a> = BTreeMap<(&'a Isin, Datum), &'a KursBeleg>;

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
            "Als bestätigte Richtung zählt ein Anstieg nach einem Kauf, ein Rückgang nach einem Verkauf ",
            "oder eine Bewegung in Prognoserichtung. Die Kursquelle muss den Kurstag belegen. ",
            "Der Kursabruf folgt der Beobachtungsliste unabhängig vom Depotbestand. ",
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
                beleg: format!(
                    "Order #{} · {richtung} · Topf {}",
                    order.nr,
                    match order.topf {
                        TopfId::A => "A",
                        TopfId::B => "B",
                    }
                ),
                isin: &order.isin,
                basis: order.bezug,
                anstieg: order.richtung == Richtung::Kauf,
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
                    "Prognose {} · {richtung} über {} Handelstage · Ereignis {}",
                    match p.berater {
                        BeraterId::Dachs => "Dachs",
                        BeraterId::Maus => "Maus",
                    },
                    p.prognose.handelstage,
                    p.prognose.ereignis_nr
                ),
                isin: &p.prognose.isin,
                basis: Some(p.ausgangskurs),
                anstieg: p.prognose.richtung == Prognoserichtung::Steigt,
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
        let schluessel = (beleg.isin(), beleg.kurs().handelstag());
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

    fn beispielorder(isin: Isin, versand: Zeitpunkt) -> OrderZeile {
        OrderZeile {
            nr: 1,
            topf: TopfId::A,
            isin,
            richtung: Richtung::Verkauf,
            stueck: 1,
            versand: Some(versand),
            frist: None,
            zustand: depot_journal::AuftragsZustand::Verfallen,
            bezug: Preis::aus_cent(Cent(10_000)),
            limit: Cent(10_000),
            limit_bp: None,
            fuellpreis: None,
            fuellpreis_bp: None,
            schluss: None,
            schluss_bp: None,
        }
    }

    #[test]
    fn auswertung_verwendet_nur_passende_entscheidungen_und_quellenkurse() -> Result<(), String> {
        use depot_journal::{Art, Journal};
        let freitag = Datum::neu(2030, 1, 11).ok_or("Freitag")?;
        let montag = Datum::neu(2030, 1, 14).ok_or("Montag")?;
        let versand = Zeitpunkt::frankfurt(freitag, 8, 0, 0).ok_or("Versand")?;
        let feierabend = |h| Zeitpunkt::frankfurt(montag, h, 0, 0).ok_or("Zeit");
        let jetzt = feierabend(21)?;
        let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let testpfad = std::env::temp_dir().join(format!("depot-nachher-{}", crate::neues_token()));
        std::fs::create_dir(&testpfad).map_err(|e| e.to_string())?;
        let pfad = testpfad.join("journal.txt");
        let fehler = |e| format!("{e:?}");
        Journal::einrichten(&pfad).map_err(fehler)?;
        let mut journal = Journal::oeffnen(&pfad).map_err(fehler)?;
        for (h, cent, herkunft) in [
            (18, 10_500, Herkunft::Quelle),
            (19, 11_000, Herkunft::Quelle),
            (20, 99_900, Herkunft::Simulation),
            (22, 88_800, Herkunft::Quelle),
        ] {
            let kurs = Kurs::neu(Cent(cent), "EUR", montag, feierabend(17)?, herkunft)
                .map_err(|e| format!("{e:?}"))?;
            let beleg = KursBeleg::neu(
                "beispielquelle",
                feierabend(h)?,
                b"test",
                isin.clone(),
                kurs,
            )?;
            journal.belege_kurs(&beleg).map_err(fehler)?;
        }
        journal.beginne_abrechnung(montag).map_err(fehler)?;
        journal
            .anhaengen(Art::Beobachtung, "2030-01-14;Beispielereignis")
            .map_err(fehler)?;
        journal
            .belege_weltprognose(&depot_journal::weltprognose::PrognoseBeleg {
                datum: montag,
                berater: BeraterId::Dachs,
                prognose: depot_journal::weltprognose::Weltprognose {
                    isin: isin.clone(),
                    richtung: Prognoserichtung::Steigt,
                    handelstage: 3,
                    ereignis_nr: 1,
                    begruendung: "Beispielgrund".into(),
                    widerlegung: "Beispielwiderlegung".into(),
                },
                ausgangskurs: Preis::aus_cent(Cent(11_000)).ok_or("Preis")?,
                ereignis: "Beispielereignis".into(),
            })
            .map_err(fehler)?;
        let order = beispielorder(isin, versand);
        let zeitraum = Zeitraum {
            von: freitag,
            bis: freitag,
        };
        let mut spaeter = order.clone();
        spaeter.nr = 2;
        spaeter.versand = Some(feierabend(22)?);
        let mut ausserhalb = order.clone();
        ausserhalb.nr = 3;
        ausserhalb.versand = Some(feierabend(18)?);
        let aus = auswertung(
            journal.stand(),
            &[order, ausserhalb.clone()],
            zeitraum,
            jetzt,
        );
        assert!(
            aus.bericht
                .contains("2030-01-14 · 110,00 € · 10,00 % · Gegenrichtung")
        );
        assert!(!aus.bericht.contains("Order #3"));
        assert!(!aus.bericht.contains("Prognose Dachs"));
        let zeitraum = Zeitraum {
            von: montag,
            bis: montag,
        };
        let aus = auswertung(journal.stand(), &[spaeter], zeitraum, jetzt);
        assert!(!aus.bericht.contains("Order #2"));
        assert!(aus.bericht.contains("Prognose Dachs · steigt"));
        let leer = auswertung(
            journal.stand(),
            &[],
            Zeitraum {
                von: freitag,
                bis: freitag,
            },
            jetzt,
        );
        assert_eq!(leer.ampel(), "Rot (keine auswertbaren Belege)");
        drop(journal);
        std::fs::remove_dir_all(testpfad).map_err(|e| e.to_string())?;
        Ok(())
    }

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
        let kurse = Schlusskurse::from([((&isin, montag), &beleg)]);
        let entscheidung = Entscheidung {
            tag: freitag,
            beleg: "Beispielentscheidung".into(),
            isin: &isin,
            basis: Preis::aus_cent(Cent(10_000)),
            anstieg: true,
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
