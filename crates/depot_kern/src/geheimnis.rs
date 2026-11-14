//! Gemeinsamer Ausgabeschutz. Adressen und Fehlermeldungen bleiben lesbar,
//! Zugangsdaten werden vor jeder Ausgabe durch `***` ersetzt.

/// Maskiert bekannte Geheimnisformen, auch ohne Zugriff auf den Schlüsselspeicher.
pub fn maskiere(text: &str) -> String {
    zeilenweise(text, |zeile| {
        let ausgabe = maskiere_roh(zeile);
        if crate::json::alle_texte(&ausgabe)
            .iter()
            .any(|t| maskiere_roh(t) != *t)
        {
            "***".into()
        } else {
            ausgabe
        }
    })
}

/// Entfernt zusätzlich den bekannten Schlüssel und alle seine Abschnitte ab zwölf Zeichen.
/// Kürzere Schlüssel werden vollständig erkannt. Der Schlüssel wird nicht gespeichert.
pub fn mit_schluessel(text: &str, schluessel: &str) -> String {
    let vorher = maskiere(text);
    zeilenweise(&vorher, |zeile| {
        if zeile.contains('\\')
            && crate::json::alle_texte(zeile)
                .iter()
                .any(|t| enthaelt(t, schluessel))
        {
            return "***".into();
        }
        let mut bereiche = Vec::new();
        for (start, _) in zeile.char_indices() {
            if start > 0
                && zeile
                    .as_bytes()
                    .get(start.saturating_sub(1))
                    .is_some_and(|b| schluesselzeichen(*b))
            {
                continue;
            }
            let ende = token_ende(zeile, start);
            let wort = zeile.get(start..ende).unwrap_or_default();
            if enthaelt(wort, schluessel) {
                bereiche.push((start, ende));
            }
        }
        let ausgabe = ersetzen(zeile, bereiche);
        if crate::json::alle_texte(&ausgabe)
            .iter()
            .any(|t| enthaelt(t, schluessel))
        {
            "***".into()
        } else {
            ausgabe
        }
    })
}

fn enthaelt(text: &str, schluessel: &str) -> bool {
    let laenge = schluessel.len().min(12);
    laenge > 0
        && text.as_bytes().windows(laenge).any(|wort| {
            schluessel
                .as_bytes()
                .windows(laenge)
                .any(|k| wort.eq_ignore_ascii_case(k))
        })
}

fn zeilenweise(text: &str, mut f: impl FnMut(&str) -> String) -> String {
    let mut ausgabe = String::with_capacity(text.len());
    for zeile in text.split_inclusive('\n') {
        let (inhalt, ende) = zeile.strip_suffix("\r\n").map_or_else(
            || zeile.strip_suffix('\n').map_or((zeile, ""), |t| (t, "\n")),
            |t| (t, "\r\n"),
        );
        ausgabe.push_str(&f(inhalt));
        ausgabe.push_str(ende);
    }
    ausgabe
}

fn schluesselzeichen(b: u8) -> bool {
    crate::gemini::api_schluesselzeichen(b)
}

fn token_ende(text: &str, start: usize) -> usize {
    let n = text
        .as_bytes()
        .get(start..)
        .unwrap_or_default()
        .iter()
        .take_while(|b| schluesselzeichen(**b))
        .count();
    start.saturating_add(n)
}

fn passt(text: &str, start: usize, muster: &str) -> bool {
    text.as_bytes()
        .get(start..start.saturating_add(muster.len()))
        .is_some_and(|t| t.eq_ignore_ascii_case(muster.as_bytes()))
}

fn leerraum(text: &str, start: usize) -> usize {
    start.saturating_add(
        text.as_bytes()
            .get(start..)
            .unwrap_or_default()
            .iter()
            .take_while(|b| b.is_ascii_whitespace())
            .count(),
    )
}

fn zuweisung(text: &str, start: usize) -> Option<usize> {
    let ende = if matches!(text.as_bytes().get(start), Some(b'"' | b'\'')) {
        start.saturating_add(1)
    } else {
        start
    };
    let ende = leerraum(text, ende);
    matches!(text.as_bytes().get(ende), Some(b'=' | b':'))
        .then(|| leerraum(text, ende.saturating_add(1)))
}

fn wert(text: &str, start: usize, kopf: bool) -> (usize, usize) {
    let quote = text
        .as_bytes()
        .get(start)
        .copied()
        .filter(|b| matches!(b, b'"' | b'\''));
    let start = if quote.is_some() {
        start.saturating_add(1)
    } else {
        start
    };
    let mut ende = start;
    let mut escape = false;
    for b in text.as_bytes().get(start..).unwrap_or_default() {
        if let Some(q) = quote {
            if !escape && *b == q {
                break;
            }
            escape = !escape && *b == b'\\';
        } else if matches!(
            b,
            b'\r' | b'\n' | b'&' | b';' | b',' | b'"' | b'\'' | b'}' | b']' | b')' | b'<' | b'>'
        ) || (!kopf && b.is_ascii_whitespace())
        {
            break;
        }
        ende = ende.saturating_add(1);
    }
    (start, ende)
}

fn maskiere_roh(text: &str) -> String {
    let mut bereiche = Vec::new();
    for (start, _) in text.char_indices() {
        if start > 0
            && text
                .as_bytes()
                .get(start.saturating_sub(1))
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            continue;
        }
        for name in [
            "apikey",
            "api_key",
            "api-key",
            "token",
            "key",
            "authorization",
        ] {
            if passt(text, start, name)
                && let Some(wertstart) = zuweisung(text, start.saturating_add(name.len()))
            {
                bereiche.push(wert(text, wertstart, name == "authorization"));
            }
        }
        for name in ["Bearer", "Basic"] {
            let nach = start.saturating_add(name.len());
            if passt(text, start, name)
                && text
                    .as_bytes()
                    .get(nach)
                    .is_some_and(u8::is_ascii_whitespace)
            {
                bereiche.push(wert(text, leerraum(text, nach), false));
            }
        }
        if text
            .get(start..)
            .is_some_and(|rest| rest.starts_with("AQ."))
        {
            let ende = token_ende(text, start.saturating_add(3));
            if ende.saturating_sub(start) >= 20 {
                bereiche.push((start, ende));
            }
        }
        if text
            .get(start..)
            .is_some_and(|rest| rest.starts_with("AIza"))
        {
            let ende = token_ende(text, start.saturating_add(4));
            if ende.saturating_sub(start) >= 39 {
                bereiche.push((start, ende));
            }
        }
        if passt(text, start, "sk-") {
            bereiche.push(wert(text, start, false));
        }
    }
    ersetzen(text, bereiche)
}

fn ersetzen(text: &str, mut bereiche: Vec<(usize, usize)>) -> String {
    bereiche.sort_unstable();
    let mut ausgabe = String::with_capacity(text.len());
    let mut gelesen = 0;
    for (start, ende) in bereiche {
        if start >= ende || ende <= gelesen {
            continue;
        }
        if start >= gelesen {
            ausgabe.push_str(text.get(gelesen..start).unwrap_or_default());
            ausgabe.push_str("***");
        }
        gelesen = ende;
    }
    ausgabe.push_str(text.get(gelesen..).unwrap_or_default());
    ausgabe
}
