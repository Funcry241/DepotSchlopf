//! Termine im Ereignisregister (R39): gleiches Format fuer jede Quelle, geprueft beim Eintritt.
//! Alle Termine hier sind erfunden (D12).

use depot_kern::{
    Betroffen, Datum, Isin, Termin, TerminFehler, Verlaesslichkeit, Zeitbezug, Zeitpunkt,
};

fn tag(j: u16, m: u8, t: u8) -> Result<Datum, String> {
    Datum::neu(j, m, t).ok_or_else(|| format!("Vorbedingung: Datum {j}-{m}-{t}"))
}

fn isin(text: &str) -> Result<Isin, String> {
    Isin::neu(text).map_err(|e| format!("Vorbedingung: ISIN {e:?}"))
}

fn groesse(name: &str) -> Result<Betroffen, String> {
    Betroffen::groesse(name).ok_or_else(|| format!("Vorbedingung: Groesse {name}"))
}

/// Datum und Zeitbezug als Text: nur genau die Formen `JJJJ-MM-TT` und `JJJJ-MM-TTThh:mm:ssZ`.
#[test]
fn zeitbezug_als_text() -> Result<(), String> {
    let d = tag(2026, 12, 17)?;
    assert_eq!(d.to_string(), "2026-12-17");
    assert_eq!(Datum::lies("2026-12-17"), Some(d));
    for falsch in ["2026-12-32", "2026-2-17", "17.12.2026", "2026-12-17 ", ""] {
        assert_eq!(Datum::lies(falsch), None, "{falsch}");
    }
    let z = Zeitpunkt::neu(d, 13, 15, 0).ok_or("Vorbedingung: Zeitpunkt")?;
    assert_eq!(Zeitbezug::lies("2026-12-17"), Some(Zeitbezug::Tag(d)));
    assert_eq!(
        Zeitbezug::lies("2026-12-17T13:15:00Z"),
        Some(Zeitbezug::Zeitpunkt(z))
    );
    assert_eq!(Zeitbezug::Zeitpunkt(z).to_string(), "2026-12-17T13:15:00Z");
    assert_eq!(Zeitbezug::Zeitpunkt(z).tag(), Some(d));
    for falsch in ["2026-12-17T13:15Z", "2026-12-17T13:15:00", "x"] {
        assert_eq!(Zeitbezug::lies(falsch), None, "{falsch}");
    }
    Ok(())
}

/// Ein gueltiger Termin; Betroffene werden geordnet.
#[test]
fn termin_gueltig() -> Result<(), String> {
    let sap = isin("DE0007164600")?;
    let t = Termin::neu(
        "ezb-geldpolitik-2026-12-17",
        "EZB: geldpolitische Sitzung, Pressekonferenz",
        Zeitbezug::Tag(tag(2026, 12, 17)?),
        vec![groesse("eur-leitzins")?, Betroffen::Wert(sap.clone())],
        Verlaesslichkeit::Bestaetigt,
    )
    .map_err(|e| format!("{e:?}"))?;
    assert_eq!(t.kennung(), "ezb-geldpolitik-2026-12-17");
    assert_eq!(
        t.betroffen(),
        [Betroffen::Wert(sap), groesse("eur-leitzins")?]
    );
    assert_eq!(t.verlaesslichkeit(), Verlaesslichkeit::Bestaetigt);
    for v in [
        Verlaesslichkeit::Bestaetigt,
        Verlaesslichkeit::Angekuendigt,
        Verlaesslichkeit::Vermutet,
    ] {
        assert_eq!(Verlaesslichkeit::aus_name(v.name()), Some(v));
    }
    assert_eq!(Verlaesslichkeit::aus_name("sicher"), None);
    Ok(())
}

/// Jede Abweichung vom Format ist ein Fehler; nichts wird repariert.
#[test]
fn termin_fehler() -> Result<(), String> {
    let zeit = Zeitbezug::Tag(tag(2026, 12, 17)?);
    let b = || -> Result<Vec<Betroffen>, String> { Ok(vec![groesse("eur-leitzins")?]) };
    let neu = |k: &str, t: &str, b: Vec<Betroffen>| {
        Termin::neu(k, t, zeit, b, Verlaesslichkeit::Bestaetigt).err()
    };
    let lang = "a".repeat(81);
    for k in [
        "",
        "EZB",
        "ezb_1",
        "-ezb",
        "ezb-",
        "ezb sitzung",
        lang.as_str(),
    ] {
        assert_eq!(neu(k, "Titel", b()?), Some(TerminFehler::Kennung), "{k:?}");
    }
    assert!(neu(&"a".repeat(80), "Titel", b()?).is_none());
    let titel_lang = "x".repeat(201);
    for t in ["", "a;b", "a\nb", "a\tb", titel_lang.as_str()] {
        assert_eq!(neu("ezb", t, b()?), Some(TerminFehler::Titel), "{t:?}");
    }
    assert!(neu("ezb", &"ä".repeat(200), b()?).is_none());
    assert_eq!(neu("ezb", "Titel", vec![]), Some(TerminFehler::Betroffen));
    assert_eq!(
        neu(
            "ezb",
            "Titel",
            vec![groesse("eur-leitzins")?, groesse("eur-leitzins")?]
        ),
        Some(TerminFehler::Betroffen)
    );
    for g in ["", "EUR", "eur_zins", "-eur", "eur-", "eur zins"] {
        assert_eq!(Betroffen::groesse(g), None, "{g:?}");
    }
    assert_eq!(
        Betroffen::lies("DE0007164600"),
        Some(Betroffen::Wert(isin("DE0007164600")?))
    );
    assert_eq!(
        Betroffen::lies("eur-leitzins"),
        Some(groesse("eur-leitzins")?)
    );
    assert_eq!(Betroffen::lies("DE0007164601"), None);
    Ok(())
}
