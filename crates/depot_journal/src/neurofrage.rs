//! Belegte Anfragen von NeuroSchlopf an Dachs und Maus.

use std::path::{Path, PathBuf};

use depot_kern::json::{self, Json, zitiere};
use depot_kern::{BeraterId, Datum, Isin, Zeitpunkt};
use depot_ki::Sperre;

use crate::{AufrufErlaubnis, KiAbrechnung, KiAufruf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NeuroRunde {
    Erstfrage,
    Rueckfrage,
}

impl NeuroRunde {
    pub fn text(self) -> &'static str {
        match self {
            Self::Erstfrage => "erstfrage",
            Self::Rueckfrage => "rueckfrage",
        }
    }

    pub fn lies(text: &str) -> Option<Self> {
        match text {
            "erstfrage" => Some(Self::Erstfrage),
            "rueckfrage" => Some(Self::Rueckfrage),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NeuroAuftragId {
    fall: String,
    runde: NeuroRunde,
}

impl NeuroAuftragId {
    pub fn neu(fall: &str, runde: NeuroRunde) -> Option<Self> {
        crate::kasse::ist_abdruck(fall).then(|| Self {
            fall: fall.into(),
            runde,
        })
    }

    pub fn fall(&self) -> &str {
        &self.fall
    }

    pub fn runde(&self) -> NeuroRunde {
        self.runde
    }
}

/// Ein echter, dauerhaft reservierter Versuch zählt auch bei unterbliebenem Versand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeuroVersuch {
    pub id: NeuroAuftragId,
    pub wer: BeraterId,
    pub datum: Datum,
    pub buchung: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum NeuroSperre {
    BereitsReserviert,
    ErstfrageOffen,
    NeuroTageslimit,
    PapierlaufOffen,
    Ki(Sperre),
}

impl From<Sperre> for NeuroSperre {
    fn from(value: Sperre) -> Self {
        Self::Ki(value)
    }
}

/// Einmalige Erlaubnis für genau den belegten NeuroSchlopf-Auftrag.
#[derive(Debug, PartialEq, Eq)]
pub struct NeuroAufrufErlaubnis {
    pub(crate) id: NeuroAuftragId,
    pub(crate) erlaubnis: AufrufErlaubnis,
}

impl NeuroAufrufErlaubnis {
    pub fn auftrag(&self) -> &NeuroAuftragId {
        &self.id
    }

    pub fn buchung(&self) -> usize {
        self.erlaubnis.buchungsnummer()
    }

    pub fn wer(&self) -> BeraterId {
        self.erlaubnis.buchung().berater
    }

    pub fn aufruf(self) -> AufrufErlaubnis {
        self.erlaubnis
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeuroFrage {
    pub id: NeuroAuftragId,
    pub wer: BeraterId,
    pub thesen_wer: BeraterId,
    pub these_vom: Datum,
    pub these_gueltig_bis: Datum,
    pub isin: Isin,
    pub these: String,
    pub bisher_zeile: Option<u64>,
    pub beleg_zeile: u64,
    pub quelle: String,
    pub bisher: Option<String>,
    pub jetzt: String,
    pub bekannt: Zeitpunkt,
    pub nachfrage: Option<String>,
}

impl NeuroFrage {
    /// Eine historische Vorprüfung hat einen Quellenbeleg und keine vorherige Fassung.
    pub fn ist_vorpruefung(&self) -> bool {
        self.bisher_zeile.is_none()
    }

    pub fn text(&self) -> String {
        format!(
            concat!(
                "{{\"fall\":{},\"runde\":{},\"wer\":{},\"thesen_wer\":{},",
                "\"these_vom\":{},\"these_gueltig_bis\":{},\"isin\":{},\"these\":{},\"bisher_zeile\":{},",
                "\"beleg_zeile\":{},\"quelle\":{},\"bisher\":{},\"jetzt\":{},",
                "\"bekannt\":{},\"nachfrage\":{}}}"
            ),
            zitiere(self.id.fall()),
            zitiere(self.id.runde().text()),
            zitiere(wer_text(self.wer)),
            zitiere(wer_text(self.thesen_wer)),
            zitiere(&self.these_vom.to_string()),
            zitiere(&self.these_gueltig_bis.to_string()),
            zitiere(self.isin.als_str()),
            zitiere(&self.these),
            self.bisher_zeile
                .map_or_else(|| "null".into(), |z| z.to_string()),
            self.beleg_zeile,
            zitiere(&self.quelle),
            optional(self.bisher.as_deref()),
            zitiere(&self.jetzt),
            self.bekannt.sekunden(),
            optional(self.nachfrage.as_deref())
        )
    }

    pub fn lies(text: &str) -> Option<Self> {
        if text.len() > 40_000 {
            return None;
        }
        let j = json::lies(text).ok()?;
        let feld = |name| j.feld(name)?.text();
        let f = Self {
            id: NeuroAuftragId::neu(feld("fall")?, NeuroRunde::lies(feld("runde")?)?)?,
            wer: lies_wer(feld("wer")?)?,
            thesen_wer: lies_wer(feld("thesen_wer")?)?,
            these_vom: Datum::lies(feld("these_vom")?)?,
            these_gueltig_bis: Datum::lies(feld("these_gueltig_bis")?)?,
            isin: Isin::neu(feld("isin")?).ok()?,
            these: feld("these")?.into(),
            bisher_zeile: j
                .feld("bisher_zeile")?
                .ganz()
                .and_then(|z| u64::try_from(z).ok()),
            beleg_zeile: u64::try_from(j.feld("beleg_zeile")?.ganz()?).ok()?,
            quelle: feld("quelle")?.into(),
            bisher: opt_feld(&j, "bisher").ok()?,
            jetzt: feld("jetzt")?.into(),
            bekannt: Zeitpunkt::aus_sekunden(j.feld("bekannt")?.ganz()?)?,
            nachfrage: opt_feld(&j, "nachfrage").ok()?,
        };
        (f.gueltig() && f.text() == text).then_some(f)
    }

    pub fn gueltig(&self) -> bool {
        self.these_vom <= self.these_gueltig_bis
            && self.beleg_zeile > 0
            && self
                .bisher_zeile
                .is_none_or(|n| n > 0 && n < self.beleg_zeile)
            && self.bisher.is_some() == self.bisher_zeile.is_some()
            && (self.id.runde() == NeuroRunde::Rueckfrage) == self.nachfrage.is_some()
            && passt(&self.these, 2_000)
            && passt(&self.quelle, 512)
            && passt(&self.jetzt, 5_000)
            && self.bisher.as_deref().is_none_or(|b| passt(b, 5_000))
            && self.nachfrage.as_deref().is_none_or(|n| passt(n, 500))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NeuroUrteilswert {
    Staerkt,
    Schwaecht,
    KeinBezug,
    Offen,
}

impl NeuroUrteilswert {
    pub fn text(self) -> &'static str {
        match self {
            Self::Staerkt => "staerkt",
            Self::Schwaecht => "schwaecht",
            Self::KeinBezug => "kein_bezug",
            Self::Offen => "offen",
        }
    }

    fn lies(text: &str) -> Option<Self> {
        [Self::Staerkt, Self::Schwaecht, Self::KeinBezug, Self::Offen]
            .into_iter()
            .find(|v| v.text() == text)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeuroUrteil {
    pub wert: NeuroUrteilswert,
    pub begruendung: String,
    pub nachfrage: Option<String>,
}

impl NeuroUrteil {
    pub fn lies(text: &str, runde: NeuroRunde) -> Option<Self> {
        let j = json::lies(text).ok()?;
        let Json::Objekt(felder) = &j else {
            return None;
        };
        if j.doppelte_felder() || felder.len() != 3 {
            return None;
        }
        let urteil = Self {
            wert: NeuroUrteilswert::lies(j.feld("urteil")?.text()?)?,
            begruendung: j.feld("begruendung")?.text()?.trim().into(),
            nachfrage: opt_feld(&j, "nachfrage").ok()?,
        };
        (passt(&urteil.begruendung, 1_000)
            && urteil
                .nachfrage
                .as_deref()
                .is_none_or(|n| runde == NeuroRunde::Erstfrage && passt(n, 500)))
        .then_some(urteil)
    }

    pub fn text(&self) -> String {
        format!(
            "{{\"urteil\":{},\"begruendung\":{},\"nachfrage\":{}}}",
            zitiere(self.wert.text()),
            zitiere(&self.begruendung),
            optional(self.nachfrage.as_deref())
        )
    }
}

/// Schalter des lokalen NeuroSchlopf-Vergleichs, den auch der externe Aufruf beachtet.
pub fn lokal_aktiv(daten: &Path) -> Result<bool, String> {
    match std::fs::read_to_string(daten.join("neuroschlopf.txt")) {
        Ok(s) if s == "an\n" => Ok(true),
        Ok(s) if s == "aus\n" => Ok(false),
        Ok(_) => Err("NeuroSchlopf-Einstellung unlesbar".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err("NeuroSchlopf-Einstellung nicht lesbar".into()),
    }
}

pub fn ergebnis_pfad(auftrag: &Path) -> PathBuf {
    auftrag.with_extension("ergebnis.json")
}

/// Vollständig zurückgekehrte Ergebnisse können später unter kurzer Journalsperre gebucht werden.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeuroErgebnis {
    pub id: NeuroAuftragId,
    pub wer: BeraterId,
    pub buchung: usize,
    pub urteil: Option<NeuroUrteil>,
    pub fehler: Option<String>,
    pub(crate) aufruf: Option<KiAufruf>,
    pub(crate) abrechnung: Option<KiAbrechnung>,
}

impl NeuroErgebnis {
    pub fn neu(
        id: NeuroAuftragId,
        wer: BeraterId,
        buchung: usize,
        antwort: Result<NeuroUrteil, String>,
        aufruf: Option<KiAufruf>,
        abrechnung: Option<KiAbrechnung>,
    ) -> Self {
        let (urteil, fehler) = match antwort {
            Ok(u) => (Some(u), None),
            Err(e) => (None, Some(e)),
        };
        Self {
            id,
            wer,
            buchung,
            urteil,
            fehler,
            aufruf,
            abrechnung,
        }
    }

    pub fn text(&self) -> String {
        let urteil = self
            .urteil
            .as_ref()
            .map_or_else(|| "null".into(), |u| zitiere(&u.text()));
        let aufruf = self.aufruf.as_ref().map(KiAufruf::text);
        let abrechnung = self.abrechnung.as_ref().map(KiAbrechnung::text);
        format!(
            "{{\"fall\":{},\"runde\":{},\"wer\":{},\"buchung\":{},\"urteil\":{},\"fehler\":{},\"aufruf\":{},\"abrechnung\":{}}}",
            zitiere(self.id.fall()),
            zitiere(self.id.runde().text()),
            zitiere(wer_text(self.wer)),
            self.buchung,
            urteil,
            optional(self.fehler.as_deref()),
            optional(aufruf.as_deref()),
            optional(abrechnung.as_deref())
        )
    }

    pub fn lies(text: &str) -> Option<Self> {
        if text.len() > 8_000 {
            return None;
        }
        let j = json::lies(text).ok()?;
        let id = NeuroAuftragId::neu(
            j.feld("fall")?.text()?,
            NeuroRunde::lies(j.feld("runde")?.text()?)?,
        )?;
        let urteil = opt_feld(&j, "urteil")
            .ok()?
            .and_then(|u| NeuroUrteil::lies(&u, id.runde()));
        let ergebnis = Self {
            id,
            wer: lies_wer(j.feld("wer")?.text()?)?,
            buchung: usize::try_from(j.feld("buchung")?.ganz()?).ok()?,
            urteil,
            fehler: opt_feld(&j, "fehler").ok()?,
            aufruf: opt_feld(&j, "aufruf")
                .ok()?
                .and_then(|a| KiAufruf::lies(&a)),
            abrechnung: opt_feld(&j, "abrechnung")
                .ok()?
                .and_then(|a| KiAbrechnung::lies(&a)),
        };
        (ergebnis.passt() && ergebnis.text() == text).then_some(ergebnis)
    }

    pub(crate) fn passt(&self) -> bool {
        self.urteil.is_some() != self.fehler.is_some()
            && self.fehler.as_deref().is_none_or(|f| passt(f, 500))
            && self.aufruf.as_ref().is_none_or(|a| a.nr == self.buchung)
            && self
                .abrechnung
                .as_ref()
                .is_none_or(|a| a.nr == self.buchung)
            && (self.urteil.is_none()
                || self
                    .aufruf
                    .as_ref()
                    .is_some_and(|a| a.ergebnis == crate::KiAufrufErgebnis::Beantwortet))
    }
}

fn passt(text: &str, max: usize) -> bool {
    !text.trim().is_empty() && text.chars().count() <= max && !text.chars().any(char::is_control)
}

fn optional(text: Option<&str>) -> String {
    text.map_or_else(|| "null".into(), zitiere)
}

fn opt_feld(j: &Json, name: &str) -> Result<Option<String>, ()> {
    let w = j.feld(name).ok_or(())?;
    match w {
        Json::Null => Ok(None),
        Json::Text(t) => Ok(Some(t.clone())),
        _ => Err(()),
    }
}

fn wer_text(wer: BeraterId) -> &'static str {
    match wer {
        BeraterId::Dachs => "dachs",
        BeraterId::Maus => "maus",
    }
}

fn lies_wer(text: &str) -> Option<BeraterId> {
    match text {
        "dachs" => Some(BeraterId::Dachs),
        "maus" => Some(BeraterId::Maus),
        _ => None,
    }
}
