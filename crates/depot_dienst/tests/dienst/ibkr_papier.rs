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
                "echo [Hinweis] Gateway neu starten.& exit /b 1",
            );
            #[cfg(not(windows))]
            let (programm, option, skript) =
                ("sh", "-c", "echo '[Hinweis] Gateway neu starten.'; exit 1");
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
    assert!(seite.contains("IBKR-Papierkonto einrichten und testen"));
    assert!(seite.contains("Schreibgeschützte API") && seite.contains("4002"));
    assert!(post("/ausfuehren", "art=ibkr_papier").contains("ibkr=format"));
    assert!(post("/ibkr-papier", "konto=U1234").contains("ibkr=format"));
    assert!(post("/ibkr-papier", "konto=DUR1234567").contains("ibkr=gespeichert"));
    let datei = wurzel.join("daten/ibkr-papierkonto.txt");
    assert_eq!(
        fs::read_to_string(&datei).map_err(|e| e.to_string())?,
        "DUR1234567\n"
    );
    let seite = get(&d, "/einrichtung", jetzt);
    assert!(seite.contains("Papierabruf testen"));
    assert!(!seite.contains("DUR1234567"));
    assert_eq!(Art::IbkrPapier.argumente(), ["ibkr", "papier", "pruefen"]);
    assert!(post("/ausfuehren", "art=ibkr_papier").contains("/einrichtung#ibkr-papier"));
    assert_eq!(wartend(&d), vec![(Art::IbkrPapier, Anlass::Hand)]);
    assert!(post("/ibkr-papier", "konto=DU7654321").contains("ibkr=belegt"));
    let (nr, kind) = some(d.schritt(jetzt), "Papierabruf gestartet")?;
    d.verfolge(nr, kind, || donnerstag(12, 0).ok());
    let status = get(&d, "/teil/ibkr-papier", jetzt);
    assert!(status.contains("Gateway neu starten."));
    assert!(status.contains("Papierabruf testen"));
    assert!(!status.contains("DUR1234567"));
    assert!(post("/ibkr-papier", "konto=DU7654321").contains("ibkr=gespeichert"));
    let status = get(&d, "/teil/ibkr-papier", jetzt);
    assert!(!status.contains("Gateway neu starten."));
    assert!(!status.contains("DU7654321"));
    Ok(())
}
