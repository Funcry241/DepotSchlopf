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

use depot_journal::{AuftragsZustand, Journal, JournalFehler, Meldung};
use depot_kern::{
    BeraterId, Cent, Grenzen, Herkunft, Kurse, Rundung, Topf, TopfId, Universum, Vorschlag,
};
use depot_ki::Sperre;
use depot_regelwerk::{
    DatenFehler, Eingabe, GenehmigterAuftrag, WaechterBefund, entscheide, notbremse_auftraege,
    pruefe_kurse, waechter,
};

use crate::abdruck;
use crate::papier::{Broker, Sendeantwort};
use crate::szenario::{Berater, Lage, Szenario, Tag};

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

/// Der Kontowaechter: Abgleich mit dem Konto. Jede Abweichung steht im Journal und haelt den
/// Lauf an; nichts wird verkauft oder ausgeglichen.
pub(crate) fn kontowaechter(
    j: &mut Journal,
    tag: &Tag,
    konto: &dyn Broker,
) -> Result<(), LaufFehler> {
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
    kontowaechter(jr, tag, &*beteiligte.konto)?;
    if !fehler.is_empty() {
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
            begruende(
                jr,
                tag,
                WAECHTER,
                &format!("{befund:?}: Halt bis zur Freigabe, der Tag bleibt offen"),
            )?;
            return Err(halt(format!("Waechter: {befund:?}")));
        }
    }
    // Fuer den Hoechststand zaehlt der aufgerundete Wert (siehe `depotwert`).
    let heute =
        depotwert(toepfe.iter(), &tag.kurse, Rundung::Auf).ok_or_else(|| rechnung("Depotwert"))?;
    let b = beteiligte;
    topf_zyklus(jr, z, TopfId::A, b.dachs, b.maus, &mut *b.konto)?;
    topf_zyklus(jr, z, TopfId::B, b.maus, b.dachs, &mut *b.konto)?;
    let hoch = hoch.max(heute);
    jr.schliesse_ab(tag.datum, hoch, z.abdruck)
        .map_err(journal)?;
    Ok(hoch)
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
            &format!("{t}: nicht angebunden, nicht gefragt; kein Vorschlag"),
        )?;
    } else if let Some(e) = erlaubnis(j, tag, zyklus, id, wer, "kein Vorschlag")? {
        let antwort = frager.vorschlagen(e, lage);
        begruende(
            j,
            tag,
            name(wer),
            &format!("{t}: {}", antwort.einschaetzung),
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
    let mut vorschlag = frage(j, z, id, frager, &lage)?;
    if vorschlag.is_some() && !pruefer.angebunden() {
        vorschlag = None;
        begruende(
            j,
            tag,
            name(zweiter),
            &format!("{t}: nicht angebunden, nicht gefragt; ohne Zweitmeinung kein Auftrag"),
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
        let wort = if urteil.zustimmung {
            "Zustimmung"
        } else {
            "Veto"
        };
        begruende(
            j,
            tag,
            name(zweiter),
            &format!("{t}: {wort}: {}", urteil.begruendung),
        )?;
        if urteil.zustimmung {
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
    // alle Verkaeufe ausgefuehrt sind; sonst verfallen sie.
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
        j.melde(nr, &Meldung::Gesendet).map_err(journal)?;
        match konto.sende(a) {
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
    konto.markt(&tag.kurse);
    kontowaechter(j, tag, &*konto)?;
    for nr in gesendet {
        let Some(l) = j.stand().auftrag(nr).cloned() else {
            return Err(halt(format!("{t}: Auftrag {nr} fehlt im Journal")));
        };
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
            begruende(
                j,
                tag,
                SIMULATION,
                &format!("{t}: Auftrag {nr} verfaellt: Kurs ausserhalb des Limits"),
            )?;
        }
    }
    Ok(())
}
