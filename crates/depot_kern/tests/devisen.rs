//! Umrechnung von Dollar in Euro ueber die Referenzkurse der EZB: Lesen des EZB-Textes, Kurs
//! nur desselben Tages, abgerundet auf sechs Nachkommastellen, und Schlusskurse der NYSE in
//! Euro. Alle Kurse sind erfunden (D12); der Text hat nur die Form der EZB-Datei.

use depot_kern::{
    Boerse, Cent, Datum, Devisen, DevisenFehler, KursFehler, Preis, QuellenFehler, Tageskurs,
    Waehrung, Zeitpunkt, schlusskurse, schlusskurse_in_euro,
};

fn tag(j: u16, m: u8, t: u8) -> Result<Datum, String> {
    Datum::neu(j, m, t).ok_or_else(|| format!("Vorbedingung: Datum {j}-{m}-{t}"))
}

fn utc(j: u16, m: u8, t: u8, h: u8, min: u8) -> Result<Zeitpunkt, String> {
    Zeitpunkt::neu(tag(j, m, t)?, h, min, 0).ok_or_else(|| "Vorbedingung: Zeitpunkt".into())
}

fn euro(text: &str) -> Result<Preis, String> {
    Preis::lies_euro(text).ok_or_else(|| format!("Vorbedingung: Preis {text}"))
}

fn isin(text: &str) -> Result<depot_kern::Isin, String> {
    depot_kern::Isin::neu(text).map_err(|e| format!("Vorbedingung: ISIN {e:?}"))
}

/// Ein Text in der Form der EZB: Kopf, dann je Tag ein `Cube time` mit Waehrungen darin.
fn ezb(tage: &[(&str, &str)]) -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><gesmes:Envelope xmlns:gesmes=\"x\" \
         xmlns=\"y\"><gesmes:subject>Reference rates</gesmes:subject><Cube>",
    );
    for (zeit, usd) in tage {
        for teil in [
            "<Cube time=\"",
            zeit,
            "\"><Cube currency=\"USD\" rate=\"",
            usd,
            "\"/><Cube currency=\"JPY\" rate=\"177.85\"/></Cube>",
        ] {
            s.push_str(teil);
        }
    }
    s.push_str("</Cube></gesmes:Envelope>");
    s
}

fn lies(text: &str) -> Result<Devisen, String> {
    Devisen::lies_ezb(text).map_err(|e| format!("Vorbedingung: EZB {e:?}"))
}

/// Gelesen wird je Tag genau der Dollarkurs; andere Waehrungen zaehlen nicht.
#[test]
fn liest_dollar_je_tag() -> Result<(), String> {
    let d = lies(&ezb(&[("2026-11-05", "1.1224"), ("2026-11-04", "1.1267")]))?;
    assert_eq!(d.usd_je_eur(tag(2026, 11, 5)?), Some(euro("1.1224")?));
    assert_eq!(d.usd_je_eur(tag(2026, 11, 4)?), Some(euro("1.1267")?));
    assert_eq!(d.usd_je_eur(tag(2026, 11, 3)?), None);
    assert_eq!(d.letzter_tag(), Some(tag(2026, 11, 5)?));
    Ok(())
}

/// Ein kaputter Eintrag verwirft den ganzen Text, statt still zu fehlen.
#[test]
fn kaputter_text_ergibt_keine_kurse() -> Result<(), String> {
    let d = tag(2026, 11, 5)?;
    assert_eq!(Devisen::lies_ezb(""), Err(DevisenFehler::Leer));
    assert_eq!(Devisen::lies_ezb(&ezb(&[])), Err(DevisenFehler::Leer));
    assert_eq!(
        Devisen::lies_ezb(&ezb(&[("2026-11-31", "1.1")])),
        Err(DevisenFehler::Datum("2026-11-31".into()))
    );
    assert_eq!(
        Devisen::lies_ezb(&ezb(&[("2026-11-05", "1.1"), ("2026-11-05", "1.2")])),
        Err(DevisenFehler::Doppelt(d))
    );
    for kaputt in ["0", "-1.1", "1,12", "1.12345678", "", "x"] {
        assert_eq!(
            Devisen::lies_ezb(&ezb(&[("2026-11-05", kaputt)])),
            Err(DevisenFehler::Kurs(d)),
            "Kurs {kaputt:?} angenommen"
        );
    }
    assert_eq!(
        Devisen::lies_ezb(
            "<gesmes:Envelope><Cube><Cube currency=\"USD\" rate=\"1.1\"/></Cube></gesmes:Envelope>"
        ),
        Err(DevisenFehler::OhneTag)
    );
    Ok(())
}

/// Euro = Dollar durch Referenzkurs desselben Tages, abgerundet auf sechs Nachkommastellen.
/// Ohne Kurs des Tages gibt es keinen Euro-Preis, auch wenn ein Vortag einen hat.
#[test]
fn umrechnung_nur_am_selben_tag_abgerundet() -> Result<(), String> {
    let d = lies(&ezb(&[("2026-11-05", "1.1224"), ("2026-11-04", "1.25")]))?;
    // 100 / 1.1224 = 89.0947968...; abgeschnitten 89.094796.
    assert_eq!(
        d.in_euro(euro("100")?, tag(2026, 11, 5)?),
        Some(euro("89.094796")?)
    );
    // Exakt teilbar: 125 / 1.25 = 100.
    assert_eq!(
        d.in_euro(euro("125")?, tag(2026, 11, 4)?),
        Some(euro("100")?)
    );
    assert_eq!(d.in_euro(euro("100")?, tag(2026, 11, 6)?), None);
    // Kleiner als die feinste Stelle ergibt keinen positiven Preis.
    let gross = lies(&ezb(&[("2026-11-05", "2")]))?;
    assert_eq!(gross.in_euro(Preis::aus_fein(1), tag(2026, 11, 5)?), None);
    assert_eq!(
        Devisen::keine().in_euro(euro("100")?, tag(2026, 11, 5)?),
        None
    );
    Ok(())
}

/// Ein Wert der NYSE ergibt mit Referenzkursen einen Kurs in Euro, heute und am Vortag je mit
/// dem Kurs seines eigenen Tages. Fehlt der Kurs eines der beiden Tage, ergibt der Wert keinen
/// Kurs. Ohne Umrechnung bleibt es bei der Ablehnung des Dollars.
#[test]
fn nyse_in_euro() -> Result<(), String> {
    let a = isin("US0378331005")?;
    let tage = vec![
        Tageskurs {
            datum: tag(2026, 11, 4)?,
            schluss: euro("125")?,
        },
        Tageskurs {
            datum: tag(2026, 11, 5)?,
            schluss: euro("100")?,
        },
    ];
    // 22:05 Uhr in Frankfurt = 21:05 UTC: der Tag in New York ist beendet.
    let spaet = utc(2026, 11, 5, 21, 5)?;
    let d = lies(&ezb(&[("2026-11-05", "1.1224"), ("2026-11-04", "1.25")]))?;
    let (heute, vortag) =
        schlusskurse_in_euro(spaet, vec![(a.clone(), Boerse::Nyse, tage.clone())], &d)
            .map_err(|e| format!("{e:?}"))?;
    let h = heute.eintrag(&a).ok_or("kein Kurs heute")?;
    let v = vortag.eintrag(&a).ok_or("kein Kurs am Vortag")?;
    assert_eq!(h.preis(), euro("89.094796")?);
    assert_eq!(h.waehrung(), Waehrung::Eur);
    assert_eq!(h.handelstag(), tag(2026, 11, 5)?);
    assert_eq!(v.preis(), euro("100")?);

    // Kurs des heutigen Tages fehlt: kein Rueckgriff auf den Vortag.
    let ohne_heute = lies(&ezb(&[("2026-11-04", "1.25")]))?;
    assert_eq!(
        schlusskurse_in_euro(
            spaet,
            vec![(a.clone(), Boerse::Nyse, tage.clone())],
            &ohne_heute
        ),
        Err(QuellenFehler::KeinDevisenkurs(tag(2026, 11, 5)?))
    );
    let ohne_vortag = lies(&ezb(&[("2026-11-05", "1.1224")]))?;
    assert_eq!(
        schlusskurse_in_euro(
            spaet,
            vec![(a.clone(), Boerse::Nyse, tage.clone())],
            &ohne_vortag
        ),
        Err(QuellenFehler::KeinDevisenkurs(tag(2026, 11, 4)?))
    );
    assert_eq!(
        schlusskurse(spaet, vec![(a, Boerse::Nyse, tage)]),
        Err(QuellenFehler::Kurs(KursFehler::Waehrung))
    );
    Ok(())
}

/// Ein Wert von Xetra bleibt mit Referenzkursen unveraendert in Euro.
#[test]
fn xetra_bleibt_unveraendert() -> Result<(), String> {
    let a = isin("DE0007164600")?;
    let tage = vec![Tageskurs {
        datum: tag(2026, 11, 5)?,
        schluss: Preis::aus_cent(Cent(4_150)).ok_or("Vorbedingung: Preis")?,
    }];
    let spaet = utc(2026, 11, 5, 21, 5)?;
    let d = lies(&ezb(&[("2026-11-05", "1.1224")]))?;
    let (mit, _) = schlusskurse_in_euro(spaet, vec![(a.clone(), Boerse::Xetra, tage.clone())], &d)
        .map_err(|e| format!("{e:?}"))?;
    let (ohne, _) =
        schlusskurse(spaet, vec![(a, Boerse::Xetra, tage)]).map_err(|e| format!("{e:?}"))?;
    assert_eq!(mit, ohne);
    Ok(())
}

/// Maus' Gegenprobe zu aba39b0: Ein abgeschnittener Text ergibt keinen Kurs, an welcher
/// Stelle er auch endet. Nur der vollstaendige Text mit geschlossener Huelle gilt.
#[test]
fn abgeschnittener_text_ergibt_keine_kurse() -> Result<(), String> {
    let voll = ezb(&[("2026-11-05", "1.1224"), ("2026-11-04", "1.1267")]);
    lies(&voll)?;
    let enden = voll.char_indices().map(|(i, _)| i).filter(|i| *i > 0);
    for ende in enden {
        let teil = voll.get(..ende).ok_or("Vorbedingung: Grenze")?;
        assert!(
            Devisen::lies_ezb(teil).is_err(),
            "abgeschnitten nach {ende} Zeichen angenommen: {teil}"
        );
    }
    Ok(())
}

/// Maus' Gegenprobe zu aba39b0: Ein Dollarkurs zaehlt nur in seinem eigenen Tagesblock. Steht
/// er ausserhalb, gehoert er zu keinem Tag, und der ganze Text wird verworfen, statt dass er
/// dem zuletzt gelesenen Tag zugeschlagen wird. Ebenso jeder andere Aufbau als Huelle, Tag,
/// Waehrung.
#[test]
fn dollar_nur_im_eigenen_tagesblock() -> Result<(), String> {
    let kopf = "<?xml version=\"1.0\"?><gesmes:Envelope><Cube>";
    let fuss = "</Cube></gesmes:Envelope>";
    let usd = "<Cube currency=\"USD\" rate=\"1.2\"/>";
    let fall = |mitte: &str| format!("{kopf}{mitte}{fuss}");
    // Nach dem Tagesblock, vor dem naechsten: nicht dem 05.11. zugeschlagen.
    assert_eq!(
        Devisen::lies_ezb(&fall(&format!(
            "<Cube time=\"2026-11-05\"><Cube currency=\"JPY\" rate=\"177\"/></Cube>{usd}"
        ))),
        Err(DevisenFehler::OhneTag)
    );
    assert_eq!(
        Devisen::lies_ezb(&fall(&format!(
            "<Cube time=\"2026-11-05\"></Cube>{usd}<Cube time=\"2026-11-04\">{usd}</Cube>"
        ))),
        Err(DevisenFehler::OhneTag)
    );
    // Falsch verschachtelt oder falsch geschlossen.
    for mitte in [
        format!("<Cube time=\"2026-11-05\"><Cube time=\"2026-11-04\">{usd}</Cube></Cube>"),
        format!("<Cube time=\"2026-11-05\">{usd}</Cube></Cube><Cube>"),
        "<Cube time=\"2026-11-05\"><Cube currency=\"USD\" rate=\"1.2\"></Cube>".to_string(),
        format!("<Cube time=\"2026-11-05\">{usd}</Cube time>"),
        format!("<Cube time=\"2026-11-05\">{usd}</Wuerfel></Cube>"),
        "<Cube time=\"2026-11-05\" currency=\"USD\" rate=\"1.2\"/>".to_string(),
        format!("<Cube time=\"2026-11-05\">{usd}<Cube></Cube></Cube>"),
    ] {
        assert!(
            Devisen::lies_ezb(&fall(&mitte)).is_err(),
            "angenommen: {mitte}"
        );
    }
    // Ohne Huelle `Cube` oder ausserhalb von `gesmes:Envelope`.
    assert!(
        Devisen::lies_ezb(&format!(
            "<gesmes:Envelope><Cube time=\"2026-11-05\">{usd}</Cube></gesmes:Envelope>"
        ))
        .is_err()
    );
    assert!(
        Devisen::lies_ezb(&format!(
            "<Cube><Cube time=\"2026-11-05\">{usd}</Cube></Cube>"
        ))
        .is_err()
    );
    // Gegenprobe: derselbe Tag richtig aufgebaut gilt.
    let d = lies(&fall(&format!("<Cube time=\"2026-11-05\">{usd}</Cube>")))?;
    assert_eq!(d.usd_je_eur(tag(2026, 11, 5)?), Some(euro("1.2")?));
    Ok(())
}

/// Maus' Restbefund zu f267b0f: Attribute werden vollstaendig gelesen. Ein Name zweimal, gleich
/// in welcher Schreibweise (Anfuehrungszeichen, Leerraum, Gross- und Kleinschreibung), verwirft
/// den ganzen Text, statt dass der erste Wert gilt. Ebenso ein unbekanntes Attribut an einem
/// `Cube` oder ein Rest, der kein Attribut ist. Gueltige Schreibweisen nach XML gelten.
#[test]
fn attribute_vollstaendig_und_eindeutig() -> Result<(), String> {
    let fall = |tag: &str| {
        format!(
            "<?xml version=\"1.0\"?><gesmes:Envelope xmlns=\"y\"><Cube>{tag}</Cube></gesmes:Envelope>"
        )
    };
    let tag_mit = |usd: &str| format!("<Cube time=\"2026-11-05\">{usd}</Cube>");
    for kaputt in [
        tag_mit("<Cube currency=\"USD\" rate=\"1.2\" rate='2.4'/>"),
        tag_mit("<Cube currency=\"USD\" rate=\"1.2\" rate=\"1.2\"/>"),
        tag_mit("<Cube currency=\"USD\" rate=\"1.2\" Rate=\"2.4\"/>"),
        tag_mit("<Cube currency=\"USD\" currency='USD' rate=\"1.2\"/>"),
        tag_mit("<Cube currency=\"USD\" rate=\"1.2\" x/>"),
        tag_mit("<Cube currency=\"USD\" rate=\"1.2\" kurs=\"1.3\"/>"),
        tag_mit("<Cube currency=\"USD\" rate=\"1.2/>"),
        tag_mit("<Cube currency=\"USD\"rate=\"1.2\"/>"),
        tag_mit("<Cube currency=USD rate=\"1.2\"/>"),
        "<Cube time=\"2026-11-05\" time='2026-11-04'><Cube currency=\"USD\" rate=\"1.2\"/></Cube>"
            .to_string(),
    ] {
        assert!(
            Devisen::lies_ezb(&fall(&kaputt)).is_err(),
            "angenommen: {kaputt}"
        );
    }
    let doppelt_huelle = "<gesmes:Envelope xmlns=\"a\" xmlns='b'><Cube><Cube time=\"2026-11-05\">\
                          <Cube currency=\"USD\" rate=\"1.2\"/></Cube></Cube></gesmes:Envelope>";
    assert!(Devisen::lies_ezb(doppelt_huelle).is_err());
    // Gueltig nach XML: einfache Anfuehrungszeichen und Leerraum um `=`.
    for gut in [
        tag_mit("<Cube currency='USD' rate='1.2'/>"),
        tag_mit("<Cube currency = \"USD\"\n rate= \"1.2\" />"),
        tag_mit("<Cube rate=\"1.2\" currency=\"USD\"/>"),
    ] {
        let d = lies(&fall(&gut))?;
        assert_eq!(d.usd_je_eur(tag(2026, 11, 5)?), Some(euro("1.2")?), "{gut}");
    }
    Ok(())
}
