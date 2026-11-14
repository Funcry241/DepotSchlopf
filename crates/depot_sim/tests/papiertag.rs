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

fn verlaufsbeleg(j: &mut Journal, tag: Datum, abruf: Zeitpunkt, euro: i64) -> Result<(), String> {
    let zeit = some(Zeitpunkt::frankfurt(tag, 17, 35, 0), "Schluss")?;
    let preis = some(Preis::aus_cent(Cent(euro.saturating_mul(100))), "Preis")?;
    let kurs = vorbedingung(
        Kurs::exakt(preis, "EUR", tag, zeit, Herkunft::Quelle),
        "Kurs",
    )?;
    let beleg = vorbedingung(
        KursBeleg::neu("alphavantage", abruf, b"Verlaufsbeleg", sap()?, kurs),
        "Beleg",
    )?;
    vorbedingung(j.belege_kurs(&beleg), "Kurs schreiben")
}

fn verlaufsprobe(j: &mut Journal, fehlt: &[i64]) -> Result<(Datum, Zeitpunkt, Vec<Datum>), String> {
    let bis = tag(13)?;
    let wissen = some(Zeitpunkt::frankfurt(bis, 18, 0, 0), "Wissensstand")?;
    let abruf = some(Zeitpunkt::frankfurt(bis, 17, 40, 0), "Abruf")?;
    let mut tage = vec![bis];
    let mut datum = bis;
    for i in 0_i64..=20 {
        if !fehlt.contains(&i) {
            verlaufsbeleg(j, datum, abruf, 121_i64.saturating_sub(i))?;
        }
        datum = some(depot_kern::Boerse::Xetra.bezugstag(datum), "Handelstag")?;
        tage.push(datum);
    }
    Ok((bis, wissen, tage))
}

#[test]
fn verlauf_21_tage() -> Result<(), String> {
    use depot_sim::kursverlauf::{Verlauf, kursverlauf};
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    let (bis, wissen, _) = verlaufsprobe(&mut j, &[])?;
    assert_eq!(
        kursverlauf(j.stand(), &sap()?, bis, wissen),
        Verlauf::Da {
            t1_bp: 83,
            t5_bp: 431,
            t20_bp: 1980,
            unter_hoch_bp: 0,
            ueber_tief_bp: 1980,
        }
    );
    Ok(())
}

#[test]
fn zu_wenig_verlauf() -> Result<(), String> {
    use depot_sim::kursverlauf::{Verlauf, kursverlauf};
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    let (bis, wissen, _) = verlaufsprobe(&mut j, &[20])?;
    assert_eq!(
        kursverlauf(j.stand(), &sap()?, bis, wissen),
        Verlauf::ZuWenig
    );
    Ok(())
}

#[test]
fn luecke_3_tage() -> Result<(), String> {
    use depot_sim::kursverlauf::{Verlauf, kursverlauf};
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    let (bis, wissen, _) = verlaufsprobe(&mut j, &[7, 8, 9])?;
    assert_eq!(
        kursverlauf(j.stand(), &sap()?, bis, wissen),
        Verlauf::ZuWenig
    );
    Ok(())
}

#[test]
fn boersenfrei_zaehlt_nicht() -> Result<(), String> {
    use depot_sim::kursverlauf::kursverlauf;
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    let (bis, wissen, _) = verlaufsprobe(&mut j, &[])?;
    let vorher = kursverlauf(j.stand(), &sap()?, bis, wissen);
    let samstag = tag(7)?;
    verlaufsbeleg(&mut j, samstag, wissen, 999)?;
    assert_eq!(kursverlauf(j.stand(), &sap()?, bis, wissen), vorher);
    Ok(())
}

#[test]
fn letzter_beleg_je_tag() -> Result<(), String> {
    use depot_sim::kursverlauf::{Verlauf, kursverlauf};
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    let (bis, wissen, tage) = verlaufsprobe(&mut j, &[20])?;
    let datum = *some(tage.get(20), "Ältester Handelstag")?;
    for (minute, preis) in [(40, 99), (41, 101), (42, 999)] {
        let abruf = some(Zeitpunkt::frankfurt(bis, 17, minute, 0), "Abruf")?;
        verlaufsbeleg(&mut j, datum, abruf, preis)?;
    }
    let wissen = some(Zeitpunkt::frankfurt(bis, 17, 41, 0), "Wissensstand")?.min(wissen);
    assert!(matches!(
        kursverlauf(j.stand(), &sap()?, bis, wissen),
        Verlauf::Da { t20_bp: 1980, .. }
    ));
    Ok(())
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
        matches!(&aus, Err(LaufFehler::Halt(t)) if t.contains("Kontowächter: [Bargeld")),
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
            prognose: None,
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
        matches!(&aus, Err(LaufFehler::Halt(t)) if t.contains("Kontowächter: [Bargeld")),
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
fn neues_kontingent_erlaubt_regulaere_beratung_nach_46_aufrufen() -> Result<(), String> {
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
                    prognose: None,
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
    assert_eq!(dachs.aufrufe, 48, "Dachs: 24 Tage zu je zwei Aufrufen");
    assert_eq!(maus.aufrufe, 48, "Maus: 24 Tage zu je zwei Aufrufen");
    let j = ort.oeffnen()?;
    assert_eq!(j.stand().ki().buchungen().len(), 96, "jede Frage gebucht");
    let t25 = tag(25)?;
    let schutz: Vec<&str> = j
        .stand()
        .begruendungen()
        .iter()
        .filter(|(d, wer, _)| *d == t25 && wer == "Kostenschutz")
        .map(|(_, _, t)| t.as_str())
        .collect();
    assert!(schutz.is_empty(), "{schutz:?}");
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
            prognose: None,
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
    assert!(depot_sim::papiertag_wiederabspielen(j.stand(), &universum()?, tag(5)?).is_err());
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
    let spaeter = some(Zeitpunkt::frankfurt(tag(5)?, 23, 0, 0), "Spaeterer Abruf")?;
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
    drop(k);
    drop(j);
    let j = ort.oeffnen()?;
    assert_eq!(j.stand().wissensstichtag(tag(5)?), Some(abruf));
    let wiederholung = vorbedingung(
        depot_sim::papiertag_wiederabspielen(j.stand(), &universum()?, tag(5)?),
        "Wiederabspielen",
    )?;
    assert_eq!(wiederholung.ereignisse, dachs.gesehen);
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
        assert!(
            !j.stand().begruendungen().iter().any(|(d, wer, text)| {
                *d == datum && wer == "Kontowaechter" && text.contains("keine Deckung")
            }),
            "Das Regelwerk darf keinen ungedeckten Kauf planen (Tag {t})"
        );
        assert!(
            vorbedingung(j.gleiche_ab(&k), "Abgleich")?.is_empty(),
            "Abgleich am Tag {t}"
        );
    }
    Ok(())
}

#[test]
fn papiertag_kuerzt_kauf_bei_gebundenem_verkaufserloes() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (sap, allianz) = (sap()?, allianz()?);
    let mut topf_a = vorbedingung(Topf::neu(TopfId::A, Cent(45_000)), "Topf A")?;
    topf_a.setze_bestand(sap.clone(), 20);
    let topf_b = vorbedingung(Topf::neu(TopfId::B, Cent(500_000)), "Topf B")?;
    let start = [topf_a, topf_b];
    let universum = universum()?;
    let preise = [(&sap, "250", "250"), (&allianz, "200", "200")];
    for tagnummer in [5, 6] {
        let datum = tag(tagnummer)?;
        let mut journal = ort.oeffnen()?;
        belege(&mut journal, tagnummer, &preise)?;
        let (mut dachs, mut maus) = kaeufer(datum, &allianz);
        let vorschlag = dachs
            .antworten
            .get_mut(&datum)
            .and_then(|antwort| antwort.vorschlag.as_mut())
            .ok_or("Vorschlag fehlt")?;
        vorschlag.zielgewichte_bp.insert(sap.clone(), 3_500);
        if tagnummer == 5 {
            vorschlag.zielgewichte_bp.remove(&allianz);
        }
        let mut konto = Papierdatei::oeffnen(&ort.konto, tagnummer != 5, &start)?;
        let aus = papiertag(
            &mut journal,
            datum,
            &universum,
            &start,
            &mut dachs,
            &mut maus,
            &mut konto,
        );
        assert!(matches!(aus, Ok(Ausgang::Abgeschlossen { .. })), "{aus:?}");
        let kauf = journal
            .stand()
            .offene_auftraege()
            .find(|l| l.beleg.topf == TopfId::A && l.beleg.isin == allianz);
        assert_eq!(
            kauf.map(|l| (l.beleg.stueck, l.versand.is_some())),
            (tagnummer == 6).then_some((2, true)),
            "{:?}",
            journal.stand().begruendungen()
        );
        assert!(vorbedingung(journal.gleiche_ab(&konto), "Abgleich")?.is_empty());
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
fn notbremse_oeffnet_erst_nach_einem_vollstaendig_geprueften_tag_wieder() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (sap, allianz) = (sap()?, allianz()?);
    let start = start()?;
    let universum = universum()?;
    {
        Papierdatei::oeffnen(&ort.konto, false, &start)?;
        let mut j = ort.oeffnen()?;
        for topf in &start {
            vorbedingung(j.setze_topf(topf), "Topf belegen")?;
        }
        let vorher = tag(4)?;
        vorbedingung(
            j.begruende(
                vorher,
                "Waechter",
                "Notbremse aktiv: nur offene Verkaeufe weiter abgleichen",
            ),
            "Aktive Notbremse nach Schutzverkauf",
        )?;
        vorbedingung(
            j.schliesse_ab(vorher, Cent(1_250_000), &"a".repeat(64)),
            "Historischer Hoechststand",
        )?;
    }
    for (t, vollstaendig, bleibt_halt, aufrufe) in [
        (5, false, true, 0),
        (6, true, false, 0),
        (9, true, false, 1),
    ] {
        let mut j = ort.oeffnen()?;
        let preise = [(&sap, "250", "250"), (&allianz, "200", "200")];
        let preise = if vollstaendig {
            preise.as_slice()
        } else {
            preise.get(..1).ok_or("Vorbedingung: SAP-Kurs")?
        };
        belege(&mut j, t, preise)?;
        let mut papierkonto = Papierdatei::oeffnen(&ort.konto, true, &start)?;
        let datum = tag(t)?;
        let (mut dachs, mut maus) = kaeufer(datum, &allianz);
        let aus = papiertag(
            &mut j,
            datum,
            &universum,
            &start,
            &mut dachs,
            &mut maus,
            &mut papierkonto,
        );
        assert_eq!(aus.is_err(), bleibt_halt, "{aus:?}");
        assert_eq!(j.stand().notbremse_aktiv(), bleibt_halt);
        assert_eq!(dachs.aufrufe, aufrufe);
        assert_eq!(j.stand().historischer_hoechststand(), Some(Cent(1_250_000)));
        assert_eq!(papierkonto.konto().sendungen(), usize::from(t == 9));
    }
    let j = ort.oeffnen()?;
    assert_eq!(
        j.eintraege()
            .iter()
            .filter(|(art, _)| *art == Art::NotbremseFreigabe)
            .count(),
        1
    );
    Ok(())
}

#[test]
fn gesamtverlust_sperrt_vor_beratung_und_bleibt_nach_neustart() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (sap, allianz) = (sap()?, allianz()?);
    let mut topf = vorbedingung(Topf::neu(TopfId::A, Cent(450_000)), "Topf A")?;
    topf.setze_bestand(sap.clone(), 10);
    let start = [
        topf,
        vorbedingung(Topf::neu(TopfId::B, Cent::NULL), "Topf B")?,
    ];
    let universum = universum()?;
    Papierdatei::oeffnen(&ort.konto, false, &start)?;
    {
        let mut j = ort.oeffnen()?;
        for topf in &start {
            vorbedingung(j.setze_topf(topf), "Topf belegen")?;
        }
        for (tag_nr, hoch) in [(3, 1_000_000), (4, 800_000)] {
            vorbedingung(
                j.schliesse_ab(tag(tag_nr)?, Cent(hoch), &"a".repeat(64)),
                "Hoechststand belegen",
            )?;
        }
    }
    for (t, sendungen) in [(5, 1), (6, 1), (9, 1)] {
        let mut j = ort.oeffnen()?;
        belege(&mut j, t, &[(&sap, "250", "250"), (&allianz, "200", "200")])?;
        let mut konto = Papierdatei::oeffnen(&ort.konto, true, &start)?;
        let (mut dachs, mut maus) = kaeufer(tag(t)?, &allianz);
        let aus = papiertag(
            &mut j,
            tag(t)?,
            &universum,
            &start,
            &mut dachs,
            &mut maus,
            &mut konto,
        );
        assert!(matches!(aus, Err(LaufFehler::Halt(grund)) if grund.contains("30 %")));
        assert!(j.stand().papier_verlustsperre_aktiv());
        assert_eq!(j.stand().historischer_hoechststand(), Some(Cent(1_000_000)));
        assert_eq!(dachs.aufrufe, 0);
        assert_eq!(konto.konto().sendungen(), sendungen);
    }
    let j = ort.oeffnen()?;
    assert!(
        j.eintraege()
            .iter()
            .any(|(art, inhalt)| { *art == Art::PapierHalt && inhalt.ends_with(";gesamtverlust") })
    );
    Ok(())
}

#[test]
fn alte_verlustsperre_bleibt_beim_oeffnen_erhalten() -> Result<(), String> {
    let ort = Ort::neu()?;
    {
        let mut j = ort.oeffnen()?;
        vorbedingung(
            j.begruende(
                tag(4)?,
                "Waechter",
                "Notbremse: 30 % Gesamtverlust erreicht. Neue Kaeufe gesperrt; Freigabe des Eigentuemers noetig",
            ),
            "Alte Verlustsperre belegen",
        )?;
        vorbedingung(
            j.schliesse_ab(tag(4)?, Cent(1_000_000), &"a".repeat(64)),
            "Abschluss",
        )?;
    }
    assert!(ort.oeffnen()?.stand().papier_verlustsperre_aktiv());
    Ok(())
}

#[test]
fn ausdrueckliche_papierfreigabe_bindet_konto_und_startet_neue_verlustmessung() -> Result<(), String>
{
    let ort = Ort::neu()?;
    let start = start()?;
    let bargeld = Cent(1_000_000);
    let historisch = Cent(1_500_000);
    let mut konto = Papierdatei::oeffnen(&ort.konto, false, &start)?;
    let ab = "a".repeat(64);
    let tag_vorher = tag(4)?;
    let bestaetigt_am = some(Zeitpunkt::neu(tag(5)?, 11, 1, 23), "Freigabezeit")?;
    let freigeben = |j: &mut Journal, abdruck: &str, konto: &Papierdatei| {
        j.gebe_papier_frei(abdruck, konto, bestaetigt_am)
    };
    let mut j = ort.oeffnen()?;
    for topf in &start {
        vorbedingung(j.setze_topf(topf), "Topf belegen")?;
    }
    vorbedingung(j.beginne_abrechnung(tag_vorher), "Tag starten")?;
    vorbedingung(
        j.belege_papier_halt(tag_vorher, depot_journal::PapierHalt::Gesamtverlust),
        "Sperre",
    )?;
    let abdruck = j.bestaetigt().1;
    assert!(
        freigeben(&mut j, &abdruck, &konto).is_err(),
        "Tag noch offen"
    );
    vorbedingung(j.schliesse_ab(tag_vorher, historisch, &ab), "Abschluss")?;
    let anker = j.bestaetigt();
    let falsches_konto = Papierdatei::oeffnen(
        &ort.dir.join("falsch.txt"),
        false,
        &[vorbedingung(
            Topf::neu(TopfId::A, Cent(900_000)),
            "Abweichendes Bargeld",
        )?],
    )?;
    assert!(
        freigeben(&mut j, &ab, &konto).is_err(),
        "Fremder Journalstand"
    );
    assert!(
        freigeben(&mut j, &anker.1, &falsches_konto).is_err(),
        "Konto weicht ab"
    );
    assert_eq!(j.bestaetigt(), anker);
    vorbedingung(freigeben(&mut j, &anker.1, &konto), "Papierfreigabe")?;
    assert!(
        freigeben(&mut j, &anker.1, &konto).is_err(),
        "Doppelte Freigabe"
    );
    drop(j);

    let mut j = ort.oeffnen()?;
    assert!(!j.stand().notbremse_aktiv());
    assert_eq!(j.stand().historischer_hoechststand(), Some(historisch));
    assert_eq!(j.stand().papier_waechterreferenz(), Some(bargeld));
    assert_eq!(
        j.stand().papier_gesamtverlust_erreicht(Cent(700_001), None),
        Some(false)
    );
    assert_eq!(
        j.stand().papier_gesamtverlust_erreicht(Cent(700_000), None),
        Some(true)
    );

    let (sap, allianz) = (sap()?, allianz()?);
    belege(&mut j, 5, &[(&sap, "250", "250"), (&allianz, "200", "200")])?;
    vorbedingung(
        papiertag(
            &mut j,
            tag(5)?,
            &universum()?,
            &start,
            &mut Stumm,
            &mut Stumm,
            &mut konto,
        ),
        "Erster regulaerer Papiertag nach Freigabe",
    )?;
    assert_eq!(j.stand().historischer_hoechststand(), Some(historisch));

    // Ein belegter Hoechststand der neuen Phase liegt unter dem historischen Hoechststand.
    // Der anschliessende echte Papiertag bewertet das reale Testkonto bei 1.000.000 Cent.
    vorbedingung(j.beginne_abrechnung(tag(6)?), "Tag mit neuem Hoechststand")?;
    vorbedingung(
        j.schliesse_ab(tag(6)?, Cent(1_428_572), &ab),
        "Hoechststand der neuen Phase",
    )?;
    drop(j);
    let mut j = ort.oeffnen()?;
    belege(&mut j, 9, &[(&sap, "250", "250"), (&allianz, "200", "200")])?;
    let (mut dachs, mut maus) = kaeufer(tag(9)?, &allianz);
    let aus = papiertag(
        &mut j,
        tag(9)?,
        &universum()?,
        &start,
        &mut dachs,
        &mut maus,
        &mut konto,
    );
    assert!(matches!(aus, Err(LaufFehler::Halt(grund)) if grund.contains("30 %")));
    assert_eq!(dachs.aufrufe, 0);
    assert!(j.stand().papier_verlustsperre_aktiv());
    assert_eq!(j.stand().historischer_hoechststand(), Some(historisch));
    Ok(())
}

#[test]
fn ohne_eintrag_bleibt_offen() -> Result<(), String> {
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
    let order = some(journal.stand().auftrag(nr), "Offene Order")?;
    assert_eq!(order.zustand, depot_journal::AuftragsZustand::Angenommen);
    assert!(order.ausfuehrungen.is_empty());
    Ok(())
}

// Reale Aufrufreihenfolge mit erfundenen Beratern und bestätigter Tokenabrechnung.
struct Diskutant {
    wer: BeraterId,
    plan: Drehbuch,
    runden: std::collections::VecDeque<Option<i64>>,
    gesehen: Vec<depot_sim::abstimmung::AbstimmungsStand>,
    kosten: Option<depot_journal::KiAbrechnung>,
    veto: bool,
    zieht_zurueck: bool,
}
impl depot_sim::Berater for Diskutant {
    fn vorschlagen(
        &mut self,
        e: depot_journal::AufrufErlaubnis,
        lage: &depot_sim::Lage<'_>,
    ) -> Antwort {
        assert!(self.gesehen.is_empty(), "Erstmeinung vor jeder Abstimmung");
        self.kosten = e.abrechnung(100, 100);
        depot_sim::Berater::vorschlagen(&mut self.plan, e, lage)
    }
    fn pruefen(
        &mut self,
        e: depot_journal::AufrufErlaubnis,
        lage: &depot_sim::Lage<'_>,
        v: &Vorschlag,
    ) -> Urteil {
        self.kosten = e.abrechnung(100, 100);
        depot_sim::Berater::pruefen(&mut self.plan, e, lage, v)
    }
    fn kann_abstimmen(&self) -> bool {
        true
    }
    fn abrechnung(&mut self) -> Option<depot_journal::KiAbrechnung> {
        self.kosten.take()
    }
    fn abstimmen(
        &mut self,
        e: depot_journal::AufrufErlaubnis,
        _: &depot_sim::Lage<'_>,
        stand: &depot_sim::abstimmung::AbstimmungsStand,
    ) -> depot_sim::abstimmung::AbstimmungsAntwort {
        self.gesehen.push(stand.clone());
        let Some(Some(bp)) = self.runden.pop_front() else {
            return depot_sim::abstimmung::AbstimmungsAntwort::unverwertbar();
        };
        self.kosten = e.abrechnung(100, 100);
        let Some(mut v) = stand.eigener_vorschlag.clone() else {
            return depot_sim::abstimmung::AbstimmungsAntwort::unverwertbar();
        };
        assert_eq!(v.berater, self.wer);
        for gewicht in v.zielgewichte_bp.values_mut() {
            *gewicht = bp;
        }
        depot_sim::abstimmung::AbstimmungsAntwort {
            eigene: Antwort {
                prognose: None,
                verwertbar: true,
                einschaetzung: format!(
                    "Runde {} begründet Änderung oder Beibehaltung",
                    stand.runde
                ),
                vorschlag: (!self.zieht_zurueck).then_some(v),
            },
            zustimmung_fremd: !self.veto,
        }
    }
}
fn diskutant(wer: BeraterId, runden: Vec<Option<i64>>, veto: bool) -> Result<Diskutant, String> {
    let mut plan = Drehbuch::default();
    let sap = sap()?;
    plan.antworten.insert(
        tag(5)?,
        Antwort {
            prognose: None,
            verwertbar: true,
            einschaetzung: format!("Unabhängige Erstmeinung {wer:?}"),
            vorschlag: Some(Vorschlag {
                berater: wer,
                zielgewichte_bp: [(sap.clone(), 2000)].into_iter().collect(),
                begruendung: [(sap, "Belegte Lage".into())].into_iter().collect(),
            }),
        },
    );
    plan.urteile.insert(
        tag(5)?,
        Urteil {
            verwertbar: true,
            zustimmung: true,
            begruendung: format!("Unabhängige Zweitmeinung {wer:?}"),
        },
    );
    Ok(Diskutant {
        wer,
        plan,
        runden: runden.into(),
        gesehen: Vec::new(),
        kosten: None,
        veto,
        zieht_zurueck: false,
    })
}
fn dialoglauf(a: &mut Diskutant, b: &mut Diskutant) -> Result<(Ort, Journal), String> {
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    belege(
        &mut j,
        5,
        &[(&sap()?, "229", "230"), (&allianz()?, "350", "351")],
    )?;
    let mut k = konto(&ort, &j)?;
    papiertag(&mut j, tag(5)?, &universum()?, &start()?, a, b, &mut k)
        .map_err(|e| format!("{e:?}"))?;
    Ok((ort, j))
}
#[test]
fn aenderung_braucht_eine_weitere_gesehene_zustimmung() -> Result<(), String> {
    let mut a = diskutant(BeraterId::Dachs, vec![Some(1000), Some(1000)], false)?;
    let mut b = diskutant(BeraterId::Maus, vec![Some(1000), Some(1000)], false)?;
    let (_ort, j) = dialoglauf(&mut a, &mut b)?;
    assert_eq!(a.gesehen.len(), 2);
    assert_eq!(b.gesehen.len(), 2);
    assert!(j.stand().auftraege().iter().all(|l| l.beleg.stueck == 2));
    assert_eq!(j.stand().auftraege().len(), 2);
    let erster = a.gesehen.first().ok_or("Erste Runde")?;
    assert!(
        erster
            .fremde_beitraege
            .contains("Unabhängige Erstmeinung Maus")
    );
    assert!(
        erster
            .fremde_beitraege
            .contains("Unabhängige Zweitmeinung Maus")
    );
    let zweiter = a.gesehen.get(1).ok_or("Zweite Runde")?;
    assert!(zweiter.fremde_beitraege.contains("Letzte Abstimmung"));
    assert_eq!(j.stand().ki().buchungen().len(), 8);
    Ok(())
}
#[test]
fn unklare_runde_stoppt_ohne_ungepruefte_aenderung() -> Result<(), String> {
    let mut a = diskutant(BeraterId::Dachs, vec![Some(1000), None, Some(1000)], false)?;
    let mut b = diskutant(BeraterId::Maus, vec![Some(1000), Some(1000)], false)?;
    let (_ort, j) = dialoglauf(&mut a, &mut b)?;
    assert_eq!(a.gesehen.len(), 2);
    assert_eq!(b.gesehen.len(), 1);
    assert!(j.stand().auftraege().is_empty());
    Ok(())
}
#[test]
fn zustimmung_zum_alten_stand_bestaetigt_keine_aenderung() -> Result<(), String> {
    let mut a = diskutant(BeraterId::Dachs, vec![Some(1000)], false)?;
    let mut b = diskutant(BeraterId::Maus, vec![Some(2000)], false)?;
    let (_ort, j) = dialoglauf(&mut a, &mut b)?;
    let [auftrag] = j.stand().auftraege() else {
        return Err("Nur der unveränderte Vorschlag darf zu einem Auftrag führen".into());
    };
    assert_eq!(auftrag.beleg.topf, TopfId::B);
    assert_eq!(auftrag.beleg.stueck, 4);
    Ok(())
}
#[test]
fn aenderung_oder_rueckzug_gilt_auch_ohne_zweite_antwort() -> Result<(), String> {
    for rueckzug in [false, true] {
        let mut a = diskutant(BeraterId::Dachs, vec![Some(1000)], false)?;
        a.zieht_zurueck = rueckzug;
        let mut b = diskutant(BeraterId::Maus, vec![None], false)?;
        let (_ort, j) = dialoglauf(&mut a, &mut b)?;
        let [auftrag] = j.stand().auftraege() else {
            return Err(
                "Nur der bisher gültige Vorschlag von Maus darf zu einem Auftrag führen".into(),
            );
        };
        assert_eq!(auftrag.beleg.topf, TopfId::B);
    }
    Ok(())
}
#[test]
fn widerruf_gilt_auch_ohne_verwertbare_zweite_antwort() -> Result<(), String> {
    let mut a = diskutant(BeraterId::Dachs, vec![Some(2000)], true)?;
    let mut b = diskutant(BeraterId::Maus, vec![None], false)?;
    let (_ort, j) = dialoglauf(&mut a, &mut b)?;
    assert_eq!(a.gesehen.len(), 1);
    assert_eq!(b.gesehen.len(), 1);
    let [auftrag] = j.stand().auftraege() else {
        return Err("Nur der nicht widerrufene Vorschlag darf zu einem Auftrag führen".into());
    };
    assert_eq!(auftrag.beleg.topf, TopfId::A);
    Ok(())
}
#[test]
fn abstimmung_kann_die_zweitmeinung_auch_widerrufen() -> Result<(), String> {
    let mut a = diskutant(BeraterId::Dachs, vec![Some(2000)], true)?;
    let mut b = diskutant(BeraterId::Maus, vec![Some(2000)], true)?;
    let (_ort, j) = dialoglauf(&mut a, &mut b)?;
    assert!(j.stand().auftraege().is_empty());
    assert_eq!(a.gesehen.len(), 1);
    Ok(())
}
#[test]
fn hoechstens_zwei_runden() -> Result<(), String> {
    let folge: Vec<_> = (0..100).map(|n| Some(1000_i64.saturating_add(n))).collect();
    let mut a = diskutant(BeraterId::Dachs, folge.clone(), false)?;
    let mut b = diskutant(BeraterId::Maus, folge, false)?;
    let (_ort, j) = dialoglauf(&mut a, &mut b)?;
    assert_eq!(a.gesehen.len(), 2);
    assert_eq!(b.gesehen.len(), 2);
    assert_eq!(j.stand().ki().buchungen().len(), 8);
    assert!(j.stand().auftraege().is_empty());
    Ok(())
}

#[test]
fn einig_nach_runde_1() -> Result<(), String> {
    let mut a = diskutant(BeraterId::Dachs, vec![Some(2000), Some(1000)], false)?;
    let mut b = diskutant(BeraterId::Maus, vec![Some(2000), Some(1000)], false)?;
    let (_ort, j) = dialoglauf(&mut a, &mut b)?;
    assert_eq!(a.gesehen.len(), 1);
    assert_eq!(b.gesehen.len(), 1);
    assert_eq!(j.stand().auftraege().len(), 2);
    Ok(())
}

#[test]
fn uneinig_nach_runde_2_kein_auftrag() -> Result<(), String> {
    let mut a = diskutant(
        BeraterId::Dachs,
        vec![Some(1000), Some(1500), Some(1500)],
        false,
    )?;
    let mut b = diskutant(BeraterId::Maus, vec![Some(2000), Some(2000)], false)?;
    let (_ort, j) = dialoglauf(&mut a, &mut b)?;
    assert_eq!(a.gesehen.len(), 2);
    assert_eq!(b.gesehen.len(), 2);
    assert!(
        j.stand()
            .auftraege()
            .iter()
            .all(|a| a.beleg.topf != TopfId::A)
    );
    Ok(())
}

#[test]
fn budget_grenze_unveraendert() {
    assert_eq!(depot_ki::KiGrenzen::standard().aufrufe_je_zyklus(), 30);
}

#[test]
fn unbrauchbare_erstmeinung_loest_keine_abstimmung_aus() -> Result<(), String> {
    let mut a = diskutant(BeraterId::Dachs, vec![Some(1000)], false)?;
    let mut b = diskutant(BeraterId::Maus, vec![Some(1000)], false)?;
    let erste = a.plan.antworten.get_mut(&tag(5)?).ok_or("Erstmeinung")?;
    erste.verwertbar = false;
    let (_ort, _j) = dialoglauf(&mut a, &mut b)?;
    assert!(a.gesehen.is_empty() && b.gesehen.is_empty());
    Ok(())
}

#[test]
fn direkter_papiertag_wartet_schreibfrei_auf_updatebestaetigung() -> Result<(), String> {
    let ort = Ort::neu()?;
    let daten = ort.dir.join("daten");
    fs::create_dir_all(&daten).map_err(|e| e.to_string())?;
    let pfad = daten.join("journal.txt");
    Journal::einrichten(&pfad).map_err(|e| format!("{e:?}"))?;
    let vorher = fs::read(&pfad).map_err(|e| e.to_string())?;
    let vermerk = fs::read(pfad.with_extension("vermerk")).map_err(|e| e.to_string())?;
    let markierung = daten.join("update_offen.txt");
    fs::write(&markierung, "Probe").map_err(|e| e.to_string())?;
    let aus =
        depot_sim::papierlauf::papier(&ort.dir, tag(5)?, &universum()?, &mut Stumm, &mut Stumm);
    assert!(aus.is_err_and(|e| e.contains("Update ist noch nicht bestätigt")));
    assert_eq!(fs::read(&pfad).map_err(|e| e.to_string())?, vorher);
    assert_eq!(
        fs::read(pfad.with_extension("vermerk")).map_err(|e| e.to_string())?,
        vermerk
    );
    assert!(!daten.join("papierkonto.txt").exists());
    fs::remove_file(markierung).map_err(|e| e.to_string())?;
    let aus =
        depot_sim::papierlauf::papier(&ort.dir, tag(5)?, &universum()?, &mut Stumm, &mut Stumm);
    assert!(!aus.is_err_and(|e| e.contains("Update ist noch nicht bestätigt")));
    Ok(())
}

fn belege_morgen(j: &mut Journal, schluss: u8, abruf: Zeitpunkt) -> Result<(), String> {
    use depot_kern::Boerse;
    let aktuell = tag(schluss)?;
    let vorher = some(Boerse::Xetra.bezugstag(aktuell), "Vergleichstag")?;
    for isin in [sap()?, allianz()?] {
        for (datum, preis) in [(vorher, Cent(23_000)), (aktuell, Cent(23_010))] {
            let kurs = vorbedingung(
                Kurs::neu(
                    preis,
                    "EUR",
                    datum,
                    some(Boerse::Xetra.schluss(datum), "Schluss")?,
                    Herkunft::Quelle,
                ),
                "Kurs",
            )?;
            let beleg = vorbedingung(
                KursBeleg::neu("alphavantage", abruf, b"Morgenprobe", isin.clone(), kurs),
                "Kursbeleg",
            )?;
            vorbedingung(j.belege_kurs(&beleg), "Kurs speichern")?;
        }
    }
    Ok(())
}

#[test]
fn montagmorgen_nutzt_freitagskurse_und_schliesst_am_montag_ab() -> Result<(), String> {
    let ort = Ort::neu()?;
    let starttag = tag(9)?;
    let mut j = ort.oeffnen()?;
    let abruf = some(Zeitpunkt::frankfurt(starttag, 7, 30, 0), "Morgen")?;
    belege_morgen(&mut j, 6, abruf)?;
    drop(j);
    let ausgang = vorbedingung(stumm_tag(&ort, 9)?, "Montagmorgenlauf")?;
    assert!(matches!(ausgang, Ausgang::Abgeschlossen { datum, .. } if datum == starttag));
    let j = ort.oeffnen()?;
    assert_eq!(j.stand().abschluss().map(|a| a.datum), Some(starttag));
    let wiederholung = vorbedingung(
        depot_sim::papiertag_wiederabspielen(j.stand(), &universum()?, starttag),
        "Wiederabspielen",
    )?;
    assert_eq!(
        wiederholung.kurse.eintrag(&sap()?).map(Kurs::handelstag),
        Some(tag(6)?)
    );
    Ok(())
}

#[test]
fn verspaeteter_papiertag_nutzt_um_mittag_bezugskurse() -> Result<(), String> {
    let ort = Ort::neu()?;
    let starttag = tag(9)?;
    let mut j = ort.oeffnen()?;
    let abruf = some(Zeitpunkt::frankfurt(starttag, 12, 0, 0), "Mittag")?;
    belege_morgen(&mut j, 6, abruf)?;
    drop(j);
    let ausgang = vorbedingung(stumm_tag(&ort, 9)?, "Montagmorgenlauf")?;
    assert!(matches!(ausgang, Ausgang::Abgeschlossen { datum, .. } if datum == starttag));
    let j = ort.oeffnen()?;
    assert_eq!(j.stand().abschluss().map(|a| a.datum), Some(starttag));
    let wiederholung = vorbedingung(
        depot_sim::papiertag_wiederabspielen(j.stand(), &universum()?, starttag),
        "Wiederabspielen",
    )?;
    assert_eq!(
        wiederholung.kurse.eintrag(&sap()?).map(Kurs::handelstag),
        Some(tag(6)?)
    );
    Ok(())
}

#[test]
fn montagmorgen_mit_donnerstagskurs_schliesst_nichts_ab() -> Result<(), String> {
    let ort = Ort::neu()?;
    let starttag = tag(9)?;
    let mut j = ort.oeffnen()?;
    belege_morgen(
        &mut j,
        5,
        some(Zeitpunkt::frankfurt(starttag, 7, 30, 0), "Morgen")?,
    )?;
    drop(j);
    let fehler = stumm_tag(&ort, 9)?
        .err()
        .ok_or("Kein Halt bei Donnerstagskurs")?;
    assert!(format!("{fehler:?}").contains("KursNichtVomBezugstag"));
    assert!(ort.oeffnen()?.stand().abschluss().is_none());
    Ok(())
}

#[test]
fn montagmorgen_versendet_order_fuer_montag_und_belegt_spaeter_dessen_schluss() -> Result<(), String>
{
    use depot_kern::Boerse;
    let ort = Ort::neu()?;
    let montag = tag(9)?;
    let dienstag = tag(10)?;
    let sap = sap()?;
    let mut j = ort.oeffnen()?;
    let mut k = konto(&ort, &j)?;
    let abruf = some(Zeitpunkt::frankfurt(montag, 7, 30, 0), "Morgen")?;
    belege_morgen(&mut j, 6, abruf)?;
    let (mut dachs, mut maus) = kaeufer(montag, &sap);
    vorbedingung(
        papiertag(
            &mut j,
            montag,
            &universum()?,
            &start()?,
            &mut dachs,
            &mut maus,
            &mut k,
        ),
        "Montag",
    )?;
    let order = j
        .stand()
        .offene_auftraege()
        .next()
        .ok_or_else(|| format!("Offene Order fehlt: {:?}", j.stand().begruendungen()))?;
    let nr = order.beleg.nr;
    assert_eq!(order.versand, Some(abruf));
    assert_eq!(order.frist, Boerse::Xetra.schluss(montag));
    assert!(order.ausfuehrungen.is_empty());
    belege_morgen(
        &mut j,
        9,
        some(Zeitpunkt::frankfurt(dienstag, 7, 30, 0), "Morgen")?,
    )?;
    vorbedingung(
        papiertag(
            &mut j,
            dienstag,
            &universum()?,
            &start()?,
            &mut Stumm,
            &mut Stumm,
            &mut k,
        ),
        "Dienstag",
    )?;
    let order = some(j.stand().auftrag(nr), "Erledigte Order")?;
    assert_eq!(
        order.ausfuehrungen.first().and_then(|a| a.kurszeit),
        Boerse::Xetra.schluss(montag)
    );
    assert!(vorbedingung(j.gleiche_ab(&k), "Kontoabgleich")?.is_empty());
    Ok(())
}

fn belege_sperrprobe(
    j: &mut Journal,
    isin: &Isin,
    preise: &[(u8, i64)],
    abruf: Zeitpunkt,
) -> Result<(), String> {
    for (nr, cent) in preise {
        let datum = tag(*nr)?;
        let kurs = vorbedingung(
            Kurs::neu(
                Cent(*cent),
                "EUR",
                datum,
                some(depot_kern::Boerse::Xetra.schluss(datum), "Schluss")?,
                Herkunft::Quelle,
            ),
            "Kurs",
        )?;
        let beleg = vorbedingung(
            KursBeleg::neu("alphavantage", abruf, b"Sperrprobe", isin.clone(), kurs),
            "Beleg",
        )?;
        vorbedingung(j.belege_kurs(&beleg), "Kurs belegen")?;
    }
    Ok(())
}

#[test]
fn gesperrt_ohne_vergleichskurs_haelt() -> Result<(), String> {
    let ort = Ort::neu()?;
    let sap = sap()?;
    let mut toepfe = start()?;
    some(toepfe.first_mut(), "Topf A")?.setze_bestand(sap.clone(), 10);
    let mut k = Papierdatei::oeffnen(&ort.konto, false, &toepfe)?;
    let mut j = ort.oeffnen()?;
    for topf in &toepfe {
        vorbedingung(j.setze_topf(topf), "Topf belegen")?;
    }
    vorbedingung(
        j.schliesse_ab(tag(6)?, Cent(1_200_000), &"a".repeat(64)),
        "Abschluss",
    )?;
    let montag = tag(9)?;
    let abruf = some(Zeitpunkt::frankfurt(montag, 7, 30, 0), "Abruf")?;
    belege_sperrprobe(&mut j, &sap, &[(5, 20_000)], abruf)?;
    belege_sperrprobe(&mut j, &allianz()?, &[(5, 35_000), (6, 35_000)], abruf)?;
    let vorher = j.eintraege().to_vec();
    let aus = papiertag(
        &mut j,
        montag,
        &universum()?,
        &toepfe,
        &mut Stumm,
        &mut Stumm,
        &mut k,
    );
    assert!(matches!(aus, Err(LaufFehler::Halt(grund)) if grund.contains("ohne Vergleichskurs")));
    assert_eq!(j.eintraege(), vorher);
    Ok(())
}

#[test]
fn alle_gehaltenen_gesperrt_schliesst_ab() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (sap, allianz) = (sap()?, allianz()?);
    let mut a = vorbedingung(Topf::neu(TopfId::A, Cent(300_000)), "Topf A")?;
    let mut b = vorbedingung(Topf::neu(TopfId::B, Cent(400_000)), "Topf B")?;
    a.setze_bestand(sap.clone(), 10);
    b.setze_bestand(allianz.clone(), 3);
    let toepfe = [a, b];
    let mut k = Papierdatei::oeffnen(&ort.konto, false, &toepfe)?;
    let mut j = ort.oeffnen()?;
    for topf in &toepfe {
        vorbedingung(j.setze_topf(topf), "Topf belegen")?;
    }
    vorbedingung(
        j.schliesse_ab(tag(6)?, Cent(1_005_000), &"a".repeat(64)),
        "Abschluss",
    )?;
    let montag = tag(9)?;
    let abruf = some(Zeitpunkt::frankfurt(montag, 7, 30, 0), "Abruf")?;
    belege_sperrprobe(&mut j, &sap, &[(4, 20_000), (5, 20_000)], abruf)?;
    belege_sperrprobe(&mut j, &allianz, &[(4, 35_000), (5, 35_000)], abruf)?;
    let (mut dachs, mut maus) = kaeufer(montag, &sap);
    let aus = papiertag(
        &mut j,
        montag,
        &universum()?,
        &toepfe,
        &mut dachs,
        &mut maus,
        &mut k,
    );
    assert!(
        matches!(aus, Ok(Ausgang::Abgeschlossen { datum, .. }) if datum == montag),
        "{aus:?}"
    );
    assert_eq!(dachs.aufrufe, 0);
    assert_eq!(maus.aufrufe, 0);
    assert_eq!(k.konto().sendungen(), 0);
    for isin in [sap, allianz] {
        let prefix = format!("{}{isin} · ", depot_sim::ERSATZKURS);
        assert!(j.stand().begruendungen().iter().any(|(d, wer, text)| {
            *d == montag && wer == depot_sim::AUSWAHL && text.starts_with(&prefix)
        }));
    }
    Ok(())
}

fn gemischter_morgen(
    sap_cent: i64,
    allianz_alt: i64,
    allianz_neu: i64,
    hoch: Cent,
    halt: Option<depot_journal::PapierHalt>,
) -> Result<(Ort, Result<Ausgang, LaufFehler>, u32, u32), String> {
    let ort = Ort::neu()?;
    let (sap, allianz) = (sap()?, allianz()?);
    let mut a = vorbedingung(Topf::neu(TopfId::A, Cent(200_000)), "Topf A")?;
    let mut b = vorbedingung(Topf::neu(TopfId::B, Cent(200_000)), "Topf B")?;
    a.setze_bestand(sap.clone(), 10);
    b.setze_bestand(allianz.clone(), 10);
    let toepfe = [a, b];
    let mut k = Papierdatei::oeffnen(&ort.konto, false, &toepfe)?;
    let mut j = ort.oeffnen()?;
    for topf in &toepfe {
        vorbedingung(j.setze_topf(topf), "Topf belegen")?;
    }
    if let Some(halt) = halt {
        vorbedingung(j.beginne_abrechnung(tag(6)?), "Abrechnung")?;
        vorbedingung(j.belege_papier_halt(tag(6)?, halt), "Sperre")?;
    }
    vorbedingung(j.schliesse_ab(tag(6)?, hoch, &"a".repeat(64)), "Abschluss")?;
    let montag = tag(9)?;
    let abruf = some(Zeitpunkt::frankfurt(montag, 7, 30, 0), "Abruf")?;
    belege_sperrprobe(&mut j, &sap, &[(4, sap_cent), (5, sap_cent)], abruf)?;
    belege_sperrprobe(
        &mut j,
        &allianz,
        &[(5, allianz_alt), (6, allianz_neu)],
        abruf,
    )?;
    let (mut dachs, mut maus) = kaeufer(montag, &allianz);
    let aus = papiertag(
        &mut j,
        montag,
        &universum()?,
        &toepfe,
        &mut dachs,
        &mut maus,
        &mut k,
    );
    drop(j);
    Ok((ort, aus, dachs.aufrufe, maus.aufrufe))
}

#[test]
fn morgens_gehaltener_wert_mit_fehler_sperrt_nur_ihn() -> Result<(), String> {
    let (ort, aus, dachs, maus) = gemischter_morgen(20_000, 40_000, 40_000, Cent(1_000_000), None)?;
    assert!(matches!(aus, Ok(Ausgang::Abgeschlossen { .. })), "{aus:?}");
    assert_eq!(dachs, 0);
    assert!(maus > 0);
    let j = ort.oeffnen()?;
    assert!(j.stand().begruendungen().iter().any(|(_, _, text)| {
        text.contains("DE0007164600 heute gesperrt") && text.contains("KI nicht gefragt")
    }));
    let sap = sap()?;
    assert!(j.stand().offene_auftraege().all(|a| a.beleg.isin != sap));
    Ok(())
}

#[test]
fn kein_hoechststand_mit_gesperrtem_wert() -> Result<(), String> {
    let (_, aus, _, _) = gemischter_morgen(20_000, 40_000, 43_000, Cent(1_000_000), None)?;
    assert!(
        matches!(
            aus,
            Ok(Ausgang::Abgeschlossen {
                hoechststand: Cent(1_000_000),
                ..
            })
        ),
        "{aus:?}"
    );
    Ok(())
}

#[test]
fn verlust_im_rest_loest_tageshalt_aus() -> Result<(), String> {
    let (_, aus, dachs, maus) = gemischter_morgen(20_000, 40_000, 34_500, Cent(1_000_000), None)?;
    assert!(
        matches!(aus, Err(LaufFehler::Halt(ref grund)) if grund.contains("Tagesverlust")),
        "{aus:?}"
    );
    assert_eq!((dachs, maus), (0, 0));
    Ok(())
}

#[test]
fn kleiner_verlust_im_rest_kein_halt() -> Result<(), String> {
    let (_, aus, dachs, maus) = gemischter_morgen(20_000, 40_000, 37_000, Cent(1_000_000), None)?;
    assert!(matches!(aus, Ok(Ausgang::Abgeschlossen { .. })), "{aus:?}");
    assert_eq!(dachs, 0);
    assert!(maus > 0);
    Ok(())
}

#[test]
fn gesamtverlust_im_rest_sperrt() -> Result<(), String> {
    let (ort, aus, dachs, maus) = gemischter_morgen(20_000, 40_000, 34_500, Cent(1_350_000), None)?;
    assert!(
        matches!(aus, Err(LaufFehler::Halt(ref grund)) if grund.contains("30 %")),
        "{aus:?}"
    );
    let j = ort.oeffnen()?;
    assert!(j.stand().papier_verlustsperre_aktiv());
    assert_eq!((dachs, maus), (0, 0));
    let auftraege: Vec<_> = j.stand().offene_auftraege().collect();
    assert_eq!(auftraege.len(), 1);
    assert_eq!(
        some(auftraege.first(), "Schutzauftrag")?.beleg.isin,
        allianz()?
    );
    Ok(())
}

#[test]
fn gesperrter_wert_fuellt_am_folgetag() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (sap, allianz) = (sap()?, allianz()?);
    let mut j = ort.oeffnen()?;
    let mut toepfe = start()?;
    some(toepfe.first_mut(), "Topf A")?.setze_bestand(sap.clone(), 1);
    let mut k = Papierdatei::oeffnen(&ort.konto, false, &toepfe)?;
    let freitag = tag(6)?;
    let abruf = some(Zeitpunkt::frankfurt(freitag, 7, 30, 0), "Abruf")?;
    belege_sperrprobe(&mut j, &sap, &[(4, 20_000), (5, 20_000)], abruf)?;
    belege_sperrprobe(&mut j, &allianz, &[(4, 35_000), (5, 35_000)], abruf)?;
    let (mut dachs, mut maus) = kaeufer(freitag, &sap);
    vorbedingung(
        papiertag(
            &mut j,
            freitag,
            &universum()?,
            &toepfe,
            &mut dachs,
            &mut maus,
            &mut k,
        ),
        "Freitag",
    )?;
    let order = some(j.stand().offene_auftraege().next(), "SAP-Order")?;
    assert_eq!(order.beleg.isin, sap);
    let nr = order.beleg.nr;
    let montag = tag(9)?;
    let abruf = some(Zeitpunkt::frankfurt(montag, 7, 30, 0), "Abruf")?;
    belege_sperrprobe(&mut j, &sap, &[(6, 16_000)], abruf)?;
    belege_sperrprobe(&mut j, &allianz, &[(6, 35_000)], abruf)?;
    vorbedingung(
        papiertag(
            &mut j,
            montag,
            &universum()?,
            &toepfe,
            &mut Stumm,
            &mut Stumm,
            &mut k,
        ),
        "Montag",
    )?;
    let order = some(j.stand().auftrag(nr), "SAP-Order")?;
    assert_eq!(order.zustand, depot_journal::AuftragsZustand::Angenommen);
    assert!(order.ausfuehrungen.is_empty());
    let dienstag = tag(10)?;
    let abruf = some(Zeitpunkt::frankfurt(dienstag, 7, 30, 0), "Abruf")?;
    belege_sperrprobe(&mut j, &sap, &[(9, 17_000)], abruf)?;
    belege_sperrprobe(&mut j, &allianz, &[(9, 35_000)], abruf)?;
    vorbedingung(
        papiertag(
            &mut j,
            dienstag,
            &universum()?,
            &toepfe,
            &mut Stumm,
            &mut Stumm,
            &mut k,
        ),
        "Dienstag",
    )?;
    drop(j);
    let j = ort.oeffnen()?;
    let order = some(j.stand().auftrag(nr), "Nachgeholte SAP-Order")?;
    assert_eq!(order.zustand, depot_journal::AuftragsZustand::Gefuellt);
    let ausfuehrung = some(order.ausfuehrungen.first(), "Ausführung")?;
    assert_eq!(ausfuehrung.kurs, Cent(16_000));
    assert_eq!(
        ausfuehrung.kurszeit,
        depot_kern::Boerse::Xetra.schluss(freitag)
    );
    Ok(())
}

#[test]
fn notbremse_verkauft_alle_ausser_gesperrtem() -> Result<(), String> {
    let (ort, aus, _, _) = gemischter_morgen(
        20_000,
        40_000,
        40_000,
        Cent(1_000_000),
        Some(depot_journal::PapierHalt::Notbremse),
    )?;
    assert!(matches!(aus, Err(LaufFehler::Halt(ref grund)) if grund.contains("Notbremse")));
    let (sap, allianz) = (sap()?, allianz()?);
    let mut j = ort.oeffnen()?;
    let auftraege: Vec<_> = j.stand().offene_auftraege().collect();
    assert_eq!(auftraege.len(), 1);
    assert_eq!(
        some(auftraege.first(), "Schutzauftrag")?.beleg.isin,
        allianz
    );
    assert!(j.stand().begruendungen().iter().any(|(_, _, text)| {
        text.contains(sap.als_str()) && text.contains("Verkauf beim nächsten Lauf")
    }));
    let dienstag = tag(10)?;
    let abruf = some(Zeitpunkt::frankfurt(dienstag, 7, 30, 0), "Abruf")?;
    belege_sperrprobe(&mut j, &sap, &[(9, 20_000)], abruf)?;
    belege_sperrprobe(&mut j, &allianz, &[(9, 40_000)], abruf)?;
    drop(j);
    assert!(
        matches!(stumm_tag(&ort, 10)?, Err(LaufFehler::Halt(ref grund)) if grund.contains("Notbremse"))
    );
    let j = ort.oeffnen()?;
    assert!(j.stand().offene_auftraege().any(|a| a.beleg.isin == sap));
    Ok(())
}

#[test]
fn verlustsperre_aktiv_mit_gesperrtem_wert() -> Result<(), String> {
    let (ort, aus, _, _) = gemischter_morgen(
        20_000,
        40_000,
        40_000,
        Cent(1_000_000),
        Some(depot_journal::PapierHalt::Gesamtverlust),
    )?;
    assert!(matches!(aus, Err(LaufFehler::Halt(ref grund)) if grund.contains("30 %")));
    let j = ort.oeffnen()?;
    let auftraege: Vec<_> = j.stand().offene_auftraege().collect();
    assert_eq!(auftraege.len(), 1);
    assert_eq!(
        some(auftraege.first(), "Schutzauftrag")?.beleg.isin,
        allianz()?
    );
    Ok(())
}

#[test]
fn kauf_wartet_ueber_ausgefallenen_tag() -> Result<(), String> {
    journal_liest_nachgeholte_ausfuehrung(None)
}

fn journal_liest_nachgeholte_ausfuehrung(kopie: Option<&std::path::Path>) -> Result<(), String> {
    use depot_journal::{AuftragsZustand, Konto as _};
    let ort = Ort::neu()?;
    let (sap, allianz) = (sap()?, allianz()?);
    let mut j = ort.oeffnen()?;
    let montag = tag(16)?;
    let abruf = some(Zeitpunkt::frankfurt(montag, 7, 30, 0), "Abruf")?;
    belege_sperrprobe(&mut j, &sap, &[(12, 25_000), (13, 25_000)], abruf)?;
    belege_sperrprobe(&mut j, &allianz, &[(12, 5_000), (13, 5_000)], abruf)?;
    let mut k = konto(&ort, &j)?;
    let (mut dachs, mut maus) = kaeufer(montag, &allianz);
    vorbedingung(
        papiertag(
            &mut j,
            montag,
            &universum()?,
            &start()?,
            &mut dachs,
            &mut maus,
            &mut k,
        ),
        "Kaufauftrag",
    )?;
    let auftrag = some(j.stand().offene_auftraege().next(), "Order")?;
    let (nr, frist) = (auftrag.beleg.nr, auftrag.frist);
    let vorher = k.auszug().bargeld;
    let mittwoch = tag(18)?;
    let abruf = some(Zeitpunkt::frankfurt(mittwoch, 7, 30, 0), "Abruf")?;
    belege_sperrprobe(&mut j, &sap, &[(16, 25_000), (17, 25_000)], abruf)?;
    belege_sperrprobe(&mut j, &allianz, &[(16, 5_000), (17, 5_050)], abruf)?;
    vorbedingung(
        papiertag(
            &mut j,
            mittwoch,
            &universum()?,
            &start()?,
            &mut Stumm,
            &mut Stumm,
            &mut k,
        ),
        "Nachholung",
    )?;
    let erwartet = k.auszug();
    drop(j);
    let mut j = ort.oeffnen()?;
    let order = some(j.stand().auftrag(nr), "Order nach Neustart")?;
    let aus = some(order.ausfuehrungen.first(), "Ausführung")?;
    assert_eq!(order.zustand, AuftragsZustand::Gefuellt);
    assert_eq!(order.frist, frist);
    assert_eq!(aus.kurszeit, frist);
    assert_eq!((aus.kurs, aus.gebuehr), (Cent(5_000), Cent(150)));
    let buch = depot_sim::ausfuehrung::ausfuehrungsbuch(j.stand());
    let zeile = some(buch.iter().find(|zeile| zeile.nr == nr), "Ausführungsbuch")?;
    assert_eq!(zeile.zustand, AuftragsZustand::Gefuellt);
    assert_eq!(zeile.fuellpreis, Preis::aus_cent(Cent(5_000)));
    let kosten = some(
        aus.kurs
            .mal_stueck(aus.stueck)
            .and_then(|betrag| betrag.checked_add(aus.gebuehr)),
        "Kosten",
    )?;
    assert_eq!(Some(erwartet.bargeld), vorher.checked_sub(kosten));
    assert!(vorbedingung(j.gleiche_ab(&k), "Kontoabgleich")?.is_empty());
    if let Some(pfad) = kopie {
        for name in ["journal.txt", "journal.vermerk", "papierkonto.txt"] {
            vorbedingung(
                fs::copy(ort.dir.join(name), pfad.join(name)),
                "Probe kopieren",
            )?;
        }
    }
    Ok(())
}

#[test]
#[ignore = "Erzeugt die Gegenprobe für einen unveränderten Checkout vor P7"]
fn alter_stand_lehnt_nachholung_ab() -> Result<(), String> {
    let ziel = std::env::var_os("DEPOTSCHLOPF_P7_PROBE").ok_or("Pfad für Journalprobe fehlt")?;
    journal_liest_nachgeholte_ausfuehrung(Some(std::path::Path::new(&ziel)))
}

#[test]
fn notbremse_verkauf_ueber_ausfall() -> Result<(), String> {
    use depot_journal::AuftragsZustand;
    let ort = Ort::neu()?;
    let (sap, allianz) = (sap()?, allianz()?);
    let mut a = vorbedingung(Topf::neu(TopfId::A, Cent(100_000)), "Topf A")?;
    a.setze_bestand(sap.clone(), 10);
    let start = [a, vorbedingung(Topf::neu(TopfId::B, Cent::NULL), "Topf B")?];
    let mut k = Papierdatei::oeffnen(&ort.konto, false, &start)?;
    let mut j = ort.oeffnen()?;
    for topf in &start {
        vorbedingung(j.setze_topf(topf), "Topf")?;
    }
    vorbedingung(
        j.schliesse_ab(tag(13)?, Cent(500_000), &"a".repeat(64)),
        "Höchststand",
    )?;
    let montag = tag(16)?;
    let abruf = some(Zeitpunkt::frankfurt(montag, 7, 30, 0), "Abruf")?;
    belege_sperrprobe(&mut j, &sap, &[(12, 25_000), (13, 25_000)], abruf)?;
    belege_sperrprobe(&mut j, &allianz, &[(12, 20_000), (13, 20_000)], abruf)?;
    let u = universum()?;
    let aus = papiertag(&mut j, montag, &u, &start, &mut Stumm, &mut Stumm, &mut k);
    assert!(
        matches!(aus, Err(LaufFehler::Halt(ref text)) if text.contains("Notbremse")),
        "{aus:?}"
    );
    let order = some(j.stand().offene_auftraege().next(), "Schutzauftrag")?;
    let (nr, frist) = (order.beleg.nr, order.frist);
    let mittwoch = tag(18)?;
    let abruf = some(Zeitpunkt::frankfurt(mittwoch, 7, 30, 0), "Abruf")?;
    belege_sperrprobe(&mut j, &sap, &[(16, 25_000), (17, 25_200)], abruf)?;
    belege_sperrprobe(&mut j, &allianz, &[(16, 20_000), (17, 20_000)], abruf)?;
    let aus = papiertag(&mut j, mittwoch, &u, &start, &mut Stumm, &mut Stumm, &mut k);
    assert!(
        matches!(aus, Err(LaufFehler::Halt(ref text)) if text.contains("Notbremse")),
        "{aus:?}"
    );
    let order = some(j.stand().auftrag(nr), "Schutzauftrag")?;
    let aus = some(order.ausfuehrungen.first(), "Ausführung")?;
    assert_eq!(order.zustand, AuftragsZustand::Gefuellt);
    assert_eq!(aus.kurszeit, frist);
    assert_eq!(aus.kurs, Cent(25_000));
    let a = some(j.stand().toepfe().get(&TopfId::A), "Topf A")?;
    assert_eq!(a.stueck(&sap), 0);
    assert_eq!(a.bargeld(), Cent(349_850));
    assert_eq!(j.stand().nicht_gutgeschrieben(TopfId::A), Some(Cent::NULL));
    Ok(())
}

#[test]
fn schluss_ohne_ezb_verfaellt() -> Result<(), String> {
    use depot_journal::AuftragsZustand;
    use depot_kern::Boerse::{Nyse, Xetra};
    let ort = Ort::neu()?;
    let sap = sap()?;
    let jpm = vorbedingung(Isin::neu("US46625H1005"), "JPMorgan")?;
    let u = Universum::neu(
        [("SAP", sap.clone()), ("JPMorgan", jpm.clone())]
            .into_iter()
            .map(|(name, isin)| Instrument {
                isin,
                name: name.into(),
                art: InstrumentArt::Aktie,
            })
            .collect(),
    );
    let mut j = ort.oeffnen()?;
    let mut k = konto(&ort, &j)?;
    let datum = |monat, tag| some(Datum::neu(2026, monat, tag), "Datum");
    let mut belege = |tag, werte: &[(Datum, depot_kern::Boerse, &Isin)]| -> Result<(), String> {
        let abruf = some(Zeitpunkt::frankfurt(tag, 7, 30, 0), "Abruf")?;
        for (schluss, boerse, isin) in werte {
            let zeit = some(boerse.schluss(*schluss), "Schluss")?;
            let kurs = vorbedingung(
                Kurs::neu(Cent(10_000), "EUR", *schluss, zeit, Herkunft::Quelle),
                "Kurs",
            )?;
            let beleg = vorbedingung(
                KursBeleg::neu("alphavantage", abruf, b"EZB-Probe", (*isin).clone(), kurs),
                "Beleg",
            )?;
            vorbedingung(j.belege_kurs(&beleg), "Kurs belegen")?;
        }
        Ok(())
    };
    let montag = datum(4, 6)?;
    belege(
        montag,
        &[
            (datum(4, 1)?, Nyse, &jpm),
            (datum(4, 2)?, Nyse, &jpm),
            (datum(4, 1)?, Xetra, &sap),
            (datum(4, 2)?, Xetra, &sap),
        ],
    )?;
    let (mut dachs, mut maus) = kaeufer(montag, &jpm);
    vorbedingung(
        papiertag(&mut j, montag, &u, &start()?, &mut dachs, &mut maus, &mut k),
        "Order am TARGET-Feiertag",
    )?;
    let order = some(j.stand().offene_auftraege().next(), "JPM-Order")?;
    assert_eq!(order.frist, Nyse.schluss(montag));
    let nr = order.beleg.nr;
    let mittwoch = datum(4, 8)?;
    let abruf = some(Zeitpunkt::frankfurt(mittwoch, 7, 30, 0), "Abruf")?;
    for (boerse, isin) in [(Nyse, &jpm), (Xetra, &sap)] {
        let dienstag = datum(4, 7)?;
        let zeit = some(boerse.schluss(dienstag), "Schluss")?;
        let kurs = vorbedingung(
            Kurs::neu(Cent(10_000), "EUR", dienstag, zeit, Herkunft::Quelle),
            "Kurs",
        )?;
        let beleg = vorbedingung(
            KursBeleg::neu("alphavantage", abruf, b"EZB-Probe", isin.clone(), kurs),
            "Beleg",
        )?;
        vorbedingung(j.belege_kurs(&beleg), "Kurs belegen")?;
    }
    vorbedingung(
        papiertag(
            &mut j,
            mittwoch,
            &u,
            &start()?,
            &mut Stumm,
            &mut Stumm,
            &mut k,
        ),
        "Morgenlauf nach TARGET-Feiertag",
    )?;
    let order = some(j.stand().auftrag(nr), "Order")?;
    assert_eq!(order.zustand, AuftragsZustand::Verfallen);
    assert!(order.ausfuehrungen.is_empty());
    Ok(())
}

#[test]
fn order_ausserhalb_auswahl_verfaellt() -> Result<(), String> {
    use depot_journal::{AuftragsZustand, verfuegbar};
    use depot_kern::Grenzen;

    let ort = Ort::neu()?;
    let (sap, allianz) = (sap()?, allianz()?);
    let mut j = ort.oeffnen()?;
    let start = start()?;
    let grenzen = Grenzen::standard();
    let mut k = konto(&ort, &j)?;
    let montag = tag(16)?;
    let abruf = some(Zeitpunkt::frankfurt(montag, 7, 30, 0), "Abruf")?;
    belege_sperrprobe(&mut j, &sap, &[(12, 25_000), (13, 25_000)], abruf)?;
    belege_sperrprobe(&mut j, &allianz, &[(12, 20_000), (13, 20_000)], abruf)?;
    let (mut dachs, mut maus) = kaeufer(montag, &allianz);
    vorbedingung(
        papiertag(
            &mut j,
            montag,
            &universum()?,
            &start,
            &mut dachs,
            &mut maus,
            &mut k,
        ),
        "Kaufauftrag",
    )?;
    let order = some(j.stand().offene_auftraege().next(), "Offener Kauf")?;
    let (nr, topf) = (order.beleg.nr, order.beleg.topf);
    assert_eq!(order.beleg.isin, allianz);
    let bargeld = some(j.stand().toepfe().get(&topf), "Kauftopf")?.bargeld();
    assert!(some(verfuegbar(&grenzen, j.stand(), topf), "Freies Geld")? < bargeld);
    let auswahl = Universum::neu(vec![Instrument {
        isin: sap.clone(),
        name: "SAP".into(),
        art: InstrumentArt::Aktie,
    }]);
    for (heute, kurse, erwartet) in [
        (
            17,
            [(13, 25_000), (16, 25_000)],
            AuftragsZustand::Angenommen,
        ),
        (19, [(17, 25_000), (18, 25_000)], AuftragsZustand::Verfallen),
    ] {
        let heute = tag(heute)?;
        let abruf = some(Zeitpunkt::frankfurt(heute, 7, 30, 0), "Abruf")?;
        belege_sperrprobe(&mut j, &sap, &kurse, abruf)?;
        vorbedingung(
            papiertag(
                &mut j, heute, &auswahl, &start, &mut Stumm, &mut Stumm, &mut k,
            ),
            "Morgenlauf ohne Allianz",
        )?;
        assert_eq!(
            some(j.stand().auftrag(nr), "Kaufauftrag")?.zustand,
            erwartet
        );
    }
    drop(j);
    let j = ort.oeffnen()?;
    assert_eq!(verfuegbar(&grenzen, j.stand(), topf), Some(bargeld));
    assert!(j.stand().begruendungen().iter().any(|(_, _, text)| {
        text.contains(&format!(
            "Auftrag {nr} verfällt: Schlusskurs nicht mehr nachholbar"
        ))
    }));
    Ok(())
}

#[test]
fn gesperrt_drei_tage_verfaellt() -> Result<(), String> {
    use depot_journal::AuftragsZustand;
    use depot_regelwerk::Richtung;

    let ort = Ort::neu()?;
    let (sap, allianz) = (sap()?, allianz()?);
    let mut topf = vorbedingung(Topf::neu(TopfId::A, Cent(100_000)), "Topf A")?;
    topf.setze_bestand(sap.clone(), 10);
    let start = [
        topf,
        vorbedingung(Topf::neu(TopfId::B, Cent::NULL), "Topf B")?,
    ];
    let mut k = Papierdatei::oeffnen(&ort.konto, false, &start)?;
    let mut j = ort.oeffnen()?;
    for topf in &start {
        vorbedingung(j.setze_topf(topf), "Topf")?;
    }
    vorbedingung(
        j.schliesse_ab(tag(13)?, Cent(500_000), &"a".repeat(64)),
        "Höchststand",
    )?;
    let u = universum()?;
    let montag = tag(16)?;
    let abruf = some(Zeitpunkt::frankfurt(montag, 7, 30, 0), "Abruf")?;
    belege_sperrprobe(&mut j, &sap, &[(12, 25_000), (13, 25_000)], abruf)?;
    belege_sperrprobe(&mut j, &allianz, &[(12, 20_000), (13, 20_000)], abruf)?;
    let aus = papiertag(&mut j, montag, &u, &start, &mut Stumm, &mut Stumm, &mut k);
    assert!(
        matches!(aus, Err(LaufFehler::Halt(ref grund)) if grund.contains("Notbremse")),
        "{aus:?}"
    );
    let order = some(j.stand().offene_auftraege().next(), "Offener Verkauf")?;
    let nr = order.beleg.nr;
    assert_eq!(order.beleg.richtung, Richtung::Verkauf);

    for (heute, schluss, kurs, erwartet) in [
        (17, 16, 20_000, AuftragsZustand::Angenommen),
        (18, 17, 25_000, AuftragsZustand::Angenommen),
        (19, 18, 20_000, AuftragsZustand::Verfallen),
    ] {
        let heute = tag(heute)?;
        let abruf = some(Zeitpunkt::frankfurt(heute, 7, 30, 0), "Abruf")?;
        belege_sperrprobe(&mut j, &sap, &[(schluss, kurs)], abruf)?;
        belege_sperrprobe(&mut j, &allianz, &[(schluss, 20_000)], abruf)?;
        let aus = papiertag(&mut j, heute, &u, &start, &mut Stumm, &mut Stumm, &mut k);
        assert!(
            matches!(aus, Err(LaufFehler::Halt(ref grund)) if grund.contains("Notbremse")),
            "{aus:?}"
        );
        let order = some(j.stand().auftrag(nr), "Verkaufsauftrag")?;
        assert_eq!(order.zustand, erwartet);
        assert!(order.ausfuehrungen.is_empty());
    }
    assert_eq!(
        some(j.stand().toepfe().get(&TopfId::A), "Topf A")?.stueck(&sap),
        10
    );
    assert!(
        j.stand()
            .offene_auftraege()
            .all(|order| order.beleg.isin != sap)
    );
    Ok(())
}

#[test]
fn zwei_tage_ausfall_verfaellt() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (sap, allianz) = (sap()?, allianz()?);
    let mut j = ort.oeffnen()?;
    belege(&mut j, 5, &[(&sap, "250", "250"), (&allianz, "200", "200")])?;
    let mut k = konto(&ort, &j)?;
    let (mut dachs, mut maus) = kaeufer(tag(5)?, &allianz);
    vorbedingung(
        papiertag(
            &mut j,
            tag(5)?,
            &universum()?,
            &start()?,
            &mut dachs,
            &mut maus,
            &mut k,
        ),
        "Kaufauftrag",
    )?;
    let nr = some(j.stand().offene_auftraege().next(), "Offene Order")?
        .beleg
        .nr;
    let tag = tag(10)?;
    let abruf = some(Zeitpunkt::frankfurt(tag, 7, 30, 0), "Abruf")?;
    // Selbst ein exakt belegter Schluss wird nach zwei ausgefallenen Läufen nicht nachgeholt.
    belege_sperrprobe(&mut j, &allianz, &[(5, 20_000), (9, 20_000)], abruf)?;
    belege_sperrprobe(&mut j, &sap, &[(6, 25_000), (9, 25_000)], abruf)?;
    vorbedingung(
        papiertag(
            &mut j,
            tag,
            &universum()?,
            &start()?,
            &mut Stumm,
            &mut Stumm,
            &mut k,
        ),
        "Morgenlauf",
    )?;
    let order = some(j.stand().auftrag(nr), "Order")?;
    assert_eq!(order.zustand, depot_journal::AuftragsZustand::Verfallen);
    assert!(order.ausfuehrungen.is_empty());
    assert!(
        j.stand()
            .begruendungen()
            .iter()
            .any(|(_, _, text)| text.contains("Schlusskurs nicht belegt"))
    );
    Ok(())
}

fn belege_schlagzeilen(j: &mut Journal, bezug: u8, rest: u8) -> Result<Zeitpunkt, String> {
    let d = tag(5)?;
    let abruf = some(Zeitpunkt::neu(d, 17, 0, 0), "Abruf")?;
    for (zahl, stunde, titel) in [(bezug, 8, "SAP Meldung"), (rest, 9, "Sportmeldung")] {
        for nr in 0..zahl {
            let zeit = some(Zeitpunkt::neu(d, stunde, nr, 0), "Publikation")?;
            let text = format!("{titel} {nr}");
            let b = NachrichtBeleg::neu("rss-test", abruf, b"Quelle", zeit, &text)?;
            vorbedingung(j.belege_nachricht(&b), "Nachricht")?;
        }
    }
    Ok(abruf)
}

#[test]
fn bestand_vor_sport() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    let mut topf = vorbedingung(Topf::neu(TopfId::A, Cent(100_000)), "Topf")?;
    topf.setze_bestand(sap()?, 1);
    vorbedingung(j.setze_topf(&topf), "Bestand")?;
    let jetzt = belege_schlagzeilen(&mut j, 1, 30)?;
    let n = depot_sim::papiertag::nachrichtenbelege_am(j.stand(), tag(5)?, jetzt);
    assert_eq!(n.len(), 30);
    assert!(
        n.first()
            .is_some_and(|n| n.text().starts_with("SAP Meldung"))
    );
    Ok(())
}

#[test]
fn rest_behaelt_zehn() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    let jetzt = belege_schlagzeilen(&mut j, 40, 10)?;
    let n = depot_sim::papiertag::nachrichtenbelege_am(j.stand(), tag(5)?, jetzt);
    assert_eq!(n.len(), 30);
    assert_eq!(
        n.iter()
            .filter(|n| n.text().starts_with("Sportmeldung"))
            .count(),
        10
    );
    assert_eq!(
        n.iter()
            .filter(|n| n.text().starts_with("SAP Meldung"))
            .count(),
        20
    );
    Ok(())
}

#[test]
fn zeile_markiert() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    let jetzt = belege_schlagzeilen(&mut j, 1, 0)?;
    let welt = depot_sim::weltlage_am(j.stand(), tag(5)?, jetzt);
    assert!(welt.iter().any(|n| n.ends_with("[betrifft: SAP]")));
    Ok(())
}
