//! Unabhängige Urteile über einzelne These-Beleg-Paare im lokalen Datenordner.
//! Für eine fehlende ID liegt noch keine Antwort vor. Ein gespeichertes
//! `unbewertet` trägt ausdrücklich kein Relevanzurteil für die spätere Auswertung.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs::{self, File},
    io::ErrorKind,
    path::Path,
};

use depot_kern::{BeraterId, Datum, Isin, Zeitpunkt, abdruck::sha256_hex};
use depot_lage::blindpruefung::{Prueffall, Zuordnung};
use depot_lage::relevanzmessung::{Richtung, Urteil};

use crate::werkbank::{lies_falls_da, schreibe_ganz};

const DATEI: &str = "neuro2026-belegurteile.tsv";
const KOPF: &str = "# neuro2026-belegurteile-v1\nfall_id\tstufe\trichtung\tstichtag\tberater\tisin\tthesentag\tthesen_sha\tref\n";
type Urteile = BTreeMap<String, Option<Urteil>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Speicherfehler {
    Ungueltig,
    Konflikt,
    Speicher,
}

pub(super) fn aus_form(stufe: &str, richtung: &str) -> Result<Option<Urteil>, Speicherfehler> {
    match (stufe, richtung) {
        ("unbewertet", "") => Ok(None),
        ("0", "") => Ok(Some(Urteil::KeinBezug)),
        ("1", "") => Ok(Some(Urteil::Bezug)),
        ("2", "stuetzt") => Ok(Some(Urteil::Entscheidend(Richtung::Stuetzt))),
        ("2", "widerlegt") => Ok(Some(Urteil::Entscheidend(Richtung::Widerlegt))),
        ("2", "offen") => Ok(Some(Urteil::Entscheidend(Richtung::Offen))),
        _ => Err(Speicherfehler::Ungueltig),
    }
}

fn felder(urteil: Option<Urteil>) -> &'static str {
    match urteil {
        None => "unbewertet\t",
        Some(Urteil::KeinBezug) => "0\t",
        Some(Urteil::Bezug) => "1\t",
        Some(Urteil::Entscheidend(Richtung::Stuetzt)) => "2\tstuetzt",
        Some(Urteil::Entscheidend(Richtung::Widerlegt)) => "2\twiderlegt",
        Some(Urteil::Entscheidend(Richtung::Offen)) => "2\toffen",
    }
}

pub(super) fn kennung_gueltig(fall: &str) -> bool {
    fall.len() == 64 && crate::rueckfall::ist_commit(fall)
}

fn regulaer_oder_fehlend(pfad: &Path) -> Result<(), Speicherfehler> {
    match fs::symlink_metadata(pfad) {
        Ok(info) if info.is_file() => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        _ => Err(Speicherfehler::Speicher),
    }
}

fn abschliessen(mut inhalt: String) -> String {
    let sha = sha256_hex(inhalt.as_bytes());
    inhalt.push_str("# sha256\t");
    inhalt.push_str(&sha);
    inhalt.push('\n');
    inhalt
}

#[derive(Debug, PartialEq, Eq)]
struct Eintrag {
    stichtag: Zeitpunkt,
    zuordnung: Zuordnung,
    urteil: Option<Urteil>,
}

impl Eintrag {
    fn schreibe(&self, id: &str, aus: &mut String) {
        let z = &self.zuordnung;
        let _ = writeln!(
            aus,
            "{id}\t{}\t{}\t{:?}\t{}\t{}\t{}\t{}",
            felder(self.urteil),
            self.stichtag,
            z.berater,
            z.isin,
            z.thesentag,
            z.thesen_sha,
            z.referenz
        );
    }

    fn lies(text: &str) -> Option<(String, Self)> {
        let [
            id,
            stufe,
            richtung,
            zeit,
            wer,
            isin,
            thesentag,
            thesen_sha,
            referenz,
        ]: [&str; 9] = text.split('\t').collect::<Vec<_>>().try_into().ok()?;
        let stichtag = Zeitpunkt::lies(zeit)?;
        let thesentag = Datum::lies(thesentag)?;
        let (tag, rest) = referenz.split_once('#')?;
        let tag = Datum::lies(tag)?;
        let (platz, beleg_sha) = rest.split_once(':')?;
        let nummer = platz.parse::<usize>().ok()?;
        let wert = Isin::neu(isin).ok()?;
        if !kennung_gueltig(thesen_sha)
            || !kennung_gueltig(beleg_sha)
            || nummer == 0
            || nummer.to_string() != platz
            || thesentag >= tag
            || stichtag.frankfurt_tag() != Some(tag)
            || wert.als_str() != isin
        {
            return None;
        }
        let zuordnung = Zuordnung {
            berater: match wer {
                "Dachs" => BeraterId::Dachs,
                "Maus" => BeraterId::Maus,
                _ => return None,
            },
            isin: wert,
            thesentag,
            thesen_sha: thesen_sha.into(),
            referenz: referenz.into(),
        };
        let urteil = aus_form(stufe, richtung).ok()?;
        (id == zuordnung.kennung(stichtag)).then(|| {
            (
                id.to_owned(),
                Self {
                    stichtag,
                    zuordnung,
                    urteil,
                },
            )
        })
    }
}

type Bestand = BTreeMap<String, Eintrag>;

fn dekodiere(text: &str) -> Result<Bestand, Speicherfehler> {
    let (inhalt, abdruck) = text
        .strip_suffix('\n')
        .and_then(|text| text.rsplit_once("# sha256\t"))
        .ok_or(Speicherfehler::Speicher)?;
    if !inhalt.ends_with('\n') || abdruck != sha256_hex(inhalt.as_bytes()) {
        return Err(Speicherfehler::Speicher);
    }
    let zeilen = inhalt.strip_prefix(KOPF).ok_or(Speicherfehler::Speicher)?;
    let mut bestand = Bestand::new();
    for zeile in zeilen.lines() {
        let (id, eintrag) = Eintrag::lies(zeile).ok_or(Speicherfehler::Speicher)?;
        if bestand.insert(id, eintrag).is_some() {
            return Err(Speicherfehler::Speicher);
        }
    }
    Ok(bestand)
}

fn lese_bestand(daten: &Path) -> Result<Bestand, Speicherfehler> {
    let pfad = daten.join(DATEI);
    regulaer_oder_fehlend(&pfad)?;
    match lies_falls_da(&pfad).map_err(|_| Speicherfehler::Speicher)? {
        Some(text) => dekodiere(&text),
        None => Ok(Bestand::new()),
    }
}

pub(super) fn lese(daten: &Path) -> Result<Urteile, Speicherfehler> {
    Ok(lese_bestand(daten)?
        .into_iter()
        .map(|(id, eintrag)| (id, eintrag.urteil))
        .collect())
}

pub(super) fn speichere(
    daten: &Path,
    fall: &Prueffall,
    urteil: Option<Urteil>,
) -> Result<(), Speicherfehler> {
    let eintrag = Eintrag {
        stichtag: fall.stichtag,
        zuordnung: fall.zuordnung().clone(),
        urteil,
    };
    if fall.kennung != eintrag.zuordnung.kennung(eintrag.stichtag) {
        return Err(Speicherfehler::Ungueltig);
    }
    schreibe(daten, &fall.kennung, eintrag)
}

/// Die separate, dauerhaft benannte Sperrdatei bleibt auch beim Ersetzen des
/// Urteilsstands dieselbe. Gleichzeitige Prozesse lesen unter der Sperre neu.
fn schreibe(daten: &Path, id: &str, eintrag: Eintrag) -> Result<(), Speicherfehler> {
    fs::create_dir_all(daten).map_err(|_| Speicherfehler::Speicher)?;
    let pfad = daten.join(DATEI);
    let sperrpfad = pfad.with_extension("lock");
    regulaer_oder_fehlend(&sperrpfad)?;
    let sperre = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(sperrpfad)
        .map_err(|_| Speicherfehler::Speicher)?;
    sperre.lock().map_err(|_| Speicherfehler::Speicher)?;
    let mut bestand = lese_bestand(daten)?;
    if let Some(vorhanden) = bestand.get(id) {
        return if *vorhanden == eintrag {
            Ok(())
        } else {
            Err(Speicherfehler::Konflikt)
        };
    }
    bestand.insert(id.to_owned(), eintrag);
    let mut text = String::from(KOPF);
    for (id, eintrag) in bestand {
        eintrag.schreibe(&id, &mut text);
    }
    regulaer_oder_fehlend(&pfad.with_extension("neu"))?;
    schreibe_ganz(&pfad, &abschliessen(text)).map_err(|_| Speicherfehler::Speicher)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Ordner(PathBuf);

    impl Ordner {
        fn neu(name: &str) -> Result<Self, Speicherfehler> {
            let pfad =
                std::env::temp_dir().join(format!("belegurteile-{}-{name}", std::process::id()));
            fs::create_dir(&pfad).map_err(|_| Speicherfehler::Speicher)?;
            Ok(Self(pfad))
        }
    }

    impl Drop for Ordner {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn eintrag(name: &str, urteil: Option<Urteil>) -> Result<(String, Eintrag), Speicherfehler> {
        let stichtag = Zeitpunkt::lies("2026-11-20T10:00:00Z").ok_or(Speicherfehler::Ungueltig)?;
        let zuordnung = Zuordnung {
            berater: BeraterId::Maus,
            thesentag: Datum::lies("2026-11-19").ok_or(Speicherfehler::Ungueltig)?,
            thesen_sha: sha256_hex(b"These"),
            isin: Isin::neu("DE0007164600").map_err(|_| Speicherfehler::Ungueltig)?,
            referenz: format!("2026-11-20#1:{}", sha256_hex(name.as_bytes())),
        };
        Ok((
            zuordnung.kennung(stichtag),
            Eintrag {
                stichtag,
                zuordnung,
                urteil,
            },
        ))
    }

    #[test]
    fn nur_ausdrueckliche_und_vollstaendige_formwerte() -> Result<(), Speicherfehler> {
        let pfad = Ordner::neu("formen")?;
        let mut erwartet = Urteile::new();
        for (stufe, richtung) in [
            ("unbewertet", ""),
            ("0", ""),
            ("1", ""),
            ("2", "stuetzt"),
            ("2", "widerlegt"),
            ("2", "offen"),
        ] {
            let urteil = aus_form(stufe, richtung)?;
            let (id, e) = eintrag(&format!("{stufe}/{richtung}"), urteil)?;
            let referenz = e.zuordnung.referenz.clone();
            schreibe(&pfad.0, &id, e)?;
            let bestand = lese_bestand(&pfad.0)?;
            assert_eq!(
                bestand.get(&id).map(|e| e.zuordnung.referenz.as_str()),
                Some(referenz.as_str())
            );
            erwartet.insert(id, urteil);
        }
        assert_eq!(lese(&pfad.0)?, erwartet);
        for (stufe, richtung) in [
            ("", ""),
            ("02", "offen"),
            ("2", ""),
            ("0", "stuetzt"),
            ("unbewertet", "offen"),
            ("2", "falsch"),
        ] {
            assert_eq!(aus_form(stufe, richtung), Err(Speicherfehler::Ungueltig));
        }
        for id in [
            "A".repeat(64),
            "a".repeat(63),
            "g".repeat(64),
            "../x".into(),
        ] {
            assert!(!kennung_gueltig(&id));
        }
        Ok(())
    }

    #[test]
    fn gleichzeitige_speicherungen_sind_verlustfrei_und_wiederholbar() -> Result<(), Speicherfehler>
    {
        let pfad = Ordner::neu("parallel")?;
        let start = std::sync::Barrier::new(2);
        let auftrag = |name, urteil| {
            let (id, e) = eintrag(name, urteil)?;
            start.wait();
            schreibe(&pfad.0, &id, e)
        };
        let ergebnisse = std::thread::scope(|s| {
            let a = s.spawn(|| auftrag("gleichzeitig", None));
            let b = s.spawn(|| auftrag("gleichzeitig", Some(Urteil::KeinBezug)));
            [a.join(), b.join()]
        });
        let [a, b] = ergebnisse.map(|e| e.unwrap_or(Err(Speicherfehler::Speicher)));
        assert!(matches!(
            (a, b),
            (Ok(()), Err(Speicherfehler::Konflikt)) | (Err(Speicherfehler::Konflikt), Ok(()))
        ));
        let (id, _) = eintrag("gleichzeitig", None)?;
        let bestand = lese(&pfad.0)?;
        let vorhanden = bestand.get(&id).ok_or(Speicherfehler::Speicher)?;
        let (_, e) = eintrag("gleichzeitig", *vorhanden)?;
        schreibe(&pfad.0, &id, e)?;
        std::thread::scope(|s| {
            let a = s.spawn(|| auftrag("a", Some(Urteil::Bezug)));
            let b = s.spawn(|| auftrag("b", None));
            a.join().map_err(|_| Speicherfehler::Speicher)??;
            b.join().map_err(|_| Speicherfehler::Speicher)?
        })?;
        assert_eq!(lese(&pfad.0)?.len(), 3);
        Ok(())
    }

    #[test]
    fn defekter_stand_bleibt_unveraendert() -> Result<(), Speicherfehler> {
        let pfad = Ordner::neu("defekt")?;
        let (id, original) = eintrag("fall", Some(Urteil::Entscheidend(Richtung::Widerlegt)))?;
        let datei = pfad.0.join(DATEI);
        let mut zeile = String::new();
        original.schreibe(&id, &mut zeile);
        for defekt in [
            String::new(),
            format!("{KOPF}{zeile}"),
            abschliessen(format!("{KOPF}{zeile}")).replace("widerlegt", "stuetzt"),
            abschliessen(format!("{KOPF}{zeile}{zeile}")),
            abschliessen(format!("{KOPF}{}", zeile.replace("widerlegt", ""))),
            abschliessen(format!("{KOPF}{}", zeile.replace("Maus", "Dachs"))),
            abschliessen(format!("{KOPF}{}", zeile.replace("#1:", "#0:"))),
            abschliessen(format!("{KOPF}{}", zeile.replace(&id, "falsch"))),
            abschliessen(String::from("neuro2026-belegurteile-v2\n")),
        ] {
            fs::write(&datei, &defekt).map_err(|_| Speicherfehler::Speicher)?;
            assert_eq!(lese(&pfad.0), Err(Speicherfehler::Speicher));
            let (_, e) = eintrag("fall", None)?;
            assert_eq!(schreibe(&pfad.0, &id, e), Err(Speicherfehler::Speicher));
            assert_eq!(fs::read_to_string(&datei).ok().as_ref(), Some(&defekt));
        }
        Ok(())
    }
}
