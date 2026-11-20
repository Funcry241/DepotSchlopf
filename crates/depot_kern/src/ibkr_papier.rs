//! Lokale Kennung für den lesenden Abruf eines IBKR-Papierkontos.

pub const KONTO_DATEI: &str = "ibkr-papierkonto.txt";
/// Präfixe der im IBKR-Papierjournal belegten Abgleiche.
pub const ABGLEICH_OK: &str = "IBKR-Papierabgleich: ohne Abweichung";
pub const ABGLEICH_HALT: &str = "IBKR-Papierabgleich: Halt";

pub fn kontonummer_gueltig(konto: &str) -> bool {
    konto
        .strip_prefix("DUR")
        .or_else(|| konto.strip_prefix("DU"))
        .is_some_and(|nr| {
            !nr.is_empty() && nr.len() <= 20 && nr.bytes().all(|b| b.is_ascii_digit())
        })
}
