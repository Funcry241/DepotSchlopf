//! SHA-256 als Hex-Text, fuer Abdruecke von Dateien ausserhalb des Journals (etwa des
//! gesicherten Programmstands im Dienst).

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

/// SHA-256 der Bytes, klein geschrieben, 64 Zeichen.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}
