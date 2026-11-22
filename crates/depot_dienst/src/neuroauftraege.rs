//! Dauerhafte Übergabe lokaler Fälle und asynchrone Abwicklung externer Fragen.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use depot_journal::neurofrage::{
    NeuroAuftragId, NeuroErgebnis, NeuroFrage, NeuroRunde, NeuroUrteilswert, ergebnis_pfad,
};
use depot_journal::{Art, Journal, KiStand, Lesecache, Stand, ki_einstellungen};
use depot_kern::modelle::KiEinstellungen;
use depot_kern::{BeraterId, Datum};
use depot_lage::blindpruefung::Prueffall;
use depot_lage::{NeuroFragefall, datum_text, html::maskiere};

use crate::werkbank::{Dienst, schreibe_ganz};

const NACHRICHT_MAX: u64 = 40_000;
const BEFUNDE_MAX: usize = 20;
const KURZFRIST: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug)]
pub(crate) enum Fragenstand {
    Journal,
    Tageslimit,
    Vorgemerkt(BeraterId),
    Wartet,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Fragenvorschau {
    tag: Datum,
    einstellungen: KiEinstellungen,
    stand: Fragenstand,
}

impl Fragenvorschau {
    pub(crate) fn stand(self, tag: Option<Datum>, e: KiEinstellungen) -> Option<Fragenstand> {
        (Some(self.tag) == tag
            && self.einstellungen.neuroschlopf == e.neuroschlopf
            && self.einstellungen.neuro_empfaenger == e.neuro_empfaenger)
            .then_some(self.stand)
    }
}

fn sammlung(daten: &Path) -> PathBuf {
    daten.join("neuroschlopf-fragen")
}

fn kurz(wer: BeraterId) -> &'static str {
    match wer {
        BeraterId::Dachs => "dachs",
        BeraterId::Maus => "maus",
    }
}

fn dateiname(frage: &NeuroFrage) -> String {
    format!(
        "{}-{}-{}.frage.json",
        frage.id.fall(),
        kurz(frage.wer),
        frage.id.runde().text(),
    )
}

fn lese(pfad: &Path, maximum: u64) -> Option<String> {
    if !fs::symlink_metadata(pfad).ok()?.is_file() {
        return None;
    }
    let mut text = String::new();
    fs::File::open(pfad)
        .ok()?
        .take(maximum.saturating_add(1))
        .read_to_string(&mut text)
        .ok()?;
    (u64::try_from(text.len()).ok()? <= maximum).then_some(text)
}

fn sichern(daten: &Path, frage: &NeuroFrage) -> Result<(), String> {
    let pfad = sammlung(daten).join(dateiname(frage));
    if frage.gueltig() && !pfad.try_exists().map_err(|e| e.to_string())? {
        // Kommandozeile und Hintergrunddienst können dieselbe Frage vormerken.
        let zwischen = pfad.with_file_name(crate::neues_token());
        schreibe_ganz(&zwischen, &frage.text())?;
        let ergebnis = depot_journal::dateiersatz::ersetze(&zwischen, &pfad);
        if ergebnis.is_err() {
            let _ = fs::remove_file(&zwischen);
        }
        ergebnis.map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn textprobe_marke(daten: &Path, frage: &NeuroFrage) -> PathBuf {
    sammlung(daten).join(format!("{}-{}.textprobe", frage.id.fall(), kurz(frage.wer)))
}

pub(crate) fn textprobe_planen<'a>(
    daten: &Path,
    faelle: impl Iterator<Item = &'a Prueffall>,
) -> Result<String, String> {
    let e = ki_einstellungen::lesen(daten)?;
    if e.neuro_berater().is_empty() || !crate::neuroschlopf::einstellung(daten)? {
        return Ok("KI-Vorprüfung pausiert. NeuroSchlopf und externe Empfänger in der Einrichtung aktivieren.".into());
    }
    let mut cache = Lesecache::default();
    let (stand, _) = cache
        .lesen(&daten.join("journal.txt"), 32 * 1024 * 1024)
        .map_err(|_| "Journal für die KI-Vorprüfung gerade nicht lesbar.")?;
    let reserviert: BTreeSet<_> = stand
        .ki()
        .neurofragen()
        .iter()
        .filter(|v| v.id.runde() == NeuroRunde::Erstfrage)
        .map(|v| (v.id.fall(), kurz(v.wer)))
        .collect();
    let (mut paare, mut neu) = (0_usize, 0_usize);
    for fall in faelle {
        paare = paare.saturating_add(1);
        for &wer in e.neuro_berater() {
            if reserviert.contains(&(fall.kennung.as_str(), kurz(wer))) {
                continue;
            }
            if let Some(frage) = fall.vorpruefung(wer, NeuroRunde::Erstfrage, None) {
                sichern(daten, &frage)?;
                match fs::File::create_new(textprobe_marke(daten, &frage)) {
                    Ok(_) => neu = neu.saturating_add(1),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                    Err(e) => return Err(e.to_string()),
                }
            }
        }
    }
    Ok(format!(
        "KI-Vorprüfung · {paare} Top-3-Paare ohne Konsens · {neu} neue Aufträge für {} · Tageslimit {}.",
        e.neuro_empfaenger.name(),
        e.neuroschlopf
    ))
}

pub(crate) fn planen(
    daten: &Path,
    e: KiEinstellungen,
    faelle: Vec<NeuroFragefall>,
) -> Result<(), String> {
    for f in faelle {
        let thesen_wer = match f.these_berater {
            "Dachs" => BeraterId::Dachs,
            "Maus" => BeraterId::Maus,
            _ => continue,
        };
        let Some(id) = NeuroAuftragId::neu(&f.schluessel, NeuroRunde::Erstfrage) else {
            continue;
        };
        for &wer in e.neuro_berater() {
            sichern(
                daten,
                &NeuroFrage {
                    id: id.clone(),
                    wer,
                    thesen_wer,
                    these_vom: f.these_vom,
                    these_gueltig_bis: f.these_gueltig_bis,
                    isin: f.isin.clone(),
                    these: f.these.clone(),
                    bisher_zeile: Some(f.ursprung_zeile),
                    beleg_zeile: f.beleg_zeile,
                    quelle: f.quelle.clone(),
                    bisher: Some(f.vorher.clone()),
                    jetzt: f.beleg.clone(),
                    bekannt: f.abruf,
                    nachfrage: None,
                },
            )?;
        }
    }
    Ok(())
}

struct Auftrag {
    pfad: PathBuf,
    frage: NeuroFrage,
    antwort: Option<NeuroErgebnis>,
    textprobe: bool,
}

impl Auftrag {
    fn prioritaet(&self) -> (bool, bool) {
        (self.frage.ist_vorpruefung(), !self.textprobe)
    }

    fn versuch(&self) -> PathBuf {
        self.pfad.with_extension("versuch.txt")
    }

    fn abgerechnet(&self) -> PathBuf {
        self.pfad.with_extension("abgerechnet.txt")
    }

    fn rueckfrage(&self) -> Option<NeuroFrage> {
        if self.frage.id.runde() != NeuroRunde::Erstfrage {
            return None;
        }
        let nachfrage = self.antwort.as_ref()?.urteil.as_ref()?.nachfrage.as_ref()?;
        Some(NeuroFrage {
            id: NeuroAuftragId::neu(self.frage.id.fall(), NeuroRunde::Rueckfrage)?,
            nachfrage: Some(nachfrage.clone()),
            ..self.frage.clone()
        })
    }
}

fn auftraege(daten: &Path) -> Vec<Auftrag> {
    let mut auftraege: Vec<_> = fs::read_dir(sammlung(daten))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|datei| {
            let name = datei.file_name();
            let name = name.to_str()?.strip_suffix(".frage.json")?;
            if name.len() > 100 || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                return None;
            }
            let pfad = datei.path();
            let frage = NeuroFrage::lies(&lese(&pfad, NACHRICHT_MAX)?)?;
            if datei.file_name() != dateiname(&frage).as_str() {
                return None;
            }
            let antwort = lese(&ergebnis_pfad(&pfad), 8_000)
                .as_deref()
                .and_then(NeuroErgebnis::lies)
                .filter(|e| e.id == frage.id && e.wer == frage.wer);
            let textprobe = frage.ist_vorpruefung() && textprobe_marke(daten, &frage).is_file();
            Some(Auftrag {
                pfad,
                frage,
                antwort,
                textprobe,
            })
        })
        .collect();
    auftraege.sort_unstable_by(|a, b| {
        b.frage
            .bekannt
            .cmp(&a.frage.bekannt)
            .then_with(|| a.frage.id.cmp(&b.frage.id))
            .then_with(|| kurz(a.frage.wer).cmp(kurz(b.frage.wer)))
    });
    auftraege
}

fn ist_aktiv(a: &Auftrag, e: KiEinstellungen, tag: Datum) -> bool {
    e.neuro_berater().contains(&a.frage.wer)
        && (a.frage.ist_vorpruefung() || a.frage.these_gueltig_bis >= tag)
}

fn vorpruefung_planen(
    daten: &Path,
    stand: &Stand,
    zeilen: &[(Art, String)],
    e: KiEinstellungen,
    tag: Datum,
    auftraege: &[Auftrag],
) -> Result<(), String> {
    let urteile = crate::belegurteile::lese(daten)
        .map_err(|_| "Die gespeicherten Blindurteile sind nicht lesbar.".to_owned())?;
    let gruppe = crate::prueftag::aus_stand(stand, zeilen, daten, tag, None)?;
    let reserviert: BTreeSet<_> = stand
        .ki()
        .neurofragen()
        .iter()
        .filter(|v| v.id.runde() == NeuroRunde::Erstfrage)
        .map(|v| (v.id.fall(), kurz(v.wer)))
        .collect();
    let ausstehend: BTreeSet<_> = auftraege
        .iter()
        .filter(|a| {
            let f = &a.frage;
            f.ist_vorpruefung()
                && f.id.runde() == NeuroRunde::Erstfrage
                && a.antwort.is_none()
                && !urteile.contains_key(f.id.fall())
                && !reserviert.contains(&(f.id.fall(), kurz(f.wer)))
        })
        .map(|a| kurz(a.frage.wer))
        .collect();
    for fall in gruppe
        .faelle
        .iter()
        .filter(|f| !urteile.contains_key(&f.kennung))
    {
        let mut fragen = e
            .neuro_berater()
            .iter()
            .copied()
            .filter(|wer| {
                !ausstehend.contains(kurz(*wer))
                    && !reserviert.contains(&(fall.kennung.as_str(), kurz(*wer)))
            })
            .filter_map(|wer| fall.vorpruefung(wer, NeuroRunde::Erstfrage, None))
            .peekable();
        if fragen.peek().is_some() {
            // Ein nicht erreichbarer Empfänger hält nur seine eigene Warteschlange an.
            // Reservierungen rechnen jede Frage auf das gleiche Tageslimit wie Korrekturen an.
            for frage in fragen {
                sichern(daten, &frage)?;
            }
            break;
        }
    }
    Ok(())
}

fn bearbeite_offene_frage(
    d: &Dienst,
    auftraege: &[Auftrag],
    gilt: impl Fn(&Auftrag) -> bool,
    merke: &impl Fn(Fragenstand),
    letzter_versuch: &mut BTreeMap<PathBuf, Instant>,
) -> bool {
    let kandidaten = auftraege
        .iter()
        .filter(|a| a.antwort.is_none() && gilt(a) && !a.versuch().exists());
    let wartestand = kandidaten
        .clone()
        .min_by_key(|a| a.prioritaet())
        .map_or(Fragenstand::Wartet, |a| {
            Fragenstand::Vorgemerkt(a.frage.wer)
        });
    merke(wartestand);
    if let Some(a) = kandidaten
        .filter(|a| !letzter_versuch.contains_key(&a.pfad))
        .min_by_key(|a| a.prioritaet())
    {
        letzter_versuch.insert(a.pfad.clone(), Instant::now());
        if let Ok(Some(kind)) = d.neuro_starten(&a.pfad, a.frage.wer, false) {
            merke(Fragenstand::Vorgemerkt(a.frage.wer));
            if d.neuro_verfolgen(kind, KURZFRIST).1 {
                let _ = schreibe_ganz(&a.versuch(), "reserviert\n");
                merke(Fragenstand::Wartet);
            } else {
                merke(wartestand);
            }
        }
        return true;
    }
    false
}

pub(crate) fn arbeite(d: &Dienst) {
    let daten = d.wurzel.join("daten");
    let journal = daten.join("journal.txt");
    let mut letzter_versuch = BTreeMap::<PathBuf, Instant>::new();
    let mut letzte_planung = None::<Instant>;
    let mut cache = Lesecache::default();
    loop {
        thread::sleep(Duration::from_secs(3));
        let Some(tag) = crate::jetzt().and_then(depot_kern::Zeitpunkt::frankfurt_tag) else {
            continue;
        };
        let auftraege = auftraege(&daten);
        letzter_versuch.retain(|_, zeit| zeit.elapsed() < Duration::from_secs(30));
        if let Some(a) = auftraege.iter().find(|a| {
            a.antwort.is_some()
                && !a.abgerechnet().exists()
                && !letzter_versuch.contains_key(&a.abgerechnet())
        }) {
            d.sperre().neuro_fragenstand = None;
            letzter_versuch.insert(a.abgerechnet(), Instant::now());
            if let Ok(Some(kind)) = d.neuro_starten(&a.pfad, a.frage.wer, true)
                && d.neuro_verfolgen(kind, KURZFRIST).0 == Some(0)
            {
                let _ = schreibe_ganz(&a.abgerechnet(), "ja\n");
            }
            continue;
        }
        let Ok(e) = ki_einstellungen::lesen(&daten) else {
            d.sperre().neuro_fragenstand = None;
            continue;
        };
        if e.neuro_berater().is_empty()
            || !matches!(crate::neuroschlopf::einstellung(&daten), Ok(true))
        {
            d.sperre().neuro_fragenstand = None;
            continue;
        }
        let merke = |stand| {
            d.sperre().neuro_fragenstand = Some(Fragenvorschau {
                tag,
                einstellungen: e,
                stand,
            });
        };
        let Ok((stand, zeilen)) = cache.lesen(&journal, 32 * 1024 * 1024) else {
            merke(Fragenstand::Journal);
            continue;
        };
        if stand.seit_abschluss() != 0 {
            merke(Fragenstand::Journal);
            continue;
        }
        if stand
            .ki()
            .neurofragen()
            .iter()
            .filter(|v| v.datum == tag)
            .count()
            >= e.neuroschlopf
        {
            merke(Fragenstand::Tageslimit);
            continue;
        }
        let urteile = crate::belegurteile::lese(&daten).ok();
        let offen = |a: &Auftrag| {
            !a.frage.ist_vorpruefung()
                || a.textprobe
                || urteile
                    .as_ref()
                    .is_some_and(|u| !u.contains_key(a.frage.id.fall()))
        };
        if bearbeite_offene_frage(
            d,
            &auftraege,
            |a| ist_aktiv(a, e, tag) && offen(a),
            &merke,
            &mut letzter_versuch,
        ) {
            continue;
        }
        let mut rueckfrage_vorhanden = false;
        for a in auftraege
            .iter()
            .filter(|a| a.abgerechnet().exists() && ist_aktiv(a, e, tag) && offen(a))
        {
            if let Some(rueckfrage) = a.rueckfrage() {
                let pfad = sammlung(&daten).join(dateiname(&rueckfrage));
                rueckfrage_vorhanden |= !pfad.exists();
                let _ = sichern(&daten, &rueckfrage);
            }
        }
        if !rueckfrage_vorhanden
            && letzte_planung.is_none_or(|zeit| zeit.elapsed() >= Duration::from_secs(30))
        {
            letzte_planung = Some(Instant::now());
            let _ = vorpruefung_planen(&daten, stand, zeilen, e, tag, &auftraege);
        }
    }
}

fn anzeige_fall(html: &mut String, a: &Auftrag, ki: &KiStand) {
    let f = &a.frage;
    let urheber = crate::gespraeche::name(kurz(f.thesen_wer));
    let antwortet = crate::gespraeche::name(kurz(f.wer));
    let runde = if f.id.runde() == NeuroRunde::Erstfrage {
        if f.ist_vorpruefung() {
            "Vorprüfung"
        } else {
            "Frage"
        }
    } else {
        "Rückfrage"
    };
    let _ = write!(
        html,
        "<article><h4>{runde} an {antwortet} · {}</h4>\
         <p>These von {urheber} vom {}. {}</p>",
        f.isin,
        datum_text(f.these_vom),
        maskiere(&f.these),
    );
    if let Some((zeile, text)) = f.bisher_zeile.zip(f.bisher.as_deref()) {
        let _ = write!(html, "<p>Bisher · J{zeile} · {}</p>", maskiere(text));
    }
    let fassung = if f.ist_vorpruefung() {
        "Historischer Beleg"
    } else {
        "Jetzt"
    };
    let _ = write!(
        html,
        "<p>{fassung} · J{} · {} · {}</p>",
        f.beleg_zeile,
        maskiere(&f.quelle),
        maskiere(&f.jetzt)
    );
    if let Some(nachfrage) = &f.nachfrage {
        let _ = write!(html, "<p>{}</p>", maskiere(nachfrage));
    }
    let versuch = ki
        .neurofragen()
        .iter()
        .find(|v| v.id == f.id && v.wer == f.wer);
    match versuch.and_then(|v| ki.neuroergebnis(v.buchung)) {
        Some(e) => {
            if let Some(urteil) = &e.urteil {
                let bewertung = bewertungstext(urteil.wert);
                let typ = if f.ist_vorpruefung() {
                    "Einzelbewertung"
                } else {
                    "KI-Bewertung"
                };
                let _ = write!(
                    html,
                    "<p><strong>{typ} · {bewertung}</strong> · {}</p>",
                    maskiere(&urteil.begruendung)
                );
            } else if let Some(grund) = &e.fehler {
                let _ = write!(
                    html,
                    "<p>Keine verwertbare Bewertung · {}</p>",
                    maskiere(grund)
                );
            }
        }
        None if versuch.is_some() => {
            html.push_str("<p>Aufruf reserviert · Urteil noch nicht im Journal belegt.</p>");
        }
        None => html.push_str("<p>Keine Übermittlung belegt.</p>"),
    }
    html.push_str("</article>");
}

pub(crate) fn bewertungstext(wert: NeuroUrteilswert) -> &'static str {
    match wert {
        NeuroUrteilswert::Staerkt => "Stärkt die These",
        NeuroUrteilswert::Schwaecht => "Schwächt die These",
        NeuroUrteilswert::KeinBezug => "Kein Bezug zur These",
        NeuroUrteilswert::Offen => "Lässt die These offen",
    }
}

fn anzeige_auftraege(html: &mut String, auftraege: &[Auftrag], ki: &KiStand) {
    let bewertet = crate::prueftag::vorbewertungen(ki);
    let mut gesehen = BTreeSet::new();
    for a in auftraege
        .iter()
        .filter(|a| !a.frage.ist_vorpruefung() || gesehen.insert(a.frage.id.fall()))
        .take(BEFUNDE_MAX)
    {
        if !a.frage.ist_vorpruefung() {
            anzeige_fall(html, a, ki);
            continue;
        }
        let id = a.frage.id.fall();
        html.push_str("<section><h4>Vorprüfung desselben These-Beleg-Paars</h4>");
        if let Some(wert) = bewertet
            .get(id)
            .and_then(crate::prueftag::Vorbewertung::uebereinstimmung)
        {
            let text = bewertungstext(wert);
            let _ = write!(
                html,
                "<p><strong>KI-Bewertung · Dachs und Maus stimmen überein</strong> · {text}</p>"
            );
        }
        for befund in auftraege.iter().filter(|b| b.frage.id.fall() == id) {
            anzeige_fall(html, befund, ki);
        }
        html.push_str("</section>");
    }
}

pub(crate) fn anzeige(d: &Dienst, daten: &Path) -> String {
    let mut html = String::from(
        "<section><h3>Externe Fragen von NeuroSchlopf</h3>\
        <p>Der lokale Vergleich und die Blindbewertung kosten 0 €. \
        Eine Frage an Maus oder Dachs kann API-Kosten verursachen.</p>",
    );
    if let Ok(e) = ki_einstellungen::lesen(daten) {
        let _ = write!(
            html,
            "<p>Gewählte Empfänger: {} · {} externe Aufrufe pro Tag · \
            <a href=\"/einrichtung#ki-einstellungen\">KI-Einstellungen</a></p>",
            e.neuro_empfaenger.name(),
            e.neuroschlopf
        );
    }
    if let Some(wer) = d.neuro_wartet() {
        let _ = write!(
            html,
            "<p><strong>NeuroSchlopf wartet auf {}</strong></p>",
            crate::gespraeche::name(kurz(wer))
        );
    }
    let auftraege = auftraege(daten);
    if auftraege.is_empty() {
        html.push_str("<p>Für diesen Journalstand ist noch keine externe Frage belegt.</p>");
    } else if Journal::pruefe_im_hintergrund(
        &daten.join("journal.txt"),
        32 * 1024 * 1024,
        &mut || Ok(()),
        |stand, _, _, _, _| {
            anzeige_auftraege(&mut html, &auftraege, stand.ki());
        },
    )
    .is_err()
    {
        html.push_str("<p>Das Journal ist gerade nicht stabil lesbar.</p>");
    }
    html.push_str("</section>");
    html
}

#[cfg(test)]
mod fragenvorschau_tests {
    use super::*;
    use depot_kern::modelle::NeuroEmpfaenger;

    #[test]
    fn vorgemerkter_empfaenger_gilt_nur_fuer_den_tag_und_die_externe_einstellung()
    -> Result<(), &'static str> {
        let tag = Datum::lies("2026-11-20").ok_or("Datum fehlt")?;
        let einstellungen = KiEinstellungen {
            neuro_empfaenger: NeuroEmpfaenger::Dachs,
            ..KiEinstellungen::default()
        };
        let vorschau = Fragenvorschau {
            tag,
            einstellungen,
            stand: Fragenstand::Vorgemerkt(BeraterId::Dachs),
        };
        assert!(matches!(
            vorschau.stand(Some(tag), einstellungen),
            Some(Fragenstand::Vorgemerkt(BeraterId::Dachs))
        ));
        assert!(vorschau.stand(tag.plus_tage(1), einstellungen).is_none());
        assert!(
            vorschau
                .stand(Some(tag), KiEinstellungen::default())
                .is_none()
        );
        assert!(
            vorschau
                .stand(
                    Some(tag),
                    KiEinstellungen {
                        neuroschlopf: 0,
                        ..einstellungen
                    },
                )
                .is_none()
        );
        Ok(())
    }
}
