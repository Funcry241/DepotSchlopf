//! Verbindet den bestätigten Journalstand eines Prüftags mit den gespeicherten KI-Fragen.
use std::path::Path;

use depot_journal::{Journal, JournalFehler, Stand, gespraeche};
use depot_kern::Datum;
use depot_lage::blindpruefung::{Prueffall, prueffaelle};

use crate::werkbank::Dienst;

#[derive(Default)]
pub(super) struct Prueftag {
    pub tage: Vec<Datum>,
    pub tag: Option<Datum>,
    pub faelle: Vec<Prueffall>,
    pub hinweis: Option<String>,
}

fn aus_stand(
    stand: &Stand,
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
        Some((eintraege, _)) => match prueffaelle(stand, &eintraege, tag) {
            Ok(faelle) => aus.faelle = faelle,
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
    Journal::pruefe_lesend_begrenzt(&pfad, 32 * 1024 * 1024, |stand| {
        aus_stand(stand, &daten, bis, wahl)
    })
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
