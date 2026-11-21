//! Fragefälle mit einem nachweisbaren Zusammenhang zur ursprünglichen Quelle einer These.

use std::collections::{BTreeMap, BTreeSet};

use depot_journal::{Art, NachrichtBeleg, Stand, TerminBeleg};
use depot_kern::{Datum, Herkunft, Isin, Zeitpunkt};
use sha2::{Digest, Sha256};

use super::OffeneThese;
use crate::papiertag::{nachricht_ohne_markierung, termin_text};

/// Ein neuer Quellenstand zu einem belegten Ausgangspunkt einer offenen These.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fall {
    /// Stabile Identität zur Wiederanlauferkennung, unabhängig vom Antwortempfänger.
    pub schluessel: String,
    pub these_berater: &'static str,
    pub these_vom: Datum,
    pub these_gueltig_bis: Datum,
    pub isin: Isin,
    pub these: String,
    pub ursprung_zeile: u64,
    pub beleg_zeile: u64,
    pub quelle: String,
    pub art: Art,
    pub vorher: String,
    pub beleg: String,
    pub abruf: Zeitpunkt,
}

#[derive(Clone, Copy)]
enum Beleg<'a> {
    Nachricht(&'a NachrichtBeleg),
    Termin(&'a TerminBeleg),
}

impl Beleg<'_> {
    fn art(self) -> Art {
        match self {
            Self::Nachricht(_) => Art::Nachricht,
            Self::Termin(_) => Art::Termin,
        }
    }

    fn abruf(self) -> Zeitpunkt {
        match self {
            Self::Nachricht(n) => n.abruf(),
            Self::Termin(t) => t.abruf(),
        }
    }

    fn ereignis(self) -> Option<String> {
        match self {
            Self::Nachricht(n) => nachricht_ohne_markierung(n.zeit(), n.quelle(), n.text()),
            Self::Termin(t) if t.herkunft() == Herkunft::Quelle => termin_text(t.termin()),
            Self::Termin(_) => None,
        }
    }

    fn aussage(self) -> String {
        match self {
            Self::Nachricht(n) => nachrichtenteile(n.text())
                .map_or(n.text(), |(titel, _)| titel)
                .to_owned(),
            Self::Termin(t) => format!(
                "{} · {} · {}",
                t.termin().zeit(),
                t.termin().titel(),
                t.termin().verlaesslichkeit().name()
            ),
        }
    }
}

impl<'a> Beleg<'a> {
    fn quellen_id(self) -> Option<(Art, &'a str, &'a str)> {
        match self {
            Self::Nachricht(n) => Some((self.art(), n.quelle(), nachrichtenteile(n.text())?.1)),
            Self::Termin(t) if t.herkunft() == Herkunft::Quelle => {
                Some((self.art(), t.quelle(), t.termin().kennung()))
            }
            Self::Termin(_) => None,
        }
    }
}

fn ohne_wertmarkierung(ereignis: &str) -> &str {
    if !ereignis.starts_with("Nachricht ") {
        return ereignis;
    }
    ereignis
        .strip_suffix(" [Makro]")
        .or_else(|| {
            ereignis
                .rsplit_once(" [betrifft: ")
                .filter(|(_, werte)| werte.ends_with(']'))
                .map(|(basis, _)| basis)
        })
        .unwrap_or(ereignis)
}

fn nachrichtenteile(text: &str) -> Option<(&str, &str)> {
    text.rsplit_once(" | ")
        .filter(|(_, url)| url.starts_with("https://") && !url.chars().any(char::is_whitespace))
}

#[derive(Clone, Copy)]
struct Journalbeleg<'a> {
    zeile: u64,
    rohzeile: &'a str,
    beleg: Beleg<'a>,
}

enum Ursprung<'a> {
    Fehlend,
    Eindeutig(Journalbeleg<'a>),
    Mehrdeutig,
}

impl<'a> Ursprung<'a> {
    fn fuege_hinzu(&mut self, b: Journalbeleg<'a>) {
        *self = match *self {
            Self::Fehlend => Self::Eindeutig(b),
            Self::Eindeutig(alt)
                if alt.beleg.quellen_id().is_some()
                    && alt.beleg.quellen_id() == b.beleg.quellen_id() =>
            {
                Self::Eindeutig(b)
            }
            _ => Self::Mehrdeutig,
        };
    }
}

struct TheseMitUrsprung<'a> {
    these: &'a OffeneThese,
    stichtag: Zeitpunkt,
    stichtagszeile: u64,
    ursprung: Ursprung<'a>,
}

fn zeilennummer(index: usize) -> Result<u64, String> {
    u64::try_from(index)
        .map(|nr| nr.saturating_add(2))
        .map_err(|_| "Journalzeilennummer zu groß".to_owned())
}

fn stichtagszeilen(eintraege: &[(Art, String)]) -> BTreeMap<Datum, u64> {
    eintraege
        .iter()
        .enumerate()
        .filter_map(|(i, (art, roh))| {
            if *art != Art::Wissensstichtag {
                return None;
            }
            Some((Datum::lies(roh.split_once(';')?.0)?, zeilennummer(i).ok()?))
        })
        .collect()
}

/// Wählt neue Quellenfassungen mit identischer Herkunft zum ursprünglichen Thesenbeleg.
///
/// Der ursprüngliche Beleg muss bereits zur festgeschriebenen Wissenssicht vorliegen.
/// Fehlt eine eindeutige Quellenidentität, entsteht kein Fragefall. Titelähnlichkeit und
/// Wertnennungen reichen nicht. Rückmeldungen der Berater fließen hier nicht ein.
///
/// # Errors
/// Die Prüfung kann vom Aufrufer angehalten werden. Eine abweichende Belegfolge ist ein Fehler.
pub fn neue_faelle(
    stand: &Stand,
    eintraege: &[(Art, String)],
    neue_zeilen: &BTreeSet<u64>,
    thesen: &[OffeneThese],
    schritt: &mut dyn FnMut() -> Result<(), String>,
) -> Result<Vec<Fall>, String> {
    if neue_zeilen.is_empty() || thesen.is_empty() {
        return Ok(Vec::new());
    }
    let stichtagszeilen = stichtagszeilen(eintraege);
    let mut thesen_nach_ereignis = BTreeMap::<_, Vec<_>>::new();
    for t in thesen {
        let (Some(stichtag), Some(&stichtagszeile)) =
            (stand.wissensstichtag(t.tag), stichtagszeilen.get(&t.tag))
        else {
            continue;
        };
        thesen_nach_ereignis
            .entry(ohne_wertmarkierung(&t.ereignis))
            .or_default()
            .push(TheseMitUrsprung {
                these: t,
                stichtag,
                stichtagszeile,
                ursprung: Ursprung::Fehlend,
            });
    }
    if thesen_nach_ereignis.is_empty() {
        return Ok(Vec::new());
    }
    let spaeteste_stichtagszeile = thesen_nach_ereignis
        .values()
        .flatten()
        .map(|t| t.stichtagszeile)
        .max()
        .unwrap_or(0);

    let mut nachrichten = stand.nachrichten().iter();
    let mut termine = stand.termine().iter();
    let mut belege = Vec::new();
    for (index, (art, rohzeile)) in eintraege.iter().enumerate() {
        schritt()?;
        let beleg = match art {
            Art::Nachricht => {
                Beleg::Nachricht(nachrichten.next().ok_or("Nachrichtenreferenz fehlt")?)
            }
            Art::Termin => Beleg::Termin(termine.next().ok_or("Terminreferenz fehlt")?),
            _ => continue,
        };
        let zeile = zeilennummer(index)?;
        let b = Journalbeleg {
            zeile,
            rohzeile,
            beleg,
        };
        if zeile < spaeteste_stichtagszeile
            && let Some(ereignis) = beleg.ereignis()
            && let Some(thesen) = thesen_nach_ereignis.get_mut(ereignis.as_str())
        {
            for t in thesen {
                if zeile < t.stichtagszeile && beleg.abruf() <= t.stichtag {
                    t.ursprung.fuege_hinzu(b);
                }
            }
        }
        if neue_zeilen.contains(&zeile) {
            belege.push(b);
        }
    }
    let mut faelle = Vec::new();
    for b in belege {
        for t in thesen_nach_ereignis.values().flatten() {
            let Ursprung::Eindeutig(ursprung) = t.ursprung else {
                continue;
            };
            if b.zeile > t.stichtagszeile
                && b.beleg.abruf() > t.stichtag
                && let Some(fall) = fall(t.these, ursprung, b)
            {
                faelle.push(fall);
            }
        }
    }
    Ok(faelle)
}

fn fall(these: &OffeneThese, alt: Journalbeleg<'_>, neu: Journalbeleg<'_>) -> Option<Fall> {
    let (art, quelle, _) = alt.beleg.quellen_id()?;
    if alt.beleg.quellen_id() != neu.beleg.quellen_id() {
        return None;
    }
    let vorher = alt.beleg.aussage();
    let beleg = neu.beleg.aussage();
    if vorher == beleg {
        return None;
    }
    let mut h = Sha256::new();
    for feld in [
        "neuroschlopf-frage-v1",
        these.wer,
        &these.tag.to_string(),
        these.isin.als_str(),
        &these.ereignis,
        alt.rohzeile,
        neu.rohzeile,
    ] {
        h.update(feld.len().to_string());
        h.update(b":");
        h.update(feld);
    }
    Some(Fall {
        schluessel: format!("{:x}", h.finalize()),
        these_berater: these.wer,
        these_vom: these.tag,
        these_gueltig_bis: these.gueltig_bis,
        isin: these.isin.clone(),
        these: these.grundtext.clone(),
        ursprung_zeile: alt.zeile,
        beleg_zeile: neu.zeile,
        quelle: quelle.to_owned(),
        art,
        vorher,
        beleg,
        abruf: neu.beleg.abruf(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use depot_journal::Journal;
    use depot_kern::{Betroffen, Termin, Verlaesslichkeit, Zeitbezug};

    use super::*;

    struct Testjournal(PathBuf);

    impl Testjournal {
        fn neu() -> Result<Self, String> {
            static ZAEHLER: AtomicU32 = AtomicU32::new(0);
            let nr = ZAEHLER.fetch_add(1, Ordering::Relaxed);
            let p =
                std::env::temp_dir().join(format!("neuro_fragefall_{}_{nr}", std::process::id()));
            fs::create_dir(&p).map_err(|e| e.to_string())?;
            Journal::einrichten(&p.join("journal.txt")).map_err(|e| format!("{e:?}"))?;
            Ok(Self(p))
        }

        fn journal(&self) -> Result<Journal, String> {
            Journal::oeffnen(&self.0.join("journal.txt")).map_err(|e| format!("{e:?}"))
        }
    }

    impl Drop for Testjournal {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn zeit(s: &str) -> Result<Zeitpunkt, String> {
        Zeitpunkt::lies(s).ok_or_else(|| format!("Testzeit fehlt: {s}"))
    }

    fn beispielthese(stichtag: Zeitpunkt, ereignis: String) -> Result<OffeneThese, String> {
        let tag = stichtag.frankfurt_tag().ok_or("Tag der These fehlt")?;
        Ok(OffeneThese {
            wer: "Dachs",
            tag,
            gueltig_bis: tag.plus_tage(4).ok_or("Zieltag fehlt")?,
            isin: Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?,
            text: "Der Cloud-Umsatz steigt.".into(),
            grundtext: "Der Cloud-Umsatz steigt.".into(),
            ereignis,
            belege: BTreeMap::new(),
        })
    }

    fn nachricht(
        j: &mut Journal,
        abruf: Zeitpunkt,
        text: &str,
    ) -> Result<(NachrichtBeleg, u64), String> {
        let quelle = NachrichtBeleg::neu("test-rss", abruf, text.as_bytes(), abruf, text)?;
        assert!(j.belege_nachricht(&quelle).map_err(|e| format!("{e:?}"))?);
        let zeile = zeilennummer(j.eintraege().len().saturating_sub(1))?;
        Ok((quelle, zeile))
    }

    #[test]
    fn korrigierte_originalmeldung_wird_auch_ohne_ki_urteil_ein_fragefall() -> Result<(), String> {
        let daten = Testjournal::neu()?;
        let mut j = daten.journal()?;
        let alt = zeit("2026-11-12T08:00:00Z")?;
        let stichtag = zeit("2026-11-12T18:00:00Z")?;
        let neu = zeit("2026-11-13T08:00:00Z")?;
        let (original, alt_zeile) = nachricht(
            &mut j,
            alt,
            "SAP erwartet Wachstum | https://example.test/sap",
        )?;
        let ereignis =
            crate::papiertag::nachricht_text(&original).ok_or("Sichtbares Ereignis fehlt")?;
        let mut these = beispielthese(stichtag, ereignis)?;
        j.belege_wissensstichtag(these.tag, stichtag)
            .map_err(|e| format!("{e:?}"))?;
        let (_, korrektur) = nachricht(
            &mut j,
            neu,
            "SAP erwartet einen Rückgang | https://example.test/sap",
        )?;
        let (_, nennung) = nachricht(
            &mut j,
            neu,
            "SAP erwartet einen Rückgang | https://example.test/anderer-beleg",
        )?;
        let zeilen = BTreeSet::from([korrektur, nennung]);
        let fragefaelle = |j: &Journal, t: &OffeneThese| {
            neue_faelle(
                j.stand(),
                j.eintraege(),
                &zeilen,
                std::slice::from_ref(t),
                &mut || Ok(()),
            )
        };
        let faelle = fragefaelle(&j, &these)?;
        assert_eq!(faelle.len(), 1);
        let fall = faelle.first().ok_or("Quellenkorrektur fehlt")?;
        assert_eq!(
            (fall.ursprung_zeile, fall.beleg_zeile),
            (alt_zeile, korrektur)
        );
        assert_eq!(fall.art, Art::Nachricht);
        assert_eq!(fall.these_gueltig_bis, these.gueltig_bis);
        assert_eq!(fall.vorher, "SAP erwartet Wachstum");
        assert_eq!(fall.beleg, "SAP erwartet einen Rückgang");
        assert_eq!(fall.schluessel.len(), 64);

        // Auch ein bereits protokolliertes Urteil macht eine andere Quelle nicht zum Fragefall.
        these.belege.insert(nennung, "Früheres Urteil".into());
        drop(j);
        let j = daten.journal()?;
        assert_eq!(fragefaelle(&j, &these)?, faelle);
        assert!(
            neue_faelle(
                j.stand(),
                j.eintraege(),
                &BTreeSet::new(),
                &[these],
                &mut || Ok(())
            )?
            .is_empty()
        );
        Ok(())
    }

    fn terminfall(mehrdeutiger_ursprung: bool) -> Result<Vec<Fall>, String> {
        let daten = Testjournal::neu()?;
        let mut j = daten.journal()?;
        let alt = zeit("2026-11-12T08:00:00Z")?;
        let stichtag = zeit("2026-11-12T18:00:00Z")?;
        let neu = zeit("2026-11-13T08:00:00Z")?;
        let datum = Datum::lies("2026-11-16").ok_or("Termin fehlt")?;
        let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let termin = |kennung, tag| {
            Termin::neu(
                kennung,
                "SAP meldet Zahlen",
                Zeitbezug::Tag(tag),
                vec![Betroffen::Wert(isin.clone())],
                Verlaesslichkeit::Bestaetigt,
            )
            .map_err(|e| format!("{e:?}"))
        };
        let t1 = termin("sap-zahlen", datum)?;
        let these = beispielthese(stichtag, termin_text(&t1).ok_or("Sichtbarer Termin fehlt")?)?;
        let belege = |j: &mut Journal, zeit: Zeitpunkt, t: Termin| {
            let beleg = TerminBeleg::neu("test-kalender", Herkunft::Quelle, zeit, b"Kalender", t)?;
            j.belege_termin(&beleg).map_err(|e| format!("{e:?}"))?;
            zeilennummer(j.eintraege().len().saturating_sub(1))
        };
        belege(&mut j, alt, t1)?;
        if mehrdeutiger_ursprung {
            belege(&mut j, alt, termin("anderer-termin", datum)?)?;
        }
        j.belege_wissensstichtag(these.tag, stichtag)
            .map_err(|e| format!("{e:?}"))?;
        let neuer_tag = datum.plus_tage(1).ok_or("Neuer Termin fehlt")?;
        let neue_zeile = belege(&mut j, neu, termin("sap-zahlen", neuer_tag)?)?;
        neue_faelle(
            j.stand(),
            j.eintraege(),
            &BTreeSet::from([neue_zeile]),
            &[these],
            &mut || Ok(()),
        )
    }

    #[test]
    fn terminverschiebung_braucht_eindeutigen_ursprung() -> Result<(), String> {
        let faelle = terminfall(false)?;
        assert_eq!(faelle.len(), 1);
        let fall = faelle.first().ok_or("Verschobener Termin fehlt")?;
        assert_eq!(fall.art, Art::Termin);
        assert!(fall.vorher.contains("2026-11-16"));
        assert!(fall.beleg.contains("2026-11-17"));
        assert!(terminfall(true)?.is_empty());
        Ok(())
    }
}
