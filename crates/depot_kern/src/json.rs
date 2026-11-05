//! Ein kleiner JSON-Leser und -Schreiber fuer die Antworten der Quellen und Anbieter (Sammler,
//! Berater). Keine fremde Bibliothek: Gelesen wird mit fester Hoechsttiefe, Zahlen bleiben Text,
//! bis jemand eine ganze Zahl verlangt. Kein Netz hier.

use std::fmt::Write as _;

/// Hoechste Verschachtelung. Tiefer verschachtelte Antworten gelten als ungueltig.
const MAX_TIEFE: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Json {
    Null,
    Bool(bool),
    /// Die Zahl, wie sie im Text steht.
    Zahl(String),
    Text(String),
    Liste(Vec<Json>),
    Objekt(Vec<(String, Json)>),
}

impl Json {
    /// Das Feld eines Objekts; bei mehrfachen Schluesseln das erste.
    pub fn feld(&self, name: &str) -> Option<&Json> {
        match self {
            Json::Objekt(felder) => felder.iter().find(|(k, _)| k == name).map(|(_, w)| w),
            _ => None,
        }
    }

    /// Kommt der Schluessel mehr als einmal vor?
    pub fn doppelte_felder(&self) -> bool {
        match self {
            Json::Objekt(felder) => felder
                .iter()
                .enumerate()
                .any(|(i, (k, _))| felder.iter().skip(i.saturating_add(1)).any(|(x, _)| x == k)),
            _ => false,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Json::Text(t) => Some(t),
            _ => None,
        }
    }

    pub fn liste(&self) -> Option<&[Json]> {
        match self {
            Json::Liste(l) => Some(l),
            _ => None,
        }
    }

    pub fn wahr(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Eine ganze Zahl ohne Bruch und Exponent.
    pub fn ganz(&self) -> Option<i64> {
        match self {
            Json::Zahl(z) => z.parse().ok(),
            _ => None,
        }
    }

    /// Eine nicht negative ganze Zahl.
    pub fn anzahl(&self) -> Option<u64> {
        match self {
            Json::Zahl(z) => z.parse().ok(),
            _ => None,
        }
    }
}

/// Ein Text als JSON-Zeichenkette, mit Anfuehrungszeichen.
pub fn zitiere(text: &str) -> String {
    let mut aus = String::with_capacity(text.len().saturating_add(2));
    aus.push('"');
    for c in text.chars() {
        match c {
            '"' => aus.push_str("\\\""),
            '\\' => aus.push_str("\\\\"),
            '\n' => aus.push_str("\\n"),
            '\r' => aus.push_str("\\r"),
            '\t' => aus.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(aus, "\\u{:04x}", u32::from(c));
            }
            c => aus.push(c),
        }
    }
    aus.push('"');
    aus
}

struct Leser<'a> {
    b: &'a [u8],
    pos: usize,
}

impl Leser<'_> {
    fn sieh(&self) -> Option<u8> {
        self.b.get(self.pos).copied()
    }

    fn weiter(&mut self) {
        self.pos = self.pos.saturating_add(1);
    }

    fn nimm(&mut self) -> Option<u8> {
        let c = self.sieh()?;
        self.weiter();
        Some(c)
    }

    fn leer(&mut self) {
        while matches!(self.sieh(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.weiter();
        }
    }

    fn erwarte(&mut self, wort: &[u8]) -> Result<(), String> {
        for w in wort {
            if self.nimm() != Some(*w) {
                return Err(format!("Zeichen {} unerwartet", self.pos));
            }
        }
        Ok(())
    }

    fn wert(&mut self, tiefe: usize) -> Result<Json, String> {
        if tiefe > MAX_TIEFE {
            return Err("zu tief verschachtelt".into());
        }
        self.leer();
        match self.sieh() {
            Some(b'{') => self.objekt(tiefe),
            Some(b'[') => self.liste(tiefe),
            Some(b'"') => self.zeichenkette().map(Json::Text),
            Some(b't') => self.erwarte(b"true").map(|()| Json::Bool(true)),
            Some(b'f') => self.erwarte(b"false").map(|()| Json::Bool(false)),
            Some(b'n') => self.erwarte(b"null").map(|()| Json::Null),
            Some(b'-' | b'0'..=b'9') => self.zahl(),
            _ => Err(format!("Zeichen {} unerwartet", self.pos)),
        }
    }

    fn objekt(&mut self, tiefe: usize) -> Result<Json, String> {
        self.weiter();
        let mut felder = Vec::new();
        self.leer();
        if self.sieh() == Some(b'}') {
            self.weiter();
            return Ok(Json::Objekt(felder));
        }
        loop {
            self.leer();
            if self.sieh() != Some(b'"') {
                return Err(format!("Schluessel erwartet bei {}", self.pos));
            }
            let k = self.zeichenkette()?;
            self.leer();
            self.erwarte(b":")?;
            let w = self.wert(tiefe.saturating_add(1))?;
            felder.push((k, w));
            self.leer();
            match self.nimm() {
                Some(b',') => {}
                Some(b'}') => return Ok(Json::Objekt(felder)),
                _ => return Err(format!("Objekt offen bei {}", self.pos)),
            }
        }
    }

    fn liste(&mut self, tiefe: usize) -> Result<Json, String> {
        self.weiter();
        let mut werte = Vec::new();
        self.leer();
        if self.sieh() == Some(b']') {
            self.weiter();
            return Ok(Json::Liste(werte));
        }
        loop {
            werte.push(self.wert(tiefe.saturating_add(1))?);
            self.leer();
            match self.nimm() {
                Some(b',') => {}
                Some(b']') => return Ok(Json::Liste(werte)),
                _ => return Err(format!("Liste offen bei {}", self.pos)),
            }
        }
    }

    fn zahl(&mut self) -> Result<Json, String> {
        let start = self.pos;
        while matches!(
            self.sieh(),
            Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
        ) {
            self.weiter();
        }
        let text = self
            .b
            .get(start..self.pos)
            .and_then(|t| std::str::from_utf8(t).ok())
            .ok_or("Zahl unlesbar")?;
        let gueltig = {
            let ohne = text.strip_prefix('-').unwrap_or(text);
            let mut teile = ohne.splitn(2, ['e', 'E']);
            let mantisse = teile.next().unwrap_or("");
            let exponent = teile.next();
            let mut m = mantisse.splitn(2, '.');
            let ganz = m.next().unwrap_or("");
            let bruch = m.next();
            !ganz.is_empty()
                && ganz.bytes().all(|c| c.is_ascii_digit())
                && (ganz == "0" || !ganz.starts_with('0'))
                && bruch.is_none_or(|b| !b.is_empty() && b.bytes().all(|c| c.is_ascii_digit()))
                && exponent.is_none_or(|e| {
                    let e = e.strip_prefix(['+', '-']).unwrap_or(e);
                    !e.is_empty() && e.bytes().all(|c| c.is_ascii_digit())
                })
        };
        if !gueltig {
            return Err(format!("Zahl {text:?} ungueltig"));
        }
        Ok(Json::Zahl(text.to_string()))
    }

    fn vier_hex(&mut self) -> Result<u32, String> {
        let mut wert = 0_u32;
        for _ in 0..4 {
            let c = self.nimm().ok_or("Escape offen")?;
            let d = char::from(c).to_digit(16).ok_or("Escape ungueltig")?;
            wert = wert
                .checked_mul(16)
                .and_then(|w| w.checked_add(d))
                .ok_or("Escape")?;
        }
        Ok(wert)
    }

    fn zeichenkette(&mut self) -> Result<String, String> {
        self.weiter();
        let mut roh: Vec<u8> = Vec::new();
        loop {
            let c = self.nimm().ok_or("Zeichenkette offen")?;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = self.nimm().ok_or("Escape offen")?;
                    let zeichen = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hoch = self.vier_hex()?;
                            let code = if (0xD800..0xDC00).contains(&hoch) {
                                self.erwarte(b"\\u")?;
                                let tief = self.vier_hex()?;
                                if !(0xDC00..0xE000).contains(&tief) {
                                    return Err("Ersatzpaar ungueltig".into());
                                }
                                hoch.checked_sub(0xD800)
                                    .and_then(|h| h.checked_mul(0x400))
                                    .and_then(|h| h.checked_add(tief.checked_sub(0xDC00)?))
                                    .and_then(|h| h.checked_add(0x1_0000))
                                    .ok_or("Ersatzpaar")?
                            } else {
                                hoch
                            };
                            char::from_u32(code).ok_or("Zeichen ungueltig")?
                        }
                        _ => return Err("Escape ungueltig".into()),
                    };
                    let mut puffer = [0_u8; 4];
                    roh.extend_from_slice(zeichen.encode_utf8(&mut puffer).as_bytes());
                }
                c if c < 0x20 => return Err("Steuerzeichen in Zeichenkette".into()),
                c => roh.push(c),
            }
        }
        String::from_utf8(roh).map_err(|_| "kein UTF-8".into())
    }
}

/// Liest genau einen JSON-Wert; danach darf nur Leerraum folgen.
///
/// # Errors
/// Kein gueltiges JSON.
pub fn lies(text: &str) -> Result<Json, String> {
    let mut l = Leser {
        b: text.as_bytes(),
        pos: 0,
    };
    let w = l.wert(0)?;
    l.leer();
    if l.sieh().is_some() {
        return Err(format!("Text nach dem Wert bei {}", l.pos));
    }
    Ok(w)
}

/// Liest das erste JSON-Objekt in einem Text, etwa einer Modellantwort mit Einleitung oder
/// Codeblock. Danach darf beliebiger Text folgen.
///
/// # Errors
/// Kein Objekt oder kein gueltiges JSON ab der ersten geschweiften Klammer.
pub fn erstes_objekt(text: &str) -> Result<Json, String> {
    let start = text.find('{').ok_or("kein JSON-Objekt in der Antwort")?;
    let rest = text.get(start..).ok_or("kein JSON-Objekt in der Antwort")?;
    let mut l = Leser {
        b: rest.as_bytes(),
        pos: 0,
    };
    l.wert(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liest_und_schreibt() {
        let w =
            lies(r#"{"a": [1, -2, 3.5e-1], "b": "x\"\u00e4\ud83d\ude00", "c": null, "d": true}"#);
        let Ok(w) = w else {
            unreachable!("{w:?}");
        };
        assert_eq!(w.feld("b").and_then(Json::text), Some("x\"ä😀"));
        assert_eq!(
            w.feld("a").and_then(Json::liste).map(<[Json]>::len),
            Some(3)
        );
        assert_eq!(w.feld("d").and_then(Json::wahr), Some(true));
        assert_eq!(zitiere("a\"b\\\n\u{1}"), "\"a\\\"b\\\\\\n\\u0001\"");
    }

    #[test]
    fn lehnt_ungueltiges_ab() {
        for t in [
            "{",
            "[1,]",
            "{\"a\" 1}",
            "01",
            "1.",
            "\"\u{1}\"",
            "nul",
            "{} x",
            "\"\\ud800\"",
        ] {
            assert!(lies(t).is_err(), "{t}");
        }
        let tief = "[".repeat(100);
        assert!(lies(&tief).is_err());
    }

    #[test]
    fn erstes_objekt_im_text() {
        let w = erstes_objekt("Hier:\n```json\n{\"x\": 2}\n```\nEnde");
        assert_eq!(
            w.ok().and_then(|w| w.feld("x").and_then(Json::ganz)),
            Some(2)
        );
    }
}
