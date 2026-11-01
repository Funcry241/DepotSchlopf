//! Syntaktische Auswertung von Rust-Quelltext auf Token-Ebene (R26).
//!
//! - Lint-Attribute samt `cfg_attr` (auch verschachtelt; die Bedingung wird nie als falsch
//!   angenommen, jede Ausnahme zaehlt).
//! - Importe samt Gruppen, `self`, Umbenennung, Glob und `extern crate … as …`.
//! - Pfade im Code, aufgeloest ueber diese Importe.
//!
//! Grenze: Makros werden nicht expandiert, Reexporte anderer Crates nicht verfolgt. Deshalb
//! erzwingen Crate-Grenzen die Reinheit zusaetzlich (reine Crates haengen nur von reinen ab),
//! und `include!` ist im Produktivcode verboten.

use std::collections::BTreeMap;

use crate::lexer::Token;

/// Ein Lint-Attribut wie `#[allow(clippy::unwrap_used, reason = "…")]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LintAttribut {
    /// `allow`, `expect`, `warn`, `deny` oder `forbid`.
    pub ebene: String,
    /// Lint-Namen wie `clippy::unwrap_used` oder `unsafe_code` (ohne `rust::`).
    pub lints: Vec<String>,
    /// Hat ein `reason = …`.
    pub grund: bool,
    /// Inneres Attribut (`#![…]`).
    pub innen: bool,
    /// Steht hinter `cfg_attr`.
    pub bedingt: bool,
}

const EBENEN: [&str; 5] = ["allow", "expect", "warn", "deny", "forbid"];

/// Index der zur oeffnenden Klammer bei `start` passenden schliessenden Klammer.
fn schliessend(t: &[Token], start: usize) -> Option<usize> {
    let mut tiefe = 0_usize;
    for (i, tok) in t.iter().enumerate().skip(start) {
        if tok.ist("(") || tok.ist("[") || tok.ist("{") {
            tiefe = tiefe.saturating_add(1);
        } else if tok.ist(")") || tok.ist("]") || tok.ist("}") {
            tiefe = tiefe.checked_sub(1)?;
            if tiefe == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// Teilt eine Tokenfolge an Kommas der obersten Ebene.
fn teile(t: &[Token]) -> Vec<&[Token]> {
    let mut aus = Vec::new();
    let mut tiefe = 0_usize;
    let mut start = 0_usize;
    for (i, tok) in t.iter().enumerate() {
        if tok.ist("(") || tok.ist("[") || tok.ist("{") {
            tiefe = tiefe.saturating_add(1);
        } else if tok.ist(")") || tok.ist("]") || tok.ist("}") {
            tiefe = tiefe.saturating_sub(1);
        } else if tok.ist(",") && tiefe == 0 {
            aus.push(t.get(start..i).unwrap_or_default());
            start = i.saturating_add(1);
        }
    }
    let rest = t.get(start..).unwrap_or_default();
    if !rest.is_empty() {
        aus.push(rest);
    }
    aus
}

/// Pfad aus Woertern und `::` als Text; `None`, wenn noch anderes darin steht.
fn pfadtext(t: &[Token]) -> Option<String> {
    let mut s = String::new();
    for tok in t {
        match tok {
            Token::Wort(w) => s.push_str(w),
            Token::Zeichen("::") => s.push_str("::"),
            _ => return None,
        }
    }
    (!s.is_empty()).then_some(s)
}

/// Wertet den Inhalt eines Attributs aus (`allow(…)`, `cfg_attr(…)`, …).
fn attribut(inhalt: &[Token], innen: bool, bedingt: bool, aus: &mut Vec<LintAttribut>) {
    let Some(Token::Wort(name)) = inhalt.first() else {
        return;
    };
    let Some(gruppe) = inhalt
        .get(1..)
        .filter(|r| r.first().is_some_and(|t| t.ist("(")) && r.last().is_some_and(|t| t.ist(")")))
        .and_then(|r| r.get(1..r.len().saturating_sub(1)))
    else {
        return;
    };
    if name == "cfg_attr" {
        for teil in teile(gruppe).into_iter().skip(1) {
            attribut(teil, innen, true, aus);
        }
        return;
    }
    if !EBENEN.contains(&name.as_str()) {
        return;
    }
    let mut a = LintAttribut {
        ebene: name.clone(),
        lints: Vec::new(),
        grund: false,
        innen,
        bedingt,
    };
    for teil in teile(gruppe) {
        if teil.first().is_some_and(|t| t.ist_wort("reason"))
            && teil.get(1).is_some_and(|t| t.ist("="))
        {
            a.grund = true;
        } else if let Some(p) = pfadtext(teil) {
            a.lints
                .push(p.strip_prefix("rust::").map_or(p.clone(), str::to_string));
        } else {
            // Unbekannte Form: als Lint mit verdaechtigem Namen behalten, damit nichts entgeht.
            a.lints.push("?".into());
        }
    }
    aus.push(a);
}

/// Alle Lint-Attribute der Datei, auch hinter `cfg_attr` und in Makrokoerpern.
pub fn lint_attribute(t: &[Token]) -> Vec<LintAttribut> {
    let mut aus = Vec::new();
    for (i, tok) in t.iter().enumerate() {
        if !tok.ist("#") {
            continue;
        }
        let innen = t.get(i.saturating_add(1)).is_some_and(|x| x.ist("!"));
        let auf = if innen {
            i.saturating_add(2)
        } else {
            i.saturating_add(1)
        };
        if !t.get(auf).is_some_and(|x| x.ist("[")) {
            continue;
        }
        let Some(zu) = schliessend(t, auf) else {
            continue;
        };
        if let Some(inhalt) = t.get(auf.saturating_add(1)..zu) {
            attribut(inhalt, innen, false, &mut aus);
        }
    }
    aus
}

/// Ein aufgeloester Import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    /// Name, unter dem der Import sichtbar ist; `None` bei `as _` oder Glob.
    pub alias: Option<String>,
    /// Voller Pfad ohne fuehrendes `::`.
    pub pfad: Vec<String>,
    pub glob: bool,
}

enum Umbenennung {
    Keine,
    Name(String),
    Unterstrich,
}

struct Baumleser<'a> {
    t: &'a [Token],
    i: usize,
}

impl Baumleser<'_> {
    fn tok(&self) -> Option<&Token> {
        self.t.get(self.i)
    }

    fn weiter(&mut self) {
        self.i = self.i.saturating_add(1);
    }

    /// `as name` oder `as _`.
    fn umbenennung(&mut self) -> Result<Umbenennung, String> {
        if !self.tok().is_some_and(|t| t.ist_wort("as")) {
            return Ok(Umbenennung::Keine);
        }
        self.weiter();
        let aus = match self.tok() {
            Some(Token::Wort(w)) if w == "_" => Umbenennung::Unterstrich,
            Some(Token::Wort(w)) => Umbenennung::Name(w.clone()),
            _ => return Err("Umbenennung nicht lesbar".into()),
        };
        self.weiter();
        Ok(aus)
    }

    fn import(&mut self, pfad: Vec<String>, aus: &mut Vec<Import>) -> Result<(), String> {
        let alias = match self.umbenennung()? {
            Umbenennung::Keine => pfad.last().cloned(),
            Umbenennung::Name(n) => Some(n),
            Umbenennung::Unterstrich => None,
        };
        aus.push(Import {
            alias,
            pfad,
            glob: false,
        });
        Ok(())
    }

    /// `use`-Baum ab `self.i` mit Praefix `vor`.
    fn baum(&mut self, vor: &[String], aus: &mut Vec<Import>) -> Result<(), String> {
        let mut pfad = vor.to_vec();
        if self.tok().is_some_and(|t| t.ist("::")) {
            self.weiter();
        }
        loop {
            match self.tok() {
                Some(Token::Wort(w)) if w == "self" && !pfad.is_empty() => {
                    self.weiter();
                    return self.import(pfad, aus);
                }
                Some(Token::Wort(w)) => {
                    pfad.push(w.clone());
                    self.weiter();
                    if self.tok().is_some_and(|t| t.ist("::")) {
                        self.weiter();
                        continue;
                    }
                    return self.import(pfad, aus);
                }
                Some(t) if t.ist("*") => {
                    self.weiter();
                    aus.push(Import {
                        alias: None,
                        pfad,
                        glob: true,
                    });
                    return Ok(());
                }
                Some(t) if t.ist("{") => {
                    self.weiter();
                    loop {
                        if self.tok().is_some_and(|t| t.ist("}")) {
                            self.weiter();
                            return Ok(());
                        }
                        self.baum(&pfad, aus)?;
                        match self.tok() {
                            Some(t) if t.ist(",") => self.weiter(),
                            Some(t) if t.ist("}") => {}
                            _ => return Err("use-Gruppe nicht lesbar".into()),
                        }
                    }
                }
                _ => return Err("use-Baum nicht lesbar".into()),
            }
        }
    }
}

/// Alle Importe der Datei (`use …;` und `extern crate … as …;`), ohne Ruecksicht auf den
/// Gueltigkeitsbereich: Ein Import in einer Funktion zaehlt fuer die ganze Datei.
///
/// # Errors
/// Ein `use`, das sich nicht lesen laesst.
pub fn importe(t: &[Token]) -> Result<Vec<Import>, String> {
    let mut aus = Vec::new();
    for (i, tok) in t.iter().enumerate() {
        if tok.ist_wort("use") {
            // `use` als Schluesselwort, nicht in `#[...]` oder als Pfadteil.
            if i.checked_sub(1)
                .and_then(|j| t.get(j))
                .is_some_and(|v| v.ist("::"))
            {
                continue;
            }
            let mut l = Baumleser {
                t,
                i: i.saturating_add(1),
            };
            l.baum(&[], &mut aus)?;
            if !l.tok().is_some_and(|x| x.ist(";")) {
                return Err("use ohne abschliessendes Semikolon".into());
            }
        } else if tok.ist_wort("extern")
            && t.get(i.saturating_add(1))
                .is_some_and(|x| x.ist_wort("crate"))
        {
            let mut l = Baumleser {
                t,
                i: i.saturating_add(2),
            };
            let Some(Token::Wort(name)) = l.tok() else {
                return Err("extern crate nicht lesbar".into());
            };
            let name = name.clone();
            l.weiter();
            l.import(vec![name], &mut aus)?;
        }
    }
    Ok(aus)
}

/// Jeder Alias mit allen Bedeutungen, die er irgendwo in der Datei hat.
pub fn aliasse(importe: &[Import]) -> BTreeMap<&str, Vec<&[String]>> {
    let mut alias: BTreeMap<&str, Vec<&[String]>> = BTreeMap::new();
    for imp in importe {
        if let Some(a) = &imp.alias {
            let bedeutungen = alias.entry(a.as_str()).or_default();
            if !bedeutungen.contains(&imp.pfad.as_slice()) {
                bedeutungen.push(imp.pfad.as_slice());
            }
        }
    }
    alias
}

/// Aliasse mit mehr als einer Bedeutung in derselben Datei.
pub fn mehrdeutige_aliasse(importe: &[Import]) -> Vec<String> {
    aliasse(importe)
        .into_iter()
        .filter(|(_, b)| b.len() > 1)
        .map(|(a, b)| {
            let b: Vec<String> = b.iter().map(|p| p.join("::")).collect();
            format!("{a} ({})", b.join(" | "))
        })
        .collect()
}

/// Namen aller Attribute der Datei, auch hinter `cfg_attr` (z. B. `path`).
pub fn attributnamen(t: &[Token]) -> Vec<String> {
    fn sammle(inhalt: &[Token], aus: &mut Vec<String>) {
        let Some(Token::Wort(name)) = inhalt.first() else {
            return;
        };
        aus.push(name.clone());
        if name == "cfg_attr"
            && let Some(gruppe) = inhalt
                .get(1..)
                .filter(|r| r.first().is_some_and(|x| x.ist("(")))
                .and_then(|r| r.get(1..r.len().saturating_sub(1)))
        {
            for teil in teile(gruppe).into_iter().skip(1) {
                sammle(teil, aus);
            }
        }
    }
    let mut aus = Vec::new();
    for (i, tok) in t.iter().enumerate() {
        if !tok.ist("#") {
            continue;
        }
        let innen = t.get(i.saturating_add(1)).is_some_and(|x| x.ist("!"));
        let auf = if innen {
            i.saturating_add(2)
        } else {
            i.saturating_add(1)
        };
        if !t.get(auf).is_some_and(|x| x.ist("[")) {
            continue;
        }
        if let (Some(zu), Some(start)) = (schliessend(t, auf), auf.checked_add(1))
            && let Some(inhalt) = t.get(start..zu)
        {
            sammle(inhalt, &mut aus);
        }
    }
    aus
}

/// Kommt die Segmentfolge `muster` irgendwo im Pfad vor (nicht nur am Anfang)?
pub fn enthaelt(pfad: &[String], muster: &[&str]) -> bool {
    !muster.is_empty()
        && pfad
            .windows(muster.len())
            .any(|w| w.iter().zip(muster).all(|(a, b)| a == b))
}

/// Alle Pfade im Code (`a::b::c`, auch mit fuehrendem `::`), aufgeloest ueber die Importe.
/// Einzelne Woerter ohne `::` zaehlen nur, wenn sie ein Import-Alias sind.
pub fn pfade(t: &[Token], importe: &[Import]) -> Vec<Vec<String>> {
    let alias = aliasse(importe);
    let mut aus = Vec::new();
    let mut i = 0_usize;
    while i < t.len() {
        let teil_eines_pfads = i
            .checked_sub(1)
            .and_then(|j| t.get(j))
            .is_some_and(|v| v.ist("::"));
        let start_wort = matches!(t.get(i), Some(Token::Wort(_))) && !teil_eines_pfads;
        let start_trenner = t.get(i).is_some_and(|x| x.ist("::"))
            && !i
                .checked_sub(1)
                .and_then(|j| t.get(j))
                .is_some_and(|v| matches!(v, Token::Wort(_)) || v.ist(">"));
        if !start_wort && !start_trenner {
            i = i.saturating_add(1);
            continue;
        }
        let mut j = if start_trenner {
            i.saturating_add(1)
        } else {
            i
        };
        let mut segmente: Vec<String> = Vec::new();
        while let Some(Token::Wort(w)) = t.get(j) {
            segmente.push(w.clone());
            if t.get(j.saturating_add(1)).is_some_and(|x| x.ist("::")) {
                j = j.saturating_add(2);
            } else {
                j = j.saturating_add(1);
                break;
            }
        }
        i = j.max(i.saturating_add(1));
        let Some(erstes) = segmente.first() else {
            continue;
        };
        match alias.get(erstes.as_str()) {
            // Jede Bedeutung des Alias zaehlt: Importe gelten je Gueltigkeitsbereich, der
            // Kodex kennt diese nicht und nimmt deshalb alle an (K7-01).
            Some(bedeutungen) if !start_trenner => {
                for voll in bedeutungen {
                    let mut p = voll.to_vec();
                    p.extend(segmente.iter().skip(1).cloned());
                    aus.push(p);
                }
            }
            _ if segmente.len() > 1 => aus.push(segmente),
            _ => {}
        }
    }
    aus
}

/// Hat der Pfad das Praefix `vor`?
pub fn beginnt_mit(pfad: &[String], vor: &[&str]) -> bool {
    pfad.len() >= vor.len() && pfad.iter().zip(vor).all(|(a, b)| a == b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::zerlege;

    fn attrs(q: &str) -> Vec<LintAttribut> {
        lint_attribute(&zerlege(q).unwrap_or_default())
    }

    #[test]
    fn cfg_attr_wird_ausgewertet() {
        let a = attrs(
            "#[cfg_attr(not(test), cfg_attr(all(), allow(clippy::panic, reason = \"x\")))] fn f() {}",
        );
        assert_eq!(a.len(), 1);
        assert_eq!(
            a.first().map(|x| x.lints.clone()),
            Some(vec!["clippy::panic".to_string()])
        );
        assert!(a.first().is_some_and(|x| x.bedingt && x.grund && !x.innen));
        let k = attrs("#![forbid(unsafe_code, rust::unused)]");
        assert!(
            k.first()
                .is_some_and(|x| x.innen && !x.bedingt && x.ebene == "forbid")
        );
        assert_eq!(
            k.first().map(|x| x.lints.clone()),
            Some(vec!["unsafe_code".to_string(), "unused".to_string()])
        );
    }

    fn aufgeloest(q: &str) -> Vec<String> {
        let t = zerlege(q).unwrap_or_default();
        let imp = importe(&t).unwrap_or_default();
        let mut p: Vec<String> = imp
            .iter()
            .map(|i| i.pfad.join("::") + if i.glob { "::*" } else { "" })
            .collect();
        p.extend(pfade(&t, &imp).into_iter().map(|x| x.join("::")));
        p
    }

    #[test]
    fn importe_werden_aufgeloest() {
        let p = aufgeloest(
            "use std::{env, fs::{self as d}, string::String as T};\n\
             extern crate std as basis;\n\
             fn f() { env::var(\"x\"); d::read(\"y\"); basis::process::id(); ::std::net::Ipv4Addr::new(); }",
        );
        for erwartet in [
            "std::env::var",
            "std::fs::read",
            "std::process::id",
            "std::net::Ipv4Addr::new",
            "std::string::String",
        ] {
            assert!(p.iter().any(|x| x == erwartet), "{erwartet} fehlt in {p:?}");
        }
        let g = aufgeloest("use std::*; use a::b as _;");
        assert!(g.iter().any(|x| x == "std::*"), "{g:?}");
    }

    #[test]
    fn alias_mit_mehreren_bedeutungen() {
        let p = aufgeloest(
            "fn a() { use std as b; b::fs::x(); }\nfn c() { use core as b; b::mem::y(); }",
        );
        assert!(p.iter().any(|x| x == "std::fs::x"), "{p:?}");
        let t = zerlege("fn a() { use std as b; } fn c() { use core as b; }").unwrap_or_default();
        assert_eq!(
            mehrdeutige_aliasse(&importe(&t).unwrap_or_default()).len(),
            1
        );
    }

    #[test]
    fn attribute_hinter_cfg_attr() {
        let t = zerlege("#[cfg_attr(all(), path = \"x.md\")] mod m;").unwrap_or_default();
        assert!(attributnamen(&t).iter().any(|n| n == "path"));
    }

    #[test]
    fn fremde_pfade_bleiben_fremd() {
        let p = aufgeloest("use crate::env; fn f() { env::x(); self::std::y(); }");
        assert!(!p.iter().any(|x| x.starts_with("std::")), "{p:?}");
    }
}
