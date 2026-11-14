//! Fehlerbelege aus typisierten Quellen. Diagnosefreitext bleibt lokal.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use depot_journal::{Art as Journalart, kurssperre};
use depot_kern::{Datum, Zeitpunkt};
use depot_lage::anbieter::{self, Eintrag, Ergebnis, Wer};
use depot_lage::zeit_text;

use crate::werkbank::{Anlass, Art, Auftrag, Frueher};

use super::{Zeitraum, ablauf::Ablauf, textdatei};

pub(super) struct Belege {
    ereignisse: Vec<Ereignis>,
    aufrufzahlen: BTreeMap<Wer, (usize, usize)>,
    anbieter_unlesbar: Option<usize>,
    dienst_luecken: bool,
    ki_sperre: bool,
    kurssperre: bool,
}

struct Ereignis {
    zeit: Zeitpunkt,
    ort: &'static str,
    stufe: &'static str,
    ursache: &'static str,
    reaktion: String,
    beleg: String,
}

impl Belege {
    pub(super) fn laden(
        daten: &Path,
        ablauf: &Ablauf,
        zeitraum: Zeitraum,
        luecken: &mut String,
    ) -> Self {
        let (mut aufrufe, anbieter_unlesbar) = match textdatei(&daten.join(anbieter::DATEI)) {
            Ok(Some(text)) => {
                let (aufrufe, unlesbar) = anbieter::lies_alle(&text);
                (aufrufe, Some(unlesbar))
            }
            Ok(None) => {
                luecken.push_str("- Anbieterprotokoll: Datei nicht vorhanden.\n");
                (Vec::new(), None)
            }
            Err(grund) => {
                let _ = writeln!(luecken, "- Anbieterprotokoll: {grund}.");
                (Vec::new(), None)
            }
        };
        aufrufe.retain(|e| zeitraum.enthaelt(e.zeit));
        let mut aufrufzahlen = BTreeMap::<Wer, (usize, usize)>::new();
        let mut ereignisse = Vec::new();
        for aufruf in aufrufe.iter().filter(|e| e.ergebnis != Ergebnis::Neu) {
            let (gesamt, auffaellig) = aufrufzahlen.entry(aufruf.wer).or_default();
            *gesamt = gesamt.saturating_add(1);
            if let Some(e) = anbieterfehler(aufruf) {
                *auffaellig = auffaellig.saturating_add(1);
                ereignisse.push(e);
            }
        }
        ereignisse.extend(dienstfehler(ablauf.laeufe(), zeitraum));

        if let Some(zeit) = kurssperre::letzter_fehlalarm(daten).filter(|z| zeitraum.enthaelt(*z)) {
            ereignisse.push(Ereignis {
                zeit,
                ort: "Kursdiagnose",
                stufe: "Warnung",
                ursache: "Fehlalarm durch sauberen Prüfabruf belegt",
                reaktion: "Automatische Aufhebung der Kurssperre vermerkt".into(),
                beleg: "Kurssperrenvermerk".into(),
            });
        }
        ereignisse.sort_by_key(|e| e.zeit);
        Self {
            ereignisse,
            aufrufzahlen,
            anbieter_unlesbar,
            dienst_luecken: ablauf.hat_luecken(),
            ki_sperre: daten.join("ki.halt").exists(),
            kurssperre: daten.join(kurssperre::KURSSPERRE).exists(),
        }
    }

    pub(super) fn ampel(&self, journal_bestaetigt: bool) -> &'static str {
        if !journal_bestaetigt || self.dienst_luecken || self.anbieter_unlesbar != Some(0) {
            "Rot (Beleglücken)"
        } else {
            "Gelb (Belege teilweise verfügbar)"
        }
    }

    pub(super) fn bericht(
        &self,
        journal: Option<&[(Journalart, String)]>,
        zeitraum: Zeitraum,
        gemini_netzfehler: &BTreeSet<Zeitpunkt>,
    ) -> String {
        let mut aus = String::from(concat!(
            "\n## Fehler und Warnungen\n\n",
            "Die Tabelle enthält datierte Anbieter- und Dienstbelege. Fehlt ein Nachweis über Ursache ",
            "oder Folge, bleibt die Lücke offen. Freie Diagnosemeldungen werden nicht exportiert. ",
            "Ein späterer erfolgreicher Anbieteraufruf belegt allein keine Wiederholung derselben Anfrage. ",
            "Von automatisch als Fehlalarm aufgehobenen Kurssperren zeigt der Export den letzten Vermerk, wenn er im Zeitraum liegt.\n\n",
            "| Zeit | Ort | Stufe | Ursache | Reaktion oder Folge | Beleg |\n",
            "|---|---|---|---|---|---|\n",
        ));
        let neue_netzfehler: Vec<_> = gemini_netzfehler
            .iter()
            .copied()
            .filter(|zeit| zeitraum.enthaelt(*zeit))
            .filter(|zeit| {
                !self.ereignisse.iter().any(|e| {
                    e.zeit == *zeit
                        && e.ort == Wer::Gemini.anzeigename()
                        && e.ursache == GEMINI_NETZ
                })
            })
            .map(|zeit| Ereignis {
                zeit,
                ort: Wer::Gemini.anzeigename(),
                stufe: "Warnung",
                ursache: GEMINI_NETZ,
                reaktion: "Versandstatus und weiterer Versuch nicht belegt".into(),
                beleg: "KI-Gespräch".into(),
            })
            .collect();
        let mut ereignisse: Vec<_> = self.ereignisse.iter().chain(&neue_netzfehler).collect();
        ereignisse.sort_by_key(|e| e.zeit);
        if ereignisse.is_empty() {
            aus.push_str("| – | – | – | Kein Ereignis aus diesen Quellen belegt | – | – |\n");
        }
        for e in ereignisse {
            let _ = writeln!(
                aus,
                "| {} | {} | {} | {} | {} | {} |",
                zeit_text(e.zeit),
                e.ort,
                e.stufe,
                e.ursache,
                e.reaktion,
                e.beleg
            );
        }
        if !neue_netzfehler.is_empty() {
            let _ = writeln!(
                aus,
                "\nZusätzliche Gemini-Netzfehler aus den KI-Gesprächen: {}. Die folgenden Aufrufzahlen stammen aus dem Anbieterprotokoll.",
                neue_netzfehler.len()
            );
        }
        if !self.aufrufzahlen.is_empty() {
            aus.push_str("\n### Anbieteraufrufe im Zeitraum\n\n");
            for (&wer, &(gesamt, auffaellig)) in &self.aufrufzahlen {
                let _ = writeln!(
                    aus,
                    "- {}: {gesamt} Aufrufbelege, davon {auffaellig} Fehler oder Warnungen.",
                    wer.anzeigename()
                );
            }
        }
        if let Some(n) = self.anbieter_unlesbar {
            let _ = writeln!(aus, "\nUnlesbare Anbieterzeilen: {n}.");
        } else {
            aus.push_str("\nAnbieterprotokoll fehlt oder ist nicht lesbar.\n");
        }
        aus.push_str("\n### Schutzmaßnahmen im Journal\n\n");
        if let Some(journal) = journal {
            journalbelege(&mut aus, journal, zeitraum);
        } else {
            aus.push_str(
                "- Journal nicht bestätigt. Historische Schutzmaßnahmen sind nicht prüfbar.\n",
            );
        }
        let _ = writeln!(
            aus,
            "\n### Sperren beim Export\n\n- KI-Sperre: {}.\n- Kurssperre: {}.\n",
            super::an_aus(self.ki_sperre),
            super::an_aus(self.kurssperre),
        );
        aus
    }
}

const GEMINI_NETZ: &str = "Keine Gemini-Antwort erhalten";

fn anbieterfehler(e: &Eintrag) -> Option<Ereignis> {
    let ki_berater = matches!(e.wer, Wer::Dachs | Wer::Maus);
    let (stufe, ursache, reaktion) = match e.ergebnis {
        Ergebnis::Ok | Ergebnis::Neu => return None,
        Ergebnis::Limit => (
            "Warnung",
            "Anbieterlimit erreicht",
            if ki_berater {
                e.warten_bis.map_or_else(
                    || "KI pausiert bis zum Ende des UTC-Tags, sofern keine neuere Anbietermeldung vorliegt".into(),
                    |bis| format!("KI pausiert bis {}, sofern keine neuere Anbietermeldung vorliegt", zeit_text(bis)),
                )
            } else {
                "Weitere Reaktion nicht belegt".into()
            },
        ),
        Ergebnis::Guthaben => (
            "Fehler",
            "Guthaben oder Kontingent beim Anbieter fehlt",
            if ki_berater {
                "KI pausiert bis zum nächsten Frankfurter Tag, sofern keine neuere Anbietermeldung vorliegt".into()
            } else {
                "Weitere Reaktion nicht belegt".into()
            },
        ),
        Ergebnis::Abgelehnt => (
            "Fehler",
            "Anbieter hat den Schlüssel abgelehnt",
            "Anfrage abgelehnt".into(),
        ),
        Ergebnis::Ueberlastet => (
            "Fehler",
            "Anbieter überlastet oder gestört",
            "Weitere Reaktion nicht belegt".into(),
        ),
        Ergebnis::Fehler => (
            "Fehler",
            "Anbieterfehler, genaue Ursache nicht belegt",
            "Weitere Reaktion nicht belegt".into(),
        ),
        Ergebnis::Netz if e.wer == Wer::Gemini => (
            "Warnung",
            GEMINI_NETZ,
            "Versandstatus und weiterer Versuch nicht belegt".into(),
        ),
        Ergebnis::Netz => (
            "Warnung",
            "Keine Verbindung zum Anbieter",
            "Anfrage nicht gesendet. Weiterer Versuch für diese Anfrage nicht belegt".into(),
        ),
        Ergebnis::Unklar => (
            "Fehler",
            "Antwort fehlt oder ist unlesbar",
            if ki_berater {
                "Kostenschutz greift. Sperrstatus beim Export steht unten".into()
            } else {
                "Weitere Reaktion nicht belegt".into()
            },
        ),
        Ergebnis::Kosten => (
            "Fehler",
            "Kosten oder Verbrauch über der Grenze",
            if ki_berater {
                "Kostenschutz greift. Sperrstatus beim Export steht unten".into()
            } else {
                "Weitere Reaktion nicht belegt".into()
            },
        ),
    };
    Some(Ereignis {
        zeit: e.zeit,
        ort: e.wer.anzeigename(),
        stufe,
        ursache,
        reaktion,
        beleg: e.http.map_or_else(
            || "Anbieterprotokoll".into(),
            |http| format!("Anbieterprotokoll, HTTP {http}"),
        ),
    })
}

fn dienstfehler(laeufe: &[Auftrag], zeitraum: Zeitraum) -> impl Iterator<Item = Ereignis> + '_ {
    laeufe.iter().filter_map(move |lauf| {
        let (zeit, ursache, folge) = match (lauf.art, lauf.frueher, lauf.erfolg) {
            (Art::PapierAusgelassen, _, _) => (
                lauf.start,
                "Bis Börsenschluss kein Start",
                "Papiertag ausgelassen",
            ),
            (_, Frueher::Unterbrochen, _) => (
                lauf.start,
                "Dienst endete während des Laufs. Zeitpunkt und Auslöser fehlen",
                "Lauf unterbrochen",
            ),
            (_, _, Some(false)) => (
                lauf.ende.unwrap_or(lauf.start),
                "Lauf als fehlerhaft vermerkt. Genauere Ursache fehlt im Dienstverlauf",
                "Kein erfolgreicher Abschluss belegt",
            ),
            _ => return None,
        };
        if !zeitraum.enthaelt(zeit) {
            return None;
        }
        let mut reaktion = folge.to_owned();
        if lauf.anlass == Anlass::Zeitplan
            && matches!(lauf.art, Art::KurseVon(_))
            && let Some(versuch) = laeufe
                .iter()
                .filter(|neu| {
                    neu.art == lauf.art
                        && neu.anlass == Anlass::Zeitplan
                        && neu.start > lauf.start
                        && neu.start >= zeit
                        && neu.start.frankfurt_tag() == lauf.start.frankfurt_tag()
                })
                .min_by_key(|neu| neu.start)
        {
            let _ = write!(
                reaktion,
                ". Weiterer automatischer Abruf um {} ({})",
                zeit_text(versuch.start),
                versuch.kennung
            );
        }
        Some(Ereignis {
            zeit,
            ort: lauf.art.name(),
            stufe: if lauf.art == Art::PapierAusgelassen {
                "Warnung"
            } else {
                "Fehler"
            },
            ursache,
            reaktion,
            beleg: if lauf.ende.is_none() {
                format!("Dienstlauf {}, Uhrzeit der Startvormerkung", lauf.kennung)
            } else {
                format!("Dienstlauf {}", lauf.kennung)
            },
        })
    })
}

fn journalbelege(aus: &mut String, journal: &[(Journalart, String)], zeitraum: Zeitraum) {
    let mut gefunden = false;
    for (art, text) in journal {
        let mut felder = text.split(';');
        if *art == Journalart::PapierFreigabe {
            if let Some(zeit) = felder
                .next_back()
                .and_then(Zeitpunkt::lies)
                .filter(|zeit| zeitraum.enthaelt(*zeit))
            {
                gefunden = true;
                let _ = writeln!(
                    aus,
                    "- {} · Einrichtung · Papier-Gesamtverlustsperre im Dialog freigegeben.",
                    zeit_text(zeit)
                );
            }
            continue;
        }
        let Some(tag) = felder
            .next()
            .and_then(Datum::lies)
            .filter(|tag| (zeitraum.von..=zeitraum.bis).contains(tag))
        else {
            continue;
        };
        let befunde = match (art, felder.next()) {
            (Journalart::PapierHalt, Some("notbremse")) => {
                "Notbremse aktiviert. Schutzverkäufe laufen weiter."
            }
            (Journalart::PapierHalt, Some("gesamtverlust")) => {
                "Gesamtverlustgrenze erreicht. Neue Käufe gesperrt."
            }
            (Journalart::NotbremseFreigabe, _) => "Notbremse beim Tagesabschluss zurückgesetzt.",
            _ => continue,
        };
        gefunden = true;
        let _ = writeln!(aus, "- {tag} · Papiertag · {befunde} Uhrzeit nicht belegt.");
    }
    if !gefunden {
        aus.push_str("- Keine datierte Schutzmaßnahme aus diesen Journalarten belegt.\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Dienst;
    use std::fs;
    #[test]
    fn fehlerampel_zeigt_anbieterluecken_bei_bestaetigtem_journal() {
        for anbieter_unlesbar in [None, Some(1)] {
            let belege = Belege {
                ereignisse: Vec::new(),
                aufrufzahlen: BTreeMap::new(),
                anbieter_unlesbar,
                dienst_luecken: false,
                ki_sperre: false,
                kurssperre: false,
            };
            assert_eq!(belege.ampel(true), "Rot (Beleglücken)");
        }
    }

    #[test]
    fn journal_zeigt_schutzmassnahmen_am_belegten_tag() -> Result<(), String> {
        let tag = Datum::neu(2030, 1, 10).ok_or("Datum")?;
        let zeitraum = Zeitraum { von: tag, bis: tag };
        let journal = [
            (Journalart::PapierHalt, "2030-01-10;notbremse".into()),
            (Journalart::PapierHalt, "2030-01-10;gesamtverlust".into()),
            (Journalart::NotbremseFreigabe, "2030-01-10;500000".into()),
            (
                Journalart::PapierFreigabe,
                "2030-01-09;500000;einrichtung;dialog;2030-01-09T23:30:00Z".into(),
            ),
            (Journalart::PapierHalt, "2030-01-09;notbremse".into()),
        ];
        let mut aus = String::new();
        journalbelege(&mut aus, &journal, zeitraum);
        assert!(aus.contains("Notbremse aktiviert. Schutzverkäufe laufen weiter."));
        assert!(aus.contains("Gesamtverlustgrenze erreicht. Neue Käufe gesperrt."));
        assert!(aus.contains("Notbremse beim Tagesabschluss zurückgesetzt."));
        assert!(aus.contains("10.01.2030 00:30 MEZ · Einrichtung · Papier-Gesamtverlustsperre"));
        assert_eq!(aus.lines().count(), 4);
        let mut aus = String::new();
        journalbelege(
            &mut aus,
            &[(Journalart::Abschluss, "2030-01-10;500000".into())],
            zeitraum,
        );
        assert!(aus.contains("Keine datierte Schutzmaßnahme"));
        Ok(())
    }

    #[test]
    fn dienstwarnungen_unterscheiden_ausfall_unterbrechung_und_wiederholung() -> Result<(), String>
    {
        let tag = Datum::neu(2030, 1, 10).ok_or("Datum")?;
        let danach = tag.plus_tage(1).ok_or("Folgetag")?;
        let zeit = |tag, h| {
            Zeitpunkt::frankfurt(tag, h, 0, 0)
                .map(Zeitpunkt::sekunden)
                .ok_or("Zeit")
        };
        let text = format!(
            concat!(
                "verlauf-v1;start;301-1;kurse_xetra;zeitplan;{}\n",
                "verlauf-v1;ende;301-2;papier_ausgelassen;zeitplan;{};{};fehler\n",
                "verlauf-v1;start;301-3;kurse_xetra;hand;{}\n",
                "verlauf-v1;start;301-4;kurse_xetra;zeitplan;{}\n",
            ),
            zeit(tag, 7)?,
            zeit(tag, 18)?,
            zeit(tag, 18)?,
            zeit(tag, 8)?,
            zeit(danach, 8)?,
        );
        let verlauf = crate::verlauf::lies_zeitraum(&text, tag, danach);
        let zeitraum = Zeitraum {
            von: tag,
            bis: danach,
        };
        let ereignisse = dienstfehler(&verlauf.auftraege, zeitraum).collect::<Vec<_>>();
        let unterbrochen = ereignisse
            .iter()
            .find(|e| e.beleg.contains("301-1"))
            .ok_or("Unterbrechung")?;
        assert_eq!(unterbrochen.reaktion, "Lauf unterbrochen");
        assert!(unterbrochen.beleg.contains("Uhrzeit der Startvormerkung"));
        let ausgelassen = ereignisse
            .iter()
            .find(|e| e.beleg.contains("301-2"))
            .ok_or("Ausfall")?;
        assert_eq!(ausgelassen.stufe, "Warnung");
        assert_eq!(ausgelassen.reaktion, "Papiertag ausgelassen");
        Ok(())
    }

    #[test]
    fn bericht_belegt_fehler_pausen_und_erneuten_abruf() -> Result<(), String> {
        use depot_lage::anbieter::{self, Eintrag, Ergebnis, Wer};

        let wurzel = std::env::temp_dir().join(format!("depot-fehler-{}", crate::neues_token()));
        let tag = Datum::neu(2030, 1, 10).ok_or("Datum")?;
        let donnerstag = |h, m| Zeitpunkt::frankfurt(tag, h, m, 0).ok_or("Zeit");
        let daten = wurzel.join("daten");
        fs::create_dir_all(&daten).map_err(|e| e.to_string())?;
        let von = donnerstag(7, 30)?;
        let fehlzeit = donnerstag(7, 31)?;
        let danach = donnerstag(8, 0)?;
        let alt = donnerstag(0, 0)?.sekunden().saturating_sub(60);
        let alt = Zeitpunkt::aus_sekunden(alt).ok_or("Vortag")?;
        let mut limit = Eintrag::neu(fehlzeit, Wer::Dachs, Ergebnis::Limit, Some(429));
        limit.warten_bis = Some(danach);
        for eintrag in [
            Eintrag::neu(alt, Wer::Gemini, Ergebnis::Ueberlastet, Some(503)),
            limit,
            Eintrag::neu(von, Wer::Maus, Ergebnis::Guthaben, Some(429)),
            Eintrag::neu(danach, Wer::Maus, Ergebnis::Ok, Some(200)),
        ] {
            anbieter::anhaengen(&daten.join(anbieter::DATEI), &eintrag)?;
        }
        fs::write(
            crate::verlauf::pfad(&wurzel),
            format!(
                concat!(
                    "verlauf-v1;ende;111-1;kurse_xetra;zeitplan;{};{};fehler\n",
                    "verlauf-v1;ende;111-2;kurse_xetra;zeitplan;{};{};ok\n",
                ),
                von.sekunden(),
                fehlzeit.sekunden(),
                danach.sekunden(),
                danach.sekunden()
            ),
        )
        .map_err(|e| e.to_string())?;
        fs::write(daten.join("ki.halt"), "Privater-Diagnosetext").map_err(|e| e.to_string())?;
        let dienst = Dienst::neu(
            wurzel.clone(),
            "Testtoken".into(),
            Box::new(|_, _| std::process::Command::new("beispielprogramm")),
            Box::new(|_, jetzt| {
                depot_lage::lagebild(&depot_lage::Umgebung {
                    jetzt,
                    journal: depot_lage::Zugang::NichtEingerichtet,
                    kursquelle: depot_lage::Hinterlegt::Nein,
                    kursquelle_us: depot_lage::Hinterlegt::Nein,
                    berater: [
                        (depot_kern::BeraterId::Dachs, depot_lage::Hinterlegt::Nein),
                        (depot_kern::BeraterId::Maus, depot_lage::Hinterlegt::Nein),
                    ],
                    sperren: Vec::new(),
                    anbieter: Vec::new(),
                    anbieter_unlesbar: 0,
                    ki_halt: None,
                })
            }),
        );
        let zeitraum = Zeitraum { von: tag, bis: tag };
        let mut luecken = String::new();
        let ablauf = Ablauf::laden(&dienst, zeitraum, donnerstag(12, 0)?, &mut luecken);
        let belege = Belege::laden(&daten, &ablauf, zeitraum, &mut luecken);
        let archiv = belege.bericht(Some(&[]), zeitraum, &BTreeSet::from([fehlzeit]));
        assert_eq!(belege.ampel(true), "Gelb (Belege teilweise verfügbar)");
        assert_eq!(belege.ampel(false), "Rot (Beleglücken)");
        fs::remove_dir_all(wurzel).map_err(|e| e.to_string())?;
        assert!(
            archiv.contains("Maus (GPT) | Fehler | Guthaben oder Kontingent beim Anbieter fehlt")
        );
        assert!(archiv.contains("Dachs (Claude) | Warnung | Anbieterlimit erreicht"));
        assert!(archiv.contains("KI pausiert bis 10.01.2030 08:00 MEZ"));
        assert!(archiv.contains("Weiterer automatischer Abruf um 10.01.2030 08:00 MEZ (111-2)"));
        assert!(archiv.contains("Gemini | Warnung | Keine Gemini-Antwort erhalten | Versandstatus und weiterer Versuch nicht belegt | KI-Gespräch"));
        assert!(archiv.contains("- KI-Sperre: an."));
        assert!(archiv.contains("Maus (GPT): 2 Aufrufbelege, davon 1 Fehler oder Warnungen."));
        for ausgeschlossen in ["Privater-Diagnosetext", "HTTP 503", "HTTP 200"] {
            assert!(!archiv.contains(ausgeschlossen));
        }
        Ok(())
    }
}
