//! Die beiden Anbieter: Anfrage bauen, Antwort lesen. Kein Netz hier.
//!
//! - Dachs = Claude Sonnet 5.5 (Anthropic, Messages API).
//! - Maus = GPT-6.1 Terra (OpenAI, Responses API).
//!
//! Beide denken mit mittlerer Tiefe (Wolfgang, 05.11.2026) und bekommen die Antwortgrenze
//! `max_token_aus` (Denken eingeschlossen) aus dem Deckel der Erlaubnis. Werkzeuge des Anbieters,
//! auch die Websuche, werden nie freigegeben: Was sie einfuegen, liesse sich vor dem Aufruf nicht
//! begrenzen (K25-01). Aus jeder Antwort wird der Verbrauch gelesen: Eingabe-Token,
//! Ausgabe-Token und Zahl der Suchen, die null sein muss.

use depot_kern::BeraterId;
use depot_kern::modelle::{Modell, Modellwahl};
use depot_ki::KiDeckel;

use crate::json::{self, Json, zitiere};

pub use depot_kern::modelle::{MODELL_DACHS, MODELL_MAUS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anbieter {
    Anthropic,
    OpenAi,
}

/// Was ein Aufruf verbraucht hat, nach Angabe des Anbieters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Verbrauch {
    pub token_ein: u64,
    pub token_aus: u64,
    pub cache_schreiben: u64,
    pub cache_lesen: u64,
    pub suchen: u64,
}

/// Die gelesene Antwort: der Text des Modells, wenn es eine vollstaendige Antwort gab, und
/// immer der Verbrauch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gelesen {
    pub text: Result<String, String>,
    pub verbrauch: Verbrauch,
    pub modell: Option<String>,
}

impl Anbieter {
    pub fn fuer(berater: BeraterId) -> Anbieter {
        match berater {
            BeraterId::Dachs => Anbieter::Anthropic,
            BeraterId::Maus => Anbieter::OpenAi,
        }
    }

    /// Adresse des Schluessels in der Anmeldeinformationsverwaltung.
    pub fn adresse(self) -> &'static str {
        match self {
            Anbieter::Anthropic => "DepotSchlopf/anthropic",
            Anbieter::OpenAi => "DepotSchlopf/openai",
        }
    }

    pub fn dienst(self) -> &'static str {
        match self {
            Anbieter::Anthropic => "anthropic",
            Anbieter::OpenAi => "openai",
        }
    }

    pub fn modell(self) -> &'static str {
        match self {
            Anbieter::Anthropic => MODELL_DACHS,
            Anbieter::OpenAi => MODELL_MAUS,
        }
    }

    pub fn url(self) -> &'static str {
        match self {
            Anbieter::Anthropic => "https://api.anthropic.com/v1/messages",
            Anbieter::OpenAi => "https://api.openai.com/v1/responses",
        }
    }

    /// Die Kopfzeilen ohne Schluessel; der Schluessel kommt erst beim Versand dazu.
    pub fn kopf<'a>(
        self,
        schluessel: &'a str,
        bearer: &'a mut String,
    ) -> Vec<(&'static str, &'a str)> {
        match self {
            Anbieter::Anthropic => vec![
                ("x-api-key", schluessel),
                ("anthropic-version", "2026-10-21"),
                ("content-type", "application/json"),
            ],
            Anbieter::OpenAi => {
                bearer.clear();
                bearer.push_str("Bearer ");
                bearer.push_str(schluessel);
                vec![
                    ("authorization", bearer.as_str()),
                    ("content-type", "application/json"),
                ]
            }
        }
    }

    /// Der Inhalt der Anfrage.
    pub fn anfrage(self, deckel: &KiDeckel, system: &str, frage: &str) -> String {
        let berater = match self {
            Self::Anthropic => BeraterId::Dachs,
            Self::OpenAi => BeraterId::Maus,
        };
        Self::anfrage_mit_modell(Modellwahl::standard(berater), deckel, system, frage)
    }

    pub(crate) fn anfrage_mit_modell(
        auswahl: Modellwahl,
        deckel: &KiDeckel,
        system: &str,
        frage: &str,
    ) -> String {
        let modell = auswahl.modell();
        let denktiefe = zitiere(auswahl.denktiefe().kennung());
        match Self::fuer(modell.berater()) {
            Anbieter::Anthropic => format!(
                concat!(
                    "{{\"model\":{},\"max_tokens\":{},",
                    "\"thinking\":{{\"type\":\"adaptive\"}},",
                    "\"output_config\":{{\"effort\":{}}},",
                    "\"system\":{},",
                    "\"messages\":[{{\"role\":\"user\",\"content\":{}}}]}}"
                ),
                zitiere(modell.kennung()),
                deckel.max_token_aus(),
                denktiefe,
                zitiere(system),
                anthropic_inhalt(frage)
            ),
            Anbieter::OpenAi => {
                let kopf = format!(
                    concat!(
                        "{{\"model\":{},\"max_output_tokens\":{},",
                        "\"reasoning\":{{\"effort\":{}}},",
                        "\"instructions\":{},\"store\":false"
                    ),
                    zitiere(modell.kennung()),
                    deckel.max_token_aus(),
                    denktiefe,
                    zitiere(system)
                );
                if let Some(eingabe) = openai_cache_eingabe(modell, frage) {
                    let mit_cache = format!(
                        r#"{kopf},"prompt_cache_options":{{"mode":"explicit","ttl":"30m"}},"input":{eingabe}}}"#
                    );
                    // Der zusätzliche JSON-Rahmen darf keine bisher passende Anfrage blockieren.
                    if u32::try_from(mit_cache.len()).is_ok_and(|n| n <= deckel.max_token_ein()) {
                        return mit_cache;
                    }
                }
                format!(r#"{kopf},"input":{}}}"#, zitiere(frage))
            }
        }
    }

    /// Liest eine Antwort mit Status 200.
    ///
    /// # Errors
    /// Die Antwort ist kein JSON oder nennt keinen Verbrauch. Dann ist unbekannt, was der
    /// Aufruf gekostet hat.
    pub fn lies(self, inhalt: &[u8]) -> Result<Gelesen, String> {
        let text = std::str::from_utf8(inhalt).map_err(|_| "Antwort ist kein UTF-8")?;
        let w = json::lies(text)?;
        if w.doppelte_felder() || w.feld("usage").is_some_and(Json::doppelte_felder) {
            return Err("Mehrdeutige Anbieterabrechnung".into());
        }
        let mut gelesen = match self {
            Anbieter::Anthropic => lies_anthropic(&w),
            Anbieter::OpenAi => lies_openai(&w),
        }?;
        gelesen.modell = w.feld("model").and_then(Json::text).map(str::to_owned);
        Ok(gelesen)
    }
}

fn anthropic_inhalt(frage: &str) -> String {
    let Some((tag, rest)) = frage.split_once(crate::frage::TOPFBLOCK) else {
        return zitiere(frage);
    };
    let rest = format!("{}{rest}", crate::frage::TOPFBLOCK);
    format!(
        r#"[{{"type":"text","text":{},"cache_control":{{"type":"ephemeral"}}}},{{"type":"text","text":{}}}]"#,
        zitiere(tag),
        zitiere(&rest)
    )
}

fn openai_cache_eingabe(modell: Modell, frage: &str) -> Option<String> {
    if !matches!(modell, Modell::Terra61 | Modell::Sol62) || !frage.starts_with("Datum: ") {
        return None;
    }
    let (tag, rest) = frage.split_once(crate::frage::TOPFBLOCK)?;
    if rest.trim().is_empty() {
        return None;
    }
    let rest = format!("{}{rest}", crate::frage::TOPFBLOCK);
    Some(format!(
        concat!(
            r#"[{{"role":"user","content":["#,
            r#"{{"type":"input_text","text":{},"prompt_cache_breakpoint":{{"mode":"explicit"}}}},"#,
            r#"{{"type":"input_text","text":{}}}]}}]"#
        ),
        zitiere(tag),
        zitiere(&rest)
    ))
}

fn zahl(w: &Json, pfad: &[&str]) -> Option<u64> {
    let mut x = w;
    for p in pfad {
        x = x.feld(p)?;
    }
    x.anzahl()
}

fn optionale_anzahl(w: &Json, feld: &str) -> Result<u64, String> {
    w.feld(feld).map_or(Ok(0), |v| {
        v.anzahl()
            .ok_or_else(|| format!("Ungültiger Verbrauch: {feld}"))
    })
}

fn lies_anthropic(w: &Json) -> Result<Gelesen, String> {
    let usage = w.feld("usage").ok_or("Antwort ohne Verbrauch")?;
    let ein = zahl(usage, &["input_tokens"]).ok_or("Verbrauch ohne input_tokens")?;
    let cache_schreiben = optionale_anzahl(usage, "cache_creation_input_tokens")?;
    let cache_lesen = optionale_anzahl(usage, "cache_read_input_tokens")?;
    let token_ein = ein
        .checked_add(cache_schreiben)
        .and_then(|summe| summe.checked_add(cache_lesen))
        .ok_or("Eingabe-Token überlaufen")?;
    let aus = zahl(usage, &["output_tokens"]).ok_or("Verbrauch ohne output_tokens")?;
    let suchen = zahl(usage, &["server_tool_use", "web_search_requests"]).unwrap_or(0);
    let verbrauch = Verbrauch {
        token_ein,
        token_aus: aus,
        cache_schreiben,
        cache_lesen,
        suchen,
    };
    let grund = w.feld("stop_reason").and_then(Json::text).unwrap_or("");
    let text = if grund == "end_turn" {
        let teile: Vec<&str> = w
            .feld("content")
            .and_then(Json::liste)
            .unwrap_or(&[])
            .iter()
            .filter(|b| b.feld("type").and_then(Json::text) == Some("text"))
            .filter_map(|b| b.feld("text").and_then(Json::text))
            .collect();
        if teile.is_empty() {
            Err("Antwort ohne Text".into())
        } else {
            Ok(teile.concat())
        }
    } else {
        Err(format!("Antwort unvollständig: stop_reason {grund:?}"))
    };
    Ok(Gelesen {
        text,
        verbrauch,
        modell: None,
    })
}

fn lies_openai(w: &Json) -> Result<Gelesen, String> {
    let usage = w.feld("usage").ok_or("Antwort ohne Verbrauch")?;
    let ein = zahl(usage, &["input_tokens"]).ok_or("Verbrauch ohne input_tokens")?;
    let aus = zahl(usage, &["output_tokens"]).ok_or("Verbrauch ohne output_tokens")?;
    let mut cache_schreiben = 0;
    let mut cache_lesen = 0;
    if let Some(details) = usage.feld("input_tokens_details") {
        if details.doppelte_felder() || !matches!(details, Json::Objekt(_)) {
            return Err("Mehrdeutige OpenAI-Eingabeabrechnung".into());
        }
        cache_schreiben = optionale_anzahl(details, "cache_write_tokens")?;
        cache_lesen = optionale_anzahl(details, "cached_tokens")?;
        if ein
            .checked_sub(cache_schreiben)
            .and_then(|n| n.checked_sub(cache_lesen))
            .is_none()
        {
            return Err("Cache-Anteile sind größer als die Eingabe".into());
        }
    }
    let ausgabe = w.feld("output").and_then(Json::liste).unwrap_or(&[]);
    let suchen = ausgabe
        .iter()
        .filter(|x| x.feld("type").and_then(Json::text) == Some("web_search_call"))
        .count();
    let verbrauch = Verbrauch {
        token_ein: ein,
        token_aus: aus,
        cache_schreiben,
        cache_lesen,
        suchen: u64::try_from(suchen).unwrap_or(u64::MAX),
    };
    let status = w.feld("status").and_then(Json::text).unwrap_or("");
    let text = if status == "completed" {
        let teile: Vec<&str> = ausgabe
            .iter()
            .filter(|x| x.feld("type").and_then(Json::text) == Some("message"))
            .flat_map(|x| x.feld("content").and_then(Json::liste).unwrap_or(&[]))
            .filter(|c| c.feld("type").and_then(Json::text) == Some("output_text"))
            .filter_map(|c| c.feld("text").and_then(Json::text))
            .collect();
        if teile.is_empty() {
            Err("Antwort ohne Text".into())
        } else {
            Ok(teile.concat())
        }
    } else {
        Err(format!("Antwort unvollständig: status {status:?}"))
    };
    Ok(Gelesen {
        text,
        verbrauch,
        modell: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frage::TOPFBLOCK;
    use depot_kern::modelle::Denktiefe;

    #[test]
    fn openai_nur_der_gemeinsame_tagesblock_wird_markiert() -> Result<(), String> {
        let tag = "Datum: 2030-01-07\nBeleg \"Rückgang\"\n";
        for modell in [Modell::Terra61, Modell::Sol62] {
            let wahl =
                Modellwahl::neu(BeraterId::Maus, modell, Denktiefe::Mittel).ok_or("Modellwahl")?;
            let deckel = KiDeckel::fuer_modell(modell);
            for aufgabe in ["Erstmeinung für A", "Zweitmeinung für B", "Abstimmung 1"] {
                let rest = format!("{TOPFBLOCK}{aufgabe}");
                let frage = format!("{tag}{rest}");
                let body = json::lies(&Anbieter::anfrage_mit_modell(
                    wahl, &deckel, "Auftrag", &frage,
                ))?;
                assert_eq!(
                    body.feld("model").and_then(Json::text),
                    Some(modell.kennung())
                );
                assert_eq!(
                    body.feld("instructions").and_then(Json::text),
                    Some("Auftrag")
                );
                assert_eq!(body.feld("store").and_then(Json::wahr), Some(false));
                assert_eq!(
                    body.feld("max_output_tokens").and_then(Json::anzahl),
                    Some(u64::from(deckel.max_token_aus()))
                );
                let optionen = body.feld("prompt_cache_options").ok_or("Cache-Optionen")?;
                assert_eq!(optionen.feld("mode").and_then(Json::text), Some("explicit"));
                assert_eq!(optionen.feld("ttl").and_then(Json::text), Some("30m"));
                let [nachricht] = body.feld("input").and_then(Json::liste).ok_or("Eingabe")? else {
                    return Err("Genau eine Nachricht erwartet".into());
                };
                assert_eq!(nachricht.feld("role").and_then(Json::text), Some("user"));
                let [erster, zweiter] = nachricht
                    .feld("content")
                    .and_then(Json::liste)
                    .ok_or("Nachrichteninhalt")?
                else {
                    return Err("Zwei Textteile erwartet".into());
                };
                for teil in [erster, zweiter] {
                    assert_eq!(teil.feld("type").and_then(Json::text), Some("input_text"));
                }
                assert_eq!(erster.feld("text").and_then(Json::text), Some(tag));
                assert_eq!(
                    zweiter.feld("text").and_then(Json::text),
                    Some(rest.as_str())
                );
                let marke = erster
                    .feld("prompt_cache_breakpoint")
                    .ok_or("Cache-Marke")?;
                assert_eq!(marke.feld("mode").and_then(Json::text), Some("explicit"));
                assert!(zweiter.feld("prompt_cache_breakpoint").is_none());
            }
        }
        Ok(())
    }

    #[test]
    fn openai_mit_ungeteilter_eingabe_ohne_tagesblock_oder_am_deckel() -> Result<(), String> {
        let deckel = KiDeckel::standard(BeraterId::Maus);
        for frage in [
            "Blindprüfung: Beleg zur These",
            "Wochenrückschau\nTopflage:\nBeispiel",
            "Datum: 2030-01-07\nTopflage:\n   ",
        ] {
            let body = json::lies(&Anbieter::OpenAi.anfrage(&deckel, "Auftrag", frage))?;
            assert_eq!(body.feld("input").and_then(Json::text), Some(frage));
            assert!(body.feld("prompt_cache_options").is_none());
        }
        let grenze = usize::try_from(deckel.max_token_ein()).map_err(|e| e.to_string())?;
        let leer = Anbieter::OpenAi.anfrage(&deckel, "Auftrag", "");
        let lage = format!("Datum: 2030-01-07{TOPFBLOCK}Topf A: ");
        let json_lage = zitiere(&lage).len().checked_sub(2).ok_or("JSON-Text")?;
        let laenge = grenze
            .checked_sub(leer.len())
            .and_then(|n| n.checked_sub(json_lage))
            .ok_or("Eingabegrenze")?;
        let frage = format!("{lage}{}", "a".repeat(laenge));
        let anfrage = Anbieter::OpenAi.anfrage(&deckel, "Auftrag", &frage);
        assert_eq!(anfrage.len(), grenze);
        let body = json::lies(&anfrage)?;
        assert_eq!(
            body.feld("input").and_then(Json::text),
            Some(frage.as_str())
        );
        assert!(body.feld("prompt_cache_options").is_none());
        Ok(())
    }
}
