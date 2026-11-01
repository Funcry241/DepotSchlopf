//! Negativtests zu den Befunden aus `docs/BEFUNDE.md` (Stufe 2, KI-Budget und Kassenbuch).
//!
//! Ein Test mit `#[ignore = "offen: …"]` zeigt einen Befund, der im heutigen Code noch besteht:
//! `cargo test -p depot_ki --test befunde -- --ignored` ist am Stand dieses Commits rot.
//! Der Umbau entfernt das `ignore`; dann laeuft der Test im Kodex.
//!
//! Jede Vorbereitung prueft ihren Erfolg ausdruecklich. Scheitert sie, endet der Test mit
//! „Vorbedingung …“ und nie mit einer Meldung, die wie der Produktfehler aussieht.

mod sonde;

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use depot_kern::{BeraterId, Cent};
use depot_ki::{Buchung, Datum, Kassenbuch, KiDeckel, KiGrenzen, Preise, Sperre, pruefe};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

const KI: [&str; 2] = ["depot_kern", "depot_ki"];
const DACHS: BeraterId = BeraterId::Dachs;

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

/// Eigenes, leeres Verzeichnis je Test; liefert den Pfad des Kassenbuchs darin.
fn neuer_pfad() -> Result<PathBuf, String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_ki_befund_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
    Ok(dir.join("kassenbuch.txt"))
}

fn tag(t: u8) -> Result<Datum, String> {
    Datum::neu(2026, 11, t).ok_or_else(|| format!("Vorbedingung: Datum {t}.11.2026"))
}

fn oeffnen(p: &Path) -> Result<Kassenbuch, String> {
    vorbedingung(Kassenbuch::oeffnen(p), "Kassenbuch oeffnen")
}

/// Fuellt den Monat von Dachs (zwei Zyklen zu je zwei Aufrufen) und schliesst das Kassenbuch.
fn monat_voll(p: &Path) -> Result<(), String> {
    let g = KiGrenzen::standard();
    let d = KiDeckel::standard(DACHS);
    let mut k = oeffnen(p)?;
    for (t, z) in [(2, 1), (2, 1), (3, 2), (3, 2)] {
        vorbedingung(k.reserviere(&g, &d, DACHS, tag(t)?, z), "Reservierung")?;
    }
    match k.reserviere(&g, &d, DACHS, tag(4)?, 3) {
        Err(Sperre::MonatVoll) => Ok(()),
        anders => Err(format!("Vorbedingung Monat voll: {anders:?}")),
    }
}

// ---------------------------------------------------------------------------------------------
// 1·K01 – KI-Grenzen als freie Argumente (R19)
// ---------------------------------------------------------------------------------------------

/// Gegenstueck: vorhandene Grenzen und Deckel weitergeben ist erlaubt.
const KI_ERLAUBT: &str = "pub fn weiter(g: &depot_ki::KiGrenzen, d: &depot_ki::KiDeckel) \
                          -> (depot_ki::KiGrenzen, depot_ki::KiDeckel) {\n    (*g, *d)\n}\n";

/// Ausserhalb von Tests laesst sich keine KI-Grenze im Literal bauen. Der Test nutzt keinen
/// Konstruktor; eine Umbenennung von `standard` macht ihn nicht gruen.
#[test]
#[ignore = "offen: 1·K01 KI-Grenzen sind im Literal frei baubar"]
fn befund_1_k01_ki_grenze_im_literal_laesst_sich_nicht_bauen() -> Result<(), String> {
    sonde::erlaubt(&KI, KI_ERLAUBT)?;
    sonde::verboten(
        &KI,
        "pub fn locker(g: depot_ki::KiGrenzen) -> depot_ki::KiGrenzen {\n    \
         depot_ki::KiGrenzen { kosten_je_monat: depot_kern::Cent(100_000), ..g }\n}\n",
        &sonde::PRIVAT,
        &["kosten_je_monat"],
    )
}

/// Ein geladener Token-Deckel laesst sich nicht nachtraeglich lockern.
#[test]
#[ignore = "offen: 1·K01 Felder des KI-Deckels sind oeffentlich aenderbar"]
fn befund_1_k01_ki_deckel_laesst_sich_nicht_lockern() -> Result<(), String> {
    sonde::erlaubt(&KI, KI_ERLAUBT)?;
    sonde::verboten(
        &KI,
        "pub fn locker(mut d: depot_ki::KiDeckel) -> depot_ki::KiDeckel {\n    \
         d.max_token_aus = 1_000_000;\n    d\n}\n",
        &sonde::PRIVAT,
        &["max_token_aus"],
    )
}

// ---------------------------------------------------------------------------------------------
// 1·K02 – negative Preise, negative Reservierung (R20)
// ---------------------------------------------------------------------------------------------

/// Ein negativer Preis wird abgelehnt und ergibt nie eine negative Reservierung.
#[test]
#[ignore = "offen: 1·K02 negativer Preis ergibt negative Reservierung"]
fn befund_1_k02_negativer_preis_wird_abgelehnt() -> Result<(), String> {
    let mut angenommen = Vec::new();
    for preise in [
        Preise {
            cent_je_mio_ein: -1_000_000,
            cent_je_mio_aus: 2_000,
        },
        Preise {
            cent_je_mio_ein: 400,
            cent_je_mio_aus: -1,
        },
    ] {
        let mut d = KiDeckel::standard(DACHS);
        d.preise = preise;
        let ergebnis = pruefe(&KiGrenzen::standard(), &d, &[], DACHS, tag(2)?, 1);
        if ergebnis.is_ok() {
            angenommen.push(format!("{preise:?} ergibt {ergebnis:?}"));
        }
    }
    if angenommen.is_empty() {
        Ok(())
    } else {
        Err(format!("negativer Preis angenommen: {angenommen:?}"))
    }
}

// ---------------------------------------------------------------------------------------------
// 1·K03 – ungueltige Kalenderdaten (R20)
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "offen: 1·K03 Datum::neu nimmt den 31.02. an"]
fn befund_1_k03_ungueltiges_kalenderdatum_wird_abgelehnt() -> Result<(), String> {
    let mut angenommen = Vec::new();
    for (j, m, t) in [(2026, 2, 31), (2026, 2, 29), (2026, 4, 31), (2026, 11, 31)] {
        if Datum::neu(j, m, t).is_some() {
            angenommen.push(format!("{t:02}.{m:02}.{j}"));
        }
    }
    if Datum::neu(2028, 2, 29).is_none() || Datum::neu(2026, 12, 31).is_none() {
        return Err("gueltiges Datum abgelehnt".into());
    }
    if angenommen.is_empty() {
        Ok(())
    } else {
        Err(format!("angenommen: {angenommen:?}"))
    }
}

/// Der alte Umgehungsweg: Ein Datum ueber die oeffentlichen Felder bauen und damit reservieren.
/// Verlangt ist: Entweder laesst sich ein solches Datum gar nicht bauen (Zugriffsfehler), oder
/// die Reservierung mit dem 31.02. wird abgelehnt und schreibt nichts.
#[test]
#[ignore = "offen: 1·K03 Datum ueber oeffentliche Felder umgeht die Pruefung"]
fn befund_1_k03_datum_ueber_felder_umgeht_die_pruefung_nicht() -> Result<(), String> {
    let p = neuer_pfad()?;
    let programm = format!(
        r#"
use depot_kern::BeraterId;
use depot_ki::{{Datum, Kassenbuch, KiDeckel, KiGrenzen}};
fn main() {{
    let p = std::path::Path::new({p:?});
    let falsch = Datum {{ jahr: 2026, monat: 2, tag: 31 }};
    let mut k = Kassenbuch::oeffnen(p).expect("Vorbedingung: Kassenbuch");
    let r = k.reserviere(&KiGrenzen::standard(), &KiDeckel::standard(BeraterId::Dachs), BeraterId::Dachs, falsch, 1);
    assert!(r.is_err(), "Reservierung am 31.02. angenommen: {{r:?}}");
    assert!(k.buchungen().is_empty(), "Buchung am 31.02. geschrieben");
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
#[ignore = "offen: 1·K04 geloeschtes Kassenbuch setzt das Budget zurueck"]
fn befund_1_k04_geloeschtes_kassenbuch_fuehrt_zu_halt() -> Result<(), String> {
    let p = neuer_pfad()?;
    monat_voll(&p)?;
    vorbedingung(fs::remove_file(&p), "Kassenbuch loeschen")?;
    match Kassenbuch::oeffnen(&p) {
        Err(_) => Ok(()),
        Ok(_) => Err("nach dem Loeschen beginnt das Budget bei null".into()),
    }
}

#[test]
#[ignore = "offen: 1·K04 geleertes Kassenbuch setzt das Budget zurueck"]
fn befund_1_k04_geleertes_kassenbuch_fuehrt_zu_halt() -> Result<(), String> {
    let p = neuer_pfad()?;
    monat_voll(&p)?;
    vorbedingung(fs::write(&p, ""), "Kassenbuch leeren")?;
    match Kassenbuch::oeffnen(&p) {
        Err(_) => Ok(()),
        Ok(_) => Err("nach dem Leeren beginnt das Budget bei null".into()),
    }
}

#[test]
#[ignore = "offen: 1·K04 um ganze Zeilen gekuerztes Kassenbuch wird angenommen"]
fn befund_1_k04_gekuerztes_kassenbuch_fuehrt_zu_halt() -> Result<(), String> {
    let p = neuer_pfad()?;
    monat_voll(&p)?;
    let inhalt = vorbedingung(fs::read_to_string(&p), "Kassenbuch lesen")?;
    let erste: String = inhalt.split_inclusive('\n').take(2).collect();
    if erste.len() >= inhalt.len() {
        return Err("Vorbedingung: Kassenbuch hat mehr als zwei Zeilen".into());
    }
    vorbedingung(fs::write(&p, erste), "Kassenbuch kuerzen")?;
    match Kassenbuch::oeffnen(&p) {
        Err(_) => Ok(()),
        Ok(_) => Err("nach dem Kuerzen auf zwei Zeilen ist wieder Budget frei".into()),
    }
}

// ---------------------------------------------------------------------------------------------
// 1·K05 – semantisch kaputte Historie (R20, R25)
// ---------------------------------------------------------------------------------------------

/// Schreibt Buchungen in der **jeweils aktuellen** Huelle des Kassenbuchs. Heute ist das eine
/// `v1`-Zeile je Buchung. Aendert der Umbau das Format (Pruefsumme, Hash-Kette, Anker), wird
/// nur diese Funktion nachgefuehrt, damit die Faelle unten weiter in einer strukturell
/// gueltigen Huelle ankommen und allein ihre Semantik falsch ist.
fn huelle(buchungen: &[Buchung]) -> String {
    buchungen.iter().fold(String::new(), |mut aus, b| {
        let berater = match b.berater {
            BeraterId::Dachs => "Dachs",
            BeraterId::Maus => "Maus",
        };
        let _ = writeln!(
            aus,
            "v1;{berater};{:04}-{:02}-{:02};{};{}",
            b.datum.jahr, b.datum.monat, b.datum.tag, b.zyklus, b.reserviert.0
        );
        aus
    })
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
#[ignore = "offen: 1·K05 semantisch kaputte Historie wird angenommen"]
fn befund_1_k05_kaputte_historie_fuehrt_zu_halt() -> Result<(), String> {
    let hoechst = KiDeckel::standard(DACHS)
        .hoechstkosten()
        .ok_or("Vorbedingung: Hoechstkosten")?
        .0;
    let gueltig = vec![buchung(2, 3, hoechst)?, buchung(5, 4, hoechst)?];
    let p = neuer_pfad()?;
    vorbedingung(
        fs::write(&p, huelle(&gueltig)),
        "gueltige Historie schreiben",
    )?;
    let k = Kassenbuch::oeffnen(&p).map_err(|e| format!("gueltige Historie abgelehnt: {e:?}"))?;
    if k.buchungen().len() != gueltig.len() {
        return Err(format!(
            "gueltige Historie falsch gelesen: {:?}",
            k.buchungen()
        ));
    }
    drop(k);

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
        vorbedingung(fs::write(&p, huelle(buchungen)), fall)?;
        match Kassenbuch::oeffnen(&p) {
            Ok(_) => angenommen.push(*fall),
            Err(e) => gruende.push((fall, format!("{e:?}"))),
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

// ---------------------------------------------------------------------------------------------
// 1·K06 – zwei Pfade umgehen die Sperre (R24)
// ---------------------------------------------------------------------------------------------

/// Ein zweiter Name fuer dieselbe Datei (harter Link) bekommt das Kassenbuch nicht.
#[test]
#[ignore = "offen: 1·K06 Sperre haengt am Pfadnamen, nicht an der Datei"]
fn befund_1_k06_zweiter_pfad_auf_dieselbe_datei_wird_gesperrt() -> Result<(), String> {
    let p = neuer_pfad()?;
    monat_voll(&p)?;
    let zweit = p.with_file_name("zweitname.txt");
    vorbedingung(fs::hard_link(&p, &zweit), "harter Link")?;
    let erster = oeffnen(&p)?;
    let ergebnis = Kassenbuch::oeffnen(&zweit);
    drop(erster);
    match ergebnis {
        Err(Sperre::KassenbuchBelegt) => Ok(()),
        anders => Err(format!(
            "dieselbe Datei ueber einen zweiten Namen: {:?}",
            anders.map(|k| k.buchungen().len())
        )),
    }
}

/// Ein symbolischer Link im Datenverzeichnis ist verboten.
#[cfg(unix)]
#[test]
#[ignore = "offen: 1·K06 Kassenbuch ueber symbolischen Link wird geoeffnet"]
fn befund_1_k06_symbolischer_link_wird_abgelehnt() -> Result<(), String> {
    let p = neuer_pfad()?;
    monat_voll(&p)?;
    let link = p.with_file_name("link.txt");
    vorbedingung(std::os::unix::fs::symlink(&p, &link), "Link")?;
    match Kassenbuch::oeffnen(&link) {
        Err(_) => Ok(()),
        Ok(_) => Err("Kassenbuch ueber einen Link geoeffnet".into()),
    }
}

// ---------------------------------------------------------------------------------------------
// 1·R02 – Weiterschreiben nach Fehlern (R29)
// ---------------------------------------------------------------------------------------------

/// Oeffnungsfehler: An der Stelle des Kassenbuchs liegt kurz ein Verzeichnis. Danach bleibt
/// das Kassenbuch gesperrt: keine Erlaubnis, keine Buchung.
#[test]
#[ignore = "offen: 1·R02 nach einem Oeffnungsfehler wird weitergeschrieben"]
fn befund_1_r02_oeffnungsfehler_sperrt_das_kassenbuch() -> Result<(), String> {
    let pfad = neuer_pfad()?;
    let grenzen = KiGrenzen::standard();
    let deckel = KiDeckel::standard(DACHS);
    let mut buch = oeffnen(&pfad)?;
    vorbedingung(
        fs::create_dir(&pfad),
        "Verzeichnis an Stelle des Kassenbuchs",
    )?;
    if buch
        .reserviere(&grenzen, &deckel, DACHS, tag(2)?, 1)
        .is_ok()
    {
        return Err("Vorbedingung: Oeffnungsfehler tritt nicht ein".into());
    }
    vorbedingung(fs::remove_dir(&pfad), "Verzeichnis entfernen")?;
    let zweiter = buch.reserviere(&grenzen, &deckel, DACHS, tag(2)?, 1);
    let datei = fs::read_to_string(&pfad).unwrap_or_default();
    match (zweiter, buch.buchungen().is_empty(), datei.is_empty()) {
        (Err(_), true, true) => Ok(()),
        (r, _, _) => Err(format!(
            "nach dem Oeffnungsfehler weitergeschrieben: {r:?}, Datei {datei:?}"
        )),
    }
}

/// Vorgeschichte von Maus aus 2025, die jede Regel einhaelt: je Monat zwei Tage mit je einem
/// Zyklus und zwei Aufrufen (vier je Monat), Daten und Zyklen steigend, Reservierung gleich den
/// Hoechstkosten. Jede Buchung wird gegen die Regeln von `pruefe` mit der Historie davor
/// geprueft. Die Zeilen entstehen ueber `huelle`, also im jeweils aktuellen Format.
fn zulaessige_vorgeschichte() -> Result<Vec<Buchung>, String> {
    let grenzen = KiGrenzen::standard();
    let deckel = KiDeckel::standard(BeraterId::Maus);
    let mut buchungen: Vec<Buchung> = Vec::new();
    let mut zyklus = 0_u32;
    'monate: for monat in 1..=12_u8 {
        for tag in [3_u8, 17] {
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
                if huelle(&buchungen).len() > 487 {
                    break 'monate;
                }
            }
        }
    }
    Ok(buchungen)
}

/// Schreibfehler mitten in der Zeile: Eine Groessengrenze fuer Dateien (`ulimit -f`, SIGXFSZ
/// ignoriert) laesst `write_all` nach einem Teil der Zeile scheitern. Danach entfernt der Test
/// den Rest und die letzte Zeile der Vorgeschichte, sodass die Grenze fuer einen zweiten
/// Versuch kein Hindernis mehr ist. Verlangt: Der zweite Versuch bekommt
/// keine Erlaubnis und schreibt nichts. Nur Unix; fuer den Fehler beim Synchronisieren gibt es
/// ohne Speicher-Schnittstelle keinen Einspeisepunkt (Teilnachweis offen, Persistenzbasis).
#[cfg(unix)]
#[test]
#[ignore = "offen: 1·R02 nach einem Teilschreibfehler wird weitergeschrieben"]
fn befund_1_r02_teilschreibfehler_sperrt_das_kassenbuch() -> Result<(), String> {
    let p = neuer_pfad()?;
    let vor_buchungen = zulaessige_vorgeschichte()?;
    let vor = huelle(&vor_buchungen);
    if !(488..512).contains(&vor.len()) {
        return Err(format!("Vorbedingung: Vorgeschichte {} Byte", vor.len()));
    }
    vorbedingung(fs::write(&p, &vor), "Vorgeschichte")?;
    // Die Vorgeschichte ist vollstaendig zulaessig: Das Kassenbuch oeffnet sie und liest jede
    // Buchung (die Regeln selbst prueft `zulaessige_vorgeschichte`).
    let gelesen = oeffnen(&p)?;
    if gelesen.buchungen() != vor_buchungen.as_slice() {
        return Err("Vorbedingung: Vorgeschichte falsch gelesen".into());
    }
    drop(gelesen);
    let kurz = vor
        .trim_end_matches('\n')
        .rfind('\n')
        .map(|i| i.saturating_add(1))
        .ok_or("Vorbedingung: Vorgeschichte hat mehrere Zeilen")?;
    let programm = format!(
        r#"
use depot_kern::BeraterId;
use depot_ki::{{Datum, Kassenbuch, KiDeckel, KiGrenzen}};
fn main() {{
    let p = std::path::Path::new({p:?});
    let laenge = {laenge}_u64;
    let kurz = {kurz}_u64;
    let g = KiGrenzen::standard();
    let d = KiDeckel::standard(BeraterId::Dachs);
    let tag = Datum::neu(2026, 11, 2).expect("Vorbedingung: Datum");
    let mut k = Kassenbuch::oeffnen(p).expect("Vorbedingung: Kassenbuch");
    let vorher = k.buchungen().len();
    let erster = k.reserviere(&g, &d, BeraterId::Dachs, tag, 1);
    let groesse = std::fs::metadata(p).expect("Vorbedingung: Groesse").len();
    assert!(erster.is_err(), "Vorbedingung: Schreibfehler tritt nicht ein");
    assert!(groesse > laenge, "Vorbedingung: kein Teilschreiben ({{groesse}} Byte)");
    std::fs::OpenOptions::new().write(true).open(p).and_then(|f| f.set_len(kurz))
        .expect("Vorbedingung: Rest entfernen");
    let zweiter = k.reserviere(&g, &d, BeraterId::Dachs, tag, 1);
    let danach = std::fs::metadata(p).expect("Vorbedingung: Groesse").len();
    assert!(zweiter.is_err(), "nach dem Teilschreibfehler Erlaubnis erteilt");
    assert_eq!(danach, kurz, "nach dem Teilschreibfehler weitergeschrieben");
    assert_eq!(k.buchungen().len(), vorher, "Buchung im Speicher");
    assert_eq!(k.buchungen().len(), {anzahl}, "Vorbedingung: Vorgeschichte gelesen");
}}
"#,
        laenge = vor.len(),
        anzahl = vor_buchungen.len()
    );
    // Eine neue Zeile von Dachs ist 25 Byte lang ("v1;Dachs;2026-11-02;1;48\n").
    if 512_usize.saturating_sub(kurz) <= 25 || 512_usize.saturating_sub(vor.len()) >= 25 {
        return Err("Vorbedingung: Platz bis zur Grenze passt nicht zum Teilschreiben".into());
    }
    let exe = sonde::programm(&KI, &programm)?;
    let aus = vorbedingung(
        std::process::Command::new("sh")
            .arg("-c")
            .arg("trap '' XFSZ; ulimit -f 1 && exec \"$0\"")
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

// ---------------------------------------------------------------------------------------------
// 2·A12 – Hoechstpreis wird aufgerundet (R21)
// ---------------------------------------------------------------------------------------------

/// Dachs bei einem Ereignis: 16.000 Token zu 4 $ und 4.000 Token zu 20 $ je Million ergeben
/// 0,144 €. Reserviert werden 15 Cent, nie 14.
#[test]
fn grenzfall_2_a12_hoechstpreis_wird_auf_ganze_cent_aufgerundet() {
    let mut d = KiDeckel::standard(DACHS);
    d.max_token_ein = 16_000;
    d.max_token_aus = 4_000;
    assert_eq!(d.hoechstkosten(), Some(Cent(15)));
    d.max_token_aus = 0;
    d.max_token_ein = 250_000; // genau 1,00 $
    assert_eq!(d.hoechstkosten(), Some(Cent(100)));
    d.max_token_ein = 250_001;
    assert_eq!(d.hoechstkosten(), Some(Cent(101)));
}
