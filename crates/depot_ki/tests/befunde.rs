//! Negativtests zu den Befunden aus `docs/BEFUNDE.md` (Stufe 2, KI-Budget und Kassenbuch).
//!
//! Die Tests entstanden in Stufe 2, Schritt 1, mit `#[ignore = "offen: …"]` und waren am alten
//! Stand rot. Der Schnitt KI-Kostenschutz hat das `ignore` entfernt; seither laufen sie im
//! Kodex. Wo der Umbau die Schnittstelle geaendert hat (Grenzen beim Oeffnen gebunden,
//! ausdrueckliche Einrichtung, Anker), sind nur Aufrufe und Vorbereitung nachgefuehrt. Seit der
//! Persistenzbasis steht das Kassenbuch im Journal. Alle Tests, die es brauchen, stehen in
//! `crates/depot_journal/tests/`; hier bleiben die Regeln ohne Datei.
//!
//! Jede Vorbereitung prueft ihren Erfolg ausdruecklich. Scheitert sie, endet der Test mit
//! „Vorbedingung …“ und nie mit einer Meldung, die wie der Produktfehler aussieht.

mod sonde;

use depot_kern::{BeraterId, Cent};
use depot_ki::{Datum, KiDeckel, KiGrenzen, pruefe};

const KI: [&str; 2] = ["depot_kern", "depot_ki"];
const DACHS: BeraterId = BeraterId::Dachs;

fn tag(t: u8) -> Result<Datum, String> {
    Datum::neu(2026, 11, t).ok_or_else(|| format!("Vorbedingung: Datum {t}.11.2026"))
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

/// Ein negativer Preis entsteht nicht und ergibt nie eine negative Reservierung. Preise lassen
/// sich ausserhalb von `depot_ki` weder im Literal bauen noch aendern; die Pruefung auf
/// positive Werte selbst steht im Modultest `preise_sind_positiv`. Jeder gebundene Deckel
/// reserviert mehr als null.
#[test]
fn befund_1_k02_negativer_preis_wird_abgelehnt() -> Result<(), String> {
    sonde::verboten(
        &KI,
        "pub fn negativ() -> depot_ki::Preise {\n    \
         depot_ki::Preise { cent_je_mio_ein: -1_000_000, cent_je_mio_aus: 2_000 }\n}\n",
        &sonde::PRIVAT,
        &["cent_je_mio_ein"],
    )?;
    for b in [DACHS, BeraterId::Maus] {
        let kosten = KiDeckel::standard(b).hoechstkosten();
        if kosten.is_none_or(|c| c.0 <= 0) {
            return Err(format!("{b:?}: Reservierung {kosten:?}"));
        }
        let r = pruefe(
            &KiGrenzen::standard(),
            &KiDeckel::standard(b),
            &[],
            b,
            tag(2)?,
            1,
        );
        if !r.as_ref().is_ok_and(|c| c.0 > 0) {
            return Err(format!("{b:?}: pruefe ergibt {r:?}"));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// 1·K03 – ungueltige Kalenderdaten (R20)
// ---------------------------------------------------------------------------------------------

#[test]
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

// Der alte Umgehungsweg ueber die Felder eines Datums (1·K03), 1·K04, 1·K05 und der
// Teilschreibtest aus 1·R02 brauchen das Kassenbuch. Sie stehen seit der Persistenzbasis in
// `crates/depot_journal/tests/ki.rs`.

// ---------------------------------------------------------------------------------------------
// 2·A12 – Hoechstpreis wird aufgerundet (R21)
// ---------------------------------------------------------------------------------------------

/// Dachs bei einem Ereignis: 16.000 Token zu 2 $ und 4.000 Token zu 10 $ je Million ergeben
/// 0,072 €. Reserviert werden 8 Cent, nie 7.
#[test]
fn grenzfall_2_a12_hoechstpreis_wird_auf_ganze_cent_aufgerundet() {
    let p = KiDeckel::standard(DACHS).preise();
    assert_eq!(p.kosten(16_000, 4_000), Some(Cent(8)));
    assert_eq!(p.kosten(500_000, 0), Some(Cent(100))); // genau 1,00 $
    assert_eq!(p.kosten(500_001, 0), Some(Cent(101)));
}
