//! Tageszeitstrahl aus dem Dienstverlauf, dem aktuellen Zeitplan und dem Journal.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use depot_journal::Stand;
use depot_kern::{Boerse, Datum, Zeitpunkt};
use depot_lage::zeit_text;

use crate::Dienst;
use crate::verlauf::{self, Geladen};
use crate::werkbank::{Anlass, Art, Frueher};
use crate::zeitplan::{self, AbrufAusgang, Erledigt, Zeitplan};

use super::{Zeitraum, quelle};

pub(super) struct Ablauf {
    zeitraum: Zeitraum,
    jetzt: Zeitpunkt,
    verlauf: Geladen,
    laufend: Option<String>,
    plan: Zeitplan,
    automatik: &'static str,
    erledigt: Erledigt,
    boersen: Vec<Boerse>,
}

impl Ablauf {
    pub(super) fn laden(
        dienst: &Dienst,
        zeitraum: Zeitraum,
        jetzt: Zeitpunkt,
        luecken: &mut String,
    ) -> Self {
        let zustand = dienst.sperre();
        let original = quelle(&verlauf::pfad(&dienst.wurzel), "Dienstverlauf", luecken);
        let mut verlauf = verlauf::lies_zeitraum(&original, zeitraum.von, zeitraum.bis);
        let laufend = zustand
            .laufend
            .as_ref()
            .filter(|a| zeitraum.enthaelt(a.start));
        if let Some(lauf) = laufend {
            verlauf.auftraege.retain(|a| a.kennung != lauf.kennung);
            verlauf.auftraege.push(lauf.clone());
        }
        Self {
            zeitraum,
            jetzt,
            verlauf,
            laufend: laufend.map(|a| a.kennung.clone()),
            plan: zustand.zeitplan,
            automatik: zustand.automatik_status(),
            erledigt: zustand.erledigt,
            boersen: zustand.boersen.clone(),
        }
    }

    pub(super) fn ampel(&self) -> &'static str {
        if self.verlauf.auftraege.is_empty() {
            "Rot (Dienstverlauf fehlt)"
        } else if self.verlauf.unlesbar > 0
            || self.zeitraum.tage().any(|tag| {
                zeitplan::handelstag(tag)
                    && Some(tag) != self.jetzt.frankfurt_tag()
                    && !self
                        .verlauf
                        .auftraege
                        .iter()
                        .any(|a| a.start.frankfurt_tag() == Some(tag))
            })
        {
            "Rot (Beleglücken)"
        } else {
            "Gelb (historische Planung unbelegt)"
        }
    }

    pub(super) fn bericht(&self, stand: Option<&Stand>) -> String {
        let mut text = String::from(
            "# Betrieb und Abrufe\n\n## Zeitstrahl je Tag\n\n             Zeiten in Frankfurter Ortszeit. Die Sollzeiten zeigen den Zeitplan zum Exportzeitpunkt.              Historische Sollzeiten und Schalterstellungen wurden nicht aufgezeichnet.              Fehlende Einträge können aus früheren Kürzungen stammen.\n",
        );
        let _ = writeln!(
            text,
            "\nAutomatik beim Export: **{}**. Unlesbare Verlaufszeilen: {}.",
            self.automatik, self.verlauf.unlesbar,
        );
        for tag in self.zeitraum.tage() {
            let _ = writeln!(text, "\n### {tag}\n");
            if self.jetzt.frankfurt_tag() == Some(tag) {
                self.heutige_planung(tag, &mut text);
            } else if zeitplan::handelstag(tag) {
                text.push_str("- Historischer Sollplan nicht belegt.\n");
            } else {
                text.push_str("- Börsenfrei laut Handelskalender.\n");
            }
            self.tageslaeufe(tag, &mut text);
            match stand {
                Some(stand) if stand.schlussbeleg(tag).is_some() => {
                    text.push_str("- Papiertag im Journal abgeschlossen. Uhrzeit des Abschlusses nicht belegt.\n");
                }
                Some(_) if zeitplan::handelstag(tag) => {
                    text.push_str("- Kein Tagesabschluss im Journal belegt.\n");
                }
                None if zeitplan::handelstag(tag) => {
                    text.push_str(
                        "- Journal nicht bestätigt. Der Tagesabschluss ist nicht prüfbar.\n",
                    );
                }
                _ => {}
            }
        }
        text
    }

    fn heutige_planung(&self, tag: Datum, text: &mut String) {
        if !zeitplan::handelstag(tag) {
            text.push_str("- Börsenfrei laut Handelskalender.\n");
        }
        if self.automatik == "an" {
            let abrufe = zeitplan::geplante_abrufe(self.plan, &self.erledigt, tag, &self.boersen);
            for ((h, m), art, vorgemerkt) in &abrufe {
                let status = if *vorgemerkt {
                    "Start vorgemerkt"
                } else if Zeitpunkt::frankfurt(tag, *h, *m, 0).is_some_and(|soll| soll > self.jetzt)
                {
                    "geplant"
                } else if matches!(art, Art::KurseVon(_))
                    && self.jetzt.in_frankfurt().is_some_and(|(_, h, m, ..)| {
                        zeitplan::morgen_abgelaufen(&self.plan, tag, (h, m), &self.boersen)
                    })
                {
                    "Kursfenster abgelaufen, kein Start vorgemerkt"
                } else {
                    "Sollzeit erreicht, kein Start vorgemerkt"
                };
                let _ = writeln!(text, "- Soll {h:02}:{m:02} · {} · {status}.", art.name());
            }
            if self.plan.papier_danach && zeitplan::morgenplan(&self.plan) && !abrufe.is_empty() {
                text.push_str(
                    "- Soll nach Kursabrufen bei ausreichenden Kursbelegen · Papiertag.\n",
                );
            }
        }
        for &boerse in &self.boersen {
            if self.erledigt.kurse_von(boerse) != Some(tag) {
                continue;
            }
            if let Some(abruf) = self.erledigt.abruf(boerse) {
                let ausgang = match abruf.ausgang {
                    AbrufAusgang::Laeuft => "läuft",
                    AbrufAusgang::Ok => "erfolgreich",
                    AbrufAusgang::Voruebergehend => "vorübergehend fehlgeschlagen",
                    AbrufAusgang::Fehler => "fehlgeschlagen",
                    AbrufAusgang::Abgebrochen => "abgebrochen",
                };
                let _ = writeln!(
                    text,
                    "- Abrufvermerk · {} · {} Versuch(e), letzter Ausgang: {ausgang}.",
                    Art::KurseVon(boerse).name(),
                    abruf.versuche
                );
            }
        }
    }

    fn tageslaeufe(&self, tag: Datum, text: &mut String) {
        let mut ereignisse = Vec::new();
        for lauf in &self.verlauf.auftraege {
            if lauf.start.frankfurt_tag() == Some(tag) {
                ereignisse.push((lauf.start, false, lauf));
            }
            if let Some(ende) = lauf.ende.filter(|e| e.frankfurt_tag() == Some(tag))
                && lauf.art != Art::PapierAusgelassen
            {
                ereignisse.push((ende, true, lauf));
            }
        }
        ereignisse.sort_by_key(|(zeit, ende, _)| (*zeit, *ende));
        if ereignisse.is_empty() {
            text.push_str("- Kein Laufereignis im verfügbaren Dienstverlauf belegt.\n");
        }
        let mut voriger_abruf = BTreeMap::new();
        for (zeit, ende, lauf) in ereignisse {
            let info = if ende {
                match lauf.erfolg {
                    Some(true) => "Ende, erfolgreich",
                    Some(false) => "Ende, Fehler vermerkt",
                    None => "Ende, Ausgang unbekannt",
                }
            } else if lauf.art == Art::PapierAusgelassen {
                "Ausgelassen"
            } else if self.laufend.as_ref() == Some(&lauf.kennung) {
                "Start, läuft beim Export"
            } else if lauf.frueher == Frueher::Unterbrochen {
                "Start vermerkt, Dienstlauf unterbrochen"
            } else {
                "Start vermerkt"
            };
            let wiederholung = if !ende
                && lauf.anlass == Anlass::Zeitplan
                && matches!(lauf.art, Art::KurseVon(_))
                && let Some(vorher) = voriger_abruf.insert(lauf.art.kurz(), lauf)
            {
                match vorher.frueher {
                    Frueher::Unterbrochen => " · Wiederholung nach unterbrochenem Abruf",
                    _ if vorher.erfolg == Some(false) => {
                        " · Wiederholung nach fehlgeschlagenem Abruf"
                    }
                    _ => " · Weiterer Versuch, Grund nicht belegt",
                }
            } else {
                ""
            };
            let anlass = match lauf.anlass {
                Anlass::Hand => "manuell",
                Anlass::Zeitplan => "automatisch",
            };
            let _ = writeln!(
                text,
                "- {} · {} · {anlass} · {info}{wiederholung} · {}",
                zeit_text(zeit),
                lauf.art.name(),
                lauf.kennung
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeitstrahl_ordnet_laufbelege_und_kennzeichnet_planluecken() -> Result<(), String> {
        let heute = Datum::neu(2030, 1, 10).ok_or("Datum")?;
        let gestern = heute.plus_tage(-1).ok_or("Vortag")?;
        let zeit = |tag, h, m| Zeitpunkt::frankfurt(tag, h, m, 0).ok_or("Zeit");
        let erster = zeit(heute, 7, 30)?;
        let zweiter = zeit(heute, 8, 0)?;
        let fehler = zeit(heute, 7, 31)?;
        let ausfall = zeit(gestern, 17, 30)?;
        let uebernacht = zeit(gestern, 23, 59)?;
        let nach_mitternacht = zeit(heute, 0, 1)?;
        let text = format!(
            "verlauf-v1;ende;10-1;kurse_xetra;zeitplan;{erster};{fehler};fehler\n\
             verlauf-v1;start;10-2;kurse_xetra;zeitplan;{zweiter}\n\
             verlauf-v1;ende;10-3;papier_ausgelassen;zeitplan;{ausfall};{ausfall};fehler\n\
             verlauf-v1;ende;10-4;termine;hand;{uebernacht};{nach_mitternacht};ok\n",
            erster = erster.sekunden(),
            fehler = fehler.sekunden(),
            zweiter = zweiter.sekunden(),
            ausfall = ausfall.sekunden(),
            uebernacht = uebernacht.sekunden(),
            nach_mitternacht = nach_mitternacht.sekunden()
        );
        let ablauf = Ablauf {
            zeitraum: Zeitraum {
                von: gestern,
                bis: heute,
            },
            jetzt: zeit(heute, 8, 5)?,
            verlauf: verlauf::lies_zeitraum(&text, gestern, heute),
            laufend: Some("10-2".into()),
            plan: Zeitplan {
                an: true,
                ..Zeitplan::default()
            },
            automatik: "an",
            erledigt: Erledigt::default(),
            boersen: Boerse::ALLE.to_vec(),
        };
        let bericht = ablauf.bericht(None);
        let (_, heute) = bericht
            .split_once("### 2030-01-10")
            .ok_or("Tagesabschnitt fehlt")?;
        assert!(bericht.contains("Historischer Sollplan nicht belegt."));
        assert!(bericht.contains("Papiertag ausgelassen: vor Börsenschluss nicht gestartet"));
        assert!(heute.contains("Soll 07:20 · Termine abrufen · Sollzeit erreicht"));
        assert!(heute.contains("00:01 MEZ · Termine abrufen · manuell · Ende, erfolgreich"));
        assert!(
            heute.contains("07:31 MEZ · Kurse Xetra abrufen · automatisch · Ende, Fehler vermerkt")
        );
        assert!(heute.contains("08:00 MEZ · Kurse Xetra abrufen · automatisch · Start, läuft beim Export · Wiederholung nach fehlgeschlagenem Abruf"));
        assert!(heute.find("07:31 MEZ") < heute.find("08:00 MEZ"));
        assert_eq!(ablauf.ampel(), "Gelb (historische Planung unbelegt)");
        Ok(())
    }

    #[test]
    fn luecke_im_verlauf_und_journalabschluss_bleiben_getrennt() -> Result<(), String> {
        use depot_journal::{Art, Journal};
        let tag = Datum::neu(2030, 1, 10).ok_or("Datum")?;
        let ablauf = Ablauf {
            zeitraum: Zeitraum { von: tag, bis: tag },
            jetzt: Zeitpunkt::frankfurt(tag, 18, 0, 0).ok_or("Zeit")?,
            verlauf: Geladen::default(),
            laufend: None,
            plan: Zeitplan::default(),
            automatik: "aus",
            erledigt: Erledigt::default(),
            boersen: Boerse::ALLE.to_vec(),
        };
        let ordner = std::env::temp_dir().join(format!("depot-ablauf-{}", crate::neues_token()));
        std::fs::create_dir(&ordner).map_err(|e| e.to_string())?;
        let pfad = ordner.join("journal.txt");
        let bytes = depot_journal::inhalt_fuer(&[(
            Art::Abschluss,
            &format!("{tag};500000;{}", "0".repeat(64)),
        )]);
        let vermerk = depot_journal::vermerk_fuer(&bytes, 0).ok_or("Vermerk")?;
        std::fs::write(&pfad, bytes).map_err(|e| e.to_string())?;
        std::fs::write(pfad.with_extension("vermerk"), vermerk).map_err(|e| e.to_string())?;
        let bericht = Journal::pruefe_lesend(&pfad, |stand| ablauf.bericht(Some(stand)))
            .map_err(|e| format!("{e:?}"))?;
        std::fs::remove_dir_all(ordner).map_err(|e| e.to_string())?;
        assert!(bericht.contains("Kein Laufereignis im verfügbaren Dienstverlauf belegt."));
        assert!(bericht.contains("Papiertag im Journal abgeschlossen."));
        assert_eq!(ablauf.ampel(), "Rot (Dienstverlauf fehlt)");
        Ok(())
    }
}
