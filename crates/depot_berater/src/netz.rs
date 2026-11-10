//! Der Versand einer Frage ueber HTTPS. Nur `POST`, nur HTTPS, keine Weiterleitung, feste
//! Zeitgrenze und feste Hoechstgroesse der Antwort.

use std::time::Duration;

/// Hoechstgroesse einer Antwort. 16.000 Ausgabe-Token haben weit weniger.
pub const MAX_ANTWORT: u64 = 4 * 1024 * 1024;
/// Zeitgrenze eines Aufrufs vom Verbindungsaufbau bis zum letzten Byte. Mit Denken dauert eine
/// Antwort laenger als ein Kursabruf.
pub const ZEITGRENZE: Duration = Duration::from_secs(300);

/// Was beim Versand schiefging.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetzFehler {
    /// Die Anfrage hat den Anbieter sicher nicht erreicht (Name, Verbindung). Sie kostet nichts.
    NichtGesendet(String),
    /// Die Anfrage kann angekommen sein, die Antwort fehlt (Zeitgrenze, Abbruch, zu gross).
    /// Was sie gekostet hat, ist unbekannt.
    Unklar(String),
}

/// Eine Antwort mit Status und Inhalt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rueckmeldung {
    pub status: u16,
    pub inhalt: Vec<u8>,
    /// `Retry-After` in Sekunden, wenn der Anbieter ihn als Zahl nennt.
    pub warten: Option<u64>,
}

/// Sendet eine Anfrage. In Tests ersetzt ein erfundener Anbieter das Netz.
pub trait Netz {
    /// # Errors
    /// Siehe `NetzFehler`.
    fn sende(
        &mut self,
        adresse: &str,
        kopf: &[(&str, &str)],
        inhalt: &[u8],
    ) -> Result<Rueckmeldung, NetzFehler>;
}

/// Versand ueber HTTPS.
#[derive(Debug)]
pub struct Https(ureq::Agent);

impl Https {
    pub fn neu() -> Https {
        Self::mit_zeitgrenze(ZEITGRENZE)
    }

    pub(crate) fn mit_zeitgrenze(zeitgrenze: Duration) -> Https {
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .timeout_global(Some(zeitgrenze))
            .http_status_as_error(false)
            .user_agent("DepotSchlopf")
            .build();
        Https(config.into())
    }
}

impl Default for Https {
    fn default() -> Https {
        Https::neu()
    }
}

impl Netz for Https {
    fn sende(
        &mut self,
        adresse: &str,
        kopf: &[(&str, &str)],
        inhalt: &[u8],
    ) -> Result<Rueckmeldung, NetzFehler> {
        let mut anfrage = self.0.post(adresse);
        for (name, wert) in kopf {
            anfrage = anfrage.header(*name, *wert);
        }
        let mut antwort = anfrage.send(inhalt).map_err(|e| match e {
            ureq::Error::HostNotFound
            | ureq::Error::ConnectionFailed
            | ureq::Error::BadUri(_)
            | ureq::Error::RequireHttpsOnly(_)
            | ureq::Error::Timeout(ureq::Timeout::Resolve | ureq::Timeout::Connect) => {
                NetzFehler::NichtGesendet(depot_kern::geheimnis::maskiere(&e.to_string()))
            }
            ureq::Error::Io(ref io) if io.kind() == std::io::ErrorKind::ConnectionRefused => {
                NetzFehler::NichtGesendet(depot_kern::geheimnis::maskiere(&e.to_string()))
            }
            anders => NetzFehler::Unklar(depot_kern::geheimnis::maskiere(&anders.to_string())),
        })?;
        let status = antwort.status().as_u16();
        let warten = antwort
            .headers()
            .get("retry-after")
            .and_then(|w| w.to_str().ok())
            .and_then(|w| w.trim().parse::<u64>().ok())
            // Hoechstens einen Tag: Mehr waere keine Wartezeit, sondern ein Fehler.
            .filter(|w| *w <= 86_400);
        let inhalt = antwort
            .body_mut()
            .with_config()
            .limit(MAX_ANTWORT)
            .read_to_vec()
            .map_err(|e| NetzFehler::Unklar(depot_kern::geheimnis::maskiere(&e.to_string())))?;
        Ok(Rueckmeldung {
            status,
            inhalt,
            warten,
        })
    }
}
