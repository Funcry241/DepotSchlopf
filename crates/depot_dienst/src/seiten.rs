//! Die zwei Seiten der Oberflaeche: **Live** (Taetigkeit, naechster Lauf, Lagebild mit Budget,
//! Depot und Gespraech) und **Einrichtung** (die eine Config-Seite). Jeder Text ist maskiert;
//! das einzige Skript ist `/app.js` (Auffrischen und Verbindungsanzeige).

use std::fmt::Write as _;

use depot_kern::Datum;
use depot_lage::html::{STIL, maskiere};
use depot_lage::{Abhilfe, Grenzwert, SchluesselStand, datum_text, ortszeit, zeit_text};

use crate::autostart;
use crate::http::feld;
use crate::kostenschutz;
use crate::schluessel::Wer;
use crate::werkbank::{Anlass, Art, Auftrag, Frueher, ZeitplanAbgelehnt, Zustand};
use crate::zeitplan::{Zeitplan, lies_uhrzeit, naechster_fuer, uhrzeit_text};

const STIL_DIENST: &str = "\
nav{display:flex;gap:16px;align-items:baseline;flex-wrap:wrap}nav a{color:inherit}\
nav a.hier{font-weight:600;text-decoration:none}\
form.knopf{display:inline;margin:0}\
button{font:inherit;padding:4px 10px;border:1px solid var(--linie);border-radius:6px;\
background:var(--flaeche);color:var(--text);cursor:pointer}\
button:disabled{opacity:.65;cursor:default}\
input[type=password],input[type=time]{font:inherit;padding:3px 6px;border:1px solid var(--linie);\
border-radius:6px;background:var(--grund);color:var(--text);max-width:100%}\
pre{white-space:pre-wrap;word-break:break-word;font:12px/1.4 ui-monospace,Consolas,monospace;\
margin:6px 0}\
.banner{position:sticky;top:0;background:var(--halt);color:#fff;padding:8px 16px;font-weight:600}\
.meldung{border-left:4px solid currentColor;padding-left:10px}\
label{display:block;margin:6px 0}";

/// Ein Knopf, der einen Auftrag einreiht.
pub fn auftrag_knopf(token: &str, art: Art, text: &str) -> String {
    format!(
        "<form class=\"knopf\" method=\"post\" action=\"/ausfuehren\">\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <input type=\"hidden\" name=\"art\" value=\"{}\">\
         <button type=\"submit\">{}</button></form>",
        maskiere(token),
        art.kurz(),
        maskiere(text)
    )
}

/// Das Bedienelement zu einer Abhilfe des Lagebilds.
pub fn abhilfe_knopf(token: &str, a: Abhilfe) -> Option<String> {
    match a {
        Abhilfe::JournalEinrichten => Some(auftrag_knopf(token, Art::Einrichten, "Einrichten")),
        Abhilfe::KurseAbrufen => Some(auftrag_knopf(token, Art::Kurse, "Kurse abrufen")),
        Abhilfe::TermineAbrufen => Some(auftrag_knopf(token, Art::Termine, "Termine abrufen")),
        Abhilfe::PapiertagStarten => Some(auftrag_knopf(token, Art::Papier, "Papiertag starten")),
        Abhilfe::SchluesselKursquelle
        | Abhilfe::SchluesselKursquelleUs
        | Abhilfe::SchluesselBerater(_) => {
            Some("<a href=\"/einrichtung#schluessel\">Zur Einrichtung</a>".into())
        }
        Abhilfe::KostenschutzPruefen => {
            Some("<a href=\"/einrichtung#kostenschutz\">Zur Prüfung</a>".into())
        }
        Abhilfe::KurssperrePruefen => {
            Some("<a href=\"/einrichtung#kurssperre\">Zur Prüfung</a>".into())
        }
        Abhilfe::Pruefen | Abhilfe::Warten => None,
    }
}

/// Meldungen nach dem gefuehrten Aufheben einer Sperre: `kostenschutz_…` oder `kurssperre_…`.
fn sperr_meldung(code: &str) -> Option<(&'static str, &'static str)> {
    let (sperre, ausgang) = code.split_once('_')?;
    let kurse = match sperre {
        "kostenschutz" => false,
        "kurssperre" => true,
        _ => return None,
    };
    Some(match (ausgang, kurse) {
        ("aufgehoben", false) => (
            "gut",
            "Sperre aufgehoben, Grund festgehalten. Die Reservierung bleibt gebucht; jeder \
             weitere Aufruf durchläuft wieder die Kostenprüfung.",
        ),
        ("aufgehoben", true) => (
            "gut",
            "Kurssperre aufgehoben, Grund festgehalten. Der nächste Abruf holt alle Kurse neu.",
        ),
        ("unbestaetigt", false) => (
            "offen",
            "Nicht aufgehoben: Bitte zuerst bestätigen, dass Sie die Kosten beim Anbieter geprüft haben.",
        ),
        ("unbestaetigt", true) => (
            "offen",
            "Nicht aufgehoben: Bitte zuerst bestätigen, dass Sie den Schlüssel der Kursquelle \
             geprüft und bei Bedarf erneuert haben.",
        ),
        ("belegt", _) => (
            "offen",
            "Nicht aufgehoben: Es läuft oder wartet ein Auftrag. Danach erneut versuchen.",
        ),
        ("geaendert", _) => (
            "halt",
            "Nicht aufgehoben: Die Sperre hat sich inzwischen geändert. Bitte den neuen Grund prüfen.",
        ),
        ("frei", false) => ("gut", "Es liegt keine Sperre des Kostenschutzes vor."),
        ("frei", true) => ("gut", "Es liegt keine Kurssperre vor."),
        ("fehler", _) => (
            "halt",
            "Nicht aufgehoben: Datei nicht les- oder schreibbar. Die Sperre gilt weiter.",
        ),
        _ => return None,
    })
}

/// Feste Meldungen nach einem Formular; nur bekannte Kennungen erscheinen.
pub fn meldung(abfrage: &str) -> Option<(&'static str, &'static str)> {
    let code = abfrage
        .split('&')
        .find_map(|p| p.strip_prefix("m="))
        .unwrap_or_default();
    if let Some(m) = sperr_meldung(code) {
        return Some(m);
    }
    Some(match code {
        "auftrag" => ("gut", "Auftrag eingereiht. Er steht unter Tätigkeit."),
        "auftrag_belegt" => (
            "offen",
            "Nicht eingereiht: Dieser Auftrag läuft oder wartet schon, ist gerade erst fertig \
             geworden (doppelt geklickt oder Seite neu gesendet?), oder es warten zu viele. \
             Nichts wurde doppelt gestartet; unter Tätigkeit steht, was läuft.",
        ),
        "schluessel" => (
            "gut",
            "Schlüssel in der Anmeldeinformationsverwaltung gespeichert. Er gilt ab dem nächsten Lauf.",
        ),
        "schluessel_protokoll" => (
            "offen",
            "Schlüssel gespeichert, aber das Anbieterprotokoll war nicht schreibbar. Der Stand \
             zeigt bis zum nächsten Aufruf den alten Befund.",
        ),
        "schluessel_ungueltig" => (
            "halt",
            "Schlüssel nicht gespeichert: Länge oder Zeichen passen nicht (Regel steht in der Zeile).",
        ),
        "schluessel_fehler" => (
            "halt",
            "Schlüssel nicht gespeichert: Die Anmeldeinformationsverwaltung hat abgelehnt.",
        ),
        "dialogtest_ja" => ("gut", "Dialogtest: Ja. Es wurde nichts freigegeben."),
        "dialogtest_nein" => ("gut", "Dialogtest: Nein. Es wurde nichts freigegeben."),
        "dialogtest_zeit" => (
            "gut",
            "Dialogtest: Zeit abgelaufen. Es wurde nichts freigegeben.",
        ),
        "dialogtest_fehler" => ("offen", "Der Windows-Dialog konnte nicht geöffnet werden."),
        "dialogtest_belegt" => ("offen", "Es ist bereits ein Freigabedialog geöffnet."),
        "papierfreigabe_ok" => (
            "gut",
            "Papierhandel freigegeben. Die neue Verlustmessung beginnt beim bestätigten Bargeld. Der nächste reguläre Papiertag kann wieder handeln.",
        ),
        "papierfreigabe_unbestaetigt" => (
            "halt",
            "Die Sperre gilt weiter. Bitte bestätige die neue Verlustmessung ausdrücklich.",
        ),
        "papierfreigabe_belegt" => (
            "offen",
            "Die Sperre gilt weiter. Warte, bis der laufende oder wartende Auftrag abgeschlossen ist.",
        ),
        "papierfreigabe_geaendert" => (
            "halt",
            "Der Journalstand hat sich geändert. Lade die Einrichtung neu und prüfe die aktuellen Beträge.",
        ),
        "papierfreigabe_fehler" => (
            "halt",
            "Die Freigabe konnte nicht bestätigt werden. Lade die Einrichtung neu und prüfe Journal und Papierkonto.",
        ),
        "zeitplan" => (
            "gut",
            "Einstellungen gespeichert. Ob der Zeitplan an oder aus ist, bleibt unverändert.",
        ),
        "zeitplan_an" => (
            "gut",
            "Zeitplan eingeschaltet. Die gespeicherten Einstellungen gelten.",
        ),
        "zeitplan_aus" => (
            "gut",
            "Zeitplan ausgeschaltet. Bereits gestartete Läufe laufen zu Ende.",
        ),
        "zeitplan_neu_laden" => (
            "offen",
            "Zeitplan unverändert. Bitte lade die Einrichtung neu und wähle die gewünschte Aktion.",
        ),
        "zeitplan_ungueltig" => (
            "halt",
            "Zeitplan nicht gespeichert: Uhrzeit im Format hh:mm. Die übrigen Eingaben stehen \
             noch im Formular.",
        ),
        "zeitplan_einrichtung" => (
            "halt",
            "Zeitplan nicht eingeschaltet: Erst muss die Einrichtung vollständig sein. Was fehlt, \
             steht beim Zeitplan.",
        ),
        "zeitplan_lage" => (
            "offen",
            "Zeitplan nicht eingeschaltet: Der Stand wird gerade neu geprüft. Bitte in ein paar \
             Sekunden den Stand neu laden und erneut einschalten.",
        ),
        "zeitplan_fehler" => (
            "halt",
            "Zeitplan unverändert. Die Datei konnte nicht gespeichert werden.",
        ),
        "autostart_an" => (
            "gut",
            "Autostart eingerichtet. DepotSchlopf startet bei deiner nächsten Windows-Anmeldung.",
        ),
        "autostart_aus" => ("gut", "Autostart entfernt."),
        "autostart_bestaetigen" => (
            "offen",
            "Autostart unverändert. Setze erst den Haken zur Bestätigung.",
        ),
        "autostart_fehler" => (
            "halt",
            "Autostart unverändert. Prüfe den Stand unten. Falls er sich ändern lässt, versuche es erneut.",
        ),
        "update" => (
            "gut",
            "Auftrag eingereiht. Sein Protokoll steht auf Live unter Tätigkeit.",
        ),
        "update_belegt" => (
            "offen",
            "Update nicht eingereiht: Es geht nur, wenn kein anderer Auftrag läuft oder wartet.",
        ),
        _ => return None,
    })
}

/// Der Seitenrahmen. `teil` ist der Pfad, mit dem `/app.js` den ganzen Inhalt regelmaessig neu
/// laedt; ohne `teil` erneuert es nur einen Bereich mit `data-teil` im Inhalt (falls vorhanden).
pub fn rahmen(
    hier: &str,
    teil: Option<&str>,
    meldung: Option<(&str, &str)>,
    inhalt: &str,
) -> String {
    let mut s = String::with_capacity(inhalt.len().saturating_add(4096));
    let nav = |pfad: &str, text: &str| {
        let klasse = if pfad == hier { " class=\"hier\"" } else { "" };
        format!("<a href=\"{pfad}\"{klasse}>{text}</a>")
    };
    let _ = write!(
        s,
        "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>DepotSchlopf</title><style>{STIL}{STIL_DIENST}</style></head><body>\
         <div id=\"getrennt\" class=\"banner\" hidden>Verbindung zum Dienst getrennt, etwa \
         beim Neustart nach einem Update. Die Seite verbindet sich von selbst neu.</div><main>\
         <nav><h1>DepotSchlopf</h1>{}{}<span class=\"leise\">Nur Papiergeld.</span></nav>",
        nav("/", "Live"),
        nav("/einrichtung", "Einrichtung")
    );
    if let Some((klasse, text)) = meldung {
        let _ = write!(
            s,
            "<section class=\"meldung {klasse}\">{}</section>",
            maskiere(text)
        );
    }
    let teil = teil
        .map(|t| format!(" data-teil=\"{}\"", maskiere(t)))
        .unwrap_or_default();
    let _ = writeln!(
        s,
        "<div id=\"teil\"{teil}>{inhalt}</div></main>\
         <script src=\"/app.js\"></script></body></html>"
    );
    s
}

fn anlass_zeile(a: &Auftrag) -> String {
    let t = anlass_text(a.anlass);
    if a.frueher == Frueher::Nein {
        t.to_string()
    } else {
        format!("{t}, vor dem Neustart")
    }
}

fn anlass_text(a: Anlass) -> &'static str {
    match a {
        Anlass::Hand => "von Hand",
        Anlass::Zeitplan => "Zeitplan",
    }
}

fn wochentag_text(d: Datum) -> &'static str {
    match d.wochentag() {
        Some(1) => "Mo",
        Some(2) => "Di",
        Some(3) => "Mi",
        Some(4) => "Do",
        Some(5) => "Fr",
        Some(6) => "Sa",
        Some(7) => "So",
        _ => "",
    }
}

/// Wie es nach einem gescheiterten Lauf weitergeht, aus dem tatsaechlichen Zeitplan: von selbst
/// nur, wenn er an ist und kein Fehler ihn anhaelt. Der Lauf selbst kennt den Zeitplan nicht.
fn weiter_text(z: &Zustand, heute: Option<Datum>) -> String {
    let naechster = naechster_text(z, heute);
    if z.erledigt_fehler.is_some() || z.zeitplan_fehler.is_some() {
        return naechster;
    }
    let naechster = naechster.trim_end_matches('.');
    if z.zeitplan.an && heute.is_some() {
        format!("Nach Zeitplan geht es von selbst weiter: {naechster}.")
    } else {
        format!(
            "DepotSchlopf startet nichts von selbst: {naechster}. Kurse und Papiertag startest \
             du von Hand über die Knöpfe im Lagebild."
        )
    }
}

/// Der naechste Lauf des Zeitplans als Satz.
pub fn naechster_text(z: &Zustand, heute: Option<Datum>) -> String {
    if let Some(f) = &z.erledigt_fehler {
        return format!(
            "Zeitplan angehalten: {f}. Datei prüfen oder löschen, dann Dienst neu starten."
        );
    }
    if let Some(f) = &z.zeitplan_fehler {
        return format!("Zeitplan aus: {f}");
    }
    if !z.zeitplan.an {
        return "Zeitplan aus. Einschalten unter Einrichtung.".into();
    }
    let Some(heute) = heute else {
        return "Uhrzeit unklar (Zeitumstellung); der Zeitplan wartet.".into();
    };
    match naechster_fuer(&z.zeitplan, &z.erledigt, heute, &z.boersen) {
        Some((art, d, u)) => {
            let mut t = format!(
                "{} am {} {} um {}",
                art.name(),
                wochentag_text(d),
                datum_text(d),
                uhrzeit_text(u)
            );
            if let Art::KurseVon(b) = art
                && z.zeitplan.papier_danach
                && crate::zeitplan::letzter_abruf_fuer(&z.zeitplan, d, b, &z.boersen)
            {
                t.push_str(", danach der Papiertag");
            }
            t
        }
        None => "Kein Lauf in den nächsten Tagen geplant.".into(),
    }
}

fn protokoll(id: &str, a: &Auftrag, offen: bool) -> String {
    let mut text = String::new();
    for z in &a.protokoll {
        text.push_str(&maskiere(z));
        text.push('\n');
    }
    if text.is_empty() {
        text.push_str("(noch keine Ausgabe)");
    }
    format!(
        "<details id=\"{id}-{}\"{}><summary>Protokoll ({} Zeilen)</summary><pre>{text}</pre></details>",
        maskiere(&a.kennung),
        if offen { " open" } else { "" },
        a.protokoll.len()
    )
}

fn ergebnis(a: &Auftrag) -> (&'static str, &'static str) {
    if a.frueher == Frueher::Unterbrochen {
        return ("halt", "unterbrochen");
    }
    match a.erfolg {
        Some(true) => ("gut", "erfolgreich"),
        Some(false) => ("halt", "abgebrochen"),
        None => ("offen", "läuft"),
    }
}

/// Was der Dienst tut: laufender Auftrag, Warteschlange, naechster Lauf, Verlauf.
pub fn taetigkeit(z: &Zustand, heute: Option<Datum>) -> String {
    let mut s = String::from("<section><h2>Tätigkeit</h2>");
    if let Some(a) = &z.laufend {
        let _ = write!(
            s,
            "<p class=\"kopf offen\">Läuft: {} ({}) seit {}</p><p class=\"leise\">Kennung {}</p>{}",
            maskiere(a.art.name()),
            anlass_text(a.anlass),
            maskiere(&zeit_text(a.start)),
            maskiere(&a.kennung),
            protokoll("l", a, true)
        );
        if let Some(h) = &a.hinweis {
            let _ = write!(
                s,
                "<p class=\"kopf offen\" id=\"hinweis\">{}</p>",
                maskiere(h)
            );
        }
    } else {
        s.push_str("<p class=\"kopf\">Gerade läuft nichts.</p>");
        // Was der letzte Lauf heute zu sagen hatte, etwa warum der Papiertag ausfiel.
        if let Some(a) = z.verlauf.front()
            && let Some(h) = &a.hinweis
            && heute.is_some_and(|t| {
                a.ende
                    .is_some_and(|e| ortszeit(e).is_some_and(|o| o.0 == t))
            })
        {
            let (klasse, weiter) = if a.erfolg == Some(true) {
                ("gut", String::new())
            } else {
                ("halt", format!(" {}", weiter_text(z, heute)))
            };
            let _ = write!(
                s,
                "<p class=\"kopf {klasse}\" id=\"hinweis\">{}: {}{}</p>",
                maskiere(a.art.name()),
                maskiere(h),
                maskiere(&weiter)
            );
        }
    }
    // Nach einem Neustart: wo der Tag stehen blieb und ob DepotSchlopf selbst weitermacht.
    if let Some((halt, text)) = heute.and_then(|t| crate::werkbank::nach_neustart(z, t)) {
        let klasse = if halt { "halt" } else { "gut" };
        let _ = write!(
            s,
            "<p class=\"kopf {klasse}\" id=\"neustart\">{}</p>",
            maskiere(&text)
        );
    }
    if !z.warteschlange.is_empty() {
        let liste: Vec<String> = z
            .warteschlange
            .iter()
            .map(|(art, anlass)| format!("{} ({})", art.name(), anlass_text(*anlass)))
            .collect();
        let _ = write!(s, "<p>Wartet: {}</p>", maskiere(&liste.join(", ")));
    }
    let _ = write!(
        s,
        "<p>Nächster geplanter Lauf: {}</p>",
        maskiere(&naechster_text(z, heute))
    );
    if !z.verlauf.is_empty() {
        s.push_str(
            "<div class=\"breit\"><table><tr><th>Auftrag</th><th>Anlass</th><th>Start</th>\
             <th>Ende</th><th>Ergebnis</th></tr>",
        );
        for a in &z.verlauf {
            let (klasse, text) = ergebnis(a);
            let _ = write!(
                s,
                "<tr><td>{}<br><span class=\"leise\">{}</span></td><td>{}</td><td>{}</td><td>{}</td>\
                 <td><span class=\"{klasse}\">{text}</span>{}{}</td></tr>",
                maskiere(a.art.name()),
                maskiere(&a.kennung),
                maskiere(&anlass_zeile(a)),
                maskiere(&zeit_text(a.start)),
                a.ende.map(|e| maskiere(&zeit_text(e))).unwrap_or_default(),
                a.hinweis
                    .as_ref()
                    .map(|h| format!("<br>{}", maskiere(h)))
                    .unwrap_or_default(),
                protokoll("v", a, false)
            );
        }
        s.push_str("</table></div>");
    }
    if let Some(f) = &z.verlauf_fehler {
        let _ = write!(s, "<p class=\"kopf halt\">{}</p>", maskiere(f));
    }
    if let Some(h) = &z.verlauf_hinweis {
        let _ = write!(s, "<p class=\"offen\">{}</p>", maskiere(h));
    }
    s.push_str("</section>");
    s
}

/// Die Kennung des Starts im Inhalt; `/app.js` laedt die Seite neu, wenn sie wechselt (neue
/// Formulare nach einem Neustart).
pub fn marke(start: &str) -> String {
    format!("<span hidden data-start=\"{}\"></span>", maskiere(start))
}

/// Hinweis zum Update, falls es einen gibt.
fn update_hinweis(z: &Zustand) -> String {
    match &z.update_meldung {
        Some((fehler, text)) => format!(
            "<p class=\"kopf {}\">{}</p>",
            if *fehler { "halt" } else { "offen" },
            maskiere(text)
        ),
        None => String::new(),
    }
}

fn gesamtstatus_teil(z: &Zustand) -> String {
    let Some(b) = z.gesamtstatus() else {
        return String::new();
    };
    let mut s = format!(
        "<section id=\"betrieb\"><div class=\"kopf {}\">{}</div>",
        b.klasse(),
        maskiere(b.titel())
    );
    if !b.gruende().is_empty() {
        s.push_str("<ul>");
        for g in b.gruende() {
            let _ = write!(s, "<li>{}</li>", maskiere(g));
        }
        s.push_str("</ul>");
    }
    s.push_str("</section>");
    s
}

/// Der Inhalt der Live-Seite (auch `/teil/live`).
pub fn live(start: &str, z: &Zustand, heute: Option<Datum>) -> String {
    let mut s = marke(start);
    s.push_str(&gesamtstatus_teil(z));
    s.push_str(&update_hinweis(z));
    s.push_str(&taetigkeit(z, heute));
    match &z.lage {
        Some(l) => s.push_str(&l.html),
        None => s.push_str("<section><p class=\"leise\">Lagebild wird berechnet …</p></section>"),
    }
    s
}

/// Kurzer Stand fuer die Einrichtung (auch `/teil/status`).
pub fn status(start: &str, z: &Zustand, heute: Option<Datum>) -> String {
    let jetzt = match &z.laufend {
        Some(a) => format!("Läuft: {} ({}).", a.art.name(), anlass_text(a.anlass)),
        None => "Gerade läuft nichts.".into(),
    };
    let lage = z
        .lage
        .as_ref()
        .map(|l| l.naechster_schritt.clone())
        .unwrap_or_default();
    format!(
        "{}{}<section>{}<div class=\"kopf\">{}</div><div>{}</div>\
         <div class=\"leise\">Nächster geplanter Lauf: {}</div></section>",
        marke(start),
        gesamtstatus_teil(z),
        update_hinweis(z),
        maskiere(&jetzt),
        maskiere(&lage),
        maskiere(&naechster_text(z, heute))
    )
}

fn regel(w: Wer) -> &'static str {
    match w {
        Wer::Kursquelle => "8 bis 64 Großbuchstaben und Ziffern",
        Wer::KursquelleUs => "20 bis 64 Kleinbuchstaben und Ziffern",
        Wer::Berater(_) => "20 bis 256 Zeichen aus A-Z, a-z, 0-9, _ und -",
    }
}

/// Eingaben des Zeitplan-Formulars, die nach einer Ablehnung wieder im Formular stehen. Nur
/// gueltige Werte werden uebernommen; ein Schluessel ist nie dabei.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Entwurf {
    pub termine: Option<(u8, u8)>,
    pub kurse: Option<(u8, u8)>,
    pub papier_danach: Option<bool>,
}

impl Entwurf {
    /// Aus einem Formular oder der Abfrage einer Seite. Ohne `entwurf=ja` ist nichts zu
    /// uebernehmen (sonst waeren fehlende Haken nicht von einem fehlenden Entwurf zu trennen).
    pub fn aus_formular(f: &[(String, String)]) -> Entwurf {
        let haken = |n: &str| match feld(f, n) {
            None => Some(false),
            Some("ja") => Some(true),
            Some(_) => None,
        };
        let zeit = |n: &str| feld(f, n).and_then(lies_uhrzeit);
        let ist_formular = feld(f, "t").is_some() || feld(f, "entwurf") == Some("ja");
        if !ist_formular {
            return Entwurf::default();
        }
        Entwurf {
            termine: zeit("termine"),
            kurse: zeit("kurse"),
            papier_danach: haken("papier_danach"),
        }
    }

    /// Als Teil einer Abfrage (`&entwurf=ja&...`), nur mit gueltigen Werten.
    pub fn als_abfrage(&self) -> String {
        let mut q = String::from("&entwurf=ja");
        if self.papier_danach == Some(true) {
            q.push_str("&papier_danach=ja");
        }
        for (n, w) in [("termine", self.termine), ("kurse", self.kurse)] {
            if let Some(u) = w {
                let _ = write!(q, "&{n}={}", uhrzeit_text(u).replace(':', "%3A"));
            }
        }
        q
    }

    fn leer(self) -> bool {
        self == Entwurf::default()
    }
}

/// Was die Einrichtung ueber Gespeichertes weiss.
#[derive(Clone, Debug)]
pub struct Einrichtung {
    /// Stand je Schluessel aus Anmeldeverwaltung und Anbieterprotokoll.
    pub schluessel: Vec<(Wer, SchluesselStand)>,
    /// Inhalt von `daten/ki.halt`, falls die Sperre des Kostenschutzes liegt.
    pub ki_halt: Result<Option<String>, String>,
    /// Inhalt von `daten/kurse.halt`, falls die Kurssperre liegt.
    pub kurse_halt: Result<Option<String>, String>,
    /// Journalabdruck und Bargeld fuer eine freigabefaehige Papier-Sperre.
    pub papier_freigabe: Option<depot_lage::PapierFreigabeAngebot>,
    /// `Some(true)`: Datei stimmt mit dem aktiven Zeitplan ueberein; `Some(false)`: weicht ab
    /// oder ist unlesbar; `None`: keine Datei (Voreinstellung).
    pub zeitplan_gespeichert: Option<bool>,
    /// Abgelehnte Eingaben, die wieder im Formular stehen.
    pub entwurf: Entwurf,
    pub autostart: autostart::Stand,
    pub wurzel: String,
    pub grenzen: Vec<Grenzwert>,
    /// Staende, die nach einem Update zurueckgesetzt wurden (`daten/update_abgelehnt.txt`),
    /// oder warum die Datei nicht lesbar ist.
    pub abgelehnt: Result<Vec<String>, String>,
}

fn schluessel_teil(token: &str, e: &Einrichtung, s: &mut String) {
    s.push_str(
        "<section id=\"schluessel\"><h2>Schlüssel</h2><p class=\"leise\">Gespeichert wird nur in \
         der Windows-Anmeldeinformationsverwaltung, nie im Projektordner. Die Seite zeigt keinen \
         Schlüssel, nur ob einer hinterlegt ist und was der Anbieter beim letzten Aufruf dazu \
         sagte. Geprüft wird nur bei echten Läufen, nie mit eigenen, kostenpflichtigen \
         Testaufrufen. <b>Speichern heißt hier zugleich aktiv:</b> Der nächste Lauf liest ihn.</p><div class=\"breit\"><table>\
         <tr><th>Wofür</th><th>Stand</th><th>Neu hinterlegen</th></tr>",
    );
    for (w, st) in &e.schluessel {
        let klasse = st.klasse();
        let text = maskiere(&st.text());
        let _ = write!(
            s,
            "<tr><td>{}<br><span class=\"leise\">{}</span></td>\
             <td><span class=\"{klasse}\">{text}</span></td><td>\
             <form method=\"post\" action=\"/schluessel\">\
             <input type=\"hidden\" name=\"t\" value=\"{}\">\
             <input type=\"hidden\" name=\"wer\" value=\"{}\">\
             <input type=\"password\" name=\"wert\" autocomplete=\"off\" required> \
             <button type=\"submit\">Speichern</button>\
             <div class=\"leise\">{}</div></form></td></tr>",
            maskiere(w.name()),
            maskiere(w.adresse()),
            maskiere(token),
            w.kurz(),
            regel(*w)
        );
    }
    s.push_str("</table></div></section>");
}

/// Was bei einer abgebrochenen KI-Anfrage geschieht, in Alltagssprache (Maus, 05.11.2026).
pub const KI_VERBINDUNG: &str = "<details id=\"ki-verbindung\"><summary>Wenn die Verbindung bei \
     einer KI-Anfrage wegbricht</summary><ul>\
     <li><b>Die Anfrage ist gar nicht erst hinausgegangen</b> (kein Internet, Adresse nicht \
     gefunden, Verbindung abgewiesen): Sie hat nichts gekostet. DepotSchlopf versucht es nach 30 \
     und nach 60 Sekunden von selbst noch einmal. Klappt es dann nicht, läuft der Papiertag \
     heute ohne diese KI. Du musst nichts tun.</li>\
     <li><b>Die Anfrage war schon abgeschickt, dann brach die Verbindung ab</b>: Sie kann beim \
     Anbieter angekommen sein und Geld gekostet haben, ohne dass DepotSchlopf die Antwort kennt. \
     Ein zweiter Versuch könnte doppelt kosten. Deshalb fragt DepotSchlopf nicht noch einmal, \
     sondern sperrt beide KIs. Du siehst im Konto des Anbieters nach, was der Aufruf gekostet \
     hat, und hebst die Sperre hier auf.</li></ul></details>";

fn kostenschutz_teil(token: &str, e: &Einrichtung, s: &mut String) {
    s.push_str("<section id=\"kostenschutz\"><h2>Sperre des Kostenschutzes</h2>");
    let inhalt = match &e.ki_halt {
        Ok(None) => {
            s.push_str(
                "<p><span class=\"gut\">frei</span>: Keine Sperre. Jeder KI-Aufruf durchläuft \
                 trotzdem die Kostenprüfung.</p>",
            );
            s.push_str(KI_VERBINDUNG);
            s.push_str("</section>");
            return;
        }
        Ok(Some(t)) => t,
        Err(f) => {
            let _ = write!(
                s,
                "<p class=\"halt\">{}. Die KI wird nicht gefragt, bis die Datei lesbar ist.</p></section>",
                maskiere(f)
            );
            return;
        }
    };
    let _ = write!(
        s,
        "<p><span class=\"halt\">gesperrt</span>: Ein KI-Aufruf hat mehr gekostet als \
         reserviert, oder seine Kosten sind unbekannt, etwa weil die Verbindung nach dem \
         Absenden abbrach. Bis zur Prüfung fragt der Papiertag keine KI.</p><pre>{}</pre>{}\
         <p class=\"leise\">So prüfen: im Konto des Anbieters die Kosten des genannten Aufrufs \
         ansehen. Aufheben ändert nichts am Kassenbuch: Die Reservierung bleibt gebucht und zählt \
         im Budget. Jeder weitere Aufruf durchläuft wieder die Kostenprüfung und sperrt bei einer \
         Abweichung erneut. Der Grund bleibt in daten/ki_halt_aufgehoben.txt.</p>\
         <form method=\"post\" action=\"/kostenschutz\">\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <input type=\"hidden\" name=\"abdruck\" value=\"{}\">\
         <label><input type=\"checkbox\" name=\"geprueft\" value=\"ja\" required> \
         Ich habe die Kosten beim Anbieter geprüft.</label>\
         <button type=\"submit\">Sperre aufheben</button></form></section>",
        maskiere(inhalt),
        KI_VERBINDUNG,
        maskiere(token),
        kostenschutz::abdruck(inhalt)
    );
}

fn kurssperre_teil(token: &str, e: &Einrichtung, s: &mut String) {
    let inhalt = match &e.kurse_halt {
        // Ohne Kurssperre kein eigener Abschnitt: Sie kommt nur nach einem Sicherheitsabbruch vor.
        Ok(None) => return,
        Ok(Some(t)) => t,
        Err(f) => {
            let _ = write!(
                s,
                "<section id=\"kurssperre\"><h2>Kurssperre</h2><p class=\"halt\">{}. Kein \
                 Abruf und kein Papiertag, bis die Datei lesbar ist.</p></section>",
                maskiere(f)
            );
            return;
        }
    };
    let _ = write!(
        s,
        "<section id=\"kurssperre\"><h2>Kurssperre</h2>\
         <p><span class=\"halt\">gesperrt</span>: Ein Kursabruf endete aus Sicherheitsgründen, \
         weil die Kursquelle den Schlüssel zurückgab. Bis zur Prüfung ruft der Sammler nichts ab, \
         und kein Papiertag beginnt, auch nicht von Hand.</p><pre>{}</pre>\
         <p class=\"leise\">So prüfen: Beim Anbieter nachsehen, ob der Schlüssel noch gilt, und \
         ihn bei Zweifel neu ausstellen und unter Schlüssel neu hinterlegen. Aufheben ändert nichts \
         am Journal. Der nächste Abruf holt alle Kurse neu und prüft sie wie immer. Der Grund \
         bleibt in daten/kurse_halt_aufgehoben.txt.</p>\
         <form method=\"post\" action=\"/kurssperre\">\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <input type=\"hidden\" name=\"abdruck\" value=\"{}\">\
         <label><input type=\"checkbox\" name=\"geprueft\" value=\"ja\" required> \
         Ich habe den Schlüssel der Kursquelle geprüft und bei Bedarf erneuert.</label>\
         <button type=\"submit\">Kurssperre aufheben</button></form></section>",
        maskiere(inhalt),
        maskiere(token),
        kostenschutz::abdruck(inhalt)
    );
}

fn quellen_teil(token: &str, z: &Zustand, s: &mut String) {
    s.push_str(
        "<section id=\"quellen\"><h2>Datenquellen und Verbindungstest</h2><p class=\"leise\">\
         Der Test ist der Abruf selbst (nur lesend); sein Protokoll steht auf Live unter \
         Tätigkeit.</p><div class=\"breit\"><table><tr><th>Quelle</th><th>Zuletzt</th><th></th></tr>",
    );
    for (art, name) in [
        (
            Art::Kurse,
            "Schlusskurse aller Börsen der Liste (Xetra: Alpha Vantage, NYSE: Tiingo mit \
             EZB-Referenzkursen; je mit Schlüssel)",
        ),
        (Art::Termine, "Termine der EZB (ohne Schlüssel)"),
    ] {
        // Ein Kursabruf je Boerse zaehlt fuer die Kursquellen mit.
        let gleich = |a: Art| a == art || (art == Art::Kurse && matches!(a, Art::KurseVon(_)));
        let zuletzt = z.verlauf.iter().find(|a| gleich(a.art)).map_or_else(
            || "<span class=\"leise\">seit Dienststart nicht</span>".into(),
            |a| {
                let (klasse, text) = ergebnis(a);
                format!(
                    "<span class=\"{klasse}\">{text}</span> {}",
                    maskiere(&zeit_text(a.start))
                )
            },
        );
        let _ = write!(
            s,
            "<tr><td>{}</td><td>{zuletzt}</td><td>{}</td></tr>",
            maskiere(name),
            auftrag_knopf(token, art, "Jetzt prüfen")
        );
    }
    s.push_str("</table></div></section>");
}

fn zeitplan_teil(token: &str, zustand: &Zustand, e: &Einrichtung, s: &mut String) {
    let p: &Zeitplan = &zustand.zeitplan;
    let fehler = zustand
        .erledigt_fehler
        .as_ref()
        .or(zustand.zeitplan_fehler.as_ref());
    let gespeichert = match (e.zeitplan_gespeichert, fehler) {
        (_, Some(f)) => format!("<p class=\"halt\">{}</p>", maskiere(f)),
        (Some(true), None) => "<p>Einstellungen gespeichert.</p>".into(),
        (Some(false), None) => "<p class=\"offen\">Datei weicht vom laufenden Stand ab.</p>".into(),
        (None, None) => "<p>Es gelten die voreingestellten Uhrzeiten.</p>".into(),
    };
    let haken = |b: bool| if b { " checked" } else { "" };
    let ent = &e.entwurf;
    let entwurf_hinweis = if ent.leer() {
        ""
    } else {
        "<p class=\"offen\">Das Formular zeigt deine letzten, nicht gespeicherten Eingaben.</p>"
    };
    let mut voraussetzung = String::new();
    let mut gesperrt = "";
    if !p.an {
        match zustand.einschalten_erlaubt() {
            Ok(()) => {}
            Err(ZeitplanAbgelehnt::Einrichtung(fehlt)) => {
                gesperrt = " disabled";
                voraussetzung
                    .push_str("<p>Einschalten geht erst, wenn diese Punkte erledigt sind.</p><ul>");
                for punkt in fehlt {
                    let _ = write!(voraussetzung, "<li>{}</li>", maskiere(&punkt));
                }
                voraussetzung.push_str("</ul><p><a href=\"#schluessel\">Schlüssel prüfen</a></p>");
            }
            Err(_) => {
                gesperrt = " disabled";
                voraussetzung.push_str(
                    "<p>Der Dienst prüft die Einrichtung. Lade den Stand \
                    nach ein paar Sekunden neu.</p>",
                );
            }
        }
    }
    if !gesperrt.is_empty() {
        voraussetzung.push_str(
            "<form method=\"get\" action=\"/einrichtung#zeitplan\">\
             <button type=\"submit\">Stand neu laden</button></form>",
        );
    }
    let (stand, aktion, knopf) = if p.an {
        ("An", "aus", "Zeitplan ausschalten")
    } else {
        ("Aus", "an", "Zeitplan einschalten")
    };
    let token = maskiere(token);
    let _ = write!(
        s,
        "<section id=\"zeitplan\"><h2>Zeitplan</h2>\
         <p><b>{stand}</b> · Empfohlen, wenn DepotSchlopf nach einem erfolgreichen manuellen \
         Test selbstständig laufen soll.</p>{gespeichert}{voraussetzung}\
         <form method=\"post\" action=\"/zeitplan\">\
         <input type=\"hidden\" name=\"t\" value=\"{token}\">\
         <input type=\"hidden\" name=\"aktion\" value=\"{aktion}\">\
         <button type=\"submit\"{gesperrt}>{knopf}</button></form>\
         <h3>Uhrzeiten und automatische Läufe</h3>\
         <p>Änderungen unten gelten erst nach dem Speichern. Der Schalter oben nutzt die zuletzt \
         gespeicherten Einstellungen. Läufe sind montags bis freitags, nach Frankfurter Zeit. \
         Für Xetra ist 17:45 Uhr voreingestellt, kurz nach Börsenschluss.</p>\
         {entwurf_hinweis}\
         <form method=\"post\" action=\"/zeitplan\">\
         <input type=\"hidden\" name=\"t\" value=\"{token}\">\
         <input type=\"hidden\" name=\"aktion\" value=\"speichern\">\
         <label>Termine abrufen um <input type=\"time\" name=\"termine\" value=\"{}\" required></label>\
         <label>Kurse Xetra abrufen um <input type=\"time\" name=\"kurse\" value=\"{}\" required></label>\
         <p class=\"leise\">Jede andere Börse der Liste wird zehn Minuten nach ihrem eigenen \
         Schluss abgerufen (NYSE meist 22:10 Uhr, mit Sommerzeit und verkürzten Tagen).</p>\
         <label><input type=\"checkbox\" name=\"papier_danach\" value=\"ja\"{}> \
         Nach dem letzten Kursabruf des Tages den Papiertag starten (wenn ein Abruf des Tages \
         erfolgreich war)</label>\
         <button type=\"submit\">Einstellungen speichern</button></form></section>",
        uhrzeit_text(ent.termine.unwrap_or(p.termine)),
        uhrzeit_text(ent.kurse.unwrap_or(p.kurse)),
        haken(ent.papier_danach.unwrap_or(p.papier_danach))
    );
}

fn autostart_teil(token: &str, e: &Einrichtung, s: &mut String) {
    s.push_str(
        "<section id=\"autostart\"><h2>Autostart mit Windows</h2>\
         <p>Optional. Empfohlen, wenn DepotSchlopf bei deiner Windows-Anmeldung von selbst \
         starten soll. Der Browser bleibt geschlossen.</p>",
    );
    let (text, knopf, wert, bestaetigung) = match e.autostart {
        autostart::Stand::An => (
            "Für diesen Projektordner eingeschaltet.",
            "Autostart ausschalten",
            "aus",
            "<label><input type=\"checkbox\" name=\"geprueft\" value=\"ja\" required> \
             Ich möchte DepotSchlopf künftig von Hand starten.</label>",
        ),
        autostart::Stand::Aus => ("Aus.", "Autostart einschalten", "an", ""),
        autostart::Stand::Fremd => {
            s.push_str("<p>Ein Autostart-Eintrag ist bereits vorhanden und gehört nicht zu \
                diesem Ordner. Öffne die Einrichtung im Hauptordner, wenn du ihn ändern willst.</p></section>");
            return;
        }
        autostart::Stand::NichtVerfuegbar => {
            s.push_str(
                "<p>Hier nicht verfügbar. Der Windows-Autostartordner oder sein Eintrag \
                ist nicht zugänglich, oder der Projektpfad ist ungeeignet. Du kannst \
                DepotSchlopf weiterhin von Hand starten.</p></section>",
            );
            return;
        }
    };
    let _ = write!(
        s,
        "<p>{text}</p><form method=\"post\" action=\"/autostart\">\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <input type=\"hidden\" name=\"an\" value=\"{wert}\">\
         {bestaetigung}<button type=\"submit\">{knopf}</button></form></section>",
        maskiere(token)
    );
}

fn update_teil(token: &str, z: &Zustand, e: &Einrichtung, s: &mut String) {
    s.push_str(
        "<section id=\"update\"><h2>Update</h2><p class=\"leise\">Das Update folgt nur \
         <code>main</code> auf GitHub. Einspielen prüft den neuen Stand zuerst mit dem ganzen Gate \
         in einem eigenen Ordner. Danach prüft der neue Stand eine Kopie von Journal und Papierkonto \
         und sichert die bisherigen Papierdaten unter <code>daten/sicherung</code>. Erst wenn alle \
         Prüfungen erfolgreich sind, rückt der Projektordner vorwärts, ohne \
         Lokales zu überschreiben. Danach startet der Dienst neu. Kommt er nicht hoch, setzt \
         DepotSchlopf.cmd den alten Stand zurück. Das Gate dauert einige Minuten.</p>",
    );
    s.push_str(&update_hinweis(z));
    if let Some((fehler, text)) = &z.programm_meldung {
        let _ = write!(
            s,
            "<p id=\"programmstand\"><span class=\"{}\">Start nach einem Neustart:</span> {}</p>",
            if *fehler { "offen" } else { "gut" },
            maskiere(text)
        );
    }
    for art in [Art::UpdatePruefen, Art::Update] {
        // Ein Kursabruf je Boerse zaehlt fuer die Kursquellen mit.
        let gleich = |a: Art| a == art || (art == Art::Kurse && matches!(a, Art::KurseVon(_)));
        let zuletzt = z.verlauf.iter().find(|a| gleich(a.art)).map_or_else(
            || "seit Dienststart nicht".to_string(),
            |a| {
                let (klasse, text) = ergebnis(a);
                format!(
                    "<span class=\"{klasse}\">{text}</span> {}",
                    maskiere(&zeit_text(a.start))
                )
            },
        );
        let _ = write!(
            s,
            "<p>{} <span class=\"leise\">Zuletzt: {zuletzt}</span></p>",
            auftrag_knopf(token, art, art.name())
        );
    }
    match &e.abgelehnt {
        Ok(l) if l.is_empty() => {}
        Ok(l) => {
            let _ = write!(
                s,
                "<p class=\"leise\">Zurückgesetzt und nicht mehr angeboten: {}</p>",
                maskiere(&l.join(", "))
            );
        }
        Err(f) => {
            let _ = write!(s, "<p class=\"halt\">{}</p>", maskiere(f));
        }
    }
    s.push_str("</section>");
}

fn grenzen_teil(e: &Einrichtung, s: &mut String) {
    s.push_str(
        "<section id=\"grenzen\"><h2>Grenzen und Budgets</h2><p class=\"leise\">Nur zur Ansicht. \
         Diese Werte stehen in der Festschreibung und ändern sich nur über eine Grenzfreigabe, \
         nie über diese Seite. Verbrauch und Rest je KI stehen auf Live.</p><table>",
    );
    for g in &e.grenzen {
        let _ = write!(
            s,
            "<tr><td>{}</td><td class=\"z\">{}</td></tr>",
            maskiere(g.name),
            maskiere(&g.wert)
        );
    }
    s.push_str("</table></section>");
}

fn papierfreigabe_teil(token: &str, e: &Einrichtung, s: &mut String) {
    let Some(angebot) = &e.papier_freigabe else {
        return;
    };
    let _ = write!(
        s,
        "<section id=\"papierfreigabe\"><h2>Papierhandel nach Verlustsperre freigeben</h2>\
         <p>Die neue Verlustmessung beginnt nach dem Kontenabgleich bei <strong>{}</strong>. \
         Gesamtverlust seit dem historischen Höchststand: <strong>{}</strong>. \
         Der historische Höchststand bleibt erhalten. Nach dem Absenden musst du die Freigabe \
         innerhalb von zwei Minuten im separaten Windows-Dialog bestätigen.</p>\
         <form method=\"post\" action=\"/papierfreigabe\">\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <input type=\"hidden\" name=\"abdruck\" value=\"{}\">\
         <label><input type=\"checkbox\" name=\"geprueft\" value=\"ja\" required> \
         Ich bestätige die neue Verlustmessung für den Papierhandel.</label>\
         <button type=\"submit\">Papierhandel freigeben</button></form></section>",
        maskiere(&angebot.neuer_start_text()),
        maskiere(&angebot.gesamtverlust_text()),
        maskiere(token),
        maskiere(&angebot.abdruck)
    );
}

/// Der Inhalt der Einrichtung.
pub fn einrichtung(
    token: &str,
    start: &str,
    z: &Zustand,
    heute: Option<Datum>,
    e: &Einrichtung,
) -> String {
    let mut s = String::with_capacity(16_384);
    let _ = write!(
        s,
        "<div id=\"stand\" data-teil=\"/teil/status\">{}</div>",
        status(start, z, heute)
    );
    s.push_str(
        "<section><h2>So fängst du an</h2><ol>\
         <li><a href=\"#schluessel\">Schlüssel einrichten</a> und auf <a href=\"/\">Live</a> prüfen, \
         was für einen manuellen Papiertag noch fehlt.</li>\
         <li>Wenn der manuelle Test klappt, kannst du den <a href=\"#zeitplan\">Zeitplan</a> \
         für automatische Läufe einschalten.</li>\
         <li><a href=\"#autostart\">Autostart</a> ist optional. Schalte ihn ein, wenn DepotSchlopf \
         bei deiner Windows-Anmeldung von selbst starten soll.</li></ol>\
         <p><a href=\"#update\">Zum Update</a> · \
         <a href=\"#papierdialog-test\">Freigabedialog gefahrlos testen</a></p></section>",
    );
    schluessel_teil(token, e, &mut s);
    kostenschutz_teil(token, e, &mut s);
    kurssperre_teil(token, e, &mut s);
    papierfreigabe_teil(token, e, &mut s);
    let _ = write!(
        s,
        "<section id=\"papierdialog-test\"><h2>Windows-Dialog prüfen</h2>\
         <p>Zeigt den Freigabedialog als Test. Es wird nichts freigegeben oder gespeichert.</p>\
         <form method=\"post\" action=\"/papierdialog-test\">\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <button type=\"submit\">Freigabedialog testen</button></form></section>",
        maskiere(token)
    );
    quellen_teil(token, z, &mut s);
    zeitplan_teil(token, z, e, &mut s);
    autostart_teil(token, e, &mut s);
    update_teil(token, z, e, &mut s);
    grenzen_teil(e, &mut s);
    let _ = write!(
        s,
        "<section><h2>Dienst</h2><p>Erreichbar nur auf diesem Rechner: \
         http://127.0.0.1:{}/ · Projektordner {}</p></section>",
        crate::http::PORT,
        maskiere(&e.wurzel)
    );
    s
}

/// Das einzige Skript: laedt den Bereich mit `data-teil` alle zwei Sekunden neu, laesst
/// aufgeklappte Protokolle offen und zeigt, wenn der Dienst nicht antwortet. Antwortet ein neu
/// gestarteter Dienst (andere `data-start`), laedt es die ganze Seite neu: Ihre Formulare
/// tragen dann das Token des neuen Starts.
pub const APP_JS: &str = r#""use strict";
(function () {
  // Doppelklicks: Ein abgesendetes Formular sperrt seinen Knopf. Der Dienst lehnt doppelte
  // Auftraege ohnehin ab; so sieht man sofort, dass der Klick angekommen ist.
  document.addEventListener("submit", function (e) {
    var b = e.target.querySelector("button[type=submit]");
    if (b) {
      if (b.disabled) { e.preventDefault(); return; }
      if (b.dataset.gesendet) { e.preventDefault(); return; }
      b.dataset.gesendet = b.textContent;
      setTimeout(function () { b.disabled = true; b.textContent = "wird gesendet …"; }, 0);
    }
  }, true);
  // Zurueck im Browser zeigt die Seite aus dem Speicher: Knoepfe wieder freigeben.
  window.addEventListener("pageshow", function () {
    document.querySelectorAll("button[data-gesendet]").forEach(function (b) {
      b.textContent = b.dataset.gesendet;
      delete b.dataset.gesendet;
      b.disabled = false;
    });
  });
  var ziel = document.querySelector("[data-teil]");
  var banner = document.getElementById("getrennt");
  if (!ziel) { return; }
  var url = ziel.getAttribute("data-teil");
  var marke = document.querySelector("[data-start]");
  var start = marke ? marke.getAttribute("data-start") : null;
  var zuletzt = null;
  var fehler = 0;
  function offen() {
    var s = {};
    ziel.querySelectorAll("details[id]").forEach(function (d) { s[d.id] = d.open; });
    return s;
  }
  function hole() {
    fetch(url, { cache: "no-store", credentials: "same-origin" })
      .then(function (r) { if (!r.ok) { throw new Error(String(r.status)); } return r.text(); })
      .then(function (t) {
        var m = /data-start="([^"]*)"/.exec(t);
        if (start && m && m[1] !== start) {
          window.location.replace(window.location.pathname);
          return;
        }
        if (t !== zuletzt) {
          var s = offen();
          ziel.innerHTML = t;
          zuletzt = t;
          Object.keys(s).forEach(function (id) {
            var d = document.getElementById(id);
            if (d) { d.open = s[id]; }
          });
        }
        banner.hidden = true;
        fehler = 0;
      })
      .catch(function () { banner.hidden = false; fehler += 1; })
      .then(function () { setTimeout(hole, fehler ? Math.min(10000, 2000 * fehler) : 2000); });
  }
  setTimeout(hole, 2000);
})();
"#;
