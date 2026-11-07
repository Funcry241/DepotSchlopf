//! Der lesende Abruf ueber HTTPS. Nur `GET`, nur HTTPS, keine Weiterleitung, feste Zeitgrenze
//! und feste Hoechstgroesse der Antwort. Ein Status ausser 200 ist ein Fehler.
//!
//! **Kurze Netzausfaelle** (Maus, 05.11.2026): Scheitert ein Abruf voruebergehend, an der
//! Verbindung, am Namen, an der Zeitgrenze oder an einem Status 5xx, versucht
//! `hole_mit_wiederholung` es nach kurzer Wartezeit erneut. Nur so oft, wie `WARTEZEITEN`
//! erlaubt, und nur, solange ein spaeter Abruf noch innerhalb der Frist beginnt. Die Frist ist
//! das bestehende Frischefenster: Schluss der Boerse plus Kurs-Hoechstalter, abzueglich der
//! Zeitgrenze eines Abrufs (`frist`). Alles andere, etwa eine Ablehnung (4xx, auch 429), eine
//! zu grosse Antwort oder ein Zertifikatsfehler, wird nie wiederholt: Ein zweiter Versuch
//! aendert daran nichts und belastet nur das Tageskontingent.

use std::fmt;
use std::time::Duration;

use depot_kern::{Boerse, Datum, Grenzen, Zeitpunkt};

/// Hoechstgroesse einer Antwort. Die taegliche Kursreihe eines Werts hat rund 6 KB.
pub const MAX_ANTWORT: u64 = 1024 * 1024;
/// Zeitgrenze eines Abrufs vom Verbindungsaufbau bis zum letzten Byte.
pub const ZEITGRENZE: Duration = Duration::from_secs(30);
/// Wartezeit vor dem zweiten, dritten und vierten Versuch. Mehr Versuche gibt es nicht.
pub const WARTEZEITEN: [Duration; 3] = [
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(120),
];

/// Warum ein Abruf scheiterte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetzFehler {
    /// Ein spaeterer Versuch kann gelingen: Verbindung, Name, Zeitgrenze, Status 5xx.
    pub voruebergehend: bool,
    /// Die Meldung. Sie darf die Adresse enthalten; der Sammler bereinigt sie vor jeder Ausgabe
    /// vom Schluessel.
    pub text: String,
}

impl NetzFehler {
    pub fn voruebergehend(text: impl Into<String>) -> NetzFehler {
        NetzFehler {
            voruebergehend: true,
            text: depot_kern::geheimnis::maskiere(&text.into()),
        }
    }

    pub fn endgueltig(text: impl Into<String>) -> NetzFehler {
        NetzFehler {
            voruebergehend: false,
            text: depot_kern::geheimnis::maskiere(&text.into()),
        }
    }
}

impl fmt::Display for NetzFehler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// Holt die rohe Antwort einer Adresse. In Tests ersetzt eine erfundene Quelle das Netz.
pub trait Netz {
    /// # Errors
    /// Jeder Fehler beim Abruf, eingeordnet als voruebergehend oder nicht.
    fn hole(&mut self, adresse: &str) -> Result<Vec<u8>, NetzFehler>;
}

/// Abruf ueber HTTPS.
#[derive(Debug)]
pub struct Https(ureq::Agent);

impl Https {
    pub fn neu() -> Https {
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .timeout_global(Some(ZEITGRENZE))
            .http_status_as_error(true)
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

/// Ordnet einen Fehler von `ureq` ein. Voruebergehend ist nur, was am Weg liegt, nicht an der
/// Anfrage oder an der Quelle selbst.
fn einordnen(e: &ureq::Error) -> NetzFehler {
    let text = e.to_string();
    match e {
        ureq::Error::HostNotFound
        | ureq::Error::ConnectionFailed
        | ureq::Error::Io(_)
        | ureq::Error::Timeout(_)
        | ureq::Error::BodyStalled => NetzFehler::voruebergehend(text),
        ureq::Error::StatusCode(s) if (500..=599).contains(s) => NetzFehler::voruebergehend(text),
        _ => NetzFehler::endgueltig(text),
    }
}

impl Netz for Https {
    fn hole(&mut self, adresse: &str) -> Result<Vec<u8>, NetzFehler> {
        let mut antwort = self.0.get(adresse).call().map_err(|e| einordnen(&e))?;
        if antwort.status() != 200 {
            return Err(NetzFehler::endgueltig(format!(
                "Status {}",
                antwort.status()
            )));
        }
        antwort
            .body_mut()
            .with_config()
            .limit(MAX_ANTWORT)
            .read_to_vec()
            .map_err(|e| einordnen(&e))
    }
}

/// Die Frist fuer Wiederholungen eines Abrufs von Kursen dieser Boerse: Ein Versuch muss so
/// beginnen, dass sein Abruf auch nach der vollen Zeitgrenze noch im Frischefenster liegt,
/// also spaetestens `Schluss + Hoechstalter - ZEITGRENZE` des letzten Handelstags bis `jetzt`.
/// `None`, wenn in den letzten zehn Tagen kein Schluss lag oder die Rechnung ueberlaeuft: Dann
/// wird nicht wiederholt.
pub fn frist(boerse: Boerse, grenzen: &Grenzen, jetzt: Zeitpunkt) -> Option<Zeitpunkt> {
    let heute = jetzt.datum()?;
    let schluss = (0..10_i64)
        .filter_map(|n| heute.plus_tage(n.checked_neg()?))
        .filter(|d: &Datum| boerse.handelstag(*d))
        .filter_map(|d| boerse.schluss(d))
        .find(|s| *s <= jetzt)?;
    let rest = grenzen
        .kurs_hoechstalter_s()
        .checked_sub(i64::try_from(ZEITGRENZE.as_secs()).ok()?)?;
    Zeitpunkt::aus_sekunden(schluss.sekunden().checked_add(rest)?)
}

/// Was waehrend der Wiederholungen geschieht, fuer Ausgabe und Oberflaeche.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verlauf {
    /// Verbindung weg; der naechste Versuch beginnt um `um`. `versuch` ist dessen Nummer.
    Wartet {
        um: Zeitpunkt,
        versuch: usize,
        von: usize,
        fehler: String,
    },
    /// Nach einem gescheiterten Versuch gelang der Abruf wieder.
    WiederDa { versuch: usize },
}

/// Warum ein Abruf endgueltig ohne Antwort blieb.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ausfall {
    /// Der Fehler ist nicht voruebergehend; nicht wiederholt.
    Endgueltig(String),
    /// Alle Versuche gescheitert.
    KeineVersucheMehr { versuche: usize, fehler: String },
    /// Ein weiterer Versuch begaenne nach der Frist: Seine Kurse waeren zu alt.
    FristVorbei {
        frist: Zeitpunkt,
        versuche: usize,
        fehler: String,
    },
}

impl Ausfall {
    /// Die letzte Meldung des Netzes.
    pub fn fehler(&self) -> &str {
        match self {
            Ausfall::Endgueltig(f)
            | Ausfall::KeineVersucheMehr { fehler: f, .. }
            | Ausfall::FristVorbei { fehler: f, .. } => f,
        }
    }

    /// Lag es an der Verbindung? Dann sind weitere Abrufe dieses Laufs zwecklos.
    pub fn verbindung(&self) -> bool {
        !matches!(self, Ausfall::Endgueltig(_))
    }
}

/// Uhr, Warten und Meldung fuer `hole_mit_wiederholung`. In Tests laeuft die Uhr mit dem Warten
/// mit, ohne echt zu schlafen.
pub struct Takt<'a> {
    pub uhr: &'a mut dyn FnMut() -> Option<Zeitpunkt>,
    pub schlafe: &'a mut dyn FnMut(Duration),
    pub melde: &'a mut dyn FnMut(&Verlauf),
}

impl fmt::Debug for Takt<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Takt").finish_non_exhaustive()
    }
}

/// Holt eine Adresse; scheitert der Abruf voruebergehend, nach `WARTEZEITEN` erneut, solange
/// der naechste Versuch spaetestens zur Frist beginnt. Die Frist rechnet `frist_ab` aus der Uhr
/// beim ersten Scheitern; `None` heisst: nicht wiederholen. Die Uhr wird nur nach einem
/// Scheitern gefragt, und noch einmal unmittelbar vor jedem weiteren Versuch: Liegt er nach der
/// Frist, unterbleibt er.
///
/// # Errors
/// Siehe `Ausfall`.
pub fn hole_mit_wiederholung(
    netz: &mut dyn Netz,
    adresse: &str,
    frist_ab: &dyn Fn(Zeitpunkt) -> Option<Zeitpunkt>,
    takt: &mut Takt<'_>,
) -> Result<Vec<u8>, Ausfall> {
    let von = WARTEZEITEN.len().saturating_add(1);
    let mut versuch = 1_usize;
    let mut warten = WARTEZEITEN.iter();
    let mut frist: Option<Option<Zeitpunkt>> = None;
    loop {
        let fehler = match netz.hole(adresse) {
            Ok(roh) => {
                if versuch > 1 {
                    (takt.melde)(&Verlauf::WiederDa { versuch });
                }
                return Ok(roh);
            }
            Err(e) if !e.voruebergehend => return Err(Ausfall::Endgueltig(e.text)),
            Err(e) => e.text,
        };
        let Some(pause) = warten.next() else {
            return Err(Ausfall::KeineVersucheMehr {
                versuche: versuch,
                fehler,
            });
        };
        let jetzt = (takt.uhr)();
        let frist = *frist.get_or_insert_with(|| jetzt.and_then(frist_ab));
        let Some(frist) = frist else {
            return Err(Ausfall::KeineVersucheMehr {
                versuche: versuch,
                fehler,
            });
        };
        let um = jetzt
            .and_then(|j| {
                Zeitpunkt::aus_sekunden(
                    j.sekunden()
                        .checked_add(i64::try_from(pause.as_secs()).ok()?)?,
                )
            })
            .filter(|um| *um <= frist);
        let Some(um) = um else {
            return Err(Ausfall::FristVorbei {
                frist,
                versuche: versuch,
                fehler,
            });
        };
        (takt.melde)(&Verlauf::Wartet {
            um,
            versuch: versuch.saturating_add(1),
            von,
            fehler: fehler.clone(),
        });
        (takt.schlafe)(*pause);
        // Unmittelbar vor dem Versuch noch einmal die Uhr: Das Warten kann laenger gedauert
        // haben als geplant (Ruhezustand, Uhr gestellt). Nach der Frist geht nichts mehr hinaus
        // (Maus, Befund 2 zu #38).
        if (takt.uhr)().is_none_or(|j| j > frist) {
            return Err(Ausfall::FristVorbei {
                frist,
                versuche: versuch,
                fehler,
            });
        }
        versuch = versuch.saturating_add(1);
    }
}

/// Der Hinweis zu einer Wiederholung in Alltagssprache (`depot_kern::hinweis`).
pub fn verlauf_text(v: &Verlauf) -> String {
    match v {
        Verlauf::Wartet {
            um, versuch, von, ..
        } => format!(
            "Verbindung weg. Nächster Versuch um {} (Versuch {versuch} von {von}). Der Abruf \
             versucht es von selbst noch einmal, du musst nichts tun.",
            depot_kern::hinweis::uhr_text(*um).unwrap_or_else(|| "gleich".into())
        ),
        Verlauf::WiederDa { versuch } => {
            format!("Verbindung wieder da (Versuch {versuch}). Der Abruf läuft weiter.")
        }
    }
}

/// Der Hinweis, wenn die Verbindung trotz Wiederholung wegblieb. `verwendbar`: ob die Kurse des
/// Laufs trotzdem reichen (nur Kandidaten fehlen).
pub fn ausfall_text(a: &Ausfall, verwendbar: bool, grenzen: &Grenzen) -> String {
    let minuten = grenzen.kurs_hoechstalter_s().checked_div(60).unwrap_or(0);
    let grund = match a {
        Ausfall::FristVorbei { frist, .. } => format!(
            "Die Verbindung kam bis {} nicht wieder. Später geholte Schlusskurse wären älter als \
             {minuten} Minuten",
            depot_kern::hinweis::uhr_text(*frist).unwrap_or_else(|| "zum Ende der Frist".into())
        ),
        Ausfall::KeineVersucheMehr { versuche, .. } => {
            format!("Die Verbindung blieb nach {versuche} Versuchen weg")
        }
        Ausfall::Endgueltig(_) => "Der Abruf wurde abgelehnt".into(),
    };
    // Nur, was aus diesem Lauf folgt. Ob und wann es von selbst weitergeht, weiss nur der
    // Dienst aus seinem Zeitplan; er sagt es dazu (Maus, Befund 3 zu #38).
    if verwendbar {
        format!(
            "{grund}. Nicht alle Kandidaten sind abgerufen; ein Papiertag mit diesen Kursen \
             läuft ohne sie."
        )
    } else {
        format!(
            "Papiertag heute ausgefallen. {grund}. Mit den Kursen dieses Laufs kann heute kein \
             Papiertag laufen. Bitte die Internetverbindung prüfen."
        )
    }
}
