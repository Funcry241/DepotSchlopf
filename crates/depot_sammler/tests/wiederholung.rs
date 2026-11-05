//! Kurze Netzausfaelle beim Kursabruf (Maus, 05.11.2026): Ein voruebergehender Fehler wird nach
//! kurzer Wartezeit wiederholt, hoechstens dreimal und nur, solange die Kurse noch frisch genug
//! waeren. Uhr und Warten sind erfunden: Die Uhr laeuft mit dem Warten mit, niemand schlaeft.
//!
//! Abnahmefall 1 (Maus): Router waehrend des Kursabrufs neu starten.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use depot_journal::Journal;
use depot_kern::{Boerse, Datum, Grenzen, Zeitpunkt, hinweis};
use depot_sammler::liste::pruefe_liste;
use depot_sammler::netz::{ausfall_text, frist, verlauf_text};
use depot_sammler::{
    Ausfall, Bericht, Ergebnis, Netz, NetzFehler, SammelFehler, Schluessel, Takt, Verlauf, Wert,
    Zugang, sammle,
};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

fn ordner() -> Result<(PathBuf, PathBuf), String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_wiederholung_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
    let journal = dir.join("journal.txt");
    vorbedingung(Journal::einrichten(&journal), "Einrichtung")?;
    Ok((journal, dir.join("roh")))
}

fn utc(m: u8, t: u8, h: u8, min: u8, s: u8) -> Result<Zeitpunkt, String> {
    Datum::neu(2026, m, t)
        .and_then(|d| Zeitpunkt::neu(d, h, min, s))
        .ok_or_else(|| "Vorbedingung: Zeitpunkt".into())
}

fn werte() -> Result<Vec<Wert>, String> {
    vorbedingung(
        pruefe_liste(&[
            ("DE0007164600", Boerse::Xetra, "SAP.DEX", "SAP"),
            (
                "DE0007100000",
                Boerse::Xetra,
                "MBG.DEX",
                "Mercedes-Benz Group",
            ),
        ]),
        "Liste",
    )
}

/// Erfundene Kurse in der echten Form von Alpha Vantage, Handelstag Mittwoch 04.11.2026.
const ROH_SAP: &str = "timestamp,open,high,low,close,volume\r\n\
2026-11-04,210.0000,212.4500,209.1050,211.8000,1200000\r\n\
2026-11-03,208.5000,210.2000,207.9000,209.9500,1100000\r\n";
const ROH_MBG: &str = "timestamp,open,high,low,close,volume\r\n\
2026-11-04,60.1000,61.0000,59.8000,60.7500,900000\r\n\
2026-11-03,59.5000,60.4000,59.2000,60.0500,850000\r\n";

/// Antworten der Reihe nach, gleich fuer welche Adresse; danach „unbekannt“ (endgueltig).
struct Folge {
    antworten: VecDeque<Result<&'static str, NetzFehler>>,
    abrufe: usize,
}

impl Folge {
    fn neu(a: Vec<Result<&'static str, NetzFehler>>) -> Folge {
        Folge {
            antworten: a.into(),
            abrufe: 0,
        }
    }
}

impl Netz for Folge {
    fn hole(&mut self, _adresse: &str) -> Result<Vec<u8>, NetzFehler> {
        self.abrufe = self.abrufe.saturating_add(1);
        self.antworten
            .pop_front()
            .unwrap_or_else(|| Err(NetzFehler::endgueltig("unbekannt")))
            .map(|t| t.as_bytes().to_vec())
    }
}

fn weg() -> Result<&'static str, NetzFehler> {
    Err(NetzFehler::voruebergehend("io: Connection reset by peer"))
}

/// Bericht, Meldungen und gewartete Sekunden eines Laufs.
type Lauf = (Result<Bericht, SammelFehler>, Vec<Verlauf>, u64);

/// Ein Lauf ab `start`; die Uhr geht nur mit dem Warten weiter. Gibt Bericht, Meldungen und
/// die Summe des Wartens zurueck.
fn lauf(netz: &mut Folge, start: Zeitpunkt) -> Result<Lauf, String> {
    lauf_mit(netz, start, 0)
}

/// Wie `lauf`; jedes Warten dauert `ueberzug` Sekunden laenger als geplant (Ruhezustand).
fn lauf_mit(netz: &mut Folge, start: Zeitpunkt, ueberzug: u64) -> Result<Lauf, String> {
    let (journal, roh) = ordner()?;
    let mut j = vorbedingung(Journal::oeffnen(&journal), "Journal oeffnen")?;
    let jetzt = Rc::new(Cell::new(start.sekunden()));
    let gewartet = Rc::new(Cell::new(0_u64));
    let meldungen = RefCell::new(Vec::new());
    let j2 = Rc::clone(&jetzt);
    let mut uhr = move || Zeitpunkt::aus_sekunden(j2.get());
    let (j3, g2) = (Rc::clone(&jetzt), Rc::clone(&gewartet));
    let mut schlafe = move |d: Duration| {
        let s = d.as_secs().saturating_add(ueberzug);
        g2.set(g2.get().saturating_add(s));
        j3.set(
            j3.get()
                .saturating_add(i64::try_from(s).unwrap_or(i64::MAX)),
        );
    };
    let mut melde = |v: &Verlauf| meldungen.borrow_mut().push(v.clone());
    let mut takt = Takt {
        uhr: &mut uhr,
        schlafe: &mut schlafe,
        melde: &mut melde,
    };
    let s = vorbedingung(Schluessel::neu("ERFUNDEN0123TEST"), "Schluessel")?;
    let b = sammle(
        &mut j,
        netz,
        &Zugang::alpha_vantage(s),
        &mut takt,
        &roh,
        Duration::ZERO,
        &werte()?,
    );
    Ok((b, meldungen.into_inner(), gewartet.get()))
}

/// Die Frist: Schluss der Boerse am letzten Handelstag bis jetzt, plus Hoechstalter (20 min),
/// minus Zeitgrenze eines Abrufs (30 s).
#[test]
fn frist_aus_schluss_und_hoechstalter() -> Result<(), String> {
    let g = Grenzen::standard();
    // Xetra 17:35 MEZ = 16:35 UTC.
    assert_eq!(
        frist(Boerse::Xetra, &g, utc(11, 4, 16, 45, 0)?),
        Some(utc(11, 4, 16, 54, 30)?)
    );
    // Vor dem Schluss gilt der Vortag: Morgens wird ein alter Kurs nicht stundenlang versucht.
    assert_eq!(
        frist(Boerse::Xetra, &g, utc(11, 4, 10, 0, 0)?),
        Some(utc(11, 3, 16, 54, 30)?)
    );
    // Montag frueh: der Freitag davor.
    assert_eq!(
        frist(Boerse::Xetra, &g, utc(11, 9, 8, 0, 0)?),
        Some(utc(11, 6, 16, 54, 30)?)
    );
    // NYSE 16:00 EST = 21:00 UTC (Winterzeit seit 01.11.2026).
    assert_eq!(
        frist(Boerse::Nyse, &g, utc(11, 4, 21, 5, 0)?),
        Some(utc(11, 4, 21, 19, 30)?)
    );
    Ok(())
}

/// Abnahmefall 1: Router waehrend des Kursabrufs neu starten. Zwei Abrufe scheitern an der
/// Verbindung, der dritte gelingt. Die Kurse sind belegt und verwendbar; die Oberflaeche sah
/// „Verbindung weg. Naechster Versuch um …“ und danach „wieder da“.
#[test]
fn router_neustart_waehrend_des_kursabrufs() -> Result<(), String> {
    let mut netz = Folge::neu(vec![weg(), weg(), Ok(ROH_SAP), Ok(ROH_MBG)]);
    let (b, meldungen, gewartet) = lauf(&mut netz, utc(11, 4, 16, 45, 0)?)?;
    let b = b.map_err(|e| format!("Lauf: {e:?}"))?;
    assert_eq!(netz.abrufe, 4);
    assert_eq!(gewartet, 90, "30 s und 60 s");
    assert!(b.verwendbar(), "{b:?}");
    assert!(b.verbindung.is_none() && b.abbruch.is_none(), "{b:?}");
    assert!(
        b.werte
            .iter()
            .all(|(_, e)| matches!(e, Ergebnis::Belegt { .. })),
        "{b:?}"
    );
    let texte: Vec<String> = meldungen.iter().map(verlauf_text).collect();
    assert_eq!(texte.len(), 3, "{texte:?}");
    let erste = texte.first().ok_or("keine Meldung")?;
    assert!(
        erste.starts_with("Verbindung weg. Nächster Versuch um 17:45 Uhr (Versuch 2 von 4)"),
        "{erste}"
    );
    assert!(erste.contains("du musst nichts tun"), "{erste}");
    let zweite = texte.get(1).ok_or("zweite Meldung fehlt")?;
    assert!(
        zweite.contains("um 17:46 Uhr (Versuch 3 von 4)"),
        "{zweite}"
    );
    assert_eq!(
        meldungen.last(),
        Some(&Verlauf::WiederDa { versuch: 3 }),
        "{meldungen:?}"
    );
    // Als Zeile fuer die Oberflaeche.
    let zeile = hinweis::zeile(erste);
    assert_eq!(hinweis::lies(&zeile), Some(erste.as_str()));
    Ok(())
}

/// Bleibt die Verbindung weg, endet die Wiederholung vor dem Ende des Frischefensters: Ein
/// Versuch, der erst nach 17:54:30 Uhr beginnen koennte, unterbleibt. Bei dieser Quelle wird
/// kein weiterer Wert abgerufen, nichts ist verwendbar, und der Hinweis sagt, warum der
/// Papiertag heute ausfaellt.
#[test]
fn verbindung_bleibt_weg_bis_zum_ende_der_frist() -> Result<(), String> {
    let mut netz = Folge::neu(vec![weg(), weg(), weg(), weg(), Ok(ROH_MBG)]);
    let (b, meldungen, gewartet) = lauf(&mut netz, utc(11, 4, 16, 52, 0)?)?;
    let b = b.map_err(|e| format!("Lauf: {e:?}"))?;
    // 16:52:00, 16:52:30, 16:53:30; der naechste begaenne 16:55:30.
    assert_eq!(netz.abrufe, 3);
    assert_eq!(gewartet, 90);
    assert_eq!(meldungen.len(), 2, "{meldungen:?}");
    let Some(a @ Ausfall::FristVorbei { versuche: 3, .. }) = &b.verbindung else {
        return Err(format!("FristVorbei erwartet: {b:?}"));
    };
    assert!(!b.verwendbar(), "{b:?}");
    // Bei derselben Quelle wird nichts mehr abgerufen.
    assert!(
        matches!(b.werte.last(), Some((_, Ergebnis::Uebersprungen(t))) if t.contains("Verbindung zu")),
        "{b:?}"
    );
    let text = ausfall_text(a, b.verwendbar(), &Grenzen::standard());
    assert!(text.starts_with("Papiertag heute ausgefallen."), "{text}");
    assert!(text.contains("bis 17:54 Uhr nicht wieder"), "{text}");
    assert!(text.contains("älter als 20 Minuten"), "{text}");
    // Ob es von selbst weitergeht, sagt der Dienst aus seinem Zeitplan, nicht der Sammler.
    assert!(!text.contains("von selbst"), "{text}");
    Ok(())
}

/// Hoechstens vier Versuche, auch wenn die Frist noch weiter reicht.
#[test]
fn hoechstens_vier_versuche() -> Result<(), String> {
    let mut netz = Folge::neu(vec![weg(), weg(), weg(), weg(), Ok(ROH_SAP)]);
    let (b, _, gewartet) = lauf(&mut netz, utc(11, 4, 16, 36, 0)?)?;
    let b = b.map_err(|e| format!("Lauf: {e:?}"))?;
    assert_eq!(netz.abrufe, 4);
    assert_eq!(gewartet, 210);
    assert!(
        matches!(
            b.verbindung,
            Some(Ausfall::KeineVersucheMehr { versuche: 4, .. })
        ),
        "{b:?}"
    );
    Ok(())
}

/// Nur voruebergehende Fehler werden wiederholt. Eine Ablehnung (etwa Status 404 oder 429)
/// zaehlt einmal; der naechste Wert wird abgerufen, als waere nichts gewesen.
#[test]
fn ablehnung_wird_nicht_wiederholt() -> Result<(), String> {
    let mut netz = Folge::neu(vec![
        Err(NetzFehler::endgueltig("http status: 429")),
        Ok(ROH_MBG),
    ]);
    let (b, meldungen, gewartet) = lauf(&mut netz, utc(11, 4, 16, 45, 0)?)?;
    let b = b.map_err(|e| format!("Lauf: {e:?}"))?;
    assert_eq!(netz.abrufe, 2);
    assert_eq!(gewartet, 0);
    assert!(meldungen.is_empty(), "{meldungen:?}");
    assert!(b.verbindung.is_none() && b.abbruch.is_none(), "{b:?}");
    assert!(
        matches!(b.werte.first(), Some((_, Ergebnis::Fehler(t))) if t.starts_with("Abruf:")),
        "{b:?}"
    );
    Ok(())
}

/// Abends, lange nach dem Frischefenster: kein zweiter Versuch, kein Warten.
#[test]
fn nach_der_frist_kein_zweiter_versuch() -> Result<(), String> {
    let mut netz = Folge::neu(vec![weg(), Ok(ROH_SAP), Ok(ROH_MBG)]);
    let (b, meldungen, gewartet) = lauf(&mut netz, utc(11, 4, 20, 0, 0)?)?;
    let b = b.map_err(|e| format!("Lauf: {e:?}"))?;
    assert_eq!(netz.abrufe, 1);
    assert_eq!(gewartet, 0);
    assert!(meldungen.is_empty(), "{meldungen:?}");
    assert!(
        matches!(b.verbindung, Some(Ausfall::FristVorbei { versuche: 1, .. })),
        "{b:?}"
    );
    Ok(())
}

/// Maus, Befund 2 zu #38: Dauert das Warten laenger als geplant (der Rechner schlief, die Uhr
/// sprang), prueft der Sammler die Frist unmittelbar vor dem naechsten Versuch noch einmal. Ein
/// Versuch nach der Frist geht nicht hinaus.
#[test]
fn nach_langem_warten_kein_versuch_nach_der_frist() -> Result<(), String> {
    let mut netz = Folge::neu(vec![weg(), Ok(ROH_SAP), Ok(ROH_MBG)]);
    // 16:52:00 gescheitert; geplant 16:52:30, das Warten dauert aber bis 16:57:30.
    let (b, meldungen, gewartet) = lauf_mit(&mut netz, utc(11, 4, 16, 52, 0)?, 300)?;
    let b = b.map_err(|e| format!("Lauf: {e:?}"))?;
    assert_eq!(netz.abrufe, 1, "kein Versuch nach 16:54:30");
    assert_eq!(gewartet, 330);
    assert_eq!(meldungen.len(), 1, "{meldungen:?}");
    assert!(
        matches!(b.verbindung, Some(Ausfall::FristVorbei { versuche: 1, .. })),
        "{b:?}"
    );
    assert!(!b.verwendbar());
    Ok(())
}
