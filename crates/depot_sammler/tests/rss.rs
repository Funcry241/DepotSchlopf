use depot_kern::{Datum, Zeitpunkt};
use depot_sammler::rss::{lies, zeit};
fn abruf() -> Result<Zeitpunkt, String> {
    Datum::neu(2026, 11, 7)
        .and_then(|d| Zeitpunkt::neu(d, 18, 0, 0))
        .ok_or("Zeit fehlt".into())
}
fn feed(inhalt: &str) -> String {
    format!("<rss version=\"2.0\"><channel>{inhalt}</channel></rss>")
}
fn item(titel: &str, datum: &str) -> String {
    format!(
        "<item><title>{titel}</title><pubDate>{datum}</pubDate><link>https://www.tagesschau.de/nachricht.html</link></item>"
    )
}
#[test]
fn datum_mit_zeitzone_wird_exakt_nach_utc_umgerechnet() {
    assert_eq!(
        zeit("Sat, 07 Nov 2026 13:00:00 +0100"),
        zeit("Sat, 07 Nov 2026 12:00:00 GMT")
    );
    assert_eq!(
        zeit("Sat, 07 Nov 2026 07:00:00 -0500"),
        zeit("Sat, 07 Nov 2026 12:00:00 GMT")
    );
    for t in [
        "Sat, 07 Nov 2026 12:00:00",
        "Sat, 07 Nov 2026 12:00:00 EST",
        "Sat, 07 Nov 2026 12:00:00 ++100",
        "Sat, 07 Nov 2026 25:00:00 GMT",
    ] {
        assert!(zeit(t).is_none(), "{t}");
    }
}
#[test]
fn cdata_entities_und_link_bleiben_nachvollziehbar() -> Result<(), String> {
    let t = feed(&item(
        "<![CDATA[Politik < Wirtschaft]]> &amp; &#x20ac;",
        "Sat, 07 Nov 2026 12:00:00 GMT",
    ));
    let (n, fehlend, _) = lies(&t, "tagesschau", abruf()?)?;
    assert_eq!(fehlend, 0);
    assert!(
        n.first()
            .is_some_and(|n| n.text.contains("Politik < Wirtschaft & €"))
    );
    Ok(())
}
#[test]
fn zukuenftige_alte_und_unvollstaendige_eintraege_werden_gezaehlt() -> Result<(), String> {
    let t = feed(
        &(item("Zukunft", "Sun, 08 Nov 2026 12:00:00 GMT")
            + &item("Alt", "Thu, 05 Nov 2026 12:00:00 GMT")
            + "<item><title>Keine Zeit</title></item>"),
    );
    let (n, ausgelassen, _) = lies(&t, "tagesschau", abruf()?)?;
    assert!(n.is_empty());
    assert_eq!(ausgelassen, 3);
    Ok(())
}
#[test]
fn dtd_trunkierung_und_falsche_verschachtelung_verwerfen_den_feed() -> Result<(), String> {
    let gut = feed(&item("Titel", "Sat, 07 Nov 2026 12:00:00 GMT"));
    for t in [
        gut.replace("</rss>", ""),
        gut.replace("</channel>", "</falsch>"),
        format!("<!DOCTYPE rss [<!ENTITY a SYSTEM 'file:///etc/passwd'>]>{gut}"),
        gut.replace("<title>", "<title><b>"),
        gut.replace("version=\"2.0\"", "version=\"2.0\" version=\"2.0\""),
        format!("{gut}{gut}"),
        gut.replace("Titel", "&#0;"),
    ] {
        assert!(lies(&t, "tagesschau", abruf()?).is_err(), "{t}");
    }
    Ok(())
}
#[test]
fn fremde_und_aktive_links_werden_nicht_uebernommen() -> Result<(), String> {
    for url in [
        "http://www.tagesschau.de/x",
        "javascript:alert(1)",
        "https://www.tagesschau.de.evil.example/x",
        "https://www.tagesschau.de@evil.example/x",
    ] {
        let t = feed(&item("Titel", "Sat, 07 Nov 2026 12:00:00 GMT"))
            .replace("https://www.tagesschau.de/nachricht.html", url);
        let (n, ausgelassen, _) = lies(&t, "tagesschau", abruf()?)?;
        assert!(n.is_empty());
        assert_eq!(ausgelassen, 1);
    }
    Ok(())
}

#[test]
fn doppelte_felder_verwerfen_nur_den_betroffenen_eintrag() -> Result<(), String> {
    for (feld, wert) in [
        ("title", "Mehrdeutig"),
        ("link", "https://www.tagesschau.de/anderer.html"),
        ("pubDate", "Sat, 07 Nov 2026 13:00:00 GMT"),
    ] {
        let davor = item("Davor", "Sat, 07 Nov 2026 12:00:00 GMT");
        let fehler = item("Doppelt", "Sat, 07 Nov 2026 12:00:00 GMT").replace(
            &format!("</{feld}>"),
            &format!("</{feld}><{feld}>{wert}</{feld}>"),
        );
        let danach = item("Danach", "Sat, 07 Nov 2026 12:00:00 GMT");
        let (n, ausgelassen, _) = lies(
            &feed(&format!("{davor}{fehler}{danach}")),
            "tagesschau",
            abruf()?,
        )?;
        assert_eq!(ausgelassen, 1, "{feld}");
        assert_eq!(n.len(), 2, "{feld}");
        assert!(n.first().is_some_and(|n| n.text.contains("Davor")));
        assert!(n.last().is_some_and(|n| n.text.contains("Danach")));
    }
    Ok(())
}
