//! Lesende Datengrundlage aus denselben historischen Paaren wie die Blindprüfung.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use depot_journal::{Art, Journal, JournalFehler, Stand, gespraeche, neurofrage::NeuroUrteilswert};
use depot_kern::{Datum, abdruck::sha256_hex, json::zitiere};
use depot_lage::blindpruefung::{Prueffall, prueffaelle_mit_zeilen};
use depot_lage::relevanzmessung::Urteil;

use crate::{belegurteile, prueftag};

pub(crate) const SCHEMA: &str = "depot-belegpaar-v1";
const MAX_PAARE: usize = 32_768;
const GESPRAECHSLIMIT: u64 = 32 * 1024 * 1024;
const JOURNALLIMIT: u64 = 48 * 1024 * 1024;

pub(crate) struct Tagesstatus {
    pub tag: Datum,
    pub grund: &'static str,
}

pub(crate) struct Zeile {
    pub tag: Datum,
    pub fall: Prueffall,
    pub merkmale: [u16; 4],
    pub fragenplatz: usize,
    pub grundrang: usize,
    pub urteil: Option<Urteil>,
    pub menschlich_bearbeitet: bool,
    pub ki_urteil: Option<NeuroUrteilswert>,
}

pub(crate) struct Datensatz {
    pub von: Datum,
    pub bis: Datum,
    pub journal_sha: String,
    pub zeilen: Vec<Zeile>,
    pub tage: Vec<Tagesstatus>,
    pub urteile_lesbar: bool,
}

/// Die Binärmerkmale, der Anteil gemeinsamer Thesenwörter im Beleg und seine Aktualität (in ‰).
/// Der eingefrorene Wissensstichtag bleibt bei einem späteren Export derselbe.
fn merkmale(fall: &Prueffall) -> [u16; 4] {
    let [wertbezug, termin, _] = fall.grundmerkmale;
    let these = fall
        .these
        .split_once(" · ")
        .map_or(fall.these.as_str(), |(_, t)| t);
    let these = these.rsplit_once(']').map_or(these, |(_, t)| t);
    let wertwoerter = inhaltswoerter(&fall.wert);
    let woerter = inhaltswoerter(these);
    let belegwoerter = inhaltswoerter(&fall.beleg);
    let ueberlappung = belegwoerter
        .intersection(&woerter)
        .filter(|wort| !wertwoerter.contains(*wort))
        .count();
    let anteil = ueberlappung
        .saturating_mul(1000)
        .checked_div(belegwoerter.len())
        .unwrap_or(0);
    // Nachrichten von jetzt bis sieben Tage alt. Termine und fehlende Zeiten tragen 0.
    let frisch = fall
        .veroeffentlicht
        .and_then(|zeit| fall.stichtag.sekunden_seit(zeit))
        .map_or(0, |alter| {
            let woche = 604_800_i64;
            woche
                .saturating_sub(alter.min(woche))
                .saturating_mul(1000)
                .checked_div(woche)
                .unwrap_or(0)
        });
    [
        if wertbezug == 0 { 0 } else { 1000 },
        if termin == 0 { 0 } else { 1000 },
        u16::try_from(anteil).unwrap_or(0),
        u16::try_from(frisch).unwrap_or(0),
    ]
}

fn inhaltswoerter(text: &str) -> BTreeSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|wort| {
            wort.chars().count() >= 4
                && wort.chars().all(char::is_alphabetic)
                && !matches!(
                    *wort,
                    "aber"
                        | "also"
                        | "auch"
                        | "beim"
                        | "dann"
                        | "dass"
                        | "eine"
                        | "einem"
                        | "einen"
                        | "einer"
                        | "oder"
                        | "sich"
                        | "sind"
                        | "über"
                        | "unter"
                        | "vom"
                        | "wenn"
                        | "wird"
                        | "with"
                        | "that"
                        | "from"
                        | "have"
                        | "into"
                        | "this"
                )
        })
        .map(str::to_owned)
        .collect()
}

pub(crate) fn laden(daten: &Path, von: Datum, bis: Datum) -> Result<Datensatz, String> {
    if von > bis {
        return Err("Der Modellzeitraum ist ungültig.".into());
    }
    Journal::pruefe_lesend_mit_eintraegen(
        &daten.join("journal.txt"),
        JOURNALLIMIT,
        |stand, zeilen, journal| vorrat(daten, stand, zeilen, journal, von, bis),
    )
    .map_err(|fehler| {
        match fehler {
            JournalFehler::Belegt => "Ein Auftrag verwendet gerade das Journal.",
            _ => "Der Journalstand konnte nicht bestätigt werden.",
        }
        .to_owned()
    })
}

pub(crate) fn vorrat(
    daten: &Path,
    stand: &Stand,
    journalzeilen: &[(Art, String)],
    journal: &str,
    von: Datum,
    bis: Datum,
) -> Datensatz {
    let urteile = belegurteile::lese(daten);
    let ki_urteile = prueftag::vorbewertungen(stand.ki());
    let mut satz = Datensatz {
        von,
        bis,
        journal_sha: sha256_hex(journal.as_bytes()),
        zeilen: Vec::new(),
        tage: Vec::new(),
        urteile_lesbar: urteile.is_ok(),
    };
    let mut budget = GESPRAECHSLIMIT;
    let tage: BTreeSet<_> = stand
        .tage_mit_wissensstichtag()
        .chain(stand.beobachtungen().iter().map(|(tag, _)| *tag))
        .filter(|tag| (von..=bis).contains(tag))
        .collect();
    for tag in tage {
        let auswahl = match gespraeche::lesen_begrenzt(daten, tag, &mut budget) {
            Ok(Some((eintraege, 0))) => {
                prueffaelle_mit_zeilen(stand, journalzeilen, &eintraege, tag)
            }
            Ok(Some(_)) => Err("Gespeicherte KI-Fragen sind unvollständig."),
            Ok(None) => Err("Gespeicherte KI-Fragen fehlen."),
            Err(_) => {
                Err("Gespeicherte KI-Fragen konnten innerhalb der Lesegrenze nicht gelesen werden.")
            }
        };
        let faelle = match auswahl {
            Ok(faelle) => faelle,
            Err(grund) => {
                satz.tage.push(Tagesstatus { tag, grund });
                continue;
            }
        };
        if satz.zeilen.len().saturating_add(faelle.len()) > MAX_PAARE {
            satz.tage.push(Tagesstatus {
                tag,
                grund: "Paargrenze erreicht, kürzeren Zeitraum wählen.",
            });
            break;
        }
        let mut nach_these = BTreeMap::<_, Vec<Prueffall>>::new();
        for fall in faelle {
            let z = fall.zuordnung();
            nach_these
                .entry((z.berater, z.thesentag, z.isin.clone(), z.thesen_sha.clone()))
                .or_default()
                .push(fall);
        }
        for mut gruppe in nach_these.into_values() {
            // Grundrang v2 mit einer einzelnen These, auf genau diesen sichtbaren Kandidaten.
            gruppe.sort_by_key(|fall| {
                let [_, _, platz] = fall.grundmerkmale;
                (Reverse(fall.grundpunkte()), platz)
            });
            for (index, fall) in gruppe.into_iter().enumerate() {
                let [_, _, fragenplatz] = fall.grundmerkmale;
                let urteil = urteile
                    .as_ref()
                    .ok()
                    .and_then(|bestand| bestand.get(&fall.kennung));
                satz.zeilen.push(Zeile {
                    tag,
                    merkmale: merkmale(&fall),
                    fragenplatz,
                    grundrang: index.saturating_add(1),
                    urteil: urteil.copied().flatten(),
                    menschlich_bearbeitet: urteil.is_some(),
                    ki_urteil: ki_urteile
                        .get(&fall.kennung)
                        .and_then(prueftag::Vorbewertung::uebereinstimmung),
                    fall,
                });
            }
        }
    }
    satz
}

fn normalisiert(tausendstel: u16) -> String {
    format!(
        "{}.{:03}",
        tausendstel.checked_div(1000).unwrap_or(0),
        tausendstel.checked_rem(1000).unwrap_or(0)
    )
}

impl Datensatz {
    /// Exportiert Fallzuordnung und Merkmale. Originaltexte und Einzelurteile bleiben lokal.
    pub(crate) fn jsonl(&self, limit: usize) -> String {
        let bearbeitet = self
            .zeilen
            .iter()
            .filter(|z| z.menschlich_bearbeitet)
            .count();
        let urteilsanzahl = self.zeilen.iter().filter(|z| z.urteil.is_some()).count();
        let ki_anzahl = self.zeilen.iter().filter(|z| z.ki_urteil.is_some()).count();
        let kopf = format!(
            "{{\"format\":\"{SCHEMA}\",\"merkmale\":[\"wertbezug\",\"termin\",\"thesenwortanteil\",\"aktualitaet\"],\"journal_sha\":\"{}\",\"von\":\"{}\",\"bis\":\"{}\",\"urteile_lesbar\":{},\"menschlich_bearbeitet\":{bearbeitet},\"menschlich_bewertet\":{urteilsanzahl},\"ki_uebereinstimmungen\":{ki_anzahl}}}\n",
            self.journal_sha, self.von, self.bis, self.urteile_lesbar
        );
        let mut aus = kopf.clone();
        for t in &self.tage {
            let _ = writeln!(
                aus,
                "{{\"tag\":\"{}\",\"status\":{}}}",
                t.tag,
                zitiere(t.grund)
            );
        }
        if self.zeilen.is_empty() && self.tage.is_empty() {
            aus.push_str("{\"status\":\"Keine historischen Prüftage im Zeitraum belegt.\"}\n");
        }
        for z in &self.zeilen {
            let fall = &z.fall;
            let identitaet = fall.zuordnung();
            let [wert, termin, wortanteil, aktualitaet] = z.merkmale.map(normalisiert);
            let _ = writeln!(
                aus,
                "{{\"tag\":\"{}\",\"stichtag\":\"{}\",\"fall_id\":\"{}\",\"berater\":\"{:?}\",\"isin\":\"{}\",\"thesentag\":\"{}\",\"thesen_sha\":\"{}\",\"ref\":\"{}\",\"x\":[{wert},{termin},{wortanteil},{aktualitaet}],\"fragenplatz\":{},\"grundrang\":{}}}",
                z.tag,
                fall.stichtag,
                fall.kennung,
                identitaet.berater,
                identitaet.isin,
                identitaet.thesentag,
                identitaet.thesen_sha,
                identitaet.referenz,
                z.fragenplatz,
                z.grundrang
            );
            if aus.len() > limit {
                break;
            }
        }
        if aus.len() > limit {
            return kopf
                + "{\"status\":\"Größenlimit erreicht, kürzeren Exportzeitraum wählen\"}\n";
        }
        aus
    }
}
