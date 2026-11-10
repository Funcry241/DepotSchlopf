//! Auflösung der Weltprognosen (Issue #114) und Rundlauf der Prognosebelege (I5a aus #111).
//! Erfundene Daten (D12). Jede Vorbereitung prüft ihren Erfolg ausdrücklich.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{
    Art, Aufloesung, Ergebnis, Journal, JournalFehler, KursBeleg, PrognoseBeleg, Prognoserichtung,
    Weltprognose, aufloesung::zieltag, werte_aus,
};
use depot_kern::{BeraterId, Cent, Datum, Herkunft, Isin, Kurs, Preis, Zeitpunkt};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

fn neuer_pfad() -> Result<PathBuf, String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_prognosen_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
    let p = dir.join("journal.txt");
    vorbedingung(Journal::einrichten(&p), "Einrichtung")?;
    Ok(p)
}

fn datum(j: u16, m: u8, t: u8) -> Result<Datum, String> {
    Datum::neu(j, m, t).ok_or_else(|| format!("Vorbedingung Datum {j}-{m}-{t}"))
}

fn sap() -> Result<Isin, String> {
    vorbedingung(Isin::neu("DE0007164600"), "ISIN SAP")
}

fn abdruck() -> String {
    "a".repeat(64)
}

/// Belegt den Schlusskurs von SAP an `d`, abgerufen um 17:40 Uhr desselben Tages.
fn belege_kurs(j: &mut Journal, d: Datum, euro: &str) -> Result<Preis, String> {
    let zeit = Zeitpunkt::frankfurt(d, 17, 35, 0).ok_or("Vorbedingung: Schluss")?;
    let abruf = Zeitpunkt::frankfurt(d, 17, 40, 0).ok_or("Vorbedingung: Abruf")?;
    let preis = Preis::lies_euro(euro).ok_or("Vorbedingung: Preis")?;
    let kurs = vorbedingung(Kurs::exakt(preis, "EUR", d, zeit, Herkunft::Quelle), "Kurs")?;
    let beleg = vorbedingung(
        KursBeleg::neu("alphavantage", abruf, euro.as_bytes(), sap()?, kurs),
        "Kursbeleg",
    )?;
    vorbedingung(j.belege_kurs(&beleg), "Kurs belegen")?;
    Ok(preis)
}

/// Ein Papiertag mit Ereignis und Prognose, danach abgeschlossen.
fn prognosetag(
    j: &mut Journal,
    d: Datum,
    richtung: Prognoserichtung,
    handelstage: u8,
    texte: (&str, &str, &str),
) -> Result<PrognoseBeleg, String> {
    let ausgang = belege_kurs(j, d, "230")?;
    vorbedingung(j.beginne_abrechnung(d), "Abrechnungstag")?;
    let (ereignis, grund, widerlegung) = texte;
    vorbedingung(j.beobachte(d, ereignis), "Beobachtung")?;
    let beleg = PrognoseBeleg {
        datum: d,
        berater: BeraterId::Dachs,
        prognose: Weltprognose {
            isin: sap()?,
            richtung,
            handelstage,
            ereignis_nr: 1,
            begruendung: grund.into(),
            widerlegung: widerlegung.into(),
        },
        ausgangskurs: ausgang,
        ereignis: ereignis.into(),
    };
    vorbedingung(j.belege_weltprognose(&beleg), "Prognose")?;
    vorbedingung(j.schliesse_ab(d, Cent(1_000_000), &abdruck()), "Abschluss")?;
    Ok(beleg)
}

fn schliesse(j: &mut Journal, d: Datum) -> Result<(), String> {
    vorbedingung(j.beginne_abrechnung(d), "Abrechnungstag")?;
    vorbedingung(j.schliesse_ab(d, Cent(1_000_000), &abdruck()), "Abschluss")
}

/// Zieltag über Wochenende, Feiertage und Monatswechsel, an Xetra und NYSE.
#[test]
fn zieltag_zaehlt_handelstage_der_boerse_des_werts() -> Result<(), String> {
    let jpm = vorbedingung(Isin::neu("US46625H1005"), "ISIN JPM")?;
    let faelle = [
        (
            sap()?,
            datum(2026, 11, 6)?,
            1,
            datum(2026, 11, 9)?,
            "Wochenende",
        ),
        (
            sap()?,
            datum(2026, 11, 30)?,
            1,
            datum(2026, 12, 1)?,
            "Monatswechsel",
        ),
        (
            sap()?,
            datum(2026, 12, 23)?,
            1,
            datum(2026, 12, 28)?,
            "Weihnachten Xetra",
        ),
        (
            sap()?,
            datum(2027, 3, 25)?,
            1,
            datum(2027, 3, 30)?,
            "Karfreitag und Ostermontag",
        ),
        (
            sap()?,
            datum(2026, 11, 2)?,
            5,
            datum(2026, 11, 9)?,
            "fünf Handelstage",
        ),
        (
            jpm.clone(),
            datum(2026, 11, 25)?,
            1,
            datum(2026, 11, 27)?,
            "Thanksgiving NYSE",
        ),
        (
            jpm,
            datum(2027, 3, 25)?,
            1,
            datum(2027, 3, 29)?,
            "NYSE: Karfreitag zu, Ostermontag offen",
        ),
    ];
    let mut offen = Vec::new();
    for (isin, von, n, soll, fall) in faelle {
        let ist = zieltag(&isin, von, n);
        if ist != Some(soll) {
            offen.push(format!("{fall}: {ist:?} statt {soll}"));
        }
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

/// Eingetroffen, Gleichstand (verfehlt), Gegenrichtung und fehlender Kurs am Zieltag. Ein
/// Kurs vom Folgetag ersetzt den Zieltag nie. Aufgelöst wird erst nach dem Abschluss des
/// Zieltags, genau einmal, und nach dem Wiederöffnen steht dasselbe da.
#[test]
fn prognose_wird_genau_am_zieltag_aufgeloest() -> Result<(), String> {
    let tag1 = datum(2026, 11, 2)?;
    let tag2 = datum(2026, 11, 3)?;
    let tag3 = datum(2026, 11, 4)?;
    let faelle = [
        (
            Prognoserichtung::Steigt,
            Some("231"),
            Ergebnis::Eingetroffen,
        ),
        (Prognoserichtung::Steigt, Some("230"), Ergebnis::Verfehlt),
        (Prognoserichtung::Faellt, Some("230"), Ergebnis::Verfehlt),
        (
            Prognoserichtung::Faellt,
            Some("229"),
            Ergebnis::Eingetroffen,
        ),
        (Prognoserichtung::Steigt, Some("229"), Ergebnis::Verfehlt),
        (Prognoserichtung::Steigt, None, Ergebnis::OhneKurs),
    ];
    let mut offen = Vec::new();
    for (richtung, schluss, soll) in faelle {
        let p = neuer_pfad()?;
        let mut j = vorbedingung(Journal::oeffnen(&p), "Journal")?;
        let beleg = prognosetag(&mut j, tag1, richtung, 1, ("EZB-Sitzung", "Grund", "Gegen"))?;
        if let Some(euro) = schluss {
            belege_kurs(&mut j, tag2, euro)?;
        }
        // Vor dem Abschluss des Zieltags ist nichts fällig.
        if vorbedingung(j.loese_prognosen_auf(), "zu früh")? != 0 {
            offen.push(format!(
                "{richtung:?} {schluss:?}: vor dem Zieltag aufgelöst"
            ));
        }
        schliesse(&mut j, tag2)?;
        // Ein Kurs vom Folgetag zählt nicht.
        belege_kurs(&mut j, tag3, "999")?;
        schliesse(&mut j, tag3)?;
        if vorbedingung(j.loese_prognosen_auf(), "Auflösung")? != 1 {
            offen.push(format!(
                "{richtung:?} {schluss:?}: nicht genau eine Auflösung"
            ));
        }
        let a = j.stand().aufloesungen().first().cloned();
        if a.as_ref().map(|a| a.ergebnis) != Some(soll)
            || a.as_ref().is_none_or(|a| a.zieltag != tag2)
        {
            offen.push(format!(
                "{richtung:?} {schluss:?}: {a:?} statt {soll:?} am {tag2}"
            ));
        }
        if !a.as_ref().is_some_and(|a| a.zu(&beleg)) {
            offen.push(format!(
                "{richtung:?} {schluss:?}: Auflösung gehört nicht zur Prognose"
            ));
        }
        // Ein zweites Mal ist nichts fällig, und von Hand geht es nicht.
        if vorbedingung(j.loese_prognosen_auf(), "zweite Auflösung")? != 0 {
            offen.push(format!("{richtung:?} {schluss:?}: doppelt aufgelöst"));
        }
        if !matches!(
            j.anhaengen(
                Art::Aufloesung,
                "2026-11-02;Dachs;DE0007164600;1;2026-11-03;-;ohne_kurs"
            ),
            Err(JournalFehler::Abgelehnt(_))
        ) {
            offen.push(format!(
                "{richtung:?} {schluss:?}: Auflösung von Hand angenommen"
            ));
        }
        let vorher = j.stand().aufloesungen().to_vec();
        drop(j);
        let j = vorbedingung(Journal::oeffnen(&p), "Wiederöffnen")?;
        if j.stand().aufloesungen() != vorher.as_slice() {
            offen.push(format!(
                "{richtung:?} {schluss:?}: nach dem Wiederöffnen anders"
            ));
        }
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

/// Eine Auflösung in der Datei, die nicht aus dem Stand folgt, ist Halt.
#[test]
fn falsche_aufloesung_in_der_datei_ist_halt() -> Result<(), String> {
    let p = neuer_pfad()?;
    let mut j = vorbedingung(Journal::oeffnen(&p), "Journal")?;
    prognosetag(
        &mut j,
        datum(2026, 11, 2)?,
        Prognoserichtung::Steigt,
        1,
        ("E", "G", "W"),
    )?;
    belege_kurs(&mut j, datum(2026, 11, 3)?, "231")?;
    schliesse(&mut j, datum(2026, 11, 3)?)?;
    let mut eintraege: Vec<(Art, String)> = j.eintraege().to_vec();
    drop(j);
    // Richtig wäre „eingetroffen“ zu 231 €.
    eintraege.push((
        Art::Aufloesung,
        "2026-11-02;Dachs;DE0007164600;1;2026-11-03;230,000000;verfehlt".into(),
    ));
    let roh: Vec<(Art, &str)> = eintraege.iter().map(|(a, t)| (*a, t.as_str())).collect();
    schreib(&p, &roh)?;
    match Journal::oeffnen(&p) {
        Err(JournalFehler::Halt(_)) => Ok(()),
        anders => Err(format!(
            "falsche Auflösung: {:?}",
            anders.map(|_| "gelesen")
        )),
    }
}

fn schreib(p: &Path, eintraege: &[(Art, &str)]) -> Result<(), String> {
    let inhalt = depot_journal::inhalt_fuer(eintraege);
    let v = depot_journal::vermerk_fuer(&inhalt, 0).ok_or("Vorbedingung: Vermerk")?;
    vorbedingung(fs::write(p, &inhalt), "Datei")?;
    vorbedingung(fs::write(p.with_extension("vermerk"), v), "Vermerk")
}

/// Rundlauf aus #111: Prognosen mit `;`, `%`, `%3B`, `%25` und Umlauten in allen Texten
/// lesen sich nach dem Wiederöffnen genau so, wie sie geschrieben wurden.
#[test]
fn prognosebelege_lesen_sich_genau_so_wie_sie_geschrieben_wurden() -> Result<(), String> {
    let texte = [
        (
            "Zins;entscheid %3B",
            "steigt; weil %25",
            "außer ä;ö;ü %253B",
        ),
        ("100 % sicher?", "%%;;", "\"zitiert\" \\ und :"),
        ("€ und ß", "%3B%25%3B", ";"),
    ];
    let p = neuer_pfad()?;
    let mut geschrieben = Vec::new();
    {
        let mut j = vorbedingung(Journal::oeffnen(&p), "Journal")?;
        let mut d = datum(2026, 11, 2)?;
        for t in texte {
            geschrieben.push(prognosetag(&mut j, d, Prognoserichtung::Faellt, 20, t)?);
            d = d.plus_tage(1).ok_or("Datum")?;
        }
    }
    let j = vorbedingung(Journal::oeffnen(&p), "Wiederöffnen")?;
    if j.stand().weltprognosen() == geschrieben.as_slice() {
        Ok(())
    } else {
        Err(format!(
            "geschrieben {geschrieben:#?}\ngelesen {:#?}",
            j.stand().weltprognosen()
        ))
    }
}

/// Die Auswertung zählt nur Fälle mit Kurs und vergleicht mit „steigt immer“ auf genau
/// denselben Fällen.
#[test]
fn auswertung_vergleicht_mit_steigt_immer() -> Result<(), String> {
    let d = datum(2026, 11, 2)?;
    let prognose = |nr: usize, richtung| -> Result<PrognoseBeleg, String> {
        Ok(PrognoseBeleg {
            datum: d,
            berater: BeraterId::Maus,
            prognose: Weltprognose {
                isin: sap()?,
                richtung,
                handelstage: 1,
                ereignis_nr: nr,
                begruendung: "G".into(),
                widerlegung: "W".into(),
            },
            ausgangskurs: Preis::aus_fein(2_300_000),
            ereignis: "E".into(),
        })
    };
    let aufl = |p: &PrognoseBeleg, schluss: Option<i64>| Aufloesung {
        datum: p.datum,
        berater: p.berater,
        isin: p.prognose.isin.clone(),
        ereignis_nr: p.prognose.ereignis_nr,
        zieltag: d,
        schluss: schluss.map(Preis::aus_fein),
        ergebnis: depot_journal::aufloesung::bewerte(p, schluss.map(Preis::aus_fein)),
    };
    let p1 = prognose(1, Prognoserichtung::Faellt)?;
    let p2 = prognose(2, Prognoserichtung::Faellt)?;
    let p3 = prognose(3, Prognoserichtung::Steigt)?;
    let p4 = prognose(4, Prognoserichtung::Steigt)?;
    let prognosen = vec![p1.clone(), p2.clone(), p3.clone(), p4.clone()];
    let aufloesungen = vec![
        aufl(&p1, Some(2_200_000)), // fällt: eingetroffen, naiv falsch
        aufl(&p2, Some(2_400_000)), // fällt: verfehlt, naiv richtig
        aufl(&p3, Some(2_400_000)), // steigt: eingetroffen, naiv richtig
        aufl(&p4, None),            // ohne Kurs: zählt nicht
    ];
    let a = werte_aus(&prognosen, &aufloesungen, Some(BeraterId::Maus)).ok_or("Auswertung")?;
    let mut offen = Vec::new();
    if (a.faelle, a.treffer, a.quote_bp, a.naiv_bp) != (3, 2, 6_666, 6_666) {
        offen.push(format!("{a:?}"));
    }
    if a.besser_als_naiv() || !a.hinweis().contains("noch nicht unterscheidbar (3 Fälle)") {
        offen.push(a.hinweis());
    }
    if werte_aus(&prognosen, &aufloesungen, Some(BeraterId::Dachs)).is_some() {
        offen.push("Dachs ohne Fälle hat eine Auswertung".into());
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}
