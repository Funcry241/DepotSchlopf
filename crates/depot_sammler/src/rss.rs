//! Begrenzter RSS-2.0-Leser: keine DTD, keine externen Entitaeten, keine Artikelabrufe.
use std::collections::BTreeMap;

use depot_kern::{Datum, Zeitpunkt};
use quick_xml::{Reader, events::Event};

use crate::nachrichten::{MAX_ALTER_STUNDEN, MAX_ZEICHEN, Nachricht};

/// Zeitangabe nach RFC 822 mit ausdruecklicher Zeitzone; kein Raten nach Rechnerzeit.
pub fn zeit(text: &str) -> Option<Zeitpunkt> {
    let (_, rest) = text.trim().split_once(',')?;
    let teile: Vec<_> = rest.split_whitespace().collect();
    let [tag, monat, jahr, uhr, zone] = teile.as_slice() else {
        return None;
    };
    let monate = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let m = monate.iter().position(|m| m == monat)?.checked_add(1)?;
    let datum = Datum::neu(jahr.parse().ok()?, u8::try_from(m).ok()?, tag.parse().ok()?)?;
    let teile: Vec<_> = uhr.split(':').collect();
    let [h, m, s] = teile.as_slice() else {
        return None;
    };
    let lokal = Zeitpunkt::neu(datum, h.parse().ok()?, m.parse().ok()?, s.parse().ok()?)?;
    let versatz = if matches!(*zone, "GMT" | "UTC" | "UT" | "Z") {
        0
    } else {
        if zone.len() != 5 || !zone.get(1..)?.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let vorzeichen = match zone.get(..1)? {
            "+" => 1_i64,
            "-" => -1,
            _ => return None,
        };
        let h: i64 = zone.get(1..3)?.parse().ok()?;
        let m: i64 = zone.get(3..5)?.parse().ok()?;
        if !(0..=23).contains(&h) || !(0..=59).contains(&m) {
            return None;
        }
        h.checked_mul(3600)?
            .checked_add(m.checked_mul(60)?)?
            .checked_mul(vorzeichen)?
    };
    Zeitpunkt::aus_sekunden(lokal.sekunden().checked_sub(versatz)?)
}

fn nachricht(f: &BTreeMap<String, String>, quelle: &str, abruf: Zeitpunkt) -> Option<Nachricht> {
    let zeit = zeit(f.get("pubDate")?)?;
    if !abruf
        .sekunden_seit(zeit)
        .is_some_and(|s| (0..=MAX_ALTER_STUNDEN.saturating_mul(3600)).contains(&s))
    {
        return None;
    }
    let titel = f
        .get("title")?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let link = f.get("link")?.trim().split('?').next()?;
    let rest = link.strip_prefix("https://")?;
    let (host, pfad) = rest.split_once('/')?;
    let erlaubt = match quelle {
        "tagesschau" => host == "www.tagesschau.de",
        "bbc-world" => matches!(host, "www.bbc.co.uk" | "www.bbc.com"),
        _ => false,
    };
    if !erlaubt
        || pfad.is_empty()
        || link.len() > 240
        || link.chars().any(char::is_whitespace)
        || link.chars().any(char::is_control)
        || titel.is_empty()
    {
        return None;
    }
    let ende = format!(" | {link}");
    let platz = MAX_ZEICHEN.saturating_sub(
        quelle
            .chars()
            .count()
            .saturating_add(2)
            .saturating_add(ende.chars().count()),
    );
    let titel: String = titel.chars().take(platz).collect();
    Some(Nachricht {
        zeit,
        text: format!("{quelle}: {titel}{ende}"),
    })
}

#[derive(Default)]
struct Leser {
    offen: Vec<String>,
    wurzel: bool,
    kanal: bool,
    felder: BTreeMap<String, String>,
    nachrichten: Vec<Nachricht>,
    ausgelassen: usize,
    eintrag_fehler: Option<&'static str>,
    dekodiert: String,
    deklaration: bool,
}

impl Leser {
    fn element(&mut self, e: &quick_xml::events::BytesStart<'_>) -> Result<(), String> {
        for a in e.attributes() {
            let a = a.map_err(|_| "Ungueltige RSS-Attribute")?;
            let wert = a
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(|_| "Ungueltiger Attributwert")?;
            self.dekodiert.push_str(&wert);
        }
        let name = e.name().as_ref().to_string();
        if name == "rss"
            && e.try_get_attribute("version")
                .map_err(|_| "RSS-Version ungueltig")?
                .is_none_or(|a| a.value.as_ref() != "2.0")
        {
            return Err("RSS 2.0 erwartet".into());
        }
        self.start(name)
    }

    fn start(&mut self, name: String) -> Result<(), String> {
        if self.feld().is_some() {
            return Err("Verschachteltes Nachrichtenfeld".into());
        }
        if self.offen.len() >= 24 {
            return Err("RSS zu tief verschachtelt".into());
        }
        if self.offen.is_empty() {
            if self.wurzel || name != "rss" {
                return Err("Genau eine RSS-Wurzel erwartet".into());
            }
            self.wurzel = true;
        } else if self.offen.as_slice() == ["rss"] && name == "channel" {
            if self.kanal {
                return Err("Doppelter RSS-Kanal".into());
            }
            self.kanal = true;
        } else if name == "item" && self.offen.as_slice() != ["rss", "channel"] {
            return Err("RSS-Eintrag ausserhalb des Kanals".into());
        }
        self.offen.push(name);
        if self.offen.as_slice() == ["rss", "channel", "item"] {
            self.felder.clear();
            self.eintrag_fehler = None;
        }
        if let Some(name) = self.feld()
            && self
                .felder
                .insert(name.to_string(), String::new())
                .is_some()
        {
            self.eintrag_fehler = Some("Doppeltes Nachrichtenfeld");
        }
        Ok(())
    }

    fn feld(&self) -> Option<&str> {
        if self.offen.len() == 4 && self.offen.get(..3)? == ["rss", "channel", "item"] {
            self.offen
                .last()
                .map(String::as_str)
                .filter(|n| matches!(*n, "title" | "link" | "pubDate"))
        } else {
            None
        }
    }

    fn text(&mut self, text: &str) -> Result<(), String> {
        if text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        {
            return Err("Ungueltiges XML-Zeichen".into());
        }
        self.dekodiert.push_str(text);
        if let Some(name) = self.feld().map(str::to_string) {
            let ziel = self.felder.get_mut(&name).ok_or("Nachrichtenfeld fehlt")?;
            if ziel.len().saturating_add(text.len()) > 8192 {
                return Err("Nachrichtenfeld zu lang".into());
            }
            ziel.push_str(text);
        } else if self.offen.is_empty() && !text.trim().is_empty() {
            return Err("Text ausserhalb der RSS-Wurzel".into());
        }
        Ok(())
    }

    fn ende(&mut self, quelle: &str, abruf: Zeitpunkt) {
        if self.offen.as_slice() == ["rss", "channel", "item"] {
            if self.eintrag_fehler.is_none()
                && let Some(n) = nachricht(&self.felder, quelle, abruf)
            {
                self.nachrichten.push(n);
            } else {
                self.ausgelassen = self.ausgelassen.saturating_add(1);
            }
        }
        self.offen.pop();
    }
}

/// Liest den ganzen Feed, bevor ein Eintrag verwendet wird. Fehler verwerfen den Feed.
pub fn lies(
    text: &str,
    quelle: &str,
    abruf: Zeitpunkt,
) -> Result<(Vec<Nachricht>, usize, String), String> {
    if text.len() > usize::try_from(crate::netz::MAX_ANTWORT).unwrap_or(0)
        || text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err("RSS zu gross oder mit ungueltigen Steuerzeichen".into());
    }
    let mut reader = Reader::from_str(text);
    reader.config_mut().check_comments = true;
    let mut l = Leser::default();
    loop {
        match reader
            .read_event()
            .map_err(|_| "RSS unvollstaendig oder fehlerhaft")?
        {
            Event::Start(e) => l.element(&e)?,
            Event::Empty(e) => {
                l.element(&e)?;
                l.ende(quelle, abruf);
            }
            Event::End(_) => l.ende(quelle, abruf),
            Event::Text(t) => l.text(&t.xml10_content())?,
            Event::CData(t) => l.text(&t.xml10_content())?,
            Event::GeneralRef(r) => {
                let s = format!("&{};", r.xml10_content());
                let t = quick_xml::escape::unescape(&s).map_err(|_| "Unbekannte XML-Entitaet")?;
                l.text(&t)?;
            }
            Event::DocType(_) => return Err("DTD und externe Entitaeten sind nicht erlaubt".into()),
            Event::Decl(d) => {
                if l.wurzel || l.deklaration {
                    return Err("XML-Deklaration an falscher Stelle".into());
                }
                l.deklaration = true;
                l.dekodiert.push_str(d.as_ref());
                if d.encoding()
                    .transpose()
                    .map_err(|_| "Ungueltige XML-Kodierung")?
                    .is_some_and(|e| !e.eq_ignore_ascii_case("UTF-8"))
                {
                    return Err("RSS muss UTF-8 sein".into());
                }
            }
            Event::PI(p) => l.dekodiert.push_str(p.as_ref()),
            Event::Comment(t) => l.dekodiert.push_str(t.as_ref()),
            Event::Eof => break,
        }
    }
    if !l.wurzel || !l.kanal || !l.offen.is_empty() {
        return Err("RSS unvollstaendig".into());
    }
    Ok((l.nachrichten, l.ausgelassen, l.dekodiert))
}
