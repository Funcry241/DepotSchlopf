//! Dauerhaftes Kassenbuch der KI-Aufrufe: eine Textdatei, nur angehaengt, jede Zeile eine Buchung.
//! Die Buchung steht auf der Platte (`sync_all`), bevor die Erlaubnis herausgegeben wird.
//! Ein Absturz nach dem Buchen kostet also hoechstens einen ungenutzten Aufruf, nie einen
//! ungezaehlten.

use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use depot_kern::{BeraterId, Cent};

use crate::budget::{AufrufErlaubnis, Buchung, Datum, KiDeckel, KiGrenzen, Sperre, pruefe};

const KOPF: &str = "v1";

/// Exklusive Sperrdatei neben dem Kassenbuch. Bleibt sie nach einem Absturz liegen,
/// bleibt die KI gesperrt, bis ein Mensch sie loescht (fail-closed).
#[derive(Debug)]
struct Sperrdatei {
    pfad: PathBuf,
}

impl Sperrdatei {
    fn nehmen(pfad: PathBuf) -> Result<Sperrdatei, Sperre> {
        match OpenOptions::new().write(true).create_new(true).open(&pfad) {
            Ok(_) => Ok(Sperrdatei { pfad }),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => Err(Sperre::KassenbuchBelegt),
            Err(e) => Err(Sperre::Kassenbuch(format!("Sperrdatei: {e}"))),
        }
    }
}

impl Drop for Sperrdatei {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.pfad);
    }
}

#[derive(Debug)]
pub struct Kassenbuch {
    pfad: PathBuf,
    buchungen: Vec<Buchung>,
    _sperre: Sperrdatei,
}

fn berater_text(b: BeraterId) -> &'static str {
    match b {
        BeraterId::Dachs => "Dachs",
        BeraterId::Maus => "Maus",
    }
}

fn zeile(b: &Buchung) -> String {
    format!(
        "{KOPF};{};{:04}-{:02}-{:02};{};{}\n",
        berater_text(b.berater),
        b.datum.jahr,
        b.datum.monat,
        b.datum.tag,
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

fn lies_zeile(text: &str) -> Option<Buchung> {
    let teile: Vec<&str> = text.split(';').collect();
    let [kopf, berater, datum, zyklus, cent] = teile.as_slice() else {
        return None;
    };
    if *kopf != KOPF {
        return None;
    }
    let berater = match *berater {
        "Dachs" => BeraterId::Dachs,
        "Maus" => BeraterId::Maus,
        _ => return None,
    };
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

impl Kassenbuch {
    /// Oeffnet exklusiv und liest alle Buchungen. Eine einzige unlesbare Zeile sperrt alles.
    ///
    /// # Errors
    /// `KassenbuchBelegt`, wenn ein anderer Lauf es offen hat; `Kassenbuch` bei Lesefehlern.
    pub fn oeffnen(pfad: &Path) -> Result<Kassenbuch, Sperre> {
        let sperre = Sperrdatei::nehmen(pfad.with_extension("sperre"))?;
        let inhalt = match fs::read_to_string(pfad) {
            Ok(t) => t,
            Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
            Err(e) => return Err(Sperre::Kassenbuch(format!("lesen: {e}"))),
        };
        let mut buchungen = Vec::new();
        for (nr, text) in inhalt.lines().enumerate() {
            let b = lies_zeile(text).ok_or_else(|| {
                Sperre::Kassenbuch(format!("Zeile {} unlesbar", nr.saturating_add(1)))
            })?;
            buchungen.push(b);
        }
        if !inhalt.is_empty() && !inhalt.ends_with('\n') {
            return Err(Sperre::Kassenbuch("letzte Zeile unvollstaendig".into()));
        }
        Ok(Kassenbuch {
            pfad: pfad.to_path_buf(),
            buchungen,
            _sperre: sperre,
        })
    }

    /// Prueft die Grenzen, bucht die Hoechstkosten dauerhaft und gibt erst dann die Erlaubnis.
    ///
    /// # Errors
    /// Jede verletzte Grenze und jeder Schreibfehler ergibt eine `Sperre`; dann wird nicht gefragt.
    pub fn reserviere(
        &mut self,
        grenzen: &KiGrenzen,
        deckel: &KiDeckel,
        berater: BeraterId,
        datum: Datum,
        zyklus: u32,
    ) -> Result<AufrufErlaubnis, Sperre> {
        let reserviert = pruefe(grenzen, deckel, &self.buchungen, berater, datum, zyklus)?;
        let buchung = Buchung {
            berater,
            datum,
            zyklus,
            reserviert,
        };
        let mut datei: File = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.pfad)
            .map_err(|e| Sperre::Kassenbuch(format!("oeffnen: {e}")))?;
        datei
            .write_all(zeile(&buchung).as_bytes())
            .and_then(|()| datei.sync_all())
            .map_err(|e| Sperre::Kassenbuch(format!("schreiben: {e}")))?;
        self.buchungen.push(buchung.clone());
        Ok(AufrufErlaubnis::neu(buchung, *deckel))
    }

    pub fn buchungen(&self) -> &[Buchung] {
        &self.buchungen
    }
}
