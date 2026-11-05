//! Die beiden Anbieter: Anfrage bauen, Antwort lesen. Kein Netz hier.
//!
//! - Dachs = Claude (Anthropic, Messages API) mit dem Werkzeug `web_search`.
//! - Maus = GPT (OpenAI, Responses API) mit dem Werkzeug `web_search`.
//!
//! Beide bekommen die Antwortgrenze `max_token_aus` und die Suchgrenze `max_suchen` aus dem
//! Deckel der Erlaubnis. Aus jeder Antwort wird der Verbrauch gelesen: Eingabe-Token (mit den
//! Suchergebnissen, die der Anbieter selbst einfuegt), Ausgabe-Token und Zahl der Suchen.

use depot_kern::BeraterId;
use depot_ki::KiDeckel;

use crate::json::{self, Json, zitiere};

/// Modell von Dachs. Preise im Deckel (`depot_ki`): 2 $ / 10 $ je Mio. Token.
pub const MODELL_DACHS: &str = "claude-sonnet-5-5";
/// Modell von Maus. Preise im Deckel (`depot_ki`): 2,50 $ / 15 $ je Mio. Token.
pub const MODELL_MAUS: &str = "gpt-5.4";

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
    pub suchen: u64,
}

/// Die gelesene Antwort: der Text des Modells, wenn es eine vollstaendige Antwort gab, und
/// immer der Verbrauch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gelesen {
    pub text: Result<String, String>,
    pub verbrauch: Verbrauch,
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
        match self {
            Anbieter::Anthropic => format!(
                concat!(
                    "{{\"model\":{},\"max_tokens\":{},",
                    "\"thinking\":{{\"type\":\"between_tools\"}},",
                    "\"output_config\":{{\"effort\":\"medium\"}},",
                    "\"system\":{},",
                    "\"tools\":[{{\"type\":\"web_search_20260209\",\"name\":\"web_search\",\"max_uses\":{}}}],",
                    "\"messages\":[{{\"role\":\"user\",\"content\":{}}}]}}"
                ),
                zitiere(self.modell()),
                deckel.max_token_aus(),
                zitiere(system),
                deckel.max_suchen(),
                zitiere(frage)
            ),
            Anbieter::OpenAi => format!(
                concat!(
                    "{{\"model\":{},\"max_output_tokens\":{},",
                    "\"reasoning\":{{\"effort\":\"low\"}},",
                    "\"instructions\":{},",
                    "\"tools\":[{{\"type\":\"web_search\"}}],\"max_tool_calls\":{},",
                    "\"store\":false,",
                    "\"input\":{}}}"
                ),
                zitiere(self.modell()),
                deckel.max_token_aus(),
                zitiere(system),
                deckel.max_suchen(),
                zitiere(frage)
            ),
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
        match self {
            Anbieter::Anthropic => lies_anthropic(&w),
            Anbieter::OpenAi => lies_openai(&w),
        }
    }
}

fn zahl(w: &Json, pfad: &[&str]) -> Option<u64> {
    let mut x = w;
    for p in pfad {
        x = x.feld(p)?;
    }
    x.anzahl()
}

fn lies_anthropic(w: &Json) -> Result<Gelesen, String> {
    let usage = w.feld("usage").ok_or("Antwort ohne Verbrauch")?;
    let ein = zahl(usage, &["input_tokens"]).ok_or("Verbrauch ohne input_tokens")?;
    // Zwischengespeicherte Eingaben zaehlen voll zum Eingabepreis: das ueberschaetzt nur.
    let cache = zahl(usage, &["cache_creation_input_tokens"])
        .unwrap_or(0)
        .saturating_add(zahl(usage, &["cache_read_input_tokens"]).unwrap_or(0));
    let aus = zahl(usage, &["output_tokens"]).ok_or("Verbrauch ohne output_tokens")?;
    let suchen = zahl(usage, &["server_tool_use", "web_search_requests"]).unwrap_or(0);
    let verbrauch = Verbrauch {
        token_ein: ein.saturating_add(cache),
        token_aus: aus,
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
        Err(format!("Antwort unvollstaendig: stop_reason {grund:?}"))
    };
    Ok(Gelesen { text, verbrauch })
}

fn lies_openai(w: &Json) -> Result<Gelesen, String> {
    let usage = w.feld("usage").ok_or("Antwort ohne Verbrauch")?;
    let ein = zahl(usage, &["input_tokens"]).ok_or("Verbrauch ohne input_tokens")?;
    let aus = zahl(usage, &["output_tokens"]).ok_or("Verbrauch ohne output_tokens")?;
    let ausgabe = w.feld("output").and_then(Json::liste).unwrap_or(&[]);
    let suchen = ausgabe
        .iter()
        .filter(|x| x.feld("type").and_then(Json::text) == Some("web_search_call"))
        .count();
    let verbrauch = Verbrauch {
        token_ein: ein,
        token_aus: aus,
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
        Err(format!("Antwort unvollstaendig: status {status:?}"))
    };
    Ok(Gelesen { text, verbrauch })
}
