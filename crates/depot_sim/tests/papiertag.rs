//! Der Papiertag mit belegten Schlusskursen. Alle Zahlen sind fuer den Test gesetzt; die
//! Kurse tragen die Herkunft `Quelle`, wie der Sammler sie belegt. Jede Vorbereitung prueft
//! ihren Erfolg ausdruecklich und endet sonst mit „Vorbedingung …“.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{Art, Journal, KursBeleg};
use depot_kern::{
    BeraterId, Cent, Datum, Herkunft, Instrument, InstrumentArt, Isin, Kurs, Preis, Topf, TopfId,
    Universum, Vorschlag, Zeitpunkt,
};
use depot_sim::{
    Antwort, Ausgang, Drehbuch, Gefragt, LaufFehler, Papierdatei, Papierkonto, Stumm, Urteil,
    gefragt, papiertag,
};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

fn some<T>(o: Option<T>, was: &str) -> Result<T, String> {
    o.ok_or_else(|| format!("Vorbedingung {was}"))
}

struct Ort {
    dir: PathBuf,
    journal: PathBuf,
    konto: PathBuf,
}

impl Ort {
    fn neu() -> Result<Ort, String> {
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("depot_papier_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
        let journal = dir.join("journal.txt");
        vorbedingung(Journal::einrichten(&journal), "Einrichtung")?;
        let konto = dir.join("papierkonto.txt");
        Ok(Ort {
            dir,
            journal,
            konto,
        })
    }

    fn oeffnen(&self) -> Result<Journal, String> {
        vorbedingung(Journal::oeffnen(&self.journal), "Journal oeffnen")
    }
}

impl Drop for Ort {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn tag(t: u8) -> Result<Datum, String> {
    some(Datum::neu(2026, 11, t), "Datum")
}

fn sap() -> Result<Isin, String> {
    vorbedingung(Isin::neu("DE0007164600"), "ISIN SAP")
}

fn allianz() -> Result<Isin, String> {
    vorbedingung(Isin::neu("DE0008404005"), "ISIN Allianz")
}

fn universum() -> Result<Universum, String> {
    Ok(Universum::neu(
        [("SAP", sap()?), ("Allianz", allianz()?)]
            .into_iter()
            .map(|(name, isin)| Instrument {
                isin,
                name: name.into(),
                art: InstrumentArt::Aktie,
            })
            .collect(),
    ))
}

fn start() -> Result<Vec<Topf>, String> {
    Ok(vec![
        vorbedingung(Topf::neu(TopfId::A, Cent(500_000)), "Topf A")?,
        vorbedingung(Topf::neu(TopfId::B, Cent(500_000)), "Topf B")?,
    ])
}

/// Belegt wie der Sammler: je Wert der Vortag und der Tag, abgerufen um 17:40 Uhr.
fn belege(j: &mut Journal, t: u8, preise: &[(&Isin, &str, &str)]) -> Result<(), String> {
    let heute = tag(t)?;
    let vortag = tag(t.saturating_sub(1))?;
    let abruf = some(Zeitpunkt::frankfurt(heute, 17, 40, 0), "Abruf")?;
    for (isin, alt, neu) in preise {
        for (d, p) in [(vortag, alt), (heute, neu)] {
            let zeit = some(Zeitpunkt::frankfurt(d, 17, 35, 0), "Schluss")?;
            let preis = some(Preis::lies_euro(p), "Preis")?;
            let kurs = vorbedingung(Kurs::exakt(preis, "EUR", d, zeit, Herkunft::Quelle), "Kurs")?;
            let beleg = vorbedingung(
                KursBeleg::neu("alphavantage", abruf, p.as_bytes(), (*isin).clone(), kurs),
                "Beleg",
            )?;
            vorbedingung(j.belege_kurs(&beleg), "Beleg schreiben")?;
        }
    }
    Ok(())
}

fn konto(ort: &Ort, j: &Journal) -> Result<Papierdatei, String> {
    Papierdatei::oeffnen(&ort.konto, !j.stand().toepfe().is_empty(), &start()?)
}

fn stumm_tag(ort: &Ort, heute: u8) -> Result<Result<Ausgang, LaufFehler>, String> {
    let mut j = ort.oeffnen()?;
    let mut k = konto(ort, &j)?;
    Ok(papiertag(
        &mut j,
        tag(heute)?,
        &universum()?,
        &start()?,
        &mut Stumm,
        &mut Stumm,
        &mut k,
    ))
}

#[test]
fn erster_papiertag_eroeffnet_depot_und_schliesst_ab() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    belege(
        &mut j,
        5,
        &[(&sap()?, "230.10", "231.00"), (&allianz()?, "350", "351.5")],
    )?;
    drop(j);
    let aus = stumm_tag(&ort, 5)?;
    assert!(
        matches!(aus, Ok(Ausgang::Abgeschlossen { .. })),
        "Ausgang: {aus:?}"
    );
    let j = ort.oeffnen()?;
    assert_eq!(j.stand().toepfe().len(), 2);
    assert_eq!(j.stand().abschluss().map(|a| a.datum), Some(tag(5)?));
    assert!(
        j.stand().ki().buchungen().is_empty(),
        "ohne Anbindung wird nichts reserviert"
    );
    let texte: Vec<&str> = j
        .stand()
        .begruendungen()
        .iter()
        .map(|(_, _, t)| t.as_str())
        .collect();
    assert!(
        texte.iter().any(|t| t.contains("nicht angebunden")),
        "{texte:?}"
    );
    // Stumm nennt keinen Grund; im Journal steht dann keine Klammer.
    assert!(
        !texte.iter().any(|t| t.contains("nicht angebunden (")),
        "{texte:?}"
    );
    assert!(ort.konto.is_file(), "Papierkonto als Datei");
    drop(j);
    // Derselbe Tag noch einmal: nichts geschieht.
    let vorher = vorbedingung(fs::read(&ort.journal), "Journal lesen")?;
    let aus = stumm_tag(&ort, 5)?;
    assert_eq!(aus, Ok(Ausgang::SchonAbgeschlossen(tag(5)?)));
    assert_eq!(
        vorbedingung(fs::read(&ort.journal), "Journal lesen")?,
        vorher
    );
    Ok(())
}

#[test]
fn kurse_nicht_von_heute_halten_ohne_eintrag() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    belege(
        &mut j,
        5,
        &[(&sap()?, "230", "231"), (&allianz()?, "350", "351")],
    )?;
    drop(j);
    let vorher = vorbedingung(fs::read(&ort.journal), "Journal lesen")?;
    let aus = stumm_tag(&ort, 6)?;
    assert!(matches!(aus, Err(LaufFehler::Halt(_))), "{aus:?}");
    assert_eq!(
        vorbedingung(fs::read(&ort.journal), "Journal lesen")?,
        vorher
    );
    Ok(())
}

#[test]
fn kursbelege_nach_abschluss_beginnen_keinen_tag() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    belege(
        &mut j,
        5,
        &[(&sap()?, "230", "231"), (&allianz()?, "350", "351")],
    )?;
    drop(j);
    stumm_tag(&ort, 5)?.map_err(|e| format!("Tag 1: {e:?}"))?;
    let mut j = ort.oeffnen()?;
    belege(
        &mut j,
        6,
        &[(&sap()?, "231", "232"), (&allianz()?, "351", "352")],
    )?;
    assert_eq!(j.stand().seit_abschluss(), 0);
    drop(j);
    let aus = stumm_tag(&ort, 6)?;
    assert!(
        matches!(aus, Ok(Ausgang::Abgeschlossen { .. })),
        "Ausgang: {aus:?}"
    );
    Ok(())
}

#[test]
fn veraendertes_papierkonto_haelt_beim_abgleich() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    belege(
        &mut j,
        5,
        &[(&sap()?, "230", "231"), (&allianz()?, "350", "351")],
    )?;
    drop(j);
    stumm_tag(&ort, 5)?.map_err(|e| format!("Tag 1: {e:?}"))?;
    let text = vorbedingung(fs::read_to_string(&ort.konto), "Konto lesen")?;
    let falsch = text.replace("bargeld;1000000", "bargeld;1000001");
    assert_ne!(falsch, text, "Vorbedingung: Bargeld in der Datei");
    vorbedingung(fs::write(&ort.konto, falsch), "Konto schreiben")?;
    let mut j = ort.oeffnen()?;
    belege(
        &mut j,
        6,
        &[(&sap()?, "231", "232"), (&allianz()?, "351", "352")],
    )?;
    drop(j);
    let aus = stumm_tag(&ort, 6)?;
    assert!(
        matches!(&aus, Err(LaufFehler::Halt(t)) if t.contains("Kontowaechter")),
        "{aus:?}"
    );
    Ok(())
}

#[test]
fn papierkonto_datei_passt_nur_zum_depot() -> Result<(), String> {
    let ort = Ort::neu()?;
    // Depot im Journal, aber keine Datei: Halt.
    assert!(Papierdatei::oeffnen(&ort.konto, true, &start()?).is_err());
    // Neues Konto ohne Depot: wird angelegt; unveraendert ohne Depot ist es eine angefangene
    // Einrichtung und wird fortgesetzt (K23-03).
    Papierdatei::oeffnen(&ort.konto, false, &start()?)?;
    Papierdatei::oeffnen(&ort.konto, false, &start()?)?;
    // Zu anderen Starttoepfen passt es nicht.
    let anders = [vorbedingung(Topf::neu(TopfId::A, Cent(1)), "Topf")?];
    assert!(Papierdatei::oeffnen(&ort.konto, false, &anders).is_err());
    Ok(())
}

#[test]
fn papierkonto_text_nur_in_eigener_form() -> Result<(), String> {
    let k = some(Papierkonto::eroeffne(&start()?), "Konto")?;
    let text = k.als_text();
    assert_eq!(Papierkonto::aus_text(&text), Ok(k));
    assert!(Papierkonto::aus_text(&text.replace("bargeld;", "bargeld;0")).is_err());
    assert!(Papierkonto::aus_text(&format!("{text}unbekannt;1\n")).is_err());
    assert!(Papierkonto::aus_text("").is_err());
    Ok(())
}

/// Dachs kauft SAP zu einem Schluss feiner als ein Cent; Maus stimmt zu. Ausgefuehrt wird zum
/// aufgerundeten Cent, Journal und Papierkonto stimmen danach ueberein.
#[test]
fn kauf_zu_feinem_kurs_wird_aufgerundet_ausgefuehrt() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (sap, allianz) = (sap()?, allianz()?);
    let mut j = ort.oeffnen()?;
    belege(
        &mut j,
        5,
        &[(&sap, "230", "230.105"), (&allianz, "350", "351")],
    )?;
    let d = tag(5)?;
    let mut dachs = Drehbuch::default();
    dachs.antworten.insert(
        d,
        Antwort {
            verwertbar: true,
            einschaetzung: "SAP nach Zahlen".into(),
            vorschlag: Some(Vorschlag {
                berater: BeraterId::Dachs,
                zielgewichte_bp: [(sap.clone(), 2_000)].into_iter().collect(),
                begruendung: [(sap.clone(), "beobachtet".to_string())]
                    .into_iter()
                    .collect(),
            }),
        },
    );
    let mut maus = Drehbuch::default();
    maus.urteile.insert(
        d,
        Urteil {
            verwertbar: true,
            zustimmung: true,
            begruendung: "passt".into(),
        },
    );
    let mut k = konto(&ort, &j)?;
    let aus = papiertag(
        &mut j,
        d,
        &universum()?,
        &start()?,
        &mut dachs,
        &mut maus,
        &mut k,
    );
    assert!(
        matches!(aus, Ok(Ausgang::Abgeschlossen { .. })),
        "Ausgang: {aus:?}"
    );
    let a = some(j.stand().toepfe().get(&TopfId::A), "Topf A")?;
    let stueck = a.stueck(&sap);
    assert!(stueck > 0, "SAP gekauft");
    let ausfuehrungen: Vec<_> = j
        .stand()
        .auftraege()
        .iter()
        .flat_map(|l| l.ausfuehrungen.clone())
        .collect();
    assert!(
        ausfuehrungen.iter().all(|x| x.kurs == Cent(23_011)),
        "zum aufgerundeten Cent: {ausfuehrungen:?}"
    );
    assert!(j.gleiche_ab(&k).map_err(|e| format!("{e:?}"))?.is_empty());
    // Das Konto in der Datei ist das Konto des Laufs.
    let datei = vorbedingung(fs::read_to_string(&ort.konto), "Konto lesen")?;
    assert_eq!(datei, k.konto().als_text());
    assert!(
        j.eintraege().iter().any(|(art, _)| *art == Art::Abschluss),
        "abgeschlossen"
    );
    Ok(())
}

/// Ein Kauf von Dachs mit Zustimmung von Maus am Tag `t`.
fn kaeufer(d: Datum, sap: &Isin) -> (Drehbuch, Drehbuch) {
    let mut dachs = Drehbuch::default();
    dachs.antworten.insert(
        d,
        Antwort {
            verwertbar: true,
            einschaetzung: "SAP".into(),
            vorschlag: Some(Vorschlag {
                berater: BeraterId::Dachs,
                zielgewichte_bp: [(sap.clone(), 2_000)].into_iter().collect(),
                begruendung: [(sap.clone(), "beobachtet".to_string())]
                    .into_iter()
                    .collect(),
            }),
        },
    );
    let mut maus = Drehbuch::default();
    maus.urteile.insert(
        d,
        Urteil {
            verwertbar: true,
            zustimmung: true,
            begruendung: "passt".into(),
        },
    );
    (dachs, maus)
}

/// Maus' Gegenprobe 1 (PR #23): Laesst sich das Papierkonto nicht schreiben, haelt der Tag an,
/// bevor das Journal eine Ausfuehrung oder den Abschluss bestaetigt.
#[test]
fn schreibfehler_des_kontos_haelt_vor_jeder_bestaetigung() -> Result<(), String> {
    let ort = Ort::neu()?;
    let sap = sap()?;
    let mut j = ort.oeffnen()?;
    belege(
        &mut j,
        5,
        &[(&sap, "230", "231"), (&allianz()?, "350", "351")],
    )?;
    let mut k = konto(&ort, &j)?;
    let vorher = vorbedingung(fs::read_to_string(&ort.konto), "Konto lesen")?;
    // Die Nebendatei ist ein Ordner: Jedes Schreiben des Kontos scheitert.
    vorbedingung(
        fs::create_dir_all(ort.konto.with_extension("txt.neu")),
        "Schreiben blockieren",
    )?;
    let d = tag(5)?;
    let (mut dachs, mut maus) = kaeufer(d, &sap);
    let aus = papiertag(
        &mut j,
        d,
        &universum()?,
        &start()?,
        &mut dachs,
        &mut maus,
        &mut k,
    );
    assert!(matches!(aus, Err(LaufFehler::Halt(_))), "{aus:?}");
    assert!(j.stand().abschluss().is_none(), "kein Abschluss");
    assert!(
        j.stand()
            .auftraege()
            .iter()
            .all(|l| l.ausfuehrungen.is_empty()),
        "keine bestaetigte Ausfuehrung"
    );
    assert_eq!(
        vorbedingung(fs::read_to_string(&ort.konto), "Konto lesen")?,
        vorher
    );
    Ok(())
}

/// Maus' Gegenprobe 2 (PR #23): Auch ein schon abgeschlossener Tag gleicht zuerst ab.
#[test]
fn schon_abgeschlossen_erst_nach_dem_abgleich() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    belege(
        &mut j,
        5,
        &[(&sap()?, "230", "231"), (&allianz()?, "350", "351")],
    )?;
    drop(j);
    stumm_tag(&ort, 5)?.map_err(|e| format!("Tag 1: {e:?}"))?;
    let text = vorbedingung(fs::read_to_string(&ort.konto), "Konto lesen")?;
    let falsch = text.replace("bargeld;1000000", "bargeld;1000001");
    assert_ne!(falsch, text, "Vorbedingung: Bargeld in der Datei");
    vorbedingung(fs::write(&ort.konto, falsch), "Konto schreiben")?;
    let aus = stumm_tag(&ort, 5)?;
    assert!(
        matches!(&aus, Err(LaufFehler::Halt(t)) if t.contains("Kontowaechter")),
        "{aus:?}"
    );
    Ok(())
}

/// Maus' Gegenprobe 3 (PR #23): Ein Erststart, der vor dem Depot anhaelt, laesst sich
/// wiederholen, sobald Kurse da sind.
#[test]
fn erststart_ohne_kurse_ist_wiederholbar() -> Result<(), String> {
    let ort = Ort::neu()?;
    let aus = stumm_tag(&ort, 5)?;
    assert!(matches!(aus, Err(LaufFehler::Halt(_))), "{aus:?}");
    let mut j = ort.oeffnen()?;
    assert!(j.stand().toepfe().is_empty(), "Vorbedingung: kein Depot");
    belege(
        &mut j,
        5,
        &[(&sap()?, "230", "231"), (&allianz()?, "350", "351")],
    )?;
    drop(j);
    let aus = stumm_tag(&ort, 5)?;
    assert!(
        matches!(aus, Ok(Ausgang::Abgeschlossen { .. })),
        "Ausgang: {aus:?}"
    );
    Ok(())
}

/// Ein angefangenes Konto mit Bewegung gilt ohne Depot im Journal nicht als Einrichtung.
#[test]
fn konto_mit_bewegung_ohne_depot_haelt() -> Result<(), String> {
    let ort = Ort::neu()?;
    Papierdatei::oeffnen(&ort.konto, false, &start()?)?;
    let text = vorbedingung(fs::read_to_string(&ort.konto), "Konto lesen")?;
    vorbedingung(
        fs::write(&ort.konto, text.replace("sendungen;0", "sendungen;1")),
        "Konto schreiben",
    )?;
    assert!(Papierdatei::oeffnen(&ort.konto, false, &start()?).is_err());
    Ok(())
}

/// Kostenschutz im Papiertag: Nach 23 Papiertagen mit Vorschlag und Zweitmeinung ist der
/// Monat beider KIs voll gebucht (46 Aufrufe). Am 24. Tag fragt der Papiertag niemanden, bucht
/// nichts und schreibt den Grund ins Journal. Der Tag schliesst trotzdem ab.
#[test]
fn voller_monat_fragt_niemanden() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (sap, allianz) = (sap()?, allianz()?);
    let mut dachs = Drehbuch::default();
    let mut maus = Drehbuch::default();
    for t in 2..=25_u8 {
        let d = tag(t)?;
        for (buch, wer) in [(&mut dachs, BeraterId::Dachs), (&mut maus, BeraterId::Maus)] {
            buch.antworten.insert(
                d,
                Antwort {
                    verwertbar: true,
                    einschaetzung: "SAP".into(),
                    vorschlag: Some(Vorschlag {
                        berater: wer,
                        zielgewichte_bp: [(sap.clone(), 2_000)].into_iter().collect(),
                        begruendung: [(sap.clone(), "beobachtet".to_string())]
                            .into_iter()
                            .collect(),
                    }),
                },
            );
            buch.urteile.insert(
                d,
                Urteil {
                    verwertbar: true,
                    zustimmung: false,
                    begruendung: "abwarten".into(),
                },
            );
        }
    }
    for t in 2..=25_u8 {
        let mut j = ort.oeffnen()?;
        belege(&mut j, t, &[(&sap, "230", "231"), (&allianz, "350", "351")])?;
        let mut k = konto(&ort, &j)?;
        let aus = papiertag(
            &mut j,
            tag(t)?,
            &universum()?,
            &start()?,
            &mut dachs,
            &mut maus,
            &mut k,
        );
        assert!(
            matches!(aus, Ok(Ausgang::Abgeschlossen { .. })),
            "Tag {t}: {aus:?}"
        );
    }
    assert_eq!(dachs.aufrufe, 46, "Dachs: 23 Tage zu je 2 Aufrufen");
    assert_eq!(maus.aufrufe, 46, "Maus: 23 Tage zu je 2 Aufrufen");
    let j = ort.oeffnen()?;
    assert_eq!(j.stand().ki().buchungen().len(), 92, "jede Frage gebucht");
    let t25 = tag(25)?;
    let schutz: Vec<&str> = j
        .stand()
        .begruendungen()
        .iter()
        .filter(|(d, wer, _)| *d == t25 && wer == "Kostenschutz")
        .map(|(_, _, t)| t.as_str())
        .collect();
    assert_eq!(schutz.len(), 2, "{schutz:?}");
    assert!(schutz.iter().all(|t| t.contains("MonatVoll")), "{schutz:?}");
    Ok(())
}

/// Maus' Befund 2 zu PR #33: Was der Papiertag verwendet, steht im Journal, und nur das zaehlt.
/// Eine verworfene Antwort traegt keinen Vorschlag weiter, ein verworfenes Urteil ist keine
/// Zustimmung; beides steht ausdruecklich als verworfen da.
#[test]
fn verworfene_antwort_und_urteil_stehen_als_verworfen() -> Result<(), String> {
    let (sap, allianz) = (sap()?, allianz()?);
    let d = tag(5)?;
    for (antwort_gut, urteil_gut) in [(false, true), (true, false), (true, true)] {
        let ort = Ort::neu()?;
        let mut j = ort.oeffnen()?;
        belege(&mut j, 5, &[(&sap, "230", "231"), (&allianz, "350", "351")])?;
        let (mut dachs, mut maus) = kaeufer(d, &sap);
        for a in dachs.antworten.values_mut() {
            a.verwertbar = antwort_gut;
        }
        for u in maus.urteile.values_mut() {
            u.verwertbar = urteil_gut;
        }
        let mut k = konto(&ort, &j)?;
        let aus = papiertag(
            &mut j,
            d,
            &universum()?,
            &start()?,
            &mut dachs,
            &mut maus,
            &mut k,
        );
        assert!(matches!(aus, Ok(Ausgang::Abgeschlossen { .. })), "{aus:?}");
        let gekauft = !j.stand().auftraege().is_empty();
        assert_eq!(
            gekauft,
            antwort_gut && urteil_gut,
            "{antwort_gut} {urteil_gut}"
        );
        let mit = |wer: &str| -> Vec<Gefragt> {
            j.stand()
                .begruendungen()
                .iter()
                .filter(|(_, w, _)| w == wer)
                .filter_map(|(_, _, t)| gefragt(t))
                .collect()
        };
        let erwartet = |gut: bool| {
            if gut {
                Gefragt::Verwendet
            } else {
                Gefragt::Verworfen
            }
        };
        assert!(
            mit("Dachs").contains(&erwartet(antwort_gut)),
            "{:?}",
            mit("Dachs")
        );
        if antwort_gut {
            assert!(
                mit("Maus").contains(&erwartet(urteil_gut)),
                "{:?}",
                mit("Maus")
            );
        } else {
            // Maus prueft nichts (kein Vorschlag); als Fragerin von Topf B antwortet sie.
            assert!(
                !mit("Maus").contains(&Gefragt::Verworfen),
                "{:?}",
                mit("Maus")
            );
        }
    }
    // Lesen: nur die eigenen Worte nach dem Topf zaehlen.
    assert_eq!(
        gefragt("Topf A: Antwort verwendet: x"),
        Some(Gefragt::Verwendet)
    );
    assert_eq!(gefragt("Topf B: Veto: x"), Some(Gefragt::Verwendet));
    assert_eq!(
        gefragt("Topf B: Antwort verworfen, kein Urteil: x"),
        Some(Gefragt::Verworfen)
    );
    assert_eq!(
        gefragt("Topf A: nicht angebunden (kein Schlüssel), nicht gefragt; kein Vorschlag"),
        Some(Gefragt::NichtAngebunden(Some("kein Schlüssel".into())))
    );
    assert_eq!(
        gefragt("Topf A: nicht angebunden, nicht gefragt; kein Vorschlag"),
        Some(Gefragt::NichtAngebunden(None))
    );
    assert_eq!(gefragt("Topf A: SAP 2000 bp: Antwort verwendet: x"), None);
    assert_eq!(gefragt("Antwort verwendet: x"), None);
    Ok(())
}
