//! Ein kleiner, strenger TOML-Leser fuer Cargo-Manifeste (R26: Manifeste semantisch pruefen).
//!
//! Unterstuetzt Tabellen, Tabellen-Listen, gepunktete und quotierte Schluessel, alle vier
//! Textformen, Listen, Inline-Tabellen, Wahrheitswerte sowie Zahlen und Zeitangaben (diese als
//! Rohtext). Was er nicht versteht, ist ein Fehler. Doppelte Schluessel sind ein Fehler.

use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wert {
    Text(String),
    /// Zahl oder Zeitangabe als Rohtext.
    Roh(String),
    Wahr(bool),
    Liste(Vec<Wert>),
    Tabelle(BTreeMap<String, Wert>),
}

impl Wert {
    pub fn feld(&self, schluessel: &str) -> Option<&Wert> {
        match self {
            Wert::Tabelle(t) => t.get(schluessel),
            _ => None,
        }
    }

    /// Feld ueber einen gepunkteten Pfad.
    pub fn pfad(&self, pfad: &[&str]) -> Option<&Wert> {
        pfad.iter().try_fold(self, |w, s| w.feld(s))
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Wert::Text(t) => Some(t),
            _ => None,
        }
    }

    pub fn tabelle(&self) -> Option<&BTreeMap<String, Wert>> {
        match self {
            Wert::Tabelle(t) => Some(t),
            _ => None,
        }
    }

    /// Eindeutige, sortierte Textform (fuer Hashes und Festschreibung).
    pub fn kanonisch(&self) -> String {
        let mut s = String::new();
        self.schreibe(&mut s);
        s
    }

    fn schreibe(&self, s: &mut String) {
        match self {
            Wert::Text(t) => {
                let _ = write!(s, "{t:?}");
            }
            Wert::Roh(r) => s.push_str(r),
            Wert::Wahr(b) => s.push_str(if *b { "true" } else { "false" }),
            Wert::Liste(l) => {
                s.push('[');
                for (i, w) in l.iter().enumerate() {
                    if i > 0 {
                        s.push(',');
                    }
                    w.schreibe(s);
                }
                s.push(']');
            }
            Wert::Tabelle(t) => {
                s.push('{');
                for (i, (k, w)) in t.iter().enumerate() {
                    if i > 0 {
                        s.push(',');
                    }
                    let _ = write!(s, "{k:?}=");
                    w.schreibe(s);
                }
                s.push('}');
            }
        }
    }
}

struct Leser {
    z: Vec<char>,
    i: usize,
}

impl Leser {
    fn an(&self, v: usize) -> Option<char> {
        self.i.checked_add(v).and_then(|j| self.z.get(j).copied())
    }

    fn weiter(&mut self, n: usize) {
        self.i = self.i.saturating_add(n);
    }

    fn fehler(&self, was: &str) -> String {
        let zeile = self
            .z
            .iter()
            .take(self.i)
            .filter(|c| **c == '\n')
            .count()
            .saturating_add(1);
        format!("TOML Zeile {zeile}: {was}")
    }

    fn folgt(&self, text: &str) -> bool {
        text.chars().enumerate().all(|(k, c)| self.an(k) == Some(c))
    }

    /// Leerzeichen und Tabs (nicht Zeilenenden).
    fn leer(&mut self) {
        while matches!(self.an(0), Some(' ' | '\t')) {
            self.weiter(1);
        }
    }

    /// Leerraum, Zeilenenden und Kommentare.
    fn leer_mit_zeilen(&mut self) {
        loop {
            match self.an(0) {
                Some(' ' | '\t' | '\n' | '\r') => self.weiter(1),
                Some('#') => self.kommentar(),
                _ => return,
            }
        }
    }

    fn kommentar(&mut self) {
        while self.an(0).is_some_and(|c| c != '\n') {
            self.weiter(1);
        }
    }

    /// Nach einem Wert oder Kopf: nur noch Leerraum und Kommentar bis Zeilenende.
    fn zeilenende(&mut self) -> Result<(), String> {
        self.leer();
        if self.an(0) == Some('#') {
            self.kommentar();
        }
        match self.an(0) {
            None => Ok(()),
            Some('\n') => {
                self.weiter(1);
                Ok(())
            }
            Some('\r') if self.an(1) == Some('\n') => {
                self.weiter(2);
                Ok(())
            }
            Some(c) => Err(self.fehler(&format!("unerwartet {c:?} am Zeilenende"))),
        }
    }

    fn schluessel_teil(&mut self) -> Result<String, String> {
        self.leer();
        match self.an(0) {
            Some('"') => self.basistext(),
            Some('\'') => self.literaltext(),
            _ => {
                let mut s = String::new();
                while let Some(c) = self
                    .an(0)
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                {
                    s.push(c);
                    self.weiter(1);
                }
                if s.is_empty() {
                    return Err(self.fehler("Schlüssel erwartet"));
                }
                Ok(s)
            }
        }
    }

    fn schluessel(&mut self) -> Result<Vec<String>, String> {
        let mut teile = vec![self.schluessel_teil()?];
        loop {
            self.leer();
            if self.an(0) == Some('.') {
                self.weiter(1);
                teile.push(self.schluessel_teil()?);
            } else {
                return Ok(teile);
            }
        }
    }

    fn escape(&mut self, s: &mut String) -> Result<(), String> {
        let c = self.an(0).ok_or_else(|| self.fehler("Escape am Ende"))?;
        self.weiter(1);
        let einfach = match c {
            'n' => Some('\n'),
            't' => Some('\t'),
            'r' => Some('\r'),
            'b' => Some('\u{8}'),
            'f' => Some('\u{c}'),
            'e' => Some('\u{1b}'),
            '"' => Some('"'),
            '\\' => Some('\\'),
            _ => None,
        };
        if let Some(e) = einfach {
            s.push(e);
            return Ok(());
        }
        let stellen = match c {
            'u' => 4,
            'U' => 8,
            'x' => 2,
            _ => return Err(self.fehler(&format!("unbekanntes Escape \\{c}"))),
        };
        let mut hex = String::new();
        for _ in 0..stellen {
            hex.push(self.an(0).ok_or_else(|| self.fehler("Escape zu kurz"))?);
            self.weiter(1);
        }
        let zahl = u32::from_str_radix(&hex, 16).map_err(|_| self.fehler("Escape ungültig"))?;
        s.push(char::from_u32(zahl).ok_or_else(|| self.fehler("Escape ungültig"))?);
        Ok(())
    }

    fn basistext(&mut self) -> Result<String, String> {
        if self.folgt("\"\"\"") {
            return self.mehrzeilig_basis();
        }
        self.weiter(1);
        let mut s = String::new();
        loop {
            match self.an(0) {
                None | Some('\n') => return Err(self.fehler("Text nicht abgeschlossen")),
                Some('"') => {
                    self.weiter(1);
                    return Ok(s);
                }
                Some('\\') => {
                    self.weiter(1);
                    self.escape(&mut s)?;
                }
                Some(c) => {
                    s.push(c);
                    self.weiter(1);
                }
            }
        }
    }

    fn mehrzeilig_basis(&mut self) -> Result<String, String> {
        self.weiter(3);
        if self.an(0) == Some('\n') {
            self.weiter(1);
        } else if self.folgt("\r\n") {
            self.weiter(2);
        }
        let mut s = String::new();
        loop {
            if self.folgt("\"\"\"") {
                // Bis zu zwei weitere Anfuehrungszeichen gehoeren zum Inhalt.
                let mut extra = 0_usize;
                while extra < 2 && self.an(extra.saturating_add(3)) == Some('"') {
                    extra = extra.saturating_add(1);
                }
                for _ in 0..extra {
                    s.push('"');
                }
                self.weiter(extra.saturating_add(3));
                return Ok(s);
            }
            match self.an(0) {
                None => return Err(self.fehler("mehrzeiliger Text nicht abgeschlossen")),
                Some('\\') => {
                    self.weiter(1);
                    if matches!(self.an(0), Some(' ' | '\t' | '\n' | '\r')) {
                        // Zeilenfortsetzung: Leerraum bis zum naechsten Zeichen faellt weg.
                        while matches!(self.an(0), Some(' ' | '\t' | '\n' | '\r')) {
                            self.weiter(1);
                        }
                    } else {
                        self.escape(&mut s)?;
                    }
                }
                Some(c) => {
                    s.push(c);
                    self.weiter(1);
                }
            }
        }
    }

    fn literaltext(&mut self) -> Result<String, String> {
        if self.folgt("'''") {
            self.weiter(3);
            if self.an(0) == Some('\n') {
                self.weiter(1);
            } else if self.folgt("\r\n") {
                self.weiter(2);
            }
            let mut s = String::new();
            loop {
                if self.folgt("'''") {
                    let mut extra = 0_usize;
                    while extra < 2 && self.an(extra.saturating_add(3)) == Some('\'') {
                        extra = extra.saturating_add(1);
                    }
                    for _ in 0..extra {
                        s.push('\'');
                    }
                    self.weiter(extra.saturating_add(3));
                    return Ok(s);
                }
                let c = self
                    .an(0)
                    .ok_or_else(|| self.fehler("mehrzeiliger Text nicht abgeschlossen"))?;
                s.push(c);
                self.weiter(1);
            }
        }
        self.weiter(1);
        let mut s = String::new();
        loop {
            match self.an(0) {
                None | Some('\n') => return Err(self.fehler("Text nicht abgeschlossen")),
                Some('\'') => {
                    self.weiter(1);
                    return Ok(s);
                }
                Some(c) => {
                    s.push(c);
                    self.weiter(1);
                }
            }
        }
    }

    fn wert(&mut self) -> Result<Wert, String> {
        self.leer();
        match self.an(0) {
            Some('"') => Ok(Wert::Text(self.basistext()?)),
            Some('\'') => Ok(Wert::Text(self.literaltext()?)),
            Some('[') => {
                self.weiter(1);
                let mut liste = Vec::new();
                loop {
                    self.leer_mit_zeilen();
                    if self.an(0) == Some(']') {
                        self.weiter(1);
                        return Ok(Wert::Liste(liste));
                    }
                    liste.push(self.wert()?);
                    self.leer_mit_zeilen();
                    match self.an(0) {
                        Some(',') => self.weiter(1),
                        Some(']') => {}
                        _ => return Err(self.fehler("Liste nicht lesbar")),
                    }
                }
            }
            Some('{') => {
                self.weiter(1);
                let mut t = BTreeMap::new();
                self.leer();
                if self.an(0) == Some('}') {
                    self.weiter(1);
                    return Ok(Wert::Tabelle(t));
                }
                loop {
                    let k = self.schluessel()?;
                    self.leer();
                    if self.an(0) != Some('=') {
                        return Err(self.fehler("= erwartet"));
                    }
                    self.weiter(1);
                    let w = self.wert()?;
                    setze(&mut t, &k, w).map_err(|e| self.fehler(&e))?;
                    self.leer();
                    match self.an(0) {
                        Some(',') => self.weiter(1),
                        Some('}') => {
                            self.weiter(1);
                            return Ok(Wert::Tabelle(t));
                        }
                        _ => return Err(self.fehler("Inline-Tabelle nicht lesbar")),
                    }
                }
            }
            _ if self.folgt("true") => {
                self.weiter(4);
                Ok(Wert::Wahr(true))
            }
            _ if self.folgt("false") => {
                self.weiter(5);
                Ok(Wert::Wahr(false))
            }
            _ => {
                let mut r = String::new();
                while let Some(c) = self.an(0).filter(|c| {
                    c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '_' | '.' | ':')
                }) {
                    r.push(c);
                    self.weiter(1);
                }
                // Zeitangaben duerfen ein Leerzeichen zwischen Datum und Uhrzeit haben.
                if self.an(0) == Some(' ')
                    && self.an(1).is_some_and(|c| c.is_ascii_digit())
                    && r.contains('-')
                {
                    r.push(' ');
                    self.weiter(1);
                    while let Some(c) = self
                        .an(0)
                        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.' | ':'))
                    {
                        r.push(c);
                        self.weiter(1);
                    }
                }
                if r.is_empty()
                    || !r.starts_with(|c: char| {
                        c.is_ascii_digit() || matches!(c, '+' | '-' | 'i' | 'n')
                    })
                {
                    return Err(self.fehler("Wert nicht lesbar"));
                }
                Ok(Wert::Roh(r))
            }
        }
    }
}

/// Setzt einen gepunkteten Schluessel; Zwischentabellen entstehen bei Bedarf.
fn setze(t: &mut BTreeMap<String, Wert>, k: &[String], w: Wert) -> Result<(), String> {
    let Some((letzter, weg)) = k.split_last() else {
        return Err("leerer Schlüssel".into());
    };
    let mut ziel = t;
    for s in weg {
        let eintrag = ziel
            .entry(s.clone())
            .or_insert_with(|| Wert::Tabelle(BTreeMap::new()));
        ziel = match eintrag {
            Wert::Tabelle(x) => x,
            _ => return Err(format!("Schlüssel {s} ist schon ein Wert")),
        };
    }
    if ziel.contains_key(letzter) {
        return Err(format!("Schlüssel {} doppelt", k.join(".")));
    }
    ziel.insert(letzter.clone(), w);
    Ok(())
}

/// Tabelle zum Kopf-Pfad; eine Tabellen-Liste liefert ihr letztes Element.
fn tabelle_bei<'a>(
    wurzel: &'a mut BTreeMap<String, Wert>,
    pfad: &[String],
) -> Result<&'a mut BTreeMap<String, Wert>, String> {
    let mut ziel = wurzel;
    for s in pfad {
        let eintrag = ziel
            .entry(s.clone())
            .or_insert_with(|| Wert::Tabelle(BTreeMap::new()));
        ziel = match eintrag {
            Wert::Tabelle(x) => x,
            Wert::Liste(l) => match l.last_mut() {
                Some(Wert::Tabelle(x)) => x,
                _ => return Err(format!("{s} ist keine Tabelle")),
            },
            _ => return Err(format!("{s} ist schon ein Wert")),
        };
    }
    Ok(ziel)
}

/// Liest ein TOML-Dokument.
///
/// # Errors
/// Jede Stelle, die der Leser nicht versteht, und doppelte Schluessel.
pub fn lies(text: &str) -> Result<Wert, String> {
    let mut l = Leser {
        z: text.chars().collect(),
        i: 0,
    };
    let mut wurzel: BTreeMap<String, Wert> = BTreeMap::new();
    let mut kopf: Vec<String> = Vec::new();
    let mut gesehen: Vec<Vec<String>> = Vec::new();
    loop {
        l.leer_mit_zeilen();
        let Some(c) = l.an(0) else { break };
        if c == '[' {
            let liste = l.an(1) == Some('[');
            l.weiter(if liste { 2 } else { 1 });
            let k = l.schluessel()?;
            l.leer();
            let ende = if liste { "]]" } else { "]" };
            if !l.folgt(ende) {
                return Err(l.fehler("Tabellenkopf nicht abgeschlossen"));
            }
            l.weiter(ende.len());
            l.zeilenende()?;
            if liste {
                let Some((letzter, weg)) = k.split_last() else {
                    return Err(l.fehler("leerer Tabellenkopf"));
                };
                let eltern = tabelle_bei(&mut wurzel, weg).map_err(|e| l.fehler(&e))?;
                let eintrag = eltern
                    .entry(letzter.clone())
                    .or_insert_with(|| Wert::Liste(Vec::new()));
                match eintrag {
                    Wert::Liste(v) => v.push(Wert::Tabelle(BTreeMap::new())),
                    _ => return Err(l.fehler(&format!("{} ist keine Tabellen-Liste", k.join(".")))),
                }
            } else {
                if gesehen.contains(&k) {
                    return Err(l.fehler(&format!("Tabelle {} doppelt", k.join("."))));
                }
                gesehen.push(k.clone());
                tabelle_bei(&mut wurzel, &k).map_err(|e| l.fehler(&e))?;
            }
            kopf = k;
            continue;
        }
        let k = l.schluessel()?;
        l.leer();
        if l.an(0) != Some('=') {
            return Err(l.fehler("= erwartet"));
        }
        l.weiter(1);
        let w = l.wert()?;
        l.zeilenende()?;
        let ziel = tabelle_bei(&mut wurzel, &kopf).map_err(|e| l.fehler(&e))?;
        setze(ziel, &k, w).map_err(|e| l.fehler(&e))?;
    }
    Ok(Wert::Tabelle(wurzel))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_in_text_ist_keine_tabelle() {
        let w = lies(
            "[package]\nname = \"x\"\n[package.metadata.d]\nnotiz = \"\"\"\n[lints]\nworkspace = true\n\"\"\"\n",
        );
        assert!(w.is_ok(), "{w:?}");
        let Ok(w) = w else { return };
        assert!(w.feld("lints").is_none());
        assert_eq!(
            w.pfad(&["package", "metadata", "d", "notiz"])
                .and_then(Wert::text),
            Some("[lints]\nworkspace = true\n")
        );
    }

    #[test]
    fn gepunktet_inline_und_listen() {
        let w = lies(
            "lints.workspace = true\n[dependencies]\na = { version = \"1\", features = [\"x\",\n \"y\", ] } # k\nb.workspace = true\n[[bin]]\nname = 'n'\n[[bin]]\nname = '''m'''\n",
        );
        assert!(w.is_ok(), "{w:?}");
        let Ok(w) = w else { return };
        assert_eq!(w.pfad(&["lints", "workspace"]), Some(&Wert::Wahr(true)));
        assert_eq!(
            w.pfad(&["dependencies", "b", "workspace"]),
            Some(&Wert::Wahr(true))
        );
        assert!(matches!(w.feld("bin"), Some(Wert::Liste(l)) if l.len() == 2));
        assert_eq!(
            w.pfad(&["dependencies", "a"]).map(Wert::kanonisch),
            Some("{\"features\"=[\"x\",\"y\"],\"version\"=\"1\"}".to_string())
        );
    }

    #[test]
    fn fehler_sind_fehler() {
        assert!(lies("a = 1\na = 2\n").is_err());
        assert!(lies("[a]\n[a]\n").is_err());
        assert!(lies("a = \"offen\n").is_err());
        assert!(lies("a = @\n").is_err());
        assert!(lies("a = 1 b\n").is_err());
    }
}
