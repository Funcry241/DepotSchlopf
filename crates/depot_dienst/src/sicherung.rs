//! Sicherungsziel und zuletzt gemeldeter Stand für Zeitplan und Oberfläche.

use std::fs::{self, File, TryLockError};
use std::path::{Path, PathBuf};

use depot_kern::Zeitpunkt;
use depot_lage::html::maskiere;

use crate::werkbank::{lies_falls_da, schreibe_ganz};

fn zielpfad(wurzel: &Path) -> PathBuf {
    wurzel.join("daten/sicherung_ziel.txt")
}

pub fn ziel(wurzel: &Path) -> String {
    lies_falls_da(&zielpfad(wurzel))
        .ok()
        .flatten()
        .unwrap_or_default()
        .trim_end_matches(['\r', '\n'])
        .to_owned()
}

/// Das Ziel war gestern um 23:30 eingerichtet, und es gab keinen Versuch.
pub fn nachholen(wurzel: &Path, stand: &Stand, jetzt: Zeitpunkt) -> bool {
    let Some(termin) = jetzt
        .frankfurt_tag()
        .and_then(|tag| tag.plus_tage(-1))
        .and_then(|tag| Zeitpunkt::frankfurt(tag, 23, 30, 0))
    else {
        return false;
    };
    let eingestellt = fs::metadata(zielpfad(wurzel))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|zeit| zeit.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|zeit| i64::try_from(zeit.as_secs()).ok())
        .and_then(Zeitpunkt::aus_sekunden);
    eingestellt.is_some_and(|zeit| zeit <= termin)
        && stand
            .auto_start
            .into_iter()
            .chain(stand.erfolg)
            .all(|zeit| zeit.frankfurt_tag() < termin.frankfurt_tag())
}

pub fn setze_ziel(wurzel: &Path, text: &str) -> Result<(), String> {
    let text = text.trim();
    if text.contains(['\r', '\n', '\0']) || !text.is_empty() && !Path::new(text).is_absolute() {
        return Err("Sicherungsziel muss ein absoluter Pfad sein".into());
    }
    schreibe_ganz(&zielpfad(wurzel), &format!("{text}\n"))
}

#[derive(Default)]
pub struct Stand {
    auto_start: Option<Zeitpunkt>,
    erfolg: Option<Zeitpunkt>,
    ergebnis: String,
    dateien: Option<u32>,
    gleicher_datentraeger: bool,
    fremd: bool,
    fehler: String,
    start: Option<Zeitpunkt>,
}

pub fn lies(wurzel: &Path) -> Stand {
    let text = match lies_falls_da(&wurzel.join("daten/sicherung_stand.txt")) {
        Ok(Some(text)) => text,
        Ok(None) => return Stand::default(),
        Err(_) => {
            return Stand {
                fremd: true,
                ..Stand::default()
            };
        }
    };
    let mut zeilen = text.lines();
    if zeilen.next() != Some("sicherung-stand-v1") {
        return Stand {
            fremd: true,
            ..Stand::default()
        };
    }
    let mut stand = Stand::default();
    let zeit = |text: &str| text.parse().ok().and_then(Zeitpunkt::aus_sekunden);
    for (name, wert) in zeilen.filter_map(|z| z.split_once('=')) {
        match name {
            "auto_start" => stand.auto_start = zeit(wert),
            "erfolg" => stand.erfolg = zeit(wert),
            "start" => stand.start = wert.split_once(';').and_then(|(z, _)| zeit(z)),
            "fehler" => wert.clone_into(&mut stand.fehler),
            "ergebnis" => wert.clone_into(&mut stand.ergebnis),
            "dateien" => stand.dateien = wert.parse().ok(),
            "gleicher_datentraeger" => stand.gleicher_datentraeger = wert == "ja",
            _ => {}
        }
    }
    stand
}

impl Stand {
    pub fn auto_faellig(&self, jetzt: Zeitpunkt, nachholen: bool) -> bool {
        let Some((tag, h, m, ..)) = jetzt.in_frankfurt() else {
            return false;
        };
        let heute = |zeit: Zeitpunkt| zeit.in_frankfurt().is_some_and(|(d, ..)| d == tag);
        ((h, m) >= (23, 30) || nachholen)
            && !self.auto_start.is_some_and(heute)
            && !self.erfolg.is_some_and(heute)
    }

    pub fn laeuft(&self, wurzel: &Path) -> bool {
        if self.ergebnis != "laeuft" {
            return false;
        }
        let pfad = wurzel.join("daten/sicherung.lock");
        fs::symlink_metadata(&pfad).is_ok_and(|m| m.is_file())
            && File::open(pfad)
                .is_ok_and(|datei| matches!(datei.try_lock_shared(), Err(TryLockError::WouldBlock)))
    }

    pub fn html(&self, jetzt: Zeitpunkt, laeuft: bool) -> String {
        let alter = self.erfolg.and_then(|e| jetzt.sekunden_seit(e));
        let offen = self.fremd
            || self.gleicher_datentraeger
            || matches!(self.ergebnis.as_str(), "fehler" | "laeuft")
            || alter.is_none_or(|s| s >= 259_200);
        let erfolg = match (self.erfolg, self.dateien) {
            (Some(zeit), Some(n)) => {
                format!("{}, {n} Dateien, geprüft", depot_lage::zeit_text(zeit))
            }
            _ => "nie gesichert".into(),
        };
        let hinweis = match self.ergebnis.as_str() {
            _ if self.fremd => " · Standdatei hat ein unbekanntes Format",
            _ if laeuft => " · Sicherung läuft",
            "laeuft" => " · letzter Versuch abgebrochen",
            "fehler" => " · letzter Versuch fehlgeschlagen (siehe Protokoll)",
            _ => "",
        };
        let detail = match self.ergebnis.as_str() {
            "laeuft" if !laeuft => self.start.map_or_else(String::new, |start| {
                format!(" ({})", depot_lage::zeit_text(start))
            }),
            "fehler" if !self.fehler.is_empty() => format!(" · {}", maskiere(&self.fehler)),
            _ => String::new(),
        };
        let datentraeger = if self.gleicher_datentraeger {
            " · gleicher Datenträger: schützt nicht vor Plattenausfall"
        } else {
            ""
        };
        let klasse = if offen { "offen" } else { "gut" };
        format!(
            "<p class=\"{klasse}\">Letzte Sicherung: {}{hinweis}{detail}{datentraeger}</p>",
            maskiere(&erfolg)
        )
    }
}

pub fn formular(token: &str, ziel: &str, status: &str) -> String {
    format!(
        "<section id=\"sicherung\"><h2>Sicherung</h2>{status}\
        <p>Ein Cloud-Ordner (z. B. OneDrive) lädt die Dateien hoch. Das Journal enthält keine Schlüssel, aber dein Papierdepot und deine Belege.</p>\
        <form method=\"post\" action=\"/sicherung-ziel\"><input type=\"hidden\" name=\"t\" value=\"{token}\">\
        <label>Zielordner <input name=\"ziel\" value=\"{}\"></label> <button>Speichern</button></form>\
        <form method=\"post\" action=\"/ausfuehren\"><input type=\"hidden\" name=\"t\" value=\"{token}\">\
        <button name=\"art\" value=\"sicherung\">Jetzt sichern</button></form></section>",
        maskiere(ziel),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_kern::Datum;

    fn zeit(h: u8, m: u8) -> Result<Zeitpunkt, String> {
        Datum::neu(2026, 11, 13)
            .and_then(|tag| Zeitpunkt::frankfurt(tag, h, m, 0))
            .ok_or_else(|| "Testzeit fehlt".into())
    }

    #[test]
    fn auto_hand_kein_zweiter_auto() -> Result<(), String> {
        let stand = Stand {
            auto_start: Some(zeit(23, 30)?),
            ergebnis: "fehler".into(),
            ..Stand::default()
        };
        assert!(!stand.auto_faellig(zeit(23, 50)?, false));
        Ok(())
    }

    #[test]
    fn hand_erfolg_ersetzt_auto() -> Result<(), String> {
        let stand = Stand {
            erfolg: Some(zeit(18, 0)?),
            ..Stand::default()
        };
        assert!(!stand.auto_faellig(zeit(23, 30)?, false));
        Ok(())
    }

    #[test]
    fn zeitplan_2330() -> Result<(), String> {
        let stand = Stand::default();
        assert!(!stand.auto_faellig(zeit(23, 29)?, false));
        assert!(stand.auto_faellig(zeit(23, 30)?, false));
        assert!(!stand.auto_faellig(zeit(8, 0)?, false));
        Ok(())
    }

    #[test]
    fn abbruch_zeigt_abgebrochen() -> Result<(), String> {
        let stand = Stand {
            ergebnis: "laeuft".into(),
            ..Stand::default()
        };
        let html = stand.html(zeit(23, 40)?, false);
        assert!(html.contains("abgebrochen") && html.contains("offen"));
        assert!(stand.html(zeit(23, 40)?, true).contains("Sicherung läuft"));
        Ok(())
    }

    #[test]
    fn standdatei_fremde_form() -> Result<(), String> {
        let stand = Stand {
            fremd: true,
            ..Stand::default()
        };
        let html = stand.html(zeit(23, 40)?, false);
        assert!(html.contains("unbekanntes Format") && html.contains("nie gesichert"));
        Ok(())
    }

    #[test]
    fn nachholen_einmal() -> Result<(), String> {
        let start = zeit(7, 0)?;
        let mut stand = Stand::default();
        assert!(stand.auto_faellig(start, true));
        stand.auto_start = Some(start);
        assert!(!stand.auto_faellig(zeit(23, 30)?, true));
        Ok(())
    }

    #[test]
    fn mitternacht() -> Result<(), String> {
        let stand = Stand {
            auto_start: Some(zeit(0, 30)?),
            ..Stand::default()
        };
        assert!(!stand.auto_faellig(zeit(23, 30)?, true));
        Ok(())
    }

    #[test]
    fn ohne_standdatei_gilt_nicht_gesichert() -> Result<(), String> {
        assert!(
            Stand::default()
                .html(zeit(23, 40)?, false)
                .contains("nie gesichert")
        );
        Ok(())
    }

    #[test]
    fn alter_erfolg_wird_gelb() -> Result<(), String> {
        let jetzt = zeit(23, 40)?;
        let alt = jetzt
            .sekunden()
            .checked_sub(259_200)
            .and_then(Zeitpunkt::aus_sekunden);
        let stand = Stand {
            erfolg: alt,
            dateien: Some(5),
            ..Stand::default()
        };
        assert!(stand.html(jetzt, false).contains("class=\"offen\""));
        Ok(())
    }
}
