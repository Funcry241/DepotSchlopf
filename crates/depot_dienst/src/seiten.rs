//! Die zwei Seiten der Oberflaeche: **Live** (Taetigkeit, naechster Lauf, Lagebild mit Budget,
//! Depot und Gespraech) und **Einrichtung** (die eine Config-Seite). Jeder Text ist maskiert;
//! das einzige Skript ist `/app.js` (Auffrischen und Verbindungsanzeige).

use std::fmt::Write as _;

use depot_kern::Datum;
use depot_lage::html::{STIL, maskiere};
use depot_lage::{Abhilfe, Grenzwert, Hinterlegt, datum_text, zeit_text};

use crate::schluessel::Wer;
use crate::werkbank::{Anlass, Art, Auftrag, Zustand};
use crate::zeitplan::{Zeitplan, naechster, uhrzeit_text};

const STIL_DIENST: &str = "\
nav{display:flex;gap:16px;align-items:baseline;flex-wrap:wrap}nav a{color:inherit}\
nav a.hier{font-weight:600;text-decoration:none}\
form.knopf{display:inline;margin:0}\
button{font:inherit;padding:4px 10px;border:1px solid var(--linie);border-radius:6px;\
background:var(--flaeche);color:var(--text);cursor:pointer}\
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
        Abhilfe::SchluesselKursquelle | Abhilfe::SchluesselBerater(_) => {
            Some("<a href=\"/einrichtung#schluessel\">Zur Einrichtung</a>".into())
        }
        Abhilfe::Pruefen | Abhilfe::Warten => None,
    }
}

/// Feste Meldungen nach einem Formular; nur bekannte Kennungen erscheinen.
pub fn meldung(abfrage: &str) -> Option<(&'static str, &'static str)> {
    let code = abfrage
        .split('&')
        .find_map(|p| p.strip_prefix("m="))
        .unwrap_or_default();
    Some(match code {
        "auftrag" => ("gut", "Auftrag eingereiht. Er steht unter Tätigkeit."),
        "auftrag_belegt" => (
            "offen",
            "Nicht eingereiht: Dieser Auftrag läuft oder wartet schon, oder es warten zu viele.",
        ),
        "schluessel" => (
            "gut",
            "Schlüssel in der Anmeldeinformationsverwaltung gespeichert. Er gilt ab dem nächsten Lauf.",
        ),
        "schluessel_ungueltig" => (
            "halt",
            "Schlüssel nicht gespeichert: Länge oder Zeichen passen nicht (Regel steht in der Zeile).",
        ),
        "schluessel_fehler" => (
            "halt",
            "Schlüssel nicht gespeichert: Die Anmeldeinformationsverwaltung hat abgelehnt.",
        ),
        "zeitplan" => ("gut", "Zeitplan gespeichert und aktiv."),
        "zeitplan_ungueltig" => (
            "halt",
            "Zeitplan nicht gespeichert: Uhrzeit im Format hh:mm.",
        ),
        "zeitplan_fehler" => (
            "halt",
            "Zeitplan nicht gespeichert: Datei nicht schreibbar. Der alte gilt weiter.",
        ),
        "autostart_an" => (
            "gut",
            "Autostart eingerichtet: ab dem nächsten Windows-Start.",
        ),
        "autostart_aus" => ("gut", "Autostart entfernt."),
        "autostart_fehler" => (
            "halt",
            "Autostart nicht geändert (Ordner oder Pfad ungeeignet).",
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
         <div id=\"getrennt\" class=\"banner\" hidden>Verbindung zum Dienst getrennt. \
         Die Seite verbindet sich von selbst neu.</div><main>\
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

/// Der naechste Lauf des Zeitplans als Satz.
pub fn naechster_text(z: &Zustand, heute: Option<Datum>) -> String {
    if let Some(f) = &z.zeitplan_fehler {
        return format!("Zeitplan aus: {f}");
    }
    if !z.zeitplan.an {
        return "Zeitplan aus. Einschalten unter Einrichtung.".into();
    }
    let Some(heute) = heute else {
        return "Uhrzeit unklar (Zeitumstellung); der Zeitplan wartet.".into();
    };
    match naechster(&z.zeitplan, &z.erledigt, heute) {
        Some((art, d, u)) => {
            let mut t = format!(
                "{} am {} {} um {}",
                art.name(),
                wochentag_text(d),
                datum_text(d),
                uhrzeit_text(u)
            );
            if art == Art::Kurse && z.zeitplan.papier_danach {
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
    match a.erfolg {
        Some(true) => ("gut", "erfolgreich"),
        Some(false) => ("halt", "abgebrochen"),
        None => ("offen", "läuft"),
    }
}

/// Was der Dienst tut: laufender Auftrag, Warteschlange, naechster Lauf, Verlauf.
pub fn taetigkeit(z: &Zustand, heute: Option<Datum>) -> String {
    let mut s = String::from("<section><h2>Tätigkeit</h2>");
    match &z.laufend {
        Some(a) => {
            let _ = write!(
                s,
                "<p class=\"kopf offen\">Läuft: {} ({}) seit {}</p><p class=\"leise\">Kennung {}</p>{}",
                maskiere(a.art.name()),
                anlass_text(a.anlass),
                maskiere(&zeit_text(a.start)),
                maskiere(&a.kennung),
                protokoll("l", a, true)
            );
        }
        None => s.push_str("<p class=\"kopf\">Gerade läuft nichts.</p>"),
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
                 <td><span class=\"{klasse}\">{text}</span>{}</td></tr>",
                maskiere(a.art.name()),
                maskiere(&a.kennung),
                anlass_text(a.anlass),
                maskiere(&zeit_text(a.start)),
                a.ende.map(|e| maskiere(&zeit_text(e))).unwrap_or_default(),
                protokoll("v", a, false)
            );
        }
        s.push_str("</table></div>");
    }
    s.push_str("</section>");
    s
}

/// Der Inhalt der Live-Seite (auch `/teil/live`).
pub fn live(z: &Zustand, heute: Option<Datum>) -> String {
    let mut s = taetigkeit(z, heute);
    match &z.lage {
        Some(l) => s.push_str(&l.html),
        None => s.push_str("<section><p class=\"leise\">Lagebild wird berechnet …</p></section>"),
    }
    s
}

/// Kurzer Stand fuer die Einrichtung (auch `/teil/status`).
pub fn status(z: &Zustand, heute: Option<Datum>) -> String {
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
        "<section><div class=\"kopf\">{}</div><div>{}</div>\
         <div class=\"leise\">Nächster geplanter Lauf: {}</div></section>",
        maskiere(&jetzt),
        maskiere(&lage),
        maskiere(&naechster_text(z, heute))
    )
}

fn hinterlegt_text(h: Hinterlegt) -> (&'static str, &'static str) {
    match h {
        Hinterlegt::Ja => ("gut", "hinterlegt"),
        Hinterlegt::Nein => ("halt", "fehlt"),
        Hinterlegt::Unbekannt => ("offen", "unbekannt (nur unter Windows)"),
    }
}

fn regel(w: Wer) -> &'static str {
    match w {
        Wer::Kursquelle => "8 bis 64 Großbuchstaben und Ziffern",
        Wer::Berater(_) => "20 bis 256 Zeichen aus A-Z, a-z, 0-9, _ und -",
    }
}

/// Was die Einrichtung ueber Gespeichertes weiss.
#[derive(Clone, Debug)]
pub struct Einrichtung {
    pub schluessel: Vec<(Wer, Hinterlegt)>,
    /// `Some(true)`: Datei stimmt mit dem aktiven Zeitplan ueberein; `Some(false)`: weicht ab
    /// oder ist unlesbar; `None`: keine Datei (Voreinstellung).
    pub zeitplan_gespeichert: Option<bool>,
    pub autostart: bool,
    pub wurzel: String,
    pub grenzen: Vec<Grenzwert>,
}

fn schluessel_teil(token: &str, e: &Einrichtung, s: &mut String) {
    s.push_str(
        "<section id=\"schluessel\"><h2>Schlüssel</h2><p class=\"leise\">Gespeichert wird nur in \
         der Windows-Anmeldeinformationsverwaltung, nie im Projektordner. Die Seite zeigt keinen \
         Schlüssel, nur ob einer hinterlegt ist. <b>Speichern heißt hier zugleich aktiv:</b> \
         Der nächste Lauf liest ihn.</p><div class=\"breit\"><table>\
         <tr><th>Wofür</th><th>Stand</th><th>Neu hinterlegen</th></tr>",
    );
    for (w, h) in &e.schluessel {
        let (klasse, text) = hinterlegt_text(*h);
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

fn quellen_teil(token: &str, z: &Zustand, s: &mut String) {
    s.push_str(
        "<section id=\"quellen\"><h2>Datenquellen und Verbindungstest</h2><p class=\"leise\">\
         Der Test ist der Abruf selbst (nur lesend); sein Protokoll steht auf Live unter \
         Tätigkeit.</p><div class=\"breit\"><table><tr><th>Quelle</th><th>Zuletzt</th><th></th></tr>",
    );
    for (art, name) in [
        (
            Art::Kurse,
            "Schlusskurse Xetra (Alpha Vantage, mit Schlüssel)",
        ),
        (Art::Termine, "Termine der EZB (ohne Schlüssel)"),
    ] {
        let zuletzt = z.verlauf.iter().find(|a| a.art == art).map_or_else(
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
    let gespeichert = match (e.zeitplan_gespeichert, &zustand.zeitplan_fehler) {
        (_, Some(f)) => format!("<span class=\"halt\">{}</span>", maskiere(f)),
        (Some(true), None) => "<span class=\"gut\">gespeichert und aktiv</span>".into(),
        (Some(false), None) => {
            "<span class=\"offen\">Datei weicht vom aktiven Zeitplan ab</span>".into()
        }
        (None, None) => {
            "<span class=\"offen\">Voreinstellung aktiv, noch nicht gespeichert</span>".into()
        }
    };
    let aktiv = if p.an {
        format!(
            "an: Termine {}, Kurse {}{}",
            uhrzeit_text(p.termine),
            uhrzeit_text(p.kurse),
            if p.papier_danach {
                ", danach Papiertag"
            } else {
                ""
            }
        )
    } else {
        "aus".into()
    };
    let haken = |b: bool| if b { " checked" } else { "" };
    let _ = write!(
        s,
        "<section id=\"zeitplan\"><h2>Zeitplan</h2>\
         <p>Aktiv: {} · {gespeichert}</p><p class=\"leise\">Montag bis Freitag, Frankfurter \
         Zeit. Jeder Lauf höchstens einmal am Tag, auch nach einem Neustart.</p>\
         <form method=\"post\" action=\"/zeitplan\">\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <label><input type=\"checkbox\" name=\"an\" value=\"ja\"{}> Zeitplan an</label>\
         <label>Termine abrufen um <input type=\"time\" name=\"termine\" value=\"{}\" required></label>\
         <label>Kurse abrufen um <input type=\"time\" name=\"kurse\" value=\"{}\" required></label>\
         <label><input type=\"checkbox\" name=\"papier_danach\" value=\"ja\"{}> \
         Nach erfolgreichem Kursabruf den Papiertag starten</label>\
         <button type=\"submit\">Speichern und aktivieren</button></form></section>",
        maskiere(&aktiv),
        maskiere(token),
        haken(p.an),
        uhrzeit_text(p.termine),
        uhrzeit_text(p.kurse),
        haken(p.papier_danach)
    );
}

fn autostart_teil(token: &str, e: &Einrichtung, s: &mut String) {
    let (klasse, text, knopf, wert) = if e.autostart {
        ("gut", "eingerichtet", "Autostart entfernen", "aus")
    } else {
        ("offen", "nicht eingerichtet", "Autostart einrichten", "an")
    };
    let _ = write!(
        s,
        "<section id=\"autostart\"><h2>Autostart mit Windows</h2>\
         <p>Stand: <span class=\"{klasse}\">{text}</span>. Gilt ab dem nächsten Windows-Start; \
         der Dienst startet dann ohne Browserfenster.</p>\
         <form class=\"knopf\" method=\"post\" action=\"/autostart\">\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <input type=\"hidden\" name=\"an\" value=\"{wert}\">\
         <button type=\"submit\">{knopf}</button></form></section>",
        maskiere(token)
    );
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

/// Der Inhalt der Einrichtung.
pub fn einrichtung(token: &str, z: &Zustand, heute: Option<Datum>, e: &Einrichtung) -> String {
    let mut s = String::with_capacity(16_384);
    let _ = write!(
        s,
        "<div id=\"stand\" data-teil=\"/teil/status\">{}</div>",
        status(z, heute)
    );
    schluessel_teil(token, e, &mut s);
    quellen_teil(token, z, &mut s);
    zeitplan_teil(token, z, e, &mut s);
    autostart_teil(token, e, &mut s);
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
/// aufgeklappte Protokolle offen und zeigt, wenn der Dienst nicht antwortet.
pub const APP_JS: &str = r#""use strict";
(function () {
  var ziel = document.querySelector("[data-teil]");
  var banner = document.getElementById("getrennt");
  if (!ziel) { return; }
  var url = ziel.getAttribute("data-teil");
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
