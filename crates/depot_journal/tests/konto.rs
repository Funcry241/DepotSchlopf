//! Auftragszustaende und Kontowaechter (R37, Kap. 9 und 10, Befunde 2·A09, 1·F07, 6·N01).
//! Erfundene Daten (D12). Das Konto ist ein Skript: Es meldet genau, was der Test vorgibt.
//!
//! Jede Vorbereitung prueft ihren Erfolg ausdruecklich und endet sonst mit „Vorbedingung …“.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{
    Abweichung, Art, AuftragsZustand, Ausfuehrung, Journal, JournalFehler, Konto, KontoOrder,
    KontoStatus, Kontoauszug, Meldung, inhalt_fuer, vermerk_fuer,
};
use depot_kern::{
    BeraterId, Cent, Datum, Grenzen, Herkunft, Instrument, InstrumentArt, Isin, Kurs, Kurse, Topf,
    TopfId, Universum, Vorschlag, Zeitpunkt,
};
use depot_regelwerk::{
    Eingabe, GenehmigterAuftrag, Kennungsvergabe, Richtung, entscheide, notbremse_auftraege,
};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

fn neuer_pfad() -> Result<PathBuf, String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_konto_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
    Ok(dir.join("journal.txt"))
}

fn oeffnen(p: &Path) -> Result<Journal, String> {
    vorbedingung(Journal::oeffnen(p), "Journal oeffnen")
}

fn lesen(p: &Path) -> Result<Vec<u8>, String> {
    vorbedingung(fs::read(p), "Journal lesen")
}

fn wert() -> Result<Isin, String> {
    vorbedingung(Isin::neu("DE0007164600"), "ISIN")
}

/// Limit der Notbremse bei 100 € Kurs: 1 % darunter.
const LIMIT: i64 = 9_900;

/// Ein Topf mit `stueck` Stueck und 1.000 € Bargeld.
fn topf(id: TopfId, stueck: u64) -> Result<Topf, String> {
    let mut t = vorbedingung(Topf::neu(id, Cent(100_000)), "Topf")?;
    if stueck > 0 {
        t.setze_bestand(wert()?, stueck);
    }
    Ok(t)
}

/// Ein genehmigter Verkauf aller Stueck eines Topfes zum Kurs 100 € mit Kennung `nr`.
fn verkauf(nr: u64, stueck: u64) -> Result<GenehmigterAuftrag, String> {
    let tag = vorbedingung(Datum::neu(2026, 11, 2).ok_or(()), "Datum")?;
    let zeit = Zeitpunkt::neu(tag, 10, 0, 0).ok_or("Vorbedingung: Zeit")?;
    let kurs = vorbedingung(
        Kurs::neu(Cent(10_000), "EUR", tag, zeit, Herkunft::Simulation),
        "Kurs",
    )?;
    let kurse = vorbedingung(Kurse::neu(zeit, vec![(wert()?, kurs)]), "Kurse")?;
    let mut vergabe = Kennungsvergabe::nach(nr.checked_sub(1).ok_or("Vorbedingung: nr")?);
    let a = notbremse_auftraege(
        &Grenzen::standard(),
        &topf(TopfId::A, stueck)?,
        &kurse,
        &mut vergabe,
    )
    .ok_or("Vorbedingung: Notbremse")?;
    match a.as_slice() {
        [g] if g.auftrag().limit == Cent(LIMIT) => Ok(g.clone()),
        _ => Err("Vorbedingung: ein Auftrag zum Limit".into()),
    }
}

/// Ein eingerichtetes Journal mit Topf A (10 Stueck) und Topf B (leer).
fn mit_toepfen() -> Result<PathBuf, String> {
    let p = neuer_pfad()?;
    vorbedingung(Journal::einrichten(&p), "Einrichtung")?;
    let mut j = oeffnen(&p)?;
    vorbedingung(j.setze_topf(&topf(TopfId::A, 10)?), "Topf A")?;
    vorbedingung(j.setze_topf(&topf(TopfId::B, 0)?), "Topf B")?;
    Ok(p)
}

fn aus(stueck: u64, kurs: i64, gebuehr: i64) -> Ausfuehrung {
    Ausfuehrung {
        stueck,
        kurs: Cent(kurs),
        gebuehr: Cent(gebuehr),
        kurszeit: None,
    }
}

/// Das Konto als Skript.
#[derive(Clone, Debug, Default)]
struct Skript {
    auszug: Kontoauszug,
    orders: BTreeMap<u64, KontoOrder>,
}

impl Konto for Skript {
    fn auszug(&self) -> Kontoauszug {
        self.auszug.clone()
    }
    fn order(&self, nr: u64) -> Option<KontoOrder> {
        self.orders.get(&nr).cloned()
    }
}

impl Skript {
    /// Konto zum Startstand: 2.000 € und 10 Stueck.
    fn start() -> Result<Skript, String> {
        let mut s = Skript::default();
        s.auszug.bargeld = Cent(200_000);
        s.auszug.bestand.insert(wert()?, 10);
        Ok(s)
    }

    /// Order 1 (Verkauf 10 Stueck) mit diesem Status und diesen Ausfuehrungen. Bargeld und
    /// Bestand folgen den Ausfuehrungen; offen ist sie nur im Status `Offen`.
    fn mit_order(status: KontoStatus, ausfuehrungen: &[Ausfuehrung]) -> Result<Skript, String> {
        let mut s = Skript::start()?;
        for a in ausfuehrungen {
            s.auszug.bargeld = a
                .kurs
                .mal_stueck(a.stueck)
                .and_then(|v| s.auszug.bargeld.checked_add(v))
                .and_then(|b| b.checked_sub(a.gebuehr))
                .ok_or("Vorbedingung: Rechnung")?;
            let rest = s
                .auszug
                .bestand
                .get(&wert()?)
                .copied()
                .unwrap_or(0)
                .checked_sub(a.stueck)
                .ok_or("Vorbedingung: Bestand")?;
            if rest == 0 {
                s.auszug.bestand.remove(&wert()?);
            } else {
                s.auszug.bestand.insert(wert()?, rest);
            }
        }
        if status == KontoStatus::Offen {
            s.auszug.offene.insert(1);
        }
        s.orders.insert(
            1,
            KontoOrder {
                status,
                ausfuehrungen: ausfuehrungen.to_vec(),
                versand: None,
            },
        );
        Ok(s)
    }
}

fn zustand(j: &Journal, nr: u64) -> Option<AuftragsZustand> {
    j.stand().auftrag(nr).map(|l| l.zustand)
}

// ---------------------------------------------------------------------------------------------
// Zustandsmaschine (R37)
// ---------------------------------------------------------------------------------------------

/// Ungültige Meldung prüfen und unveränderte Bytes außerhalb der Dateisperre vergleichen.
fn pruefe_abgelehnt(
    j: Journal,
    p: &Path,
    fall: &str,
    m: &Meldung,
    offen: &mut Vec<String>,
) -> Result<Journal, String> {
    drop(j);
    let vorher = lesen(p)?;
    let mut j = oeffnen(p)?;
    match j.melde(1, m) {
        Err(JournalFehler::Abgelehnt(_)) => {}
        anders => offen.push(format!("{fall}: {anders:?}")),
    }
    drop(j);
    if lesen(p)? != vorher {
        offen.push(format!("{fall}: geschrieben"));
    }
    oeffnen(p)
}

/// Jeder Schritt ausserhalb der Zustandsmaschine wird abgelehnt und nichts geschrieben: ein
/// zweiter Versand, eine Ausfuehrung vor der Annahme, ausserhalb des Limits oder ueber die
/// Menge, ein Abschluss mit offenem Auftrag. Dieselben Eintraege in der Datei sind Halt.
#[test]
fn zustandsmaschine_laesst_nur_erlaubte_schritte_zu() -> Result<(), String> {
    let p = mit_toepfen()?;
    let mut j = oeffnen(&p)?;
    let grenzen = Grenzen::standard();
    vorbedingung(
        j.reserviere_auftrag(&grenzen, TopfId::A, &verkauf(1, 10)?),
        "reservieren",
    )?;
    let tag = vorbedingung(Datum::neu(2026, 11, 2).ok_or(()), "Datum")?;
    let abschluss = "0".repeat(64);
    let mut offen = Vec::new();
    let mut pruefe =
        |j: Journal, fall: &str, m: Meldung| pruefe_abgelehnt(j, &p, fall, &m, &mut offen);
    j = pruefe(j, "angenommen vor dem Versand", Meldung::Angenommen)?;
    j = pruefe(
        j,
        "Ausfuehrung vor dem Versand",
        Meldung::Ausfuehrung(aus(1, LIMIT, 0)),
    )?;
    vorbedingung(j.melde(1, &Meldung::Gesendet), "gesendet")?;
    j = pruefe(j, "zweiter Versand", Meldung::Gesendet)?;
    j = pruefe(j, "verworfen nach dem Versand", Meldung::Verworfen)?;
    j = pruefe(
        j,
        "Ausfuehrung vor der Annahme",
        Meldung::Ausfuehrung(aus(1, LIMIT, 0)),
    )?;
    vorbedingung(j.melde(1, &Meldung::Unbekannt), "unbekannt")?;
    j = pruefe(j, "Versand nach unbekannt", Meldung::Gesendet)?;
    vorbedingung(j.melde(1, &Meldung::Angenommen), "angenommen")?;
    j = pruefe(j, "unter dem Limit", Meldung::Ausfuehrung(aus(1, 9_899, 0)))?;
    j = pruefe(
        j,
        "mehr als beauftragt",
        Meldung::Ausfuehrung(aus(11, LIMIT, 0)),
    )?;
    j = pruefe(j, "null Stueck", Meldung::Ausfuehrung(aus(0, LIMIT, 0)))?;
    match j.schliesse_ab(tag, Cent(1), &abschluss) {
        Err(JournalFehler::Abgelehnt(m)) if m.contains("offenem Auftrag") => {}
        anders => offen.push(format!("Abschluss mit offenem Auftrag: {anders:?}")),
    }
    vorbedingung(
        j.melde(1, &Meldung::Ausfuehrung(aus(10, LIMIT, 150))),
        "gefuellt",
    )?;
    if zustand(&j, 1) != Some(AuftragsZustand::Gefuellt) {
        offen.push(format!("nach voller Ausfuehrung: {:?}", zustand(&j, 1)));
    }
    let mut pruefe2 = |j: &mut Journal, fall: &str, m: Meldung| match j.melde(1, &m) {
        Err(JournalFehler::Abgelehnt(_)) => {}
        anders => offen.push(format!("{fall}: {anders:?}")),
    };
    pruefe2(&mut j, "Storno nach Endzustand", Meldung::Storniert);
    pruefe2(
        &mut j,
        "Ausfuehrung nach Endzustand",
        Meldung::Ausfuehrung(aus(1, LIMIT, 0)),
    );
    if j.schliesse_ab(tag, Cent(1), &abschluss).is_err() {
        offen.push("Abschluss ohne offenen Auftrag abgelehnt".into());
    }
    // Ueber `anhaengen` entsteht weder ein Auftrag noch ein Zustand.
    for (art, inhalt) in [
        (Art::Auftrag, "2;A;DE0007164600;Verkauf;5;9900;Notbremse"),
        (Art::Zustand, "1;storniert"),
    ] {
        if !matches!(j.anhaengen(art, inhalt), Err(JournalFehler::Abgelehnt(_))) {
            offen.push(format!("anhaengen {art:?} angenommen"));
        }
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

/// Ein zweiter Versand in der Datei ist Halt: Leser und Schreiber nehmen denselben
/// Zustandsraum an (R20).
#[test]
fn zweiter_versand_in_der_datei_ist_halt() -> Result<(), String> {
    let mut offen = Vec::new();
    let datei = neuer_pfad()?;
    let inhalt = inhalt_fuer(&[
        (Art::Topf, "A;100000;DE0007164600:10"),
        (Art::Auftrag, "1;A;DE0007164600;Verkauf;10;9900;Notbremse"),
        (Art::Zustand, "1;gesendet"),
        (Art::Zustand, "1;gesendet"),
    ]);
    vorbedingung(fs::write(&datei, &inhalt), "Datei")?;
    let v = vermerk_fuer(&inhalt, 0).ok_or("Vorbedingung: Vermerk")?;
    vorbedingung(fs::write(datei.with_extension("vermerk"), v), "Vermerk")?;
    match Journal::oeffnen(&datei) {
        Err(JournalFehler::Halt(m)) if m.contains("gesendet") => {}
        anders => offen.push(format!("zweiter Versand in der Datei: {anders:?}")),
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

// ---------------------------------------------------------------------------------------------
// Deckung (R37, Kap. 9; 2·A09 derselbe Wert in zwei Bereichen)
// ---------------------------------------------------------------------------------------------

/// Stueck in einem offenen Verkauf sind gebunden: Ein zweiter Verkauf derselben Stueck wird
/// nicht reserviert. Die Stueck eines anderen Topfes zaehlen nicht, auch wenn das Konto sie
/// fuehrt. Nach dem Endzustand ist die Reservierung frei.
#[test]
fn befund_2_a09_reservierung_bindet_stueck_je_topf() -> Result<(), String> {
    let p = neuer_pfad()?;
    vorbedingung(Journal::einrichten(&p), "Einrichtung")?;
    let mut j = oeffnen(&p)?;
    vorbedingung(j.setze_topf(&topf(TopfId::A, 10)?), "Topf A")?;
    vorbedingung(j.setze_topf(&topf(TopfId::B, 10)?), "Topf B")?;
    let grenzen = Grenzen::standard();
    let mut offen = Vec::new();
    vorbedingung(
        j.reserviere_auftrag(&grenzen, TopfId::A, &verkauf(1, 10)?),
        "erster",
    )?;
    match j.reserviere_auftrag(&grenzen, TopfId::A, &verkauf(2, 10)?) {
        Err(JournalFehler::Abgelehnt(m)) if m.contains("Deckung") => {}
        anders => offen.push(format!("zweiter Verkauf derselben Stueck: {anders:?}")),
    }
    // Topf B haelt dieselben Stueck noch einmal; Topf A darf sie nicht verkaufen.
    vorbedingung(
        j.reserviere_auftrag(&grenzen, TopfId::B, &verkauf(2, 10)?),
        "Topf B",
    )?;
    match j.reserviere_auftrag(&grenzen, TopfId::B, &verkauf(3, 10)?) {
        Err(JournalFehler::Abgelehnt(m)) if m.contains("Deckung") => {}
        anders => offen.push(format!("Topf B doppelt: {anders:?}")),
    }
    // Ein Topf ohne Stueck darf nicht verkaufen, auch wenn das Konto sie fuehrt.
    vorbedingung(j.melde(1, &Meldung::Verworfen), "verworfen")?;
    vorbedingung(j.setze_topf(&topf(TopfId::A, 0)?), "Topf A leer")?;
    match j.reserviere_auftrag(&grenzen, TopfId::A, &verkauf(4, 10)?) {
        Err(JournalFehler::Abgelehnt(m)) if m.contains("Deckung") => {}
        anders => offen.push(format!("Verkauf ohne Bestand im Topf: {anders:?}")),
    }
    // Ein Topf mit offenem Verkauf laesst sich nicht unter die gebundenen Stueck setzen.
    match j.setze_topf(&topf(TopfId::B, 5)?) {
        Err(JournalFehler::Abgelehnt(m)) if m.contains("Deckung") => {}
        anders => offen.push(format!("Topf unter gebundenen Stueck: {anders:?}")),
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

/// Ein genehmigter Kauf aus dem Regelwerk: Dachs will 25 % des Werts in Topf A mit 10.000 €.
fn kauf() -> Result<GenehmigterAuftrag, String> {
    let w = wert()?;
    let universum = Universum::neu(vec![Instrument {
        isin: w.clone(),
        name: "erfundener Wert".into(),
        art: InstrumentArt::Aktie,
    }]);
    let tag = vorbedingung(Datum::neu(2026, 11, 2).ok_or(()), "Datum")?;
    let zeit = Zeitpunkt::neu(tag, 10, 0, 0).ok_or("Vorbedingung: Zeit")?;
    let kurs = vorbedingung(
        Kurs::neu(Cent(10_000), "EUR", tag, zeit, Herkunft::Simulation),
        "Kurs",
    )?;
    let kurse = vorbedingung(Kurse::neu(zeit, vec![(w.clone(), kurs)]), "Kurse")?;
    let topf = vorbedingung(Topf::neu(TopfId::A, Cent(1_000_000)), "Topf")?;
    let vorschlag = Vorschlag {
        limit_bp: std::collections::BTreeMap::new(),
        berater: BeraterId::Dachs,
        zielgewichte_bp: [(w.clone(), 2_500)].into_iter().collect(),
        begruendung: [(w, "erfunden".to_string())].into_iter().collect(),
    };
    let grenzen = Grenzen::standard();
    let ent = entscheide(
        &Eingabe {
            grenzen: &grenzen,
            universum: &universum,
            topf: &topf,
            kurse: &kurse,
            vorschlag: Some(&vorschlag),
            halt: false,
        },
        &mut Kennungsvergabe::nach(0),
    );
    match ent.kaeufe.as_slice() {
        [k] if k.auftrag().richtung == Richtung::Kauf && ent.verkaeufe.is_empty() => Ok(k.clone()),
        _ => Err(format!("Vorbedingung: ein Kauf: {ent:?}")),
    }
}

/// Der Kontowaechter reserviert einen Kauf nur mit Limit mal Menge plus hoechstmoeglicher
/// Gebuehr. Reicht das Bargeld nur fuer Limit mal Menge, wird nicht reserviert; mit der Gebuehr
/// dazu schon. Danach ist der Betrag samt Gebuehr verplant.
#[test]
fn kauf_wird_nur_mit_gebuehr_reserviert() -> Result<(), String> {
    let grenzen = Grenzen::standard();
    let genehmigt = kauf()?;
    let beauftragt = genehmigt.auftrag();
    let volumen = beauftragt
        .limit
        .mal_stueck(beauftragt.stueck)
        .ok_or("Vorbedingung: Volumen")?;
    let gebuehr = grenzen
        .gebuehren()
        .fuer(volumen)
        .ok_or("Vorbedingung: Gebuehr")?;
    let mit = volumen.checked_add(gebuehr).ok_or("Vorbedingung: Summe")?;
    let mut offen = Vec::new();
    for (fall, bargeld, geht) in [("ohne Gebuehr", volumen, false), ("mit Gebuehr", mit, true)] {
        let p = neuer_pfad()?;
        vorbedingung(Journal::einrichten(&p), "Einrichtung")?;
        let mut j = oeffnen(&p)?;
        vorbedingung(
            j.setze_topf(&vorbedingung(Topf::neu(TopfId::A, bargeld), "Topf")?),
            "Topf A",
        )?;
        let ergebnis = j.reserviere_auftrag(&grenzen, TopfId::A, &genehmigt);
        match (&ergebnis, geht) {
            (Ok(()), true) => {
                if depot_journal::verfuegbar(&grenzen, j.stand(), TopfId::A) != Some(Cent::NULL) {
                    offen.push(format!("{fall}: Gebuehr nicht verplant"));
                }
            }
            (Err(JournalFehler::Abgelehnt(m)), false) if m.contains("keine Deckung") => {}
            _ => offen.push(format!("{fall}: {ergebnis:?}")),
        }
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

// ---------------------------------------------------------------------------------------------
// Absturz direkt nach dem Senden (Maus, Vorgabe fuer Schritt 1)
// ---------------------------------------------------------------------------------------------

/// Der Auftrag ist reserviert und als gesendet belegt, dann bricht der Lauf ab. Beim Neustart
/// gleicht der Kontowaechter zuerst ab und belegt, was das Konto meldet: offen, teilweise
/// ausgefuehrt, ganz ausgefuehrt oder abgelehnt. Ein zweiter Abgleich bucht nichts doppelt.
/// Kennt das Konto den Auftrag nicht, bleibt er offen, und es ist eine Abweichung: Halt.
#[test]
fn absturz_direkt_nach_dem_senden_wird_nur_abgeglichen() -> Result<(), String> {
    type Fall = (&'static str, KontoStatus, Vec<Ausfuehrung>, AuftragsZustand);
    let faelle: [Fall; 4] = [
        (
            "offen",
            KontoStatus::Offen,
            vec![],
            AuftragsZustand::Angenommen,
        ),
        (
            "teilweise",
            KontoStatus::Offen,
            vec![aus(4, 10_000, 150)],
            AuftragsZustand::Teilgefuellt,
        ),
        (
            "ganz",
            KontoStatus::Gefuellt,
            vec![aus(4, 10_000, 150), aus(6, 9_950, 150)],
            AuftragsZustand::Gefuellt,
        ),
        (
            "abgelehnt",
            KontoStatus::Abgelehnt,
            vec![],
            AuftragsZustand::Abgelehnt,
        ),
    ];
    let mut offen = Vec::new();
    for (fall, status, ausfuehrungen, erwartet) in faelle {
        let p = mit_toepfen()?;
        {
            let mut j = oeffnen(&p)?;
            let grenzen = Grenzen::standard();
            vorbedingung(
                j.reserviere_auftrag(&grenzen, TopfId::A, &verkauf(1, 10)?),
                "reservieren",
            )?;
            vorbedingung(j.melde(1, &Meldung::Gesendet), "gesendet")?;
            // Absturz: Die Antwort des Kontos kommt nicht mehr an.
        }
        let konto = Skript::mit_order(status, &ausfuehrungen)?;
        let mut j = oeffnen(&p)?;
        let abw = vorbedingung(j.gleiche_ab(&konto), "Abgleich")?;
        if !abw.is_empty() {
            offen.push(format!("{fall}: Abweichungen {abw:?}"));
        }
        if zustand(&j, 1) != Some(erwartet) {
            offen.push(format!("{fall}: Zustand {:?}", zustand(&j, 1)));
        }
        let gebucht = j.stand().auftrag(1).map(|l| l.ausfuehrungen.clone());
        if gebucht.as_ref() != Some(&ausfuehrungen) {
            offen.push(format!("{fall}: Ausfuehrungen {gebucht:?}"));
        }
        // Zweiter Neustart, derselbe Abgleich: nichts Neues im Journal.
        drop(j);
        let vorher = lesen(&p)?;
        let mut j = oeffnen(&p)?;
        let abw = vorbedingung(j.gleiche_ab(&konto), "zweiter Abgleich")?;
        drop(j);
        if !abw.is_empty() || lesen(&p)? != vorher {
            offen.push(format!(
                "{fall}: zweiter Abgleich schreibt oder weicht ab: {abw:?}"
            ));
        }
    }

    // Das Konto kennt den Auftrag nicht: unklar. Er bleibt gesendet, nichts wird geschrieben,
    // und die Abweichung haelt an.
    let p = mit_toepfen()?;
    {
        let mut j = oeffnen(&p)?;
        let grenzen = Grenzen::standard();
        vorbedingung(
            j.reserviere_auftrag(&grenzen, TopfId::A, &verkauf(1, 10)?),
            "reservieren",
        )?;
        vorbedingung(j.melde(1, &Meldung::Gesendet), "gesendet")?;
    }
    let vorher = lesen(&p)?;
    let mut j = oeffnen(&p)?;
    let abw = vorbedingung(j.gleiche_ab(&Skript::start()?), "Abgleich")?;
    drop(j);
    if abw != vec![Abweichung::OrderFehlt(1), Abweichung::OrderNichtImAuszug(1)] {
        offen.push(format!("unbekannt beim Konto: {abw:?}"));
    }
    if lesen(&p)? != vorher {
        offen.push("unbekannt beim Konto: geschrieben".into());
    }
    if zustand(&oeffnen(&p)?, 1) != Some(AuftragsZustand::Gesendet) {
        offen.push("unbekannt beim Konto: Zustand geaendert".into());
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

/// Absturz vor dem Versand: Reserviert, aber nie als gesendet belegt. Kennt das Konto den
/// Auftrag nicht, wird er verworfen und die Reservierung frei. Kennt es ihn doch, ist das eine
/// Abweichung, denn gesendet wird erst nach dem Beleg.
#[test]
fn absturz_vor_dem_senden_verwirft_nur_was_nie_gesendet_wurde() -> Result<(), String> {
    let mut offen = Vec::new();
    for (fall, konto, erwartet, abweichung) in [
        (
            "Konto kennt ihn nicht",
            Skript::start()?,
            AuftragsZustand::Verworfen,
            vec![],
        ),
        (
            "Konto kennt ihn",
            Skript::mit_order(KontoStatus::Offen, &[])?,
            AuftragsZustand::Reserviert,
            vec![
                Abweichung::OrderOhneVersand(1),
                Abweichung::OrderNurBeimKonto(1),
            ],
        ),
    ] {
        let p = mit_toepfen()?;
        {
            let mut j = oeffnen(&p)?;
            let grenzen = Grenzen::standard();
            vorbedingung(
                j.reserviere_auftrag(&grenzen, TopfId::A, &verkauf(1, 10)?),
                "reservieren",
            )?;
        }
        let mut j = oeffnen(&p)?;
        let abw = vorbedingung(j.gleiche_ab(&konto), "Abgleich")?;
        if abw != abweichung {
            offen.push(format!("{fall}: {abw:?}"));
        }
        if zustand(&j, 1) != Some(erwartet) {
            offen.push(format!("{fall}: Zustand {:?}", zustand(&j, 1)));
        }
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

// ---------------------------------------------------------------------------------------------
// Teilausfuehrung mit Neustart (Maus, Vorgabe fuer Schritt 1; 2·A09, 1·F07)
// ---------------------------------------------------------------------------------------------

/// Von 10 Stueck sind 4 verkauft. Nach dem Neustart steht genau das im Journal: Zustand
/// teilgefuellt, 4 Stueck gebucht, 6 gebunden, der Tag nicht abschliessbar. Meldet das Konto
/// danach den Rest, wird nur der Rest gebucht. Weicht eine schon gebuchte Ausfuehrung ab,
/// ist das eine Abweichung, und nichts wird gebucht.
#[test]
fn befund_2_a09_teilausfuehrung_mit_neustart() -> Result<(), String> {
    let p = mit_toepfen()?;
    let mut offen = Vec::new();
    {
        let mut j = oeffnen(&p)?;
        let grenzen = Grenzen::standard();
        vorbedingung(
            j.reserviere_auftrag(&grenzen, TopfId::A, &verkauf(1, 10)?),
            "reservieren",
        )?;
        vorbedingung(j.melde(1, &Meldung::Gesendet), "gesendet")?;
        vorbedingung(j.melde(1, &Meldung::Angenommen), "angenommen")?;
        let teil = Skript::mit_order(KontoStatus::Offen, &[aus(4, 10_000, 150)])?;
        let abw = vorbedingung(j.gleiche_ab(&teil), "Abgleich Teil")?;
        if !abw.is_empty() {
            offen.push(format!("Teil: {abw:?}"));
        }
    }
    let mut j = oeffnen(&p)?;
    let a = j.stand().toepfe().get(&TopfId::A).cloned();
    let lage = j.stand().auftrag(1).cloned();
    match (&a, &lage) {
        (Some(a), Some(l))
            if l.zustand == AuftragsZustand::Teilgefuellt
                && l.gefuellt() == 4
                && l.offen_stueck() == 6
                && a.stueck(&wert()?) == 6
                && a.bargeld() == Cent(139_850) => {}
        _ => offen.push(format!("nach dem Neustart: {a:?} {lage:?}")),
    }
    // Die 6 offenen Stueck sind gebunden.
    if j.reserviere_auftrag(&Grenzen::standard(), TopfId::A, &verkauf(2, 6)?)
        .is_ok()
    {
        offen.push("gebundene Stueck noch einmal verkauft".into());
    }
    let tag = vorbedingung(Datum::neu(2026, 11, 2).ok_or(()), "Datum")?;
    if j.schliesse_ab(tag, Cent(1), &"0".repeat(64)).is_ok() {
        offen.push("Abschluss trotz Teilausfuehrung".into());
    }
    // Eine schon gebuchte Ausfuehrung meldet das Konto anders: Abweichung, nichts gebucht.
    drop(j);
    let vorher = lesen(&p)?;
    let mut j = oeffnen(&p)?;
    let anders = Skript::mit_order(
        KontoStatus::Gefuellt,
        &[aus(5, 10_000, 150), aus(5, 9_950, 150)],
    )?;
    let abw = vorbedingung(j.gleiche_ab(&anders), "Abgleich anders")?;
    drop(j);
    if !abw
        .iter()
        .any(|a| matches!(a, Abweichung::Unbuchbar { nr: 1, .. }))
        || lesen(&p)? != vorher
    {
        offen.push(format!("abweichende Ausfuehrung: {abw:?}"));
    }
    let mut j = oeffnen(&p)?;
    // Der Rest kommt: nur er wird gebucht.
    let ganz = Skript::mit_order(
        KontoStatus::Gefuellt,
        &[aus(4, 10_000, 150), aus(6, 9_950, 150)],
    )?;
    let abw = vorbedingung(j.gleiche_ab(&ganz), "Abgleich Rest")?;
    let lage = j.stand().auftrag(1).cloned();
    if !abw.is_empty()
        || lage.as_ref().map(|l| (l.zustand, l.ausfuehrungen.len()))
            != Some((AuftragsZustand::Gefuellt, 2))
    {
        offen.push(format!("Rest: {abw:?} {lage:?}"));
    }
    if j.schliesse_ab(tag, Cent(1), &"0".repeat(64)).is_err() {
        offen.push("Abschluss nach dem Rest abgelehnt".into());
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

// ---------------------------------------------------------------------------------------------
// Halt bei widerspruechlichen Bestaenden (Maus, Vorgabe fuer Schritt 1; 6·N01)
// ---------------------------------------------------------------------------------------------

/// Jede Unstimmigkeit zwischen Summe der Toepfe und Konto ist eine Abweichung: anderer
/// Bestand, Saldo unter null, Saldo in fremder Waehrung, unbekannte Belastung, offene Order,
/// die das Journal nicht kennt. Der Abgleich schreibt dabei nichts und gleicht nichts aus.
#[test]
fn befund_6_n01_widerspruechliche_bestaende_sind_abweichungen() -> Result<(), String> {
    type Aenderung = fn(&mut Kontoauszug) -> Result<(), String>;
    let faelle: [(&str, Aenderung); 6] = [
        ("Bestand anders", |a| {
            a.bestand.insert(wert()?, 9);
            Ok(())
        }),
        ("Saldo -0,01 €", |a| {
            a.bargeld = Cent(-1);
            Ok(())
        }),
        ("Saldo in US-Dollar", |a| {
            a.fremde_waehrungen.insert("USD".into());
            Ok(())
        }),
        ("unbekannte Belastung", |a| {
            a.bargeld = a.bargeld.checked_sub(Cent(500)).ok_or("Rechnung")?;
            Ok(())
        }),
        ("fremde offene Order", |a| {
            a.offene.insert(99);
            Ok(())
        }),
        ("Wert, den kein Topf haelt", |a| {
            a.bestand
                .insert(vorbedingung(Isin::neu("DE0008404005"), "ISIN")?, 1);
            Ok(())
        }),
    ];
    let mut offen = Vec::new();
    let p = mit_toepfen()?;
    let mut j = oeffnen(&p)?;
    let stimmt = vorbedingung(j.gleiche_ab(&Skript::start()?), "Kontrolle")?;
    if !stimmt.is_empty() {
        return Err(format!("Vorbedingung: Startstand stimmt nicht: {stimmt:?}"));
    }
    drop(j);
    let vorher = lesen(&p)?;
    let mut j = oeffnen(&p)?;
    for (fall, aendere) in faelle {
        let mut konto = Skript::start()?;
        aendere(&mut konto.auszug)?;
        let abw = vorbedingung(j.gleiche_ab(&konto), fall)?;
        if abw.is_empty() {
            offen.push(format!("{fall}: keine Abweichung"));
        }
    }
    drop(j);
    if lesen(&p)? != vorher {
        offen.push("Abgleich hat geschrieben".into());
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

/// K16-01 (Maus, PR #16): Die Einzelabfrage meldet einen Auftrag als offen, im Kontoauszug
/// fehlt er. Das Konto widerspricht sich selbst; der Abgleich muss das melden, statt den
/// Auftrag still als angenommen zu fuehren. Ebenso umgekehrt: Der Auszug fuehrt ihn offen,
/// die Einzelabfrage meldet ein Ende.
#[test]
fn befund_k16_01_offene_auftraege_in_beide_richtungen() -> Result<(), String> {
    let mut offen = Vec::new();
    for (fall, status, im_auszug) in [
        (
            "offen laut Abfrage, fehlt im Auszug",
            KontoStatus::Offen,
            false,
        ),
        (
            "im Auszug offen, laut Abfrage storniert",
            KontoStatus::Storniert,
            true,
        ),
    ] {
        let p = mit_toepfen()?;
        let mut j = oeffnen(&p)?;
        let grenzen = Grenzen::standard();
        vorbedingung(
            j.reserviere_auftrag(&grenzen, TopfId::A, &verkauf(1, 10)?),
            "reservieren",
        )?;
        vorbedingung(j.melde(1, &Meldung::Gesendet), "gesendet")?;
        vorbedingung(j.melde(1, &Meldung::Angenommen), "angenommen")?;
        let mut konto = Skript::mit_order(status, &[])?;
        if im_auszug {
            konto.auszug.offene.insert(1);
        } else {
            konto.auszug.offene.remove(&1);
        }
        let abw = vorbedingung(j.gleiche_ab(&konto), "Abgleich")?;
        if abw.is_empty() {
            offen.push(format!(
                "{fall}: keine Abweichung, Zustand {:?}",
                zustand(&j, 1)
            ));
        }
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

#[test]
fn papierabrechnung_verlangt_einen_nachweis_auch_ohne_verkaeufe() -> Result<(), String> {
    let p = mit_toepfen()?;
    let mut j = oeffnen(&p)?;
    let tag = Datum::neu(2026, 11, 5).ok_or("Vorbedingung: Datum")?;
    vorbedingung(j.beginne_abrechnung(tag), "Abrechnungstag")?;
    let mut konto = Skript::start()?;
    assert_eq!(
        vorbedingung(j.gleiche_ab(&konto), "Abgleich ohne Nachweis")?,
        vec![Abweichung::AbrechnungsnachweisFehlt]
    );
    konto.auszug.abrechnungen = Some(BTreeMap::new());
    assert!(vorbedingung(j.gleiche_ab(&konto), "Abgleich mit Nachweis")?.is_empty());
    Ok(())
}
