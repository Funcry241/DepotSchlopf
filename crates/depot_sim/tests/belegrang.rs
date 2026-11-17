//! Grundrang für die Belegauswahl (#325): Ein Journal mit einer Weltprognose, deren Beleg zu
//! einer bestehenden These gehört. Die heutige Reihenfolge zeigt ihn erst an vierter Stelle,
//! die Grundrang unter den ersten drei.

use std::fs;

use depot_journal::{Journal, KursBeleg, PrognoseBeleg, Prognoserichtung, Weltprognose};
use depot_kern::liste::{als_universum, beobachtungsliste};
use depot_kern::{BeraterId, Datum, Herkunft, Isin, Kurs, Preis, Zeitpunkt};
use depot_sim::belegrang::{Grundrang, Heute, Messung, kandidaten_am, messe, thesen_vor};

fn tag(t: u8) -> Result<Datum, String> {
    Datum::neu(2026, 11, t).ok_or_else(|| format!("Tag {t}"))
}

#[test]
fn grundrang_findet_den_beleg_zur_these() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("depot_belegrang_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let pfad = dir.join("journal.txt");
    Journal::einrichten(&pfad).map_err(|e| format!("{e:?}"))?;
    let mut j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
    let universum = als_universum(&beobachtungsliste()?);
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
    for text in gesehen {
        j.beobachte(tag(16)?, text).map_err(|e| format!("{e:?}"))?;
    }
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
            ereignis_nr: 4,
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
    };
    let woche = tag(16)?..=tag(20)?;
    assert_eq!(messe(j.stand(), &universum, &Heute, woche.clone()), alle(0));
    assert_eq!(messe(j.stand(), &universum, &Grundrang, woche), alle(1));
    let davor = tag(9)?..=tag(13)?;
    assert_eq!(
        messe(j.stand(), &universum, &Grundrang, davor),
        Messung::default()
    );
    drop(j);
    fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}
