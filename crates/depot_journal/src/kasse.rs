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
use depot_ki::{Buchung, Datum, KiDeckel, KiGrenzen, Sperre, grenzabdruck, pruefe};

use crate::stand::{datum_text, lies_datum};

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
    _siegel: siegel::Siegel,
}

impl AufrufErlaubnis {
    pub(crate) fn neu(
        buchung: Buchung,
        deckel: KiDeckel,
        auswahl: Modellwahl,
        nr: usize,
    ) -> AufrufErlaubnis {
        AufrufErlaubnis {
            buchung,
            nr,
            deckel,
            auswahl,
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

fn ist_abdruck(text: &str) -> bool {
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
        ]
        .into_iter()
        .find(|z| z.text() == s)
    }
    pub(crate) fn ohne_handelsbeginn(self) -> bool {
        matches!(self, Self::Simulation | Self::Wochenrueckschau)
    }
    pub(crate) fn deckel(self, modell: Modell) -> KiDeckel {
        match self {
            Self::Abstimmung | Self::Vorbereitung | Self::Wochenrueckschau => {
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
}

impl KiStand {
    pub fn buchungen(&self) -> &[Buchung] {
        &self.buchungen
    }

    /// Belegter Zweck und Abrechnungsstatus zu einer Buchungsnummer.
    pub fn buchungsbeleg(&self, nr: usize) -> Option<(KiZweck, bool)> {
        Some((*self.zwecke.get(nr)?, self.abgerechnet.contains(&nr)))
    }

    pub fn aufruf(&self, nr: usize) -> Option<&crate::KiAufruf> {
        self.aufrufe.get(&nr)
    }

    pub fn modellwahl(&self, nr: usize) -> Option<Modellwahl> {
        self.modellwahlen.get(nr).copied().flatten()
    }

    fn pruefe_tageslimit(
        &self,
        berater: BeraterId,
        datum: Datum,
        einstellung: BeraterEinstellung,
    ) -> Result<(), Sperre> {
        if einstellung.auswahl.modell().berater() != berater {
            return Err(Sperre::Rechenfehler);
        }
        if einstellung.max_aufrufe >= depot_kern::modelle::MAX_BERATER_AUFRUFE {
            return Ok(());
        }
        let heute = self
            .buchungen
            .iter()
            .filter(|b| b.berater == berater && b.datum == datum)
            .count();
        if heute >= einstellung.max_aufrufe {
            return Err(Sperre::TagVoll);
        }
        Ok(())
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

    pub(crate) fn naechste_mit_einstellung(
        &self,
        berater: BeraterId,
        datum: Datum,
        zyklus: u32,
        zweck: KiZweck,
        einstellung: BeraterEinstellung,
    ) -> Result<(Buchung, KiDeckel), Sperre> {
        self.pruefe_tageslimit(berater, datum, einstellung)?;
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
        if *abschnitt == grenzabdruck_fuer_auswahl(auswahl) {
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
        self.obergrenzen.push(b.reserviert);
        self.zwecke.push(zweck);
        self.modellwahlen.push(auswahl);
        self.buchungen.push(b);
        Ok(())
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
    let deckel = zweck.deckel(modell);
    let reserviert = pruefe(
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
    let simulation = zweck == KiZweck::Simulation;
    let tage =
        depot_ki::handelstage_rest(datum).saturating_sub(u32::from(ist_handelstag && !simulation));
    if simulation && ist_handelstag {
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
