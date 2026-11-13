//! Zerlegt Rust-Quelltext in Token (R26: Pruefen heisst verstehen).
//!
//! Kommentare, Text-, Zeichen- und Zahlliterale werden erkannt und uebersprungen; was in ihnen
//! steht, ist kein Code. Bezeichner und Satzzeichen bleiben uebrig. Ein nicht abgeschlossener
//! Kommentar oder Text ist ein Fehler, kein Ende der Datei.
//!
//! Grenze: Der Lexer kennt die Rust-Syntax, nicht ihre Bedeutung. Makros werden nicht
//! ausgewertet; ihre Token werden wie gewoehnlicher Code geprueft.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    /// Bezeichner oder Schluesselwort; `r#name` wird zu `name`.
    Wort(String),
    /// `::` oder ein einzelnes Satzzeichen.
    Zeichen(&'static str),
    /// Text-, Zeichen- oder Zahlliteral (Inhalt verworfen).
    Literal,
    Textliteral,
    /// Lebensdauer oder Sprungmarke (`'a`).
    Lebensdauer,
}

impl Token {
    pub fn ist_wort(&self, w: &str) -> bool {
        matches!(self, Token::Wort(x) if x == w)
    }

    pub fn ist(&self, z: &str) -> bool {
        matches!(self, Token::Zeichen(x) if *x == z)
    }
}

const ZEICHEN: [&str; 30] = [
    "#", "!", "[", "]", "(", ")", "{", "}", ",", ";", ":", "*", "=", "<", ">", "&", "|", "+", "-",
    "/", "%", "^", ".", "@", "?", "$", "~", "\\", "'", "\"",
];

struct Leser {
    z: Vec<char>,
    i: usize,
    texte: Option<Vec<String>>,
}

impl Leser {
    fn an(&self, versatz: usize) -> Option<char> {
        self.i
            .checked_add(versatz)
            .and_then(|j| self.z.get(j).copied())
    }

    fn weiter(&mut self, n: usize) {
        self.i = self.i.saturating_add(n);
    }

    fn zeile(&self) -> usize {
        self.z
            .iter()
            .take(self.i)
            .filter(|c| **c == '\n')
            .count()
            .saturating_add(1)
    }

    fn fehler(&self, was: &str) -> String {
        format!("Zeile {}: {was}", self.zeile())
    }

    fn merke_text(&mut self, start: usize) {
        if let Some(texte) = &mut self.texte {
            texte.push(
                self.z
                    .get(start..self.i)
                    .unwrap_or_default()
                    .iter()
                    .collect(),
            );
        }
    }

    /// `"…"` mit Escapes; `self.i` steht auf dem oeffnenden `"`.
    fn text(&mut self) -> Result<(), String> {
        self.weiter(1);
        let start = self.i;
        loop {
            match self.an(0) {
                None => return Err(self.fehler("Text nicht abgeschlossen")),
                Some('\\') => self.weiter(2),
                Some('"') => {
                    self.merke_text(start);
                    self.weiter(1);
                    return Ok(());
                }
                Some(_) => self.weiter(1),
            }
        }
    }

    /// `r#*"…"#*`; `self.i` steht auf dem ersten `#` oder `"` nach dem Praefix.
    fn roher_text(&mut self) -> Result<(), String> {
        let mut rauten = 0_usize;
        while self.an(0) == Some('#') {
            rauten = rauten.saturating_add(1);
            self.weiter(1);
        }
        if self.an(0) != Some('"') {
            return Err(self.fehler("roher Text ohne Anführungszeichen"));
        }
        self.weiter(1);
        let start = self.i;
        loop {
            match self.an(0) {
                None => return Err(self.fehler("roher Text nicht abgeschlossen")),
                Some('"') if (1..=rauten).all(|k| self.an(k) == Some('#')) => {
                    self.merke_text(start);
                    self.weiter(rauten.saturating_add(1));
                    return Ok(());
                }
                Some(_) => self.weiter(1),
            }
        }
    }

    /// `'x'`, `'\n'` oder Lebensdauer `'a`; `self.i` steht auf `'`.
    fn zeichen_oder_lebensdauer(&mut self) -> Result<Token, String> {
        if self.an(1) == Some('\\') {
            self.weiter(2);
            loop {
                match self.an(0) {
                    None | Some('\n') => return Err(self.fehler("Zeichen nicht abgeschlossen")),
                    Some('\'') => {
                        self.weiter(1);
                        return Ok(Token::Literal);
                    }
                    Some(_) => self.weiter(1),
                }
            }
        }
        if self.an(2) == Some('\'') {
            self.weiter(3);
            return Ok(Token::Literal);
        }
        self.weiter(1);
        while self.an(0).is_some_and(|c| c.is_alphanumeric() || c == '_') {
            self.weiter(1);
        }
        Ok(Token::Lebensdauer)
    }

    fn block_kommentar(&mut self) -> Result<(), String> {
        let mut tiefe = 0_usize;
        loop {
            match (self.an(0), self.an(1)) {
                (None, _) => return Err(self.fehler("Kommentar nicht abgeschlossen")),
                (Some('/'), Some('*')) => {
                    tiefe = tiefe.saturating_add(1);
                    self.weiter(2);
                }
                (Some('*'), Some('/')) => {
                    tiefe = tiefe.saturating_sub(1);
                    self.weiter(2);
                    if tiefe == 0 {
                        return Ok(());
                    }
                }
                _ => self.weiter(1),
            }
        }
    }

    fn wort(&mut self) -> String {
        let mut w = String::new();
        while let Some(c) = self.an(0).filter(|c| c.is_alphanumeric() || *c == '_') {
            w.push(c);
            self.weiter(1);
        }
        w
    }
}

/// Zerlegt `quelle` in Token.
///
/// # Errors
/// Nicht abgeschlossene Kommentare, Texte oder Zeichen.
pub fn zerlege(quelle: &str) -> Result<Vec<Token>, String> {
    zerlege_mit_text(quelle, false).map(|(token, _)| token)
}

/// Zeichenketten mit ursprünglichen Escapes, Kommentare und Zeichenliterale ausgeschlossen.
pub fn zeichenketten(quelle: &str) -> Result<Vec<String>, String> {
    zerlege_mit_text(quelle, true).map(|(_, texte)| texte)
}

fn zerlege_mit_text(quelle: &str, sammeln: bool) -> Result<(Vec<Token>, Vec<String>), String> {
    let mut l = Leser {
        z: quelle.chars().collect(),
        i: 0,
        texte: sammeln.then(Vec::new),
    };
    let mut aus = Vec::new();
    while let Some(c) = l.an(0) {
        let n = l.an(1);
        match c {
            _ if c.is_whitespace() => l.weiter(1),
            '/' if n == Some('/') => {
                while l.an(0).is_some_and(|c| c != '\n') {
                    l.weiter(1);
                }
            }
            '/' if n == Some('*') => l.block_kommentar()?,
            '"' => {
                l.text()?;
                aus.push(Token::Textliteral);
            }
            '\'' => aus.push(l.zeichen_oder_lebensdauer()?),
            _ if c.is_ascii_digit() => {
                while let Some(d) = l.an(0) {
                    let exponent = (d == '+' || d == '-')
                        && l.i
                            .checked_sub(1)
                            .and_then(|j| l.z.get(j))
                            .is_some_and(|e| matches!(e, 'e' | 'E'));
                    let punkt = d == '.' && l.an(1).is_some_and(|x| x.is_ascii_digit());
                    if d.is_alphanumeric() || d == '_' || punkt || exponent {
                        l.weiter(1);
                    } else {
                        break;
                    }
                }
                aus.push(Token::Literal);
            }
            _ if c.is_alphabetic() || c == '_' => {
                let start = l.i;
                let w = l.wort();
                let praefix = matches!(w.as_str(), "r" | "br" | "cr");
                let text_praefix = matches!(w.as_str(), "b" | "c");
                match l.an(0) {
                    Some('"' | '#')
                        if praefix
                            && (l.an(0) == Some('"')
                                || l.an(1) == Some('"')
                                || l.an(1) == Some('#')) =>
                    {
                        l.roher_text()?;
                        aus.push(if w == "r" {
                            Token::Textliteral
                        } else {
                            Token::Literal
                        });
                    }
                    Some('#') if w == "r" => {
                        l.weiter(1);
                        aus.push(Token::Wort(l.wort()));
                    }
                    Some('"') if text_praefix => {
                        l.text()?;
                        aus.push(Token::Literal);
                    }
                    Some('\'') if w == "b" => {
                        aus.push(l.zeichen_oder_lebensdauer()?);
                    }
                    _ => {
                        if l.i == start {
                            return Err(l.fehler("unerwartetes Zeichen"));
                        }
                        aus.push(Token::Wort(w));
                    }
                }
            }
            ':' if n == Some(':') => {
                l.weiter(2);
                aus.push(Token::Zeichen("::"));
            }
            _ => {
                let Some(z) = ZEICHEN.iter().find(|z| z.starts_with(c)) else {
                    return Err(l.fehler(&format!("unbekanntes Zeichen {c:?}")));
                };
                l.weiter(1);
                aus.push(Token::Zeichen(z));
            }
        }
    }
    Ok((aus, l.texte.unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn woerter(q: &str) -> Vec<String> {
        zerlege(q)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|t| match t {
                Token::Wort(w) => Some(w),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn literale_und_kommentare_sind_kein_code() {
        let q = r##"
            // use std::fs;
            /* use std::env; /* verschachtelt */ */
            let a = "use std::net;";
            let b = r#"std::process "innen" "#;
            let c = b"std"; let d = 'x'; let e = '\''; let f: &'static str = "";
            let g = 1.5e-3_f64; let r#type = 0;
        "##;
        let w = woerter(q);
        assert!(!w.iter().any(|x| x == "std"), "{w:?}");
        assert!(w.iter().any(|x| x == "type"));
        assert!(!w.iter().any(|x| x == "static"));
    }

    #[test]
    fn offene_texte_sind_fehler() {
        assert!(zerlege("let a = \"offen").is_err());
        assert!(zerlege("/* offen").is_err());
        assert!(zerlege("let a = r#\"offen\"").is_err());
    }

    #[test]
    fn pfadtrenner() {
        let t = zerlege("std::env").unwrap_or_default();
        assert_eq!(
            t,
            vec![
                Token::Wort("std".into()),
                Token::Zeichen("::"),
                Token::Wort("env".into())
            ]
        );
    }
}
