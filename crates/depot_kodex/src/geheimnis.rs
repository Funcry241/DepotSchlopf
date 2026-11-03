//! Inhaltspruefung auf Geheimnismuster (R28, 2·W3, 1·E01).
//!
//! Zugangsdaten liegen nie in Dateien des Projekts, sondern ausserhalb (Windows-Anmeldeverwaltung).
//! Eine Liste verbotener Dateinamen kennt nie alle Geheimnisse; deshalb wird jeder Inhalt
//! geprueft. Erkannt werden:
//!
//! - private Schluessel im PEM-Format,
//! - Zugangsschluessel mit bekanntem Praefix (Anthropic, OpenAI, GitHub, AWS, Slack, GitLab),
//! - IBANs mit gueltiger Pruefziffer, auch in Vierergruppen,
//! - Zuweisungen an Namen wie `password`, `secret` oder `api_key` mit einem Wert, der nach
//!   Schluessel aussieht.
//!
//! Geprueft werden Arbeitsbaum und Index, denn ein Commit nimmt den Index (K10-01). Gelesen wird
//! UTF-8 und UTF-16 mit BOM; was sich so nicht lesen laesst, ist nicht pruefbar und damit ein
//! Fehler (K10-02).
//!
//! Die Meldung nennt Datei, Zeile und Art, nie das Geheimnis selbst. Ein Treffer ist ein
//! Fehler; eine Ausnahmeliste gibt es nicht. Beispiele in Tests entstehen erst zur Laufzeit.

use std::fs;
use std::path::Path;

/// Ein Fund: Art des Musters und Zeile (ab 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Treffer {
    pub art: &'static str,
    pub zeile: usize,
}

/// Praefixe von Zugangsschluesseln und die Mindestlaenge des Rests.
const PRAEFIXE: [(&str, usize); 11] = [
    ("sk-ant-", 20),
    ("sk-proj-", 20),
    ("sk-", 32),
    ("ghp_", 30),
    ("gho_", 30),
    ("ghs_", 30),
    ("github_pat_", 30),
    ("glpat-", 20),
    ("xoxb-", 20),
    ("xoxp-", 20),
    ("AKIA", 16),
];

/// Namen, deren zugewiesener Wert ein Geheimnis ist.
const SCHLUESSELNAMEN: [&str; 9] = [
    "password",
    "passwort",
    "kennwort",
    "secret",
    "api_key",
    "apikey",
    "api-key",
    "access_token",
    "auth_token",
];

fn wortzeichen(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Steht an Byte `i` von `zeile` ein Wortanfang?
fn wortanfang(zeile: &str, i: usize) -> bool {
    zeile
        .get(..i)
        .and_then(|vor| vor.chars().next_back())
        .is_none_or(|c| !wortzeichen(c))
}

fn praefix_schluessel(zeile: &str) -> bool {
    PRAEFIXE.iter().any(|(praefix, rest)| {
        zeile.match_indices(praefix).any(|(i, _)| {
            wortanfang(zeile, i)
                && zeile
                    .get(i.saturating_add(praefix.len())..)
                    .is_some_and(|r| r.chars().take_while(|c| wortzeichen(*c)).count() >= *rest)
        })
    })
}

/// Rest einer IBAN modulo 97 (ISO 13616): die ersten vier Zeichen ans Ende, Buchstaben als
/// Zahlen 10 bis 35.
fn iban_rest(iban: &[char]) -> Option<u32> {
    let (kopf, rumpf) = (iban.get(..4)?, iban.get(4..)?);
    let mut rest: u32 = 0;
    for c in rumpf.iter().chain(kopf) {
        let wert = c.to_digit(36)?;
        let faktor = if wert >= 10 { 100 } else { 10 };
        rest = rest
            .checked_mul(faktor)?
            .checked_add(wert)?
            .checked_rem(97)?;
    }
    Some(rest)
}

/// Sucht IBANs: zwei Grossbuchstaben, zwei Ziffern, dann Grossbuchstaben und Ziffern, auch in
/// Gruppen mit einzelnen Leerzeichen. Gueltig ist eine Laenge von 15 bis 34 mit Rest 1.
fn iban(zeile: &str) -> bool {
    let zeichen: Vec<(usize, char)> = zeile.char_indices().collect();
    for (pos, (i, _)) in zeichen.iter().enumerate() {
        if !wortanfang(zeile, *i) {
            continue;
        }
        let mut kette: Vec<char> = Vec::new();
        let mut k = pos;
        while let Some((_, c)) = zeichen.get(k) {
            let gross_oder_ziffer = c.is_ascii_uppercase() || c.is_ascii_digit();
            if gross_oder_ziffer {
                kette.push(*c);
            } else if *c == ' '
                && !kette.is_empty()
                && zeichen
                    .get(k.saturating_add(1))
                    .is_some_and(|(_, n)| n.is_ascii_uppercase() || n.is_ascii_digit())
            {
                // einzelnes Leerzeichen zwischen Gruppen
            } else {
                break;
            }
            if kette.len() >= 34 {
                break;
            }
            k = k.saturating_add(1);
        }
        let kopf_ok = kette.first().is_some_and(char::is_ascii_uppercase)
            && kette.get(1).is_some_and(char::is_ascii_uppercase)
            && kette.get(2).is_some_and(char::is_ascii_digit)
            && kette.get(3).is_some_and(char::is_ascii_digit);
        if !kopf_ok {
            continue;
        }
        if (15..=kette.len()).any(|n| kette.get(..n).and_then(iban_rest) == Some(1)) {
            return true;
        }
    }
    false
}

/// `name = "wert"` oder `name: wert`, wobei `wert` nach Schluessel aussieht: mindestens 12
/// Zeichen ohne Leerzeichen, mit Buchstaben und Ziffern, kein Ausdruck.
fn zuweisung(zeile: &str) -> bool {
    let klein = zeile.to_ascii_lowercase();
    SCHLUESSELNAMEN.iter().any(|name| {
        klein.match_indices(name).any(|(i, _)| {
            let Some(rest) = klein.get(i.saturating_add(name.len())..) else {
                return false;
            };
            let rest = rest.trim_start_matches(wortzeichen);
            let rest = rest.trim_start_matches([' ', '"', '\'']);
            let Some(rest) = rest.strip_prefix([':', '=']) else {
                return false;
            };
            let wert_anfang = rest.trim_start_matches([' ', '"', '\'']);
            let wert: String = wert_anfang
                .chars()
                .take_while(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | ',' | ';'))
                .collect();
            wert.chars().count() >= 12
                && wert.chars().any(|c| c.is_ascii_alphabetic())
                && wert.chars().any(|c| c.is_ascii_digit())
                && !wert.contains(['(', ')', '{', '}', '[', ']', '<', '>'])
                && !wert.contains("::")
                && !wert.starts_with(['&', '$', '%', '.', '/'])
        })
    })
}

/// Kopfzeile eines privaten Schluessels im PEM-Format: `BEGIN ... PRIVATE KEY` zwischen je
/// fuenf Strichen.
fn privater_schluessel(zeile: &str) -> bool {
    zeile
        .find("-----BEGIN ")
        .and_then(|i| zeile.get(i..))
        .is_some_and(|rest| rest.contains("PRIVATE KEY-----"))
}

/// Alle Funde in einem Text.
pub fn finde(text: &str) -> Vec<Treffer> {
    let mut treffer = Vec::new();
    for (nr, zeile) in text.lines().enumerate() {
        let zeile_nr = nr.saturating_add(1);
        let mut fund = |art| {
            treffer.push(Treffer {
                art,
                zeile: zeile_nr,
            });
        };
        if privater_schluessel(zeile) {
            fund("privater Schluessel");
        }
        if praefix_schluessel(zeile) {
            fund("Zugangsschluessel");
        }
        if iban(zeile) {
            fund("IBAN");
        }
        if zuweisung(zeile) {
            fund("Passwort- oder Schluesselzuweisung");
        }
    }
    treffer
}

/// Liest einen Inhalt als Text (K10-02). Unterstuetzt: UTF-8 (mit oder ohne BOM) und UTF-16
/// mit BOM (LE oder BE). Alles andere ist `None`: nicht pruefbar. Verlustbehaftetes Lesen gibt
/// es nicht, denn ein falsch gelesener Inhalt ist nicht geprueft.
pub fn dekodiere(inhalt: &[u8]) -> Option<String> {
    let utf16 = |rest: &[u8], gross: bool| -> Option<String> {
        if rest.len().checked_rem(2)? != 0 {
            return None;
        }
        let (paare, _) = rest.as_chunks::<2>();
        let einheiten = paare.iter().map(|p| {
            if gross {
                u16::from_be_bytes(*p)
            } else {
                u16::from_le_bytes(*p)
            }
        });
        char::decode_utf16(einheiten)
            .collect::<Result<String, _>>()
            .ok()
    };
    if let Some(rest) = inhalt.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return std::str::from_utf8(rest).ok().map(str::to_string);
    }
    if let Some(rest) = inhalt.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, false);
    }
    if let Some(rest) = inhalt.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, true);
    }
    std::str::from_utf8(inhalt).ok().map(str::to_string)
}

/// Meldungen fuer einen Inhalt; leer, wenn nichts gefunden wurde. Ein Inhalt, der sich nicht
/// lesen laesst, ist ein Fehler (K10-02). Enthaelt der Text Nullzeichen, etwa UTF-16 ohne BOM,
/// wird er zusaetzlich ohne sie geprueft.
pub fn pruefe_inhalt(anzeige: &str, inhalt: &[u8]) -> Vec<String> {
    let Some(text) = dekodiere(inhalt) else {
        return vec![format!(
            "{anzeige}: nicht pruefbar, weder UTF-8 noch UTF-16 mit BOM. Ungepruefte Inhalte \
             gelten nicht als geheimnisfrei (R28)."
        )];
    };
    let mut treffer = finde(&text);
    if text.contains('\0') {
        treffer.extend(finde(&text.replace('\0', "")));
        treffer.sort_by_key(|t| (t.zeile, t.art));
        treffer.dedup();
    }
    treffer
        .into_iter()
        .map(|t| {
            format!(
                "{anzeige}, Zeile {}: Geheimnismuster ({}). Zugangsdaten gehoeren nie in den \
                 Projektordner (R28).",
                t.zeile, t.art
            )
        })
        .collect()
}

fn alle_dateien(wurzel: &Path, dir: &Path, aus: &mut Vec<String>, fehler: &mut Vec<String>) {
    let eintraege = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            fehler.push(format!("{}: nicht lesbar: {e}", dir.display()));
            return;
        }
    };
    for e in eintraege {
        let p = match e {
            Ok(e) => e.path(),
            Err(e) => {
                fehler.push(format!("{}: nicht lesbar: {e}", dir.display()));
                continue;
            }
        };
        let rel = p
            .strip_prefix(wurzel)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        if rel == "target" || rel == ".git" {
            continue;
        }
        match fs::symlink_metadata(&p) {
            Ok(m) if m.is_dir() => alle_dateien(wurzel, &p, aus, fehler),
            Ok(_) => aus.push(rel),
            Err(e) => fehler.push(format!("{rel}: nicht lesbar: {e}")),
        }
    }
}

/// Prueft jede Datei, die ein Commit mitnehmen koennte, in beiden Staenden: im Arbeitsbaum
/// (verfolgt oder neu und nicht ignoriert) und im Index, denn den nimmt ein Commit (K10-01).
/// Ohne Git jede Datei ausser `target/`. Links werden nicht verfolgt, ihr Ziel nicht gelesen.
/// Lesefehler sind Fehler (R29).
pub fn pruefe(wurzel: &Path) -> Vec<String> {
    let mut fehler = Vec::new();
    let mit_git = wurzel.join(".git").exists();
    let dateien = if mit_git {
        match crate::git::projektdateien(wurzel) {
            Ok(d) => d,
            Err(e) => return vec![format!("Geheimnispruefung: Dateiliste fehlt: {e}")],
        }
    } else {
        let mut d = Vec::new();
        alle_dateien(wurzel, wurzel, &mut d, &mut fehler);
        d
    };
    for d in dateien {
        let p = wurzel.join(&d);
        match fs::symlink_metadata(&p) {
            // Im Arbeitsbaum geloescht: nichts zu pruefen.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => fehler.push(format!("{d}: nicht lesbar: {e}")),
            Ok(m) if !m.is_file() => {}
            Ok(_) => match fs::read(&p) {
                Ok(inhalt) => fehler.extend(pruefe_inhalt(&d, &inhalt)),
                Err(e) => fehler.push(format!("{d}: nicht lesbar: {e}")),
            },
        }
    }
    if mit_git {
        match crate::git::index_dateien(wurzel) {
            Ok(index) => {
                for datei in index {
                    let anzeige = format!("{} (Index)", datei.pfad);
                    match crate::git::blob(wurzel, &datei.blob) {
                        Ok(inhalt) => fehler.extend(pruefe_inhalt(&anzeige, &inhalt)),
                        Err(e) => fehler.push(format!("{anzeige}: nicht lesbar: {e}")),
                    }
                }
            }
            Err(e) => fehler.push(format!("Geheimnispruefung: Index nicht lesbar: {e}")),
        }
    }
    fehler
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arten(text: &str) -> Vec<&'static str> {
        finde(text).into_iter().map(|t| t.art).collect()
    }

    #[test]
    fn erkennt_beispiele() {
        let iban = ["DE89", "3704", "0044", "0532", "0130", "00"];
        assert_eq!(arten(&iban.concat()), ["IBAN"]);
        assert_eq!(arten(&format!("Konto {} EUR", iban.join(" "))), ["IBAN"]);
        let gb = ["GB82", "WEST", "1234", "5698", "7654", "32"].concat();
        assert_eq!(arten(&gb), ["IBAN"]);
        let schluessel = ["sk", "-proj-", "Ab3dE5gH7jK9mN1pQ3sT5vX7z"].concat();
        assert_eq!(
            arten(&format!("key \"{schluessel}\"")),
            ["Zugangsschluessel"]
        );
        let aws = ["AK", "IA", "Z7Q4K2M9X5B1N8V3"].concat();
        assert_eq!(arten(&aws), ["Zugangsschluessel"]);
        let anfang = "-----BEGIN RSA ";
        let ende = "PRIVATE KEY-----";
        let pem = [anfang, ende].concat();
        assert_eq!(arten(&pem), ["privater Schluessel"]);
        let zuweisung = ["API_", "KEY=", "x8Kq2Lm9Zt4Vw7Ny"].concat();
        assert_eq!(arten(&zuweisung), ["Passwort- oder Schluesselzuweisung"]);
        let zeilen = format!("a\nb\n{}\n", iban.concat());
        assert_eq!(finde(&zeilen).first().map(|t| t.zeile), Some(3));
    }

    #[test]
    fn harmloses_bleibt_harmlos() {
        for text in [
            "DE0007164600",
            "ISIN DE0007164600 und US0378331005",
            "DE89 ist kein Konto",
            ["DE88", "3704", "0044", "0532", "0130", "00"]
                .concat()
                .as_str(),
            "grenzen: ecdc99b85c76654c8b9dcfed6e75e6da7c430cdb42642b5f175ea6b8d52f0452",
            "let token = Token::Wort(w);",
            "password = std::env::var(\"PASSWORT\")?;",
            "secret: Geheimnis::neu(&quelle),",
            "Der Task-Runner und task-1234567890abcdef1234567890abcdef",
            "Zugangsdaten nur in der Windows-Anmeldeverwaltung.",
            "ask-question-and-answer-in-the-thread-please",
        ] {
            assert!(finde(text).is_empty(), "{text:?}: {:?}", finde(text));
        }
    }

    #[test]
    fn kodierungen() {
        let text = "a\u{e4}b";
        assert_eq!(dekodiere(text.as_bytes()).as_deref(), Some(text));
        let mut bom8 = vec![0xEF, 0xBB, 0xBF];
        bom8.extend(text.as_bytes());
        assert_eq!(dekodiere(&bom8).as_deref(), Some(text));
        let mut le = vec![0xFF, 0xFE];
        let mut be = vec![0xFE, 0xFF];
        for e in text.encode_utf16() {
            le.extend(e.to_le_bytes());
            be.extend(e.to_be_bytes());
        }
        assert_eq!(dekodiere(&le).as_deref(), Some(text));
        assert_eq!(dekodiere(&be).as_deref(), Some(text));
        assert_eq!(dekodiere(&[0xC3, 0x28]), None);
        assert_eq!(dekodiere(&[0xFF, 0xFE, 0x41]), None);
        assert_eq!(dekodiere(&[0xFF, 0xFE, 0x00, 0xD8]), None);
        assert!(
            pruefe_inhalt("x", &[0xC3, 0x28])
                .iter()
                .all(|m| m.contains("nicht pruefbar"))
        );
    }

    #[test]
    fn meldung_nennt_das_geheimnis_nicht() {
        let iban = ["DE89", "3704", "0044", "0532", "0130", "00"].concat();
        let m = pruefe_inhalt("a.md", iban.as_bytes());
        assert_eq!(m.len(), 1);
        assert!(
            m.iter()
                .all(|z| !z.contains(&iban) && z.contains("a.md, Zeile 1"))
        );
    }
}
