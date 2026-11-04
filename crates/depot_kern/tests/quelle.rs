//! Tageskurse aus einer lesenden Quelle: Zeit in Frankfurt, Zeitpunkt als Text, Lesen der
//! Kursdatei und Schlusskurse eines Abrufs. Alle Kurse sind erfunden (D12); die Dateien haben
//! nur die Form der Quellen.

use depot_kern::{
    Cent, Datum, Herkunft, Isin, Preis, QuellenFehler, Tageskurs, Zeitpunkt, lies_preis,
    lies_tageskurse, schlusskurse, xetra_schluss,
};

fn tag(j: u16, m: u8, t: u8) -> Result<Datum, String> {
    Datum::neu(j, m, t).ok_or_else(|| format!("Vorbedingung: Datum {j}-{m}-{t}"))
}

fn utc(j: u16, m: u8, t: u8, h: u8, min: u8) -> Result<Zeitpunkt, String> {
    Zeitpunkt::neu(tag(j, m, t)?, h, min, 0).ok_or_else(|| "Vorbedingung: Zeitpunkt".into())
}

fn cent(c: i64) -> Preis {
    Preis::aus_cent(Cent(c)).unwrap_or_else(|| unreachable!("Testpreis {c}"))
}

fn isin(text: &str) -> Result<Isin, String> {
    Isin::neu(text).map_err(|e| format!("Vorbedingung: ISIN {e:?}"))
}

/// Ein Zeitpunkt steht als `JJJJ-MM-TTThh:mm:ssZ` und wird nur genau so gelesen.
#[test]
fn zeitpunkt_als_text() -> Result<(), String> {
    let z = utc(2026, 11, 3, 16, 35)?;
    assert_eq!(z.to_string(), "2026-11-03T16:35:00Z");
    assert_eq!(Zeitpunkt::lies("2026-11-03T16:35:00Z"), Some(z));
    assert_eq!(z.datum(), Some(tag(2026, 11, 3)?));
    assert_eq!(z.uhrzeit(), Some((16, 35, 0)));
    let grenzen = [
        utc(1970, 1, 1, 0, 0)?,
        utc(2000, 2, 29, 23, 59)?,
        utc(9999, 12, 31, 23, 59)?,
    ];
    for g in grenzen {
        assert_eq!(Zeitpunkt::lies(&g.to_string()), Some(g), "{g}");
    }
    for falsch in [
        "2026-11-03T16:35:00",
        "2026-11-03 16:35:00Z",
        "2026-11-03T16:35Z",
        "2026-11-3T16:35:00Z",
        "2026-11-03T16:35:00+01:00",
        "2026-02-30T16:35:00Z",
        "2026-11-03T24:00:00Z",
        "+026-11-03T16:35:00Z",
        "2026-11-03T16:35:00ZZ",
        "",
    ] {
        assert_eq!(Zeitpunkt::lies(falsch), None, "{falsch}");
    }
    Ok(())
}

/// Wochentage nach ISO 8601.
#[test]
fn wochentage() -> Result<(), String> {
    assert_eq!(tag(1970, 1, 1)?.wochentag(), Some(4));
    assert_eq!(tag(2026, 11, 4)?.wochentag(), Some(3));
    assert_eq!(tag(2026, 3, 29)?.wochentag(), Some(7));
    assert_eq!(tag(2026, 10, 26)?.wochentag(), Some(1));
    Ok(())
}

/// Frankfurter Zeit nach EU-Recht: Umstellung am letzten Sonntag im Maerz und im Oktober um
/// 01:00 Uhr UTC. Eine Uhrzeit, die es nicht gibt oder doppelt gibt, ergibt keinen Zeitpunkt.
#[test]
fn frankfurter_zeit() -> Result<(), String> {
    let ff = |j, m, t, h, min| -> Result<Option<Zeitpunkt>, String> {
        Ok(Zeitpunkt::frankfurt(tag(j, m, t)?, h, min, 0))
    };
    // Winter: UTC+1, Sommer: UTC+2.
    assert_eq!(ff(2026, 11, 3, 17, 35)?, Some(utc(2026, 11, 3, 16, 35)?));
    assert_eq!(ff(2026, 7, 1, 17, 35)?, Some(utc(2026, 7, 1, 15, 35)?));
    // 29.03.2026: 02:00 springt auf 03:00.
    assert_eq!(ff(2026, 3, 29, 1, 59)?, Some(utc(2026, 3, 29, 0, 59)?));
    assert_eq!(ff(2026, 3, 29, 2, 30)?, None);
    assert_eq!(ff(2026, 3, 29, 3, 0)?, Some(utc(2026, 3, 29, 1, 0)?));
    assert_eq!(ff(2026, 3, 28, 17, 35)?, Some(utc(2026, 3, 28, 16, 35)?));
    // 25.10.2026: 03:00 faellt auf 02:00 zurueck, 02:xx gibt es zweimal.
    assert_eq!(ff(2026, 10, 25, 1, 59)?, Some(utc(2026, 10, 24, 23, 59)?));
    assert_eq!(ff(2026, 10, 25, 2, 30)?, None);
    assert_eq!(ff(2026, 10, 25, 3, 0)?, Some(utc(2026, 10, 25, 2, 0)?));
    assert_eq!(ff(2026, 10, 23, 17, 35)?, Some(utc(2026, 10, 23, 15, 35)?));
    // Ein anderes Jahr: 2027 ist der letzte Sonntag im Maerz der 28.
    assert_eq!(ff(2027, 3, 28, 2, 30)?, None);
    assert_eq!(ff(2027, 3, 27, 17, 35)?, Some(utc(2027, 3, 27, 16, 35)?));
    assert_eq!(
        xetra_schluss(tag(2026, 11, 3)?),
        Some(utc(2026, 11, 3, 16, 35)?)
    );
    Ok(())
}

/// Preise exakt bis sechs Nachkommastellen, auch feiner als ein Cent; noch feiner wird nicht
/// gerundet, sondern abgelehnt. Nicht Positives gilt nicht.
#[test]
fn preise_exakt() {
    assert_eq!(lies_preis("215.35"), Some(cent(21_535)));
    assert_eq!(lies_preis("215.3"), Some(cent(21_530)));
    assert_eq!(lies_preis("215"), Some(cent(21_500)));
    assert_eq!(lies_preis("215.3500"), Some(cent(21_535)));
    assert_eq!(lies_preis("0.01"), Some(cent(1)));
    assert_eq!(lies_preis("215.355"), Some(Preis::aus_fein(215_355_000)));
    assert_eq!(lies_preis("0.000001"), Some(Preis::aus_fein(1)));
    for falsch in [
        "215.3555555",
        "0",
        "0.00",
        "-1.00",
        "+1.00",
        "1e3",
        "",
        ".5",
        "5.",
        "1,5",
        " 1.5",
        "1.5.0",
        "N/D",
        "99999999999999999999",
    ] {
        assert_eq!(lies_preis(falsch), None, "{falsch}");
    }
}

const STOOQ: &str = "Date,Open,High,Low,Close,Volume\r\n\
2026-10-30,100.5,102,99.8,101.2,120000\r\n\
2026-11-02,101.2,103.4,100.9,103,98000\r\n\
2026-11-03,103,104.1,102.5,103.75,101000\r\n";

const ALPHA: &str = "timestamp,open,high,low,close,volume
2026-11-03,103.0000,104.1000,102.5000,103.7500,101000
2026-11-02,101.2000,103.4000,100.9000,103.0000,98000
";

/// Die Form von Stooq und von Alpha Vantage (neuester Tag zuerst) ergibt dieselben Kurse,
/// aeltester Tag zuerst.
#[test]
fn kursdatei_lesen() -> Result<(), String> {
    let k = lies_tageskurse(STOOQ).map_err(|e| format!("{e:?}"))?;
    let erwartet = vec![
        Tageskurs {
            datum: tag(2026, 10, 30)?,
            schluss: cent(10_120),
        },
        Tageskurs {
            datum: tag(2026, 11, 2)?,
            schluss: cent(10_300),
        },
        Tageskurs {
            datum: tag(2026, 11, 3)?,
            schluss: cent(10_375),
        },
    ];
    assert_eq!(k, erwartet);
    let a = lies_tageskurse(ALPHA).map_err(|e| format!("{e:?}"))?;
    assert_eq!(a, erwartet.get(1..).unwrap_or_default());
    // Echte Form von Alpha Vantage: Zeilenende CRLF, vier Nachkommastellen, Tief und Hoch
    // feiner als ein Cent. Der Schluss in ganzen Cent zaehlt.
    let fein = "timestamp,open,high,low,close,volume\r\n\
2026-11-03,103.0000,104.1050,102.4995,103.7500,101000\r\n\
2026-11-02,101.2000,103.4000,100.9000,103.0000,98000\r\n";
    let f = lies_tageskurse(fein).map_err(|e| format!("{e:?}"))?;
    assert_eq!(f, erwartet.get(1..).unwrap_or_default());
    // Nur Datum und Schluss genuegen.
    let knapp = lies_tageskurse("date,close\n2026-11-03,1.00\n").map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        knapp,
        vec![Tageskurs {
            datum: tag(2026, 11, 3)?,
            schluss: cent(100)
        }]
    );
    Ok(())
}

/// Jede Abweichung von der Form ist ein Fehler mit Zeile; nichts wird geraten oder repariert.
#[test]
fn kursdatei_fehler() {
    let kopf = "Date,Open,High,Low,Close,Volume\n";
    let fall = |zeilen: &str| lies_tageskurse(&format!("{kopf}{zeilen}"));
    assert_eq!(lies_tageskurse(""), Err(QuellenFehler::Leer));
    assert_eq!(lies_tageskurse(kopf), Err(QuellenFehler::Leer));
    // Stooq meldet einen unbekannten Wert mit einer Zeile ohne Kopf.
    assert_eq!(lies_tageskurse("No data"), Err(QuellenFehler::Kopf));
    assert_eq!(
        lies_tageskurse("Date,Open\n2026-11-03,1\n"),
        Err(QuellenFehler::Kopf)
    );
    assert_eq!(
        lies_tageskurse("Close,Open\n1,1\n"),
        Err(QuellenFehler::Kopf)
    );
    assert_eq!(
        lies_tageskurse("date,timestamp,close\n2026-11-03,2026-11-03,1\n"),
        Err(QuellenFehler::Kopf)
    );
    assert_eq!(
        lies_tageskurse("date,close,close\n2026-11-03,1,1\n"),
        Err(QuellenFehler::Kopf)
    );
    assert_eq!(fall("2026-11-03,1,1,1,1\n"), Err(QuellenFehler::Felder(2)));
    assert_eq!(
        fall("2026-11-03,1,1,1,1,1,1\n"),
        Err(QuellenFehler::Felder(2))
    );
    assert_eq!(fall("2026-11-31,1,1,1,1,1\n"), Err(QuellenFehler::Datum(2)));
    assert_eq!(fall("03.11.2026,1,1,1,1,1\n"), Err(QuellenFehler::Datum(2)));
    assert_eq!(fall("2026-11-03,1,1,1,0,1\n"), Err(QuellenFehler::Preis(2)));
    assert_eq!(
        fall("2026-11-03,1,1,1,1.0000001,1\n"),
        Err(QuellenFehler::Preis(2))
    );
    // Ein halber Cent ueber dem Hoch ist zu hoch, nicht gerundet gleich.
    assert_eq!(
        fall("2026-11-03,1,1,1,1.005,1\n"),
        Err(QuellenFehler::Spanne(2))
    );
    assert_eq!(fall("2026-11-03,1,1,x,1,1\n"), Err(QuellenFehler::Preis(2)));
    assert_eq!(
        fall("2026-11-03,1,1.0000001,1,1,1\n"),
        Err(QuellenFehler::Preis(2))
    );
    assert_eq!(fall("2026-11-03,1,1,0,1,1\n"), Err(QuellenFehler::Preis(2)));
    // Exakt verglichen: ein Zehntausendstel Cent ueber dem Schluss ist zu hoch.
    assert_eq!(
        fall("2026-11-03,1,2,1.000001,1,1\n"),
        Err(QuellenFehler::Spanne(2))
    );
    assert_eq!(
        fall("2026-11-03,1,0.999999,0.5,1,1\n"),
        Err(QuellenFehler::Spanne(2))
    );
    assert_eq!(
        fall("2026-11-03,1,2,1.5,1.4,1\n"),
        Err(QuellenFehler::Spanne(2))
    );
    assert_eq!(
        fall("2026-11-03,1,2,1,2.1,1\n"),
        Err(QuellenFehler::Spanne(2))
    );
    assert_eq!(
        fall("2026-11-02,1,1,1,1,1\n2026-11-02,1,1,1,1,1\n"),
        Err(QuellenFehler::Reihenfolge(3))
    );
    assert_eq!(
        fall("2026-11-02,1,1,1,1,1\n2026-11-03,1,1,1,1,1\n2026-10-30,1,1,1,1,1\n"),
        Err(QuellenFehler::Reihenfolge(4))
    );
    assert_eq!(
        fall("2026-11-03,1,1,1,1,1\n2026-11-02,1,1,1,1,1\n2026-11-04,1,1,1,1,1\n"),
        Err(QuellenFehler::Reihenfolge(4))
    );
}

/// Schlusskurse eines Abrufs: Ein Tag, dessen Xetra-Handel beim Abruf nicht beendet war, zaehlt
/// nicht. `heute` ist der letzte beendete Tag, `vortag` der davor, beide mit dem Abruf als Stand
/// und der Herkunft `Quelle`.
#[test]
fn schlusskurse_eines_abrufs() -> Result<(), String> {
    let a = isin("DE0007164600")?;
    let b = isin("DE0007100000")?;
    let tage = lies_tageskurse(STOOQ).map_err(|e| format!("{e:?}"))?;
    let nur_zwei = tage.get(..2).map(<[Tageskurs]>::to_vec).unwrap_or_default();

    // Abruf am 03.11. um 17:34 Uhr in Frankfurt: Der 03.11. ist noch nicht geschlossen.
    let frueh = utc(2026, 11, 3, 16, 34)?;
    let (heute, vortag) =
        schlusskurse(frueh, vec![(a.clone(), tage.clone())]).map_err(|e| format!("{e:?}"))?;
    assert_eq!(heute.stand(), frueh);
    assert_eq!(heute.kurs(&a), Some(Cent(10_300)));
    assert_eq!(vortag.kurs(&a), Some(Cent(10_120)));

    // Um 17:45 Uhr ist er es; sein Kurs ist zehn Minuten alt.
    let spaet = utc(2026, 11, 3, 16, 45)?;
    let (heute, vortag) = schlusskurse(spaet, vec![(a.clone(), tage), (b.clone(), nur_zwei)])
        .map_err(|e| format!("{e:?}"))?;
    let k = heute.eintrag(&a).ok_or("Kurs fehlt")?;
    assert_eq!(k.preis_cent(), Some(Cent(10_375)));
    assert_eq!(k.herkunft(), Herkunft::Quelle);
    assert_eq!(k.handelstag(), tag(2026, 11, 3)?);
    assert_eq!(Some(k.zeitpunkt()), xetra_schluss(tag(2026, 11, 3)?));
    assert_eq!(heute.alter_s(&a), Some(600));
    assert_eq!(vortag.kurs(&a), Some(Cent(10_300)));
    assert_eq!(vortag.stand(), spaet);
    assert_eq!(heute.kurs(&b), Some(Cent(10_300)));
    assert_eq!(vortag.kurs(&b), Some(Cent(10_120)));

    // Nur ein beendeter Tag: Der Vortag fehlt, er wird nicht erfunden.
    let ein_tag = vec![Tageskurs {
        datum: tag(2026, 11, 3)?,
        schluss: cent(500),
    }];
    let (heute, vortag) =
        schlusskurse(spaet, vec![(a.clone(), ein_tag)]).map_err(|e| format!("{e:?}"))?;
    assert_eq!(heute.kurs(&a), Some(Cent(500)));
    assert_eq!(vortag.kurs(&a), None);

    // Kein beendeter Tag: Der Wert fehlt in beiden.
    let morgen = vec![Tageskurs {
        datum: tag(2026, 11, 4)?,
        schluss: cent(500),
    }];
    let (heute, vortag) =
        schlusskurse(spaet, vec![(a.clone(), morgen)]).map_err(|e| format!("{e:?}"))?;
    assert_eq!((heute.kurs(&a), vortag.kurs(&a)), (None, None));
    Ok(())
}

/// Ein Wert zweimal oder Tage in falscher Reihenfolge: kein Ergebnis.
#[test]
fn schlusskurse_fehler() -> Result<(), String> {
    let a = isin("DE0007164600")?;
    let spaet = utc(2026, 11, 3, 16, 45)?;
    let t = |d, c| -> Result<Tageskurs, String> {
        Ok(Tageskurs {
            datum: d,
            schluss: cent(c),
        })
    };
    let gut = vec![t(tag(2026, 11, 2)?, 100)?, t(tag(2026, 11, 3)?, 101)?];
    assert_eq!(
        schlusskurse(spaet, vec![(a.clone(), gut.clone()), (a.clone(), gut)]),
        Err(QuellenFehler::Doppelt(a.clone()))
    );
    let verkehrt = vec![t(tag(2026, 11, 3)?, 101)?, t(tag(2026, 11, 2)?, 100)?];
    assert_eq!(
        schlusskurse(spaet, vec![(a.clone(), verkehrt)]),
        Err(QuellenFehler::Reihenfolge(2))
    );
    let doppelt = vec![t(tag(2026, 11, 3)?, 101)?, t(tag(2026, 11, 3)?, 102)?];
    assert_eq!(
        schlusskurse(spaet, vec![(a, doppelt)]),
        Err(QuellenFehler::Reihenfolge(2))
    );
    Ok(())
}

/// Echte Zeilen von Alpha Vantage aus dem ersten Lauf am 04.11.2026, je mit Kopfzeile: BASF
/// (Zeile 24), Mercedes-Benz (Zeile 3, der Vortag des Laufs) und Infineon (Zeile 10). Der Schluss
/// liegt jeweils auf einem halben Cent: 49,925 €, 41,155 € und 49,885 €.
const HALBE_CENT: [&str; 3] = [
    "timestamp,open,high,low,close,volume\r\n\
2026-10-02,49.6550,50.2300,49.3600,49.9250,1579740\r\n",
    "timestamp,open,high,low,close,volume\r\n\
2026-11-02,40.6250,41.3650,40.2600,41.1550,2443353\r\n",
    "timestamp,open,high,low,close,volume\r\n\
2026-10-22,51.6000,52.1600,49.5600,49.8850,4830045\r\n",
];

/// Echte Antwort von Alpha Vantage fuer SAP (Abruf 04.11.2026, Auszug): nur ganze Cent.
const SAP_ECHT: &str = "timestamp,open,high,low,close,volume\r\n\
2026-11-03,208.2000,209.1000,203.0500,206.6500,1987935\r\n\
2026-11-02,209.7500,211.2000,207.9500,210.5000,1476327\r\n\
2026-10-30,210.7000,214.4000,210.1500,211.2500,2032568\r\n";

/// Ein Schluss feiner als ein Cent ist ein gueltiger Kurs: Er wird gelesen, nicht verworfen und
/// nicht gerundet.
#[test]
fn schluss_mit_bruchteil_eines_cents() -> Result<(), String> {
    let erwartet = [
        ((2026, 10, 2), 49_925_000, "4992.5"),
        ((2026, 11, 2), 41_155_000, "4115.5"),
        ((2026, 10, 22), 49_885_000, "4988.5"),
    ];
    for (roh, ((j, m, t), fein, text)) in HALBE_CENT.into_iter().zip(erwartet) {
        let k = lies_tageskurse(roh).map_err(|e| format!("{e:?} in {roh}"))?;
        let schluss = Preis::aus_fein(fein);
        assert_eq!(
            k,
            vec![Tageskurs {
                datum: tag(j, m, t)?,
                schluss
            }]
        );
        assert_eq!(schluss.cent_text(), text);
        // Kein ganzer Cent: Fuer Bewertung und Limits gibt es keinen still gerundeten Betrag.
        assert_eq!(schluss.als_cent(), None);
        // Als Kurs eines Abrufs bleibt er exakt.
        let abruf = utc(j, m, t, 18, 0)?;
        let wert = isin("DE000BASF111")?;
        let (heute, _) =
            schlusskurse(abruf, vec![(wert.clone(), k)]).map_err(|e| format!("{e:?}"))?;
        assert_eq!(
            heute.eintrag(&wert).map(depot_kern::Kurs::preis),
            Some(schluss)
        );
        assert_eq!(heute.kurs(&wert), None);
    }
    // Echte Antwort in ganzen Cent: unveraendert.
    let sap = lies_tageskurse(SAP_ECHT).map_err(|e| format!("{e:?}"))?;
    let schluesse: Vec<Option<Cent>> = sap.iter().map(|t| t.schluss.als_cent()).collect();
    assert_eq!(
        schluesse,
        vec![Some(Cent(21_125)), Some(Cent(21_050)), Some(Cent(20_665))]
    );
    Ok(())
}
