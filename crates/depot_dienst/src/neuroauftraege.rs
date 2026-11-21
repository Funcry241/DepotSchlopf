//! Dauerhafte Übergabe lokaler Fälle und asynchrone Abwicklung externer Fragen.
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use depot_journal::neurofrage::{
    NeuroAuftragId, NeuroErgebnis, NeuroFrage, NeuroRunde, NeuroUrteilswert, ergebnis_pfad,
};
use depot_journal::{Journal, KiStand, ki_einstellungen};
use depot_kern::modelle::KiEinstellungen;
use depot_kern::{BeraterId, Datum};
use depot_lage::{NeuroFragefall, datum_text, html::maskiere};

use crate::werkbank::{Dienst, schreibe_ganz};

const NACHRICHT_MAX: u64 = 40_000;
const BEFUNDE_MAX: usize = 20;
const KURZFRIST: Duration = Duration::from_secs(10);

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
        schreibe_ganz(&pfad, &frage.text())?;
    }
    Ok(())
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
}

impl Auftrag {
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
            Some(Auftrag {
                pfad,
                frage,
                antwort,
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
    e.neuro_berater().contains(&a.frage.wer) && a.frage.these_gueltig_bis >= tag
}

pub(crate) fn arbeite(d: &Dienst) {
    let daten = d.wurzel.join("daten");
    let mut letzter_versuch = BTreeMap::<PathBuf, Instant>::new();
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
            if let Ok(Some(kind)) = d.neuro_starten(&a.pfad, a.frage.wer, true) {
                letzter_versuch.insert(a.abgerechnet(), Instant::now());
                if d.neuro_verfolgen(kind, KURZFRIST).0 == Some(0) {
                    let _ = schreibe_ganz(&a.abgerechnet(), "ja\n");
                }
            }
            continue;
        }
        let Ok(e) = ki_einstellungen::lesen(&daten) else {
            continue;
        };
        if e.neuro_berater().is_empty()
            || !matches!(crate::neuroschlopf::einstellung(&daten), Ok(true))
        {
            continue;
        }
        if let Some(a) = auftraege.iter().find(|a| {
            a.antwort.is_none()
                && ist_aktiv(a, e, tag)
                && !a.versuch().exists()
                && !letzter_versuch.contains_key(&a.pfad)
        }) {
            if let Ok(Some(kind)) = d.neuro_starten(&a.pfad, a.frage.wer, false) {
                letzter_versuch.insert(a.pfad.clone(), Instant::now());
                if d.neuro_verfolgen(kind, KURZFRIST).1 {
                    let _ = schreibe_ganz(&a.versuch(), "reserviert\n");
                }
            }
            continue;
        }
        for a in auftraege
            .iter()
            .filter(|a| a.abgerechnet().exists() && ist_aktiv(a, e, tag))
        {
            if let Some(rueckfrage) = a.rueckfrage() {
                let _ = sichern(&daten, &rueckfrage);
            }
        }
    }
}

fn anzeige_fall(html: &mut String, a: &Auftrag, ki: &KiStand) {
    let f = &a.frage;
    let urheber = crate::gespraeche::name(kurz(f.thesen_wer));
    let antwortet = crate::gespraeche::name(kurz(f.wer));
    let runde = if f.id.runde() == NeuroRunde::Erstfrage {
        "Frage"
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
    let _ = write!(
        html,
        "<p>Jetzt · J{} · {} · {}</p>",
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
                let bewertung = match urteil.wert {
                    NeuroUrteilswert::Staerkt => "Stärkt die These",
                    NeuroUrteilswert::Schwaecht => "Schwächt die These",
                    NeuroUrteilswert::Offen => "Lässt die These offen",
                };
                let _ = write!(
                    html,
                    "<p><strong>KI-Bewertung · {bewertung}</strong> · {}</p>",
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
            for a in auftraege.iter().take(BEFUNDE_MAX) {
                anzeige_fall(&mut html, a, stand.ki());
            }
        },
    )
    .is_err()
    {
        html.push_str("<p>Das Journal ist gerade nicht stabil lesbar.</p>");
    }
    html.push_str("</section>");
    html
}
