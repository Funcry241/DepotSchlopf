//! Das Lagebild zeigt, was Papiertag und Kostenschutz entscheiden wuerden. Alle Zahlen sind
//! fuer den Test gesetzt; die Kurse tragen die Herkunft `Quelle`, wie der Sammler sie belegt.
//! Jede Vorbereitung prueft ihren Erfolg ausdruecklich und endet sonst mit „Vorbedingung …“.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{Journal, KursBeleg};
use depot_kern::liste::{als_universum, beobachtungsliste};
use depot_kern::{BeraterId, Cent, Datum, Herkunft, Kurs, Preis, Topf, TopfId, Zeitpunkt};
use depot_ki::{Buchung, KiDeckel, KiGrenzen};
use depot_lage::{
    Abhilfe, Ampel, BudgetStatus, Hinterlegt, Lagebild, Umgebung, Zugang, html, ki_budget, lagebild,
};
use depot_sim::{Ausgang, Papierdatei, Stumm, papiertag};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

fn some<T>(o: Option<T>, was: &str) -> Result<T, String> {
    o.ok_or_else(|| format!("Vorbedingung {was}"))
}

struct Ort {
    dir: PathBuf,
    journal: PathBuf,
    konto: PathBuf,
}

impl Ort {
    fn neu() -> Result<Ort, String> {
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("depot_lage_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
        let journal = dir.join("journal.txt");
        vorbedingung(Journal::einrichten(&journal), "Einrichtung")?;
        Ok(Ort {
            konto: dir.join("papierkonto.txt"),
            dir,
            journal,
        })
    }

    fn oeffnen(&self) -> Result<Journal, String> {
        vorbedingung(Journal::oeffnen(&self.journal), "Journal oeffnen")
    }
}

impl Drop for Ort {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn tag(t: u8) -> Result<Datum, String> {
    some(Datum::neu(2026, 11, t), "Datum")
}

/// 18:00 Uhr in Frankfurt.
fn abends(t: u8) -> Result<Zeitpunkt, String> {
    some(Zeitpunkt::frankfurt(tag(t)?, 18, 0, 0), "Zeitpunkt")
}

fn start() -> Result<Vec<Topf>, String> {
    Ok(vec![
        vorbedingung(Topf::neu(TopfId::A, Cent(500_000)), "Topf A")?,
        vorbedingung(Topf::neu(TopfId::B, Cent(500_000)), "Topf B")?,
    ])
}

/// Belegt wie der Sammler alle Werte der Beobachtungsliste: Vortag 100 €, Tag 101 €.
fn belege_alle(j: &mut Journal, t: u8) -> Result<(), String> {
    let heute = tag(t)?;
    let vortag = tag(t.saturating_sub(1))?;
    let abruf = some(Zeitpunkt::frankfurt(heute, 17, 40, 0), "Abruf")?;
    for w in vorbedingung(beobachtungsliste(), "Liste")? {
        for (d, p) in [(vortag, "100"), (heute, "101")] {
            let zeit = some(Zeitpunkt::frankfurt(d, 17, 35, 0), "Schluss")?;
            let preis = some(Preis::lies_euro(p), "Preis")?;
            let kurs = vorbedingung(Kurs::exakt(preis, "EUR", d, zeit, Herkunft::Quelle), "Kurs")?;
            let beleg = vorbedingung(
                KursBeleg::neu("alphavantage", abruf, p.as_bytes(), w.isin.clone(), kurs),
                "Beleg",
            )?;
            vorbedingung(j.belege_kurs(&beleg), "Beleg schreiben")?;
        }
    }
    Ok(())
}

fn umgebung(jetzt: Zeitpunkt, journal: Zugang<'_>) -> Umgebung<'_> {
    Umgebung {
        jetzt,
        journal,
        kursquelle: Hinterlegt::Ja,
        berater: [
            (BeraterId::Dachs, Hinterlegt::Nein),
            (BeraterId::Maus, Hinterlegt::Nein),
        ],
        sperren: Vec::new(),
    }
}

fn punkt<'a>(l: &'a Lagebild, name: &str) -> Result<&'a depot_lage::Punkt, String> {
    l.punkte
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| format!("Punkt {name} fehlt: {:?}", l.punkte))
}

fn stummer_tag(ort: &Ort, t: u8) -> Result<Ausgang, String> {
    let mut j = ort.oeffnen()?;
    let start = start()?;
    let mut k = Papierdatei::oeffnen(&ort.konto, !j.stand().toepfe().is_empty(), &start)?;
    let universum = als_universum(&vorbedingung(beobachtungsliste(), "Liste")?);
    vorbedingung(
        papiertag(
            &mut j,
            tag(t)?,
            &universum,
            &start,
            &mut Stumm,
            &mut Stumm,
            &mut k,
        ),
        "Papiertag",
    )
}

#[test]
fn ohne_journal_ist_einrichten_der_naechste_schritt() -> Result<(), String> {
    let l = lagebild(&umgebung(abends(5)?, Zugang::NichtEingerichtet));
    let j = punkt(&l, "Journal")?;
    assert_eq!(j.ampel, Ampel::Offen);
    assert_eq!(j.abhilfe, Some(Abhilfe::JournalEinrichten));
    assert_eq!(l.gesamt, Ampel::Offen);
    assert!(
        l.naechster_schritt.starts_with("Journal"),
        "{}",
        l.naechster_schritt
    );
    assert!(l.budget.is_empty() && l.depot.is_none());
    Ok(())
}

#[test]
fn belegtes_journal_wartet_und_unlesbares_haelt() -> Result<(), String> {
    let l = lagebild(&umgebung(abends(5)?, Zugang::Belegt));
    assert_eq!(punkt(&l, "Journal")?.abhilfe, Some(Abhilfe::Warten));
    assert_eq!(l.gesamt, Ampel::Offen);
    let l = lagebild(&umgebung(abends(5)?, Zugang::Fehler("Kopfzeile fehlt")));
    assert_eq!(l.gesamt, Ampel::Halt);
    assert!(
        l.naechster_schritt.starts_with("Halt"),
        "{}",
        l.naechster_schritt
    );
    Ok(())
}

#[test]
fn sperrdatei_ist_halt() -> Result<(), String> {
    let mut u = umgebung(abends(5)?, Zugang::NichtEingerichtet);
    u.sperren = vec!["ki.sperre".into()];
    let l = lagebild(&u);
    assert_eq!(l.gesamt, Ampel::Halt);
    assert!(
        l.naechster_schritt.contains("ki.sperre"),
        "{}",
        l.naechster_schritt
    );
    Ok(())
}

#[test]
fn fehlender_schluessel_der_kursquelle_ist_offen() -> Result<(), String> {
    let mut u = umgebung(abends(5)?, Zugang::NichtEingerichtet);
    u.kursquelle = Hinterlegt::Nein;
    let l = lagebild(&u);
    assert_eq!(
        punkt(&l, "Kursquelle Alpha Vantage")?.abhilfe,
        Some(Abhilfe::SchluesselKursquelle)
    );
    assert_eq!(
        punkt(&l, "Dachs (Claude)")?.abhilfe,
        Some(Abhilfe::SchluesselBerater(BeraterId::Dachs))
    );
    Ok(())
}

#[test]
fn leeres_journal_braucht_kurse_und_termine() -> Result<(), String> {
    let ort = Ort::neu()?;
    let j = ort.oeffnen()?;
    let l = lagebild(&umgebung(abends(5)?, Zugang::Offen(j.stand())));
    assert_eq!(
        punkt(&l, "Schlusskurse")?.abhilfe,
        Some(Abhilfe::KurseAbrufen)
    );
    assert_eq!(punkt(&l, "Termine")?.abhilfe, Some(Abhilfe::TermineAbrufen));
    assert_eq!(punkt(&l, "Papiertag")?.abhilfe, Some(Abhilfe::Warten));
    assert_eq!(l.gesamt, Ampel::Offen);
    assert!(
        l.naechster_schritt.starts_with("Schlusskurse"),
        "{}",
        l.naechster_schritt
    );
    assert_eq!(l.budget.len(), 2);
    assert!(l.budget.iter().all(|b| b.status == BudgetStatus::Bereit));
    Ok(())
}

/// Kurse von gestern: Der Papiertag haelt, das Lagebild verlangt neue Kurse.
#[test]
fn kurse_von_gestern_verlangen_abruf() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    belege_alle(&mut j, 5)?;
    let l = lagebild(&umgebung(abends(6)?, Zugang::Offen(j.stand())));
    let k = punkt(&l, "Schlusskurse")?;
    assert_eq!(k.abhilfe, Some(Abhilfe::KurseAbrufen));
    assert!(k.text.contains("05.11.2026"), "{}", k.text);
    Ok(())
}

/// Kurse von heute: bereit; nach dem Papiertag abgeschlossen, Depot und Begruendungen sichtbar.
/// Das Lagebild sagt vorher „kann laufen“, und der Papiertag laeuft wirklich.
#[test]
fn bereit_dann_abgeschlossen_wie_der_papiertag() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    belege_alle(&mut j, 5)?;
    let l = lagebild(&umgebung(abends(5)?, Zugang::Offen(j.stand())));
    assert_eq!(
        punkt(&l, "Schlusskurse")?.ampel,
        Ampel::Gut,
        "{:?}",
        l.punkte
    );
    assert_eq!(
        punkt(&l, "Papiertag")?.abhilfe,
        Some(Abhilfe::PapiertagStarten)
    );
    assert!(
        l.naechster_schritt.starts_with("Papiertag"),
        "{}",
        l.naechster_schritt
    );
    drop(j);

    let aus = stummer_tag(&ort, 5)?;
    assert!(matches!(aus, Ausgang::Abgeschlossen { .. }), "{aus:?}");

    let j = ort.oeffnen()?;
    let l = lagebild(&umgebung(abends(5)?, Zugang::Offen(j.stand())));
    let p = punkt(&l, "Papiertag")?;
    assert_eq!(p.ampel, Ampel::Gut);
    assert!(p.text.contains("abgeschlossen"), "{}", p.text);
    let d = some(l.depot.as_ref(), "Depot")?;
    assert_eq!(d.toepfe.len(), 2);
    assert_eq!(d.letzter_abschluss, Some(tag(5)?));
    assert_eq!(d.gesamt, Some(Cent(1_000_000)));
    assert!(
        l.gespraech.iter().any(|b| b.wer == "Papiertag"),
        "{:?}",
        l.gespraech
    );
    Ok(())
}

/// Ein angefangener Tag haelt den Papiertag an; das Lagebild zeigt denselben Grund.
#[test]
fn angefangener_tag_ist_halt_mit_grund() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    belege_alle(&mut j, 5)?;
    drop(j);
    stummer_tag(&ort, 5)?;
    let mut j = ort.oeffnen()?;
    belege_alle(&mut j, 6)?;
    vorbedingung(j.begruende(tag(6)?, "Dachs", "angefangen"), "Begruendung")?;
    let l = lagebild(&umgebung(abends(6)?, Zugang::Offen(j.stand())));
    let p = punkt(&l, "Papiertag")?;
    assert_eq!(p.ampel, Ampel::Halt);
    assert!(p.text.contains("angefangen"), "{}", p.text);
    assert_eq!(l.gesamt, Ampel::Halt);
    drop(j);
    // Gegenprobe: Der Papiertag haelt wirklich.
    let mut j = ort.oeffnen()?;
    let start = start()?;
    let mut k = Papierdatei::oeffnen(&ort.konto, true, &start)?;
    let universum = als_universum(&vorbedingung(beobachtungsliste(), "Liste")?);
    let aus = papiertag(
        &mut j,
        tag(6)?,
        &universum,
        &start,
        &mut Stumm,
        &mut Stumm,
        &mut k,
    );
    assert!(aus.is_err(), "{aus:?}");
    Ok(())
}

fn buchung(berater: BeraterId, d: Datum, zyklus: u32) -> Result<Buchung, String> {
    Ok(Buchung {
        berater,
        datum: d,
        zyklus,
        reserviert: some(KiDeckel::standard(berater).hoechstkosten(), "Hoechstkosten")?,
    })
}

fn zyklus(d: Datum) -> Result<u32, String> {
    some(depot_sim::tagnummer(d), "Tagnummer")
}

#[test]
fn budget_zeigt_verbrauch_rest_und_pause() -> Result<(), String> {
    let heute = tag(5)?;
    let z = zyklus(heute)?;
    let je = some(
        KiDeckel::standard(BeraterId::Dachs).hoechstkosten(),
        "Kosten",
    )?;
    let grenzen = KiGrenzen::standard();
    let mut b = Vec::new();
    for _ in 0..grenzen.aufrufe_je_zyklus() {
        b.push(buchung(BeraterId::Dachs, heute, z)?);
    }
    let dachs = ki_budget(&b, BeraterId::Dachs, heute);
    assert_eq!(dachs.status, BudgetStatus::PausiertBisMorgen);
    assert_eq!(
        dachs.heute_aufrufe,
        usize::try_from(grenzen.aufrufe_je_zyklus()).unwrap_or(0)
    );
    let gebucht = some(
        je.mal_stueck(u64::from(grenzen.aufrufe_je_zyklus())),
        "Summe",
    )?;
    assert_eq!(dachs.heute_gebucht, gebucht);
    assert_eq!(dachs.monat_gebucht, gebucht);
    assert_eq!(
        dachs.monat_rest,
        some(grenzen.kosten_je_monat().checked_sub(gebucht), "Rest")?
    );
    assert_eq!(dachs.je_aufruf, Some(je));
    // Maus bleibt unberuehrt.
    let maus = ki_budget(&b, BeraterId::Maus, heute);
    assert_eq!(maus.status, BudgetStatus::Bereit);
    assert_eq!(maus.monat_gebucht, Cent::NULL);
    assert_eq!(maus.monat_rest, grenzen.kosten_je_monat());
    // Am naechsten Tag ist Dachs wieder bereit, solange der Monat Platz hat.
    let morgen = tag(6)?;
    let dachs = ki_budget(&b, BeraterId::Dachs, morgen);
    assert_eq!(dachs.status, BudgetStatus::Bereit);
    assert_eq!(dachs.heute_aufrufe, 0);
    Ok(())
}

#[test]
fn budget_monat_voll_und_uhr_rueckwaerts() -> Result<(), String> {
    let grenzen = KiGrenzen::standard();
    let mut b = Vec::new();
    let mut t = 2;
    while b.len() < usize::try_from(grenzen.aufrufe_je_monat()).unwrap_or(0) {
        let d = tag(t)?;
        b.push(buchung(BeraterId::Maus, d, zyklus(d)?)?);
        t = t.saturating_add(1);
    }
    let maus = ki_budget(&b, BeraterId::Maus, tag(20)?);
    assert_eq!(maus.status, BudgetStatus::PausiertBisMonatsanfang);
    let maus = ki_budget(&b, BeraterId::Maus, tag(1)?);
    assert!(
        matches!(maus.status, BudgetStatus::Gesperrt(_)),
        "{:?}",
        maus.status
    );
    // Neuer Monat: wieder bereit, das Jahr zaehlt weiter.
    let dez = some(Datum::neu(2026, 12, 1), "Datum")?;
    let maus = ki_budget(&b, BeraterId::Maus, dez);
    assert_eq!(maus.status, BudgetStatus::Bereit);
    assert_eq!(maus.monat_gebucht, Cent::NULL);
    assert!(maus.jahr_gebucht.0 > 0);
    Ok(())
}

#[test]
fn seite_maskiert_jeden_text() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    vorbedingung(
        j.begruende(tag(5)?, "Dachs", "<script>alert(1)</script> & \"x\""),
        "Begruendung",
    )?;
    let l = lagebild(&umgebung(abends(5)?, Zugang::Offen(j.stand())));
    let s = html::seite(&l);
    assert!(!s.contains("<script"), "{s}");
    assert!(s.contains("&lt;script&gt;alert(1)&lt;/script&gt; &amp; &quot;x&quot;"));
    assert!(s.contains("KI-Budget") && s.contains("Grenzen und Budgets"));
    assert_eq!(html::maskiere("a'b"), "a&#39;b");
    Ok(())
}

#[test]
fn zeiten_stehen_in_frankfurter_ortszeit() -> Result<(), String> {
    let l = lagebild(&umgebung(abends(5)?, Zugang::NichtEingerichtet));
    assert!(
        html::seite(&l).contains("05.11.2026 18:00 MEZ"),
        "{}",
        html::seite(&l)
    );
    let sommer = some(
        Zeitpunkt::frankfurt(some(Datum::neu(2026, 7, 1), "Datum")?, 9, 30, 0),
        "Zeitpunkt",
    )?;
    let l = lagebild(&umgebung(sommer, Zugang::NichtEingerichtet));
    assert!(html::seite(&l).contains("01.07.2026 09:30 MESZ"));
    Ok(())
}
