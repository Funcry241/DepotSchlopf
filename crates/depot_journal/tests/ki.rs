//! Negativtests zum KI-Kassenbuch im Journal (`docs/BEFUNDE.md`, Stufe 2). Aus
//! `crates/depot_ki/tests/befunde.rs` hierher verlegt, seit das Kassenbuch im Journal steht.
//!
//! Die Tests entstanden in Stufe 2, Schritt 1, mit `#[ignore = "offen: …"]` und waren am alten
//! Stand rot. Der Schnitt KI-Kostenschutz hat das `ignore` entfernt; seither laufen sie im
//! Kodex. Wo der Umbau die Schnittstelle geaendert hat (Grenzen beim Oeffnen gebunden,
//! ausdrueckliche Einrichtung, Anker), sind nur Aufrufe und Vorbereitung nachgefuehrt. Seit der
//! Persistenzbasis steht das Kassenbuch im Journal; die Huelle und die Aufrufe sind
//! nachgefuehrt.
//!
//! Jede Vorbereitung prueft ihren Erfolg ausdruecklich. Scheitert sie, endet der Test mit
//! „Vorbedingung …“ und nie mit einer Meldung, die wie der Produktfehler aussieht.

mod sonde;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{Art, Journal, inhalt_fuer, vermerk_fuer};
use depot_kern::{BeraterId, Cent};
use depot_ki::{Buchung, Datum, KiDeckel, grenzabdruck};
#[cfg(unix)]
use depot_ki::{KiGrenzen, pruefe};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

const KI: [&str; 4] = ["depot_kern", "depot_regelwerk", "depot_ki", "depot_journal"];
const DACHS: BeraterId = BeraterId::Dachs;

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

/// Eigenes, leeres Verzeichnis je Test; liefert den Pfad des Journals darin. Eingerichtet ist
/// es nicht. Das Kassenbuch steht seit der Persistenzbasis im Journal.
fn neuer_pfad() -> Result<PathBuf, String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_ki_befund_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
    Ok(dir.join("journal.txt"))
}

/// Wie `neuer_pfad`, mit ausdruecklich eingerichtetem Journal.
fn eingerichtet() -> Result<PathBuf, String> {
    let p = neuer_pfad()?;
    vorbedingung(Journal::einrichten(&p), "Einrichtung")?;
    Ok(p)
}

fn tag(t: u8) -> Result<Datum, String> {
    Datum::neu(2026, 11, t).ok_or_else(|| format!("Vorbedingung: Datum {t}.11.2026"))
}

fn journal(p: &Path) -> Result<Journal, String> {
    vorbedingung(Journal::oeffnen(p), "Journal oeffnen")
}

/// Liest das Kassenbuch aus dem Journal: Anzahl der Buchungen, oder der Grund des Halts.
fn buchungen_in(p: &Path) -> Result<usize, String> {
    let j = Journal::oeffnen(p).map_err(|e| format!("{e:?}"))?;
    Ok(j.stand().ki().buchungen().len())
}

/// Fuellt den Monat von Dachs (23 Zyklen zu je zwei Aufrufen) und schliesst das Journal.
fn monat_voll(p: &Path) -> Result<(), String> {
    let mut k = journal(p)?;
    for t in 1..=23_u8 {
        for _ in 0..2 {
            vorbedingung(k.reserviere(DACHS, tag(t)?, u32::from(t)), "Reservierung")?;
        }
    }
    match k.reserviere(DACHS, tag(24)?, 24) {
        Err(depot_ki::Sperre::MonatVoll) => Ok(()),
        anders => Err(format!("Vorbedingung Monat voll: {anders:?}")),
    }
}

// ---------------------------------------------------------------------------------------------
// 1·K03 – ungueltige Kalenderdaten (R20)
// ---------------------------------------------------------------------------------------------

/// Der alte Umgehungsweg: Ein Datum ueber die oeffentlichen Felder bauen und damit reservieren.
/// Verlangt ist: Entweder laesst sich ein solches Datum gar nicht bauen (Zugriffsfehler), oder
/// die Reservierung mit dem 31.02. wird abgelehnt und schreibt nichts.
#[test]
fn befund_1_k03_datum_ueber_felder_umgeht_die_pruefung_nicht() -> Result<(), String> {
    let p = eingerichtet()?;
    let programm = format!(
        r#"
use depot_kern::BeraterId;
use depot_journal::Journal;
use depot_ki::Datum;
fn main() {{
    let p = std::path::Path::new({p:?});
    let falsch = Datum {{ jahr: 2026, monat: 2, tag: 31 }};
    let mut k = Journal::oeffnen(p).expect("Vorbedingung: Journal");
    let r = k.reserviere(BeraterId::Dachs, falsch, 1);
    assert!(r.is_err(), "Reservierung am 31.02. angenommen: {{r:?}}");
    assert!(k.stand().ki().buchungen().is_empty(), "Buchung am 31.02. geschrieben");
}}
"#
    );
    match sonde::laeuft(&KI, &programm) {
        sonde::Ergebnis::Baut { lauf_ok: true, .. } => Ok(()),
        sonde::Ergebnis::Fehler(d)
            if d.iter().all(|x| {
                x.code
                    .as_deref()
                    .is_some_and(|c| sonde::PRIVAT.contains(&c))
            }) =>
        {
            Ok(())
        }
        anders => Err(format!("{anders:?}")),
    }
}

// ---------------------------------------------------------------------------------------------
// 1·K04 – leeres oder fehlendes Kassenbuch setzt die Grenzen zurueck (R25, R38)
// ---------------------------------------------------------------------------------------------

#[test]
fn befund_1_k04_geloeschtes_kassenbuch_fuehrt_zu_halt() -> Result<(), String> {
    let p = eingerichtet()?;
    monat_voll(&p)?;
    vorbedingung(fs::remove_file(&p), "Journal loeschen")?;
    if buchungen_in(&p).is_ok() {
        return Err("nach dem Loeschen beginnt das Budget bei null".into());
    }
    // Auch ohne Vermerk richtet das Oeffnen nichts neu ein; das tut nur `einrichten`.
    vorbedingung(
        fs::remove_file(p.with_extension("vermerk")),
        "Vermerk loeschen",
    )?;
    match buchungen_in(&p) {
        Err(_) if !p.exists() => Ok(()),
        anders => Err(format!(
            "nach dem Loeschen von Journal und Vermerk: {anders:?}, Datei da: {}",
            p.exists()
        )),
    }
}

#[test]
fn befund_1_k04_geleertes_kassenbuch_fuehrt_zu_halt() -> Result<(), String> {
    let p = eingerichtet()?;
    monat_voll(&p)?;
    vorbedingung(fs::write(&p, ""), "Journal leeren")?;
    match buchungen_in(&p) {
        Err(_) => Ok(()),
        Ok(n) => Err(format!(
            "nach dem Leeren beginnt das Budget bei {n} Buchungen"
        )),
    }
}

#[test]
fn befund_1_k04_gekuerztes_kassenbuch_fuehrt_zu_halt() -> Result<(), String> {
    let p = eingerichtet()?;
    monat_voll(&p)?;
    let inhalt = vorbedingung(fs::read_to_string(&p), "Journal lesen")?;
    // Kopf, Grenzabdruck und zwei Buchungen bleiben, zwei Buchungen fallen weg.
    let erste: String = inhalt.split_inclusive('\n').take(4).collect();
    if erste.len() >= inhalt.len() {
        return Err("Vorbedingung: Journal hat mehr als vier Zeilen".into());
    }
    vorbedingung(fs::write(&p, erste), "Journal kuerzen")?;
    match buchungen_in(&p) {
        Err(_) => Ok(()),
        Ok(n) => Err(format!(
            "nach dem Kuerzen auf {n} Buchungen ist wieder Budget frei"
        )),
    }
}

// ---------------------------------------------------------------------------------------------
// 1·K05 – semantisch kaputte Historie (R20, R25)
// ---------------------------------------------------------------------------------------------

/// Die KI-Eintraege fuer genau diese Buchungen im **jeweils aktuellen** Format, unter dem
/// heutigen Grenzabdruck. Aendert der Umbau das Format, werden nur diese Funktionen
/// nachgefuehrt, damit die Faelle unten in einer strukturell gueltigen Huelle ankommen und
/// allein ihre Semantik falsch ist.
fn huelle(buchungen: &[Buchung]) -> Vec<(Art, String)> {
    let mut aus = vec![(Art::KiGrenzen, grenzabdruck())];
    for b in buchungen {
        let berater = match b.berater {
            BeraterId::Dachs => "Dachs",
            BeraterId::Maus => "Maus",
        };
        aus.push((
            Art::Ki,
            format!(
                "{berater};{:04}-{:02}-{:02};{};{}",
                b.datum.jahr(),
                b.datum.monat(),
                b.datum.tag(),
                b.zyklus,
                b.reserviert.0
            ),
        ));
    }
    aus
}

/// Journal und passender Vermerk fuer genau diese Eintraege, alle bestaetigt.
fn schreib_eintraege(p: &Path, eintraege: &[(Art, String)], was: &str) -> Result<Vec<u8>, String> {
    let liste: Vec<(Art, &str)> = eintraege.iter().map(|(a, t)| (*a, t.as_str())).collect();
    let inhalt = inhalt_fuer(&liste);
    let v = vermerk_fuer(&inhalt, 0).ok_or("Vorbedingung: Vermerk")?;
    vorbedingung(fs::write(p, &inhalt), was)?;
    vorbedingung(fs::write(p.with_extension("vermerk"), v), was)?;
    Ok(inhalt)
}

fn schreib_huelle(p: &Path, buchungen: &[Buchung], was: &str) -> Result<Vec<u8>, String> {
    schreib_eintraege(p, &huelle(buchungen), was)
}

fn hoechstkosten() -> Result<i64, String> {
    Ok(KiDeckel::standard(DACHS)
        .hoechstkosten()
        .ok_or("Vorbedingung: Hoechstkosten")?
        .0)
}

fn buchung(t: u8, zyklus: u32, cent: i64) -> Result<Buchung, String> {
    Ok(Buchung {
        berater: DACHS,
        datum: tag(t)?,
        zyklus,
        reserviert: Cent(cent),
    })
}

/// Eine gueltige Historie in derselben Huelle oeffnet. Jede Historie mit genau einem
/// semantischen Fehler fuehrt zu Halt, und die Fehlerursachen sind unterscheidbar.
#[test]
fn befund_1_k05_kaputte_historie_fuehrt_zu_halt() -> Result<(), String> {
    let hoechst = hoechstkosten()?;
    let gueltig = vec![buchung(2, 3, hoechst)?, buchung(5, 4, hoechst)?];
    let p = neuer_pfad()?;
    schreib_huelle(&p, &gueltig, "gueltige Historie schreiben")?;
    let n = buchungen_in(&p).map_err(|e| format!("gueltige Historie abgelehnt: {e}"))?;
    if n != gueltig.len() {
        return Err(format!("gueltige Historie falsch gelesen: {n} Buchungen"));
    }

    let faelle = [
        (
            "Datum rueckwaerts",
            vec![buchung(5, 3, hoechst)?, buchung(2, 4, hoechst)?],
        ),
        (
            "Zyklus rueckwaerts",
            vec![buchung(2, 9, hoechst)?, buchung(3, 4, hoechst)?],
        ),
        (
            "Reservierung ueber dem Hoechstpreis eines Aufrufs",
            vec![buchung(2, 1, 999_999)?],
        ),
        (
            "mehr Aufrufe im Zyklus als erlaubt",
            vec![
                buchung(2, 1, hoechst)?,
                buchung(2, 1, hoechst)?,
                buchung(2, 1, hoechst)?,
            ],
        ),
    ];
    let mut angenommen = Vec::new();
    let mut gruende: Vec<(&str, String)> = Vec::new();
    for (fall, buchungen) in &faelle {
        let p = neuer_pfad()?;
        schreib_huelle(&p, buchungen, fall)?;
        match buchungen_in(&p) {
            Ok(_) => angenommen.push(*fall),
            Err(e) => gruende.push((fall, e)),
        }
    }
    if !angenommen.is_empty() {
        return Err(format!("angenommen: {angenommen:?}"));
    }
    for (i, (fa, ga)) in gruende.iter().enumerate() {
        for (fb, gb) in gruende.iter().skip(i.saturating_add(1)) {
            if ga == gb {
                return Err(format!("nicht unterscheidbar: {fa} und {fb} melden {ga}"));
            }
        }
    }
    Ok(())
}

// 1·K06, K12-01 und K12-02 (Sperre, Links, Zwischendatei) betreffen seit der Persistenzbasis
// das Journal und stehen in `crates/depot_journal/tests/befunde.rs`.

// ---------------------------------------------------------------------------------------------
// 1·R02 – Weiterschreiben nach Fehlern (R29)
// ---------------------------------------------------------------------------------------------

// Den Fehler beim Schreiben des Vermerks und den eingespeisten Synchronisationsfehler prueft
// das Journal (`crates/depot_journal`).

/// Vorgeschichte von Maus aus 2025, die jede Regel einhaelt: je Monat zwei Tage mit je einem
/// Zyklus und zwei Aufrufen, Daten und Zyklen steigend, Reservierung gleich den Hoechstkosten.
/// Jede Buchung wird gegen die Regeln von `pruefe` mit der Historie davor geprueft.
#[cfg(unix)]
fn zulaessige_vorgeschichte() -> Result<Vec<Buchung>, String> {
    let grenzen = KiGrenzen::standard();
    let deckel = KiDeckel::standard(BeraterId::Maus);
    let mut buchungen: Vec<Buchung> = Vec::new();
    let mut zyklus = 0_u32;
    for (monat, tag) in [(1_u8, 3_u8), (1, 17), (2, 3)] {
        zyklus = zyklus.saturating_add(1);
        let datum = Datum::neu(2025, monat, tag)
            .ok_or_else(|| format!("Vorbedingung: Datum {tag}.{monat}.2025"))?;
        for _ in 0..2 {
            let reserviert = pruefe(
                &grenzen,
                &deckel,
                &buchungen,
                BeraterId::Maus,
                datum,
                zyklus,
            )
            .map_err(|e| format!("Vorbedingung: Vorgeschichte unzulaessig: {e:?}"))?;
            buchungen.push(Buchung {
                berater: BeraterId::Maus,
                datum,
                zyklus,
                reserviert,
            });
        }
    }
    Ok(buchungen)
}

/// Groessengrenze fuer Dateien in der Probe: `ulimit -f 2`, also 1024 Byte.
#[cfg(unix)]
const GRENZE: usize = 1024;

/// Schreibfehler mitten in der Zeile: Eine Groessengrenze fuer Dateien (`ulimit -f`, SIGXFSZ
/// ignoriert) laesst `write_all` nach einem Teil der Zeile scheitern. Die Vorgeschichte endet
/// knapp unter der Grenze mit einer langen Beobachtung. Danach entfernt der Test den Rest und
/// diese Beobachtung, sodass die Grenze fuer einen zweiten Versuch kein Hindernis mehr ist.
/// Verlangt: Der zweite Versuch bekommt keine Erlaubnis und schreibt nichts. Nur Unix.
#[cfg(unix)]
#[test]
fn befund_1_r02_teilschreibfehler_sperrt_das_kassenbuch() -> Result<(), String> {
    let p = eingerichtet()?;
    let vor_buchungen = zulaessige_vorgeschichte()?;
    let mut eintraege = huelle(&vor_buchungen);
    let ohne = inhalt_fuer(
        &eintraege
            .iter()
            .map(|(a, t)| (*a, t.as_str()))
            .collect::<Vec<_>>(),
    )
    .len();
    // Eine neue Zeile von Dachs ist rund 90 Byte lang; die Vorgeschichte endet 10 Byte vor
    // der Grenze.
    let mut fuellung = None;
    for k in 1..GRENZE {
        let mut probe = eintraege.clone();
        probe.push((Art::Beobachtung, format!("2025-03-01;{}", "x".repeat(k))));
        let n = inhalt_fuer(
            &probe
                .iter()
                .map(|(a, t)| (*a, t.as_str()))
                .collect::<Vec<_>>(),
        )
        .len();
        if (GRENZE - 12..GRENZE - 8).contains(&n) {
            fuellung = Some(probe);
            break;
        }
    }
    eintraege = fuellung.ok_or("Vorbedingung: Fuellung passt nicht")?;
    let vor = schreib_eintraege(&p, &eintraege, "Vorgeschichte")?;
    let kurz = ohne;
    // Die Vorgeschichte ist vollstaendig zulaessig und wird gelesen.
    if buchungen_in(&p)? != vor_buchungen.len() {
        return Err("Vorbedingung: Vorgeschichte falsch gelesen".into());
    }
    let programm = format!(
        r#"
use depot_kern::BeraterId;
use depot_journal::Journal;
use depot_ki::Datum;
fn main() {{
    let p = std::path::Path::new({p:?});
    let laenge = {laenge}_u64;
    let kurz = {kurz}_u64;
    let tag = Datum::neu(2026, 11, 2).expect("Vorbedingung: Datum");
    let mut k = Journal::oeffnen(p).expect("Vorbedingung: Journal");
    let vorher = k.stand().ki().buchungen().len();
    let erster = k.reserviere(BeraterId::Dachs, tag, 1);
    let groesse = std::fs::metadata(p).expect("Vorbedingung: Groesse").len();
    assert!(erster.is_err(), "Vorbedingung: Schreibfehler tritt nicht ein");
    assert!(groesse > laenge, "Vorbedingung: kein Teilschreiben ({{groesse}} Byte)");
    std::fs::OpenOptions::new().write(true).open(p).and_then(|f| f.set_len(kurz))
        .expect("Vorbedingung: Rest entfernen");
    let zweiter = k.reserviere(BeraterId::Dachs, tag, 1);
    let danach = std::fs::metadata(p).expect("Vorbedingung: Groesse").len();
    assert!(zweiter.is_err(), "nach dem Teilschreibfehler Erlaubnis erteilt");
    assert_eq!(danach, kurz, "nach dem Teilschreibfehler weitergeschrieben");
    assert_eq!(k.stand().ki().buchungen().len(), vorher, "Buchung im Speicher");
    assert_eq!(k.stand().ki().buchungen().len(), {anzahl}, "Vorbedingung: Vorgeschichte gelesen");
}}
"#,
        laenge = vor.len(),
        anzahl = vor_buchungen.len()
    );
    if GRENZE.saturating_sub(kurz) <= 100 {
        return Err("Vorbedingung: Platz bis zur Grenze passt nicht zum Teilschreiben".into());
    }
    let exe = sonde::programm(&KI, &programm)?;
    let aus = vorbedingung(
        std::process::Command::new("sh")
            .arg("-c")
            .arg("trap '' XFSZ; ulimit -f 2 && exec \"$0\"")
            .arg(&exe)
            .output(),
        "Probe starten",
    )?;
    if aus.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&aus.stderr).into_owned())
    }
}
