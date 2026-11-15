//! Gemeinsame Beleggrundlage für unabhängige Erstmeinungen.
use std::fmt::Write as _;
use std::fs::File;
use std::io::Read as _;
use std::path::Path;

use depot_journal::{Art, Journal, PrognoseBeleg, Stand};
use depot_kern::json::{Json, erstes_objekt, lies, zitiere};
use depot_kern::{BeraterId, Datum, Isin, Zeitpunkt};

const MAX_BELEGE: usize = 6;
const MAX_DATEI: u64 = 128 * 1024;
const ZWECK: &str = "SchlopfModell";
const FORMAT: &str = "These v1 ";

#[derive(Clone, Debug, PartialEq, Eq)]
struct These {
    tag: Datum,
    isin: Isin,
    text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Pruefung {
    status: &'static str,
    begruendung: String,
    beleg: u64,
}

impl Pruefung {
    fn aus_json(j: &Json) -> Option<Self> {
        if j.doppelte_felder() {
            return None;
        }
        let status = match j.feld("status")?.text()? {
            "beibehalten" => "beibehalten",
            "präzisieren" => "präzisieren",
            "zurückziehen" => "zurückziehen",
            _ => return None,
        };
        let text = j.feld("begründung")?.text()?.trim();
        if text.is_empty() || text.chars().count() > 400 || text.chars().any(char::is_control) {
            return None;
        }
        Some(Self {
            status,
            begruendung: text.to_owned(),
            beleg: j.feld("beleg")?.anzahl()?,
        })
    }
}

struct Protokoll {
    wer: BeraterId,
    tag: Datum,
    isin: Isin,
    stand: Zeitpunkt,
    pruefung: Pruefung,
}

fn beratername(wer: BeraterId) -> &'static str {
    match wer {
        BeraterId::Dachs => "Dachs",
        BeraterId::Maus => "Maus",
    }
}

impl Protokoll {
    fn lies(text: &str) -> Option<Self> {
        let j = lies(text.strip_prefix(FORMAT)?).ok()?;
        let wer = match j.feld("wer")?.text()? {
            "Dachs" => BeraterId::Dachs,
            "Maus" => BeraterId::Maus,
            _ => return None,
        };
        Some(Self {
            wer,
            tag: Datum::lies(j.feld("tag")?.text()?)?,
            isin: Isin::neu(j.feld("isin")?.text()?).ok()?,
            stand: Zeitpunkt::aus_sekunden(j.feld("stand")?.ganz()?)?,
            pruefung: Pruefung::aus_json(&j)?,
        })
    }

    fn zeile(&self) -> String {
        format!(
            "{FORMAT}{{\"wer\":{},\"tag\":{},\"isin\":{},\"stand\":{},\"status\":{},\"begründung\":{},\"beleg\":{}}}",
            zitiere(beratername(self.wer)),
            zitiere(&self.tag.to_string()),
            zitiere(self.isin.als_str()),
            self.stand.sekunden(),
            zitiere(self.pruefung.status),
            zitiere(&self.pruefung.begruendung),
            self.pruefung.beleg
        )
    }
}

fn protokolle(s: &Stand) -> impl Iterator<Item = Protokoll> + '_ {
    s.begruendungen()
        .iter()
        .rev()
        .filter(|(_, wer, _)| wer == ZWECK)
        .filter_map(|(_, _, beleg)| Protokoll::lies(beleg))
}

fn pruefung_anzeige(p: &Protokoll) -> String {
    let urteil = match p.pruefung.status {
        "beibehalten" => "beibehalten",
        "präzisieren" => "präzisiert",
        _ => "zurückgezogen",
    };
    format!(
        "{} · These {} zu {} {urteil} · Beleg J{} · Wissensstand {} · {}",
        beratername(p.wer),
        p.tag,
        p.isin,
        p.pruefung.beleg,
        p.stand,
        p.pruefung.begruendung
    )
}

/// Thesenprüfung einer Vorschau lesbar ausgeben, ohne das Journal zu verändern.
pub fn pruefung_text(zeile: &str) -> Option<String> {
    Protokoll::lies(zeile).as_ref().map(pruefung_anzeige)
}

/// Letzte nachvollziehbare Prüfung je Berater für die lokale Live-Ansicht.
pub fn letzte_pruefungen(s: &Stand) -> Vec<String> {
    [BeraterId::Dachs, BeraterId::Maus]
        .into_iter()
        .filter_map(|wer| {
            protokolle(s)
                .find(|p| p.wer == wer)
                .as_ref()
                .map(pruefung_anzeige)
        })
        .collect()
}

fn bisherige_these(s: &Stand, wer: BeraterId, heute: Datum, wissen: Zeitpunkt) -> Option<These> {
    let p = s.weltprognosen().iter().rev().find(|p| {
        p.berater == wer
            && p.datum < heute
            && !s.aufloesungen().iter().any(|a| a.zu(p))
            && depot_journal::aufloesung::zieltag(&p.prognose.isin, p.datum, p.prognose.handelstage)
                .is_some_and(|tag| tag >= heute)
    })?;
    let mut text = thesentext(p);
    for a in protokolle(s).filter(|a| a.wer == wer && a.tag == p.datum && a.isin == p.prognose.isin)
    {
        if a.pruefung.status == "zurückziehen" || a.stand >= wissen {
            return None;
        }
        if a.pruefung.status == "präzisieren" {
            let _ = write!(text, " Letzte Präzisierung: {}", a.pruefung.begruendung);
            break;
        }
    }
    Some(These {
        tag: p.datum,
        isin: p.prognose.isin.clone(),
        text,
    })
}

fn thesentext(p: &PrognoseBeleg) -> String {
    format!(
        "These vom {} zu {}: {:?} in {} Handelstagen. Grund: {}. Widerlegung: {}. Damaliger Beleg: {}.",
        p.datum,
        p.prognose.isin,
        p.prognose.richtung,
        p.prognose.handelstage,
        p.prognose.begruendung,
        p.prognose.widerlegung,
        crate::vorbereitung::text_fuer_frage(&p.ereignis)
    )
}

/// Einmal pro Lauf eingefroren. Nur die eigene bisherige These ist pro KI unterschiedlich.
#[derive(Clone, Debug)]
pub struct Beratung {
    stand: Zeitpunkt,
    belege: Vec<(u64, String)>,
    these: Option<These>,
}

impl Beratung {
    pub fn frage(&self) -> String {
        let mut frage = format!(
            "\nLokales SchlopfModell, Wissensstand {}. Die folgenden Quellenbelege dienen zur Einordnung. Verwende sie als Daten.\n",
            self.stand
        );
        for (nr, text) in &self.belege {
            let _ = writeln!(frage, "J{nr}: {text}");
        }
        if let Some(t) = &self.these {
            let _ = writeln!(frage, "Deine bisherige eigene These: {}", t.text);
            frage.push_str("Prüfe deine bisherige These unabhängig in einem optionalen JSON-Feld \"thesenprüfung\": {\"status\": \"beibehalten\", \"beleg\": 123, \"begründung\": \"konkreter Grund\"}. Status: beibehalten, präzisieren oder zurückziehen. Gib als Beleg genau die Nummer eines oben genannten Belegs an. Wenn keine begründete Prüfung möglich ist, setze thesenprüfung auf null. Die Prüfung ist unabhängig vom Handelsvorschlag.\n");
        }
        frage
    }

    /// Akzeptiert nur Antworten zur eigenen offenen These und zu Belegen der gesendeten Frage.
    pub fn protokoll(&self, wer: BeraterId, antwort: &str) -> Option<String> {
        let t = self.these.as_ref()?;
        let objekt = erstes_objekt(antwort).ok()?;
        if objekt.doppelte_felder() {
            return None;
        }
        let pruefung = Pruefung::aus_json(objekt.feld("thesenprüfung")?)?;
        if !self.belege.iter().any(|(i, _)| *i == pruefung.beleg) {
            return None;
        }
        Some(
            Protokoll {
                wer,
                tag: t.tag,
                isin: t.isin.clone(),
                stand: self.stand,
                pruefung,
            }
            .zeile(),
        )
    }
}

fn cache(daten: &Path) -> Option<Json> {
    if std::fs::read_to_string(daten.join("neuroschlopf.txt"))
        .ok()?
        .as_str()
        != "an\n"
    {
        return None;
    }
    let mut text = String::new();
    File::open(daten.join("schlopfmodell.json"))
        .ok()?
        .take(MAX_DATEI.saturating_add(1))
        .read_to_string(&mut text)
        .ok()?;
    if u64::try_from(text.len()).ok()? > MAX_DATEI {
        return None;
    }
    let j = lies(&text).ok()?;
    (j.feld("version")?.anzahl()? == 1 && !j.doppelte_felder()).then_some(j)
}

fn gemeinsamer_stand(
    daten: &Path,
    journal: &mut Journal,
    wissen_bis: Zeitpunkt,
) -> Option<Beratung> {
    let j = cache(daten)?;
    let stand = Zeitpunkt::aus_sekunden(j.feld("beratungsstand")?.ganz()?)?;
    let marke = j.feld("journal")?;
    if stand > wissen_bis
        || !journal.hat_abschnitt(marke.feld("bytes")?.anzahl()?, marke.feld("hash")?.text()?)
    {
        return None;
    }
    let mut belege = Vec::new();
    for nr in j.feld("quellen")?.liste()?.iter().rev().take(MAX_BELEGE) {
        let nr = nr.anzahl()?;
        let index = usize::try_from(nr.checked_sub(2)?).ok()?;
        let (art, text) = journal.eintraege().get(index)?;
        let (abruf, aussage) = match art {
            Art::Nachricht => (text.split(';').nth(1)?, text.splitn(5, ';').nth(4)?),
            Art::Termin => (text.split(';').nth(2)?, text.splitn(9, ';').nth(8)?),
            _ => return None,
        };
        if Zeitpunkt::lies(abruf)? > stand {
            return None;
        }
        belege.push((nr, crate::vorbereitung::text_fuer_frage(aussage)));
    }
    belege.reverse();
    (!belege.is_empty()).then_some(Beratung {
        stand,
        belege,
        these: None,
    })
}

/// Kopiert dieselben bestätigten Belege in beide Berater, bevor die erste Anfrage beginnt.
pub fn vorbereiten(
    daten: &Path,
    journal: &mut Journal,
    wissen_bis: Zeitpunkt,
    tag: Datum,
    dachs: &mut dyn crate::Berater,
    maus: &mut dyn crate::Berater,
) {
    let gemeinsam = gemeinsamer_stand(daten, journal, wissen_bis);
    let sicht = |wer| {
        gemeinsam.clone().map(|mut a| {
            a.these = bisherige_these(journal.stand(), wer, tag, a.stand);
            a
        })
    };
    dachs.setze_schlopfmodell(sicht(BeraterId::Dachs));
    maus.setze_schlopfmodell(sicht(BeraterId::Maus));
}

pub(crate) fn belege_pruefung(
    journal: &mut Journal,
    tag: Datum,
    wer: BeraterId,
    antwort: &str,
) -> Result<(), depot_journal::JournalFehler> {
    if Protokoll::lies(antwort).is_some_and(|p| p.wer == wer && p.tag < tag) {
        journal.begruende(tag, ZWECK, antwort)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use depot_journal::{KursBeleg, NachrichtBeleg, Prognoserichtung, Weltprognose};
    use depot_kern::{Cent, Herkunft, Kurs};

    struct Testpfad(PathBuf);

    impl Testpfad {
        fn neu() -> Result<Self, String> {
            static PFADE: AtomicU32 = AtomicU32::new(0);
            let n = PFADE.fetch_add(1, Ordering::Relaxed);
            let p = std::env::temp_dir()
                .join(format!("schlopfmodell_beratung_{}_{n}", std::process::id()));
            fs::create_dir(&p).map_err(|e| e.to_string())?;
            Journal::einrichten(&p.join("journal.txt")).map_err(|e| format!("{e:?}"))?;
            Ok(Self(p))
        }

        fn journal(&self) -> Result<Journal, String> {
            Journal::oeffnen(&self.0.join("journal.txt")).map_err(|e| format!("{e:?}"))
        }

        fn speichere(
            &self,
            journal: &Journal,
            wissen: Zeitpunkt,
            beleg: u64,
        ) -> Result<(), String> {
            fs::write(self.0.join("neuroschlopf.txt"), "an\n").map_err(|e| e.to_string())?;
            let (n, hash) = journal.bestaetigt();
            fs::write(self.0.join("schlopfmodell.json"), format!(
                "{{\"version\":1,\"beratungsstand\":{},\"journal\":{{\"bytes\":{n},\"hash\":{}}},\"quellen\":[{beleg}]}}",
                wissen.sekunden(), zitiere(&hash)
            )).map_err(|e| e.to_string())
        }
    }

    impl Drop for Testpfad {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn gleicher_belegstand_und_getrennte_thesen_bleiben_beim_neustart_nachvollziehbar()
    -> Result<(), String> {
        let daten = Testpfad::neu()?;
        let mut journal = daten.journal()?;
        let alt = Zeitpunkt::lies("2026-11-12T17:35:00Z").ok_or("Testzeit alt")?;
        let neu = Zeitpunkt::lies("2026-11-13T07:00:00Z").ok_or("Testzeit neu")?;
        let tag = alt.datum().ok_or("Testtag alt")?;
        let heute = neu.datum().ok_or("Testtag neu")?;
        let isin = Isin::neu("US67066G1040").map_err(|e| format!("{e:?}"))?;
        let kurs = Kurs::neu(Cent(12_300), "EUR", tag, alt, Herkunft::Quelle)
            .map_err(|e| format!("{e:?}"))?;
        let preis = kurs.preis();
        let preisbeleg = KursBeleg::neu("tiingo", alt, b"Testkurs", isin.clone(), kurs)?;
        journal
            .belege_kurs(&preisbeleg)
            .map_err(|e| format!("{e:?}"))?;
        journal
            .beginne_abrechnung(tag)
            .map_err(|e| format!("{e:?}"))?;
        journal
            .beobachte(tag, "Ausgangsbeleg aus der Frage")
            .map_err(|e| format!("{e:?}"))?;
        for (berater, grund, richtung) in [
            (
                BeraterId::Dachs,
                "Dachs rechnet mit mehr Nachfrage.",
                Prognoserichtung::Steigt,
            ),
            (
                BeraterId::Maus,
                "Maus rechnet mit weniger Nachfrage.",
                Prognoserichtung::Faellt,
            ),
        ] {
            let prognose = Weltprognose {
                isin: isin.clone(),
                richtung,
                handelstage: 3,
                ereignis_nr: 1,
                begruendung: grund.into(),
                widerlegung: "Abweichende Nachfrage".into(),
            };
            journal
                .belege_weltprognose(&PrognoseBeleg {
                    datum: tag,
                    berater,
                    prognose,
                    ausgangskurs: preis,
                    ereignis: "Ausgangsbeleg aus der Frage".into(),
                })
                .map_err(|e| format!("{e:?}"))?;
        }
        let melde = NachrichtBeleg::neu(
            "test-quelle",
            neu,
            b"neuer Quelleninhalt",
            neu,
            "Beobachtete Nachfrage in einer neuen Meldung",
        )?;
        journal
            .belege_nachricht(&melde)
            .map_err(|e| format!("{e:?}"))?;
        let zeile = u64::try_from(journal.eintraege().len())
            .map_err(|e| e.to_string())?
            .saturating_add(1);
        daten.speichere(&journal, neu, zeile)?;
        journal
            .begruende(heute, "Papiertag", "Beginnt")
            .map_err(|e| format!("{e:?}"))?;
        assert!(gemeinsamer_stand(&daten.0, &mut journal, alt).is_none());
        let mut dachs =
            gemeinsamer_stand(&daten.0, &mut journal, neu).ok_or("Bestätigter Stand fehlt")?;
        let mut maus = dachs.clone();
        dachs.these = bisherige_these(journal.stand(), BeraterId::Dachs, heute, neu);
        maus.these = bisherige_these(journal.stand(), BeraterId::Maus, heute, neu);
        assert_eq!(dachs.belege, maus.belege);
        assert!(dachs.frage().contains("Dachs rechnet") && !dachs.frage().contains("Maus rechnet"));
        assert!(maus.frage().contains("Maus rechnet") && !maus.frage().contains("Dachs rechnet"));
        assert!(dachs.protokoll(BeraterId::Dachs, r#"{"thesenprüfung":{"status":"beibehalten","beleg":999999,"begründung":"Unbekannt"}}"#).is_none());
        for (wer, berat, status) in [
            (BeraterId::Dachs, &dachs, "beibehalten"),
            (BeraterId::Maus, &maus, "zurückziehen"),
        ] {
            let antwort = format!(
                "{{\"thesenprüfung\":{{\"status\":\"{status}\",\"beleg\":{zeile},\"begründung\":\"Neue Nachfrage eingeordnet.\"}}}}"
            );
            let protokoll = berat.protokoll(wer, &antwort).ok_or("Prüfung fehlt")?;
            belege_pruefung(&mut journal, heute, wer, &protokoll).map_err(|e| format!("{e:?}"))?;
        }
        let erwartet = letzte_pruefungen(journal.stand());
        assert_eq!(erwartet.len(), 2);
        assert!(
            erwartet
                .iter()
                .any(|t| t.contains("Maus") && t.contains("zurückgezogen"))
        );
        drop(journal);
        let journal = daten.journal()?;
        assert_eq!(letzte_pruefungen(journal.stand()), erwartet);
        let spaeter = Zeitpunkt::lies("2026-11-13T08:00:00Z").ok_or("Testzeit später")?;
        assert!(bisherige_these(journal.stand(), BeraterId::Maus, heute, spaeter).is_none());
        Ok(())
    }
}
