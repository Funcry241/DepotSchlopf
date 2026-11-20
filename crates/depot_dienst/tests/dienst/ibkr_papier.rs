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
