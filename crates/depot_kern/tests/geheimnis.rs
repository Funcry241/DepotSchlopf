//! Maskieren ohne Zugriff auf Schlüssel, plus bekannte Fragmente als Quellenkontrolle.
use depot_kern::geheimnis::{maskiere, mit_schluessel};

#[test]
fn url_header_und_json_bleiben_ohne_zugangsdaten_lesbar() {
    assert_eq!(
        maskiere(
            "https://example.com/aapl/prices?startDate=2026-10-21&format=csv&token=geheim&x=1"
        ),
        "https://example.com/aapl/prices?startDate=2026-10-21&format=csv&token=***&x=1"
    );
    for name in ["apikey", "api_key", "key", "ToKeN"] {
        assert_eq!(
            maskiere(&format!("Adresse?{name}=geheim&x=1")),
            format!("Adresse?{name}=***&x=1")
        );
        assert_eq!(
            maskiere(&format!("{{\"{name}\": \"geheim\", \"status\": 503}}")),
            format!("{{\"{name}\": \"***\", \"status\": 503}}")
        );
    }
    for t in [
        "Authorization: Basic dXNlcjpwYXNz",
        "Authorization: Bearer geheim",
        "Basic dXNlcjpwYXNz",
        "Bearer geheim",
    ] {
        let aus = maskiere(t);
        assert!(aus.contains("***"));
        assert!(!aus.contains("geheim"));
        assert!(!aus.contains("dXNlcjpwYXNz"));
    }
    assert_eq!(
        maskiere("Risk-Management, alles bereit"),
        "Risk-Management, alles bereit"
    );
}

#[test]
fn mehrere_werte_escapes_fragmente_und_zeilenenden() {
    let key = ["GEHEIM", "SCHLUESSEL", "WEITER"].concat();
    let text = "Anfang\r\nDebug EIMSCHLUESSEL\r\nEnde";
    assert_eq!(mit_schluessel(text, &key), "Anfang\r\nDebug ***\r\nEnde");
    assert_eq!(
        mit_schluessel("Schlüssel abcdefgh", "ABCDEFGH"),
        "Schlüssel ***"
    );
    assert_eq!(maskiere("apikey='abc' token=def"), "apikey='***' token=***");
    assert_eq!(maskiere(r#"{"x":"s\u006b-ant-fremd"}"#), "***");
    assert_eq!(
        mit_schluessel(r#"{"x":"\u0047EHEIMSCHLUESSELWEITER"}"#, &key),
        "***"
    );
    let bereinigt = maskiere("key=geheim&token=anderer");
    assert_eq!(maskiere(&bereinigt), bereinigt);
}

#[test]
fn gemini_autorisierungsschluessel_wird_auch_ohne_speicher_maskiert() {
    let key = "AQ.Erfundener_Gemini_Testwert";
    assert_eq!(
        maskiere(&format!("Gemini meldet ({key}).")),
        "Gemini meldet (***)."
    );
    assert_eq!(mit_schluessel(&format!("Kopf: {key}"), key), "Kopf: ***");
}

#[test]
fn aiza_ohne_speicher_wird_maskiert() {
    let wert = format!("AIza{}", "X".repeat(35));
    for (text, erwartet) in [
        (format!("Fehler ({wert})."), "Fehler (***)."),
        (format!(r#"{{"key":"{wert}"}}"#), r#"{"key":"***"}"#),
        (
            format!(r#"{{"text":"Meldung {wert}."}}"#),
            r#"{"text":"Meldung ***."}"#,
        ),
        (format!("Antwort {wert}"), "Antwort ***"),
    ] {
        assert_eq!(maskiere(&text), erwartet);
    }
}

#[test]
fn aiza_kurz_und_fremdwort_bleiben() {
    for text in [
        format!("AIza{}", "X".repeat(10)),
        format!("FAIza{}", "X".repeat(35)),
        "AIza ist ein Präfix".into(),
    ] {
        assert_eq!(maskiere(&text), text);
    }
}

#[test]
fn maskiere_ist_idempotent_fuer_aiza() {
    let text = format!("Antwort (AIza{})", "X".repeat(35));
    let maskiert = maskiere(&text);
    assert_eq!(maskiert, "Antwort (***)");
    assert_eq!(maskiere(&maskiert), maskiert);
}
