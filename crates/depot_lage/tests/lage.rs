//! Das Lagebild zeigt, was Papiertag und Kostenschutz entscheiden wuerden. Alle Zahlen sind
//! fuer den Test gesetzt; die Kurse tragen die Herkunft `Quelle`, wie der Sammler sie belegt.
//! Jede Vorbereitung prueft ihren Erfolg ausdruecklich und endet sonst mit „Vorbedingung …“.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::AufrufErlaubnis;
use depot_journal::anbieter::{Eintrag, Ergebnis, Wer};
use depot_journal::{Journal, KursBeleg};
use depot_kern::Vorschlag;
use depot_kern::liste::{als_universum, beobachtungsliste};
use depot_kern::{BeraterId, Cent, Datum, Herkunft, Kurs, Preis, Topf, TopfId, Zeitpunkt};
use depot_ki::{Buchung, KiDeckel, KiGrenzen};
use depot_lage::TagStand;
use depot_lage::{
    Abhilfe, Ampel, Betrieb, BudgetStatus, Hinterlegt, Lagebild, PAPIERKONTO, Umgebung, Zugang,
    html, ki_budget, lagebild, mit_journal,
};
use depot_sim::{Antwort, Ausgang, Berater, Lage, Papierdatei, Stumm, Urteil, papiertag};

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
    belege_um(j, t, 17, 40)
}

/// Wie `belege_alle`, mit dem Abruf zur angegebenen Uhrzeit (Frankfurt).
fn belege_um(j: &mut Journal, t: u8, stunde: u8, minute: u8) -> Result<(), String> {
    let heute = tag(t)?;
    let vortag = tag(t.saturating_sub(1))?;
    let abruf = some(Zeitpunkt::frankfurt(heute, stunde, minute, 0), "Abruf")?;
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
        kursquelle_us: Hinterlegt::Ja,
        berater: [
            (BeraterId::Dachs, Hinterlegt::Nein),
            (BeraterId::Maus, Hinterlegt::Nein),
        ],
        sperren: Vec::new(),
        anbieter: Vec::new(),
        anbieter_unlesbar: 0,
        ki_halt: None,
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
fn bereitschaft_trennt_handlung_warten_und_unbekannt() -> Result<(), String> {
    let mut u = umgebung(abends(5)?, Zugang::NichtEingerichtet);
    u.kursquelle = Hinterlegt::Unbekannt;
    let l = lagebild(&u);
    let html = html::inhalt(&l, &|_| None);
    assert!(
        html.contains("class=\"marke offen\">bitte erledigen</span></td><td>Journal"),
        "{html}"
    );
    assert!(
        html.contains("class=\"marke leise\">unbekannt</span>"),
        "{html}"
    );
    let l = lagebild(&umgebung(abends(5)?, Zugang::Belegt));
    let html = html::inhalt(&l, &|_| None);
    assert!(
        html.contains("class=\"marke leise\">wartet</span></td><td>Journal"),
        "{html}"
    );
    let teil = html::inhalt_mit(&l, &|_| None, false);
    assert!(!teil.contains("id=\"betrieb\""));
    assert!(
        !teil.contains(&l.naechster_schritt),
        "Die Dienst-Zusammenfassung steht nur einmal da"
    );
    Ok(())
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
fn vorbereitung_zeigt_beide_ergebnisse_maskiert_und_nur_echte_antworten_gruen() -> Result<(), String>
{
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    let zeit = abends(7)?;
    j.starte_vorbereitung(zeit, tag(9)?, vec!["Quelle <script> mit Ereignis".into()])
        .map_err(|e| format!("{e:?}"))?;
    let _ = j
        .reserviere_vorbereitung(zeit, BeraterId::Dachs)
        .map_err(|e| format!("{e:?}"))?;
    j.belege_vorbereitungsantwort(
        zeit,
        depot_journal::VorbereitungsAntwort {
            zeit,
            berater: BeraterId::Dachs,
            gefragt: true,
            verwertbar: true,
            text: "These <script>".into(),
            prognose: None,
        },
    )
    .map_err(|e| format!("{e:?}"))?;
    j.belege_vorbereitungsantwort(
        zeit,
        depot_journal::VorbereitungsAntwort {
            zeit,
            berater: BeraterId::Maus,
            gefragt: false,
            verwertbar: false,
            text: "Anbieterpause".into(),
            prognose: None,
        },
    )
    .map_err(|e| format!("{e:?}"))?;
    let l = lagebild(&umgebung(zeit, Zugang::Offen(j.stand())));
    let html = html::inhalt(&l, &|_| None);
    assert!(html.contains("09.11.2026"));
    assert_eq!(html.matches("Einschätzung vorhanden").count(), 1);
    assert!(html.contains("Nicht gefragt") && html.contains("Anbieterpause"));
    assert!(html.contains("These &lt;script&gt;") && !html.contains("<script>"));

    let heute = abends(8)?;
    let html = html::inhalt(
        &lagebild(&umgebung(heute, Zugang::Offen(j.stand()))),
        &|_| None,
    );
    assert!(html.contains("Heute ist noch kein neuer Wissensstand belegt"));

    j.starte_vorbereitung(
        heute,
        tag(9)?,
        vec!["Neuer Quellenstand vom Sonntag".into()],
    )
    .map_err(|e| format!("{e:?}"))?;
    j.belege_vorbereitungsantwort(
        heute,
        depot_journal::VorbereitungsAntwort {
            zeit: heute,
            berater: BeraterId::Dachs,
            gefragt: false,
            verwertbar: false,
            text: "Tagesrahmen ausgeschöpft".into(),
            prognose: None,
        },
    )
    .map_err(|e| format!("{e:?}"))?;
    let html = html::inhalt(
        &lagebild(&umgebung(heute, Zugang::Offen(j.stand()))),
        &|_| None,
    );
    assert!(html.contains("Neuer Quellenstand vom Sonntag"));
    assert!(html.contains("Tagesrahmen ausgeschöpft"));
    assert!(html.contains("Letzte belegte Einschätzung vom 07.11.2026"));
    assert!(html.contains("These &lt;script&gt;"));
    assert!(!html.contains("Heute ist noch kein neuer Wissensstand belegt"));
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

/// Die Liste hat NYSE-Werte: Der Tiingo-Schluessel ist ein eigener Punkt. Fehlt er, ist er der
/// naechste Schritt nach dem Schluessel von Alpha Vantage und braucht einen Menschen.
#[test]
fn fehlender_schluessel_von_tiingo_ist_offen() -> Result<(), String> {
    let mut u = umgebung(abends(5)?, Zugang::NichtEingerichtet);
    let l = lagebild(&u);
    let p = punkt(&l, "Kursquelle Tiingo (US-Börsen)")?;
    assert_eq!((p.ampel, p.abhilfe), (Ampel::Gut, None), "{p:?}");
    u.kursquelle_us = Hinterlegt::Nein;
    let l = lagebild(&u);
    let p = punkt(&l, "Kursquelle Tiingo (US-Börsen)")?;
    assert_eq!(
        (p.ampel, p.abhilfe),
        (Ampel::Offen, Some(Abhilfe::SchluesselKursquelleUs))
    );
    assert!(Abhilfe::SchluesselKursquelleUs.braucht_menschen());
    u.journal = Zugang::Belegt;
    let l = lagebild(&u);
    assert!(
        l.naechster_schritt
            .starts_with("Kursquelle Tiingo (US-Börsen)"),
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
        punkt(&l, "Kursquelle Alpha Vantage (Xetra)")?.abhilfe,
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
    // Den Monat mit so vielen Aufrufen je Tag fuellen, wie ein Zyklus erlaubt; die Zahl der
    // Tage folgt aus den Grenzen und nicht aus einer Annahme im Test.
    let mut b = Vec::new();
    let mut t = 2;
    let voll = usize::try_from(grenzen.aufrufe_je_monat()).unwrap_or(0);
    while b.len() < voll {
        let d = tag(t)?;
        for _ in 0..grenzen.aufrufe_je_zyklus() {
            if b.len() < voll {
                b.push(buchung(BeraterId::Maus, d, zyklus(d)?)?);
            }
        }
        t = t.saturating_add(1);
    }
    let maus = ki_budget(&b, BeraterId::Maus, tag(t)?);
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

/// Das Lagebild, wie `run.cmd lage` es ueber `mit_journal` liest.
fn lage_aus(dir: &std::path::Path, t: u8) -> Result<Lagebild, String> {
    let jetzt = abends(t)?;
    Ok(mit_journal(dir, |z| lagebild(&umgebung(jetzt, z))))
}

/// K26-01 (Maus' Gegenprobe): Journal geloescht, Vermerk noch da. Das ist kein neuer,
/// sondern ein unvollstaendiger Datenbestand: Halt, nie „einrichten“.
#[test]
fn geloeschtes_journal_mit_vermerk_ist_halt() -> Result<(), String> {
    let ort = Ort::neu()?;
    vorbedingung(fs::remove_file(&ort.journal), "Journal loeschen")?;
    let l = lage_aus(&ort.dir, 5)?;
    let j = punkt(&l, "Journal")?;
    assert_eq!(j.ampel, Ampel::Halt, "{j:?}");
    assert_ne!(j.abhilfe, Some(Abhilfe::JournalEinrichten));
    assert_eq!(l.gesamt, Ampel::Halt);
    assert!(
        l.naechster_schritt.starts_with("Halt"),
        "{}",
        l.naechster_schritt
    );
    // Einrichten ginge auch nicht: Der Vermerk ist da.
    assert!(Journal::einrichten(&ort.journal).is_err());
    Ok(())
}

/// Ein Papierkonto ohne Journal und Vermerk ist ebenfalls ein unvollstaendiger Bestand.
#[test]
fn papierkonto_ohne_journal_ist_halt() -> Result<(), String> {
    let ort = Ort::neu()?;
    vorbedingung(fs::remove_file(&ort.journal), "Journal loeschen")?;
    vorbedingung(
        fs::remove_file(ort.journal.with_extension("vermerk")),
        "Vermerk loeschen",
    )?;
    vorbedingung(fs::write(ort.dir.join(PAPIERKONTO), "x"), "Papierkonto")?;
    let l = lage_aus(&ort.dir, 5)?;
    assert_eq!(punkt(&l, "Journal")?.ampel, Ampel::Halt);
    Ok(())
}

/// Ohne jede Spur ist der Bestand neu; mit eingerichtetem Journal ist er offen; hat ein Lauf
/// das Journal, ist er belegt.
#[test]
fn neu_offen_und_belegt_ueber_mit_journal() -> Result<(), String> {
    let ort = Ort::neu()?;
    let leer = ort.dir.join("leer");
    vorbedingung(fs::create_dir_all(&leer), "leerer Ordner")?;
    let l = lage_aus(&leer, 5)?;
    assert_eq!(
        punkt(&l, "Journal")?.abhilfe,
        Some(Abhilfe::JournalEinrichten)
    );
    let l = lage_aus(&ort.dir, 5)?;
    assert_eq!(punkt(&l, "Journal")?.ampel, Ampel::Gut);
    let offen = ort.oeffnen()?;
    let l = lage_aus(&ort.dir, 5)?;
    assert_eq!(punkt(&l, "Journal")?.abhilfe, Some(Abhilfe::Warten));
    drop(offen);
    Ok(())
}

/// Sperrdateien: `*.sperre` und `*.halt` in jeder Schreibweise, sonst nichts.
#[test]
fn sperrdateien_sperre_und_halt() -> Result<(), String> {
    let ort = Ort::neu()?;
    for n in ["lauf.sperre", "ki.halt", "X.HALT", "notiz.txt"] {
        vorbedingung(fs::write(ort.dir.join(n), ""), "Datei")?;
    }
    assert_eq!(
        depot_lage::sperrdateien(&ort.dir),
        vec!["X.HALT".to_string(), "ki.halt".into(), "lauf.sperre".into()]
    );
    Ok(())
}

/// Maus zu #31: „Vollständig bereit“ nur, wenn auch die Einrichtung fertig ist. Mit beiden
/// KI-Schluesseln, aber ohne Journal oder ohne Schluessel der Kursquelle, ist ein Eingriff
/// noetig; mit Journal und Kursen ist der Betrieb bereit.
#[test]
fn vollstaendig_bereit_nur_mit_fertiger_einrichtung() -> Result<(), String> {
    let mit_kis = |journal| {
        let mut u = umgebung(abends(5)?, journal);
        u.berater = [
            (BeraterId::Dachs, Hinterlegt::Ja),
            (BeraterId::Maus, Hinterlegt::Ja),
        ];
        Ok::<_, String>(u)
    };
    let l = lagebild(&mit_kis(Zugang::NichtEingerichtet)?);
    assert!(
        matches!(&l.betrieb, Betrieb::EingriffNoetig(g) if g.iter().any(|t| t.starts_with("Journal"))),
        "{:?}",
        l.betrieb
    );
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    belege_alle(&mut j, 5)?;
    let l = lagebild(&mit_kis(Zugang::Offen(j.stand()))?);
    assert_eq!(l.betrieb, Betrieb::Bereit, "{:?}", l.punkte);
    let mut ohne_quelle = mit_kis(Zugang::Offen(j.stand()))?;
    ohne_quelle.kursquelle = Hinterlegt::Nein;
    assert!(matches!(
        lagebild(&ohne_quelle).betrieb,
        Betrieb::EingriffNoetig(_)
    ));
    Ok(())
}

/// Ein angebundener Berater, der antwortet, aber nichts vorschlaegt.
struct Antwortet;

impl Berater for Antwortet {
    fn vorschlagen(&mut self, _erlaubnis: AufrufErlaubnis, _lage: &Lage<'_>) -> Antwort {
        Antwort {
            prognose: None,
            verwertbar: true,
            einschaetzung: "Heute nichts kaufen.".into(),
            vorschlag: None,
        }
    }

    fn pruefen(
        &mut self,
        _erlaubnis: AufrufErlaubnis,
        _lage: &Lage<'_>,
        _vorschlag: &Vorschlag,
    ) -> Urteil {
        Urteil {
            verwertbar: true,
            zustimmung: false,
            begruendung: "nein".into(),
        }
    }
}

/// Ein angebundener Berater, dessen Antwort nicht verwertbar ist (etwa unlesbar).
struct Verwirft;

impl Berater for Verwirft {
    fn vorschlagen(&mut self, _erlaubnis: AufrufErlaubnis, _lage: &Lage<'_>) -> Antwort {
        Antwort {
            prognose: None,
            verwertbar: false,
            einschaetzung: "keine verwertbare Antwort: Limit".into(),
            vorschlag: None,
        }
    }

    fn pruefen(
        &mut self,
        _erlaubnis: AufrufErlaubnis,
        _lage: &Lage<'_>,
        _vorschlag: &Vorschlag,
    ) -> Urteil {
        Urteil {
            verwertbar: false,
            zustimmung: false,
            begruendung: "keine verwertbare Antwort: Limit".into(),
        }
    }
}

/// Ein Berater ohne Schluessel, der seinen Grund nennt.
struct OhneSchluessel;

impl Berater for OhneSchluessel {
    fn angebunden(&self) -> bool {
        false
    }

    fn warum_nicht(&self) -> Option<String> {
        Some("kein Schlüssel hinterlegt".into())
    }

    fn vorschlagen(&mut self, _erlaubnis: AufrufErlaubnis, _lage: &Lage<'_>) -> Antwort {
        Antwort {
            prognose: None,
            verwertbar: false,
            einschaetzung: "nicht angebunden".into(),
            vorschlag: None,
        }
    }

    fn pruefen(
        &mut self,
        _erlaubnis: AufrufErlaubnis,
        _lage: &Lage<'_>,
        _vorschlag: &Vorschlag,
    ) -> Urteil {
        Urteil {
            verwertbar: false,
            zustimmung: false,
            begruendung: "nicht angebunden".into(),
        }
    }
}

fn tag_nr(l: &Lagebild, i: usize) -> Result<&depot_lage::TagErgebnis, String> {
    l.tage
        .get(i)
        .ok_or_else(|| format!("Tag {i} fehlt: {:?}", l.tage))
}

fn ki_nr(
    t: &depot_lage::TagErgebnis,
    i: usize,
) -> Result<&(BeraterId, depot_lage::KiBeitrag), String> {
    t.ki.get(i)
        .ok_or_else(|| format!("KI {i} fehlt: {:?}", t.ki))
}

/// Maus' Praxistest: Das Tagesergebnis zeigt ohne Journal und Konsole, welche Tage liefen,
/// welche ausfielen und warum, und welche KI tatsaechlich beigetragen hat.
#[test]
fn tagesergebnis_zeigt_ausfall_und_beitrag_je_ki() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut jr = ort.oeffnen()?;
    belege_alle(&mut jr, 5)?;
    let start = start()?;
    let mut konto = Papierdatei::oeffnen(&ort.konto, false, &start)?;
    let universum = als_universum(&vorbedingung(beobachtungsliste(), "Liste")?);
    let aus = vorbedingung(
        papiertag(
            &mut jr,
            tag(5)?,
            &universum,
            &start,
            &mut Antwortet,
            &mut OhneSchluessel,
            &mut konto,
        ),
        "Papiertag",
    )?;
    assert!(matches!(aus, Ausgang::Abgeschlossen { .. }), "{aus:?}");
    // Freitag, der 6., ohne Kurse; Montag, der 9., mit einem nachgeholten Abruf um 20 Uhr:
    // Kurse zu alt, darum kein Papiertag; Dienstag, der 10., mit Kursen, aber ohne Papiertag.
    belege_um(&mut jr, 9, 20, 0)?;
    belege_alle(&mut jr, 10)?;
    let mut umg = umgebung(abends(11)?, Zugang::Offen(jr.stand()));
    // Maus' Befund 2: Ein erfolgreicher Aufruf im Protokoll macht Maus nicht gruen, wenn der
    // Papiertag sie laut Journal gar nicht gefragt hat.
    let zeit = some(Zeitpunkt::frankfurt(tag(5)?, 17, 50, 0), "Zeit")?;
    umg.anbieter = vec![Eintrag::neu(zeit, Wer::Maus, Ergebnis::Ok, Some(200))];
    let l = lagebild(&umg);
    let tage: Vec<(Datum, &TagStand)> = l.tage.iter().map(|t| (t.datum, &t.stand)).collect();
    assert_eq!(l.tage.len(), 5, "{tage:?}");
    assert_eq!(tag_nr(&l, 0)?.stand, TagStand::Offen);
    assert!(
        matches!(&tag_nr(&l, 1)?.stand, TagStand::Ausgefallen(g) if g.contains("Schlusskurse liegen vor")),
        "{tage:?}"
    );
    // Maus' Befund 1: Der konkrete Grund steht in der Zeile des verpassten Tages. Der Abruf
    // vom 10. belegt den 9. noch einmal; fuer den 9. zaehlt nur, was am 9. abgerufen war.
    assert!(
        matches!(&tag_nr(&l, 2)?.stand, TagStand::Ausgefallen(g)
            if g.contains("Kursprüfung") && g.contains("Kurse zu alt")),
        "{tage:?}"
    );
    assert!(
        matches!(&tag_nr(&l, 3)?.stand, TagStand::Ausgefallen(g) if g.contains("keine Schlusskurse")),
        "{tage:?}"
    );
    let fuenf = tag_nr(&l, 4)?;
    assert_eq!(fuenf.datum, tag(5)?);
    assert_eq!(fuenf.stand, TagStand::Abgeschlossen);
    let dachs = ki_nr(fuenf, 0)?;
    assert_eq!(dachs.0, BeraterId::Dachs);
    assert_eq!(dachs.1.klasse, "gut", "{dachs:?}");
    assert!(dachs.1.text.contains("hat beigetragen"), "{dachs:?}");
    let maus = ki_nr(fuenf, 1)?;
    assert_eq!(maus.0, BeraterId::Maus);
    assert_eq!(maus.1.klasse, "halt", "{maus:?}");
    assert!(
        maus.1
            .text
            .contains("ausgefallen, nicht gefragt: kein Schlüssel hinterlegt"),
        "{maus:?}"
    );
    let s = html::seite(&l);
    assert!(s.contains("Tagesergebnis"), "{s}");
    assert!(s.contains("kein Schlüssel hinterlegt"), "{s}");
    assert!(s.contains("Schlusskurse liegen vor"), "{s}");
    assert!(s.contains("Kurse zu alt"), "{s}");
    Ok(())
}

/// Gefragt, aber die Antwort wurde verworfen: rot, mit dem Grund aus dem Protokoll. Ein
/// erfolgreicher Aufruf im Protokoll aendert daran nichts (Maus' Befund 2: Die Antwort kann
/// nach dem Aufruf verworfen worden sein).
#[test]
fn tagesergebnis_nennt_den_fehler_des_anbieters() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut jr = ort.oeffnen()?;
    belege_alle(&mut jr, 5)?;
    let start = start()?;
    let mut konto = Papierdatei::oeffnen(&ort.konto, false, &start)?;
    let universum = als_universum(&vorbedingung(beobachtungsliste(), "Liste")?);
    vorbedingung(
        papiertag(
            &mut jr,
            tag(5)?,
            &universum,
            &start,
            &mut Antwortet,
            &mut Verwirft,
            &mut konto,
        ),
        "Papiertag",
    )?;
    let mut umg = umgebung(abends(5)?, Zugang::Offen(jr.stand()));
    let zeit = some(Zeitpunkt::frankfurt(tag(5)?, 17, 50, 0), "Zeit")?;
    umg.anbieter = vec![
        Eintrag::neu(zeit, Wer::Maus, Ergebnis::Ok, Some(200)),
        Eintrag::neu(zeit, Wer::Maus, Ergebnis::Limit, Some(429)),
    ];
    let l = lagebild(&umg);
    let t = some(l.tage.first(), "Tag")?;
    assert_eq!(t.stand, TagStand::Abgeschlossen);
    // Dachs' Antwort hat der Papiertag verwendet; das Protokoll braucht es dafuer nicht.
    assert_eq!(ki_nr(t, 0)?.1.klasse, "gut", "{:?}", t.ki);
    assert_eq!(ki_nr(t, 1)?.1.klasse, "halt", "{:?}", t.ki);
    assert!(
        ki_nr(t, 1)?
            .1
            .text
            .contains("gefragt ohne verwendbare Antwort: Anbieterlimit"),
        "{:?}",
        t.ki
    );
    // Ohne Protokoll bleibt Maus rot, mit allgemeinem Grund.
    umg.anbieter.clear();
    let l = lagebild(&umg);
    let t = some(l.tage.first(), "Tag")?;
    assert_eq!(ki_nr(t, 1)?.1.klasse, "halt", "{:?}", t.ki);
    assert!(
        ki_nr(t, 1)?.1.text.contains("Antwort verworfen"),
        "{:?}",
        t.ki
    );
    Ok(())
}

/// Maus' Restbefund zu PR #33: Ein altes Journal schrieb ein verworfenes Urteil als „Veto:
/// keine verwertbare Antwort“. Das ist kein Beitrag; ein echtes Veto schon.
#[test]
fn altes_veto_ohne_verwertbare_antwort_ist_kein_beitrag() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut jr = ort.oeffnen()?;
    belege_alle(&mut jr, 5)?;
    drop(jr);
    stummer_tag(&ort, 5)?;
    let mut jr = ort.oeffnen()?;
    belege_alle(&mut jr, 6)?;
    for (wer, text) in [
        ("Papiertag", "Papiergeld; Ausfuehrung zum Schlusskurs"),
        ("Dachs", "Topf A: SAP gefaellt [Kosten 3 von 24 Cent]"),
        (
            "Maus",
            "Topf A: Veto: keine verwertbare Antwort: Limit [Kosten 0 von 24 Cent]",
        ),
        (
            "Maus",
            "Topf B: keine verwertbare Antwort: unlesbar [Kosten 0 von 24 Cent]",
        ),
    ] {
        vorbedingung(jr.begruende(tag(6)?, wer, text), "Begruendung")?;
    }
    let l = lagebild(&umgebung(abends(6)?, Zugang::Offen(jr.stand())));
    let t = tag_nr(&l, 0)?;
    assert_eq!(t.datum, tag(6)?);
    let maus = ki_nr(t, 1)?;
    assert_eq!(maus.1.klasse, "halt", "{maus:?}");
    assert!(maus.1.text.contains("ohne verwendbare Antwort"), "{maus:?}");
    // Dachs' alter Eintrag ohne Kennwort ist weder gruen noch rot.
    assert_eq!(ki_nr(t, 0)?.1.klasse, "offen", "{:?}", t.ki);
    drop(l);
    vorbedingung(
        jr.begruende(tag(6)?, "Maus", "Topf A: Veto: zu konzentriert"),
        "Begruendung",
    )?;
    let l = lagebild(&umgebung(abends(6)?, Zugang::Offen(jr.stand())));
    let maus = ki_nr(tag_nr(&l, 0)?, 1)?;
    assert_eq!(maus.1.klasse, "gut", "{maus:?}");
    assert!(maus.1.text.contains("2 verworfen"), "{maus:?}");
    Ok(())
}

/// Ein nachgeholter Abruf (Rechner schlief um 17:35) heisst „Kurse zu alt“, ohne Knopf, der
/// heute nichts mehr aendert; der Tag laeuft ohne Fragen an die Berater und sagt das.
#[test]
fn nachgeholter_abruf_heisst_kurse_zu_alt() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut jr = ort.oeffnen()?;
    belege_um(&mut jr, 5, 20, 0)?;
    let spaet = some(Zeitpunkt::frankfurt(tag(5)?, 21, 0, 0), "Zeit")?;
    let l = lagebild(&umgebung(spaet, Zugang::Offen(jr.stand())));
    let p = punkt(&l, "Schlusskurse")?;
    assert!(p.text.contains("Kurse zu alt"), "{}", p.text);
    assert!(p.text.contains("Morgen rechtzeitig"), "{}", p.text);
    assert_eq!(p.abhilfe, None, "{p:?}");
    drop(jr);
    let aus = stummer_tag(&ort, 5)?;
    assert!(matches!(aus, Ausgang::Abgeschlossen { .. }), "{aus:?}");
    let jr = ort.oeffnen()?;
    let l = lagebild(&umgebung(spaet, Zugang::Offen(jr.stand())));
    let t = some(l.tage.first(), "Tag")?;
    assert!(
        matches!(&t.stand, TagStand::OhneHandel(b) if b.iter().any(|x| x.contains("Kurse zu alt"))),
        "{t:?}"
    );
    assert!(
        t.ki.iter()
            .all(|(_, b)| b.text.contains("Kursprüfung ließ den Tag ausfallen")),
        "{t:?}"
    );
    let s = html::seite(&l);
    assert!(s.contains("Abgeschlossen ohne Handel"), "{s}");
    Ok(())
}

/// Das Papierdepot sagt, dass jedes Ergebnis eine simulierte Ausfuehrung zum Schlusskurs ist.
#[test]
fn papierdepot_nennt_die_simulation() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut jr = ort.oeffnen()?;
    belege_alle(&mut jr, 5)?;
    drop(jr);
    stummer_tag(&ort, 5)?;
    let jr = ort.oeffnen()?;
    let l = lagebild(&umgebung(abends(5)?, Zugang::Offen(jr.stand())));
    let s = html::seite(&l);
    assert!(
        s.contains("Nur Papiergeld, simuliert")
            && s.contains("geprüften Schlusskurs nach ihrem Versand"),
        "{s}"
    );
    Ok(())
}

/// Belegt wie der Sammler die Werte der Liste ausser den ersten `ohne`: Vortag 100 €, Tag 101 €.
fn belege_ohne(j: &mut Journal, heute: Datum, vortag: Datum, ohne: usize) -> Result<(), String> {
    let abruf = some(Zeitpunkt::frankfurt(heute, 17, 40, 0), "Abruf")?;
    for w in vorbedingung(beobachtungsliste(), "Liste")?
        .into_iter()
        .skip(ohne)
    {
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

/// Ein Feiertag aller Boersen der Liste ist geschlossen, nicht verpasst; ein Tag, an dem nur
/// eine Boerse schliesst (Heiligabend: Xetra zu, NYSE offen), ist ein Handelstag. Ein Tag, an dem
/// einzelne Kandidaten ohne Kurs ausgelassen wurden, ist abgeschlossen und nennt sie. An einem
/// Feiertag verlangt die Bereitschaft keinen Abruf.
#[test]
fn feiertag_ist_geschlossen_und_auslassung_sichtbar() -> Result<(), String> {
    let d = |t| some(Datum::neu(2026, 12, t), "Datum");
    let ort = Ort::neu()?;
    let mut jr = ort.oeffnen()?;
    belege_ohne(&mut jr, d(23)?, d(22)?, 2)?;
    let start = start()?;
    let mut konto = Papierdatei::oeffnen(&ort.konto, false, &start)?;
    let universum = als_universum(&vorbedingung(beobachtungsliste(), "Liste")?);
    let aus = vorbedingung(
        papiertag(
            &mut jr,
            d(23)?,
            &universum,
            &start,
            &mut Stumm,
            &mut Stumm,
            &mut konto,
        ),
        "Papiertag",
    )?;
    assert!(matches!(aus, Ausgang::Abgeschlossen { .. }), "{aus:?}");

    let montag = some(Zeitpunkt::frankfurt(d(28)?, 18, 0, 0), "Zeit")?;
    let l = lagebild(&umgebung(montag, Zugang::Offen(jr.stand())));
    let tage: Vec<(Datum, &TagStand)> = l.tage.iter().map(|t| (t.datum, &t.stand)).collect();
    assert_eq!(l.tage.len(), 4, "{tage:?}");
    assert_eq!(tag_nr(&l, 0)?.stand, TagStand::Offen);
    assert!(
        matches!(&tag_nr(&l, 1)?.stand, TagStand::Geschlossen(g)
            if g.contains("Xetra: 1. Weihnachtstag") && g.contains("NYSE: Christmas Day")),
        "{tage:?}"
    );
    assert_eq!(tag_nr(&l, 1)?.stand.klasse(), "offen");
    assert!(
        matches!(&tag_nr(&l, 2)?.stand, TagStand::Ausgefallen(_)),
        "Heiligabend handelt die NYSE: {tage:?}"
    );
    assert!(
        matches!(&tag_nr(&l, 3)?.stand, TagStand::MitAuslassung(b) if b.iter().any(|x| x.contains("2× Kurs fehlt"))),
        "{tage:?}"
    );
    assert_eq!(tag_nr(&l, 3)?.stand.klasse(), "gut");

    let weihnachten = some(Zeitpunkt::frankfurt(d(25)?, 18, 0, 0), "Zeit")?;
    let l = lagebild(&umgebung(weihnachten, Zugang::Offen(jr.stand())));
    let k = punkt(&l, "Schlusskurse")?;
    assert!(k.text.contains("Weihnachtstag"), "{k:?}");
    assert_eq!(k.abhilfe, None, "{k:?}");
    let p = punkt(&l, "Papiertag")?;
    assert!(p.text.contains("Börse geschlossen"), "{p:?}");
    Ok(())
}

/// Die Kurssperre nach einem Sicherheitsabbruch ist Halt mit eigenem Text.
#[test]
fn kurssperre_ist_halt() -> Result<(), String> {
    let ort = Ort::neu()?;
    let jr = ort.oeffnen()?;
    let mut umg = umgebung(abends(5)?, Zugang::Offen(jr.stand()));
    umg.sperren = vec!["kurse.halt".into()];
    let l = lagebild(&umg);
    let p = punkt(&l, "Kurssperre")?;
    assert_eq!(p.ampel, depot_lage::Ampel::Halt, "{p:?}");
    assert!(p.text.contains("Sicherheitsgründen"), "{p:?}");
    Ok(())
}

/// Belegt alle Werte der Liste mit Vortag `vorher` und Tag `heute` (Euro als Text).
fn belege_preise(j: &mut Journal, t: u8, vorher: &str, heute_p: &str) -> Result<(), String> {
    let heute = tag(t)?;
    let vortag = tag(t.saturating_sub(1))?;
    let abruf = some(Zeitpunkt::frankfurt(heute, 17, 40, 0), "Abruf")?;
    for w in vorbedingung(beobachtungsliste(), "Liste")? {
        for (d, p) in [(vortag, vorher), (heute, heute_p)] {
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

/// #118: Vergleich seit Start. Einstieg am 05.11. zu 101 €: je Wert 333,33 €, abzüglich 1,50 €
/// Gebühr bleiben 331,83 €, also 3 Stück für 304,50 €; 15 Werte kosten 4.567,50 €, Bargeld
/// 432,50 €. Am 06.11. zu 102 €: 4.590,00 € + 432,50 € = 5.022,50 €, also +0,45 %. Dachs und Maus
/// waren stumm: 5.000,00 €, ohne KI-Kosten.
#[test]
fn vergleich_seit_start_mit_pruefwerten() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut jr = ort.oeffnen()?;
    belege_alle(&mut jr, 5)?;
    drop(jr);
    stummer_tag(&ort, 5)?;
    let mut jr = ort.oeffnen()?;
    belege_preise(&mut jr, 6, "101", "102")?;
    drop(jr);
    stummer_tag(&ort, 6)?;
    let jr = ort.oeffnen()?;
    assert_eq!(jr.stand().tagesschluesse().len(), 2);
    let l = lagebild(&umgebung(abends(6)?, Zugang::Offen(jr.stand())));
    let d = some(l.depot.as_ref(), "Depot")?;
    let v = some(d.vergleich.as_ref(), "Vergleich")?;
    assert_eq!((v.start, v.bewertet, v.tage), (tag(5)?, tag(6)?, 2));
    let zeile = |i: usize| some(v.zeilen.get(i), "Zeile");
    assert_eq!(zeile(0)?.wert, Some(Cent(500_000)));
    assert_eq!(zeile(0)?.rendite_bp, Some(0));
    assert_eq!(zeile(0)?.ki_kosten, Some(Cent::NULL));
    assert_eq!(zeile(0)?.nach_ki_bp, Some(0));
    assert_eq!(zeile(2)?.wert, Some(Cent(502_250)));
    assert_eq!(zeile(2)?.rendite_bp, Some(45));
    assert_eq!(zeile(3)?.wert, Some(Cent(500_000)));
    assert!(v.ohne.is_empty());
    let s = html::seite(&l);
    assert!(s.contains("Vergleich seit Start"), "{s}");
    assert!(s.contains("+0,45 %"), "{s}");
    assert!(s.contains("±0,00 %"), "{s}");
    assert!(
        s.contains("Ein Topf liegt vorn, wenn er nach KI-Kosten"),
        "{s}"
    );
    Ok(())
}

/// #118: Vor dem ersten abgeschlossenen Papiertag gibt es keinen Vergleich.
#[test]
fn vergleich_fehlt_vor_dem_ersten_papiertag() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut jr = ort.oeffnen()?;
    belege_alle(&mut jr, 5)?;
    let l = lagebild(&umgebung(abends(5)?, Zugang::Offen(jr.stand())));
    assert!(jr.stand().tagesschluesse().is_empty());
    assert_eq!(l.depot.as_ref().and_then(|d| d.vergleich.as_ref()), None);
    assert!(!html::seite(&l).contains("Vergleich seit Start"));
    Ok(())
}

/// #118: Fehlt am Bewertungstag der Kurs eines Vergleichswerts, steht dort „nicht bewertbar“.
#[test]
fn vergleich_ohne_kurs_ist_nicht_bewertbar() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut jr = ort.oeffnen()?;
    belege_alle(&mut jr, 5)?;
    drop(jr);
    stummer_tag(&ort, 5)?;
    let mut jr = ort.oeffnen()?;
    // Am 06.11. fehlt der erste Wert der Liste (SAP).
    belege_ohne(&mut jr, tag(6)?, tag(5)?, 1)?;
    drop(jr);
    stummer_tag(&ort, 6)?;
    let jr = ort.oeffnen()?;
    let l = lagebild(&umgebung(abends(6)?, Zugang::Offen(jr.stand())));
    let v = some(
        l.depot.as_ref().and_then(|d| d.vergleich.as_ref()),
        "Vergleich",
    )?;
    let z = some(v.zeilen.get(2), "Zeile")?;
    assert_eq!(z.wert, None, "{z:?}");
    let grund = some(z.grund.as_ref(), "Grund")?;
    assert!(grund.contains("SAP"), "{grund}");
    assert!(html::seite(&l).contains("nicht bewertbar"));
    Ok(())
}
