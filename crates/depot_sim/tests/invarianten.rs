//! Invarianten der Abstimmung: jede Folge einer Runde und die Einzelfälle (docs/MATHEMATIK.md, I3).
//!
//! Modell, Spieler und Hilfen stehen in `invarianten_modell/mod.rs`. Die Tests sind auf
//! mehrere Testprogramme verteilt, damit das Gate sie parallel startet (C1, #190).

#![allow(
    clippy::many_single_char_names,
    reason = "Testcode: kurze Namen für Zufall, Tage und Spieler"
)]

mod invarianten_modell;

use depot_journal::{Journal, vermerk_fuer};
use depot_kern::{BeraterId, TopfId};
use depot_sim::papiertag;
use invarianten_modell::{
    ERST_BP, Ort, Schritt, belege, einzelrunden, journal_eines_tages, nur_was_beide_zuletzt_tragen,
    oeffne_mit, sap, some, spieler, start, stueck, tag, universum, vorbedingung,
};

/// Jede Kombination einer Runde (siehe `nur_was_beide_zuletzt_tragen`).
#[test]
fn abstimmung_fuehrt_nur_aus_was_beide_zuletzt_tragen_eine_runde() -> Result<(), String> {
    nur_was_beide_zuletzt_tragen(&einzelrunden())
}

/// P35: Stimmen beide Zweitmeinungen zu, ist kein Punkt offen. Die Abstimmung entfällt ohne
/// Aufruf, und beide Erstmeinungen werden ausgeführt.
#[test]
fn abstimmung_entfaellt_ohne_offenen_punkt() -> Result<(), String> {
    let mut a = spieler(BeraterId::Dachs, vec![(Schritt::Aendern(1_000), true)])?;
    let mut b = spieler(BeraterId::Maus, vec![(Schritt::Aendern(1_000), true)])?;
    for urteil in b.plan.urteile.values_mut() {
        urteil.zustimmung = true;
    }
    let ort = Ort::neu()?;
    let mut j = vorbedingung(Journal::oeffnen(&ort.journal), "Journal")?;
    belege(&mut j)?;
    let mut k = depot_sim::Papierdatei::oeffnen(&ort.konto, false, &start()?)?;
    papiertag(
        &mut j,
        tag()?,
        &universum()?,
        &start()?,
        &mut a,
        &mut b,
        &mut k,
    )
    .map_err(|e| format!("Papiertag {e:?}"))?;
    assert!(
        a.gesehen.is_empty() && b.gesehen.is_empty(),
        "keine Abstimmungsrunde"
    );
    for wer in [BeraterId::Dachs, BeraterId::Maus] {
        let n = j
            .stand()
            .ki()
            .buchungen()
            .iter()
            .filter(|x| x.berater == wer)
            .count();
        assert_eq!(n, 2, "{wer:?}: nur Erst- und Zweitmeinung");
    }
    assert!(
        j.stand()
            .begruendungen()
            .iter()
            .any(|(_, _, text)| text == "Abstimmung entfällt: kein offener Punkt")
    );
    for topf in [TopfId::A, TopfId::B] {
        let ist: Vec<u64> = j
            .stand()
            .auftraege()
            .iter()
            .filter(|l| l.beleg.topf == topf)
            .map(|l| l.beleg.stueck)
            .collect();
        assert_eq!(ist, vec![stueck(ERST_BP)], "{topf:?}");
    }
    Ok(())
}

#[test]
fn abstimmungsjournal_belegt_aenderung_und_letzte_kaufthese() -> Result<(), String> {
    let bytes = journal_eines_tages(&[
        ((Schritt::Aendern(1_000), true), (Schritt::Behalten, true)),
        ((Schritt::Behalten, true), (Schritt::Behalten, true)),
    ])?;
    let ort = Ort::neu()?;
    let vermerk = some(vermerk_fuer(&bytes, 0), "Vermerk")?;
    let journal = vorbedingung(oeffne_mit(&ort, &bytes, &vermerk)?, "Journal")?;
    let gruende = journal.stand().begruendungen();
    let isin = sap()?;
    let aenderung =
        format!("Abstimmung Runde 1: {isin} 1000 bp: Begründung der Änderung (Runde 1)");
    assert_eq!(
        gruende
            .iter()
            .filter(|(_, wer, text)| wer == "Dachs" && text == &aenderung)
            .count(),
        1
    );
    assert!(
        !gruende
            .iter()
            .any(|(_, _, text)| text.starts_with(&format!("Abstimmung Runde 2: {isin}")))
    );
    assert!(
        gruende.iter().any(|(_, wer, text)| wer == "Kaufthese"
            && text.ends_with("Begründung der Änderung (Runde 1)"))
    );
    Ok(())
}

/// Derselbe Kursbeleg zweimal ändert den Stand nicht: Er wird abgelehnt, oder er steht nur
/// als zusätzlicher Beleg da, ohne Topf, Aufträge, Kassenbuch oder Abschluss zu berühren.
#[test]
fn doppelter_kursbeleg_aendert_den_stand_nicht() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = vorbedingung(Journal::oeffnen(&ort.journal), "Journal")?;
    belege(&mut j)?;
    let vorher = j.stand().clone();
    let erneut = belege(&mut j);
    let nachher = j.stand();
    let gleich = nachher.toepfe() == vorher.toepfe()
        && nachher.auftraege() == vorher.auftraege()
        && nachher.ki() == vorher.ki()
        && nachher.abschluss() == vorher.abschluss();
    match erneut {
        Err(_) if *nachher == vorher => Ok(()),
        Ok(()) if gleich => Ok(()),
        anders => Err(format!("zweiter Beleg: {anders:?}, Stand gleich: {gleich}")),
    }
}
