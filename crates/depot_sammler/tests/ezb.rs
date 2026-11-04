//! Termine der EZB von ihrer eigenen Seite (R39). Die Seite `SEITE` ist der Inhalt von `main`
//! der echten Seite, abgerufen am 04.11.2026; nur Kopf und Fuss der Seite sind weggelassen.

use depot_kern::{Betroffen, Datum, TerminFehler, Verlaesslichkeit, Zeitbezug};
use depot_sammler::ezb::{EzbFehler, lies_ezb_termine};

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
    assert_eq!(lies_ezb_termine("<dl>{gut}</dl>"), Err(EzbFehler::Seite));
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
