use super::*;

#[test]
fn auftrag_laeuft_mit_protokoll_und_eindeutiger_kennung() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(12, 0)?;
    d.einreihen(Art::Termine, Anlass::Hand)
        .map_err(|e| format!("Vorbedingung einreihen: {e}"))?;
    let (nr, kind) = some(d.schritt(t), "Start")?;
    assert!(d.sperre().laufend.is_some());
    assert!(
        d.einreihen(Art::Termine, Anlass::Hand).is_err(),
        "laeuft schon"
    );
    // Waehrend ein Auftrag laeuft, wird kein Lagebild gerechnet.
    assert!(d.sperre().lage.is_none());
    let detail = format!("<details id=\"protokoll-{}-{nr}\"", t.sekunden());
    assert!(get(&d, "/teil/live", t).contains(&format!("{detail} open>")));
    d.verfolge(nr, kind, || donnerstag(12, 1).ok());
    let z = d.sperre();
    assert!(z.laufend.is_none());
    let a = some(z.verlauf.front(), "Verlauf")?;
    assert_eq!(a.erfolg, Some(true));
    assert_eq!(a.protokoll, vec!["lauf termine".to_string()]);
    assert_eq!(a.kennung, format!("{}-{nr}", t.sekunden()));
    let seite = seiten::live(TOKEN, "s", &z, t);
    assert!(seite.contains(&format!("{detail}>")));
    assert!(seite.contains("data-protokoll-kopieren>Auszug kopieren"));
    assert!(seite.contains("<pre class=\"konsole\" tabindex=\"0\">lauf termine\n</pre>"));
    drop(z);
    assert!(d.schritt(t).is_none());
    assert!(d.sperre().lage.is_some(), "danach das Lagebild");
    Ok(())
}

#[test]
fn live_zeitstrahl_zeigt_boersenzeiten_und_bedingten_papiertag() -> Result<(), String> {
    let d = dienst(ordner()?, false);
    let t = donnerstag(7, 19)?;
    let tag = some(t.datum(), "Tag des Zeitplans")?;
    let html = get(&d, "/teil/live", t);
    assert!(!html.contains("<h2>Zeitstrahl</h2><p>Zeitplan aus"));
    assert_eq!(get(&d, "/", t).matches("Zeitplan aus.").count(), 1);
    {
        let mut z = d.sperre();
        z.boersen = vec![Boerse::Xetra, Boerse::Nyse];
        z.zeitplan.an = true;
        z.zeitplan.termine = (7, 20);
        z.zeitplan.kurse = (7, 30);
        z.zeitplan.papier_danach = true;
        z.zeitplan.nachrichten_an = true;
    }
    let html = get(&d, "/teil/live", t);
    let termine = some(html.find("<b>07:20 Uhr</b> · Termine"), "Terminabruf")?;
    let xetra = some(html.find("<b>07:30 Uhr</b> · Kurse Xetra"), "Xetra-Abruf")?;
    let nyse = some(html.find("<b>07:30 Uhr</b> · Kurse US-Börsen"), "US-Abruf")?;
    assert!(termine < xetra && xetra < nyse);
    assert!(html.contains("Kurse Xetra abrufen · ohne KI"));
    assert!(html.contains(&format!("Dachs ({})", depot_kern::modelle::MODELL_DACHS)));
    assert!(html.contains(&format!("Maus ({})", depot_kern::modelle::MODELL_MAUS)));
    assert!(
        html.contains("nach dem letzten Kursabruf, wenn Kurse für alle gehaltenen Werte vorliegen")
    );
    assert!(html.contains("täglich von 08:00 bis vor 23:00 Uhr"));
    assert!(!html.contains("danach optional kostenlose Gemini-Zuarbeit"));
    let schalter = depot_lage::gespraeche::gemini_pfad(&d.wurzel.join("daten"));
    fs::write(schalter, "gratisprojekt=ja\n").map_err(|e| e.to_string())?;
    assert!(get(&d, "/teil/live", t).contains("danach optional kostenlose Gemini-Zuarbeit"));

    {
        let mut z = d.sperre();
        z.erledigt.kurse = Some(tag);
        z.erledigt.kurse_nyse = Some(tag);
    }
    let html = get(&d, "/teil/live", donnerstag(8, 0)?);
    assert!(html.contains("entfällt: kein erfolgreicher Kursabruf"));
    d.sperre().erledigt.termine = Some(tag);
    let html = get(&d, "/teil/live", donnerstag(8, 0)?);
    assert!(html.contains("Nächster Börsentag · Fr 06.11.2026"));
    d.sperre().zeitplan.kurse = (7, 30);
    let einrichtung = get(&d, "/einrichtung", t);
    assert!(einrichtung.contains("Im Morgenlauf gilt der letzte abgeschlossene Handelstag"));
    assert!(!einrichtung.contains("Beim Abendplan wird jede andere Börse"));

    Ok(())
}

#[test]
fn live_zeigt_wiederanlauf_aus_journal_und_papierkonto() -> Result<(), String> {
    use depot_journal::Art::{Abrechnungstag, Abschluss, Auftrag, Topf, Zustand};
    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    let t = donnerstag(18, 0)?;
    let abschluss = format!("2026-11-04;1000000;{}", "a".repeat(64));
    let journal = depot_journal::inhalt_fuer(&[
        (Topf, "A;500000;"),
        (Topf, "B;500000;"),
        (Abrechnungstag, "2026-11-04"),
        (Abschluss, &abschluss),
        (Abrechnungstag, "2026-11-05"),
        (Auftrag, "1;A;DE0007164600;Kauf;1;10000;Umschichtung"),
        (Zustand, "1;gesendet"),
        (Zustand, "1;angenommen"),
    ]);
    let vermerk = some(depot_journal::vermerk_fuer(&journal, 0), "Vermerk")?;
    fs::write(daten.join("journal.txt"), &journal).map_err(|e| e.to_string())?;
    fs::write(daten.join("journal.vermerk"), &vermerk).map_err(|e| e.to_string())?;

    let live = || {
        let d = Dienst::neu(
            wurzel.clone(),
            TOKEN.into(),
            echo_starter(true),
            depot_dienst::lagerechner(Arc::new(Attrappe::default())),
        );
        assert!(d.schritt(t).is_none());
        get(&d, "/teil/live", t)
    };
    assert!(live().contains("Papierkonto nicht prüfbar"));

    let konto = daten.join("papierkonto.txt");
    fs::write(
        &konto,
        concat!(
            "papierkonto-v1\nbargeld;1000000\nsendungen;1\n",
            "order;1;DE0007164600;kauf;1;10000;offen\n"
        ),
    )
    .map_err(|e| e.to_string())?;
    assert!(live().contains("Journal und Papierkonto stimmen in den geprüften Daten überein."));

    let ausgefuehrt = concat!(
        "papierkonto-v1\nbargeld;989850\nsendungen;1\n",
        "bestand;DE0007164600;1\norder;1;DE0007164600;kauf;1;10000;gefuellt\n",
        "ausfuehrung;1;1;10000;150\n"
    );
    fs::write(&konto, ausgefuehrt).map_err(|e| e.to_string())?;
    let html = live();
    assert!(html.contains("Papiertag ohne Abschluss"));
    assert!(html.contains("Letzter bestätigter Abschluss: 04.11.2026"));
    assert!(html.contains("liegt beim Papiermakler · 0 Stück ausgeführt"));
    assert!(html.contains("ausgeführt · 1 Stück ausgeführt"));
    assert!(html.contains("Journal und Papierkonto müssen abgeglichen werden."));
    assert!(html.contains("Bestand DE0007164600: Journal 0 Stück, Papierkonto 1 Stück."));
    assert_eq!(
        fs::read(daten.join("journal.txt")).map_err(|e| e.to_string())?,
        journal
    );
    assert_eq!(
        fs::read_to_string(daten.join("journal.vermerk")).map_err(|e| e.to_string())?,
        vermerk
    );
    assert_eq!(
        fs::read_to_string(&konto).map_err(|e| e.to_string())?,
        ausgefuehrt
    );
    Ok(())
}

#[test]
fn angehaltene_automatik_zeigt_wartenden_handauftrag_und_rot_im_zeitstrahl() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(12, 0)?;
    {
        let mut z = d.sperre();
        z.zeitplan.an = true;
        z.erledigt_fehler = Some("Laufstand unlesbar".into());
    }
    d.einreihen(Art::Termine, Anlass::Hand)?;
    let html = get(&d, "/teil/live", t);
    assert!(html.contains("Als Nächstes: Termine abrufen (von Hand)"));
    assert!(html.contains(r#"<h2>Zeitstrahl</h2><p class="halt">Zeitplan angehalten"#));
    Ok(())
}

#[test]
fn konsole_stil_hell_dunkel() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let html = get(&d, "/", donnerstag(12, 0)?);
    assert!(html.contains(":root{--konsole:#f0f0ec}"));
    assert!(html.contains("@media (prefers-color-scheme:dark){:root{--konsole:#10100f}}"));
    assert!(html.contains(".konsole{border:1px solid var(--linie)"));
    assert!(html.contains("max-height:24em;overflow:auto"));
    Ok(())
}

#[test]
fn app_js_liefert_die_gepruefte_fassung() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let antwort = get(&d, "/app.js", donnerstag(12, 0)?);
    let (_, js) = some(antwort.split_once("\r\n\r\n"), "HTTP-Antwort")?;
    // SHA-256 der geprüften Fassung mit unabhängigen Aktivitätssignalen im Kopf.
    assert_eq!(
        depot_kern::abdruck::sha256_hex(js.as_bytes()),
        "04e7d109003bea477213e3a09356d215d4c750344ea5e8640b966326ac27f910"
    );
    Ok(())
}

#[test]
fn app_js_rettet_scrollstand() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let js = get(&d, "/app.js", donnerstag(12, 0)?);
    let (vorher, nachher) = some(js.split_once("ziel.innerHTML = t"), "Auffrischung")?;
    assert!(vorher.contains("pre.konsole"));
    assert!(vorher.contains("pre.scrollHeight - pre.scrollTop - pre.clientHeight < 20"));
    assert!(nachher.contains("konsolePosition(d, vorher && vorher.position)"));
    assert!(js.contains("navigator.clipboard.writeText(log.textContent)"));
    Ok(())
}

#[test]
fn andere_pre_unveraendert() -> Result<(), String> {
    let wurzel = ordner()?;
    fs::write(kostenschutz::pfad(&wurzel), "Kosten unbekannt").map_err(|e| e.to_string())?;
    fs::write(kostenschutz::kurse_pfad(&wurzel), "Kursabruf angehalten")
        .map_err(|e| e.to_string())?;
    let d = dienst(wurzel, true);
    let html = get(&d, "/einrichtung", donnerstag(12, 0)?);
    assert!(html.contains("<pre>Kosten unbekannt</pre>"));
    assert!(html.contains("<pre>Kursabruf angehalten</pre>"));
    assert!(!html.contains("<pre class=\"konsole\""));
    Ok(())
}

#[test]
fn gespeichertes_protokoll_entfernt_nur_die_trefferzeile() -> Result<(), String> {
    let wurzel = ordner()?;
    fs::create_dir(wurzel.join("daten/protokolle")).map_err(|e| e.to_string())?;
    // Ein älteres Log, das noch nicht mit der heutigen Musterschicht geschrieben wurde.
    let text = "Anfang bleibt\r\nAuthorization: Basic beispiel\r\nEnde bleibt ohne Zeilenende";
    fs::write(wurzel.join("daten/protokolle/123-1.log"), text).map_err(|e| e.to_string())?;
    let d = dienst(wurzel.clone(), true);
    let t = donnerstag(12, 0)?;
    for adresse in ["/protokoll?lauf=123-1", "/protokoll.txt?lauf=123-1"] {
        let antwort = get(&d, adresse, t);
        assert!(antwort.starts_with("HTTP/1.1 200"));
        assert!(antwort.contains(
            "Anfang bleibt\r\n[Ausgabe mit Zugangsdaten entfernt.]\r\nEnde bleibt ohne Zeilenende"
        ), "{adresse}: {antwort:?}");
        assert!(!antwort.contains("beispiel"));
    }
    assert_eq!(
        fs::read_to_string(wurzel.join("daten/protokolle/123-1.log")).map_err(|e| e.to_string())?,
        text
    );
    fs::remove_dir_all(wurzel).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn live_zeigt_den_tatsaechlichen_stand_der_vorbereitungsautomatik() -> Result<(), String> {
    use depot_dienst::zeitplan::VorbereitungsPlan;

    let d = dienst(ordner()?, true);
    let t = donnerstag(12, 0)?;
    assert!(d.schritt(t).is_none());
    for (an, vorbereitung, fehler, erwartet) in [
        (
            false,
            VorbereitungsPlan::An,
            None,
            "Automatische Vorbereitung pausiert.",
        ),
        (true, VorbereitungsPlan::Aus, None, "ist ausgeschaltet."),
        (true, VorbereitungsPlan::An, None, "ist eingeschaltet."),
        (
            true,
            VorbereitungsPlan::An,
            Some("Zeitplan nicht lesbar"),
            "Zeitplan angehalten.",
        ),
    ] {
        {
            let mut z = d.sperre();
            z.zeitplan.an = an;
            z.zeitplan.nachrichten_an = true;
            z.zeitplan.vorbereitung = vorbereitung;
            z.zeitplan_fehler = fehler.map(str::to_owned);
        }
        let live = get(&d, "/teil/live", t);
        let abschnitt = live
            .split_once("<section id=\"vorbereitung\">")
            .and_then(|(_, rest)| rest.split_once("</section>"))
            .map(|(abschnitt, _)| abschnitt)
            .ok_or("Vorbereitung fehlt")?;
        assert!(abschnitt.contains("Noch keine separate Vorbereitung belegt."));
        assert!(abschnitt.contains(erwartet), "{abschnitt}");
        assert!(abschnitt.contains("Heute ist Handelstag."));
        assert!(!abschnitt.contains("Automatik einrichten"));
    }
    Ok(())
}

#[test]
fn live_zustand_in_worten() -> Result<(), String> {
    let d = depot_mit_order()?;
    let zeit = donnerstag(18, 0)?;
    let bild = depot_lage::mit_journal(&d.wurzel.join("daten"), |journal| {
        lagebild(&Umgebung {
            jetzt: zeit,
            journal,
            kursquelle: Hinterlegt::Nein,
            kursquelle_us: Hinterlegt::Nein,
            berater: [
                (BeraterId::Dachs, Hinterlegt::Nein),
                (BeraterId::Maus, Hinterlegt::Nein),
            ],
            sperren: Vec::new(),
            anbieter: Vec::new(),
            anbieter_unlesbar: 0,
            ki_halt: None,
        })
    });
    d.sperre().lage = Some(depot_dienst::werkbank::Lage {
        stand: zeit,
        depot_version: 1,
        bild,
    });
    let html = get(&d, "/teil/live", zeit);
    assert!(
        html.contains("liegt beim Papiermakler") && html.contains("Mehr zum Depot"),
        "{html}"
    );
    assert!(!html.contains("Angenommen"));
    Ok(())
}

#[test]
fn neuro_lernlauf_zeigt_den_referenzstand_unabhaengig_vom_belegvergleich() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(12, 0)?;
    let link = "href=\"/neuroschlopf/lernlauf\"";
    // Klickweg: Live → Vergleich ansehen → Referenzlauf.
    assert!(get(&d, "/teil/live", t).contains("href=\"/neuroschlopf\">Vergleich ansehen"));
    assert!(get(&d, "/neuroschlopf", t).contains(link));
    assert!(get(&d, "/einrichtung", t).contains(link));
    let seite = get(&d, "/neuroschlopf/lernlauf", t);
    assert!(seite.starts_with("HTTP/1.1 200 OK"));
    assert!(seite.contains("Der lokale Belegvergleich ist ausgeschaltet."));
    assert!(seite.contains("Referenzlauf Multiplikation"));
    assert!(seite.contains("data-messung=\"/neuroschlopf/lernlauf.json\""));
    let antwort = get(&d, "/neuroschlopf/lernlauf.json", t);
    assert!(antwort.contains("Content-Type: application/json"));
    let (_, messdaten) = antwort.split_once("\r\n\r\n").ok_or("HTTP-Rumpf fehlt")?;
    assert_eq!(
        depot_kern::abdruck::sha256_hex(messdaten.as_bytes()),
        "025649b4417e88eb92f6484914ff60280a510a30b797045e8bb92dbe57a2ddb4"
    );
    let post = formular_post("/neuroschlopf", &format!("t={TOKEN}&an=ja"));
    assert!(text(&behandle(&d, &Attrappe::default(), &post, t)).contains("303 See Other"));
    assert!(
        get(&d, "/neuroschlopf/lernlauf", t)
            .contains("Der lokale Belegvergleich ist eingeschaltet.")
    );
    assert_eq!(get(&d, "/neuroschlopf/lernlauf.json", t), antwort);
    Ok(())
}

fn dateien(wurzel: &std::path::Path) -> Result<Vec<(std::path::PathBuf, Vec<u8>)>, String> {
    let mut alle = Vec::new();
    let mut offen = vec![wurzel.to_path_buf()];
    while let Some(ordner) = offen.pop() {
        for e in std::fs::read_dir(&ordner).map_err(|e| e.to_string())? {
            let p = e.map_err(|e| e.to_string())?.path();
            if p.is_dir() {
                offen.push(p);
            } else {
                let inhalt = std::fs::read(&p).map_err(|e| e.to_string())?;
                alle.push((p, inhalt));
            }
        }
    }
    alle.sort();
    Ok(alle)
}

#[test]
fn neuroschlopf_seite_ist_ueber_menue_und_live_erreichbar_und_nur_lesend() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let t = donnerstag(12, 0)?;
    for pfad in ["/", "/teil/live"] {
        let html = get(&d, pfad, t);
        assert!(html.contains("<section id=\"schlopfmodell\">"), "{pfad}");
        assert!(
            html.contains("<a href=\"/neuroschlopf\">Vergleich ansehen</a>"),
            "{pfad}"
        );
    }
    assert!(get(&d, "/", t).contains("<a href=\"/neuroschlopf\">NeuroSchlopf</a>"));
    let vorher = dateien(&wurzel)?;
    for _ in 0..3 {
        let seite = get(&d, "/neuroschlopf", t);
        assert!(seite.starts_with("HTTP/1.1 200 OK"));
        assert_eq!(seite.matches("aria-current=\"page\"").count(), 1);
        assert!(seite.contains("<a href=\"/neuroschlopf\" aria-current=\"page\">NeuroSchlopf</a>"));
        for teil in [
            "Hintergrundvergleich · Ausgeschaltet",
            "Noch kein abgeschlossener Vergleich. Es fehlt ein bestätigter Stand.",
            "Neuronales Modell noch nicht eingebunden",
            "href=\"/neuroschlopf/lernlauf\">Neuro2026-Referenzlauf Multiplikation</a>",
            "href=\"/neuroschlopf/belegpruefung\">Blindbewertung</a>",
            "Übermittlung an Dachs und Maus",
        ] {
            assert!(seite.contains(teil), "{teil}");
        }
    }
    assert_eq!(
        dateien(&wurzel)?,
        vorher,
        "Öffnen der Seite ändert keine Datei"
    );
    let post = formular_post("/neuroschlopf", &format!("t={TOKEN}&an=ja"));
    assert!(
        text(&behandle(&d, &Attrappe::default(), &post, t)).contains("/einrichtung#neuroschlopf")
    );
    let vorher = dateien(&wurzel)?;
    let seite = get(&d, "/neuroschlopf", t);
    assert!(seite.contains("Wartet auf einen bestätigten Journalstand."));
    assert_eq!(dateien(&wurzel)?, vorher);
    for unterseite in ["/neuroschlopf/lernlauf", "/neuroschlopf/belegpruefung"] {
        assert!(
            get(&d, unterseite, t).contains("<a href=\"/neuroschlopf\">Zurück zu NeuroSchlopf</a>"),
            "{unterseite}"
        );
    }
    Ok(())
}
