//! Geheimnisfreie Metadaten eines Sicherheitsabbruchs. Kein Antwortausschnitt verlässt
//! diese Funktion; auch die Namen stammen nur aus der festen Quellenliste.
use crate::{Anbieter, Netz, Wert, Zugang};
use depot_kern::json;
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
}
