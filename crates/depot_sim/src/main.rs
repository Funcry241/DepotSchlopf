//! Lesender Einstieg für den historischen Rücktest.

#![forbid(
    unsafe_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::float_arithmetic,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let werte = match depot_kern::liste::beobachtungsliste() {
        Ok(werte) => werte,
        Err(grund) => {
            eprintln!("[Rücktest] {grund}");
            return ExitCode::FAILURE;
        }
    };
    let universum = depot_kern::liste::als_universum(&werte);
    let grenzen = depot_kern::Grenzen::standard();
    match depot_journal::Journal::pruefe_lesend(Path::new("daten/journal.txt"), |stand| {
        depot_sim::ruecktest::rechne(stand, &grenzen, &universum)
    }) {
        Ok(aus) => {
            depot_sim::ruecktest::drucke(&aus);
            ExitCode::SUCCESS
        }
        Err(grund) => {
            eprintln!("[Rücktest] Journal nicht lesbar: {grund:?}");
            ExitCode::FAILURE
        }
    }
}
