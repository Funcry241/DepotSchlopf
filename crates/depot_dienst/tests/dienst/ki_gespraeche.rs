use super::*;

#[test]
fn gespraeche_zeigt_alle_ki_rollen_filtert_und_maskiert() -> Result<(), String> {
    use depot_lage::gespraeche::{Gespraech, anhaengen};
    use std::io::Write as _;

    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    let d = dienst(wurzel, true);
    let zeit = donnerstag(17, 50)?;
    let tag = some(zeit.in_frankfurt().map(|(d, ..)| d), "Frankfurter Tag")?;
    let eintrag = Gespraech {
        zeit,
        lauf: "123-1".into(),
        wer: "dachs".into(),
        rolle: "haupt".into(),
        modell: "Hauptmodell".into(),
        zweck: "erstmeinung".into(),
        ergebnis: "ok".into(),
        http: Some(200),
        token_ein: Some(123),
        token_aus: Some(45),
        cache: None,
        buchung: None,
        schlopfmodell: None,
        kosten_cent: Some(3),
        system: Some("Systemtext bleibt im Protokoll".into()),
        frage: Some("Frage von Dachs".into()),
        antwort: Some(r#"{"einschaetzung":"<script>Antwort von Dachs</script>","ziele":[{"isin":"DE0007164600","gewicht_bp":2000,"begruendung":"Konkreter Kurstreiber."}]}"#.into()),
        fehler: None,
    };
    anhaengen(&daten, &eintrag).map_err(|e| e.to_string())?;
    anhaengen(
        &daten,
        &Gespraech {
            wer: "gemini".into(),
            rolle: "zusatz".into(),
            modell: "Zusatzmodell".into(),
            frage: Some("Frage von Gemini".into()),
            antwort: Some(
                r#"{"meldungen":[{"kurz":"Gemini fasst die Meldung zusammen."}]}"#.into(),
            ),
            ..eintrag.clone()
        },
    )
    .map_err(|e| e.to_string())?;
    anhaengen(
        &daten,
        &Gespraech {
            wer: "maus".into(),
            modell: "Maus-Modell".into(),
            zweck: "zweitmeinung".into(),
            ergebnis: "guthaben".into(),
            http: Some(429),
            antwort: None,
            fehler: Some(r#"{"error":"insufficient_quota"}"#.into()),
            ..eintrag
        },
    )
    .map_err(|e| e.to_string())?;
    std::fs::OpenOptions::new()
        .append(true)
        .open(daten.join("gespraeche").join(format!("{tag}.jsonl")))
        .and_then(|mut f| f.write_all(b"{kaputt}\n"))
        .map_err(|e| e.to_string())?;

    let alles = get(&d, "/gespraeche", zeit);
    assert!(alles.contains("KI-Gespräche") && alles.contains("Zusatz-KI"));
    assert!(alles.contains("Dachs (Hauptmodell)") && alles.contains("Gemini (Zusatzmodell)"));
    assert!(alles.contains("&lt;script&gt;Antwort von Dachs&lt;/script&gt;"));
    assert!(alles.contains("DE0007164600 20,00 %. Konkreter Kurstreiber."));
    assert!(alles.contains("Beim Anbieter fehlt Guthaben."));
    assert!(alles.contains("Nicht lesbare Zeilen: 1"));
    for technik in [
        "<script>Antwort von Dachs</script>",
        "Systemtext bleibt im Protokoll",
        "Frage von Dachs",
        "insufficient_quota",
        "gewicht_bp",
        "HTTP 429",
        "Token ein/aus",
    ] {
        assert!(!alles.contains(technik), "{technik}");
    }
    let gefiltert = get(&d, &format!("/gespraeche?tag={tag}&wer=gemini"), zeit);
    assert!(gefiltert.contains("Gemini fasst die Meldung zusammen."));
    assert!(!gefiltert.contains("Antwort von Dachs"));
    Ok(())
}

#[test]
fn gespraech_zeigt_journalbeleg_und_liefert_nur_gepruefte_rohtexte() -> Result<(), String> {
    use depot_journal::{Journal, KiAufruf, KiAufrufErgebnis};
    use depot_kern::abdruck::sha256_hex;
    use depot_lage::gespraeche::{Gespraech, anhaengen};

    let d = dienst(ordner()?, true);
    let daten = d.wurzel.join("daten");
    let zeit = donnerstag(17, 50)?;
    let (tag, ..) = some(zeit.in_frankfurt(), "Frankfurter Tag")?;
    let pfad = daten.join("journal.txt");
    Journal::einrichten(&pfad).map_err(|e| format!("{e:?}"))?;
    let mut journal = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
    let nr = journal
        .reserviere(BeraterId::Dachs, tag, 1)
        .map_err(|e| format!("{e:?}"))?
        .buchungsnummer();
    let frage = "<script>Belegte Frage</script>";
    let antwort = "Belegte Antwort";
    let frage_sha = sha256_hex(frage.as_bytes());
    let antwort_sha = sha256_hex(antwort.as_bytes());
    let vorlage = sha256_hex(b"Testvorlage");
    journal
        .belege_ki_aufruf(&KiAufruf {
            nr,
            anbieter: "test".into(),
            modell: "Journalmodell".into(),
            vorlage: vorlage.clone(),
            frage: Some(frage_sha.clone()),
            antwort: Some(antwort_sha.clone()),
            token_ein: None,
            token_aus: None,
            ergebnis: KiAufrufErgebnis::Beantwortet,
        })
        .map_err(|e| format!("{e:?}"))?;
    drop(journal);
    let gespraech = Gespraech {
        zeit,
        lauf: "probe".into(),
        wer: "dachs".into(),
        rolle: "haupt".into(),
        modell: "Protokollmodell".into(),
        zweck: "erstmeinung".into(),
        ergebnis: "ok".into(),
        http: Some(200),
        token_ein: None,
        token_aus: None,
        cache: None,
        buchung: Some(nr),
        schlopfmodell: None,
        kosten_cent: Some(u64::MAX),
        system: None,
        frage: None,
        antwort: Some("Zusammengefasste Antwort".into()),
        fehler: None,
    };
    anhaengen(&daten, &gespraech).map_err(|e| e.to_string())?;
    anhaengen(
        &daten,
        &Gespraech {
            wer: "maus".into(),
            ..gespraech
        },
    )
    .map_err(|e| e.to_string())?;
    let roh = daten.join("roh/ki");
    fs::create_dir_all(&roh).map_err(|e| e.to_string())?;
    let frage_pfad = roh.join(format!("{frage_sha}.txt"));
    fs::write(&frage_pfad, frage).map_err(|e| e.to_string())?;

    let html = get(&d, "/gespraeche", zeit);
    assert_eq!(
        html.matches("Journalbeleg · Modell Journalmodell").count(),
        1
    );
    assert!(html.contains(&format!(
        "<code title=\"{vorlage}\">{}</code>",
        vorlage.get(..8).unwrap_or_default()
    )));
    assert!(html.contains(&format!("/gespraeche/rohtext?nr={nr}&amp;art=frage")));
    assert!(html.contains(&format!(
        "Antwort: Rohtext fehlt <code>{antwort_sha}</code>"
    )));
    assert!(html.contains("Kosten noch offen"));
    assert!(!html.contains(frage));

    let lese = |art| get(&d, &format!("/gespraeche/rohtext?nr={nr}&art={art}"), zeit);
    let geliefert = lese("frage");
    assert!(geliefert.starts_with("HTTP/1.1 200 OK"));
    assert!(geliefert.contains("Content-Type: text/plain; charset=utf-8"));
    assert!(geliefert.contains("X-Content-Type-Options: nosniff"));
    assert!(geliefert.ends_with(frage));
    assert!(lese("antwort").starts_with("HTTP/1.1 404"));
    assert!(lese("../journal.txt").starts_with("HTTP/1.1 404"));
    assert!(
        get(&d, "/gespraeche/rohtext?nr=nicht-belegt&art=frage", zeit).starts_with("HTTP/1.1 404")
    );

    fs::write(roh.join(format!("{antwort_sha}.txt")), antwort).map_err(|e| e.to_string())?;
    assert!(lese("antwort").ends_with(antwort));
    fs::write(frage_pfad, "Abweichender Inhalt").map_err(|e| e.to_string())?;
    assert!(lese("frage").starts_with("HTTP/1.1 404"));
    Ok(())
}

#[test]
fn zeitstrahl_ordnet_modelle_und_gespraeche_dem_genauen_lauf_zu() -> Result<(), String> {
    use depot_dienst::werkbank::{Auftrag, Frueher};
    use depot_lage::gespraeche::{Gespraech, anhaengen};

    let d = dienst(ordner()?, true);
    let zeit = donnerstag(17, 50)?;
    let tag = some(zeit.in_frankfurt().map(|(d, ..)| d), "Frankfurter Tag")?;
    let kennung = format!("{}-1", zeit.sekunden());
    {
        let mut z = d.sperre();
        z.zeitplan.an = true;
        z.zeitplan.papier_danach = true;
        z.boersen = vec![Boerse::Xetra];
        z.verlauf.push_front(Auftrag {
            nr: 1,
            kennung: kennung.clone(),
            art: Art::Papier,
            anlass: Anlass::Zeitplan,
            start: zeit,
            ende: Some(zeit),
            erfolg: Some(true),
            protokoll: Vec::new(),
            hinweis: None,
            frueher: Frueher::Nein,
            update_pause: None,
        });
    }
    let html = get(&d, "/teil/live", zeit);
    assert!(html.contains("Modell nicht protokolliert") && !html.contains("Dachs ("));

    for (lauf, wer, rolle, modell, ergebnis, frage) in [
        (
            kennung.as_str(),
            "dachs",
            "haupt",
            "Dachs-alt<script>",
            "ok",
            "Frage für diesen Lauf",
        ),
        (
            kennung.as_str(),
            "maus",
            "haupt",
            "Maus-alt",
            "limit",
            "Frage trotz Limit",
        ),
        (
            kennung.as_str(),
            "gemini",
            "zusatz",
            "Gemini-alt",
            "ok",
            "Zusatzfrage",
        ),
        (
            "anderer-lauf",
            "dachs",
            "haupt",
            "falsches Modell",
            "ok",
            "Frage aus anderem Lauf",
        ),
    ] {
        let e = Gespraech {
            zeit,
            lauf: lauf.into(),
            wer: wer.into(),
            rolle: rolle.into(),
            modell: modell.into(),
            zweck: "erstmeinung".into(),
            ergebnis: ergebnis.into(),
            http: Some(200),
            token_ein: None,
            token_aus: None,
            cache: None,
            buchung: None,
            schlopfmodell: None,
            kosten_cent: None,
            system: None,
            frage: None,
            antwort: Some(frage.into()),
            fehler: None,
        };
        anhaengen(&d.wurzel.join("daten"), &e).map_err(|e| e.to_string())?;
    }
    let html = get(&d, "/teil/live", zeit);
    assert!(html.contains("Dachs (Dachs-alt&lt;script&gt;)"));
    assert!(html.contains("Maus (Maus-alt)</a> · Anbieterlimit"));
    assert!(html.contains("Gemini (Gemini-alt)</a> · Antwort erhalten · optional"));
    assert!(!html.contains("falsches Modell") && !html.contains("Modell nicht protokolliert"));
    assert!(html.contains(&format!(
        "/gespraeche?tag={tag}&amp;wer=dachs&amp;lauf={kennung}"
    )));
    let gespraech = get(
        &d,
        &format!("/gespraeche?tag={tag}&wer=dachs&lauf={kennung}"),
        zeit,
    );
    assert!(gespraech.contains("Frage für diesen Lauf"));
    assert!(!gespraech.contains("Frage aus anderem Lauf"));
    Ok(())
}

#[test]
fn gemini_ohne_anfrage_hat_eigenen_lauf_und_keine_erfolgsmeldung() -> Result<(), String> {
    use depot_dienst::werkbank::{Auftrag, Frueher};
    use depot_lage::gespraeche::{Gespraech, anhaengen};

    let d = dienst(ordner()?, true);
    let zeit = donnerstag(17, 50)?;
    let lauf = format!("{}-2", zeit.sekunden());
    d.sperre().verlauf.push_front(Auftrag {
        nr: 2,
        kennung: lauf.clone(),
        art: Art::Gemini,
        anlass: Anlass::Zeitplan,
        start: zeit,
        ende: Some(zeit),
        erfolg: Some(true),
        protokoll: Vec::new(),
        hinweis: None,
        frueher: Frueher::Nein,
        update_pause: None,
    });
    anhaengen(
        &d.wurzel.join("daten"),
        &Gespraech {
            zeit,
            lauf: lauf.clone(),
            wer: "gemini".into(),
            rolle: "zusatz".into(),
            modell: "Gemini-Test".into(),
            zweck: "nachrichten-vorsortieren".into(),
            ergebnis: "uebersprungen".into(),
            http: None,
            token_ein: None,
            token_aus: None,
            cache: None,
            buchung: None,
            schlopfmodell: None,
            kosten_cent: None,
            system: None,
            frage: None,
            antwort: None,
            fehler: Some(depot_kern::gemini::KEINE_NACHRICHTEN.into()),
        },
    )
    .map_err(|e| e.to_string())?;
    let html = get(&d, "/teil/live", zeit);
    let (zeitstrahl, taetigkeit) = some(html.split_once("id=\"taetigkeit\""), "Tätigkeit")?;
    assert!(zeitstrahl.contains(&format!("wer=gemini&amp;lauf={lauf}")));
    assert!(zeitstrahl.contains("Nicht gefragt: Keine belegten Nachrichten. · optional"));
    let (_, gemini_zeile) = some(taetigkeit.split_once(&lauf), "Gemini-Zeile")?;
    let (gemini_ergebnis, _) = some(gemini_zeile.split_once("</tr>"), "Gemini-Ergebnis")?;
    assert!(gemini_ergebnis.contains("Nicht gefragt: Keine belegten Nachrichten."));
    assert!(!gemini_ergebnis.contains("erfolgreich"));
    Ok(())
}

#[test]
fn gemini_bleibt_ohne_bestaetigtes_gratisprojekt_aus() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(17, 50)?;
    let html = get(&d, "/einrichtung", t);
    assert!(html.contains("Zusatz-KIs (optional)"));
    assert!(html.contains("Ausgeschaltet</strong> · Aufrufe heute 0 von 5"));
    assert!(html.contains(r#"value="nein" disabled>Gemini ausschalten"#));
    let ziel = d.wurzel.join("daten");
    let sende = |text: &str| {
        let a = formular_post("/gemini", &format!("t={TOKEN}&{text}"));
        crate::text(&behandle(&d, &Attrappe::default(), &a, t))
    };
    assert!(sende("an=ja").starts_with("HTTP/1.1 400"));
    assert!(!depot_lage::gespraeche::gemini_aktiv(&ziel));
    assert!(sende("an=ja&gratisprojekt=ja").contains("/einrichtung#zusatz"));
    assert!(depot_lage::gespraeche::gemini_aktiv(&ziel));
    let umw = Attrappe {
        gemini_schluessel: Some(Hinterlegt::Unbekannt),
        ..Attrappe::default()
    };
    let html = text(&behandle(
        &d,
        &umw,
        &anfrage("GET", "/einrichtung", &[("Host", HOST)], ""),
        t,
    ));
    assert!(html.contains("Eingeschaltet</strong>"));
    assert!(html.contains(r#"value="ja" disabled>Gemini einschalten"#));
    assert!(html.contains("Anmeldeinformationsverwaltung nicht prüfbar"));
    let speichere = formular_post(
        "/schluessel",
        &format!("t={TOKEN}&wer=gemini&wert=ungueltigAlsEchterSchluessel"),
    );
    let antwort = text(&behandle(&d, &Attrappe::default(), &speichere, t));
    assert!(antwort.contains("/einrichtung?m=schluessel&wer=gemini#zusatz"));
    assert!(get(&d, "/einrichtung", t).contains("hinterlegt, noch nicht benutzt"));
    let rechnen = depot_dienst::lagerechner(Arc::new(Attrappe::default()));
    let budget = depot_lage::html::seite(&rechnen(&d.wurzel, t));
    assert!(budget.contains("<h3>Gemini</h3><p class=\"gut\">Eingeschaltet"));
    assert!(budget.contains("Aufrufe heute 0 von 5 · 0 €"));
    assert!(sende("an=nein").contains("/einrichtung#zusatz"));
    assert!(!depot_lage::gespraeche::gemini_aktiv(&ziel));
    Ok(())
}

#[test]
fn gemini_speichern_meldet_fehlenden_ruecklesebeleg_am_feld() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(17, 50)?;
    let umwelt = Attrappe {
        gemini_schluessel: Some(Hinterlegt::Nein),
        ..Attrappe::default()
    };
    let probe = "ungueltigAlsEchterSchluessel";
    let post = formular_post("/schluessel", &format!("t={TOKEN}&wer=gemini&wert={probe}"));
    let antwort = text(&behandle(&d, &umwelt, &post, t));
    assert!(antwort.contains("/einrichtung?m=schluessel_fehler&wer=gemini#zusatz"));
    for meldung in ["schluessel_fehler", "schluessel", "schluessel_protokoll"] {
        let html = text(&behandle(
            &d,
            &umwelt,
            &anfrage(
                "GET",
                &format!("/einrichtung?m={meldung}&wer=gemini"),
                &[("Host", HOST)],
                "",
            ),
            t,
        ));
        let (_, zusatz) = some(html.split_once("id=\"zusatz\""), "Gemini-Abschnitt")?;
        assert!(zusatz.contains("Gemini-Schlüssel konnte nicht bestätigt werden."));
        assert!(!html.contains("Schlüssel in der Anmeldeinformationsverwaltung gespeichert."));
        assert!(!html.contains(probe));
    }
    Ok(())
}

#[test]
fn papiertag_startet_trotz_laufendem_gemini() -> Result<(), String> {
    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    fs::write(
        depot_lage::gespraeche::gemini_pfad(&daten),
        "gratisprojekt=ja\n",
    )
    .map_err(|e| e.to_string())?;
    let starter: Starter = Box::new(|art, _| {
        let mut c = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/C", "echo Probe-Standardfehler 1>&2"]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", "echo Probe-Standardfehler >&2"]);
            c
        };
        if art != Art::Gemini {
            c = echo_starter(true)(art, Path::new("."));
        }
        c
    });
    let d = Arc::new(Dienst::neu(
        wurzel.clone(),
        TOKEN.into(),
        starter,
        leeres_lagebild(),
    ));
    let terminzeit = donnerstag(7, 21)?;
    let tag = some(terminzeit.frankfurt_tag(), "Handelstag")?;
    {
        let mut z = d.sperre();
        z.zeitplan.an = true;
        z.zeitplan.papier_danach = true;
        z.boersen = vec![Boerse::Xetra];
        z.erledigt.setze(Art::Termine, tag);
    }
    let (gemini_nr, gemini_kind) = some(d.schritt(terminzeit), "Gemini-Vorlauf gestartet")?;
    assert_eq!(d.sperre().auftrag_stand(Art::Gemini), Some("läuft …"));
    assert!(d.einreihen(Art::Gemini, Anlass::Hand).is_err());
    let kurszeit = donnerstag(7, 30)?;
    let (kurs_nr, kurs_kind) = some(d.schritt(kurszeit), "Kursabruf trotz Gemini gestartet")?;
    assert_eq!(
        d.sperre().laufend.as_ref().map(|a| a.art),
        Some(Art::KurseVon(Boerse::Xetra))
    );
    d.verfolge(kurs_nr, kurs_kind, || donnerstag(7, 30).ok());

    let (papier_nr, papier_kind) = some(d.schritt(kurszeit), "Papiertag trotz Gemini gestartet")?;
    assert_eq!(
        d.sperre().laufend.as_ref().map(|a| a.art),
        Some(Art::Papier)
    );
    let live = get(&d, "/teil/live", kurszeit);
    assert!(live.contains("Gemini-Zuarbeit läuft im Hintergrund."));
    d.verfolge(gemini_nr, gemini_kind, || donnerstag(7, 31).ok());
    assert_eq!(
        d.sperre().laufend.as_ref().map(|a| a.art),
        Some(Art::Papier)
    );
    assert!(d.sperre().verlauf.front().is_some_and(|a| {
        a.art == Art::Gemini
            && a.protokoll
                .iter()
                .any(|z| z.contains("Probe-Standardfehler"))
    }));
    d.verfolge(papier_nr, papier_kind, || donnerstag(7, 32).ok());
    let lese_protokoll = |nr, zeit: Zeitpunkt| {
        depot_dienst::protokoll::lesen(&wurzel, &format!("{}-{nr}", zeit.sekunden()))
    };
    let gemini_protokoll = lese_protokoll(gemini_nr, terminzeit)?;
    let papier_protokoll = lese_protokoll(papier_nr, kurszeit)?;
    assert!(gemini_protokoll.contains("Probe-Standardfehler"));
    assert!(!papier_protokoll.contains("Probe-Standardfehler"));
    assert!(papier_protokoll.contains("Lauf erfolgreich beendet."));
    let neu = dienst(wurzel, true);
    assert!(
        neu.sperre()
            .verlauf
            .iter()
            .any(|a| a.art == Art::Gemini && a.erfolg == Some(true))
    );
    assert!(
        neu.sperre()
            .verlauf
            .iter()
            .any(|a| a.art == Art::Papier && a.erfolg == Some(true))
    );
    Ok(())
}

#[test]
fn otter_auf_live_bleibt_ausserhalb_der_aktualisierten_teile() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let zeit = donnerstag(12, 0)?;
    let html = get(&d, "/", zeit);
    assert_eq!(html.matches("class=\"otter\"").count(), 1);
    assert!(html.contains("prefers-reduced-motion:no-preference"));
    assert!(html.contains("</div>\n<div class=\"otter\" aria-hidden=\"true\">"));
    for pfad in ["/teil/kopf", "/teil/live", "/einrichtung"] {
        assert!(!get(&d, pfad, zeit).contains("class=\"otter\""), "{pfad}");
    }
    Ok(())
}

#[test]
fn otter_wird_bei_einem_auftrag_wach_und_ruht_danach() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let zeit = donnerstag(12, 0)?;
    let pose = |pose| get(&d, "/teil/kopf", zeit).contains(&format!("data-otter=\"{pose}\""));
    assert!(pose("ruht"));
    d.einreihen(Art::Termine, Anlass::Hand)?;
    let (nr, kind) = some(d.schritt(zeit), "Termine gestartet")?;
    assert!(pose("wach"));
    d.verfolge(nr, kind, || donnerstag(12, 1).ok());
    assert!(pose("ruht"));
    Ok(())
}
