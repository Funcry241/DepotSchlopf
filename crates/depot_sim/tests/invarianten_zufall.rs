//! Invarianten der Abstimmung: zufällige Folgen bis drei Runden mit festen Startwerten (docs/MATHEMATIK.md, I3).
//!
//! Modell, Spieler und Hilfen stehen in `invarianten_modell/mod.rs`. Die Tests sind auf
//! mehrere Testprogramme verteilt, damit das Gate sie parallel startet (C1, #190).

#![allow(
    clippy::many_single_char_names,
    reason = "Testcode: kurze Namen für Zufall, Tage und Spieler"
)]

mod invarianten_modell;

use invarianten_modell::{nur_was_beide_zuletzt_tragen, startwert, zufallsfolgen};

/// Zufällige Folgen bis drei Runden, erster Startwert.
#[test]
fn abstimmung_fuehrt_nur_aus_was_beide_zuletzt_tragen_zufall_1() -> Result<(), String> {
    nur_was_beide_zuletzt_tragen(&zufallsfolgen(startwert(0)?))
}

/// Zufällige Folgen bis drei Runden, zweiter Startwert.
#[test]
fn abstimmung_fuehrt_nur_aus_was_beide_zuletzt_tragen_zufall_2() -> Result<(), String> {
    nur_was_beide_zuletzt_tragen(&zufallsfolgen(startwert(1)?))
}

/// Zufällige Folgen bis drei Runden, dritter Startwert.
#[test]
fn abstimmung_fuehrt_nur_aus_was_beide_zuletzt_tragen_zufall_3() -> Result<(), String> {
    nur_was_beide_zuletzt_tragen(&zufallsfolgen(startwert(2)?))
}
