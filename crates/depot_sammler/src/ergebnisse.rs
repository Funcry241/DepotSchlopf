//! Quartalszahlen aus dem Ergebniskalender von Alpha Vantage als Termine (#120, R39).
//!
//! Abruf: `function=EARNINGS_CALENDAR&horizon=3month`, eine CSV mit genau der Kopfzeile `KOPF`.
//! Nur Werte der Liste an der US-Boerse ergeben Termine (der Kalender kennt die Xetra-Werte
//! nicht; das ist eine Luecke der Quelle, sie wird nicht geraten). Eine Zeile, die nicht genau
//! die Form hat, verwirft die ganze Antwort, wie bei der EZB.
//!
//! - **Kennung:** `ergebnis-<kuerzel>-<Quartalsende>`. Das Quartal ist die Identitaet;
//!   verschiebt sich der Tag, wird der Termin unter derselben Kennung fortgeschrieben.
//! - **Verlaesslichkeit:** angekuendigt, weil es eine Drittquelle ist und nicht das Unternehmen
//!   selbst.
//! - **Abruf:** hoechstens ein Versuch je Frankfurter Kalendertag, nur mit
//!   Schluessel von Alpha Vantage. Die Antwort wird gegen jeden Schluessel geprueft; enthaelt
//!   sie einen, wird sie weder abgelegt noch gelesen (`Bericht::sicherheitsabbruch`).

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use depot_journal::{Fortschreibung, Journal, JournalFehler, TerminBeleg};
use depot_kern::liste::Wert;
use depot_kern::{
    Betroffen, Boerse, Datum, Herkunft, Termin, TerminFehler, Verlaesslichkeit, Zeitbezug,
    Zeitpunkt,
};

use crate::netz::Netz;
use crate::sammeln::{SammelFehler, lege_ab};
use crate::schluessel::{Anbieter, Zugang};

/// Name der Quelle im Journal.
pub const QUELLE_ERGEBNISSE: &str = "alphavantage-ergebnisse";
/// Die Kopfzeile, genau so.
pub const KOPF: &str = "symbol,name,reportDate,fiscalDateEnding,estimate,currency,timeOfTheDay";

/// Warum der Text keine Termine ergibt. Zeilen zählen ab 1, die Kopfzeile ist Zeile 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KalenderFehler {
    Kopf,
    Zeile(usize),
    Termin(usize, TerminFehler),
}

fn tt_mm_jjjj(d: Datum) -> String {
    format!("{:02}.{:02}.{}", d.tag(), d.monat(), d.jahr())
}

/// Liest den ganzen Kalender; nur Werte der Liste an der US-Börse ergeben Termine.
///
/// # Errors
/// Kopf oder eine Zeile weicht ab: dann gibt es keinen Termin.
pub fn lies_kalender(csv: &str, werte: &[Wert]) -> Result<Vec<Termin>, KalenderFehler> {
    let mut zeilen = csv.lines();
    if zeilen.next().map(str::trim_end) != Some(KOPF) {
        return Err(KalenderFehler::Kopf);
    }
    let mut termine = Vec::new();
    let mut kennungen = BTreeSet::new();
    for (i, zeile) in zeilen.enumerate() {
        let nr = i.saturating_add(2);
        let zeile = zeile.trim_end_matches('\r');
        if zeile.is_empty() {
            continue;
        }
        // Der Name kann in Anführungszeichen Kommas enthalten ("STAR GROUP, L.P."): Symbol von
        // links, die fünf festen Felder von rechts, der Rest ist der Name.
        let Some((symbol, rest)) = zeile.split_once(',') else {
            return Err(KalenderFehler::Zeile(nr));
        };
        let f: Vec<&str> = rest.rsplitn(6, ',').collect();
        let [tageszeit, _waehrung, _schaetzung, quartal, bericht, _name] = f.as_slice() else {
            return Err(KalenderFehler::Zeile(nr));
        };
        let (Some(bericht), Some(quartal)) = (Datum::lies(bericht), Datum::lies(quartal)) else {
            return Err(KalenderFehler::Zeile(nr));
        };
        let zusatz = match *tageszeit {
            "" => "",
            "pre-market" => ", vor Börsenbeginn",
            "post-market" => ", nach Börsenschluss",
            _ => return Err(KalenderFehler::Zeile(nr)),
        };
        let Some(w) = werte
            .iter()
            .find(|w| w.boerse == Boerse::Nyse && w.kuerzel == *symbol)
        else {
            continue; // der Rest des Marktes
        };
        let kennung = format!("ergebnis-{}-{quartal}", symbol.to_ascii_lowercase());
        if !kennungen.insert(kennung.clone()) {
            return Err(KalenderFehler::Zeile(nr));
        }
        let titel = format!(
            "{}: Quartalszahlen (Quartal bis {}){zusatz}",
            w.name,
            tt_mm_jjjj(quartal)
        );
        let t = Termin::neu(
            &kennung,
            &titel,
            Zeitbezug::Tag(bericht),
            vec![Betroffen::Wert(w.isin.clone())],
            Verlaesslichkeit::Angekuendigt,
        )
        .map_err(|e| KalenderFehler::Termin(nr, e))?;
        termine.push(t);
    }
    Ok(termine)
}

/// Die Abrufadresse mit Schluessel.
fn adresse(schluessel: &str) -> String {
    format!(
        "https://www.alphavantage.co/query?function=EARNINGS_CALENDAR&horizon=3month&apikey={schluessel}"
    )
}

/// Der Bericht eines Kalenderabrufs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KalenderBericht {
    /// Kein Abruf: kein Schluessel oder heute schon abgerufen, mit Grund.
    pub uebersprungen: Option<String>,
    pub rohdatei: Option<PathBuf>,
    pub neu: usize,
    pub geaendert: usize,
    pub unveraendert: usize,
    /// Kuenftige Termine dieser Quelle im Journal, die im Kalender fehlen, aufsteigend.
    pub fehlen: Vec<String>,
    /// Warum nichts belegt wurde.
    pub fehler: Option<String>,
    /// Die Antwort enthielt einen Schluessel: Kurssperre anlegen.
    pub sicherheitsabbruch: bool,
}

/// Vermerkt auch einen Abruf, der vor der Rohablage scheitert.
fn merke_abruf(ablage: &Path, tag: Datum) -> Result<bool, String> {
    let ordner = ablage.join(QUELLE_ERGEBNISSE);
    fs::create_dir_all(&ordner).map_err(|e| format!("{}: {e}", ordner.display()))?;
    let pfad = ordner.join(format!("abruf-{tag}.vermerkt"));
    match OpenOptions::new().write(true).create_new(true).open(&pfad) {
        Ok(f) => f
            .sync_all()
            .map(|()| true)
            .map_err(|e| format!("{}: {e}", pfad.display())),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(format!("{}: {e}", pfad.display())),
    }
}

/// Ein Abruf des Ergebniskalenders.
///
/// # Errors
/// Journal, Ablage oder Uhr versagen. Was bis dahin belegt ist, bleibt belegt.
pub fn sammle_ergebnisse(
    journal: &mut Journal,
    netz: &mut dyn Netz,
    uhr: &mut dyn FnMut() -> Option<Zeitpunkt>,
    ablage: &Path,
    zugang: &Zugang,
    werte: &[Wert],
) -> Result<KalenderBericht, SammelFehler> {
    let mut bericht = KalenderBericht::default();
    let Some(schluessel) = zugang.von(Anbieter::AlphaVantage) else {
        bericht.uebersprungen = Some("kein Schlüssel von Alpha Vantage".into());
        return Ok(bericht);
    };
    let vorher = uhr().ok_or(SammelFehler::Uhr)?;
    let (heute, ..) = vorher.in_frankfurt().ok_or(SammelFehler::Uhr)?;
    if !merke_abruf(ablage, heute).map_err(SammelFehler::Ablage)? {
        bericht.uebersprungen = Some("heute schon abgerufen".into());
        return Ok(bericht);
    }
    let roh = match netz.hole(&adresse(schluessel.als_str())) {
        Ok(r) => r,
        Err(e) => {
            bericht.fehler = Some(zugang.bereinige(&format!("Abruf: {e}")));
            return Ok(bericht);
        }
    };
    if zugang.steht_in(&roh) {
        bericht.sicherheitsabbruch = true;
        bericht.fehler = Some("Die Antwort enthielt einen Schlüssel; nichts abgelegt".into());
        return Ok(bericht);
    }
    let abruf = uhr().ok_or(SammelFehler::Uhr)?;
    bericht.rohdatei = Some(
        lege_ab(ablage, QUELLE_ERGEBNISSE, abruf, ".csv", &roh).map_err(SammelFehler::Ablage)?,
    );
    let text = String::from_utf8_lossy(&roh);
    if text.trim_start().starts_with('{') {
        let kurz: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        bericht.fehler = Some(format!(
            "Hinweis der Quelle statt Kalender: {}",
            kurz.chars().take(200).collect::<String>()
        ));
        return Ok(bericht);
    }
    let termine = match lies_kalender(&text, werte) {
        Ok(t) => t,
        Err(e) => {
            bericht.fehler = Some(format!("Kalender unlesbar: {e:?}"));
            return Ok(bericht);
        }
    };
    let (tag, ..) = abruf.in_frankfurt().ok_or(SammelFehler::Uhr)?;
    let im_kalender: BTreeSet<&str> = termine.iter().map(Termin::kennung).collect();
    bericht.fehlen = journal
        .stand()
        .aktuelle_termine()
        .into_iter()
        .filter(|(kennung, b)| {
            b.quelle() == QUELLE_ERGEBNISSE
                && !im_kalender.contains(kennung)
                && b.termin().zeit().tag().is_some_and(|t| t >= tag)
        })
        .map(|(kennung, _)| kennung.to_string())
        .collect();
    for termin in termine {
        let beleg = TerminBeleg::neu(QUELLE_ERGEBNISSE, Herkunft::Quelle, abruf, &roh, termin)
            .map_err(|e| SammelFehler::Journal(JournalFehler::Abgelehnt(e)))?;
        match journal
            .belege_termin(&beleg)
            .map_err(SammelFehler::Journal)?
        {
            Fortschreibung::Neu => bericht.neu = bericht.neu.saturating_add(1),
            Fortschreibung::Geaendert => bericht.geaendert = bericht.geaendert.saturating_add(1),
            Fortschreibung::Unveraendert => {
                bericht.unveraendert = bericht.unveraendert.saturating_add(1);
            }
        }
    }
    Ok(bericht)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VORLAGE: &str = r#"symbol,name,reportDate,fiscalDateEnding,estimate,currency,timeOfTheDay
JPM,JPMORGAN CHASE & COMPANY,2027-01-14,2026-12-31,,USD,
KO,COCA-COLA CO,2027-02-10,2026-12-31,0.55,USD,pre-market
NVDA,NVIDIA CORPORATION,2026-11-17,2026-10-31,2.49,USD,post-market
SGU,"STAR GROUP, L.P.",2026-12-14,2026-09-30,,USD,
"#;

    fn liste() -> Result<Vec<Wert>, String> {
        depot_kern::liste::beobachtungsliste()
    }

    #[test]
    fn nur_werte_der_liste_und_felder() -> Result<(), String> {
        let t = lies_kalender(VORLAGE, &liste()?).map_err(|e| format!("{e:?}"))?;
        let (Some(jpm), Some(ko)) = (t.first(), t.get(1)) else {
            return Err(format!("zwei Termine erwartet: {t:?}"));
        };
        assert_eq!(t.len(), 2);
        assert_eq!(jpm.kennung(), "ergebnis-jpm-2026-12-31");
        assert_eq!(
            jpm.titel(),
            "JPMorgan Chase: Quartalszahlen (Quartal bis 31.12.2026)"
        );
        assert_eq!(jpm.zeit().tag(), Datum::neu(2027, 1, 14));
        assert_eq!(
            jpm.betroffen()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["US46625H1005".to_string()]
        );
        assert_eq!(jpm.verlaesslichkeit(), Verlaesslichkeit::Angekuendigt);
        assert_eq!(
            ko.titel(),
            "Coca-Cola: Quartalszahlen (Quartal bis 31.12.2026), vor Börsenbeginn"
        );
        Ok(())
    }

    #[test]
    fn name_mit_komma_ist_kein_fehler() -> Result<(), String> {
        let liste = liste()?;
        assert_eq!(lies_kalender(VORLAGE, &liste).map(|t| t.len()), Ok(2));
        let kurz = VORLAGE.replace(",2026-12-14,2026-09-30,,USD,", ",2026-12-14");
        assert_eq!(lies_kalender(&kurz, &liste), Err(KalenderFehler::Zeile(5)));
        Ok(())
    }

    #[test]
    fn falscher_kopf_belegt_nichts() -> Result<(), String> {
        let liste = liste()?;
        let csv = VORLAGE.replace(",timeOfTheDay", "");
        assert_eq!(lies_kalender(&csv, &liste), Err(KalenderFehler::Kopf));
        Ok(())
    }

    #[test]
    fn unbekannte_tageszeit_verwirft_alles() -> Result<(), String> {
        let liste = liste()?;
        let csv = VORLAGE.replace("pre-market", "during-market");
        assert_eq!(lies_kalender(&csv, &liste), Err(KalenderFehler::Zeile(3)));
        Ok(())
    }

    #[test]
    fn hinweis_statt_kalender() -> Result<(), String> {
        let liste = liste()?;
        let csv = "{\"Information\":\"rate limit\"}";
        assert_eq!(lies_kalender(csv, &liste), Err(KalenderFehler::Kopf));
        Ok(())
    }
}
