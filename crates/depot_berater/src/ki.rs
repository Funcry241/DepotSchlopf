//! Dachs und Maus als Berater: eine Frage, ein Aufruf, eine Kostenpruefung danach.
//!
//! Vor jedem Aufruf hat das Journal die Hoechstkosten gebucht (`AufrufErlaubnis`); die Anfrage
//! ist vorher auf `max_token_ein` Bytes begrenzt, die Antwort auf `max_token_aus`, und es gibt
//! keine Werkzeuge des Anbieters. Nach jedem
//! Aufruf rechnet der Berater aus dem Verbrauch, den der Anbieter meldet, die tatsaechlichen
//! Kosten. Liegen sie ueber der Reservierung, gab es eine Suche, sind mehr Token
//! verbraucht als erlaubt, oder ist der Verbrauch unbekannt, schreibt er die Sperrdatei
//! (`daten/ki.halt`). Solange sie liegt, ist keine KI angebunden: Niemand wird gefragt, bis
//! Wolfgang den Grund geprueft und die Datei geloescht hat.

use std::fmt;
use std::fs;
use std::path::PathBuf;

use depot_journal::AufrufErlaubnis;
use depot_journal::anbieter::{self, Eintrag, Ergebnis as Ausgang, Kosten};
use depot_kern::{BeraterId, Cent, Grenzen, Universum, Vorschlag, Zeitpunkt};
use depot_sim::{Antwort, Berater, KEINE_VERWERTBARE_ANTWORT, Lage, Urteil};

use crate::anbieter::{Anbieter, Verbrauch};
use crate::frage::{self, sauber};
use crate::netz::{Netz, NetzFehler};
use crate::schluessel::Schluessel;

/// Pfad des Anbieterprotokolls und die Uhr fuer seine Eintraege.
type Protokoll = (PathBuf, fn() -> Option<Zeitpunkt>);

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
    /// Anbieterprotokoll und Uhr; ohne beides wird nichts vermerkt.
    protokoll: Option<Protokoll>,
    /// Der Anbieter hat in diesem Lauf ein Limit gemeldet; bis zum Ende des Laufs pausiert,
    /// auch wenn das Protokoll nicht geschrieben werden konnte.
    limit_im_lauf: bool,
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
            protokoll: None,
            limit_im_lauf: false,
        }
    }

    /// Vermerkt jeden Aufruf in `pfad` (`daten/anbieter.txt`): Ergebnis, Kosten, Wartezeit.
    #[must_use]
    pub fn mit_protokoll(mut self, pfad: PathBuf, uhr: fn() -> Option<Zeitpunkt>) -> Self {
        self.protokoll = Some((pfad, uhr));
        self
    }

    /// Haengt einen Eintrag an das Protokoll. Ein Schreibfehler aendert nichts am Aufruf; das
    /// Protokoll dient nur der Anzeige.
    fn vermerke(
        &self,
        ergebnis: Ausgang,
        http: Option<u16>,
        ist: Kosten,
        reserviert: Cent,
        warten: Option<u64>,
    ) {
        let Some((pfad, uhr)) = &self.protokoll else {
            return;
        };
        let Some(zeit) = uhr() else {
            return;
        };
        let wer = self.protokoll_wer();
        let warten_bis = warten
            .and_then(|w| i64::try_from(w).ok())
            .and_then(|w| zeit.sekunden().checked_add(w))
            .and_then(Zeitpunkt::aus_sekunden);
        let e = Eintrag {
            ist,
            reserviert: Some(reserviert),
            warten_bis,
            ..Eintrag::neu(zeit, wer, ergebnis, http)
        };
        if let Err(f) = anbieter::anhaengen(pfad, &e) {
            eprintln!("[Berater] Anbieterprotokoll nicht geschrieben: {f}");
        }
    }

    /// Das Netz, fuer Tests mit einem erfundenen Anbieter.
    pub fn netz(&self) -> &N {
        &self.netz
    }

    /// Muss dieser Berater gerade pausieren? Ja, wenn der Anbieter ein Limit gemeldet hat, das
    /// nach `depot_journal::anbieter::pausiert` noch gilt, in diesem Lauf ein Limit kam, oder
    /// das Protokoll nicht lesbar ist (dann im Zweifel kein Aufruf). Ohne Protokoll: nein.
    pub fn pause(&self) -> Option<String> {
        if self.limit_im_lauf {
            return Some("Anbieterlimit in diesem Lauf".into());
        }
        let (pfad, uhr) = self.protokoll.as_ref()?;
        let Some(jetzt) = uhr() else {
            return Some("Uhr liefert keine Zeit".into());
        };
        let text = match fs::read_to_string(pfad) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => return Some(format!("{} nicht lesbar: {e}", pfad.display())),
        };
        let (eintraege, _) = anbieter::lies_alle(&text);
        anbieter::pausiert(&eintraege, self.protokoll_wer(), jetzt)
            .then(|| "Anbieterlimit, Wartezeit laeuft noch".to_string())
    }

    fn protokoll_wer(&self) -> anbieter::Wer {
        match self.wer {
            BeraterId::Dachs => anbieter::Wer::Dachs,
            BeraterId::Maus => anbieter::Wer::Maus,
        }
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

    /// Eine Ablehnung des Anbieters: einordnen, vermerken, ohne Schluessel beschreiben.
    fn ablehnung(&mut self, antwort: &crate::netz::Rueckmeldung, reserviert: Cent) -> String {
        let roh = String::from_utf8_lossy(&antwort.inhalt);
        let kurz: String = roh.chars().take(300).collect();
        let art = anbieter::einordnen(antwort.status, &kurz);
        if art == Ausgang::Limit {
            self.limit_im_lauf = true;
        }
        self.vermerke(
            art,
            Some(antwort.status),
            Kosten::Keine,
            reserviert,
            antwort.warten,
        );
        format!(
            "{}, Status {}: {}",
            ablehnung_text(art),
            antwort.status,
            sauber(&self.bereinigt(&kurz))
        )
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
        // Auch wer mit Erlaubnis fragt, wartet eine Pause des Anbieters ab (zweite Pruefung
        // direkt vor dem Senden; die erste ist `angebunden`, vor der Reservierung).
        if let Some(p) = self.pause() {
            return ohne(format!("pausiert: {p}"));
        }
        let deckel = *erlaubnis.deckel();
        let reserviert = erlaubnis.buchung().reserviert;
        // Die ganze Anfrage vor dem Senden begrenzen (K25-01). Ein Token umfasst mindestens ein
        // Byte, und ohne Werkzeuge fuegt der Anbieter nichts ein; die Anfrage enthaelt mehr
        // Bytes als ihre Rahmenzeichen Token. So sind die reservierten Hoechstkosten eine
        // gesicherte Obergrenze.
        let inhalt = self.anbieter.anfrage(&deckel, system, frage);
        if !erlaubnis.eingabe_passt(&inhalt) {
            return ohne("Anfrage ueber dem Deckel, nicht gesendet".into());
        }
        let mut bearer = String::new();
        let kopf = self.anbieter.kopf(schluessel.als_str(), &mut bearer);
        let r = self
            .netz
            .sende(self.anbieter.url(), &kopf, inhalt.as_bytes());
        let antwort = match r {
            Err(NetzFehler::NichtGesendet(e)) => {
                self.vermerke(Ausgang::Netz, None, Kosten::Keine, reserviert, None);
                return ohne(format!("nicht gesendet: {}", self.bereinigt(&e)));
            }
            Err(NetzFehler::Unklar(e)) => {
                self.vermerke(Ausgang::Unklar, None, Kosten::Unbekannt, reserviert, None);
                let e = self.bereinigt(&e);
                let m = self.sperre(&format!("Antwort fehlt, Kosten unbekannt: {e}"));
                return ohne(m);
            }
            Ok(a) => a,
        };
        if antwort.status != 200 {
            return ohne(self.ablehnung(&antwort, reserviert));
        }
        // Geheimnisschutz fuer jede Antwort, nicht nur fuer Fehler: Gibt der Anbieter den
        // Schluessel zurueck, wird kein Text davon verwendet. Geprueft wird hier roh und unten
        // am fertigen Ergebnis (`ohne_geheimnis`), denn JSON kann ihn verschluesselt enthalten.
        // Der Verbrauch wird trotzdem gelesen und geprueft: Kostenschutz geht vor.
        let geheim = self.enthaelt_schluessel(&String::from_utf8_lossy(&antwort.inhalt));
        let gelesen = match self.anbieter.lies(&antwort.inhalt) {
            Ok(g) => g,
            Err(e) => {
                self.vermerke(
                    Ausgang::Unklar,
                    Some(antwort.status),
                    Kosten::Unbekannt,
                    reserviert,
                    None,
                );
                let e = if geheim {
                    "verworfen".into()
                } else {
                    self.bereinigt(&e)
                };
                let m = self.sperre(&format!("Antwort unlesbar, Kosten unbekannt: {e}"));
                return ohne(m);
            }
        };
        let v = gelesen.verbrauch;
        let ist = deckel.kosten_ist(v.token_ein, v.token_aus);
        let kosten = kostenzeile(ist.map(|c| c.0), reserviert.0, v);
        let ueber = ueberschreitung(ist, reserviert, v, &deckel);
        let art = if ueber.is_some() {
            Ausgang::Kosten
        } else {
            Ausgang::Ok
        };
        self.vermerke(
            art,
            Some(antwort.status),
            ist.map_or(Kosten::Unbekannt, Kosten::Betrag),
            reserviert,
            None,
        );
        if let Some(grund) = ueber {
            let m = self.sperre(&format!("{grund}; {kosten}"));
            return Ergebnis {
                text: Err(m),
                kosten,
            };
        }
        if geheim {
            return Ergebnis {
                text: Err(GEHEIMNIS.into()),
                kosten,
            };
        }
        Ergebnis {
            text: gelesen.text.map_err(|e| self.bereinigt(&e)),
            kosten,
        }
    }
}

/// Was am tatsaechlichen Verbrauch ueber dem Erlaubten liegt; `None`, wenn alles passt.
fn ueberschreitung(
    ist: Option<Cent>,
    reserviert: Cent,
    v: Verbrauch,
    deckel: &depot_ki::KiDeckel,
) -> Option<String> {
    match ist {
        None => Some("Kosten nicht berechenbar".to_string()),
        Some(c) if c > reserviert => {
            Some(format!("{} Cent ueber {} reserviert", c.0, reserviert.0))
        }
        Some(_) if v.suchen > 0 => Some(format!("{} Suchen, keine erlaubt", v.suchen)),
        Some(_) if v.token_ein > u64::from(deckel.max_token_ein()) => Some(format!(
            "{} Eingabe-Token ueber {} erlaubt",
            v.token_ein,
            deckel.max_token_ein()
        )),
        Some(_) if v.token_aus > u64::from(deckel.max_token_aus()) => Some(format!(
            "{} Ausgabe-Token ueber {} erlaubt",
            v.token_aus,
            deckel.max_token_aus()
        )),
        Some(_) => None,
    }
}

/// Die Ablehnung in Worten fuer Journal und Protokoll.
fn ablehnung_text(e: Ausgang) -> &'static str {
    match e {
        Ausgang::Limit => "Anbieterlimit erreicht, naechster Versuch beim naechsten Papiertag",
        Ausgang::Guthaben => "Guthaben beim Anbieter fehlt",
        Ausgang::Abgelehnt => "Anbieter lehnt den Schluessel ab",
        Ausgang::Ueberlastet => "Anbieter ueberlastet oder gestoert",
        _ => "Anbieter lehnt ab",
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
        self.schluessel.is_some() && !self.gesperrt() && self.pause().is_none()
    }

    fn warum_nicht(&self) -> Option<String> {
        if self.schluessel.is_none() {
            return Some("kein Schlüssel hinterlegt".into());
        }
        if self.gesperrt() {
            return Some("Kostenschutz gesperrt, daten/ki.halt".into());
        }
        self.pause().map(|p| format!("Anbieterpause: {p}"))
    }

    fn vorschlagen(&mut self, erlaubnis: AufrufErlaubnis, lage: &Lage<'_>) -> Antwort {
        let system = frage::system(self.wer, &self.grenzen);
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
                verwertbar: false,
                einschaetzung: format!(
                    "{KEINE_VERWERTBARE_ANTWORT}: {} [{}]",
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
        let system = frage::system(self.wer, &self.grenzen);
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
                verwertbar: false,
                zustimmung: false,
                begruendung: format!(
                    "{KEINE_VERWERTBARE_ANTWORT}: {} [{}]",
                    sauber(&grund),
                    e.kosten
                ),
            },
        }
    }
}
