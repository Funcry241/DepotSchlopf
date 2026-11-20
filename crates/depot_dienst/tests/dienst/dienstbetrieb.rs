use super::*;

#[test]
fn xetra_entwarnung_bezieht_sich_nur_auf_den_geprueften_verdacht() -> Result<(), String> {
    use depot_journal::{Journal, NachrichtBeleg, splitverdacht};
    let wurzel = ordner()?;
    let pfad = wurzel.join("daten/journal.txt");
    Journal::einrichten(&pfad).map_err(|e| format!("{e:?}"))?;
    let t = donnerstag(12, 0)?;
    let tag = some(t.frankfurt_tag(), "Tag")?;
    let mut j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
    let belege = [
        format!("Möglicher Split 4:1 bei SAP (DE0007164600) am {tag}."),
        format!("Möglicher Split 2:1 bei SAP (DE0007164600) am {tag}."),
        format!("Möglicher Split 4:1 bei Allianz (DE0008404005) am {tag}."),
    ]
    .map(|text| {
        NachrichtBeleg::neu(
            depot_kern::quelle::QUELLE_XETRA_SPLITVERDACHT,
            t,
            b"Probe",
            t,
            &text,
        )
    });
    let [sap, neuer_sap, allianz] = belege;
    let sap = sap?;
    for beleg in [&sap, &neuer_sap?, &allianz?] {
        assert!(j.belege_nachricht(beleg).map_err(|e| format!("{e:?}"))?);
    }
    drop(j);
    let d = dienst(wurzel, true);
    let u = Attrappe::default();
    let html = get(&d, "/einrichtung", t);
    assert!(html.contains("Fehlalarm bestätigen") && html.contains(&splitverdacht::kennung(&sap)));
    let paket = format!("t={TOKEN}&kennung={}", splitverdacht::kennung(&sap));
    let post = |extra| formular_post("/splitverdacht", &format!("{paket}{extra}"));
    assert_eq!(status(&behandle(&d, &u, &post(""), t))?, 400);
    assert_eq!(status(&behandle(&d, &u, &post("&geprueft=ja"), t))?, 303);
    assert_eq!(status(&behandle(&d, &u, &post("&geprueft=ja"), t))?, 409);
    let repariert = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
    let offen = splitverdacht::aktiv(repariert.stand(), None);
    assert_eq!(offen.len(), 2);
    assert!(
        offen
            .values()
            .any(|b| b.text().contains("Split 2:1 bei SAP"))
    );
    assert!(offen.values().any(|b| b.text().contains("bei Allianz")));
    Ok(())
}

#[test]
fn antworten_verbieten_einbetten_und_fremde_skripte() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let umw = Attrappe::default();
    let a = anfrage("GET", "/", &[("Host", HOST)], "");
    let r = text(&behandle(&d, &umw, &a, donnerstag(12, 0)?));
    for kopf in [
        "Content-Security-Policy: default-src 'none'; script-src 'self';",
        "frame-ancestors 'none'",
        "X-Frame-Options: DENY",
        "X-Content-Type-Options: nosniff",
        "Cache-Control: no-store",
        // Gleiche Herkunft: Der Browser sendet bei Formularen die eigene Herkunft, nie "null".
        "Referrer-Policy: same-origin",
    ] {
        assert!(r.contains(kopf), "{kopf}");
    }
    assert!(!r.contains("<script>"), "kein eingebettetes Skript");
    assert!(r.contains("<script src=\"/app.js\"></script>"));
    Ok(())
}

#[test]
fn meldungen_spiegeln_keine_eingabe() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let umw = Attrappe::default();
    let a = anfrage(
        "GET",
        "/?m=%3Cscript%3Ealert(1)%3C/script%3E<b>",
        &[("Host", HOST)],
        "",
    );
    let r = text(&behandle(&d, &umw, &a, donnerstag(12, 0)?));
    assert!(!r.contains("alert(1)"));
    assert!(!r.contains("<b>"));
    let a = anfrage("GET", "/einrichtung?m=zeitplan", &[("Host", HOST)], "");
    let r = text(&behandle(&d, &umw, &a, donnerstag(12, 0)?));
    assert!(r.contains("Einstellungen gespeichert."));
    Ok(())
}

#[test]
fn autostart_nur_mit_harmlosem_pfad() -> Result<(), String> {
    let gut = autostart::inhalt(Path::new("C:\\Test\\Neu"))?;
    assert!(gut.contains("\"C:\\Test\\Neu\\DepotSchlopf.cmd\" --ohne-browser"));
    for boese in ["C:\\a%PATH%", "C:\\a\"&calc", "C:\\a!x!", "C:\\a\r\nx", ""] {
        assert!(autostart::inhalt(Path::new(boese)).is_err(), "{boese:?}");
    }
    let d = dienst(ordner()?, true);
    let umw = Attrappe::default();
    let a = formular_post("/autostart", &format!("t={TOKEN}&an=an"));
    assert!(text(&behandle(&d, &umw, &a, donnerstag(12, 0)?)).contains("m=autostart_an"));
    assert_eq!(
        *umw.autostart.lock().unwrap_or_else(PoisonError::into_inner),
        Some(true)
    );
    let aus = formular_post("/autostart", &format!("t={TOKEN}&an=aus"));
    let t = donnerstag(12, 0)?;
    assert!(text(&behandle(&d, &umw, &aus, t)).contains("m=autostart_bestaetigen"));
    assert_eq!(
        *umw.autostart.lock().unwrap_or_else(PoisonError::into_inner),
        Some(true)
    );
    let aus = formular_post("/autostart", &format!("t={TOKEN}&an=aus&geprueft=ja"));
    assert!(text(&behandle(&d, &umw, &aus, t)).contains("m=autostart_aus"));
    assert_eq!(
        *umw.autostart.lock().unwrap_or_else(PoisonError::into_inner),
        Some(false)
    );
    Ok(())
}

#[test]
fn sperre_des_kostenschutzes_gefuehrt_aufheben() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let umw = Attrappe::default();
    let t = donnerstag(18, 0)?;
    let halt = kostenschutz::pfad(&wurzel);
    let grund = "Kosten unbekannt: Antwort fehlte (Dachs) <b>\nreserviert 40 Cent\n";
    fs::write(&halt, grund).map_err(|e| format!("Vorbedingung ki.halt: {e}"))?;
    let journal = wurzel.join("daten").join("journal.txt");
    fs::write(&journal, "unberuehrt").map_err(|e| format!("Vorbedingung Journal: {e}"))?;
    let abdruck = kostenschutz::abdruck(grund);

    let seite = anfrage("GET", "/einrichtung", &[("Host", HOST)], "");
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(s.contains("id=\"kostenschutz\""), "{s}");
    assert_eq!(
        s.matches("id=\"ki-verbindung\"").count(),
        1,
        "Aktive Sperre: keine doppelten Abschnitte"
    );
    assert!(
        s.contains("Antwort fehlte (Dachs) &lt;b&gt;"),
        "maskiert: {s}"
    );
    assert!(s.contains(&format!("name=\"abdruck\" value=\"{abdruck}\"")));
    assert!(s.contains("name=\"geprueft\" value=\"ja\" required"));

    // Ohne Bestaetigung, mit fremdem Abdruck oder waehrend ein Auftrag wartet: nichts.
    let r = text(&behandle(&d, &umw, &aufheben(&abdruck, false), t));
    assert!(r.contains("m=kostenschutz_unbestaetigt"), "{r}");
    let r = text(&behandle(&d, &umw, &aufheben("0000000000000000", true), t));
    assert!(r.contains("m=kostenschutz_geaendert"), "{r}");
    d.einreihen(Art::Papier, Anlass::Hand)
        .map_err(|e| format!("Vorbedingung einreihen: {e}"))?;
    let r = text(&behandle(&d, &umw, &aufheben(&abdruck, true), t));
    assert!(r.contains("m=kostenschutz_belegt"), "{r}");
    assert!(halt.exists(), "die Sperre bleibt");
    d.sperre().warteschlange.clear();
    // Ohne Token wie jedes Formular abgelehnt.
    let fremd = formular_post(
        "/kostenschutz",
        &format!("t=falsch&abdruck={abdruck}&geprueft=ja"),
    );
    assert_eq!(status(&behandle(&d, &umw, &fremd, t))?, 403);
    assert!(halt.exists());

    let r = text(&behandle(&d, &umw, &aufheben(&abdruck, true), t));
    assert!(r.contains("m=kostenschutz_dialog_unbestaetigt"), "{r}");
    assert!(halt.exists());
    let umw = Attrappe {
        sperrfreigabe: true,
        ..umw
    };

    let r = text(&behandle(&d, &umw, &aufheben(&abdruck, true), t));
    assert!(r.contains("m=kostenschutz_aufgehoben"), "{r}");
    assert!(!halt.exists());
    let festgehalten = fs::read_to_string(kostenschutz::protokoll_pfad(&wurzel))
        .map_err(|e| format!("Protokoll: {e}"))?;
    assert!(festgehalten.contains(grund), "{festgehalten}");
    assert_eq!(
        fs::read_to_string(&journal).map_err(|e| format!("Journal: {e}"))?,
        "unberuehrt",
        "Kassenbuch und Journal bleiben, wie sie sind"
    );
    assert!(d.sperre().lage_veraltet);
    let r = text(&behandle(&d, &umw, &aufheben(&abdruck, true), t));
    assert!(r.contains("m=kostenschutz_frei"), "{r}");
    let s = text(&behandle(&d, &umw, &seite, t));
    let werkzeuge = s.find("id=\"werkzeuge\"").ok_or("Werkzeuge fehlen")?;
    let frei = s
        .find("id=\"kostenschutz\"")
        .ok_or("Rücksprung nach Aufheben fehlt")?;
    assert!(frei > werkzeuge && s.contains("Keine Sperre"), "{s}");
    assert_eq!(
        s.matches("id=\"ki-verbindung\"").count(),
        1,
        "Freie Sperre: keine doppelten Abschnitte"
    );
    assert!(
        s.contains("id=\"ki-verbindung\""),
        "Die Erklärung bleibt unter Werkzeuge erreichbar: {s}"
    );
    Ok(())
}

#[test]
fn bestaetigter_dialog_hebt_keine_inzwischen_geaenderte_sperre_auf() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let halt = kostenschutz::pfad(&wurzel);
    fs::write(&halt, "Alte Sperre").map_err(|e| e.to_string())?;
    let umw = Attrappe {
        sperrfreigabe: true,
        sperrdatei_wechsel: Some(halt.clone()),
        ..Attrappe::default()
    };
    let post = aufheben(&kostenschutz::abdruck("Alte Sperre"), true);
    let r = text(&behandle(&d, &umw, &post, donnerstag(18, 0)?));
    assert!(r.contains("m=kostenschutz_geaendert"), "{r}");
    assert_eq!(
        fs::read_to_string(halt).map_err(|e| e.to_string())?,
        "Neue Sperre"
    );
    assert!(!kostenschutz::protokoll_pfad(&wurzel).exists());
    Ok(())
}

#[test]
fn desktopdialog_gibt_http_frei_und_verwirft_einen_geaenderten_stand() -> Result<(), String> {
    use std::sync::mpsc;
    use std::time::Duration;

    let t = donnerstag(18, 0)?;
    let limit = Duration::from_secs(5);
    for ordnerwahl in [false, true] {
        let wurzel = ordner()?;
        let d = dienst(wurzel.clone(), true);
        let halt = kostenschutz::pfad(&wurzel);
        let neues_ziel = format!("{}\n", wurzel.join("neues-ziel").display());
        let ziel_datei = wurzel.join("daten/sicherung_ziel.txt");
        let post = if ordnerwahl {
            formular_post("/sicherung-ziel", &format!("t={TOKEN}&aktion=waehlen"))
        } else {
            fs::write(&halt, "Alte Sperre").map_err(|e| e.to_string())?;
            aufheben(&kostenschutz::abdruck("Alte Sperre"), true)
        };
        let (offen, gestartet) = mpsc::channel();
        let (freigeben, weiter) = mpsc::channel();
        let u: Arc<dyn Umwelt> = Arc::new(Attrappe {
            sperrfreigabe: true,
            sicherungsziel: Some(wurzel.join("dialog-ziel").to_string_lossy().into_owned()),
            dialogpause: Some(Dialogpause {
                offen,
                weiter: Mutex::new(weiter),
            }),
            ..Attrappe::default()
        });
        let (http_senden, http_antwort) = mpsc::channel();
        let (dienst, umwelt, formular) = (Arc::clone(&d), Arc::clone(&u), post.clone());
        std::thread::spawn(move || {
            let antwort = depot_dienst::behandle_am_desktop(&dienst, &umwelt, &formular, t);
            let _ = http_senden.send(text(&antwort));
        });

        gestartet.recv_timeout(limit).map_err(|e| e.to_string())?;
        let antwort = http_antwort
            .recv_timeout(limit)
            .map_err(|e| e.to_string())?;
        let ziel = ort(&antwort);
        assert!(ziel.starts_with("/desktop-dialog?id="), "{antwort}");
        assert!(get(&d, &ziel, t).contains("Die Antwort steht aus."));
        let doppelt = depot_dienst::behandle_am_desktop(&d, &u, &post, t);
        assert_eq!(ort(&text(&doppelt)), ziel);
        assert!(d.einreihen(Art::Termine, Anlass::Hand).is_ok());
        let (nr, kind) = some(d.schritt(t), "Auftrag trotz offenem Windows-Dialog")?;
        d.verfolge(nr, kind, || donnerstag(18, 0).ok());

        if ordnerwahl {
            fs::write(&ziel_datei, &neues_ziel).map_err(|e| e.to_string())?;
        } else {
            fs::write(&halt, "Neue Sperre").map_err(|e| e.to_string())?;
        }
        freigeben.send(()).map_err(|e| e.to_string())?;
        let teil = format!("/teil{ziel}");
        let bis = std::time::Instant::now() + limit;
        loop {
            let html = get(&d, &teil, t);
            if !html.contains("Die Antwort steht aus.") {
                assert!(html.contains("hat sich inzwischen geändert"), "{html}");
                break;
            }
            if std::time::Instant::now() >= bis {
                return Err("Die Dialogantwort fehlt".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if ordnerwahl {
            assert_eq!(
                fs::read_to_string(&ziel_datei).map_err(|e| e.to_string())?,
                neues_ziel
            );
        } else {
            assert_eq!(
                fs::read_to_string(&halt).map_err(|e| e.to_string())?,
                "Neue Sperre"
            );
        }
    }
    Ok(())
}

/// Die Kurssperre nach einem Sicherheitsabbruch wird wie die Sperre des Kostenschutzes gefuehrt
/// aufgehoben, nie von Hand geloescht (Hinweis zu #34).
#[test]
fn kurssperre_gefuehrt_aufheben() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let umw = Attrappe::default();
    let t = donnerstag(18, 0)?;
    let seite = anfrage("GET", "/einrichtung", &[("Host", HOST)], "");
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(
        s.contains("id=\"kurssperre\""),
        "Rücksprung bleibt ohne Sperre vorhanden"
    );
    assert!(
        !s.contains("action=\"/kurssperre\""),
        "Keine Aufhebung ohne Sperre"
    );

    let halt = kostenschutz::kurse_pfad(&wurzel);
    assert!(halt.ends_with("kurse.halt"));
    let grund = "Sicherheitsabbruch: Die Kursquelle gab den Schluessel zurueck <i>\n";
    fs::write(&halt, grund).map_err(|e| format!("Vorbedingung kurse.halt: {e}"))?;
    let abdruck = kostenschutz::abdruck(grund);
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(s.contains("id=\"kurssperre\""), "{s}");
    assert!(s.contains("zurueck &lt;i&gt;"), "maskiert: {s}");
    assert!(s.contains("action=\"/kurssperre\""), "{s}");

    let post = |abdruck: &str, haken: bool| {
        let h = if haken { "&geprueft=ja" } else { "" };
        formular_post("/kurssperre", &format!("t={TOKEN}&abdruck={abdruck}{h}"))
    };
    let r = text(&behandle(&d, &umw, &post(&abdruck, false), t));
    assert!(r.contains("m=kurssperre_unbestaetigt"), "{r}");
    let r = text(&behandle(&d, &umw, &post("0000000000000000", true), t));
    assert!(r.contains("m=kurssperre_geaendert"), "{r}");
    d.einreihen(Art::Kurse, Anlass::Hand)
        .map_err(|e| format!("Vorbedingung einreihen: {e}"))?;
    let r = text(&behandle(&d, &umw, &post(&abdruck, true), t));
    assert!(r.contains("m=kurssperre_belegt"), "{r}");
    assert!(halt.exists(), "die Sperre bleibt");
    d.sperre().warteschlange.clear();

    let r = text(&behandle(&d, &umw, &post(&abdruck, true), t));
    assert!(r.contains("m=kurssperre_dialog_unbestaetigt"), "{r}");
    assert!(halt.exists());
    let umw = Attrappe {
        sperrfreigabe: true,
        ..umw
    };

    let r = text(&behandle(&d, &umw, &post(&abdruck, true), t));
    assert!(r.contains("m=kurssperre_aufgehoben"), "{r}");
    assert!(!halt.exists());
    let festgehalten = fs::read_to_string(kostenschutz::kurse_protokoll_pfad(&wurzel))
        .map_err(|e| format!("Protokoll: {e}"))?;
    assert!(festgehalten.contains(grund), "{festgehalten}");
    let r = text(&behandle(&d, &umw, &post(&abdruck, true), t));
    assert!(r.contains("m=kurssperre_frei"), "{r}");
    Ok(())
}

#[test]
fn navigation_markiert_aktuelle_seite_und_zeigt_laufenden_stand() -> Result<(), String> {
    let mut d = Dienst::neu(
        ordner()?,
        TOKEN.into(),
        echo_starter(true),
        leeres_lagebild(),
    );
    let t = donnerstag(12, 0)?;
    assert!(get(&d, "/", t).contains("Commit unbekannt"));
    let sha = "a".repeat(40);
    d.version = Some(sha.clone());
    for (pfad, titel) in [("/", "Live"), ("/einrichtung", "Einrichtung")] {
        let html = get(&d, pfad, t);
        assert_eq!(html.matches("aria-current=\"page\"").count(), 1);
        assert!(html.contains(&format!(
            "<a href=\"{pfad}\" aria-current=\"page\">{titel}</a>"
        )));
        assert!(html.contains(&format!("title=\"{sha}\">Version aaaaaaaaaa</span>")));
    }
    Ok(())
}

#[test]
fn testdialog_ist_ohne_sperre_verfuegbar_und_schreibt_nichts() -> Result<(), String> {
    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    let d = dienst(wurzel, true);
    let t = donnerstag(12, 0)?;
    assert!(get(&d, "/einrichtung", t).contains("Freigabedialog testen"));
    for (name, inhalt) in [
        ("journal.txt", "Originaljournal"),
        ("journal.vermerk", "Originalvermerk"),
        ("papierkonto.txt", "Originalkonto"),
    ] {
        fs::write(daten.join(name), inhalt).map_err(|e| e.to_string())?;
    }
    let a = formular_post("/papierdialog-test", &format!("t={TOKEN}"));
    assert!(text(&behandle(&d, &Attrappe::default(), &a, t)).contains("dialogtest_ja"));
    for (name, inhalt) in [
        ("journal.txt", "Originaljournal"),
        ("journal.vermerk", "Originalvermerk"),
        ("papierkonto.txt", "Originalkonto"),
    ] {
        assert_eq!(
            fs::read(daten.join(name)).map_err(|e| e.to_string())?,
            inhalt.as_bytes()
        );
    }
    Ok(())
}

#[test]
fn speichern_und_einschalten_uebernimmt_genau_die_ausgefuellten_werte() -> Result<(), String> {
    let w = ordner()?;
    let d = Arc::new(Dienst::neu(
        w.clone(),
        TOKEN.into(),
        echo_starter(true),
        lagebild_mit_schluesseln(true),
    ));
    let t = donnerstag(12, 0)?;
    assert!(d.schritt(t).is_none());
    let sende = |werte: &str| {
        text(&behandle(
            &d,
            &Attrappe::default(),
            &formular_post(
                "/zeitplan",
                &format!("t={TOKEN}&aktion=speichern_an&{werte}"),
            ),
            t,
        ))
    };
    let r = sende("termine=08%3A15&kurse=08%3A05&nachrichten_an=ja");
    assert!(r.contains("m=zeitplan_gespeichert_an"), "{r}");
    let plan = d.sperre().zeitplan;
    assert!(plan.an && plan.nachrichten_an);
    assert_eq!(plan.termine, (8, 15));
    assert_eq!(plan.kurse, (8, 5));
    assert_eq!(
        Zeitplan::lies(
            &fs::read_to_string(w.join("daten/zeitplan.txt")).map_err(|e| e.to_string())?
        ),
        Ok(plan)
    );
    let seite = get(&d, &ort(&r), t);
    assert!(seite.contains("Einstellungen gespeichert und Zeitplan eingeschaltet"));
    // Ungültig heißt weder speichern noch einschalten; die bisherigen Werte gelten weiter.
    let r = sende("termine=ungueltig&kurse=19%3A05");
    assert!(r.contains("m=zeitplan_ungueltig"));
    assert_eq!(d.sperre().zeitplan, plan);
    let html = get(&d, &ort(&r), t);
    assert!(html.contains("value=\"19:05\""));
    assert!(html.contains("data-entwurf=\"ja\""));
    assert!(html.contains("href=\"#zeitplan\">Zeitplan und Eingaben ansehen"));
    Ok(())
}

#[test]
fn quellenbereich_zeigt_den_tatsaechlichen_abruf_und_sein_ergebnis() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(12, 0)?;
    assert!(get(&d, "/einrichtung", t).contains("data-teil=\"/teil/quellen\""));
    assert!(get(&d, "/teil/journal", t).contains("data-start="));
    let quellen = || get(&d, "/teil/quellen", t);
    assert!(quellen().contains("data-start="));
    assert!(quellen().contains("Kein Abruf im Verlauf"));
    for beschriftung in ["Kurse abrufen", "Termine abrufen", "Schlagzeilen abrufen"] {
        assert!(quellen().contains(&format!(">{beschriftung}</button>")));
    }
    let xetra = Art::KurseVon(Boerse::Xetra);
    d.einreihen(xetra, Anlass::Hand)?;
    assert!(quellen().contains("Kurse Xetra abrufen · wartet"));
    let (nr, kind) = some(d.schritt(t), "Kursabruf")?;
    assert!(quellen().contains("Kurse Xetra abrufen · läuft"));
    d.verfolge(nr, kind, || donnerstag(12, 0).ok());
    assert!(quellen().contains("Xetra · <span class=\"gut\">erfolgreich</span>"));
    assert!(!quellen().contains("Kurse Xetra abrufen · läuft"));
    Ok(())
}

#[test]
fn ungueltige_eingabe_laesst_die_gueltigen_stehen() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(12, 0)?;
    let f = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&aktion=speichern&termine=8%3A00&kurse=18%3A20&papier_danach=ja"),
    );
    let r = text(&behandle(&d, &Attrappe::default(), &f, t));
    let ziel = ort(&r);
    assert!(ziel.contains("m=zeitplan_ungueltig"), "{ziel}");
    assert!(
        ziel.contains("kurse=18%3A20") && !ziel.contains("termine="),
        "{ziel}"
    );
    let seite = get(&d, &ziel, t);
    assert!(seite.contains("value=\"18:20\""), "{seite}");
    // Fremde Werte in der Abfrage erscheinen nicht.
    let seite = get(
        &d,
        "/einrichtung?entwurf=ja&kurse=%3Cb%3E&termine=99%3A99",
        t,
    );
    assert!(
        !seite.contains("&lt;b&gt;") && !seite.contains("99:99"),
        "{seite}"
    );
    Ok(())
}

#[test]
fn einrichten_bleibt_auf_der_config_seite_und_zeigt_den_arbeitsstand() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(12, 0)?;
    assert!(d.schritt(t).is_none());
    let html = get(&d, "/einrichtung", t);
    let pos = |anker| {
        html.find(anker)
            .ok_or_else(|| format!("Abschnitt fehlt: {anker}"))
    };
    assert!(pos("id=\"journal\"")? < pos("id=\"schluessel\"")?);
    assert!(pos("id=\"erste-schritte\"")? < pos("id=\"quellen\"")?);
    assert!(html.contains("href=\"#journal\">Journal einrichten"));
    assert!(pos("id=\"quellen\"")? < pos("id=\"zeitplan\"")?);
    assert!(pos("id=\"zeitplan\"")? < pos("id=\"update\"")?);
    assert!(pos("id=\"update\"")? < pos("id=\"selten\"")?);
    assert!(pos("id=\"schluessel\"")? < pos("id=\"quellen\"")?);
    assert!(pos("id=\"schluessel\"")? < pos("id=\"autostart\"")?);
    assert!(pos("id=\"autostart\"")? < pos("id=\"werkzeuge\"")?);
    assert!(html.contains("name=\"ziel\" value=\"einrichtung\""));
    for anker in [
        "kostenschutz",
        "kurssperre",
        "papierfreigabe",
        "ki-verbindung",
    ] {
        assert_eq!(
            html.matches(&format!("id=\"{anker}\"")).count(),
            1,
            "Rücksprung fehlt oder ist doppelt: {anker}"
        );
    }
    let post = formular_post(
        "/ausfuehren",
        &format!("t={TOKEN}&art=einrichten&ziel=einrichtung"),
    );
    let antwort = text(&behandle(&d, &Attrappe::default(), &post, t));
    assert!(
        antwort.contains("/einrichtung?m=auftrag#journal"),
        "{antwort}"
    );
    assert_eq!(wartend(&d), vec![(Art::Einrichten, Anlass::Hand)]);
    let html = get(&d, "/teil/journal", t);
    assert!(
        html.contains("disabled>Journal einrichten · wartet"),
        "{html}"
    );
    assert!(html.contains("Bitte warte auf den Abschluss."), "{html}");
    let antwort = text(&behandle(&d, &Attrappe::default(), &post, t));
    assert!(
        antwort.contains("/einrichtung?m=auftrag_belegt#journal"),
        "{antwort}"
    );
    assert_eq!(wartend(&d).len(), 1);
    // Ein frei eingesandtes Ziel wird nicht als Umleitungsadresse uebernommen.
    let fremd = formular_post(
        "/ausfuehren",
        &format!("t={TOKEN}&art=einrichten&ziel=https://fremd.test"),
    );
    let antwort = text(&behandle(&d, &Attrappe::default(), &fremd, t));
    assert!(!antwort.contains("fremd.test"), "{antwort}");
    Ok(())
}

#[test]
fn journal_knopf_zeigt_arbeitsstand() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(12, 0)?;
    assert!(d.schritt(t).is_none());
    assert!(get(&d, "/", t).contains(">Journal einrichten</button>"));
    assert_eq!(
        get(&d, "/teil/live", t)
            .matches("Journal: Noch nicht eingerichtet.")
            .count(),
        1
    );
    d.einreihen(Art::Einrichten, Anlass::Hand)?;
    assert!(get(&d, "/teil/live", t).contains("disabled>Journal einrichten · wartet"));
    let (nr, kind) = some(d.schritt(t), "Start")?;
    let live = get(&d, "/teil/live", t);
    assert!(live.contains("disabled>Journal einrichten · läuft"));
    assert!(live.contains("Der Lauf wird vorbereitet"));
    d.verfolge(nr, kind, || donnerstag(12, 0).ok());
    Ok(())
}

#[test]
fn doppelklick_startet_nichts_doppelt() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let umw = Attrappe::default();
    let t = donnerstag(12, 0)?;
    let bestaetigung = get(&d, "/bestaetigen?art=papier", t);
    assert!(bestaetigung.contains("echtes KI-Budget") && bestaetigung.contains("Abbrechen"));
    assert!(
        wartend(&d).is_empty(),
        "Öffnen der Bestätigung startet nichts"
    );
    let ohne = formular_post("/ausfuehren", &format!("t={TOKEN}&art=papier"));
    assert!(text(&behandle(&d, &umw, &ohne, t)).contains("m=papier_bestaetigen"));
    assert!(wartend(&d).is_empty());
    let klick = formular_post("/ausfuehren", &format!("t={TOKEN}&art=papier&geprueft=ja"));
    assert!(text(&behandle(&d, &umw, &klick, t)).contains("m=auftrag"));
    let r = text(&behandle(&d, &umw, &klick, t));
    assert!(r.contains("m=auftrag_belegt"), "{r}");
    assert_eq!(wartend(&d), vec![(Art::Papier, Anlass::Hand)]);
    let (nr, kind) = some(d.schritt(t), "Start")?;
    d.verfolge(nr, kind, || donnerstag(12, 0).ok());
    // Erneut gesendet gleich nach dem Ende: nicht noch einmal.
    let r = text(&behandle(&d, &umw, &klick, donnerstag(12, 0)?));
    assert!(r.contains("m=auftrag_belegt"), "{r}");
    assert!(wartend(&d).is_empty());
    // Mit Abstand ist es ein neuer, gewollter Auftrag.
    let spaeter = some(
        Zeitpunkt::aus_sekunden(t.sekunden().saturating_add(60)),
        "spaeter",
    )?;
    assert!(text(&behandle(&d, &umw, &klick, spaeter)).contains("m=auftrag"));
    assert_eq!(wartend(&d), vec![(Art::Papier, Anlass::Hand)]);
    // Das Skript sperrt abgesendete Knoepfe.
    let js = get(&d, "/app.js", t);
    assert!(js.contains("b.disabled = true"), "{js}");
    Ok(())
}

#[test]
fn wiederanlauf_erhaelt_verlauf_vor_einer_abgerissenen_zeile() -> Result<(), String> {
    let wurzel = ordner()?;
    let t = donnerstag(12, 0)?;
    let alt = format!("{}-7", t.sekunden());
    let datei = depot_dienst::verlauf::pfad(&wurzel);
    fs::create_dir_all(wurzel.join("daten")).map_err(|e| e.to_string())?;
    let start = format!("verlauf-v1;start;{alt};termine;hand;{}\n", t.sekunden());
    let mut original = start.as_bytes().to_vec();
    original.extend_from_slice(b"verlauf-v1;ende;\xff");
    fs::write(&datei, &original).map_err(|e| e.to_string())?;

    assert!(depot_dienst::verlauf::vermerke(&wurzel, start.trim_end()).is_err());
    assert_eq!(fs::read(&datei).map_err(|e| e.to_string())?, original);

    let d = dienst(wurzel.clone(), true);
    {
        let z = d.sperre();
        let lauf = some(z.verlauf.front(), "Alter Lauf fehlt")?;
        assert_eq!(lauf.kennung, alt);
        assert_eq!(lauf.frueher, Frueher::Unterbrochen);
        assert!(
            z.verlauf_hinweis
                .as_deref()
                .is_some_and(|h| h.contains("gesichert"))
        );
    }
    let hash = depot_kern::abdruck::sha256_hex(&original);
    let sicherung = datei.with_file_name(format!("dienst_verlauf-{hash}.quarantaene"));
    assert_eq!(fs::read(sicherung).map_err(|e| e.to_string())?, original);

    d.einreihen(Art::Kurse, Anlass::Hand)?;
    let (nr, kind) = some(d.schritt(t), "Auftrag nach dem Neustart")?;
    assert_eq!(nr, 8);
    d.verfolge(nr, kind, || donnerstag(12, 0).ok());
    drop(d);
    let d = dienst(wurzel, true);
    let z = d.sperre();
    assert_eq!(z.verlauf.len(), 2);
    assert_eq!(z.naechste_nr, 9);
    assert_eq!(z.verlauf.front().map(|a| a.frueher), Some(Frueher::Beendet));
    Ok(())
}

#[test]
fn verlauf_liest_streng() {
    let g = depot_dienst::verlauf::lies(
        "verlauf-v1;start;1-1;kurse;hand;1800000000\n\
         kaputt\n\
         verlauf-v1;start;1-2;fuchs;hand;1800000000\n\
         verlauf-v1;ende;1-1;kurse;hand;1800000000;1800000060;ok\n\
         verlauf-v1;ende;1-3;termine;zeitplan;1800000000;-;fehler;mehr\n",
    );
    assert_eq!(g.unlesbar, 3);
    assert_eq!(g.auftraege.len(), 1);
    assert!(
        g.auftraege
            .first()
            .is_some_and(|a| a.erfolg == Some(true) && a.frueher == Frueher::Beendet)
    );
}

/// Maus zu #32: Erst dauerhaft vermerken, dann starten. Laesst sich der Verlauf nicht
/// schreiben, startet nichts, und die Oberflaeche zeigt es sofort.
#[test]
fn ohne_schreibbaren_verlauf_startet_nichts() -> Result<(), String> {
    let wurzel = ordner()?;
    let datei = depot_dienst::verlauf::pfad(&wurzel);
    fs::create_dir_all(&datei).map_err(|e| format!("Vorbedingung: {e}"))?;
    let d = dienst(wurzel, true);
    d.sperre().zeitplan.an = true;
    let t = donnerstag(12, 0)?;
    d.einreihen(Art::Termine, Anlass::Hand)
        .map_err(|e| format!("Vorbedingung: {e}"))?;
    assert!(d.schritt(t).is_none(), "nichts gestartet");
    let z = d.sperre();
    assert!(z.laufend.is_none());
    let a = some(z.verlauf.front(), "Verlauf")?;
    assert_eq!(a.erfolg, Some(false));
    assert!(a.protokoll.iter().any(|p| p.starts_with("Nicht gestartet")));
    let seite = seiten::live(TOKEN, "s", &z, t);
    drop(z);
    assert!(seite.contains("Verlauf nicht schreibbar"), "{seite}");
    assert!(seite.contains("Zeitplan angehalten"));
    assert!(!seite.contains("Nächster geplanter Lauf"));
    assert!(get(&d, "/einrichtung", t).contains("<b>angehalten</b>"));
    // Wieder schreibbar: Der naechste Auftrag startet.
    fs::remove_dir(&datei).map_err(|e| format!("Vorbedingung: {e}"))?;
    d.einreihen(Art::Kurse, Anlass::Hand)
        .map_err(|e| format!("Vorbedingung: {e}"))?;
    let (nr, kind) = some(d.schritt(t), "Start")?;
    d.verfolge(nr, kind, || donnerstag(12, 1).ok());
    let text = fs::read_to_string(&datei).map_err(|e| format!("Verlauf: {e}"))?;
    assert!(text.starts_with("verlauf-v1;start;"), "{text}");
    assert!(d.sperre().verlauf_fehler.is_none(), "Fehler verschwindet");
    Ok(())
}

/// Maus zu den Netzausfaellen: Die Live-Ansicht zeigt gross, was der Lauf zuletzt als Hinweis
/// sagte, auch nach seinem Ende: etwa warum der Papiertag heute ausfiel. Der Hinweis steht auch
/// in der Zeile des Verlaufs.
#[test]
fn hinweis_des_laufs_steht_gross_in_der_live_ansicht() -> Result<(), String> {
    let d = Arc::new(Dienst::neu(
        ordner()?,
        TOKEN.into(),
        zeilen_starter(&[
            "[Sammler] Hole 2 Werte",
            "[Hinweis] Verbindung weg. Naechster Versuch um 17:48 Uhr (Versuch 2 von 4).",
            "[Hinweis] Papiertag heute ausgefallen. Die Verbindung kam bis 17:54 Uhr nicht wieder.",
            "[Sammler] Kurse NICHT verwendbar",
        ]),
        leeres_lagebild(),
    ));
    let t = donnerstag(17, 45)?;
    d.einreihen(Art::Kurse, Anlass::Hand)
        .map_err(|e| format!("Vorbedingung einreihen: {e}"))?;
    let (nr, kind) = some(d.schritt(t), "Start")?;
    d.verfolge(nr, kind, || donnerstag(17, 55).ok());
    let z = d.sperre();
    let a = some(z.verlauf.front(), "Verlauf")?;
    assert_eq!(
        a.hinweis.as_deref(),
        Some("Papiertag heute ausgefallen. Die Verbindung kam bis 17:54 Uhr nicht wieder.")
    );
    assert_eq!(a.protokoll.len(), 4, "Hinweise bleiben im Protokoll");
    let seite = seiten::live(TOKEN, "s", &z, t);
    assert!(
        seite.contains(
            "<p class=\"kopf halt\" id=\"hinweis\">Kurse abrufen: Papiertag heute ausgefallen."
        ),
        "{seite}"
    );
    // Am naechsten Tag steht der alte Hinweis nicht mehr oben, nur im Verlauf.
    let morgen = some(t.datum().and_then(|d| d.plus_tage(1)), "morgen")?;
    let morgen = some(Zeitpunkt::frankfurt(morgen, 12, 0, 0), "Zeit morgen")?;
    let seite = seiten::live(TOKEN, "s", &z, morgen);
    assert!(!seite.contains("id=\"hinweis\""), "{seite}");
    assert!(seite.contains("Papiertag heute ausgefallen"), "{seite}");
    Ok(())
}

#[test]
fn kurse_je_boerse_morgens_auch_am_feiertag_einer_boerse() -> Result<(), String> {
    let p = Zeitplan {
        an: true,
        ..morgenplan_test()
    };
    let beide = [Boerse::Xetra, Boerse::Nyse];
    for (monat, tag) in [(11, 5), (11, 26), (12, 24)] {
        let d = some(Datum::neu(2026, monat, tag), "Datum")?;
        assert_eq!(
            abrufe_fuer(&p, d, &beide),
            vec![(Boerse::Xetra, (7, 30)), (Boerse::Nyse, (7, 30))]
        );
        assert!(letzter_abruf_fuer(&p, d, Boerse::Nyse, &beide));
        assert!(!letzter_abruf_fuer(&p, d, Boerse::Xetra, &beide));
    }
    Ok(())
}

/// Ein Erledigt der frueheren Form (ohne Abruf je Boerse) wird gelesen; `kurse` war Xetra.
/// Geschrieben wird nur die neue Form.
#[test]
fn erledigt_frueherer_form() -> Result<(), String> {
    let tag = some(Datum::neu(2026, 11, 5), "Datum")?;
    let alt = Erledigt::lies("erledigt-v1\ntermine=2026-11-05\nkurse=2026-11-05\n")?;
    assert_eq!(
        alt,
        Erledigt {
            termine: Some(tag),
            kurse: Some(tag),
            kurse_nyse: None,
            ..Erledigt::default()
        }
    );
    assert_eq!(
        alt.als_text(),
        "erledigt-v4\ntermine=2026-11-05\nkurse=2026-11-05\nkurse_nyse=\npapier=\nxetra_abruf=\nnyse_abruf=\n"
    );
    assert!(Erledigt::lies("erledigt-v1\ntermine=\nkurse=\nkurse_nyse=\n").is_err());
    assert!(Erledigt::lies("erledigt-v2\ntermine=\nkurse=\n").is_err());
    // Aus der Form ohne Erfolg folgt kein Papiertag: kurse_ok bleibt leer.
    let v2 = Erledigt::lies("erledigt-v2\ntermine=\nkurse=2026-11-05\nkurse_nyse=\n")?;
    assert!(!v2.erfolgreich(Boerse::Xetra, tag));
    assert!(Erledigt::lies("erledigt-v3\ntermine=\nkurse=\nkurse_nyse=\nkurse_ok=\n").is_err());
    Ok(())
}

#[test]
fn ausschalten_entfernt_wartenden_abruf_und_wiedereinschalten_holt_ihn_einmal_nach()
-> Result<(), String> {
    let wurzel = eingeschaltet(ordner()?)?;
    let d = Arc::new(Dienst::neu(
        wurzel.clone(),
        TOKEN.into(),
        echo_starter(true),
        lagebild_mit_schluesseln(true),
    ));
    d.sperre().boersen = vec![Boerse::Xetra];
    let t = donnerstag(7, 31)?;
    let tag = some(t.datum(), "Handelstag")?;
    let (nr, kind) = some(d.schritt(t), "Terminabruf")?;
    assert!(d.schritt(t).is_none());
    assert_eq!(
        wartend(&d),
        vec![(Art::KurseVon(Boerse::Xetra), Anlass::Zeitplan)]
    );
    assert_eq!(d.sperre().erledigt.kurse, None);
    d.einreihen(Art::Nachrichten, Anlass::Hand)?;

    let aus = formular_post("/zeitplan", &format!("t={TOKEN}&aktion=aus"));
    assert!(text(&behandle(&d, &Attrappe::default(), &aus, t)).contains("m=zeitplan_aus"));
    assert_eq!(wartend(&d), vec![(Art::Nachrichten, Anlass::Hand)]);
    assert_eq!(
        d.sperre().laufend.as_ref().map(|a| a.art),
        Some(Art::Termine)
    );
    d.verfolge(nr, kind, || donnerstag(7, 31).ok());
    let (nr, kind) = some(d.schritt(t), "manuelle Schlagzeilen")?;
    d.verfolge(nr, kind, || donnerstag(7, 31).ok());
    assert!(d.schritt(t).is_none());
    assert_eq!(d.sperre().erledigt.kurse, None);

    let an = formular_post("/zeitplan", &format!("t={TOKEN}&aktion=an"));
    assert!(text(&behandle(&d, &Attrappe::default(), &an, t)).contains("m=zeitplan_an"));
    let (nr, kind) = some(d.schritt(t), "nachgeholter Kursabruf")?;
    assert_eq!(d.sperre().erledigt.kurse, Some(tag));
    d.verfolge(nr, kind, || donnerstag(7, 31).ok());
    let live = get(&d, "/", t);
    assert!(
        live.contains("Als Nächstes: Papiertag (Zeitplan)"),
        "{live}"
    );
    assert!(
        live.contains("<li><b>Papiertag</b> <span class=\"offen\">wartet</span>"),
        "{live}"
    );
    assert!(
        live.contains("Danach laut Zeitplan: Termine abrufen am Fr"),
        "{live}"
    );
    drop(d);

    let neu = nur_xetra(wurzel);
    let (nr, kind) = some(neu.schritt(t), "Papiertag nach Neustart")?;
    assert_eq!(
        neu.sperre().laufend.as_ref().map(|a| a.art),
        Some(Art::Papier)
    );
    neu.verfolge(nr, kind, || donnerstag(7, 31).ok());
    assert!(neu.schritt(t).is_none());
    Ok(())
}

/// Der Kindprozess liefert dieselbe lange Ausgabe unter Windows und Linux.

#[test]
fn fehlendes_diagnoselog_wird_erklaert_ohne_lauf_zu_verhindern() -> Result<(), String> {
    let wurzel = ordner()?;
    fs::write(wurzel.join("daten/protokolle"), "kein Ordner").map_err(|e| e.to_string())?;
    let d = dienst(wurzel.clone(), true);
    let t = donnerstag(12, 0)?;
    d.einreihen(Art::Termine, Anlass::Hand)
        .map_err(std::string::ToString::to_string)?;
    let (nr, kind) = some(d.schritt(t), "Start")?;
    d.verfolge(nr, kind, || donnerstag(12, 1).ok());
    assert_eq!(
        d.sperre().verlauf.front().and_then(|a| a.erfolg),
        Some(true)
    );
    assert!(get(&d, "/", t).contains("Diagnoseprotokoll nicht gespeichert"));
    let kennung = format!("{}-{nr}", t.sekunden());
    assert!(get(&d, &format!("/protokoll?lauf={kennung}"), t).starts_with("HTTP/1.1 404"));
    fs::remove_dir_all(wurzel).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn maskierungs_kindprozess() {
    if std::env::var_os("DEPOT_TEST_DIENST_MASKIERUNG").is_some() {
        println!("Anfang bleibt");
        println!("https://example.com/prices?token=unbekannt&x=1");
        println!("Debug EIMSCHLUESSEL");
        println!("Ende bleibt");
    }
}

#[test]
fn dienst_maskiert_vor_speicherung_und_meldet_das_zweite_netz() -> Result<(), String> {
    let wurzel = ordner()?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let d = Arc::new(Dienst::neu(
        wurzel.clone(),
        TOKEN.into(),
        Box::new(move |_, _| {
            let mut c = Command::new(&exe);
            c.args([
                "--exact",
                "dienstbetrieb::maskierungs_kindprozess",
                "--nocapture",
            ])
            .env("DEPOT_TEST_DIENST_MASKIERUNG", "ja");
            c
        }),
        leeres_lagebild(),
    ));
    let t = donnerstag(12, 0)?;
    let key = ["GEHEIM", "SCHLUESSEL", "WEITER"].concat();
    let speichern = formular_post(
        "/schluessel",
        &format!("t={TOKEN}&wer=anthropic&wert={key}"),
    );
    assert_eq!(
        status(&behandle(&d, &Attrappe::default(), &speichern, t))?,
        303
    );
    d.einreihen(Art::Termine, Anlass::Hand)
        .map_err(std::string::ToString::to_string)?;
    let (nr, kind) = some(d.schritt(t), "Start")?;
    d.verfolge(nr, kind, || donnerstag(12, 1).ok());
    let kennung = format!("{}-{nr}", t.sekunden());
    let log = depot_dienst::protokoll::lesen(&wurzel, &kennung)?;
    assert!(log.contains("prices?token=***&x=1"));
    assert!(!log.contains("token=unbekannt"));
    assert!(!log.contains("EIMSCHLUESSEL"));
    assert!(log.contains("Anfang bleibt") && log.contains("Ende bleibt"));
    assert!(get(&d, "/", t).contains("Ein Lauf hat Zugangsdaten ausgegeben"));
    let download = get(&d, &format!("/protokoll.txt?lauf={kennung}"), t);
    assert!(download.contains("prices?token=***&x=1"));
    assert!(download.contains("Ende bleibt"));
    fs::remove_dir_all(wurzel).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn vorbereiten_braucht_bestaetigung_und_automatik_braucht_schlagzeilen() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = Datum::neu(2026, 11, 7)
        .and_then(|d| Zeitpunkt::frankfurt(d, 12, 0, 0))
        .ok_or("Samstag")?;
    let seite = get(&d, "/bestaetigen?art=vorbereiten", t);
    assert!(
        seite.contains("echtes KI-Budget")
            && seite.contains("geprueft")
            && seite.contains("Abbrechen")
    );
    let post = formular_post("/ausfuehren", &format!("t={TOKEN}&art=vorbereiten"));
    assert!(
        text(&behandle(&d, &Attrappe::default(), &post, t)).contains("m=vorbereitung_bestaetigen")
    );
    assert!(wartend(&d).is_empty());
    let plan = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&aktion=speichern&termine=17%3A40&kurse=07%3A30&vorbereitung_an=ja"),
    );
    let antwort = text(&behandle(&d, &Attrappe::default(), &plan, t));
    assert!(antwort.contains("m=vorbereitung_quellen"));
    assert!(get(&d, &ort(&antwort), t).contains("name=\"vorbereitung_an\" value=\"ja\" checked"));
    assert!(!d.sperre().zeitplan.vorbereitung.aktiv());
    let post = formular_post(
        "/ausfuehren",
        &format!("t={TOKEN}&art=vorbereiten&geprueft=ja"),
    );
    assert!(text(&behandle(&d, &Attrappe::default(), &post, t)).contains("m=auftrag"));
    assert_eq!(wartend(&d), vec![(Art::Vorbereiten, Anlass::Hand)]);
    Ok(())
}

#[test]
fn vorbereitung_zeigt_frankfurter_zeitfenster_und_prueft_alte_bestaetigungen() -> Result<(), String>
{
    let d = dienst(ordner()?, true);
    let samstag = some(Datum::neu(2026, 11, 7), "Samstag")?;
    let offen = some(Zeitpunkt::frankfurt(samstag, 22, 59, 0), "Zeit vor Schluss")?;
    let geschlossen = some(Zeitpunkt::frankfurt(samstag, 23, 0, 0), "Zeit nach Schluss")?;
    let ziel = "/bestaetigen?art=vorbereiten";
    let post = formular_post(
        "/ausfuehren",
        &format!("t={TOKEN}&art=vorbereiten&geprueft=ja"),
    );
    assert!(d.schritt(offen).is_none());
    assert!(get(&d, "/teil/live", offen).contains(ziel));
    assert!(get(&d, ziel, offen).contains("echtes KI-Budget"));
    let seite = get(&d, "/teil/live", geschlossen);
    assert!(seite.contains("bis vor 23:00 Uhr Frankfurter Zeit"));
    assert!(!seite.contains(ziel));
    for (zeit, grund) in [
        (geschlossen, "vorbereitung_uhrzeit"),
        (donnerstag(12, 0)?, "vorbereitung_handelstag"),
    ] {
        assert!(get(&d, ziel, zeit).contains(grund));
        let antwort = text(&behandle(&d, &Attrappe::default(), &post, zeit));
        assert!(antwort.contains(grund));
        assert!(wartend(&d).is_empty());
    }
    Ok(())
}

#[test]
fn vorbereitung_vor_start_ausschalten_und_alte_zeitplaene_bleiben_aus() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = Datum::neu(2026, 11, 7)
        .and_then(|d| Zeitpunkt::frankfurt(d, 12, 0, 0))
        .ok_or("Samstag")?;
    d.einreihen(Art::Vorbereiten, Anlass::Zeitplan)?;
    assert!(d.schritt(t).is_none());
    assert!(d.sperre().laufend.is_none());
    let alter = Zeitplan::default().als_text();
    assert!(!Zeitplan::lies(&alter)?.vorbereitung.aktiv());
    let plan = Zeitplan {
        nachrichten_an: true,
        vorbereitung: depot_dienst::zeitplan::VorbereitungsPlan::An,
        ..Zeitplan::default()
    };
    assert_eq!(Zeitplan::lies(&plan.als_text())?, plan);
    assert!(Zeitplan::lies(&(plan.als_text() + "vorbereitung_an=ja\n")).is_err());
    Ok(())
}

#[test]
fn rss_checkbox_bleibt_bei_fehlern_erhalten_und_wird_dauerhaft_gespeichert() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let u = Attrappe::default();
    let t = donnerstag(12, 0)?;
    let falsch = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&aktion=speichern&termine=kaputt&kurse=07%3A30&nachrichten_an=ja"),
    );
    let antwort = text(&behandle(&d, &u, &falsch, t));
    assert!(antwort.contains("nachrichten_an=ja"), "{antwort}");
    assert!(!d.sperre().zeitplan.nachrichten_an);
    let richtig = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&aktion=speichern&termine=17%3A40&kurse=07%3A30&nachrichten_an=ja"),
    );
    assert!(text(&behandle(&d, &u, &richtig, t)).contains("m=zeitplan#zeitplan"));
    assert!(d.sperre().zeitplan.nachrichten_an);
    let inhalt =
        fs::read_to_string(wurzel.join("daten/zeitplan.txt")).map_err(|e| e.to_string())?;
    assert!(Zeitplan::lies(&inhalt)?.nachrichten_an);
    let seite = text(&behandle(
        &d,
        &u,
        &anfrage("GET", "/einrichtung", &[("Host", HOST)], ""),
        t,
    ));
    assert!(seite.contains("name=\"nachrichten_an\""));
    Ok(())
}

#[test]
fn wartende_schlagzeilen_starten_nach_ausschalten_nicht_mehr() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    d.einreihen(Art::Nachrichten, Anlass::Zeitplan)
        .map_err(str::to_string)?;
    assert!(d.schritt(donnerstag(12, 0)?).is_none());
    assert!(d.sperre().laufend.is_none());
    Ok(())
}

/// Auch eine alte, unspezifische Kurssperre kann ohne Aufheben untersucht werden.
#[test]
fn kurssperre_pruefabruf_ist_getrennt_und_doppelklickfest() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let umw = Attrappe::default();
    let t = donnerstag(18, 0)?;
    let seite = anfrage("GET", "/einrichtung", &[("Host", HOST)], "");
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(!s.contains("value=\"kursdiagnose\""));
    let halt = kostenschutz::kurse_pfad(&wurzel);
    let grund = "Sicherheitsabbruch: Die Kursquelle gab den Schluessel zurueck\n";
    fs::write(&halt, grund).map_err(|e| e.to_string())?;
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(s.contains("value=\"kursdiagnose\""));
    assert!(s.contains("Kurssperre sicher untersuchen"));
    assert!(s.contains("Sperre bleibt bestehen") && s.contains("Quellenkontingent"));
    let post = formular_post("/ausfuehren", &format!("t={TOKEN}&art=kursdiagnose"));
    for _ in 0..2 {
        let _ = behandle(&d, &umw, &post, t);
    }
    assert_eq!(d.sperre().warteschlange.len(), 1);
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(s.contains("disabled>Kurssperre sicher untersuchen"));
    assert_eq!(fs::read_to_string(&halt).map_err(|e| e.to_string())?, grund);
    assert_eq!(Art::Kursdiagnose.argumente(), &["kurse", "diagnose"]);
    assert_eq!(Art::aus_kurz("kursdiagnose"), Some(Art::Kursdiagnose));
    Ok(())
}

#[test]
fn hinweis_meldet_neuen_grund_einmal_und_nach_rueckkehr_wieder() -> Result<(), String> {
    let wurzel = ordner()?;
    let gruende = Gruende::default();
    setze(&gruende, &["Notbremse: Topf A unter 70 %"]);
    let (d, g) = hinweis_dienst(wurzel.clone(), &gruende, false);
    schritte(&d, &[0, 10, 20, 30])?;
    let gesehen = g.lock().unwrap_or_else(PoisonError::into_inner).clone();
    assert_eq!(gesehen.len(), 1);
    assert_eq!(
        gesehen.first().map(|(t, x)| (t.as_str(), x.as_str())),
        Some((
            "DepotSchlopf: Eingriff nötig",
            "Notbremse: Topf A unter 70 % Details auf http://127.0.0.1:47410/"
        ))
    );
    let h = depot_kern::abdruck::sha256_hex("Notbremse: Topf A unter 70 %".as_bytes());
    assert!(hinweise_datei(&wurzel).starts_with(&format!("{h};2026-11-07T11:00:10Z")));
    setze(&gruende, &[]);
    schritte(&d, &[100, 110])?;
    assert_eq!(hinweise_datei(&wurzel), "");
    setze(&gruende, &["Notbremse: Topf A unter 70 %"]);
    schritte(&d, &[200, 210])?;
    assert_eq!(gezeigt(&g).len(), 2);
    Ok(())
}

#[test]
fn buendelt_in_60_sekunden() -> Result<(), String> {
    let wurzel = ordner()?;
    let gruende = Gruende::default();
    let (d, g) = hinweis_dienst(wurzel, &gruende, false);
    setze(&gruende, &["A"]);
    schritte(&d, &[0])?;
    setze(&gruende, &["A", "B"]);
    schritte(&d, &[5])?;
    setze(&gruende, &["A", "B", "C"]);
    schritte(&d, &[10, 20])?;
    assert_eq!(
        gezeigt(&g),
        vec!["A (und 2 weitere) Details auf http://127.0.0.1:47410/".to_string()]
    );
    // Ein weiterer Grund kurz danach wartet, bis 60 Sekunden seit dem letzten Hinweis um sind.
    setze(&gruende, &["A", "B", "C", "D"]);
    schritte(&d, &[30, 40, 50, 60])?;
    assert_eq!(gezeigt(&g).len(), 1);
    schritte(&d, &[70])?;
    assert_eq!(
        gezeigt(&g).last().map(String::as_str),
        Some("D Details auf http://127.0.0.1:47410/")
    );
    Ok(())
}

#[test]
fn ausgefallener_tag_einmal() -> Result<(), String> {
    let wurzel = ordner()?;
    let gruende = Gruende::default();
    let (d, g) = hinweis_dienst(wurzel, &gruende, true);
    schritte(&d, &[0, 10, 20, 30, 40])?;
    assert_eq!(
        gezeigt(&g),
        vec![
            "Papiertag am 09.11.2026 ausgefallen. Details auf http://127.0.0.1:47410/".to_string()
        ]
    );
    Ok(())
}

#[test]
fn kaputte_hinweisdatei_gilt_leer() -> Result<(), String> {
    let wurzel = ordner()?;
    fs::write(wurzel.join("daten").join("hinweise.txt"), "xyz")
        .map_err(|e| format!("Vorbedingung Datei: {e}"))?;
    let gruende = Gruende::default();
    setze(&gruende, &["A"]);
    let (d, g) = hinweis_dienst(wurzel.clone(), &gruende, false);
    schritte(&d, &[0, 10])?;
    assert_eq!(gezeigt(&g).len(), 1);
    let h = depot_kern::abdruck::sha256_hex(b"A");
    assert_eq!(
        hinweise_datei(&wurzel),
        format!("{h};2026-11-07T11:00:10Z\n")
    );
    Ok(())
}

#[test]
fn hinweis_schalter_aus_pflegt_trotzdem() -> Result<(), String> {
    let wurzel = ordner()?;
    let plan = Zeitplan {
        hinweis: depot_dienst::zeitplan::HinweisPlan::Aus,
        ..Zeitplan::default()
    };
    fs::write(wurzel.join("daten").join("zeitplan.txt"), plan.als_text())
        .map_err(|e| format!("Vorbedingung Zeitplan: {e}"))?;
    let gruende = Gruende::default();
    setze(&gruende, &["A"]);
    let (d, g) = hinweis_dienst(wurzel.clone(), &gruende, false);
    schritte(&d, &[0, 10, 20])?;
    assert!(gezeigt(&g).is_empty());
    let h = depot_kern::abdruck::sha256_hex(b"A");
    assert!(hinweise_datei(&wurzel).starts_with(&h));
    Ok(())
}

#[test]
fn z6_gespeicherter_plan_2210_angehalten() -> Result<(), String> {
    let wurzel = ordner()?;
    let pfad = wurzel.join("daten/zeitplan.txt");
    let alt = Zeitplan {
        an: true,
        kurse: (22, 10),
        ..morgenplan_test()
    };
    fs::write(&pfad, alt.als_text()).map_err(|e| e.to_string())?;
    let d = dienst(wurzel.clone(), true);
    assert_eq!(d.sperre().zeitplan, alt);
    let jetzt = donnerstag(22, 10)?;
    let tag = some(jetzt.frankfurt_tag(), "Tag")?;
    let beide = Boerse::ALLE;
    assert_eq!(
        faellig_fuer(&alt, &Erledigt::default(), tag, (22, 10), &beide),
        Some(Art::Termine)
    );
    let erledigt = Erledigt {
        termine: Some(tag),
        ..Erledigt::default()
    };
    assert_eq!(faellig_fuer(&alt, &erledigt, tag, (22, 10), &beide), None);
    assert!(get(&d, "/teil/live", jetzt).contains("Kursabruf 22:10 liegt außerhalb 07:00–08:30"));
    Ok(())
}

#[test]
fn z6_abendplan_sichert_weiter() -> Result<(), String> {
    let wurzel = ordner()?;
    let abend = Zeitplan {
        an: true,
        kurse: (22, 10),
        ..morgenplan_test()
    };
    fs::write(wurzel.join("daten/zeitplan.txt"), abend.als_text()).map_err(|e| e.to_string())?;
    let ziel = if cfg!(windows) {
        r"E:\"
    } else {
        "/DepotSicherung"
    };
    fs::write(wurzel.join("daten/sicherung_ziel.txt"), ziel).map_err(|e| e.to_string())?;
    let d = dienst(wurzel, true);
    assert!(d.schritt(donnerstag(23, 30)?).is_some());
    assert_eq!(
        d.sperre().laufend.as_ref().map(|a| a.art),
        Some(Art::Sicherung)
    );
    Ok(())
}

#[test]
fn z6_formular_lehnt_0900_ab() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let vorher = d.sperre().zeitplan;
    let a = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&aktion=speichern&termine=07%3A20&kurse=09%3A00"),
    );
    let antwort = text(&behandle(&d, &Attrappe::default(), &a, donnerstag(8, 0)?));
    assert!(antwort.contains("m=zeitplan_kursfenster"));
    assert_eq!(d.sperre().zeitplan, vorher);
    Ok(())
}

#[test]
fn geschlossene_xetra_bestimmt_den_nyse_schluss_nicht() -> Result<(), String> {
    let tag = some(Datum::neu(2026, 12, 24), "Heiligabend")?;
    let boersen = [Boerse::Xetra, Boerse::Nyse];
    assert!(!Boerse::Xetra.handelstag(tag) && Boerse::Nyse.handelstag(tag));
    assert_eq!(
        depot_kern::liste::erster_schluss(tag, &boersen),
        Boerse::Nyse.schluss(tag)
    );
    let zeit = some(Zeitpunkt::frankfurt(tag, 18, 0, 0), "Frankfurt")?;
    assert!(depot_kern::liste::morgenlauf(zeit));
    assert!(!depot_kern::liste::morgenlauf(some(
        Boerse::Nyse.schluss(tag),
        "NYSE-Schluss"
    )?));
    Ok(())
}

#[test]
fn betriebsleiste_zaehlt_nur_auftraege_nach_dem_naechsten() -> Result<(), String> {
    let d = dienst(ordner()?, false);
    let jetzt = donnerstag(17, 39)?;
    d.einreihen(Art::Termine, Anlass::Hand)?;
    let html = get(&d, "/teil/kopf", jetzt);
    assert!(html.contains("Als Nächstes: Termine"));
    assert!(!html.contains("Danach warte"));
    d.einreihen(Art::Nachrichten, Anlass::Hand)?;
    assert!(get(&d, "/teil/kopf", jetzt).contains("Danach wartet 1 Auftrag."));
    d.einreihen(Art::Kurse, Anlass::Hand)?;
    assert!(get(&d, "/teil/kopf", jetzt).contains("Danach warten 2 Aufträge."));
    Ok(())
}

#[test]
fn betriebsleiste_zeigt_kurssperre_mit_art_zeit_und_grund() -> Result<(), String> {
    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    let d = dienst(wurzel, true);
    let zeit = donnerstag(12, 0)?;
    let vorher = get(&d, "/teil/kopf", zeit);
    assert!(!vorher.contains("Kurssperre seit"));
    let schutz = depot_lage::kurssperre::sperren(&daten)?;
    schutz.sicherheit(
        depot_lage::kurssperre::Quelle::AlphaVantage,
        Some(zeit),
        "Prüfgrund",
    )?;
    drop(schutz);
    let html = get(&d, "/teil/kopf", zeit);
    assert!(html.contains("Kurssperre seit") && html.contains("Prüfgrund · sicherheit"));
    assert!(html.contains(&depot_lage::zeit_text(zeit)));
    assert!(html.contains("data-otter=\"pfote\""));
    assert!(html.contains("/einrichtung#kurssperre"));
    Ok(())
}

#[test]
fn automatisch_aufgehobene_kurssperre_erzeugt_einen_hinweis() -> Result<(), String> {
    use depot_lage::kurssperre;
    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    let zeit = donnerstag(12, 0)?;
    fs::write(
        daten.join(kurssperre::KURSSPERRE),
        "Sicherheitsabbruch: Die Kursquelle gab den Schlüssel zurück.",
    )
    .map_err(|e| e.to_string())?;
    let schutz = kurssperre::sperren(&daten)?;
    let sperre = some(kurssperre::lies(&daten)?, "Altsperre")?;
    schutz.fehlalarm_aufheben(&sperre, zeit)?;
    drop(schutz);
    let gruende = Gruende::default();
    let (d, meldungen) = hinweis_dienst(wurzel.clone(), &gruende, false);
    assert!(d.schritt(zeit).is_none());
    assert!(d.schritt(donnerstag(12, 1)?).is_none());
    assert_eq!(
        gezeigt(&meldungen)
            .iter()
            .filter(|s| s.contains("Kurssperre aufgehoben: Fehlalarm"))
            .count(),
        1
    );
    drop(d);
    let (d, meldungen) = hinweis_dienst(wurzel, &gruende, false);
    assert!(d.schritt(donnerstag(12, 2)?).is_none());
    assert!(gezeigt(&meldungen).is_empty());
    Ok(())
}

#[test]
fn formular_ohne_rohdaten() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let html = get(&d, "/", donnerstag(12, 0)?);
    assert!(html.contains("action=\"/export\""));
    assert!(!html.contains("name=\"rohdaten\""));
    assert!(!html.contains("name=\"ki_texte\""));
    Ok(())
}

#[test]
fn export_enthaelt_nie_rohdaten() -> Result<(), String> {
    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    depot_journal::Journal::einrichten(&daten.join("journal.txt")).map_err(|e| format!("{e:?}"))?;
    fs::create_dir_all(daten.join("gespraeche")).map_err(|e| e.to_string())?;
    fs::write(daten.join("gespraeche/2026-11-05.jsonl"), "{}\n").map_err(|e| e.to_string())?;
    let d = dienst(wurzel, true);
    let post = formular_post(
        "/export",
        &format!("t={TOKEN}&zeitraum=woche&rohdaten=ja&ki_texte=ja"),
    );
    let antwort = behandle(&d, &Attrappe::default(), &post, donnerstag(12, 0)?);
    assert_eq!(status(&antwort)?, 200);
    let archiv = text(&antwort);
    assert!(archiv.contains("MANIFEST.txt") && archiv.contains("03_journal.md"));
    for muster in [
        "rohdaten/",
        "protokolle/",
        ".jsonl",
        ".log",
        "journal.txt",
        "anbieter.txt",
    ] {
        assert!(!archiv.contains(muster), "{muster}");
    }
    Ok(())
}

#[test]
fn pruefbericht_steht_zuerst_und_standard_ist_diese_woche() -> Result<(), String> {
    let wurzel = ordner()?;
    depot_journal::Journal::einrichten(&wurzel.join("daten/journal.txt"))
        .map_err(|e| format!("{e:?}"))?;
    let d = dienst(wurzel, true);
    let zeit = donnerstag(12, 0)?;
    let html = get(&d, "/", zeit);
    assert!(html.contains("Diese Woche exportieren"));
    let eigenes_formular = html
        .split("</form>")
        .find(|teil| teil.contains("name=\"von\""))
        .ok_or("Formular für Von/Bis fehlt")?;
    assert!(eigenes_formular.contains("name=\"zeitraum\" value=\"frei\""));
    assert!(eigenes_formular.contains("name=\"bis\""));
    assert_eq!(eigenes_formular.matches("type=\"submit\"").count(), 1);
    let antwort = behandle(
        &d,
        &Attrappe::default(),
        &formular_post("/export", &format!("t={TOKEN}")),
        zeit,
    );
    assert_eq!(status(&antwort)?, 200);
    let antwort = text(&antwort);
    let (_, zip) = antwort.split_once("\r\n\r\n").ok_or("ZIP fehlt")?;
    assert!(zip.starts_with("PK"));
    assert!(antwort.contains("DepotSchlopf_Analyse_2026-11-02_bis_2026-11-05.zip"));
    assert!(
        zip.find("PRUEFBERICHT.md")
            .zip(zip.find("01_ueberblick.md"))
            .is_some_and(|(bericht, ueberblick)| bericht < ueberblick)
    );
    Ok(())
}

#[test]
fn heute_exportieren_liefert_zip_ohne_schreibzugriff() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let jetzt = donnerstag(12, 0)?;
    let daten = wurzel.join("daten");
    let journal = daten.join("journal.txt");
    let vermerk = daten.join("journal.vermerk");
    depot_journal::Journal::einrichten(&journal).map_err(|e| format!("{e:?}"))?;
    let lies = |p| fs::read(p).map_err(|e| e.to_string());
    let vorher = (lies(&journal)?, lies(&vermerk)?);
    let umwelt = Attrappe::default();
    let live = behandle(
        &d,
        &umwelt,
        &anfrage("GET", "/", &[("Host", HOST)], ""),
        jetzt,
    );
    let live = text(&live);
    let (fest, _) = live
        .split_once("data-teil=\"/teil/live\"")
        .ok_or("Live-Teil fehlt")?;
    assert!(fest.contains("action=\"/export\" data-download"));
    let teil = anfrage("GET", "/teil/live", &[("Host", HOST)], "");
    assert!(!text(&behandle(&d, &umwelt, &teil, jetzt)).contains("action=\"/export\""));
    let post = formular_post("/export", &format!("t={TOKEN}&zeitraum=heute"));
    let lesen = behandle(&d, &umwelt, &post, jetzt);
    assert_eq!(status(&lesen)?, 200);
    let antwort = text(&lesen);
    assert!(antwort.contains("Content-Type: application/zip"));
    assert!(antwort.contains("filename=\"DepotSchlopf_Analyse_2026-11-05.zip\""));
    assert!(antwort.contains("\r\n\r\nPK"));
    for datei in [
        "LIESMICH.md",
        "01_ueberblick.md",
        "03_journal.md",
        "ausfuehrungen.csv",
        "MANIFEST.txt",
    ] {
        assert!(antwort.contains(datei));
    }
    let zeitraum = formular_post(
        "/export",
        &format!("t={TOKEN}&zeitraum=frei&von=2026-11-03&bis=2026-11-05"),
    );
    let lesen = behandle(&d, &umwelt, &zeitraum, jetzt);
    assert_eq!(status(&lesen)?, 200);
    assert!(text(&lesen).contains("DepotSchlopf_Analyse_2026-11-03_bis_2026-11-05.zip"));
    let zukunft = formular_post(
        "/export",
        &format!("t={TOKEN}&zeitraum=frei&von=2026-11-03&bis=2026-11-06"),
    );
    assert_eq!(status(&behandle(&d, &umwelt, &zukunft, jetzt))?, 400);
    assert_eq!((lies(&journal)?, lies(&vermerk)?), vorher);

    let sperre = depot_journal::Journal::oeffnen(&journal).map_err(|e| format!("{e:?}"))?;
    assert_eq!(status(&behandle(&d, &umwelt, &post, jetzt))?, 409);
    drop(sperre);

    let fremd = anfrage(
        "POST",
        "/export",
        &[("Host", HOST), ("Origin", "https://example.org")],
        &format!("t={TOKEN}"),
    );
    assert_eq!(status(&behandle(&d, &umwelt, &fremd, jetzt))?, 403);
    let get = anfrage("GET", "/export", &[("Host", HOST)], "");
    assert_eq!(status(&behandle(&d, &umwelt, &get, jetzt))?, 405);
    fs::remove_dir_all(wurzel).map_err(|e| e.to_string())
}

#[test]
fn ohne_ziel_kein_lauf() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let zeit = donnerstag(23, 30)?;
    assert!(d.schritt(zeit).is_none());
    assert!(get(&d, "/teil/live", zeit).contains("Sicherung: aus"));
    assert!(get(&d, "/einrichtung", zeit).contains("Jetzt sichern"));
    Ok(())
}

#[test]
fn menue_ueberall() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    for pfad in ["/", "/gespraeche", "/einrichtung"] {
        assert!(
            get(&d, pfad, donnerstag(12, 0)?).contains("href=\"/depot\""),
            "{pfad}"
        );
    }
    Ok(())
}

#[test]
fn zeitraum_links() -> Result<(), String> {
    let alt = some(depot_kern::Datum::neu(2026, 7, 13), "Alter Abschluss")?;
    let neu = some(depot_kern::Datum::neu(2026, 10, 13), "Neuer Abschluss")?;
    let d = depot_mit_verlauf(&[alt, neu])?;
    let jetzt = some(
        Zeitpunkt::frankfurt(
            some(depot_kern::Datum::neu(2026, 11, 13), "Heute")?,
            18,
            0,
            0,
        ),
        "Uhrzeit",
    )?;
    let html = get(&d, "/depot?zeitraum=3m", jetzt);
    assert!(html.contains("/depot?zeitraum=3m\" aria-current=\"true\">3 Monate</a>"));
    assert!(html.contains("<th scope=\"row\">13.10.2026</th>"));
    assert!(!html.contains("<th scope=\"row\">13.07.2026</th>"));
    Ok(())
}

#[test]
fn zeitraum_leer() -> Result<(), String> {
    let alt = some(depot_kern::Datum::neu(2026, 9, 10), "Alter Abschluss")?;
    let d = depot_mit_verlauf(&[alt])?;
    let jetzt = some(
        Zeitpunkt::frankfurt(
            some(depot_kern::Datum::neu(2026, 11, 13), "Heute")?,
            18,
            0,
            0,
        ),
        "Uhrzeit",
    )?;
    let html = get(&d, "/depot?zeitraum=1m", jetzt);
    assert!(html.contains("Im gewählten Zeitraum gibt es noch keinen abgeschlossenen Papiertag."));
    assert!(!html.contains("id=\"depot-diagramm-titel\""));
    Ok(())
}

#[test]
fn export_schwaerzt_sec_kontakt_ohne_at() -> Result<(), String> {
    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    let journal = daten.join("journal.txt");
    depot_journal::Journal::einrichten(&journal).map_err(|e| format!("{e:?}"))?;
    let mut buch = depot_journal::Journal::oeffnen(&journal).map_err(|e| format!("{e:?}"))?;
    let jetzt = donnerstag(12, 0)?;
    let tag = some(jetzt.frankfurt_tag(), "Exporttag")?;
    buch.beobachte(tag, "Testperson Beispiel meldet Befund")
        .map_err(|e| format!("{e:?}"))?;
    drop(buch);
    let kontakt = daten.join("sec_kontakt.txt");
    let d = dienst(wurzel, true);
    let post = formular_post("/export", &format!("t={TOKEN}&zeitraum=heute"));
    let umwelt = Attrappe::default();
    let ohne = behandle(&d, &umwelt, &post, jetzt);
    fs::write(&kontakt, "  Testperson Beispiel  \r\n").map_err(|e| e.to_string())?;
    let mit = behandle(&d, &umwelt, &post, jetzt);
    assert_eq!(status(&mit)?, 200);
    assert!(text(&mit).contains("Content-Type: application/zip"));
    assert_ne!(mit, ohne, "Der SEC-Kontakt muss den Export verändern");
    Ok(())
}

#[test]
fn export_ohne_zip_bei_unlesbarem_sec_kontakt() -> Result<(), String> {
    let wurzel = ordner()?;
    fs::create_dir(wurzel.join("daten/sec_kontakt.txt")).map_err(|e| e.to_string())?;
    let d = dienst(wurzel, true);
    let post = formular_post("/export", &format!("t={TOKEN}&zeitraum=heute"));
    let antwort = behandle(&d, &Attrappe::default(), &post, donnerstag(12, 0)?);
    assert_eq!(status(&antwort)?, 409);
    let antwort = text(&antwort);
    assert!(antwort.contains("SEC-Kontakt nicht lesbar"));
    assert!(!antwort.contains("application/zip"));
    Ok(())
}

#[test]
fn startgeld_gleich_fuer_kennzahl_und_diagramm() -> Result<(), String> {
    use depot_journal::{Journal, KursBeleg};
    use depot_kern::{Cent, Herkunft, Isin, Kurs, Topf, TopfId};

    let wurzel = ordner()?;
    let pfad = wurzel.join("daten/journal.txt");
    Journal::einrichten(&pfad).map_err(|e| format!("{e:?}"))?;
    let mut j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
    for (id, geld) in [(TopfId::A, Cent(600_000)), (TopfId::B, Cent(400_000))] {
        let topf = Topf::neu(id, geld).map_err(|e| format!("{e:?}"))?;
        j.setze_topf(&topf).map_err(|e| format!("{e:?}"))?;
    }
    // Der erste abgeschlossene Stand enthält bereits eine Minderung von 1,50 €.
    let mut a = Topf::neu(TopfId::A, Cent(589_850)).map_err(|e| format!("{e:?}"))?;
    let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
    a.setze_bestand(isin.clone(), 1);
    j.setze_topf(&a).map_err(|e| format!("{e:?}"))?;
    let jetzt = donnerstag(18, 0)?;
    let tag = some(jetzt.frankfurt_tag(), "Kurstag")?;
    for datum in [some(Boerse::Xetra.bezugstag(tag), "Vortag")?, tag] {
        let schluss = some(Boerse::Xetra.schluss(datum), "Schluss")?;
        let kurs = Kurs::neu(Cent(10_000), "EUR", datum, schluss, Herkunft::Quelle)
            .map_err(|e| format!("{e:?}"))?;
        let beleg = KursBeleg::neu("alphavantage", jetzt, b"Startgeldprobe", isin.clone(), kurs)
            .map_err(|e| format!("{e:?}"))?;
        j.belege_kurs(&beleg).map_err(|e| format!("{e:?}"))?;
    }
    j.schliesse_ab(tag, Cent(1_000_000), &"a".repeat(64))
        .map_err(|e| format!("{e:?}"))?;
    drop(j);
    let html = get(&dienst(wurzel, true), "/depot", jetzt);
    assert!(html.contains(
        "Seit Start <span data-schluessel=\"depot-start\" data-wert=\"-150\">-1,50 €</span>"
    ));
    assert!(
        html.contains("Startgeld 10.000,00 € (dünn gestrichelt)"),
        "{html}"
    );
    assert!(html.contains("stroke-dasharray=\"8 5\""), "{html}");
    Ok(())
}

#[test]
fn simulation_verlangt_kostenbestaetigung_und_reiht_doppelklick_nur_einmal_ein()
-> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = Datum::neu(2026, 11, 15)
        .and_then(|tag| Zeitpunkt::frankfurt(tag, 12, 0, 0))
        .ok_or("Sonntag")?;
    assert!(get(&d, "/", t).contains("/bestaetigen?art=simulation"));
    let seite = get(&d, "/bestaetigen?art=simulation", t);
    assert!(seite.contains("Maximale KI-Reservierung") && seite.contains("geprueft"));
    let ohne = formular_post("/ausfuehren", &format!("t={TOKEN}&art=simulation"));
    assert!(text(&behandle(&d, &Attrappe::default(), &ohne, t)).contains("simulation_bestaetigen"));
    assert!(wartend(&d).is_empty());
    let mit = formular_post(
        "/ausfuehren",
        &format!("t={TOKEN}&art=simulation&geprueft=ja"),
    );
    d.von_hand(Art::Termine, t)?;
    let live = get(&d, "/", t);
    assert!(live.contains("KI-Simulation starten · Dienst belegt"));
    assert!(!live.contains("/bestaetigen?art=simulation"));
    assert!(get(&d, "/bestaetigen?art=simulation", t).contains("m=simulation_belegt"));
    assert!(text(&behandle(&d, &Attrappe::default(), &mit, t)).contains("m=simulation_belegt"));
    assert_eq!(wartend(&d), vec![(Art::Termine, Anlass::Hand)]);
    let d = dienst(ordner()?, true);
    assert!(text(&behandle(&d, &Attrappe::default(), &mit, t)).contains("m=auftrag#ki-simulation"));
    assert!(text(&behandle(&d, &Attrappe::default(), &mit, t)).contains("m=simulation_belegt"));
    assert_eq!(wartend(&d), vec![(Art::Simulation, Anlass::Hand)]);
    Ok(())
}
