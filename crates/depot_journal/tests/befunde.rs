//! Negativtests der Persistenzbasis zu `docs/BEFUNDE.md`: 3·P4, 4·B02, 5·B02, 1·F09 (Neustart)
//! und die vom Kassenbuch ins Journal gewanderten Befunde 1·K06, K12-01, K12-02 und 1·R02.
//!
//! Das Journal ist die einzige dauerhafte Datei. Ein Absturz wird nachgestellt, indem der Test
//! Journal und Anhangsvermerk genau so hinterlaesst, wie sie an dieser Stelle auf der Platte
//! stuenden. Jede Vorbereitung prueft ihren Erfolg ausdruecklich und endet sonst mit
//! „Vorbedingung …“.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{Art, Journal, JournalFehler, inhalt_fuer, vermerk_fuer};
use depot_kern::{
    Cent, Datum, Grenzen, Herkunft, Isin, Kurs, Kurse, Topf, TopfId, Zeitpunkt,
};
use depot_regelwerk::{Kennungsvergabe, notbremse_auftraege};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

/// Eigenes, leeres Verzeichnis je Test; liefert den Pfad des Journals darin (nicht eingerichtet).
fn neuer_pfad() -> Result<PathBuf, String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir =
        std::env::temp_dir().join(format!("depot_journal_befund_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
    Ok(dir.join("journal.txt"))
}

fn eingerichtet() -> Result<PathBuf, String> {
    let p = neuer_pfad()?;
    vorbedingung(Journal::einrichten(&p), "Einrichtung")?;
    Ok(p)
}

fn oeffnen(p: &Path) -> Result<Journal, String> {
    vorbedingung(Journal::oeffnen(p), "Journal oeffnen")
}

fn vermerk(p: &Path) -> PathBuf {
    p.with_extension("vermerk")
}

fn tag(t: u8) -> Result<Datum, String> {
    Datum::neu(2026, 11, t).ok_or_else(|| format!("Vorbedingung: Datum {t}.11.2026"))
}

fn isin(text: &str) -> Result<Isin, String> {
    vorbedingung(Isin::neu(text), "ISIN")
}

/// Alle Dateien im Verzeichnis des Journals ausser Journal und Vermerk.
fn weitere_dateien(p: &Path) -> Result<BTreeSet<String>, String> {
    let dir = p.parent().ok_or("Vorbedingung: Verzeichnis")?;
    let mut aus = BTreeSet::new();
    for e in vorbedingung(fs::read_dir(dir), "Verzeichnis lesen")? {
        let e = vorbedingung(e, "Verzeichniseintrag")?;
        let name = e.file_name().to_string_lossy().into_owned();
        if name != "journal.txt" && name != "journal.vermerk" {
            aus.insert(name);
        }
    }
    Ok(aus)
}

/// Zwei bestaetigte Beobachtungen, ein dritter Eintrag geplant. Liefert den bestaetigten Inhalt
/// und die volle Zeile des geplanten Eintrags.
fn zwei_und_geplant() -> Result<(Vec<u8>, Vec<u8>), String> {
    let zwei = inhalt_fuer(&[
        (Art::Beobachtung, "2026-11-02;Zinsentscheid angekuendigt"),
        (Art::Beobachtung, "2026-11-03;Treffen verschoben"),
    ]);
    let drei = inhalt_fuer(&[
        (Art::Beobachtung, "2026-11-02;Zinsentscheid angekuendigt"),
        (Art::Beobachtung, "2026-11-03;Treffen verschoben"),
        (Art::Beobachtung, "2026-11-04;dritter Eintrag"),
    ]);
    let zeile = drei
        .strip_prefix(zwei.as_slice())
        .ok_or("Vorbedingung: Inhalt waechst nur hinten")?
        .to_vec();
    if zwei.is_empty() || zeile.len() < 10 {
        return Err("Vorbedingung: Inhalt".into());
    }
    Ok((zwei, zeile))
}

/// Hinterlaesst den Stand mitten im Anhaengen: Vermerk mit geplantem Eintrag, im Journal der
/// bestaetigte Inhalt und `teil` Byte der neuen Zeile.
fn abgebrochen(p: &Path, zwei: &[u8], zeile: &[u8], teil: usize) -> Result<(), String> {
    let geplant = u64::try_from(zeile.len()).map_err(|e| e.to_string())?;
    let v = vermerk_fuer(zwei, geplant).ok_or("Vorbedingung: Vermerk")?;
    let mut inhalt = zwei.to_vec();
    inhalt.extend(zeile.iter().take(teil));
    vorbedingung(fs::write(p, inhalt), "Journal schreiben")?;
    vorbedingung(fs::write(vermerk(p), v), "Vermerk schreiben")
}

fn bestaetigt_geschrieben(p: &Path, inhalt: &[u8]) -> Result<(), String> {
    let v = vermerk_fuer(inhalt, 0).ok_or("Vorbedingung: Vermerk")?;
    vorbedingung(fs::write(p, inhalt), "Journal schreiben")?;
    vorbedingung(fs::write(vermerk(p), v), "Vermerk schreiben")
}

fn halt(r: Result<Journal, JournalFehler>) -> Result<String, String> {
    match r {
        Err(JournalFehler::Halt(m)) => Ok(m),
        Err(anders) => Err(format!("statt Halt: {anders:?}")),
        Ok(j) => Err(format!("geoeffnet mit {} Eintraegen", j.eintraege().len())),
    }
}

// ---------------------------------------------------------------------------------------------
// Einrichtung und Grundbetrieb (R25)
// ---------------------------------------------------------------------------------------------

#[test]
fn einrichtung_ist_ausdruecklich_und_ueberschreibt_nichts() -> Result<(), String> {
    let p = neuer_pfad()?;
    halt(Journal::oeffnen(&p))?;
    if p.exists() || vermerk(&p).exists() {
        return Err("Oeffnen hat still eingerichtet".into());
    }
    vorbedingung(Journal::einrichten(&p), "Einrichtung")?;
    let mut j = oeffnen(&p)?;
    vorbedingung(j.beobachte(tag(2)?, "erste Beobachtung"), "Beobachtung")?;
    drop(j);
    let vorher = vorbedingung(fs::read(&p), "Journal lesen")?;
    if Journal::einrichten(&p).is_ok() {
        return Err("zweite Einrichtung angenommen".into());
    }
    let nachher = vorbedingung(fs::read(&p), "Journal lesen")?;
    if vorher != nachher || oeffnen(&p)?.eintraege().len() != 1 {
        return Err("zweite Einrichtung hat das Journal veraendert".into());
    }
    Ok(())
}

#[test]
fn fehlendes_journal_oder_fehlender_vermerk_fuehrt_zu_halt() -> Result<(), String> {
    let p = eingerichtet()?;
    let mut j = oeffnen(&p)?;
    vorbedingung(j.beobachte(tag(2)?, "Beobachtung"), "Beobachtung")?;
    drop(j);
    let v = vorbedingung(fs::read(vermerk(&p)), "Vermerk lesen")?;
    vorbedingung(fs::remove_file(vermerk(&p)), "Vermerk loeschen")?;
    halt(Journal::oeffnen(&p)).map_err(|e| format!("ohne Vermerk: {e}"))?;
    vorbedingung(fs::write(vermerk(&p), v), "Vermerk zurueck")?;
    vorbedingung(fs::remove_file(&p), "Journal loeschen")?;
    halt(Journal::oeffnen(&p)).map_err(|e| format!("ohne Journal: {e}"))?;
    if p.exists() {
        return Err("Oeffnen hat ein neues Journal angelegt".into());
    }
    Ok(())
}

#[test]
fn geleertes_oder_gekuerztes_journal_fuehrt_zu_halt() -> Result<(), String> {
    let p = eingerichtet()?;
    let mut j = oeffnen(&p)?;
    for t in 2..5 {
        vorbedingung(j.beobachte(tag(t)?, "Beobachtung"), "Beobachtung")?;
    }
    drop(j);
    let voll = vorbedingung(fs::read(&p), "Journal lesen")?;
    let mut offen = Vec::new();
    let zeilen: Vec<usize> = voll
        .iter()
        .enumerate()
        .filter(|(_, b)| **b == b'\n')
        .map(|(i, _)| i.saturating_add(1))
        .collect();
    let mut faelle: Vec<(String, Vec<u8>)> = vec![("leer".into(), Vec::new())];
    for ende in zeilen.iter().take(zeilen.len().saturating_sub(1)) {
        faelle.push((format!("auf {ende} Byte"), voll.iter().take(*ende).copied().collect()));
    }
    for (fall, inhalt) in faelle {
        vorbedingung(fs::write(&p, &inhalt), "kuerzen")?;
        if let Err(e) = halt(Journal::oeffnen(&p)) {
            offen.push(format!("{fall}: {e}"));
        }
        if vorbedingung(fs::read(&p), "lesen")? != inhalt {
            offen.push(format!("{fall}: beim Oeffnen veraendert"));
        }
    }
    if offen.is_empty() { Ok(()) } else { Err(format!("{offen:#?}")) }
}

/// Jede Zeile der ganzen Kette wird geprueft, nicht nur die letzte.
#[test]
fn geaenderte_fruehere_zeile_fuehrt_zu_halt() -> Result<(), String> {
    let p = neuer_pfad()?;
    let echt = inhalt_fuer(&[
        (Art::Beobachtung, "2026-11-02;Kurs 100"),
        (Art::Beobachtung, "2026-11-03;Kurs 200"),
    ]);
    bestaetigt_geschrieben(&p, &echt)?;
    oeffnen(&p)?;
    // Gleiche Laenge, anderer Inhalt in der ersten Beobachtung; der Vermerk bleibt echt.
    let falsch: Vec<u8> = String::from_utf8_lossy(&echt).replacen("Kurs 100", "Kurs 900", 1).into_bytes();
    if falsch == echt || falsch.len() != echt.len() {
        return Err("Vorbedingung: Aenderung".into());
    }
    vorbedingung(fs::write(&p, &falsch), "Journal aendern")?;
    halt(Journal::oeffnen(&p))?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// 3·P4, 4·B02, 5·B02 – Absturz beim Anhaengen und in der Wiederherstellung (R38, Kap. 9)
// ---------------------------------------------------------------------------------------------

/// 3·P4, 5·B02 (3): Ein echter Schreibabbruch mit passendem Anhangsvermerk wird gekuerzt. Der
/// abgeschnittene Rest liegt byte-gleich in einer Quarantaenedatei. Das gilt fuer jede Stelle
/// des Abbruchs. Ein zweites Oeffnen aendert nichts mehr.
#[test]
fn befund_3_p4_abgebrochener_anhang_wird_gesichert_und_gekuerzt() -> Result<(), String> {
    let (zwei, zeile) = zwei_und_geplant()?;
    let mut offen = Vec::new();
    for teil in 1..zeile.len() {
        let p = neuer_pfad()?;
        abgebrochen(&p, &zwei, &zeile, teil)?;
        let j = match Journal::oeffnen(&p) {
            Ok(j) => j,
            Err(e) => {
                offen.push(format!("{teil} Byte: {e:?}"));
                continue;
            }
        };
        let n = j.eintraege().len();
        drop(j);
        let jetzt = vorbedingung(fs::read(&p), "Journal lesen")?;
        let q = weitere_dateien(&p)?;
        let gesichert: Vec<Vec<u8>> = q
            .iter()
            .filter_map(|name| p.parent().map(|d| d.join(name)))
            .filter_map(|f| fs::read(f).ok())
            .collect();
        let rest: Vec<u8> = zeile.iter().take(teil).copied().collect();
        if n != 2 || jetzt != zwei || gesichert != vec![rest] {
            offen.push(format!(
                "{teil} Byte: {n} Eintraege, Journal {} Byte, Quarantaene {q:?}",
                jetzt.len()
            ));
            continue;
        }
        // Wiederholt: dasselbe Ergebnis, und das Journal arbeitet normal weiter.
        let mut j = oeffnen(&p)?;
        if j.eintraege().len() != 2 || weitere_dateien(&p)? != q {
            offen.push(format!("{teil} Byte: zweites Oeffnen anders"));
        }
        if j.beobachte(tag(5)?, "danach").is_err() {
            offen.push(format!("{teil} Byte: danach nicht beschreibbar"));
        }
    }
    if offen.is_empty() { Ok(()) } else { Err(format!("{offen:#?}")) }
}

/// 5·B02 (1): Abschlusszeichen der letzten bestaetigten Zeile entfernt: Halt, nichts wird
/// gekuerzt, keine Quarantaene. Gilt auch, wenn der Vermerk einen Anhang ankuendigt.
#[test]
fn befund_5_b02_entferntes_abschlusszeichen_fuehrt_zu_halt() -> Result<(), String> {
    let (zwei, zeile) = zwei_und_geplant()?;
    let mut offen = Vec::new();
    for angekuendigt in [false, true] {
        let p = neuer_pfad()?;
        if angekuendigt {
            abgebrochen(&p, &zwei, &zeile, 0)?;
        } else {
            bestaetigt_geschrieben(&p, &zwei)?;
        }
        let ohne: Vec<u8> = zwei.iter().take(zwei.len().saturating_sub(1)).copied().collect();
        vorbedingung(fs::write(&p, &ohne), "Abschlusszeichen entfernen")?;
        if let Err(e) = halt(Journal::oeffnen(&p)) {
            offen.push(format!("angekuendigt {angekuendigt}: {e}"));
        }
        if vorbedingung(fs::read(&p), "lesen")? != ohne || !weitere_dateien(&p)?.is_empty() {
            offen.push(format!("angekuendigt {angekuendigt}: veraendert"));
        }
    }
    if offen.is_empty() { Ok(()) } else { Err(format!("{offen:#?}")) }
}

/// 5·B02 (2): Letzte bestaetigte Zeile teilweise gekuerzt: Halt, unveraendert.
#[test]
fn befund_5_b02_gekuerzte_bestaetigte_zeile_fuehrt_zu_halt() -> Result<(), String> {
    let (zwei, zeile) = zwei_und_geplant()?;
    let mut offen = Vec::new();
    for weg in [2_usize, 10, 40] {
        for angekuendigt in [false, true] {
            let p = neuer_pfad()?;
            if angekuendigt {
                abgebrochen(&p, &zwei, &zeile, 0)?;
            } else {
                bestaetigt_geschrieben(&p, &zwei)?;
            }
            let kurz: Vec<u8> = zwei.iter().take(zwei.len().saturating_sub(weg)).copied().collect();
            vorbedingung(fs::write(&p, &kurz), "kuerzen")?;
            if let Err(e) = halt(Journal::oeffnen(&p)) {
                offen.push(format!("{weg} Byte weg, angekuendigt {angekuendigt}: {e}"));
            }
            if vorbedingung(fs::read(&p), "lesen")? != kurz || !weitere_dateien(&p)?.is_empty() {
                offen.push(format!("{weg} Byte weg, angekuendigt {angekuendigt}: veraendert"));
            }
        }
    }
    if offen.is_empty() { Ok(()) } else { Err(format!("{offen:#?}")) }
}

/// 3·P4, 5·B02 (4): Absturz an jedem Schritt der Wiederherstellung. Nachgestellt werden die
/// Zwischenstaende: Quarantaene geschrieben, Journal noch lang; Journal gekuerzt, Vermerk noch
/// mit geplantem Anhang. Jeder fuehrt beim naechsten Start zum selben Ergebnis wie ein
/// Durchlauf ohne Absturz.
#[test]
fn befund_5_b02_absturz_in_der_wiederherstellung_endet_gleich() -> Result<(), String> {
    let (zwei, zeile) = zwei_und_geplant()?;
    let teil = zeile.len() / 2;
    // Durchlauf ohne Absturz als Massstab.
    let mass = neuer_pfad()?;
    abgebrochen(&mass, &zwei, &zeile, teil)?;
    let mass_j = oeffnen(&mass)?;
    let mass_eintraege = mass_j.eintraege().to_vec();
    let mass_stand = mass_j.bestaetigt();
    drop(mass_j);
    let q = weitere_dateien(&mass)?;
    let [qname] = q.iter().collect::<Vec<_>>()[..] else {
        return Err(format!("Vorbedingung: genau eine Quarantaene, nicht {q:?}"));
    };
    let qinhalt = vorbedingung(
        fs::read(mass.with_file_name(qname)),
        "Quarantaene lesen",
    )?;

    let mut offen = Vec::new();
    for fall in ["Quarantaene geschrieben", "Journal gekuerzt"] {
        let p = neuer_pfad()?;
        abgebrochen(&p, &zwei, &zeile, teil)?;
        vorbedingung(fs::write(p.with_file_name(qname), &qinhalt), "Quarantaene")?;
        if fall == "Journal gekuerzt" {
            vorbedingung(fs::write(&p, &zwei), "kuerzen")?;
        }
        match Journal::oeffnen(&p) {
            Ok(j) => {
                if j.eintraege() != mass_eintraege.as_slice() || j.bestaetigt() != mass_stand {
                    offen.push(format!("{fall}: anderer Stand"));
                }
            }
            Err(e) => offen.push(format!("{fall}: {e:?}")),
        }
        if vorbedingung(fs::read(&p), "lesen")? != zwei || weitere_dateien(&p)? != q {
            offen.push(format!("{fall}: andere Dateien"));
        }
        // Und ein dritter Start bleibt dabei.
        if oeffnen(&p).map(|j| j.eintraege().len()).ok() != Some(2) {
            offen.push(format!("{fall}: dritter Start anders"));
        }
    }
    if offen.is_empty() { Ok(()) } else { Err(format!("{offen:#?}")) }
}

/// 4·B02 (1): Beschaedigte letzte Zeile nach bestaetigtem Vorgang (falsche Pruefsumme, Laenge
/// und Abschlusszeichen stimmen): Halt, nichts wird zurueckgerollt.
#[test]
fn befund_4_b02_beschaedigte_letzte_zeile_fuehrt_zu_halt() -> Result<(), String> {
    let (zwei, zeile) = zwei_und_geplant()?;
    let mut offen = Vec::new();
    for angekuendigt in [false, true] {
        let p = neuer_pfad()?;
        if angekuendigt {
            abgebrochen(&p, &zwei, &zeile, 0)?;
        } else {
            bestaetigt_geschrieben(&p, &zwei)?;
        }
        let kaputt: Vec<u8> = String::from_utf8_lossy(&zwei)
            .replacen("verschoben", "verschobeN", 1)
            .into_bytes();
        if kaputt == zwei {
            return Err("Vorbedingung: Beschaedigung".into());
        }
        vorbedingung(fs::write(&p, &kaputt), "beschaedigen")?;
        if let Err(e) = halt(Journal::oeffnen(&p)) {
            offen.push(format!("angekuendigt {angekuendigt}: {e}"));
        }
        if vorbedingung(fs::read(&p), "lesen")? != kaputt || !weitere_dateien(&p)?.is_empty() {
            offen.push(format!("angekuendigt {angekuendigt}: veraendert"));
        }
    }
    if offen.is_empty() { Ok(()) } else { Err(format!("{offen:#?}")) }
}

/// 4·B02 (2): Absturz nach dem Schreiben, vor dem Bestaetigen: Der vollstaendige, unbestaetigte
/// Eintrag wird geprueft und bestaetigt. Ist er beschaedigt, gilt Halt. Ein Rest ueber den
/// vermerkten Anhang hinaus und ein Anhang ohne Vermerk fuehren zu Halt.
#[test]
fn befund_4_b02_nur_der_vermerkte_anhang_wird_behandelt() -> Result<(), String> {
    let (zwei, zeile) = zwei_und_geplant()?;
    let mut offen = Vec::new();

    let p = neuer_pfad()?;
    abgebrochen(&p, &zwei, &zeile, zeile.len())?;
    match Journal::oeffnen(&p) {
        Ok(j) if j.eintraege().len() == 3 => {}
        anders => offen.push(format!("vollstaendig: {:?}", anders.map(|j| j.eintraege().len()))),
    }
    if oeffnen(&p).map(|j| j.eintraege().len()).ok() != Some(3) {
        offen.push("vollstaendig: zweiter Start anders".into());
    }

    let p = neuer_pfad()?;
    let mut kaputt = zeile.clone();
    if let Some(b) = kaputt.iter_mut().rev().nth(2) {
        *b = if *b == b'g' { b'h' } else { b'g' };
    }
    abgebrochen(&p, &zwei, &kaputt, kaputt.len())?;
    if let Err(e) = halt(Journal::oeffnen(&p)) {
        offen.push(format!("vollstaendig, beschaedigt: {e}"));
    }

    let p = neuer_pfad()?;
    let mut lang = zeile.clone();
    lang.extend_from_slice(b"x");
    abgebrochen(&p, &zwei, &zeile, 0)?;
    let mut inhalt = zwei.clone();
    inhalt.extend_from_slice(&lang);
    vorbedingung(fs::write(&p, &inhalt), "zu lang")?;
    if let Err(e) = halt(Journal::oeffnen(&p)) {
        offen.push(format!("laenger als vermerkt: {e}"));
    }

    let p = neuer_pfad()?;
    bestaetigt_geschrieben(&p, &zwei)?;
    let mut inhalt = zwei.clone();
    inhalt.extend_from_slice(&zeile);
    vorbedingung(fs::write(&p, &inhalt), "Anhang ohne Vermerk")?;
    if let Err(e) = halt(Journal::oeffnen(&p)) {
        offen.push(format!("Anhang ohne Vermerk: {e}"));
    }
    let teil: Vec<u8> = zwei.iter().chain(zeile.iter().take(5)).copied().collect();
    vorbedingung(fs::write(&p, &teil), "Teilanhang ohne Vermerk")?;
    if let Err(e) = halt(Journal::oeffnen(&p)) {
        offen.push(format!("Teilanhang ohne Vermerk: {e}"));
    }
    if !weitere_dateien(&p)?.is_empty() {
        offen.push("ohne Vermerk: Quarantaene angelegt".into());
    }
    if offen.is_empty() { Ok(()) } else { Err(format!("{offen:#?}")) }
}

/// 4·B02 (3): Eine vorhandene, aber abweichende Quarantaenedatei verhindert das Abschneiden.
#[test]
fn befund_4_b02_abweichende_quarantaene_verhindert_das_kuerzen() -> Result<(), String> {
    let (zwei, zeile) = zwei_und_geplant()?;
    let teil = zeile.len() / 2;
    let mass = neuer_pfad()?;
    abgebrochen(&mass, &zwei, &zeile, teil)?;
    oeffnen(&mass)?;
    let q = weitere_dateien(&mass)?;
    let [qname] = q.iter().collect::<Vec<_>>()[..] else {
        return Err(format!("Vorbedingung: genau eine Quarantaene, nicht {q:?}"));
    };
    let p = neuer_pfad()?;
    abgebrochen(&p, &zwei, &zeile, teil)?;
    let vorher = vorbedingung(fs::read(&p), "lesen")?;
    vorbedingung(fs::write(p.with_file_name(qname), b"anderer Inhalt"), "Quarantaene")?;
    halt(Journal::oeffnen(&p))?;
    if vorbedingung(fs::read(&p), "lesen")? != vorher {
        return Err("trotz abweichender Quarantaene gekuerzt".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Papierdepot, Auftraege und Beobachtungen ueberstehen den Neustart; 1·F09 (R24)
// ---------------------------------------------------------------------------------------------

#[test]
fn papierdepot_und_beobachtungen_ueberstehen_den_neustart() -> Result<(), String> {
    let p = eingerichtet()?;
    let mut topf = vorbedingung(Topf::neu(TopfId::A, Cent(250_000)), "Topf")?;
    topf.setze_bestand(isin("DE0007164600")?, 12);
    topf.setze_bestand(isin("DE0008404005")?, 3);
    let leer = vorbedingung(Topf::neu(TopfId::B, Cent(0)), "Topf")?;
    let mut j = oeffnen(&p)?;
    vorbedingung(j.setze_topf(&topf), "Topf A")?;
    vorbedingung(j.setze_topf(&leer), "Topf B")?;
    vorbedingung(j.beobachte(tag(3)?, "EZB-Termin am 5.11.; Markt ruhig"), "Beobachtung")?;
    let vorher = j.stand().clone();
    drop(j);
    let j = oeffnen(&p)?;
    if *j.stand() != vorher {
        return Err(format!("nach dem Neustart: {:?} statt {vorher:?}", j.stand()));
    }
    if j.stand().toepfe().get(&TopfId::A) != Some(&topf)
        || j.stand().toepfe().get(&TopfId::B) != Some(&leer)
        || j.stand().beobachtungen().len() != 1
    {
        return Err(format!("Stand falsch: {:?}", j.stand()));
    }
    Ok(())
}

fn auftrag(nr: u64) -> String {
    format!("{nr};A;DE0007164600;Verkauf;5;9900;Notbremse")
}

/// 1·F09: Die Vergabe setzt nach einem Neustart hinter der letzten belegten Kennung fort. Das
/// Journal nimmt keine Kennung zweimal und keine kleinere an; eine solche Datei fuehrt zu Halt.
#[test]
fn befund_1_f09_kennungen_ueberstehen_den_neustart() -> Result<(), String> {
    let p = eingerichtet()?;
    let mut j = oeffnen(&p)?;
    if j.kennungsvergabe() != Kennungsvergabe::nach(0) {
        return Err("Vorbedingung: leeres Journal beginnt bei 0".into());
    }
    vorbedingung(j.anhaengen(Art::Auftrag, &auftrag(7)), "Auftrag 7")?;
    drop(j);
    let mut j = oeffnen(&p)?;
    if j.kennungsvergabe().letzte() != 7 || j.stand().auftraege().len() != 1 {
        return Err(format!("nach dem Neustart: letzte {}", j.kennungsvergabe().letzte()));
    }
    let vorher = vorbedingung(fs::read(&p), "lesen")?;
    let mut offen = Vec::new();
    for nr in [7, 3] {
        match j.anhaengen(Art::Auftrag, &auftrag(nr)) {
            Err(JournalFehler::Abgelehnt(_)) => {}
            anders => offen.push(format!("Kennung {nr}: {anders:?}")),
        }
    }
    if vorbedingung(fs::read(&p), "lesen")? != vorher {
        offen.push("abgelehnte Kennung geschrieben".into());
    }
    if j.anhaengen(Art::Auftrag, &auftrag(8)).is_err() {
        offen.push("naechste Kennung nach Ablehnung nicht angenommen".into());
    }
    drop(j);
    for (fall, nrn) in [("doppelt", [4, 4]), ("kleiner", [4, 2])] {
        let q = neuer_pfad()?;
        let a = auftrag(nrn[0]);
        let b = auftrag(nrn[1]);
        bestaetigt_geschrieben(&q, &inhalt_fuer(&[(Art::Auftrag, &a), (Art::Auftrag, &b)]))?;
        if let Err(e) = halt(Journal::oeffnen(&q)) {
            offen.push(format!("Datei {fall}: {e}"));
        }
    }
    if offen.is_empty() { Ok(()) } else { Err(format!("{offen:#?}")) }
}

/// Ein Auftrag aus dem Regelwerk wird mit seiner Kennung belegt; die Vergabe danach liegt
/// dahinter.
#[test]
fn genehmigter_auftrag_wird_belegt() -> Result<(), String> {
    let p = eingerichtet()?;
    let mut topf = vorbedingung(Topf::neu(TopfId::A, Cent(100_000)), "Topf")?;
    let wert = isin("DE0007164600")?;
    topf.setze_bestand(wert.clone(), 10);
    let tag2 = tag(2)?;
    let zeit = Zeitpunkt::neu(tag2, 10, 0, 0).ok_or("Vorbedingung: Zeit")?;
    let kurs = vorbedingung(Kurs::neu(Cent(10_000), "EUR", tag2, zeit, Herkunft::Simulation), "Kurs")?;
    let kurse = vorbedingung(Kurse::neu(zeit, [(wert, kurs)].into_iter().collect()), "Kurse")?;
    let mut j = oeffnen(&p)?;
    vorbedingung(j.anhaengen(Art::Auftrag, &auftrag(41)), "Auftrag 41")?;
    let mut vergabe = j.kennungsvergabe();
    let auftraege = notbremse_auftraege(&Grenzen::standard(), &topf, &kurse, &mut vergabe)
        .ok_or("Vorbedingung: Notbremse")?;
    let [a] = auftraege.as_slice() else {
        return Err("Vorbedingung: ein Auftrag".into());
    };
    vorbedingung(j.belege(TopfId::A, a), "belegen")?;
    if j.belege(TopfId::A, a).is_ok() {
        return Err("derselbe Auftrag zweimal belegt".into());
    }
    drop(j);
    let j = oeffnen(&p)?;
    match j.stand().auftraege().last() {
        Some(b) if b.auftrag == *a.auftrag() && b.anlass == a.beleg().anlass && b.topf == TopfId::A => {}
        anders => return Err(format!("belegt: {anders:?}")),
    }
    if j.kennungsvergabe().letzte() != 42 {
        return Err(format!("letzte Kennung {}", j.kennungsvergabe().letzte()));
    }
    Ok(())
}

/// Schreiber und Leser akzeptieren denselben Zustandsraum: Was das Journal beim Anhaengen
/// ablehnt, fuehrt in der Datei zu Halt. Eine Ablehnung schreibt nichts und sperrt nicht.
#[test]
fn unzulaessige_eintraege_werden_abgelehnt() -> Result<(), String> {
    let faelle: [(&str, Art, &str); 11] = [
        ("Zeilenumbruch", Art::Beobachtung, "2026-11-02;eins\nzwei"),
        ("Steuerzeichen", Art::Beobachtung, "2026-11-02;eins\rzwei"),
        ("leere Beobachtung", Art::Beobachtung, "2026-11-02;"),
        ("ungueltiges Datum", Art::Beobachtung, "2026-02-31;Text"),
        ("Datum nicht kanonisch", Art::Beobachtung, "2026-11-2;Text"),
        ("negatives Bargeld", Art::Topf, "A;-5;"),
        ("fuehrende Null", Art::Topf, "A;0100;"),
        ("ungueltige ISIN", Art::Topf, "A;100;DE0007164601:3"),
        ("Bestand null", Art::Topf, "A;100;DE0007164600:0"),
        ("Stueckzahl null", Art::Auftrag, "1;A;DE0007164600;Kauf;0;9900;Umschichtung"),
        ("unbekannter Anlass", Art::Auftrag, "1;A;DE0007164600;Kauf;5;9900;Laune"),
    ];
    let p = eingerichtet()?;
    let mut j = oeffnen(&p)?;
    let vorher = vorbedingung(fs::read(&p), "lesen")?;
    let mut offen = Vec::new();
    for (fall, art, inhalt) in faelle {
        match j.anhaengen(art, inhalt) {
            Err(JournalFehler::Abgelehnt(_)) => {}
            anders => offen.push(format!("{fall}: {anders:?}")),
        }
        if !inhalt.contains(['\n', '\r']) {
            let q = neuer_pfad()?;
            bestaetigt_geschrieben(&q, &inhalt_fuer(&[(art, inhalt)]))?;
            if let Err(e) = halt(Journal::oeffnen(&q)) {
                offen.push(format!("{fall} in der Datei: {e}"));
            }
        }
    }
    if vorbedingung(fs::read(&p), "lesen")? != vorher {
        offen.push("Abgelehntes geschrieben".into());
    }
    if j.beobachte(tag(2)?, "gueltig").is_err() {
        offen.push("nach Ablehnung gesperrt".into());
    }
    if offen.is_empty() { Ok(()) } else { Err(format!("{offen:#?}")) }
}

/// Der bestaetigte Stand (Laenge und Kettenhash) fuer den Anker ausserhalb des Rechners:
/// Er rueckt mit jedem Eintrag vor und ist nach dem Neustart derselbe.
#[test]
fn bestaetigter_stand_fuer_den_abendbericht() -> Result<(), String> {
    let p = eingerichtet()?;
    let mut j = oeffnen(&p)?;
    let leer = j.bestaetigt();
    vorbedingung(j.beobachte(tag(2)?, "Beobachtung"), "Beobachtung")?;
    let eins = j.bestaetigt();
    drop(j);
    let groesse = vorbedingung(fs::metadata(&p), "Groesse")?.len();
    if leer == eins || eins.0 != groesse || eins.1.len() != 64 || oeffnen(&p)?.bestaetigt() != eins {
        return Err(format!("{leer:?} -> {eins:?}, Datei {groesse} Byte"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// 1·K06, K12-01, K12-02 – Sperre und Links (R24); aus dem Kassenbuch uebernommen
// ---------------------------------------------------------------------------------------------

/// 1·K06: Ein zweiter Name fuer dieselbe Datei (harter Link) bekommt das Journal nicht.
#[test]
fn befund_1_k06_zweiter_pfad_auf_dieselbe_datei_wird_gesperrt() -> Result<(), String> {
    let p = eingerichtet()?;
    let zweit = p.with_file_name("zweitname.txt");
    vorbedingung(fs::hard_link(&p, &zweit), "harter Link")?;
    vorbedingung(fs::copy(vermerk(&p), vermerk(&zweit)), "Vermerk kopieren")?;
    let erster = oeffnen(&p)?;
    let ergebnis = Journal::oeffnen(&zweit);
    drop(erster);
    match ergebnis {
        Err(JournalFehler::Belegt) => Ok(()),
        anders => Err(format!("dieselbe Datei ueber einen zweiten Namen: {:?}", anders.map(|j| j.eintraege().len()))),
    }
}

/// 1·K06, K12-02: Ein Link im Pfad ist verboten, im letzten Teil, im Elternverzeichnis und als
/// Vermerk.
#[cfg(unix)]
#[test]
fn befund_k12_02_links_im_pfad_werden_abgelehnt() -> Result<(), String> {
    use std::os::unix::fs::symlink;
    let p = eingerichtet()?;
    let dir = p.parent().ok_or("Vorbedingung: Verzeichnis")?.to_path_buf();
    // Journal als Link, Vermerk daneben als echte Kopie.
    let link = dir.join("link.txt");
    vorbedingung(symlink(&p, &link), "Link")?;
    vorbedingung(fs::copy(vermerk(&p), vermerk(&link)), "Vermerk kopieren")?;
    // Vermerk als Link auf eine echte Kopie, Journal daneben echt.
    let zweit = dir.join("zweit.txt");
    vorbedingung(fs::copy(&p, &zweit), "Journal kopieren")?;
    let kopie = dir.join("vermerk-kopie");
    vorbedingung(fs::copy(vermerk(&p), &kopie), "Vermerk kopieren")?;
    vorbedingung(symlink(&kopie, vermerk(&zweit)), "Link")?;
    // Elternverzeichnis als Link.
    let echt = dir.join("echt");
    vorbedingung(fs::create_dir(&echt), "Verzeichnis")?;
    vorbedingung(Journal::einrichten(&echt.join("journal.txt")), "Einrichtung")?;
    let eltern = dir.join("eltern");
    vorbedingung(symlink(&echt, &eltern), "Link")?;
    let mut offen = Vec::new();
    for (fall, pfad) in [
        ("Journal als Link", link),
        ("Vermerk als Link", zweit),
        ("Elternverzeichnis als Link", eltern.join("journal.txt")),
    ] {
        match Journal::oeffnen(&pfad) {
            Err(JournalFehler::Halt(m)) if m.contains("Link") => {}
            anders => offen.push(format!("{fall}: {:?}", anders.map(|j| j.eintraege().len()))),
        }
    }
    if Journal::einrichten(&eltern.join("neu.txt")).is_ok() {
        offen.push("Einrichtung ueber Elternlink".into());
    }
    if offen.is_empty() { Ok(()) } else { Err(format!("{offen:#?}")) }
}

/// K12-01: Eine liegen gebliebene Zwischendatei des Vermerks, die ein Link auf eine fremde
/// Datei ist, veraendert diese Datei nie. Gilt fuer Einrichtung, Anhaengen und Wiederherstellung.
#[cfg(unix)]
#[test]
fn befund_k12_01_zwischendatei_veraendert_keine_fremde_datei() -> Result<(), String> {
    let vorher = b"FREMDE_DATEI_UNVERAENDERT\n";
    let (zwei, zeile) = zwei_und_geplant()?;
    let mut offen = Vec::new();
    for (fall, hart) in [("symbolischer Link", false), ("harter Link", true)] {
        for schritt in ["einrichten", "anhaengen", "wiederherstellen"] {
            let p = neuer_pfad()?;
            let fremd = p.with_file_name("fremd.txt");
            vorbedingung(fs::write(&fremd, vorher), "fremde Datei")?;
            let tmp = p.with_extension("vermerk.tmp");
            let lege = || -> Result<(), String> {
                if hart {
                    vorbedingung(fs::hard_link(&fremd, &tmp), "harter Link")
                } else {
                    vorbedingung(std::os::unix::fs::symlink(&fremd, &tmp), "Link")
                }
            };
            match schritt {
                "einrichten" => {
                    lege()?;
                    let _ = Journal::einrichten(&p);
                }
                "anhaengen" => {
                    vorbedingung(Journal::einrichten(&p), "Einrichtung")?;
                    let mut j = oeffnen(&p)?;
                    lege()?;
                    let _ = j.beobachte(tag(2)?, "Beobachtung");
                }
                _ => {
                    abgebrochen(&p, &zwei, &zeile, 3)?;
                    lege()?;
                    let _ = Journal::oeffnen(&p);
                }
            }
            if vorbedingung(fs::read(&fremd), "fremde Datei lesen")? != vorher {
                offen.push(format!("{fall}, {schritt}: fremde Datei veraendert"));
            }
        }
    }
    if offen.is_empty() { Ok(()) } else { Err(format!("{offen:#?}")) }
}

// ---------------------------------------------------------------------------------------------
// 1·R02 – Weiterschreiben nach Fehlern (R29)
// ---------------------------------------------------------------------------------------------

/// An der Stelle der Zwischendatei des Vermerks liegt kurz ein Verzeichnis: Das Anhaengen
/// scheitert, bevor etwas im Journal steht. Danach bleibt das Journal gesperrt: kein Eintrag,
/// auch nicht, wenn das Hindernis weg ist. Nach dem Neustart arbeitet es wieder.
#[test]
fn befund_1_r02_vermerkfehler_sperrt_das_journal() -> Result<(), String> {
    let p = eingerichtet()?;
    let mut j = oeffnen(&p)?;
    let tmp = p.with_extension("vermerk.tmp");
    vorbedingung(fs::create_dir(&tmp), "Verzeichnis an Stelle der Zwischendatei")?;
    if j.beobachte(tag(2)?, "erste").is_ok() {
        return Err("Vorbedingung: Fehler tritt nicht ein".into());
    }
    vorbedingung(fs::remove_dir(&tmp), "Verzeichnis entfernen")?;
    let vorher = vorbedingung(fs::read(&p), "lesen")?;
    let zweiter = j.beobachte(tag(2)?, "zweite");
    let nachher = vorbedingung(fs::read(&p), "lesen")?;
    if zweiter.is_ok() || vorher != nachher || !j.eintraege().is_empty() {
        return Err(format!("nach dem Fehler weitergeschrieben: {zweiter:?}"));
    }
    drop(j);
    let mut j = oeffnen(&p)?;
    if !j.eintraege().is_empty() || j.beobachte(tag(2)?, "dritte").is_err() {
        return Err("nach dem Neustart falscher Stand".into());
    }
    Ok(())
}
