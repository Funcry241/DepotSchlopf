//! Grundrang für die Belegauswahl (#325): Ein Journal mit einer Weltprognose, deren Beleg zu
//! einer bestehenden These gehört. Die heutige Reihenfolge zeigt ihn erst an vierter Stelle,
//! die Grundrang unter den ersten drei.

use std::fs;

use depot_journal::{Journal, KursBeleg, PrognoseBeleg, Prognoserichtung, Weltprognose};
use depot_kern::liste::{als_universum, beobachtungsliste};
use depot_kern::{BeraterId, Datum, Herkunft, Isin, Kurs, Preis, Zeitpunkt};
use depot_sim::belegrang::{
    Grundrang, Heute, KAUFBELEG, Kaufbeleg, Messung, kandidaten_am, kaufbelege, messe, thesen_vor,
};

fn tag(t: u8) -> Result<Datum, String> {
    Datum::neu(2026, 11, t).ok_or_else(|| format!("Tag {t}"))
}

fn journal(name: &str) -> Result<(std::path::PathBuf, Journal), String> {
    let dir = std::env::temp_dir().join(format!("depot_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let pfad = dir.join("journal.txt");
    Journal::einrichten(&pfad).map_err(|e| format!("{e:?}"))?;
    let j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
    Ok((dir, j))
}

/// Gegenprobe Beleg↔These (#325): Ein Kauf nennt die Ereignisse 2 und 4 des Tages. Die
/// heutige Reihenfolge zeigt Nr. 2 unter den ersten drei, der Grundrang wegen der SAP-These
/// Nr. 4. Fremde oder kaputte Kaufbelegzeilen entfallen beim Lesen, ohne Fehler.
#[test]
fn kaufbelege_zaehlen_in_der_messung() -> Result<(), String> {
    let (dir, mut j) = journal("kaufbeleg")?;
    let werte = beobachtungsliste()?;
    let universum = als_universum(&werte);
    let grundrang = Grundrang::neu(&werte);
    j.begruende(
        tag(13)?,
        "Dachs",
        "Topf A: DE0007164600 2000 bp: Auftragslage",
    )
    .map_err(|e| format!("{e:?}"))?;
    for text in [
        "Gold",
        "Autobauer",
        "Öl",
        "Zinsen",
        "SAP hebt die Prognose an",
    ] {
        j.beobachte(tag(16)?, text).map_err(|e| format!("{e:?}"))?;
    }
    for (wer, text) in [
        (KAUFBELEG, "7 Dachs 5"),
        (KAUFBELEG, "8 Maus 0"),
        (KAUFBELEG, "9 Igel 1"),
        (KAUFBELEG, "x Maus 1"),
        (KAUFBELEG, "10 Maus 1 mehr"),
        ("Kaufthese", "11 Maus 1"),
    ] {
        j.begruende(tag(16)?, wer, text)
            .map_err(|e| format!("{e:?}"))?;
    }
    assert_eq!(
        kaufbelege(j.stand()),
        vec![Kaufbeleg {
            datum: tag(16)?,
            auftrag: 7,
            berater: BeraterId::Dachs,
            nummern: vec![5],
        }]
    );
    let woche = tag(16)?..=tag(20)?;
    let kauf = |kauf_unter_drei| Messung {
        kauf_faelle: 1,
        kauf_unter_drei,
        ..Messung::default()
    };
    assert_eq!(messe(j.stand(), &universum, &Heute, woche.clone()), kauf(0));
    assert_eq!(messe(j.stand(), &universum, &grundrang, woche), kauf(1));
    drop(j);
    fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

#[test]
fn grundrang_findet_den_beleg_zur_these() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("depot_belegrang_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let pfad = dir.join("journal.txt");
    Journal::einrichten(&pfad).map_err(|e| format!("{e:?}"))?;
    let mut j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
    let werte = beobachtungsliste()?;
    let universum = als_universum(&werte);
    let grundrang = Grundrang::neu(&werte);
    let sap = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
    // Am Freitag nennt Dachs SAP als Ziel; das ist die bestehende These vom Montag.
    j.begruende(
        tag(13)?,
        "Dachs",
        "Topf A: DE0007164600 2000 bp: Auftragslage",
    )
    .map_err(|e| format!("{e:?}"))?;
    // Montag: Die Frage sah vier Belege, die SAP-Meldung an vierter Stelle.
    let gesehen = [
        "Gold auf Rekordhoch",
        "Autobauer im Minus",
        "Ölpreis steigt",
        "SAP hebt die Jahresprognose an",
    ];
    j.beobachte(tag(16)?, "Tiingo-Normalisierung: Kurs gerundet")
        .map_err(|e| format!("{e:?}"))?;
    for text in gesehen {
        j.beobachte(tag(16)?, text).map_err(|e| format!("{e:?}"))?;
    }
    j.begruende(tag(16)?, "Frageliste", "1 4")
        .map_err(|e| format!("{e:?}"))?;
    j.begruende(tag(16)?, KAUFBELEG, "7 Dachs 5")
        .map_err(|e| format!("{e:?}"))?;
    j.beobachte(tag(16)?, "Spätere Beobachtung")
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(kandidaten_am(j.stand(), tag(16)?), gesehen);
    let ereignis = gesehen[3].to_string();
    let schluss = Zeitpunkt::frankfurt(tag(16)?, 17, 35, 0).ok_or("Schluss")?;
    let abruf = Zeitpunkt::frankfurt(tag(16)?, 17, 40, 0).ok_or("Abruf")?;
    let preis = Preis::lies_euro("230").ok_or("Preis")?;
    let kurs = Kurs::exakt(preis, "EUR", tag(16)?, schluss, Herkunft::Quelle)
        .map_err(|e| format!("{e:?}"))?;
    let beleg = KursBeleg::neu("alphavantage", abruf, b"230", sap.clone(), kurs)
        .map_err(|e| format!("{e:?}"))?;
    j.belege_kurs(&beleg).map_err(|e| format!("{e:?}"))?;
    j.beginne_abrechnung(tag(16)?)
        .map_err(|e| format!("{e:?}"))?;
    j.belege_weltprognose(&PrognoseBeleg {
        datum: tag(16)?,
        berater: BeraterId::Dachs,
        prognose: Weltprognose {
            isin: sap.clone(),
            richtung: Prognoserichtung::Steigt,
            handelstage: 3,
            ereignis_nr: 5,
            begruendung: "Prognose angehoben".into(),
            widerlegung: "Kurs fällt trotzdem".into(),
        },
        ausgangskurs: preis,
        ereignis,
    })
    .map_err(|e| format!("{e:?}"))?;
    let thesen = thesen_vor(j.stand(), &universum, BeraterId::Dachs, tag(16)?);
    assert_eq!(
        thesen.iter().map(|t| &t.isin).collect::<Vec<_>>(),
        vec![&sap]
    );
    assert!(thesen_vor(j.stand(), &universum, BeraterId::Dachs, tag(13)?).is_empty());
    let alle = |unter_drei| Messung {
        faelle: 1,
        unter_drei,
        kauf_faelle: 1,
        kauf_unter_drei: unter_drei,
    };
    let woche = tag(16)?..=tag(20)?;
    assert_eq!(messe(j.stand(), &universum, &Heute, woche.clone()), alle(0));
    assert_eq!(messe(j.stand(), &universum, &grundrang, woche), alle(1));
    let davor = tag(9)?..=tag(13)?;
    assert_eq!(
        messe(j.stand(), &universum, &grundrang, davor),
        Messung::default()
    );
    drop(j);
    fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}
