//! Kassenbuch der KI-Aufrufe. Es steht seit der Persistenzbasis im Journal: jede Buchung ein
//! Eintrag der Art `Ki`. Die Buchung steht synchronisiert auf der Platte, bevor die Erlaubnis
//! herausgegeben wird. Ein Absturz nach dem Buchen kostet also hoechstens einen ungenutzten
//! Aufruf, nie einen ungezaehlten.
//!
//! **Speicher (R24, R25, R29, R38):** Einrichtung, Sperre, Link-Verbot, Anhangsvermerk,
//! Wiederherstellung und Sperre nach Fehlern leistet das Journal (`depot_journal`). Das
//! Kassenbuch leiht sich das geoeffnete Journal; solange es lebt, schreibt niemand sonst.
//!
//! **Ganze Historie (R20, R25):** Beim Oeffnen wird jede Buchung mit denselben Grenzen
//! nachgerechnet, mit denen sie gebucht wurde. Schreiber und Leser akzeptieren denselben
//! Zustandsraum.
//!
//! **Uebergang bei geaenderten Grenzen:** Vor der ersten Buchung unter neuen Grenzen steht
//! deren Abdruck im Journal (`KiGrenzen`). Buchungen unter einem frueheren Abdruck werden nicht
//! nach den heutigen Grenzen beurteilt, zaehlen aber weiter mit: Ein voller Monat bleibt voll.
//! Die Grenzen selbst aendert nur eine Grenzfreigabe (R30).

use std::fmt::Write as _;

use depot_journal::{Art, Journal, JournalFehler};
use depot_kern::BeraterId;
use sha2::{Digest, Sha256};

use crate::budget::{AufrufErlaubnis, Buchung, Datum, KiDeckel, KiGrenzen, Sperre, pruefe};

fn halt(text: impl Into<String>) -> Sperre {
    Sperre::Kassenbuch(text.into())
}

fn vom_journal(e: JournalFehler) -> Sperre {
    match e {
        JournalFehler::Belegt => halt("Journal belegt"),
        JournalFehler::Halt(m) => halt(format!("Journal: {m}")),
        JournalFehler::Abgelehnt(m) => halt(format!("Journal lehnt ab: {m}")),
    }
}

/// Der Abdruck der festgeschriebenen KI-Grenzen und beider Deckel: SHA-256 ihrer Werte.
pub fn grenzabdruck() -> String {
    let text = format!(
        "{:?}|{:?}|{:?}",
        KiGrenzen::standard(),
        KiDeckel::standard(BeraterId::Dachs),
        KiDeckel::standard(BeraterId::Maus)
    );
    Sha256::digest(text.as_bytes())
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn berater_text(b: BeraterId) -> &'static str {
    match b {
        BeraterId::Dachs => "Dachs",
        BeraterId::Maus => "Maus",
    }
}

fn eintrag(b: &Buchung) -> String {
    format!(
        "{};{:04}-{:02}-{:02};{};{}",
        berater_text(b.berater),
        b.datum.jahr(),
        b.datum.monat(),
        b.datum.tag(),
        b.zyklus,
        b.reserviert.0
    )
}

fn lies_datum(text: &str) -> Option<Datum> {
    let mut teile = text.split('-');
    let jahr = teile.next()?.parse().ok()?;
    let monat = teile.next()?.parse().ok()?;
    let tag = teile.next()?.parse().ok()?;
    if teile.next().is_some() {
        return None;
    }
    Datum::neu(jahr, monat, tag)
}

/// Liest eine Buchung; nur die kanonische Form gilt.
fn lies_eintrag(text: &str) -> Option<Buchung> {
    let teile: Vec<&str> = text.split(';').collect();
    let [berater, datum, zyklus, cent] = teile.as_slice() else {
        return None;
    };
    let berater = match *berater {
        "Dachs" => BeraterId::Dachs,
        "Maus" => BeraterId::Maus,
        _ => return None,
    };
    let reserviert = depot_kern::Cent(cent.parse().ok()?);
    if reserviert.0 < 0 {
        return None;
    }
    let b = Buchung {
        berater,
        datum: lies_datum(datum)?,
        zyklus: zyklus.parse().ok()?,
        reserviert,
    };
    (eintrag(&b) == text).then_some(b)
}

fn ist_abdruck(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Debug)]
pub struct Kassenbuch<'j> {
    journal: &'j mut Journal,
    grenzen: KiGrenzen,
    abdruck: String,
    /// Steht der heutige Abdruck als letzter im Journal?
    abdruck_aktuell: bool,
    buchungen: Vec<Buchung>,
}

impl<'j> Kassenbuch<'j> {
    /// Liest alle Buchungen aus dem Journal und prueft die Historie. Grenzen und Deckel werden
    /// dabei fest gebunden (R19). Jede Unstimmigkeit sperrt alles.
    ///
    /// # Errors
    /// `Kassenbuch` bei unlesbarer Buchung, Buchung ohne Grenzabdruck oder unstimmiger
    /// Historie.
    pub fn oeffnen(journal: &'j mut Journal) -> Result<Kassenbuch<'j>, Sperre> {
        let grenzen = KiGrenzen::standard();
        let abdruck = grenzabdruck();
        let mut abschnitt: Option<&str> = None;
        let mut buchungen: Vec<Buchung> = Vec::new();
        for (i, (art, text)) in journal.eintraege().iter().enumerate() {
            let nr = i.saturating_add(1);
            match art {
                Art::KiGrenzen => {
                    if !ist_abdruck(text) {
                        return Err(halt(format!("Eintrag {nr}: Grenzabdruck unlesbar")));
                    }
                    abschnitt = Some(text);
                }
                Art::Ki => {
                    let b = lies_eintrag(text)
                        .ok_or_else(|| halt(format!("Eintrag {nr}: Buchung unlesbar")))?;
                    match abschnitt {
                        None => {
                            return Err(halt(format!("Eintrag {nr}: Buchung ohne Grenzabdruck")));
                        }
                        Some(a) if a == abdruck => {
                            let erwartet = pruefe(
                                &grenzen,
                                &KiDeckel::standard(b.berater),
                                &buchungen,
                                b.berater,
                                b.datum,
                                b.zyklus,
                            )
                            .map_err(|s| {
                                halt(format!(
                                    "Eintrag {nr}: Historie verletzt die Grenzen: {s:?}"
                                ))
                            })?;
                            if erwartet != b.reserviert {
                                return Err(halt(format!(
                                    "Eintrag {nr}: Reservierung {} statt {} Cent",
                                    b.reserviert.0, erwartet.0
                                )));
                            }
                        }
                        // Frueherer Abdruck: zaehlt, wird aber nicht nach heutigen Grenzen
                        // beurteilt.
                        Some(_) => {}
                    }
                    buchungen.push(b);
                }
                Art::Beobachtung | Art::Topf | Art::Auftrag => {}
            }
        }
        let abdruck_aktuell = abschnitt == Some(abdruck.as_str());
        Ok(Kassenbuch {
            journal,
            grenzen,
            abdruck,
            abdruck_aktuell,
            buchungen,
        })
    }

    /// Prueft die Grenzen, bucht die Hoechstkosten dauerhaft und gibt erst dann die Erlaubnis.
    ///
    /// # Errors
    /// Jede verletzte Grenze und jeder Schreibfehler ergibt eine `Sperre`; dann wird nicht
    /// gefragt. Nach einem Schreibfehler bleibt das Journal gesperrt.
    pub fn reserviere(
        &mut self,
        berater: BeraterId,
        datum: Datum,
        zyklus: u32,
    ) -> Result<AufrufErlaubnis, Sperre> {
        let deckel = KiDeckel::standard(berater);
        let reserviert = pruefe(
            &self.grenzen,
            &deckel,
            &self.buchungen,
            berater,
            datum,
            zyklus,
        )?;
        if !self.abdruck_aktuell {
            self.journal
                .anhaengen(Art::KiGrenzen, &self.abdruck)
                .map_err(vom_journal)?;
            self.abdruck_aktuell = true;
        }
        let buchung = Buchung {
            berater,
            datum,
            zyklus,
            reserviert,
        };
        self.journal
            .anhaengen(Art::Ki, &eintrag(&buchung))
            .map_err(vom_journal)?;
        self.buchungen.push(buchung.clone());
        Ok(AufrufErlaubnis::neu(buchung, deckel))
    }

    pub fn buchungen(&self) -> &[Buchung] {
        &self.buchungen
    }
}
