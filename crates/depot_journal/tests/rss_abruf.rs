//! Synthetische Regression für alte und neue RSS-Verwaltungsbelege; keine Nutzerdaten.
use depot_journal::{Art, Journal, inhalt_fuer, vermerk_fuer};
use depot_kern::{Datum, Zeitpunkt};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};
static NR: AtomicU32 = AtomicU32::new(0);
fn ort() -> Result<PathBuf, String> {
    let p = std::env::temp_dir().join(format!(
        "depot_rss_j_{}_{}",
        std::process::id(),
        NR.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p.join("journal.txt"))
}
fn zeit() -> Result<Zeitpunkt, String> {
    Zeitpunkt::neu(Datum::neu(2026, 11, 8).ok_or("Datum")?, 7, 0, 0).ok_or("Zeit".into())
}
#[test]
fn alte_rss_vermerke_werden_ohne_dateiumschreiben_als_verwaltung_gelesen() -> Result<(), String> {
    let p = ort()?;
    let t = zeit()?;
    let tag = t.datum().ok_or("Datum")?;
    let alpha = format!("{tag};rss-abruf: tagesschau;{}", t.sekunden());
    let bbc = format!("{tag};rss-abruf: bbc-world;{}", t.sekunden());
    let bytes = inhalt_fuer(&[(Art::Beobachtung, &alpha), (Art::Beobachtung, &bbc)]);
    let vermerk = vermerk_fuer(&bytes, 0).ok_or("Vermerk")?;
    fs::write(&p, &bytes).map_err(|e| e.to_string())?;
    fs::write(p.with_extension("vermerk"), &vermerk).map_err(|e| e.to_string())?;
    let j = Journal::oeffnen(&p).map_err(|e| format!("{e:?}"))?;
    assert_eq!(j.stand().seit_abschluss(), 0);
    assert_eq!(j.stand().rss_abruf("tagesschau"), Some(t));
    assert_eq!(j.stand().rss_abruf("bbc-world"), Some(t));
    assert!(j.stand().beobachtungen().is_empty());
    assert_eq!(j.eintraege().len(), 2);
    drop(j);
    assert_eq!(fs::read(&p).map_err(|e| e.to_string())?, bytes);
    assert_eq!(
        fs::read_to_string(p.with_extension("vermerk")).map_err(|e| e.to_string())?,
        vermerk
    );
    Ok(())
}
#[test]
fn typisierte_rss_abrufe_bleiben_beim_neustart_ohne_handelszustand() -> Result<(), String> {
    let p = ort()?;
    Journal::einrichten(&p).map_err(|e| format!("{e:?}"))?;
    let t = zeit()?;
    {
        let mut j = Journal::oeffnen(&p).map_err(|e| format!("{e:?}"))?;
        j.belege_rss_abruf("tagesschau", t)
            .map_err(|e| format!("{e:?}"))?;
        j.belege_rss_abruf("fruehere-quelle", t)
            .map_err(|e| format!("{e:?}"))?;
        assert!(j.belege_rss_abruf("Ungültige Quelle", t).is_err());
        for falsch in [
            "tagesschau;+1794121200",
            "tagesschau;01794121200",
            "tagesschau;falsch",
            "BBC-WORLD;1794121200",
        ] {
            assert!(j.anhaengen(Art::RssAbruf, falsch).is_err());
        }
    }
    let j = Journal::oeffnen(&p).map_err(|e| format!("{e:?}"))?;
    assert_eq!(j.stand().seit_abschluss(), 0);
    assert_eq!(j.stand().rss_abruf("tagesschau"), Some(t));
    assert_eq!(j.stand().rss_abruf("fruehere-quelle"), Some(t));
    assert!(j.stand().beobachtungen().is_empty());
    assert!(j.stand().toepfe().is_empty() && j.stand().auftraege().is_empty());
    Ok(())
}
#[test]
fn echte_beobachtung_oder_nur_aehnlicher_text_beginnt_weiterhin_einen_tag() -> Result<(), String> {
    let t = zeit()?;
    let tag = t.datum().ok_or("Datum")?;
    for falsch in [
        "Handelsbeobachtung".to_string(),
        format!("rss-abruf: unbekannt;{}", t.sekunden()),
        format!("rss-abruf: tagesschau;0{}", t.sekunden()),
        format!(
            "rss-abruf: tagesschau;{}",
            t.sekunden().saturating_sub(86_400)
        ),
    ] {
        let p = ort()?;
        Journal::einrichten(&p).map_err(|e| format!("{e:?}"))?;
        let mut j = Journal::oeffnen(&p).map_err(|e| format!("{e:?}"))?;
        j.beobachte(tag, &falsch).map_err(|e| format!("{e:?}"))?;
        j.belege_rss_abruf("tagesschau", t)
            .map_err(|e| format!("{e:?}"))?;
        assert_eq!(j.stand().seit_abschluss(), 1);
        assert!(
            j.starte_vorbereitung(t, tag.plus_tage(1).ok_or("Folgetag")?, vec!["Beleg".into()])
                .is_err()
        );
    }
    Ok(())
}
