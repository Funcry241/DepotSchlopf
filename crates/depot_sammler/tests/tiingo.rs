//! Abruf je Boerse: Werte der NYSE von Tiingo mit eigenem Schluessel, in Euro umgerechnet mit
//! den Referenzkursen der EZB desselben Tages. Die Antworten haben die Form der Quellen; die
//! Kurse sind erfunden (D12). Die Form von Tiingo (`format=csv`, `columns=date,close,high,low`)
//! ist aus der Dokumentation; der erste echte Lauf bestaetigt sie.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use depot_journal::Journal;
use depot_kern::{Boerse, Cent, Datum, Preis, Topf, TopfId, Zeitpunkt};
use depot_regelwerk::DatenFehler;
use depot_sammler::liste::pruefe_liste;
use depot_sammler::sammeln::{ABLAGE_DEVISEN, adresse};
use depot_sammler::{
    ADRESSE_DEVISEN, Anbieter, Bericht, Ergebnis, Netz, NetzFehler, QUELLE, QUELLE_US,
    SammelFehler, Schluessel, Takt, Verlauf, Wert, Zugang, sammle,
};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

fn ordner() -> Result<(PathBuf, PathBuf), String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_tiingo_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
    let journal = dir.join("journal.txt");
    vorbedingung(Journal::einrichten(&journal), "Einrichtung")?;
    Ok((journal, dir.join("roh")))
}

fn tag(m: u8, t: u8) -> Result<Datum, String> {
    Datum::neu(2026, m, t).ok_or_else(|| "Vorbedingung: Datum".into())
}

fn utc(m: u8, t: u8, h: u8, min: u8) -> Result<Zeitpunkt, String> {
    Zeitpunkt::neu(tag(m, t)?, h, min, 0).ok_or_else(|| "Vorbedingung: Zeitpunkt".into())
}

fn euro(text: &str) -> Result<Preis, String> {
    Preis::lies_euro(text).ok_or_else(|| format!("Vorbedingung: Preis {text}"))
}

const AV: &str = "ERFUNDEN0123TEST";
const TI: &str = "0123456789abcdef0123456789abcdef01234567";

fn zugang() -> Result<Zugang, String> {
    Ok(
        Zugang::alpha_vantage(vorbedingung(Schluessel::neu(AV), "Schluessel AV")?).mit(
            Anbieter::Tiingo,
            vorbedingung(Schluessel::fuer(Anbieter::Tiingo, TI), "Schluessel Tiingo")?,
        ),
    )
}

fn werte() -> Result<Vec<Wert>, String> {
    vorbedingung(
        pruefe_liste(&[
            ("DE0007164600", Boerse::Xetra, "SAP.DEX", "SAP"),
            ("US0378331005", Boerse::Nyse, "AAPL", "Apple"),
        ]),
        "Liste",
    )
}

fn nur(werte: &[Wert], b: Boerse) -> Vec<Wert> {
    werte.iter().filter(|w| w.boerse == b).cloned().collect()
}

/// Erfundene Kurse in der Form von Tiingo (`columns=date,close,high,low`), aeltester zuerst.
const ROH_AAPL: &str =
    "date,close,high,low\n2026-11-03,250.0,252.0,248.0\n2026-11-04,224.48,226.0,223.1\n";
const ROH_SAP: &str = "timestamp,open,high,low,close,volume\r\n\
2026-11-04,210.0000,212.4500,209.1050,211.8000,1200000\r\n\
2026-11-03,208.5000,210.2000,207.9000,209.9500,1100000\r\n";

fn ezb(tage: &[(&str, &str)]) -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><gesmes:Envelope xmlns:gesmes=\"x\" xmlns=\"y\"><gesmes:subject>Reference rates</gesmes:subject><Cube>",
    );
    for (zeit, usd) in tage {
        for teil in [
            "<Cube time=\"",
            zeit,
            "\"><Cube currency=\"USD\" rate=\"",
            usd,
            "\"/></Cube>",
        ] {
            s.push_str(teil);
        }
    }
    s.push_str("</Cube></gesmes:Envelope>");
    s
}

/// Erfundene Quellen: Antwort je Adresse, erkannt am Kuerzel oder an der EZB-Adresse.
struct Erfunden {
    antworten: BTreeMap<String, Result<Vec<u8>, String>>,
    abgerufen: Vec<String>,
}

impl Erfunden {
    fn neu(antworten: &[(&str, Result<String, &str>)]) -> Erfunden {
        Erfunden {
            antworten: antworten
                .iter()
                .map(|(k, a)| {
                    (
                        (*k).to_string(),
                        a.clone().map(String::into_bytes).map_err(str::to_string),
                    )
                })
                .collect(),
            abgerufen: Vec::new(),
        }
    }
}

impl Netz for Erfunden {
    fn hole(&mut self, adresse: &str) -> Result<Vec<u8>, NetzFehler> {
        self.abgerufen.push(adresse.to_string());
        let schluessel = if adresse == ADRESSE_DEVISEN {
            "ezb".to_string()
        } else if let Some(rest) = adresse.strip_prefix("https://api.tiingo.com/tiingo/daily/") {
            rest.split('/').next().unwrap_or_default().to_string()
        } else {
            adresse
                .split('&')
                .find_map(|t| t.strip_prefix("symbol="))
                .unwrap_or_default()
                .to_string()
        };
        self.antworten
            .get(&schluessel)
            .cloned()
            .unwrap_or_else(|| Err("unbekannt".into()))
            // „weg: …“ ist ein voruebergehender Fehler (Verbindung, Status 5xx).
            .map_err(|e| match e.strip_prefix("weg: ") {
                Some(w) => NetzFehler::voruebergehend(w),
                None => NetzFehler::endgueltig(e),
            })
    }
}

fn lauf(
    journal: &std::path::Path,
    roh: &std::path::Path,
    netz: &mut Erfunden,
    zugang: &Zugang,
    zeiten: Vec<Zeitpunkt>,
    werte: &[Wert],
) -> Result<Result<Bericht, SammelFehler>, String> {
    let mut j = vorbedingung(Journal::oeffnen(journal), "Journal oeffnen")?;
    let mut i = zeiten.into_iter();
    let mut u = move || i.next();
    let (mut schlafe, mut melde) = (|_| {}, |_: &Verlauf| {});
    let mut takt = Takt {
        uhr: &mut u,
        schlafe: &mut schlafe,
        melde: &mut melde,
    };
    Ok(sammle(
        &mut j,
        netz,
        zugang,
        &mut takt,
        roh,
        Duration::ZERO,
        werte,
    ))
}

/// Der Schluessel von Tiingo: Kleinbuchstaben und Ziffern, ohne Textform, aus jeder Meldung
/// bereinigt, auch gross geschrieben.
#[test]
fn schluessel_von_tiingo() -> Result<(), String> {
    let s = vorbedingung(Schluessel::fuer(Anbieter::Tiingo, TI), "Schluessel")?;
    assert_eq!(format!("{s:?}"), "Schlüssel(***)");
    assert_eq!(s.bereinige(&format!("token={TI} x")), "token=*** x");
    assert_eq!(
        s.bereinige(&format!("token={} x", TI.to_ascii_uppercase())),
        "token=*** x"
    );
    for falsch in [
        "",
        "kurz",
        "0123456789ABCDEF0123456789ABCDEF01234567",
        "0123456789abcdef0123&token=x",
        "0123456789abcdef 0123456789abcdef",
    ] {
        let e = Schluessel::fuer(Anbieter::Tiingo, falsch);
        assert!(e.is_err(), "{falsch:?}");
        assert!(!format!("{e:?}").contains("0123456789"), "{e:?}");
    }
    assert_eq!(Anbieter::Tiingo.adresse(), "DepotSchlopf/tiingo");
    assert_eq!(Anbieter::fuer(Boerse::Nyse), Anbieter::Tiingo);
    assert_eq!(Anbieter::fuer(Boerse::Xetra), Anbieter::AlphaVantage);
    let z = zugang()?;
    assert_eq!(z.bereinige(&format!("{AV} und {TI}")), "*** und ***");
    assert!(z.steht_in(TI.to_ascii_uppercase().as_bytes()));
    Ok(())
}

/// Ein Wert der NYSE wird bei Tiingo abgerufen: Kuerzel klein, Klasse mit Bindestrich, nur
/// Datum, Schluss, Tief und Hoch als CSV, ab einem Beginn.
#[test]
fn abrufadresse_tiingo() -> Result<(), String> {
    let l = vorbedingung(
        pruefe_liste(&[
            ("US0378331005", Boerse::Nyse, "AAPL", "Apple"),
            ("US0846707026", Boerse::Nyse, "BRK.B", "Berkshire"),
        ]),
        "Liste",
    )?;
    let s = vorbedingung(Schluessel::fuer(Anbieter::Tiingo, TI), "Schluessel")?;
    let ab = Some(tag(10, 21)?);
    let a = adresse(l.first().ok_or("Vorbedingung")?, &s, ab);
    assert_eq!(
        a,
        format!(
            "https://api.tiingo.com/tiingo/daily/aapl/prices?startDate=2026-10-21&format=csv&columns=date,close,high,low&token={TI}"
        )
    );
    let b = adresse(l.get(1).ok_or("Vorbedingung")?, &s, ab);
    assert!(
        b.starts_with("https://api.tiingo.com/tiingo/daily/brk-b/prices?"),
        "{b}"
    );
    Ok(())
}

/// Ein Lauf nur fuer die NYSE kurz nach ihrem Schluss (16:00 Uhr New York = 21:00 Uhr UTC im
/// November): erst einmal die Referenzkurse der EZB, roh abgelegt, dann Tiingo. Belegt wird der
/// Schluss in Euro mit dem Kurs desselben Tages, unter der Quelle `tiingo`. Ein gehaltener
/// Xetra-Wert ist in diesem Lauf keine Pflicht; ihn prueft der Papiertag.
#[test]
fn nyse_von_tiingo_in_euro() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let alle = werte()?;
    let sap = alle.first().map(|w| w.isin.clone()).ok_or("Vorbedingung")?;
    let aapl = alle.get(1).map(|w| w.isin.clone()).ok_or("Vorbedingung")?;
    // SAP liegt im Depot.
    let mut j = vorbedingung(Journal::oeffnen(&journal), "Journal oeffnen")?;
    let mut topf = vorbedingung(Topf::neu(TopfId::A, Cent(100_000)), "Topf")?;
    topf.setze_bestand(sap, 10);
    vorbedingung(j.setze_topf(&topf), "Topf belegen")?;
    drop(j);

    let mut netz = Erfunden::neu(&[
        (
            "ezb",
            Ok(ezb(&[("2026-11-04", "1.1224"), ("2026-11-03", "1.25")])),
        ),
        ("aapl", Ok(ROH_AAPL.to_string())),
    ]);
    let (beginn, abruf) = (utc(11, 4, 21, 4)?, utc(11, 4, 21, 5)?);
    let b = lauf(
        &journal,
        &roh,
        &mut netz,
        &zugang()?,
        vec![beginn, abruf],
        &nur(&alle, Boerse::Nyse),
    )?
    .map_err(|e| format!("Lauf: {e:?}"))?;
    assert_eq!(netz.abgerufen.len(), 2);
    assert_eq!(
        netz.abgerufen.first().map(String::as_str),
        Some(ADRESSE_DEVISEN)
    );
    assert!(
        netz.abgerufen
            .get(1)
            .is_some_and(|a| a.contains("/aapl/prices?startDate=2026-10-21&")),
        "{:?}",
        netz.abgerufen
    );
    assert_eq!(b.abbruch, None);
    assert!(b.pruefung.is_empty(), "{:?}", b.pruefung);
    assert!(b.verwendbar());
    // 224.48 / 1.1224 = 200 genau; 250 / 1.25 = 200.
    let Some((
        _,
        Ergebnis::Belegt {
            heute,
            vortag,
            rohdatei,
        },
    )) = b.werte.first()
    else {
        return Err(format!("nicht belegt: {:?}", b.werte));
    };
    assert_eq!(*heute, Some((tag(11, 4)?, euro("200")?)));
    assert_eq!(*vortag, Some((tag(11, 3)?, euro("200")?)));
    assert!(
        rohdatei.starts_with(roh.join(QUELLE_US)),
        "{}",
        rohdatei.display()
    );
    let ezb_roh = vorbedingung(fs::read_dir(roh.join(ABLAGE_DEVISEN)), "EZB-Ablage")?.count();
    assert_eq!(ezb_roh, 1);
    let j = vorbedingung(Journal::oeffnen(&journal), "erneut oeffnen")?;
    let k = j.stand().kurse();
    assert_eq!(k.len(), 2);
    assert!(
        k.iter()
            .all(|b| b.quelle() == QUELLE_US && b.isin() == &aapl)
    );
    Ok(())
}

/// Fehlt der Schluessel von Tiingo, werden nur die Werte der NYSE nicht abgerufen, und auch die
/// EZB nicht. Xetra laeuft wie bisher.
#[test]
fn ohne_tiingo_schluessel_nur_xetra() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let alle = werte()?;
    let nur_av = Zugang::alpha_vantage(vorbedingung(Schluessel::neu(AV), "Schluessel")?);
    let mut netz = Erfunden::neu(&[("SAP.DEX", Ok(ROH_SAP.to_string()))]);
    let b = lauf(
        &journal,
        &roh,
        &mut netz,
        &nur_av,
        vec![utc(11, 4, 16, 39)?, utc(11, 4, 16, 40)?],
        &alle,
    )?
    .map_err(|e| format!("Lauf: {e:?}"))?;
    assert_eq!(netz.abgerufen.len(), 1, "{:?}", netz.abgerufen);
    assert!(matches!(
        b.werte.first(),
        Some((_, Ergebnis::Belegt { .. }))
    ));
    assert!(
        matches!(b.werte.get(1), Some((_, Ergebnis::Uebersprungen(t))) if t.contains("DepotSchlopf/tiingo")),
        "{:?}",
        b.werte
    );
    let aapl = alle.get(1).map(|w| w.isin.clone()).ok_or("Vorbedingung")?;
    assert_eq!(b.ausgelassen, vec![DatenFehler::KursFehlt(aapl)]);
    assert!(b.verwendbar());
    Ok(())
}

/// Lassen sich die Referenzkurse nicht holen oder lesen, wird kein Wert in Dollar abgerufen;
/// Xetra bleibt unberuehrt. Fehlt der Kurs des Tages, ergibt der Wert keinen Kurs.
#[test]
fn ohne_referenzkurse_kein_dollarwert() -> Result<(), String> {
    let alle = werte()?;
    // Auch eine EZB, die voruebergehend nicht antwortet (Status 503), haelt Xetra nicht auf:
    // Werte in Euro brauchen keine Referenzkurse (Maus, Befund 1 zu #38).
    for ezb_antwort in [
        Err("Netz"),
        Err("weg: Status 503"),
        Ok("<gesmes:Envelope><Cube>".to_string()),
    ] {
        let (journal, roh) = ordner()?;
        let mut netz = Erfunden::neu(&[
            ("ezb", ezb_antwort),
            ("SAP.DEX", Ok(ROH_SAP.to_string())),
            ("aapl", Ok(ROH_AAPL.to_string())),
        ]);
        let b = lauf(
            &journal,
            &roh,
            &mut netz,
            &zugang()?,
            vec![
                utc(11, 4, 16, 39)?,
                utc(11, 4, 16, 40)?,
                utc(11, 4, 16, 41)?,
            ],
            &alle,
        )?
        .map_err(|e| format!("Lauf: {e:?}"))?;
        assert!(
            !netz.abgerufen.iter().any(|a| a.contains("tiingo")),
            "{:?}",
            netz.abgerufen
        );
        assert!(matches!(
            b.werte.first(),
            Some((_, Ergebnis::Belegt { .. }))
        ));
        assert!(
            matches!(b.werte.get(1), Some((_, Ergebnis::Uebersprungen(t))) if t.contains("Referenzkurse der EZB")),
            "{:?}",
            b.werte
        );
        assert!(b.verwendbar());
    }
    // Referenzkurse ohne den 04.11.: kein Kurs fuer Apple.
    let (journal, roh) = ordner()?;
    let mut netz = Erfunden::neu(&[
        ("ezb", Ok(ezb(&[("2026-11-03", "1.25")]))),
        ("aapl", Ok(ROH_AAPL.to_string())),
    ]);
    let b = lauf(
        &journal,
        &roh,
        &mut netz,
        &zugang()?,
        vec![utc(11, 4, 21, 4)?, utc(11, 4, 21, 5)?],
        &nur(&alle, Boerse::Nyse),
    )?
    .map_err(|e| format!("Lauf: {e:?}"))?;
    assert!(
        matches!(b.werte.first(), Some((_, Ergebnis::Fehler(t))) if t.contains("KeinDevisenkurs")),
        "{:?}",
        b.werte
    );
    assert!(!b.verwendbar());
    Ok(())
}

/// Gibt Tiingo oder die EZB einen der Schluessel zurueck, ist das ein Sicherheitsabbruch: Die
/// Antwort wird nicht abgelegt, und kein Kurs dieses Laufs wird belegt.
#[test]
fn schluessel_in_antwort_ist_sicherheitsabbruch() -> Result<(), String> {
    let alle = werte()?;
    let mit_ti = format!("date,close,high,low\n2026-11-04,1.0,1.0,1.0 {TI}\n");
    let ezb_mit_av = format!("<gesmes:Envelope>{AV}</gesmes:Envelope>");
    for (ezb_antwort, aapl) in [
        (ezb(&[("2026-11-04", "1.1224")]), mit_ti),
        (ezb_mit_av, ROH_AAPL.to_string()),
    ] {
        let (journal, roh) = ordner()?;
        let mut netz = Erfunden::neu(&[
            ("ezb", Ok(ezb_antwort)),
            ("SAP.DEX", Ok(ROH_SAP.to_string())),
            ("aapl", Ok(aapl)),
        ]);
        let b = lauf(
            &journal,
            &roh,
            &mut netz,
            &zugang()?,
            vec![
                utc(11, 4, 16, 39)?,
                utc(11, 4, 16, 40)?,
                utc(11, 4, 16, 41)?,
            ],
            &alle,
        )?
        .map_err(|e| format!("Lauf: {e:?}"))?;
        assert!(b.sicherheitsabbruch);
        assert!(!b.verwendbar());
        let diagnose = b.abbruch.as_deref().ok_or("Diagnose fehlt")?;
        assert!(diagnose.contains("Quelle der Antwort:"));
        assert!(diagnose.contains("Erkannter gespeicherter Zugang:"));
        assert!(diagnose.contains("Antwortart:"));
        assert!(!diagnose.contains(AV) && !diagnose.contains(TI));
        let j = vorbedingung(Journal::oeffnen(&journal), "erneut oeffnen")?;
        assert!(j.stand().kurse().is_empty());
        for ordner in [QUELLE, QUELLE_US, ABLAGE_DEVISEN] {
            for datei in fs::read_dir(roh.join(ordner))
                .into_iter()
                .flatten()
                .flatten()
            {
                let inhalt = vorbedingung(fs::read(datei.path()), "Rohdatei")?;
                let text = String::from_utf8_lossy(&inhalt).to_ascii_uppercase();
                assert!(!text.contains(&AV.to_ascii_uppercase()));
                assert!(!text.contains(&TI.to_ascii_uppercase()));
            }
        }
    }
    Ok(())
}

/// Maus' Befund 1 zu 5b70fa5: Auch ein Lauf fuer eine einzige Boerse kennt fuer den Schutz alle
/// hinterlegten Schluessel. Gibt Alpha Vantage den Tiingo-Schluessel zurueck, ist das ein
/// Sicherheitsabbruch, und keine Datei enthaelt ihn. Gemeldet wird ein fehlender Schluessel nur,
/// wenn ein Wert des Laufs ihn braucht.
#[test]
fn einzelboerse_kennt_alle_schluessel() -> Result<(), String> {
    let alle = werte()?;
    let xetra = nur(&alle, Boerse::Xetra);
    let laden = |a: Anbieter| match a {
        Anbieter::AlphaVantage => Schluessel::neu(AV),
        Anbieter::Tiingo => Schluessel::fuer(Anbieter::Tiingo, TI),
    };
    let (zugang, meldungen) = depot_sammler::zugang_laden(&xetra, laden);
    assert!(meldungen.is_empty(), "{meldungen:?}");
    assert!(zugang.steht_in(TI.as_bytes()));
    assert!(zugang.steht_in(AV.as_bytes()));

    let (journal, roh) = ordner()?;
    let mit_ti = format!("{ROH_SAP}{TI}\r\n");
    let mut netz = Erfunden::neu(&[("SAP.DEX", Ok(mit_ti))]);
    let b = lauf(
        &journal,
        &roh,
        &mut netz,
        &zugang,
        vec![utc(11, 4, 16, 40)?],
        &xetra,
    )?
    .map_err(|e| format!("Lauf: {e:?}"))?;
    assert!(b.sicherheitsabbruch);
    assert_eq!(fs::read_dir(roh.join(QUELLE)).map_or(0, Iterator::count), 0);

    // Fehlt der Tiingo-Schluessel, meldet ein Xetra-Lauf das nicht; ein NYSE-Lauf schon.
    let ohne_ti = |a: Anbieter| match a {
        Anbieter::AlphaVantage => Schluessel::neu(AV),
        Anbieter::Tiingo => Err("fehlt".to_string()),
    };
    let (_, m) = depot_sammler::zugang_laden(&xetra, ohne_ti);
    assert!(m.is_empty(), "{m:?}");
    let (_, m) = depot_sammler::zugang_laden(&nur(&alle, Boerse::Nyse), ohne_ti);
    assert_eq!(m.len(), 1, "{m:?}");
    Ok(())
}

/// Maus' Befund 2 zu 5b70fa5: Das Anbieterprotokoll verbucht je Quelle. Ein erfolgreicher
/// Tiingo-Abruf sagt nichts ueber Alpha Vantage, und ein nicht abgerufener Wert (Schluessel
/// fehlt, Referenzkurse fehlen) zaehlt fuer keine Quelle.
#[test]
fn anbieterprotokoll_je_quelle() -> Result<(), String> {
    use depot_journal::anbieter::{Ergebnis as E, Wer};
    let (journal, roh) = ordner()?;
    let alle = werte()?;
    let nur_ti = Zugang::leer().mit(
        Anbieter::Tiingo,
        vorbedingung(Schluessel::fuer(Anbieter::Tiingo, TI), "Schluessel")?,
    );
    let mut netz = Erfunden::neu(&[
        (
            "ezb",
            Ok(ezb(&[("2026-11-04", "1.1224"), ("2026-11-03", "1.25")])),
        ),
        ("aapl", Ok(ROH_AAPL.to_string())),
    ]);
    let b = lauf(
        &journal,
        &roh,
        &mut netz,
        &nur_ti,
        vec![utc(11, 4, 21, 4)?, utc(11, 4, 21, 5)?],
        &alle,
    )?
    .map_err(|e| format!("Lauf: {e:?}"))?;
    assert!(matches!(
        b.werte.first(),
        Some((_, Ergebnis::Uebersprungen(_)))
    ));
    assert_eq!(depot_sammler::einordnen(&b, Anbieter::Tiingo), Some(E::Ok));
    assert_eq!(depot_sammler::einordnen(&b, Anbieter::AlphaVantage), None);
    assert_eq!(Anbieter::Tiingo.protokoll(), Wer::Tiingo);
    assert_eq!(Anbieter::AlphaVantage.protokoll(), Wer::Kursquelle);
    assert_eq!(Wer::aus_kurz(Wer::Tiingo.kurz()), Some(Wer::Tiingo));
    Ok(())
}

#[test]
fn tiingo_protokoll_unterscheidet_ablehnung_und_limit() -> Result<(), String> {
    use depot_journal::anbieter::Ergebnis as E;
    let nyse = nur(&werte()?, Boerse::Nyse);
    for (fehler, erwartet) in [
        ("http status: 401", E::Abgelehnt),
        ("http status: 429", E::Limit),
    ] {
        let (journal, roh) = ordner()?;
        let mut netz = Erfunden::neu(&[
            ("ezb", Ok(ezb(&[("2026-11-04", "1.1224")]))),
            ("aapl", Err(fehler)),
        ]);
        let b = lauf(
            &journal,
            &roh,
            &mut netz,
            &zugang()?,
            vec![utc(11, 4, 21, 4)?],
            &nyse,
        )?
        .map_err(|e| format!("Lauf: {e:?}"))?;
        assert_eq!(
            depot_sammler::einordnen(&b, Anbieter::Tiingo),
            Some(erwartet)
        );
        assert_eq!(depot_sammler::einordnen(&b, Anbieter::AlphaVantage), None);
    }
    Ok(())
}

#[test]
fn kursfehler_verraten_in_ausgaben_und_journal_keine_fragmente() -> Result<(), String> {
    if std::env::var_os("DEPOT_TEST_KURS_MASKIERUNG").is_none() {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let aus = std::process::Command::new(exe)
            .args([
                "--exact",
                "kursfehler_verraten_in_ausgaben_und_journal_keine_fragmente",
                "--nocapture",
            ])
            .env("DEPOT_TEST_KURS_MASKIERUNG", "ja")
            .output()
            .map_err(|e| e.to_string())?;
        assert!(
            aus.status.success(),
            "{}",
            String::from_utf8_lossy(&aus.stderr)
        );
        for daten in [&aus.stdout, &aus.stderr] {
            for key in [AV, TI] {
                assert!(!daten.windows(12).any(|w| {
                    key.as_bytes()
                        .windows(12)
                        .any(|k| w.eq_ignore_ascii_case(k))
                }));
            }
        }
        return Ok(());
    }
    let alle = werte()?;
    for boerse in Boerse::ALLE {
        let (journal, roh) = ordner()?;
        let av_fehler = format!(
            "https://example.com/query?apikey={AV}&x=1 Debug {}",
            AV.get(..12).ok_or("Fragment fehlt")?
        );
        let ti_fehler = format!(
            "https://example.com/prices?token={TI}&x=1 Debug {}",
            TI.get(..12).ok_or("Fragment fehlt")?
        );
        let mut netz = Erfunden::neu(&[
            ("ezb", Ok(ezb(&[("2026-11-04", "1.1224")]))),
            ("SAP.DEX", Err(av_fehler.as_str())),
            ("aapl", Err(ti_fehler.as_str())),
        ]);
        let b = lauf(
            &journal,
            &roh,
            &mut netz,
            &zugang()?,
            vec![utc(11, 4, 21, 4)?, utc(11, 4, 21, 5)?],
            &nur(&alle, boerse),
        )?
        .map_err(|e| format!("Lauf: {e:?}"))?;
        let j = fs::read(&journal).map_err(|e| e.to_string())?;
        for key in [AV, TI] {
            assert!(!j.windows(12).any(|w| {
                key.as_bytes()
                    .windows(12)
                    .any(|k| w.eq_ignore_ascii_case(k))
            }));
        }
        println!("{b:?}");
        eprintln!("{b:?}");
    }
    Ok(())
}

#[test]
fn normalisierung_steht_vor_dem_kurs_im_journal_und_nie_nach_sicherheitsabbruch()
-> Result<(), String> {
    let alle = werte()?;
    let aapl = alle.get(1).ok_or("Apple fehlt")?.clone();
    let sap = alle.first().ok_or("SAP fehlt")?.clone();
    let antwort = "date,close,high,low,adjClose\n2026-11-04,224.47999999999998,226,223.1,112.24\n2026-11-03,224.47999999999998,226,223.1,112.24\n2026-11-02,224.47999999999998,226,223.1,112.24\n";
    for abbrechen in [false, true] {
        let (journal, roh) = ordner()?;
        let mut netz = Erfunden::neu(&[
            (
                "ezb",
                Ok(ezb(&[
                    ("2026-11-04", "1.1224"),
                    ("2026-11-03", "1.1224"),
                    ("2026-11-02", "1.1224"),
                ])),
            ),
            ("aapl", Ok(antwort.to_string())),
            ("SAP.DEX", Ok(format!("Schluessel {AV}"))),
        ]);
        let liste = if abbrechen {
            vec![aapl.clone(), sap.clone()]
        } else {
            vec![aapl.clone()]
        };
        let b = lauf(
            &journal,
            &roh,
            &mut netz,
            &zugang()?,
            vec![utc(11, 4, 21, 4)?, utc(11, 4, 21, 5)?],
            &liste,
        )?
        .map_err(|e| format!("Lauf: {e:?}"))?;
        let j = vorbedingung(Journal::oeffnen(&journal), "Journal erneut öffnen")?;
        let notiz = j
            .eintraege()
            .iter()
            .position(|(_, t)| t.contains("Tiingo-Normalisierung"));
        if abbrechen {
            assert!(b.sicherheitsabbruch);
            assert!(notiz.is_none());
            assert!(j.stand().kurse().is_empty());
        } else {
            assert!(!b.sicherheitsabbruch);
            let belege: Vec<_> = j
                .eintraege()
                .iter()
                .filter(|(_, t)| t.contains("Tiingo-Normalisierung"))
                .collect();
            assert_eq!(belege.len(), 2, "Nur heute und Vortag belegen");
            assert!(belege.iter().all(|(_, t)| !t.contains("zeile=4;")));
            let index = notiz.ok_or("Normalisierungsbeleg fehlt")?;
            let text = &j.eintraege().get(index).ok_or("Beleg fehlt")?.1;
            for teil in [
                "original=224.47999999999998",
                "nach-millionstel-usd=224480000",
                "rundung=halb-gerade",
                "roh=",
                "feld=close",
                "zeile=2",
                "abruf=",
            ] {
                assert!(text.contains(teil), "{text}");
            }
            assert_eq!(
                j.stand().kurse().first().map(|k| k.kurs().preis()),
                Some(euro("200")?)
            );
            let kurs = j
                .eintraege()
                .iter()
                .position(|(art, _)| *art == depot_journal::Art::Kurs)
                .ok_or("Kurs fehlt")?;
            assert!(index < kurs);
        }
    }
    Ok(())
}
