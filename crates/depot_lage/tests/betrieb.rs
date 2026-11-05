//! Schluesselstatus, Hinweise des Anbieters, Gesamtstatus und Verbrauch, alle aus dem
//! Anbieterprotokoll. Nichts davon entscheidet ueber einen KI-Aufruf; die Tests pruefen nur,
//! dass die Anzeige sagt, was gilt.

use depot_kern::{BeraterId, Cent, Datum, Zeitpunkt};
use depot_ki::Buchung;
use depot_lage::anbieter::{Eintrag, Ergebnis, Kosten, Wer};
use depot_lage::betrieb::{KiLage, anbieter_hinweis, betrieb, schluessel_stand};
use depot_lage::{
    Abhilfe, AnbieterHinweis, Betrieb, BudgetStatus, Hinterlegt, SchluesselStand, Umgebung, Zugang,
    ki_budget, lagebild, mit_verbrauch,
};

fn some<T>(o: Option<T>, was: &str) -> Result<T, String> {
    o.ok_or_else(|| format!("Vorbedingung {was}"))
}

fn tag(m: u8, t: u8) -> Result<Datum, String> {
    some(Datum::neu(2026, m, t), "Datum")
}

fn um(m: u8, t: u8, stunde: u8) -> Result<Zeitpunkt, String> {
    some(Zeitpunkt::frankfurt(tag(m, t)?, stunde, 0, 0), "Zeitpunkt")
}

fn e(zeit: Zeitpunkt, wer: Wer, ergebnis: Ergebnis) -> Eintrag {
    Eintrag::neu(zeit, wer, ergebnis, None)
}

fn lage(name: &'static str, schluessel: SchluesselStand) -> KiLage {
    KiLage {
        name,
        schluessel,
        hinweis: None,
        budget: Some(BudgetStatus::Bereit),
    }
}

#[test]
fn schluessel_stand_folgt_dem_letzten_aussagekraeftigen_aufruf() -> Result<(), String> {
    let a = um(11, 3, 18)?;
    let b = um(11, 4, 18)?;
    let c = um(11, 5, 18)?;
    // Ohne Schluessel hilft kein Protokoll.
    let ok = [e(a, Wer::Dachs, Ergebnis::Ok)];
    assert_eq!(
        schluessel_stand(Hinterlegt::Nein, &ok, Wer::Dachs),
        SchluesselStand::Fehlt
    );
    assert_eq!(
        schluessel_stand(Hinterlegt::Ja, &[], Wer::Dachs),
        SchluesselStand::Hinterlegt
    );
    assert_eq!(
        schluessel_stand(Hinterlegt::Unbekannt, &[], Wer::Dachs),
        SchluesselStand::Unbekannt
    );
    assert_eq!(
        schluessel_stand(Hinterlegt::Ja, &ok, Wer::Dachs),
        SchluesselStand::Geprueft(a)
    );
    // Ein Limit oder fehlendes Guthaben sagt: Schluessel angenommen. Netz sagt nichts.
    let p = [
        e(a, Wer::Dachs, Ergebnis::Abgelehnt),
        e(b, Wer::Dachs, Ergebnis::Limit),
        e(c, Wer::Dachs, Ergebnis::Netz),
        e(c, Wer::Maus, Ergebnis::Abgelehnt),
    ];
    assert_eq!(
        schluessel_stand(Hinterlegt::Ja, &p, Wer::Dachs),
        SchluesselStand::Geprueft(b)
    );
    assert_eq!(
        schluessel_stand(Hinterlegt::Ja, &p, Wer::Maus),
        SchluesselStand::Abgelehnt(c)
    );
    // Ein neu hinterlegter Schluessel ist wieder nur hinterlegt.
    let neu = [
        e(a, Wer::Maus, Ergebnis::Abgelehnt),
        e(b, Wer::Maus, Ergebnis::Neu),
    ];
    assert_eq!(
        schluessel_stand(Hinterlegt::Ja, &neu, Wer::Maus),
        SchluesselStand::Hinterlegt
    );
    assert_eq!(SchluesselStand::Abgelehnt(c).klasse(), "halt");
    assert_eq!(SchluesselStand::Geprueft(c).klasse(), "gut");
    Ok(())
}

#[test]
fn anbieterlimit_nennt_den_naechsten_versuch_und_laeuft_ab() -> Result<(), String> {
    let gestern = um(11, 4, 18)?;
    let heute = um(11, 5, 18)?;
    let spaeter = um(11, 5, 20)?;
    let limit = [e(heute, Wer::Maus, Ergebnis::Limit)];
    let h = some(anbieter_hinweis(&limit, Wer::Maus, spaeter), "Hinweis")?;
    assert!(matches!(h, AnbieterHinweis::Limit { aktiv: true, .. }));
    assert!(
        h.text()
            .contains("Nächster Versuch: beim nächsten Papiertag"),
        "{}",
        h.text()
    );
    // Am naechsten Tag ist das Limit Geschichte.
    let alt = [e(gestern, Wer::Maus, Ergebnis::Limit)];
    assert!(matches!(
        anbieter_hinweis(&alt, Wer::Maus, heute),
        Some(AnbieterHinweis::Limit { aktiv: false, .. })
    ));
    // Mit genannter Wartezeit gilt sie, auch ueber den Tag hinaus.
    let mut w = e(gestern, Wer::Maus, Ergebnis::Limit);
    w.warten_bis = Some(spaeter);
    let h = some(anbieter_hinweis(&[w], Wer::Maus, heute), "Hinweis")?;
    assert!(matches!(h, AnbieterHinweis::Limit { aktiv: true, .. }));
    assert!(
        h.text()
            .contains("beim ersten Papiertag nach 05.11.2026 20:00"),
        "{}",
        h.text()
    );
    // Ein erfolgreicher Aufruf danach loescht den Hinweis; Netzfehler nicht.
    let wieder = [
        e(gestern, Wer::Maus, Ergebnis::Guthaben),
        e(heute, Wer::Maus, Ergebnis::Ok),
    ];
    assert_eq!(anbieter_hinweis(&wieder, Wer::Maus, spaeter), None);
    let netz = [
        e(gestern, Wer::Maus, Ergebnis::Guthaben),
        e(heute, Wer::Maus, Ergebnis::Netz),
    ];
    assert_eq!(
        anbieter_hinweis(&netz, Wer::Maus, spaeter),
        Some(AnbieterHinweis::Guthaben(gestern))
    );
    Ok(())
}

#[test]
fn gesamtstatus_in_einem_wort() -> Result<(), String> {
    let z = um(11, 5, 18)?;
    let bereit = [
        lage("Dachs", SchluesselStand::Geprueft(z)),
        lage("Maus", SchluesselStand::Hinterlegt),
    ];
    assert_eq!(betrieb(Vec::new(), &bereit), Betrieb::Bereit);

    let mut ohne = bereit.clone();
    ohne[1].schluessel = SchluesselStand::Fehlt;
    let b = betrieb(Vec::new(), &ohne);
    assert!(matches!(b, Betrieb::OhneKi(_)), "{b:?}");
    assert!(b.gruende().iter().any(|g| g.contains("Pflichtaufträge")));

    // Nur eine pausiert: Der Betrieb laeuft ohne sie weiter.
    let mut eine = bereit.clone();
    eine[0].budget = Some(BudgetStatus::PausiertBisMorgen);
    assert!(matches!(betrieb(Vec::new(), &eine), Betrieb::OhneKi(_)));

    // Beide warten von selbst: pausiert.
    let mut beide = eine.clone();
    beide[1].hinweis = Some(AnbieterHinweis::Limit {
        zeit: z,
        warten_bis: None,
        aktiv: true,
    });
    let b = betrieb(Vec::new(), &beide);
    assert!(
        matches!(b, Betrieb::Pausiert(ref g) if g.len() == 2),
        "{b:?}"
    );
    assert_eq!(b.klasse(), "offen");

    // Ein abgelaufenes Limit pausiert nicht.
    let mut alt = bereit.clone();
    alt[1].hinweis = Some(AnbieterHinweis::Limit {
        zeit: z,
        warten_bis: None,
        aktiv: false,
    });
    assert_eq!(betrieb(Vec::new(), &alt), Betrieb::Bereit);

    // Abgelehnter Schluessel, fehlendes Guthaben und jeder Halt brauchen einen Menschen.
    let mut abgelehnt = bereit.clone();
    abgelehnt[0].schluessel = SchluesselStand::Abgelehnt(z);
    let mut guthaben = bereit.clone();
    guthaben[0].hinweis = Some(AnbieterHinweis::Guthaben(z));
    for b in [
        betrieb(Vec::new(), &abgelehnt),
        betrieb(Vec::new(), &guthaben),
        betrieb(vec!["Sperrdatei: x".into()], &bereit),
        betrieb(vec!["Sperrdatei: x".into()], &beide),
    ] {
        assert!(matches!(b, Betrieb::EingriffNoetig(_)), "{b:?}");
        assert_eq!(b.klasse(), "halt");
    }
    Ok(())
}

#[test]
fn verbrauch_kommt_aus_dem_protokoll_reserviert_aus_dem_kassenbuch() -> Result<(), String> {
    let heute = tag(11, 5)?;
    let buchung = Buchung {
        berater: BeraterId::Dachs,
        datum: heute,
        zyklus: some(depot_sim::tagnummer(heute), "Zyklus")?,
        reserviert: Cent(40),
    };
    let mut ok = e(um(11, 5, 17)?, Wer::Dachs, Ergebnis::Ok);
    ok.ist = Kosten::Betrag(Cent(12));
    let mut ok2 = e(um(11, 3, 17)?, Wer::Dachs, Ergebnis::Ok);
    ok2.ist = Kosten::Betrag(Cent(9));
    let mut unklar = e(um(11, 4, 17)?, Wer::Dachs, Ergebnis::Unklar);
    unklar.ist = Kosten::Unbekannt;
    let mut vormonat = e(um(10, 30, 17)?, Wer::Dachs, Ergebnis::Ok);
    vormonat.ist = Kosten::Betrag(Cent(500));
    let mut maus = e(um(11, 5, 17)?, Wer::Maus, Ergebnis::Ok);
    maus.ist = Kosten::Betrag(Cent(700));
    let protokoll = [ok, ok2, unklar, vormonat, maus];
    let b = mit_verbrauch(
        ki_budget(&[buchung], BeraterId::Dachs, heute),
        &protokoll,
        heute,
    );
    assert_eq!(b.monat_verbraucht, Cent(21));
    assert_eq!(b.monat_abgerechnet, 2);
    assert_eq!(b.monat_unbekannt, 1);
    // Der Kostenschutz rechnet weiter mit der Reservierung.
    assert_eq!(b.monat_gebucht, Cent(40));
    assert_eq!(b.status, BudgetStatus::Bereit);
    Ok(())
}

fn umgebung(jetzt: Zeitpunkt) -> Umgebung<'static> {
    Umgebung {
        jetzt,
        journal: Zugang::NichtEingerichtet,
        kursquelle: Hinterlegt::Ja,
        kursquelle_us: Hinterlegt::Ja,
        berater: [
            (BeraterId::Dachs, Hinterlegt::Ja),
            (BeraterId::Maus, Hinterlegt::Ja),
        ],
        sperren: Vec::new(),
        anbieter: Vec::new(),
        anbieter_unlesbar: 0,
        ki_halt: None,
    }
}

#[test]
fn sperre_des_kostenschutzes_fuehrt_zur_pruefung() -> Result<(), String> {
    let mut u = umgebung(um(11, 5, 18)?);
    u.sperren = vec!["ki.halt".into()];
    u.ki_halt = Some("Kosten unbekannt: Antwort fehlte (Dachs)\nmehr".into());
    let l = lagebild(&u);
    let p = some(
        l.punkte.iter().find(|p| p.name == "KI-Kostenschutz"),
        "Punkt Kostenschutz",
    )?;
    assert_eq!(p.abhilfe, Some(Abhilfe::KostenschutzPruefen));
    assert!(
        p.text.contains("Kosten unbekannt: Antwort fehlte (Dachs)"),
        "{}",
        p.text
    );
    assert!(!p.text.contains("mehr"), "{}", p.text);
    assert!(
        matches!(l.betrieb, Betrieb::EingriffNoetig(_)),
        "{:?}",
        l.betrieb
    );
    Ok(())
}

#[test]
fn abgelehnter_schluessel_und_limit_im_lagebild() -> Result<(), String> {
    let jetzt = um(11, 5, 18)?;
    let mut u = umgebung(jetzt);
    u.anbieter = vec![
        e(um(11, 5, 17)?, Wer::Dachs, Ergebnis::Limit),
        e(um(11, 5, 17)?, Wer::Maus, Ergebnis::Abgelehnt),
        e(um(11, 5, 17)?, Wer::Kursquelle, Ergebnis::Ok),
    ];
    let l = lagebild(&u);
    assert!(
        l.schluessel
            .contains(&(Wer::Maus, SchluesselStand::Abgelehnt(um(11, 5, 17)?))),
        "{:?}",
        l.schluessel
    );
    let gruende = l.betrieb.gruende().join("\n");
    assert!(matches!(l.betrieb, Betrieb::EingriffNoetig(_)), "{gruende}");
    assert!(
        gruende.contains("Maus (GPT): Schlüssel vom Anbieter abgelehnt"),
        "{gruende}"
    );
    // Ohne Ablehnung bleibt der Eingriff fuer das fehlende Journal; das Limit steht dahinter.
    u.anbieter.retain(|x| x.wer != Wer::Maus);
    let l = lagebild(&u);
    let gruende = l.betrieb.gruende().join("\n");
    assert!(matches!(l.betrieb, Betrieb::EingriffNoetig(_)), "{gruende}");
    assert!(
        gruende.contains("Journal: Noch nicht eingerichtet"),
        "{gruende}"
    );
    assert!(gruende.contains("Anbieterlimit erreicht"), "{gruende}");
    // Eine abgelehnte Kursquelle ist Halt.
    u.anbieter = vec![e(jetzt, Wer::Kursquelle, Ergebnis::Abgelehnt)];
    assert!(matches!(lagebild(&u).betrieb, Betrieb::EingriffNoetig(_)));
    Ok(())
}
