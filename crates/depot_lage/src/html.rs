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

/// Eine offene Voraussetzung kann eine Handlung, Warten oder fehlendes Wissen bedeuten.
fn punkt_status(p: &crate::Punkt) -> (&'static str, &'static str) {
    match (p.ampel, p.abhilfe) {
        (Ampel::Gut, _) => ("gut", "bereit"),
        (Ampel::Halt, _) => ("halt", "Halt"),
        (Ampel::Offen, None) => ("leise", "unbekannt"),
        (Ampel::Offen, Some(Abhilfe::Warten)) => ("leise", "wartet"),
        (Ampel::Offen, Some(_)) => ("offen", "bitte erledigen"),
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
.budget-uebersicht{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,245px),1fr));gap:12px}\
.budget-karte{min-width:0;border:1px solid var(--linie);border-radius:8px;padding:12px;overflow-wrap:anywhere}\
.budget-karte h3{font-size:15px;margin:0 0 2px}.budget-karte p{margin:0}\
.budget-karte dl{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1.25fr);gap:8px;margin:12px 0 0}\
.budget-karte dd{margin:0;text-align:right;font-variant-numeric:tabular-nums}\
.kopf{font-size:17px;font-weight:600}.breit{overflow-x:auto}\
ol.gespraech{list-style:none;padding:0;margin:0}ol.gespraech li{padding:6px 0;\
border-bottom:1px solid var(--linie)}ol.gespraech b{display:inline-block;min-width:90px}";

fn kopf(l: &Lagebild, s: &mut String) {
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
    s.push_str("<section><h2>Bereitschaft</h2><div class=\"breit\"><table>");
    for p in &l.punkte {
        let k = p.abhilfe.and_then(knopf).unwrap_or_default();
        let (klasse, text) = punkt_status(p);
        let _ = write!(
            s,
            "<tr><td><span class=\"marke {}\">{}</span></td><td>{}</td><td>{}</td><td>{k}</td></tr>",
            klasse,
            text,
            maskiere(p.name),
            maskiere(&p.text)
        );
    }
    s.push_str("</table></div></section>");
}

fn vorbereitungsthese(
    a: &depot_journal::VorbereitungsAntwort,
    beleg: Option<&str>,
    s: &mut String,
) {
    if let Some(w) = &a.prognose {
        let richtung = match w.richtung {
            depot_journal::Prognoserichtung::Steigt => "steigt",
            depot_journal::Prognoserichtung::Faellt => "fällt",
        };
        let _ = write!(
            s,
            "<p><b>Beobachtung:</b> {} {richtung}, Horizont {} Handelstage.<br>\
             Wirkung: {}<br>Gegenargument / Widerlegung: {}</p>",
            maskiere(w.isin.als_str()),
            w.handelstage,
            maskiere(&w.begruendung),
            maskiere(&w.widerlegung)
        );
        if let Some(e) = beleg {
            let _ = write!(
                s,
                "<details><summary>Ursprünglicher Beleg</summary><p>{}</p></details>",
                maskiere(e)
            );
        }
    }
}

/// Gemeinsamer Hinweis zur Häufigkeit der Vorbereitung.
pub const VORBEREITUNG_HAEUFIGKEIT: &str =
    "Je KI ist an einem börsenfreien Tag höchstens ein kurzer Aufruf möglich.";

fn vorbereitung(l: &Lagebild, knopf: Knopf<'_>, s: &mut String) {
    s.push_str("<section id=\"vorbereitung\"><h2>Vorbereitung auf den nächsten Handelstag</h2>");
    if let Some(p) = &l.vorbereitung {
        let _ = write!(
            s,
            "<p>Für <b>{}</b> · Wissensstand {} · {} belegte Ereignisse.</p>",
            datum_text(p.fuer),
            zeit_text(p.zeit),
            p.ereignisse.len()
        );
        let heute = l.jetzt.frankfurt_tag();
        if heute
            .is_some_and(|d| Some(d) != p.zeit.frankfurt_tag() && !depot_kern::liste::handelstag(d))
        {
            s.push_str("<p class=\"offen\">Heute ist noch kein neuer Wissensstand belegt. Angezeigt wird der letzte belegte Stand.</p>");
        }
        if heute.is_some_and(|d| d > p.fuer) {
            s.push_str("<p class=\"leise\">Das Ziel-Datum ist vergangen. Bei neuen Entscheidungen werden die Thesen erneut geprüft.</p>");
        }
        for b in [BeraterId::Dachs, BeraterId::Maus] {
            let _ = write!(s, "<h3>{}</h3>", berater_text(b));
            if let Some(a) = p.antworten.iter().find(|a| a.berater == b) {
                let (klasse, stand) = if a.verwertbar {
                    ("gut", "Einschätzung vorhanden")
                } else if a.gefragt {
                    ("offen", "Keine verwertbare Antwort")
                } else {
                    ("offen", "Nicht gefragt")
                };
                let _ = write!(
                    s,
                    "<p class=\"{klasse}\">{stand} · {}</p><p>{}</p>",
                    zeit_text(a.zeit),
                    maskiere(&a.text)
                );
                let beleg = a
                    .prognose
                    .as_ref()
                    .and_then(|w| w.ereignis_nr.checked_sub(1))
                    .and_then(|n| p.ereignisse.get(n));
                vorbereitungsthese(a, beleg.map(String::as_str), s);
            } else {
                let text = if p.reserviert.contains(&b) {
                    "Keine Antwort belegt. Falls der Lauf unterbrochen wurde, bleibt die Reservierung gebucht; heute wird nicht erneut gefragt."
                } else {
                    "Noch keine Antwort belegt. Ein begonnener Lauf wird nach einer Unterbrechung heute nicht wiederholt."
                };
                let _ = write!(s, "<p class=\"offen\">{text}</p>");
            }
            if let Some(frueher) = l
                .fruehere_vorbereitung
                .iter()
                .find(|p| p.antwort.berater == b)
            {
                let _ = write!(
                    s,
                    "<details><summary>Letzte belegte Einschätzung vom {} · damals für {}</summary><p>{}</p>",
                    zeit_text(frueher.antwort.zeit),
                    datum_text(frueher.fuer),
                    maskiere(&frueher.antwort.text)
                );
                vorbereitungsthese(&frueher.antwort, frueher.beleg.as_deref(), s);
                s.push_str("</details>");
            }
        }
        s.push_str("<details><summary>Belegte Quellen der Vorbereitung</summary><ol>");
        for e in &p.ereignisse {
            let _ = write!(s, "<li>{}</li>", maskiere(e));
        }
        s.push_str("</ol></details>");
    } else {
        s.push_str("<p>Noch keine separate Vorbereitung belegt.</p>");
    }
    let _ = write!(
        s,
        "<p class=\"leise\">Belegte Quellen bleiben auch ohne KI-Einschätzung sichtbar. {VORBEREITUNG_HAEUFIGKEIT} An Handelstagen werden die Hypothesen in den regulären Aufrufen neu geprüft.</p>"
    );
    if let Some(k) = knopf(Abhilfe::Vorbereiten) {
        s.push_str(&k);
    }
    s.push_str("</section>");
}

fn budget(l: &Lagebild, s: &mut String) {
    if l.budget.is_empty() && l.gemini_budget.is_none() {
        return;
    }
    s.push_str("<section><h2>KI-Budget</h2><div class=\"budget-uebersicht\">");
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
            "<article class=\"budget-karte\"><h3>{}</h3><p class=\"{klasse}\">{}</p><dl>\
             <dt>Verbraucht im Monat</dt><dd>{}<br><span class=\"leise\">{} Aufrufe abgerechnet{unbekannt}</span></dd>\
             <dt>Gebucht im Monat</dt><dd>{} von {}<br><span class=\"leise\">{}/{} Aufrufe</span></dd>\
             <dt>Verfügbar im Monat</dt><dd><b>{}</b><br><span class=\"leise\">{} Handelstage übrig</span></dd>\
             <dt>Heute / Tagesrahmen frei</dt><dd>{} · {}/{} Aufrufe<br>noch {}</dd>\
             <dt>Jahr</dt><dd>{} von {}</dd><dt>Je Aufruf</dt><dd>{}</dd></dl></article>",
            berater_text(b.berater),
            maskiere(&b.status.text()),
            euro(b.monat_verbraucht),
            b.monat_abgerechnet,
            euro(b.monat_gebucht),
            euro(b.monat_grenze),
            b.monat_aufrufe,
            b.monat_aufrufe_max,
            euro(b.monat_rest),
            b.handelstage_rest,
            euro(b.heute_gebucht),
            b.heute_aufrufe,
            b.heute_aufrufe_max,
            euro(b.tagesrahmen),
            euro(b.jahr_gebucht),
            euro(b.jahr_grenze),
            geld(b.je_aufruf),
        );
    }
    if let Some(gemini) = l.gemini_budget {
        let (klasse, status) = if gemini.an {
            ("gut", "Eingeschaltet")
        } else {
            ("leise", "Ausgeschaltet")
        };
        let aufrufe = gemini.aufrufe.map_or_else(
            || "unbekannt".into(),
            |n| format!("{n} von {}", depot_kern::gemini::TAGESGRENZE),
        );
        let _ = write!(
            s,
            "<article class=\"budget-karte\"><h3>Gemini</h3><p class=\"{klasse}\">{status}</p>\
             <dl><dt>Tarif</dt><dd>Gratis-Projekt</dd><dt>Aufrufe heute</dt><dd>{aufrufe}</dd></dl></article>"
        );
    }
    s.push_str(
        "</div><p class=\"leise\"><b>Verbraucht</b> ist, was die Anbieter je Aufruf \
         gemeldet haben (daten/anbieter.txt). <b>Gebucht</b> sind bestätigte tatsächliche Kosten \
         plus ungeklärte Höchstkosten im Journal. Fehlende Abrechnungen bleiben voll gebucht. \
         <b>Verfügbar</b> ist der Monatsrest. Der Tagesrahmen verteilt ihn auf die verbleibenden \
         Handelstage. An börsenfreien Tagen zählt heute für die Vorbereitung mit. Nach dem letzten Handelstag \
         gilt höchstens der Betrag für einen Kurzaufruf je KI. Zusatzfragen lassen die Rücklage für Erstmeinungen frei.</p></section>",
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

pub fn ausfuehrungsbilanz(depot: &crate::DepotLage, text: &mut String) {
    use crate::ausfuehrung::Kennzahlen;
    let quote = |k: Kennzahlen| {
        let gesamt = k.gefuellt.saturating_add(k.verfallen);
        let bp = i64::try_from(k.gefuellt).ok().and_then(|anzahl| {
            anzahl
                .checked_mul(10_000)?
                .checked_div(i64::try_from(gesamt).ok()?)
        });
        bp.map_or_else(
            || "–".into(),
            |bp| format!("{} ({}/{gesamt})", prozent(bp), k.gefuellt),
        )
    };
    let abstand = |bp: Option<i64>| bp.map_or_else(|| "–".into(), |wert| format!("{wert:+} bp"));
    if depot.ausfuehrungen.iter().all(|(_, kauf, verkauf)| {
        kauf.gefuellt == 0 && kauf.verfallen == 0 && verkauf.gefuellt == 0 && verkauf.verfallen == 0
    }) {
        return;
    }
    text.push_str("<h3>Ausführungen der letzten 20 Handelstage</h3><ul>");
    for (topf, kauf, verkauf) in &depot.ausfuehrungen {
        let _ = write!(
            text,
            "<li>{} · Kauf gefüllt {} (Ø Füllabstand {}) · Verkauf gefüllt {} (Ø Füllabstand {}) · Verfallene Käufe {} (Ø Kursbewegung {})</li>",
            topf_text(*topf),
            quote(*kauf),
            abstand(kauf.mittlerer_fuellpreis_bp),
            quote(*verkauf),
            abstand(verkauf.mittlerer_fuellpreis_bp),
            kauf.verfallen,
            abstand(kauf.mittlere_bewegung_bp)
        );
    }
    text.push_str("</ul>");
}

fn depot(l: &Lagebild, s: &mut String) {
    let Some(d) = &l.depot else {
        return;
    };
    s.push_str(
        "<section><h2>Papierdepot</h2><p class=\"leise\">Nur Papiergeld, simuliert: Neue \
         Orders warten auf einen geprüften Schlusskurs nach ihrem Versand. Ob eine echte \
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
    if let Some(v) = &d.vergleich {
        vergleich(v, s);
    }
    ausfuehrungsbilanz(d, s);
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
    s.push_str("<p><a href=\"/depot\">Mehr zum Depot</a></p></section>");
}

/// Basispunkte als Prozent mit Vorzeichen: `+2,46 %`, `−0,01 %`, `±0,00 %`.
pub fn prozent(bp: i64) -> String {
    let betrag = bp.unsigned_abs();
    let zeichen = match bp.signum() {
        1 => "+",
        -1 => "−",
        _ => "±",
    };
    format!(
        "{zeichen}{},{:02} %",
        betrag.checked_div(100).unwrap_or(0),
        betrag.checked_rem(100).unwrap_or(0)
    )
}

fn vergleich(v: &crate::vergleich::VergleichLage, s: &mut String) {
    let _ = write!(
        s,
        "<h2 style=\"margin-top:12px\">Vergleich seit Start</h2><div class=\"breit\"><table><tr>\
         <th>Wer</th><th class=\"z\">Wert am {}</th><th class=\"z\">seit Start</th>\
         <th class=\"z\">KI-Kosten</th><th class=\"z\">nach KI-Kosten</th></tr>",
        datum_text(v.bewertet)
    );
    for z in &v.zeilen {
        let wert = match (&z.wert, &z.grund) {
            (Some(w), _) => euro(*w),
            (None, Some(g)) => format!("nicht bewertbar: {}", maskiere(g)),
            (None, None) => "nicht bewertbar".into(),
        };
        let _ = write!(
            s,
            "<tr><td>{}</td><td class=\"z\">{}</td><td class=\"z\">{}</td><td class=\"z\">{}</td>\
             <td class=\"z\">{}</td></tr>",
            maskiere(&z.name),
            wert,
            z.rendite_bp.map_or_else(|| "–".into(), prozent),
            z.ki_kosten.map_or_else(|| "–".into(), euro),
            z.nach_ki_bp.map_or_else(|| "–".into(), prozent)
        );
    }
    s.push_str("</table></div>");
    let _ = write!(
        s,
        "<p class=\"leise\">Ein Topf liegt vorn, wenn er nach KI-Kosten über dem Vergleich liegt. \
         Start {} mit je {}, {} abgeschlossene Papiertage. Das Vergleichsdepot kauft zu den \
         Schlusskursen des ersten Papiertags mit derselben Gebühr je Order und hält danach nur.",
        datum_text(v.start),
        euro(v.startgeld),
        v.tage
    );
    if !v.ohne.is_empty() {
        let _ = write!(
            s,
            " Ohne {} (kein Kurs am Einstiegstag).",
            maskiere(&v.ohne.join(", "))
        );
    }
    s.push_str("</p>");
}

fn prognosen(l: &Lagebild, s: &mut String) {
    if l.prognosen.is_empty() {
        return;
    }
    s.push_str(
        "<section id=\"prognosen\"><h2>Prognosen</h2><p class=\"leise\">Hinweis, keine \
         Regel: aufgelöste Weltprognosen nach ihrem Horizont, mit 95-%-Intervall und dem \
         Vergleich „steigt immer“ auf denselben Fällen.</p><table>",
    );
    for (wer, text) in &l.prognosen {
        let _ = write!(
            s,
            "<tr><td>{}</td><td>{}</td></tr>",
            maskiere(wer),
            maskiere(text)
        );
    }
    s.push_str("</table></section>");
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

/// Gemeinsame Anzeige der geltenden Grenzen in Live und Einrichtung.
pub fn grenzen(s: &mut String, werte: &[crate::Grenzwert]) {
    s.push_str("<section id=\"grenzen\"><h2>Grenzen und Budgets</h2><table>");
    for g in werte {
        let _ = write!(
            s,
            "<tr><td>{}</td><td class=\"z\">{}</td></tr>",
            maskiere(g.name),
            maskiere(&g.wert)
        );
    }
    s.push_str("</table><p class=\"leise\">Diese Schutzgrenzen gelten für jeden Lauf. Änderungen brauchen deine ausdrückliche Freigabe. Verbrauch und Rest je KI stehen auf Live.</p></section>");
}

/// Die Abschnitte des Lagebilds ohne Seitenrahmen. `knopf` setzt zu jeder Abhilfe ein
/// Bedienelement; ohne Bedienung `&|_| None`.
pub fn inhalt(l: &Lagebild, knopf: Knopf<'_>) -> String {
    inhalt_mit(l, knopf, true)
}

/// Wie `inhalt`; ohne Gesamtstatus, wenn die Oberflaeche ihn selbst zeigt (mit dem Zeitplan).
pub fn inhalt_mit(l: &Lagebild, knopf: Knopf<'_>, mit_betrieb: bool) -> String {
    let mut s = String::with_capacity(16_384);
    if mit_betrieb {
        kopf(l, &mut s);
    }
    vorbereitung(l, knopf, &mut s);
    punkte(l, knopf, &mut s);
    tage(l, &mut s);
    budget(l, &mut s);
    depot(l, &mut s);
    prognosen(l, &mut s);
    gespraech(l, &mut s);
    grenzen(&mut s, &l.grenzen);
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
