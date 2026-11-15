//! Lokaler Belegvergleich für das gemeinsame SchlopfModell.
use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use depot_journal::{Art, Journal, JournalFehler, Stand};
use depot_kern::Zeitpunkt;
use depot_kern::json::{Json, lies, zitiere};
use depot_lage::html::maskiere;

use crate::http::{feld, umleitung};
use crate::werkbank::{Dienst, lies_falls_da, schreibe_ganz};

const AUSGABE_MAX: usize = 16;
const JOURNAL_MAX: u64 = 32 * 1024 * 1024;
const CPU_PROZENT: u32 = 10;
const ARBEITSBLOCK: Duration = Duration::from_millis(10);
const KONTROLLPAUSE: Duration = Duration::from_millis(20);

fn einstellung(daten: &Path) -> Result<bool, String> {
    match lies_falls_da(&daten.join("neuroschlopf.txt"))?.as_deref() {
        Some("an\n") => Ok(true),
        None | Some("aus\n") => Ok(false),
        Some(_) => Err("NeuroSchlopf-Einstellung unlesbar".into()),
    }
}

pub(crate) fn speichern(d: &Dienst, f: &[(String, String)]) -> Vec<u8> {
    let wert = match feld(f, "an") {
        Some("ja") => "an\n",
        Some("nein") => "aus\n",
        _ => return crate::seiten::fehler(400, "NeuroSchlopf erwartet an oder aus."),
    };
    let _z = d.sperre();
    match schreibe_ganz(&d.wurzel.join("daten/neuroschlopf.txt"), wert) {
        Ok(()) => umleitung("/einrichtung#neuroschlopf"),
        Err(_) => crate::seiten::fehler(500, "NeuroSchlopf-Einstellung nicht gespeichert."),
    }
}

pub(crate) fn formular(d: &Dienst) -> String {
    let (stand, an, aus) = match einstellung(&d.wurzel.join("daten")) {
        Ok(true) => ("Eingeschaltet", " disabled", ""),
        Ok(false) => ("Ausgeschaltet", "", " disabled"),
        Err(_) => ("Einstellung unlesbar", "", ""),
    };
    format!(
        "<section id=\"neuroschlopf\" aria-labelledby=\"neuroschlopf-titel\">\
        <h3 id=\"neuroschlopf-titel\">NeuroSchlopf</h3>\
        <p>{stand} · Lokal · Keine API-Kosten · CPU-Budget {CPU_PROZENT} % des Rechners</p>\
        <form method=\"post\" action=\"/neuroschlopf\"><input type=\"hidden\" name=\"t\" value=\"{}\">\
        <button type=\"submit\" name=\"an\" value=\"ja\"{an}>NeuroSchlopf einschalten</button> \
        <button type=\"submit\" name=\"an\" value=\"nein\"{aus}>NeuroSchlopf ausschalten</button>\
        </form></section>",
        maskiere(&d.token)
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Marke {
    bytes: u64,
    hash: String,
}

impl From<(u64, String)> for Marke {
    fn from((bytes, hash): (u64, String)) -> Self {
        Self { bytes, hash }
    }
}

impl Marke {
    fn lies(j: &Json) -> Option<Self> {
        let hash = j.feld("hash")?.text()?;
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return None;
        }
        Some(Self {
            bytes: j.feld("bytes")?.anzahl()?,
            hash: hash.into(),
        })
    }

    fn json(&self) -> String {
        format!(
            "{{\"bytes\":{},\"hash\":{}}}",
            self.bytes,
            zitiere(&self.hash)
        )
    }

    fn im_journal(&self, bytes: &[u8]) -> bool {
        usize::try_from(self.bytes)
            .ok()
            .and_then(|n| bytes.get(..n))
            .and_then(|a| a.strip_suffix(b"\n"))
            .and_then(|a| a.rsplit(|b| *b == b'\n').next())
            .and_then(|a| a.split(|b| *b == b';').nth(1))
            == Some(self.hash.as_bytes())
    }
}

#[derive(Clone, Debug)]
struct Bild {
    journal: Marke,
    stichtag: Zeitpunkt,
    basis: Option<Zeitpunkt>,
    aenderungen: VecDeque<String>,
    quellen: VecDeque<u64>,
    beratungsstand: Option<Zeitpunkt>,
}

impl Bild {
    fn pfad(daten: &Path) -> PathBuf {
        daten.join("schlopfmodell.json")
    }

    fn lese(daten: &Path) -> Option<Self> {
        let text = lies_falls_da(&Self::pfad(daten)).ok()??;
        let j = lies(&text).ok()?;
        if j.feld("version")?.anzahl()? != 1 || j.doppelte_felder() {
            return None;
        }
        let journal = Marke::lies(j.feld("journal")?)?;
        let stichtag = Zeitpunkt::aus_sekunden(j.feld("stichtag")?.ganz()?)?;
        let basis = match j.feld("basis")? {
            Json::Null => None,
            b => Some(Zeitpunkt::aus_sekunden(b.ganz()?)?),
        };
        let aenderungen = j.feld("aenderungen")?.liste()?;
        if basis.is_some_and(|b| b > stichtag) || aenderungen.len() > AUSGABE_MAX {
            return None;
        }
        let quellen = match j.feld("quellen") {
            Some(q) => q
                .liste()?
                .iter()
                .map(Json::anzahl)
                .collect::<Option<VecDeque<_>>>()?,
            None => VecDeque::new(),
        };
        let beratungsstand = match j.feld("beratungsstand") {
            Some(Json::Null) | None => None,
            Some(t) => Some(Zeitpunkt::aus_sekunden(t.ganz()?)?),
        };
        if quellen.len() > AUSGABE_MAX || beratungsstand.is_some_and(|t| t > stichtag) {
            return None;
        }
        Some(Self {
            journal,
            stichtag,
            basis,
            quellen,
            beratungsstand,
            aenderungen: aenderungen
                .iter()
                .map(|a| {
                    a.text()
                        .filter(|s| s.chars().count() <= 1_600)
                        .map(str::to_owned)
                })
                .collect::<Option<_>>()?,
        })
    }

    fn json(&self) -> String {
        let aenderungen = self
            .aenderungen
            .iter()
            .map(|s| zitiere(s))
            .collect::<Vec<_>>()
            .join(",");
        let basis = self
            .basis
            .map_or_else(|| "null".into(), |b| b.sekunden().to_string());
        let quellen = self
            .quellen
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let beratungsstand = self
            .beratungsstand
            .map_or_else(|| "null".into(), |t| t.sekunden().to_string());
        format!(
            "{{\"version\":1,\"journal\":{},\"stichtag\":{},\"basis\":{basis},\"beratungsstand\":{beratungsstand},\"quellen\":[{quellen}],\"aenderungen\":[{aenderungen}]}}\n",
            self.journal.json(),
            self.stichtag.sekunden()
        )
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) enum Anzeige {
    #[default]
    Wartet,
    Berechnet,
    Fehler(String),
    Fertig(String),
}

fn zeige(bild: &Bild) -> String {
    let basis = bild.basis.map_or_else(
        || "Erster belegter Stand. Eine Vergleichsbasis entsteht mit der nächsten Änderung.".into(),
        |b| format!("Vergleich seit {b}"),
    );
    let mut html = format!("<p>Wissensstand {} · {basis}</p>", bild.stichtag);
    if bild.aenderungen.is_empty() {
        html.push_str("<p>Keine Änderungen an Quellenbelegen.</p>");
    } else {
        if bild.aenderungen.len() == AUSGABE_MAX {
            let _ = write!(
                html,
                "<p>Die letzten {AUSGABE_MAX} Belege im Vergleich.</p>"
            );
        }
        html.push_str("<ul>");
        for s in &bild.aenderungen {
            let _ = write!(html, "<li>{}</li>", maskiere(s));
        }
        html.push_str("</ul>");
    }
    html
}

pub(crate) fn live(d: &Dienst) -> String {
    if !matches!(einstellung(&d.wurzel.join("daten")), Ok(true)) {
        return String::new();
    }
    let status = d.neuro.lock().unwrap_or_else(PoisonError::into_inner);
    let inhalt = match &*status {
        Anzeige::Wartet => "<p>Wartet auf einen bestätigten Journalstand.</p>".into(),
        Anzeige::Berechnet => {
            "<p>SchlopfModell wird im Hintergrund aufgebaut oder aktualisiert.</p>".into()
        }
        Anzeige::Fehler(e) => format!(
            "<p>SchlopfModell derzeit nicht verfügbar: {}</p>",
            maskiere(e)
        ),
        Anzeige::Fertig(html) => html.clone(),
    };
    format!("<section id=\"schlopfmodell\"><h2>SchlopfModell</h2>{inhalt}</section>")
}

struct Vergleich<'a> {
    bisher: BTreeMap<(&'static str, &'a str, String), String>,
    ausgabe: VecDeque<String>,
    quellen: VecDeque<u64>,
    vergleichbar: bool,
    grenze: u64,
}

impl<'a> Vergleich<'a> {
    fn belege(
        &mut self,
        zeile: u64,
        art: &'static str,
        quelle: &'a str,
        id: String,
        aussage: &str,
    ) {
        let alt = self.bisher.insert((art, quelle, id), aussage.to_owned());
        if zeile <= self.grenze {
            return;
        }
        if self.vergleichbar && alt.as_deref() != Some(aussage) {
            if self.quellen.len() == AUSGABE_MAX {
                self.quellen.pop_front();
            }
            self.quellen.push_back(zeile);
        }
        let art = if self.vergleichbar {
            match alt {
                None => "Neuer Beleg",
                Some(t) if t == aussage => "Erneut erfasst",
                Some(_) => "Quelle hat Aussage geändert",
            }
        } else {
            "Beleg im ersten Stand"
        };
        if self.ausgabe.len() == AUSGABE_MAX {
            self.ausgabe.pop_front();
        }
        self.ausgabe.push_back(format!(
            "{art} · Journalzeile {zeile} · {quelle} · {aussage}"
        ));
    }
}

fn verglichen(
    stand: &Stand,
    eintraege: &[(Art, String)],
    journalbytes: &[u8],
    journal: Marke,
    jetzt: Zeitpunkt,
    vorher: Option<&Bild>,
    schritt: &mut dyn FnMut() -> Result<(), String>,
) -> Result<Bild, String> {
    let basis = vorher.filter(|v| v.journal.im_journal(journalbytes) && v.stichtag <= jetzt);
    let mut grenze = 0_u64;
    if let Some(bild) = basis {
        let n = usize::try_from(bild.journal.bytes).map_err(|_| "Journalabschnitt zu lang")?;
        let prefix = journalbytes.get(..n).ok_or("Journalabschnitt fehlt")?;
        for line in prefix.chunks(16_384) {
            schritt()?;
            grenze = grenze.saturating_add(
                u64::try_from(line.split(|&b| b == b'\n').count().saturating_sub(1))
                    .map_err(|_| "Zeilenanzahl zu groß")?,
            );
        }
    }
    let mut v = Vergleich {
        bisher: BTreeMap::new(),
        ausgabe: VecDeque::new(),
        quellen: VecDeque::new(),
        vergleichbar: basis.is_some(),
        grenze,
    };
    let mut nachrichten = stand.nachrichten().iter();
    let mut termine = stand.termine().iter();
    for (index, (art, _)) in eintraege.iter().enumerate() {
        schritt()?;
        let zeile = u64::try_from(index)
            .map_err(|_| "Zeilenanzahl zu groß")?
            .saturating_add(2);
        match art {
            Art::Nachricht => {
                let n = nachrichten.next().ok_or("Nachrichtenreferenz fehlt")?;
                if depot_kern::quelle::kapitalmeldung(n.quelle())
                    || n.quelle() == depot_journal::splitverdacht::ENTWARNUNG
                {
                    continue;
                }
                if n.abruf() > jetzt {
                    return Err("Wissensstand liegt vor einem belegten Abruf.".into());
                }
                let (aussage, id) = n
                    .text()
                    .rsplit_once(" | ")
                    .filter(|(_, url)| {
                        url.starts_with("https://") && !url.chars().any(char::is_whitespace)
                    })
                    .unwrap_or((n.text(), n.text()));
                v.belege(zeile, "nachricht", n.quelle(), id.into(), aussage);
            }
            Art::Termin => {
                let n = termine.next().ok_or("Terminreferenz fehlt")?;
                if n.abruf() > jetzt {
                    return Err("Wissensstand liegt vor einem belegten Abruf.".into());
                }
                let t = n.termin();
                v.belege(
                    zeile,
                    "termin",
                    n.quelle(),
                    t.kennung().into(),
                    &format!(
                        "{} · {} · {}",
                        t.zeit(),
                        t.verlaesslichkeit().name(),
                        t.titel()
                    ),
                );
            }
            _ => {}
        }
    }
    if let Some(vorher) = basis.filter(|_| v.ausgabe.is_empty()) {
        let mut neu = vorher.clone();
        neu.journal = journal;
        return Ok(neu);
    }
    let (quellen, beratungsstand) = if v.quellen.is_empty() {
        basis.map_or_else(
            || (VecDeque::new(), None),
            |b| (b.quellen.clone(), b.beratungsstand),
        )
    } else {
        (v.quellen, Some(jetzt))
    };
    Ok(Bild {
        journal,
        stichtag: jetzt,
        basis: basis.map(|v| v.stichtag),
        aenderungen: v.ausgabe,
        quellen,
        beratungsstand,
    })
}

struct Budget {
    block: Instant,
    anteil_eines_kerns: u32,
}

impl Budget {
    fn anteil(logische_prozessoren: u32) -> u32 {
        // Ein Faden. Sicherheitsabstand für Betriebssystem und einzelne längere Schritte.
        logische_prozessoren
            .saturating_mul(CPU_PROZENT.saturating_sub(1))
            .clamp(1, 100)
    }

    fn neu() -> Self {
        let n = thread::available_parallelism()
            .ok()
            .and_then(|n| u32::try_from(n.get()).ok())
            .unwrap_or(1);
        Self {
            block: Instant::now(),
            anteil_eines_kerns: Self::anteil(n),
        }
    }

    fn pause(arbeit: Duration, anteil: u32) -> Duration {
        arbeit
            .saturating_mul(100_u32.saturating_sub(anteil))
            .checked_div(anteil)
            .unwrap_or(Duration::MAX)
    }

    fn schritt(&mut self, erlaubt: impl Fn() -> bool) -> Result<(), String> {
        let arbeit = self.block.elapsed();
        if arbeit < ARBEITSBLOCK {
            return Ok(());
        }
        let pause = Self::pause(arbeit, self.anteil_eines_kerns);
        let start = Instant::now();
        while let Some(rest) = pause.checked_sub(start.elapsed()).filter(|d| !d.is_zero()) {
            if !erlaubt() {
                return Err("Hintergrundarbeit zurückgestellt".into());
            }
            thread::sleep(rest.min(KONTROLLPAUSE));
        }
        if !erlaubt() {
            return Err("Hintergrundarbeit zurückgestellt".into());
        }
        self.block = Instant::now();
        Ok(())
    }
}

fn frei(d: &Dienst, daten: &Path) -> bool {
    if !matches!(einstellung(daten), Ok(true)) || crate::rueckfall::offen_pfad(&d.wurzel).exists() {
        return false;
    }
    let z = d.sperre();
    z.laufend.is_none() && z.warteschlange.is_empty() && !z.neustart
}

fn anzeige(feld: &Mutex<Anzeige>, status: Anzeige) {
    *feld.lock().unwrap_or_else(PoisonError::into_inner) = status;
}

pub(crate) fn arbeite(d: &Arc<Dienst>) {
    let daten = d.wurzel.join("daten");
    let pfad = daten.join("journal.txt");
    let mut gespeichert = Bild::lese(&daten);
    let mut geprueft = false;
    loop {
        thread::sleep(Duration::from_secs(2));
        let Some(jetzt) = crate::jetzt().filter(|_| frei(d, &daten)) else {
            continue;
        };
        let marke = match Journal::hintergrund_abdruck(&pfad) {
            Ok(m) => Marke::from(m),
            Err(JournalFehler::Belegt) => continue,
            Err(_) if !pfad.exists() => {
                anzeige(&d.neuro, Anzeige::Wartet);
                geprueft = false;
                continue;
            }
            Err(e) => {
                geprueft = false;
                anzeige(&d.neuro, Anzeige::Fehler(format!("{e:?}")));
                continue;
            }
        };
        if geprueft && gespeichert.as_ref().is_some_and(|b| b.journal == marke) {
            continue;
        }
        anzeige(&d.neuro, Anzeige::Berechnet);
        let mut budget = Budget::neu();
        let mut schritt = || budget.schritt(|| frei(d, &daten));
        let neu = Journal::pruefe_im_hintergrund(
            &pfad,
            JOURNAL_MAX,
            &mut schritt,
            |s, e, bytes, m, schritt| {
                verglichen(s, e, bytes, m.into(), jetzt, gespeichert.as_ref(), schritt)
                    .map(|bild| (bild, depot_lage::schlopfmodell_pruefungen(s)))
            },
        );
        if !frei(d, &daten) {
            continue;
        }
        let (bild, pruefungen) = match neu {
            Ok(Ok(bild)) => bild,
            Err(JournalFehler::Belegt) => continue,
            Ok(Err(e)) | Err(JournalFehler::Halt(e)) => {
                geprueft = false;
                anzeige(&d.neuro, Anzeige::Fehler(e));
                continue;
            }
            Err(e) => {
                geprueft = false;
                anzeige(&d.neuro, Anzeige::Fehler(format!("{e:?}")));
                continue;
            }
        };
        if !Journal::hintergrund_abdruck(&pfad).is_ok_and(|m| Marke::from(m) == bild.journal) {
            continue;
        }
        if !gespeichert
            .as_ref()
            .is_some_and(|b| b.journal == bild.journal && b.stichtag == bild.stichtag)
            && let Err(e) = schreibe_ganz(&Bild::pfad(&daten), &bild.json())
        {
            geprueft = false;
            anzeige(&d.neuro, Anzeige::Fehler(e));
            continue;
        }
        let mut html = zeige(&bild);
        if !pruefungen.is_empty() {
            html.push_str("<h3>Letzte Thesenprüfung</h3><ul>");
            for pruefung in pruefungen {
                let _ = write!(html, "<li>{}</li>", maskiere(&pruefung));
            }
            html.push_str("</ul>");
        }
        anzeige(&d.neuro, Anzeige::Fertig(html));
        gespeichert = Some(bild);
        geprueft = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use depot_journal::NachrichtBeleg;

    struct Testdaten(PathBuf);

    impl Testdaten {
        fn neu() -> Result<Self, String> {
            static NR: AtomicUsize = AtomicUsize::new(0);
            let n = NR.fetch_add(1, Ordering::Relaxed);
            let p = std::env::temp_dir().join(format!("schlopfmodell-{}-{n}", std::process::id()));
            fs::create_dir(&p).map_err(|e| e.to_string())?;
            Journal::einrichten(&p.join("journal.txt")).map_err(|e| format!("{e:?}"))?;
            Ok(Self(p))
        }

        fn belege(&self, zeit: &str, text: &str, url: &str) -> Result<(), String> {
            let t = Zeitpunkt::lies(zeit).ok_or("Testzeit ungültig")?;
            let meldung = NachrichtBeleg::neu(
                "quelle-test",
                t,
                zeit.as_bytes(),
                t,
                format!("{text} | {url}").as_str(),
            )?;
            let mut j =
                Journal::oeffnen(&self.0.join("journal.txt")).map_err(|e| format!("{e:?}"))?;
            if !j.belege_nachricht(&meldung).map_err(|e| format!("{e:?}"))? {
                return Err("Testbeleg nicht gespeichert".into());
            }
            Ok(())
        }

        fn bild(&self, zeit: &str, bisher: Option<&Bild>) -> Result<Bild, String> {
            let zeit = Zeitpunkt::lies(zeit).ok_or("Testzeit ungültig")?;
            Journal::pruefe_im_hintergrund(
                &self.0.join("journal.txt"),
                JOURNAL_MAX,
                &mut || Ok(()),
                |s, e, b, m, schritt| verglichen(s, e, b, m.into(), zeit, bisher, schritt),
            )
            .map_err(|e| format!("{e:?}"))?
        }
    }

    impl Drop for Testdaten {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn fortsetzen_nach_neustart_und_rollback() -> Result<(), String> {
        let fortlaufend = Testdaten::neu()?;
        let sicherung = Testdaten::neu()?;
        let url = "https://example.test/eintrag";
        let t1 = "2026-11-12T16:00:00Z";
        let t2 = "2026-11-12T17:00:00Z";
        let t3 = "2026-11-13T06:00:00Z";
        for d in [&fortlaufend, &sicherung] {
            d.belege(t1, "Erste Fassung", url)?;
        }
        let erster = fortlaufend.bild(t1, None)?;
        schreibe_ganz(&Bild::pfad(&fortlaufend.0), &erster.json())?;
        // Wie nach einem Abbruch vor dem atomaren Ersetzen.
        fs::write(Bild::pfad(&fortlaufend.0).with_extension("neu"), "{")
            .map_err(|e| e.to_string())?;
        let gespeichert = Bild::lese(&fortlaufend.0).ok_or("Vollständiger Stand fehlt")?;
        fortlaufend.belege(t2, "Erste Fassung", url)?;
        fortlaufend.belege(t3, "Korrigierte Fassung", url)?;
        fortlaufend.belege(t3, "Zweite Meldung", "https://example.test/zwei")?;
        let nach_neustart = fortlaufend.bild(t3, Some(&gespeichert))?;
        assert_eq!(
            nach_neustart.json(),
            fortlaufend.bild(t3, Some(&erster))?.json()
        );
        let formulierungen = nach_neustart
            .aenderungen
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(formulierungen.contains("Erneut erfasst"));
        assert!(formulierungen.contains("Quelle hat Aussage geändert"));
        assert!(formulierungen.contains("Neuer Beleg"));
        assert_eq!(nach_neustart.aenderungen.len(), 3);
        assert_eq!(nach_neustart.quellen.len(), 2);
        assert!(fortlaufend.bild(t2, Some(&gespeichert)).is_err());
        assert_eq!(
            fortlaufend.bild(t3, Some(&nach_neustart))?.json(),
            nach_neustart.json()
        );
        let nach_wiederherstellung = sicherung.bild(t3, Some(&nach_neustart))?;
        assert!(nach_wiederherstellung.basis.is_none());
        assert_eq!(nach_wiederherstellung.aenderungen.len(), 1);
        assert!(
            nach_wiederherstellung
                .aenderungen
                .iter()
                .all(|a| a.starts_with("Beleg im ersten Stand"))
        );
        Ok(())
    }

    #[test]
    fn journal_darf_waehrend_des_lesens_fortgeschrieben_werden() -> Result<(), String> {
        let daten = Testdaten::neu()?;
        let url = "https://example.test/eintrag";
        daten.belege("2026-11-12T16:00:00Z", "Erster Eintrag", url)?;
        let mut fortgeschrieben = false;
        let ergebnis = Journal::pruefe_im_hintergrund(
            &daten.0.join("journal.txt"),
            JOURNAL_MAX,
            &mut || {
                if !fortgeschrieben {
                    daten.belege("2026-11-12T17:00:00Z", "Ergänzung", url)?;
                    fortgeschrieben = true;
                }
                Ok(())
            },
            |_, _, _, _, _| (),
        );
        assert!(fortgeschrieben);
        assert!(matches!(ergebnis, Err(JournalFehler::Belegt)));
        Ok(())
    }

    #[test]
    fn schalter_ist_geschuetzt_und_unabhaengig_von_gemini() -> Result<(), String> {
        let root = Testdaten::neu()?;
        let daten = root.0.join("daten");
        let d = Dienst::neu(
            root.0.clone(),
            "testtoken".into(),
            crate::werkbank::cargo_starter(),
            crate::lagerechner(Arc::new(crate::System)),
        );
        let t = Zeitpunkt::lies("2026-11-12T16:00:00Z").ok_or("Testzeit")?;
        let aufruf = |token: &str, an: &str| {
            let a = crate::http::Anfrage {
                methode: "POST".into(),
                pfad: "/neuroschlopf".into(),
                abfrage: String::new(),
                kopf: [
                    ("host", "127.0.0.1:47410"),
                    ("origin", "http://127.0.0.1:47410"),
                    (
                        "content-type",
                        "application/x-www-form-urlencoded; charset=UTF-8",
                    ),
                ]
                .into_iter()
                .map(|(n, w)| (n.into(), w.into()))
                .collect(),
                rumpf: format!("t={token}&an={an}").into_bytes(),
            };
            String::from_utf8_lossy(&crate::behandle(&d, &crate::System, &a, t)).into_owned()
        };
        let gemini = depot_lage::gespraeche::gemini_pfad(&daten);
        schreibe_ganz(&gemini, "gratisprojekt=ja\n")?;
        assert!(!einstellung(&daten)?);
        assert!(live(&d).is_empty());
        assert!(aufruf("falsch", "ja").contains("403"));
        assert!(!einstellung(&daten)?);
        assert!(aufruf("testtoken", "ja").contains("/einrichtung#neuroschlopf"));
        assert!(einstellung(&daten)?);
        assert!(live(&d).contains("SchlopfModell"));
        assert!(aufruf("testtoken", "nein").contains("/einrichtung#neuroschlopf"));
        assert!(!einstellung(&daten)?);
        assert_eq!(
            lies_falls_da(&gemini)?.as_deref(),
            Some("gratisprojekt=ja\n")
        );
        Ok(())
    }

    #[test]
    fn drossel_rechnet_gegen_die_gesamte_cpu_kapazitaet() {
        let arbeit = Duration::from_millis(10);
        assert_eq!(Budget::anteil(1), 9);
        assert_eq!(
            Budget::pause(arbeit, Budget::anteil(1)),
            Duration::from_nanos(101_111_111)
        );
        assert_eq!(Budget::anteil(8), 72);
        assert_eq!(Budget::pause(arbeit, Budget::anteil(32)), Duration::ZERO);
    }
    #[test]
    fn windows_lasttest_misst_cpu_zeit_des_hintergrunds() -> Result<(), String> {
        const KIND: &str = "DEPOTSCHLOPF_NEURO_CPU_MESSKIND";
        if !cfg!(windows) {
            return Ok(());
        }
        // Der Kindprozess führt nur diese Probe aus. Andere Tests desselben
        // Rust-Testprogramms würden die CPU-Zeit der Messung sonst mit erhöhen.
        if std::env::var_os(KIND).is_none() {
            let programm = std::env::current_exe().map_err(|e| e.to_string())?;
            let ausgabe = std::process::Command::new(programm)
                .args([
                    "--exact",
                    "neuroschlopf::tests::windows_lasttest_misst_cpu_zeit_des_hintergrunds",
                    "--nocapture",
                ])
                .env(KIND, "1")
                .output()
                .map_err(|e| e.to_string())?;
            let text = String::from_utf8_lossy(&ausgabe.stdout);
            assert!(
                ausgabe.status.success() && text.contains("NeuroSchlopf CPU-Messung"),
                "{text}\n{}",
                String::from_utf8_lossy(&ausgabe.stderr)
            );
            println!("{text}");
            return Ok(());
        }

        let cpu = || -> Result<Duration, String> {
            let befehl = format!(
                "(Get-Process -Id {}).TotalProcessorTime.Ticks",
                std::process::id()
            );
            let ausgabe = std::process::Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", &befehl])
                .output()
                .map_err(|e| e.to_string())?;
            if !ausgabe.status.success() {
                return Err(String::from_utf8_lossy(&ausgabe.stderr).into_owned());
            }
            let ticks: u64 = String::from_utf8_lossy(&ausgabe.stdout)
                .trim()
                .parse()
                .map_err(|e| format!("CPU-Zeit unlesbar: {e}"))?;
            Ok(Duration::from_nanos(ticks.saturating_mul(100)))
        };

        let vorher = cpu()?;
        let beginn = Instant::now();
        let mut budget = Budget {
            block: Instant::now(),
            anteil_eines_kerns: Budget::anteil(1),
        };
        while beginn.elapsed() < Duration::from_secs(10) {
            for nr in 0..256 {
                std::hint::black_box(nr);
            }
            budget.schritt(|| true)?;
        }
        let fenster = beginn.elapsed();
        let verbraucht = cpu()?.checked_sub(vorher).ok_or("CPU-Uhr lief rückwärts")?;
        let grenze = fenster
            .saturating_mul(CPU_PROZENT)
            .checked_div(100)
            .ok_or("CPU-Grenze unlesbar")?;
        println!(
            "NeuroSchlopf CPU-Messung bei Dauerlast · {verbraucht:?} CPU-Zeit · {grenze:?} erlaubt · {fenster:?} Dauer"
        );
        assert!(
            verbraucht <= grenze,
            "CPU-Grenze überschritten: {verbraucht:?} > {grenze:?}"
        );
        Ok(())
    }
}
