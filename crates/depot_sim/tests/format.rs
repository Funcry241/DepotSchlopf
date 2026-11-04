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
