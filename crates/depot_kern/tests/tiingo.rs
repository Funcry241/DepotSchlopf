//! Dezimalartefakte nur am Tiingo-Eingang, alle späteren Preise bleiben exakt.
use depot_kern::tiingo::lies_tageskurse;
use depot_kern::{Preis, QuellenFehler};

#[test]
fn schluss_hoch_und_tief_werden_gleich_normalisiert() -> Result<(), String> {
    for zeile in [
        "2026-11-03,1,0.9999999999999999,0.5",
        "2026-11-03,1,2,1.0000000000000001",
        "2026-11-03,177.19799999999998,180.3,177.19799999999998",
    ] {
        let gelesen = lies_tageskurse(&format!("date,close,high,low\n{zeile}\n"))
            .map_err(|e| format!("{e:?}"))?;
        assert!(!gelesen.rundungen.is_empty());
        assert!(gelesen.rundungen.iter().all(|r| r.zeile == 2));
    }
    Ok(())
}

#[test]
fn toleranz_ist_exakt_begrenzt() -> Result<(), String> {
    let fall = |schluss: &str| {
        lies_tageskurse(&format!(
            "date,close,high,low\n2026-11-03,{schluss},2,0.5\n"
        ))
    };
    for gut in ["1.000000001", "0.999999999", "1.0000000009", "0.9999999999"] {
        let gelesen = fall(gut).map_err(|e| format!("{gut}: {e:?}"))?;
        assert_eq!(
            gelesen.kurse.first().map(|k| k.schluss),
            Some(Preis::aus_fein(1_000_000))
        );
    }
    for falsch in [
        "1.0000000010001",
        "0.9999999989999",
        "1.0000005",
        "1.0000015",
        "1.0000001",
        ".5859999999",
        "NaN",
        "inf",
        "-1.0000000001",
        "+1.0000000001",
        "1e0",
        "0.00000000001",
        "1.2.0000000",
    ] {
        assert_eq!(fall(falsch), Err(QuellenFehler::Preis(2)), "{falsch}");
    }
    assert_eq!(
        lies_tageskurse("date,close,high,low\n2026-11-03,1,0.999999,0.5\n"),
        Err(QuellenFehler::Spanne(2))
    );
    Ok(())
}

#[test]
fn unbereinigter_schluss_und_strenger_standardleser() -> Result<(), String> {
    let text = "date,close,high,low,adjClose\r\n2026-11-03,177.19799999999998,180.3,177.19799999999998,88.5\r\n";
    let gelesen = lies_tageskurse(text).map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        gelesen.kurse.first().map(|k| k.schluss),
        Some(Preis::aus_fein(177_198_000))
    );
    assert_eq!(gelesen.rundungen.len(), 2);
    assert_eq!(
        depot_kern::lies_tageskurse(text),
        Err(QuellenFehler::Preis(2))
    );
    Ok(())
}
