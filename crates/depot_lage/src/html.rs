//! Das Lagebild als HTML-Seite: ohne Skript, ohne fremde Quellen, jeder Text maskiert.

use std::fmt::Write as _;

use depot_kern::{BeraterId, Cent, TopfId};

use crate::budget::{BudgetStatus, euro};
use crate::{Abhilfe, Ampel, Lagebild, datum_text, zeit_text};

/// Maskiert Text fuer HTML, auch in Attributen.
pub fn maskiere(text: &str) -> String {
    let mut aus = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => aus.push_str("&amp;"),
            '<' => aus.push_str("&lt;"),
            '>' => aus.push_str("&gt;"),
            '"' => aus.push_str("&quot;"),
            '\'' => aus.push_str("&#39;"),
            c => aus.push(c),
        }
    }
    aus
}

fn geld(c: Option<Cent>) -> String {
    c.map_or_else(|| "–".into(), euro)
}

fn ampel_klasse(a: Ampel) -> &'static str {
    match a {
        Ampel::Gut => "gut",
        Ampel::Offen => "offen",
        Ampel::Halt => "halt",
    }
}

fn ampel_text(a: Ampel) -> &'static str {
    match a {
        Ampel::Gut => "bereit",
        Ampel::Offen => "offen",
        Ampel::Halt => "Halt",
    }
}

fn topf_text(t: TopfId) -> &'static str {
    match t {
        TopfId::A => "Topf A (Dachs)",
        TopfId::B => "Topf B (Maus)",
    }
}

fn berater_text(b: BeraterId) -> &'static str {
    match b {
        BeraterId::Dachs => "Dachs",
        BeraterId::Maus => "Maus",
    }
}

/// Das Stylesheet der Seiten, auch fuer den Dienst.
pub const STIL: &str = "\
:root{--grund:#f7f7f5;--flaeche:#fff;--text:#1d1d1b;--leise:#6b6b66;--linie:#e3e3de;\
--gut:#1f7a3d;--offen:#9a6700;--halt:#b42318;color-scheme:light dark}\
@media (prefers-color-scheme:dark){:root{--grund:#161615;--flaeche:#1f1f1d;--text:#ececea;\
--leise:#a3a39d;--linie:#33332f;--gut:#4cc27a;--offen:#e0a537;--halt:#f2675b}}\
*{box-sizing:border-box}body{margin:0;background:var(--grund);color:var(--text);\
font:15px/1.45 system-ui,-apple-system,'Segoe UI',sans-serif}\
main{max-width:960px;margin:0 auto;padding:16px}\
h1{font-size:20px;margin:8px 0 4px}h2{font-size:16px;margin:0 0 8px}\
section{background:var(--flaeche);border:1px solid var(--linie);border-radius:10px;\
padding:14px 16px;margin:12px 0}\
.leise{color:var(--leise)}.gut{color:var(--gut)}.offen{color:var(--offen)}.halt{color:var(--halt)}\
.marke{display:inline-block;min-width:56px;font-weight:600}\
table{border-collapse:collapse;width:100%}td,th{text-align:left;padding:4px 8px 4px 0;\
border-bottom:1px solid var(--linie);vertical-align:top}th{font-weight:600}\
td.z,th.z{text-align:right;font-variant-numeric:tabular-nums;white-space:nowrap}\
.kopf{font-size:17px;font-weight:600}.breit{overflow-x:auto}\
ol.gespraech{list-style:none;padding:0;margin:0}ol.gespraech li{padding:6px 0;\
border-bottom:1px solid var(--linie)}ol.gespraech b{display:inline-block;min-width:90px}";

fn kopf(l: &Lagebild, mit_betrieb: bool, s: &mut String) {
    if !mit_betrieb {
        let _ = write!(
            s,
            "<section><div class=\"kopf {}\">{}</div><div class=\"leise\">Stand {}</div></section>",
            ampel_klasse(l.gesamt),
            maskiere(&l.naechster_schritt),
            maskiere(&zeit_text(l.jetzt))
        );
        return;
    }
    let _ = write!(
        s,
        "<section id=\"betrieb\"><div class=\"kopf {}\">{}</div>",
        l.betrieb.klasse(),
        maskiere(l.betrieb.titel())
    );
    if !l.betrieb.gruende().is_empty() {
        s.push_str("<ul>");
        for g in l.betrieb.gruende() {
            let _ = write!(s, "<li>{}</li>", maskiere(g));
        }
        s.push_str("</ul>");
    }
    let _ = write!(
        s,
        "<div class=\"{}\">{}</div><div class=\"leise\">Stand {}</div></section>",
        ampel_klasse(l.gesamt),
        maskiere(&l.naechster_schritt),
        maskiere(&zeit_text(l.jetzt))
    );
}

/// Erzeugt zu einer Abhilfe fertiges, sicheres HTML (etwa einen Knopf) oder nichts.
pub type Knopf<'a> = &'a dyn Fn(Abhilfe) -> Option<String>;

fn punkte(l: &Lagebild, knopf: Knopf<'_>, s: &mut String) {
    s.push_str("<section><h2>Bereitschaft</h2><table>");
    for p in &l.punkte {
        let k = p.abhilfe.and_then(knopf).unwrap_or_default();
        let _ = write!(
            s,
            "<tr><td><span class=\"marke {}\">{}</span></td><td>{}</td><td>{}</td><td>{k}</td></tr>",
            ampel_klasse(p.ampel),
            ampel_text(p.ampel),
            maskiere(p.name),
            maskiere(&p.text)
        );
    }
    s.push_str("</table></section>");
}

fn budget(l: &Lagebild, s: &mut String) {
    if l.budget.is_empty() {
        return;
    }
    s.push_str(
        "<section><h2>KI-Budget</h2><div class=\"breit\"><table><tr><th>KI</th>\
         <th>Status</th><th class=\"z\">Verbraucht im Monat</th>\
         <th class=\"z\">Reserviert im Monat</th><th class=\"z\">Verfügbar im Monat</th>\
         <th class=\"z\">Heute</th><th class=\"z\">Jahr</th><th class=\"z\">je Aufruf</th>\
         </tr>",
    );
    for b in &l.budget {
        let klasse = match b.status {
            BudgetStatus::Bereit => "gut",
            BudgetStatus::Gesperrt(_) => "halt",
            _ => "offen",
        };
        let unbekannt = if b.monat_unbekannt > 0 {
            format!(" · {} Aufrufe unbekannt", b.monat_unbekannt)
        } else {
            String::new()
        };
        let _ = write!(
            s,
            "<tr><td>{}</td><td class=\"{klasse}\">{}</td>\
             <td class=\"z\">{}<br><span class=\"leise\">{} Aufrufe abgerechnet{unbekannt}</span></td>\
             <td class=\"z\">{} von {}<br><span class=\"leise\">{}/{} Aufrufe</span></td>\
             <td class=\"z\"><b>{}</b></td>\
             <td class=\"z\">{} · {}/{} Aufrufe</td>\
             <td class=\"z\">{} von {}</td><td class=\"z\">{}</td></tr>",
            berater_text(b.berater),
            maskiere(&b.status.text()),
            euro(b.monat_verbraucht),
            b.monat_abgerechnet,
            euro(b.monat_gebucht),
            euro(b.monat_grenze),
            b.monat_aufrufe,
            b.monat_aufrufe_max,
            euro(b.monat_rest),
            euro(b.heute_gebucht),
            b.heute_aufrufe,
            b.heute_aufrufe_max,
            euro(b.jahr_gebucht),
            euro(b.jahr_grenze),
            geld(b.je_aufruf),
        );
    }
    s.push_str(
        "</table></div><p class=\"leise\"><b>Verbraucht</b> ist, was die Anbieter je Aufruf \
         gemeldet haben (daten/anbieter.txt). <b>Reserviert</b> sind die Höchstkosten jedes \
         Aufrufs im KI-Kassenbuch des Journals; damit rechnet der Kostenschutz, auch wenn \
         weniger verbraucht wurde. <b>Verfügbar</b> ist die Monatsgrenze minus Reserviertes: \
         So viel darf in diesem Monat noch reserviert werden.</p></section>",
    );
}

/// Je Handelstag: lief der Papiertag, und wer von Dachs und Maus hat beigetragen.
fn tage(l: &Lagebild, s: &mut String) {
    if l.tage.is_empty() {
        return;
    }
    s.push_str(
        "<section id=\"tage\"><h2>Tagesergebnis</h2><p class=\"leise\">Je Handelstag seit dem \
         ersten Papiertag: ob er lief, und ob Dachs und Maus tatsächlich geantwortet haben. Ein \
         abgeschlossener Tag allein sagt nicht, dass eine KI mitgewirkt hat.</p>\
         <div class=\"breit\"><table><tr><th>Tag</th><th>Papiertag</th><th>Dachs</th>\
         <th>Maus</th></tr>",
    );
    for t in &l.tage {
        let ki = |b: BeraterId| {
            t.ki.iter().find(|(x, _)| *x == b).map_or_else(
                || "<span class=\"leise\">–</span>".to_string(),
                |(_, k)| format!("<span class=\"{}\">{}</span>", k.klasse, maskiere(&k.text)),
            )
        };
        let _ = write!(
            s,
            "<tr><td>{}</td><td><span class=\"{}\">{}</span></td><td>{}</td><td>{}</td></tr>",
            datum_text(t.datum),
            t.stand.klasse(),
            maskiere(&t.stand.text()),
            ki(BeraterId::Dachs),
            ki(BeraterId::Maus)
        );
    }
    s.push_str("</table></div></section>");
}

fn depot(l: &Lagebild, s: &mut String) {
    let Some(d) = &l.depot else {
        return;
    };
    s.push_str(
        "<section><h2>Papierdepot</h2><p class=\"leise\">Nur Papiergeld, simuliert: Jeder \
         Neue Orders warten auf einen geprüften Schlusskurs nach ihrem Versand. Ob eine echte \
         Order zu diesem Preis ausgeführt worden wäre, bleibt offen.</p>",
    );
    if d.toepfe.is_empty() {
        s.push_str("<p class=\"leise\">Noch kein Depot: Es entsteht mit dem ersten Papiertag.</p>");
    } else {
        let _ = write!(
            s,
            "<p>Gesamtwert <b>{}</b> · Höchststand {} · letzter Papiertag {} · bewertet mit \
             Schlusskursen vom {}</p>",
            geld(d.gesamt),
            geld(d.hoechststand),
            d.letzter_abschluss.map_or_else(|| "–".into(), datum_text),
            d.kurse_vom.map_or_else(|| "–".into(), datum_text)
        );
        s.push_str(
            "<div class=\"breit\"><table><tr><th>Topf</th><th>Position</th><th class=\"z\">Stück</th>\
             <th class=\"z\">Wert</th></tr>",
        );
        for t in &d.toepfe {
            let _ = write!(
                s,
                "<tr><td><b>{}</b></td><td>Bargeld</td><td></td><td class=\"z\">{}</td></tr>",
                topf_text(t.topf),
                euro(t.bargeld)
            );
            for p in &t.positionen {
                let _ = write!(
                    s,
                    "<tr><td></td><td>{}</td><td class=\"z\">{}</td><td class=\"z\">{}</td></tr>",
                    maskiere(&p.name),
                    p.stueck,
                    geld(p.wert)
                );
            }
            let _ = write!(
                s,
                "<tr><td></td><td><i>zusammen</i></td><td></td><td class=\"z\"><i>{}</i></td></tr>",
                geld(t.wert)
            );
        }
        s.push_str("</table></div>");
    }
    if !d.auftraege.is_empty() {
        s.push_str(
            "<h2 style=\"margin-top:12px\">Aufträge</h2><div class=\"breit\"><table><tr>\
             <th class=\"z\">Nr</th><th>Topf</th><th>Wert</th><th>Richtung</th>\
             <th class=\"z\">Stück</th><th class=\"z\">Limit</th><th>Anlass</th><th>Zustand</th></tr>",
        );
        for a in &d.auftraege {
            let _ = write!(
                s,
                "<tr><td class=\"z\">{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"z\">{}</td>\
                 <td class=\"z\">{}</td><td>{}</td><td>{}</td></tr>",
                a.nr,
                topf_text(a.topf),
                maskiere(&a.name),
                a.richtung,
                a.stueck,
                euro(a.limit),
                a.anlass,
                maskiere(&a.zustand)
            );
        }
        s.push_str("</table></div>");
    }
    s.push_str("</section>");
}

fn gespraech(l: &Lagebild, s: &mut String) {
    s.push_str("<section><h2>Gespräch und Begründungen</h2>");
    if l.gespraech.is_empty() {
        s.push_str("<p class=\"leise\">Noch keine Begründungen im Journal.</p>");
    } else {
        s.push_str("<ol class=\"gespraech\">");
        for b in &l.gespraech {
            let _ = write!(
                s,
                "<li><span class=\"leise\">{}</span> <b>{}</b> {}</li>",
                datum_text(b.datum),
                maskiere(&b.wer),
                maskiere(&b.text)
            );
        }
        s.push_str("</ol>");
    }
    s.push_str("</section>");
}

fn grenzen(l: &Lagebild, s: &mut String) {
    s.push_str("<section><h2>Grenzen und Budgets</h2><table>");
    for g in &l.grenzen {
        let _ = write!(
            s,
            "<tr><td>{}</td><td class=\"z\">{}</td></tr>",
            maskiere(g.name),
            maskiere(&g.wert)
        );
    }
    s.push_str(
        "</table><p class=\"leise\">Diese Werte gelten. Sie stehen in der Festschreibung und \
         ändern sich nur über eine Grenzfreigabe, nie mit einem Klick.</p></section>",
    );
}

/// Die Abschnitte des Lagebilds ohne Seitenrahmen. `knopf` setzt zu jeder Abhilfe ein
/// Bedienelement; ohne Bedienung `&|_| None`.
pub fn inhalt(l: &Lagebild, knopf: Knopf<'_>) -> String {
    inhalt_mit(l, knopf, true)
}

/// Wie `inhalt`; ohne Gesamtstatus, wenn die Oberflaeche ihn selbst zeigt (mit dem Zeitplan).
pub fn inhalt_mit(l: &Lagebild, knopf: Knopf<'_>, mit_betrieb: bool) -> String {
    let mut s = String::with_capacity(16_384);
    kopf(l, mit_betrieb, &mut s);
    punkte(l, knopf, &mut s);
    tage(l, &mut s);
    budget(l, &mut s);
    depot(l, &mut s);
    gespraech(l, &mut s);
    grenzen(l, &mut s);
    s
}

/// Die ganze Seite.
pub fn seite(l: &Lagebild) -> String {
    let mut s = String::with_capacity(16_384);
    let _ = write!(
        s,
        "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>DepotSchlopf</title><style>{STIL}</style></head><body><main>\
         <h1>DepotSchlopf</h1><div class=\"leise\">Nur Papiergeld.</div>"
    );
    s.push_str(&inhalt(l, &|_| None));
    s.push_str("</main></body></html>\n");
    s
}
