//! Das Kassenbuch der KI-Aufrufe im Journal und die `AufrufErlaubnis`.
//!
//! Jede Buchung ist ein Eintrag der Art `Ki`. Das Journal prueft sie nach den Regeln aus
//! `depot_ki`, beim Anhaengen wie beim Oeffnen, auch als unbestaetigten Anhang (K13-02). Es
//! gibt also keinen Eintrag, den das Journal annimmt und der Budgetwaechter ablehnt.
//!
//! **Erst buchen, dann fragen (R8, D06):** Eine `AufrufErlaubnis` entsteht nur in
//! `Journal::reserviere`, nachdem die Buchung der Hoechstkosten synchronisiert und bestaetigt
//! im Journal steht. Ein Absturz danach kostet hoechstens einen ungenutzten Aufruf.
//!
//! **Uebergang bei geaenderten Grenzen:** Vor der ersten Buchung unter neuen Grenzen steht
//! deren Abdruck (`KiGrenzen`). Buchungen unter einem frueheren Abdruck werden nicht nach den
//! heutigen Grenzen beurteilt, zaehlen aber weiter mit: Ein voller Monat bleibt voll. Eine
//! Buchung ohne vorangehenden Abdruck ist unzulaessig.

use depot_kern::modelle::{BeraterEinstellung, Denktiefe, Modell, Modellwahl};
use depot_kern::{BeraterId, Cent};
use depot_ki::{
    Buchung, Datum, KiDeckel, KiGrenzen, Sperre, grenzabdruck, pruefe, pruefe_simulation,
};

use crate::neurofrage::{NeuroAuftragId, NeuroErgebnis, NeuroRunde, NeuroSperre, NeuroVersuch};
use crate::stand::{datum_text, lies_abschluss, lies_datum};

mod siegel {
    /// Nur in `depot_journal` erzeugbar.
    #[derive(Debug, PartialEq, Eq)]
    pub struct Siegel(());

    impl Siegel {
        pub(crate) fn neu() -> Siegel {
            Siegel(())
        }
    }
}

/// Berechtigt zu genau einem KI-Aufruf. Nicht kopierbar, nur durch eine Buchung im Journal
/// erzeugbar. Der KI-Client nimmt sie beim Aufruf als Wert entgegen und verbraucht sie dabei.
#[derive(Debug, PartialEq, Eq)]
pub struct AufrufErlaubnis {
    buchung: Buchung,
    nr: usize,
    deckel: KiDeckel,
    auswahl: Modellwahl,
    zweck: KiZweck,
    _siegel: siegel::Siegel,
}

impl AufrufErlaubnis {
    pub(crate) fn neu(
        buchung: Buchung,
        deckel: KiDeckel,
        auswahl: Modellwahl,
        nr: usize,
        zweck: KiZweck,
    ) -> AufrufErlaubnis {
        AufrufErlaubnis {
            buchung,
            nr,
            deckel,
            auswahl,
            zweck,
            _siegel: siegel::Siegel::neu(),
        }
    }

    /// Nur eine plausible Anbieterabrechnung kann eine Reservierung vermindern.
    pub fn abrechnung(&self, token_ein: u64, token_aus: u64) -> Option<crate::KiAbrechnung> {
        self.abrechnung_mit_cache(token_ein, token_aus, 0, 0)
    }

    /// Bucht die tatsächlichen Kosten mit den beiden belegten Cache-Anteilen.
    pub fn abrechnung_mit_cache(
        &self,
        token_ein: u64,
        token_aus: u64,
        cache_schreiben: u64,
        cache_lesen: u64,
    ) -> Option<crate::KiAbrechnung> {
        if token_ein > u64::from(self.deckel.max_token_ein())
            || token_aus > u64::from(self.deckel.max_token_aus())
        {
            return None;
        }
        let ist =
            self.deckel
                .kosten_mit_cache(token_ein, token_aus, cache_schreiben, cache_lesen)?;
        (ist <= self.buchung.reserviert).then_some(crate::KiAbrechnung {
            nr: self.nr,
            reserviert: self.buchung.reserviert,
            ist,
        })
    }

    pub fn buchungsnummer(&self) -> usize {
        self.nr
    }

    pub fn buchung(&self) -> &Buchung {
        &self.buchung
    }

    pub fn deckel(&self) -> &KiDeckel {
        &self.deckel
    }

    pub fn zweck(&self) -> KiZweck {
        self.zweck
    }

    pub fn modellwahl(&self) -> Modellwahl {
        self.auswahl
    }

    /// Ein Token umfasst mindestens ein Byte. Wer hoechstens `max_token_ein` Bytes sendet,
    /// sendet also sicher nicht mehr Token als erlaubt.
    pub fn eingabe_passt(&self, eingabe: &str) -> bool {
        u32::try_from(eingabe.len()).is_ok_and(|n| n <= self.deckel.max_token_ein())
    }
}

fn berater_text(b: BeraterId) -> &'static str {
    match b {
        BeraterId::Dachs => "Dachs",
        BeraterId::Maus => "Maus",
    }
}

fn buchung_text(b: &Buchung) -> String {
    format!(
        "{};{};{};{}",
        berater_text(b.berater),
        datum_text(b.datum),
        b.zyklus,
        b.reserviert.0
    )
}

fn lies_buchung(text: &str) -> Option<Buchung> {
    let teile: Vec<&str> = text.split(';').collect();
    let [berater, datum, zyklus, cent] = teile.as_slice() else {
        return None;
    };
    let berater = [BeraterId::Dachs, BeraterId::Maus]
        .into_iter()
        .find(|b| berater_text(*b) == *berater)?;
    let reserviert = Cent(cent.parse().ok()?);
    if reserviert.0 < 0 {
        return None;
    }
    Some(Buchung {
        berater,
        datum: lies_datum(datum)?,
        zyklus: zyklus.parse().ok()?,
        reserviert,
    })
}

pub(crate) fn buchung_text_mit_modell(b: &Buchung, auswahl: Option<Modellwahl>) -> String {
    let text = buchung_text(b);
    auswahl.map_or_else(
        || text.clone(),
        |wahl| {
            format!(
                "{text};{};{}",
                wahl.modell().kennung(),
                wahl.denktiefe().kennung()
            )
        },
    )
}

pub(crate) fn lies_buchung_mit_modell(text: &str) -> Option<(Buchung, Option<Modellwahl>)> {
    let (buchung, auswahl) = if let Some(b) = lies_buchung(text) {
        (b, None)
    } else {
        let (text, tiefe) = text.rsplit_once(';')?;
        let (text, modell) = text.rsplit_once(';')?;
        let b = lies_buchung(text)?;
        let auswahl = Modellwahl::neu(b.berater, Modell::lies(modell)?, Denktiefe::lies(tiefe)?)?;
        (b, Some(auswahl))
    };
    (buchung_text_mit_modell(&buchung, auswahl) == text).then_some((buchung, auswahl))
}

/// Tarife der Modellwahl sind Teil des Buchungsregimes. Der alte Abdruck bleibt unverändert.
pub(crate) fn grenzabdruck_fuer_auswahl(auswahl: Option<Modellwahl>) -> String {
    if auswahl.is_none() {
        return grenzabdruck();
    }
    let text = format!(
        "{}|{:?}|{:?}|{:?}|{:?}",
        grenzabdruck(),
        KiDeckel::fuer_modell(Modell::Opus55),
        KiDeckel::kurz_fuer_modell(Modell::Opus55),
        KiDeckel::fuer_modell(Modell::Sol62),
        KiDeckel::kurz_fuer_modell(Modell::Sol62),
    );
    depot_kern::abdruck::sha256_hex(text.as_bytes())
}

pub(crate) fn grenzabdruck_fuer_zweck(auswahl: Option<Modellwahl>, zweck: KiZweck) -> String {
    let abdruck = grenzabdruck_fuer_auswahl(auswahl);
    if zweck == KiZweck::Simulation {
        // Frühere Programmstände lesen diese Buchungen als früheres Budgetregime.
        depot_kern::abdruck::sha256_hex(format!("simulation-monatsrahmen|{abdruck}").as_bytes())
    } else {
        abdruck
    }
}

pub(crate) fn ist_abdruck(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Zweck bestimmt Priorität und Ausgabedeckel; keine freie Konfiguration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KiZweck {
    Erstmeinung,
    Zweitmeinung,
    Abstimmung,
    Vorbereitung,
    Simulation,
    Wochenrueckschau,
    Neurofrage,
}
impl KiZweck {
    pub fn text(self) -> &'static str {
        match self {
            Self::Erstmeinung => "erstmeinung",
            Self::Zweitmeinung => "zweitmeinung",
            Self::Abstimmung => "abstimmung",
            Self::Vorbereitung => "vorbereitung",
            Self::Simulation => "simulation",
            Self::Wochenrueckschau => "wochenrueckschau",
            Self::Neurofrage => "neurofrage",
        }
    }
    fn lies(s: &str) -> Option<Self> {
        [
            Self::Erstmeinung,
            Self::Zweitmeinung,
            Self::Abstimmung,
            Self::Vorbereitung,
            Self::Simulation,
            Self::Wochenrueckschau,
            Self::Neurofrage,
        ]
        .into_iter()
        .find(|z| z.text() == s)
    }
    pub(crate) fn ohne_handelsbeginn(self) -> bool {
        matches!(
            self,
            Self::Simulation | Self::Wochenrueckschau | Self::Neurofrage
        )
    }
    pub(crate) fn deckel(self, modell: Modell) -> KiDeckel {
        match self {
            Self::Abstimmung | Self::Vorbereitung | Self::Wochenrueckschau | Self::Neurofrage => {
                KiDeckel::kurz_fuer_modell(modell)
            }
            _ => KiDeckel::fuer_modell(modell),
        }
    }
}
pub(crate) fn buchung_fuer(b: &Buchung, zweck: KiZweck, auswahl: Option<Modellwahl>) -> String {
    let text = buchung_text_mit_modell(b, auswahl);
    match zweck {
        KiZweck::Erstmeinung => text,
        _ => format!("{text};{}", zweck.text()),
    }
}

/// Das Kassenbuch, wie es aus dem Journal folgt.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KiStand {
    /// Der zuletzt eingetragene Grenzabdruck.
    abdruck: Option<String>,
    buchungen: Vec<Buchung>,
    obergrenzen: Vec<Cent>,
    abgerechnet: std::collections::BTreeSet<usize>,
    aufrufe: std::collections::BTreeMap<usize, crate::KiAufruf>,
    zwecke: Vec<KiZweck>,
    modellwahlen: Vec<Option<Modellwahl>>,
    wiederholungen: std::collections::BTreeMap<usize, usize>,
    neurofragen: Vec<NeuroVersuch>,
    neuroergebnisse: std::collections::BTreeMap<usize, NeuroErgebnis>,
}

/// Abrechnung und neue, am Fensterende noch offene Reservierungen im Journalfenster.
#[derive(Debug)]
pub struct KiKostenfenster {
    pub abgerechnet: Cent,
    pub offen_reserviert: Cent,
}

impl KiStand {
    pub fn buchungen(&self) -> &[Buchung] {
        &self.buchungen
    }

    /// Belegter Zweck und Abrechnungsstatus zu einer Buchungsnummer.
    pub fn buchungsbeleg(&self, nr: usize) -> Option<(KiZweck, bool)> {
        Some((*self.zwecke.get(nr)?, self.abgerechnet.contains(&nr)))
    }

    /// Zählt die Buchungsereignisse zwischen zwei Abschlüssen desselben geprüften Journals.
    /// Eine Abrechnung zählt an ihrer Journalposition, auch bei einer älteren Reservierung.
    pub fn kosten_zwischen_abschluessen(
        &self,
        eintraege: &[(crate::Art, String)],
        anfang: Datum,
        ende: Datum,
    ) -> Option<KiKostenfenster> {
        use crate::{Art, KiAbrechnung};

        let abschluss = |(art, inhalt): &(Art, String), datum| {
            *art == Art::Abschluss && lies_abschluss(inhalt).is_some_and(|a| a.datum == datum)
        };
        let start = eintraege.iter().position(|e| abschluss(e, anfang))?;
        let (vorher, rest) = eintraege.split_at_checked(start.checked_add(1)?)?;
        let stop = rest.iter().position(|e| abschluss(e, ende))?;
        let mut nr = vorher.iter().filter(|(art, _)| *art == Art::Ki).count();
        let mut reservierungen = std::collections::BTreeMap::new();
        let mut abgerechnet = Cent::NULL;
        for (art, inhalt) in rest.get(..stop)? {
            let abrechnung = match art {
                Art::Ki => {
                    reservierungen.insert(nr, *self.obergrenzen.get(nr)?);
                    nr = nr.checked_add(1)?;
                    None
                }
                Art::KiAbrechnung => Some(KiAbrechnung::lies(inhalt)?),
                Art::NeuroErgebnis => NeuroErgebnis::lies(inhalt)?.abrechnung,
                _ => None,
            };
            if let Some(a) = abrechnung {
                reservierungen.remove(&a.nr);
                abgerechnet = abgerechnet.checked_add(a.kosten())?;
            }
        }
        let offen_reserviert = reservierungen
            .into_values()
            .try_fold(Cent::NULL, Cent::checked_add)?;
        Some(KiKostenfenster {
            abgerechnet,
            offen_reserviert,
        })
    }

    pub fn aufruf(&self, nr: usize) -> Option<&crate::KiAufruf> {
        self.aufrufe.get(&nr)
    }

    pub fn modellwahl(&self, nr: usize) -> Option<Modellwahl> {
        self.modellwahlen.get(nr).copied().flatten()
    }

    pub fn neurofragen(&self) -> &[NeuroVersuch] {
        &self.neurofragen
    }

    pub fn neuroergebnis(&self, buchung: usize) -> Option<&NeuroErgebnis> {
        self.neuroergebnisse.get(&buchung)
    }

    pub(crate) fn pruefe_neuroauftrag(
        &self,
        id: &NeuroAuftragId,
        wer: BeraterId,
    ) -> Result<(), NeuroSperre> {
        let gleiche_faelle = || {
            self.neurofragen
                .iter()
                .filter(|v| v.wer == wer && v.id.fall() == id.fall())
        };
        if gleiche_faelle().any(|v| v.id.runde() == id.runde()) {
            return Err(NeuroSperre::BereitsReserviert);
        }
        if id.runde() == NeuroRunde::Rueckfrage
            && !gleiche_faelle().any(|v| {
                v.id.runde() == NeuroRunde::Erstfrage
                    && self
                        .neuroergebnis(v.buchung)
                        .is_some_and(|e| e.urteil.as_ref().is_some_and(|u| u.nachfrage.is_some()))
            })
        {
            return Err(NeuroSperre::ErstfrageOffen);
        }
        Ok(())
    }

    pub(crate) fn pruefe_tageslimit(
        &self,
        berater: BeraterId,
        datum: Datum,
        zweck: KiZweck,
        einstellung: BeraterEinstellung,
    ) -> Result<(), Sperre> {
        if einstellung.auswahl.modell().berater() != berater {
            return Err(Sperre::Rechenfehler);
        }
        if zweck == KiZweck::Simulation
            || einstellung.max_aufrufe >= depot_kern::modelle::MAX_BERATER_AUFRUFE
        {
            return Ok(());
        }
        let heute = self
            .buchungen
            .iter()
            .zip(&self.zwecke)
            .filter(|(b, zweck)| {
                b.berater == berater && b.datum == datum && **zweck != KiZweck::Simulation
            })
            .count();
        if heute >= einstellung.max_aufrufe {
            return Err(Sperre::TagVoll);
        }
        Ok(())
    }

    /// Buchungsnummer des erneut gestellten Aufrufs.
    pub fn wiederholung(&self, erste: usize) -> Option<usize> {
        self.wiederholungen.get(&erste).copied()
    }

    pub fn ist_wiederholungsbuchung(&self, nr: usize) -> bool {
        self.wiederholungen.values().any(|&buchung| buchung == nr)
    }

    pub(crate) fn wiederholung_fuer(
        &self,
        erste: usize,
    ) -> Result<(Buchung, KiDeckel, KiZweck, Modellwahl), Sperre> {
        let sperre = || Sperre::Kassenbuch("Keine wiederholbare KI-Buchung".into());
        let ursprung = self.buchungen.get(erste).ok_or_else(sperre)?;
        let zweck = *self.zwecke.get(erste).ok_or_else(sperre)?;
        if matches!(zweck, KiZweck::Vorbereitung | KiZweck::Neurofrage)
            || self.wiederholungen.contains_key(&erste)
            || self.ist_wiederholungsbuchung(erste)
            || !self.abgerechnet.contains(&erste)
            || !self
                .aufruf(erste)
                .is_some_and(|a| a.ergebnis == crate::KiAufrufErgebnis::Ausgabelimit)
        {
            return Err(sperre());
        }
        let wahl = self
            .modellwahlen
            .get(erste)
            .ok_or_else(sperre)?
            .unwrap_or_else(|| Modellwahl::standard(ursprung.berater));
        let deckel = zweck.deckel(wahl.modell()).fuer_wiederholung();
        let (buchung, deckel) =
            buche_mit_deckel(self, ursprung.datum, ursprung.zyklus, zweck, wahl, deckel)?;
        if zweck == KiZweck::Erstmeinung {
            pruefe_zusatzreserve(
                self,
                wahl.modell(),
                ursprung.datum,
                buchung.reserviert,
                zweck,
            )?;
        }
        Ok((buchung, deckel, zweck, wahl))
    }

    fn buche_wiederholung(&mut self, text: &str, ursprung: &str) -> Result<KiZweck, String> {
        let erste = ursprung
            .parse::<usize>()
            .map_err(|_| "Erste Buchung unlesbar")?;
        let (b, _, zweck, wahl) = self
            .wiederholung_fuer(erste)
            .map_err(|s| format!("{s:?}"))?;
        if buchung_text_mit_modell(&b, Some(wahl)) != text || erste.to_string() != ursprung {
            return Err("Wiederholungsbuchung passt nicht zum ersten Aufruf".into());
        }
        self.wiederholungen.insert(erste, self.buchungen.len());
        self.speichere_buchung(b, zweck, Some(wahl));
        Ok(zweck)
    }

    /// Prüft mit denselben Grenzen wie eine neue Reservierung.
    pub fn pruefe_reservierung(
        &self,
        berater: BeraterId,
        datum: Datum,
        zyklus: u32,
        zweck: KiZweck,
        einstellung: BeraterEinstellung,
    ) -> Result<Cent, Sperre> {
        self.naechste_mit_einstellung(berater, datum, zyklus, zweck, einstellung)
            .map(|(buchung, _)| buchung.reserviert)
    }

    /// Prüft alle Aufrufe einer Simulation gegen denselben Kassenstand.
    /// Der Aufrufer hält das Journal bis zum Ende der Simulation exklusiv.
    pub fn pruefe_simulationslauf(
        &self,
        datum: Datum,
        zyklus: u32,
        aufrufe_je_berater: u32,
        auswahl: [Modellwahl; 2],
    ) -> Result<(), (BeraterId, Sperre)> {
        let mut vorschau = self.clone();
        for wahl in auswahl {
            for _ in 0..aufrufe_je_berater {
                let zweck = KiZweck::Simulation;
                let (buchung, _) = naechste_fuer(&vorschau, datum, zyklus, zweck, wahl)
                    .map_err(|grund| (wahl.modell().berater(), grund))?;
                vorschau.buchungen.push(buchung);
                vorschau.zwecke.push(zweck);
            }
        }
        Ok(())
    }

    pub(crate) fn naechste_mit_einstellung(
        &self,
        berater: BeraterId,
        datum: Datum,
        zyklus: u32,
        zweck: KiZweck,
        einstellung: BeraterEinstellung,
    ) -> Result<(Buchung, KiDeckel), Sperre> {
        self.pruefe_tageslimit(berater, datum, zweck, einstellung)?;
        let (buchung, deckel) = naechste_fuer(self, datum, zyklus, zweck, einstellung.auswahl)?;
        if zweck == KiZweck::Vorbereitung {
            let rahmen = depot_ki::vorbereitungsrahmen_fuer_modell(
                &KiGrenzen::standard(),
                &self.buchungen,
                einstellung.auswahl.modell(),
                datum,
            )?;
            if buchung.reserviert > rahmen {
                return Err(Sperre::TagesrahmenErschoepft);
            }
        }
        Ok((buchung, deckel))
    }

    pub(crate) fn belege_neuroergebnis(&mut self, inhalt: &str) -> Result<(), String> {
        let e = NeuroErgebnis::lies(inhalt).ok_or("NeuroSchlopf-Ergebnis unlesbar")?;
        if !self
            .neurofragen
            .iter()
            .any(|v| v.buchung == e.buchung && v.id == e.id && v.wer == e.wer)
        {
            return Err("NeuroSchlopf-Ergebnis ohne passende Buchung".into());
        }
        if self.neuroergebnisse.contains_key(&e.buchung) {
            return Err("NeuroSchlopf-Ergebnis bereits belegt".into());
        }
        let mut neu = self.clone();
        if let Some(a) = &e.aufruf {
            neu.belege_aufruf(&a.text())?;
        }
        if let Some(a) = &e.abrechnung {
            neu.rechne_ab(&a.text())?;
        }
        neu.neuroergebnisse.insert(e.buchung, e);
        *self = neu;
        Ok(())
    }

    pub(crate) fn belege_aufruf(&mut self, inhalt: &str) -> Result<(), String> {
        let a = crate::KiAufruf::lies(inhalt).ok_or("KI-Aufruf unlesbar")?;
        if self.buchungen.get(a.nr).is_none() {
            return Err(format!("KI-Aufruf {} ohne Buchung", a.nr));
        }
        if self.aufrufe.contains_key(&a.nr) {
            return Err(format!("KI-Aufruf {} doppelt", a.nr));
        }
        self.aufrufe.insert(a.nr, a);
        Ok(())
    }

    pub(crate) fn rechne_ab(&mut self, inhalt: &str) -> Result<(), String> {
        let a = crate::ki_abrechnung::KiAbrechnung::lies(inhalt).ok_or("KI-Abrechnung unlesbar")?;
        if self.abgerechnet.contains(&a.nr) || self.obergrenzen.get(a.nr) != Some(&a.reserviert) {
            return Err("KI-Abrechnung ohne passende offene Reservierung".into());
        }
        let b = self
            .buchungen
            .get_mut(a.nr)
            .ok_or("KI-Reservierung fehlt")?;
        b.reserviert = a.ist;
        self.abgerechnet.insert(a.nr);
        Ok(())
    }

    /// Steht der heutige Abdruck als letzter im Journal?
    pub(crate) fn abdruck_aktuell(&self, abdruck: &str) -> bool {
        self.abdruck.as_deref() == Some(abdruck)
    }

    pub(crate) fn abdruck(&mut self, inhalt: &str) -> Result<(), String> {
        if !ist_abdruck(inhalt) {
            return Err("Grenzabdruck unlesbar".into());
        }
        self.abdruck = Some(inhalt.to_string());
        Ok(())
    }

    /// Prueft eine Buchung nach den Regeln ihres Abschnitts und zaehlt sie.
    pub(crate) fn buche(&mut self, inhalt: &str) -> Result<KiZweck, String> {
        if let Some((text, ursprung)) = inhalt.rsplit_once(";wiederholung;") {
            return self.buche_wiederholung(text, ursprung);
        }
        if let Some((buchung, zusatz)) = inhalt.rsplit_once(";neurofrage;") {
            let (fall, rest) = zusatz.split_once(';').ok_or("NeuroSchlopf-Fall fehlt")?;
            let (runde, abdruck) = rest.split_once(';').ok_or("NeuroSchlopf-Runde fehlt")?;
            let runde = NeuroRunde::lies(runde).ok_or("NeuroSchlopf-Runde unbekannt")?;
            let id = NeuroAuftragId::neu(fall, runde).ok_or("NeuroSchlopf-Fall unlesbar")?;
            let (b, auswahl) =
                lies_buchung_mit_modell(buchung).ok_or("NeuroSchlopf-Buchung unlesbar")?;
            if auswahl.is_none() {
                return Err("NeuroSchlopf-Buchung ohne Modellwahl".into());
            }
            self.pruefe_neuroauftrag(&id, b.berater)
                .map_err(|e| format!("NeuroSchlopf-Auftrag: {e:?}"))?;
            let mut neu = self.clone();
            neu.abdruck(abdruck)?;
            neu.buche_fuer(buchung, KiZweck::Neurofrage)?;
            neu.neurofragen.push(NeuroVersuch {
                id,
                wer: b.berater,
                datum: b.datum,
                buchung: self.buchungen.len(),
            });
            *self = neu;
            return Ok(KiZweck::Neurofrage);
        }
        // Die Simulation hat keinen Handelsbeginn. Ihr Grenzabdruck und ihre Buchung
        // gehören in denselben Eintrag, auch bei der ersten Buchung überhaupt.
        for (marke, zweck) in [
            (";simulation;", KiZweck::Simulation),
            (";wochenrueckschau;", KiZweck::Wochenrueckschau),
        ] {
            if let Some((buchung, abdruck)) = inhalt.rsplit_once(marke) {
                let mut neu = self.clone();
                neu.abdruck(abdruck)?;
                neu.buche_fuer(buchung, zweck)?;
                *self = neu;
                return Ok(zweck);
            }
        }
        let (text, zweck) = inhalt
            .rsplit_once(';')
            .and_then(|(t, z)| KiZweck::lies(z).map(|z| (t, z)))
            .unwrap_or((inhalt, KiZweck::Erstmeinung));
        if zweck.ohne_handelsbeginn() {
            return Err("Simulationsbuchung ohne Grenzabdruck".into());
        }
        let (b, auswahl) = lies_buchung_mit_modell(text).ok_or("Buchung unlesbar")?;
        if buchung_fuer(&b, zweck, auswahl) != inhalt {
            return Err("Buchung nicht in kanonischer Form".into());
        }
        self.buche_fuer(text, zweck)?;
        Ok(zweck)
    }

    pub(crate) fn buche_fuer(&mut self, inhalt: &str, zweck: KiZweck) -> Result<(), String> {
        let (b, auswahl) = lies_buchung_mit_modell(inhalt).ok_or("Buchung unlesbar")?;
        let Some(abschnitt) = &self.abdruck else {
            return Err("Buchung ohne Grenzabdruck".into());
        };
        if *abschnitt == grenzabdruck_fuer_zweck(auswahl, zweck)
            || *abschnitt == grenzabdruck_fuer_auswahl(auswahl)
        {
            let wahl = auswahl.unwrap_or_else(|| Modellwahl::standard(b.berater));
            let erwartet = naechste_fuer(self, b.datum, b.zyklus, zweck, wahl)
                .map(|(b, _)| b.reserviert)
                .map_err(|s| format!("Historie verletzt die Grenzen: {s:?}"))?;
            if erwartet != b.reserviert {
                return Err(format!(
                    "Reservierung {} statt {} Cent",
                    b.reserviert.0, erwartet.0
                ));
            }
        }
        // Frueherer Abdruck: zaehlt, wird aber nicht nach heutigen Grenzen beurteilt.
        self.speichere_buchung(b, zweck, auswahl);
        Ok(())
    }

    fn speichere_buchung(&mut self, b: Buchung, zweck: KiZweck, auswahl: Option<Modellwahl>) {
        self.obergrenzen.push(b.reserviert);
        self.zwecke.push(zweck);
        self.modellwahlen.push(auswahl);
        self.buchungen.push(b);
    }
}

/// Die Buchung, die `reserviere` schreiben wuerde, und ihr Deckel.
pub(crate) fn naechste_fuer(
    stand: &KiStand,
    datum: Datum,
    zyklus: u32,
    zweck: KiZweck,
    auswahl: Modellwahl,
) -> Result<(Buchung, KiDeckel), Sperre> {
    let modell = auswahl.modell();
    let berater = modell.berater();
    if matches!(zweck, KiZweck::Zweitmeinung | KiZweck::Abstimmung)
        && !stand
            .buchungen
            .iter()
            .zip(&stand.zwecke)
            .any(|(b, z)| b.berater == berater && b.datum == datum && *z == KiZweck::Erstmeinung)
    {
        return Err(depot_ki::Sperre::ErstmeinungReserviert);
    }
    buche_mit_deckel(stand, datum, zyklus, zweck, auswahl, zweck.deckel(modell))
}

fn buche_mit_deckel(
    stand: &KiStand,
    datum: Datum,
    zyklus: u32,
    zweck: KiZweck,
    auswahl: Modellwahl,
    deckel: KiDeckel,
) -> Result<(Buchung, KiDeckel), Sperre> {
    let modell = auswahl.modell();
    let berater = modell.berater();
    let pruefe_budget = if zweck == KiZweck::Simulation {
        pruefe_simulation
    } else {
        pruefe
    };
    let reserviert = pruefe_budget(
        &KiGrenzen::standard(),
        &deckel,
        &stand.buchungen,
        berater,
        datum,
        zyklus,
    )?;
    if zweck != KiZweck::Erstmeinung {
        pruefe_zusatzreserve(stand, modell, datum, reserviert, zweck)?;
    }
    Ok((
        Buchung {
            berater,
            datum,
            zyklus,
            reserviert,
        },
        deckel,
    ))
}

/// Zusatzfragen lassen Aufrufe und Höchstkosten aller übrigen Erstmeinungen frei.
fn pruefe_zusatzreserve(
    stand: &KiStand,
    modell: Modell,
    datum: Datum,
    reserviert: Cent,
    zweck: KiZweck,
) -> Result<(), depot_ki::Sperre> {
    let berater = modell.berater();
    let g = KiGrenzen::standard();
    let gebucht = stand
        .buchungen
        .iter()
        .filter(|b| {
            b.berater == berater
                && b.datum.jahr() == datum.jahr()
                && b.datum.monat() == datum.monat()
        })
        .try_fold(Cent::NULL, |s, b| s.checked_add(b.reserviert))
        .ok_or(depot_ki::Sperre::Rechenfehler)?;
    let ist_handelstag = depot_kern::liste::handelstag(datum);
    let vorab = matches!(zweck, KiZweck::Simulation | KiZweck::Neurofrage);
    let tage =
        depot_ki::handelstage_rest(datum).saturating_sub(u32::from(ist_handelstag && !vorab));
    if vorab && ist_handelstag {
        // Zwei Fachfragen, zwei Abstimmungsrunden und die Nachfrage zum Mindesteinsatz.
        let handelsaufrufe = 5;
        let heute = stand
            .buchungen
            .iter()
            .filter(|b| b.berater == berater && b.datum == datum)
            .count();
        let heute = u32::try_from(heute).map_err(|_| depot_ki::Sperre::Rechenfehler)?;
        if heute.saturating_add(1) > g.aufrufe_je_zyklus().saturating_sub(handelsaufrufe) {
            return Err(depot_ki::Sperre::ErstmeinungReserviert);
        }
    }
    let monat_aufrufe = stand
        .buchungen
        .iter()
        .filter(|b| {
            b.berater == berater
                && b.datum.jahr() == datum.jahr()
                && b.datum.monat() == datum.monat()
        })
        .count();
    let nachher = monat_aufrufe
        .checked_add(1)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or(depot_ki::Sperre::Rechenfehler)?;
    if nachher > g.aufrufe_je_monat().saturating_sub(tage) {
        return Err(depot_ki::Sperre::ErstmeinungReserviert);
    }
    let morgen = KiDeckel::fuer_modell(modell)
        .hoechstkosten()
        .and_then(|c| c.mal_stueck(u64::from(tage.max(1))))
        .ok_or(depot_ki::Sperre::Rechenfehler)?;
    let frei = g
        .kosten_je_monat()
        .checked_sub(gebucht)
        .and_then(|r| r.checked_sub(reserviert))
        .ok_or(depot_ki::Sperre::Rechenfehler)?;
    let jahr = stand
        .buchungen
        .iter()
        .filter(|b| b.berater == berater && b.datum.jahr() == datum.jahr())
        .try_fold(Cent::NULL, |s, b| s.checked_add(b.reserviert))
        .ok_or(depot_ki::Sperre::Rechenfehler)?;
    let jahr_frei = g
        .kosten_je_jahr()
        .checked_sub(jahr)
        .and_then(|r| r.checked_sub(reserviert))
        .ok_or(depot_ki::Sperre::Rechenfehler)?;
    if frei < morgen || jahr_frei < morgen {
        return Err(depot_ki::Sperre::ErstmeinungReserviert);
    }
    Ok(())
}
