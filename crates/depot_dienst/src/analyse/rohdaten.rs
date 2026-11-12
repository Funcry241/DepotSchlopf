//! Positivliste für zeitlich gefilterte Originalzeilen und Laufprotokolle.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use depot_kern::{Zeitpunkt, json};
use depot_lage::anbieter::Eintrag;

use super::{Betrieb, Zeitraum, textdatei};
use crate::{protokoll, verlauf};

const BUDGET: usize = 8 * 1024 * 1024;

#[derive(Default)]
pub(super) struct Rohdaten {
    pub(super) dateien: Vec<(String, String)>,
    verbraucht: usize,
}

impl Rohdaten {
    pub(super) fn datei(&mut self, name: String, text: String, luecken: &mut String) {
        if text.is_empty() {
            return;
        }
        let groesse = self.verbraucht.saturating_add(text.len());
        if groesse > BUDGET {
            let _ = writeln!(luecken, "- {name} fehlt wegen der Größenbegrenzung.");
            return;
        }
        self.verbraucht = groesse;
        self.dateien.push((name, text));
    }

    pub(super) fn sammeln(
        &mut self,
        wurzel: &Path,
        betrieb: &Betrieb,
        zeitraum: Zeitraum,
        luecken: &mut String,
    ) {
        self.datei(
            "rohdaten/anbieter.txt".into(),
            zeilen(&betrieb.anbieter, |z| {
                Eintrag::lies(z).is_some_and(|e| zeitraum.enthaelt(e.zeit))
            }),
            luecken,
        );
        self.datei(
            "rohdaten/dienst_verlauf.txt".into(),
            zeilen(&betrieb.verlauf, |z| {
                verlauf::zeile_im_zeitraum(z, zeitraum.von, zeitraum.bis)
            }),
            luecken,
        );
        self.gespraeche(&wurzel.join("daten"), zeitraum, luecken);
        self.protokolle(wurzel, &betrieb.verlauf, zeitraum, luecken);
    }

    fn gespraeche(&mut self, daten: &Path, zeitraum: Zeitraum, luecken: &mut String) {
        let ordner = daten.join("gespraeche");
        if !fs::symlink_metadata(&ordner).is_ok_and(|m| m.is_dir()) {
            luecken.push_str(
                "- Gesprächsdateien fehlen oder der Ordner ist kein reguläres Verzeichnis.\n",
            );
            return;
        }
        for tag in zeitraum.tage() {
            let name = format!("{tag}.jsonl");
            match textdatei(&ordner.join(&name)) {
                Ok(Some(text)) => {
                    let passend = zeilen(&text, |z| {
                        json::lies(z)
                            .ok()
                            .filter(|j| !j.doppelte_felder())
                            .and_then(|j| Zeitpunkt::lies(j.feld("zeit")?.text()?))
                            .and_then(Zeitpunkt::in_frankfurt)
                            .is_some_and(|(datum, ..)| datum == tag)
                    });
                    if passend.lines().count() != text.lines().count() {
                        let _ = writeln!(
                            luecken,
                            "- {name}: unvollständige, unlesbare oder abweichend datierte Zeilen fehlen."
                        );
                    }
                    self.datei(format!("rohdaten/gespraeche/{name}"), passend, luecken);
                }
                Ok(None) => {}
                Err(grund) => {
                    let _ = writeln!(luecken, "- Originaldatei {name}: {grund}.");
                }
            }
        }
    }

    fn protokolle(
        &mut self,
        wurzel: &Path,
        dienst: &str,
        zeitraum: Zeitraum,
        luecken: &mut String,
    ) {
        let mut laeufe = verlauf::lies_zeitraum(dienst, zeitraum.von, zeitraum.bis);
        laeufe
            .auftraege
            .sort_by_key(|lauf| lauf.erfolg == Some(true));
        for lauf in laeufe.auftraege.iter().take(protokoll::AUFBEWAHREN) {
            let name = format!("protokolle/{}.log", lauf.kennung);
            match protokoll::lesen(wurzel, &lauf.kennung) {
                Ok(text) => self.datei(name, text, luecken),
                Err(grund) => {
                    let _ = writeln!(luecken, "- {name}: {grund}");
                }
            }
        }
        if laeufe.auftraege.len() > protokoll::AUFBEWAHREN {
            luecken.push_str("- Ältere Laufprotokolle fehlen wegen der Aufbewahrungsgrenze.\n");
        }
    }
}

fn zeilen(text: &str, mut auswahl: impl FnMut(&str) -> bool) -> String {
    text.split_inclusive('\n')
        .filter(|z| z.ends_with('\n') && auswahl(z.trim_end_matches(['\r', '\n'])))
        .collect()
}
