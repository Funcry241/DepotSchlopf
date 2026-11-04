//! Die Termine der EZB von ihrer eigenen Seite: Sitzungen des EZB-Rats und des Erweiterten
//! Rats, je Tag ein Eintrag. Die EZB veroeffentlicht sie selbst; sie gelten deshalb als
//! bestaetigt (R39).
//!
//! Gelesen wird genau die Form der Seite: im Element `main` genau eine Liste `dl` aus Paaren
//! `<dt>TT/MM/JJJJ</dt><dd>Text<br></dd>`. Jede Abweichung ist ein Fehler mit der Nummer des
//! Eintrags; nichts wird geraten, uebersprungen oder umgeschrieben. Eine Uhrzeit nennt die Seite
//! nicht, also hat kein Termin eine.
//!
//! - **Kennung:** `ezb-JJJJ-MM-TT-n`, `n` zaehlt die Eintraege desselben Tags ab 1 in der
//!   Reihenfolge der Seite.
//! - **Titel:** der Text der Seite, Leerraum zu einem Leerzeichen zusammengefasst.
//! - **Betroffen:** `eur-leitzins` fuer eine geldpolitische Sitzung (der Text nennt
//!   „monetary policy meeting“, nicht „non-monetary“), sonst `ezb`.

use depot_kern::{Betroffen, Datum, Termin, TerminFehler, Verlaesslichkeit, Zeitbezug};

/// Name der Quelle im Journal.
pub const QUELLE_EZB: &str = "ezb";
/// Die Seite mit den Terminen.
pub const ADRESSE_EZB: &str = "https://www.ecb.europa.eu/press/calendars/mgcgc/html/index.en.html";

/// Warum die Seite keine Termine ergibt. Eintraege zaehlen ab 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EzbFehler {
    /// Kein `main`, keine oder mehr als eine Liste, oder die Liste ist leer.
    Seite,
    /// Der Eintrag hat nicht die Form `<dt>…</dt><dd>…</dd>`.
    Form(usize),
    /// Kein Datum `TT/MM/JJJJ` im Kalender.
    Datum(usize),
    /// Der Text enthaelt etwas anderes als Text und ein abschliessendes `<br>`.
    Text(usize),
    /// Der Eintrag ergibt keinen gueltigen Termin.
    Termin(usize, TerminFehler),
    /// An diesem Tag laesst sich nicht eindeutig sagen, welcher Eintrag welcher bekannte
    /// Termin ist.
    Mehrdeutig(Datum),
}

/// Ein Eintrag der Seite, noch ohne Kennung. `nr` zaehlt ab 1 in der Reihenfolge der Seite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EzbEintrag {
    pub nr: usize,
    pub datum: Datum,
    pub titel: String,
    pub betroffen: Betroffen,
}

/// Liest alle Eintraege der Seite in ihrer Reihenfolge.
///
/// # Errors
/// Jede Abweichung von der Form; dann gibt es keinen Eintrag.
pub fn lies_ezb_seite(html: &str) -> Result<Vec<EzbEintrag>, EzbFehler> {
    let liste = liste(html).ok_or(EzbFehler::Seite)?;
    let mut rest = liste.trim_start();
    let mut eintraege: Vec<EzbEintrag> = Vec::new();
    let mut nr: usize = 0;
    while !rest.is_empty() {
        nr = nr.saturating_add(1);
        let (kopf, nach_kopf) = element(rest, "dt").ok_or(EzbFehler::Form(nr))?;
        let (inhalt, nach_inhalt) =
            element(nach_kopf.trim_start(), "dd").ok_or(EzbFehler::Form(nr))?;
        rest = nach_inhalt.trim_start();
        let datum = lies_datum(kopf.trim()).ok_or(EzbFehler::Datum(nr))?;
        let titel = lies_text(inhalt).ok_or(EzbFehler::Text(nr))?;
        let betroffen =
            if titel.contains("monetary policy meeting") && !titel.contains("non-monetary") {
                "eur-leitzins"
            } else {
                "ezb"
            };
        let betroffen = Betroffen::groesse(betroffen).ok_or(EzbFehler::Form(nr))?;
        // Jeder Eintrag muss einen gueltigen Termin ergeben, gleich unter welcher Kennung.
        Termin::neu(
            QUELLE_EZB,
            &titel,
            Zeitbezug::Tag(datum),
            vec![betroffen.clone()],
            Verlaesslichkeit::Bestaetigt,
        )
        .map_err(|e| EzbFehler::Termin(nr, e))?;
        eintraege.push(EzbEintrag {
            nr,
            datum,
            titel,
            betroffen,
        });
    }
    if eintraege.is_empty() {
        return Err(EzbFehler::Seite);
    }
    Ok(eintraege)
}

/// Ordnet die Eintraege der Seite den bekannten Terminen der EZB zu (noch: nach Reihenfolge).
///
/// # Errors
/// Ein Eintrag ergibt keinen gueltigen Termin.
pub fn ordne_zu(
    eintraege: Vec<EzbEintrag>,
    _bekannt: &[&Termin],
) -> Result<Vec<Termin>, EzbFehler> {
    let mut termine: Vec<Termin> = Vec::new();
    for e in eintraege {
        let gleicher_tag = termine
            .iter()
            .filter(|t| t.zeit() == Zeitbezug::Tag(e.datum))
            .count();
        let kennung = format!("ezb-{}-{}", e.datum, gleicher_tag.saturating_add(1));
        termine.push(
            Termin::neu(
                &kennung,
                &e.titel,
                Zeitbezug::Tag(e.datum),
                vec![e.betroffen],
                Verlaesslichkeit::Bestaetigt,
            )
            .map_err(|f| EzbFehler::Termin(e.nr, f))?,
        );
    }
    Ok(termine)
}

/// Liest alle Termine der Seite, ohne bekannte Termine: Kennungen ab `-1` je Tag.
///
/// # Errors
/// Wie `lies_ezb_seite`.
pub fn lies_ezb_termine(html: &str) -> Result<Vec<Termin>, EzbFehler> {
    ordne_zu(lies_ezb_seite(html)?, &[])
}

/// Der Inhalt der einzigen Liste `<dl>…</dl>` im einzigen Element `main`.
fn liste(html: &str) -> Option<&str> {
    let (_, nach) = html.split_once("<main")?;
    let (main, _) = nach.split_once("</main>")?;
    if nach.matches("<main").count() > 0 || main.matches("<dl>").count() != 1 {
        return None;
    }
    let (_, ab) = main.split_once("<dl>")?;
    let (inhalt, _) = ab.split_once("</dl>")?;
    Some(inhalt)
}

/// `<name>inhalt</name>` am Anfang von `text`: Inhalt und Rest danach.
fn element<'a>(text: &'a str, name: &str) -> Option<(&'a str, &'a str)> {
    let rest = text.strip_prefix(&format!("<{name}>"))?;
    let ende = format!("</{name}>");
    let (inhalt, danach) = rest.split_once(&ende)?;
    (!inhalt.contains('<') || name == "dd").then_some((inhalt, danach))
}

/// `TT/MM/JJJJ`.
fn lies_datum(text: &str) -> Option<Datum> {
    let mut teile = text.split('/');
    let mut zahl = |n: usize| {
        let t = teile.next()?;
        (t.len() == n && t.bytes().all(|b| b.is_ascii_digit()))
            .then(|| t.parse::<u16>().ok())
            .flatten()
    };
    let tag = u8::try_from(zahl(2)?).ok()?;
    let monat = u8::try_from(zahl(2)?).ok()?;
    let jahr = zahl(4)?;
    if teile.next().is_some() {
        return None;
    }
    Datum::neu(jahr, monat, tag)
}

/// Text mit genau einem abschliessenden `<br>`, sonst ohne Auszeichnung und ohne Entitaet.
fn lies_text(dd: &str) -> Option<String> {
    let text = dd.trim_end().strip_suffix("<br>")?;
    if text.contains('<') || text.contains('>') || text.contains('&') {
        return None;
    }
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}
