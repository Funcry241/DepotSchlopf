//! Der Verlauf ueber Neustarts: `daten/dienst_verlauf.txt`.
//!
//! Jeder Auftrag schreibt beim Start und beim Ende eine Zeile. Nach einem Neustart (Fenster zu,
//! Rechner neu gestartet, Absturz) zeigt die Oberflaeche so, was zuletzt lief und wie es endete.
//! Ein Start ohne Ende heisst: Der Dienst endete waehrend des Laufs. Was der Lauf geschafft hat,
//! steht im Journal, und das Lagebild zeigt den Stand (etwa einen angefangenen Papiertag als
//! Halt).
//!
//! Die Datei entscheidet nichts; sie dient nur der Anzeige. Zeilen:
//! `verlauf-v1;start;<kennung>;<art>;<anlass>;<start>` und
//! `verlauf-v1;ende;<kennung>;<art>;<anlass>;<start>;<ende|->;<ok|fehler>`.

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use depot_kern::Zeitpunkt;

use crate::werkbank::{Anlass, Art, Auftrag, Frueher, VERLAUF, lies_falls_da, schreibe_ganz};

const KOPF: &str = "verlauf-v1";

pub fn pfad(wurzel: &Path) -> PathBuf {
    wurzel.join("daten").join("dienst_verlauf.txt")
}

fn anlass_kurz(a: Anlass) -> &'static str {
    match a {
        Anlass::Hand => "hand",
        Anlass::Zeitplan => "zeitplan",
    }
}

fn anlass_aus(k: &str) -> Option<Anlass> {
    match k {
        "hand" => Some(Anlass::Hand),
        "zeitplan" => Some(Anlass::Zeitplan),
        _ => None,
    }
}

fn kennung_gut(k: &str) -> bool {
    !k.is_empty() && k.len() <= 40 && k.bytes().all(|b| b.is_ascii_digit() || b == b'-')
}

/// Die Zeile beim Start.
pub fn start_zeile(a: &Auftrag) -> String {
    format!(
        "{KOPF};start;{};{};{};{}",
        a.kennung,
        a.art.kurz(),
        anlass_kurz(a.anlass),
        a.start.sekunden()
    )
}

/// Die Zeile beim Ende.
pub fn ende_zeile(a: &Auftrag) -> String {
    format!(
        "{KOPF};ende;{};{};{};{};{};{}",
        a.kennung,
        a.art.kurz(),
        anlass_kurz(a.anlass),
        a.start.sekunden(),
        a.ende
            .map_or_else(|| "-".into(), |e| e.sekunden().to_string()),
        if a.erfolg == Some(true) {
            "ok"
        } else {
            "fehler"
        }
    )
}

/// Ein gelesener Eintrag: Kennung, Art, Anlass, Start und, falls beendet, Ende und Erfolg.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Zeile {
    kennung: String,
    art: Art,
    anlass: Anlass,
    start: Zeitpunkt,
    ende: Option<(Option<Zeitpunkt>, bool)>,
}

fn zeit(t: &str) -> Option<Zeitpunkt> {
    let s: i64 = t.parse().ok()?;
    (s.to_string() == t).then_some(())?;
    Zeitpunkt::aus_sekunden(s)
}

fn lies_zeile(z: &str) -> Option<Zeile> {
    let f: Vec<&str> = z.split(';').collect();
    let (kopf, typ, kennung, art, anlass, start) = (
        *f.first()?,
        *f.get(1)?,
        *f.get(2)?,
        *f.get(3)?,
        *f.get(4)?,
        *f.get(5)?,
    );
    if kopf != KOPF || !kennung_gut(kennung) {
        return None;
    }
    let mut zeile = Zeile {
        kennung: kennung.to_string(),
        art: Art::aus_kurz(art)?,
        anlass: anlass_aus(anlass)?,
        start: zeit(start)?,
        ende: None,
    };
    match (typ, f.len()) {
        ("start", 6) => {}
        ("ende", 8) => {
            let ende = match *f.get(6)? {
                "-" => None,
                t => Some(zeit(t)?),
            };
            let erfolg = match *f.get(7)? {
                "ok" => true,
                "fehler" => false,
                _ => return None,
            };
            zeile.ende = Some((ende, erfolg));
        }
        _ => return None,
    }
    Some(zeile)
}

/// Was nach einem Neustart vom Verlauf bekannt ist: die letzten Auftraege, neueste zuerst, und
/// die Zahl unlesbarer Zeilen.
#[derive(Debug, Default)]
pub struct Geladen {
    pub auftraege: Vec<Auftrag>,
    pub unlesbar: usize,
}

/// Liest den Verlauf. Ein Start ohne Ende wird als unterbrochen gezeigt.
pub fn lies(text: &str) -> Geladen {
    let mut zeilen: Vec<Zeile> = Vec::new();
    let mut unlesbar = 0_usize;
    for z in text.lines().filter(|z| !z.is_empty()) {
        match lies_zeile(z) {
            Some(neu) => match zeilen.iter_mut().find(|x| x.kennung == neu.kennung) {
                Some(alt) if neu.ende.is_some() => *alt = neu,
                Some(_) => unlesbar = unlesbar.saturating_add(1),
                None => zeilen.push(neu),
            },
            None => unlesbar = unlesbar.saturating_add(1),
        }
    }
    let auftraege = zeilen
        .into_iter()
        .rev()
        .take(VERLAUF)
        .map(|z| {
            let (ende, erfolg, frueher, protokoll) = match z.ende {
                Some((ende, erfolg)) => (
                    ende,
                    erfolg,
                    Frueher::Beendet,
                    "Vor dem letzten Neustart des Dienstes; das Protokoll ist nicht mehr da.",
                ),
                None => (
                    None,
                    false,
                    Frueher::Unterbrochen,
                    "Unterbrochen: Der Dienst endete während dieses Laufs (Fenster geschlossen, \
                     Rechner neu gestartet oder Absturz). Was der Lauf geschafft hat, steht im \
                     Journal; das Lagebild zeigt den Stand und was noch offen ist.",
                ),
            };
            Auftrag {
                nr: 0,
                kennung: z.kennung,
                art: z.art,
                anlass: z.anlass,
                start: z.start,
                ende,
                erfolg: Some(erfolg),
                protokoll: vec![protokoll.to_string()],
                hinweis: None,
                frueher,
            }
        })
        .collect();
    Geladen {
        auftraege,
        unlesbar,
    }
}

/// Laedt den Verlauf und schreibt die Datei gekuerzt neu (nur die gezeigten Auftraege, je mit
/// ihrem Ende, falls bekannt). Ein Schreibfehler aendert nichts an der Anzeige.
pub fn laden(wurzel: &Path) -> Result<Geladen, String> {
    let p = pfad(wurzel);
    let Some(text) = lies_falls_da(&p)? else {
        return Ok(Geladen::default());
    };
    let g = lies(&text);
    let mut neu = String::new();
    for a in g.auftraege.iter().rev() {
        let z = if a.frueher == Frueher::Unterbrochen {
            start_zeile(a)
        } else {
            ende_zeile(a)
        };
        neu.push_str(&z);
        neu.push('\n');
    }
    if let Err(e) = schreibe_ganz(&p, &neu) {
        eprintln!("[Dienst] Verlauf nicht gekuerzt: {e}");
    }
    Ok(g)
}

/// Haengt eine Zeile an und schreibt sie auf den Datentraeger.
///
/// # Errors
/// Die Datei laesst sich nicht oeffnen oder schreiben. Die Werkbank startet dann keinen
/// Auftrag (Startzeile) oder zeigt den Fehler an (Endzeile).
pub fn vermerke(wurzel: &Path, zeile: &str) -> Result<(), String> {
    let p = pfad(wurzel);
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
        .and_then(|mut f| {
            writeln!(f, "{zeile}")?;
            f.sync_all()
        })
        .map_err(|e| format!("{}: {e}", p.display()))
}
