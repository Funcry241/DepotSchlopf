use super::*;
use depot_dienst::werkbank::Auftrag;

fn verlaufslauf(nr: u64, art: Art, zeit: Zeitpunkt, erfolg: bool) -> Auftrag {
    Auftrag {
        nr,
        kennung: format!("{}-{nr}", zeit.sekunden()),
        art,
        anlass: Anlass::Hand,
        start: zeit,
        ende: Some(zeit),
        erfolg: Some(erfolg),
        protokoll: if erfolg {
            Vec::new()
        } else {
            vec!["FEHLER: Beispiel".into()]
        },
        hinweis: None,
        frueher: Frueher::Nein,
        update_pause: None,
    }
}

#[test]
fn roter_fehlerhinweis_verschwindet_nur_nach_erfolg_derselben_art() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(12, 0)?;
    let fehler = verlaufslauf(1, Art::Termine, t, false);
    d.sperre().verlauf.push_front(fehler.clone());
    let neu = donnerstag(12, 1)?;
    d.sperre()
        .verlauf
        .push_front(verlaufslauf(2, Art::Nachrichten, neu, true));
    assert!(get(&d, "/", neu).contains("Letzter fehlgeschlagener Lauf"));
    d.sperre()
        .verlauf
        .push_front(verlaufslauf(3, Art::Termine, neu, true));
    let live = get(&d, "/", neu);
    assert!(!live.contains("Letzter fehlgeschlagener Lauf"));
    assert!(live.contains(&fehler.kennung) && live.contains("FEHLER: Beispiel"));
    Ok(())
}

#[test]
fn bestaetigtes_update_erledigt_aelteren_updatefehler() -> Result<(), String> {
    use depot_dienst::aktualisierung::{Ergebnis, Phase};
    let d = dienst(ordner()?, true);
    let alt = donnerstag(12, 0)?;
    let neu = donnerstag(12, 1)?;
    d.sperre()
        .verlauf
        .push_front(verlaufslauf(1, Art::Update, alt, false));
    assert!(get(&d, "/", neu).contains("Letzter fehlgeschlagener Lauf"));
    d.sperre().update_ergebnisse.push(Ergebnis {
        vorher: "a".repeat(40),
        neu: "b".repeat(40),
        lauf: "-".into(),
        phase: Phase::Bestaetigt,
        zeit: neu,
        grund: "Neuer Dienst geprüft.".into(),
    });
    assert!(!get(&d, "/", neu).contains("Letzter fehlgeschlagener Lauf"));
    let spaeter = donnerstag(12, 2)?;
    d.sperre()
        .verlauf
        .push_front(verlaufslauf(2, Art::Update, spaeter, false));
    assert!(get(&d, "/", spaeter).contains("Letzter fehlgeschlagener Lauf"));
    Ok(())
}

#[test]
fn neustart_erwartet_aktivierungsbeleg_nur_nach_einem_erfolgreichen_einspielen()
-> Result<(), String> {
    let t = donnerstag(12, 0)?;
    for erfolg in [false, true] {
        let wurzel = ordner()?;
        let lauf = verlaufslauf(1, Art::Update, t, erfolg);
        depot_dienst::verlauf::vermerke(&wurzel, &depot_dienst::verlauf::ende_zeile(&lauf))?;
        let neu = dienst(wurzel, true);
        let html = get(&neu, "/teil/update", t);
        assert_eq!(
            html.contains("keine bestätigte Aktivierung"),
            erfolg,
            "{html}"
        );
    }
    Ok(())
}

#[test]
fn verlauf_uebersteht_neustart_und_zeigt_unterbrochenes() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let t = donnerstag(12, 0)?;
    d.einreihen(Art::Termine, Anlass::Hand)
        .map_err(|e| format!("Vorbedingung: {e}"))?;
    let (nr, kind) = some(d.schritt(t), "Termine")?;
    d.verfolge(nr, kind, || donnerstag(12, 1).ok());
    // Der naechste Lauf wird gestartet, aber der Dienst endet, bevor er fertig ist.
    d.einreihen(Art::Kurse, Anlass::Hand)
        .map_err(|e| format!("Vorbedingung: {e}"))?;
    let (_, mut kind) = some(d.schritt(donnerstag(12, 2)?), "Kurse")?;
    let _ = kind.wait();
    drop(d);
    let neu = dienst(wurzel.clone(), true);
    let z = neu.sperre();
    let v: Vec<(Art, Frueher, Option<bool>)> = z
        .verlauf
        .iter()
        .map(|a| (a.art, a.frueher, a.erfolg))
        .collect();
    assert_eq!(
        v,
        vec![
            (Art::Kurse, Frueher::Unterbrochen, Some(false)),
            (Art::Termine, Frueher::Beendet, Some(true)),
        ]
    );
    let seite = seiten::live(TOKEN, "s", &z, t);
    drop(z);
    assert!(
        seite.contains("unterbrochen") && seite.contains("vor dem Neustart"),
        "{seite}"
    );
    assert!(
        seite.contains("Der Dienst endete während dieses Laufs"),
        "{seite}"
    );
    // Ein zweiter Neustart zeigt dasselbe (die Datei wird nur gekuerzt).
    drop(neu);
    let wieder = dienst(wurzel, true);
    assert_eq!(wieder.sperre().verlauf.len(), 2);
    Ok(())
}

#[test]
fn ausstiegsfrist_wird_aus_der_ausgefuehrten_order_nach_neustart_gelesen() -> Result<(), String> {
    use depot_journal::Art::{
        Abrechnungstag, Abschluss, Auftrag, Begruendung, Kurs, Topf, Zustand,
    };
    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    let abschluss = format!("2026-11-05;1000000;{}", "a".repeat(64));
    let kurs = format!(
        "kursprobe;quelle;2026-11-05T17:00:00Z;{};DE0007164600;10000;2026-11-05T16:30:00Z",
        "a".repeat(64)
    );
    let inhalt = depot_journal::inhalt_fuer(&[
        (Topf, "A;500000;"),
        (Topf, "B;500000;"),
        (Abrechnungstag, "2026-11-05"),
        (Kurs, &kurs),
        (Auftrag, "1;A;DE0007164600;Kauf;1;10000;Umschichtung"),
        (
            Begruendung,
            "2026-11-05;Kaufthese;1 [Zahlen] [Ausstieg 14500 1] Testthese",
        ),
        (
            Zustand,
            "1;gesendet;2026-11-05T08:00:00Z;2026-11-05T16:30:00Z",
        ),
        (Zustand, "1;angenommen"),
        (Zustand, "1;ausfuehrung;1;10000;150;2026-11-05T16:30:00Z"),
        (Abschluss, &abschluss),
    ]);
    let vermerk = some(depot_journal::vermerk_fuer(&inhalt, 0), "Vermerk")?;
    fs::write(daten.join("journal.txt"), &inhalt).map_err(|e| e.to_string())?;
    fs::write(daten.join("journal.vermerk"), &vermerk).map_err(|e| e.to_string())?;
    depot_journal::Journal::oeffnen(&daten.join("journal.txt")).map_err(|e| format!("{e:?}"))?;
    for tag in [5, 6] {
        let t = some(
            Datum::neu(2026, 11, tag).and_then(|d| Zeitpunkt::frankfurt(d, 18, 0, 0)),
            "Zeit",
        )?;
        let d = Dienst::neu(
            wurzel.clone(),
            TOKEN.into(),
            echo_starter(true),
            depot_dienst::lagerechner(Arc::new(Attrappe::default())),
        );
        assert!(d.schritt(t).is_none());
        let html = get(&d, "/teil/live", t);
        assert_eq!(html.contains("Ausstiegspläne zur Prüfung"), tag == 6);
        if tag == 6 {
            assert!(html.contains("Dachs · DE0007164600 · 1 von 1 Handelstagen"));
            assert!(html.contains("Kursziel beim Kauf 145,00 €"));
        }
    }
    Ok(())
}

#[test]
fn zeitplan_holt_kursabruf_und_papiertag_nach_update_probe_nach() -> Result<(), String> {
    let faellig = donnerstag(7, 30)?;
    let spaeter = donnerstag(7, 32)?;
    let tag = some(faellig.datum(), "Handelstag")?;
    for (kurs_vorher, erwartet) in [
        (None, Art::KurseVon(Boerse::Xetra)),
        (Some(tag), Art::Papier),
    ] {
        let wurzel = eingeschaltet(ordner()?)?;
        let erledigt = Erledigt {
            termine: Some(tag),
            kurse: kurs_vorher,
            xetra_abruf: kurs_vorher.map(|_| depot_dienst::zeitplan::Abruf {
                anker: faellig,
                versuche: 1,
                ausgang: depot_dienst::zeitplan::AbrufAusgang::Ok,
            }),
            ..Erledigt::default()
        };
        let datei = wurzel.join("daten/dienst_erledigt.txt");
        fs::write(&datei, erledigt.als_text()).map_err(|e| e.to_string())?;
        let offen = depot_dienst::rueckfall::offen_pfad(&wurzel);
        fs::write(&offen, "Update auf Probe").map_err(|e| e.to_string())?;
        let d = nur_xetra(wurzel);
        assert!(d.schritt(faellig).is_none());
        assert!(d.schritt(spaeter).is_none());
        assert!(wartend(&d).is_empty());
        assert_eq!(
            fs::read_to_string(datei).map_err(|e| e.to_string())?,
            erledigt.als_text()
        );

        fs::remove_file(offen).map_err(|e| e.to_string())?;
        let (nr, kind) = some(d.schritt(spaeter), "Fälliger Auftrag nach Bestätigung")?;
        assert_eq!(
            d.sperre().laufend.as_ref().map(|a| (a.art, a.anlass)),
            Some((erwartet, Anlass::Zeitplan))
        );
        d.verfolge(nr, kind, || donnerstag(7, 33).ok());
    }
    Ok(())
}

/// Maus' Abnahmefall 2: Windows startet zwischen Kursabruf und Papiertag neu. Der neue Dienst
/// holt den nicht begonnenen Papiertag mit den Kursen von heute nach und sagt das; danach
/// startet nichts mehr. Ohne Neustart steht kein solcher Satz da.
#[test]
fn neustart_zwischen_kursabruf_und_papiertag_holt_ihn_nach() -> Result<(), String> {
    let wurzel = eingeschaltet(ordner()?)?;
    let t = donnerstag(7, 31)?;
    {
        let d = nur_xetra(wurzel.clone());
        let (nr, kind) = some(d.schritt(t), "Termine")?;
        d.verfolge(nr, kind, || donnerstag(7, 31).ok());
        let (nr, kind) = some(d.schritt(t), "Kurse")?;
        d.verfolge(nr, kind, || donnerstag(7, 32).ok());
        assert_eq!(wartend(&d), vec![(Art::Papier, Anlass::Zeitplan)]);
        assert_eq!(neustart_satz(&d, t), None, "kein Neustart");
        // Hier endet der Dienst, bevor der Papiertag beginnt.
    }
    let spaeter = donnerstag(8, 20)?;
    let d = nur_xetra(wurzel.clone());
    let (nr, kind) = some(d.schritt(spaeter), "Papiertag nachgeholt")?;
    assert_eq!(
        d.sperre().laufend.as_ref().map(|a| (a.art, a.anlass)),
        Some((Art::Papier, Anlass::Zeitplan))
    );
    let satz = some(neustart_satz(&d, spaeter), "Satz")?;
    assert!(satz.contains("holt DepotSchlopf den Papiertag"), "{satz}");
    assert!(satz.contains("Du musst nichts tun"), "{satz}");
    d.verfolge(nr, kind, || donnerstag(8, 21).ok());
    let satz = some(neustart_satz(&d, spaeter), "Satz")?;
    assert!(satz.contains("nachgeholt"), "{satz}");
    assert!(d.schritt(spaeter).is_none(), "heute nichts mehr");
    // Ein weiterer Neustart startet ihn nicht ein zweites Mal.
    let wieder = nur_xetra(wurzel);
    assert!(wieder.schritt(donnerstag(19, 0)?).is_none());
    assert!(wartend(&wieder).is_empty());
    Ok(())
}

/// Lief der Papiertag schon, als der Dienst endete, bleibt der heutige Schutz: Kein neuer
/// Start von selbst, und die Ansicht sagt, wo es stehen blieb und wo es weitergeht.
#[test]
fn neustart_waehrend_des_papiertags_startet_ihn_nicht_neu() -> Result<(), String> {
    let wurzel = eingeschaltet(ordner()?)?;
    let t = donnerstag(7, 31)?;
    {
        let d = nur_xetra(wurzel.clone());
        let (nr, kind) = some(d.schritt(t), "Termine")?;
        d.verfolge(nr, kind, || donnerstag(7, 31).ok());
        let (nr, kind) = some(d.schritt(t), "Kurse")?;
        d.verfolge(nr, kind, || donnerstag(7, 32).ok());
        let (_, mut kind) = some(d.schritt(t), "Papier")?;
        let _ = kind.wait();
        // Der Dienst endet, waehrend der Papiertag laeuft: Sein Ende steht nie im Verlauf.
    }
    let spaeter = donnerstag(18, 20)?;
    let d = nur_xetra(wurzel);
    assert!(d.schritt(spaeter).is_none(), "kein zweiter Papiertag");
    assert!(wartend(&d).is_empty());
    let satz = some(neustart_satz(&d, spaeter), "Satz")?;
    assert!(satz.contains("Der Papiertag lief schon"), "{satz}");
    assert!(satz.contains("heutige Schutz bleibt"), "{satz}");
    assert!(seiten::live(TOKEN, "s", &d.sperre(), spaeter).contains("kopf halt\" id=\"neustart\""));
    Ok(())
}

/// Endet der Dienst waehrend des Kursabrufs, gibt es heute keinen Papiertag von selbst: Die
/// Kurse fehlen. Die Ansicht sagt das; der Abruf startet nicht doppelt.
#[test]
fn neustart_waehrend_des_kursabrufs_ohne_papiertag() -> Result<(), String> {
    let wurzel = eingeschaltet(ordner()?)?;
    let t = donnerstag(7, 31)?;
    {
        let d = nur_xetra(wurzel.clone());
        let (nr, kind) = some(d.schritt(t), "Termine")?;
        d.verfolge(nr, kind, || donnerstag(7, 31).ok());
        let (_, mut kind) = some(d.schritt(t), "Kurse")?;
        let _ = kind.wait();
    }
    let spaeter = donnerstag(18, 20)?;
    let d = nur_xetra(wurzel);
    assert!(d.schritt(spaeter).is_none());
    assert!(wartend(&d).is_empty());
    let satz = some(neustart_satz(&d, spaeter), "Satz")?;
    assert!(satz.contains("Xetra-Abruf unterbrochen"), "{satz}");
    Ok(())
}

#[test]
fn lange_diagnose_bleibt_nach_neustart_kopierbar() -> Result<(), String> {
    let wurzel = ordner()?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut d = Dienst::neu(
        wurzel.clone(),
        TOKEN.into(),
        Box::new(move |_, _| {
            let mut c = Command::new(&exe);
            c.args([
                "--exact",
                "update_leiste::diagnose_kindprozess",
                "--nocapture",
            ])
            .env("DEPOT_DIAGNOSE_KIND", "ja");
            c
        }),
        leeres_lagebild(),
    );
    d.version = Some("abcdef0123456789".into());
    let d = Arc::new(d);
    let t = donnerstag(12, 0)?;
    d.einreihen(Art::Termine, Anlass::Hand)
        .map_err(std::string::ToString::to_string)?;
    let (nr, kind) = some(d.schritt(t), "Start")?;
    d.verfolge(nr, kind, || donnerstag(12, 1).ok());
    let kennung = {
        let z = d.sperre();
        let a = some(z.verlauf.front(), "Verlauf")?;
        assert_eq!(a.erfolg, Some(false));
        assert_eq!(a.protokoll.len(), 500);
        assert!(a.protokoll.iter().any(|l| l.contains("Diagnosezeile 599")));
        a.kennung.clone()
    };
    let gespeichert = depot_dienst::protokoll::lesen(&wurzel, &kennung)?;
    for teil in [
        "Diagnosezeile 0\n",
        "Diagnosezeile 599",
        "FEHLER: Beispiel",
        "Version abcdef0123456789",
        "Lauf abgebrochen",
    ] {
        assert!(gespeichert.contains(teil), "fehlt: {teil}");
    }
    assert_eq!(gespeichert.matches("FEHLER: Beispiel").count(), 1);
    assert!(
        gespeichert.find("Ausgabe vor der Fehlermeldung") < gespeichert.find("FEHLER: Beispiel")
    );
    assert!(gespeichert.find("FEHLER: Beispiel") < gespeichert.find("Diagnosezeile 0"));
    drop(d);
    let neu = dienst(wurzel.clone(), true);
    assert!(
        neu.sperre()
            .verlauf
            .front()
            .is_some_and(|a| a.protokoll.iter().any(|l| l.contains("Diagnosezeile 599")))
    );
    let live = get(&neu, "/", t);
    assert!(live.contains("Letzter fehlgeschlagener Lauf"));
    let html = get(&neu, &format!("/protokoll?lauf={kennung}"), t);
    assert!(html.contains("data-protokoll-kopieren>Protokoll kopieren"));
    assert!(html.contains("<pre class=\"konsole\" tabindex=\"0\">"));
    assert!(html.contains("Diagnosezeile 0\n"));
    let download = get(&neu, &format!("/protokoll.txt?lauf={kennung}"), t);
    assert!(download.contains(&gespeichert));
    assert!(get(&neu, "/protokoll.txt?lauf=..%2Fjournal", t).starts_with("HTTP/1.1 400"));
    fs::remove_dir_all(wurzel).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn rss_intraday_ist_opt_in_spart_abrufe_und_uebersteht_neustart() -> Result<(), String> {
    let wurzel = eingeschaltet(ordner()?)?;
    let t = some(
        Datum::neu(2026, 11, 7).and_then(|d| Zeitpunkt::frankfurt(d, 12, 0, 0)),
        "Samstag",
    )?;
    let d = dienst(wurzel.clone(), true);
    assert!(
        d.schritt(t).is_none(),
        "kein neuer Netzzugriff ohne Einschalten"
    );
    {
        let mut z = d.sperre();
        z.zeitplan.nachrichten_an = true;
        fs::write(wurzel.join("daten/zeitplan.txt"), z.zeitplan.als_text())
            .map_err(|e| e.to_string())?;
    }
    let (nr, kind) = some(d.schritt(t), "RSS auch am Wochenende")?;
    assert_eq!(
        d.sperre().laufend.as_ref().map(|a| a.art),
        Some(Art::Nachrichten)
    );
    d.verfolge(nr, kind, || {
        Datum::neu(2026, 11, 7).and_then(|d| Zeitpunkt::frankfurt(d, 12, 0, 0))
    });
    assert!(d.schritt(t).is_none());
    drop(d);
    let neu = dienst(wurzel, true);
    assert!(
        neu.schritt(t).is_none(),
        "Neustart setzt die Pause nicht zurueck"
    );
    let spaeter = some(
        Datum::neu(2026, 11, 7).and_then(|d| Zeitpunkt::frankfurt(d, 13, 30, 0)),
        "spaeter",
    )?;
    let (_, mut kind) = some(neu.schritt(spaeter), "RSS nach 90 Minuten")?;
    assert_eq!(
        neu.sperre().laufend.as_ref().map(|a| a.art),
        Some(Art::Nachrichten)
    );
    let _ = kind.wait();
    Ok(())
}

#[test]
fn vorbereitungsautomatik_folgt_erfolgreichen_nachrichten_und_bleibt_nach_neustart_einmalig()
-> Result<(), String> {
    let wurzel = eingeschaltet(ordner()?)?;
    let t = Datum::neu(2026, 11, 7)
        .and_then(|d| Zeitpunkt::frankfurt(d, 12, 0, 0))
        .ok_or("Samstag")?;
    let d = dienst(wurzel.clone(), true);
    {
        let mut z = d.sperre();
        z.zeitplan.nachrichten_an = true;
        z.zeitplan.vorbereitung = depot_dienst::zeitplan::VorbereitungsPlan::An;
        fs::write(wurzel.join("daten/zeitplan.txt"), z.zeitplan.als_text())
            .map_err(|e| e.to_string())?;
    }
    let einrichtung = get(&d, "/einrichtung", t);
    assert!(einrichtung.contains("Diese Vorbereitung nutzt KI-Budget."));
    assert!(!einrichtung.contains("keine zusätzlichen KI-Aufrufe"));
    let spaeter = Zeitpunkt::frankfurt(t.datum().ok_or("Datum")?, 13, 30, 0).ok_or("Zeit")?;
    for (jetzt, erwartet) in [
        (t, Art::Nachrichten),
        (spaeter, Art::Vorbereiten),
        (spaeter, Art::Nachrichten),
    ] {
        let (nr, kind) = d.schritt(jetzt).ok_or("Auftrag fehlt")?;
        assert_eq!(d.sperre().laufend.as_ref().map(|a| a.art), Some(erwartet));
        d.verfolge(nr, kind, || {
            Datum::neu(2026, 11, 7).and_then(|d| Zeitpunkt::frankfurt(d, 13, 30, 0))
        });
    }
    drop(d);
    let neu = dienst(wurzel, true);
    assert!(
        neu.schritt(spaeter).is_none(),
        "Weitere Schlagzeilen lösen keine zweite Vorbereitung aus"
    );
    Ok(())
}

#[test]
fn vorbereitung_erst_nach_updatebestaetigung_einschalten_und_starten() -> Result<(), String> {
    let p = ordner()?;
    let d = dienst(p.clone(), true);
    let t = donnerstag(12, 0)?;
    let offen = p.join("daten/update_offen.txt");
    fs::write(&offen, "Probe").map_err(|e| e.to_string())?;
    let post = formular_post(
        "/ausfuehren",
        &format!("t={TOKEN}&art=vorbereiten&geprueft=ja"),
    );
    let antwort = text(&behandle(&d, &Attrappe::default(), &post, t));
    assert!(antwort.contains("m=vorbereitung_update"));
    assert!(wartend(&d).is_empty());
    let plan = Zeitplan {
        nachrichten_an: true,
        vorbereitung: depot_dienst::zeitplan::VorbereitungsPlan::An,
        ..Zeitplan::default()
    };
    assert_eq!(
        d.setze_zeitplan(plan),
        Err(depot_dienst::werkbank::ZeitplanAbgelehnt::UpdateOffen)
    );
    assert!(!d.sperre().zeitplan.vorbereitung.aktiv());
    fs::remove_file(offen).map_err(|e| e.to_string())?;
    d.setze_zeitplan(plan).map_err(|e| format!("{e:?}"))?;
    let samstag = some(
        Datum::neu(2026, 11, 7).and_then(|d| Zeitpunkt::frankfurt(d, 12, 0, 0)),
        "Samstag",
    )?;
    assert!(d.von_hand(Art::Vorbereiten, samstag).is_ok());
    Ok(())
}

#[test]
fn neustart_meldet_nicht_doppelt() -> Result<(), String> {
    let wurzel = ordner()?;
    let h = depot_kern::abdruck::sha256_hex(b"A");
    fs::write(
        wurzel.join("daten").join("hinweise.txt"),
        format!("{h};2026-11-07T10:00:00Z\n"),
    )
    .map_err(|e| format!("Vorbedingung Datei: {e}"))?;
    let gruende = Gruende::default();
    setze(&gruende, &["A"]);
    let (d, g) = hinweis_dienst(wurzel.clone(), &gruende, false);
    schritte(&d, &[0, 10, 20, 80])?;
    assert!(gezeigt(&g).is_empty());
    assert_eq!(
        hinweise_datei(&wurzel),
        format!("{h};2026-11-07T10:00:00Z\n")
    );
    Ok(())
}

#[test]
fn spaeter_neustart_holt_nach_oder_vermerkt_den_ausfall_einmal() -> Result<(), String> {
    let wurzel = ordner()?;
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
    let d = dienst(wurzel.clone(), true);
    let zeit = donnerstag(10, 15)?;
    for art in [
        Art::Termine,
        Art::KurseVon(Boerse::Xetra),
        Art::KurseVon(Boerse::Nyse),
        Art::Papier,
    ] {
        let (nr, kind) = some(d.schritt(zeit), "Auftrag nachholen")?;
        assert_eq!(d.sperre().laufend.as_ref().map(|a| a.art), Some(art));
        d.verfolge(nr, kind, || donnerstag(10, 16).ok());
    }
    drop(d);
    assert!(dienst(wurzel, true).schritt(donnerstag(10, 20)?).is_none());

    let wurzel = ordner()?;
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
    let d = dienst(wurzel.clone(), true);
    let (nr, kind) = some(d.schritt(donnerstag(17, 50)?), "Termine bleiben möglich")?;
    assert_eq!(
        d.sperre().laufend.as_ref().map(|a| a.art),
        Some(Art::Termine)
    );
    d.verfolge(nr, kind, || donnerstag(17, 51).ok());
    drop(d);
    let d = dienst(wurzel, true);
    assert!(d.schritt(donnerstag(18, 30)?).is_none());
    let verlauf =
        fs::read_to_string(daten.join("dienst_verlauf.txt")).map_err(|e| e.to_string())?;
    assert_eq!(verlauf.matches(";papier_ausgelassen;").count(), 1);
    let morgen = some(
        Datum::neu(2026, 11, 6).and_then(|d| Zeitpunkt::frankfurt(d, 7, 30, 0)),
        "Freitag",
    )?;
    let (nr, kind) = some(d.schritt(morgen), "Termine am Folgetag")?;
    assert_eq!(
        d.sperre().laufend.as_ref().map(|a| a.art),
        Some(Art::Termine)
    );
    d.verfolge(nr, kind, || None);
    Ok(())
}

#[test]
fn diagnose_kindprozess() {
    if std::env::var_os("DEPOT_DIAGNOSE_KIND").is_some() {
        println!("Ausgabe vor der Fehlermeldung");
        eprintln!("FEHLER: Beispiel vor langem Folgeprotokoll");
        for n in 0..600 {
            println!("Diagnosezeile {n}");
        }
        std::process::exit(3);
    }
}
