use super::*;

#[test]
fn kein_papiertag_nach_gescheitertem_kursabruf() -> Result<(), String> {
    let d = dienst(eingeschaltet(ordner()?)?, false);
    d.sperre().boersen = vec![Boerse::Xetra];
    let t = donnerstag(7, 31)?;
    let (nr, kind) = some(d.schritt(t), "Termine")?;
    d.verfolge(nr, kind, || donnerstag(7, 31).ok());
    let (nr, kind) = some(d.schritt(t), "Kurse")?;
    d.verfolge(nr, kind, || donnerstag(7, 32).ok());
    assert!(wartend(&d).is_empty());
    let z = d.sperre();
    assert_eq!(
        z.verlauf.iter().map(|a| a.erfolg).collect::<Vec<_>>(),
        vec![Some(false), Some(false)]
    );
    Ok(())
}

#[test]
fn papierfreigabe_verlangt_bestaetigung_und_journalabdruck() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let umw = Attrappe::default();
    let t = donnerstag(12, 0)?;
    let post = |f: &str| {
        let a = formular_post("/papierfreigabe", &format!("t={TOKEN}&{f}"));
        text(&behandle(&d, &umw, &a, t))
    };
    assert!(post("").contains("papierfreigabe_unbestaetigt"));
    assert!(post("geprueft=ja&abdruck=ungültig").starts_with("HTTP/1.1 400"));
    assert!(!get(&d, "/einrichtung", t).contains("Papierhandel nach Verlustsperre freigeben"));
    Ok(())
}

/// Mit zwei Boersen: Der Papiertag folgt erst dem letzten Abruf des Tages (NYSE), und nur, wenn
/// ein Abruf dieses Tages erfolgreich war. Ein gescheiterter Xetra-Abruf haelt ihn nicht auf,
/// wenn die NYSE gelingt, und umgekehrt.
#[test]
fn papiertag_nach_dem_letzten_abruf_des_tages() -> Result<(), String> {
    for (xetra_ok, nyse_ok, zeitplan_an, papier) in [
        (true, true, true, true),
        (false, true, true, false),
        (true, false, true, false),
        (false, false, true, false),
        (true, true, false, false),
    ] {
        let wurzel = eingeschaltet(ordner()?)?;
        // Ein Starter, der je Auftrag so endet, wie diese Runde es will.
        let d = Arc::new(Dienst::neu(
            wurzel,
            TOKEN.into(),
            echo_starter_je_art(move |a| match a {
                Art::KurseVon(Boerse::Xetra) => xetra_ok,
                Art::KurseVon(Boerse::Nyse) => nyse_ok,
                _ => true,
            }),
            leeres_lagebild(),
        ));
        d.sperre().boersen = vec![Boerse::Xetra, Boerse::Nyse];
        let (nr, kind) = some(d.schritt(donnerstag(7, 31)?), "Termine")?;
        d.verfolge(nr, kind, || donnerstag(7, 31).ok());
        let (nr, kind) = some(d.schritt(donnerstag(7, 31)?), "Xetra")?;
        assert_eq!(
            d.sperre().laufend.as_ref().map(|a| a.art),
            Some(Art::KurseVon(Boerse::Xetra))
        );
        d.verfolge(nr, kind, || donnerstag(7, 32).ok());
        assert!(!wartend(&d).iter().any(|(art, _)| *art == Art::Papier));
        let (nr, kind) = some(d.schritt(donnerstag(7, 32)?), "NYSE")?;
        assert_eq!(
            d.sperre().laufend.as_ref().map(|a| a.art),
            Some(Art::KurseVon(Boerse::Nyse))
        );
        if !zeitplan_an {
            d.setze_zeitplan(Zeitplan {
                an: false,
                ..Zeitplan::default()
            })
            .map_err(|e| format!("Zeitplan ausschalten: {e:?}"))?;
        }
        d.verfolge(nr, kind, || donnerstag(7, 33).ok());
        let erwartet = if papier {
            vec![(Art::Papier, Anlass::Zeitplan)]
        } else {
            Vec::new()
        };
        assert_eq!(wartend(&d), erwartet, "Xetra {xetra_ok}, NYSE {nyse_ok}");
    }
    Ok(())
}

#[test]
fn ausgelassener_papiertag_meldet_sich_genau_einmal() -> Result<(), String> {
    let wurzel = ordner()?;
    let tag = some(donnerstag(17, 50)?.datum(), "Tag")?;
    let daten = wurzel.join("daten");
    fs::write(
        daten.join("zeitplan.txt"),
        Zeitplan {
            an: true,
            ..Zeitplan::default()
        }
        .als_text(),
    )
    .map_err(|e| e.to_string())?;
    let mut erledigt = Erledigt::default();
    erledigt.setze(Art::Termine, tag);
    fs::write(daten.join("dienst_erledigt.txt"), erledigt.als_text()).map_err(|e| e.to_string())?;
    let gruende = Gruende::default();
    let (d, meldungen) = hinweis_dienst(wurzel.clone(), &gruende, false);
    assert!(d.schritt(donnerstag(17, 50)?).is_none());
    assert!(d.schritt(donnerstag(17, 51)?).is_none());
    let erste = gezeigt(&meldungen);
    assert_eq!(erste.len(), 1);
    assert!(erste.iter().any(|text| text.contains("ausgelassen")));
    drop(d);
    let (d, meldungen) = hinweis_dienst(wurzel, &gruende, false);
    assert!(d.schritt(donnerstag(18, 30)?).is_none());
    assert!(gezeigt(&meldungen).is_empty());
    Ok(())
}

#[test]
fn sicherung_formulare_speichern_und_starten() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let zeit = donnerstag(12, 0)?;
    let html = get(&d, "/einrichtung", zeit);
    let (_, formular) = some(html.split_once("id=\"sicherung\""), "Sicherungsabschnitt")?;
    assert!(formular.contains("action=\"/sicherung-ziel\""));
    assert!(formular.contains("value=\"starten\">Jetzt sichern"));
    assert!(formular.contains("data-teil=\"/teil/sicherung\""));
    assert_eq!(formular.contains("value=\"waehlen\""), cfg!(windows));
    let ziel = if cfg!(windows) {
        "E%3A%5C"
    } else {
        "%2FDepotSicherung"
    };
    let erwartet = if cfg!(windows) {
        "E:\\"
    } else {
        "/DepotSicherung"
    };
    let starten = formular_post(
        "/sicherung-ziel",
        &format!("t={TOKEN}&aktion=starten&ziel={ziel}"),
    );
    let antwort = text(&behandle(&d, &Attrappe::default(), &starten, zeit));
    assert!(
        antwort.contains("Location: /einrichtung#sicherung"),
        "{antwort}"
    );
    let einrichtung = get(&d, "/einrichtung?m=sicherung_start", zeit);
    assert!(!einrichtung.contains("Sicherung eingereiht."));
    assert!(einrichtung.contains("Sicherung wartet auf den Start."));
    assert_eq!(
        fs::read_to_string(wurzel.join("daten/sicherung_ziel.txt"))
            .map_err(|e| e.to_string())?
            .trim(),
        erwartet
    );
    assert_eq!(
        d.sperre().warteschlange.front().map(|a| a.art),
        Some(Art::Sicherung)
    );
    assert!(get(&d, "/teil/sicherung", zeit).contains("Sicherung wartet auf den Start"));
    let wechseln = formular_post(
        "/sicherung-ziel",
        &format!("t={TOKEN}&aktion=speichern&ziel="),
    );
    assert_eq!(
        status(&behandle(&d, &Attrappe::default(), &wechseln, zeit))?,
        409
    );
    assert_eq!(
        fs::read_to_string(wurzel.join("daten/sicherung_ziel.txt"))
            .map_err(|e| e.to_string())?
            .trim(),
        erwartet
    );
    Ok(())
}

#[test]
fn sicherungsdialog_und_anzahl_am_ziel() -> Result<(), String> {
    let wurzel = ordner()?;
    let ziel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let zeit = donnerstag(12, 0)?;
    let auswahl = formular_post("/sicherung-ziel", &format!("t={TOKEN}&aktion=waehlen"));
    let antwort = text(&behandle(&d, &Attrappe::default(), &auswahl, zeit));
    assert!(antwort.contains("Location: /einrichtung#sicherung"));
    assert!(!wurzel.join("daten/sicherung_ziel.txt").exists());

    let umwelt = Attrappe {
        sicherungsziel: Some(ziel.display().to_string()),
        ..Attrappe::default()
    };
    let antwort = text(&behandle(&d, &umwelt, &auswahl, zeit));
    assert!(antwort.contains("Location: /einrichtung?m=sicherung_ziel#sicherung"));
    assert!(d.sperre().warteschlange.is_empty());
    assert!(get(&d, "/teil/sicherung", zeit).contains("Sicherungen am Ziel: 0"));

    let sicherungen = ziel.join("DepotSchlopf");
    let name = format!("utc-00000000000000000001-000000001_{}", "a".repeat(40));
    fs::create_dir_all(sicherungen.join(&name)).map_err(|e| e.to_string())?;
    fs::create_dir(sicherungen.join(format!("{name}.tmp"))).map_err(|e| e.to_string())?;
    fs::create_dir(sicherungen.join("fremd")).map_err(|e| e.to_string())?;
    assert!(get(&d, "/teil/sicherung", zeit).contains("Sicherungen am Ziel: 1"));
    fs::remove_dir_all(ziel).map_err(|e| e.to_string())?;
    assert!(get(&d, "/teil/sicherung", zeit).contains("Anzahl nicht verfügbar"));
    let ungueltig = formular_post(
        "/sicherung-ziel",
        &format!("t={TOKEN}&aktion=starten&ziel=relativ"),
    );
    assert_eq!(status(&behandle(&d, &umwelt, &ungueltig, zeit))?, 400);
    assert!(d.sperre().warteschlange.is_empty());
    Ok(())
}

#[test]
fn depot_seite_da() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let html = get(&d, "/depot", donnerstag(12, 0)?);
    assert!(html.contains("200 OK"), "{html}");
    assert!(
        html.contains("aria-current=\"page\" href=\"/depot\"")
            || html.contains("href=\"/depot\" aria-current=\"page\"")
    );
    Ok(())
}

#[test]
fn depot_ohne_journal() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let html = get(&d, "/depot", donnerstag(12, 0)?);
    assert!(html.contains("Noch kein Depot: Es entsteht mit dem ersten Papiertag."));
    Ok(())
}

#[test]
fn depot_aktualisiert_nur_einen_geaenderten_stand() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let zeit = donnerstag(12, 0)?;
    let html = get(&d, "/depot?zeitraum=1m", zeit);
    assert!(html.contains("data-teil=\"/teil/depot?zeitraum=1m\""));
    assert!(!html.contains("data-depot-stand"));

    let bild = leeres_lagebild()(&d.wurzel, zeit);
    {
        let mut z = d.sperre();
        z.lage = Some(depot_dienst::werkbank::Lage {
            stand: zeit,
            depot_version: 2,
            bild,
        });
        z.lage_veraltet = false;
    }
    let gleich = get(&d, "/teil/depot?zeitraum=1m&stand=2", zeit);
    assert!(gleich.ends_with("\r\n\r\n"), "{gleich}");
    let neuer_stand = get(&d, "/teil/depot?zeitraum=1m&stand=1", zeit);
    assert!(neuer_stand.contains("data-depot-stand=\"2\""));
    assert!(neuer_stand.contains("Noch kein Depot"));
    assert!(!neuer_stand.contains("<nav"));
    Ok(())
}

#[test]
fn depot_journal_kaputt() -> Result<(), String> {
    let wurzel = ordner()?;
    fs::write(wurzel.join("daten/journal.txt"), "kaputt\n").map_err(|e| e.to_string())?;
    let d = dienst(wurzel, true);
    let html = get(&d, "/depot", donnerstag(12, 0)?);
    assert!(html.contains("class=\"halt\">Halt:"), "{html}");
    assert!(!html.contains("Gesamtwert"));
    Ok(())
}

#[test]
fn depot_sortierung() -> Result<(), String> {
    let d = depot_mit_positionen()?;
    let html = get(&d, "/depot", donnerstag(18, 0)?);
    let tabelle = some(
        html.split_once("Wo bin ich investiert?"),
        "Positionstabelle",
    )?
    .1;
    let mut zuletzt = 0;
    for wort in ["SAP", "Allianz", "BASF", "Bargeld"] {
        let stelle = some(tabelle.find(wort), wort)?;
        assert!(stelle > zuletzt);
        zuletzt = stelle;
    }
    Ok(())
}

#[test]
fn depot_anteil() -> Result<(), String> {
    let d = depot_mit_positionen()?;
    let html = get(&d, "/depot", donnerstag(18, 0)?);
    assert!(html.contains("33,33 %"), "{html}");
    Ok(())
}

#[test]
fn depot_einstand_nicht_belegt() -> Result<(), String> {
    let d = depot_mit_positionen()?;
    let html = get(&d, "/depot", donnerstag(18, 0)?);
    assert!(html.contains("<td>nicht belegt</td><td>–</td>"));
    Ok(())
}

#[test]
fn depot_tagesergebnis_ohne_vortag() -> Result<(), String> {
    let d = depot_mit_positionen()?;
    assert!(get(&d, "/depot", donnerstag(18, 0)?).contains("noch kein Vortag"));
    Ok(())
}

#[test]
fn depot_kurse_veraltet() -> Result<(), String> {
    let d = depot_mit_positionen()?;
    let vor_schluss = some(Zeitpunkt::lies("2026-11-06T15:00:00Z"), "Vor Börsenschluss")?;
    let nach_schluss = some(
        Zeitpunkt::lies("2026-11-06T18:00:00Z"),
        "Nach Börsenschluss",
    )?;
    let kursdatum = "<p class=\"leise\">Bewertet mit Schlusskursen vom 05.11.2026";
    let frisch = get(&d, "/depot", vor_schluss);
    assert!(frisch.contains(&format!("{kursdatum}</p>")));
    let veraltet = get(&d, "/depot", nach_schluss);
    assert!(veraltet.contains(&format!(
        "{kursdatum} <strong class=\"offen\">(veraltet)</strong></p>"
    )));
    Ok(())
}

#[test]
fn handelsbilanz_zeigt_offenen_verlust_ohne_verkauf() -> Result<(), String> {
    use depot_journal::Art::{Abrechnungstag, Abschluss, Auftrag, Kurs, Topf, Zustand};
    let wurzel = ordner()?;
    let abdruck = "a".repeat(64);
    let kurs = format!(
        "kursprobe;quelle;2026-11-06T17:00:00Z;{abdruck};DE0007164600;8000;2026-11-06T16:30:00Z"
    );
    let abschluss = format!("2026-11-06;500000;{abdruck}");
    let journal = depot_journal::inhalt_fuer(&[
        (Topf, "A;500000;"),
        (Auftrag, "1;A;DE0007164600;Kauf;1;10000;Umschichtung"),
        (Zustand, "1;gesendet"),
        (Zustand, "1;angenommen"),
        (Zustand, "1;ausfuehrung;1;10000;150"),
        (Kurs, &kurs),
        (Abrechnungstag, "2026-11-06"),
        (Abschluss, &abschluss),
    ]);
    let vermerk = some(depot_journal::vermerk_fuer(&journal, 0), "Journalvermerk")?;
    fs::write(wurzel.join("daten/journal.txt"), journal).map_err(|e| e.to_string())?;
    fs::write(wurzel.join("daten/journal.vermerk"), vermerk).map_err(|e| e.to_string())?;
    let d = dienst(wurzel, true);
    let zeit = some(Zeitpunkt::lies("2026-11-06T18:00:00Z"), "Zeit")?;
    let html = get(&d, "/depot?zeitraum=alles", zeit);
    let (_, bilanz) = html
        .split_once("id=\"handelsbilanz\"")
        .ok_or("Handelsbilanz")?;
    assert!(bilanz.contains("Keine abgeschlossenen Verkäufe"));
    assert!(bilanz.contains("offen, bewertet zum letzten Schluss"));
    assert!(bilanz.contains("Realisiert + offen: -23,00 €"), "{bilanz}");
    assert!(bilanz.contains("Käufe nicht zugeordnet: 1"), "{bilanz}");
    Ok(())
}

#[test]
fn depot_offene_orders_haben_tabellenkopf() -> Result<(), String> {
    let d = depot_mit_order()?;
    let html = get(&d, "/depot", donnerstag(18, 0)?);
    let (_, orders) = some(html.split_once("<h2>Offene Orders</h2>"), "Offene Orders")?;
    let (orders, _) = some(orders.split_once("</section>"), "Ende der offenen Orders")?;
    let (_, tabelle) = some(orders.split_once("<table><thead><tr>"), "Tabellenkopf")?;
    let (kopf, inhalt) = some(tabelle.split_once("</tr></thead><tbody>"), "Tabellenkörper")?;
    for spalte in [
        "Wertpapier",
        "Richtung",
        "Stück offen",
        "Limit",
        "Topf",
        "Zustand",
    ] {
        assert!(kopf.contains(&format!("<th scope=\"col\">{spalte}</th>")));
    }
    assert!(inhalt.contains("<tr><td>SAP</td>"));
    assert!(inhalt.contains("liegt beim Papiermakler"), "{inhalt}");
    assert!(inhalt.ends_with("</tbody></table></div>"));
    Ok(())
}

#[test]
fn depot_ohne_technik() -> Result<(), String> {
    let d = depot_mit_order()?;
    let html = get(&d, "/depot", donnerstag(18, 0)?);
    assert!(html.contains("liegt beim Papiermakler"), "{html}");
    let (_, handelsbilanz) = html
        .split_once("id=\"handelsbilanz\"")
        .ok_or("Handelsbilanz")?;
    assert!(handelsbilanz.contains("<h4 id=\"handelskarte-dachs\">Dachs</h4>"));
    assert!(handelsbilanz.contains("<h4 id=\"handelskarte-maus\">Maus</h4>"));
    assert!(!handelsbilanz.contains("class=\"handelskarte-skala\""));
    assert!(handelsbilanz.contains("Keine abgeschlossenen Verkäufe im Zeitraum."));

    assert!(handelsbilanz.contains("Papierhandels-Reife"));
    assert!(handelsbilanz.contains("0 von 40"));
    assert!(handelsbilanz.contains("0 von 30"));
    assert!(handelsbilanz.contains("Nicht belegt"));

    for roh in [
        "Some(",
        "None",
        "Teilgefuellt",
        "Reserviert",
        "DE0007164600",
    ] {
        assert!(!html.contains(roh), "{roh}");
    }
    Ok(())
}
