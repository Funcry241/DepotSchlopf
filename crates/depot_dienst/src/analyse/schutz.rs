//! Zusätzliche Schwärzung für exportierte Laufzeitdaten.

use depot_kern::json;

use crate::protokoll::Schutz;

pub(super) struct Exportschutz<'a> {
    schluessel: &'a Schutz,
    namen: Vec<String>,
    sec_kontakt: Option<String>,
}

impl<'a> Exportschutz<'a> {
    pub(super) fn neu(schluessel: &'a Schutz, sec_kontakt: Option<&str>) -> Self {
        let namen = ["USERNAME", "USER", "COMPUTERNAME", "HOSTNAME"]
            .into_iter()
            .filter_map(|name| std::env::var(name).ok())
            .filter(|name| name.len() >= 3)
            .map(|name| name.to_lowercase())
            .collect();
        Self {
            schluessel,
            namen,
            sec_kontakt: sec_kontakt
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_lowercase),
        }
    }

    fn personenbezogen(&self, text: &str) -> bool {
        let klein = text.to_lowercase();
        let woerter = klein.split(|c: char| !c.is_alphanumeric() && !matches!(c, '-' | '_' | '.'));
        let pfad = text.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '(' | '[' | '`' | '=' | ';'))
            .any(|wort| wort.starts_with('/') || wort.starts_with('\\')
                || matches!(wort.as_bytes().get(..3), Some([a, b':', b'\\' | b'/']) if a.is_ascii_alphabetic()));
        pfad || text.chars().any(|c| c.is_control() && c != '\t')
            || text.contains('@')
            || self
                .sec_kontakt
                .as_ref()
                .is_some_and(|kontakt| klein.contains(kontakt))
            || [
                "iban",
                "bic",
                "swift",
                "kartennummer",
                "card number",
                "sec-contact",
            ]
            .iter()
            .any(|wort| klein.contains(&format!("{wort}:")) || klein.contains(&format!("{wort}=")))
            || woerter
                .clone()
                .any(|wort| self.namen.iter().any(|name| wort.trim_matches('.') == name))
            || woerter.clone().any(bankdaten)
            || gruppierte_bankdaten(&klein)
    }

    pub(super) fn text(&self, text: &str, jsonl: bool) -> (String, usize) {
        let mut aus = String::with_capacity(text.len());
        let mut anzahl = 0_usize;
        for zeile in text.split_inclusive('\n') {
            let (inhalt, ende) = zeile.strip_suffix("\r\n").map_or_else(
                || zeile.strip_suffix('\n').map_or((zeile, ""), |z| (z, "\n")),
                |z| (z, "\r\n"),
            );
            let persoenlich = self.personenbezogen(inhalt)
                || (inhalt.contains('\\')
                    && json::alle_texte(inhalt)
                        .iter()
                        .any(|s| self.personenbezogen(s)));
            if persoenlich || self.schluessel.bereinige(inhalt) != inhalt {
                anzahl = anzahl.saturating_add(1);
                aus.push_str(if jsonl {
                    "{\"geschwaerzt\":true}"
                } else {
                    "[Zeile geschwärzt]"
                });
            } else {
                aus.push_str(inhalt);
            }
            aus.push_str(ende);
        }
        (aus, anzahl)
    }
}

fn gruppierte_bankdaten(text: &str) -> bool {
    for abschnitt in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == ' ' || c == '-')) {
        let mut offset = 0_usize;
        for wort in abschnitt.split_inclusive([' ', '-']) {
            let start = offset;
            offset = offset.saturating_add(wort.len());
            let anfang = wort.trim_end_matches([' ', '-']).as_bytes();
            let plausibel = matches!(anfang, [a, b, c, d]
                if a.is_ascii_lowercase() && b.is_ascii_lowercase()
                    && c.is_ascii_digit() && d.is_ascii_digit())
                || ((3..=6).contains(&anfang.len()) && anfang.iter().all(u8::is_ascii_digit));
            if !plausibel {
                continue;
            }
            let mut nummer = String::new();
            for gruppe in abschnitt
                .get(start..)
                .unwrap_or_default()
                .split_inclusive([' ', '-'])
                .take(9)
            {
                let gruppe = gruppe.trim_end_matches([' ', '-']);
                if gruppe.is_empty() || gruppe.len() > 6 {
                    break;
                }
                nummer.push_str(gruppe);
                if bankdaten(&nummer) {
                    return true;
                }
            }
        }
    }
    false
}

fn bankdaten(text: &str) -> bool {
    if !(13..=34).contains(&text.len()) {
        return false;
    }
    match text.as_bytes() {
        [a, b, c, d, rest @ ..]
            if a.is_ascii_lowercase()
                && b.is_ascii_lowercase()
                && c.is_ascii_digit()
                && d.is_ascii_digit()
                && (11..=30).contains(&rest.len()) =>
        {
            let restwert = rest.iter().chain([a, b, c, d]).try_fold(0_u32, |summe, z| {
                let (faktor, wert) = match *z {
                    b'0'..=b'9' => (10, u32::from(z.saturating_sub(b'0'))),
                    b'a'..=b'z' => (100, u32::from(z.saturating_sub(b'a')).saturating_add(10)),
                    _ => return None,
                };
                Some(
                    summe
                        .saturating_mul(faktor)
                        .saturating_add(wert)
                        .wrapping_rem(97),
                )
            });
            restwert == Some(1)
        }
        daten if (13..=19).contains(&daten.len()) && daten.iter().all(u8::is_ascii_digit) => {
            let summe = daten.iter().rev().enumerate().fold(0_u32, |summe, (i, b)| {
                let z = u32::from(b.saturating_sub(b'0'));
                let wert = if i.is_multiple_of(2) {
                    z
                } else {
                    z.saturating_mul(2)
                        .saturating_sub(if z > 4 { 9 } else { 0 })
                };
                summe.saturating_add(wert)
            });
            summe != 0 && summe.is_multiple_of(10)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schwaerzt_rohtext_und_kodierte_gespraeche() {
        let mut schluessel = Schutz::default();
        schluessel.merken("FiktiverLaufzeitschluessel");
        let schutz = Exportschutz {
            schluessel: &schluessel,
            namen: vec!["testperson".into(), "testrechner".into()],
            sec_kontakt: None,
        };
        let iban = ["DE89", "3704", "0044", "0532", "0130", "00"].join(" ");
        let karte = ["4111", "1111", "1111", "1111"].join(" ");
        let iban = format!("Text mit {iban} im Satz");
        let karte = format!("Text mit {karte} im Satz");
        for text in [
            "LAUFZEITSCHLUESSEL",
            r#"{"frage":"test\u0040example.test"}"#,
            r#"{"antwort":"C:\\Users\\Testperson\\log.txt"}"#,
            "Datei /home/testperson/ausgabe.txt",
            "pfad=/srv/vertraulich/ausgabe.txt",
            "Auf Testrechner beendet",
            &iban,
            &karte,
            "Empfänger BIC: TESTDEHH",
        ] {
            assert_eq!(
                schutz.text(text, true),
                ("{\"geschwaerzt\":true}".into(), 1)
            );
        }
        for (roh, kodiert) in [
            (
                "sk-ant-ErfundenerTestwert".into(),
                r#"{"text":"s\u006b-ant-ErfundenerTestwert"}"#.into(),
            ),
            (
                "sk-proj-ErfundenerTestwert".into(),
                r#"{"text":"s\u006b-proj-ErfundenerTestwert"}"#.into(),
            ),
            (
                "AQ.Erfundener_Gemini_Testwert".into(),
                r#"{"text":"A\u0051.Erfundener_Gemini_Testwert"}"#.into(),
            ),
            (
                format!("AIza{}", "X".repeat(35)),
                format!(r#"{{"text":"AI\u007aa{}"}}"#, "X".repeat(35)),
            ),
            (
                "Bearer erfundenerWert".into(),
                r#"{"text":"Bear\u0065r erfundenerWert"}"#.into(),
            ),
            (
                "key=erfundenerWert".into(),
                r#"{"text":"k\u0065y=erfundenerWert"}"#.into(),
            ),
        ] {
            for text in [roh, kodiert] {
                assert_eq!(
                    schutz.text(&text, true),
                    ("{\"geschwaerzt\":true}".into(), 1)
                );
            }
        }
        assert_eq!(
            schutz.text("Depotwert 1.024,68 EUR\r\n", false),
            ("Depotwert 1.024,68 EUR\r\n".into(), 0)
        );
        assert_eq!(schutz.text("{\"geschwaerzt\":true}\n", true).1, 0);
        assert_eq!(
            schutz
                .text("Quelle https://example.test/nachricht", false)
                .1,
            0
        );
    }
}
