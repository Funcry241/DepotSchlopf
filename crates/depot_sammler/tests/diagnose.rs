//! Der Diagnoseweg verarbeitet Antworten ausschließlich im Speicher; Netzwerk und Wartezeit
//! sind hier erfunden. Kein Test braucht oder protokolliert echte Zugangsdaten.
use depot_sammler::{Anbieter, Netz, NetzFehler, Schluessel, Zugang, beobachtungsliste};
use std::time::Duration;

const KEY: &str = "ERFUNDEN0123TEST";
const TI: &str = "0123456789abcdef0123456789abcdef01234567";
struct Quelle {
    anzahl: usize,
    bei: usize,
    antwort: Vec<u8>,
}
impl Netz for Quelle {
    fn hole_autorisiert(
        &mut self,
        adresse: &str,
        autorisiert: &str,
    ) -> Result<Vec<u8>, NetzFehler> {
        assert!(adresse.starts_with("https://api.tiingo.com/"));
        assert!(!adresse.contains(TI) && !adresse.contains("token="));
        assert!(autorisiert == format!("Token {TI}"));
        self.hole(adresse)
    }

    fn hole(&mut self, _adresse: &str) -> Result<Vec<u8>, NetzFehler> {
        self.anzahl = self.anzahl.saturating_add(1);
        if self.anzahl == self.bei {
            Ok(self.antwort.clone())
        } else {
            Ok(b"timestamp,open,high,low,close,volume\n2026-11-06,1,1,1,1,1\n".to_vec())
        }
    }
}

#[test]
fn pruefabruf_stoppt_am_treffer_ohne_rohtext_oder_weitere_anfragen() -> Result<(), String> {
    let werte = beobachtungsliste()?;
    let z = Zugang::alpha_vantage(Schluessel::neu(KEY)?)
        .mit(Anbieter::Tiingo, Schluessel::fuer(Anbieter::Tiingo, TI)?);
    for (bei, quelle) in [(6, "Alpha Vantage"), (11, "Tiingo")] {
        let mut q = Quelle {
            anzahl: 0,
            bei,
            antwort: format!("{{\"Information\":\"PRIVATER-ANTWORTTEXT {TI}\"}}").into_bytes(),
        };
        let mut warten = Vec::new();
        let mut zeilen = Vec::new();
        depot_sammler::diagnose::pruefabruf(
            &mut q,
            &z,
            &werte,
            &mut |d| warten.push(d),
            &mut |s| zeilen.push(s.to_string()),
        );
        assert_eq!(q.anzahl, bei);
        assert_eq!(warten.len(), bei.saturating_sub(1));
        assert!(warten.iter().all(|d| *d == Duration::from_secs(15)));
        let text = zeilen.join("\n");
        assert!(text.contains(&format!("Quelle der Antwort: {quelle}")));
        assert!(text.contains("gespeicherter Zugang: Tiingo"));
        assert!(text.contains("JSON-Hinweis") && text.contains("Kurssperre bleibt bestehen"));
        assert!(
            !text.contains(KEY) && !text.contains(TI) && !text.contains("PRIVATER-ANTWORTTEXT")
        );
    }
    Ok(())
}

#[test]
fn pruefabruf_verspricht_ohne_treffer_keine_entwarnung_und_maskiert_auch_fehlermeldungen()
-> Result<(), String> {
    struct Fehler;
    impl Netz for Fehler {
        fn hole(&mut self, _: &str) -> Result<Vec<u8>, NetzFehler> {
            Err(NetzFehler::endgueltig(format!("PRIVATE-URL?apikey={KEY}")))
        }
    }
    let werte = beobachtungsliste()?;
    let z = Zugang::alpha_vantage(Schluessel::neu(KEY)?);
    for netzfehler in [false, true] {
        let mut q = Quelle {
            anzahl: 0,
            bei: 0,
            antwort: Vec::new(),
        };
        let mut zeilen = Vec::new();
        let mut melde = |s: &str| zeilen.push(s.to_string());
        if netzfehler {
            depot_sammler::diagnose::pruefabruf(&mut Fehler, &z, &werte, &mut |_| {}, &mut melde);
        } else {
            depot_sammler::diagnose::pruefabruf(&mut q, &z, &werte, &mut |_| {}, &mut melde);
            assert_eq!(q.anzahl, 10, "fehlender Tiingo-Zugang wird nicht abgerufen");
        }
        let text = zeilen.join("\n");
        assert!(text.contains("Kurssperre bleibt bestehen"));
        assert!(text.contains("Zugang fehlt oder ist unlesbar"));
        assert!(!text.contains(KEY) && !text.contains("PRIVATE-URL"));
        if !netzfehler {
            assert!(text.contains("keine Aussage über die Gültigkeit"));
        }
    }
    Ok(())
}

#[test]
fn cli_ohne_sperre_ruft_nichts_ab_und_normaler_abruf_bleibt_gesperrt() -> Result<(), String> {
    let wurzel = std::env::temp_dir().join(format!("depot_diagnose_cli_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    std::fs::create_dir_all(wurzel.join("kodex")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(wurzel.join("daten")).map_err(|e| e.to_string())?;
    std::fs::write(
        wurzel.join("kodex/festschreibung.txt"),
        "nur Wurzelmarker im Test",
    )
    .map_err(|e| e.to_string())?;
    let journal = wurzel.join("daten/journal.txt");
    let papier = wurzel.join("daten/papierkonto.txt");
    std::fs::write(&journal, b"absichtlich unlesbares Testjournal").map_err(|e| e.to_string())?;
    std::fs::write(&papier, b"Papierkonto unveraendert").map_err(|e| e.to_string())?;
    let j = std::fs::read(&journal).map_err(|e| e.to_string())?;
    let p = std::fs::read(&papier).map_err(|e| e.to_string())?;
    let r = std::process::Command::new(env!("CARGO_BIN_EXE_depot_sammler"))
        .args(["kurse", "diagnose"])
        .current_dir(&wurzel)
        .output()
        .map_err(|e| e.to_string())?;
    assert!(r.status.success());
    assert!(String::from_utf8_lossy(&r.stdout).contains("nichts abgerufen"));
    let halt = wurzel.join("daten/kurse.halt");
    let grund = "Sicherheitsabbruch: Die Kursquelle gab den Schluessel zurueck\n";
    std::fs::write(&halt, grund).map_err(|e| e.to_string())?;
    let r = std::process::Command::new(env!("CARGO_BIN_EXE_depot_sammler"))
        .arg("kurse")
        .current_dir(&wurzel)
        .output()
        .map_err(|e| e.to_string())?;
    assert!(!r.status.success());
    assert_eq!(
        std::fs::read_to_string(&halt).map_err(|e| e.to_string())?,
        grund
    );
    assert_eq!(std::fs::read(&journal).map_err(|e| e.to_string())?, j);
    assert_eq!(std::fs::read(&papier).map_err(|e| e.to_string())?, p);
    assert!(!wurzel.join("daten/roh").exists());
    std::fs::remove_dir_all(&wurzel).map_err(|e| e.to_string())?;
    Ok(())
}
