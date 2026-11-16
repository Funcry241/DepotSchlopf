//! Gegenproben am Kalender der Fed, abgerufen am 16.11.2026.
//! Zeilenenden und nachgestellte Leerzeichen der HTML-Vorlage sind normalisiert.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{Journal, kurssperre};
use depot_kern::{Betroffen, Datum, Verlaesslichkeit, Zeitbezug, Zeitpunkt};
use depot_sammler::ezb::ADRESSE_EZB;
use depot_sammler::fed::{ADRESSE_FED, FedFehler, QUELLE_FED, lies_fed_seite, sammle_fed};
use depot_sammler::{Netz, NetzFehler, Schluessel, Zugang, sammle_termine};

const VORLAGE: &str = include_str!("../../../docs/befunde/vorlagen/fomc.html");
static ORTE: AtomicU32 = AtomicU32::new(0);

fn tag(n: u8) -> Result<Datum, String> {
    Datum::neu(2026, 11, n).ok_or_else(|| "Tag fehlt".to_owned())
}

fn zeit(n: u8) -> Result<Zeitpunkt, String> {
    Zeitpunkt::frankfurt(tag(n)?, 7, 20, 0).ok_or_else(|| "Zeit fehlt".to_owned())
}

struct Ort(PathBuf);
impl Ort {
    fn neu() -> Result<Self, String> {
        let nr = ORTE.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("depot-fed-{}-{nr}", std::process::id()));
        fs::create_dir(&dir).map_err(|e| e.to_string())?;
        Journal::einrichten(&dir.join("journal.txt")).map_err(|e| format!("{e:?}"))?;
        Ok(Self(dir))
    }

    fn journal(&self) -> Result<Journal, String> {
        Journal::oeffnen(&self.0.join("journal.txt")).map_err(|e| format!("{e:?}"))
    }
}
impl Drop for Ort {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Quelle {
    fed: Result<Vec<u8>, NetzFehler>,
    fed_abrufe: usize,
}
impl Quelle {
    fn seite(html: &str) -> Self {
        Self {
            fed: Ok(html.as_bytes().to_vec()),
            fed_abrufe: 0,
        }
    }
}
impl Netz for Quelle {
    fn hole(&mut self, adresse: &str) -> Result<Vec<u8>, NetzFehler> {
        match adresse {
            ADRESSE_FED => {
                self.fed_abrufe = self.fed_abrufe.saturating_add(1);
                self.fed.clone()
            }
            ADRESSE_EZB => Ok(
                b"<main><dl><dt>17/12/2026</dt><dd>Monetary policy meeting<br></dd></dl></main>"
                    .to_vec(),
            ),
            _ => Err(NetzFehler::endgueltig("Unerwartete Testadresse")),
        }
    }
}

#[test]
fn liest_alle_zinsentscheidungen_und_meldet_strategiesitzung() -> Result<(), String> {
    assert!(u64::try_from(VORLAGE.len()).is_ok_and(|n| n < depot_sammler::netz::MAX_ANTWORT));
    let kalender = lies_fed_seite(VORLAGE, tag(13)?).map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        kalender
            .termine
            .iter()
            .filter(|t| t.kennung().starts_with("fed-2026"))
            .count(),
        8
    );
    assert_eq!(
        kalender
            .termine
            .iter()
            .filter(|t| t.kennung().starts_with("fed-2027"))
            .count(),
        7
    );
    assert_eq!(kalender.ausgelassen, ["2027-10-27: Strategiesitzung"]);
    let januar = kalender
        .termine
        .iter()
        .find(|t| t.kennung().starts_with("fed-2027"))
        .ok_or("Erster Termin 2027")?;
    assert_eq!(januar.zeit().tag(), Datum::neu(2027, 1, 27));
    let dezember = kalender
        .termine
        .iter()
        .find(|t| t.kennung() == "fed-2026-12-09")
        .ok_or("Dezember")?;
    assert!(dezember.titel().ends_with(", mit Projektionen"));
    assert_eq!(
        kalender
            .termine
            .iter()
            .filter(|t| t.titel().ends_with(", mit Projektionen"))
            .count(),
        4
    );
    assert!(kalender.termine.iter().all(|t| {
        matches!(t.zeit(), Zeitbezug::Tag(_))
            && t.verlaesslichkeit() == Verlaesslichkeit::Bestaetigt
            && t.betroffen()
                .iter()
                .any(|wert| matches!(wert, Betroffen::Groesse(name) if name == "usd-leitzins"))
            && !t.titel().contains([';', '*'])
    }));
    Ok(())
}

#[test]
fn monatswechsel_und_alte_jahre() -> Result<(), String> {
    let nach_monatswechsel = VORLAGE
        .replacen("<strong>April</strong>", "<strong>Apr/May</strong>", 1)
        .replacen(">28-29</div>", ">30-1</div>", 1);
    let (aktuell, alt) = nach_monatswechsel
        .split_once("2025 FOMC Meetings")
        .ok_or("Vorlage enthält kein Altjahr")?;
    let alt = alt.replacen(">28-29</div>", ">kaputt</div>", 1);
    let kaputtes_altjahr = format!("{aktuell}2025 FOMC Meetings{alt}");
    let termine = lies_fed_seite(&kaputtes_altjahr, tag(13)?)
        .map_err(|e| format!("{e:?}"))?
        .termine;
    let t = termine
        .iter()
        .find(|t| t.kennung() == "fed-2026-05-01")
        .ok_or("Monatswechsel")?;
    assert_eq!(t.zeit().tag(), Datum::neu(2026, 5, 1));
    Ok(())
}

#[test]
fn kaputte_zeile_belegt_nichts() -> Result<(), String> {
    assert_eq!(
        lies_fed_seite("<html></html>", tag(13)?),
        Err(FedFehler::Seite)
    );
    let ort = Ort::neu()?;
    let mut j = ort.journal()?;
    let kaputt = VORLAGE.replacen(">27-28</div>", ">27-</div>", 1);
    assert_eq!(lies_fed_seite(&kaputt, tag(13)?), Err(FedFehler::Zeile(1)));
    let bericht = sammle_fed(
        &mut j,
        &mut Quelle::seite(&kaputt),
        &mut || zeit(13).ok(),
        &ort.0,
        &Zugang::leer(),
    )
    .map_err(|e| format!("{e:?}"))?;
    assert!(bericht.fehler.is_some());
    assert!(bericht.rohdatei.is_some());
    assert!(j.stand().aktuelle_termine().is_empty());
    Ok(())
}

#[test]
fn nur_ein_abruf_je_tag_unveraendert_und_verschoben() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.journal()?;
    let mut quelle = Quelle::seite(VORLAGE);
    let abruf = |j: &mut Journal, quelle: &mut Quelle, tag| {
        sammle_fed(j, quelle, &mut || zeit(tag).ok(), &ort.0, &Zugang::leer())
            .map_err(|e| format!("{e:?}"))
    };
    assert_eq!(abruf(&mut j, &mut quelle, 13)?.neu, 15);
    assert!(abruf(&mut j, &mut quelle, 13)?.uebersprungen.is_some());
    assert_eq!(quelle.fed_abrufe, 1);
    assert_eq!(abruf(&mut j, &mut quelle, 14)?.unveraendert, 15);
    let verschoben = VORLAGE.replacen(">8-9*</div>", ">9-10*</div>", 1);
    let bericht = abruf(&mut j, &mut Quelle::seite(&verschoben), 15)?;
    assert_eq!(bericht.neu, 1);
    assert_eq!(bericht.fehlen, ["fed-2026-12-09"]);
    drop(j);
    let j = ort.journal()?;
    assert_eq!(j.stand().aktuelle_termine().len(), 16);
    assert!(
        j.stand()
            .aktuelle_termine()
            .values()
            .all(|b| b.quelle() == QUELLE_FED)
    );
    Ok(())
}

#[test]
fn fed_fehler_laesst_ezb_stehen() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.journal()?;
    let mut quelle = Quelle {
        fed: Err(NetzFehler::voruebergehend("HTTP 500")),
        fed_abrufe: 0,
    };
    let mut uhr = || zeit(13).ok();
    let ezb =
        sammle_termine(&mut j, &mut quelle, &mut uhr, &ort.0).map_err(|e| format!("{e:?}"))?;
    assert_eq!(ezb.neu, 1);
    let fed = sammle_fed(&mut j, &mut quelle, &mut uhr, &ort.0, &Zugang::leer())
        .map_err(|e| format!("{e:?}"))?;
    assert!(fed.fehler.is_some() && fed.rohdatei.is_none());
    assert_eq!(j.stand().aktuelle_termine().len(), 1);
    assert!(
        j.stand()
            .aktuelle_termine()
            .values()
            .all(|b| b.quelle() == "ezb")
    );
    Ok(())
}

#[test]
fn sicherheitsabbruch_vor_rohablage_und_fed_sperre_lesbar() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.journal()?;
    let schluessel = "ERFUNDEN0123TEST";
    let zugang = Zugang::alpha_vantage(Schluessel::neu(schluessel)?);
    let mut quelle = Quelle::seite(&format!("<p>{schluessel}</p>"));
    let bericht = sammle_fed(&mut j, &mut quelle, &mut || zeit(13).ok(), &ort.0, &zugang)
        .map_err(|e| format!("{e:?}"))?;
    assert!(bericht.sicherheitsabbruch && bericht.rohdatei.is_none());
    assert!(
        bericht
            .fehler
            .as_ref()
            .is_some_and(|f| !f.contains(schluessel))
    );
    assert!(j.stand().aktuelle_termine().is_empty());
    assert_eq!(
        fs::read_dir(ort.0.join(QUELLE_FED))
            .map_err(|e| e.to_string())?
            .count(),
        1
    );
    kurssperre::sperren(&ort.0)?.sicherheit(kurssperre::Quelle::Fed, Some(zeit(13)?), "Test")?;
    assert_eq!(
        kurssperre::lies(&ort.0)?.and_then(|s| s.quelle),
        Some(kurssperre::Quelle::Fed)
    );
    Ok(())
}
