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
    DatenFehler, Eingabe, GenehmigterAuftrag, Richtung, WaechterBefund, entscheide_mit_kaufbudget,
    notbremse_auftraege, pruefe_kurse, waechter,
};

use crate::abdruck;
use crate::papier::{Broker, Sendeantwort};
use crate::szenario::{Berater, KEINE_VERWERTBARE_ANTWORT, Lage, Szenario, Tag};

/// Wer im Journal begruendet, ausser Dachs und Maus.
const DATEN: &str = "Daten";
const WAECHTER: &str = "Waechter";
const KOSTENSCHUTZ: &str = "Kostenschutz";
const REGELWERK: &str = "Regelwerk";
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

fn topf_name(t: TopfId) -> &'static str {
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
    let mut beteiligte = Beteiligte { dachs, maus, konto };
    for (i, (tag, abdruck)) in s.tage.iter().zip(&abdruecke).enumerate() {
        let nr = u32::try_from(i)
            .ok()
            .and_then(|z| z.checked_add(1))
            .ok_or_else(|| rechnung("Zyklus"))?;
        let fehler = pruefe_kurse(&grenzen, &s.universum, tag.datum, &tag.kurse, vortag);
        if fertig.is_none_or(|(index, _)| i > index) {
            let z = Zyklus {
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

pub(crate) struct Beteiligte<'a, 'b, 'c> {
    pub(crate) dachs: &'a mut dyn Berater,
    pub(crate) maus: &'b mut dyn Berater,
    pub(crate) konto: &'c mut dyn Broker,
}

/// Was fuer einen Tag fest ist.
pub(crate) struct Zyklus<'a> {
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
            if z.uhr.is_some() {
                stoppe_kaeufe(jr, tag, &mut *beteiligte.konto)?;
            }
            begruende(
                jr,
                tag,
                WAECHTER,
                &format!("{befund:?}: Halt bis zur Freigabe, der Tag bleibt offen"),
            )?;
            return Err(halt(format!("Wächter: {befund:?}")));
        }
    }
    if z.uhr.is_some() {
        markt_offener_orders(jr, z, &mut *beteiligte.konto, true)?;
        if pruefe_papier_verlustsperre(jr, z, hoch, true)? {
            return papier_notbremse(jr, z, hoch, &mut *beteiligte.konto, true);
        }
    }
    // Fuer den Hoechststand zaehlt der aufgerundete Wert (siehe `depotwert`).
    let heute = depotwert(jr.stand().toepfe().values(), z.bewertung, Rundung::Auf)
        .ok_or_else(|| rechnung("Depotwert"))?;
    let b = beteiligte;
    let topf_a = frage_topf(jr, z, TopfId::A, b.dachs)?;
    let topf_b = frage_topf(jr, z, TopfId::B, b.maus)?;
    let erste = [topf_a.clone(), topf_b.clone()];
    let topf_a = zweite_meinung(jr, z, topf_a, b.maus)?;
    let topf_b = zweite_meinung(jr, z, topf_b, b.dachs)?;
    let [topf_a, topf_b] =
        crate::abstimmung::berate(jr, z, erste, [topf_a, topf_b], b.dachs, b.maus)?;
    beide_topfzyklen(jr, z, [topf_a, topf_b], &mut *b.konto)?;
    let hoch = if z.gesperrt.is_empty() {
        hoch.max(heute)
    } else {
        hoch
    };
    jr.schliesse_ab(tag.datum, hoch, z.abdruck)
        .map_err(journal)?;
    Ok(hoch)
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
    konto: &mut dyn Broker,
) -> Result<(), LaufFehler> {
    if !beratungszeit(z) {
        return begruende(
            j,
            z.tag,
            DATEN,
            "Beratung beendet: Datenfrische abgelaufen oder Uhr unklar. Keine neuen Aufträge.",
        );
    }
    for antwort in antworten {
        topf_zyklus(j, z, antwort, konto)?;
    }
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
) -> Result<(Option<Vorschlag>, bool), LaufFehler> {
    let (tag, zyklus) = (z.tag, z.nr);
    let wer = id.berater();
    let t = topf_name(id);
    let mut vorschlag: Option<Vorschlag> = None;
    let mut verwertbar = false;
    // Ein Berater ohne Anbindung wird nicht gefragt und kostet nichts: Fuer ihn wird keine
    // Erlaubnis reserviert. Ohne Vorschlag oder ohne Zweitmeinung gibt es keinen Auftrag.
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
        if let Some(a) = frager.abrechnung() {
            j.rechne_ki_ab(&a).map_err(journal)?;
        }
        verwertbar = antwort.verwertbar;
        let wie = if antwort.verwertbar {
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
        if let Some(v) = &antwort.vorschlag {
            for (isin, bp) in &v.zielgewichte_bp {
                let grund = v.begruendung.get(isin).map_or("", String::as_str);
                begruende(j, tag, name(wer), &format!("{t}: {isin} {bp} bp: {grund}"))?;
            }
        }
        vorschlag = antwort.vorschlag;
    }
    Ok((vorschlag, verwertbar))
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

#[derive(Clone)]
pub(crate) struct TopfAntwort {
    pub(crate) topf: Topf,
    pub(crate) vorschlag: Option<Vorschlag>,
    pub(crate) erste_verwertbar: bool,
}

fn prognosebilanz(stand: &depot_journal::Stand) -> Vec<(BeraterId, depot_journal::Auswertung)> {
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

fn frage_topf(
    j: &mut Journal,
    z: &Zyklus<'_>,
    id: TopfId,
    frager: &mut dyn Berater,
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
    let lage = Lage {
        datum: tag.datum,
        topf: &topf,
        kaufbar: kaufbudget(j, z.grenzen, id)?,
        kurse: &tag.kurse,
        verlauf: &verlauf,
        kaufbilanz: crate::ausfuehrung::kaufbilanz(j.stand(), topf.id(), tag.datum),
        positionskaeufe: &positionskaeufe,
        ereignisse: &tag.ereignisse,
    };
    let wartet = j
        .stand()
        .offene_auftraege()
        .find(|l| l.versand.is_some() && l.beleg.topf == id)
        .map(|l| l.beleg.nr);
    let gesperrt = z.gesperrt.iter().find(|isin| topf.stueck(isin) != 0);
    let (vorschlag, erste_verwertbar) = if let Some(isin) = gesperrt {
        begruende(
            j,
            tag,
            AUFTRAGSSTELLE,
            &format!("{t}: {isin} heute gesperrt; keine freiwilligen Aufträge, KI nicht gefragt."),
        )?;
        (None, false)
    } else if let Some(nr) = wartet {
        begruende(
            j,
            tag,
            AUFTRAGSSTELLE,
            &format!("{t}: wartet auf offenen Auftrag {nr}"),
        )?;
        (None, false)
    } else {
        frage(j, z, id, frager, &lage)?
    };
    Ok(TopfAntwort {
        topf,
        vorschlag,
        erste_verwertbar,
    })
}

/// Der zweite Berater prueft den Vorschlag, dann entscheidet das Regelwerk.
fn zweite_meinung(
    j: &mut Journal,
    z: &Zyklus<'_>,
    antwort: TopfAntwort,
    pruefer: &mut dyn Berater,
) -> Result<TopfAntwort, LaufFehler> {
    let TopfAntwort {
        topf,
        mut vorschlag,
        erste_verwertbar,
    } = antwort;
    let (tag, zyklus) = (z.tag, z.nr);
    let id = topf.id();
    let zweiter = andere(id.berater());
    let t = topf_name(id);
    let positionskaeufe = crate::positionskauf::lies(j.stand(), &topf);
    let verlauf = crate::kursverlauf::fuer_lage(j.stand(), &tag.kurse, z.universum, tag.datum);
    let lage = Lage {
        datum: tag.datum,
        topf: &topf,
        kaufbar: kaufbudget(j, z.grenzen, id)?,
        kurse: &tag.kurse,
        verlauf: &verlauf,
        kaufbilanz: crate::ausfuehrung::kaufbilanz(j.stand(), topf.id(), tag.datum),
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
        if let Some(a) = pruefer.abrechnung() {
            j.rechne_ki_ab(&a).map_err(journal)?;
        }
        let wort = match (urteil.verwertbar, urteil.zustimmung) {
            (true, true) => ZUSTIMMUNG,
            (true, false) => VETO,
            (false, _) => URTEIL_VERWORFEN,
        };
        begruende(
            j,
            tag,
            name(zweiter),
            &format!("{t}: {wort}: {}", urteil.begruendung),
        )?;
        if urteil.verwertbar && urteil.zustimmung {
            vorschlag = Some(v);
        }
    }
    Ok(TopfAntwort {
        topf,
        vorschlag,
        erste_verwertbar,
    })
}

fn topf_zyklus(
    j: &mut Journal,
    z: &Zyklus<'_>,
    antwort: TopfAntwort,
    konto: &mut dyn Broker,
) -> Result<(), LaufFehler> {
    let TopfAntwort {
        topf, vorschlag, ..
    } = antwort;
    let tag = z.tag;
    let id = topf.id();
    let t = topf_name(id);
    let mut vergabe = j.kennungsvergabe();
    let ent = entscheide_mit_kaufbudget(
        &Eingabe {
            grenzen: z.grenzen,
            universum: z.universum,
            topf: &topf,
            kurse: &tag.kurse,
            vorschlag: vorschlag.as_ref(),
            halt: false,
        },
        &mut vergabe,
        kaufbudget(j, z.grenzen, id)?,
    );
    for b in &ent.befunde {
        let grund = match b {
            depot_regelwerk::Befund::LimitabstandAusserhalb {
                isin,
                gefragt,
                verwendet,
            } => format!(
                "{isin}: Limitabstand {gefragt} bp außerhalb des Rahmens, {verwendet} bp verwendet"
            ),
            _ => format!("{b:?}"),
        };
        begruende(j, tag, REGELWERK, &format!("{t}: {grund}"))?;
    }
    if ent.verkaeufe.is_empty() {
        return fuehre_aus(j, z, id, &ent.kaeufe, vorschlag.as_ref(), konto);
    }
    let alle = fuehre_aus(j, z, id, &ent.verkaeufe, None, konto);
    // Positions- und Anteilsgrenzen können einen ausgeführten Verkauf voraussetzen.
    // Der geplante Erlös steht für Käufe erst nach der Abrechnung zur Verfügung.
    match alle {
        Ok(()) if verkauft(j, &ent.verkaeufe) => {
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

/// Sind alle diese Auftraege ganz ausgefuehrt?
fn verkauft(j: &Journal, auftraege: &[GenehmigterAuftrag]) -> bool {
    auftraege.iter().all(|a| {
        j.stand()
            .auftrag(a.auftrag().kennung.nr())
            .map(|l| l.zustand)
            == Some(AuftragsZustand::Gefuellt)
    })
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
        // Erst nach der Reservierung und vor dem Versand dauerhaft belegen. In der
        // Folgeberatung zählt diese These erst, sobald der Auftrag ausgeführt wurde.
        if let Some(these) = vorschlag
            .and_then(|v| v.begruendung.get(&a.auftrag().isin))
            .filter(|text| !text.trim().is_empty())
        {
            begruende(
                j,
                tag,
                crate::positionskauf::KAUFTHESE,
                &format!("{nr} {these}"),
            )?;
        }
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
    fn laengerer_dialog_verlaengert_die_kursfrist_nicht() -> Result<(), LaufFehler> {
        let (szenario, _, _) = crate::Szenario::beispiel().ok_or_else(|| halt("Szenario"))?;
        let tag = szenario.tage.first().ok_or_else(|| halt("Tag"))?;
        let stand = tag.kurse.stand();
        let jetzt = std::cell::Cell::new(Some(stand));
        let uhr = || jetzt.get();
        let grenzen = Grenzen::standard();
        let zyklus = Zyklus {
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
            datum,
            topf: &topf,
            kaufbar: topf.bargeld(),
            kurse: &kurse,
            verlauf: &[],
            kaufbilanz: crate::ausfuehrung::Kennzahlen::default(),
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
        assert!(
            j.stand()
                .begruendungen()
                .iter()
                .any(|(_, _, t)| t.starts_with("Prognose verworfen:"))
        );
        j.schliesse_ab(datum, Cent(500_000), &"a".repeat(64))
            .map_err(journal)?;
        assert_eq!(j.stand().seit_abschluss(), 0);
        drop(j);
        std::fs::remove_dir_all(ordner).map_err(|e| halt(e.to_string()))?;
        Ok(())
    }
}
