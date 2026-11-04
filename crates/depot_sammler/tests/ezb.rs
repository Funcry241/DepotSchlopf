//! Termine der EZB von ihrer eigenen Seite (R39). Die Seite `SEITE` ist der Inhalt von `main`
//! der echten Seite, abgerufen am 04.11.2026; nur Kopf und Fuss der Seite sind weggelassen.
//!
//! Der Lauf des Sammlers ueber die Termine bekommt diese Seite von einer erfundenen Quelle statt
//! aus dem Netz. Jede Vorbereitung prueft ihren Erfolg ausdruecklich und endet sonst mit
//! „Vorbedingung …“.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{Journal, JournalFehler};
use depot_kern::{
    Betroffen, Datum, Herkunft, TerminFehler, Verlaesslichkeit, Zeitbezug, Zeitpunkt,
};
use depot_sammler::ezb::{ADRESSE_EZB, EzbFehler, QUELLE_EZB, lies_ezb_termine};
use depot_sammler::{Netz, SammelFehler, TerminBericht, sammle_termine};

const SEITE: &str = r#"<!DOCTYPE html>
<html lang="en"><head><title>ECB</title></head><body>
<main >
			<div class="title">
<h1>Schedules for the meetings of the Governing Council and General Council of the ECB and related press conferences</h1>
</div>
<div class="definition-list -zebra">
<dl>
<dt> 	
25/11/2026
</dt>
<dd>        
Governing Council of the ECB: non-monetary policy meeting (in Frankfurt)<br>
</dd>	        	
<dt> 	
26/11/2026
</dt>
<dd>        
General Council meeting of the ECB in Frankfurt<br>
</dd>	        	
<dt> 	
16/12/2026
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 1)<br>
</dd>	        	
<dt> 	
17/12/2026
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 2), followed by press conference<br>
</dd>	        	
<dt> 	
03/02/2027
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 1)<br>
</dd>	        	
<dt> 	
04/02/2027
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 2), followed by press conference<br>
</dd>	        	
<dt> 	
24/02/2027
</dt>
<dd>        
Governing Council of the ECB: non-monetary policy meeting (virtual)<br>
</dd>	        	
<dt> 	
17/03/2027
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 1)<br>
</dd>	        	
<dt> 	
18/03/2027
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 2), followed by press conference<br>
</dd>	        	
<dt> 	
07/04/2027
</dt>
<dd>        
Governing Council of the ECB: non-monetary policy meeting (virtual)<br>
</dd>	        	
<dt> 	
28/04/2027
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 1)<br>
</dd>	        	
<dt> 	
29/04/2027
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 2), followed by press conference<br>
</dd>	        	
<dt> 	
19/05/2027
</dt>
<dd>        
Governing Council of the ECB: non-monetary policy meeting (virtual)<br>
</dd>	        	
<dt> 	
09/06/2027
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 1)<br>
</dd>	        	
<dt> 	
10/06/2027
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 2), followed by press conference<br>
</dd>	        	
<dt> 	
23/06/2027
</dt>
<dd>        
Governing Council of the ECB: non-monetary policy meeting (virtual)<br>
</dd>	        	
<dt> 	
24/06/2027
</dt>
<dd>        
General Council meeting of the ECB (virtual)<br>
</dd>	        	
<dt> 	
21/07/2027
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 1)<br>
</dd>	        	
<dt> 	
22/07/2027
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 2), followed by press conference<br>
</dd>	        	
<dt> 	
08/09/2027
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 1)<br>
</dd>	        	
<dt> 	
09/09/2027
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 2), followed by press conference<br>
</dd>	        	
<dt> 	
29/09/2027
</dt>
<dd>        
Governing Council of the ECB: non-monetary policy meeting (virtual)<br>
</dd>	        	
<dt> 	
27/10/2027
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 1)<br>
</dd>	        	
<dt> 	
28/10/2027
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 2), followed by press conference<br>
</dd>	        	
<dt> 	
24/11/2027
</dt>
<dd>        
Governing Council of the ECB: non-monetary policy meeting in Frankfurt<br>
</dd>	        	
<dt> 	
25/11/2027
</dt>
<dd>        
General Council meeting of the ECB in Frankfurt<br>
</dd>	        	
<dt> 	
15/12/2027
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 1)<br>
</dd>	        	
<dt> 	
16/12/2027
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 2), followed by press conference<br>
</dd>	        	
<dt> 	
02/02/2028
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 1)<br>
</dd>	        	
<dt> 	
03/02/2028
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 2), followed by press conference<br>
</dd>	        	
<dt> 	
23/02/2028
</dt>
<dd>        
Governing Council of the ECB: non-monetary policy meeting (virtual)<br>
</dd>	        	
<dt> 	
22/03/2028
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 1)<br>
</dd>	        	
<dt> 	
23/03/2028
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 2), followed by press conference<br>
</dd>	        	
<dt> 	
05/04/2028
</dt>
<dd>        
Governing Council of the ECB: non-monetary policy meeting (virtual)<br>
</dd>	        	
<dt> 	
03/05/2028
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 1)<br>
</dd>	        	
<dt> 	
04/05/2028
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 2), followed by press conference<br>
</dd>	        	
<dt> 	
24/05/2028
</dt>
<dd>        
Governing Council of the ECB: non-monetary policy meeting (virtual)<br>
</dd>	        	
<dt> 	
07/06/2028
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 1)<br>
</dd>	        	
<dt> 	
08/06/2028
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 2), followed by press conference<br>
</dd>	        	
<dt> 	
21/06/2028
</dt>
<dd>        
Governing Council of the ECB: non-monetary policy meeting (virtual)<br>
</dd>	        	
<dt> 	
22/06/2028
</dt>
<dd>        
General Council meeting of the ECB (virtual)<br>
</dd>	        	
<dt> 	
19/07/2028
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 1)<br>
</dd>	        	
<dt> 	
20/07/2028
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 2), followed by press conference<br>
</dd>	        	
<dt> 	
06/09/2028
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 1)<br>
</dd>	        	
<dt> 	
07/09/2028
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 2), followed by press conference<br>
</dd>	        	
<dt> 	
27/09/2028
</dt>
<dd>        
Governing Council of the ECB: non-monetary policy meeting (virtual)<br>
</dd>	        	
<dt> 	
11/10/2028
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt<br>
</dd>	        	
<dt> 	
12/10/2028
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 2), followed by press conference<br>
</dd>	        	
<dt> 	
22/11/2028
</dt>
<dd>        
Governing Council of the ECB: non-monetary policy meeting in Frankfurt<br>
</dd>	        	
<dt> 	
23/11/2028
</dt>
<dd>        
General Council meeting of the ECB in Frankfurt<br>
</dd>	        	
<dt> 	
06/12/2028
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 1)<br>
</dd>	        	
<dt> 	
07/12/2028
</dt>
<dd>        
Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 2), followed by press conference<br>
</dd>	        	
</dl>	        	                
</div>

			
			<div class="in-this-section" id="pages-in-section-holder">
    <h3>All pages in this section</h3>
    <div class="content-box">
        
    </div>
</div>
		</main>
</body></html>
"#;

fn tag(j: u16, m: u8, t: u8) -> Result<Datum, String> {
    Datum::neu(j, m, t).ok_or_else(|| format!("Vorbedingung: Datum {j}-{m}-{t}"))
}

/// Die echte Seite: 52 Termine vom 25.11.2026 bis 07.12.2028, alle bestaetigt, ohne Uhrzeit.
#[test]
fn echte_seite() -> Result<(), String> {
    let t = lies_ezb_termine(SEITE).map_err(|e| format!("{e:?}"))?;
    assert_eq!(t.len(), 52);
    let erste = t.first().ok_or("leer")?;
    assert_eq!(erste.kennung(), "ezb-2026-11-25-1");
    assert_eq!(
        erste.titel(),
        "Governing Council of the ECB: non-monetary policy meeting (in Frankfurt)"
    );
    assert_eq!(erste.zeit(), Zeitbezug::Tag(tag(2026, 11, 25)?));
    assert_eq!(erste.betroffen(), [Betroffen::groesse("ezb").ok_or("ezb")?]);
    let zins = t
        .iter()
        .find(|x| x.kennung() == "ezb-2026-12-17-1")
        .ok_or("17.12.2026 fehlt")?;
    assert_eq!(
        zins.titel(),
        "Governing Council of the ECB: monetary policy meeting in Frankfurt (Day 2), followed by press conference"
    );
    assert_eq!(
        zins.betroffen(),
        [Betroffen::groesse("eur-leitzins").ok_or("eur-leitzins")?]
    );
    let letzte = t.last().ok_or("leer")?;
    assert_eq!(letzte.kennung(), "ezb-2028-12-07-1");
    assert!(
        t.iter()
            .all(|x| x.verlaesslichkeit() == Verlaesslichkeit::Bestaetigt)
    );
    let zins_tage = t
        .iter()
        .filter(|x| {
            x.betroffen()
                .iter()
                .any(|b| b.to_string() == "eur-leitzins")
        })
        .count();
    let nicht_geld = t
        .iter()
        .filter(|x| x.titel().contains("non-monetary"))
        .count();
    assert!(zins_tage > 0 && nicht_geld > 0);
    assert!(
        t.iter()
            .filter(|x| x.titel().contains("non-monetary"))
            .all(|x| x.betroffen().iter().all(|b| b.to_string() == "ezb"))
    );
    Ok(())
}

fn seite(eintraege: &str) -> String {
    format!("<html><body><main><div><dl>\n{eintraege}\n</dl></div></main></body></html>")
}

/// Zwei Eintraege am selben Tag bekommen fortlaufende Kennungen.
#[test]
fn zwei_eintraege_am_selben_tag() -> Result<(), String> {
    let t = lies_ezb_termine(&seite(
        "<dt> 01/12/2026 </dt><dd> A meeting<br> </dd>\n<dt>01/12/2026</dt><dd>Another   meeting\n<br></dd>",
    ))
    .map_err(|e| format!("{e:?}"))?;
    let k: Vec<(&str, &str)> = t.iter().map(|x| (x.kennung(), x.titel())).collect();
    assert_eq!(
        k,
        vec![
            ("ezb-2026-12-01-1", "A meeting"),
            ("ezb-2026-12-01-2", "Another meeting")
        ]
    );
    Ok(())
}

/// Jede Abweichung ist ein Fehler mit der Nummer des Eintrags; nichts wird geraten.
#[test]
fn abweichungen() {
    let gut = "<dt>01/12/2026</dt><dd>Meeting<br></dd>";
    assert_eq!(lies_ezb_termine(""), Err(EzbFehler::Seite));
    assert_eq!(
        lies_ezb_termine(&format!("<dl>{gut}</dl>")),
        Err(EzbFehler::Seite)
    );
    assert_eq!(lies_ezb_termine(&seite("")), Err(EzbFehler::Seite));
    assert_eq!(
        lies_ezb_termine(&format!("<main><dl>{gut}</dl><dl>{gut}</dl></main>")),
        Err(EzbFehler::Seite)
    );
    let fall = |zweiter: &str| lies_ezb_termine(&seite(&format!("{gut}{zweiter}")));
    for (zweiter, fehler) in [
        ("<dt>32/12/2026</dt><dd>X<br></dd>", EzbFehler::Datum(2)),
        ("<dt>2026-12-01</dt><dd>X<br></dd>", EzbFehler::Datum(2)),
        ("<dt>1/12/2026</dt><dd>X<br></dd>", EzbFehler::Datum(2)),
        ("<dt>01/12/2026</dt>", EzbFehler::Form(2)),
        ("<dd>X<br></dd>", EzbFehler::Form(2)),
        ("<dt>01/12/2026</dt>text<dd>X<br></dd>", EzbFehler::Form(2)),
        ("<dt>01/12/2026</dt><dd>X</dd>", EzbFehler::Text(2)),
        (
            "<dt>01/12/2026</dt><dd>X <b>Y</b><br></dd>",
            EzbFehler::Text(2),
        ),
        (
            "<dt>01/12/2026</dt><dd>X &amp; Y<br></dd>",
            EzbFehler::Text(2),
        ),
        ("<dt>01/12/2026</dt><dd><br></dd>", EzbFehler::Text(2)),
        (
            "<dt>01/12/2026</dt><dd>X; Y<br></dd>",
            EzbFehler::Termin(2, TerminFehler::Titel),
        ),
        ("<p>x</p>", EzbFehler::Form(2)),
    ] {
        assert_eq!(fall(zweiter), Err(fehler), "{zweiter}");
    }
}

// ---- Der Lauf des Sammlers ueber die Termine ----

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

/// Ein leerer Ordner mit eingerichtetem Journal.
fn ordner() -> Result<(PathBuf, PathBuf), String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_termine_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
    let journal = dir.join("journal.txt");
    vorbedingung(Journal::einrichten(&journal), "Einrichtung")?;
    Ok((journal, dir.join("roh")))
}

fn utc(t: u8, h: u8, min: u8) -> Result<Zeitpunkt, String> {
    Zeitpunkt::neu(tag(2026, 11, t)?, h, min, 0).ok_or_else(|| "Vorbedingung: Zeitpunkt".into())
}

/// Erfundene Quelle: eine feste Antwort, merkt sich jede Adresse.
struct Erfunden {
    antwort: Result<Vec<u8>, String>,
    abgerufen: Vec<String>,
}

impl Netz for Erfunden {
    fn hole(&mut self, adresse: &str) -> Result<Vec<u8>, String> {
        self.abgerufen.push(adresse.to_string());
        self.antwort.clone()
    }
}

fn lauf(
    journal: &Path,
    roh: &Path,
    antwort: Result<&str, &str>,
    um: Zeitpunkt,
) -> Result<Result<TerminBericht, SammelFehler>, String> {
    let mut j = vorbedingung(Journal::oeffnen(journal), "Journal oeffnen")?;
    let mut netz = Erfunden {
        antwort: antwort
            .map(|t| t.as_bytes().to_vec())
            .map_err(str::to_string),
        abgerufen: Vec::new(),
    };
    let mut zeiten = vec![um].into_iter();
    let mut uhr = move || zeiten.next();
    let bericht = sammle_termine(&mut j, &mut netz, &mut uhr, roh);
    assert_eq!(netz.abgerufen, vec![ADRESSE_EZB.to_string()]);
    Ok(bericht)
}

fn eintraege(journal: &Path) -> Result<usize, String> {
    Ok(vorbedingung(Journal::oeffnen(journal), "Journal oeffnen")?
        .eintraege()
        .len())
}

fn dateien(roh: &Path) -> Vec<PathBuf> {
    let mut d: Vec<PathBuf> = fs::read_dir(roh.join(QUELLE_EZB))
        .map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    d.sort();
    d
}

/// Erster Lauf: die rohe Seite liegt unveraendert unter `roh/ezb/`, jeder der 52 Termine steht
/// mit Quelle, Abrufzeit und Abdruck der Seite im Journal.
#[test]
fn erster_lauf_belegt_jeden_termin() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let um = utc(4, 18, 0)?;
    let b = lauf(&journal, &roh, Ok(SEITE), um)?.map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.fehler, None);
    assert_eq!((b.neu, b.geaendert, b.unveraendert), (52, 0, 0));
    assert!(b.fehlen.is_empty());
    let datei = b.rohdatei.ok_or("keine Rohdatei")?;
    assert_eq!(dateien(&roh), vec![datei.clone()]);
    assert_eq!(
        datei.file_name().and_then(|n| n.to_str()),
        Some("20261104T180000Z.html")
    );
    assert_eq!(
        fs::read(&datei).map_err(|e| e.to_string())?,
        SEITE.as_bytes()
    );

    let j = vorbedingung(Journal::oeffnen(&journal), "Journal oeffnen")?;
    let belegt = j.stand().termine();
    assert_eq!(belegt.len(), 52);
    let abdruck = belegt.first().ok_or("leer")?.roh().to_string();
    assert_eq!(abdruck.len(), 64);
    assert!(belegt.iter().all(|t| t.quelle() == QUELLE_EZB
        && t.herkunft() == Herkunft::Quelle
        && t.abruf() == um
        && t.roh() == abdruck));
    let erwartet = lies_ezb_termine(SEITE).map_err(|e| format!("{e:?}"))?;
    let gelesen: Vec<_> = belegt.iter().map(|t| t.termin().clone()).collect();
    assert_eq!(gelesen, erwartet);
    Ok(())
}

/// Zweiter Lauf mit derselben Seite: nichts Neues im Journal, aber die Seite liegt wieder roh ab.
#[test]
fn zweiter_lauf_schreibt_nichts_doppelt() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    lauf(&journal, &roh, Ok(SEITE), utc(4, 18, 0)?)?.map_err(|e| format!("{e:?}"))?;
    let vorher = eintraege(&journal)?;
    let b = lauf(&journal, &roh, Ok(SEITE), utc(5, 18, 0)?)?.map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.fehler, None);
    assert_eq!((b.neu, b.geaendert, b.unveraendert), (0, 0, 52));
    assert!(b.fehlen.is_empty());
    assert_eq!(eintraege(&journal)?, vorher);
    assert_eq!(dateien(&roh).len(), 2);
    Ok(())
}

/// Aendert sich der Text eines Termins, wird er unter seiner Kennung fortgeschrieben; die alte
/// Fassung bleibt im Journal.
#[test]
fn geaenderter_termin_wird_fortgeschrieben() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    lauf(&journal, &roh, Ok(SEITE), utc(4, 18, 0)?)?.map_err(|e| format!("{e:?}"))?;
    let alt = "General Council meeting of the ECB in Frankfurt<br>";
    let neu = "General Council meeting of the ECB (virtual)<br>";
    assert_eq!(SEITE.matches(alt).count(), 1, "Vorbedingung: Text einmal");
    let seite = SEITE.replacen(alt, neu, 1);
    let um = utc(5, 18, 0)?;
    let b = lauf(&journal, &roh, Ok(&seite), um)?.map_err(|e| format!("{e:?}"))?;
    assert_eq!((b.neu, b.geaendert, b.unveraendert), (0, 1, 51));
    let j = vorbedingung(Journal::oeffnen(&journal), "Journal oeffnen")?;
    let t = j.stand().termin("ezb-2026-11-26-1").ok_or("Termin fehlt")?;
    assert_eq!(
        t.termin().titel(),
        "General Council meeting of the ECB (virtual)"
    );
    assert_eq!(t.abruf(), um);
    let fassungen: Vec<&str> = j
        .stand()
        .termine()
        .iter()
        .filter(|t| t.termin().kennung() == "ezb-2026-11-26-1")
        .map(|t| t.termin().titel())
        .collect();
    assert_eq!(
        fassungen,
        vec![
            "General Council meeting of the ECB in Frankfurt",
            "General Council meeting of the ECB (virtual)"
        ]
    );
    Ok(())
}

/// Ein kuenftiger Termin, der nicht mehr auf der Seite steht, wird gemeldet, nicht geloescht.
/// Ein vergangener fehlt auf der Seite regelmaessig und wird nicht gemeldet.
#[test]
fn fehlender_termin_wird_nur_gemeldet() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    lauf(&journal, &roh, Ok(SEITE), utc(4, 18, 0)?)?.map_err(|e| format!("{e:?}"))?;
    let vorher = eintraege(&journal)?;
    // Der Eintrag vom 26.11.2026 verschwindet von der Seite.
    let start = SEITE.find("26/11/2026").ok_or("Vorbedingung: 26.11.")?;
    let ab = SEITE
        .get(..start)
        .and_then(|s| s.rfind("<dt>"))
        .ok_or("Vorbedingung: dt")?;
    let ende = SEITE
        .get(ab..)
        .and_then(|s| s.find("</dd>"))
        .ok_or("Vorbedingung: dd")?;
    let bis = ab
        .checked_add(ende)
        .and_then(|e| e.checked_add(5))
        .ok_or("Vorbedingung")?;
    let seite = format!(
        "{}{}",
        SEITE.get(..ab).ok_or("Vorbedingung")?,
        SEITE.get(bis..).ok_or("Vorbedingung")?
    );
    let b = lauf(&journal, &roh, Ok(&seite), utc(5, 18, 0)?)?.map_err(|e| format!("{e:?}"))?;
    assert_eq!(b.fehler, None);
    assert_eq!((b.neu, b.geaendert, b.unveraendert), (0, 0, 51));
    assert_eq!(b.fehlen, vec!["ezb-2026-11-26-1".to_string()]);
    assert_eq!(eintraege(&journal)?, vorher);

    // Am 27.11. ist der 26.11. vergangen; er wird nicht mehr gemeldet.
    let b = lauf(&journal, &roh, Ok(&seite), utc(27, 18, 0)?)?.map_err(|e| format!("{e:?}"))?;
    assert!(b.fehlen.is_empty(), "{:?}", b.fehlen);
    Ok(())
}

/// Eine unlesbare Seite: nichts belegt, die rohe Seite bleibt zur Durchsicht liegen.
#[test]
fn unlesbare_seite_wird_nicht_belegt() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let vorher = eintraege(&journal)?;
    let kaputt = SEITE.replacen("26/11/2026", "31/11/2026", 1);
    let b = lauf(&journal, &roh, Ok(&kaputt), utc(4, 18, 0)?)?.map_err(|e| format!("{e:?}"))?;
    let fehler = b.fehler.ok_or("kein Fehler gemeldet")?;
    assert!(fehler.contains("Datum(2)"), "{fehler}");
    assert_eq!((b.neu, b.geaendert, b.unveraendert), (0, 0, 0));
    assert_eq!(eintraege(&journal)?, vorher);
    let datei = b.rohdatei.ok_or("keine Rohdatei")?;
    assert_eq!(
        fs::read(&datei).map_err(|e| e.to_string())?,
        kaputt.as_bytes()
    );

    // Auch kein UTF-8 ist unlesbar und wird nicht belegt.
    let mut j = vorbedingung(Journal::oeffnen(&journal), "Journal oeffnen")?;
    let mut netz = Erfunden {
        antwort: Ok(vec![0xff, 0xfe]),
        abgerufen: Vec::new(),
    };
    let mut zeiten = vec![utc(4, 19, 0)?].into_iter();
    let mut uhr = move || zeiten.next();
    let b = sammle_termine(&mut j, &mut netz, &mut uhr, &roh).map_err(|e| format!("{e:?}"))?;
    assert!(b.fehler.is_some());
    assert!(b.rohdatei.is_some());
    assert_eq!(eintraege(&journal)?, vorher);
    Ok(())
}

/// Scheitert der Abruf, wird nichts abgelegt und nichts belegt.
#[test]
fn netzfehler_legt_nichts_ab() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    let vorher = eintraege(&journal)?;
    let b =
        lauf(&journal, &roh, Err("Status 503"), utc(4, 18, 0)?)?.map_err(|e| format!("{e:?}"))?;
    let fehler = b.fehler.ok_or("kein Fehler gemeldet")?;
    assert!(fehler.contains("Status 503"), "{fehler}");
    assert_eq!(b.rohdatei, None);
    assert!(dateien(&roh).is_empty());
    assert_eq!(eintraege(&journal)?, vorher);
    Ok(())
}

/// Faellt die Uhr hinter den letzten Abruf zurueck, haelt der Lauf an, bevor er etwas belegt.
#[test]
fn uhr_zurueck_haelt_an() -> Result<(), String> {
    let (journal, roh) = ordner()?;
    lauf(&journal, &roh, Ok(SEITE), utc(4, 18, 0)?)?.map_err(|e| format!("{e:?}"))?;
    let seite = SEITE.replacen(
        "General Council meeting of the ECB in Frankfurt<br>",
        "General Council meeting of the ECB (virtual)<br>",
        1,
    );
    let vorher = eintraege(&journal)?;
    let r = lauf(&journal, &roh, Ok(&seite), utc(4, 17, 0)?)?;
    assert!(
        matches!(r, Err(SammelFehler::Journal(JournalFehler::Abgelehnt(_)))),
        "{r:?}"
    );
    assert_eq!(eintraege(&journal)?, vorher);
    Ok(())
}
