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
    deckel: KiDeckel,
    _siegel: siegel::Siegel,
}

impl AufrufErlaubnis {
    pub(crate) fn neu(buchung: Buchung, deckel: KiDeckel) -> AufrufErlaubnis {
        AufrufErlaubnis {
            buchung,
            deckel,
            _siegel: siegel::Siegel::neu(),
        }
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

/// Das Kassenbuch, wie es aus dem Journal folgt.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KiStand {
    /// Der zuletzt eingetragene Grenzabdruck.
    abdruck: Option<String>,
    buchungen: Vec<Buchung>,
}

impl KiStand {
    pub fn buchungen(&self) -> &[Buchung] {
        &self.buchungen
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
        let b = lies_buchung(inhalt).ok_or("Buchung unlesbar")?;
        if buchung_text(&b) != inhalt {
            return Err(format!("Buchung nicht in kanonischer Form: `{inhalt}`"));
        }
        let Some(abschnitt) = &self.abdruck else {
            return Err("Buchung ohne Grenzabdruck".into());
        };
        if *abschnitt == grenzabdruck() {
            let erwartet = pruefe(
                &KiGrenzen::standard(),
                &KiDeckel::standard(b.berater),
                &self.buchungen,
                b.berater,
                b.datum,
                b.zyklus,
            )
            .map_err(|s| format!("Historie verletzt die Grenzen: {s:?}"))?;
            if erwartet != b.reserviert {
                return Err(format!(
                    "Reservierung {} statt {} Cent",
                    b.reserviert.0, erwartet.0
                ));
            }
        }
        // Frueherer Abdruck: zaehlt, wird aber nicht nach heutigen Grenzen beurteilt.
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
    let deckel = KiDeckel::standard(berater);
    let reserviert = pruefe(
        &KiGrenzen::standard(),
        &deckel,
        &stand.buchungen,
        berater,
        datum,
        zyklus,
    )?;
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
