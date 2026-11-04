//! Der simulierte Lauf: Tag fuer Tag beobachten, fragen, pruefen lassen, entscheiden, auf
//! Papier ausfuehren, abschliessen. Alles steht im Journal.
//!
//! **Schutzrahmen statt Kaufregeln:** Was gekauft wird, schlagen Dachs und Maus vor und
//! begruenden es selbst. Der Lauf schreibt keine Abwaegung vor. Er sorgt nur dafuer, dass jede
//! Frage vorher gebucht ist (R8), dass nur eine ausdrueckliche Zweitmeinung einen Vorschlag
//! weiterlaesst (R31), dass das Regelwerk entscheidet (D01) und dass Daten- und Verlustgrenzen
//! halten (D03).
//!
//! **Ablauf eines Tages:** Ereignisse beobachten. Kurse pruefen; bei einem Fehler faellt der
//! Zyklus aus. Waechter: Tagesverlust oder unplausible Werte halten an, die Notbremse
//! verkauft alles und haelt an; der Tag bleibt dann offen. Dann je Topf: der zustaendige
//! Berater schlaegt vor, der andere prueft, das Regelwerk entscheidet, jeder Auftrag wird
//! belegt und auf Papier zum Schlusskurs ausgefuehrt. Zuletzt der Abschluss.
//!
//! **Wiederanlauf:** Der Lauf setzt nach dem letzten Abschluss fort. Hat das Journal Eintraege
//! nach dem letzten Abschluss, war ein Tag angefangen: Halt, und niemand wird neu gefragt.
//! Was ein angefangener Tag gekostet hat, steht im Journal; ihn zu wiederholen, waere eine
//! zweite Frage fuer denselben Zyklus.

use depot_journal::{Journal, JournalFehler};
use depot_kern::{BeraterId, Cent, Grenzen, Herkunft, Kurse, Topf, TopfId, Vorschlag};
use depot_ki::Sperre;
use depot_regelwerk::{
    DatenFehler, Eingabe, GenehmigterAuftrag, Richtung, WaechterBefund, entscheide,
    notbremse_auftraege, pruefe_kurse, waechter,
};

use crate::abdruck;
use crate::szenario::{Berater, Lage, Szenario, Tag};

/// Wer im Journal begruendet, ausser Dachs und Maus.
const DATEN: &str = "Daten";
const WAECHTER: &str = "Waechter";
const KOSTENSCHUTZ: &str = "Kostenschutz";
const REGELWERK: &str = "Regelwerk";
const SIMULATION: &str = "Simulation";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaufFehler {
    /// Der Lauf steht. Was bis dahin abgeschlossen ist, steht im Journal.
    Halt(String),
}

fn halt(text: impl Into<String>) -> LaufFehler {
    LaufFehler::Halt(text.into())
}

fn journal(e: JournalFehler) -> LaufFehler {
    halt(match e {
        JournalFehler::Belegt => "Journal belegt".into(),
        JournalFehler::Halt(t) => format!("Journal: Halt: {t}"),
        JournalFehler::Abgelehnt(t) => format!("Journal: abgelehnt: {t}"),
    })
}

fn rechnung(was: &str) -> LaufFehler {
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

fn depotwert<'a>(toepfe: impl Iterator<Item = &'a Topf>, kurse: &Kurse) -> Option<Cent> {
    toepfe
        .into_iter()
        .try_fold(Cent::NULL, |s, t| s.checked_add(t.wert(kurse)?))
}

/// Laeuft das Szenario ab dem Tag nach dem letzten Abschluss.
///
/// # Errors
/// `Halt` bei Kursen, die nicht erfunden sind, bei einem angefangenen Tag im Journal, bei
/// einem Journal, das nicht zum Szenario passt, bei einem Befund des Waechters und bei jedem
/// Fehler des Journals.
pub fn lauf(
    j: &mut Journal,
    s: &Szenario,
    dachs: &mut dyn Berater,
    maus: &mut dyn Berater,
) -> Result<(), LaufFehler> {
    nur_simulation(s)?;
    let st = j.stand();
    if st.seit_abschluss() > 0 {
        return Err(halt(format!(
            "{} Eintraege nach dem letzten Abschluss: ein Tag ist angefangen und nicht \
             abgeschlossen. Halt bis zur Pruefung; niemand wird neu gefragt",
            st.seit_abschluss()
        )));
    }
    // Der letzte Abschluss muss genau die Vorgeschichte dieses Szenarios abdrucken (K15-01).
    // Dann sind Zyklusnummern, Vergleichskurse und Stand bis dahin dieselben wie beim ersten
    // Lauf; nur die Tage danach duerfen anders sein.
    let abdruecke = abdruck::je_tag(s);
    let fertig = match st.abschluss() {
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
    let grenzen = Grenzen::standard();
    let mut hoch = if let Some((_, h)) = fertig {
        h
    } else {
        for t in &s.start {
            j.setze_topf(t).map_err(journal)?;
        }
        depotwert(s.start.iter(), &s.vorlauf).ok_or_else(|| rechnung("Startwert"))?
    };
    let mut vortag = &s.vorlauf;
    let mut beteiligte = Beteiligte { dachs, maus };
    for (i, (tag, abdruck)) in s.tage.iter().zip(&abdruecke).enumerate() {
        let nr = u32::try_from(i)
            .ok()
            .and_then(|z| z.checked_add(1))
            .ok_or_else(|| rechnung("Zyklus"))?;
        let fehler = pruefe_kurse(&grenzen, &s.universum, tag.datum, &tag.kurse, vortag);
        if fertig.is_none_or(|(index, _)| i > index) {
            let z = Zyklus {
                grenzen: &grenzen,
                szenario: s,
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

struct Beteiligte<'a, 'b> {
    dachs: &'a mut dyn Berater,
    maus: &'b mut dyn Berater,
}

/// Was fuer einen Tag fest ist.
struct Zyklus<'a> {
    grenzen: &'a Grenzen,
    szenario: &'a Szenario,
    tag: &'a Tag,
    /// Die Kurse des letzten plausiblen Tages davor.
    vortag: &'a Kurse,
    /// Zyklusnummer fuer den Kostenschutz: der wievielte Tag des Szenarios.
    nr: u32,
    /// Abdruck der Vorgeschichte bis einschliesslich dieses Tages.
    abdruck: &'a str,
}

/// Ein Tag. Liefert den Hoechststand nach dem Tag.
fn ein_tag(
    jr: &mut Journal,
    z: &Zyklus<'_>,
    hoch: Cent,
    fehler: &[DatenFehler],
    beteiligte: &mut Beteiligte<'_, '_>,
) -> Result<Cent, LaufFehler> {
    let tag = z.tag;
    for ereignis in &tag.ereignisse {
        jr.beobachte(tag.datum, &sauber(ereignis))
            .map_err(journal)?;
    }
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
    let heute = depotwert(toepfe.iter(), &tag.kurse);
    let befund = match (heute, depotwert(toepfe.iter(), z.vortag)) {
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
                fuehre_aus(jr, z, topf, &auftraege)?;
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
    let heute = heute.ok_or_else(|| rechnung("Depotwert"))?;
    topf_zyklus(jr, z, TopfId::A, beteiligte.dachs, beteiligte.maus)?;
    topf_zyklus(jr, z, TopfId::B, beteiligte.maus, beteiligte.dachs)?;
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

/// Ein Topf in einem Zyklus: Der zustaendige Berater schlaegt vor, der andere prueft, das
/// Regelwerk entscheidet.
fn topf_zyklus(
    j: &mut Journal,
    z: &Zyklus<'_>,
    id: TopfId,
    frager: &mut dyn Berater,
    pruefer: &mut dyn Berater,
) -> Result<(), LaufFehler> {
    let (tag, zyklus) = (z.tag, z.nr);
    let topf = j
        .stand()
        .toepfe()
        .get(&id)
        .cloned()
        .ok_or_else(|| halt(format!("{} fehlt im Journal", topf_name(id))))?;
    let (wer, zweiter) = (id.berater(), andere(id.berater()));
    let t = topf_name(id);
    let lage = Lage {
        datum: tag.datum,
        topf: &topf,
        kurse: &tag.kurse,
        ereignisse: &tag.ereignisse,
    };
    let mut vorschlag: Option<Vorschlag> = None;
    if let Some(e) = erlaubnis(j, tag, zyklus, id, wer, "kein Vorschlag")? {
        let antwort = frager.vorschlagen(e, &lage);
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
            universum: &z.szenario.universum,
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
    let auftraege: Vec<GenehmigterAuftrag> = ent.verkaeufe.into_iter().chain(ent.kaeufe).collect();
    fuehre_aus(j, z, &topf, &auftraege)
}

/// Belegt jeden Auftrag und fuehrt ihn auf Papier zum Schlusskurs aus, wenn der Kurs das Limit
/// einhaelt; sonst verfaellt er. Verkaeufe kommen vor Kaeufen. Jede Ausfuehrung kostet die
/// Gebuehr nach Tarif. Bargeld und Bestand werden nie negativ; sonst Halt.
fn fuehre_aus(
    j: &mut Journal,
    z: &Zyklus<'_>,
    topf: &Topf,
    auftraege: &[GenehmigterAuftrag],
) -> Result<(), LaufFehler> {
    let (g, tag) = (z.grenzen, z.tag);
    if auftraege.is_empty() {
        return Ok(());
    }
    let id = topf.id();
    let t = topf_name(id);
    let mut bargeld = topf.bargeld();
    let mut bestand = topf.positionen().clone();
    for a in auftraege {
        j.belege(id, a).map_err(journal)?;
        let auf = a.auftrag();
        let nr = auf.kennung.nr();
        let kurs = tag
            .kurse
            .kurs(&auf.isin)
            .ok_or_else(|| halt(format!("Kurs von {} fehlt", auf.isin)))?;
        let im_limit = match auf.richtung {
            Richtung::Kauf => kurs <= auf.limit,
            Richtung::Verkauf => kurs >= auf.limit,
        };
        if !im_limit {
            begruende(
                j,
                tag,
                SIMULATION,
                &format!(
                    "{t}: Auftrag {nr} verfaellt: Kurs {} ausserhalb des Limits",
                    kurs.0
                ),
            )?;
            continue;
        }
        let volumen = kurs
            .mal_stueck(auf.stueck)
            .ok_or_else(|| rechnung("Volumen"))?;
        let gebuehr = g
            .gebuehren()
            .fuer(volumen)
            .ok_or_else(|| rechnung("Gebuehr"))?;
        let gehalten = bestand.get(&auf.isin).copied().unwrap_or(0);
        let (neu_bargeld, neu_stueck) = match auf.richtung {
            Richtung::Kauf => (
                bargeld
                    .checked_sub(volumen)
                    .and_then(|b| b.checked_sub(gebuehr)),
                gehalten.checked_add(auf.stueck),
            ),
            Richtung::Verkauf => (
                bargeld
                    .checked_add(volumen)
                    .and_then(|b| b.checked_sub(gebuehr)),
                gehalten.checked_sub(auf.stueck),
            ),
        };
        let (Some(neu_bargeld), Some(neu_stueck)) = (neu_bargeld, neu_stueck) else {
            return Err(halt(format!("{t}: Auftrag {nr} nicht ausfuehrbar")));
        };
        if neu_bargeld.0 < 0 {
            return Err(halt(format!("{t}: Auftrag {nr}: Bargeld wuerde negativ")));
        }
        bargeld = neu_bargeld;
        if neu_stueck == 0 {
            bestand.remove(&auf.isin);
        } else {
            bestand.insert(auf.isin.clone(), neu_stueck);
        }
        begruende(
            j,
            tag,
            SIMULATION,
            &format!(
                "{t}: Auftrag {nr} ausgefuehrt: {:?} {} Stueck {} zu {} Cent, Gebuehr {} Cent",
                auf.richtung, auf.stueck, auf.isin, kurs.0, gebuehr.0
            ),
        )?;
    }
    let mut neu = Topf::neu(id, bargeld).map_err(|e| halt(format!("{t}: {e:?}")))?;
    for (isin, stueck) in bestand {
        neu.setze_bestand(isin, stueck);
    }
    j.setze_topf(&neu).map_err(journal)
}
