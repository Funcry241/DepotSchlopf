//! Dachs und Maus als Berater: eine Frage, ein Aufruf, eine Kostenpruefung danach.
//!
//! Vor jedem Aufruf hat das Journal die Hoechstkosten gebucht (`AufrufErlaubnis`). Nach jedem
//! Aufruf rechnet der Berater aus dem Verbrauch, den der Anbieter meldet, die tatsaechlichen
//! Kosten. Liegen sie ueber der Reservierung, sind mehr Suchen oder mehr Ausgabe-Token
//! verbraucht als erlaubt, oder ist der Verbrauch unbekannt, schreibt er die Sperrdatei
//! (`daten/ki.halt`). Solange sie liegt, ist keine KI angebunden: Niemand wird gefragt, bis
//! Wolfgang den Grund geprueft und die Datei geloescht hat.

use std::fmt;
use std::fs;
use std::path::PathBuf;

use depot_journal::AufrufErlaubnis;
use depot_kern::{BeraterId, Grenzen, Universum, Vorschlag};
use depot_sim::{Antwort, Berater, Lage, Urteil};

use crate::anbieter::{Anbieter, Verbrauch};
use crate::frage::{self, sauber};
use crate::netz::{Netz, NetzFehler};
use crate::schluessel::Schluessel;

/// Dachs oder Maus mit Anbindung an den Anbieter.
pub struct KiBerater<N: Netz> {
    wer: BeraterId,
    anbieter: Anbieter,
    schluessel: Option<Schluessel>,
    netz: N,
    universum: Universum,
    grenzen: Grenzen,
    sperre: PathBuf,
    gesperrt: bool,
}

impl<N: Netz> fmt::Debug for KiBerater<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KiBerater")
            .field("wer", &self.wer)
            .field("anbieter", &self.anbieter)
            .field("schluessel", &self.schluessel)
            .field("sperre", &self.sperre)
            .field("gesperrt", &self.gesperrt)
            .finish_non_exhaustive()
    }
}

/// Was ein Aufruf ergeben hat: der Text des Modells und eine Zeile ueber die Kosten.
struct Ergebnis {
    text: Result<String, String>,
    kosten: String,
}

impl<N: Netz> KiBerater<N> {
    /// Ohne Schluessel ist der Berater nicht angebunden und wird nie gefragt.
    pub fn neu(
        wer: BeraterId,
        schluessel: Option<Schluessel>,
        netz: N,
        universum: Universum,
        sperre: PathBuf,
    ) -> KiBerater<N> {
        KiBerater {
            wer,
            anbieter: Anbieter::fuer(wer),
            schluessel,
            netz,
            universum,
            grenzen: Grenzen::standard(),
            sperre,
            gesperrt: false,
        }
    }

    /// Das Netz, fuer Tests mit einem erfundenen Anbieter.
    pub fn netz(&self) -> &N {
        &self.netz
    }

    /// Liegt die Sperrdatei, oder hat dieser Lauf gesperrt?
    pub fn gesperrt(&self) -> bool {
        self.gesperrt || self.sperre.exists()
    }

    /// Steht der eigene Schluessel in diesem Text?
    fn enthaelt_schluessel(&self, text: &str) -> bool {
        self.schluessel
            .as_ref()
            .is_some_and(|s| text.contains(s.als_str()))
    }

    fn bereinigt(&self, text: &str) -> String {
        self.schluessel
            .as_ref()
            .map_or_else(|| text.to_string(), |s| s.bereinige(text))
    }

    /// Sperrt alle KIs: Datei schreiben und im Lauf merken. Misslingt das Schreiben, bleibt
    /// die Sperre im Lauf, und die Meldung sagt es.
    fn sperre(&mut self, grund: &str) -> String {
        self.gesperrt = true;
        let text = format!(
            "{:?} am Kostenschutz gesperrt: {grund}\nPruefen, dann diese Datei loeschen.\n",
            self.wer
        );
        match fs::write(&self.sperre, text) {
            Ok(()) => format!("KI gesperrt ({}): {grund}", self.sperre.display()),
            Err(e) => format!(
                "KI gesperrt, Sperrdatei {} nicht geschrieben ({e}): {grund}",
                self.sperre.display()
            ),
        }
    }

    fn fragen(&mut self, erlaubnis: &AufrufErlaubnis, system: &str, frage: &str) -> Ergebnis {
        let ohne = |t: String| Ergebnis {
            text: Err(t),
            kosten: "keine Kosten".into(),
        };
        let Some(schluessel) = self.schluessel.clone() else {
            return ohne("kein Schluessel".into());
        };
        if self.gesperrt() {
            return ohne("KI gesperrt".into());
        }
        let deckel = *erlaubnis.deckel();
        // Eingabe vor dem Senden begrenzen: Anweisung und Frage zusammen.
        if !erlaubnis.eingabe_passt(&format!("{system}{frage}")) {
            return ohne("Eingabe ueber dem Deckel, nicht gesendet".into());
        }
        let inhalt = self.anbieter.anfrage(&deckel, system, frage);
        let mut bearer = String::new();
        let kopf = self.anbieter.kopf(schluessel.als_str(), &mut bearer);
        let r = self
            .netz
            .sende(self.anbieter.url(), &kopf, inhalt.as_bytes());
        let antwort = match r {
            Err(NetzFehler::NichtGesendet(e)) => {
                return ohne(format!("nicht gesendet: {}", self.bereinigt(&e)));
            }
            Err(NetzFehler::Unklar(e)) => {
                let e = self.bereinigt(&e);
                let m = self.sperre(&format!("Antwort fehlt, Kosten unbekannt: {e}"));
                return ohne(m);
            }
            Ok(a) => a,
        };
        if antwort.status != 200 {
            let roh = String::from_utf8_lossy(&antwort.inhalt);
            let kurz: String = roh.chars().take(300).collect();
            return ohne(format!(
                "Anbieter lehnt ab, Status {}: {}",
                antwort.status,
                sauber(&self.bereinigt(&kurz))
            ));
        }
        // Geheimnisschutz fuer jede Antwort, nicht nur fuer Fehler: Gibt der Anbieter den
        // Schluessel zurueck, wird nichts davon verwendet. Geprueft wird hier roh und unten am
        // fertigen Ergebnis (`ohne_geheimnis`), denn JSON kann ihn verschluesselt enthalten.
        if self.enthaelt_schluessel(&String::from_utf8_lossy(&antwort.inhalt)) {
            return Ergebnis {
                text: Err(GEHEIMNIS.into()),
                kosten: "Kosten nicht gelesen".into(),
            };
        }
        let gelesen = match self.anbieter.lies(&antwort.inhalt) {
            Ok(g) => g,
            Err(e) => {
                let m = self.sperre(&format!("Antwort unlesbar, Kosten unbekannt: {e}"));
                return ohne(m);
            }
        };
        let v = gelesen.verbrauch;
        let reserviert = erlaubnis.buchung().reserviert;
        let ist = deckel.kosten_ist(v.token_ein, v.token_aus, v.suchen);
        let kosten = kostenzeile(ist.map(|c| c.0), reserviert.0, v);
        let ueber = match ist {
            None => Some("Kosten nicht berechenbar".to_string()),
            Some(c) if c > reserviert => {
                Some(format!("{} Cent ueber {} reserviert", c.0, reserviert.0))
            }
            Some(_) if v.suchen > u64::from(deckel.max_suchen()) => Some(format!(
                "{} Suchen ueber {} erlaubt",
                v.suchen,
                deckel.max_suchen()
            )),
            Some(_) if v.token_aus > u64::from(deckel.max_token_aus()) => Some(format!(
                "{} Ausgabe-Token ueber {} erlaubt",
                v.token_aus,
                deckel.max_token_aus()
            )),
            Some(_) => None,
        };
        if let Some(grund) = ueber {
            let m = self.sperre(&format!("{grund}; {kosten}"));
            return Ergebnis {
                text: Err(m),
                kosten,
            };
        }
        Ergebnis {
            text: gelesen.text.map_err(|e| self.bereinigt(&e)),
            kosten,
        }
    }
}

/// Meldung, wenn eine Antwort den Schluessel enthaelt. Sie nennt ihn nicht.
const GEHEIMNIS: &str = "Antwort enthielt den Schluessel des Anbieters; verworfen";

fn kostenzeile(ist: Option<i64>, reserviert: i64, v: Verbrauch) -> String {
    let ist = ist.map_or_else(|| "?".into(), |c| c.to_string());
    format!(
        "Kosten {ist} von {reserviert} Cent reserviert, {} Token rein, {} raus, {} Suchen",
        v.token_ein, v.token_aus, v.suchen
    )
}

impl<N: Netz> KiBerater<N> {
    /// Jeder Text, der aus einer Antwort ins Journal geht, ohne Schluessel; sonst verworfen.
    fn ohne_geheimnis_antwort(&self, a: Antwort) -> Result<Antwort, String> {
        let mut texte = std::iter::once(&a.einschaetzung)
            .chain(a.vorschlag.iter().flat_map(|v| v.begruendung.values()));
        if texte.any(|t| self.enthaelt_schluessel(t)) {
            return Err(GEHEIMNIS.into());
        }
        Ok(a)
    }

    fn ohne_geheimnis_urteil(&self, u: Urteil) -> Result<Urteil, String> {
        if self.enthaelt_schluessel(&u.begruendung) {
            return Err(GEHEIMNIS.into());
        }
        Ok(u)
    }
}

impl<N: Netz> Berater for KiBerater<N> {
    fn angebunden(&self) -> bool {
        self.schluessel.is_some() && !self.gesperrt()
    }

    fn vorschlagen(&mut self, erlaubnis: AufrufErlaubnis, lage: &Lage<'_>) -> Antwort {
        let system = frage::system(self.wer, &self.grenzen, erlaubnis.deckel().max_suchen());
        let f = frage::vorschlag_frage(lage, &self.universum);
        let e = self.fragen(&erlaubnis, &system, &f);
        match e
            .text
            .and_then(|t| frage::lies_vorschlag(&t, self.wer, &self.universum))
            .and_then(|a| self.ohne_geheimnis_antwort(a))
        {
            Ok(mut a) => {
                a.einschaetzung = format!("{} [{}]", a.einschaetzung, e.kosten);
                a
            }
            Err(grund) => Antwort {
                einschaetzung: format!(
                    "keine verwertbare Antwort: {} [{}]",
                    sauber(&grund),
                    e.kosten
                ),
                vorschlag: None,
            },
        }
    }

    fn pruefen(
        &mut self,
        erlaubnis: AufrufErlaubnis,
        lage: &Lage<'_>,
        vorschlag: &Vorschlag,
    ) -> Urteil {
        let system = frage::system(self.wer, &self.grenzen, erlaubnis.deckel().max_suchen());
        let f = frage::urteil_frage(lage, &self.universum, vorschlag);
        let e = self.fragen(&erlaubnis, &system, &f);
        match e
            .text
            .and_then(|t| frage::lies_urteil(&t))
            .and_then(|u| self.ohne_geheimnis_urteil(u))
        {
            Ok(mut u) => {
                u.begruendung = format!("{} [{}]", u.begruendung, e.kosten);
                u
            }
            // Ohne verwertbare Antwort keine Zustimmung (R31).
            Err(grund) => Urteil {
                zustimmung: false,
                begruendung: format!(
                    "keine verwertbare Antwort: {} [{}]",
                    sauber(&grund),
                    e.kosten
                ),
            },
        }
    }
}
