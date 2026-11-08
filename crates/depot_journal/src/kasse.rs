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

use depot_kern::{BeraterId, Cent};
use depot_ki::{Buchung, Datum, KiDeckel, KiGrenzen, grenzabdruck, pruefe};

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
    _siegel: siegel::Siegel,
}

impl AufrufErlaubnis {
    pub(crate) fn neu(buchung: Buchung, deckel: KiDeckel, nr: usize) -> AufrufErlaubnis {
        AufrufErlaubnis {
            buchung,
            nr,
            deckel,
            _siegel: siegel::Siegel::neu(),
        }
    }

    /// Nur eine plausible Anbieterabrechnung kann eine Reservierung vermindern.
    pub fn abrechnung(&self, token_ein: u64, token_aus: u64) -> Option<crate::KiAbrechnung> {
        if token_ein > u64::from(self.deckel.max_token_ein())
            || token_aus > u64::from(self.deckel.max_token_aus())
        {
            return None;
        }
        let ist = self.deckel.kosten_ist(token_ein, token_aus)?;
        (ist <= self.buchung.reserviert).then_some(crate::KiAbrechnung {
            nr: self.nr,
            reserviert: self.buchung.reserviert,
            ist,
        })
    }

    pub fn buchung(&self) -> &Buchung {
        &self.buchung
    }

    pub fn deckel(&self) -> &KiDeckel {
        &self.deckel
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

pub(crate) fn buchung_text(b: &Buchung) -> String {
    format!(
        "{};{};{};{}",
        berater_text(b.berater),
        datum_text(b.datum),
        b.zyklus,
        b.reserviert.0
    )
}

pub(crate) fn lies_buchung(text: &str) -> Option<Buchung> {
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
}
impl KiZweck {
    pub fn text(self) -> &'static str {
        match self {
            Self::Erstmeinung => "erstmeinung",
            Self::Zweitmeinung => "zweitmeinung",
            Self::Abstimmung => "abstimmung",
            Self::Vorbereitung => "vorbereitung",
        }
    }
    fn lies(s: &str) -> Option<Self> {
        [
            Self::Erstmeinung,
            Self::Zweitmeinung,
            Self::Abstimmung,
            Self::Vorbereitung,
        ]
        .into_iter()
        .find(|z| z.text() == s)
    }
    pub(crate) fn deckel(self, wer: BeraterId) -> KiDeckel {
        match self {
            Self::Abstimmung | Self::Vorbereitung => KiDeckel::kurz(wer),
            _ => KiDeckel::standard(wer),
        }
    }
}
pub(crate) fn buchung_fuer(b: &Buchung, zweck: KiZweck) -> String {
    match zweck {
        KiZweck::Erstmeinung => buchung_text(b),
        _ => format!("{};{}", buchung_text(b), zweck.text()),
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
    zwecke: Vec<KiZweck>,
}

impl KiStand {
    pub fn buchungen(&self) -> &[Buchung] {
        &self.buchungen
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
    pub(crate) fn abdruck_aktuell(&self) -> bool {
        self.abdruck.as_deref() == Some(grenzabdruck().as_str())
    }

    pub(crate) fn abdruck(&mut self, inhalt: &str) -> Result<(), String> {
        if !ist_abdruck(inhalt) {
            return Err("Grenzabdruck unlesbar".into());
        }
        self.abdruck = Some(inhalt.to_string());
        Ok(())
    }

    /// Prueft eine Buchung nach den Regeln ihres Abschnitts und zaehlt sie.
    pub(crate) fn buche(&mut self, inhalt: &str) -> Result<(), String> {
        let (text, zweck) = inhalt
            .rsplit_once(';')
            .and_then(|(t, z)| KiZweck::lies(z).map(|z| (t, z)))
            .unwrap_or((inhalt, KiZweck::Erstmeinung));
        let b = lies_buchung(text).ok_or("Buchung unlesbar")?;
        if buchung_fuer(&b, zweck) != inhalt {
            return Err("Buchung nicht in kanonischer Form".into());
        }
        self.buche_fuer(text, zweck)
    }

    pub(crate) fn buche_fuer(&mut self, inhalt: &str, zweck: KiZweck) -> Result<(), String> {
        let b = lies_buchung(inhalt).ok_or("Buchung unlesbar")?;
        if buchung_text(&b) != inhalt {
            return Err("Buchung nicht in kanonischer Form".into());
        }
        let Some(abschnitt) = &self.abdruck else {
            return Err("Buchung ohne Grenzabdruck".into());
        };
        if *abschnitt == grenzabdruck() {
            let erwartet = naechste_fuer(self, b.berater, b.datum, b.zyklus, zweck)
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
        self.buchungen.push(b);
        Ok(())
    }
}

/// Die Buchung, die `reserviere` schreiben wuerde, und ihr Deckel.
pub(crate) fn naechste(
    stand: &KiStand,
    berater: BeraterId,
    datum: Datum,
    zyklus: u32,
) -> Result<(Buchung, KiDeckel), depot_ki::Sperre> {
    naechste_fuer(stand, berater, datum, zyklus, KiZweck::Erstmeinung)
}

pub(crate) fn naechste_fuer(
    stand: &KiStand,
    berater: BeraterId,
    datum: Datum,
    zyklus: u32,
    zweck: KiZweck,
) -> Result<(Buchung, KiDeckel), depot_ki::Sperre> {
    if matches!(zweck, KiZweck::Zweitmeinung | KiZweck::Abstimmung)
        && !stand
            .buchungen
            .iter()
            .zip(&stand.zwecke)
            .any(|(b, z)| b.berater == berater && b.datum == datum && *z == KiZweck::Erstmeinung)
    {
        return Err(depot_ki::Sperre::ErstmeinungReserviert);
    }
    let deckel = zweck.deckel(berater);
    let reserviert = pruefe(
        &KiGrenzen::standard(),
        &deckel,
        &stand.buchungen,
        berater,
        datum,
        zyklus,
    )?;
    if zweck != KiZweck::Erstmeinung {
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
        let morgen = KiDeckel::standard(berater)
            .hoechstkosten()
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
