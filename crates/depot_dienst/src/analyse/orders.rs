//! Belegte Orderbewegungen und Zahlungsbeträge im Papierhandel.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use depot_journal::{Art, AuftragsLage, AuftragsZustand, Ausfuehrung, Stand};
use depot_kern::{Cent, Datum, Preis};
use depot_lage::ausfuehrung::{OrderZeile, Richtung};

use super::{Zeitraum, geld, prozent};
use depot_lage::zeit_text;

pub(super) fn bericht(
    stand: &Stand,
    eintraege: &[(Art, String)],
    buch: &[OrderZeile],
    zeitraum: Zeitraum,
) -> String {
    let mut aus = String::from(
        "\n## Im Zeitraum belegte Orderbewegungen\n\nDie Auswahl nutzt Versand, Frist, Ausführung und datierte Journalbewegungen. Der angezeigte Zustand gilt zum Exportzeitpunkt. Die Frist ist der vorgesehene Börsenschluss. Ausführungszeilen zeigen die gesamte belegte Orderhistorie, auch außerhalb des gewählten Zeitraums. Netto bezeichnet den Geldabfluss beim Kauf und den Geldzufluss beim Verkauf nach Gebühren. Ein Geld-Brief-Spread ist im Papierjournal nicht belegt.\n",
    );
    let (bewegungen, mut undatiert) = journalbewegungen(eintraege, zeitraum);
    let gruende = verfallsgruende(stand, zeitraum);
    let mut anzahl = 0_usize;
    for z in buch {
        let Some(order) = stand.auftrag(z.nr) else {
            continue;
        };
        if !bewegungen.contains(&z.nr)
            && !gruende.contains_key(&z.nr)
            && !z.versand.is_some_and(|zeit| zeitraum.enthaelt(zeit))
            && !z.frist.is_some_and(|zeit| zeitraum.enthaelt(zeit))
            && !order
                .ausfuehrungen
                .iter()
                .any(|f| f.kurszeit.is_some_and(|zeit| zeitraum.enthaelt(zeit)))
        {
            continue;
        }
        undatiert.remove(&z.nr);
        anzahl = anzahl.saturating_add(1);
        schreibe_order(&mut aus, z, order, gruende.get(&z.nr).copied());
    }
    if anzahl == 0 {
        aus.push_str("\nKeine datierte Orderbewegung im Zeitraum belegt.\n");
    }
    if !undatiert.is_empty() {
        aus.push_str(
            "\nOrdernummern mit undatierten Journalbewegungen, deren Zeitraum offen bleibt: ",
        );
        let nummern: Vec<_> = undatiert.iter().map(|nr| format!("#{nr}")).collect();
        let _ = writeln!(aus, "{}.", nummern.join(", "));
    }
    aus
}

fn journalbewegungen(
    eintraege: &[(Art, String)],
    zeitraum: Zeitraum,
) -> (BTreeSet<u64>, BTreeSet<u64>) {
    let mut im_zeitraum = BTreeSet::new();
    let mut undatiert = BTreeSet::new();
    let mut abrechnungstag = None;
    for (art, text) in eintraege {
        match art {
            Art::Abrechnungstag => abrechnungstag = Datum::lies(text),
            Art::Abschluss | Art::NotbremseFreigabe => abrechnungstag = None,
            Art::Auftrag | Art::Zustand => {
                let Some(nr) = text.split(';').next().and_then(|nr| nr.parse().ok()) else {
                    continue;
                };
                let datum = super::journal::orderdatum(*art, text, abrechnungstag);
                match datum {
                    Some(tag) if (zeitraum.von..=zeitraum.bis).contains(&tag) => {
                        im_zeitraum.insert(nr);
                    }
                    None => {
                        undatiert.insert(nr);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    (im_zeitraum, undatiert)
}

fn verfallsgruende(stand: &Stand, zeitraum: Zeitraum) -> BTreeMap<u64, (Datum, &'static str)> {
    stand
        .begruendungen()
        .iter()
        .filter_map(|(tag, wer, text)| {
            if wer != "Simulation" || !(zeitraum.von..=zeitraum.bis).contains(tag) {
                return None;
            }
            let (topf, rest) = text.split_once(": Auftrag ")?;
            let (nr, grund) = rest.split_once(" verfällt: ")?;
            let nr = nr.parse().ok()?;
            let order = stand.auftrag(nr)?;
            if !matches!(
                (topf, order.beleg.topf),
                ("Topf A", depot_kern::TopfId::A) | ("Topf B", depot_kern::TopfId::B)
            ) {
                return None;
            }
            let grund = match grund {
                "Schlusskurs nicht mehr nachholbar" => "Schlusskurs nicht mehr nachholbar",
                "Schlusskurs nicht belegt" => "Schlusskurs nicht belegt",
                "Kurs ausserhalb des Limits" | "Kurs außerhalb des Limits" => {
                    "Kurs außerhalb des Limits"
                }
                _ => return None,
            };
            Some((nr, (*tag, grund)))
        })
        .collect()
}

fn schreibe_order(
    aus: &mut String,
    z: &OrderZeile,
    order: &AuftragsLage,
    grund: Option<(Datum, &str)>,
) {
    let richtung = match z.richtung {
        Richtung::Kauf => "Kauf",
        Richtung::Verkauf => "Verkauf",
    };
    let preis = |p: Option<Preis>| {
        p.map_or_else(|| "nicht belegt".into(), |p| format!("{} €", p.euro_text()))
    };
    let zeit =
        |t: Option<depot_kern::Zeitpunkt>| t.map_or_else(|| "nicht belegt".into(), zeit_text);
    let _ = writeln!(
        aus,
        "\n### Order #{} · Topf {:?} · {} · {richtung}\n\n- {} Stück · Limit {} · Bezugskurs {} · Limitabstand {}\n- Versand {} · Frist {} · Zustand {}",
        z.nr,
        z.topf,
        z.isin,
        z.stueck,
        z.limit,
        preis(z.bezug),
        prozent(z.limit_bp),
        zeit(z.versand),
        zeit(z.frist),
        depot_lage::zustand_text(z.zustand)
    );
    if z.zustand == AuftragsZustand::Verfallen {
        if let Some((tag, grund)) = grund {
            let _ = writeln!(aus, "- Verfallsgrund am {tag} belegt: {grund}.");
        } else {
            aus.push_str("- Verfallsgrund nicht im Zeitraum belegt.\n");
        }
    }
    if order.ausfuehrungen.is_empty() {
        aus.push_str("- Keine Ausführung belegt.\n");
        return;
    }
    let _ = writeln!(
        aus,
        "- Durchschnittlicher Füllpreis {} · Abstand zum Bezugskurs {}.",
        preis(z.fuellpreis),
        prozent(z.fuellpreis_bp)
    );
    for f in &order.ausfuehrungen {
        let _ = writeln!(
            aus,
            "  - {} · {} Stück zu {} · Gebühren {} · Netto-Zahlungsbetrag {}",
            zeit(f.kurszeit),
            f.stueck,
            f.kurs,
            f.gebuehr,
            geld(netto(z.richtung, f))
        );
    }
}

fn netto(richtung: Richtung, f: &Ausfuehrung) -> Option<Cent> {
    let brutto = f.kurs.mal_stueck(f.stueck)?;
    match richtung {
        Richtung::Kauf => Cent::NULL.checked_sub(brutto.checked_add(f.gebuehr)?),
        Richtung::Verkauf => brutto.checked_sub(f.gebuehr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_zeigen_belegte_fruehere_und_spaetere_bewegungen() -> Result<(), String> {
        let von = Datum::neu(2030, 1, 8).ok_or("Tag")?;
        let zeitraum = Zeitraum { von, bis: von };
        let eintraege = [
            (Art::Abrechnungstag, "2030-01-07"),
            (Art::Auftrag, "1;A;DE0007164600;Kauf;2;1000;Umschichtung"),
            (
                Art::Zustand,
                "1;gesendet;2030-01-07T10:00:00Z;2030-01-07T16:30:00Z",
            ),
            (Art::Abschluss, "2030-01-07"),
            (Art::Abrechnungstag, "2030-01-08"),
            (Art::Zustand, "1;verfallen"),
            (
                Art::Zustand,
                "2;ausfuehrung;1;1000;150;2030-01-07T16:30:00Z",
            ),
            (Art::Zustand, "3;ausfuehrung;1;1000;150"),
            (Art::Abschluss, "2030-01-08"),
            (Art::Zustand, "4;storniert"),
            (
                Art::Zustand,
                "5;gesendet;2030-01-08T23:30:00Z;2030-01-09T16:30:00Z",
            ),
        ]
        .map(|(art, text)| (art, text.to_owned()));
        let (belegt, offen) = journalbewegungen(&eintraege, zeitraum);
        assert_eq!(belegt, [1, 3].into());
        assert_eq!(offen, [4].into());
        Ok(())
    }

    #[test]
    fn bericht_zeigt_frist_verfallsbelege_und_netto() -> Result<(), String> {
        let von = Datum::neu(2030, 1, 8).ok_or("Tag")?;
        let zeitraum = Zeitraum { von, bis: von };
        let zeit = depot_kern::Zeitpunkt::lies("2030-01-08T16:30:00Z").ok_or("Frist")?;
        let isin = depot_kern::Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let zeile = OrderZeile {
            nr: 1,
            topf: depot_kern::TopfId::A,
            isin,
            richtung: Richtung::Kauf,
            stueck: 2,
            versand: None,
            frist: Some(zeit),
            zustand: AuftragsZustand::Verfallen,
            bezug: None,
            limit: Cent(1_000),
            limit_bp: None,
            fuellpreis: None,
            fuellpreis_bp: None,
            schluss: None,
            schluss_bp: None,
        };
        let eintraege = [
            (Art::Topf, "A;100000;"),
            (Art::Auftrag, "1;A;DE0007164600;Kauf;2;1000;Umschichtung"),
            (Art::Zustand, "1;gesendet"),
            (Art::Zustand, "1;angenommen"),
            (Art::Zustand, "1;verfallen"),
            (Art::Auftrag, "2;A;DE0007164600;Kauf;1;1000;Umschichtung"),
            (
                Art::Begruendung,
                "2030-01-08;Simulation;Topf A: Auftrag 1 verfällt: Beispiel-Freitext",
            ),
        ];
        let ordner = std::env::temp_dir().join(format!("depot-order-{}", crate::neues_token()));
        std::fs::create_dir(&ordner).map_err(|e| e.to_string())?;
        let pfad = ordner.join("journal.txt");
        let bytes = depot_journal::inhalt_fuer(&eintraege);
        let vermerk = depot_journal::vermerk_fuer(&bytes, 0).ok_or("Vermerk")?;
        std::fs::write(&pfad, bytes).map_err(|e| e.to_string())?;
        std::fs::write(pfad.with_extension("vermerk"), vermerk).map_err(|e| e.to_string())?;
        let mut journal = depot_journal::Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
        let andere = OrderZeile {
            nr: 2,
            frist: None,
            ..zeile.clone()
        };
        let buch = [zeile.clone(), andere];
        let text = bericht(journal.stand(), journal.eintraege(), &buch, zeitraum);
        assert!(text.contains("### Order #1"));
        assert!(!text.contains("### Order #2"));
        assert!(text.contains("Verfallsgrund nicht im Zeitraum belegt"));
        assert!(!text.contains("Beispiel-Freitext"));
        assert!(!text.contains("Bewegungen, deren Zeitraum offen bleibt: #1"));
        let folgetag = von.plus_tage(1).ok_or("Folgetag")?;
        let text = bericht(
            journal.stand(),
            journal.eintraege(),
            &buch,
            Zeitraum {
                von: folgetag,
                bis: folgetag,
            },
        );
        assert!(text.contains("Keine datierte Orderbewegung im Zeitraum belegt."));
        journal
            .begruende(
                von,
                "Simulation",
                "Topf A: Auftrag 1 verfällt: Kurs ausserhalb des Limits",
            )
            .map_err(|e| format!("{e:?}"))?;
        let text = bericht(journal.stand(), journal.eintraege(), &buch, zeitraum);
        assert!(text.contains("Verfallsgrund am 2030-01-08 belegt: Kurs außerhalb des Limits."));
        let mut auftrag = journal.stand().auftrag(1).ok_or("Order")?.clone();
        auftrag.ausfuehrungen.push(Ausfuehrung {
            stueck: 2,
            kurs: Cent(1_000),
            gebuehr: Cent(150),
            kurszeit: Some(zeit),
        });
        for (richtung, betrag) in [(Richtung::Kauf, "-21,50 €"), (Richtung::Verkauf, "18,50 €")]
        {
            let mut text = String::new();
            let z = OrderZeile {
                richtung,
                ..zeile.clone()
            };
            schreibe_order(&mut text, &z, &auftrag, None);
            assert!(text.contains(&format!(
                "2 Stück zu 10,00 € · Gebühren 1,50 € · Netto-Zahlungsbetrag {betrag}"
            )));
        }
        drop(journal);
        std::fs::remove_dir_all(ordner).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn netto_beruecksichtigt_gebuehren_und_zahlungsrichtung() {
        let ausfuehrung = Ausfuehrung {
            stueck: 2,
            kurs: Cent(1_000),
            gebuehr: Cent(150),
            kurszeit: None,
        };
        assert_eq!(netto(Richtung::Kauf, &ausfuehrung), Some(Cent(-2_150)));
        assert_eq!(netto(Richtung::Verkauf, &ausfuehrung), Some(Cent(1_850)));
    }
}
