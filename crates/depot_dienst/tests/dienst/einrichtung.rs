use super::*;

#[test]
fn formular_entschluesselt_streng() {
    assert_eq!(
        http::formular(b"a=1+2&b=%C3%A4"),
        Some(vec![("a".into(), "1 2".into()), ("b".into(), "ä".into())])
    );
    assert_eq!(http::formular(b"a=%G1"), None);
    assert_eq!(http::formular(b"a=%FF"), None);
    assert!(http::gleich("abc", "abc"));
    assert!(!http::gleich("abc", "abd"));
    assert!(!http::gleich("abc", "abcd"));
}

#[test]
fn schluessel_wird_geprueft_abgelegt_und_nie_gezeigt() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let umw = Attrappe::default();
    let t = donnerstag(12, 0)?;
    let ungueltig = formular_post(
        "/schluessel",
        &format!("t={TOKEN}&wer=alphavantage&wert=kurz"),
    );
    let r = text(&behandle(&d, &umw, &ungueltig, t));
    assert!(r.contains("m=schluessel_ungueltig"));
    let geheim = "Attrappe_Dachs-0123456789_ABCDEFGHIJ";
    let gut = formular_post(
        "/schluessel",
        &format!("t={TOKEN}&wer=anthropic&wert=%20{geheim}%20"),
    );
    let r = text(&behandle(&d, &umw, &gut, t));
    assert!(r.contains("Location: /einrichtung?m=schluessel#schluessel"));
    assert!(!r.contains(geheim));
    assert_eq!(
        *umw.abgelegt.lock().unwrap_or_else(PoisonError::into_inner),
        vec![(Wer::Berater(BeraterId::Dachs), geheim.to_string())]
    );
    let seite = anfrage("GET", "/einrichtung", &[("Host", HOST)], "");
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(!s.contains(geheim), "die Seite zeigt keinen Schluessel");
    assert!(s.contains("hinterlegt"));
    assert!(s.contains("DepotSchlopf/anthropic"));
    assert!(s.contains("type=\"password\""));
    Ok(())
}

#[test]
fn schluesselregeln() {
    assert!(Wer::Kursquelle.pruefe("ABCD1234").is_ok());
    assert!(Wer::Kursquelle.pruefe("abcd1234").is_err());
    assert!(Wer::Kursquelle.pruefe("ABC123").is_err());
    let dachs = Wer::Berater(BeraterId::Dachs);
    assert!(dachs.pruefe("Attrappe_Maus-abcdefghij").is_ok());
    assert!(dachs.pruefe("Attrappe Maus-abcdefghij").is_err());
    assert!(dachs.pruefe("kurz").is_err());
    assert!(
        dachs
            .pruefe(concat!("AQ.", "Erfundener_Gemini_Testwert"))
            .is_err()
    );
    assert!(
        Wer::Gemini
            .pruefe(concat!("AQ.", "Erfundener_Gemini_Testwert"))
            .is_ok()
    );
    assert!(Wer::Gemini.pruefe("AQ.Erfundener.Gemini_Testwert").is_err());
    assert!(Wer::Gemini.pruefe("AQ.Erfundener\r\nX-Header").is_err());
    assert!(dachs.pruefe(&"a".repeat(257)).is_err());
    // Tiingo wie im Sammler: 20 bis 64 Kleinbuchstaben und Ziffern.
    let tiingo = Wer::KursquelleUs;
    assert_eq!(tiingo.adresse(), "DepotSchlopf/tiingo");
    assert!(tiingo.pruefe(&"a1".repeat(20)).is_ok());
    assert!(tiingo.pruefe(&"A1".repeat(20)).is_err());
    assert!(tiingo.pruefe(&"a1".repeat(9)).is_err());
    assert!(tiingo.pruefe(&"a".repeat(65)).is_err());
    assert_eq!(Wer::aus_kurz("tiingo"), Some(tiingo));
    assert_eq!(Wer::aus_kurz("openai"), Some(Wer::Berater(BeraterId::Maus)));
    assert_eq!(Wer::aus_kurz("fremd"), None);
}

#[test]
fn zeitplan_text_streng() -> Result<(), String> {
    let p = Zeitplan::default();
    assert_eq!(Zeitplan::lies(&p.als_text()), Ok(p));
    for kaputt in [
        "zeitplan-v1\nan=ja\ntermine=17:40\nkurse=17:45\n",
        "zeitplan-v1\nan=ja\ntermine=7:40\nkurse=17:45\npapier_danach=ja\n",
        "zeitplan-v1\nan=ja\ntermine=24:00\nkurse=17:45\npapier_danach=ja\n",
        "zeitplan-v1\nan=JA\ntermine=17:40\nkurse=17:45\npapier_danach=ja\n",
        "zeitplan-v1\nan=ja\ntermine=17:40\nkurse=17:45\npapier_danach=ja",
        "zeitplan-v1\r\nan=ja\r\ntermine=17:40\r\nkurse=17:45\r\npapier_danach=ja\r\n",
        "zeitplan-v1\nan=ja\ntermine=17:40\nkurse=17:45\npapier_danach=ja\nmehr\n",
    ] {
        assert!(Zeitplan::lies(kaputt).is_err(), "{kaputt:?}");
    }
    let tag = some(Datum::neu(2026, 11, 5), "Datum")?;
    let e = Erledigt {
        termine: Some(tag),
        kurse: None,
        kurse_nyse: None,
        ..Erledigt::default()
    };
    assert_eq!(Erledigt::lies(&e.als_text()), Ok(e));
    assert!(Erledigt::lies("erledigt-v1\ntermine=2026-13-01\nkurse=\n").is_err());
    Ok(())
}

#[test]
fn zeitplan_faellig_einmal_je_handelstag() -> Result<(), String> {
    let p = Zeitplan {
        an: true,
        ..morgenplan_test()
    };
    let do_ = some(Datum::neu(2026, 11, 5), "Donnerstag")?;
    let sa = some(Datum::neu(2026, 11, 7), "Samstag")?;
    let mo = some(Datum::neu(2026, 11, 9), "Montag")?;
    let leer = Erledigt::default();
    assert_eq!(faellig_x(p, &leer, do_, (7, 19)), None);
    assert_eq!(faellig_x(p, &leer, do_, (7, 20)), Some(Art::Termine));
    assert_eq!(faellig_x(p, &leer, do_, (23, 0)), Some(Art::Termine));
    let termine = Erledigt {
        termine: Some(do_),
        kurse: None,
        kurse_nyse: None,
        ..Erledigt::default()
    };
    assert_eq!(faellig_x(p, &termine, do_, (7, 29)), None);
    assert_eq!(
        faellig_x(p, &termine, do_, (7, 30)),
        Some(Art::KurseVon(Boerse::Xetra))
    );
    let beide = Erledigt {
        termine: Some(do_),
        kurse: Some(do_),
        kurse_nyse: None,
        ..Erledigt::default()
    };
    assert_eq!(faellig_x(p, &beide, do_, (23, 59)), None);
    assert_eq!(faellig_x(p, &leer, sa, (18, 0)), None);
    let aus = Zeitplan { an: false, ..p };
    assert_eq!(faellig_x(aus, &leer, do_, (18, 0)), None);
    assert_eq!(naechster_x(aus, &leer, do_), None);
    assert_eq!(
        naechster_x(p, &beide, do_),
        Some((
            Art::Termine,
            some(Datum::neu(2026, 11, 6), "Freitag")?,
            (7, 20)
        ))
    );
    let freitag_fertig = Erledigt {
        termine: some(Datum::neu(2026, 11, 6), "Freitag")?.into(),
        kurse: some(Datum::neu(2026, 11, 6), "Freitag")?.into(),
        kurse_nyse: None,
        ..Erledigt::default()
    };
    assert_eq!(
        naechster_x(
            p,
            &freitag_fertig,
            some(Datum::neu(2026, 11, 6), "Freitag")?
        ),
        Some((Art::Termine, mo, (7, 20)))
    );
    Ok(())
}

/// Die festgelegte Liste hat Werte von Xetra und NYSE: Der Zeitplan ruft beide ab, die NYSE
/// nach ihrem Schluss, und kennt keinen weiteren Lauf, wenn beide erledigt sind.
#[test]
fn zeitplan_der_liste_ruft_xetra_und_nyse_ab() -> Result<(), String> {
    assert_eq!(
        depot_kern::liste::boersen(),
        vec![Boerse::Xetra, Boerse::Nyse]
    );
    let p = Zeitplan {
        an: true,
        ..morgenplan_test()
    };
    let do_ = some(Datum::neu(2026, 11, 5), "Donnerstag")?;
    let xetra = Erledigt {
        termine: Some(do_),
        kurse: Some(do_),
        ..Erledigt::default()
    };
    assert_eq!(faellig(&p, &xetra, do_, (7, 29)), None);
    assert_eq!(
        faellig(&p, &xetra, do_, (7, 30)),
        Some(Art::KurseVon(Boerse::Nyse))
    );
    assert_eq!(
        naechster(&p, &xetra, do_),
        Some((Art::KurseVon(Boerse::Nyse), do_, (7, 30)))
    );
    let alle = Erledigt {
        kurse_nyse: Some(do_),
        ..xetra
    };
    assert_eq!(faellig(&p, &alle, do_, (23, 59)), None);
    Ok(())
}

/// An Feiertagen aller Boersen (hier nur Xetra) laeuft nichts; der naechste Lauf ist der naechste Tag,
/// an dem gehandelt wird.
#[test]
fn zeitplan_ruht_an_boersenfeiertagen() -> Result<(), String> {
    let p = Zeitplan {
        an: true,
        ..morgenplan_test()
    };
    let leer = Erledigt::default();
    let heiligabend = some(Datum::neu(2026, 12, 24), "Heiligabend")?;
    assert_eq!(faellig_x(p, &leer, heiligabend, (18, 0)), None);
    let karfreitag = some(Datum::neu(2026, 4, 3), "Karfreitag")?;
    assert_eq!(faellig_x(p, &leer, karfreitag, (18, 0)), None);
    let d23 = some(Datum::neu(2026, 12, 23), "23.12.")?;
    let fertig = Erledigt {
        termine: Some(d23),
        kurse: Some(d23),
        kurse_nyse: None,
        ..Erledigt::default()
    };
    assert_eq!(
        naechster_x(p, &fertig, d23),
        Some((
            Art::Termine,
            some(Datum::neu(2026, 12, 28), "28.12.")?,
            (7, 20)
        ))
    );
    Ok(())
}

#[test]
fn zeitplan_startet_termine_kurse_und_danach_papier() -> Result<(), String> {
    let wurzel = eingeschaltet(ordner()?)?;
    let d = dienst(wurzel.clone(), true);
    d.sperre().boersen = vec![Boerse::Xetra];
    // 07:31: beide faellig, Termine zuerst.
    let t = donnerstag(7, 31)?;
    let (nr, kind) = some(d.schritt(t), "Termine")?;
    assert_eq!(
        d.sperre().laufend.as_ref().map(|a| (a.art, a.anlass)),
        Some((Art::Termine, Anlass::Zeitplan))
    );
    d.verfolge(nr, kind, || donnerstag(7, 31).ok());
    let (nr, kind) = some(d.schritt(t), "Kurse")?;
    d.verfolge(nr, kind, || donnerstag(7, 32).ok());
    assert_eq!(wartend(&d), vec![(Art::Papier, Anlass::Zeitplan)]);
    let (nr, kind) = some(d.schritt(t), "Papier")?;
    d.verfolge(nr, kind, || donnerstag(7, 33).ok());
    assert!(d.schritt(t).is_none(), "heute nichts mehr");
    // Ein Neustart startet heute nichts noch einmal.
    let erledigt = fs::read_to_string(wurzel.join("daten").join("dienst_erledigt.txt"))
        .map_err(|e| format!("Erledigt fehlt: {e}"))?;
    let gelesen = Erledigt::lies(&erledigt)?;
    assert_eq!(gelesen.papier, t.frankfurt_tag());
    assert!(gelesen.erfolgreich(Boerse::Xetra, some(t.frankfurt_tag(), "Tag")?));
    let neu = dienst(wurzel, true);
    assert!(neu.schritt(donnerstag(20, 0)?).is_none());
    assert!(wartend(&neu).is_empty());
    Ok(())
}

#[test]
fn unlesbarer_zeitplan_schaltet_ihn_aus() -> Result<(), String> {
    let wurzel = ordner()?;
    fs::write(wurzel.join("daten").join("zeitplan.txt"), "an=ja\n")
        .map_err(|e| format!("Vorbedingung: {e}"))?;
    let d = dienst(wurzel.clone(), true);
    assert!(!d.sperre().zeitplan.an);
    assert!(d.sperre().zeitplan_fehler.is_some());
    assert!(d.schritt(donnerstag(18, 0)?).is_none());
    assert!(wartend(&d).is_empty());
    // Dasselbe bei unlesbarem Erledigt: Es koennte sonst zweimal am Tag starten.
    let w2 = ordner()?;
    fs::write(
        w2.join("daten").join("dienst_erledigt.txt"),
        "erledigt-v1\n",
    )
    .map_err(|e| format!("Vorbedingung: {e}"))?;
    let d2 = dienst(w2, true);
    assert!(!d2.sperre().zeitplan.an);
    assert!(d2.schritt(donnerstag(18, 0)?).is_none());
    Ok(())
}

/// Maus K29-01: Ein Lesefehler ist kein Erststart. Nur eine fehlende Datei nimmt die
/// Voreinstellung; jeder andere Fehler haelt den Zeitplan sichtbar an.
#[test]
fn k29_01_lesefehler_halten_den_zeitplan_an() -> Result<(), String> {
    let umw = Attrappe::default();
    // Zeitplan nicht lesbar (hier ein Ordner an seiner Stelle): nichts startet, Grund sichtbar.
    let w1 = ordner()?;
    fs::create_dir_all(w1.join("daten").join("zeitplan.txt"))
        .map_err(|e| format!("Vorbedingung: {e}"))?;
    let d1 = dienst(w1, true);
    assert!(!d1.sperre().zeitplan.an);
    assert!(d1.sperre().zeitplan_fehler.is_some());
    assert!(d1.schritt(donnerstag(18, 0)?).is_none());
    assert!(wartend(&d1).is_empty());
    let seite = text(&behandle(
        &d1,
        &umw,
        &anfrage("GET", "/einrichtung", &[("Host", HOST)], ""),
        donnerstag(18, 0)?,
    ));
    assert!(seite.contains("nicht lesbar"), "Grund sichtbar");
    // Erledigt nicht lesbar (kein UTF-8): nichts startet, auch nicht nach neuem Speichern.
    let w2 = ordner()?;
    fs::write(
        w2.join("daten").join("dienst_erledigt.txt"),
        [0xff_u8, 0xfe, 0x00],
    )
    .map_err(|e| format!("Vorbedingung: {e}"))?;
    // Mit fertiger Einrichtung, damit das Einschalten selbst erlaubt ist.
    let d2 = Arc::new(Dienst::neu(
        w2,
        TOKEN.into(),
        echo_starter(true),
        lagebild_mit_schluesseln(true),
    ));
    assert!(!d2.sperre().zeitplan.an);
    assert!(d2.sperre().erledigt_fehler.is_some());
    assert!(d2.schritt(donnerstag(17, 59)?).is_none());
    let an = formular_post("/zeitplan", &format!("t={TOKEN}&aktion=an"));
    assert!(
        text(&behandle(&d2, &umw, &an, donnerstag(18, 0)?)).contains("m=zeitplan_einrichtung#")
    );
    let seite = get(&d2, "/einrichtung", donnerstag(18, 0)?);
    assert!(seite.contains("<b>angehalten</b>"));
    assert!(seite.contains("disabled>Speichern und Zeitplan einschalten"));
    assert!(!d2.sperre().zeitplan.an);
    assert!(d2.schritt(donnerstag(18, 0)?).is_none());
    assert!(
        wartend(&d2).is_empty(),
        "ohne gesichertes Erledigt startet nichts"
    );
    // Fehlen beide Dateien, gilt die Voreinstellung (Erststart): Zeitplan aus, ohne Fehler.
    // Ein erster neugieriger Start loest nichts aus, auch keinen KI-Aufruf.
    let d3 = dienst(ordner()?, true);
    assert!(!d3.sperre().zeitplan.an);
    assert!(d3.sperre().zeitplan_fehler.is_none());
    assert!(d3.schritt(donnerstag(18, 0)?).is_none());
    assert!(wartend(&d3).is_empty());
    assert!(d3.sperre().verlauf.is_empty());
    Ok(())
}

#[test]
fn zeitplan_ueber_die_seite_speichern_und_aktivieren() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let umw = Attrappe::default();
    let t = donnerstag(12, 0)?;
    let ungueltig = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&aktion=speichern&termine=8:00&kurse=18:00"),
    );
    assert!(text(&behandle(&d, &umw, &ungueltig, t)).contains("m=zeitplan_ungueltig"));
    assert_eq!(d.sperre().zeitplan, Zeitplan::default());
    let a = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&aktion=speichern&termine=08%3A00&kurse=08%3A05"),
    );
    assert!(text(&behandle(&d, &umw, &a, t)).contains("m=zeitplan#zeitplan"));
    let erwartet = Zeitplan {
        an: false,
        termine: (8, 0),
        kurse: (8, 5),
        papier_danach: false,
        nachrichten_an: false,
        vorbereitung: depot_dienst::zeitplan::VorbereitungsPlan::Aus,
        hinweis: depot_dienst::zeitplan::HinweisPlan::Aus,
    };
    assert_eq!(d.sperre().zeitplan, erwartet);
    let datei = fs::read_to_string(wurzel.join("daten").join("zeitplan.txt"))
        .map_err(|e| format!("Datei fehlt: {e}"))?;
    assert_eq!(Zeitplan::lies(&datei), Ok(erwartet));
    let seite = text(&behandle(
        &d,
        &umw,
        &anfrage("GET", "/einrichtung", &[("Host", HOST)], ""),
        t,
    ));
    assert!(seite.contains("Einstellungen gespeichert."));
    Ok(())
}

#[test]
fn neuer_schluessel_ist_wieder_nur_hinterlegt() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let umw = Attrappe::default();
    let t = donnerstag(12, 0)?;
    // Der Anbieter hat Maus' alten Schluessel abgelehnt.
    let alt = depot_lage::anbieter::Eintrag::neu(
        donnerstag(11, 0)?,
        depot_lage::anbieter::Wer::Maus,
        depot_lage::anbieter::Ergebnis::Abgelehnt,
        Some(401),
    );
    let datei = wurzel.join("daten").join(depot_lage::anbieter::DATEI);
    depot_lage::anbieter::anhaengen(&datei, &alt)?;
    let seite = anfrage("GET", "/einrichtung", &[("Host", HOST)], "");
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(s.contains("abgelehnt (05.11.2026 11:00"), "{s}");
    let neu = formular_post(
        "/schluessel",
        &format!("t={TOKEN}&wer=openai&wert=Attrappe_Maus-0123456789_ABCDEFGHIJ"),
    );
    let r = text(&behandle(&d, &umw, &neu, t));
    assert!(
        r.contains("Location: /einrichtung?m=schluessel#schluessel"),
        "{r}"
    );
    let zeilen = protokoll_text(&wurzel);
    assert_eq!(zeilen.lines().count(), 2, "{zeilen}");
    assert!(
        zeilen
            .lines()
            .last()
            .is_some_and(|z| z.contains(";maus;neu;")),
        "{zeilen}"
    );
    assert!(!zeilen.contains("Attrappe"), "kein Schluessel im Protokoll");
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(!s.contains("abgelehnt (05.11.2026 11:00"), "{s}");
    assert!(s.contains("hinterlegt, noch nicht benutzt"), "{s}");
    Ok(())
}

#[test]
fn erste_schritte_zeigen_bereits_angenommene_zugangsdaten() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let t = donnerstag(12, 0)?;
    let datei = wurzel.join("daten").join(depot_lage::anbieter::DATEI);
    for wer in depot_dienst::schluessel::ALLE {
        let eintrag = depot_lage::anbieter::Eintrag::neu(
            t,
            wer.protokoll(),
            depot_lage::anbieter::Ergebnis::Ok,
            Some(200),
        );
        depot_lage::anbieter::anhaengen(&datei, &eintrag)?;
    }
    let seite = get(&d, "/einrichtung", t);
    assert!(seite.contains("alle Schlüssel wurden vom jeweiligen Anbieter angenommen"));
    assert!(seite.contains("geprüft (angenommen"));
    Ok(())
}

#[test]
fn gesamtstatus_ergaenzt_den_zeitplan() -> Result<(), String> {
    let mit = |als_bereit| {
        Ok::<_, String>(Arc::new(Dienst::neu(
            ordner()?,
            TOKEN.into(),
            echo_starter(true),
            lagebild_mit_schluesseln(als_bereit),
        )))
    };
    let t = donnerstag(12, 0)?;
    let heute = t.datum();
    let live = |d: &Dienst| seiten::live(TOKEN, "s", &d.sperre(), t);
    // Maus zu #31: Alle Schluessel da, aber kein Journal: nie „vollständig bereit“, auch mit
    // eingeschaltetem Zeitplan.
    let d = mit(false)?;
    assert!(d.schritt(t).is_none());
    d.sperre().zeitplan.an = true;
    let s = live(&d);
    assert!(
        s.contains("Eingriff nötig") && s.contains("Journal: Noch nicht eingerichtet"),
        "{s}"
    );
    assert!(!s.contains("Vollständig bereit"), "{s}");
    let d = mit(true)?;
    assert!(d.schritt(t).is_none());
    if let Some(lage) = d.sperre().lage.as_mut() {
        lage.bild.betrieb = depot_lage::Betrieb::OhneKi(vec!["Maus: kein Schlüssel.".into()]);
    }
    let s = live(&d);
    assert!(
        s.contains(">Läuft ohne KI<") && s.contains("Automatik: Zeitplan aus"),
        "{s}"
    );
    assert!(s.contains("Maus: kein Schlüssel."), "{s}");
    let d = mit(true)?;
    assert!(d.schritt(t).is_none());
    // Ab Werk ist der Zeitplan aus: Nichts laeuft von selbst.
    assert!(!d.sperre().zeitplan.an);
    assert!(live(&d).contains(">Pausiert<"), "{}", live(&d));
    d.sperre().zeitplan.an = true;
    assert!(live(&d).contains("Vollständig bereit"), "{}", live(&d));
    d.sperre().zeitplan.an = false;
    let s = live(&d);
    assert!(
        s.contains(">Pausiert<") && s.contains("Zeitplan aus"),
        "{s}"
    );
    d.sperre().zeitplan_fehler = Some("Zeitplan: Kopfzeile fehlt".into());
    let s = seiten::status(TOKEN, "s", &d.sperre(), heute);
    assert!(
        s.contains("Eingriff nötig") && s.contains("Kopfzeile fehlt"),
        "{s}"
    );
    // Ohne Schluessel: Die Kursquelle braucht einen Eingriff, die KIs stehen dahinter.
    let d = dienst(ordner()?, true);
    assert!(d.schritt(t).is_none());
    let s = live(&d);
    assert!(
        s.contains("Eingriff nötig") && s.contains("kein Schlüssel, wird nicht gefragt"),
        "{s}"
    );
    Ok(())
}

#[test]
fn papierfreigabe_ueber_einrichtung_schreibt_bestaetigten_stand() -> Result<(), String> {
    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    // Kanonisches Journal mit zwei Papier-Toepfen, Gesamtverlustsperre und Tagesabschluss.
    for (name, inhalt) in [
        (
            "journal.txt",
            concat!(
                "10;277cd26269b17f0a01f2d37944a7b625b839c8e433f0a1c7b8de229fc619ca67;journal-v2\n",
                "14;7b07d15e2cdbee2e29d32e6c3aaf5b49ff371bee867e3cf54d435e3ded8b6b03;topf;A;500000;\n",
                "14;fe147cc4e9a53116352cd71f6356db399e4320dd5ed3e60f74189c928f5b6871;topf;B;500000;\n",
                "25;284202d38e87a7d710bea9c848eeb6f46ddd11bccfa81fa08cafc800d72726b2;abrechnungstag;2026-11-04\n",
                "36;ecfa1ce0c24ac8bed0ccce2b440f8bf8c4c3454d579131113d54498cf7bc55fc;papier-halt;2026-11-04;gesamtverlust\n",
                "93;5e56b0f77370678f32322b1fe88395cad52a46b2a9bc91369339af7092cd38e4;abschluss;2026-11-04;1500000;aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
            ),
        ),
        (
            "journal.vermerk",
            "vermerk-v1;606;5e56b0f77370678f32322b1fe88395cad52a46b2a9bc91369339af7092cd38e4;0\n",
        ),
        (
            "papierkonto.txt",
            concat!("papierkonto-v1\n", "bargeld;1000000\n", "sendungen;0\n",),
        ),
    ] {
        fs::write(daten.join(name), inhalt).map_err(|e| e.to_string())?;
    }
    let d = dienst(wurzel.clone(), true);
    let t = donnerstag(12, 0)?;
    assert!(get(&d, "/einrichtung", t).contains("33,33 %"));
    let angebot = some(
        depot_lage::papier_freigabe_angebot(&wurzel),
        "Freigabeangebot",
    )?;
    let umw = Attrappe {
        papierfreigabe: true,
        ..Attrappe::default()
    };
    let formular = |abdruck: &str| {
        formular_post(
            "/papierfreigabe",
            &format!("t={TOKEN}&geprueft=ja&abdruck={abdruck}"),
        )
    };
    let r = |a: &Anfrage, umwelt: &Attrappe| text(&behandle(&d, umwelt, a, t));
    assert!(r(&formular(&"a".repeat(64)), &umw).contains("papierfreigabe_geaendert"));
    let anfrage = formular(&angebot.abdruck);
    assert!(r(&anfrage, &Attrappe::default()).contains("papierfreigabe_unbestaetigt"));
    let antwort = r(&anfrage, &umw);
    assert!(antwort.contains("papierfreigabe_ok"), "{antwort}");
    assert!(depot_lage::papier_freigabe_angebot(&wurzel).is_none());
    let journal = fs::read_to_string(daten.join("journal.txt")).map_err(|e| e.to_string())?;
    assert!(journal.contains(concat!(
        "papier-freigabe;2026-11-04;1000000;einrichtung;dialog;",
        "2026-11-05T11:01:23Z"
    )));
    assert!(depot_lage::mit_journal(&daten, |zugang| {
        let Zugang::Offen(stand) = zugang else {
            return false;
        };
        stand.begruendungen().iter().any(|(_, wer, text)| {
            wer == "Einrichtung, Dialog" && text.contains("2026-11-05T11:01:23Z")
        })
    }));
    Ok(())
}

#[test]
fn zeitplan_einschalten_erst_nach_vollstaendiger_einrichtung() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let umw = Attrappe::default();
    let t = donnerstag(12, 0)?;
    let an = formular_post("/zeitplan", &format!("t={TOKEN}&aktion=an"));
    // Noch kein Lagebild: nicht eingeschaltet.
    let r = text(&behandle(&d, &umw, &an, t));
    assert!(r.contains("m=zeitplan_lage"), "{r}");
    assert!(!d.sperre().zeitplan.an);
    // Lagebild da, aber Eingriff noetig (kein Journal, keine Schluessel).
    assert!(d.schritt(t).is_none());
    let r = text(&behandle(&d, &umw, &an, t));
    assert!(r.contains("m=zeitplan_einrichtung"), "{r}");
    assert!(!d.sperre().zeitplan.an);
    assert!(
        !wurzel.join("daten").join("zeitplan.txt").exists(),
        "nichts gespeichert"
    );
    // Die Seite erklärt, warum der Einschaltknopf gesperrt ist.
    let seite = get(&d, &ort(&r), t);
    assert!(seite.contains("Zeitplan nicht eingeschaltet"), "{seite}");
    assert!(
        seite.contains("disabled>Speichern und Zeitplan einschalten"),
        "{seite}"
    );
    assert!(seite.contains("Einschalten geht erst"), "{seite}");
    // Ohne Entwurf zeigt die Seite den aktiven Plan.
    let seite = get(&d, "/einrichtung", t);
    assert!(seite.contains("value=\"07:20\""), "{seite}");
    // Uhrzeiten aendern ohne Einschalten geht immer.
    let aus = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&aktion=speichern&termine=08%3A15&kurse=08%3A05"),
    );
    assert!(text(&behandle(&d, &umw, &aus, t)).contains("m=zeitplan#"));
    // Mit fertiger Einrichtung geht das Einschalten.
    let fertig = Arc::new(Dienst::neu(
        ordner()?,
        TOKEN.into(),
        echo_starter(true),
        lagebild_mit_schluesseln(true),
    ));
    assert!(fertig.schritt(t).is_none());
    assert!(get(&fertig, "/einrichtung", t).contains("Einrichtung – Schritt für Schritt"));
    assert!(text(&behandle(&fertig, &umw, &an, t)).contains("m=zeitplan_an#"));
    assert!(fertig.sperre().zeitplan.an);
    let seite = get(&fertig, "/einrichtung", t);
    assert!(!seite.contains("Einrichtung – Schritt für Schritt"));
    let selten = some(seite.split_once("id=\"selten\""), "Seltene Einstellungen")?.1;
    assert!(selten.contains("id=\"schluessel\""));
    assert!(text(&behandle(&fertig, &umw, &aus, t)).contains("m=zeitplan#"));
    assert!(fertig.sperre().zeitplan.an, "Speichern schaltet nicht aus");
    let altes_formular = formular_post("/zeitplan", &format!("t={TOKEN}&an=ja"));
    assert!(text(&behandle(&fertig, &umw, &altes_formular, t)).contains("m=zeitplan_neu_laden"));
    let ausschalten = formular_post("/zeitplan", &format!("t={TOKEN}&aktion=aus"));
    assert!(text(&behandle(&fertig, &umw, &ausschalten, t)).contains("m=zeitplan_aus#"));
    let plan = fertig.sperre().zeitplan;
    assert!(!plan.an);
    assert_eq!(plan.termine, (8, 15));
    assert_eq!(plan.kurse, (8, 5));
    Ok(())
}

#[test]
fn zeitplanformular_bewahrt_entwurf_bei_aenderung_im_anderen_tab() -> Result<(), String> {
    let d = Dienst::neu(
        ordner()?,
        TOKEN.into(),
        echo_starter(true),
        lagebild_mit_schluesseln(true),
    );
    let t = donnerstag(12, 0)?;
    assert!(d.schritt(t).is_none());
    let formularstand = |html: &str| {
        html.split_once("name=\"planstand\" value=\"")
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(stand, _)| stand.to_owned())
            .ok_or("Zeitplanstand fehlt")
    };
    let seite = get(&d, "/einrichtung", t);
    let alter_stand = formularstand(&seite)?;
    let nur_speichern = some(seite.find("value=\"speichern\""), "Nur speichern")?;
    let einschalten = some(seite.find("value=\"speichern_an\""), "Einschalten")?;
    assert!(
        nur_speichern < einschalten,
        "Enter darf den Zeitplan nicht einschalten"
    );
    let umw = Attrappe::default();
    let an = formular_post("/zeitplan", &format!("t={TOKEN}&aktion=an"));
    assert!(text(&behandle(&d, &umw, &an, t)).contains("m=zeitplan_an"));
    let vorher = d.sperre().zeitplan;
    let speichern = |stand: &str| {
        let post = formular_post(
            "/zeitplan",
            &format!("t={TOKEN}&aktion=speichern&planstand={stand}&termine=08%3A15&kurse=08%3A05"),
        );
        text(&behandle(&d, &umw, &post, t))
    };
    let antwort = speichern(&alter_stand);
    assert!(antwort.contains("m=zeitplan_stand_geaendert"));
    assert_eq!(d.sperre().zeitplan, vorher);
    let seite = get(&d, &ort(&antwort), t);
    let stand_nach_schalterwechsel = formularstand(&seite)?;
    assert_ne!(stand_nach_schalterwechsel, alter_stand);
    assert!(seite.contains("Der Zeitplan bleibt eingeschaltet."));
    assert!(!seite.contains("Nur speichern"));
    assert!(seite.contains("value=\"08:05\""));
    assert!(seite.contains("Deine Eingaben stehen als Entwurf"));
    assert!(seite.contains("href=\"#zeitplan\">Zeitplan und Eingaben ansehen"));
    assert!(speichern(&stand_nach_schalterwechsel).contains("m=zeitplan#"));
    let nachher = d.sperre().zeitplan;
    assert!(nachher.an);
    assert_eq!(nachher.kurse, (8, 5));
    // Auch neue Uhrzeiten bei unverändertem Schalterstand bleiben erhalten.
    let antwort = speichern(&stand_nach_schalterwechsel);
    assert!(antwort.contains("m=zeitplan_stand_geaendert"));
    assert_eq!(d.sperre().zeitplan, nachher);
    assert!(
        get(&d, &ort(&antwort), t).contains("Gespeicherte Uhrzeiten: Termine 08:15, Kurse 08:05.")
    );
    Ok(())
}

#[test]
fn kombiniertes_einschalten_bleibt_bei_fehlender_einrichtung_ohne_wirkung() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let jetzt = donnerstag(12, 0)?;
    assert!(d.schritt(jetzt).is_none());
    let formular = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&aktion=speichern_an&termine=08%3A15&kurse=08%3A05"),
    );
    let antwort = text(&behandle(&d, &Attrappe::default(), &formular, jetzt));
    assert!(antwort.contains("m=zeitplan_einrichtung"));
    assert!(!d.sperre().zeitplan.an);
    assert!(!wurzel.join("daten/zeitplan.txt").exists());
    let html = get(&d, &ort(&antwort), jetzt);
    assert!(html.contains("value=\"08:05\""));
    assert!(html.contains("disabled>Speichern und Zeitplan einschalten"));
    assert!(html.contains("Du kannst die Einstellungen schon speichern."));
    assert!(!html.contains("wähle unten <b>Speichern und Zeitplan einschalten</b>"));
    Ok(())
}

/// Maus zu #32: „Läuft ohne KI“ reicht zum Einschalten nicht; jeder Schluessel wird
/// ausdruecklich geprueft, und die Seite nennt, was fehlt.
#[test]
fn einschalten_verlangt_jeden_schluessel() -> Result<(), String> {
    let ohne_maus: Lagerechner = Box::new(|_wurzel: &Path, jetzt: Zeitpunkt| {
        let mut l = lagebild(&Umgebung {
            jetzt,
            journal: Zugang::NichtEingerichtet,
            kursquelle: Hinterlegt::Ja,
            kursquelle_us: Hinterlegt::Ja,
            berater: [
                (BeraterId::Dachs, Hinterlegt::Ja),
                (BeraterId::Maus, Hinterlegt::Nein),
            ],
            sperren: Vec::new(),
            anbieter: Vec::new(),
            anbieter_unlesbar: 0,
            ki_halt: None,
        });
        // Wie mit eingerichtetem Journal: nur Maus fehlt.
        l.betrieb = depot_lage::Betrieb::OhneKi(vec!["Maus (GPT): kein Schlüssel".into()]);
        l
    });
    let d = Arc::new(Dienst::neu(
        ordner()?,
        TOKEN.into(),
        echo_starter(true),
        ohne_maus,
    ));
    let t = donnerstag(12, 0)?;
    assert!(d.schritt(t).is_none());
    let an = formular_post("/zeitplan", &format!("t={TOKEN}&aktion=an"));
    let r = text(&behandle(&d, &Attrappe::default(), &an, t));
    assert!(r.contains("m=zeitplan_einrichtung"), "{r}");
    assert!(!d.sperre().zeitplan.an);
    let seite = get(&d, "/einrichtung", t);
    assert!(
        seite.contains("<li>Maus (GPT): Schlüssel fehlt</li>"),
        "{seite}"
    );
    assert!(!seite.contains("<li>Dachs (Claude)"), "{seite}");
    Ok(())
}

#[test]
fn fehlender_abschlussvermerk_zeigt_hinweis_und_haelt_zeitplan_nicht_an() -> Result<(), String> {
    let wurzel = ordner()?;
    let datei = depot_dienst::verlauf::pfad(&wurzel);
    let sicherung = datei.with_extension("test-sicherung");
    let d = dienst(wurzel, true);
    d.sperre().zeitplan.an = true;
    let t = donnerstag(12, 0)?;
    d.einreihen(Art::Termine, Anlass::Hand)?;
    let (nr, kind) = some(d.schritt(t), "Terminabruf startet")?;
    fs::rename(&datei, &sicherung).map_err(|e| e.to_string())?;
    fs::create_dir(&datei).map_err(|e| e.to_string())?;
    d.verfolge(nr, kind, || donnerstag(12, 1).ok());

    let seite = get(&d, "/teil/live", t);
    assert!(seite.contains("nicht im Verlauf vermerkt"));
    assert!(seite.contains("Geplante Startzeiten in Frankfurter Ortszeit"));
    assert!(!seite.contains("Zeitplan angehalten"));
    assert!(get(&d, "/einrichtung", t).contains("<b>an</b>"));

    fs::remove_dir(&datei).map_err(|e| e.to_string())?;
    fs::rename(&sicherung, &datei).map_err(|e| e.to_string())?;
    d.einreihen(Art::Kurse, Anlass::Hand)?;
    let (nr, kind) = some(d.schritt(t), "weiterer Abruf startet")?;
    assert!(d.sperre().verlauf_vermerkfehler.is_none());
    d.verfolge(nr, kind, || donnerstag(12, 2).ok());
    Ok(())
}

/// Maus zu KI-Aufrufen: Die Einrichtung erklaert in Alltagssprache den Unterschied zwischen
/// einer Anfrage, die nie hinausging (wird wiederholt, kostet nichts), und einer, die nach dem
/// Absenden abbrach (Kostensperre, nie wiederholt). Mit und ohne Sperre.
#[test]
fn einrichtung_erklaert_abgebrochene_ki_anfragen() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let t = donnerstag(18, 0)?;
    let ohne = get(&d, "/einrichtung", t);
    assert!(ohne.contains("gar nicht erst hinausgegangen"), "{ohne}");
    assert!(
        ohne.contains("Ein zweiter Versuch könnte doppelt kosten"),
        "{ohne}"
    );
    fs::write(
        kostenschutz::pfad(&wurzel),
        "Dachs am Kostenschutz gesperrt: Verbindung nach dem Absenden abgebrochen\n",
    )
    .map_err(|e| format!("Vorbedingung Sperre: {e}"))?;
    let mit = get(&d, "/einrichtung", t);
    assert!(
        mit.contains("Verbindung nach dem Absenden abgebrochen"),
        "{mit}"
    );
    assert!(mit.contains("id=\"ki-verbindung\""), "{mit}");
    Ok(())
}

/// Maus, Befund 3 zu #38: Ob es nach einem Ausfall von selbst weitergeht, sagt der Dienst aus
/// seinem tatsaechlichen Zeitplan, nicht der Sammler. Ohne Zeitplan: nichts von selbst; mit
/// Zeitplan: der naechste geplante Lauf. Bei unlesbarem Erledigt-Stand bleibt der Zeitplan
/// auch nach dem Einschalten angehalten.
#[test]
fn nach_einem_ausfall_sagt_der_zeitplan_wie_es_weitergeht() -> Result<(), String> {
    let zeilen: &'static [&'static str] =
        &["[Hinweis] Papiertag heute ausgefallen. Die Verbindung kam bis 17:54 Uhr nicht wieder."];
    let t = donnerstag(17, 45)?;
    for (an, angehalten) in [(false, false), (true, false), (true, true)] {
        let wurzel = if an {
            eingeschaltet(ordner()?)?
        } else {
            ordner()?
        };
        if angehalten {
            fs::write(
                wurzel.join("daten").join("dienst_erledigt.txt"),
                b"unlesbar",
            )
            .map_err(|e| format!("Vorbedingung Erledigt-Stand: {e}"))?;
        }
        let d = Arc::new(Dienst::neu(
            wurzel,
            TOKEN.into(),
            zeilen_starter(zeilen),
            lagebild_mit_schluesseln(true),
        ));
        if angehalten {
            assert!(d.schritt(t).is_none());
            // Simuliert einen bereits aktiven Zeitplan mit beschädigtem Laufstand.
            d.sperre().zeitplan.an = true;
        }
        d.einreihen(Art::Kurse, Anlass::Hand)
            .map_err(|e| format!("Vorbedingung einreihen: {e}"))?;
        let (nr, kind) = some(d.schritt(t), "Start")?;
        d.verfolge(nr, kind, || donnerstag(17, 55).ok());
        let seite = seiten::live(TOKEN, "s", &d.sperre(), t);
        let (_, rest) = some(seite.split_once("id=\"hinweis\">"), "Hinweis")?;
        let (hinweis, _) = some(rest.split_once("</p>"), "Ende")?;
        if angehalten {
            assert!(hinweis.contains("Zeitplan angehalten"), "{hinweis}");
            assert!(hinweis.contains("Dienst neu starten"), "{hinweis}");
            assert!(!hinweis.contains("geht es von selbst"), "{hinweis}");
        } else if an {
            assert!(
                hinweis.contains("Nach Zeitplan geht es von selbst weiter"),
                "{hinweis}"
            );
        } else {
            assert!(
                hinweis.contains("DepotSchlopf startet nichts von selbst"),
                "{hinweis}"
            );
            assert!(!hinweis.contains("geht es von selbst"), "{hinweis}");
        }
    }
    Ok(())
}

#[test]
fn wartender_papiertag_startet_bei_ausgeschaltetem_zeitplan_nicht() -> Result<(), String> {
    let d = nur_xetra(eingeschaltet(ordner()?)?);
    let t = donnerstag(7, 31)?;
    let (nr, kind) = some(d.schritt(t), "Termine")?;
    d.verfolge(nr, kind, || donnerstag(7, 31).ok());
    let (nr, kind) = some(d.schritt(t), "Kurse")?;
    d.verfolge(nr, kind, || donnerstag(7, 32).ok());
    assert_eq!(wartend(&d), vec![(Art::Papier, Anlass::Zeitplan)]);
    d.setze_zeitplan(Zeitplan::default())
        .map_err(|e| format!("Zeitplan ausschalten: {e:?}"))?;
    assert!(d.schritt(donnerstag(7, 33)?).is_none());
    assert!(wartend(&d).is_empty());
    assert_eq!(d.sperre().erledigt.papier, None);
    Ok(())
}

#[test]
fn zeitplan_ohne_hinweisfeld_heisst_an() -> Result<(), String> {
    let alt = "zeitplan-v1\nan=ja\ntermine=17:40\nkurse=17:45\npapier_danach=ja\n";
    assert_eq!(
        Zeitplan::lies(alt)?.hinweis,
        depot_dienst::zeitplan::HinweisPlan::An
    );
    let aus = Zeitplan {
        hinweis: depot_dienst::zeitplan::HinweisPlan::Aus,
        ..Zeitplan::default()
    };
    assert!(aus.als_text().ends_with("hinweis_aus=ja\n"));
    assert_eq!(Zeitplan::lies(&aus.als_text())?, aus);
    assert!(Zeitplan::lies(&(aus.als_text() + "hinweis_aus=ja\n")).is_err());
    Ok(())
}

#[test]
fn einrichtung_zeigt_grenzen_und_gemini_filter_vor_dem_ersten_aufruf() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let zeit = donnerstag(17, 39)?;
    let seite = get(&d, "/einrichtung", zeit);
    let journal = some(seite.find("id=\"journal\""), "Journal")?;
    let zugaenge = some(seite.find("id=\"schluessel\""), "Zugangsdaten")?;
    let selten = some(seite.find("id=\"selten\""), "Seltene Einstellungen")?;
    let zusatz = some(seite.find("id=\"zusatz\""), "Zusatz-KIs")?;
    let dienst = some(seite.find("<h2>Dienst</h2>"), "Dienst")?;
    assert!(journal < zugaenge && zugaenge < selten && selten < zusatz && zusatz < dienst);
    for text in [
        "Kursquelle Alpha Vantage (Xetra)",
        "Kursquelle Tiingo (US-Börsen)",
        "Papier-Sperre bei Gesamtverlust</td><td class=\"z\">30 %",
        "Kurshöchstalter",
        "Maximaler Kurssprung",
        "Maximaler Limitabstand",
        "Papiergebühr je Order",
        "Gemini-Aufrufe je Tag",
    ] {
        assert!(seite.contains(text), "{text}");
    }
    assert!(!get(&d, "/gespraeche", zeit).contains("<option value=\"gemini\""));
    fs::write(
        depot_lage::gespraeche::gemini_pfad(&d.wurzel.join("daten")),
        "gratisprojekt=ja\n",
    )
    .map_err(|e| e.to_string())?;
    assert!(get(&d, "/gespraeche", zeit).contains("<option value=\"gemini\">Gemini"));
    Ok(())
}

#[test]
fn gemini_speichern_uebernimmt_schluessel_aus_der_einrichtung() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(17, 50)?;
    let umwelt = Attrappe {
        gemini_speicherprobe: true,
        ..Attrappe::default()
    };
    let einrichtung = |pfad: &str| {
        text(&behandle(
            &d,
            &umwelt,
            &anfrage("GET", pfad, &[("Host", HOST)], ""),
            t,
        ))
    };
    let vorher = einrichtung("/einrichtung");
    assert!(vorher.contains(">fehlt</span>"));
    assert!(vorher.contains(">Gemini-Schlüssel speichern</button>"));

    let probe = concat!("AQ.", "Nur_Ein_Erfundener_Testwert");
    let post = formular_post("/schluessel", &format!("t={TOKEN}&wer=gemini&wert={probe}"));
    let antwort = text(&behandle(&d, &umwelt, &post, t));
    assert!(antwort.contains("/einrichtung?m=schluessel&wer=gemini#zusatz"));
    let danach = einrichtung("/einrichtung?m=schluessel&wer=gemini");
    let (_, zusatz) = some(danach.split_once("id=\"zusatz\""), "Gemini-Abschnitt")?;
    assert!(zusatz.contains("hinterlegt, noch nicht benutzt"));
    assert!(zusatz.contains("Schlüssel in der Anmeldeinformationsverwaltung gespeichert."));
    assert!(!danach.contains(probe) && !antwort.contains(probe));
    Ok(())
}

#[test]
fn zeitplan_alte_datei() -> Result<(), String> {
    let wurzel = eingeschaltet(ordner()?)?;
    let datei = wurzel.join("daten/zeitplan.txt");
    let vorher = fs::read_to_string(&datei).map_err(|e| e.to_string())?;
    assert!(!vorher.contains("sicherung"));
    assert!(Zeitplan::lies(&vorher).is_ok());
    let d = dienst(wurzel, true);
    assert!(get(&d, "/teil/live", donnerstag(12, 0)?).contains("Sicherung: aus"));
    Ok(())
}
