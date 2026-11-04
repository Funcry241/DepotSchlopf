//! Der simulierte Lauf mit erfundenen Daten. Alle Zahlen, Werte und Ereignisse sind erfunden
//! (D12). Jede Vorbereitung prueft ihren Erfolg ausdruecklich und endet sonst mit
//! „Vorbedingung …“.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{Art, Journal};
use depot_kern::{Cent, Datum, Herkunft, Isin, Kurs, Kurse, Topf, TopfId};
use depot_sim::{Drehbuch, LaufFehler, Papierkonto, Szenario, lauf};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

/// Eigenes Verzeichnis je Test mit eingerichtetem Journal.
struct Ort {
    dir: PathBuf,
    journal: PathBuf,
}

impl Ort {
    fn neu() -> Result<Ort, String> {
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("depot_sim_{}_{n}", std::process::id()));
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

/// Das Papierkonto zum Startstand des Szenarios.
fn konto(s: &Szenario) -> Result<Papierkonto, String> {
    Papierkonto::eroeffne(&s.start).ok_or_else(|| "Vorbedingung: Papierkonto".to_string())
}

fn datum(t: u8, m: u8) -> Result<Datum, String> {
    Datum::neu(2026, m, t).ok_or_else(|| format!("Vorbedingung: Datum {t}.{m}."))
}

fn isin(stamm: &str) -> Result<Isin, String> {
    (0..=9)
        .find_map(|d| Isin::neu(&format!("{stamm}{d}")).ok())
        .ok_or_else(|| format!("Vorbedingung: ISIN {stamm}"))
}

/// Laeuft das Beispiel ganz auf einem frischen Journal.
fn ganzer_lauf(ort: &Ort) -> Result<(Journal, Drehbuch, Drehbuch), String> {
    let (s, mut dachs, mut maus) = beispiel()?;
    let mut k = konto(&s)?;
    let mut j = ort.oeffnen()?;
    lauf(&mut j, &s, &mut dachs, &mut maus, &mut k).map_err(|e| format!("Lauf: {e:?}"))?;
    Ok((j, dachs, maus))
}

/// Begruendungen eines Tages von `wer`.
fn von<'a>(j: &'a Journal, d: Datum, wer: &str) -> Vec<&'a str> {
    j.stand()
        .begruendungen()
        .iter()
        .filter(|(t, w, _)| *t == d && w == wer)
        .map(|(_, _, text)| text.as_str())
        .collect()
}

fn buchungen_am(j: &Journal, d: Datum) -> usize {
    j.stand()
        .ki()
        .buchungen()
        .iter()
        .filter(|b| b.datum == d)
        .count()
}

/// Wert aller Toepfe zu diesen Kursen.
fn depotwert(j: &Journal, kurse: &Kurse) -> Result<Cent, String> {
    j.stand()
        .toepfe()
        .values()
        .try_fold(Cent::NULL, |s, t| s.checked_add(t.wert(kurse)?))
        .ok_or_else(|| "Depotwert nicht berechenbar".to_string())
}

#[test]
fn beispiel_laeuft_durch_und_steht_im_journal() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (j, _, _) = ganzer_lauf(&ort)?;
    let st = j.stand();
    let letzter = datum(1, 12)?;
    assert_eq!(st.abschluss().map(|a| a.datum), Some(letzter));
    assert_eq!(st.seit_abschluss(), 0, "jeder Tag ist abgeschlossen");
    assert_eq!(st.beobachtungen().len(), 6, "jedes Ereignis ist beobachtet");
    assert_eq!(st.toepfe().len(), 2);
    assert!(!st.auftraege().is_empty(), "es gibt Papierauftraege");
    let t25 = datum(25, 11)?;
    let dachs = von(&j, t25, "Dachs");
    let beta = isin("XA0000BETA0")?;
    assert!(
        dachs.iter().any(|t| t.starts_with("Topf A: ")
            && t.contains(beta.als_str())
            && t.contains("2000 bp: ruhiger Wert")),
        "Dachs begruendet jedes Ziel: {dachs:?}"
    );
    assert!(!von(&j, t25, "Maus").is_empty(), "Maus begruendet");
    assert!(!von(&j, t25, "Simulation").is_empty(), "Ausfuehrung belegt");
    // Wiedergeoeffnet steht derselbe Stand da.
    let stand = st.clone();
    drop(j);
    assert_eq!(ort.oeffnen()?.stand(), &stand);
    Ok(())
}

#[test]
fn kostenschutz_greift_und_jeder_aufruf_ist_gebucht() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (j, dachs, maus) = ganzer_lauf(&ort)?;
    let t27 = datum(27, 11)?;
    let gesamt =
        usize::try_from(dachs.aufrufe.saturating_add(maus.aufrufe)).map_err(|e| e.to_string())?;
    assert_eq!(
        j.stand().ki().buchungen().len(),
        gesamt,
        "jede Frage ist gebucht"
    );
    assert_eq!(gesamt, 10, "4 + 4 im November, 2 im Dezember");
    assert_eq!(buchungen_am(&j, t27), 0, "Monat voll: niemand gefragt");
    let schutz = von(&j, t27, "Kostenschutz");
    assert_eq!(schutz.len(), 2, "{schutz:?}");
    assert!(schutz.iter().all(|t| t.contains("MonatVoll")), "{schutz:?}");
    assert!(von(&j, t27, "Dachs").is_empty() && von(&j, t27, "Maus").is_empty());
    assert_eq!(buchungen_am(&j, datum(1, 12)?), 2, "neuer Monat");
    Ok(())
}

#[test]
fn veto_sperrt_freiwillige_auftraege() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (j, _, _) = ganzer_lauf(&ort)?;
    let t26 = datum(26, 11)?;
    let veto = von(&j, t26, "Dachs");
    assert!(
        veto.iter().any(|t| t.starts_with("Topf B: Veto")),
        "{veto:?}"
    );
    let simulation = von(&j, t26, "Simulation");
    assert!(
        simulation.iter().all(|t| !t.starts_with("Topf B")),
        "{simulation:?}"
    );
    assert!(
        simulation.iter().any(|t| t.starts_with("Topf A")),
        "Topf A hat Zustimmung: {simulation:?}"
    );
    Ok(())
}

#[test]
fn regelwerk_kuerzt_zu_hohes_gewicht() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (j, _, _) = ganzer_lauf(&ort)?;
    let alpha = isin("XA000ALPHA0")?;
    let regel = von(&j, datum(26, 11)?, "Regelwerk");
    assert!(
        regel.iter().any(|t| t.starts_with("Topf A")
            && t.contains("GewichtGekuerzt")
            && t.contains(alpha.als_str())
            && t.contains("auf_bp: 2500")),
        "{regel:?}"
    );
    Ok(())
}

#[test]
fn unplausible_kurse_lassen_den_zyklus_ausfallen() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (j, _, _) = ganzer_lauf(&ort)?;
    let t30 = datum(30, 11)?;
    let daten = von(&j, t30, "Daten");
    assert!(daten.iter().any(|t| t.contains("Kurssprung")), "{daten:?}");
    assert_eq!(buchungen_am(&j, t30), 0, "niemand gefragt");
    assert!(von(&j, t30, "Kostenschutz").is_empty());
    assert!(von(&j, t30, "Simulation").is_empty(), "kein Auftrag");
    // Am Folgetag gilt der letzte plausible Kurs als Vortag: Gamma ist wieder plausibel.
    assert!(von(&j, datum(1, 12)?, "Daten").is_empty());
    Ok(())
}

#[test]
fn wiederanlauf_setzt_nach_dem_letzten_abschluss_fort() -> Result<(), String> {
    let ganz = Ort::neu()?;
    drop(ganzer_lauf(&ganz)?);

    let geteilt = Ort::neu()?;
    let (s, mut dachs, mut maus) = beispiel()?;
    let mut erster = s.clone();
    erster.tage.truncate(2);
    // Das Konto lebt ueber den Neustart weiter.
    let mut k = konto(&s)?;
    {
        let mut j = geteilt.oeffnen()?;
        lauf(&mut j, &erster, &mut dachs, &mut maus, &mut k)
            .map_err(|e| format!("erster Teil: {e:?}"))?;
    }
    let (_, mut dachs2, mut maus2) = beispiel()?;
    {
        let mut j = geteilt.oeffnen()?;
        lauf(&mut j, &s, &mut dachs2, &mut maus2, &mut k)
            .map_err(|e| format!("zweiter Teil: {e:?}"))?;
    }
    assert_eq!(geteilt.bytes()?, ganz.bytes()?, "byte-gleiches Journal");
    assert_eq!(
        dachs.aufrufe.saturating_add(dachs2.aufrufe),
        5,
        "wie im ganzen Lauf"
    );
    // Ein dritter Lauf findet nichts mehr zu tun.
    let vorher = geteilt.bytes()?;
    let (_, mut d3, mut m3) = beispiel()?;
    let mut j = geteilt.oeffnen()?;
    lauf(&mut j, &s, &mut d3, &mut m3, &mut k).map_err(|e| format!("dritter Lauf: {e:?}"))?;
    drop(j);
    assert_eq!(geteilt.bytes()?, vorher);
    assert_eq!(d3.aufrufe.saturating_add(m3.aufrufe), 0);
    Ok(())
}

#[test]
fn unvollstaendiger_tag_haelt_ohne_neue_frage() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (s, mut dachs, mut maus) = beispiel()?;
    let mut erster = s.clone();
    erster.tage.truncate(1);
    let mut k = konto(&s)?;
    {
        let mut j = ort.oeffnen()?;
        vorbedingung(
            lauf(&mut j, &erster, &mut dachs, &mut maus, &mut k),
            "erster Tag",
        )?;
        // Ein Absturz mitten im zweiten Tag: Ein Eintrag steht, der Abschluss fehlt.
        vorbedingung(
            j.beobachte(datum(26, 11)?, "angefangener Tag"),
            "angefangener Tag",
        )?;
    }
    let vorher = ort.bytes()?;
    let (_, mut d2, mut m2) = beispiel()?;
    let mut j = ort.oeffnen()?;
    let r = lauf(&mut j, &s, &mut d2, &mut m2, &mut k);
    drop(j);
    assert!(matches!(r, Err(LaufFehler::Halt(_))), "{r:?}");
    assert_eq!(ort.bytes()?, vorher, "nichts geschrieben");
    assert_eq!(d2.aufrufe.saturating_add(m2.aufrufe), 0, "niemand gefragt");
    Ok(())
}

/// Dieselben Kurse mit anderer Herkunft.
fn mit_herkunft(kurse: &Kurse, isins: &[Isin], h: Herkunft) -> Result<Kurse, String> {
    let mut neu = Vec::new();
    for i in isins {
        let k = kurse.eintrag(i).ok_or("Vorbedingung: Kurs fehlt")?;
        neu.push((
            i.clone(),
            vorbedingung(
                Kurs::exakt(k.preis(), "EUR", k.handelstag(), k.zeitpunkt(), h),
                "Kurs",
            )?,
        ));
    }
    vorbedingung(Kurse::neu(kurse.stand(), neu), "Kurse")
}

#[test]
fn echte_kurse_werden_abgelehnt() -> Result<(), String> {
    for h in [Herkunft::Broker, Herkunft::Aufzeichnung] {
        let ort = Ort::neu()?;
        let (mut s, mut dachs, mut maus) = beispiel()?;
        let isins: Vec<Isin> = s.universum.isins().into_iter().collect();
        let letzter = s.tage.last_mut().ok_or("Vorbedingung: Tage")?;
        letzter.kurse = mit_herkunft(&letzter.kurse, &isins, h)?;
        let vorher = ort.bytes()?;
        let mut j = ort.oeffnen()?;
        let r = lauf(&mut j, &s, &mut dachs, &mut maus, &mut konto(&s)?);
        drop(j);
        assert!(matches!(r, Err(LaufFehler::Halt(_))), "{h:?}: {r:?}");
        assert_eq!(ort.bytes()?, vorher, "{h:?}: nichts geschrieben");
        assert_eq!(dachs.aufrufe.saturating_add(maus.aufrufe), 0);
    }
    Ok(())
}

#[test]
fn papiergeld_wird_nie_negativ_und_kosten_mindern_den_wert() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (j, _, _) = ganzer_lauf(&ort)?;
    let (s, _, _) = beispiel()?;
    for t in j.stand().toepfe().values() {
        assert!(t.bargeld().0 >= 0, "{t:?}");
    }
    // Zu den Kursen des Vorlaufs bewertet ist das Depot hoechstens so viel wert wie am
    // Anfang: Jede Ausfuehrung kostet Gebuehren, kein Kurs wurde zu Gunsten gerechnet.
    let start: Cent = s
        .start
        .iter()
        .try_fold(Cent::NULL, |a, t| a.checked_add(t.bargeld()))
        .ok_or("Startwert")?;
    assert!(depotwert(&j, &s.vorlauf)? < start);
    Ok(())
}

#[test]
fn tagesverlust_haelt_bis_zur_freigabe() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (mut s, mut dachs, mut maus) = beispiel()?;
    let beta = isin("XA0000BETA0")?;
    let alpha = isin("XA000ALPHA0")?;
    let gamma = isin("XA000GAMMA0")?;
    // Topf A haelt nur Beta; Beta faellt am ersten Tag um 7,5 % (unter der Sprunggrenze).
    let mut topf_a = vorbedingung(Topf::neu(TopfId::A, Cent(0)), "Topf A")?;
    topf_a.setze_bestand(beta.clone(), 250);
    let topf_b = vorbedingung(Topf::neu(TopfId::B, Cent(0)), "Topf B")?;
    s.start = vec![topf_a, topf_b];
    let t25 = datum(25, 11)?;
    let zeit = depot_kern::Zeitpunkt::neu(t25, 17, 30, 0).ok_or("Vorbedingung: Zeit")?;
    let kurs = |p: i64| Kurs::neu(Cent(p), "EUR", t25, zeit, Herkunft::Simulation);
    let erster = s.tage.first_mut().ok_or("Vorbedingung: Tage")?;
    erster.kurse = vorbedingung(
        Kurse::neu(
            zeit,
            vec![
                (alpha, vorbedingung(kurs(5_010), "Kurs")?),
                (beta, vorbedingung(kurs(7_400), "Kurs")?),
                (gamma, vorbedingung(kurs(2_005), "Kurs")?),
            ],
        ),
        "Kurse",
    )?;
    {
        let mut j = ort.oeffnen()?;
        let r = lauf(&mut j, &s, &mut dachs, &mut maus, &mut konto(&s)?);
        assert!(matches!(r, Err(LaufFehler::Halt(_))), "{r:?}");
        let waechter = von(&j, t25, "Waechter");
        assert!(
            waechter.iter().any(|t| t.contains("Tagesverlust")),
            "{waechter:?}"
        );
        assert!(j.stand().seit_abschluss() > 0, "der Tag bleibt offen");
        assert_eq!(j.stand().auftraege().len(), 0);
    }
    assert_eq!(
        dachs.aufrufe.saturating_add(maus.aufrufe),
        0,
        "niemand gefragt"
    );
    // Ein neuer Start fragt nicht einfach weiter.
    let vorher = ort.bytes()?;
    let mut j = ort.oeffnen()?;
    let r = lauf(&mut j, &s, &mut dachs, &mut maus, &mut konto(&s)?);
    drop(j);
    assert!(matches!(r, Err(LaufFehler::Halt(_))), "{r:?}");
    assert_eq!(ort.bytes()?, vorher);
    Ok(())
}

#[test]
fn steuerzeichen_in_antworten_brechen_das_journal_nicht() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (s, mut dachs, mut maus) = beispiel()?;
    let t25 = datum(25, 11)?;
    if let Some(a) = dachs.antworten.get_mut(&t25) {
        a.einschaetzung = "Zeile eins\nZeile zwei\u{7}".into();
    }
    let mut j = ort.oeffnen()?;
    vorbedingung(
        lauf(&mut j, &s, &mut dachs, &mut maus, &mut konto(&s)?),
        "Lauf",
    )?;
    let d = von(&j, t25, "Dachs");
    assert!(
        d.iter().any(|t| t.contains("Zeile eins Zeile zwei")),
        "{d:?}"
    );
    assert!(d.iter().all(|t| !t.chars().any(char::is_control)));
    Ok(())
}

#[test]
fn journal_eintraege_sind_alle_lesbar() -> Result<(), String> {
    // Jeder geschriebene Eintrag hat eine bekannte Art; Begruendung und Abschluss stehen drin.
    let ort = Ort::neu()?;
    let (j, _, _) = ganzer_lauf(&ort)?;
    let arten: Vec<Art> = j.eintraege().iter().map(|(a, _)| *a).collect();
    assert!(arten.contains(&Art::Begruendung));
    assert!(arten.contains(&Art::Abschluss));
    assert_eq!(arten.last(), Some(&Art::Abschluss));
    Ok(())
}

/// Kurse eines erfundenen Tages fuer die drei Werte zum selben Preis.
fn gleiche_kurse(d: Datum, isins: &[Isin], preis: i64) -> Result<Kurse, String> {
    let zeit = depot_kern::Zeitpunkt::neu(d, 17, 30, 0).ok_or("Vorbedingung: Zeit")?;
    let mut alle = Vec::new();
    for i in isins {
        alle.push((
            i.clone(),
            vorbedingung(
                Kurs::neu(Cent(preis), "EUR", d, zeit, Herkunft::Simulation),
                "Kurs",
            )?,
        ));
    }
    vorbedingung(Kurse::neu(zeit, alle), "Kurse")
}

#[test]
fn notbremse_verkauft_alles_und_haelt() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (mut s, _, _) = beispiel()?;
    let isins: Vec<Isin> = s.universum.isins().into_iter().collect();
    // Topf A haelt alle drei Werte; die Kurse fallen taeglich um 4,4 %: kein Tagesverlust,
    // kein Kurssprung, aber nach einigen Tagen mehr als 20 % unter dem Hoechststand.
    let mut topf_a = vorbedingung(Topf::neu(TopfId::A, Cent(0)), "Topf A")?;
    for i in &isins {
        topf_a.setze_bestand(i.clone(), 100);
    }
    s.start = vec![
        topf_a,
        vorbedingung(Topf::neu(TopfId::B, Cent(0)), "Topf B")?,
    ];
    s.vorlauf = gleiche_kurse(datum(1, 11)?, &isins, 10_000)?;
    let mut preis: i64 = 10_000;
    s.tage.clear();
    for t in 2..=12 {
        preis = preis
            .checked_mul(956)
            .and_then(|p| p.checked_div(1000))
            .ok_or("Vorbedingung: Preis")?;
        let d = datum(t, 11)?;
        s.tage.push(depot_sim::Tag {
            datum: d,
            kurse: gleiche_kurse(d, &isins, preis)?,
            ereignisse: vec!["erfundener Kursrueckgang".into()],
        });
    }
    let (mut dachs, mut maus) = (Drehbuch::default(), Drehbuch::default());
    let mut j = ort.oeffnen()?;
    let r = lauf(&mut j, &s, &mut dachs, &mut maus, &mut konto(&s)?);
    assert!(matches!(r, Err(LaufFehler::Halt(_))), "{r:?}");
    let st = j.stand();
    let waechter: Vec<&str> = st
        .begruendungen()
        .iter()
        .filter(|(_, w, _)| w == "Waechter")
        .map(|(_, _, t)| t.as_str())
        .collect();
    assert!(
        waechter.iter().any(|t| t.contains("Notbremse")),
        "{waechter:?}"
    );
    let a = st.toepfe().get(&TopfId::A).ok_or("Topf A fehlt")?;
    assert!(a.positionen().is_empty(), "alles verkauft: {a:?}");
    assert!(a.bargeld().0 > 0);
    assert!(
        st.auftraege()
            .iter()
            .any(|b| b.beleg.anlass == depot_regelwerk::Anlass::Notbremse)
    );
    assert!(st.seit_abschluss() > 0, "der Tag bleibt offen");
    Ok(())
}

/// Laeuft die ersten `tage` Tage des Beispiels auf einem frischen Journal. Liefert das Konto,
/// das danach weiterlebt.
fn angefangen(ort: &Ort, tage: usize) -> Result<Papierkonto, String> {
    let (mut s, mut dachs, mut maus) = beispiel()?;
    let mut k = konto(&s)?;
    s.tage.truncate(tage);
    let mut j = ort.oeffnen()?;
    vorbedingung(
        lauf(&mut j, &s, &mut dachs, &mut maus, &mut k),
        "erster Teil",
    )?;
    Ok(k)
}

/// K15-01 (Maus, PR #15): Der Wiederanlauf darf nur mit derselben abgeschlossenen
/// Vorgeschichte fortsetzen. Jede Aenderung davor haelt an, ohne zu schreiben und ohne zu
/// fragen; nur die Tage nach dem letzten Abschluss duerfen anders sein.
#[test]
fn befund_k15_01_wiederanlauf_prueft_die_vorgeschichte() -> Result<(), String> {
    type Aenderung = fn(&mut Szenario) -> Result<(), String>;
    let faelle: [(&str, Aenderung); 6] = [
        ("abgeschlossener Tag entfernt", |s| {
            s.tage.remove(1);
            Ok(())
        }),
        ("Kurs eines abgeschlossenen Tags", |s| {
            let isins: Vec<Isin> = s.universum.isins().into_iter().collect();
            let t = s.tage.first_mut().ok_or("Tage")?;
            let d = t.datum;
            t.kurse = gleiche_kurse(d, &isins, 5_010)?;
            Ok(())
        }),
        ("Ereignis eines abgeschlossenen Tags", |s| {
            let t = s.tage.first_mut().ok_or("Tage")?;
            t.ereignisse.push("nachtraeglich erfunden".into());
            Ok(())
        }),
        ("Vorlauf", |s| {
            let isins: Vec<Isin> = s.universum.isins().into_iter().collect();
            s.vorlauf = gleiche_kurse(datum(24, 11)?, &isins, 5_000)?;
            Ok(())
        }),
        ("Startstand", |s| {
            s.start = vec![
                Topf::neu(TopfId::A, Cent(1_000_001)).map_err(|e| format!("{e:?}"))?,
                Topf::neu(TopfId::B, Cent(1_000_000)).map_err(|e| format!("{e:?}"))?,
            ];
            Ok(())
        }),
        ("Positivliste", |s| {
            s.universum = depot_kern::Universum::neu(Vec::new());
            Ok(())
        }),
    ];
    for (fall, aendere) in faelle {
        let ort = Ort::neu()?;
        let mut k = angefangen(&ort, 3)?;
        let vorher = ort.bytes()?;
        let (mut s, mut dachs, mut maus) = beispiel()?;
        aendere(&mut s)?;
        let mut j = ort.oeffnen()?;
        let r = lauf(&mut j, &s, &mut dachs, &mut maus, &mut k);
        drop(j);
        assert!(matches!(r, Err(LaufFehler::Halt(_))), "{fall}: {r:?}");
        assert_eq!(ort.bytes()?, vorher, "{fall}: nichts geschrieben");
        assert_eq!(
            dachs.aufrufe.saturating_add(maus.aufrufe),
            0,
            "{fall}: niemand gefragt"
        );
    }

    // Kontrolle: Ein anderer Tag nach dem letzten Abschluss ist erlaubt.
    let ort = Ort::neu()?;
    let mut k = angefangen(&ort, 3)?;
    let (mut s, mut dachs, mut maus) = beispiel()?;
    let letzter = s.tage.last_mut().ok_or("Tage")?;
    letzter.ereignisse.push("anderer Ausgang".into());
    let mut j = ort.oeffnen()?;
    vorbedingung(
        lauf(&mut j, &s, &mut dachs, &mut maus, &mut k),
        "Fortsetzung",
    )?;
    assert_eq!(j.stand().abschluss().map(|a| a.datum), Some(datum(1, 12)?));
    Ok(())
}

/// K15-02 (Maus, PR #15): Ein Kurssatz gehoert zu seinem Handelstag. Traegt der 26.11. die
/// Kurse des 25.11., faellt der Zyklus aus: Niemand wird gefragt, kein Auftrag.
#[test]
fn befund_k15_02_alter_kurssatz_gilt_nicht_fuer_einen_neuen_tag() -> Result<(), String> {
    let ort = Ort::neu()?;
    let (mut s, mut dachs, mut maus) = beispiel()?;
    let erster = s.tage.first().ok_or("Tage")?.kurse.clone();
    let zweiter = s.tage.get_mut(1).ok_or("Tage")?;
    zweiter.kurse = erster;
    let mut j = ort.oeffnen()?;
    vorbedingung(
        lauf(&mut j, &s, &mut dachs, &mut maus, &mut konto(&s)?),
        "Lauf",
    )?;
    let t26 = datum(26, 11)?;
    let daten = von(&j, t26, "Daten");
    assert!(
        daten.iter().any(|t| t.contains("AbrufNichtAmHandelstag")),
        "{daten:?}"
    );
    assert_eq!(buchungen_am(&j, t26), 0, "niemand gefragt");
    assert!(von(&j, t26, "Simulation").is_empty(), "kein Auftrag");
    Ok(())
}

/// K15-03 (Maus, PR #15): Ein Vorlaufkurs mit Handelstag 24.11. und Zeitpunkt 26.11. lag am
/// 25.11. noch nicht vor. Er wird abgelehnt, bevor ein Berater gefragt oder ein Auftrag belegt
/// wird: schon beim Bau des Kurses oder spaetestens in der Kursprüfung des 25.11.
#[test]
fn befund_k15_03_vergleichskurs_aus_der_zukunft() -> Result<(), String> {
    let (mut s, mut dachs, mut maus) = beispiel()?;
    let t24 = datum(24, 11)?;
    let zukunft = depot_kern::Zeitpunkt::neu(datum(26, 11)?, 17, 30, 0).ok_or("Zeit")?;
    // Dieselben Preise wie im Vorlauf des Beispiels, nur mit dem Zeitpunkt 26.11.
    let mut eintraege = Vec::new();
    for (isin, alt) in s.vorlauf.alle() {
        match Kurs::exakt(alt.preis(), "EUR", t24, zukunft, Herkunft::Simulation) {
            Ok(k) => eintraege.push((isin.clone(), k)),
            // Schon der Bau lehnt ab: Der Kurs kann nirgends ankommen.
            Err(_) => return Ok(()),
        }
    }
    s.vorlauf = vorbedingung(Kurse::neu(zukunft, eintraege), "Vorlauf")?;
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    let r = lauf(&mut j, &s, &mut dachs, &mut maus, &mut konto(&s)?);
    let t25 = datum(25, 11)?;
    assert_eq!(buchungen_am(&j, t25), 0, "niemand gefragt: {r:?}");
    assert!(von(&j, t25, "Simulation").is_empty(), "kein Auftrag");
    Ok(())
}

/// K15-03, zweiter Weg: Die Vorlaufkurse liegen am 24.11., aber ihr Abruf ist vom 26.11. Auch
/// das lag am 25.11. noch nicht vor; niemand wird gefragt, kein Auftrag.
#[test]
fn befund_k15_03_vergleichsabruf_aus_der_zukunft() -> Result<(), String> {
    let (mut s, mut dachs, mut maus) = beispiel()?;
    let zukunft = depot_kern::Zeitpunkt::neu(datum(26, 11)?, 17, 30, 0).ok_or("Zeit")?;
    let eintraege = s
        .vorlauf
        .alle()
        .map(|(i, k)| (i.clone(), k.clone()))
        .collect();
    s.vorlauf = vorbedingung(Kurse::neu(zukunft, eintraege), "Vorlauf")?;
    let ort = Ort::neu()?;
    let mut j = ort.oeffnen()?;
    vorbedingung(
        lauf(&mut j, &s, &mut dachs, &mut maus, &mut konto(&s)?),
        "Lauf",
    )?;
    let t25 = datum(25, 11)?;
    let daten = von(&j, t25, "Daten");
    assert!(
        daten.iter().any(|t| t.contains("VergleichsabrufNachAbruf")),
        "{daten:?}"
    );
    assert_eq!(buchungen_am(&j, t25), 0, "niemand gefragt");
    assert!(von(&j, t25, "Simulation").is_empty(), "kein Auftrag");
    Ok(())
}
