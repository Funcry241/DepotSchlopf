//! Der Papiertag mit belegten Schlusskursen. Alle Zahlen sind fuer den Test gesetzt; die
//! Kurse tragen die Herkunft `Quelle`, wie der Sammler sie belegt. Jede Vorbereitung prueft
//! ihren Erfolg ausdruecklich und endet sonst mit „Vorbedingung …“.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{Art, Journal, KursBeleg, NachrichtBeleg};
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

/// Die Kauforder bleibt bis zum naechsten Schluss offen und uebersteht den Neustart.
/// Der spaetere Kurs wird zum aufgerundeten Cent ausgefuehrt.
#[test]
fn kauf_wartet_auf_spaeteren_kurs_und_rundet_auf() -> Result<(), String> {
    use depot_journal::Konto;
    use depot_sim::Broker;

    let ort = Ort::neu()?;
    let (sap, allianz) = (sap()?, allianz()?);
    let mut j = ort.oeffnen()?;
    belege(
        &mut j,
        5,
        &[(&sap, "230", "230.105"), (&allianz, "350", "351")],
    )?;
    let heute = tag(5)?;
    let (mut dachs, mut maus) = kaeufer(heute, &sap);
    let mut k = konto(&ort, &j)?;
    let aus = papiertag(
        &mut j,
        heute,
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
    let lage = some(j.stand().offene_auftraege().next(), "Offene Kauforder")?;
    let nr = lage.beleg.nr;
    let versand = some(lage.versand, "Versandzeit")?;
    assert!(lage.ausfuehrungen.is_empty());
    drop(k);
    drop(j);

    let mut j = ort.oeffnen()?;
    let mut k = konto(&ort, &j)?;
    let alte_kurse = vorbedingung(
        depot_sim::tageskurse(j.stand(), &universum()?),
        "Alte Kurse",
    )?;
    k.markt(&alte_kurse.heute);
    let order = some(k.order(nr), "Offene Order im Papierkonto")?;
    assert_eq!(order.status, depot_journal::KontoStatus::Offen);
    assert!(order.ausfuehrungen.is_empty());
    // Der Vortag wird beim naechsten Abruf nochmals belegt. Sein Kurs liegt vor dem Versand.
    belege(
        &mut j,
        6,
        &[(&sap, "230.105", "230.106"), (&allianz, "351", "351")],
    )?;
    let aus = papiertag(
        &mut j,
        tag(6)?,
        &universum()?,
        &start()?,
        &mut Stumm,
        &mut Stumm,
        &mut k,
    );
    assert!(matches!(aus, Ok(Ausgang::Abgeschlossen { .. })), "{aus:?}");
    let lage = some(j.stand().auftrag(nr), "Kauforder")?;
    let fill = some(lage.ausfuehrungen.first(), "Ausfuehrung")?;
    let kurszeit = some(fill.kurszeit, "Kurszeit")?;
    assert_eq!(lage.versand, Some(versand));
    assert_eq!(kurszeit.datum(), Some(tag(6)?));
    assert!(kurszeit > versand);
    assert_eq!(k.konto().sendungen(), 1, "kein zweiter Versand");
    let a = some(j.stand().toepfe().get(&TopfId::A), "Topf A")?;
    let stueck = a.stueck(&sap);
    assert!(stueck > 0, "SAP gekauft");
    assert_eq!(fill.kurs, Cent(23_011), "zum aufgerundeten Cent");
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
    // Alte Journale (vor `verwertbar`): verworfene Antworten und Urteile sind kein Beitrag,
    // ein echtes Veto schon (Maus' Restbefund zu PR #33).
    assert_eq!(
        gefragt("Topf B: Veto: keine verwertbare Antwort: Limit [Kosten 0 von 24 Cent]"),
        Some(Gefragt::Verworfen)
    );
    assert_eq!(
        gefragt("Topf A: keine verwertbare Antwort: unlesbar [Kosten 3 von 24 Cent]"),
        Some(Gefragt::Verworfen)
    );
    assert_eq!(
        gefragt("Topf B: Veto: zu konzentriert [Kosten 3 von 24 Cent]"),
        Some(Gefragt::Verwendet)
    );
    assert_eq!(
        gefragt("Topf A: Antwort verwendet: keine verwertbare Antwort: x"),
        Some(Gefragt::Verworfen)
    );
    assert_eq!(gefragt("Topf A: SAP 2000 bp: Antwort verwendet: x"), None);
    assert_eq!(gefragt("Antwort verwendet: x"), None);
    Ok(())
}

/// Nur gehaltene Werte sind Pflicht: Fehlt einem Kandidaten der Kurs von heute, laeuft der Tag
/// ohne ihn, und der Grund steht unter `Auswahl`. Fehlt er einem gehaltenen Wert, faellt der
/// Zyklus aus, und niemand wird gefragt.
#[test]
fn kandidat_ohne_kurs_faellt_aus_gehaltener_wert_haelt_den_zyklus() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (sap, allianz) = (sap()?, allianz()?);
    let mut j = ort.oeffnen()?;
    // Am 05.11. hat nur SAP einen Kurs; Allianz ist heute nicht handelbar.
    belege(&mut j, 5, &[(&sap, "230", "231")])?;
    let d = tag(5)?;
    let (mut dachs, mut maus) = kaeufer(d, &sap);
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
    let begr = j.stand().begruendungen().to_vec();
    let auswahl: Vec<&str> = begr
        .iter()
        .filter(|(_, wer, _)| wer == depot_sim::AUSWAHL)
        .map(|(_, _, t)| t.as_str())
        .collect();
    assert_eq!(auswahl.len(), 1, "{auswahl:?}");
    assert!(
        auswahl.iter().all(|t| t.contains("KursFehlt")),
        "{auswahl:?}"
    );
    assert!(
        !begr.iter().any(|(_, wer, _)| wer == "Daten"),
        "kein Ausfall: {begr:?}"
    );
    assert!(j.stand().offene_auftraege().any(|l| l.beleg.isin == sap));
    // Zunaechst wird die Order am naechsten Schluss ausgefuehrt.
    belege(&mut j, 6, &[(&sap, "231", "231"), (&allianz, "350", "351")])?;
    drop(k);
    drop(j);
    let aus = stumm_tag(&ort, 6)?;
    assert!(matches!(aus, Ok(Ausgang::Abgeschlossen { .. })), "{aus:?}");
    let mut j = ort.oeffnen()?;
    assert!(some(j.stand().toepfe().get(&TopfId::A), "Topf A")?.stueck(&sap) > 0);

    // Am 09.11. fehlt der Kurs des nun gehaltenen SAP: Der Zyklus faellt aus.
    belege(&mut j, 9, &[(&allianz, "350", "351")])?;
    drop(j);
    let aus = stumm_tag(&ort, 9)?;
    assert!(
        matches!(aus, Ok(Ausgang::Abgeschlossen { .. })),
        "Ausgang: {aus:?}"
    );
    let j = ort.oeffnen()?;
    let montag = tag(9)?;
    let daten: Vec<&str> = j
        .stand()
        .begruendungen()
        .iter()
        .filter(|(tag, wer, _)| *tag == montag && wer == "Daten")
        .map(|(_, _, t)| t.as_str())
        .collect();
    assert!(
        daten.iter().any(|t| t.contains("KursFehlt")),
        "gehaltener Wert ohne Kurs: {daten:?}"
    );
    assert!(
        daten.iter().any(|t| t.starts_with("Zyklus faellt aus")),
        "{daten:?}"
    );
    Ok(())
}

/// Maus' Restbefund zu #34: Ein Sicherheitsabbruch des Sammlers (die Quelle gab den Schluessel
/// zurueck) hinterlaesst `daten/kurse.halt`. Solange sie liegt, beginnt auch ein von Hand
/// gestarteter Papiertag nicht; die schon belegten Teilkurse werden nicht verwendet.
#[test]
fn kurssperre_haelt_auch_den_handstart() -> Result<(), String> {
    let ort = Ort::neu()?;
    let daten = ort.dir.join("daten");
    vorbedingung(fs::create_dir_all(&daten), "daten")?;
    let pfad = daten.join("journal.txt");
    vorbedingung(Journal::einrichten(&pfad), "Einrichtung")?;
    let mut j = vorbedingung(Journal::oeffnen(&pfad), "Journal")?;
    belege(&mut j, 5, &[(&sap()?, "230", "231")])?;
    drop(j);
    vorbedingung(
        fs::write(daten.join("kurse.halt"), "Sicherheitsabbruch\n"),
        "Sperre",
    )?;
    let vorher = vorbedingung(fs::read(&pfad), "Journal lesen")?;
    let r = depot_sim::papierlauf::papier(&ort.dir, tag(5)?, &universum()?, &mut Stumm, &mut Stumm);
    assert!(
        r.as_ref().is_err_and(|e| e.contains("kurse.halt")),
        "Papiertag trotz Kurssperre: {r:?}"
    );
    assert_eq!(vorbedingung(fs::read(&pfad), "Journal lesen")?, vorher);
    assert!(!daten.join("papierkonto.txt").exists());
    Ok(())
}

/// Abruf je Boerse (Wolfgang, 05.11.2026): Werte verschiedener Boersen werden je kurz nach
/// ihrem Schluss abgerufen. Das Alter eines Kurses misst sich an seinem eigenen Abruf, nicht am
/// spaetesten Abruf des Tages: Der Xetra-Kurs von 17:40 Uhr ist nicht zu alt, nur weil ein
/// anderer Wert um 22:00 Uhr abgerufen wurde.
#[test]
fn alter_je_abruf_nicht_je_spaetestem_abruf() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    belege(&mut j, 5, &[(&sap()?, "230", "231")])?;
    // Allianz hier als Wert einer spaeteren Boerse: Schluss 21:50, Abruf 22:00 Uhr.
    let heute = tag(5)?;
    let vortag = tag(4)?;
    let abruf = some(Zeitpunkt::frankfurt(heute, 22, 0, 0), "Abruf")?;
    for (d, p, h, m) in [(vortag, "350", 21, 50), (heute, "351", 21, 50)] {
        let zeit = some(Zeitpunkt::frankfurt(d, h, m, 0), "Schluss")?;
        let preis = some(Preis::lies_euro(p), "Preis")?;
        let kurs = vorbedingung(Kurs::exakt(preis, "EUR", d, zeit, Herkunft::Quelle), "Kurs")?;
        let beleg = vorbedingung(
            KursBeleg::neu("alphavantage", abruf, p.as_bytes(), allianz()?, kurs),
            "Beleg",
        )?;
        vorbedingung(j.belege_kurs(&beleg), "Beleg schreiben")?;
    }
    drop(j);
    let aus = stumm_tag(&ort, 5)?;
    assert!(
        matches!(aus, Ok(Ausgang::Abgeschlossen { .. })),
        "Ausgang: {aus:?}"
    );
    let j = ort.oeffnen()?;
    let begr = j.stand().begruendungen();
    assert!(
        !begr
            .iter()
            .any(|(_, wer, t)| (wer == "Daten" || wer == depot_sim::AUSWAHL)
                && t.contains("KursZuAlt")),
        "{begr:?}"
    );
    Ok(())
}

/// Merkt sich, was er in der Lage gesehen hat; schlaegt nichts vor.
#[derive(Default)]
struct Leser {
    gesehen: Vec<String>,
}

impl depot_sim::Berater for Leser {
    fn vorschlagen(
        &mut self,
        _erlaubnis: depot_journal::AufrufErlaubnis,
        lage: &depot_sim::Lage<'_>,
    ) -> Antwort {
        self.gesehen.extend(lage.ereignisse.iter().cloned());
        Antwort {
            verwertbar: true,
            einschaetzung: "gelesen".into(),
            vorschlag: None,
        }
    }

    fn pruefen(
        &mut self,
        _erlaubnis: depot_journal::AufrufErlaubnis,
        _lage: &depot_sim::Lage<'_>,
        _vorschlag: &Vorschlag,
    ) -> Urteil {
        Urteil {
            verwertbar: true,
            zustimmung: false,
            begruendung: "nicht gefragt".into(),
        }
    }
}

/// Nachrichten des Sammlers stehen als eigene Belege im Journal und beginnen keinen Tag. Die
/// Berater sehen die vom Handelstag und vom Tag davor (UTC), hoechstens
/// `MAX_NACHRICHTEN_LAGE`, die neueste zuerst.
#[test]
fn berater_sehen_die_nachrichten_der_letzten_tage() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    // Wie `run.cmd kurse`: erst die Kurse, dann die Nachrichten.
    belege_kurse(&mut j)?;
    let abruf = some(Zeitpunkt::neu(tag(5)?, 17, 0, 0), "Abruf")?;
    let mut belege = |zeit: Zeitpunkt, text: &str| -> Result<(), String> {
        let b = vorbedingung(
            NachrichtBeleg::neu("alphavantage-news", abruf, b"roh", zeit, text),
            "Nachricht",
        )?;
        vorbedingung(j.belege_nachricht(&b), "Beleg").map(|_| ())
    };
    belege(some(Zeitpunkt::neu(tag(3)?, 23, 59, 0), "Zeit")?, "zu alt")?;
    for i in 0..40u8 {
        belege(
            some(Zeitpunkt::neu(tag(4)?, i / 2, (i % 2) * 30, 0), "Zeit")?,
            &format!("gestern {i:02}"),
        )?;
    }
    let publiziert = some(Zeitpunkt::neu(tag(5)?, 15, 14, 0), "Zeit")?;
    belege(publiziert, "heute")?;
    let spaeter = some(Zeitpunkt::neu(tag(6)?, 10, 0, 0), "Spaeterer Abruf")?;
    let nachgereicht = vorbedingung(
        NachrichtBeleg::neu(
            "alphavantage-news",
            spaeter,
            b"nachtrag",
            publiziert,
            "Nachtrag",
        ),
        "Nachgereichte Nachricht",
    )?;
    vorbedingung(j.belege_nachricht(&nachgereicht), "Nachtrag belegen")?;
    let wissensstand = depot_sim::weltlage_am(j.stand(), tag(5)?, abruf);
    assert!(!wissensstand.iter().any(|t| t.contains("Nachtrag")));
    let spaeterer_stand = depot_sim::weltlage_am(j.stand(), tag(5)?, spaeter);
    assert!(spaeterer_stand.iter().any(|t| t.contains("Nachtrag")));
    let mut k = konto(&ort, &j)?;
    let mut dachs = Leser::default();
    let aus = papiertag(
        &mut j,
        tag(5)?,
        &universum()?,
        &start()?,
        &mut dachs,
        &mut Stumm,
        &mut k,
    );
    assert!(matches!(aus, Ok(Ausgang::Abgeschlossen { .. })), "{aus:?}");
    let n: Vec<&String> = dachs
        .gesehen
        .iter()
        .filter(|t| t.starts_with("Nachricht "))
        .collect();
    assert_eq!(n.len(), depot_sim::MAX_NACHRICHTEN_LAGE);
    assert_eq!(
        n.first().map(|t| t.as_str()),
        Some("Nachricht 05.11.2026 15:14 UTC (alphavantage-news): heute")
    );
    // Von 40 Nachrichten des Vortags fallen die 11 aeltesten weg (00:00 bis 05:00).
    assert_eq!(
        n.last().map(|t| t.as_str()),
        Some("Nachricht 04.11.2026 05:30 UTC (alphavantage-news): gestern 11")
    );
    assert!(!dachs.gesehen.iter().any(|t| t.contains("zu alt")));
    assert!(!dachs.gesehen.iter().any(|t| t.contains("Nachtrag")));
    assert!(
        j.stand().begruendungen().iter().any(|(_, wer, text)| {
            wer == "Papiertag" && text.contains("Weltmodell: Wissensstand")
        })
    );
    Ok(())
}

fn belege_kurse(j: &mut Journal) -> Result<(), String> {
    belege(
        j,
        5,
        &[(&sap()?, "230.10", "231.00"), (&allianz()?, "350", "351.5")],
    )
}

#[test]
fn verkaufserloes_bleibt_bis_zur_abrechnung_gesperrt() -> Result<(), String> {
    use depot_journal::Konto;
    use depot_kern::{Grenzen, Kurse};
    use depot_regelwerk::{Eingabe, Kennungsvergabe, entscheide, notbremse_auftraege};
    use depot_sim::{Broker, Sendeantwort};

    let ort = Ort::neu()?;
    let isin = sap()?;
    let freitag = tag(6)?;
    let schluss = some(Zeitpunkt::frankfurt(freitag, 17, 35, 0), "Schluss")?;
    let kurs = vorbedingung(
        Kurs::neu(Cent(25_000), "EUR", freitag, schluss, Herkunft::Quelle),
        "Kurs",
    )?;
    let kurse = vorbedingung(Kurse::neu(schluss, vec![(isin.clone(), kurs)]), "Kurse")?;
    let mut start = vorbedingung(Topf::neu(TopfId::A, Cent::NULL), "Topf")?;
    start.setze_bestand(isin.clone(), 20);
    let grenzen = Grenzen::standard();
    let mut vergabe = Kennungsvergabe::nach(0);
    let verkauf = some(
        notbremse_auftraege(&grenzen, &start, &kurse, &mut vergabe),
        "Verkauf",
    )?;
    let verkauf = some(verkauf.first(), "Verkaufsauftrag")?;

    let liquid = vorbedingung(Topf::neu(TopfId::A, Cent(499_725)), "Liquid")?;
    let vorschlag = Vorschlag {
        berater: BeraterId::Dachs,
        zielgewichte_bp: [(isin.clone(), 2_000)].into_iter().collect(),
        begruendung: [(isin, "beobachtet".into())].into_iter().collect(),
    };
    let universum = universum()?;
    let eingabe = Eingabe {
        grenzen: &grenzen,
        universum: &universum,
        topf: &liquid,
        kurse: &kurse,
        vorschlag: Some(&vorschlag),
        halt: false,
    };
    let kauf = entscheide(&eingabe, &mut vergabe);
    let kauf = some(kauf.kaeufe.first(), "Kaufauftrag")?;
    let kauf_nach_abrechnung = entscheide(&eingabe, &mut vergabe);
    let kauf_nach_abrechnung = some(kauf_nach_abrechnung.kaeufe.first(), "Zweiter Kaufauftrag")?;

    let start = [start];
    let mut k = Papierdatei::oeffnen(&ort.konto, false, &start)?;
    assert_eq!(k.sende(verkauf), Sendeantwort::Angenommen);
    k.markt(&kurse);
    assert_eq!(k.auszug().bargeld, Cent(499_725));
    drop(k);

    let mut k = Papierdatei::oeffnen(&ort.konto, true, &start)?;
    k.rechne_ab(tag(9)?);
    assert!(matches!(k.sende(kauf), Sendeantwort::Abgelehnt(_)));
    k.rechne_ab(tag(10)?);
    assert_eq!(k.stoerung(), None);
    drop(k);

    let mut k = Papierdatei::oeffnen(&ort.konto, true, &start)?;
    k.rechne_ab(tag(10)?);
    assert_eq!(k.auszug().bargeld, Cent(499_725));
    assert_eq!(k.sende(kauf_nach_abrechnung), Sendeantwort::Angenommen);
    Ok(())
}

#[test]
fn papiertag_gleicht_abrechnung_ab_und_gibt_erloes_am_zweiten_handelstag_frei() -> Result<(), String>
{
    let ort = Ort::neu()?;
    let (sap, allianz) = (sap()?, allianz()?);
    let mut a = vorbedingung(Topf::neu(TopfId::A, Cent::NULL), "Topf A")?;
    a.setze_bestand(sap.clone(), 20);
    let b = vorbedingung(Topf::neu(TopfId::B, Cent(500_000)), "Topf B")?;
    let start = [a, b];
    let universum = universum()?;
    let preise = [(&sap, "250", "250"), (&allianz, "200", "200")];
    for t in [5, 6, 9, 10, 11] {
        let datum = tag(t)?;
        let mut j = ort.oeffnen()?;
        belege(&mut j, t, &preise)?;
        let (mut dachs, mut maus) = kaeufer(datum, &allianz);
        if let Some(v) = dachs
            .antworten
            .get_mut(&datum)
            .and_then(|a| a.vorschlag.as_mut())
        {
            v.zielgewichte_bp.insert(sap.clone(), 3_500);
            v.begruendung.insert(sap.clone(), "Bestand halten".into());
        }
        if t == 9 {
            let text = vorbedingung(fs::read_to_string(&ort.konto), "Kontodatei")?;
            let ohne: String = text
                .split_inclusive('\n')
                .filter(|z| !z.starts_with("abrechnung;"))
                .collect();
            assert_ne!(text, ohne, "Verkauf und Abrechnung muessen belegt sein");
            vorbedingung(fs::write(&ort.konto, ohne), "Abrechnung entfernen")?;
            let mut k = Papierdatei::oeffnen(&ort.konto, true, &start)?;
            let aus = papiertag(
                &mut j, datum, &universum, &start, &mut dachs, &mut maus, &mut k,
            );
            assert!(
                matches!(aus, Err(LaufFehler::Halt(ref grund)) if grund.contains("Abrechnung")),
                "{aus:?}"
            );
            assert_eq!(j.stand().seit_abschluss(), 0, "Halt vor Beginn des Tages");
            assert_eq!(dachs.aufrufe, 0);
            vorbedingung(fs::write(&ort.konto, text), "Kontodatei wiederherstellen")?;
        }
        let mut k = Papierdatei::oeffnen(&ort.konto, t != 5, &start)?;
        let aus = papiertag(
            &mut j, datum, &universum, &start, &mut dachs, &mut maus, &mut k,
        );
        assert!(matches!(aus, Ok(Ausgang::Abgeschlossen { .. })), "{aus:?}");
        let a = some(j.stand().toepfe().get(&TopfId::A), "Topf A nach dem Lauf")?;
        assert_eq!(
            a.bargeld() > Cent::NULL,
            t != 5,
            "Verkaufserloes am Tag {t}"
        );
        assert_eq!(a.stueck(&allianz) > 0, t == 11, "Kauf am Tag {t}");
        assert_eq!(
            j.stand().begruendungen().iter().any(|(d, wer, text)| {
                *d == datum && wer == "Kontowaechter" && text.contains("keine Deckung")
            }),
            matches!(t, 6 | 9),
            "Kaufversuch im Journal am Tag {t}"
        );
        assert!(
            vorbedingung(j.gleiche_ab(&k), "Abgleich")?.is_empty(),
            "Abgleich am Tag {t}"
        );
    }
    Ok(())
}

#[test]
fn notbremse_arbeitet_verkaeufe_nach_neustart_ab_und_bleibt_aktiv() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (sap, allianz) = (sap()?, allianz()?);
    let mut topf = vorbedingung(Topf::neu(TopfId::A, Cent(100_000)), "Topf A")?;
    topf.setze_bestand(sap.clone(), 10);
    let start = [
        topf,
        vorbedingung(Topf::neu(TopfId::B, Cent::NULL), "Topf B")?,
    ];
    let universum = universum()?;
    {
        Papierdatei::oeffnen(&ort.konto, false, &start)?;
        let mut j = ort.oeffnen()?;
        for topf in &start {
            vorbedingung(j.setze_topf(topf), "Topf belegen")?;
        }
        vorbedingung(
            j.schliesse_ab(tag(4)?, Cent(500_000), &"a".repeat(64)),
            "Vorheriger Hoechststand",
        )?;
    }
    for (t, vorher, kurs, sendungen, bestand) in [
        (5, "250", "250", 1, 10),
        (6, "250", "240", 2, 10),
        (9, "240", "230", 3, 10),
        (10, "230", "230", 3, 0),
    ] {
        let mut j = ort.oeffnen()?;
        belege(&mut j, t, &[(&sap, vorher, kurs), (&allianz, "200", "200")])?;
        let mut k = Papierdatei::oeffnen(&ort.konto, true, &start)?;
        let (mut dachs, mut maus) = kaeufer(tag(t)?, &allianz);
        let aus = papiertag(
            &mut j,
            tag(t)?,
            &universum,
            &start,
            &mut dachs,
            &mut maus,
            &mut k,
        );
        assert!(matches!(aus, Err(LaufFehler::Halt(ref grund)) if grund.contains("Notbremse")));
        assert_eq!(dachs.aufrufe, 0);
        assert_eq!(k.konto().sendungen(), sendungen);
        let a = some(j.stand().toepfe().get(&TopfId::A), "Topf A")?;
        assert_eq!(a.stueck(&sap), bestand);
        assert_eq!(
            j.stand().offene_auftraege().count(),
            usize::from(bestand > 0)
        );
    }
    Ok(())
}

#[test]
fn order_ohne_kurs_verfaellt_und_gibt_den_topf_wieder_frei() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (sap, allianz) = (sap()?, allianz()?);
    let mut journal = ort.oeffnen()?;
    belege(
        &mut journal,
        5,
        &[(&sap, "250", "250"), (&allianz, "200", "200")],
    )?;
    let mut papierkonto = konto(&ort, &journal)?;
    let (mut dachs, mut maus) = kaeufer(tag(5)?, &allianz);
    vorbedingung(
        papiertag(
            &mut journal,
            tag(5)?,
            &universum()?,
            &start()?,
            &mut dachs,
            &mut maus,
            &mut papierkonto,
        ),
        "Kaufauftrag",
    )?;
    let order = some(journal.stand().offene_auftraege().next(), "Offene Order")?;
    let nr = order.beleg.nr;
    assert_eq!(order.beleg.isin, allianz);
    drop(papierkonto);
    drop(journal);

    let mut journal = ort.oeffnen()?;
    belege(&mut journal, 6, &[(&sap, "250", "250")])?;
    let mut papierkonto = konto(&ort, &journal)?;
    let (mut dachs, mut maus) = kaeufer(tag(6)?, &sap);
    vorbedingung(
        papiertag(
            &mut journal,
            tag(6)?,
            &universum()?,
            &start()?,
            &mut dachs,
            &mut maus,
            &mut papierkonto,
        ),
        "Papiertag ohne Kurs der offenen Order",
    )?;
    let order = some(journal.stand().auftrag(nr), "Verfallene Order")?;
    assert_eq!(order.zustand, depot_journal::AuftragsZustand::Verfallen);
    assert!(order.ausfuehrungen.is_empty());
    assert!(journal.stand().begruendungen().iter().any(|(_, _, text)| {
        text.contains(&format!("Auftrag {nr} verfaellt: kein gepruefter Kurs"))
    }));
    assert!(
        journal
            .stand()
            .offene_auftraege()
            .any(|l| l.beleg.isin == sap)
    );
    Ok(())
}
