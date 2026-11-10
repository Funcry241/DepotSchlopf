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
//!
//! **Scheitert der Versand** (Maus, 05.11.2026), zaehlt allein, ob die Anfrage den Anbieter
//! erreicht haben kann (`NetzFehler`):
//! - Sicher nicht gesendet (Name, Verbindungsaufbau): Sie kostet nichts. Der Berater versucht es
//!   nach `WARTEZEITEN` erneut, unter derselben Reservierung. So erreicht hoechstens eine
//!   Anfrage je Reservierung den Anbieter, und die gebuchten Hoechstkosten bleiben die Grenze.
//! - Vielleicht angekommen (Abbruch nach dem Absenden, Zeitgrenze): Was sie gekostet hat, ist
//!   unbekannt. Nie wiederholen: Ein zweiter Versuch koennte doppelt kosten. Die Sperre
//!   `daten/ki.halt` greift, bis Wolfgang die Kosten beim Anbieter geprueft hat.
//!
//! Beides steht als Hinweiszeile (`depot_kern::hinweis`) in Alltagssprache in der Ausgabe.

use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use depot_journal::anbieter::{self, Eintrag, Ergebnis as Ausgang, Kosten};
use depot_journal::{
    AufrufErlaubnis, KiZweck,
    gespraeche::{self, Gespraech},
};
use depot_kern::{BeraterId, Cent, Grenzen, Universum, Vorschlag, Zeitpunkt};
use depot_sim::{Antwort, Berater, KEINE_VERWERTBARE_ANTWORT, Lage, Urteil};

use crate::anbieter::{Anbieter, Verbrauch};
use crate::frage::{self, sauber};
use crate::netz::{Netz, NetzFehler};
use crate::schluessel::Schluessel;

/// Wartezeit vor dem zweiten und dritten Versand einer sicher nicht gesendeten Anfrage. Mehr
/// Versuche gibt es nicht.
pub const WARTEZEITEN: [Duration; 2] = [Duration::from_secs(30), Duration::from_secs(60)];

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
    gespraechslauf: Option<String>,
    weiterer_schluessel: Option<Schluessel>,
    /// Der Anbieter hat in diesem Lauf ein Limit gemeldet; bis zum Ende des Laufs pausiert,
    /// auch wenn das Protokoll nicht geschrieben werden konnte.
    limit_im_lauf: bool,
    /// Wartet vor einem neuen Versand; in Tests ohne echtes Warten.
    schlafe: fn(Duration),
    vorbereitung: Option<Zeitpunkt>,
    abrechnung: Option<depot_journal::KiAbrechnung>,
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
            gespraechslauf: None,
            weiterer_schluessel: None,
            limit_im_lauf: false,
            schlafe: std::thread::sleep,
            vorbereitung: None,
            abrechnung: None,
        }
    }

    /// Derselbe Kostenschutz, aber ein reiner Beobachtungsauftrag ohne Handelsziele.
    #[must_use]
    pub fn fuer_vorbereitung(mut self, zeit: Zeitpunkt) -> Self {
        self.vorbereitung = Some(zeit);
        self
    }

    /// Wartet mit `schlafe` statt echt (fuer Tests).
    #[must_use]
    pub fn mit_warten(mut self, schlafe: fn(Duration)) -> Self {
        self.schlafe = schlafe;
        self
    }

    fn name(&self) -> &'static str {
        match self.wer {
            BeraterId::Dachs => "Dachs",
            BeraterId::Maus => "Maus",
        }
    }

    /// Gibt eine Hinweiszeile fuer die Oberflaeche aus.
    fn hinweis(&self, text: &str) {
        println!(
            "{}",
            depot_kern::hinweis::zeile(&format!("{}: {text}", self.name()))
        );
    }

    /// Wann der naechste Versuch beginnt, als Uhrzeit, wenn die Uhr des Protokolls sie kennt.
    fn um(&self, pause: Duration) -> String {
        self.protokoll
            .as_ref()
            .and_then(|(_, uhr)| uhr())
            .and_then(|j| {
                Zeitpunkt::aus_sekunden(
                    j.sekunden()
                        .checked_add(i64::try_from(pause.as_secs()).ok()?)?,
                )
            })
            .and_then(depot_kern::hinweis::uhr_text)
            .map_or_else(
                || format!("in {} Sekunden", pause.as_secs()),
                |u| format!("um {u}"),
            )
    }

    /// Vermerkt jeden Aufruf in `pfad` (`daten/anbieter.txt`): Ergebnis, Kosten, Wartezeit.
    #[must_use]
    pub fn mit_protokoll(mut self, pfad: PathBuf, uhr: fn() -> Option<Zeitpunkt>) -> Self {
        self.protokoll = Some((pfad, uhr));
        self
    }

    #[must_use]
    pub fn mit_gespraechen(
        mut self,
        lauf: String,
        weiterer_schluessel: Option<Schluessel>,
    ) -> Self {
        self.gespraechslauf = Some(lauf);
        self.weiterer_schluessel = weiterer_schluessel;
        self
    }

    fn gespraech(&self, zweck: KiZweck, zeit: Option<Zeitpunkt>) -> Option<Gespraech> {
        Some(Gespraech {
            zeit: zeit?,
            lauf: self.gespraechslauf.clone()?,
            wer: self.protokoll_wer().kurz().into(),
            rolle: "haupt".into(),
            modell: self.anbieter.modell().into(),
            zweck: zweck.text().into(),
            ergebnis: "uebersprungen".into(),
            http: None,
            token_ein: None,
            token_aus: None,
            buchung: None,
            kosten_cent: None,
            system: None,
            frage: None,
            antwort: None,
            fehler: None,
        })
    }

    fn gespraechszeit(&self) -> Option<Zeitpunkt> {
        self.protokoll.as_ref().and_then(|(_, uhr)| uhr())
    }

    fn geheim_im_gespraech(&self, text: &str) -> bool {
        self.enthaelt_schluessel(text)
            || self.bereinigt(text) != text
            || self
                .weiterer_schluessel
                .as_ref()
                .is_some_and(|s| s.bereinige(text) != text)
    }

    fn notiere_gespraech(&self, mut e: Gespraech, geheim: bool) {
        let Some(daten) = self.protokoll.as_ref().and_then(|(p, _)| p.parent()) else {
            return;
        };
        if geheim
            || [&e.system, &e.frage, &e.antwort, &e.fehler]
                .into_iter()
                .flatten()
                .any(|text| self.geheim_im_gespraech(text))
        {
            e.system = None;
            e.frage = None;
            e.antwort = None;
            e.fehler = Some("Gespräch wegen Zugangsdaten verworfen.".into());
        }
        if self.geheim_im_gespraech(&e.modell) {
            e.modell = self.anbieter.modell().into();
        }
        if let Err(f) = gespraeche::anhaengen(daten, &e) {
            self.hinweis(&format!("Gespräch nicht gespeichert: {f}"));
        }
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
            .then(|| "Anbieterlimit, Wartezeit läuft noch".to_string())
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

    /// Steht der eigene Schluessel in diesem Text, roh oder hinter JSON-Escapes, auch
    /// geschachtelt (`json::alle_texte`, K28-01)?
    fn enthaelt_schluessel(&self, text: &str) -> bool {
        self.schluessel.as_ref().is_some_and(|s| {
            crate::json::alle_texte(text)
                .iter()
                .any(|t| t.contains(s.als_str()))
        })
    }

    /// Ein Text fuer Meldung und Journal: ohne Schluessel. Steht er nur verborgen darin, etwa
    /// hinter Escapes, gilt der ganze Text als Geheimnis.
    fn bereinigt(&self, text: &str) -> String {
        let roh = self.schluessel.as_ref().map_or_else(
            || depot_kern::geheimnis::maskiere(text),
            |s| s.bereinige(text),
        );
        if self.enthaelt_schluessel(&roh) {
            return GEHEIMNIS.into();
        }
        roh
    }

    /// Sperrt alle KIs: Datei schreiben und im Lauf merken. Misslingt das Schreiben, bleibt
    /// die Sperre im Lauf, und die Meldung sagt es.
    fn sperre(&mut self, grund: &str) -> String {
        self.gesperrt = true;
        let text = format!(
            "{:?} am Kostenschutz gesperrt: {grund}\nPrüfen, dann diese Datei löschen.\n",
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

    /// Sendet die Anfrage; eine sicher nicht gesendete nach `WARTEZEITEN` erneut, unter
    /// derselben Reservierung. Eine vielleicht angekommene nie: Dann sperrt der Berater.
    ///
    /// # Errors
    /// Die Meldung fuer den Aufruf: nicht gesendet, oder gesperrt mit Grund.
    fn sende(
        &mut self,
        kopf: &[(&str, &str)],
        inhalt: &str,
        reserviert: Cent,
    ) -> Result<crate::netz::Rueckmeldung, (Option<Ausgang>, String)> {
        let von = WARTEZEITEN.len().saturating_add(1);
        let mut warten = WARTEZEITEN.iter();
        let mut versuch = 1_usize;
        loop {
            if let Some(start) = self.vorbereitung {
                let jetzt = self.protokoll.as_ref().and_then(|(_, uhr)| uhr());
                if jetzt.is_none_or(|j| {
                    j < start
                        || j.datum() != start.datum()
                        || depot_sim::vorbereitung::tag_jetzt(j).is_none()
                }) {
                    return Err((None, "nicht gesendet: Zeitfenster der Vorbereitung abgelaufen oder Rechneruhr zurückgestellt".into()));
                }
            }
            match self
                .netz
                .sende(self.anbieter.url(), kopf, inhalt.as_bytes())
            {
                Ok(a) => return Ok(a),
                Err(NetzFehler::NichtGesendet(e)) => {
                    self.vermerke(Ausgang::Netz, None, Kosten::Keine, reserviert, None);
                    let e = self.bereinigt(&e);
                    // Vor einem neuen Versand noch einmal: keine Sperre, keine Pause.
                    let frei = !self.gesperrt() && self.pause().is_none();
                    let Some(pause) = warten.next().filter(|_| frei) else {
                        self.hinweis(&format!(
 "Anfrage hat den Anbieter auch nach {versuch} Versuchen nicht erreicht (keine Verbindung). Sie hat nichts gekostet. Dieser Papiertag läuft ohne diese KI weiter."
 ));
                        return Err((Some(Ausgang::Netz), format!("nicht gesendet: {e}")));
                    };
                    versuch = versuch.saturating_add(1);
                    self.hinweis(&format!(
 "Anfrage hat den Anbieter nicht erreicht (keine Verbindung) und nichts gekostet. Nächster Versuch {} (Versuch {versuch} von {von}). DepotSchlopf macht von selbst weiter, du musst nichts tun.",
 self.um(*pause)
 ));
                    (self.schlafe)(*pause);
                    if self.gesperrt() {
                        self.hinweis("Kein weiterer Versuch: Der Kostenschutz ist gesperrt.");
                        return Err((Some(Ausgang::Netz), "nicht gesendet: KI gesperrt".into()));
                    }
                    if let Some(p) = self.pause() {
                        self.hinweis("Kein weiterer Versuch: Der Anbieter pausiert.");
                        return Err((
                            Some(Ausgang::Netz),
                            format!("nicht gesendet: pausiert: {p}"),
                        ));
                    }
                }
                Err(NetzFehler::Unklar(e)) => {
                    self.vermerke(Ausgang::Unklar, None, Kosten::Unbekannt, reserviert, None);
                    let e = self.bereinigt(&e);
                    self.hinweis(
 "Verbindung nach dem Absenden abgebrochen. Die Anfrage kann beim Anbieter angekommen sein und Geld gekostet haben. Deshalb fragt DepotSchlopf nicht noch einmal, sondern sperrt beide KIs. Bitte im Konto des Anbieters die Kosten ansehen und dann in der Einrichtung die Sperre aufheben.",
 );
                    let m = self.sperre(&format!(
 "Verbindung nach dem Absenden abgebrochen; die Anfrage kann angekommen sein und gekostet haben, Kosten unbekannt: {e}"
                ));
                    return Err((Some(Ausgang::Unklar), m));
                }
            }
        }
    }

    fn fragen(
        &mut self,
        erlaubnis: &AufrufErlaubnis,
        system: &str,
        frage: &str,
        zweck: KiZweck,
    ) -> Ergebnis {
        let mut e = self.gespraech(zweck, self.gespraechszeit());
        if let Some(eintrag) = &mut e {
            eintrag.buchung = Some(erlaubnis.buchungsnummer());
        }
        let mut geheim = false;
        let aus = self.fragen_intern(erlaubnis, system, frage, &mut e, &mut geheim);
        if let Some(mut e) = e {
            e.system = Some(system.into());
            e.frage = Some(frage.into());
            e.fehler = aus.text.as_ref().err().cloned();
            self.notiere_gespraech(e, geheim);
        }
        aus
    }

    fn erfasse_http(&self, e: &mut Gespraech, antwort: &crate::netz::Rueckmeldung) -> bool {
        let roh = String::from_utf8_lossy(&antwort.inhalt);
        e.http = Some(antwort.status);
        if antwort.status == 200 {
            e.ergebnis = Ausgang::Unklar.kurz().into();
            if let Some(modell) = crate::json::lies(&roh)
                .ok()
                .and_then(|j| j.feld("model")?.text().map(str::to_owned))
                .filter(|m| !m.is_empty() && m.len() <= 128)
            {
                e.modell = modell;
            }
        } else {
            let kurz: String = roh.chars().take(300).collect();
            e.ergebnis = anbieter::einordnen(antwort.status, &kurz).kurz().into();
            e.kosten_cent = Some(0);
        }
        self.geheim_im_gespraech(&roh)
    }

    fn erfasse_verbrauch(
        eintrag: Option<&mut Gespraech>,
        art: Ausgang,
        v: Verbrauch,
        ist: Option<Cent>,
        text: &Result<String, String>,
    ) {
        if let Some(e) = eintrag {
            e.ergebnis = art.kurz().into();
            e.token_ein = Some(v.token_ein);
            e.token_aus = Some(v.token_aus);
            e.kosten_cent = ist.and_then(|c| u64::try_from(c.0).ok());
            e.antwort = text.as_ref().ok().cloned();
        }
    }

    fn fragen_intern(
        &mut self,
        erlaubnis: &AufrufErlaubnis,
        system: &str,
        frage: &str,
        e: &mut Option<Gespraech>,
        geheim: &mut bool,
    ) -> Ergebnis {
        self.abrechnung = None;
        let ohne = |t: String| Ergebnis {
            text: Err(t),
            kosten: "keine Kosten".into(),
        };
        let Some(schluessel) = self.schluessel.clone() else {
            return ohne("kein Schlüssel".into());
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
            return ohne("Anfrage über dem Deckel, nicht gesendet".into());
        }
        let mut bearer = String::new();
        let kopf = self.anbieter.kopf(schluessel.als_str(), &mut bearer);
        let antwort = match self.sende(&kopf, &inhalt, reserviert) {
            Ok(a) => a,
            Err((status, meldung)) => {
                if let Some((e, status)) = e.as_mut().zip(status) {
                    e.ergebnis = status.kurz().into();
                    if status == Ausgang::Netz {
                        e.kosten_cent = Some(0);
                    }
                }
                return ohne(meldung);
            }
        };
        if let Some(e) = e {
            *geheim = self.erfasse_http(e, &antwort);
        }
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
        Self::erfasse_verbrauch(e.as_mut(), art, v, ist, &gelesen.text);
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
        if gelesen.text.is_ok() {
            self.abrechnung = erlaubnis.abrechnung(v.token_ein, v.token_aus);
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
        Some(c) if c > reserviert => Some(format!("{} Cent über {} reserviert", c.0, reserviert.0)),
        Some(_) if v.suchen > 0 => Some(format!("{} Suchen, keine erlaubt", v.suchen)),
        Some(_) if v.token_ein > u64::from(deckel.max_token_ein()) => Some(format!(
            "{} Eingabe-Token über {} erlaubt",
            v.token_ein,
            deckel.max_token_ein()
        )),
        Some(_) if v.token_aus > u64::from(deckel.max_token_aus()) => Some(format!(
            "{} Ausgabe-Token über {} erlaubt",
            v.token_aus,
            deckel.max_token_aus()
        )),
        Some(_) => None,
    }
}

/// Die Ablehnung in Worten fuer Journal und Protokoll.
fn ablehnung_text(e: Ausgang) -> &'static str {
    match e {
        Ausgang::Limit => "Anbieterlimit erreicht, nächster Versuch beim nächsten Papiertag",
        Ausgang::Guthaben => "Guthaben beim Anbieter fehlt",
        Ausgang::Abgelehnt => "Anbieter lehnt den Schlüssel ab",
        Ausgang::Ueberlastet => "Anbieter überlastet oder gestört",
        _ => "Anbieter lehnt ab",
    }
}

/// Meldung, wenn eine Antwort den Schluessel enthaelt. Sie nennt ihn nicht.
const GEHEIMNIS: &str = "Antwort enthielt den Schlüssel des Anbieters; verworfen";

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
        if std::iter::once(&a.einschaetzung)
            .chain(a.vorschlag.iter().flat_map(|v| v.begruendung.values()))
            .chain(
                a.prognose
                    .iter()
                    .flat_map(|p| [&p.begruendung, &p.widerlegung]),
            )
            .any(|t| self.enthaelt_schluessel(t))
        {
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
    fn kann_abstimmen(&self) -> bool {
        true
    }
    fn abstimmen(
        &mut self,
        erlaubnis: AufrufErlaubnis,
        lage: &Lage<'_>,
        stand: &depot_sim::abstimmung::AbstimmungsStand,
    ) -> depot_sim::abstimmung::AbstimmungsAntwort {
        let system = frage::system(self.wer, &self.grenzen);
        let f = frage::abstimmungs_frage(lage, &self.universum, stand);
        let e = self.fragen(&erlaubnis, &system, &f, KiZweck::Abstimmung);
        match e
            .text
            .and_then(|t| frage::lies_abstimmung(&t, self.wer, &self.universum))
            .and_then(|mut a| {
                a.eigene = self.ohne_geheimnis_antwort(a.eigene)?;
                Ok(a)
            }) {
            Ok(mut a) => {
                a.eigene.prognose = None;
                a.eigene.einschaetzung = format!("{} [{}]", a.eigene.einschaetzung, e.kosten);
                a
            }
            Err(g) => {
                self.abrechnung = None;
                let mut a = depot_sim::abstimmung::AbstimmungsAntwort::unverwertbar();
                a.eigene.einschaetzung = sauber(&self.bereinigt(&g));
                a
            }
        }
    }
    fn abrechnung(&mut self) -> Option<depot_journal::KiAbrechnung> {
        self.abrechnung.take()
    }
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

    fn anfrage_uebersprungen(&self, zweck: KiZweck, grund: &str) {
        if let Some(mut e) = self.gespraech(zweck, self.gespraechszeit()) {
            e.fehler = Some(grund.into());
            self.notiere_gespraech(e, false);
        }
    }

    fn vorschlagen(&mut self, erlaubnis: AufrufErlaubnis, lage: &Lage<'_>) -> Antwort {
        self.vorschlagen_mit_bilanz(erlaubnis, lage, &[])
    }

    fn vorschlagen_mit_bilanz(
        &mut self,
        erlaubnis: AufrufErlaubnis,
        lage: &Lage<'_>,
        bilanz: &[(BeraterId, depot_journal::Auswertung)],
    ) -> Antwort {
        let system = self.vorbereitung.map_or_else(
            || frage::system(self.wer, &self.grenzen),
            |_| frage::vorbereitung_system(self.wer),
        );
        let f = self.vorbereitung.map_or_else(
            || frage::vorschlag_frage(lage, &self.universum, bilanz),
            |zeit| frage::vorbereitung_frage(lage, &self.universum, zeit),
        );
        let e = self.fragen(
            &erlaubnis,
            &system,
            &f,
            if self.vorbereitung.is_some() {
                KiZweck::Vorbereitung
            } else {
                KiZweck::Erstmeinung
            },
        );
        match e
            .text
            .and_then(|t| frage::lies_vorschlag(&t, self.wer, &self.universum))
            .and_then(|a| {
                if self.vorbereitung.is_some() && a.vorschlag.is_some() {
                    Err("Vorbereitung enthält unerlaubte Handelsziele".into())
                } else {
                    self.ohne_geheimnis_antwort(a)
                }
            }) {
            Ok(mut a) => {
                a.prognose = a.prognose.filter(|p| {
                    if self.vorbereitung.is_some() {
                        frage::prognose_ereignis_sichtbar(p, lage, &f)
                    } else {
                        frage::prognose_sichtbar(p, lage, &f)
                    }
                });
                a.einschaetzung = format!("{} [{}]", a.einschaetzung, e.kosten);
                a
            }
            Err(grund) => {
                self.abrechnung = None;
                Antwort {
                    prognose: None,
                    verwertbar: false,
                    einschaetzung: format!(
                        "{KEINE_VERWERTBARE_ANTWORT}: {} [{}]",
                        sauber(&self.bereinigt(&grund)),
                        e.kosten
                    ),
                    vorschlag: None,
                }
            }
        }
    }

    fn pruefen(
        &mut self,
        erlaubnis: AufrufErlaubnis,
        lage: &Lage<'_>,
        vorschlag: &Vorschlag,
    ) -> Urteil {
        self.pruefen_mit_bilanz(erlaubnis, lage, vorschlag, &[])
    }

    fn pruefen_mit_bilanz(
        &mut self,
        erlaubnis: AufrufErlaubnis,
        lage: &Lage<'_>,
        vorschlag: &Vorschlag,
        bilanz: &[(BeraterId, depot_journal::Auswertung)],
    ) -> Urteil {
        let system = frage::system(self.wer, &self.grenzen);
        let f = frage::urteil_frage(lage, &self.universum, vorschlag, bilanz);
        let e = self.fragen(&erlaubnis, &system, &f, KiZweck::Zweitmeinung);
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
            Err(grund) => {
                self.abrechnung = None;
                Urteil {
                    verwertbar: false,
                    zustimmung: false,
                    begruendung: format!(
                        "{KEINE_VERWERTBARE_ANTWORT}: {} [{}]",
                        sauber(&self.bereinigt(&grund)),
                        e.kosten
                    ),
                }
            }
        }
    }
}
