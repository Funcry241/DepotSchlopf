//! Geheimnisfreie Metadaten eines Sicherheitsabbruchs. Kein Antwortausschnitt verlässt
//! diese Funktion; auch die Namen stammen nur aus der festen Quellenliste.
use crate::{Anbieter, Netz, Wert, Zugang};
use depot_journal::kurssperre::{self, Art, Quelle};
use depot_kern::{Zeitpunkt, json};
use std::path::Path;
use std::time::Duration;

fn antwortart(roh: &[u8]) -> &'static str {
    let Ok(text) = std::str::from_utf8(roh) else {
        return "nicht als UTF-8 lesbar";
    };
    if let Ok(w) = json::lies(text) {
        if w.feld("Error Message").is_some()
            || w.feld("error").is_some()
            || w.feld("detail").is_some()
        {
            return "JSON-Fehlermeldung des Anbieters (kein Fehlertext übernommen)";
        }
        if w.feld("Information").is_some() || w.feld("Note").is_some() {
            return "JSON-Hinweis des Anbieters (kein Hinweistext übernommen)";
        }
        return "JSON-Antwort ohne eindeutig erkannten Meldungstyp";
    }
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if text.starts_with("timestamp,open,high,low,close,volume")
        || text.starts_with("date,close,high,low")
    {
        "CSV-Kursdaten (nur am Spaltenkopf erkannt, nicht als gültige Kurse bestätigt)"
    } else if text.starts_with('<') {
        "XML/HTML-Antwort (Inhalt nicht ausgewertet)"
    } else {
        "Antwortart nicht erkannt"
    }
}

fn name(quelle: &str) -> &'static str {
    match quelle {
        "alphavantage" => "Alpha Vantage",
        "tiingo" => "Tiingo",
        "ezb" => "EZB",
        _ => "nicht zugeordnet",
    }
}

pub(crate) const ALPHA_LIMIT: &str = "Abruf: Alpha Vantage meldet Tageslimit";

/// Nur der eng begrenzte Hinweis des Anbieters darf dessen eigenen Zugang wiederholen.
pub(crate) fn alpha_limit(zugang: &Zugang, quelle: Anbieter, roh: &[u8]) -> bool {
    if quelle != Anbieter::AlphaVantage {
        return false;
    }
    let Some(json::Json::Objekt(felder)) = std::str::from_utf8(roh)
        .ok()
        .and_then(|t| json::lies(t).ok())
    else {
        return false;
    };
    let [(name, json::Json::Text(text))] = felder.as_slice() else {
        return false;
    };
    let meldung = text.to_ascii_lowercase();
    matches!(name.as_str(), "Information" | "Note")
        && (meldung.contains("rate limit") || meldung.contains("requests per day"))
        && zugang
            .treffer(roh)
            .iter()
            .all(|a| *a == Anbieter::AlphaVantage)
        && zugang.treffer(roh) == zugang.treffer(text.as_bytes())
        && !zugang.steht_in(name.as_bytes())
}

pub(crate) fn sicherheitsabbruch(zugang: &Zugang, quelle: &str, roh: &[u8]) -> Option<String> {
    let treffer = zugang.treffer(roh);
    if treffer.is_empty() {
        return None;
    }
    let wer = treffer
        .iter()
        .map(|a| match a {
            Anbieter::AlphaVantage => "Alpha Vantage",
            Anbieter::Tiingo => "Tiingo",
        })
        .collect::<Vec<_>>()
        .join(", ");
    Some(zugang.bereinige(&format!(
        "Die Antwort enthält den Schlüssel: nicht abgelegt, nicht belegt, Lauf beendet. \
         Quelle der Antwort: {}. Erkannter gespeicherter Zugang: {wer}. Antwortart: {}. \
         Treffer auf einen vollständigen hinterlegten Schlüssel, eventuell nach JSON-Dekodierung; \
         keine Aussage über Ursache oder Zugriff durch Dritte.",
        name(quelle),
        antwortart(roh)
    )))
}

/// Ausdrücklich gestarteter Prüfabruf bei bestehender Kurssperre. Kein Journal, keine
/// Rohdateien, kein Entsperren. Je Wert nur ein Versuch über die bereits freigegebene Adresse.
/// Alle Ausgaben sind feste Metadaten und werden zusätzlich gegen sämtliche Zugänge bereinigt.
pub fn pruefabruf(
    netz: &mut impl Netz,
    zugang: &Zugang,
    werte: &[Wert],
    schlafe: &mut impl FnMut(Duration),
    melde: &mut impl FnMut(&str),
) {
    let mut ausgabe = |text: &str| melde(&zugang.bereinige(text));
    let mut begonnen = false;
    let mut alpha_beendet = false;
    for wert in werte {
        let a = Anbieter::fuer(wert.boerse);
        if alpha_beendet && a == Anbieter::AlphaVantage {
            continue;
        }
        let Some(key) = zugang.von(a) else {
            ausgabe(&zugang.bereinige(&format!(
                "Quelle {}: Zugang fehlt oder ist unlesbar; nicht abgerufen.",
                name(a.dienst())
            )));
            continue;
        };
        if begonnen {
            schlafe(Duration::from_secs(15));
        }
        begonnen = true;
        let roh = netz.hole(&crate::sammeln::adresse(wert, key, None));
        let Ok(roh) = roh else {
            // Ein Netzfehler kann eine URL mit Zugang oder einen Antwortausschnitt enthalten.
            ausgabe(&format!(
                "Quelle {}: Abruf fehlgeschlagen; kein Fehlertext übernommen.",
                name(a.dienst())
            ));
            continue;
        };
        if alpha_limit(zugang, a, &roh) {
            ausgabe(ALPHA_LIMIT);
            alpha_beendet = true;
            continue;
        }
        if let Some(d) = sicherheitsabbruch(zugang, a.dienst(), &roh) {
            ausgabe(&d);
            ausgabe(
                "Prüfabruf beendet. Keine Antwort und keine Kurse gespeichert. Kurssperre bleibt bestehen.",
            );
            return;
        }
        ausgabe(&zugang.bereinige(&format!(
            "Quelle {}: {}. Kein Schlüsseltreffer bei diesem Abruf; keine Aussage über die Gültigkeit der Kurse.",
            name(a.dienst()), antwortart(&roh)
        )));
    }
    ausgabe(
        "Prüfabruf beendet. Keine Antwort und keine Kurse gespeichert. Kurssperre bleibt bestehen.",
    );
}

/// Ergebnis der einzigen automatischen Prüfung pro Frankfurter Kalendertag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutomatischePruefung {
    KeinBedarf,
    Aufgehoben,
    Sicherheit,
    Morgen,
}

pub(crate) fn sperrquelle(a: Anbieter) -> Quelle {
    match a {
        Anbieter::AlphaVantage => Quelle::AlphaVantage,
        Anbieter::Tiingo => Quelle::Tiingo,
    }
}

fn pruefadresse(
    quelle: Quelle,
    zugang: &Zugang,
    werte: &[Wert],
) -> Option<(String, Option<Anbieter>)> {
    let anbieter = match quelle {
        Quelle::AlphaVantage => Anbieter::AlphaVantage,
        Quelle::Tiingo => Anbieter::Tiingo,
        Quelle::Ezb => return Some((crate::ADRESSE_DEVISEN.into(), None)),
        Quelle::Rss => return None,
    };
    let wert = werte
        .iter()
        .find(|w| Anbieter::fuer(w.boerse) == anbieter)?;
    let key = zugang.von(anbieter)?;
    Some((crate::sammeln::adresse(wert, key, None), Some(anbieter)))
}

fn saubere_daten(quelle: Quelle, roh: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(roh) else {
        return false;
    };
    match quelle {
        Quelle::AlphaVantage => depot_kern::lies_tageskurse(text).is_ok_and(|t| !t.is_empty()),
        Quelle::Tiingo => {
            depot_kern::tiingo::lies_tageskurse(text).is_ok_and(|t| !t.kurse.is_empty())
        }
        Quelle::Ezb => depot_kern::Devisen::lies_ezb(text).is_ok(),
        Quelle::Rss => false,
    }
}

/// Prüft einen Altfall mit der zugeordneten Quelle. Fehler und unbekannte Antworten
/// bleiben gesperrt. Rohantwort und Zugang verlassen die Prüfung nie.
pub fn pruefe_altsperre(
    daten: &Path,
    jetzt: Zeitpunkt,
    netz: &mut dyn Netz,
    zugang: &Zugang,
    werte: &[Wert],
) -> Result<AutomatischePruefung, String> {
    use AutomatischePruefung::{Aufgehoben, KeinBedarf, Morgen, Sicherheit};
    if !kurssperre::lies(daten)?.is_some_and(|s| s.art == Art::Unklar && s.quelle.is_some()) {
        return Ok(KeinBedarf);
    }
    let schutz = kurssperre::sperren(daten)?;
    let Some(sperre) = kurssperre::lies(daten)?.filter(|s| s.art == Art::Unklar) else {
        return Ok(KeinBedarf);
    };
    let Some(quelle) = sperre.quelle else {
        return Ok(KeinBedarf);
    };
    let Some((adresse, anbieter)) = pruefadresse(quelle, zugang, werte) else {
        return Ok(KeinBedarf);
    };
    let (tag, ..) = jetzt.in_frankfurt().ok_or("Prüfuhr unlesbar")?;
    if !schutz.pruefung_vermerken(tag)?
        || (anbieter == Some(Anbieter::AlphaVantage) && netz.pruefe_alpha(1).is_err())
    {
        return Ok(Morgen);
    }
    let Ok(roh) = netz.hole(&adresse) else {
        return Ok(Morgen);
    };
    let limit = anbieter.is_some_and(|a| alpha_limit(zugang, a, &roh));
    if !limit && zugang.steht_in(&roh) {
        schutz.sicherheit(
            quelle,
            Some(jetzt),
            "Prüfabruf bestätigte einen Schlüsselfund. Schlüssel prüfen und bei Bedarf erneuern.",
        )?;
        return Ok(Sicherheit);
    }
    if limit || saubere_daten(quelle, &roh) {
        schutz.fehlalarm_aufheben(&sperre, jetzt)?;
        return Ok(Aufgehoben);
    }
    Ok(Morgen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Schluessel;
    use std::fmt::Write as _;

    #[test]
    fn limit_erfordert_genau_das_hinweisfeld_und_nur_eigenen_zugang() -> Result<(), String> {
        let alpha = "ERFUNDEN0123TEST";
        let tiingo = "0123456789abcdef0123456789abcdef01234567";
        let zugang = Zugang::alpha_vantage(Schluessel::neu(alpha)?).mit(
            Anbieter::Tiingo,
            Schluessel::fuer(Anbieter::Tiingo, tiingo)?,
        );
        let hinweis = format!(r#"{{"Note":"RATE LIMIT {alpha}"}}"#);
        assert!(alpha_limit(
            &zugang,
            Anbieter::AlphaVantage,
            hinweis.as_bytes()
        ));
        assert!(!alpha_limit(&zugang, Anbieter::Tiingo, hinweis.as_bytes()));
        for roh in [
            format!(r#"{{"Note":"rate limit {alpha}","x":"{alpha}"}}"#),
            format!(r#"{{"Note":"rate limit {tiingo}"}}"#),
            format!(r#"{{"Note":"premium endpoint {alpha}"}}"#),
            format!("symbol,close\n{alpha},1"),
        ] {
            assert!(!alpha_limit(
                &zugang,
                Anbieter::AlphaVantage,
                roh.as_bytes()
            ));
            assert!(sicherheitsabbruch(&zugang, "alphavantage", roh.as_bytes()).is_some());
        }
        Ok(())
    }

    #[test]
    fn diagnose_enthaelt_nur_feste_metadaten_auch_bei_fremdem_oder_kodiertem_zugang()
    -> Result<(), String> {
        let alpha = "ERFUNDEN0123TEST";
        let tiingo = "0123456789abcdef0123456789abcdef01234567";
        let z = Zugang::alpha_vantage(Schluessel::neu(alpha)?).mit(
            Anbieter::Tiingo,
            Schluessel::fuer(Anbieter::Tiingo, tiingo)?,
        );
        let geheim = json::zitiere(tiingo);
        let mut verborgen = String::new();
        for c in tiingo.chars() {
            let _ = write!(verborgen, "\\u{:04x}", u32::from(c));
        }
        for roh in [
            format!("{{\"Information\":{geheim}}}"),
            format!("{{\"Information\":\"{verborgen}\"}}"),
        ] {
            let d =
                sicherheitsabbruch(&z, "alphavantage", roh.as_bytes()).ok_or("Treffer fehlt")?;
            assert!(d.contains("Quelle der Antwort: Alpha Vantage"));
            assert!(d.contains("gespeicherter Zugang: Tiingo"));
            assert!(d.contains("JSON-Hinweis"));
            assert!(!d.contains(alpha) && !d.contains(tiingo) && !d.contains(&verborgen));
        }
        let roh = format!("{{\"Error Message\":\"PRIVATER-ANTWORTTEXT {alpha}\"}}");
        let d = sicherheitsabbruch(&z, "alphavantage", roh.as_bytes()).ok_or("Treffer fehlt")?;
        assert!(d.contains("JSON-Fehlermeldung") && !d.contains("PRIVATER-ANTWORTTEXT"));
        assert!(
            sicherheitsabbruch(&z, "alphavantage", b"{\"Information\":\"kein Geheimnis\"}")
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn csv_unbekannte_antwort_und_kollision_mit_metadatennamen_geben_keinen_schluessel_aus()
    -> Result<(), String> {
        let z = Zugang::alpha_vantage(Schluessel::neu("ALPHAVANTAGE")?);
        for roh in [
            b"timestamp,open,high,low,close,volume\nALPHAVANTAGE".as_slice(),
            b"Unbekannter-Text ALPHAVANTAGE".as_slice(),
            b"<html>ALPHAVANTAGE</html>".as_slice(),
            b"\xff ALPHAVANTAGE".as_slice(),
        ] {
            let d = sicherheitsabbruch(&z, "alphavantage", roh).ok_or("Treffer fehlt")?;
            assert!(!d.to_ascii_uppercase().contains("ALPHAVANTAGE"));
            assert!(!d.contains("Unbekannter-Text") && !d.contains("<html>"));
        }
        Ok(())
    }
    const ALT: &str = "Sicherheitsabbruch: Die Kursquelle gab den Schlüssel zurück. Teilkurse dieses Abrufs nicht verwenden; Schlüssel prüfen und bei Bedarf erneuern";
    const TESTKEY: &str = "ERFUNDEN0123TEST";
    const CSV: &str = "timestamp,open,high,low,close,volume\n2026-11-09,230,231,229,230,1000\n";

    struct Netzprobe {
        roh: Option<Vec<u8>>,
        aufrufe: usize,
        alpha_frei: bool,
    }

    impl Netz for Netzprobe {
        fn pruefe_alpha(&mut self, _: usize) -> Result<(), String> {
            if self.alpha_frei {
                Ok(())
            } else {
                Err("Kontingent ausgeschöpft".into())
            }
        }
        fn hole(&mut self, _: &str) -> Result<Vec<u8>, crate::NetzFehler> {
            self.aufrufe = self.aufrufe.saturating_add(1);
            self.roh
                .clone()
                .ok_or_else(|| crate::NetzFehler::voruebergehend("Netzfehler"))
        }
    }

    fn mit_altfall(
        test: impl FnOnce(&Path, Zeitpunkt, &Zugang, &[Wert]) -> Result<(), String>,
    ) -> Result<(), String> {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NR: AtomicU32 = AtomicU32::new(0);
        let nr = NR.fetch_add(1, Ordering::Relaxed);
        let daten = std::env::temp_dir().join(format!("depot_r2_{}_{nr}", std::process::id()));
        std::fs::create_dir_all(&daten).map_err(|e| e.to_string())?;
        let ergebnis = (|| {
            std::fs::write(daten.join(kurssperre::KURSSPERRE), ALT).map_err(|e| e.to_string())?;
            let jetzt = Zeitpunkt::lies("2026-11-10T06:20:00Z").ok_or("Uhr fehlt")?;
            let zugang = Zugang::alpha_vantage(Schluessel::neu(TESTKEY)?);
            let werte = crate::beobachtungsliste()?;
            test(&daten, jetzt, &zugang, &werte)
        })();
        std::fs::remove_dir_all(daten).map_err(|e| e.to_string())?;
        ergebnis
    }

    #[test]
    fn alter_limitfehlalarm_oder_saubere_csv_wird_einmal_aufgehoben() -> Result<(), String> {
        for antwort in [
            format!(r#"{{"Information":"rate limit apikey={TESTKEY}"}}"#),
            CSV.into(),
        ] {
            mit_altfall(|daten, zeit, zugang, werte| {
                let mut netz = Netzprobe {
                    roh: Some(antwort.into_bytes()),
                    aufrufe: 0,
                    alpha_frei: true,
                };
                assert_eq!(
                    pruefe_altsperre(daten, zeit, &mut netz, zugang, werte)?,
                    AutomatischePruefung::Aufgehoben
                );
                assert_eq!(
                    pruefe_altsperre(daten, zeit, &mut netz, zugang, werte)?,
                    AutomatischePruefung::KeinBedarf
                );
                assert_eq!(netz.aufrufe, 1);
                assert_eq!(kurssperre::kurssperre(daten), None);
                assert_eq!(kurssperre::letzter_fehlalarm(daten), Some(zeit));
                let protokoll = std::fs::read_to_string(daten.join(kurssperre::AUFGEHOBEN))
                    .map_err(|e| e.to_string())?;
                assert_eq!(protokoll.matches("automatisch: Fehlalarm").count(), 1);
                assert!(!protokoll.contains(TESTKEY));
                assert!(!daten.join("roh").exists());
                Ok(())
            })?;
        }
        Ok(())
    }

    #[test]
    fn schluesselfund_macht_alte_sperre_dauerhaft_und_geheimnisfrei() -> Result<(), String> {
        mit_altfall(|daten, zeit, zugang, werte| {
            let mut netz = Netzprobe {
                roh: Some(format!("{CSV}{TESTKEY}").into_bytes()),
                aufrufe: 0,
                alpha_frei: true,
            };
            assert_eq!(
                pruefe_altsperre(daten, zeit, &mut netz, zugang, werte)?,
                AutomatischePruefung::Sicherheit
            );
            let sperre = kurssperre::lies(daten)?.ok_or("Sperre fehlt")?;
            assert_eq!(
                (sperre.art, sperre.seit, sperre.quelle),
                (Art::Sicherheit, Some(zeit), Some(Quelle::AlphaVantage))
            );
            netz.roh = Some(CSV.as_bytes().to_vec());
            assert_eq!(
                pruefe_altsperre(daten, zeit, &mut netz, zugang, werte)?,
                AutomatischePruefung::KeinBedarf
            );
            assert_eq!(netz.aufrufe, 1);
            let text = std::fs::read_to_string(daten.join(kurssperre::KURSSPERRE))
                .map_err(|e| e.to_string())?;
            assert!(!text.contains(TESTKEY) && !daten.join("roh").exists());
            Ok(())
        })
    }

    #[test]
    fn kontingentfehler_und_netzfehler_warten_bis_morgen() -> Result<(), String> {
        for alpha_frei in [false, true] {
            mit_altfall(|daten, zeit, zugang, werte| {
                let mut netz = Netzprobe {
                    roh: None,
                    aufrufe: 0,
                    alpha_frei,
                };
                assert_eq!(
                    pruefe_altsperre(daten, zeit, &mut netz, zugang, werte)?,
                    AutomatischePruefung::Morgen
                );
                assert_eq!(netz.aufrufe, usize::from(alpha_frei));
                netz.alpha_frei = true;
                netz.roh = Some(CSV.as_bytes().to_vec());
                assert_eq!(
                    pruefe_altsperre(daten, zeit, &mut netz, zugang, werte)?,
                    AutomatischePruefung::Morgen
                );
                assert_eq!(netz.aufrufe, usize::from(alpha_frei));
                let morgen = Zeitpunkt::lies("2026-11-11T06:20:00Z").ok_or("Uhr fehlt")?;
                assert_eq!(
                    pruefe_altsperre(daten, morgen, &mut netz, zugang, werte)?,
                    AutomatischePruefung::Aufgehoben
                );
                Ok(())
            })?;
        }
        Ok(())
    }
}
