//! Lokaler Belegvergleich für das gemeinsame SchlopfModell.
use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use depot_journal::{Art, Journal, JournalFehler, Stand};
use depot_kern::json::{Json, lies, zitiere};
use depot_kern::{Betroffen, Datum, Isin, Verlaesslichkeit, Zeitbezug, Zeitpunkt};
use depot_lage::html::maskiere;
use depot_lage::{OffeneThese, Thesenpruefung, datum_text, zeit_text};

use crate::http::{antwort, feld, umleitung};
use crate::werkbank::{Dienst, lies_falls_da, schreibe_ganz};

const LERNLAUF_LINK: &str = r#"<p><a href="/neuroschlopf/lernlauf">Neuro2026 · Lernkurve und Vorhersagen des Referenzlaufs</a></p>"#;

const BLINDPRUEFUNG_LINK: &str =
    r#"<p><a href="/neuroschlopf/belegpruefung">Belege für NeuroSchlopf blind bewerten</a></p>"#;

const AUSGABE_MAX: usize = 16;
const EINGABEN_MAX: usize = 40;
const JOURNAL_MAX: u64 = 32 * 1024 * 1024;
const CPU_PROZENT: u32 = 10;
const ARBEITSBLOCK: Duration = Duration::from_millis(10);
const KONTROLLPAUSE: Duration = Duration::from_millis(20);

pub(super) fn einstellung(daten: &Path) -> Result<bool, String> {
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
        </form>{BLINDPRUEFUNG_LINK}{LERNLAUF_LINK}</section>",
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
    /// Letzte Journalzeile des Vergleichsstands. Fehlt bei älteren gespeicherten Ständen.
    grenze: Option<u64>,
}

impl Bild {
    fn pfad(daten: &Path) -> PathBuf {
        daten.join("schlopfmodell.json")
    }

    fn lese(daten: &Path) -> Option<Self> {
        Self::lies_text(&lies_falls_da(&Self::pfad(daten)).ok()??)
    }

    fn lies_text(text: &str) -> Option<Self> {
        let j = lies(text).ok()?;
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
        let grenze = match j.feld("grenze") {
            Some(Json::Null) | None => None,
            Some(g) => Some(g.anzahl()?),
        };
        Some(Self {
            journal,
            stichtag,
            basis,
            quellen,
            beratungsstand,
            grenze,
            aenderungen: aenderungen
                .iter()
                .map(|a| {
                    a.text()
                        .filter(|s| s.chars().count() <= 3_400)
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
        let grenze = self.grenze.map_or_else(|| "null".into(), |g| g.to_string());
        format!(
            "{{\"version\":1,\"journal\":{},\"stichtag\":{},\"basis\":{basis},\"grenze\":{grenze},\"beratungsstand\":{beratungsstand},\"quellen\":[{quellen}],\"aenderungen\":[{aenderungen}]}}\n",
            self.journal.json(),
            self.stichtag.sekunden()
        )
    }
}

#[derive(Debug, Default)]
pub(crate) struct Anzeige {
    status: Status,
    fertig: Option<Fertig>,
}

#[derive(Debug, Default)]
enum Status {
    #[default]
    Wartet,
    Berechnet,
    Fehler(String),
    Fertig,
}

#[derive(Debug)]
struct Fertig {
    geprueft: Zeitpunkt,
    bild: Bild,
    eingaben: Option<Eingaben>,
    bereit: VecDeque<Eingabe>,
    offene_thesen: Vec<OffeneThese>,
    pruefungen: Vec<Thesenpruefung>,
}

/// Wie ein Beleg im verglichenen Abschnitt gegenüber dem vorigen Stand einzuordnen ist.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Einordnung {
    Erststand,
    Neu,
    Geaendert(String),
    Wiederholt,
}

/// Eine berücksichtigte Nachricht oder ein Termin mit Journalzeile.
#[derive(Clone, Debug)]
struct Eingabe {
    zeile: u64,
    termin: bool,
    quelle: String,
    aussage: String,
    zeit: String,
    abruf: Zeitpunkt,
    einordnung: Einordnung,
    werte: Vec<Isin>,
}

/// Strukturierte Belege genau des Abschnitts, den der Vergleich geprüft hat.
#[derive(Clone, Debug, Default)]
struct Eingaben {
    von: u64,
    bis: u64,
    gesamt: usize,
    neu: usize,
    geaendert: usize,
    wiederholt: usize,
    liste: VecDeque<Eingabe>,
}

impl Eingaben {
    fn zaehle(&mut self, e: Eingabe) {
        self.gesamt = self.gesamt.saturating_add(1);
        match e.einordnung {
            Einordnung::Neu => self.neu = self.neu.saturating_add(1),
            Einordnung::Geaendert(_) => self.geaendert = self.geaendert.saturating_add(1),
            Einordnung::Wiederholt => self.wiederholt = self.wiederholt.saturating_add(1),
            Einordnung::Erststand => {}
        }
        if self.liste.len() == EINGABEN_MAX {
            self.liste.pop_front();
        }
        self.liste.push_back(e);
    }
}

fn zeige(bild: &Bild) -> String {
    let basis = bild.basis.map_or_else(
        || "Erster bestätigter Stand. Er dient als Basis für den nächsten Vergleich.".into(),
        |b| format!("Vergleich seit {b}"),
    );
    let mut html = format!(
        "<h3>Befunde</h3><p>Wissensstand {} · {basis}</p>",
        bild.stichtag
    );
    let wiederholt = |s: &&String| s.starts_with("Erneut erfasst · ");
    if bild.aenderungen.iter().all(|s| wiederholt(&s)) {
        if bild.basis.is_some() {
            html.push_str("<p>Keine neuen oder geänderten Belege.</p>");
        }
    } else {
        html.push_str("<ul>");
        for s in bild.aenderungen.iter().filter(|s| !wiederholt(s)) {
            let _ = write!(html, "<li>{}</li>", maskiere(s));
        }
        html.push_str("</ul>");
    }
    if bild.aenderungen.iter().any(|s| wiederholt(&s)) {
        html.push_str("<details><summary>Unverändert erneut erfasst</summary><ul>");
        for s in bild.aenderungen.iter().filter(wiederholt) {
            let _ = write!(html, "<li>{}</li>", maskiere(s));
        }
        html.push_str("</ul></details>");
    }
    html
}

/// Zustand des Hintergrundvergleichs in klaren Worten.
fn aktivitaet(d: &Dienst, anzeige: &Anzeige, ein: &Result<bool, String>) -> String {
    let daten = d.wurzel.join("daten");
    match ein {
        Err(_) => "NeuroSchlopf-Einstellung unlesbar.".into(),
        Ok(false) => {
            "Ausgeschaltet · <a href=\"/einrichtung#neuroschlopf\">NeuroSchlopf einrichten</a>"
                .into()
        }
        Ok(true) if !frei(d, &daten) => {
            "Hintergrundarbeit pausiert. Depotaufträge und Updates haben Vorrang.".into()
        }
        Ok(true) => match &anzeige.status {
            Status::Wartet => "Wartet auf einen bestätigten Journalstand.".into(),
            Status::Berechnet => "Vergleich läuft.".into(),
            Status::Fehler(e) => format!("Vergleich fehlgeschlagen: {}", maskiere(e)),
            Status::Fertig => "Vergleich abgeschlossen.".into(),
        },
    }
}

pub(crate) fn live(d: &Dienst) -> String {
    let ein = einstellung(&d.wurzel.join("daten"));
    let anzeige = d.neuro.lock().unwrap_or_else(PoisonError::into_inner);
    let mut html = format!(
        "<section id=\"schlopfmodell\"><h2>NeuroSchlopf</h2><p>{}</p>",
        aktivitaet(d, &anzeige, &ein)
    );
    if let Some(f) = &anzeige.fertig {
        let _ = write!(
            html,
            "<p>Letzter abgeschlossener Vergleich {}</p>",
            zeit_text(f.geprueft)
        );
    }
    html.push_str("<p><a href=\"/neuroschlopf\">Vergleich ansehen</a></p></section>");
    html
}

/// Lesende Detailseite. Sie löst keinen Vergleich aus und schreibt nichts.
pub(crate) fn seite(
    d: &Dienst,
    aufrufe: &crate::zeitstrahl_ki::Aufrufe,
    tag: Option<Datum>,
) -> Vec<u8> {
    let daten = d.wurzel.join("daten");
    let ein = einstellung(&daten);
    let anzeige = d.neuro.lock().unwrap_or_else(PoisonError::into_inner);
    let mut html = String::from(
        "<section id=\"neuroschlopf-vergleich\"><h2>NeuroSchlopf · Belegvergleich</h2>",
    );
    offene_thesen_html(&mut html, anzeige.fertig.as_ref(), tag);
    let _ = write!(
        html,
        "<h3>Aktivität</h3><p>Hintergrundvergleich · {}</p>",
        aktivitaet(d, &anzeige, &ein)
    );
    let gespeichert;
    let (bild, eingaben, pruefungen) = if let Some(f) = &anzeige.fertig {
        {
            let laeuft = matches!(anzeige.status, Status::Berechnet) && matches!(ein, Ok(true));
            if laeuft {
                html.push_str("<p>Ein weiterer Vergleich läuft. Bis er abgeschlossen ist, gilt der folgende letzte abgeschlossene Vergleich.</p>");
            }
            let _ = write!(
                html,
                "<p><strong>Letzter abgeschlossener Vergleich</strong> · Geprüft {}</p>",
                zeit_text(f.geprueft)
            );
            if f.bild.stichtag == f.geprueft {
                let _ = write!(
                    html,
                    "<p>Wissensstand der Daten {}</p>",
                    zeit_text(f.bild.stichtag)
                );
            } else {
                let _ = write!(
                    html,
                    "<p>Wissensstand der Daten {} · Das Journal hat sich seitdem nicht geändert. Der Prüfzeitpunkt liegt deshalb später.</p>",
                    zeit_text(f.bild.stichtag)
                );
            }
            (
                Some(&f.bild),
                f.eingaben.as_ref(),
                Some(f.pruefungen.as_slice()),
            )
        }
    } else {
        {
            gespeichert = Bild::lese(&daten);
            match &gespeichert {
                Some(b) => {
                    let _ = write!(
                        html,
                        "<p><strong>Gespeicherter Stand</strong> · Wissensstand der Daten {} · Seit dem Start des Dienstes noch nicht erneut geprüft.</p>",
                        zeit_text(b.stichtag)
                    );
                }
                None => html.push_str(
                    "<p>Noch kein abgeschlossener Vergleich. Es fehlt ein bestätigter Stand.</p>",
                ),
            }
            (gespeichert.as_ref(), None, None)
        }
    };
    if let Some(b) = bild {
        bereit_html(&mut html, b);
    }
    aufrufe.schlopfmodell(&mut html);
    thesen_html(&mut html, bild, pruefungen);
    if let Some(b) = bild {
        html.push_str("<details><summary>Alle neuen Belege</summary>");
        ergebnis_html(&mut html, b, eingaben);
        eingaben_html(&mut html, b, eingaben);
        html.push_str("</details>");
    }
    drop(anzeige);
    let _ = write!(
        html,
        "<h3>Modellstatus</h3><p>Neuronales Modell noch nicht eingebunden. Der produktive Vergleich arbeitet mit dem Belegabgleich oben. Die Einbindung eines Modells für Depotbelege folgt mit #325.</p>\
        <ul><li><a href=\"/neuroschlopf/lernlauf\">Neuro2026-Referenzlauf Multiplikation</a> · abgeschlossener Lernlauf mit echten Messwerten</li>\
        <li><a href=\"/neuroschlopf/belegpruefung\">Blindbewertung</a> · historische Thesen und Belege von Hand bewerten</li></ul>\
        </section>"
    );
    let seite = crate::seiten::rahmen(
        "/neuroschlopf",
        None,
        None,
        &html,
        d.version.as_deref(),
        (&crate::betriebsleiste(d, tag), &crate::update_leiste(d)),
        ("", ""),
    );
    antwort(200, crate::HTML, seite.as_bytes())
}

fn offene_thesen_html(html: &mut String, fertig: Option<&Fertig>, heute: Option<Datum>) {
    html.push_str("<h3>Offene Thesen und ihre Belege</h3>");
    let Some(f) = fertig else {
        html.push_str("<p>Die offenen Thesen erscheinen nach der nächsten bestätigten Prüfung des Journals.</p>");
        return;
    };
    let heute = heute.or_else(|| f.geprueft.frankfurt_tag());
    let mut thesen = f
        .offene_thesen
        .iter()
        .filter(|t| heute.is_none_or(|tag| t.gueltig_bis >= tag))
        .peekable();
    if thesen.peek().is_none() {
        html.push_str(
            "<p>Im geprüften Journal ist keine offene These von Dachs oder Maus belegt.</p>",
        );
    }
    for t in thesen {
        let _ = write!(
            html,
            "<h4>{} · {} · These vom {}</h4><p>{}</p>",
            t.wer,
            t.isin,
            datum_text(t.tag),
            maskiere(&t.text)
        );
        if t.belege.is_empty() {
            html.push_str("<p>Kein Beleg aus dem verfügbaren Stand in einer Prüfung dieser These zitiert.</p>");
        } else {
            html.push_str("<p>In einer Prüfung dieser These zitierte Belege</p><ul>");
            for b in &f.bereit {
                if let Some(grund) = t.belege.get(&b.zeile) {
                    fokus_beleg_html(html, b, "", Some(grund));
                }
            }
            html.push_str("</ul>");
        }
        let mut nennungen = f
            .bereit
            .iter()
            .filter(|b| b.werte.contains(&t.isin) && !t.belege.contains_key(&b.zeile))
            .peekable();
        if nennungen.peek().is_some() {
            html.push_str(
                "<details><summary>Weitere Belege zum Wert · Thesenbezug offen</summary><ul>",
            );
            for b in nennungen {
                fokus_beleg_html(
                    html,
                    b,
                    if b.termin {
                        "Termin zum Wert · "
                    } else {
                        "Wert in der Nachricht genannt · "
                    },
                    None,
                );
            }
            html.push_str("</ul></details>");
        }
    }
}

fn fokus_beleg_html(html: &mut String, b: &Eingabe, einordnung: &str, grund: Option<&str>) {
    let _ = write!(
        html,
        "<li>{einordnung}<strong>J{}</strong> · {} · {}",
        b.zeile,
        maskiere(&b.quelle),
        maskiere(&b.aussage)
    );
    if let Some(grund) = grund {
        let _ = write!(html, "<br>Begründung aus der Prüfung · {}", maskiere(grund));
    }
    html.push_str("</li>");
}

fn eingaben_html(html: &mut String, bild: &Bild, eingaben: Option<&Eingaben>) {
    let _ = write!(
        html,
        "<details><summary>Technische Details</summary><p>Bestätigter Journalstand · {} Bytes · Prüfsumme <code>{}</code></p>",
        bild.journal.bytes,
        maskiere(bild.journal.hash.get(..16).unwrap_or(&bild.journal.hash))
    );
    let Some(e) = eingaben else {
        html.push_str("</details>");
        if bild.grenze.is_some() {
            html.push_str("<p>Die einzelnen Eingaben dieses gespeicherten Stands erscheinen nach der nächsten Prüfung des Journals. Dafür muss das Journal nicht fortgeschrieben werden.</p>");
        } else {
            html.push_str("<p>Für diesen gespeicherten Stand sind der verglichene Abschnitt und die einzelnen Eingaben nicht belegt. Sie erscheinen nach dem nächsten Vergleich eines fortgeschriebenen Journals.</p>");
        }
        return;
    };
    let von = e.von.max(2);
    if bild.basis.is_none() && von > e.bis {
        html.push_str("<p>Erster bestätigter Stand · Das Journal enthält noch keine Einträge.</p>");
    } else if bild.basis.is_none() {
        let _ = write!(
            html,
            "<p>Erster bestätigter Stand · Journalzeilen J{von} bis J{}</p>",
            e.bis
        );
    } else if von > e.bis {
        html.push_str("<p>Verglichener Abschnitt · Seit dem vorigen Stand sind keine Journalzeilen hinzugekommen.</p>");
    } else {
        let _ = write!(
            html,
            "<p>Verglichener Abschnitt · Journalzeilen J{von} bis J{}</p>",
            e.bis
        );
    }
    html.push_str("</details>");
    if e.gesamt == 0 {
        html.push_str("<p>Im verglichenen Abschnitt stehen keine Nachrichten oder Termine.</p>");
        return;
    }
    if e.liste.len() < e.gesamt {
        let _ = write!(
            html,
            "<p>Gezeigt werden die letzten {} von {} Nachrichten und Terminen dieses Abschnitts.</p>",
            e.liste.len(),
            e.gesamt
        );
    } else {
        let _ = write!(
            html,
            "<p>{} Nachrichten und Termine in diesem Abschnitt.</p>",
            e.gesamt
        );
    }
    html.push_str("<div class=\"tabelle\"><table><thead><tr><th>Zeile</th><th>Typ</th><th>Quelle</th><th>Titel oder Aussage</th><th>Zeitpunkt</th><th>Abruf</th><th>Einordnung</th></tr></thead><tbody>");
    for x in &e.liste {
        let (typ, zeitart) = if x.termin {
            ("Termin", "Termin")
        } else {
            ("Nachricht", "Veröffentlicht")
        };
        let einordnung = match &x.einordnung {
            Einordnung::Erststand => "Erster Stand".into(),
            Einordnung::Neu => "Neuer Beleg".into(),
            Einordnung::Wiederholt => "Identische Wiederholung".into(),
            Einordnung::Geaendert(alt) => format!(
                "Aussage geändert · Bisher {} · Jetzt {}",
                maskiere(alt),
                maskiere(&x.aussage)
            ),
        };
        let _ = write!(
            html,
            "<tr><td>J{}</td><td>{typ}</td><td>{}</td><td>{}</td><td>{zeitart} {}</td><td>Abgerufen {}</td><td>{einordnung}</td></tr>",
            x.zeile,
            maskiere(&x.quelle),
            maskiere(&x.aussage),
            maskiere(&x.zeit),
            zeit_text(x.abruf)
        );
    }
    html.push_str("</tbody></table></div>");
}

fn ergebnis_html(html: &mut String, bild: &Bild, eingaben: Option<&Eingaben>) {
    html.push_str("<h3>Ergebnis</h3>");
    match eingaben {
        None => html.push_str(&zeige(bild)),
        Some(e) if bild.basis.is_none() => {
            let _ = write!(
                html,
                "<p>Erster bestätigter Stand mit {} Nachrichten und Terminen. Er dient als Basis für den nächsten Vergleich.</p>",
                e.gesamt
            );
        }
        Some(e) => {
            if let Some(b) = bild.basis {
                let _ = write!(html, "<p>Vergleich mit dem Stand vom {}</p>", zeit_text(b));
            }
            let _ = write!(
                html,
                "<p>Neue Belege {} · Geänderte Aussagen {} · Identische Wiederholungen {}</p>",
                e.neu, e.geaendert, e.wiederholt
            );
            if e.neu == 0 && e.geaendert == 0 {
                html.push_str("<p>Keine neuen oder geänderten Belege.</p>");
            }
        }
    }
}

fn bereit_html(html: &mut String, bild: &Bild) {
    html.push_str("<h3>Zur Prüfung verfügbare Belege</h3>");
    match bild.beratungsstand {
        Some(t) => {
            let _ = write!(
                html,
                "<p>Für Dachs und Maus vorbereitet · Wissensstand {} · Journalzeilen {}</p>",
                zeit_text(t),
                bild.quellen
                    .iter()
                    .map(|z| format!("J{z}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        None => {
            html.push_str("<p>Für Dachs und Maus sind noch keine neuen Belege vorbereitet.</p>");
        }
    }
}

fn thesen_html(html: &mut String, bild: Option<&Bild>, pruefungen: Option<&[Thesenpruefung]>) {
    html.push_str("<h3>Letzte protokollierte Thesenprüfungen</h3>");
    let Some(pruefungen) = pruefungen else {
        html.push_str("<p>Die Thesenprüfungen werden beim nächsten abgeschlossenen Vergleich aus dem Journal gelesen.</p>");
        return;
    };
    html.push_str("<ul>");
    for wer in ["Dachs", "Maus"] {
        let Some(p) = pruefungen.iter().find(|p| p.wer == wer) else {
            let _ = write!(html, "<li>{wer} · Keine Thesenprüfung belegt</li>");
            continue;
        };
        let these = p.these.as_deref().map_or_else(
            || "Wortlaut der These im Journal nicht gefunden.".into(),
            maskiere,
        );
        let _ = write!(
            html,
            "<li><strong>{wer}</strong> · These vom {} zu {} {} · Beleg J{} · Wissensstand {} · Protokolliert am {}<br>These: {these}<br>Begründung: {}",
            datum_text(p.these_vom),
            maskiere(&p.isin),
            p.urteil,
            p.beleg,
            zeit_text(p.wissensstand),
            datum_text(p.protokolliert),
            maskiere(&p.begruendung)
        );
        if let Some(stand) = bild
            .and_then(|b| b.beratungsstand)
            .filter(|s| p.wissensstand < *s)
        {
            let _ = write!(
                html,
                "<br><em>Ältere Prüfung</em> · Sie beruht auf dem Wissensstand {}. Die aktuell bestätigten Belege haben den Wissensstand {}.",
                zeit_text(p.wissensstand),
                zeit_text(stand)
            );
        }
        html.push_str("</li>");
    }
    html.push_str("</ul>");
}

struct Fund<'a> {
    zeile: u64,
    art: &'static str,
    quelle: &'a str,
    id: String,
    aussage: String,
    anzeige: String,
    zeit: String,
    abruf: Zeitpunkt,
    werte: Vec<Isin>,
}

impl Fund<'_> {
    fn eingabe(&self, einordnung: Einordnung) -> Eingabe {
        Eingabe {
            zeile: self.zeile,
            termin: self.art == "termin",
            quelle: self.quelle.to_owned(),
            aussage: self.anzeige.clone(),
            zeit: self.zeit.clone(),
            abruf: self.abruf,
            einordnung,
            werte: self.werte.clone(),
        }
    }
}

struct Vergleich<'a> {
    bisher: BTreeMap<(&'static str, &'a str, String), String>,
    ausgabe: VecDeque<String>,
    bereit: VecDeque<Eingabe>,
    bisher_bereit: VecDeque<Eingabe>,
    bisher_quellen: Option<&'a VecDeque<u64>>,
    vergleichbar: bool,
    grenze: u64,
    eingaben: Eingaben,
}

impl<'a> Vergleich<'a> {
    fn belege(&mut self, mut f: Fund<'a>) {
        if self.bisher_quellen.is_some_and(|q| q.contains(&f.zeile)) {
            self.bisher_bereit
                .push_back(f.eingabe(Einordnung::Erststand));
        }
        let alt = self.bisher.insert(
            (f.art, f.quelle, std::mem::take(&mut f.id)),
            f.aussage.clone(),
        );
        if f.zeile <= self.grenze {
            return;
        }
        let einordnung = match alt {
            _ if !self.vergleichbar => Einordnung::Erststand,
            None => Einordnung::Neu,
            Some(t) if t == f.aussage => Einordnung::Wiederholt,
            Some(t) => Einordnung::Geaendert(t),
        };
        let (art, betrag) = match &einordnung {
            Einordnung::Erststand => ("Beleg im ersten Stand", f.aussage.clone()),
            Einordnung::Neu => ("Neuer Beleg", f.aussage.clone()),
            Einordnung::Wiederholt => ("Erneut erfasst", f.aussage.clone()),
            Einordnung::Geaendert(t) => (
                "Quelle hat Aussage geändert",
                format!("Bisher {t} · Jetzt {}", f.aussage),
            ),
        };
        if self.ausgabe.len() == AUSGABE_MAX {
            self.ausgabe.pop_front();
        }
        self.ausgabe.push_back(format!(
            "{art} · Journalzeile {} · {} · {betrag}",
            f.zeile, f.quelle
        ));
        let eingabe = f.eingabe(einordnung);
        if matches!(
            eingabe.einordnung,
            Einordnung::Neu | Einordnung::Geaendert(_)
        ) {
            if self.bereit.len() == AUSGABE_MAX {
                self.bereit.pop_front();
            }
            self.bereit.push_back(eingabe.clone());
        }
        self.eingaben.zaehle(eingabe);
    }
}

fn nachricht_fund(zeile: u64, n: &depot_journal::NachrichtBeleg) -> Fund<'_> {
    let (aussage, id) = n
        .text()
        .rsplit_once(" | ")
        .filter(|(_, url)| url.starts_with("https://") && !url.chars().any(char::is_whitespace))
        .unwrap_or((n.text(), n.text()));
    Fund {
        zeile,
        art: "nachricht",
        quelle: n.quelle(),
        id: id.into(),
        aussage: aussage.into(),
        anzeige: aussage.into(),
        zeit: zeit_text(n.zeit()),
        abruf: n.abruf(),
        werte: Vec::new(),
    }
}

fn termin_fund(zeile: u64, n: &depot_journal::TerminBeleg) -> Fund<'_> {
    let t = n.termin();
    let verlaesslichkeit = match t.verlaesslichkeit() {
        Verlaesslichkeit::Bestaetigt => "bestätigt",
        Verlaesslichkeit::Angekuendigt => "angekündigt",
        Verlaesslichkeit::Vermutet => "vermutet",
    };
    Fund {
        zeile,
        art: "termin",
        quelle: n.quelle(),
        id: t.kennung().into(),
        aussage: format!(
            "{} · {} · {}",
            t.zeit(),
            t.verlaesslichkeit().name(),
            t.titel()
        ),
        anzeige: format!("{} · {verlaesslichkeit}", t.titel()),
        zeit: match t.zeit() {
            Zeitbezug::Tag(tag) => datum_text(tag),
            Zeitbezug::Zeitpunkt(z) => zeit_text(z),
        },
        abruf: n.abruf(),
        werte: t
            .betroffen()
            .iter()
            .filter_map(|b| match b {
                Betroffen::Wert(isin) => Some(isin.clone()),
                Betroffen::Groesse(_) => None,
            })
            .collect(),
    }
}

struct Vergleichsergebnis {
    bild: Bild,
    eingaben: Option<Eingaben>,
    bereit: VecDeque<Eingabe>,
}

impl Vergleichsergebnis {
    fn fertig(
        mut self,
        stand: &Stand,
        jetzt: Zeitpunkt,
        schritt: &mut dyn FnMut() -> Result<(), String>,
    ) -> Result<Fertig, String> {
        let werte = depot_kern::liste::beobachtungsliste().unwrap_or_default();
        for b in self.bereit.iter_mut().filter(|b| !b.termin) {
            schritt()?;
            b.werte = depot_lage::nachricht_wertnennungen(&b.aussage, &werte);
        }
        let belegzeilen = self.bereit.iter().map(|b| b.zeile).collect();
        let offene_thesen = jetzt.frankfurt_tag().map_or_else(
            || Ok(Vec::new()),
            |tag| depot_lage::offene_thesen(stand, tag, &belegzeilen, schritt),
        )?;
        Ok(Fertig {
            geprueft: jetzt,
            bild: self.bild,
            eingaben: self.eingaben,
            bereit: self.bereit,
            offene_thesen,
            pruefungen: depot_lage::schlopfmodell_thesenpruefungen(stand),
        })
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
) -> Result<Vergleichsergebnis, String> {
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
    // Unverändertes Journal: Die Eingaben stammen aus dem Abschnitt, den der gespeicherte
    // Stand damals verglichen hat. Ältere Stände ohne Grenze belegen ihn nicht.
    let gleich = basis.filter(|b| b.journal == journal);
    let (pruefgrenze, vergleichbar, belegt) = match gleich {
        Some(b) => match b.grenze {
            Some(g) => (g, b.basis.is_some(), true),
            None => (grenze, true, false),
        },
        None => (grenze, basis.is_some(), true),
    };
    let mut v = Vergleich {
        bisher: BTreeMap::new(),
        ausgabe: VecDeque::new(),
        bereit: VecDeque::new(),
        bisher_bereit: VecDeque::new(),
        bisher_quellen: basis.map(|b| &b.quellen),
        vergleichbar,
        grenze: pruefgrenze,
        eingaben: Eingaben::default(),
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
                v.belege(nachricht_fund(zeile, n));
            }
            Art::Termin => {
                let n = termine.next().ok_or("Terminreferenz fehlt")?;
                if n.abruf() > jetzt {
                    return Err("Wissensstand liegt vor einem belegten Abruf.".into());
                }
                v.belege(termin_fund(zeile, n));
            }
            _ => {}
        }
    }
    let mut eingaben = std::mem::take(&mut v.eingaben);
    eingaben.von = pruefgrenze.saturating_add(1);
    eingaben.bis = u64::try_from(eintraege.len())
        .map_err(|_| "Zeilenanzahl zu groß")?
        .saturating_add(1);
    let eingaben = belegt.then_some(eingaben);
    if let Some(vorher) = gleich {
        return Ok(Vergleichsergebnis {
            bild: vorher.clone(),
            eingaben,
            bereit: v.bisher_bereit,
        });
    }
    // Ein Abschnitt mit bloßen Journalnotizen ändert den für die Beratung verfügbaren
    // Belegstand nicht. Dann bleiben genau dessen belegte Quellen verfügbar.
    let (quellen, beratungsstand, bereit) = if v.bereit.is_empty() {
        let (quellen, beratungsstand) = basis.map_or_else(
            || (VecDeque::new(), None),
            |b| (b.quellen.clone(), b.beratungsstand),
        );
        (quellen, beratungsstand, v.bisher_bereit)
    } else {
        (
            v.bereit.iter().map(|b| b.zeile).collect(),
            Some(jetzt),
            v.bereit,
        )
    };
    Ok(Vergleichsergebnis {
        bild: Bild {
            journal,
            stichtag: jetzt,
            basis: basis.map(|v| v.stichtag),
            aenderungen: v.ausgabe,
            quellen,
            beratungsstand,
            grenze: Some(grenze),
        },
        eingaben,
        bereit,
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

fn anzeige(feld: &Mutex<Anzeige>, status: Status) {
    feld.lock().unwrap_or_else(PoisonError::into_inner).status = status;
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
                anzeige(&d.neuro, Status::Wartet);
                geprueft = false;
                continue;
            }
            Err(e) => {
                geprueft = false;
                anzeige(&d.neuro, Status::Fehler(format!("{e:?}")));
                continue;
            }
        };
        if geprueft && gespeichert.as_ref().is_some_and(|b| b.journal == marke) {
            continue;
        }
        anzeige(&d.neuro, Status::Berechnet);
        let mut budget = Budget::neu();
        let mut schritt = || budget.schritt(|| frei(d, &daten));
        let neu = Journal::pruefe_im_hintergrund(
            &pfad,
            JOURNAL_MAX,
            &mut schritt,
            |s, e, bytes, m, schritt| {
                verglichen(s, e, bytes, m.into(), jetzt, gespeichert.as_ref(), schritt)?
                    .fertig(s, jetzt, schritt)
            },
        );
        if !frei(d, &daten) {
            continue;
        }
        let fertig = match neu {
            Ok(Ok(bild)) => bild,
            Err(JournalFehler::Belegt) => continue,
            Ok(Err(e)) | Err(JournalFehler::Halt(e)) => {
                geprueft = false;
                anzeige(&d.neuro, Status::Fehler(e));
                continue;
            }
            Err(e) => {
                geprueft = false;
                anzeige(&d.neuro, Status::Fehler(format!("{e:?}")));
                continue;
            }
        };
        let bild = &fertig.bild;
        if !Journal::hintergrund_abdruck(&pfad).is_ok_and(|m| Marke::from(m) == bild.journal) {
            continue;
        }
        if !gespeichert
            .as_ref()
            .is_some_and(|b| b.journal == bild.journal && b.stichtag == bild.stichtag)
            && let Err(e) = schreibe_ganz(&Bild::pfad(&daten), &bild.json())
        {
            geprueft = false;
            anzeige(&d.neuro, Status::Fehler(e));
            continue;
        }
        gespeichert = Some(bild.clone());
        let mut anzeige = d.neuro.lock().unwrap_or_else(PoisonError::into_inner);
        anzeige.status = Status::Fertig;
        anzeige.fertig = Some(fertig);
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

        fn vergleich(
            &self,
            zeit: &str,
            bisher: Option<&Bild>,
        ) -> Result<(Bild, Option<Eingaben>), String> {
            self.vergleich_voll(zeit, bisher)
                .map(|v| (v.bild, v.eingaben))
        }

        fn vergleich_voll(
            &self,
            zeit: &str,
            bisher: Option<&Bild>,
        ) -> Result<Vergleichsergebnis, String> {
            let zeit = Zeitpunkt::lies(zeit).ok_or("Testzeit ungültig")?;
            Journal::pruefe_im_hintergrund(
                &self.0.join("journal.txt"),
                JOURNAL_MAX,
                &mut || Ok(()),
                |s, e, b, m, schritt| verglichen(s, e, b, m.into(), zeit, bisher, schritt),
            )
            .map_err(|e| format!("{e:?}"))?
        }

        fn bild(&self, zeit: &str, bisher: Option<&Bild>) -> Result<Bild, String> {
            self.vergleich(zeit, bisher).map(|(bild, _)| bild)
        }

        fn journal(&self) -> Result<Journal, String> {
            Journal::oeffnen(&self.0.join("journal.txt")).map_err(|e| format!("{e:?}"))
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
        assert!(formulierungen.contains("Bisher Erste Fassung · Jetzt Korrigierte Fassung"));
        assert!(zeige(&nach_neustart).contains("<summary>Unverändert erneut erfasst</summary>"));
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

    fn ansicht(bild: &Bild, e: &Eingaben) -> String {
        let mut html = String::new();
        eingaben_html(&mut html, bild, Some(e));
        ergebnis_html(&mut html, bild, Some(e));
        bereit_html(&mut html, bild);
        html
    }

    fn erststand_belegen(daten: &Testdaten, url: &str) -> Result<(), String> {
        use depot_journal::TerminBeleg;
        use depot_kern::{Betroffen, Herkunft, Isin, Termin};
        let zeit = |t: &str| Zeitpunkt::lies(t).ok_or_else(|| format!("Testzeit {t}"));
        let mut j = daten.journal()?;
        let meldung = NachrichtBeleg::neu(
            "quelle-test",
            zeit("2026-11-12T10:00:00Z")?,
            b"roh-1",
            zeit("2026-11-12T09:00:00Z")?,
            &format!("SAP erhöht die Prognose | {url}"),
        )?;
        assert!(j.belege_nachricht(&meldung).map_err(|e| format!("{e:?}"))?);
        let termin = Termin::neu(
            "sap-zahlen",
            "SAP nennt Zahlen",
            Zeitbezug::Tag(Datum::lies("2026-11-25").ok_or("Termintag")?),
            vec![Betroffen::Wert(
                Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?,
            )],
            Verlaesslichkeit::Bestaetigt,
        )
        .map_err(|e| format!("{e:?}"))?;
        let beleg = TerminBeleg::neu(
            "kalender",
            Herkunft::Quelle,
            zeit("2026-11-12T10:05:00Z")?,
            b"termin",
            termin,
        )?;
        j.belege_termin(&beleg).map_err(|e| format!("{e:?}"))?;
        drop(j);
        Ok(())
    }

    #[test]
    fn offene_these_zeigt_zitat_und_nennung_mit_unterschiedlichem_status() -> Result<(), String> {
        let daten = Testdaten::neu()?;
        let t1 = "2026-11-12T10:30:00Z";
        let t2 = "2026-11-12T12:30:00Z";
        let basis = daten.bild(t1, None)?;
        for (aussage, url) in [
            (
                "Neue Aufträge im Cloud-Geschäft",
                "https://example.test/auftraege",
            ),
            ("SAP verschiebt Konferenz", "https://example.test/sap"),
            ("Weihnachtsbaum gefällt", "https://example.test/baum"),
        ] {
            daten.belege(t2, aussage, url)?;
        }
        let Vergleichsergebnis {
            bild,
            eingaben,
            mut bereit,
        } = daten.vergleich_voll(t2, Some(&basis))?;
        let werte = depot_kern::liste::beobachtungsliste()?;
        for b in &mut bereit {
            b.werte = depot_lage::nachricht_wertnennungen(&b.aussage, &werte);
        }
        let geprueft = Zeitpunkt::lies(t2).ok_or("Testzeit fehlt")?;
        let tag = geprueft.frankfurt_tag().ok_or("Testtag fehlt")?;
        let f = Fertig {
            geprueft,
            bild,
            eingaben,
            bereit,
            offene_thesen: vec![OffeneThese {
                wer: "Dachs",
                tag,
                isin: Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?,
                text: "Cloud-Umsatz nimmt zu".into(),
                gueltig_bis: tag,
                belege: BTreeMap::from([(2, "Die Aufträge stützen den erwarteten Umsatz.".into())]),
            }],
            pruefungen: Vec::new(),
        };
        let mut html = String::new();
        offene_thesen_html(&mut html, Some(&f), Some(tag));
        assert!(html.contains(
            "In einer Prüfung dieser These zitierte Belege</p><ul><li><strong>J2</strong>"
        ));
        assert!(html.contains("Wert in der Nachricht genannt · <strong>J3</strong>"));
        assert!(html.contains("Die Aufträge stützen den erwarteten Umsatz."));
        assert!(!html.contains("Weihnachtsbaum"));
        let e = f.eingaben.as_ref().ok_or("Verglichene Eingaben fehlen")?;
        assert!(ansicht(&f.bild, e).contains("Weihnachtsbaum"));
        // Eine spätere Beratungsnotiz lässt die Quellen des verfügbaren Stands bestehen.
        let mut j = daten.journal()?;
        j.begruende(tag, "Test", "Eigenständige Notiz")
            .map_err(|e| format!("{e:?}"))?;
        drop(j);
        let spaeter = daten.vergleich_voll("2026-11-12T13:00:00Z", Some(&f.bild))?;
        let vorliegend = spaeter.bereit.front().ok_or("Vorbereiteter Beleg fehlt")?;
        assert_eq!(vorliegend.zeile, 2);
        assert_eq!(vorliegend.aussage, "Neue Aufträge im Cloud-Geschäft");
        Ok(())
    }

    #[test]
    fn leeres_journal_und_gespeicherter_stand_nach_neustart() -> Result<(), String> {
        let daten = Testdaten::neu()?;
        let (leer, e) = daten.vergleich("2026-11-12T10:30:00Z", None)?;
        let e = e.ok_or("Eingaben fehlen")?;
        assert_eq!((e.gesamt, e.bis), (0, 1));
        let html = ansicht(&leer, &e);
        assert!(
            html.contains("Erster bestätigter Stand · Das Journal enthält noch keine Einträge.")
        );
        assert!(!html.contains("J2 bis J1"), "{html}");
        // Nach Neustart: Stand mit Grenze braucht nur die erneute Prüfung.
        let mut html = String::new();
        eingaben_html(&mut html, &leer, None);
        assert!(html.contains("nach der nächsten Prüfung des Journals"));
        assert!(!html.contains("fortgeschriebenen"));
        let mut html = String::new();
        eingaben_html(
            &mut html,
            &Bild {
                grenze: None,
                ..leer
            },
            None,
        );
        assert!(html.contains("fortgeschriebenen Journals"));
        Ok(())
    }

    #[test]
    fn gegenprobe_zeigt_zeile_quelle_zeit_und_einordnung_je_stand() -> Result<(), String> {
        let daten = Testdaten::neu()?;
        let zeit = |t: &str| Zeitpunkt::lies(t).ok_or_else(|| format!("Testzeit {t}"));
        let url = "https://example.test/sap";
        erststand_belegen(&daten, url)?;
        // Erststand mit Nachricht und Termin.
        let (erster, e1) = daten.vergleich("2026-11-12T10:30:00Z", None)?;
        let e1 = e1.ok_or("Eingaben des Erststands fehlen")?;
        assert_eq!((e1.gesamt, e1.von, e1.bis), (2, 1, 3));
        let n = e1.liste.front().ok_or("Nachricht fehlt")?;
        assert_eq!(
            (n.zeile, n.termin, n.quelle.as_str()),
            (2, false, "quelle-test")
        );
        assert_eq!(n.zeit, "12.11.2026 10:00 MEZ");
        assert_eq!(zeit_text(n.abruf), "12.11.2026 11:00 MEZ");
        let t = e1.liste.back().ok_or("Termin fehlt")?;
        assert_eq!(
            (t.zeile, t.termin, t.zeit.as_str()),
            (3, true, "25.11.2026")
        );
        assert!(
            e1.liste
                .iter()
                .all(|e| e.einordnung == Einordnung::Erststand)
        );
        let html = ansicht(&erster, &e1);
        for teil in [
            "Erster bestätigter Stand · Journalzeilen J2 bis J3",
            "<td>J2</td><td>Nachricht</td><td>quelle-test</td><td>SAP erhöht die Prognose</td><td>Veröffentlicht 12.11.2026 10:00 MEZ</td><td>Abgerufen 12.11.2026 11:00 MEZ</td>",
            "<td>J3</td><td>Termin</td><td>kalender</td><td>SAP nennt Zahlen · bestätigt</td><td>Termin 25.11.2026</td>",
            "Erster bestätigter Stand mit 2 Nachrichten und Terminen",
        ] {
            assert!(html.contains(teil), "{teil}\n{html}");
        }

        // Korrigierte Nachricht derselben Quelle.
        let mut j = daten.journal()?;
        let korrektur = NachrichtBeleg::neu(
            "quelle-test",
            zeit("2026-11-12T12:00:00Z")?,
            b"roh-2",
            zeit("2026-11-12T11:30:00Z")?,
            &format!("SAP senkt die Prognose | {url}"),
        )?;
        assert!(
            j.belege_nachricht(&korrektur)
                .map_err(|e| format!("{e:?}"))?
        );
        drop(j);
        let (zweiter, e2) = daten.vergleich("2026-11-12T12:30:00Z", Some(&erster))?;
        let e2 = e2.ok_or("Eingaben des Folgestands fehlen")?;
        assert_eq!(
            (e2.gesamt, e2.geaendert, e2.neu, e2.von, e2.bis),
            (1, 1, 0, 4, 4)
        );
        let k = e2.liste.front().ok_or("Korrektur fehlt")?;
        assert_eq!(k.zeile, 4);
        assert_eq!(
            k.einordnung,
            Einordnung::Geaendert("SAP erhöht die Prognose".into())
        );
        let html = ansicht(&zweiter, &e2);
        assert!(html.contains("Verglichener Abschnitt · Journalzeilen J4 bis J4"));
        assert!(html.contains(
            "Aussage geändert · Bisher SAP erhöht die Prognose · Jetzt SAP senkt die Prognose"
        ));
        assert!(html.contains("Neue Belege 0 · Geänderte Aussagen 1"));
        assert!(html.contains("Journalzeilen J4</p>"), "{html}");
        assert!(!html.contains("Keine neuen oder geänderten Belege"));

        // Danach nur eine unabhängige Journalnotiz.
        let mut j = daten.journal()?;
        j.begruende(
            Datum::lies("2026-11-12").ok_or("Tag")?,
            "Test",
            "Eigenständige Notiz",
        )
        .map_err(|e| format!("{e:?}"))?;
        drop(j);
        let (dritter, e3) = daten.vergleich("2026-11-12T13:00:00Z", Some(&zweiter))?;
        let e3 = e3.ok_or("Eingaben des dritten Stands fehlen")?;
        assert_eq!((e3.gesamt, e3.von, e3.bis), (0, 5, 5));
        let html = ansicht(&dritter, &e3);
        assert!(html.contains("Im verglichenen Abschnitt stehen keine Nachrichten oder Termine."));
        assert!(html.contains("Keine neuen oder geänderten Belege."));
        assert!(
            !html.contains("SAP senkt"),
            "Frühere Korrektur gilt nicht als neue Eingabe"
        );
        assert_eq!(dritter.beratungsstand, zweiter.beratungsstand);

        // Unverändertes Journal: derselbe Abschnitt bleibt belegt, ohne neuen Stand.
        let gespeichert = Bild::lies_text(&dritter.json()).ok_or("Stand unlesbar")?;
        let (gleich, e4) = daten.vergleich("2026-11-12T14:00:00Z", Some(&gespeichert))?;
        assert_eq!(gleich.json(), dritter.json());
        assert_eq!(e4.map(|e| (e.gesamt, e.von)), Some((0, 5)));
        // Älterer gespeicherter Stand ohne Grenze: Lücke statt geratener Eingaben.
        let alt = Bild {
            grenze: None,
            ..dritter.clone()
        };
        let (_, e5) = daten.vergleich("2026-11-12T14:00:00Z", Some(&alt))?;
        assert!(e5.is_none());
        let mut html = String::new();
        eingaben_html(&mut html, &alt, None);
        assert!(html.contains("einzelnen Eingaben nicht belegt"));
        Ok(())
    }

    #[test]
    fn thesenpruefung_je_berater_mit_beleg_oder_fehlender_pruefung() -> Result<(), String> {
        let stand = Zeitpunkt::lies("2026-11-12T12:30:00Z").ok_or("Zeit")?;
        let bild = Bild {
            journal: Marke {
                bytes: 1,
                hash: "0".repeat(64),
            },
            stichtag: stand,
            basis: None,
            aenderungen: VecDeque::new(),
            quellen: VecDeque::from([4]),
            beratungsstand: Some(stand),
            grenze: Some(0),
        };
        let pruefung = Thesenpruefung {
            wer: "Dachs",
            these_vom: Datum::lies("2026-11-11").ok_or("Tag")?,
            isin: "DE0007164600".into(),
            these: Some("These vom 2026-11-11 zu DE0007164600: Steigt".into()),
            urteil: "beibehalten",
            begruendung: "Prognose bestätigt <b>".into(),
            wissensstand: Zeitpunkt::lies("2026-11-12T10:30:00Z").ok_or("Zeit")?,
            beleg: 4,
            protokolliert: Datum::lies("2026-11-12").ok_or("Tag")?,
        };
        let mut html = String::new();
        thesen_html(&mut html, Some(&bild), Some(&[pruefung]));
        for teil in [
            "<strong>Dachs</strong> · These vom 11.11.2026 zu DE0007164600 beibehalten · Beleg J4 · Wissensstand 12.11.2026 11:30 MEZ · Protokolliert am 12.11.2026",
            "Begründung: Prognose bestätigt &lt;b&gt;",
            "<em>Ältere Prüfung</em>",
            "<li>Maus · Keine Thesenprüfung belegt</li>",
        ] {
            assert!(html.contains(teil), "{teil}\n{html}");
        }
        let mut leer = String::new();
        thesen_html(&mut leer, None, None);
        assert!(leer.contains("beim nächsten abgeschlossenen Vergleich"));
        Ok(())
    }

    #[test]
    fn vergleich_ohne_neue_quellenbelege_zeigt_einen_neuen_pruefstand() -> Result<(), String> {
        let daten = Testdaten::neu()?;
        let url = "https://example.test/eintrag";
        let t1 = "2026-11-12T16:00:00Z";
        let t2 = "2026-11-12T17:00:00Z";
        let t3 = "2026-11-12T18:00:00Z";
        daten.belege(t1, "Erste Fassung", url)?;
        let erster = daten.bild(t1, None)?;
        daten.belege(t2, "Zweite Fassung", url)?;
        let geaendert = daten.bild(t2, Some(&erster))?;
        let mut journal =
            Journal::oeffnen(&daten.0.join("journal.txt")).map_err(|e| format!("{e:?}"))?;
        let tag = depot_kern::Datum::lies("2026-11-12").ok_or("Tag fehlt")?;
        journal
            .begruende(tag, "Test", "Eigenständige Notiz")
            .map_err(|e| format!("{e:?}"))?;
        drop(journal);
        let neu = daten.bild(t3, Some(&geaendert))?;
        assert!(zeige(&neu).contains("Keine neuen oder geänderten Belege"));
        assert_eq!(neu.beratungsstand, geaendert.beratungsstand);
        assert_eq!(neu.quellen, geaendert.quellen);
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
        assert!(live(&d).contains("Ausgeschaltet"));
        assert!(aufruf("falsch", "ja").contains("403"));
        assert!(!einstellung(&daten)?);
        assert!(aufruf("testtoken", "ja").contains("/einrichtung#neuroschlopf"));
        assert!(einstellung(&daten)?);
        assert!(live(&d).contains("Wartet auf einen bestätigten Journalstand"));
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
