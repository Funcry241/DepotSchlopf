//! Zwei kurze Journalkontakte um einen unabhängigen NeuroSchlopf-Aufruf.
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::path::Path;
use std::process::ExitCode;

use depot_berater::{Https, KiBerater, SPERRDATEI};
use depot_journal::neurofrage::{
    NeuroAufrufErlaubnis, NeuroErgebnis, NeuroFrage, NeuroRunde, NeuroSperre, ergebnis_pfad,
    lokal_aktiv,
};
use depot_journal::{Art, Journal, JournalFehler, Lesecache, Stand, gespraeche};
use depot_kern::modelle::BeraterEinstellung;
use depot_kern::{BeraterId, Universum, Zeitpunkt};
use depot_sim::Berater;
use depot_sim::blindpruefung::prueffaelle_mit_zeilen;
use depot_sim::schlopfmodell::{kandidaten::neue_faelle, offene_thesen};

const JOURNAL_MAX: u64 = 32 * 1024 * 1024;

const RESERVIERT: &str = "NeuroSchlopf: Reservierung abgeschlossen";

#[derive(Debug)]
enum Fehler {
    Uebersprungen(String),
    Halt(String),
}

impl From<String> for Fehler {
    fn from(e: String) -> Self {
        Self::Halt(e)
    }
}

impl From<std::io::Error> for Fehler {
    fn from(e: std::io::Error) -> Self {
        Self::Halt(e.to_string())
    }
}

impl From<JournalFehler> for Fehler {
    fn from(e: JournalFehler) -> Self {
        if e == JournalFehler::Belegt {
            Self::Uebersprungen("Journal ist gerade belegt".into())
        } else {
            Self::Halt(format!("Journal: {e:?}"))
        }
    }
}

fn ueberspringe(grund: &str) -> Fehler {
    Fehler::Uebersprungen(grund.into())
}

fn lies(pfad: &Path, max: u64) -> Result<String, Fehler> {
    let mut text = String::new();
    File::open(pfad)?
        .take(max.saturating_add(1))
        .read_to_string(&mut text)?;
    if u64::try_from(text.len()).is_ok_and(|n| n <= max) {
        Ok(text)
    } else {
        Err(Fehler::Halt("Datei zu groß".into()))
    }
}

fn frage_lesen(pfad: &Path) -> Result<NeuroFrage, Fehler> {
    NeuroFrage::lies(lies(pfad, 40_000)?.trim())
        .ok_or_else(|| Fehler::Halt("NeuroSchlopf-Auftrag unlesbar".into()))
}

fn einstellungen(daten: &Path, wer: BeraterId) -> Result<(BeraterEinstellung, usize), Fehler> {
    super::update_bestaetigt(daten)?;
    let e = depot_journal::ki_einstellungen::lesen(daten)?;
    if e.neuroschlopf == 0 || !e.neuro_berater().contains(&wer) || !lokal_aktiv(daten)? {
        return Err(ueberspringe(
            "Externe Frage in der Einrichtung ausgeschaltet",
        ));
    }
    let profil = match wer {
        BeraterId::Dachs => e.dachs,
        BeraterId::Maus => e.maus,
    };
    Ok((profil, e.neuroschlopf))
}

fn frage_bestaetigt(
    pfad: &Path,
    frage: &NeuroFrage,
    jetzt: Zeitpunkt,
) -> Result<(u64, String), Fehler> {
    let heute = jetzt
        .frankfurt_tag()
        .ok_or("Kalendertag fehlt".to_owned())?;
    if (!frage.ist_vorpruefung() && heute > frage.these_gueltig_bis) || frage.bekannt > jetzt {
        return Err(ueberspringe(
            "Frage liegt außerhalb des bestätigten Zeitfensters",
        ));
    }
    let mut cache = Lesecache::default();
    let (stand, eintraege) = cache.lesen(pfad, JOURNAL_MAX)?;
    // Die aufwendigere Kandidatensuche nutzt den bestätigten Cache ohne Journalsperre.
    let passend = if frage.ist_vorpruefung() {
        vorpruefung_bestaetigt(pfad, stand, eintraege, frage)?
    } else {
        korrektur_bestaetigt(stand, eintraege, frage, heute)?
    };
    if !passend {
        return Err(ueberspringe(
            "Kein passender bestätigter Fragefall mehr vorhanden",
        ));
    }
    cache
        .bestaetigt()
        .ok_or_else(|| Fehler::Halt("Bestätigter Journalstand fehlt".into()))
}

fn vorpruefung_bestaetigt(
    pfad: &Path,
    stand: &Stand,
    eintraege: &[(Art, String)],
    frage: &NeuroFrage,
) -> Result<bool, Fehler> {
    let daten = pfad.parent().ok_or("Datenpfad fehlt".to_owned())?;
    let tag = frage.these_gueltig_bis;
    let mut budget = 8 * 1024 * 1024;
    let aufrufe = gespraeche::lesen_begrenzt(daten, tag, &mut budget)
        .map_err(|_| "Gespeicherte Erstmeinungsfragen sind nicht vollständig lesbar".to_owned())?;
    let Some((gespraeche, 0)) = aufrufe else {
        return Ok(false);
    };
    Ok(
        prueffaelle_mit_zeilen(stand, eintraege, &gespraeche, tag).is_ok_and(|faelle| {
            faelle.iter().any(|fall| {
                fall.kennung == frage.id.fall()
                    && fall
                        .vorpruefung(frage.wer, frage.id.runde(), frage.nachfrage.clone())
                        .as_ref()
                        == Some(frage)
            })
        }),
    )
}

fn korrektur_bestaetigt(
    stand: &Stand,
    eintraege: &[(Art, String)],
    frage: &NeuroFrage,
    heute: depot_kern::Datum,
) -> Result<bool, Fehler> {
    let thesen = offene_thesen(stand, heute, &BTreeSet::new(), &mut || Ok(()))?;
    let faelle = neue_faelle(
        stand,
        eintraege,
        &BTreeSet::from([frage.beleg_zeile]),
        &thesen,
        &mut || Ok(()),
    )?;
    let wer = match frage.thesen_wer {
        BeraterId::Dachs => "Dachs",
        BeraterId::Maus => "Maus",
    };
    Ok(faelle.iter().any(|f| {
        f.schluessel == frage.id.fall()
            && f.these_berater == wer
            && f.these_vom == frage.these_vom
            && f.these_gueltig_bis == frage.these_gueltig_bis
            && f.isin == frage.isin
            && f.these == frage.these
            && Some(f.ursprung_zeile) == frage.bisher_zeile
            && f.beleg_zeile == frage.beleg_zeile
            && f.quelle == frage.quelle
            && Some(f.vorher.as_str()) == frage.bisher.as_deref()
            && f.beleg == frage.jetzt
            && f.abruf == frage.bekannt
    }))
}

fn reserviere(
    pfad: &Path,
    frage: &NeuroFrage,
    profil: BeraterEinstellung,
    neuro_max: usize,
    jetzt: Zeitpunkt,
    anker: &(u64, String),
) -> Result<NeuroAufrufErlaubnis, Fehler> {
    let datum = jetzt
        .frankfurt_tag()
        .ok_or("Kalendertag fehlt".to_owned())?;
    let zyklus = depot_sim::tagnummer(datum).ok_or("Tageskennung fehlt".to_owned())?;
    let mut j = Journal::oeffnen(pfad)?;
    if &j.bestaetigt() != anker {
        return Err(ueberspringe(
            "Journal wurde während der Fallprüfung fortgeschrieben",
        ));
    }
    if frage.id.runde() == NeuroRunde::Rueckfrage {
        let ki = j.stand().ki();
        let passend = ki.neurofragen().iter().any(|v| {
            v.wer == frage.wer
                && v.id.fall() == frage.id.fall()
                && v.id.runde() == NeuroRunde::Erstfrage
                && ki
                    .neuroergebnis(v.buchung)
                    .and_then(|e| e.urteil.as_ref())
                    .and_then(|u| u.nachfrage.as_ref())
                    == frage.nachfrage.as_ref()
        });
        if !passend {
            return Err(ueberspringe(
                "Rückfrage gehört zu keiner bestätigten Erstfrage",
            ));
        }
    }
    j.reserviere_neurofrage(&frage.id, frage.wer, datum, zyklus, profil, neuro_max)
        .map_err(|e| {
            if let NeuroSperre::Ki(depot_ki::Sperre::Kassenbuch(fehler)) = e {
                Fehler::Halt(fehler)
            } else {
                Fehler::Uebersprungen(format!("Reservierung: {e:?}"))
            }
        })
}

fn speichere_ergebnis(pfad: &Path, e: &NeuroErgebnis) -> Result<(), Fehler> {
    let temp = pfad.with_extension(format!("{}.tmp", std::process::id()));
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)?;
    let schreiben = f.write_all(e.text().as_bytes()).and_then(|()| f.sync_all());
    drop(f);
    let ersatz = schreiben.and_then(|()| depot_journal::dateiersatz::ersetze(&temp, pfad));
    if let Err(e) = ersatz {
        let _ = fs::remove_file(&temp);
        return Err(e.into());
    }
    #[cfg(not(windows))]
    if let Some(ordner) = pfad.parent() {
        File::open(ordner)?.sync_all()?;
    }
    Ok(())
}

fn wiederholung_reservieren(
    daten: &Path,
    frage: &NeuroFrage,
    ergebnis: &NeuroErgebnis,
    erster_start: Zeitpunkt,
) -> Result<NeuroAufrufErlaubnis, Fehler> {
    super::update_bestaetigt(daten)?;
    let jetzt = super::jetzt().ok_or("Uhr liefert keine Zeit".to_owned())?;
    let datum = jetzt
        .frankfurt_tag()
        .ok_or("Kalendertag fehlt".to_owned())?;
    if Some(datum) != erster_start.frankfurt_tag() {
        return Err(ueberspringe("Zeitfenster für Wiederholung abgelaufen"));
    }
    let (profil, neuro_max) = einstellungen(daten, frage.wer)?;
    let pfad = daten.join("journal.txt");
    let anker = frage_bestaetigt(&pfad, frage, jetzt)?;
    let mut j = Journal::oeffnen(&pfad)?;
    if j.bestaetigt() != anker {
        return Err(ueberspringe(
            "Journal wurde während der Fallprüfung fortgeschrieben",
        ));
    }
    j.belege_neuroergebnis(ergebnis)?;
    j.reserviere_neuro_wiederholung(ergebnis, datum, profil, neuro_max)
        .map_err(|e| match e {
            NeuroSperre::Ki(depot_ki::Sperre::Kassenbuch(fehler)) => Fehler::Halt(fehler),
            _ => Fehler::Uebersprungen(format!("Reservierung der Wiederholung: {e:?}")),
        })
}

fn senden(daten: &Path, auftrag: &Path) -> Result<(), Fehler> {
    let frage = frage_lesen(auftrag)?;
    let ausgabe = ergebnis_pfad(auftrag);
    if ausgabe.try_exists()? {
        return Err(ueberspringe("Ergebnisdatei liegt bereits vor"));
    }
    let jetzt = super::jetzt().ok_or("Uhr liefert keine Zeit".to_owned())?;
    let (profil, neuro_max) = einstellungen(daten, frage.wer)?;
    let mut berater = KiBerater::neu(
        frage.wer,
        super::schluessel(frage.wer),
        Https::neu(),
        Universum::neu(Vec::new()),
        daten.join(SPERRDATEI),
    )
    .mit_einstellung(profil)?
    .mit_protokoll(daten.join(depot_journal::anbieter::DATEI), super::jetzt)
    .mit_gespraechen(super::laufkennung(), None);
    if !berater.angebunden() {
        return Err(ueberspringe("Berater ist zur Zeit nicht verfügbar"));
    }
    let pfad = daten.join("journal.txt");
    let anker = frage_bestaetigt(&pfad, &frage, jetzt)?;
    let einstellung_aktuell = einstellungen(daten, frage.wer)?;
    let tag_aktuell = super::jetzt().and_then(Zeitpunkt::frankfurt_tag);
    if einstellung_aktuell != (profil, neuro_max) || tag_aktuell != jetzt.frankfurt_tag() {
        return Err(ueberspringe("Einstellungen oder Tag inzwischen geändert"));
    }
    let erlaubnis = reserviere(&pfad, &frage, profil, neuro_max, jetzt, &anker)?;
    println!("{RESERVIERT}");
    std::io::stdout().flush()?;
    let ergebnis = berater.neurofrage(erlaubnis, &frage);
    speichere_ergebnis(&ausgabe, &ergebnis)?;
    if !berater.ausgabelimit_erreicht() {
        return Ok(());
    }
    if !berater.angebunden() || berater.kostensperre() {
        let grund = berater
            .warum_nicht()
            .unwrap_or_else(|| "Kostenschutz oder Anbieterpause".into());
        berater.anfrage_uebersprungen(depot_journal::KiZweck::Neurofrage, &grund);
        return Ok(());
    }
    let erneut = match wiederholung_reservieren(daten, &frage, &ergebnis, jetzt) {
        Ok(erlaubnis) => erlaubnis,
        Err(Fehler::Uebersprungen(grund)) => {
            berater.anfrage_uebersprungen(depot_journal::KiZweck::Neurofrage, &grund);
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    berater.kurze_antwort_anfordern();
    speichere_ergebnis(&ausgabe, &berater.neurofrage(erneut, &frage))
}

fn abrechnen(daten: &Path, pfad: &Path) -> Result<(), Fehler> {
    super::update_bestaetigt(daten)?;
    let frage = frage_lesen(pfad)?;
    let ergebnis_pfad = ergebnis_pfad(pfad);
    if !ergebnis_pfad.try_exists()? {
        return Err(ueberspringe(
            "Für diese Reservierung liegt kein bestätigbares Ergebnis vor",
        ));
    }
    let e = NeuroErgebnis::lies(lies(&ergebnis_pfad, 8_000)?.trim())
        .ok_or("NeuroSchlopf-Ergebnis unlesbar".to_owned())?;
    if e.id != frage.id || e.wer != frage.wer {
        return Err(Fehler::Halt(
            "Ergebnis gehört zu einem anderen Auftrag".into(),
        ));
    }
    Journal::oeffnen(&daten.join("journal.txt"))?.belege_neuroergebnis(&e)?;
    Ok(())
}

pub fn lauf(wurzel: &Path, befehl: &str, pfad: &Path) -> ExitCode {
    let daten = wurzel.join("daten");
    let ergebnis = if befehl == "neurofrage" {
        senden(&daten, pfad)
    } else {
        abrechnen(&daten, pfad)
    };
    match ergebnis {
        Ok(()) => ExitCode::SUCCESS,
        Err(Fehler::Uebersprungen(grund)) => {
            eprintln!("[NeuroSchlopf] {grund}.");
            ExitCode::from(3)
        }
        Err(Fehler::Halt(grund)) => {
            eprintln!("[NeuroSchlopf] {grund}.");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_journal::neurofrage::NeuroAuftragId;
    use depot_kern::{Datum, Isin};

    #[test]
    fn papier_kann_waehrend_einer_offenen_neuroerlaubnis_reservieren() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("neuro-journalsperre-{}", std::process::id()));
        fs::create_dir(&dir).map_err(|e| e.to_string())?;
        let pfad = dir.join("journal.txt");
        Journal::einrichten(&pfad).map_err(|e| format!("{e:?}"))?;
        let anker = Journal::hintergrund_abdruck(&pfad).map_err(|e| format!("{e:?}"))?;
        let tag = Datum::neu(2026, 11, 4).ok_or("Tag")?;
        let jetzt = Zeitpunkt::frankfurt(tag, 11, 0, 0).ok_or("Zeit")?;
        let wer = BeraterId::Maus;
        let frage = NeuroFrage {
            id: NeuroAuftragId::neu(&"a".repeat(64), NeuroRunde::Erstfrage).ok_or("ID")?,
            wer,
            thesen_wer: wer,
            these_vom: tag,
            these_gueltig_bis: tag,
            isin: Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?,
            these: "Der Umsatz steigt.".into(),
            bisher_zeile: Some(2),
            beleg_zeile: 3,
            quelle: "test".into(),
            bisher: Some("Bisher".into()),
            jetzt: "Jetzt".into(),
            bekannt: jetzt,
            nachfrage: None,
        };
        let profil = BeraterEinstellung::standard(wer);
        let offen =
            reserviere(&pfad, &frage, profil, 1, jetzt, &anker).map_err(|e| format!("{e:?}"))?;
        // `reserviere` hat ihre exklusive Sperre bereits freigegeben. Auch derselbe Berater
        // bekommt eine reguläre Erlaubnis, bevor ein externes Ergebnis vorliegt.
        let mut j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
        let zyklus = depot_sim::tagnummer(tag).ok_or("Zyklus")?;
        let papier = j
            .reserviere(wer, tag, zyklus)
            .map_err(|e| format!("{e:?}"))?;
        assert!(papier.buchungsnummer() > offen.buchung());
        assert!(j.stand().ki().neuroergebnis(offen.buchung()).is_none());
        drop(j);
        fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
        Ok(())
    }
}
