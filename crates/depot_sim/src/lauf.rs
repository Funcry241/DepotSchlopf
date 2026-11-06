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

use depot_journal::{AuftragsZustand, Journal, JournalFehler, Meldung, Stand};
use depot_kern::{
    BeraterId, Cent, Grenzen, Herkunft, Kurse, Rundung, Topf, TopfId, Universum, Vorschlag,
    Zeitpunkt,
};
use depot_kern::liste::papierorder_schluss;
use depot_ki::Sperre;
use depot_regelwerk::{
    DatenFehler, Eingabe, GenehmigterAuftrag, Richtung, WaechterBefund, entscheide,
    notbremse_auftraege, pruefe_kurse, waechter,
};

use crate::abdruck;
use crate::papier::{Broker, Sendeantwort};
use crate::szenario::{Berater, KEINE_VERWERTBARE_ANTWORT, Lage, Szenario, Tag};

/// Wer im Journal begruendet, ausser Dachs und Maus.
const DATEN: &str = "Daten";
const WAECHTER: &str = "Waechter";
const NOTBREMSE_AKTIV: &str = "Notbremse aktiv: nur offene Verkaeufe weiter abgleichen";
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

fn begruende(j: &mut Journal, tag: &Tag, wer: &str, text: &str) -> Result<(), LaufFehler> {
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
                "Kontowaechter beim Wiederanlauf: {abweichungen:?}. Halt bis zur Pruefung; \
                 niemand wird gefragt, nichts gesendet"
            )));
        }
    }
    let st = j.stand();
    if st.seit_abschluss() > 0 {
        return Err(halt(format!(
            "{} Eintraege nach dem letzten Abschluss: ein Tag ist angefangen und nicht \
             abgeschlossen. Halt bis zur Pruefung; niemand wird neu gefragt",
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
            "Konto gestoert: {s}. Halt; nichts wird mehr bestaetigt"
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
        "Abweichung zum Konto: Halt bis zur Pruefung; kein Verkauf, kein Ausgleich",
    )?;
    Err(halt(format!("Kontowaechter: {abweichungen:?}")))
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
    if z.uhr.is_some() && notbremse_aktiv(jr.stand()) {
        return papier_notbremse(jr, z, hoch, &mut *beteiligte.konto, fehler.is_empty());
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
    let heute = depotwert(toepfe.iter(), &tag.kurse, Rundung::Ab);
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
                fuehre_aus(jr, z, topf.id(), &auftraege, &mut *beteiligte.konto)?;
            }
            return Err(halt(format!("Waechter: {befund:?}")));
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
            return Err(halt(format!("Waechter: {befund:?}")));
        }
    }
    if z.uhr.is_some() {
        markt_offener_orders(jr, z, &mut *beteiligte.konto, true)?;
    }
    // Fuer den Hoechststand zaehlt der aufgerundete Wert (siehe `depotwert`).
    let heute = depotwert(jr.stand().toepfe().values(), &tag.kurse, Rundung::Auf)
        .ok_or_else(|| rechnung("Depotwert"))?;
    let b = beteiligte;
    topf_zyklus(jr, z, TopfId::A, b.dachs, b.maus, &mut *b.konto)?;
    topf_zyklus(jr, z, TopfId::B, b.maus, b.dachs, &mut *b.konto)?;
    let hoch = hoch.max(heute);
    jr.schliesse_ab(tag.datum, hoch, z.abdruck)
        .map_err(journal)?;
    Ok(hoch)
}

pub(crate) fn notbremse_aktiv(stand: &Stand) -> bool {
    stand
        .begruendungen()
        .iter()
        .any(|(_, wer, text)| wer == WAECHTER && text == NOTBREMSE_AKTIV)
}

fn stoppe_kaeufe(j: &mut Journal, tag: &Tag, konto: &mut dyn Broker) -> Result<(), LaufFehler> {
    if j.stand()
        .offene_auftraege()
        .any(|l| l.beleg.richtung == Richtung::Kauf)
    {
        if !konto.stoppe_kaeufe() {
            return Err(halt("Offene Kaeufe konnten nicht sicher storniert werden"));
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

fn papier_notbremse(
    j: &mut Journal,
    z: &Zyklus<'_>,
    hoch: Cent,
    konto: &mut dyn Broker,
    kurse_ok: bool,
) -> Result<Cent, LaufFehler> {
    stoppe_kaeufe(j, z.tag, konto)?;
    if !notbremse_aktiv(j.stand()) {
        begruende(j, z.tag, WAECHTER, NOTBREMSE_AKTIV)?;
    }
    markt_offener_orders(j, z, konto, kurse_ok)?;
    if kurse_ok {
        erneuere_schutzverkaeufe(j, z, konto)?;
    }
    j.schliesse_ab(z.tag.datum, hoch, z.abdruck)
        .map_err(journal)?;
    Err(halt(NOTBREMSE_AKTIV))
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
        let mut vergabe = j.kennungsvergabe();
        let auftraege = notbremse_auftraege(z.grenzen, &topf, &z.tag.kurse, &mut vergabe)
            .ok_or_else(|| halt("Notbremse: Kurs fehlt; Halt ohne Verkauf"))?;
        fuehre_aus(j, z, topf.id(), &auftraege, konto)?;
    }
    Ok(())
}

fn markt_offener_orders(
    j: &mut Journal,
    z: &Zyklus<'_>,
    konto: &mut dyn Broker,
    kurse_ok: bool,
) -> Result<(), LaufFehler> {
    let offen: Vec<_> = j.stand().offene_auftraege().map(|l| l.beleg.nr).collect();
    if offen.is_empty() {
        return Ok(());
    }
    let kurse = &z.tag.kurse;
    let handelbar = z.universum.isins();
    let eintraege = kurse
        .alle()
        .filter(|(isin, _)| kurse_ok && handelbar.contains(*isin))
        .filter_map(|(isin, kurs)| Some((isin.clone(), kurs.clone(), kurse.abruf(isin)?)))
        .collect();
    let stand = z
        .uhr
        .and_then(|uhr| uhr())
        .filter(|zeit| *zeit >= kurse.stand())
        .ok_or_else(|| halt("Keine verlaessliche Marktzeit"))?;
    let kurse = Kurse::mit_abrufen(stand, eintraege)
        .map_err(|e| halt(format!("Kurse fuer offene Orders: {e:?}")))?;
    konto.markt(&kurse);
    kontowaechter(j, z.tag, &*konto)?;
    protokolliere_ausfuehrungen(j, z.tag, &offen, Some(&kurse))
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
) -> Result<Option<depot_journal::AufrufErlaubnis>, LaufFehler> {
    match j.reserviere(berater, tag.datum, zyklus) {
        Ok(e) => Ok(Some(e)),
        Err(Sperre::Kassenbuch(e)) => Err(halt(format!("Kassenbuch: {e}"))),
        Err(sperre) => {
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
) -> Result<Option<Vorschlag>, LaufFehler> {
    let (tag, zyklus) = (z.tag, z.nr);
    let wer = id.berater();
    let t = topf_name(id);
    let mut vorschlag: Option<Vorschlag> = None;
    // Ein Berater ohne Anbindung wird nicht gefragt und kostet nichts: Fuer ihn wird keine
    // Erlaubnis reserviert. Ohne Vorschlag oder ohne Zweitmeinung gibt es keinen Auftrag.
    if !frager.angebunden() {
        begruende(
            j,
            tag,
            name(wer),
            &format!(
                "{t}: {}, nicht gefragt; kein Vorschlag",
                nicht_angebunden(frager)
            ),
        )?;
    } else if let Some(e) = erlaubnis(j, tag, zyklus, id, wer, "kein Vorschlag")? {
        let mut antwort = frager.vorschlagen(e, lage);
        let wie = if antwort.verwertbar {
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
    Ok(vorschlag)
}

/// Ein Topf in einem Zyklus: Der zustaendige Berater schlaegt vor, der andere prueft, das
/// Regelwerk entscheidet.
fn topf_zyklus(
    j: &mut Journal,
    z: &Zyklus<'_>,
    id: TopfId,
    frager: &mut dyn Berater,
    pruefer: &mut dyn Berater,
    konto: &mut dyn Broker,
) -> Result<(), LaufFehler> {
    let (tag, zyklus) = (z.tag, z.nr);
    let topf = j
        .stand()
        .toepfe()
        .get(&id)
        .cloned()
        .ok_or_else(|| halt(format!("{} fehlt im Journal", topf_name(id))))?;
    let zweiter = andere(id.berater());
    let t = topf_name(id);
    let lage = Lage {
        datum: tag.datum,
        topf: &topf,
        kurse: &tag.kurse,
        ereignisse: &tag.ereignisse,
    };
    let wartet = j
        .stand()
        .offene_auftraege()
        .find(|l| l.versand.is_some() && l.beleg.topf == id)
        .map(|l| l.beleg.nr);
    let mut vorschlag = if let Some(nr) = wartet {
        begruende(
            j,
            tag,
            AUFTRAGSSTELLE,
            &format!("{t}: wartet auf offenen Auftrag {nr}"),
        )?;
        None
    } else {
        frage(j, z, id, frager, &lage)?
    };
    if vorschlag.is_some() && !pruefer.angebunden() {
        vorschlag = None;
        begruende(
            j,
            tag,
            name(zweiter),
            &format!(
                "{t}: {}, nicht gefragt; ohne Zweitmeinung kein Auftrag",
                nicht_angebunden(pruefer)
            ),
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
        )?
    {
        let urteil = pruefer.pruefen(e, &lage, &v);
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
    let mut vergabe = j.kennungsvergabe();
    let ent = entscheide(
        &Eingabe {
            grenzen: z.grenzen,
            universum: z.universum,
            topf: &topf,
            kurse: &tag.kurse,
            vorschlag: vorschlag.as_ref(),
            halt: false,
        },
        &mut vergabe,
    );
    for b in &ent.befunde {
        begruende(j, tag, REGELWERK, &format!("{t}: {b:?}"))?;
    }
    if ent.verkaeufe.is_empty() {
        return fuehre_aus(j, z, id, &ent.kaeufe, konto);
    }
    let alle = fuehre_aus(j, z, id, &ent.verkaeufe, konto);
    // Das Regelwerk rechnet die Kaeufe mit dem Erloes der Verkaeufe: Sie gehen nur raus, wenn
    // alle Verkaeufe ausgefuehrt sind und das bereits abgerechnete Geld ausreicht.
    match alle {
        Ok(()) if verkauft(j, &ent.verkaeufe) => fuehre_aus(j, z, id, &ent.kaeufe, konto),
        Ok(()) if ent.kaeufe.is_empty() => Ok(()),
        Ok(()) => begruende(
            j,
            tag,
            SIMULATION,
            &format!("{t}: Kaeufe entfallen: nicht alle Verkaeufe ausgefuehrt"),
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
    konto: &mut dyn Broker,
) -> Result<(), LaufFehler> {
    let tag = z.tag;
    if auftraege.is_empty() {
        return Ok(());
    }
    let t = topf_name(id);
    let mut gesendet = Vec::new();
    for a in auftraege {
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
        let versand = z
            .uhr
            .map(|uhr| {
                uhr()
                    .filter(|zeit| *zeit >= tag.kurse.stand())
                    .ok_or_else(|| halt("Keine verlaessliche Versandzeit"))
            })
            .transpose()?;
        let meldung = match versand {
            Some(zeit) => {
                let frist = papierorder_schluss(&a.auftrag().isin, zeit)
                    .ok_or_else(|| halt("Keine Boersenfrist fuer den Auftrag"))?;
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
    papierkurse: Option<&Kurse>,
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
                    "{t}: Auftrag {nr} ausgefuehrt: {:?} {} Stueck {} zu {} Cent, Gebuehr {} Cent",
                    l.beleg.richtung, a.stueck, l.beleg.isin, a.kurs.0, a.gebuehr.0
                ),
            )?;
        }
        if l.zustand == AuftragsZustand::Verfallen {
            let grund = match (papierkurse, l.frist) {
                (Some(kurse), Some(schluss)) => {
                    if schluss.datum() != kurse.stand().datum() {
                        "Boersentag der Order bereits vorbei"
                    } else if kurse.eintrag(&l.beleg.isin).map(depot_kern::Kurs::zeitpunkt) != Some(schluss) {
                        "kein gepruefter Kurs am Boersenschluss"
                    } else {
                        "Kurs ausserhalb des Limits"
                    }
                }
                _ => "Kurs ausserhalb des Limits",
            };
            begruende(
                j,
                tag,
                SIMULATION,
                &format!("{t}: Auftrag {nr} verfaellt: {grund}"),
            )?;
        }
    }
    Ok(())
}
