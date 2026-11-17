//! Der simulierte Lauf: Tag fuer Tag beobachten, fragen, pruefen lassen, entscheiden, auf
//! Papier ausfuehren, abschliessen. Alles steht im Journal.
//!
//! **Schutzrahmen statt Kaufregeln:** Was gekauft wird, schlagen Dachs und Maus vor und
//! begruenden es selbst. Der Lauf schreibt keine Abwaegung vor. Er sorgt nur dafuer, dass jede
//! Frage vorher gebucht ist (R8), dass nur eine ausdrueckliche Zweitmeinung einen Vorschlag
//! weiterlaesst (R31), dass das Regelwerk entscheidet (D01) und dass Daten- und Verlustgrenzen
//! halten (D03).
//!
//! **Ablauf eines Tages:** Ereignisse beobachten. Kontowaechter: Journal und Papierkonto
//! stimmen ueberein, sonst Halt. Kurse pruefen; bei einem Fehler faellt der Zyklus aus.
//! Waechter: Tagesverlust oder unplausible Werte halten an, die Notbremse verkauft alles und
//! haelt an; der Tag bleibt dann offen. Dann je Topf: der zustaendige Berater schlaegt vor,
//! der andere prueft, das Regelwerk entscheidet. Jeder Auftrag wird mit Deckung reserviert,
//! als gesendet belegt und dann an das Papierkonto gesendet; nach dem Markt gleicht der
//! Kontowaechter ab und belegt jede Ausfuehrung. Kaeufe erst, wenn alle Verkaeufe des Topfes
//! ausgefuehrt sind. Zuletzt der Abschluss; er setzt voraus, dass kein Auftrag offen ist.
//!
//! **Wiederanlauf:** Der Lauf setzt nach dem letzten Abschluss fort. Zuerst gleicht der
//! Kontowaechter mit dem Papierkonto ab und belegt, was dort aus offenen Auftraegen geworden
//! ist; gesendet wird dabei nichts (R37). Jede Abweichung ist Halt. Hat das Journal Eintraege
//! nach dem letzten Abschluss, war ein Tag angefangen: Halt, und niemand wird neu gefragt.
//! Was ein angefangener Tag gekostet hat, steht im Journal; ihn zu wiederholen, waere eine
//! zweite Frage fuer denselben Zyklus.

use std::collections::{BTreeMap, BTreeSet};

use depot_journal::{
    AuftragsZustand, Journal, JournalFehler, Meldung, PapierHalt, PrognoseBeleg, Weltprognose,
};
use depot_kern::liste::papierorder_schluss;
use depot_kern::{
    BeraterId, Cent, Grenzen, Herkunft, Isin, Kurse, Rundung, Topf, TopfId, Universum, Vorschlag,
    Zeitpunkt,
};
use depot_ki::Sperre;
use depot_regelwerk::{
    DatenFehler, Eingabe, GenehmigterAuftrag, Richtung, WaechterBefund, entscheide_ersatzkauf,
    entscheide_mit_kaufbudget, notbremse_auftraege, pruefe_kurse, stopp_auftraege, waechter,
};

use crate::abdruck;
use crate::papier::{Broker, Sendeantwort};
use crate::szenario::{Berater, KEINE_VERWERTBARE_ANTWORT, Lage, Szenario, Tag};
use crate::teilveto::Urteilsfolge;

/// Wer im Journal begruendet, ausser Dachs und Maus.
const DATEN: &str = "Daten";
const WAECHTER: &str = "Waechter";
const KOSTENSCHUTZ: &str = "Kostenschutz";
pub(crate) const REGELWERK: &str = "Regelwerk";
const SIMULATION: &str = "Simulation";
const KONTOWAECHTER: &str = "Kontowaechter";
const AUFTRAGSSTELLE: &str = "Auftragsstelle";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaufFehler {
    /// Der Lauf steht. Was bis dahin abgeschlossen ist, steht im Journal.
    Halt(String),
}

pub(crate) fn halt(text: impl Into<String>) -> LaufFehler {
    LaufFehler::Halt(text.into())
}

pub(crate) fn journal(e: JournalFehler) -> LaufFehler {
    halt(match e {
        JournalFehler::Belegt => "Journal belegt".into(),
        JournalFehler::Halt(t) => format!("Journal: Halt: {t}"),
        JournalFehler::Abgelehnt(t) => format!("Journal: abgelehnt: {t}"),
    })
}

pub(crate) fn rechnung(was: &str) -> LaufFehler {
    halt(format!("Rechenfehler: {was}"))
}

/// „nicht angebunden“, mit dem Grund des Beraters, wenn er einen nennt.
fn nicht_angebunden(b: &dyn Berater) -> String {
    b.warum_nicht().map_or_else(
        || NICHT_ANGEBUNDEN.to_string(),
        |g| format!("{NICHT_ANGEBUNDEN} ({})", sauber(&g)),
    )
}

fn name(b: BeraterId) -> &'static str {
    match b {
        BeraterId::Dachs => "Dachs",
        BeraterId::Maus => "Maus",
    }
}

fn andere(b: BeraterId) -> BeraterId {
    match b {
        BeraterId::Dachs => BeraterId::Maus,
        BeraterId::Maus => BeraterId::Dachs,
    }
}

/// So steht im Journal, was ein Berater in einem Topf beigetragen hat. Die Anzeige liest es mit
/// `gefragt` zurueck; dieselben Worte fuer Schreiben und Lesen.
const ANTWORT_VERWENDET: &str = "Antwort verwendet";
const ANTWORT_VERWORFEN: &str = "Antwort verworfen";
const ZUSTIMMUNG: &str = "Zustimmung";
const VETO: &str = "Veto";
const URTEIL_VERWORFEN: &str = "Antwort verworfen, kein Urteil";
const NICHT_ANGEBUNDEN: &str = "nicht angebunden";

/// Was eine Begruendung eines Beraters ueber seine Mitwirkung sagt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gefragt {
    /// Gefragt, und der Papiertag hat die Antwort verwendet (Vorschlag, Zustimmung oder Veto).
    Verwendet,
    /// Gefragt, aber die Antwort wurde verworfen.
    Verworfen,
    /// Nicht gefragt, mit dem Grund, falls der Berater ihn nannte.
    NichtAngebunden(Option<String>),
}

/// Liest eine Begruendung, wie `frage` und `topf_zyklus` sie schreiben (`Topf A: …`). Andere
/// Begruendungen (etwa Zielgewichte) sagen nichts ueber die Mitwirkung: `None`.
pub fn gefragt(text: &str) -> Option<Gefragt> {
    let rest = [TopfId::A, TopfId::B]
        .into_iter()
        .find_map(|t| text.strip_prefix(topf_name(t))?.strip_prefix(": "))?;
    let nach = |wort: &str| rest.strip_prefix(wort)?.strip_prefix(": ");
    if nach(URTEIL_VERWORFEN).is_some() || nach(ANTWORT_VERWORFEN).is_some() {
        return Some(Gefragt::Verworfen);
    }
    // Journale vor dem ausdruecklichen „Antwort verworfen“ schrieben eine verworfene Antwort
    // als `Topf A: keine verwertbare Antwort: …` und ein verworfenes Urteil als `Topf B: Veto:
    // keine verwertbare Antwort: …`. Beides ist kein Beitrag (Maus, Restbefund zu PR #33). Im
    // Zweifel zaehlt ein Eintrag als verworfen, nie als Beitrag.
    if rest.starts_with(KEINE_VERWERTBARE_ANTWORT)
        || [ZUSTIMMUNG, VETO, ANTWORT_VERWENDET]
            .into_iter()
            .filter_map(nach)
            .any(|r| r.starts_with(KEINE_VERWERTBARE_ANTWORT))
    {
        return Some(Gefragt::Verworfen);
    }
    if [ANTWORT_VERWENDET, ZUSTIMMUNG, VETO]
        .into_iter()
        .any(|w| nach(w).is_some())
    {
        return Some(Gefragt::Verwendet);
    }
    let r = rest.strip_prefix(NICHT_ANGEBUNDEN)?;
    let grund = r
        .strip_prefix(" (")
        .and_then(|g| g.split_once("), nicht gefragt"))
        .map(|(g, _)| g.to_string());
    (grund.is_some() || r.starts_with(", nicht gefragt")).then_some(Gefragt::NichtAngebunden(grund))
}

pub(crate) fn topf_name(t: TopfId) -> &'static str {
    match t {
        TopfId::A => "Topf A",
        TopfId::B => "Topf B",
    }
}

/// Text fuer das Journal: Steuerzeichen werden Leerzeichen, leerer Text wird `(leer)`. So
/// kann keine Antwort eines Beraters eine Zeile des Journals brechen.
fn sauber(text: &str) -> String {
    let t: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if t.trim().is_empty() {
        "(leer)".into()
    } else {
        t
    }
}

pub(crate) fn begruende(
    j: &mut Journal,
    tag: &Tag,
    wer: &str,
    text: &str,
) -> Result<(), LaufFehler> {
    j.begruende(tag.datum, wer, &sauber(text)).map_err(journal)
}

/// Nur erfundene Kurse. Ein Kurs anderer Herkunft haelt den Lauf an, bevor etwas geschrieben
/// wird.
fn nur_simulation(s: &Szenario) -> Result<(), LaufFehler> {
    let alle = std::iter::once(&s.vorlauf).chain(s.tage.iter().map(|t| &t.kurse));
    for kurse in alle {
        if let Some((isin, k)) = kurse
            .alle()
            .find(|(_, k)| k.herkunft() != Herkunft::Simulation)
        {
            return Err(halt(format!(
                "Kurs von {isin} mit Herkunft {:?}: der simulierte Lauf nimmt nur erfundene Kurse",
                k.herkunft()
            )));
        }
    }
    if s.tage.windows(2).any(|w| match w {
        [a, b] => a.datum >= b.datum,
        _ => false,
    }) {
        return Err(halt("Tage nicht streng aufsteigend"));
    }
    Ok(())
}

/// Depotwert in ganzen Cent. Der heutige Wert wird abgerundet, Vergleichswerte (Vortag,
/// Hoechststand) aufgerundet: So verdeckt die Rundung keinen Verlust (K21-01). Bei Kursen in
/// ganzen Cent ist beides derselbe Wert.
pub(crate) fn depotwert<'a>(
    toepfe: impl Iterator<Item = &'a Topf>,
    kurse: &Kurse,
    rundung: Rundung,
) -> Option<Cent> {
    toepfe.into_iter().try_fold(Cent::NULL, |s, t| {
        s.checked_add(t.wert_gerundet(kurse, rundung)?)
    })
}

/// Laeuft das Szenario ab dem Tag nach dem letzten Abschluss. `konto` ist das Papierkonto;
/// es lebt unabhaengig vom Journal weiter, auch ueber einen Neustart.
///
/// # Errors
/// `Halt` bei Kurse, die nicht erfunden sind, bei einem angefangenen Tag im Journal, bei
/// einem Journal, das nicht zum Szenario passt, bei einer Abweichung zum Konto, bei einem
/// Befund des Waechters und bei jedem Fehler des Journals.
pub fn lauf(
    j: &mut Journal,
    s: &Szenario,
    dachs: &mut dyn Berater,
    maus: &mut dyn Berater,
    konto: &mut dyn Broker,
) -> Result<(), LaufFehler> {
    nur_simulation(s)?;
    // Der letzte Abschluss muss genau die Vorgeschichte dieses Szenarios abdrucken (K15-01).
    // Dann sind Zyklusnummern, Vergleichskurse und Stand bis dahin dieselben wie beim ersten
    // Lauf; nur die Tage danach duerfen anders sein.
    let abdruecke = abdruck::je_tag(s);
    let fertig = match j.stand().abschluss() {
        None => None,
        Some(a) => {
            let index = s
                .tage
                .iter()
                .zip(&abdruecke)
                .position(|(t, ab)| t.datum == a.datum && *ab == a.abdruck)
                .ok_or_else(|| {
                    halt(
                        "Vorgeschichte weicht ab: Das Szenario ergibt bis zum letzten Abschluss \
                         nicht denselben Abdruck. Halt; niemand wird gefragt",
                    )
                })?;
            Some((index, a.hoechststand))
        }
    };
    let neu = fertig.is_none() && j.stand().seit_abschluss() == 0;
    if !neu {
        // Wiederanlauf: zuerst der Abgleich (Kap. 11). Er belegt, was aus offenen Auftraegen
        // geworden ist, und sendet nichts.
        pruefe_stoerung(&*konto)?;
        let abweichungen = j.gleiche_ab(&*konto).map_err(journal)?;
        if !abweichungen.is_empty() {
            return Err(halt(format!(
                "Kontowächter beim Wiederanlauf: {abweichungen:?}. Halt bis zur Prüfung; \
                 niemand wird gefragt, nichts gesendet"
            )));
        }
    }
    let st = j.stand();
    if st.seit_abschluss() > 0 {
        return Err(halt(format!(
            "{} Einträge nach dem letzten Abschluss: ein Tag ist angefangen und nicht \
             abgeschlossen. Halt bis zur Prüfung; niemand wird neu gefragt",
            st.seit_abschluss()
        )));
    }
    // Ein abgeschlossenes Journal im Format 1 wird umgestellt, bevor neue Auftraege entstehen
    // (K16-02). Ein neues Journal hat schon Format 2.
    j.stelle_um().map_err(journal)?;
    let grenzen = Grenzen::standard();
    let mut hoch = if let Some((_, h)) = fertig {
        h
    } else {
        for t in &s.start {
            j.setze_topf(t).map_err(journal)?;
        }
        depotwert(s.start.iter(), &s.vorlauf, Rundung::Auf).ok_or_else(|| rechnung("Startwert"))?
    };
    let mut vortag = &s.vorlauf;
    let mut beteiligte = Beteiligte {
        dachs,
        maus,
        konto,
        mindesteinsatz_bp: 0,
        anlass_pruefen: false,
    };
    for (i, (tag, abdruck)) in s.tage.iter().zip(&abdruecke).enumerate() {
        let nr = u32::try_from(i)
            .ok()
            .and_then(|z| z.checked_add(1))
            .ok_or_else(|| rechnung("Zyklus"))?;
        let fehler = pruefe_kurse(&grenzen, &s.universum, tag.datum, &tag.kurse, vortag);
        if fertig.is_none_or(|(index, _)| i > index) {
            let z = Zyklus {
                anlass_pruefen: false,
                grenzen: &grenzen,
                universum: &s.universum,
                tag,
                vortag,
                nr,
                abdruck,
                uhr: None,
                vollstaendige_kurse: false,
                gesperrt: &BTreeSet::new(),
                bewertung: &tag.kurse,
            };
            hoch = ein_tag(j, &z, hoch, &fehler, &mut beteiligte)?;
        }
        // Vergleichskurs ist der letzte plausible Tag: Ein gestoerter Tag verschiebt ihn nicht.
        if fehler.is_empty() {
            vortag = &tag.kurse;
        }
    }
    Ok(())
}

/// Halt bei einer Stoerung des Kontos, ohne weiteren Eintrag im Journal (K23-01). Was das
/// Journal bis dahin hat, klaert der naechste Lauf ueber den Abgleich.
pub(crate) fn pruefe_stoerung(konto: &dyn Broker) -> Result<(), LaufFehler> {
    match konto.stoerung() {
        Some(s) => Err(halt(format!(
            "Konto gestört: {s}. Halt; nichts wird mehr bestätigt"
        ))),
        None => Ok(()),
    }
}

/// Der Kontowaechter: Abgleich mit dem Konto. Jede Abweichung steht im Journal und haelt den
/// Lauf an; nichts wird verkauft oder ausgeglichen.
pub(crate) fn kontowaechter(
    j: &mut Journal,
    tag: &Tag,
    konto: &dyn Broker,
) -> Result<(), LaufFehler> {
    pruefe_stoerung(konto)?;
    let abweichungen = j.gleiche_ab(konto).map_err(journal)?;
    if abweichungen.is_empty() {
        return Ok(());
    }
    for a in &abweichungen {
        begruende(j, tag, KONTOWAECHTER, &format!("{a:?}"))?;
    }
    begruende(
        j,
        tag,
        KONTOWAECHTER,
        "Abweichung zum Konto: Halt bis zur Prüfung; kein Verkauf, kein Ausgleich",
    )?;
    Err(halt(format!("Kontowächter: {abweichungen:?}")))
}

fn fuehre_schutzstopps_aus(
    j: &mut Journal,
    z: &Zyklus<'_>,
    konto: &mut dyn Broker,
) -> Result<BTreeSet<TopfId>, LaufFehler> {
    let mut ausgeloest = BTreeSet::new();
    if !j
        .stand()
        .wissensstichtag(z.tag.datum)
        .is_some_and(depot_kern::liste::morgenlauf)
    {
        return Ok(ausgeloest);
    }
    let toepfe: Vec<_> = j.stand().toepfe().values().cloned().collect();
    for topf in &toepfe {
        let stopps = topf
            .positionen()
            .keys()
            .filter(|isin| !z.gesperrt.contains(*isin))
            .filter(|isin| {
                depot_kern::liste::boerse_von(isin).is_some_and(|b| b.handelstag(z.tag.datum))
            })
            .filter(|isin| {
                !j.stand()
                    .offene_auftraege()
                    .any(|a| a.beleg.topf == topf.id() && &a.beleg.isin == *isin)
            })
            .filter_map(|isin| Some((isin.clone(), j.stand().stoppkurs(topf.id(), isin)?)))
            .collect();
        let auftraege = stopp_auftraege(
            z.grenzen,
            topf,
            &z.tag.kurse,
            &stopps,
            &mut j.kennungsvergabe(),
        )
        .ok_or_else(|| rechnung("Schutz-Stopp"))?;
        if !auftraege.is_empty() {
            ausgeloest.insert(topf.id());
        }
        for auftrag in &auftraege {
            begruende(
                j,
                z.tag,
                REGELWERK,
                &format!(
                    "{}: Schutz-Stopp für {} ausgelöst",
                    topf_name(topf.id()),
                    auftrag.auftrag().isin
                ),
            )?;
        }
        fuehre_aus(j, z, topf.id(), &auftraege, None, konto)?;
    }
    Ok(ausgeloest)
}

pub(crate) struct Beteiligte<'a, 'b, 'c> {
    pub(crate) dachs: &'a mut dyn Berater,
    pub(crate) maus: &'b mut dyn Berater,
    pub(crate) konto: &'c mut dyn Broker,
    pub(crate) mindesteinsatz_bp: i64,
    pub(crate) anlass_pruefen: bool,
}

/// Was fuer einen Tag fest ist.
pub(crate) struct Zyklus<'a> {
    pub(crate) anlass_pruefen: bool,
    pub(crate) grenzen: &'a Grenzen,
    /// Was gehandelt werden darf (Positivliste).
    pub(crate) universum: &'a Universum,
    pub(crate) tag: &'a Tag,
    /// Die Kurse des letzten plausiblen Tages davor.
    pub(crate) vortag: &'a Kurse,
    /// Zyklusnummer fuer den Kostenschutz; sie steigt von Tag zu Tag.
    pub(crate) nr: u32,
    /// Abdruck der Vorgeschichte bis einschliesslich dieses Tages.
    pub(crate) abdruck: &'a str,
    /// Reale Versandzeit im Papiertag. Die historische Beispielszenario-Simulation hat keine Uhr.
    pub(crate) uhr: Option<&'a dyn Fn() -> Option<Zeitpunkt>>,
    pub(crate) vollstaendige_kurse: bool,
    pub(crate) gesperrt: &'a BTreeSet<Isin>,
    pub(crate) bewertung: &'a Kurse,
}

/// Ein Tag. Liefert den Hoechststand nach dem Tag.
pub(crate) fn ein_tag(
    jr: &mut Journal,
    z: &Zyklus<'_>,
    hoch: Cent,
    fehler: &[DatenFehler],
    beteiligte: &mut Beteiligte<'_, '_, '_>,
) -> Result<Cent, LaufFehler> {
    let tag = z.tag;
    for ereignis in &tag.ereignisse {
        jr.beobachte(tag.datum, &sauber(ereignis))
            .map_err(journal)?;
    }
    jr.beginne_abrechnung(tag.datum).map_err(journal)?;
    kontowaechter(jr, tag, &*beteiligte.konto)?;
    beteiligte.konto.rechne_ab(tag.datum);
    pruefe_stoerung(&*beteiligte.konto)?;
    if z.uhr.is_some()
        && (pruefe_papier_verlustsperre(jr, z, hoch, fehler.is_empty())?
            || jr.stand().notbremse_aktiv())
    {
        return papier_notbremse_fortsetzen(jr, z, hoch, &mut *beteiligte.konto, fehler.is_empty());
    }
    if !fehler.is_empty() {
        if z.uhr.is_some() {
            markt_offener_orders(jr, z, &mut *beteiligte.konto, false)?;
        }
        for f in fehler {
            begruende(jr, tag, DATEN, &format!("{f:?}"))?;
        }
        begruende(
            jr,
            tag,
            DATEN,
            "Zyklus faellt aus: niemand wird gefragt, kein Auftrag",
        )?;
        jr.schliesse_ab(tag.datum, hoch, z.abdruck)
            .map_err(journal)?;
        return Ok(hoch);
    }
    let toepfe: Vec<Topf> = jr.stand().toepfe().values().cloned().collect();
    let heute = depotwert(toepfe.iter(), z.bewertung, Rundung::Ab);
    let befund = match (heute, depotwert(toepfe.iter(), z.vortag, Rundung::Auf)) {
        (Some(wert), Some(gestern)) => waechter(z.grenzen, wert, gestern, hoch),
        _ => WaechterBefund::Unplausibel,
    };
    match befund {
        WaechterBefund::InOrdnung => {}
        WaechterBefund::Notbremse { .. } if z.uhr.is_some() => {
            return papier_notbremse(jr, z, hoch, &mut *beteiligte.konto, true);
        }
        WaechterBefund::Notbremse { .. } => {
            begruende(
                jr,
                tag,
                WAECHTER,
                &format!("{befund:?}: alles wird verkauft, dann Halt bis zur Freigabe"),
            )?;
            for topf in &toepfe {
                let mut vergabe = jr.kennungsvergabe();
                let auftraege = notbremse_auftraege(z.grenzen, topf, &tag.kurse, &mut vergabe)
                    .ok_or_else(|| halt("Notbremse: Kurs fehlt; Halt ohne Verkauf"))?;
                fuehre_aus(jr, z, topf.id(), &auftraege, None, &mut *beteiligte.konto)?;
            }
            return Err(halt(format!("Wächter: {befund:?}")));
        }
        WaechterBefund::Tagesverlust { .. } | WaechterBefund::Unplausibel => {
            return halte_mit_schutzstopps(jr, z, &mut *beteiligte.konto, befund);
        }
    }
    if z.uhr.is_some() {
        markt_offener_orders(jr, z, &mut *beteiligte.konto, true)?;
        if pruefe_papier_verlustsperre(jr, z, hoch, true)? {
            return papier_notbremse(jr, z, hoch, &mut *beteiligte.konto, true);
        }
    }
    pruefe_topfgewichtung(jr, z)?;
    let gestoppt = fuehre_schutzstopps_aus(jr, z, &mut *beteiligte.konto)?;
    // Fuer den Hoechststand zaehlt der aufgerundete Wert (siehe `depotwert`).
    let heute = depotwert(jr.stand().toepfe().values(), z.bewertung, Rundung::Auf)
        .ok_or_else(|| rechnung("Depotwert"))?;
    let b = beteiligte;
    let topf_b = frage_topf(jr, z, TopfId::B, b.maus, &gestoppt)?;
    let topf_a = frage_topf(jr, z, TopfId::A, b.dachs, &gestoppt)?;
    let erste = [topf_a.clone(), topf_b.clone()];
    let topf_b = zweite_meinung(jr, z, topf_b, b.dachs)?;
    let topf_a = zweite_meinung(jr, z, topf_a, b.maus)?;
    let mut antworten = crate::abstimmung::berate(jr, z, erste, [topf_a, topf_b], b.dachs, b.maus)?;
    if b.dachs.kostensperre() || b.maus.kostensperre() {
        for antwort in &mut antworten {
            antwort.verwerfe_ersatzkauf();
        }
    }
    beide_topfzyklen(jr, z, antworten, b)?;
    let hoch = if z.gesperrt.is_empty() {
        hoch.max(heute)
    } else {
        hoch
    };
    belege_schatten(jr, z)?;
    jr.schliesse_ab(tag.datum, hoch, z.abdruck)
        .map_err(journal)?;
    Ok(hoch)
}

fn pruefe_topfgewichtung(j: &mut Journal, z: &Zyklus<'_>) -> Result<(), LaufFehler> {
    use crate::topfgewichtung::{BELEG, faellig, umbuchen};
    if z.uhr.is_none() || !z.gesperrt.is_empty() || !faellig(j.stand()) {
        return Ok(());
    }
    let frei = [
        kaufbudget(j, z.grenzen, TopfId::A)?,
        kaufbudget(j, z.grenzen, TopfId::B)?,
    ];
    match umbuchen(j.stand(), z.bewertung, z.grenzen, frei) {
        Ok(betrag) if betrag != Cent::NULL => {
            j.buche_topfgeld(z.tag.datum, betrag).map_err(journal)?;
            begruende(j, z.tag, BELEG, &format!("Bargeldzugang Dachs: {betrag}"))
        }
        Ok(_) => begruende(j, z.tag, BELEG, "Keine Änderung der Bargeldzuordnung."),
        Err(grund) => begruende(j, z.tag, BELEG, &format!("Keine Umbuchung. {grund}.")),
    }
}

fn belege_schatten(j: &mut Journal, z: &Zyklus<'_>) -> Result<(), LaufFehler> {
    let Some(wissen) = j.stand().wissensstichtag(z.tag.datum) else {
        return Ok(());
    };
    let mut schatten =
        crate::schatten::Schattenstand::letzter(j.stand(), Some(z.tag.datum)).unwrap_or_default();
    schatten.fortschreiben(
        j.stand(),
        z.grenzen,
        z.universum,
        z.tag,
        wissen,
        crate::schatten::Variante::default(),
    );
    begruende(j, z.tag, crate::schatten::BELEG, &schatten.text())
}

fn halte_mit_schutzstopps(
    j: &mut Journal,
    z: &Zyklus<'_>,
    konto: &mut dyn Broker,
    befund: WaechterBefund,
) -> Result<Cent, LaufFehler> {
    if z.uhr.is_some() {
        stoppe_kaeufe(j, z.tag, konto)?;
        fuehre_schutzstopps_aus(j, z, konto)?;
    }
    begruende(
        j,
        z.tag,
        WAECHTER,
        &format!("{befund:?}: Halt bis zur Freigabe, der Tag bleibt offen"),
    )?;
    Err(halt(format!("Wächter: {befund:?}")))
}

/// Auch ein langer Dialog darf die bestehende Datenfrische nicht verlängern.
pub(crate) fn beratungszeit(z: &Zyklus<'_>) -> bool {
    let Some(uhr) = z.uhr else {
        return true;
    };
    let Some(jetzt) = uhr() else {
        return false;
    };
    jetzt.am_tag(z.tag.datum)
        && jetzt >= z.tag.kurse.stand()
        && z.universum
            .isins()
            .iter()
            .filter(|i| !z.gesperrt.contains(*i))
            .all(|isin| {
                z.tag
                    .kurse
                    .abruf(isin)
                    .and_then(|abruf| jetzt.sekunden_seit(abruf))
                    .is_some_and(|alter| alter <= z.grenzen.kurs_hoechstalter_s())
            })
}

fn beide_topfzyklen(
    j: &mut Journal,
    z: &Zyklus<'_>,
    antworten: [TopfAntwort; 2],
    beteiligte: &mut Beteiligte<'_, '_, '_>,
) -> Result<(), LaufFehler> {
    if !beratungszeit(z) {
        return begruende(
            j,
            z.tag,
            DATEN,
            "Beratung beendet: Datenfrische abgelaufen oder Uhr unklar. Keine neuen Aufträge.",
        );
    }
    let gesperrt = beteiligte.dachs.kostensperre() || beteiligte.maus.kostensperre();
    let [a, b] = antworten;
    let minimum = if gesperrt {
        0
    } else {
        beteiligte.mindesteinsatz_bp
    };
    topf_zyklus(j, z, a, beteiligte.konto, beteiligte.dachs, minimum)?;
    let minimum = if beteiligte.dachs.kostensperre() || beteiligte.maus.kostensperre() {
        0
    } else {
        minimum
    };
    topf_zyklus(j, z, b, beteiligte.konto, beteiligte.maus, minimum)?;
    Ok(())
}

fn stoppe_kaeufe(j: &mut Journal, tag: &Tag, konto: &mut dyn Broker) -> Result<(), LaufFehler> {
    if j.stand()
        .offene_auftraege()
        .any(|l| l.beleg.richtung == Richtung::Kauf)
    {
        if !konto.stoppe_kaeufe() {
            return Err(halt("Offene Käufe konnten nicht sicher storniert werden"));
        }
        kontowaechter(j, tag, &*konto)?;
        if j.stand()
            .offene_auftraege()
            .any(|l| l.beleg.richtung == Richtung::Kauf)
        {
            return Err(halt("Kauf nach Storno noch offen"));
        }
    }
    Ok(())
}

/// Prueft vor der Marktverarbeitung und nach Ausfuehrungen inklusive Gebuehren.
/// Ohne gepruefte Kurse laesst sich Bestand nicht bewerten; der Lauf sperrt Kaeufe an diesem Tag ohnehin.
fn pruefe_papier_verlustsperre(
    j: &mut Journal,
    z: &Zyklus<'_>,
    hoch: Cent,
    kurse_ok: bool,
) -> Result<bool, LaufFehler> {
    if j.stand().papier_verlustsperre_aktiv() {
        return Ok(true);
    }
    let stand = j.stand();
    let wert = if kurse_ok || stand.gehalten().is_empty() {
        depotwert(stand.toepfe().values(), z.bewertung, Rundung::Ab)
    } else {
        None
    };
    if wert.is_some_and(|wert| stand.papier_gesamtverlust_erreicht(wert, Some(hoch)) == Some(true))
    {
        j.belege_papier_halt(z.tag.datum, PapierHalt::Gesamtverlust)
            .map_err(journal)?;
        return Ok(true);
    }
    Ok(false)
}

fn papier_notbremse_fortsetzen(
    j: &mut Journal,
    z: &Zyklus<'_>,
    hoch: Cent,
    konto: &mut dyn Broker,
    kurse_ok: bool,
) -> Result<Cent, LaufFehler> {
    if !j.stand().papier_verlustsperre_aktiv()
        && j.stand().prueft_wiederanlauf()
        && kurse_ok
        && z.vollstaendige_kurse
    {
        return j
            .schliesse_wiederanlauf_ab(z.tag.datum, z.abdruck)
            .map_err(journal);
    }
    papier_notbremse(j, z, hoch, konto, kurse_ok)
}

fn papier_notbremse(
    j: &mut Journal,
    z: &Zyklus<'_>,
    hoch: Cent,
    konto: &mut dyn Broker,
    kurse_ok: bool,
) -> Result<Cent, LaufFehler> {
    stoppe_kaeufe(j, z.tag, konto)?;
    if !j.stand().notbremse_aktiv() {
        j.belege_papier_halt(z.tag.datum, PapierHalt::Notbremse)
            .map_err(journal)?;
    }
    markt_offener_orders(j, z, konto, kurse_ok)?;
    pruefe_papier_verlustsperre(j, z, hoch, kurse_ok)?;
    if kurse_ok {
        erneuere_schutzverkaeufe(j, z, konto)?;
    }
    j.schliesse_ab(z.tag.datum, hoch, z.abdruck)
        .map_err(journal)?;
    Err(halt(
        j.stand()
            .papier_halt()
            .unwrap_or(PapierHalt::Notbremse)
            .meldung(),
    ))
}

fn erneuere_schutzverkaeufe(
    j: &mut Journal,
    z: &Zyklus<'_>,
    konto: &mut dyn Broker,
) -> Result<(), LaufFehler> {
    let toepfe: Vec<_> = j.stand().toepfe().values().cloned().collect();
    for mut topf in toepfe {
        let topf_id = topf.id();
        // Bereits beauftragte Verkaeufe bleiben bestehen und binden ihren offenen Rest.
        for l in j
            .stand()
            .offene_auftraege()
            .filter(|l| l.beleg.topf == topf_id)
        {
            let frei = topf
                .stueck(&l.beleg.isin)
                .checked_sub(l.offen_stueck())
                .ok_or_else(|| rechnung("Offener Verkauf"))?;
            topf.setze_bestand(l.beleg.isin.clone(), frei);
        }
        for isin in z.gesperrt {
            if topf.stueck(isin) != 0 {
                topf.setze_bestand(isin.clone(), 0);
                begruende(
                    j,
                    z.tag,
                    WAECHTER,
                    &format!(
                        "Notbremse: {isin} heute gesperrt, Verkauf beim nächsten Lauf mit gültigem Kurs."
                    ),
                )?;
            }
        }
        let mut vergabe = j.kennungsvergabe();
        let auftraege = notbremse_auftraege(z.grenzen, &topf, &z.tag.kurse, &mut vergabe)
            .ok_or_else(|| halt("Notbremse: Kurs fehlt; Halt ohne Verkauf"))?;
        fuehre_aus(j, z, topf.id(), &auftraege, None, konto)?;
    }
    Ok(())
}

fn markt_offener_orders(
    j: &mut Journal,
    z: &Zyklus<'_>,
    konto: &mut dyn Broker,
    kurse_ok: bool,
) -> Result<(), LaufFehler> {
    let mut gruppen = BTreeMap::<_, BTreeSet<_>>::new();
    for order in j.stand().offene_auftraege() {
        if let Some(schluss) = order.frist {
            gruppen.entry(schluss).or_default().insert(order.beleg.nr);
        }
    }
    if gruppen.is_empty() || !kurse_ok {
        return Ok(());
    }
    let stand = z
        .uhr
        .and_then(|uhr| uhr())
        .filter(|zeit| *zeit >= z.tag.kurse.stand())
        .ok_or_else(|| halt("Keine verlässliche Marktzeit"))?;
    let wissen = j
        .stand()
        .wissensstichtag(z.tag.datum)
        .unwrap_or(stand)
        .min(stand);
    let handelbar = z.universum.isins();
    for (schluss, nummern) in gruppen {
        let mut eintraege = BTreeMap::new();
        let mut verfallen = BTreeSet::new();
        let mut nachholfrist_abgelaufen = BTreeSet::new();
        for nr in &nummern {
            let Some(order) = j.stand().auftrag(*nr) else {
                continue;
            };
            let isin = &order.beleg.isin;
            let im_zeitfenster = depot_kern::liste::boerse_von(isin).is_some_and(|boerse| {
                let fruehestens = boerse
                    .bezugstag(z.tag.datum)
                    .and_then(|letzter| boerse.bezugstag(letzter));
                schluss
                    .frankfurt_tag()
                    .zip(fruehestens)
                    .is_some_and(|(tag, fruehestens)| tag >= fruehestens)
            });
            if !handelbar.contains(isin) || z.gesperrt.contains(isin) {
                if !im_zeitfenster && stand >= schluss {
                    nachholfrist_abgelaufen.insert(*nr);
                    verfallen.insert(*nr);
                }
                continue;
            }
            let aktuell = z.tag.kurse.eintrag(isin);
            let beleg = j.stand().kurse().iter().rev().find(|b| {
                im_zeitfenster
                    && b.isin() == isin
                    && b.abruf() <= wissen
                    && b.kurs().herkunft() == Herkunft::Quelle
                    && b.kurs().zeitpunkt() == schluss
            });
            if let Some(beleg) = beleg {
                eintraege.insert(isin.clone(), (beleg.kurs().clone(), beleg.abruf()));
            } else if stand >= schluss && aktuell.is_some_and(|kurs| kurs.zeitpunkt() > schluss) {
                verfallen.insert(*nr);
            }
        }
        let kurse = Kurse::mit_abrufen(
            stand,
            eintraege
                .into_iter()
                .map(|(isin, (kurs, abruf))| (isin, kurs, abruf))
                .collect(),
        )
        .map_err(|e| halt(format!("Kurse für offene Orders: {e:?}")))?;
        konto.markt_fuer(&kurse, &nummern);
        if !verfallen.is_empty() {
            konto.verfalle(&verfallen);
        }
        kontowaechter(j, z.tag, &*konto)?;
        let erledigt: Vec<_> = nummern
            .into_iter()
            .filter(|nr| j.stand().auftrag(*nr).is_some_and(|a| !a.zustand.offen()))
            .collect();
        protokolliere_ausfuehrungen(
            j,
            z.tag,
            &erledigt,
            Some((&kurse, &nachholfrist_abgelaufen)),
        )?;
    }
    Ok(())
}

pub(crate) fn belege_ki(
    j: &mut Journal,
    berater: &mut dyn Berater,
) -> Result<(), depot_journal::JournalFehler> {
    if let Some(aufruf) = berater.aufruf() {
        j.belege_ki_aufruf(&aufruf)?;
    }
    if let Some(abrechnung) = berater.abrechnung() {
        j.rechne_ki_ab(&abrechnung)?;
    }
    Ok(())
}

/// Fragt einen Berater, wenn der Kostenschutz es erlaubt. `None` heisst: nicht gefragt; der
/// Grund steht im Journal.
fn erlaubnis(
    j: &mut Journal,
    tag: &Tag,
    zyklus: u32,
    topf: TopfId,
    berater: BeraterId,
    folge: &str,
    ki: &dyn Berater,
) -> Result<Option<depot_journal::AufrufErlaubnis>, LaufFehler> {
    let zweck = if folge == "kein Vorschlag" {
        depot_journal::KiZweck::Erstmeinung
    } else {
        depot_journal::KiZweck::Zweitmeinung
    };
    match j.reserviere_fuer(berater, tag.datum, zyklus, zweck) {
        Ok(e) => Ok(Some(e)),
        Err(Sperre::Kassenbuch(e)) => Err(halt(format!("Kassenbuch: {e}"))),
        Err(sperre) => {
            ki.anfrage_uebersprungen(zweck, &format!("Kostenschutz: {sperre:?}"));
            begruende(
                j,
                tag,
                KOSTENSCHUTZ,
                &format!(
                    "{}: {} nicht gefragt: {sperre:?}; {folge}",
                    topf_name(topf),
                    name(berater)
                ),
            )?;
            Ok(None)
        }
    }
}

/// Fragt den zustaendigen Berater nach einem Vorschlag, wenn er angebunden ist und der
/// Kostenschutz es erlaubt. Einschaetzung und Begruendungen stehen danach im Journal.
fn frage(
    j: &mut Journal,
    z: &Zyklus<'_>,
    id: TopfId,
    frager: &mut dyn Berater,
    lage: &Lage<'_>,
) -> Result<(Option<Vorschlag>, bool, BTreeSet<Isin>), LaufFehler> {
    let (tag, zyklus) = (z.tag, z.nr);
    let wer = id.berater();
    let t = topf_name(id);
    let mut vorschlag: Option<Vorschlag> = None;
    let mut verwertbar = false;
    let mut fristverkaeufe = BTreeSet::new();
    // Ein Berater ohne Anbindung wird nicht gefragt und kostet nichts: Fuer ihn wird keine
    // Erlaubnis reserviert. Ohne verwendbaren Vorschlag plant das Regelwerk nur zwingende Aufträge.
    if !frager.angebunden() {
        let grund = nicht_angebunden(frager);
        frager.anfrage_uebersprungen(depot_journal::KiZweck::Erstmeinung, &grund);
        begruende(
            j,
            tag,
            name(wer),
            &format!("{t}: {grund}, nicht gefragt; kein Vorschlag"),
        )?;
    } else if let Some(e) = erlaubnis(j, tag, zyklus, id, wer, "kein Vorschlag", frager)? {
        let mut antwort = frager.vorschlagen_mit_bilanz(e, lage, &prognosebilanz(j.stand()));
        belege_ki(j, frager).map_err(journal)?;
        verwertbar = antwort.verwertbar;
        let wie = if antwort.verwertbar {
            if let Some(pruefung) = frager.thesenpruefung() {
                crate::schlopfmodell::belege_pruefung(j, tag.datum, wer, &pruefung)
                    .map_err(journal)?;
            }
            if z.uhr.is_some()
                && let Some(prognose) = antwort.prognose.take()
                && z.universum.isins().contains(&prognose.isin)
            {
                belege_prognose(j, lage, wer, prognose)?;
            }
            ANTWORT_VERWENDET
        } else {
            // Eine verworfene Antwort traegt nie einen Vorschlag weiter.
            antwort.vorschlag = None;
            ANTWORT_VERWORFEN
        };
        begruende(
            j,
            tag,
            name(wer),
            &format!("{t}: {wie}: {}", antwort.einschaetzung),
        )?;
        if z.uhr.is_some() && verwertbar {
            fristverkaeufe = pruefe_fristen(j, tag, lage, &antwort)?;
        }
        crate::ausgelassen::belege_ausgelassene(j, z, lage, &mut antwort.vorschlag, name(wer))?;
        if let Some(v) = &antwort.vorschlag {
            for (isin, bp) in &v.zielgewichte_bp {
                let grund = v.begruendung.get(isin).map_or("", String::as_str);
                begruende(j, tag, name(wer), &format!("{t}: {isin} {bp} bp: {grund}"))?;
            }
            if let Some(p) = &v.park {
                begruende(
                    j,
                    tag,
                    name(wer),
                    &format!(
                        "{t}: Parkwert {}, Reserve {} bp, nächster Kauf frühestens in {} Handelstagen: {}",
                        p.isin, p.reserve_bp, p.naechster_kauf_tage, p.begruendung
                    ),
                )?;
            }
        }
        if fristverkaeufe.is_empty() {
            vorschlag = antwort.vorschlag;
        }
    }
    Ok((vorschlag, verwertbar, fristverkaeufe))
}

fn pruefe_fristen(
    j: &mut Journal,
    tag: &Tag,
    lage: &Lage<'_>,
    antwort: &crate::Antwort,
) -> Result<BTreeSet<Isin>, LaufFehler> {
    let mut verkaeufe = BTreeSet::new();
    for kauf in lage
        .positionskaeufe
        .iter()
        .filter(|kauf| kauf.frist_abgelaufen(lage.datum))
    {
        let expliziter_verkauf = antwort
            .vorschlag
            .as_ref()
            .and_then(|v| v.zielgewichte_bp.get(&kauf.isin))
            == Some(&0);
        let halten = antwort
            .halten
            .get(&kauf.isin)
            .filter(|grund| !grund.trim().is_empty());
        if let Some(grund) = halten.filter(|_| !expliziter_verkauf) {
            begruende(
                j,
                tag,
                "Haltefrist",
                &format!(
                    "{} hält {} weiter: {grund}",
                    topf_name(lage.topf.id()),
                    kauf.isin
                ),
            )?;
        } else {
            verkaeufe.insert(kauf.isin.clone());
        }
    }
    Ok(verkaeufe)
}

fn belege_prognose(
    j: &mut Journal,
    lage: &Lage<'_>,
    berater: BeraterId,
    prognose: Weltprognose,
) -> Result<(), LaufFehler> {
    let ereignis = prognose
        .ereignis_nr
        .checked_sub(1)
        .and_then(|nr| lage.ereignisse.get(nr));
    if let (Some(ereignis), Some(ausgangskurs)) = (ereignis, lage.kurse.kurs(&prognose.isin))
        && prognose.gueltig()
    {
        match j.belege_weltprognose(&PrognoseBeleg {
            datum: lage.datum,
            berater,
            prognose,
            ausgangskurs,
            ereignis: sauber(ereignis),
        }) {
            Ok(()) => {}
            Err(JournalFehler::Abgelehnt(grund)) => j
                .begruende(
                    lage.datum,
                    name(berater),
                    &format!("Prognose verworfen: {grund}"),
                )
                .map_err(journal)?,
            Err(e) => return Err(journal(e)),
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Erstmeinung {
    Ruhetag,
    Unverwertbar,
    Verwertbar,
}

#[derive(Clone)]
pub(crate) struct TopfAntwort {
    pub(crate) topf: Topf,
    pub(crate) vorschlag: Option<Vorschlag>,
    pub(crate) erstmeinung: Erstmeinung,
    zweitmeinung_ausgefallen: bool,
    ersatzkauf: bool,
    fristverkaeufe: BTreeSet<Isin>,
    /// Verfallene KI-Kauforder, die heute ohne KI wiederholt wird (#311 H2b).
    wiederholung: Option<depot_regelwerk::Wiederholung>,
}

impl TopfAntwort {
    pub(crate) fn ersatzkauf_offen(&self) -> bool {
        self.ersatzkauf
    }

    pub(crate) fn verwerfe_ersatzkauf(&mut self) {
        if self.ersatzkauf {
            self.vorschlag = None;
            self.ersatzkauf = false;
        }
    }
}

pub(crate) fn prognosebilanz(
    stand: &depot_journal::Stand,
) -> Vec<(BeraterId, depot_journal::Auswertung)> {
    [BeraterId::Dachs, BeraterId::Maus]
        .into_iter()
        .filter_map(|wer| {
            depot_journal::werte_aus(stand.weltprognosen(), stand.aufloesungen(), Some(wer))
                .map(|auswertung| (wer, auswertung))
        })
        .collect()
}

pub(crate) fn kaufbudget(j: &Journal, g: &Grenzen, id: TopfId) -> Result<Cent, LaufFehler> {
    depot_journal::konto::verfuegbar(g, j.stand(), id).ok_or_else(|| {
        halt(format!(
            "{}: freies Bargeld nicht berechenbar",
            topf_name(id)
        ))
    })
}

/// Was der Morgenlauf für einen Topf ohne Sperre und ohne offenen Auftrag tut.
enum Heute {
    /// Kein Anlass: keine KI, höchstens die Wiederholung einer verfallenen Kauforder.
    Ruhetag(Option<depot_regelwerk::Wiederholung>),
    /// Die KI wird gefragt, mit diesem Anlass.
    Fragen(Option<String>),
}

fn anlass_heute(
    j: &mut Journal,
    z: &Zyklus<'_>,
    t: &str,
    lage: &Lage<'_>,
) -> Result<Heute, LaufFehler> {
    let tag = z.tag;
    let mut anlass = if z.anlass_pruefen {
        crate::anlass::anlass(j.stand(), z.universum, lage)
    } else if z.uhr.is_some() {
        Some("Papiertag von Hand".into())
    } else {
        None
    };
    let pruefung = if z.anlass_pruefen && anlass.is_none() {
        crate::wiederholung::pruefe(j.stand(), z.grenzen, lage)
    } else {
        None
    };
    if let Some(crate::wiederholung::Pruefung::Anlass(grund)) = &pruefung {
        anlass = Some(grund.clone());
    }
    if !z.anlass_pruefen || anlass.is_some() {
        return Ok(Heute::Fragen(anlass));
    }
    begruende(
        j,
        tag,
        SIMULATION,
        &format!("{t}: {}", crate::anlass::RUHETAG),
    )?;
    let Some(crate::wiederholung::Pruefung::Wiederholen { nr, kauf }) = pruefung else {
        return Ok(Heute::Ruhetag(None));
    };
    begruende(
        j,
        tag,
        AUFTRAGSSTELLE,
        &format!(
            "{t}: Wiederholung von Auftrag {nr}: {} Stück {}, Limitabstand {} bp, ohne neue KI-Antwort",
            kauf.stueck, kauf.isin, kauf.limit_bp
        ),
    )?;
    Ok(Heute::Ruhetag(Some(kauf)))
}

fn frage_topf(
    j: &mut Journal,
    z: &Zyklus<'_>,
    id: TopfId,
    frager: &mut dyn Berater,
    gestoppt: &BTreeSet<TopfId>,
) -> Result<TopfAntwort, LaufFehler> {
    let tag = z.tag;
    let topf = j
        .stand()
        .toepfe()
        .get(&id)
        .cloned()
        .ok_or_else(|| halt(format!("{} fehlt im Journal", topf_name(id))))?;
    let t = topf_name(id);
    let positionskaeufe = crate::positionskauf::lies(j.stand(), &topf);
    let verlauf = crate::kursverlauf::fuer_lage(j.stand(), &tag.kurse, z.universum, tag.datum);
    let relative_staerke =
        crate::relative_staerke::fuer_lage(j.stand(), &tag.kurse, z.universum, tag.datum);
    let mut lage = Lage {
        anlass: None,
        datum: tag.datum,
        topf: &topf,
        kaufbar: kaufbudget(j, z.grenzen, id)?,
        kurse: &tag.kurse,
        verlauf: &verlauf,
        kaufbilanz: crate::ausfuehrung::kaufbilanz(j.stand(), topf.id(), tag.datum),
        relative_staerke: &relative_staerke,
        positionskaeufe: &positionskaeufe,
        ereignisse: &tag.ereignisse,
    };
    let wartet = j
        .stand()
        .offene_auftraege()
        .find(|l| l.versand.is_some() && l.beleg.topf == id)
        .map(|l| l.beleg.nr);
    let gesperrt = z.gesperrt.iter().find(|isin| topf.stueck(isin) != 0);
    let mut wiederholung = None;
    let (vorschlag, erstmeinung, fristverkaeufe) = if let Some(isin) = gesperrt {
        begruende(
            j,
            tag,
            AUFTRAGSSTELLE,
            &format!("{t}: {isin} heute gesperrt; keine freiwilligen Aufträge, KI nicht gefragt."),
        )?;
        (None, Erstmeinung::Unverwertbar, BTreeSet::new())
    } else if let Some(nr) = wartet {
        begruende(
            j,
            tag,
            AUFTRAGSSTELLE,
            &format!("{t}: wartet auf offenen Auftrag {nr}"),
        )?;
        (None, Erstmeinung::Unverwertbar, BTreeSet::new())
    } else if gestoppt.contains(&id) {
        // Auch nach abgewiesenem Schutzverkauf keine neuen Käufe für diesen Topf.
        (None, Erstmeinung::Unverwertbar, BTreeSet::new())
    } else {
        match anlass_heute(j, z, t, &lage)? {
            Heute::Ruhetag(w) => {
                wiederholung = w;
                (None, Erstmeinung::Ruhetag, BTreeSet::new())
            }
            Heute::Fragen(anlass) => {
                if let Some(grund) = &anlass {
                    begruende(
                        j,
                        tag,
                        SIMULATION,
                        &format!("{t}: {}{grund}", crate::anlass::ANLASS),
                    )?;
                }
                lage.anlass = anlass.as_deref();
                let (vorschlag, verwertbar, fristverkaeufe) = frage(j, z, id, frager, &lage)?;
                let erstmeinung = if verwertbar {
                    Erstmeinung::Verwertbar
                } else {
                    Erstmeinung::Unverwertbar
                };
                (vorschlag, erstmeinung, fristverkaeufe)
            }
        }
    };
    Ok(TopfAntwort {
        topf,
        vorschlag,
        erstmeinung,
        zweitmeinung_ausgefallen: false,
        ersatzkauf: false,
        fristverkaeufe,
        wiederholung,
    })
}

/// Der zweite Berater prueft den Vorschlag, dann entscheidet das Regelwerk.
/// Journalwort eines Urteils. Ein Teilveto ist lesbar wie ein Veto und nennt die abgelehnten
/// Werte; `vetoschatten` rechnet nur diese (#311 H4).
fn urteil_wort(folge: Option<&Urteilsfolge>) -> String {
    match folge {
        Some(Urteilsfolge::Zustimmung) => ZUSTIMMUNG.to_string(),
        Some(Urteilsfolge::Veto) => VETO.to_string(),
        Some(Urteilsfolge::Teilveto { abgelehnt, .. }) => format!(
            "{VETO}: nur {}",
            abgelehnt
                .iter()
                .map(Isin::als_str)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        None => URTEIL_VERWORFEN.to_string(),
    }
}

fn zweite_meinung(
    j: &mut Journal,
    z: &Zyklus<'_>,
    antwort: TopfAntwort,
    pruefer: &mut dyn Berater,
) -> Result<TopfAntwort, LaufFehler> {
    if z.uhr.is_some()
        && antwort
            .vorschlag
            .as_ref()
            .is_some_and(|v| crate::abstimmung::nur_verkleinerung(&antwort.topf, &z.tag.kurse, v))
    {
        crate::abstimmung::belege_verkleinerung(j, z, &antwort.topf)?;
        return Ok(antwort);
    }
    let TopfAntwort {
        topf,
        mut vorschlag,
        erstmeinung,
        mut zweitmeinung_ausgefallen,
        mut ersatzkauf,
        fristverkaeufe,
        wiederholung,
    } = antwort;
    let (tag, zyklus) = (z.tag, z.nr);
    let id = topf.id();
    let zweiter = andere(id.berater());
    let t = topf_name(id);
    let positionskaeufe = crate::positionskauf::lies(j.stand(), &topf);
    let verlauf = crate::kursverlauf::fuer_lage(j.stand(), &tag.kurse, z.universum, tag.datum);
    let relative_staerke =
        crate::relative_staerke::fuer_lage(j.stand(), &tag.kurse, z.universum, tag.datum);
    let lage = Lage {
        anlass: None,
        datum: tag.datum,
        topf: &topf,
        kaufbar: kaufbudget(j, z.grenzen, id)?,
        kurse: &tag.kurse,
        verlauf: &verlauf,
        kaufbilanz: crate::ausfuehrung::kaufbilanz(j.stand(), topf.id(), tag.datum),
        relative_staerke: &relative_staerke,
        positionskaeufe: &positionskaeufe,
        ereignisse: &tag.ereignisse,
    };
    if vorschlag.is_some() && !pruefer.angebunden() {
        let grund = nicht_angebunden(pruefer);
        pruefer.anfrage_uebersprungen(depot_journal::KiZweck::Zweitmeinung, &grund);
        vorschlag = None;
        begruende(
            j,
            tag,
            name(zweiter),
            &format!("{t}: {grund}, nicht gefragt; ohne Zweitmeinung kein Auftrag"),
        )?;
    }
    if let Some(v) = vorschlag.take()
        && let Some(e) = erlaubnis(
            j,
            tag,
            zyklus,
            id,
            zweiter,
            "ohne Zweitmeinung kein Auftrag",
            pruefer,
        )?
    {
        let urteil = pruefer.pruefen_mit_bilanz(e, &lage, &v, &prognosebilanz(j.stand()));
        belege_ki(j, pruefer).map_err(journal)?;
        let folge = urteil.verwertbar.then(|| {
            crate::teilveto::urteilsfolge(&v, urteil.zustimmung, urteil.abgelehnt.as_deref())
        });
        let wort = urteil_wort(folge.as_ref());
        begruende(
            j,
            tag,
            name(zweiter),
            &format!("{t}: {wort}: {}", urteil.begruendung),
        )?;
        zweitmeinung_ausgefallen = !urteil.verwertbar;
        ersatzkauf = zweitmeinung_ausgefallen
            && pruefer.zweitmeinung_technisch_ausgefallen()
            && !pruefer.kostensperre();
        vorschlag = match folge {
            Some(Urteilsfolge::Zustimmung) => Some(v),
            Some(Urteilsfolge::Teilveto {
                vorschlag: rest, ..
            }) => Some(*rest),
            _ if ersatzkauf => Some(v),
            _ => None,
        };
    }
    Ok(TopfAntwort {
        topf,
        vorschlag,
        erstmeinung,
        zweitmeinung_ausgefallen,
        ersatzkauf,
        fristverkaeufe,
        wiederholung,
    })
}

/// Der Parkwert zählt nicht zum Mindesteinsatz: Die 30-%-Regel soll zu aktiven Käufen führen
/// (#311 H3, Festlegung 2).
fn geplanter_einsatz(
    topf: &Topf,
    kurse: &Kurse,
    plan: &depot_regelwerk::Entscheidung,
    park: Option<&Isin>,
) -> Option<i64> {
    let gesamt = topf.wert_exakt(kurse)?;
    let bargeld = depot_kern::Preis::aus_cent(topf.bargeld())?;
    let mut eingesetzt = gesamt.checked_sub(bargeld)?;
    if let Some(p) = park.filter(|p| topf.stueck(p) > 0) {
        eingesetzt = eingesetzt.checked_sub(kurse.kurs(p)?.mal_stueck_exakt(topf.stueck(p))?)?;
    }
    for auftrag in plan.verkaeufe.iter().chain(&plan.kaeufe) {
        let a = auftrag.auftrag();
        if Some(&a.isin) == park {
            continue;
        }
        let wert = kurse.kurs(&a.isin)?.mal_stueck_exakt(a.stueck)?;
        eingesetzt = match a.richtung {
            Richtung::Kauf => eingesetzt.checked_add(wert)?,
            Richtung::Verkauf => eingesetzt.checked_sub(wert)?,
        };
    }
    eingesetzt.in_bp_von(gesamt)
}

fn mindesteinsatz(
    j: &mut Journal,
    z: &Zyklus<'_>,
    topf: &Topf,
    plan: &depot_regelwerk::Entscheidung,
    vorschlag: Option<&Vorschlag>,
    berater: &mut dyn Berater,
    minimum_bp: i64,
) -> Result<(), LaufFehler> {
    if minimum_bp == 0
        || !z.gesperrt.is_empty()
        || !z.vollstaendige_kurse
        || !depot_kern::liste::handelstag(z.tag.datum)
    {
        return Ok(());
    }
    let park = vorschlag.and_then(|v| v.park.as_ref()).map(|p| &p.isin);
    let Some(einsatz_bp) =
        geplanter_einsatz(topf, &z.tag.kurse, plan, park).filter(|bp| *bp < minimum_bp)
    else {
        return Ok(());
    };
    // Keine Nachfrage mehr: Der Grund kommt mit der Erstmeinung (#312-3, P37).
    let grund = berater.mindesteinsatz_begruendung(z.tag.datum);
    let gehalten: BTreeMap<Isin, i64> = topf
        .positionen()
        .keys()
        .filter_map(|isin| {
            depot_regelwerk::entscheidung::ist_gewicht_bp(topf, &z.tag.kurse, isin)
                .map(|bp| (isin.clone(), bp))
        })
        .collect();
    let ki_bp = crate::mindesteinsatz::ki_einsatz_bp(&gehalten, vorschlag);
    let status = crate::mindesteinsatz::vermerk(grund.as_deref(), ki_bp, minimum_bp);
    begruende(
        j,
        z.tag,
        REGELWERK,
        &format!(
            "{}: unter_mindesteinsatz {einsatz_bp} von {minimum_bp} bp · {status}",
            topf_name(topf.id())
        ),
    )?;
    Ok(())
}

fn fuehre_fristverkaeufe_aus(
    j: &mut Journal,
    z: &Zyklus<'_>,
    topf: &Topf,
    fristen: BTreeSet<Isin>,
    konto: &mut dyn Broker,
) -> Result<(), LaufFehler> {
    let faellig = fristen
        .into_iter()
        .filter(|isin| {
            !z.gesperrt.contains(isin)
                && depot_kern::liste::boerse_von(isin).is_some_and(|b| b.handelstag(z.tag.datum))
                && !j
                    .stand()
                    .offene_auftraege()
                    .any(|a| a.beleg.isin == *isin && a.beleg.topf == topf.id())
        })
        .collect();
    let orders = depot_regelwerk::frist_auftraege(
        z.grenzen,
        topf,
        &z.tag.kurse,
        &faellig,
        &mut j.kennungsvergabe(),
    )
    .ok_or_else(|| rechnung("Fristverkauf"))?;
    for order in &orders {
        begruende(
            j,
            z.tag,
            "Haltefrist",
            &format!(
                "{}: {} verkaufen. Haltefrist ohne neue Begründung.",
                topf_name(topf.id()),
                order.auftrag().isin
            ),
        )?;
    }
    fuehre_aus(j, z, topf.id(), &orders, None, konto)
}

fn topf_zyklus(
    j: &mut Journal,
    z: &Zyklus<'_>,
    antwort: TopfAntwort,
    konto: &mut dyn Broker,
    berater: &mut dyn Berater,
    minimum_bp: i64,
) -> Result<(), LaufFehler> {
    let TopfAntwort {
        topf,
        vorschlag,
        zweitmeinung_ausgefallen,
        ersatzkauf,
        fristverkaeufe,
        wiederholung,
        ..
    } = antwort;
    let ruhetag = antwort.erstmeinung == Erstmeinung::Ruhetag;
    if !fristverkaeufe.is_empty() {
        return fuehre_fristverkaeufe_aus(j, z, &topf, fristverkaeufe, konto);
    }
    let tag = z.tag;
    let id = topf.id();
    let t = topf_name(id);
    let mut vergabe = j.kennungsvergabe();
    let vor_valuta: Vec<_> = j
        .stand()
        .vor_valuta(id, tag.datum)
        .into_iter()
        .map(|(isin, _)| isin)
        .collect();
    let eingabe = Eingabe {
        grenzen: z.grenzen,
        universum: z.universum,
        topf: &topf,
        kurse: &tag.kurse,
        vorschlag: vorschlag.as_ref(),
        halt: false,
        vor_valuta: &vor_valuta,
    };
    let frei = kaufbudget(j, z.grenzen, id)?;
    let ent = match &wiederholung {
        Some(w) => depot_regelwerk::entscheide_wiederholung(&eingabe, &mut vergabe, frei, w),
        None => plane_topf(j, &eingabe, frei, ersatzkauf, konto.tausch_moeglich()),
    };
    if !ruhetag {
        mindesteinsatz(j, z, &topf, &ent, vorschlag.as_ref(), berater, minimum_bp)?;
    }
    if ersatzkauf {
        begruende(
            j,
            tag,
            REGELWERK,
            &format!("{t}: ZweitmeinungAusgefallen · Kaufzielgewichte × 0,5"),
        )?;
    }
    belege_topfbefunde(j, tag, t, &ent, ruhetag, zweitmeinung_ausgefallen)?;
    if ent.verkaeufe.is_empty() {
        return fuehre_aus(j, z, id, &ent.kaeufe, vorschlag.as_ref(), konto);
    }
    let alle = fuehre_aus(j, z, id, &ent.verkaeufe, None, konto);
    // Positions- und Anteilsgrenzen können einen ausgeführten Verkauf voraussetzen. Ein
    // Tauschkauf hängt an allen Verkäufen des Plans und füllt nur nach ihnen (#311 H1b).
    let tausch = ent.kaeufe.iter().any(|k| !k.nach().is_empty());
    match alle {
        Ok(()) if verkauft(j, &ent.verkaeufe) || (tausch && unterwegs(j, &ent.verkaeufe)) => {
            fuehre_aus(j, z, id, &ent.kaeufe, vorschlag.as_ref(), konto)
        }
        Ok(()) if ent.kaeufe.is_empty() => Ok(()),
        Ok(()) => begruende(
            j,
            tag,
            SIMULATION,
            &format!("{t}: Käufe entfallen: nicht alle Verkäufe ausgeführt"),
        ),
        Err(e) => Err(e),
    }
}

fn belege_topfbefunde(
    j: &mut Journal,
    tag: &Tag,
    t: &str,
    ent: &depot_regelwerk::Entscheidung,
    ruhetag: bool,
    zweitmeinung_ausgefallen: bool,
) -> Result<(), LaufFehler> {
    for b in &ent.befunde {
        if ruhetag && matches!(b, depot_regelwerk::Befund::KeinVorschlag) {
            continue;
        }
        let grund = if zweitmeinung_ausgefallen
            && matches!(b, depot_regelwerk::Befund::KeinVorschlag)
        {
            format!("{t}: ZweitmeinungAusgefallen")
        } else {
            match b {
                depot_regelwerk::Befund::LimitabstandAusserhalb {
                    isin,
                    gefragt,
                    verwendet,
                } => format!(
                    "{t}: {isin}: Limitabstand {gefragt} bp außerhalb des Rahmens, {verwendet} bp verwendet"
                ),
                depot_regelwerk::Befund::StoppFehlt { isin, gefragt } => gefragt.map_or_else(
                    || format!("{t}: {isin}: kein Schutz-Stopp im Kaufvorschlag"),
                    |bp| format!("{t}: {isin}: Schutz-Stopp mit {bp} bp nicht übernommen"),
                ),
                _ => format!("{t}: {b:?}"),
            }
        };
        begruende(j, tag, REGELWERK, &grund)?;
    }
    Ok(())
}

/// Der Plan eines Topfes. Tausch am selben Schluss nur im Papierkonto und nur an derselben
/// Börse (#311 H1b); sonst warten Käufe auf die Füllung der Verkäufe.
fn plane_topf(
    j: &Journal,
    eingabe: &Eingabe<'_>,
    frei: Cent,
    ersatzkauf: bool,
    tausch: bool,
) -> depot_regelwerk::Entscheidung {
    if tausch {
        let ent =
            depot_regelwerk::entscheide_tausch(eingabe, &mut j.kennungsvergabe(), frei, ersatzkauf);
        if gleicher_schluss(&ent) {
            return ent;
        }
    }
    let mut vergabe = j.kennungsvergabe();
    if ersatzkauf {
        entscheide_ersatzkauf(eingabe, &mut vergabe, frei)
    } else {
        entscheide_mit_kaufbudget(eingabe, &mut vergabe, frei)
    }
}

/// Kaufen und Verkaufen alle Aufträge an derselben Börse, also zum selben Schluss? Sonst
/// lohnt kein Tausch: Der Kauf käme vor dem Erlös (#311 H1b).
fn gleicher_schluss(ent: &depot_regelwerk::Entscheidung) -> bool {
    let mut boersen = ent
        .verkaeufe
        .iter()
        .chain(&ent.kaeufe)
        .map(|a| depot_kern::liste::boerse_von(&a.auftrag().isin));
    let erste = boersen.next().flatten();
    erste.is_some() && boersen.all(|b| b == erste)
}

/// Sind alle diese Verkäufe gesendet und noch offen oder gefüllt? Nur dann kann ein Tauschkauf
/// nach ihnen füllen.
fn unterwegs(j: &Journal, auftraege: &[GenehmigterAuftrag]) -> bool {
    auftraege.iter().all(|a| {
        j.stand()
            .auftrag(a.auftrag().kennung.nr())
            .is_some_and(|l| {
                l.zustand == AuftragsZustand::Gefuellt
                    || (l.zustand.offen() && l.zustand != AuftragsZustand::Reserviert)
            })
    })
}

/// Sind alle diese Auftraege ganz ausgefuehrt?
fn verkauft(j: &Journal, auftraege: &[GenehmigterAuftrag]) -> bool {
    auftraege.iter().all(|a| {
        j.stand()
            .auftrag(a.auftrag().kennung.nr())
            .map(|l| l.zustand)
            == Some(AuftragsZustand::Gefuellt)
    })
}

const ERWARTUNG: &str = "Erwartung";

fn belege_erwartung(
    j: &mut Journal,
    z: &Zyklus<'_>,
    a: &depot_regelwerk::Auftrag,
    v: &Vorschlag,
) -> Result<(), LaufFehler> {
    let nr = a.kennung.nr();
    let Some(bp) = v.erwartung_bp.get(&a.isin) else {
        return Ok(());
    };
    let Some(bp) = bp.filter(|bp| (0..=10_000).contains(bp)) else {
        return begruende(
            j,
            z.tag,
            ERWARTUNG,
            &format!("Auftrag {nr}: unplausible Erwartung ignoriert"),
        );
    };
    let frist = v
        .begruendung
        .get(&a.isin)
        .and_then(|text| crate::Ausstiegsplan::lies(text))
        .map_or_else(|| "-".to_owned(), |plan| plan.handelstage.to_string());
    begruende(j, z.tag, ERWARTUNG, &format!("{nr} {bp} {frist}"))?;
    let huerde = a.limit.mal_stueck(a.stueck).and_then(|volumen| {
        crate::kostenhuerde::gesamt_bp(
            volumen,
            z.grenzen.gebuehren().fuer(volumen)?,
            depot_kern::liste::boerse_von(&a.isin),
        )
    });
    if let Some(huerde) = huerde.filter(|h| bp < *h) {
        begruende(
            j,
            z.tag,
            ERWARTUNG,
            &format!("Auftrag {nr}: Erwartung unter Hürde ({bp} von {huerde} Basispunkten)"),
        )?;
    }
    Ok(())
}

fn belege_kauf(
    j: &mut Journal,
    z: &Zyklus<'_>,
    a: &depot_regelwerk::Auftrag,
    vorschlag: Option<&Vorschlag>,
) -> Result<(), LaufFehler> {
    let nr = a.kennung.nr();
    let tag = z.tag;
    // Erst nach der Reservierung und vor dem Versand dauerhaft belegen. In der
    // Folgeberatung zählt diese These erst, sobald der Auftrag ausgeführt wurde.
    if a.richtung == Richtung::Kauf {
        let huerde = a
            .limit
            .mal_stueck(a.stueck)
            .and_then(|volumen| {
                crate::kostenhuerde::hinweis(
                    volumen,
                    z.grenzen.gebuehren().fuer(volumen)?,
                    depot_kern::liste::boerse_von(&a.isin),
                )
            })
            .unwrap_or_else(|| "Kostenhürde nicht berechenbar. Spread nicht belegt.".into());
        begruende(j, tag, "Kostenhürde", &format!("Auftrag {nr}: {huerde}"))?;
        if let Some(v) = vorschlag {
            belege_erwartung(j, z, a, v)?;
        }
        let lang =
            crate::relative_staerke::fuer_lage(j.stand(), &tag.kurse, z.universum, tag.datum);
        let kurz = crate::relative_staerke::kurzrang(j.stand(), &tag.kurse, z.universum, tag.datum);
        for (name, liste, zusatz) in [
            (crate::relative_staerke::RANG_BELEG, lang, " 120"),
            (crate::relative_staerke::RANG20_BELEG, kurz, ""),
        ] {
            if let Some(rang) = liste
                .iter()
                .position(|(isin, _)| *isin == a.isin)
                .and_then(|index| index.checked_add(1))
            {
                begruende(j, tag, name, &format!("{nr} {rang}{zusatz}"))?;
            }
        }
    }
    if let Some(these) = vorschlag
        .and_then(|v| v.begruendung.get(&a.isin))
        .filter(|text| !text.trim().is_empty())
    {
        if a.richtung == Richtung::Kauf
            && crate::Ausstiegsplan::lies(these).is_some()
            && j.stand()
                .auftrag(nr)
                .and_then(|o| j.stand().toepfe().get(&o.beleg.topf))
                .is_some_and(|t| t.stueck(&a.isin) != 0)
        {
            begruende(
                j,
                tag,
                "Haltefrist",
                &format!("Auftrag {nr}: Frist durch Zukauf neu ab Ausführung."),
            )?;
        }
        let grund = kaufeinstieg(j, tag, a, these)?;
        begruende(
            j,
            tag,
            crate::positionskauf::KAUFTHESE,
            &format!("{nr} {grund}"),
        )?;
    }
    if a.richtung == Richtung::Kauf
        && let Some(v) = vorschlag
        && let Some(nummern) = v.belege.get(&a.isin)
    {
        belege_kaufbelege(j, tag, nr, v.berater, nummern)?;
    }
    Ok(())
}

/// Verknüpft den Kauf mit den Ereignissen des Tages, auf die die KI ihn stützt (#325).
/// Nur Nummern, die heute als Beobachtung im Journal stehen, zählen. Ältere Stände lesen
/// die Zeile als gewöhnliche Begründung.
fn belege_kaufbelege(
    j: &mut Journal,
    tag: &Tag,
    nr: u64,
    wer: BeraterId,
    nummern: &[usize],
) -> Result<(), LaufFehler> {
    let heute = j
        .stand()
        .beobachtungen()
        .iter()
        .filter(|(d, _)| *d == tag.datum)
        .count();
    let gueltig: Vec<String> = nummern
        .iter()
        .filter(|n| (1..=heute).contains(*n))
        .map(ToString::to_string)
        .collect();
    if gueltig.is_empty() {
        return Ok(());
    }
    begruende(
        j,
        tag,
        crate::belegrang::KAUFBELEG,
        &format!("{nr} {} {}", name(wer), gueltig.join(",")),
    )
}

/// Der besondere Kaufanlass zählt nur mit der zum Auftrag belegten Kursreaktion.
fn kaufeinstieg(
    j: &mut Journal,
    tag: &Tag,
    a: &depot_regelwerk::Auftrag,
    these: &str,
) -> Result<String, LaufFehler> {
    let marke = crate::positionskauf::ZAHLEN_UEBERRASCHUNG;
    let Some(rest) = these
        .strip_prefix('[')
        .and_then(|t| t.strip_prefix(marke))
        .and_then(|t| t.strip_prefix("] "))
    else {
        return Ok(these.to_owned());
    };
    let wissen = j
        .stand()
        .wissensstichtag(tag.datum)
        .unwrap_or_else(|| tag.kurse.stand());
    let belege = (a.richtung == Richtung::Kauf)
        .then(|| crate::zahlenreaktion::belege(j.stand(), &tag.kurse, wissen));
    belege_zahlenmarke(
        j,
        tag,
        a.kennung.nr(),
        rest,
        belege.as_ref().and_then(|b| b.get(&a.isin)),
    )
}

fn belege_zahlenmarke(
    j: &mut Journal,
    tag: &Tag,
    nr: u64,
    rest: &str,
    beleg: Option<&crate::zahlenreaktion::Reaktion>,
) -> Result<String, LaufFehler> {
    let marke = crate::positionskauf::ZAHLEN_UEBERRASCHUNG;
    if let Some(beleg) = beleg.filter(|b| b.positiv) {
        begruende(j, tag, marke, &format!("Auftrag {nr}: {}", beleg.text))?;
        return Ok(format!("[{marke}] {rest}"));
    }
    begruende(
        j,
        tag,
        marke,
        &format!("Auftrag {nr}: Marke ohne Beleg herabgestuft"),
    )?;
    Ok(format!("[Zahlen] {rest}"))
}

/// Die Auftragsstelle: Jeder Auftrag wird mit Deckung reserviert (Kontowaechter), als
/// gesendet belegt und erst dann an das Konto gesendet. Ohne Antwort gilt der Sendestatus als
/// unbekannt; geklaert wird nur ueber den Abgleich, nie durch einen zweiten Versand (R37).
/// Danach laeuft der Markt, und der Kontowaechter gleicht ab: Jede Ausfuehrung steht im
/// Journal, jede Abweichung haelt an.
fn fuehre_aus(
    j: &mut Journal,
    z: &Zyklus<'_>,
    id: TopfId,
    auftraege: &[GenehmigterAuftrag],
    vorschlag: Option<&Vorschlag>,
    konto: &mut dyn Broker,
) -> Result<(), LaufFehler> {
    let tag = z.tag;
    if auftraege.is_empty() {
        return Ok(());
    }
    let t = topf_name(id);
    let mut gesendet = Vec::new();
    let morgens = j
        .stand()
        .wissensstichtag(tag.datum)
        .is_some_and(depot_kern::liste::morgenlauf);
    for a in auftraege {
        if morgens
            && !depot_kern::liste::boerse_von(&a.auftrag().isin)
                .is_some_and(|b| b.handelstag(tag.datum))
        {
            begruende(
                j,
                tag,
                AUFTRAGSSTELLE,
                &format!("{t}: {} heute börsenfrei, kein Versand.", a.auftrag().isin),
            )?;
            continue;
        }
        let nr = a.auftrag().kennung.nr();
        match j.reserviere_auftrag(z.grenzen, id, a) {
            Ok(()) => {}
            Err(JournalFehler::Abgelehnt(grund)) => {
                begruende(
                    j,
                    tag,
                    KONTOWAECHTER,
                    &format!("{t}: Auftrag {nr} nicht reserviert: {grund}"),
                )?;
                continue;
            }
            Err(e) => return Err(journal(e)),
        }
        belege_kauf(j, z, a.auftrag(), vorschlag)?;
        let versand = z
            .uhr
            .map(|uhr| {
                uhr()
                    .filter(|zeit| *zeit >= tag.kurse.stand())
                    .ok_or_else(|| halt("Keine verlässliche Versandzeit"))
            })
            .transpose()?;
        let meldung = match versand {
            Some(zeit) => {
                let frist = papierorder_schluss(&a.auftrag().isin, zeit)
                    .ok_or_else(|| halt("Keine Börsenfrist für den Auftrag"))?;
                Meldung::GesendetAm(zeit, frist)
            }
            None => Meldung::Gesendet,
        };
        j.melde(nr, &meldung).map_err(journal)?;
        let antwort = match versand {
            Some(zeit) => konto.sende_am(a, zeit),
            None => konto.sende(a),
        };
        // Ist das Konto nicht gesichert, bestaetigt das Journal nichts mehr davon (K23-01).
        pruefe_stoerung(&*konto)?;
        match antwort {
            Sendeantwort::Angenommen => j.melde(nr, &Meldung::Angenommen).map_err(journal)?,
            Sendeantwort::Abgelehnt(grund) => {
                j.melde(nr, &Meldung::Abgelehnt).map_err(journal)?;
                begruende(
                    j,
                    tag,
                    AUFTRAGSSTELLE,
                    &format!("{t}: Auftrag {nr} abgelehnt: {grund}"),
                )?;
            }
            Sendeantwort::KeineAntwort => {
                j.melde(nr, &Meldung::Unbekannt).map_err(journal)?;
                begruende(
                    j,
                    tag,
                    AUFTRAGSSTELLE,
                    &format!(
                        "{t}: Auftrag {nr}: Sendestatus unbekannt; nur Abgleich, kein neuer Versand"
                    ),
                )?;
            }
        }
        gesendet.push(nr);
    }
    if z.uhr.is_none() {
        konto.markt(&tag.kurse);
    }
    kontowaechter(j, tag, &*konto)?;
    protokolliere_ausfuehrungen(j, tag, &gesendet, None)
}

fn protokolliere_ausfuehrungen(
    j: &mut Journal,
    tag: &Tag,
    nummern: &[u64],
    papierabgleich: Option<(&Kurse, &BTreeSet<u64>)>,
) -> Result<(), LaufFehler> {
    for nr in nummern {
        let Some(l) = j.stand().auftrag(*nr).cloned() else {
            return Err(halt(format!("Auftrag {nr} fehlt im Journal")));
        };
        let t = topf_name(l.beleg.topf);
        for a in &l.ausfuehrungen {
            begruende(
                j,
                tag,
                SIMULATION,
                &format!(
                    "{t}: Auftrag {nr} ausgeführt: {:?} {} Stück {} zu {} Cent, Gebühr {} Cent",
                    l.beleg.richtung, a.stueck, l.beleg.isin, a.kurs.0, a.gebuehr.0
                ),
            )?;
        }
        if l.zustand == AuftragsZustand::Verfallen {
            let grund = match (papierabgleich, l.frist) {
                (Some((_, abgelaufen)), _) if abgelaufen.contains(nr) => {
                    "Schlusskurs nicht mehr nachholbar"
                }
                (Some((kurse, _)), Some(schluss))
                    if kurse
                        .eintrag(&l.beleg.isin)
                        .map(depot_kern::Kurs::zeitpunkt)
                        != Some(schluss) =>
                {
                    "Schlusskurs nicht belegt"
                }
                _ => "Kurs ausserhalb des Limits",
            };
            begruende(
                j,
                tag,
                SIMULATION,
                &format!("{t}: Auftrag {nr} verfällt: {grund}"),
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_zahlenreaktion_stuft_kaufmarke_herab() -> Result<(), LaufFehler> {
        let ordner =
            std::env::temp_dir().join(format!("depot-quartalsmarke-{}", std::process::id()));
        std::fs::create_dir_all(&ordner).map_err(|e| halt(e.to_string()))?;
        let pfad = ordner.join("journal.txt");
        Journal::einrichten(&pfad).map_err(journal)?;
        let mut j = Journal::oeffnen(&pfad).map_err(journal)?;
        let datum = depot_kern::Datum::neu(2026, 11, 9).ok_or_else(|| halt("Datum"))?;
        let zeit = Zeitpunkt::neu(datum, 22, 0, 0).ok_or_else(|| halt("Zeit"))?;
        let kurse = Kurse::neu(zeit, Vec::new()).map_err(|e| halt(format!("{e:?}")))?;
        let tag = Tag {
            datum,
            kurse,
            ereignisse: Vec::new(),
        };
        let negativ = crate::zahlenreaktion::Reaktion {
            positiv: false,
            abstand_bp: -600,
            text: "−6,00 %".to_owned(),
        };
        let rest = "[Ausstieg 14500 10] These";
        assert_eq!(
            belege_zahlenmarke(&mut j, &tag, 1, rest, Some(&negativ))?,
            format!("[Zahlen] {rest}")
        );
        assert!(
            j.stand()
                .begruendungen()
                .iter()
                .any(|(_, wer, text)| wer == "Zahlen-Überraschung"
                    && text == "Auftrag 1: Marke ohne Beleg herabgestuft")
        );
        drop(j);
        std::fs::remove_dir_all(ordner).map_err(|e| halt(e.to_string()))?;
        Ok(())
    }

    #[test]
    fn ruhetag_spart_aufrufe_und_handstart_fragt() -> Result<(), LaufFehler> {
        struct Probe(Option<String>);
        impl Berater for Probe {
            fn vorschlagen(
                &mut self,
                _: depot_journal::AufrufErlaubnis,
                lage: &Lage<'_>,
            ) -> crate::Antwort {
                self.0 = lage.anlass.map(str::to_owned);
                crate::Antwort {
                    halten: std::collections::BTreeMap::new(),
                    prognose: None,
                    verwertbar: true,
                    einschaetzung: "Bargeld".into(),
                    vorschlag: None,
                }
            }
            fn pruefen(
                &mut self,
                _: depot_journal::AufrufErlaubnis,
                _: &Lage<'_>,
                _: &Vorschlag,
            ) -> crate::Urteil {
                unreachable!("ohne Vorschlag gibt es nichts zu prüfen")
            }
        }
        let ordner = std::env::temp_dir().join(format!("depot-ruhetag-{}", std::process::id()));
        std::fs::create_dir_all(&ordner).map_err(|e| halt(e.to_string()))?;
        let pfad = ordner.join("journal.txt");
        Journal::einrichten(&pfad).map_err(journal)?;
        let mut j = Journal::oeffnen(&pfad).map_err(journal)?;
        let datum = depot_kern::Datum::neu(2030, 1, 10).ok_or_else(|| halt("Datum"))?;
        let gestern = datum.plus_tage(-1).ok_or_else(|| halt("Vortag"))?;
        let zeit = Zeitpunkt::neu(datum, 12, 0, 0).ok_or_else(|| halt("Zeit"))?;
        let kurse = Kurse::neu(zeit, vec![]).map_err(|e| halt(format!("{e:?}")))?;
        let tag = Tag {
            datum,
            kurse,
            ereignisse: Vec::new(),
        };
        let topf = Topf::neu(TopfId::A, Cent(50_000)).map_err(|e| halt(format!("{e:?}")))?;
        j.setze_topf(&topf).map_err(journal)?;
        j.reserviere(BeraterId::Dachs, gestern, 1)
            .map_err(|e| halt(format!("{e:?}")))?;
        let uhr = || Some(zeit);
        let grenzen = Grenzen::standard();
        let z = Zyklus {
            anlass_pruefen: true,
            grenzen: &grenzen,
            universum: &Universum::neu(vec![]),
            tag: &tag,
            vortag: &tag.kurse,
            nr: 2,
            abdruck: "Probe",
            uhr: Some(&uhr),
            vollstaendige_kurse: true,
            gesperrt: &BTreeSet::new(),
            bewertung: &tag.kurse,
        };
        let mut probe = Probe(None);
        let antwort = frage_topf(&mut j, &z, TopfId::A, &mut probe, &BTreeSet::new())?;
        assert!(antwort.erstmeinung == Erstmeinung::Ruhetag);
        let mut konto =
            crate::papier::Papierkonto::eroeffne(&[topf]).ok_or_else(|| halt("Papierkonto"))?;
        topf_zyklus(&mut j, &z, antwort, &mut konto, &mut probe, 3_000)?;
        assert_eq!(j.stand().ki().buchungen().len(), 1);
        assert!(
            j.stand()
                .begruendungen()
                .iter()
                .any(|(_, _, t)| t == "Topf A: Ruhetag, kein Anlass")
        );
        assert!(
            j.stand()
                .begruendungen()
                .iter()
                .all(|(_, _, t)| !t.contains("KeinVorschlag"))
        );
        let hand = Zyklus {
            anlass_pruefen: false,
            ..z
        };
        frage_topf(&mut j, &hand, TopfId::A, &mut probe, &BTreeSet::new())?;
        assert_eq!(probe.0.as_deref(), Some("Papiertag von Hand"));
        assert_eq!(j.stand().ki().buchungen().len(), 2);
        drop(j);
        std::fs::remove_dir_all(ordner).map_err(|e| halt(e.to_string()))?;
        Ok(())
    }

    /// #311 H3, Festlegung 2: Der Parkwert füllt den Mindesteinsatz nicht.
    #[test]
    fn parkwert_zaehlt_nicht_zum_mindesteinsatz() -> Result<(), LaufFehler> {
        let datum = depot_kern::Datum::neu(2026, 11, 6).ok_or_else(|| halt("Datum"))?;
        let zeit = Zeitpunkt::neu(datum, 16, 35, 0).ok_or_else(|| halt("Zeit"))?;
        let etf = depot_kern::Isin::neu("IE00B4L5Y983").map_err(|e| halt(format!("{e:?}")))?;
        let preis = depot_kern::Preis::lies_euro("100").ok_or_else(|| halt("Preis"))?;
        let kurs = depot_kern::Kurs::exakt(preis, "EUR", datum, zeit, Herkunft::Quelle)
            .map_err(|e| halt(format!("{e:?}")))?;
        let kurse =
            Kurse::neu(zeit, vec![(etf.clone(), kurs)]).map_err(|e| halt(format!("{e:?}")))?;
        let mut topf = Topf::neu(TopfId::A, Cent(600_000)).map_err(|e| halt(format!("{e:?}")))?;
        topf.setze_bestand(etf.clone(), 40);
        let plan = depot_regelwerk::Entscheidung::default();
        assert_eq!(geplanter_einsatz(&topf, &kurse, &plan, None), Some(4_000));
        assert_eq!(geplanter_einsatz(&topf, &kurse, &plan, Some(&etf)), Some(0));
        Ok(())
    }

    fn sap_vorschlag(bp: i64) -> Result<Vorschlag, LaufFehler> {
        let isin = depot_kern::Isin::neu("DE0007164600").map_err(|e| halt(format!("{e:?}")))?;
        Ok(Vorschlag {
            erwartung_bp: std::collections::BTreeMap::new(),
            limit_bp: std::collections::BTreeMap::new(),
            stopp_bp: std::collections::BTreeMap::new(),
            berater: BeraterId::Dachs,
            zielgewichte_bp: [(isin, bp)].into_iter().collect(),
            begruendung: std::collections::BTreeMap::new(),
            belege: std::collections::BTreeMap::new(),
            park: None,
        })
    }

    /// #312-3: Der Grund kommt nur mit der Erstmeinung, keine Nachfrage. Ziele 40 %, die das
    /// Regelwerk nicht plant, sind kein Versäumnis der KI („Regelwerk kürzte“).
    #[test]
    fn mindesteinsatz_ohne_feld_fragt_nicht() -> Result<(), LaufFehler> {
        struct Nachfrage(Option<String>);
        impl Berater for Nachfrage {
            fn vorschlagen(
                &mut self,
                _: depot_journal::AufrufErlaubnis,
                _: &Lage<'_>,
            ) -> crate::Antwort {
                unreachable!("nur Nachfrage")
            }
            fn pruefen(
                &mut self,
                _: depot_journal::AufrufErlaubnis,
                _: &Lage<'_>,
                _: &Vorschlag,
            ) -> crate::Urteil {
                unreachable!("nur Nachfrage")
            }
            fn mindesteinsatz_begruendung(&mut self, _: depot_kern::Datum) -> Option<String> {
                self.0.take()
            }
        }
        let ordner =
            std::env::temp_dir().join(format!("depot-mindesteinsatz-{}", std::process::id()));
        std::fs::create_dir_all(&ordner).map_err(|e| halt(e.to_string()))?;
        let pfad = ordner.join("journal.txt");
        Journal::einrichten(&pfad).map_err(journal)?;
        let mut j = Journal::oeffnen(&pfad).map_err(journal)?;
        let datum = depot_kern::Datum::neu(2030, 1, 10).ok_or_else(|| halt("Datum"))?;
        let zeit = Zeitpunkt::neu(datum, 12, 0, 0).ok_or_else(|| halt("Zeit"))?;
        let kursliste = Kurse::neu(zeit, vec![]).map_err(|e| halt(format!("{e:?}")))?;
        let tag = Tag {
            datum,
            kurse: kursliste,
            ereignisse: Vec::new(),
        };
        let z = Zyklus {
            anlass_pruefen: false,
            grenzen: &Grenzen::standard(),
            universum: &Universum::default(),
            tag: &tag,
            vortag: &tag.kurse,
            nr: 1,
            abdruck: "Probe",
            uhr: None,
            vollstaendige_kurse: true,
            gesperrt: &BTreeSet::new(),
            bewertung: &tag.kurse,
        };
        let mut berater = Nachfrage(None);
        let topf = Topf::neu(TopfId::A, Cent(500_000)).map_err(|e| halt(format!("{e:?}")))?;
        let erster = j
            .reserviere(BeraterId::Dachs, datum, 1)
            .map_err(|e| halt(format!("{e:?}")))?;
        j.rechne_ki_ab(&erster.abrechnung(0, 0).ok_or_else(|| halt("Abrechnung"))?)
            .map_err(journal)?;
        let plan = depot_regelwerk::Entscheidung::default();
        let ki = sap_vorschlag(4_000)?;
        let mut pruefe = |j: &mut Journal, z: &Zyklus<'_>, v, grund, minimum| {
            berater.0 = grund;
            mindesteinsatz(j, z, &topf, &plan, v, &mut berater, minimum)
        };
        pruefe(&mut j, &z, None, None, 0)?;
        let gesperrt = Zyklus {
            vollstaendige_kurse: false,
            ..z
        };
        pruefe(&mut j, &gesperrt, None, None, 3_000)?;
        assert!(j.stand().begruendungen().is_empty());
        pruefe(&mut j, &z, None, None, 3_000)?;
        pruefe(&mut j, &z, Some(&ki), None, 3_000)?;
        pruefe(&mut j, &z, None, Some("NVIDIA abwarten".into()), 3_000)?;
        let zeilen: Vec<&str> = j
            .stand()
            .begruendungen()
            .iter()
            .map(|(_, _, text)| text.as_str())
            .collect();
        assert_eq!(
            zeilen,
            [
                "Topf A: unter_mindesteinsatz 0 von 3000 bp · fehlt",
                "Topf A: unter_mindesteinsatz 0 von 3000 bp · fehlt, Regelwerk kürzte",
                "Topf A: unter_mindesteinsatz 0 von 3000 bp · begründet: NVIDIA abwarten",
            ]
        );
        assert_eq!(
            j.stand().ki().buchungen().len(),
            1,
            "keine Nachfrage gebucht"
        );
        drop(j);
        std::fs::remove_dir_all(ordner).map_err(|e| halt(e.to_string()))?;
        Ok(())
    }

    #[test]
    fn laengerer_dialog_verlaengert_die_kursfrist_nicht() -> Result<(), LaufFehler> {
        let (szenario, _, _) = crate::Szenario::beispiel().ok_or_else(|| halt("Szenario"))?;
        let tag = szenario.tage.first().ok_or_else(|| halt("Tag"))?;
        let stand = tag.kurse.stand();
        let jetzt = std::cell::Cell::new(Some(stand));
        let uhr = || jetzt.get();
        let grenzen = Grenzen::standard();
        let zyklus = Zyklus {
            anlass_pruefen: false,
            grenzen: &grenzen,
            universum: &szenario.universum,
            tag,
            vortag: &szenario.vorlauf,
            nr: 1,
            abdruck: "Probe",
            uhr: Some(&uhr),
            vollstaendige_kurse: true,
            gesperrt: &BTreeSet::new(),
            bewertung: &tag.kurse,
        };
        assert!(beratungszeit(&zyklus));
        jetzt.set(Zeitpunkt::aus_sekunden(
            stand
                .sekunden()
                .saturating_add(grenzen.kurs_hoechstalter_s()),
        ));
        assert!(beratungszeit(&zyklus));
        jetzt.set(Zeitpunkt::aus_sekunden(
            stand
                .sekunden()
                .saturating_add(grenzen.kurs_hoechstalter_s())
                .saturating_add(1),
        ));
        assert!(!beratungszeit(&zyklus));
        jetzt.set(Zeitpunkt::aus_sekunden(stand.sekunden().saturating_sub(1)));
        assert!(!beratungszeit(&zyklus));
        jetzt.set(None);
        assert!(!beratungszeit(&zyklus));
        Ok(())
    }

    #[test]
    fn abgelehnte_prognose_laesst_den_tagesabschluss_zu() -> Result<(), LaufFehler> {
        let ordner = std::env::temp_dir().join(format!(
            "depot-weltprognose-abgelehnt-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&ordner).map_err(|e| halt(e.to_string()))?;
        let pfad = ordner.join("journal.txt");
        Journal::einrichten(&pfad).map_err(journal)?;
        let mut j = Journal::oeffnen(&pfad).map_err(journal)?;
        let datum = depot_kern::Datum::neu(2026, 11, 6).ok_or_else(|| halt("Datum"))?;
        let zeit = Zeitpunkt::neu(datum, 16, 35, 0).ok_or_else(|| halt("Zeit"))?;
        let isin = depot_kern::Isin::neu("DE0007164600").map_err(|e| halt(format!("{e:?}")))?;
        let preis = depot_kern::Preis::lies_euro("230").ok_or_else(|| halt("Preis"))?;
        let kurs = depot_kern::Kurs::exakt(preis, "EUR", datum, zeit, Herkunft::Quelle)
            .map_err(|e| halt(format!("{e:?}")))?;
        let kurse =
            Kurse::neu(zeit, vec![(isin.clone(), kurs)]).map_err(|e| halt(format!("{e:?}")))?;
        let topf = Topf::neu(TopfId::A, Cent(500_000)).map_err(|e| halt(format!("{e:?}")))?;
        j.beginne_abrechnung(datum).map_err(journal)?;
        let lage = Lage {
            anlass: None,
            datum,
            topf: &topf,
            kaufbar: topf.bargeld(),
            kurse: &kurse,
            verlauf: &[],
            kaufbilanz: crate::ausfuehrung::Kennzahlen::default(),
            relative_staerke: &[],
            positionskaeufe: &[],
            ereignisse: &["Meldung".into()],
        };
        let prognose = Weltprognose {
            isin,
            richtung: depot_journal::Prognoserichtung::Steigt,
            handelstage: 3,
            ereignis_nr: 1,
            begruendung: "Ursache".into(),
            widerlegung: "Gegenbeleg".into(),
        };
        // In dieser Probe fehlen der Kurs- und der Ereignisbeleg im Journal.
        belege_prognose(&mut j, &lage, BeraterId::Dachs, prognose)?;
        assert!(j.stand().weltprognosen().is_empty());
        assert!(j.stand().begruendungen().iter().any(|(_, _, t)| {
            t.starts_with("Prognose verworfen: Weltprognose: Ereignisnummer und Ereignistext")
        }));
        j.schliesse_ab(datum, Cent(500_000), &"a".repeat(64))
            .map_err(journal)?;
        assert_eq!(j.stand().seit_abschluss(), 0);
        drop(j);
        std::fs::remove_dir_all(ordner).map_err(|e| halt(e.to_string()))?;
        Ok(())
    }
}

/// #311 H2b: Eine verfallene KI-Kauforder wird einmal ohne neue KI-Antwort wiederholt.
#[cfg(test)]
mod wiederholung_tests {
    use super::*;
    use depot_journal::{KursBeleg, Meldung};
    use depot_kern::{Boerse, Datum, Herkunft, Instrument, InstrumentArt, Kurs};

    struct Probe(Option<String>);
    impl Berater for Probe {
        fn vorschlagen(
            &mut self,
            _: depot_journal::AufrufErlaubnis,
            lage: &Lage<'_>,
        ) -> crate::Antwort {
            self.0 = Some(lage.anlass.unwrap_or("ohne Anlass").to_owned());
            crate::Antwort {
                halten: std::collections::BTreeMap::new(),
                prognose: None,
                verwertbar: true,
                einschaetzung: "Bargeld".into(),
                vorschlag: None,
            }
        }
        fn pruefen(
            &mut self,
            _: depot_journal::AufrufErlaubnis,
            _: &Lage<'_>,
            _: &Vorschlag,
        ) -> crate::Urteil {
            unreachable!("ohne Vorschlag gibt es nichts zu prüfen")
        }
    }

    fn sap() -> Result<Isin, LaufFehler> {
        Isin::neu("DE0007164600").map_err(|e| halt(format!("{e:?}")))
    }

    fn datum(t: u8) -> Result<Datum, LaufFehler> {
        Datum::neu(2026, 11, t).ok_or_else(|| halt("Datum"))
    }

    fn schlusskurs(t: u8, cent: i64) -> Result<Kurs, LaufFehler> {
        let d = datum(t)?;
        let zeit = Boerse::Xetra.schluss(d).ok_or_else(|| halt("Schluss"))?;
        Kurs::neu(Cent(cent), "EUR", d, zeit, Herkunft::Quelle).map_err(|e| halt(format!("{e:?}")))
    }

    /// Freitag 13.11. Schluss 100,00 €, Montag 16.11. Kauf von 20 SAP mit Limit 101,00 €
    /// (100 bp), versandt um 7:30, verfallen am Montagsschluss `schluss_montag`. Mit
    /// `erstmeinung_montag` hatte Topf A am Montag eine Erstmeinung, sonst am Freitag.
    struct Probelauf {
        _ordner: std::path::PathBuf,
        j: Journal,
        topf: Topf,
        tag: Tag,
        universum: Universum,
        grenzen: Grenzen,
    }

    fn probelauf(
        name: &str,
        schluss_montag: i64,
        erstmeinung_montag: bool,
    ) -> Result<Probelauf, LaufFehler> {
        probelauf_mit(name, schluss_montag, erstmeinung_montag, 0)
    }

    /// Der Montagskauf über 20 SAP, versandt um 7:30 und am Montagsschluss verfallen.
    fn montagskauf(
        j: &mut Journal,
        grenzen: &Grenzen,
        universum: &Universum,
        isin: &Isin,
        alv: &Isin,
        basf: &Isin,
    ) -> Result<(), LaufFehler> {
        let freitag = schlusskurs(13, 10_000)?;
        let versand = Zeitpunkt::frankfurt(datum(16)?, 7, 30, 0).ok_or_else(|| halt("Versand"))?;
        let kurse_montag = Kurse::neu(
            versand,
            vec![
                (isin.clone(), freitag),
                (alv.clone(), schlusskurs(13, 10_000)?),
                (basf.clone(), schlusskurs(13, 10_000)?),
            ],
        )
        .map_err(|e| halt(format!("{e:?}")))?;
        // Der Montagskauf allein, ohne die Zusatzpositionen: Nur er wird später wiederholt.
        let nur_bargeld =
            Topf::neu(TopfId::A, Cent(1_000_000)).map_err(|e| halt(format!("{e:?}")))?;
        let ent = depot_regelwerk::entscheide_wiederholung(
            &Eingabe {
                grenzen,
                universum,
                topf: &nur_bargeld,
                kurse: &kurse_montag,
                vorschlag: None,
                halt: false,
                vor_valuta: &[],
            },
            &mut j.kennungsvergabe(),
            nur_bargeld.bargeld(),
            &depot_regelwerk::Wiederholung {
                isin: isin.clone(),
                stueck: 20,
                limit_bp: 100,
                stopp_bp: Some(500),
            },
        );
        let kauf = ent.kaeufe.first().ok_or_else(|| halt("Kauf"))?;
        j.reserviere_auftrag(grenzen, TopfId::A, kauf)
            .map_err(journal)?;
        let frist = Boerse::Xetra
            .schluss(datum(16)?)
            .ok_or_else(|| halt("Frist"))?;
        j.melde(1, &Meldung::GesendetAm(versand, frist))
            .map_err(journal)?;
        j.melde(1, &Meldung::Angenommen).map_err(journal)?;
        j.melde(1, &Meldung::Verfallen).map_err(journal)?;
        Ok(())
    }

    /// Je `allianz` Stück Allianz und BASF zu 100,00 € dazu: Über 50 % gebunden ist ein
    /// Kurssprung eines nicht gehaltenen Werts kein Anlass, die Prüfung der Wiederholung
    /// entscheidet.
    fn probelauf_mit(
        name: &str,
        schluss_montag: i64,
        erstmeinung_montag: bool,
        allianz: u64,
    ) -> Result<Probelauf, LaufFehler> {
        let ordner =
            std::env::temp_dir().join(format!("depot-wiederholung-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(&ordner).map_err(|e| halt(e.to_string()))?;
        let pfad = ordner.join("journal.txt");
        Journal::einrichten(&pfad).map_err(journal)?;
        let mut j = Journal::oeffnen(&pfad).map_err(journal)?;
        let mut topf = Topf::neu(TopfId::A, Cent(1_000_000)).map_err(|e| halt(format!("{e:?}")))?;
        let alv = Isin::neu("DE0008404005").map_err(|e| halt(format!("{e:?}")))?;
        let basf = Isin::neu("DE000BASF111").map_err(|e| halt(format!("{e:?}")))?;
        if allianz > 0 {
            topf.setze_bestand(alv.clone(), allianz);
            topf.setze_bestand(basf.clone(), allianz);
        }
        j.setze_topf(&topf).map_err(journal)?;
        let isin = sap()?;
        let freitag = schlusskurs(13, 10_000)?;
        let abruf = Zeitpunkt::frankfurt(datum(14)?, 9, 0, 0).ok_or_else(|| halt("Abruf"))?;
        j.belege_kurs(
            &KursBeleg::neu(
                "alphavantage",
                abruf,
                b"Probe",
                isin.clone(),
                freitag.clone(),
            )
            .map_err(|e| halt(format!("{e:?}")))?,
        )
        .map_err(journal)?;
        let erstmeinung = if erstmeinung_montag { 16 } else { 13 };
        j.reserviere(BeraterId::Dachs, datum(erstmeinung)?, 1)
            .map_err(|e| halt(format!("{e:?}")))?;
        let universum = Universum::neu(
            [(&isin, "SAP"), (&alv, "Allianz"), (&basf, "BASF")]
                .into_iter()
                .map(|(isin, name)| Instrument {
                    isin: isin.clone(),
                    name: name.into(),
                    art: InstrumentArt::Aktie,
                })
                .collect(),
        );
        let grenzen = Grenzen::standard();
        montagskauf(&mut j, &grenzen, &universum, &isin, &alv, &basf)?;
        let dienstag = Zeitpunkt::frankfurt(datum(17)?, 7, 30, 0).ok_or_else(|| halt("Morgen"))?;
        let montag = schlusskurs(16, schluss_montag)?;
        j.belege_kurs(
            &KursBeleg::neu(
                "alphavantage",
                dienstag,
                b"Probe",
                isin.clone(),
                montag.clone(),
            )
            .map_err(|e| halt(format!("{e:?}")))?,
        )
        .map_err(journal)?;
        let tag = Tag {
            datum: datum(17)?,
            kurse: Kurse::neu(
                dienstag,
                vec![
                    (isin, montag),
                    (alv, schlusskurs(16, 10_000)?),
                    (basf, schlusskurs(16, 10_000)?),
                ],
            )
            .map_err(|e| halt(format!("{e:?}")))?,
            ereignisse: Vec::new(),
        };
        Ok(Probelauf {
            _ordner: ordner,
            j,
            topf,
            tag,
            universum,
            grenzen,
        })
    }

    fn morgen(p: &mut Probelauf, anlass_pruefen: bool) -> Result<(TopfAntwort, Probe), LaufFehler> {
        let zeit = p.tag.kurse.stand();
        let uhr = move || Some(zeit);
        let z = Zyklus {
            anlass_pruefen,
            grenzen: &p.grenzen,
            universum: &p.universum,
            tag: &p.tag,
            vortag: &p.tag.kurse,
            nr: 2,
            abdruck: "Probe",
            uhr: Some(&uhr),
            vollstaendige_kurse: true,
            gesperrt: &BTreeSet::new(),
            bewertung: &p.tag.kurse,
        };
        let mut probe = Probe(None);
        let antwort = frage_topf(&mut p.j, &z, TopfId::A, &mut probe, &BTreeSet::new())?;
        if antwort.wiederholung.is_some() {
            let mut konto = crate::papier::Papierkonto::eroeffne(std::slice::from_ref(&p.topf))
                .ok_or_else(|| halt("Papierkonto"))?;
            topf_zyklus(&mut p.j, &z, antwort.clone(), &mut konto, &mut probe, 0)?;
        }
        Ok((antwort, probe))
    }

    /// Entschluss 100,00 €, 100 bp, Verfall bei Schluss 101,50 €: neue Order über 20 Stück mit
    /// Limit 101,50 × 1,01 = 102,515 €, abgerundet 102,51 € (≤ 103,00 €), Stopp übernommen.
    #[test]
    fn wiederholung_mit_neuem_limit() -> Result<(), LaufFehler> {
        let mut p = probelauf("limit", 10_150, true)?;
        let (antwort, probe) = morgen(&mut p, true)?;
        assert_eq!(probe.0, None, "KI nicht gefragt");
        assert!(antwort.wiederholung.is_some());
        let neu =
            p.j.stand()
                .auftraege()
                .iter()
                .find(|a| a.beleg.nr == 2)
                .ok_or_else(|| halt("neue Order"))?;
        assert_eq!(neu.beleg.stueck, 20);
        assert_eq!(neu.beleg.limit, Cent(10_251));
        assert_eq!(neu.beleg.stopp_bp, Some(500));
        assert_eq!(p.j.stand().ki().buchungen().len(), 1, "kein KI-Aufruf");
        assert!(
            p.j.stand()
                .begruendungen()
                .iter()
                .any(|(_, _, t)| t.starts_with("Topf A: Wiederholung von Auftrag 1: 20 Stück"))
        );
        Ok(())
    }

    /// Ein Schluss über Entschlusskurs × 1,03 macht die verfallene Order zum Anlass.
    #[test]
    fn keine_wiederholung_ueber_3_prozent() -> Result<(), LaufFehler> {
        let mut p = probelauf_mit("hoch", 10_350, true, 600)?;
        let (antwort, probe) = morgen(&mut p, true)?;
        assert!(antwort.wiederholung.is_none());
        let anlass = probe.0.ok_or_else(|| halt("KI nicht gefragt"))?;
        assert!(
            anlass.starts_with("verfallene Kauforder bei DE0007164600"),
            "{anlass}"
        );
        Ok(())
    }

    /// Ohne Erstmeinung am Versandtag war die Order schon eine Wiederholung: keine zweite.
    #[test]
    fn hoechstens_eine_wiederholung() -> Result<(), LaufFehler> {
        let mut p = probelauf("zweite", 10_150, false)?;
        let (antwort, probe) = morgen(&mut p, true)?;
        assert!(antwort.wiederholung.is_none());
        let anlass = probe.0.ok_or_else(|| halt("KI nicht gefragt"))?;
        assert!(anlass.contains("Wiederholung 1 verfallen"), "{anlass}");
        Ok(())
    }

    /// Hat der Topf ohnehin einen Anlass (hier der Handstart), fragt der Lauf die KI.
    #[test]
    fn anlass_schlaegt_wiederholung() -> Result<(), LaufFehler> {
        let mut p = probelauf("anlass", 10_150, true)?;
        let (antwort, probe) = morgen(&mut p, false)?;
        assert!(antwort.wiederholung.is_none());
        assert_eq!(probe.0.as_deref(), Some("Papiertag von Hand"));
        Ok(())
    }
}
