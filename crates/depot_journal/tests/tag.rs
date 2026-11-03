//! Begruendungen und Tagesabschluss im Journal (simulierter Lauf). Erfundene Daten (D12).
//!
//! Jede Vorbereitung prueft ihren Erfolg ausdruecklich und endet sonst mit „Vorbedingung …“.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{Art, Journal, JournalFehler, inhalt_fuer, vermerk_fuer};
use depot_kern::{Cent, Datum};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

struct Ort {
    dir: PathBuf,
    journal: PathBuf,
}

impl Ort {
    fn neu() -> Result<Ort, String> {
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("depot_tag_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
        Ok(Ort {
            journal: dir.join("journal.txt"),
            dir,
        })
    }

    fn eingerichtet() -> Result<Ort, String> {
        let o = Ort::neu()?;
        vorbedingung(Journal::einrichten(&o.journal), "Einrichtung")?;
        Ok(o)
    }

    /// Schreibt ein Journal mit genau diesen Eintraegen samt passendem Vermerk.
    fn mit(eintraege: &[(Art, &str)]) -> Result<Ort, String> {
        let o = Ort::neu()?;
        let inhalt = inhalt_fuer(eintraege);
        let vermerk = vermerk_fuer(&inhalt, 0).ok_or("Vorbedingung: Vermerk")?;
        vorbedingung(fs::write(&o.journal, &inhalt), "Journal schreiben")?;
        vorbedingung(
            fs::write(o.journal.with_extension("vermerk"), vermerk),
            "Vermerk schreiben",
        )?;
        Ok(o)
    }

    fn oeffnen(&self) -> Result<Journal, String> {
        vorbedingung(Journal::oeffnen(&self.journal), "Journal oeffnen")
    }
}

impl Drop for Ort {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn tag(t: u8) -> Result<Datum, String> {
    Datum::neu(2026, 11, t).ok_or_else(|| format!("Vorbedingung: Datum {t}.11."))
}

fn abgelehnt<T: std::fmt::Debug>(r: &Result<T, JournalFehler>, fall: &str) {
    assert!(
        matches!(r, Err(JournalFehler::Abgelehnt(_))),
        "{fall}: {r:?}"
    );
}

#[test]
fn begruendung_wird_belegt_und_nachgerechnet() -> Result<(), String> {
    let o = Ort::eingerichtet()?;
    {
        let mut j = o.oeffnen()?;
        vorbedingung(
            j.begruende(tag(25)?, "Dachs", "breit; klein"),
            "Begruendung",
        )?;
    }
    let j = o.oeffnen()?;
    assert_eq!(
        j.stand().begruendungen(),
        &[(tag(25)?, "Dachs".to_string(), "breit; klein".to_string())]
    );
    assert_eq!(
        j.eintraege(),
        &[(
            Art::Begruendung,
            "2026-11-25;Dachs;breit; klein".to_string()
        )]
    );
    Ok(())
}

#[test]
fn unzulaessige_begruendung_wird_abgelehnt() -> Result<(), String> {
    let o = Ort::eingerichtet()?;
    let mut j = o.oeffnen()?;
    abgelehnt(&j.begruende(tag(25)?, "", "Text"), "wer leer");
    abgelehnt(&j.begruende(tag(25)?, "Da;chs", "Text"), "wer mit Trenner");
    abgelehnt(&j.begruende(tag(25)?, "Dachs", " "), "Text leer");
    abgelehnt(&j.begruende(tag(25)?, "Dachs", "a\nb"), "Steuerzeichen");
    assert!(j.eintraege().is_empty());
    Ok(())
}

#[test]
fn abschluss_nur_vorwaerts_und_mit_positivem_hoechststand() -> Result<(), String> {
    let o = Ort::eingerichtet()?;
    let mut j = o.oeffnen()?;
    assert_eq!(j.stand().abschluss(), None);
    abgelehnt(&j.schliesse_ab(tag(25)?, Cent(0)), "Hoechststand null");
    vorbedingung(
        j.schliesse_ab(tag(25)?, Cent(2_000_000)),
        "erster Abschluss",
    )?;
    abgelehnt(&j.schliesse_ab(tag(25)?, Cent(2_000_000)), "derselbe Tag");
    abgelehnt(&j.schliesse_ab(tag(24)?, Cent(2_000_000)), "frueherer Tag");
    vorbedingung(
        j.schliesse_ab(tag(26)?, Cent(2_100_000)),
        "zweiter Abschluss",
    )?;
    assert_eq!(j.stand().abschluss(), Some((tag(26)?, Cent(2_100_000))));
    Ok(())
}

#[test]
fn eintraege_seit_dem_abschluss_werden_gezaehlt() -> Result<(), String> {
    let o = Ort::eingerichtet()?;
    {
        let mut j = o.oeffnen()?;
        assert_eq!(j.stand().seit_abschluss(), 0);
        vorbedingung(j.beobachte(tag(25)?, "Termin"), "Beobachtung")?;
        vorbedingung(j.begruende(tag(25)?, "Maus", "warten"), "Begruendung")?;
        assert_eq!(j.stand().seit_abschluss(), 2);
        vorbedingung(j.schliesse_ab(tag(25)?, Cent(1)), "Abschluss")?;
        assert_eq!(j.stand().seit_abschluss(), 0);
        vorbedingung(
            j.reserviere(depot_kern::BeraterId::Dachs, tag(26)?, 2),
            "KI",
        )?;
    }
    // Auch KI-Buchungen zaehlen; nachgerechnet ergibt sich derselbe Stand.
    let j = o.oeffnen()?;
    assert_eq!(j.stand().seit_abschluss(), 2, "Grenzabdruck und Buchung");
    Ok(())
}

#[test]
fn nicht_kanonische_eintraege_halten_beim_oeffnen() -> Result<(), String> {
    let gut = Ort::mit(&[
        (Art::Begruendung, "2026-11-25;Dachs;Text"),
        (Art::Abschluss, "2026-11-25;100"),
    ])?;
    gut.oeffnen()?;
    for (fall, eintraege) in [
        (
            "Datum kurz",
            vec![(Art::Begruendung, "2026-11-5;Dachs;Text")],
        ),
        ("wer leer", vec![(Art::Begruendung, "2026-11-25;;Text")]),
        ("Text fehlt", vec![(Art::Begruendung, "2026-11-25;Dachs")]),
        ("fuehrende Null", vec![(Art::Abschluss, "2026-11-25;0100")]),
        ("Abschluss null", vec![(Art::Abschluss, "2026-11-25;0")]),
        (
            "Abschluss rueckwaerts",
            vec![
                (Art::Abschluss, "2026-11-26;100"),
                (Art::Abschluss, "2026-11-25;100"),
            ],
        ),
    ] {
        let o = Ort::mit(&eintraege)?;
        let r = Journal::oeffnen(&o.journal);
        assert!(matches!(r, Err(JournalFehler::Halt(_))), "{fall}: {r:?}");
    }
    Ok(())
}
