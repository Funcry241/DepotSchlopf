//! Kontowaechter und Auftragszustaende im simulierten Lauf, mit erfundenen Antworten des
//! Papierkontos (D12). Maus' Vorgaben fuer Schritt 1: normaler Ablauf, Absturz direkt nach
//! dem Senden, Halt bei widerspruechlichen Bestaenden; die Teilausfuehrung mit Neustart prueft
//! `depot_journal/tests/konto.rs` mit einem Konto, das Teilausfuehrungen meldet.
//!
//! Jede Vorbereitung prueft ihren Erfolg ausdruecklich und endet sonst mit „Vorbedingung …“.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{AuftragsZustand, Journal, Konto, KontoOrder, Kontoauszug, Meldung};
use depot_kern::{Cent, Datum, Grenzen, Kurse, TopfId};
use depot_regelwerk::{GenehmigterAuftrag, notbremse_auftraege};
use depot_sim::{Broker, Drehbuch, LaufFehler, Papierkonto, Sendeantwort, Szenario, lauf};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn vorbedingung<T, E: std::fmt::Debug>(erg: Result<T, E>, was: &str) -> Result<T, String> {
    erg.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

struct Ort {
    dir: PathBuf,
    journal: PathBuf,
}

impl Ort {
    fn neu() -> Result<Ort, String> {
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("depot_sim_konto_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
        let journal = dir.join("journal.txt");
        vorbedingung(Journal::einrichten(&journal), "Einrichtung")?;
        Ok(Ort { dir, journal })
    }

    fn oeffnen(&self) -> Result<Journal, String> {
        vorbedingung(Journal::oeffnen(&self.journal), "Journal oeffnen")
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        vorbedingung(fs::read(&self.journal), "Journal lesen")
    }
}

impl Drop for Ort {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn beispiel() -> Result<(Szenario, Drehbuch, Drehbuch), String> {
    Szenario::beispiel().ok_or_else(|| "Vorbedingung: Beispiel ungueltig".to_string())
}

fn konto(s: &Szenario) -> Result<Papierkonto, String> {
    Papierkonto::eroeffne(&s.start).ok_or_else(|| "Vorbedingung: Papierkonto".to_string())
}

fn von(j: &Journal, wer: &str) -> Vec<String> {
    j.stand()
        .begruendungen()
        .iter()
        .filter(|(_, w, _)| w == wer)
        .map(|(_, _, t)| t.clone())
        .collect()
}

/// Ein Konto, das Antworten verliert oder fremde Buchungen bekommt.
#[derive(Debug)]
struct Gestoert {
    innen: Papierkonto,
    /// Der Auftrag kommt an, die Antwort nicht.
    antwort_verloren: bool,
    /// Der Auftrag kommt gar nicht an, und es gibt keine Antwort.
    auftrag_verloren: bool,
    /// Fremde Buchung beim ersten Markt.
    fremd: Option<Cent>,
}

impl Gestoert {
    fn neu(innen: Papierkonto) -> Gestoert {
        Gestoert {
            innen,
            antwort_verloren: false,
            auftrag_verloren: false,
            fremd: None,
        }
    }
}

impl Konto for Gestoert {
    fn auszug(&self) -> Kontoauszug {
        self.innen.auszug()
    }
    fn order(&self, nr: u64) -> Option<KontoOrder> {
        self.innen.order(nr)
    }
}

impl Broker for Gestoert {
    fn sende(&mut self, a: &GenehmigterAuftrag) -> Sendeantwort {
        if self.auftrag_verloren {
            return Sendeantwort::KeineAntwort;
        }
        let antwort = self.innen.sende(a);
        if self.antwort_verloren {
            Sendeantwort::KeineAntwort
        } else {
            antwort
        }
    }
    fn rechne_ab(&mut self, tag: Datum) {
        self.innen.rechne_ab(tag);
    }

    fn markt(&mut self, kurse: &Kurse) {
        self.innen.markt(kurse);
        if let Some(b) = self.fremd.take() {
            let _ = self.innen.buche_fremd(b);
        }
    }
}

/// Wie viele Auftraege das Journal als gesendet belegt hat.
fn gesendet(j: &Journal) -> usize {
    j.stand()
        .auftraege()
        .iter()
        .filter(|l| {
            !matches!(
                l.zustand,
                AuftragsZustand::Reserviert | AuftragsZustand::Verworfen
            )
        })
        .count()
}

/// Normaler Ablauf: Jeder Auftrag ist genau einmal gesendet und hat einen Endzustand laut
/// Konto; jede Ausfuehrung ist einmal gebucht; Journal und Konto stimmen ueberein, und der
/// Kontowaechter hat nichts zu melden.
#[test]
fn normaler_ablauf_stimmt_mit_dem_konto_ueberein() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (s, mut dachs, mut maus) = beispiel()?;
    let mut pk = konto(&s)?;
    let mut j = ort.oeffnen()?;
    vorbedingung(lauf(&mut j, &s, &mut dachs, &mut maus, &mut pk), "Lauf")?;
    let st = j.stand();
    if st.auftraege().is_empty() {
        return Err("Vorbedingung: keine Auftraege".into());
    }
    let mut offen = Vec::new();
    if gesendet(&j) != pk.sendungen() || pk.sendungen() != st.auftraege().len() {
        offen.push(format!(
            "gesendet laut Journal {}, Sendungen {}, Auftraege {}",
            gesendet(&j),
            pk.sendungen(),
            st.auftraege().len()
        ));
    }
    for l in st.auftraege() {
        let o = pk.order(l.beleg.nr);
        let erwartet = matches!(
            l.zustand,
            AuftragsZustand::Gefuellt | AuftragsZustand::Verfallen
        );
        if !erwartet || o.map(|o| o.ausfuehrungen) != Some(l.ausfuehrungen.clone()) {
            offen.push(format!("Auftrag {}: {:?}", l.beleg.nr, l.zustand));
        }
    }
    if !von(&j, "Kontowaechter").is_empty() {
        offen.push(format!("Kontowaechter: {:?}", von(&j, "Kontowaechter")));
    }
    let abw = vorbedingung(j.gleiche_ab(&pk), "Abgleich")?;
    if !abw.is_empty() {
        offen.push(format!("Abweichungen: {abw:?}"));
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

/// Bleibt die Antwort aus, gilt der Sendestatus als unbekannt. Der Lauf sendet nicht noch
/// einmal; der Abgleich klaert den Auftrag, und der Lauf geht weiter.
#[test]
fn befund_2_a09_unbekannter_sendestatus_sendet_nicht_noch_einmal() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (s, mut dachs, mut maus) = beispiel()?;
    let mut pk = Gestoert::neu(konto(&s)?);
    pk.antwort_verloren = true;
    let mut j = ort.oeffnen()?;
    vorbedingung(lauf(&mut j, &s, &mut dachs, &mut maus, &mut pk), "Lauf")?;
    let mut offen = Vec::new();
    if pk.innen.sendungen() != j.stand().auftraege().len() {
        offen.push(format!(
            "{} Sendungen fuer {} Auftraege",
            pk.innen.sendungen(),
            j.stand().auftraege().len()
        ));
    }
    let unbekannt = von(&j, "Auftragsstelle");
    if unbekannt.len() != j.stand().auftraege().len()
        || !unbekannt
            .iter()
            .all(|t| t.contains("Sendestatus unbekannt"))
    {
        offen.push(format!("Auftragsstelle: {unbekannt:?}"));
    }
    if j.stand().offene_auftraege().next().is_some() {
        offen.push("Auftrag offen geblieben".into());
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

/// Kommt der Auftrag nicht an und bleibt die Antwort aus, ist er unklar: Halt, der Tag
/// bleibt offen. Ein Neustart gleicht ab, haelt wieder und sendet nichts.
#[test]
fn verlorener_auftrag_haelt_ohne_zweiten_versand() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (s, mut dachs, mut maus) = beispiel()?;
    let mut pk = Gestoert::neu(konto(&s)?);
    pk.auftrag_verloren = true;
    {
        let mut j = ort.oeffnen()?;
        let erg = lauf(&mut j, &s, &mut dachs, &mut maus, &mut pk);
        if !matches!(erg, Err(LaufFehler::Halt(_))) {
            return Err(format!("kein Halt: {erg:?}"));
        }
        let w = von(&j, "Kontowaechter");
        if !w.iter().any(|t| t.contains("OrderFehlt")) {
            return Err(format!("Kontowaechter: {w:?}"));
        }
        if j.stand().abschluss().is_some() {
            return Err("Tag trotz unklarem Auftrag abgeschlossen".into());
        }
    }
    pk.auftrag_verloren = false;
    let vorher = ort.bytes()?;
    let fragen = dachs.aufrufe.saturating_add(maus.aufrufe);
    let mut j = ort.oeffnen()?;
    let erg = lauf(&mut j, &s, &mut dachs, &mut maus, &mut pk);
    drop(j);
    if !matches!(erg, Err(LaufFehler::Halt(_))) || ort.bytes()? != vorher {
        return Err(format!("Neustart: {erg:?}"));
    }
    if pk.innen.sendungen() != 0 || dachs.aufrufe.saturating_add(maus.aufrufe) != fragen {
        return Err("Neustart hat gesendet oder gefragt".into());
    }
    Ok(())
}

/// Absturz direkt nach dem Senden (Maus): Der Auftrag ist reserviert, als gesendet belegt und
/// beim Konto angekommen, dann bricht der Lauf ab; der Markt fuehrt ihn aus. Der Neustart
/// gleicht zuerst ab und bucht die Ausfuehrung genau einmal, sendet nichts und fragt niemanden;
/// weil der Tag angefangen ist, haelt er danach an. Ein weiterer Neustart aendert nichts.
#[test]
fn absturz_direkt_nach_dem_senden_bucht_einmal_und_sendet_nicht() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (s, mut dachs, mut maus) = beispiel()?;
    let mut pk = konto(&s)?;
    let mut erster = s.clone();
    erster.tage.truncate(1);
    {
        let mut j = ort.oeffnen()?;
        vorbedingung(
            lauf(&mut j, &erster, &mut dachs, &mut maus, &mut pk),
            "erster Tag",
        )?;
    }
    let tag2 = s.tage.get(1).ok_or("Vorbedingung: zweiter Tag")?;
    let nr;
    {
        let mut j = ort.oeffnen()?;
        vorbedingung(j.beginne_abrechnung(tag2.datum), "Abrechnungstag")?;
        let topf_a = j
            .stand()
            .toepfe()
            .get(&TopfId::A)
            .cloned()
            .ok_or("Vorbedingung: Topf A")?;
        if topf_a.positionen().is_empty() {
            return Err("Vorbedingung: Topf A haelt nichts".into());
        }
        let mut vergabe = j.kennungsvergabe();
        let g = Grenzen::standard();
        let auftraege = notbremse_auftraege(&g, &topf_a, &tag2.kurse, &mut vergabe)
            .ok_or("Vorbedingung: Verkauf")?;
        let auftrag = auftraege.first().ok_or("Vorbedingung: Auftrag")?;
        nr = auftrag.auftrag().kennung.nr();
        vorbedingung(j.reserviere_auftrag(&g, TopfId::A, auftrag), "reservieren")?;
        vorbedingung(j.melde(nr, &Meldung::Gesendet), "gesendet")?;
        if pk.sende(auftrag) != Sendeantwort::Angenommen {
            return Err("Vorbedingung: Konto nimmt nicht an".into());
        }
        // Absturz: Die Antwort wird nicht mehr belegt.
    }
    pk.markt(&tag2.kurse);
    let sendungen = pk.sendungen();
    let fragen = dachs.aufrufe.saturating_add(maus.aufrufe);
    let mut offen = Vec::new();
    {
        let mut j = ort.oeffnen()?;
        let erg = lauf(&mut j, &s, &mut dachs, &mut maus, &mut pk);
        if !matches!(erg, Err(LaufFehler::Halt(_))) {
            offen.push(format!("kein Halt: {erg:?}"));
        }
        match j.stand().auftrag(nr) {
            Some(l) if l.zustand == AuftragsZustand::Gefuellt && l.ausfuehrungen.len() == 1 => {}
            anders => offen.push(format!("Auftrag {nr}: {anders:?}")),
        }
        let abw = vorbedingung(j.gleiche_ab(&pk), "Abgleich")?;
        if !abw.is_empty() {
            offen.push(format!("Journal und Konto: {abw:?}"));
        }
    }
    let vorher = ort.bytes()?;
    let mut j = ort.oeffnen()?;
    let erg = lauf(&mut j, &s, &mut dachs, &mut maus, &mut pk);
    drop(j);
    if !matches!(erg, Err(LaufFehler::Halt(_))) || ort.bytes()? != vorher {
        offen.push(format!("zweiter Neustart: {erg:?}"));
    }
    if pk.sendungen() != sendungen {
        offen.push("Neustart hat gesendet".into());
    }
    if dachs.aufrufe.saturating_add(maus.aufrufe) != fragen {
        offen.push("Neustart hat gefragt".into());
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

/// Halt bei widerspruechlichen Bestaenden (Maus, 6·N01): Eine fremde Belastung waehrend des
/// Laufs haelt nach dem Markt an. Der Grund steht im Journal, der Tag bleibt offen, und danach
/// wird nichts mehr verkauft, gekauft oder gefragt.
#[test]
fn befund_6_n01_fremde_belastung_haelt_den_lauf() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (s, mut dachs, mut maus) = beispiel()?;
    let mut pk = Gestoert::neu(konto(&s)?);
    pk.fremd = Some(Cent(-500));
    let mut j = ort.oeffnen()?;
    let erg = lauf(&mut j, &s, &mut dachs, &mut maus, &mut pk);
    if !matches!(erg, Err(LaufFehler::Halt(_))) {
        return Err(format!("kein Halt: {erg:?}"));
    }
    let w = von(&j, "Kontowaechter");
    if !w.iter().any(|t| t.contains("Bargeld")) {
        return Err(format!("Kontowaechter: {w:?}"));
    }
    let auftraege = j.stand().auftraege().len();
    let fragen = dachs.aufrufe.saturating_add(maus.aufrufe);
    if j.stand().abschluss().is_some() {
        return Err("Tag trotz Abweichung abgeschlossen".into());
    }
    drop(j);
    // Neustart: Halt, bevor etwas geschrieben, gesendet oder gefragt wird.
    let vorher = ort.bytes()?;
    let sendungen = pk.innen.sendungen();
    let mut j = ort.oeffnen()?;
    let erg = lauf(&mut j, &s, &mut dachs, &mut maus, &mut pk);
    let danach = j.stand().auftraege().len();
    drop(j);
    if !matches!(erg, Err(LaufFehler::Halt(_)))
        || ort.bytes()? != vorher
        || danach != auftraege
        || pk.innen.sendungen() != sendungen
        || dachs.aufrufe.saturating_add(maus.aufrufe) != fragen
    {
        return Err(format!("Neustart: {erg:?}"));
    }
    Ok(())
}

/// Widerspruechliche Bestaende zwischen zwei Tagen: Das Konto zeigt eine Belastung, die kein
/// Auftrag erklaert. Der Neustart haelt beim Abgleich, schreibt nichts und fragt niemanden.
#[test]
fn befund_6_n01_abweichung_zwischen_zwei_tagen_haelt_beim_neustart() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (s, mut dachs, mut maus) = beispiel()?;
    let mut pk = konto(&s)?;
    let mut erster = s.clone();
    erster.tage.truncate(2);
    {
        let mut j = ort.oeffnen()?;
        vorbedingung(
            lauf(&mut j, &erster, &mut dachs, &mut maus, &mut pk),
            "zwei Tage",
        )?;
    }
    pk.buche_fremd(Cent(-1)).ok_or("Vorbedingung: Buchung")?;
    let vorher = ort.bytes()?;
    let fragen = dachs.aufrufe.saturating_add(maus.aufrufe);
    let sendungen = pk.sendungen();
    let mut j = ort.oeffnen()?;
    let erg = lauf(&mut j, &s, &mut dachs, &mut maus, &mut pk);
    drop(j);
    match &erg {
        Err(LaufFehler::Halt(m)) if m.contains("Kontowächter beim Wiederanlauf: [Bargeld") => {}
        _ => return Err(format!("kein Halt des Kontowaechters: {erg:?}")),
    }
    if ort.bytes()? != vorher
        || pk.sendungen() != sendungen
        || dachs.aufrufe.saturating_add(maus.aufrufe) != fragen
    {
        return Err("Neustart hat geschrieben, gesendet oder gefragt".into());
    }
    Ok(())
}
