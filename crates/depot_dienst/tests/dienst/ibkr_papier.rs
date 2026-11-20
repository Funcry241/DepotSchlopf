use super::*;

#[test]
fn ibkr_papierkonto_in_der_oberflaeche_speichern_und_lesen() -> Result<(), String> {
    let wurzel = ordner()?;
    let jetzt = donnerstag(12, 0)?;
    let d = Arc::new(Dienst::neu(
        wurzel.clone(),
        TOKEN.into(),
        Box::new(|_, _| {
            #[cfg(windows)]
            let (programm, option, skript) = (
                "cmd",
                "/C",
                "echo [IBKR-Diagnose] Zeilen fuer SettledCash: 0& echo [Hinweis] Gateway neu starten.& exit /b 1",
            );
            #[cfg(not(windows))]
            let (programm, option, skript) = (
                "sh",
                "-c",
                "echo '[IBKR-Diagnose] Zeilen fuer SettledCash: 0'; echo '[Hinweis] Gateway neu starten.'; exit 1",
            );
            let mut c = Command::new(programm);
            c.args([option, skript]);
            c
        }),
        leeres_lagebild(),
    ));
    let post = |ziel: &str, felder: &str| {
        text(&behandle(
            &d,
            &Attrappe::default(),
            &formular_post(ziel, &format!("t={TOKEN}&{felder}")),
            jetzt,
        ))
    };
    let seite = get(&d, "/einrichtung", jetzt);
    assert!(seite.contains("href=\"#ibkr-papier\">IBKR-Papierkonto einrichten</a>"));
    assert!(!seite.contains("href=\"#ibkr-papier\">IBKR-Papierabruf testen</a>"));
    assert!(seite.contains("Schreibgeschützte API") && seite.contains("4002"));
    assert!(!seite.contains("<summary>Papierkontonummer ändern</summary>"));
    assert!(post("/ausfuehren", "art=ibkr_papier").contains("ibkr=format"));
    assert!(post("/ibkr-papier", "konto=U1234").contains("ibkr=format"));
    assert!(post("/ibkr-papier", "konto=DUR1234567").contains("ibkr=gespeichert"));
    let datei = wurzel.join("daten/ibkr-papierkonto.txt");
    assert_eq!(
        fs::read_to_string(&datei).map_err(|e| e.to_string())?,
        "DUR1234567\n"
    );
    let seite = get(&d, "/einrichtung", jetzt);
    assert!(seite.contains("href=\"#ibkr-papier\">IBKR-Papierabruf testen</a>"));
    assert!(!seite.contains("href=\"#ibkr-papier\">IBKR-Papierkonto einrichten"));
    let (_, ibkr) = some(seite.split_once("id=\"ibkr-papier\""), "IBKR-Abschnitt")?;
    let (status, details) = some(ibkr.split_once("<details>"), "eingeklappte Bearbeitung")?;
    assert!(status.contains("class=\"gut\">✓ Papierkontonummer lokal gespeichert"));
    assert!(status.contains("Papierabruf testen"));
    assert!(status.contains("03:00 Uhr Ortszeit"));
    assert!(status.contains("am Wochenende meldest du dich wieder von Hand an"));
    assert!(!status.contains("name=\"konto\""));
    assert!(details.starts_with("<summary>Papierkontonummer ändern</summary>"));
    assert!(details.contains("<details><summary>Hilfe zur Einrichtung</summary>"));
    assert!(details.contains(
        "Konfigurieren → Einstellungen → Sperren und schließen → Automatischer Neustart"
    ));
    assert!(details.contains("03:00 AM"));
    assert!(!seite.contains("DUR1234567"));
    let formatfehler = get(&d, "/einrichtung?ibkr=format", jetzt);
    assert!(formatfehler.contains("<details open><summary>Papierkontonummer ändern</summary>"));
    assert_eq!(Art::IbkrPapier.argumente(), ["ibkr", "papier", "pruefen"]);
    assert!(post("/ausfuehren", "art=ibkr_papier").contains("/einrichtung#ibkr-papier"));
    assert_eq!(wartend(&d), vec![(Art::IbkrPapier, Anlass::Hand)]);
    assert!(post("/ibkr-papier", "konto=DU7654321").contains("ibkr=belegt"));
    let (nr, kind) = some(d.schritt(jetzt), "Papierabruf gestartet")?;
    d.verfolge(nr, kind, || donnerstag(12, 0).ok());
    let status = get(&d, "/teil/ibkr-papier", jetzt);
    assert!(status.contains("Gateway neu starten."));
    assert!(status.contains("Gateway-Diagnose"));
    assert!(status.contains("Zeilen fuer SettledCash: 0"));
    assert!(status.contains("Papierabruf testen"));
    assert!(!status.contains("DUR1234567"));
    assert!(post("/ibkr-papier", "konto=DU7654321").contains("ibkr=gespeichert"));
    let status = get(&d, "/teil/ibkr-papier", jetzt);
    assert!(!status.contains("Gateway neu starten."));
    assert!(!status.contains("DU7654321"));
    Ok(())
}

#[test]
fn depot_zeigt_letzten_ibkr_abruf_getrennt_von_der_simulation() -> Result<(), String> {
    use depot_journal::{Journal, ibkr_abruf::vermerken};
    use depot_kern::{Cent, Topf, TopfId};

    let wurzel = ordner()?;
    let journal = wurzel.join("daten/journal.txt");
    Journal::einrichten(&journal).map_err(|e| format!("{e:?}"))?;
    let mut journal = Journal::oeffnen(&journal).map_err(|e| format!("{e:?}"))?;
    for id in [TopfId::A, TopfId::B] {
        let topf = Topf::neu(id, Cent(500_000)).map_err(|e| format!("{e:?}"))?;
        journal.setze_topf(&topf).map_err(|e| format!("{e:?}"))?;
    }
    drop(journal);
    let t = donnerstag(12, 0)?;
    let d = dienst(wurzel.clone(), true);
    let datei = wurzel.join("daten/ibkr-papierkonto.txt");
    let abschnitt = |d| get(d, "/teil/ibkr-depot", t);
    assert!(abschnitt(&d).contains("Noch kein Papierabruf"));
    assert!(abschnitt(&d).contains("/einrichtung#ibkr-papier"));

    let konto = "DUR1234567";
    fs::write(&datei, konto).map_err(|e| e.to_string())?;
    let vermerke = |abruf| vermerken(&wurzel, konto, abruf).map_err(|e| e.to_string());
    vermerke(None)?;
    assert!(abschnitt(&d).contains("Der jüngste Papierabruf ist fehlgeschlagen."));
    assert!(!abschnitt(&d).contains("<strong"));

    vermerke(Some((t, Cent(100_000))))?;
    let seite = get(&d, "/depot", t);
    let (ibkr, intern) = some(
        seite.split_once("<h2>Interne Simulation</h2>"),
        "interne Simulation",
    )?;
    assert!(ibkr.contains("data-teil=\"/teil/ibkr-depot\""));
    assert!(ibkr.contains("<h2>IBKR-Papierkonto</h2>"));
    assert!(ibkr.contains(&format!(
        "<strong class=\"gross\">{}</strong>",
        Cent(100_000)
    )));
    assert!(ibkr.contains(&depot_lage::zeit_text(t)));
    assert!(intern.contains("10.000,00"));
    assert!(!intern.contains(&Cent(100_000).to_string()));
    assert!(!seite.contains(konto));

    vermerke(None)?;
    let neu = dienst(wurzel.clone(), true);
    let html = abschnitt(&neu);
    assert!(html.contains("Der jüngste Papierabruf ist fehlgeschlagen."));
    assert!(html.contains(&Cent(100_000).to_string()));

    vermerke(Some((t, Cent(105_000))))?;
    assert!(abschnitt(&neu).contains(&Cent(105_000).to_string()));
    assert!(!abschnitt(&neu).contains("fehlgeschlagen"));
    fs::write(&datei, "DU7654321").map_err(|e| e.to_string())?;
    assert!(abschnitt(&neu).contains("Noch kein Papierabruf"));
    assert!(!abschnitt(&neu).contains(&Cent(105_000).to_string()));
    Ok(())
}

#[test]
fn ibkr_journalabgleich_laesst_sich_einrichten_und_je_handelstag_planen() -> Result<(), String> {
    use depot_journal::Journal;
    use depot_kern::ibkr_papier::{ABGLEICH_OK, KONTO_DATEI};
    use depot_kern::{Cent, Topf, TopfId};

    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let t = donnerstag(9, 0)?;
    let zeitpunkt = |tag, h, m| {
        some(
            Datum::neu(2026, 11, tag).and_then(|d| Zeitpunkt::frankfurt(d, h, m, 0)),
            "Zeitpunkt",
        )
    };
    let post = |ziel: &str, felder: &str| {
        let a = formular_post(ziel, &format!("t={TOKEN}&{felder}"));
        text(&behandle(&d, &Attrappe::default(), &a, t))
    };
    let konto = "DUR123456";
    fs::write(wurzel.join("daten").join(KONTO_DATEI), konto).map_err(|e| e.to_string())?;
    let schalter = "aktiv=ja&uhrzeit=09%3A00";
    assert!(post("/ibkr-abgleich", schalter).contains("ibkrabgleich=journal"));
    let teil = || get(&d, "/teil/ibkr-abgleich", t);
    assert!(teil().contains("IBKR-Papierjournal einrichten"));
    assert!(post("/ausfuehren", "art=ibkr_journal").contains("ibkrabgleich=eingereiht"));
    let (nr, kind) = some(d.schritt(t), "Manuelle Journaleinrichtung")?;
    d.verfolge(nr, kind, || donnerstag(9, 0).ok());

    let daten = wurzel.join("daten/ibkr-papier").join(konto);
    fs::create_dir_all(&daten).map_err(|e| e.to_string())?;
    let pfad = daten.join("journal.txt");
    Journal::einrichten(&pfad).map_err(|e| format!("{e:?}"))?;
    let mut j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
    let topf = Topf::neu(TopfId::A, Cent(100_000)).map_err(|e| format!("{e:?}"))?;
    j.setze_topf(&topf).map_err(|e| format!("{e:?}"))?;
    let vortag = some(zeitpunkt(4, 9, 0)?.frankfurt_tag(), "Erster Handelstag")?;
    for _ in 0..2 {
        j.beobachte(vortag, ABGLEICH_OK)
            .map_err(|e| format!("{e:?}"))?;
    }
    drop(j);
    assert!(teil().contains("1 von 5 Handelstagen belegt."));
    assert!(teil().contains("Jetzt abgleichen"));
    assert!(!teil().contains(konto));
    let vorher = get(&d, "/einrichtung", t);
    assert!(!vorher.contains("name=\"aktiv\" value=\"ja\" checked"));

    assert!(post("/ibkr-abgleich", "aktiv=ja&uhrzeit=25%3A00").contains("ibkrabgleich=uhrzeit"));
    assert!(post("/ibkr-abgleich", schalter).contains("ibkrabgleich=gespeichert"));
    assert!(d.schritt(t).is_none()); // Hauptzeitplan aus.
    let vorbereiten = |d: &Dienst| {
        let mut z = d.sperre();
        z.zeitplan.an = true;
        z.zeitplan.termine = (23, 0);
        z.zeitplan.kurse = (23, 1);
        z.zeitplan.papier_danach = false;
    };
    vorbereiten(&d);
    assert!(d.schritt(donnerstag(8, 59)?).is_none());
    let (nr, kind) = some(d.schritt(t), "Geplanter IBKR-Abgleich")?;
    assert!(
        d.sperre()
            .laufend
            .as_ref()
            .is_some_and(|a| a.art == Art::IbkrAbgleich && a.anlass == Anlass::Zeitplan)
    );
    d.verfolge(nr, kind, || donnerstag(9, 0).ok());
    assert!(d.schritt(donnerstag(9, 1)?).is_none());

    let neu = dienst(wurzel, true);
    vorbereiten(&neu);
    assert!(neu.schritt(donnerstag(9, 2)?).is_none());
    let freitag = zeitpunkt(6, 8, 30)?;
    let freitag_tag = some(freitag.frankfurt_tag(), "Manuell belegter Freitag")?;
    let mut j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
    j.beobachte(freitag_tag, ABGLEICH_OK)
        .map_err(|e| format!("{e:?}"))?;
    drop(j);
    assert!(neu.schritt(zeitpunkt(6, 9, 0)?).is_none());
    assert!(teil().contains("2 von 5 Handelstagen belegt."));
    assert!(neu.schritt(zeitpunkt(7, 9, 0)?).is_none());
    assert!(post("/ibkr-abgleich", "uhrzeit=09%3A00").contains("ibkrabgleich=gespeichert"));
    assert!(neu.schritt(zeitpunkt(9, 9, 0)?).is_none());
    Ok(())
}
