//! Verbindet den bestätigten Journalstand eines Prüftags mit den gespeicherten KI-Fragen.
use std::{collections::BTreeMap, path::Path};

use depot_journal::neurofrage::NeuroUrteilswert;
use depot_journal::{Art, Journal, JournalFehler, KiStand, Stand, gespraeche};
use depot_kern::{BeraterId, Datum};
use depot_lage::blindpruefung::{Prueffall, prueffaelle_mit_zeilen};

use crate::werkbank::Dienst;

#[derive(Default)]
pub(super) struct Prueftag {
    pub tage: Vec<Datum>,
    pub tag: Option<Datum>,
    pub faelle: Vec<Prueffall>,
    pub hinweis: Option<String>,
    pub vorbewertungen: BTreeMap<String, Vorbewertung>,
}

#[derive(Default)]
pub(super) struct Vorbewertung {
    dachs: Option<NeuroUrteilswert>,
    maus: Option<NeuroUrteilswert>,
}

impl Vorbewertung {
    pub fn uebereinstimmung(&self) -> Option<NeuroUrteilswert> {
        self.dachs
            .filter(|wert| Some(*wert) == self.maus && *wert != NeuroUrteilswert::Offen)
    }

    pub fn widerspruch(&self) -> bool {
        self.dachs.zip(self.maus).is_some_and(|(dachs, maus)| {
            dachs != maus && dachs != NeuroUrteilswert::Offen && maus != NeuroUrteilswert::Offen
        })
    }
}

pub(super) fn vorbewertungen(ki: &KiStand) -> BTreeMap<String, Vorbewertung> {
    let mut aus = BTreeMap::<String, Vorbewertung>::new();
    // Rückfragen zählen erst nach ihrer protokollierten Antwort als abschließendes Urteil.
    for versuch in ki.neurofragen() {
        let wert = ki
            .neuroergebnis(versuch.buchung)
            .and_then(|e| e.urteil.as_ref())
            .filter(|u| u.nachfrage.is_none())
            .map(|u| u.wert);
        let bewertung = aus.entry(versuch.id.fall().to_owned()).or_default();
        match versuch.wer {
            BeraterId::Dachs => bewertung.dachs = wert,
            BeraterId::Maus => bewertung.maus = wert,
        }
    }
    aus
}

impl Prueftag {
    pub fn offene_faelle(&self) -> impl Iterator<Item = &Prueffall> + Clone {
        let mut stichprobe = self.faelle.len().div_ceil(10).clamp(1, 5);
        self.faelle.iter().filter(move |fall| {
            let einig = self
                .vorbewertungen
                .get(&fall.kennung)
                .and_then(Vorbewertung::uebereinstimmung)
                .is_some();
            if !einig {
                return true;
            }
            // Auswahl aus übereinstimmenden Fällen nach vorgemischter Kennung, ohne Richtung.
            let gewaehlt = stichprobe > 0;
            stichprobe = stichprobe.saturating_sub(1);
            gewaehlt
        })
    }
}

pub(super) fn aus_stand(
    stand: &Stand,
    zeilen: &[(Art, String)],
    daten: &Path,
    bis: Datum,
    wahl: Option<Datum>,
) -> Result<Prueftag, String> {
    let mut tage: Vec<_> = stand
        .tage_mit_wissensstichtag()
        .filter(|t| *t <= bis)
        .collect();
    tage.sort_unstable_by(|a, b| b.cmp(a));
    let tag = wahl.or_else(|| tage.first().copied());
    let mut aus = Prueftag {
        tage,
        tag,
        ..Prueftag::default()
    };
    let Some(tag) = tag else { return Ok(aus) };
    if !aus.tage.contains(&tag) {
        aus.hinweis = Some("Für diesen Tag ist kein Wissensstichtag im Journal belegt.".into());
        return Ok(aus);
    }
    let mut budget = 8 * 1024 * 1024;
    let aufrufe = gespraeche::lesen_begrenzt(daten, tag, &mut budget)
        .map_err(|_| "Die gespeicherten KI-Fragen konnten nicht vollständig gelesen werden.")?;
    match aufrufe {
        None => aus.hinweis = Some("Für diesen Tag fehlen die gespeicherten KI-Fragen.".into()),
        Some((_, n)) if n > 0 => {
            aus.hinweis =
                Some("Die Gesprächsdatei dieses Tages enthält unlesbare Einträge.".into());
        }
        Some((eintraege, _)) => match prueffaelle_mit_zeilen(stand, zeilen, &eintraege, tag) {
            Ok(faelle) => {
                aus.faelle = faelle;
                aus.vorbewertungen = vorbewertungen(stand.ki());
            }
            Err(grund) => aus.hinweis = Some(grund.into()),
        },
    }
    Ok(aus)
}

pub(super) fn laden(
    d: &Dienst,
    bis: Option<Datum>,
    wahl: Option<Datum>,
) -> Result<Prueftag, String> {
    let bis = bis.ok_or("Der heutige Frankfurter Tag konnte nicht bestimmt werden.")?;
    let daten = d.wurzel.join("daten");
    let pfad = daten.join("journal.txt");
    if !Journal::eingerichtet(&pfad).map_err(|e| journalfehler(&e))? {
        return Ok(Prueftag::default());
    }
    Journal::pruefe_im_hintergrund(
        &pfad,
        32 * 1024 * 1024,
        &mut || Ok(()),
        |stand, zeilen, _, _, _| aus_stand(stand, zeilen, &daten, bis, wahl),
    )
    .map_err(|e| journalfehler(&e))?
}

fn journalfehler(fehler: &JournalFehler) -> String {
    match fehler {
        JournalFehler::Belegt => {
            "Ein Auftrag verwendet gerade das Journal. Öffne die Belegprüfung danach erneut.".into()
        }
        JournalFehler::Abgelehnt(_) | JournalFehler::Halt(_) => {
            "Der Journalstand konnte nicht bestätigt werden.".into()
        }
    }
}
