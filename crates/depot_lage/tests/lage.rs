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
fn lagebild_zeigt_neuen_monatsverbrauch_um_frankfurter_mitternacht() -> Result<(), String> {
    let ort = Ort::neu()?;
    let j = ort.oeffnen()?;
    let monat = tag(1)?;
    let vormonat = some(Datum::neu(2026, 10, 31), "Vormonat")?;
    let jetzt = some(Zeitpunkt::frankfurt(monat, 0, 30, 0), "Jetzt")?;
    let zeit = some(Zeitpunkt::frankfurt(monat, 0, 15, 0), "Anbieteraufruf")?;
    assert_eq!(jetzt.datum(), Some(vormonat));

    let mut alt = Eintrag::neu(
        some(
            Zeitpunkt::frankfurt(vormonat, 23, 55, 0),
            "Alter Anbieteraufruf",
        )?,
        Wer::Dachs,
        Ergebnis::Ok,
        None,
    );
    alt.ist = depot_lage::anbieter::Kosten::Betrag(Cent(19));
    let mut neu = Eintrag::neu(zeit, Wer::Dachs, Ergebnis::Ok, None);
    neu.ist = depot_lage::anbieter::Kosten::Betrag(Cent(23));

    let mut u = umgebung(jetzt, Zugang::Offen(j.stand()));
    u.anbieter = vec![alt, neu];
    let l = lagebild(&u);
    let budget = some(
        l.budget.iter().find(|b| b.berater == BeraterId::Dachs),
        "Dachs-Budget",
    )?;
    assert_eq!(budget.monat_verbraucht, Cent(23));
    assert_eq!(budget.monat_abgerechnet, 1);
    Ok(())
}

#[test]
fn vorbereitungshinweise_wechseln_um_frankfurter_mitternacht() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    let wissensstand = abends(7)?;
    let ziel = tag(9)?;
    vorbedingung(
        j.starte_vorbereitung(wissensstand, ziel, vec!["Belegter Quellenstand".into()]),
        "Vorbereitung",
    )?;

    let sonntag = some(Zeitpunkt::frankfurt(tag(8)?, 0, 30, 0), "Sonntag")?;
    let html = html::inhalt(
        &lagebild(&umgebung(sonntag, Zugang::Offen(j.stand()))),
        &|_| None,
    );
    assert!(html.contains("Heute ist noch kein neuer Wissensstand belegt"));

    let dienstag = some(Zeitpunkt::frankfurt(tag(10)?, 0, 30, 0), "Dienstag")?;
    let html = html::inhalt(
        &lagebild(&umgebung(dienstag, Zugang::Offen(j.stand()))),
        &|_| None,
    );
    assert!(html.contains("Das Ziel-Datum ist vergangen"));
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

fn depot_testjournal(eintraege: &[(depot_journal::Art, &str)]) -> Result<Ort, String> {
    let ort = Ort::neu()?;
    let bytes = depot_journal::inhalt_fuer(eintraege);
    let vermerk = some(depot_journal::vermerk_fuer(&bytes, 0), "Vermerk")?;
    vorbedingung(fs::write(&ort.journal, bytes), "Journal")?;
    vorbedingung(
        fs::write(ort.journal.with_extension("vermerk"), vermerk),
        "Vermerk",
    )?;
    Ok(ort)
}

fn kaufbuch(stueck: u64, kurs: u64, danach: &[(depot_journal::Art, &str)]) -> Result<Ort, String> {
    use depot_journal::Art::{Auftrag, Topf, Zustand};
    let kauf = format!("1;A;DE0007164600;Kauf;{stueck};{kurs};Umschichtung");
    let aus = format!("1;ausfuehrung;{stueck};{kurs};150");
    let mut eintraege = vec![
        (Topf, "A;500000;"),
        (Auftrag, kauf.as_str()),
        (Zustand, "1;gesendet"),
        (Zustand, "1;angenommen"),
        (Zustand, aus.as_str()),
    ];
    eintraege.extend_from_slice(danach);
    depot_testjournal(&eintraege)
}

fn depot_vom(ort: &Ort) -> Result<depot_lage::DepotLage, String> {
    let j = ort.oeffnen()?;
    Ok(depot_lage::depotstand(
        j.stand(),
        &vorbedingung(beobachtungsliste(), "Werteliste")?,
    ))
}

fn topf_a(depot: &depot_lage::DepotLage) -> Result<&depot_lage::TopfLage, String> {
    some(depot.toepfe.iter().find(|t| t.topf == TopfId::A), "Topf A")
}

#[test]
fn einstand_nur_kaeufe() -> Result<(), String> {
    let depot = depot_vom(&kaufbuch(10, 12_345, &[])?)?;
    let topf = topf_a(&depot)?;
    let einstand = some(
        some(topf.positionen.first(), "Position")?.einstand,
        "Einstand",
    )?;
    assert_eq!(einstand, Cent(123_600));
    assert_eq!(topf.bargeld.checked_add(einstand), Some(Cent(500_000)));
    Ok(())
}

#[test]
fn einstand_zwei_kaeufe() -> Result<(), String> {
    use depot_journal::Art::{Auftrag, Zustand};
    let depot = depot_vom(&kaufbuch(
        10,
        2_000,
        &[
            (Auftrag, "2;A;DE0007164600;Kauf;5;2600;Umschichtung"),
            (Zustand, "2;gesendet"),
            (Zustand, "2;angenommen"),
            (Zustand, "2;ausfuehrung;5;2600;150"),
        ],
    )?)?;
    let p = some(topf_a(&depot)?.positionen.first(), "Position")?;
    assert_eq!((p.stueck, p.einstand), (15, Some(Cent(33_300))));
    Ok(())
}

fn verkaufsbuch(menge: u64) -> Result<Ort, String> {
    use depot_journal::Art::{Auftrag, Zustand};
    kaufbuch(
        7,
        1_000,
        &[
            (
                Auftrag,
                &format!("2;A;DE0007164600;Verkauf;{menge};1000;Notbremse"),
            ),
            (Zustand, "2;gesendet"),
            (Zustand, "2;angenommen"),
            (Zustand, &format!("2;ausfuehrung;{menge};1000;150")),
        ],
    )
}

#[test]
fn verkaufsnetto_enthaelt_anteiligen_einstand_und_beide_gebuehren() -> Result<(), String> {
    use depot_journal::Art::{Auftrag, Zustand};
    let ort = kaufbuch(
        7,
        1_000,
        &[
            (Auftrag, "2;A;DE0007164600;Verkauf;2;1000;Notbremse"),
            (Zustand, "2;gesendet"),
            (Zustand, "2;angenommen"),
            (Zustand, "2;ausfuehrung;1;1200;70"),
            (Zustand, "2;ausfuehrung;1;1200;80"),
            (Auftrag, "3;A;DE0007164600;Verkauf;2;1000;Notbremse"),
            (Zustand, "3;gesendet"),
            (Zustand, "3;angenommen"),
            (Zustand, "3;ausfuehrung;1;1200;70"),
        ],
    )?;
    let journal = ort.oeffnen()?;
    let verkaeufe = depot_lage::handelsergebnis::verkaeufe(journal.stand());
    assert_eq!(verkaeufe.len(), 1, "offene Order fehlt");
    let verkauf = some(verkaeufe.first(), "Verkauf")?;
    assert_eq!(verkauf.nr, 2);
    assert_eq!(verkauf.netto, Some(Cent(208)));
    assert_eq!(verkauf.tag, None);
    Ok(())
}

#[test]
fn einstand_teilverkauf_rundet_auf() -> Result<(), String> {
    let depot = depot_vom(&verkaufsbuch(2)?)?;
    let p = some(topf_a(&depot)?.positionen.first(), "Position")?;
    assert_eq!((p.stueck, p.einstand), (5, Some(Cent(5_108))));
    Ok(())
}

#[test]
fn einstand_ganz_verkauft() -> Result<(), String> {
    assert!(
        topf_a(&depot_vom(&verkaufsbuch(7)?)?)?
            .positionen
            .is_empty()
    );
    Ok(())
}

#[test]
fn einstand_nicht_belegt() -> Result<(), String> {
    let ort = kaufbuch(
        6,
        1_000,
        &[(depot_journal::Art::Topf, "A;490000;DE0007164600:10")],
    )?;
    let depot = depot_vom(&ort)?;
    let p = some(topf_a(&depot)?.positionen.first(), "Position")?;
    assert_eq!((p.stueck, p.einstand, p.gv), (10, None, None));
    Ok(())
}

#[test]
fn gv_rundet_gegen_uns() -> Result<(), String> {
    let ort = kaufbuch(1, 10_000, &[])?;
    let mut j = ort.oeffnen()?;
    belege_preise(&mut j, 12, "100", "99.999999")?;
    let werte = vorbedingung(beobachtungsliste(), "Werteliste")?;
    let depot = depot_lage::depotstand(j.stand(), &werte);
    let p = some(topf_a(&depot)?.positionen.first(), "Position")?;
    assert_eq!(
        (p.einstand, p.wert, p.gv),
        (Some(Cent(10_150)), Some(Cent(9_999)), Some(Cent(-151)))
    );
    assert_eq!(html::prozent(some(p.gv_bp, "Rendite")?), "−1,49 %");
    Ok(())
}

#[test]
fn cash_teile_ergeben_bargeld() -> Result<(), String> {
    use depot_journal::Art::{Abrechnungstag, Auftrag, Zustand};
    let ort = kaufbuch(
        7,
        1_000,
        &[
            (Abrechnungstag, "2026-11-12"),
            (Auftrag, "2;A;DE0007164600;Verkauf;2;1000;Notbremse"),
            (Zustand, "2;gesendet"),
            (Zustand, "2;angenommen"),
            (Zustand, "2;ausfuehrung;2;1000;150"),
            (Auftrag, "3;A;DE0007164600;Kauf;2;1000;Umschichtung"),
        ],
    )?;
    let depot = depot_vom(&ort)?;
    let j = ort.oeffnen()?;
    let topf = topf_a(&depot)?;
    let frei = some(topf.frei, "Freies Bargeld")?;
    let vorgemerkt = some(topf.vorgemerkt, "Vorgemerkt")?;
    let valuta = some(topf.nicht_gutgeschrieben, "Ausstehend")?;
    assert_eq!(
        frei.checked_add(vorgemerkt)
            .and_then(|v| v.checked_add(valuta)),
        Some(topf.bargeld)
    );
    assert_eq!(
        Some(frei),
        depot_journal::konto::verfuegbar(&depot_kern::Grenzen::standard(), j.stand(), TopfId::A)
    );
    assert_eq!((vorgemerkt, valuta), (Cent(2_150), Cent(1_850)));
    assert_eq!(topf.naechste_valuta, depot_journal::papier_valuta(tag(12)?));
    Ok(())
}

fn tagesergebnis_buch(zwei_abschluesse: bool) -> Result<Ort, String> {
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    let mut topf = vorbedingung(Topf::neu(TopfId::A, Cent(500_000)), "Topf")?;
    topf.setze_bestand(
        vorbedingung(depot_kern::Isin::neu("DE0007164600"), "ISIN")?,
        1,
    );
    vorbedingung(j.setze_topf(&topf), "Topf A")?;
    vorbedingung(
        j.setze_topf(&vorbedingung(Topf::neu(TopfId::B, Cent(490_000)), "Topf")?),
        "Topf B",
    )?;
    belege_preise(&mut j, 11, "99", "100")?;
    if zwei_abschluesse {
        vorbedingung(
            j.schliesse_ab(tag(11)?, Cent(1_000_000), &"0".repeat(64)),
            "Erster Abschluss",
        )?;
    }
    belege_preise(&mut j, 12, "100", "200")?;
    vorbedingung(
        j.schliesse_ab(tag(12)?, Cent(1_010_000), &"1".repeat(64)),
        "Letzter Abschluss",
    )?;
    Ok(ort)
}

#[test]
fn tagesergebnis_morgens() -> Result<(), String> {
    let ort = tagesergebnis_buch(true)?;
    let j = ort.oeffnen()?;
    let jetzt = some(Zeitpunkt::frankfurt(tag(13)?, 8, 0, 0), "Morgen")?;
    let lage = lagebild(&umgebung(jetzt, Zugang::Offen(j.stand())));
    assert_eq!(
        some(lage.depot, "Depot")?.tagesergebnis,
        depot_lage::Tagesergebnis::Belegt {
            tag: tag(12)?,
            betrag: Cent(10_000),
            bp: Some(100),
        }
    );
    Ok(())
}

#[test]
fn tagesergebnis_ohne_vortag() -> Result<(), String> {
    let depot = depot_vom(&tagesergebnis_buch(false)?)?;
    assert_eq!(depot.tagesergebnis, depot_lage::Tagesergebnis::OhneVortag);
    Ok(())
}

#[test]
fn zustand_in_worten() {
    use depot_journal::AuftragsZustand as Z;
    for (zustand, text) in [
        (Z::Reserviert, "vorgemerkt, noch nicht gesendet"),
        (Z::Gesendet, "gesendet"),
        (Z::Unbekannt, "Versand ungeklärt, wird abgeglichen"),
        (Z::Angenommen, "liegt beim Papiermakler"),
        (Z::Teilgefuellt, "teilweise ausgeführt"),
        (Z::Gefuellt, "ausgeführt"),
        (Z::Abgelehnt, "abgelehnt"),
        (Z::Storniert, "storniert"),
        (Z::Verfallen, "verfallen"),
        (Z::Verworfen, "nicht gesendet, verworfen"),
        (Z::Alt, "ausgeführt (altes Journal)"),
    ] {
        assert_eq!(depot_lage::zustand_text(zustand), text);
        assert_ne!(format!("{zustand:?}"), text);
    }
}

#[test]
fn lagebild_zeigt_gesperrten_wert() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    let mut topf = vorbedingung(Topf::neu(TopfId::A, Cent(500_000)), "Topf")?;
    let sap = vorbedingung(depot_kern::Isin::neu("DE0007164600"), "SAP")?;
    topf.setze_bestand(sap.clone(), 1);
    vorbedingung(j.setze_topf(&topf), "Topf")?;
    let heute = tag(9)?;
    let abruf = some(Zeitpunkt::frankfurt(heute, 7, 30, 0), "Morgen")?;
    for wert in beobachtungsliste()? {
        let bezug = some(wert.boerse.bezugstag(heute), "Bezug")?;
        let vergleich = some(wert.boerse.bezugstag(bezug), "Vergleich")?;
        for datum in [bezug, vergleich] {
            if wert.isin == sap && datum == bezug {
                continue;
            }
            let kurs = vorbedingung(
                Kurs::neu(
                    Cent(20_000),
                    "EUR",
                    datum,
                    some(wert.boerse.schluss(datum), "Schluss")?,
                    Herkunft::Quelle,
                ),
                "Kurs",
            )?;
            let beleg = vorbedingung(
                KursBeleg::neu(
                    depot_kern::liste::quelle(wert.boerse),
                    abruf,
                    b"Morgenprobe",
                    wert.isin.clone(),
                    kurs,
                ),
                "Beleg",
            )?;
            vorbedingung(j.belege_kurs(&beleg), "Kurs belegen")?;
        }
        if wert.isin == sap {
            let datum = some(wert.boerse.bezugstag(vergleich), "Ersatz")?;
            let kurs = vorbedingung(
                Kurs::neu(
                    Cent(20_000),
                    "EUR",
                    datum,
                    some(wert.boerse.schluss(datum), "Schluss")?,
                    Herkunft::Quelle,
                ),
                "Ersatzkurs",
            )?;
            let beleg = vorbedingung(
                KursBeleg::neu("alphavantage", abruf, b"Morgenprobe", sap.clone(), kurs),
                "Ersatzbeleg",
            )?;
            vorbedingung(j.belege_kurs(&beleg), "Ersatz belegen")?;
        }
    }
    let l = lagebild(&umgebung(abruf, Zugang::Offen(j.stand())));
    let punkt = some(
        l.punkte.iter().find(|p| p.name == "Schlusskurse"),
        "Schlusskurse",
    )?;
    assert_eq!(punkt.ampel, Ampel::Offen);
    assert!(
        punkt.text.contains("gesperrt") && punkt.text.contains("SAP"),
        "{punkt:?}"
    );
    Ok(())
}

fn historischer_verlauf(ort: &Ort) -> Result<Vec<depot_lage::depotverlauf::Tagespunkt>, String> {
    let j = ort.oeffnen()?;
    let werte = depot_kern::liste::beobachtungsliste()?;
    Ok(depot_lage::depotverlauf::depotverlauf(j.stand(), &werte))
}

#[test]
fn verlauf_ein_punkt_je_abschluss() -> Result<(), String> {
    let ort = tagesergebnis_buch(true)?;
    {
        let mut j = ort.oeffnen()?;
        vorbedingung(j.beginne_abrechnung(tag(13)?), "Tag anfangen")?;
    }
    let punkte = historischer_verlauf(&ort)?;
    assert_eq!(
        punkte.iter().map(|p| p.tag).collect::<Vec<_>>(),
        [tag(11)?, tag(12)?]
    );
    Ok(())
}

#[test]
fn verlauf_summe() -> Result<(), String> {
    for punkt in historischer_verlauf(&tagesergebnis_buch(true)?)? {
        let cash = some(punkt.bargeld, "Bargeld")?;
        let wertpapiere = some(punkt.wertpapiere, "Wertpapiere")?;
        assert_eq!(punkt.gesamt, cash.checked_add(wertpapiere));
    }
    Ok(())
}

#[test]
fn verlauf_passt_zum_tagesschluss() -> Result<(), String> {
    let ort = tagesergebnis_buch(true)?;
    let punkte = historischer_verlauf(&ort)?;
    let j = ort.oeffnen()?;
    let werte = depot_kern::liste::beobachtungsliste()?;
    assert_eq!(
        some(punkte.first(), "Erster Punkt")?.gesamt,
        Some(Cent(1_000_000))
    );
    let letzter = some(punkte.last(), "Letzter Punkt")?;
    assert_eq!(letzter.gesamt, Some(Cent(1_010_000)));
    assert_eq!(
        letzter.gesamt,
        depot_lage::letzter_tagesschluss(j.stand(), &werte, tag(13)?)
    );
    Ok(())
}

#[test]
fn verlauf_ohne_beobachtung() -> Result<(), String> {
    let ort = tagesergebnis_buch(false)?;
    let j = ort.oeffnen()?;
    let punkte = depot_lage::depotverlauf::depotverlauf(j.stand(), &[]);
    let punkt = some(punkte.first(), "Abschluss")?;
    assert!(some(punkt.grund.as_ref(), "Grund")?.contains("nicht mehr in der Beobachtungsliste"));
    assert_eq!(punkt.gesamt, None);
    assert_eq!(punkt.bargeld, Some(Cent(990_000)));
    Ok(())
}

#[test]
fn verlauf_leer() -> Result<(), String> {
    assert!(historischer_verlauf(&Ort::neu()?)?.is_empty());
    Ok(())
}

#[test]
fn zeitraum_monat() -> Result<(), String> {
    use depot_lage::depotverlauf::Zeitraum;
    assert_eq!(
        Zeitraum::aus_abfrage("1m").ab(tag(13)?),
        Datum::neu(2026, 10, 13)
    );
    Ok(())
}

#[test]
fn zeitraum_monatsende() -> Result<(), String> {
    use depot_lage::depotverlauf::Zeitraum;
    for (art, heute, erwartet) in [
        ("1m", Datum::neu(2027, 3, 31), Datum::neu(2027, 2, 28)),
        ("1j", Datum::neu(2028, 2, 29), Datum::neu(2027, 2, 28)),
        ("3m", Datum::neu(2027, 1, 15), Datum::neu(2026, 10, 15)),
    ] {
        assert_eq!(
            Zeitraum::aus_abfrage(art).ab(some(heute, "Datum")?),
            erwartet
        );
    }
    Ok(())
}

#[test]
fn zeitraum_unbekannt() -> Result<(), String> {
    use depot_lage::depotverlauf::Zeitraum;
    for text in ["xyz", "", "alles"] {
        let zeitraum = Zeitraum::aus_abfrage(text);
        assert_eq!(zeitraum, Zeitraum::Alles);
        assert_eq!(zeitraum.ab(tag(13)?), None);
    }
    Ok(())
}

#[test]
fn im_zeitraum_einschliesslich() -> Result<(), String> {
    let punkte = historischer_verlauf(&tagesergebnis_buch(true)?)?;
    let ausschnitt = depot_lage::depotverlauf::im_zeitraum(&punkte, Some(tag(12)?));
    assert_eq!(ausschnitt.len(), 1);
    assert_eq!(some(ausschnitt.first(), "Punkt")?.tag, tag(12)?);
    Ok(())
}

#[test]
fn zeitraum_ohne_punkte() -> Result<(), String> {
    let punkte = historischer_verlauf(&tagesergebnis_buch(true)?)?;
    assert!(depot_lage::depotverlauf::im_zeitraum(&punkte, Datum::neu(2026, 12, 1)).is_empty());
    Ok(())
}

fn verlaufsbeleg(
    j: &mut Journal,
    isin: &str,
    boerse: depot_kern::Boerse,
    datum: Datum,
    abruf: Zeitpunkt,
    preis: Cent,
) -> Result<(), String> {
    let kurs = vorbedingung(
        Kurs::neu(
            preis,
            "EUR",
            datum,
            some(boerse.schluss(datum), "Schluss")?,
            Herkunft::Quelle,
        ),
        "Kurs",
    )?;
    let beleg = vorbedingung(
        KursBeleg::neu(
            depot_kern::liste::quelle(boerse),
            abruf,
            b"Verlaufsprobe",
            vorbedingung(depot_kern::Isin::neu(isin), "ISIN")?,
            kurs,
        ),
        "Beleg",
    )?;
    vorbedingung(j.belege_kurs(&beleg), "Kurs belegen")
}

#[test]
fn verlauf_luecke() -> Result<(), String> {
    use depot_kern::Boerse::Xetra;
    let ort = tagesergebnis_buch(false)?;
    let mut j = ort.oeffnen()?;
    for (nr, isin) in [(13, "DE0008404005"), (16, "DE0007164600")] {
        let datum = tag(nr)?;
        verlaufsbeleg(
            &mut j,
            isin,
            Xetra,
            datum,
            some(Zeitpunkt::frankfurt(datum, 18, 0, 0), "Abruf")?,
            Cent(20_000),
        )?;
        vorbedingung(
            j.schliesse_ab(datum, Cent(1_010_000), &"2".repeat(64)),
            "Abschluss",
        )?;
    }
    drop(j);
    let punkte = historischer_verlauf(&ort)?;
    assert!(some(punkte.first(), "Erster")?.gesamt.is_some());
    let luecke = some(punkte.get(1), "Lücke")?;
    assert_eq!(luecke.gesamt, None);
    assert_eq!(luecke.wertpapiere, None);
    assert_eq!(luecke.bargeld, Some(Cent(990_000)));
    assert!(some(luecke.grund.as_ref(), "Grund")?.contains("SAP"));
    assert!(some(punkte.last(), "Letzter")?.gesamt.is_some());
    Ok(())
}

#[test]
fn verlauf_kurstage_morgens() -> Result<(), String> {
    use depot_kern::Boerse::Xetra;
    let ort = tagesergebnis_buch(false)?;
    let mut j = ort.oeffnen()?;
    let datum = tag(13)?;
    let abruf = some(Zeitpunkt::frankfurt(datum, 7, 30, 0), "Morgen")?;
    vorbedingung(j.belege_wissensstichtag(datum, abruf), "Stichtag")?;
    vorbedingung(
        j.schliesse_ab(datum, Cent(1_010_000), &"2".repeat(64)),
        "Abschluss",
    )?;
    let spaeter = some(Zeitpunkt::frankfurt(datum, 18, 0, 0), "Später")?;
    verlaufsbeleg(&mut j, "DE0007164600", Xetra, datum, spaeter, Cent(50_000))?;
    drop(j);
    let punkte = historischer_verlauf(&ort)?;
    let punkt = some(punkte.last(), "Letzter")?;
    assert_eq!(punkt.tag, datum);
    assert_eq!(punkt.kurstage, [tag(12)?]);
    assert_eq!(punkt.gesamt, Some(Cent(1_010_000)));
    Ok(())
}

#[test]
fn verlauf_kurstage_zwei_boersen() -> Result<(), String> {
    use depot_kern::Boerse::{Nyse, Xetra};
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    let mut a = vorbedingung(Topf::neu(TopfId::A, Cent(100_000)), "Topf")?;
    for isin in ["DE0007164600", "US46625H1005"] {
        a.setze_bestand(vorbedingung(depot_kern::Isin::neu(isin), "ISIN")?, 1);
    }
    vorbedingung(j.setze_topf(&a), "Topf")?;
    vorbedingung(
        j.schliesse_ab(tag(5)?, Cent(300_000), &"0".repeat(64)),
        "Startabschluss",
    )?;
    let dez = |tag| some(Datum::neu(2026, 12, tag), "Dezember");
    let datum = dez(28)?;
    let abruf = some(Zeitpunkt::frankfurt(datum, 7, 30, 0), "Morgen")?;
    for (isin, boerse, tage) in [
        ("DE0007164600", Xetra, [22, 23]),
        ("US46625H1005", Nyse, [23, 24]),
    ] {
        for tag in tage {
            verlaufsbeleg(&mut j, isin, boerse, dez(tag)?, abruf, Cent(20_000))?;
        }
    }
    vorbedingung(j.belege_wissensstichtag(datum, abruf), "Stichtag")?;
    vorbedingung(
        j.schliesse_ab(datum, Cent(140_000), &"2".repeat(64)),
        "Abschluss",
    )?;
    drop(j);
    let punkte = historischer_verlauf(&ort)?;
    let punkt = some(punkte.last(), "Letzter")?;
    assert_eq!(punkt.kurstage, [dez(23)?, dez(24)?]);
    assert_eq!(punkt.gesamt, Some(Cent(140_000)));
    Ok(())
}

#[test]
fn ersatz_durchgehend() -> Result<(), String> {
    use depot_kern::Boerse::Xetra;
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    let sap = "DE0007164600";
    let mut a = vorbedingung(Topf::neu(TopfId::A, Cent(100_000)), "Topf")?;
    a.setze_bestand(vorbedingung(depot_kern::Isin::neu(sap), "SAP")?, 10);
    vorbedingung(j.setze_topf(&a), "Topf")?;
    vorbedingung(
        j.schliesse_ab(tag(5)?, Cent(300_000), &"0".repeat(64)),
        "Startabschluss",
    )?;
    for (nr, vorher, cent) in [(10, 9, 20_000), (11, 10, 28_000), (12, 11, 19_000)] {
        let datum = tag(nr)?;
        let abruf = some(Zeitpunkt::frankfurt(datum, 7, 30, 0), "Morgen")?;
        if nr == 10 {
            verlaufsbeleg(&mut j, sap, Xetra, tag(6)?, abruf, Cent(20_000))?;
        }
        verlaufsbeleg(&mut j, sap, Xetra, tag(vorher)?, abruf, Cent(cent))?;
        vorbedingung(j.belege_wissensstichtag(datum, abruf), "Stichtag")?;
        if nr == 11 {
            vorbedingung(
                j.begruende(
                    datum,
                    depot_sim::AUSWAHL,
                    &format!("{}{sap} · Kurssprung", depot_sim::ERSATZKURS),
                ),
                "Ersatz",
            )?;
        }
        vorbedingung(
            j.schliesse_ab(datum, Cent(300_000), &"2".repeat(64)),
            "Abschluss",
        )?;
    }
    drop(j);
    let punkte = historischer_verlauf(&ort)?;
    let ersatz = some(punkte.iter().find(|p| !p.ersatz.is_empty()), "Ersatzpunkt")?;
    assert_eq!(ersatz.gesamt, Some(Cent(300_000)));
    assert_eq!(ersatz.kurstage, [tag(9)?]);
    assert_eq!(
        ersatz.ersatz,
        [vorbedingung(depot_kern::Isin::neu(sap), "SAP")?]
    );
    assert_eq!(some(punkte.last(), "Letzter")?.gesamt, Some(Cent(290_000)));
    Ok(())
}

#[test]
#[ignore = "Laufzeit im Release-Build messen"]
fn depotverlauf_250_tage() -> Result<(), String> {
    use depot_kern::Boerse::Xetra;
    use depot_kern::Isin;
    use depot_kern::liste::Wert;

    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    let mut topf = vorbedingung(Topf::neu(TopfId::A, Cent(100_000)), "Topf")?;
    let mut werte = Vec::new();
    for nr in 0..23 {
        let isin = some(
            (0..10).find_map(|z| Isin::neu(&format!("DE{nr:09}{z}")).ok()),
            "ISIN",
        )?;
        topf.setze_bestand(isin.clone(), 1);
        werte.push(Wert {
            isin,
            boerse: Xetra,
            name: format!("Testwert {nr}"),
            kuerzel: format!("TEST{nr}.DEX"),
        });
    }
    vorbedingung(j.setze_topf(&topf), "Topf")?;
    let mut datum = some(Datum::neu(2025, 1, 2), "Start")?;
    for nr in 0..=250 {
        while Xetra.schluss(datum).is_none() {
            datum = some(datum.plus_tage(1), "Nächster Tag")?;
        }
        let abruf = some(Zeitpunkt::frankfurt(datum, 18, 0, 0), "Abruf")?;
        for wert in &werte {
            verlaufsbeleg(
                &mut j,
                wert.isin.als_str(),
                Xetra,
                datum,
                abruf,
                Cent(20_000),
            )?;
        }
        if nr != 0 {
            vorbedingung(
                j.schliesse_ab(datum, Cent(560_000), &"2".repeat(64)),
                "Abschluss",
            )?;
        }
        datum = some(datum.plus_tage(1), "Nächster Tag")?;
    }
    let start = std::time::Instant::now();
    let punkte = depot_lage::depotverlauf::depotverlauf(j.stand(), &werte);
    println!("250 Abschlüsse × 23 Werte: {:?}", start.elapsed());
    assert_eq!(punkte.len(), 250);
    assert!(punkte.iter().all(|p| p.gesamt == Some(Cent(560_000))));
    Ok(())
}

#[test]
fn verlauf_abgelehnte_kurse_sind_unbestaetigt() -> Result<(), String> {
    let ort = tagesergebnis_buch(false)?;
    let mut j = ort.oeffnen()?;
    belege_preise(&mut j, 13, "200", "300")?;
    vorbedingung(
        j.begruende(
            tag(13)?,
            "Daten",
            "Zyklus faellt aus: niemand wird gefragt, kein Auftrag",
        ),
        "Kursfehler",
    )?;
    vorbedingung(
        j.schliesse_ab(tag(13)?, Cent(1_010_000), &"3".repeat(64)),
        "Abschluss",
    )?;
    drop(j);
    let punkte = historischer_verlauf(&ort)?;
    let punkt = some(punkte.last(), "Abschluss")?;
    assert_eq!(punkt.bargeld, Some(Cent(990_000)));
    assert_eq!(punkt.gesamt, None);
    assert!(some(punkt.grund.as_ref(), "Grund")?.contains("Kursprüfung"));
    Ok(())
}
