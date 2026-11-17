//! Messjournal für die Windows-Abnahme von P51 (#313 E5). Nur erfundene Daten (D12).
//!
//! Erzeugt mit dem echten simulierten Lauf ein großes, gültiges Journal: drei erfundene Werte,
//! zwei Töpfe, Drehbuch-Berater mit täglichem Handel und vier erfundenen Ereignissen je Tag.
//! Der Test ist `#[ignore]` und läuft nur ausdrücklich:
//!
//! `MESSJOURNAL_ZIEL=<ordner> MESSJOURNAL_TAGE=1000 cargo test --release -p depot_sim \
//!  --test messjournal -- --ignored --nocapture`
//!
//! Danach liegen `journal.txt` und `journal.vermerk` (Name wie im Journal-Crate) im Zielordner.

use std::path::PathBuf;

use depot_journal::Journal;
use depot_kern::{BeraterId, Cent, Datum, Herkunft, Isin, Kurs, Kurse, Vorschlag, Zeitpunkt};
use depot_sim::{Antwort, Drehbuch, Papierkonto, Szenario, Tag, Urteil, lauf};

fn kurse(datum: Datum, preise: &[(Isin, i64)]) -> Result<Kurse, String> {
    let zeit = Zeitpunkt::neu(datum, 17, 30, 0).ok_or("Zeit")?;
    let eintraege = preise
        .iter()
        .map(|(i, p)| {
            Kurs::neu(Cent(*p), "EUR", datum, zeit, Herkunft::Simulation)
                .map(|k| (i.clone(), k))
                .map_err(|e| format!("{e:?}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Kurse::neu(zeit, eintraege).map_err(|e| format!("{e:?}"))
}

fn vorschlag(wer: BeraterId, isin: &Isin, bp: i64) -> Vorschlag {
    Vorschlag {
        erwartung_bp: [(isin.clone(), Some(400))].into(),
        limit_bp: std::collections::BTreeMap::new(),
        stopp_bp: std::collections::BTreeMap::new(),
        berater: wer,
        zielgewichte_bp: [(isin.clone(), bp)].into(),
        begruendung: [(
            isin.clone(),
            "[Kursbewegung] Erfundene Messprobe ohne Marktbezug [Ausstieg +800bp 10]".to_string(),
        )]
        .into(),
        belege: std::collections::BTreeMap::new(),
        park: None,
    }
}

fn antwort(v: Vorschlag) -> Antwort {
    Antwort {
        halten: std::collections::BTreeMap::new(),
        prognose: None,
        verwertbar: true,
        einschaetzung: "Erfundene Messlage, nur für die Ladezeit des Journals.".into(),
        vorschlag: Some(v),
    }
}

fn zustimmung() -> Urteil {
    Urteil {
        verwertbar: true,
        zustimmung: true,
        begruendung: "Erfundene Zweitmeinung für das Messjournal.".into(),
        abgelehnt: None,
    }
}

#[test]
#[ignore = "erzeugt nur auf Wunsch ein großes Messjournal"]
fn erzeuge_messjournal() -> Result<(), String> {
    let ziel = PathBuf::from(std::env::var("MESSJOURNAL_ZIEL").map_err(|_| "MESSJOURNAL_ZIEL")?);
    let anzahl: usize = std::env::var("MESSJOURNAL_TAGE")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(1_000);
    let (beispiel, _, _) = Szenario::beispiel().ok_or("Beispiel")?;
    let wert = |name: &str| {
        beispiel
            .universum
            .instrumente()
            .iter()
            .find(|i| i.name == name)
            .map(|i| i.isin.clone())
            .ok_or_else(|| format!("{name} fehlt"))
    };
    let (alpha, beta, gamma) = (wert("Alpha Werke")?, wert("Beta Bau")?, wert("Gamma Netz")?);
    // Dieselben Kurse wie im Vorlauf des Beispiels (24.11.).
    let basis = [(&alpha, 5_000_i64), (&beta, 8_000), (&gamma, 2_000)];
    let mut tage = Vec::new();
    let mut dachs = Drehbuch::default();
    let mut maus = Drehbuch::default();
    // Rückwärts so beginnen, dass der letzte Handelstag vor heute liegt: Ein Journal aus der
    // Zukunft hält die Lage an und misst dann etwas anderes.
    let start = std::env::var("MESSJOURNAL_START").unwrap_or_else(|_| "2022-11-01".into());
    let mut teile = start.split('-').filter_map(|t| t.parse::<u16>().ok());
    let (j, m, t) = (teile.next(), teile.next(), teile.next());
    let mut datum = j
        .zip(m)
        .zip(t)
        .and_then(|((j, m), t)| Datum::neu(j, u8::try_from(m).ok()?, u8::try_from(t).ok()?))
        .ok_or("MESSJOURNAL_START")?;
    let vortag = datum.plus_tage(-1).ok_or("Vortag")?;
    let vorlauf = kurse(
        vortag,
        &basis
            .iter()
            .map(|(i, p)| ((*i).clone(), *p))
            .collect::<Vec<_>>(),
    )?;
    let mut nr: i64 = 0;
    let mut kalendertage = 0_usize;
    while tage.len() < anzahl {
        kalendertage = kalendertage.saturating_add(1);
        if kalendertage > anzahl.saturating_mul(3) {
            return Err(format!(
                "nach {kalendertage} Kalendertagen erst {} Handelstage",
                tage.len()
            ));
        }
        if depot_kern::liste::handelstag(datum) {
            // Kleine Schwingung um den Vorlaufkurs: kein Sprung, keine Verlustgrenze.
            let schwingung = [0_i64, 6, 12, 6, 0, -6, -12, -6];
            let idx = usize::try_from(nr.rem_euclid(8)).map_err(|e| e.to_string())?;
            let s = schwingung.get(idx).copied().unwrap_or(0);
            let preise: Vec<(Isin, i64)> = basis
                .iter()
                .map(|(i, p)| {
                    (
                        (*i).clone(),
                        p.saturating_mul(1_000_i64.saturating_add(s)) / 1_000,
                    )
                })
                .collect();
            let gewicht = if nr.rem_euclid(2) == 0 { 2_000 } else { 0 };
            dachs
                .antworten
                .insert(datum, antwort(vorschlag(BeraterId::Dachs, &alpha, gewicht)));
            maus.antworten
                .insert(datum, antwort(vorschlag(BeraterId::Maus, &beta, gewicht)));
            dachs.urteile.insert(datum, zustimmung());
            maus.urteile.insert(datum, zustimmung());
            tage.push(Tag {
                datum,
                kurse: kurse(datum, &preise)?,
                ereignisse: (1..=4)
                    .map(|k| {
                        format!(
                            "Erfundene Meldung {k} vom {datum}: Gamma Netz, Alpha Werke und Beta Bau \
                             ohne neue Angaben; Text nur für die Größe des Messjournals."
                        )
                    })
                    .collect(),
            });
            nr = nr.saturating_add(1);
        }
        datum = datum.plus_tage(1).ok_or("Datum")?;
    }
    let szenario = Szenario {
        tage,
        vorlauf,
        ..beispiel
    };
    std::fs::create_dir_all(&ziel).map_err(|e| e.to_string())?;
    let pfad = ziel.join("journal.txt");
    Journal::einrichten(&pfad).map_err(|e| format!("{e:?}"))?;
    let mut j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
    let mut konto = Papierkonto::eroeffne(&szenario.start).ok_or("Papierkonto")?;
    lauf(&mut j, &szenario, &mut dachs, &mut maus, &mut konto).map_err(|e| format!("{e:?}"))?;
    let eintraege = j.eintraege().len();
    drop(j);
    let bytes = std::fs::metadata(&pfad).map_err(|e| e.to_string())?.len();
    println!("[Messjournal] {anzahl} Handelstage, {eintraege} Einträge, {bytes} Byte");
    Ok(())
}

/// Wie lange `Journal::oeffnen` (ganze Kette gegen den Vermerk, Stand nachrechnen) für das
/// Messjournal braucht. Fünf Mal hintereinander, jeweils frisch geöffnet.
#[test]
#[ignore = "misst nur auf Wunsch"]
fn miss_oeffnen() -> Result<(), String> {
    let ziel = PathBuf::from(std::env::var("MESSJOURNAL_ZIEL").map_err(|_| "MESSJOURNAL_ZIEL")?);
    let pfad = ziel.join("journal.txt");
    let mut zeiten = Vec::new();
    for _ in 0..5 {
        let start = std::time::Instant::now();
        let j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
        zeiten.push(start.elapsed().as_millis());
        drop(j);
    }
    println!("[Messjournal] Journal::oeffnen in ms: {zeiten:?}");
    Ok(())
}
