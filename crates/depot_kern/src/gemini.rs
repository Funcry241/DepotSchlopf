//! Gemeinsame Angaben der optionalen Gemini-Zuarbeit.
// Wolfgang bestätigte die Modellbezeichnung am 10.11.2026.
pub const MODELL: &str = "gemini-3.8-flash";
pub const DIENST: &str = "gemini";
pub const ADRESSE: &str = "DepotSchlopf/gemini";
pub const TAGESGRENZE: usize = 5;
pub const RUECKLAGE: usize = 2;
pub const WARTET_AUF_UPDATE: &str = "Das Update wartet auf Bestätigung.";
pub const KEIN_HANDELSTAG: &str = "Kein Zielhandelstag verfügbar.";
pub const KEINE_NACHRICHTEN: &str = "Keine belegten Nachrichten.";
pub const JOURNAL_UNVERFUEGBAR: &str = "Das Journal ist gerade nicht lesbar.";

/// Prüft das lokale Format eines Gemini-Schlüssels für den HTTP-Header.
///
/// # Errors
/// Der Schlüssel hat eine unzulässige Länge oder enthält unzulässige Zeichen.
pub fn pruefe_schluessel(text: &str) -> Result<&str, &'static str> {
    let text = text.trim();
    if !(20..=256).contains(&text.len()) {
        return Err("20 bis 256 Zeichen erwartet");
    }
    let inhalt = text.strip_prefix("AQ.").unwrap_or(text);
    if !inhalt.bytes().all(api_schluesselzeichen) {
        return Err(
            "Buchstaben, Ziffern, _ und - erwartet. Bei einem AQ-Schlüssel ist der Punkt nach AQ erlaubt.",
        );
    }
    Ok(text)
}

pub(crate) fn api_schluesselzeichen(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')
}
