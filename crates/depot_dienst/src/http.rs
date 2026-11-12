//! Ein kleines HTTP/1.1 nur fuer die eigene Oberflaeche: jede Verbindung eine Anfrage, danach
//! geschlossen. Gelesen wird mit festen Grenzen; was nicht genau passt, wird abgelehnt.
//!
//! **Schutz gegen fremde Webseiten:** Der Dienst lauscht nur auf 127.0.0.1 (Kodex). Ein Browser
//! kann trotzdem von einer fremden Seite aus Anfragen an 127.0.0.1 schicken. Darum:
//! - Jede Anfrage muss `Host: 127.0.0.1:PORT` oder `Host: localhost:PORT` tragen. Eine fremde
//!   Seite, die ihren Namen auf 127.0.0.1 umlenkt (DNS-Rebinding), traegt ihren eigenen Namen.
//! - Jede aendernde Anfrage (POST) muss `Origin` der eigenen Seite tragen und das Formularfeld
//!   `t` mit dem Zufallswert dieses Dienststarts. Fremde Seiten kennen ihn nicht.
//! - Antworten verbieten Einbetten, fremde Skripte und Zwischenspeichern.

use std::io::Read;

/// Fester Port der Oberflaeche.
pub const PORT: u16 = 47_410;
/// Groesste Kopfzeilen samt Anfragezeile.
pub const KOPF_MAX: usize = 16 * 1024;
/// Groesster Rumpf (Formular).
pub const RUMPF_MAX: usize = 16 * 1024;

/// Eine gelesene Anfrage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Anfrage {
    pub methode: String,
    /// Pfad ohne Abfrage, etwa `/einrichtung`.
    pub pfad: String,
    /// Abfrage nach `?`, ohne `?`.
    pub abfrage: String,
    pub kopf: Vec<(String, String)>,
    pub rumpf: Vec<u8>,
}

impl Anfrage {
    /// Wert einer Kopfzeile, Name ohne Ruecksicht auf Gross- und Kleinschreibung.
    pub fn kopf(&self, name: &str) -> Option<&str> {
        self.kopf
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, w)| w.as_str())
    }
}

/// Warum eine Anfrage nicht gelesen wurde.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeseFehler {
    ZuGross,
    Kaputt,
    Abgebrochen,
}

fn finde(heu: &[u8], nadel: &[u8]) -> Option<usize> {
    heu.windows(nadel.len()).position(|w| w == nadel)
}

/// Liest genau eine Anfrage.
///
/// # Errors
/// Kopf oder Rumpf zu gross, Form falsch, Verbindung abgebrochen.
pub fn lies(r: &mut impl Read) -> Result<Anfrage, LeseFehler> {
    let mut puffer: Vec<u8> = Vec::with_capacity(2048);
    let mut stueck = [0_u8; 2048];
    let kopf_ende = loop {
        if let Some(i) = finde(&puffer, b"\r\n\r\n") {
            break i;
        }
        if puffer.len() > KOPF_MAX {
            return Err(LeseFehler::ZuGross);
        }
        let n = r.read(&mut stueck).map_err(|_| LeseFehler::Abgebrochen)?;
        if n == 0 {
            return Err(LeseFehler::Abgebrochen);
        }
        puffer.extend_from_slice(stueck.get(..n).ok_or(LeseFehler::Kaputt)?);
    };
    if kopf_ende > KOPF_MAX {
        return Err(LeseFehler::ZuGross);
    }
    let kopf_text = std::str::from_utf8(puffer.get(..kopf_ende).ok_or(LeseFehler::Kaputt)?)
        .map_err(|_| LeseFehler::Kaputt)?;
    let mut zeilen = kopf_text.split("\r\n");
    let erste = zeilen.next().ok_or(LeseFehler::Kaputt)?;
    let mut teile = erste.split(' ');
    let (Some(methode), Some(ziel), Some(version), None) =
        (teile.next(), teile.next(), teile.next(), teile.next())
    else {
        return Err(LeseFehler::Kaputt);
    };
    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        return Err(LeseFehler::Kaputt);
    }
    if !ziel.starts_with('/') {
        return Err(LeseFehler::Kaputt);
    }
    let (pfad, abfrage) = ziel.split_once('?').unwrap_or((ziel, ""));
    let mut kopf = Vec::new();
    for z in zeilen {
        let (n, w) = z.split_once(':').ok_or(LeseFehler::Kaputt)?;
        if n.is_empty() || n.bytes().any(|b| b.is_ascii_whitespace()) {
            return Err(LeseFehler::Kaputt);
        }
        kopf.push((n.to_string(), w.trim().to_string()));
    }
    let mut a = Anfrage {
        methode: methode.to_string(),
        pfad: pfad.to_string(),
        abfrage: abfrage.to_string(),
        kopf,
        rumpf: Vec::new(),
    };
    if a.kopf("transfer-encoding").is_some() {
        return Err(LeseFehler::Kaputt);
    }
    let laengen: Vec<&str> = a
        .kopf
        .iter()
        .filter(|(n, _)| n.eq_ignore_ascii_case("content-length"))
        .map(|(_, w)| w.as_str())
        .collect();
    let laenge = match laengen.as_slice() {
        [] => 0,
        [l] => l.parse::<usize>().map_err(|_| LeseFehler::Kaputt)?,
        _ => return Err(LeseFehler::Kaputt),
    };
    if laenge > RUMPF_MAX {
        return Err(LeseFehler::ZuGross);
    }
    let start = kopf_ende.checked_add(4).ok_or(LeseFehler::Kaputt)?;
    let mut rumpf = puffer.get(start..).unwrap_or_default().to_vec();
    while rumpf.len() < laenge {
        let n = r.read(&mut stueck).map_err(|_| LeseFehler::Abgebrochen)?;
        if n == 0 {
            return Err(LeseFehler::Abgebrochen);
        }
        rumpf.extend_from_slice(stueck.get(..n).ok_or(LeseFehler::Kaputt)?);
    }
    if rumpf.len() > laenge {
        return Err(LeseFehler::Kaputt);
    }
    a.rumpf = rumpf;
    Ok(a)
}

/// Nur die eigenen Namen mit dem eigenen Port.
pub fn host_erlaubt(host: Option<&str>) -> bool {
    let p = PORT.to_string();
    host.and_then(|h| h.rsplit_once(':'))
        .is_some_and(|(name, port)| port == p && (name == "127.0.0.1" || name == "localhost"))
}

/// Nur die eigene Seite.
pub fn herkunft_erlaubt(origin: Option<&str>) -> bool {
    let p = PORT.to_string();
    origin.is_some_and(|o| {
        o == format!("http://127.0.0.1:{p}") || o == format!("http://localhost:{p}")
    })
}

fn hex_wert(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => b.checked_sub(b'0'),
        b'a'..=b'f' => b.checked_sub(b'a').and_then(|x| x.checked_add(10)),
        b'A'..=b'F' => b.checked_sub(b'A').and_then(|x| x.checked_add(10)),
        _ => None,
    }
}

fn entschluessle(teil: &str) -> Option<String> {
    let mut aus = Vec::with_capacity(teil.len());
    let mut b = teil.bytes();
    while let Some(c) = b.next() {
        match c {
            b'+' => aus.push(b' '),
            b'%' => {
                let h = hex_wert(b.next()?)?;
                let l = hex_wert(b.next()?)?;
                aus.push(h.checked_mul(16)?.checked_add(l)?);
            }
            c => aus.push(c),
        }
    }
    String::from_utf8(aus).ok()
}

/// Liest ein Formular `a=1&b=2`. Ein Feld, das sich nicht entschluesseln laesst, macht das
/// ganze Formular ungueltig.
pub fn formular(text: &[u8]) -> Option<Vec<(String, String)>> {
    let text = std::str::from_utf8(text).ok()?;
    if text.is_empty() {
        return Some(Vec::new());
    }
    text.split('&')
        .map(|paar| {
            let (n, w) = paar.split_once('=').unwrap_or((paar, ""));
            Some((entschluessle(n)?, entschluessle(w)?))
        })
        .collect()
}

/// Wert eines Formularfelds.
pub fn feld<'a>(f: &'a [(String, String)], name: &str) -> Option<&'a str> {
    f.iter().find(|(n, _)| n == name).map(|(_, w)| w.as_str())
}

/// Vergleich ohne fruehen Ausstieg: Die Dauer verraet nicht, wie viele Zeichen stimmen.
pub fn gleich(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0_u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

const SICHERHEIT: &str = "Content-Security-Policy: default-src 'none'; script-src 'self'; \
style-src 'unsafe-inline'; connect-src 'self'; form-action 'self'; frame-ancestors 'none'; \
base-uri 'none'\r\nX-Frame-Options: DENY\r\nX-Content-Type-Options: nosniff\r\n\
Referrer-Policy: same-origin\r\nCache-Control: no-store\r\nConnection: close\r\n";

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        303 => "See Other",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        _ => "Error",
    }
}

/// Eine vollstaendige Antwort.
pub fn antwort(status: u16, typ: &str, rumpf: &[u8]) -> Vec<u8> {
    antwort_mit_kopf(status, typ, "", rumpf)
}

pub(crate) fn download(name: &str, rumpf: &[u8]) -> Vec<u8> {
    antwort_mit_kopf(
        200,
        "application/zip",
        &format!("Content-Disposition: attachment; filename=\"{name}\"\r\n"),
        rumpf,
    )
}

fn antwort_mit_kopf(status: u16, typ: &str, kopf: &str, rumpf: &[u8]) -> Vec<u8> {
    let mut a = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: {typ}\r\nContent-Length: {}\r\n{kopf}{SICHERHEIT}\r\n",
        status_text(status),
        rumpf.len()
    )
    .into_bytes();
    a.extend_from_slice(rumpf);
    a
}

/// Umleitung nach einem Formular (Post/Redirect/Get). `ort` ist ein fester, eigener Pfad.
pub fn umleitung(ort: &str) -> Vec<u8> {
    format!("HTTP/1.1 303 See Other\r\nLocation: {ort}\r\nContent-Length: 0\r\n{SICHERHEIT}\r\n")
        .into_bytes()
}

/// Eine kurze Textantwort fuer Fehler.
pub fn text(status: u16, t: &str) -> Vec<u8> {
    antwort(status, "text/plain; charset=utf-8", t.as_bytes())
}
