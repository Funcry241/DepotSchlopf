//! Formatuebergang: Ein abgeschlossenes Journal aus PR #15, also ohne Auftragszustaende,
//! bleibt lesbar und laesst sich fortsetzen (K16-02). Erfundene Daten (D12).
//!
//! Jede Vorbereitung prueft ihren Erfolg ausdruecklich und endet sonst mit „Vorbedingung …“.

mod alt;

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{AuftragsZustand, Journal};
use depot_kern::{Datum, Topf};
use depot_sim::{Papierkonto, Szenario, lauf};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

/// Legt das alte Journal samt Vermerk in ein eigenes Verzeichnis.
fn altes_journal() -> Result<PathBuf, String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_format_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
    let p = dir.join("journal.txt");
    vorbedingung(fs::write(&p, alt::JOURNAL), "Journal")?;
    vorbedingung(
        fs::write(p.with_extension("vermerk"), alt::VERMERK),
        "Vermerk",
    )?;
    Ok(p)
}

fn datum(t: u8, m: u8) -> Result<Datum, String> {
    Datum::neu(2026, m, t).ok_or_else(|| format!("Vorbedingung: Datum {t}.{m}."))
}

/// K16-02 (Maus, PR #16): Ein mit PR #15 abgeschlossenes Journal laesst sich oeffnen. Seine
/// Auftraege sind damals ausgefuehrt und im Topf gebucht; sie binden nichts und sind nicht
/// offen. Der Stand ist derselbe wie damals.
#[test]
fn befund_k16_02_altes_journal_laesst_sich_oeffnen() -> Result<(), String> {
    let p = altes_journal()?;
    let j = Journal::oeffnen(&p).map_err(|e| format!("altes Journal: {e:?}"))?;
    let st = j.stand();
    let mut offen = Vec::new();
    if st.abschluss().map(|a| a.datum) != Some(datum(25, 11)?) {
        offen.push(format!("Abschluss: {:?}", st.abschluss()));
    }
    if st.auftraege().len() != 3 || st.offene_auftraege().next().is_some() {
        offen.push(format!("Auftraege: {:?}", st.auftraege()));
    }
    if st.letzte_kennung() != 3 || st.seit_abschluss() != 0 {
        offen.push(format!(
            "Kennung {} seit Abschluss {}",
            st.letzte_kennung(),
            st.seit_abschluss()
        ));
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

/// K16-02: Der Lauf setzt das alte Journal fort. Das Papierkonto entsteht aus seinem Stand,
/// denn ein Konto gab es damals nicht. Neue Auftraege durchlaufen die Zustandsmaschine.
#[test]
fn befund_k16_02_altes_journal_wird_fortgesetzt() -> Result<(), String> {
    let p = altes_journal()?;
    let (s, mut dachs, mut maus) =
        Szenario::beispiel().ok_or_else(|| "Vorbedingung: Beispiel".to_string())?;
    let mut j = Journal::oeffnen(&p).map_err(|e| format!("altes Journal: {e:?}"))?;
    let toepfe: Vec<Topf> = j.stand().toepfe().values().cloned().collect();
    let mut konto = Papierkonto::eroeffne(&toepfe).ok_or("Vorbedingung: Konto")?;
    let alt = j.stand().auftraege().len();
    lauf(&mut j, &s, &mut dachs, &mut maus, &mut konto).map_err(|e| format!("Lauf: {e:?}"))?;
    let st = j.stand();
    let mut offen = Vec::new();
    if st.abschluss().map(|a| a.datum) != Some(datum(1, 12)?) {
        offen.push(format!("Abschluss: {:?}", st.abschluss()));
    }
    let neu: Vec<_> = st.auftraege().iter().skip(alt).collect();
    if neu.is_empty()
        || !neu.iter().all(|l| {
            matches!(
                l.zustand,
                AuftragsZustand::Gefuellt | AuftragsZustand::Verfallen
            )
        })
    {
        offen.push(format!("neue Auftraege: {neu:?}"));
    }
    if konto.sendungen() != neu.len() {
        offen.push(format!("{} Sendungen", konto.sendungen()));
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

/// Die Regel des Formatuebergangs (K16-02): Ein Journal im Format 1 nimmt weder Auftraege
/// noch Zustaende an, bis es umgestellt ist. Umgestellt wird nur ohne angefangenen Tag, nur
/// einmal und nur ueber `stelle_um`. Danach bleiben die alten Auftraege im Endzustand `Alt`,
/// neue durchlaufen die Zustandsmaschine. Neue Journale haben gleich Format 2.
#[test]
fn befund_k16_02_umstellung_nur_abgeschlossen_und_einmal() -> Result<(), String> {
    use depot_journal::{Art, FORMAT, FORMAT_ALT, JournalFehler, Meldung};
    let mut offen = Vec::new();
    let abgelehnt = |r: Result<(), JournalFehler>| matches!(r, Err(JournalFehler::Abgelehnt(_)));

    // Ohne Umstellung: kein neuer Zustand, keine Umstellung ueber anhaengen.
    let p = altes_journal()?;
    let mut j = Journal::oeffnen(&p).map_err(|e| format!("altes Journal: {e:?}"))?;
    if j.stand().format() != FORMAT_ALT {
        offen.push(format!("altes Format erkannt: {}", j.stand().format()));
    }
    // Ein alter Auftrag hat keinen Schritt mehr.
    if !abgelehnt(j.melde(1, &Meldung::Gesendet)) {
        offen.push("Zustand fuer alten Auftrag angenommen".into());
    }
    // Ein neuer Auftrag braucht erst die Umstellung.
    let topf_a = j
        .stand()
        .toepfe()
        .get(&depot_kern::TopfId::A)
        .cloned()
        .ok_or("Vorbedingung: Topf A")?;
    let (szenario, _, _) =
        Szenario::beispiel().ok_or_else(|| "Vorbedingung: Beispiel".to_string())?;
    let kurse = &szenario.tage.first().ok_or("Vorbedingung: Tag")?.kurse;
    let grenzen = depot_kern::Grenzen::standard();
    let mut vergabe = j.kennungsvergabe();
    let verkauf = depot_regelwerk::notbremse_auftraege(&grenzen, &topf_a, kurse, &mut vergabe)
        .and_then(|a| a.into_iter().next())
        .ok_or("Vorbedingung: Verkauf")?;
    if !abgelehnt(j.reserviere_auftrag(&grenzen, depot_kern::TopfId::A, &verkauf)) {
        offen.push("neuer Auftrag im Format 1 angenommen".into());
    }
    if !abgelehnt(j.anhaengen(Art::Format, "2")) {
        offen.push("Umstellung ueber anhaengen angenommen".into());
    }
    // Ein angefangener Tag laesst sich nicht umstellen.
    vorbedingung(j.beobachte(datum(26, 11)?, "angefangen"), "Beobachtung")?;
    if !abgelehnt(j.stelle_um()) {
        offen.push("Umstellung mit angefangenem Tag angenommen".into());
    }
    drop(j);

    // Abgeschlossen: Umstellung, danach Format 2, alte Auftraege im Endzustand.
    let p = altes_journal()?;
    {
        let mut j = Journal::oeffnen(&p).map_err(|e| format!("altes Journal: {e:?}"))?;
        vorbedingung(j.stelle_um(), "Umstellung")?;
        vorbedingung(j.stelle_um(), "zweiter Aufruf tut nichts")?;
    }
    let j = Journal::oeffnen(&p).map_err(|e| format!("umgestellt: {e:?}"))?;
    let st = j.stand();
    if st.format() != FORMAT
        || st.seit_abschluss() != 0
        || !st
            .auftraege()
            .iter()
            .all(|l| l.zustand == AuftragsZustand::Alt)
    {
        offen.push(format!("nach der Umstellung: {st:?}"));
    }
    let umstellungen = j
        .eintraege()
        .iter()
        .filter(|(a, _)| *a == Art::Format)
        .count();
    if umstellungen != 1 {
        offen.push(format!("{umstellungen} Umstellungen"));
    }
    drop(j);

    // In der Datei: eine Umstellung im Format 2 ist Halt.
    let dir = std::env::temp_dir().join(format!(
        "depot_format_neu_{}_{}",
        std::process::id(),
        ZAEHLER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    vorbedingung(fs::create_dir_all(&dir), "Verzeichnis")?;
    let datei = dir.join("journal.txt");
    let inhalt = depot_journal::inhalt_fuer(&[(Art::Format, "2")]);
    vorbedingung(fs::write(&datei, &inhalt), "Datei")?;
    let v = depot_journal::vermerk_fuer(&inhalt, 0).ok_or("Vorbedingung: Vermerk")?;
    vorbedingung(fs::write(datei.with_extension("vermerk"), v), "Vermerk")?;
    if !matches!(Journal::oeffnen(&datei), Err(JournalFehler::Halt(_))) {
        offen.push("Umstellung im Format 2 in der Datei angenommen".into());
    }

    // Ein neues Journal hat Format 2.
    let neu = dir.join("neu.txt");
    vorbedingung(Journal::einrichten(&neu), "Einrichtung")?;
    let j = Journal::oeffnen(&neu).map_err(|e| format!("neu: {e:?}"))?;
    if j.stand().format() != FORMAT {
        offen.push(format!("neues Journal im Format {}", j.stand().format()));
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}
