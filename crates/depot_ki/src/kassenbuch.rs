//! Dauerhaftes Kassenbuch der KI-Aufrufe: eine Textdatei, nur angehaengt, jede Zeile eine
//! Buchung. Die Buchung steht auf der Platte (`sync_all`), bevor die Erlaubnis herausgegeben
//! wird. Ein Absturz nach dem Buchen kostet also hoechstens einen ungenutzten Aufruf, nie einen
//! ungezaehlten.
//!
//! **Einrichtung und Anker (R25, R38):** Das Kassenbuch entsteht nur durch `einrichten`, nie
//! nebenbei beim Oeffnen. Neben ihm liegt ein Anker mit dem bestaetigten Stand (Zeilenzahl und
//! SHA-256 dieser Zeilen). Er rueckt erst vor, wenn die neue Zeile synchronisiert ist. Fehlt
//! eine der beiden Dateien, ist das Kassenbuch kuerzer als der Anker oder weicht es in den
//! bestaetigten Zeilen ab, gilt Halt. Hoechstens eine vollstaendige Zeile darf ueber den
//! Anker hinausgehen: Sie ist gebucht, aber der Anker wurde nicht mehr geschrieben. Sie wird
//! beim Oeffnen geprueft und bestaetigt; sie zaehlt also, auch wenn ihre Erlaubnis nie
//! herausging. Dieser Anker liegt bis zur Persistenzbasis neben dem Kassenbuch; danach steht
//! er im Journal.
//!
//! **Ganze Historie (R20, R25):** Beim Oeffnen wird jede Zeile mit denselben Grenzen
//! nachgerechnet, mit denen sie gebucht wurde. Schreiber und Leser akzeptieren denselben
//! Zustandsraum.
//!
//! **Sperre (R24):** Gesperrt wird die geoeffnete Datei selbst ueber das Betriebssystem, nicht
//! ein abgeleiteter Pfadname. Ein Link als Kassenbuch oder Anker ist verboten.
//!
//! **Fehler sperren (R29):** Nach einem Schreib- oder Synchronisationsfehler gibt dieses
//! Kassenbuch keine Erlaubnis mehr und schreibt nichts mehr, bis es neu geoeffnet und dabei
//! geprueft wird.

use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{ErrorKind, Read, Write as _};
use std::path::{Path, PathBuf};

use depot_kern::{BeraterId, Cent};
use sha2::{Digest, Sha256};

use crate::budget::{AufrufErlaubnis, Buchung, Datum, KiDeckel, KiGrenzen, Sperre, pruefe};

const KOPF: &str = "v1";
const ANKER_KOPF: &str = "anker-v1";

fn halt(text: impl Into<String>) -> Sperre {
    Sperre::Kassenbuch(text.into())
}

#[derive(Debug)]
pub struct Kassenbuch {
    anker: PathBuf,
    /// Die geoeffnete und vom Betriebssystem gesperrte Datei. Die Sperre endet mit ihr.
    datei: File,
    grenzen: KiGrenzen,
    buchungen: Vec<Buchung>,
    /// SHA-256 ueber alle bestaetigten Zeilen.
    pruefsumme: Sha256,
    /// Grund, falls ein Fehler das Kassenbuch gesperrt hat.
    gesperrt: Option<String>,
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

fn hex(pruefsumme: &Sha256) -> String {
    pruefsumme
        .clone()
        .finalize()
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Der Ankertext fuer ein Kassenbuch mit genau diesem Inhalt (alle Zeilen bestaetigt).
/// Er traegt keine Berechtigung: Wer ihn schreiben kann, kann auch das Kassenbuch schreiben.
pub fn anker_fuer(inhalt: &[u8]) -> String {
    // Zahl der Zeilenenden.
    let zeilen = inhalt.split(|b| *b == b'\n').count().saturating_sub(1);
    let mut p = Sha256::new();
    p.update(inhalt);
    format!("{ANKER_KOPF};{zeilen};{}\n", hex(&p))
}

fn anker_pfad(pfad: &Path) -> PathBuf {
    pfad.with_extension("anker")
}

/// Gibt es die Datei? Ein Link oder etwas anderes als eine gewoehnliche Datei ist ein Fehler.
fn datei_da(pfad: &Path) -> Result<bool, Sperre> {
    match fs::symlink_metadata(pfad) {
        Ok(m) if m.file_type().is_symlink() => {
            Err(halt(format!("{}: Link verboten", pfad.display())))
        }
        Ok(m) if m.is_file() => Ok(true),
        Ok(_) => Err(halt(format!("{}: keine Datei", pfad.display()))),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(halt(format!("{}: nicht lesbar: {e}", pfad.display()))),
    }
}

/// Synchronisiert den Verzeichniseintrag einer neuen oder umbenannten Datei.
#[cfg(unix)]
fn verzeichnis_sync(pfad: &Path) -> Result<(), String> {
    let dir = pfad
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| format!("{}: Verzeichnis nicht synchronisiert: {e}", dir.display()))
}

/// Unter Windows laesst sich ein Verzeichnis so nicht oeffnen; dort uebernimmt das
/// Dateisystem den Verzeichniseintrag.
#[cfg(not(unix))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "gleiche Schnittstelle wie unter Unix"
)]
fn verzeichnis_sync(_: &Path) -> Result<(), String> {
    Ok(())
}

/// Schreibt den Anker atomar: Zwischendatei, synchronisieren, umbenennen, Verzeichnis
/// synchronisieren.
fn schreibe_anker(anker: &Path, text: &str) -> Result<(), String> {
    let tmp = anker.with_extension("anker.tmp");
    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp)
        .map_err(|e| format!("Anker: Zwischendatei: {e}"))?;
    f.write_all(text.as_bytes())
        .and_then(|()| f.sync_all())
        .map_err(|e| format!("Anker: schreiben: {e}"))?;
    drop(f);
    fs::rename(&tmp, anker).map_err(|e| format!("Anker: umbenennen: {e}"))?;
    verzeichnis_sync(anker)
}

/// Bestaetigter Stand laut Anker: Zeilenzahl und Pruefsumme.
fn lies_anker(anker: &Path) -> Result<(usize, String), Sperre> {
    let text = fs::read_to_string(anker).map_err(|e| halt(format!("Anker: nicht lesbar: {e}")))?;
    let zeile = text
        .strip_suffix('\n')
        .ok_or_else(|| halt("Anker: unvollstaendig"))?;
    let teile: Vec<&str> = zeile.split(';').collect();
    let [kopf, zeilen, summe] = teile.as_slice() else {
        return Err(halt("Anker: unlesbar"));
    };
    let zeilen: usize = zeilen.parse().map_err(|_| halt("Anker: unlesbar"))?;
    if *kopf != ANKER_KOPF || summe.len() != 64 || !summe.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(halt("Anker: unlesbar"));
    }
    Ok((zeilen, (*summe).to_string()))
}

impl Kassenbuch {
    /// Richtet ein leeres Kassenbuch mit Anker ein. Nur ein ausdruecklicher Aufruf legt es an;
    /// gibt es Kassenbuch oder Anker schon, wird nichts veraendert.
    ///
    /// # Errors
    /// `Kassenbuch`, wenn eine der Dateien schon da ist oder nicht geschrieben werden kann.
    pub fn einrichten(pfad: &Path) -> Result<(), Sperre> {
        let anker = anker_pfad(pfad);
        if datei_da(pfad)? || datei_da(&anker)? {
            return Err(halt("Einrichtung: Kassenbuch oder Anker gibt es schon"));
        }
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(pfad)
            .and_then(|f| f.sync_all())
            .map_err(|e| halt(format!("Einrichtung: {e}")))?;
        verzeichnis_sync(pfad).map_err(halt)?;
        schreibe_anker(&anker, &anker_fuer(b"")).map_err(halt)
    }

    /// Oeffnet exklusiv, prueft Anker und ganze Historie und liest alle Buchungen. Grenzen und
    /// Deckel werden dabei fest gebunden (R19). Jede Unstimmigkeit sperrt alles.
    ///
    /// # Errors
    /// `KassenbuchBelegt`, wenn ein anderer Lauf es offen hat; `Kassenbuch` bei fehlender
    /// Einrichtung, fehlender Datei, Abweichung vom Anker, unstimmiger Historie oder
    /// Lesefehlern.
    pub fn oeffnen(pfad: &Path) -> Result<Kassenbuch, Sperre> {
        let anker = anker_pfad(pfad);
        if !datei_da(pfad)? {
            return Err(halt(if datei_da(&anker)? {
                "Kassenbuch fehlt"
            } else {
                "nicht eingerichtet: Kassenbuch und Anker fehlen"
            }));
        }
        // Erst sperren, dann alles Weitere lesen: Die Sperre gilt der Datei, nicht dem Namen.
        let mut datei = OpenOptions::new()
            .read(true)
            .append(true)
            .open(pfad)
            .map_err(|e| halt(format!("oeffnen: {e}")))?;
        match datei.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(Sperre::KassenbuchBelegt),
            Err(TryLockError::Error(e)) => return Err(halt(format!("sperren: {e}"))),
        }
        if !datei_da(&anker)? {
            return Err(halt("Anker fehlt"));
        }
        let mut inhalt = Vec::new();
        datei
            .read_to_end(&mut inhalt)
            .map_err(|e| halt(format!("lesen: {e}")))?;
        if !inhalt.is_empty() && !inhalt.ends_with(b"\n") {
            return Err(halt("letzte Zeile unvollstaendig"));
        }
        let (bestaetigt, summe) = lies_anker(&anker)?;
        let zeilen: Vec<&[u8]> = inhalt.split_inclusive(|b| *b == b'\n').collect();
        if zeilen.len() < bestaetigt {
            return Err(halt(format!(
                "kuerzer als der bestaetigte Stand: {} von {bestaetigt} Zeilen",
                zeilen.len()
            )));
        }
        if zeilen.len() > bestaetigt.saturating_add(1) {
            return Err(halt(format!(
                "mehr als ein unbestaetigter Eintrag: {} Zeilen, bestaetigt {bestaetigt}",
                zeilen.len()
            )));
        }
        let mut pruefsumme = Sha256::new();
        for z in zeilen.iter().take(bestaetigt) {
            pruefsumme.update(z);
        }
        if hex(&pruefsumme) != summe {
            return Err(halt("weicht vom bestaetigten Stand ab"));
        }

        let grenzen = KiGrenzen::standard();
        let mut buchungen: Vec<Buchung> = Vec::new();
        for (i, z) in zeilen.iter().enumerate() {
            let nr = i.saturating_add(1);
            let b = std::str::from_utf8(z)
                .ok()
                .and_then(|t| lies_zeile(t.trim_end_matches('\n')))
                .ok_or_else(|| halt(format!("Zeile {nr} unlesbar")))?;
            let erwartet = pruefe(
                &grenzen,
                &KiDeckel::standard(b.berater),
                &buchungen,
                b.berater,
                b.datum,
                b.zyklus,
            )
            .map_err(|s| halt(format!("Zeile {nr}: Historie verletzt die Grenzen: {s:?}")))?;
            if erwartet != b.reserviert {
                return Err(halt(format!(
                    "Zeile {nr}: Reservierung {} statt {} Cent",
                    b.reserviert.0, erwartet.0
                )));
            }
            buchungen.push(b);
        }
        if let Some(offen) = zeilen.get(bestaetigt) {
            // Gebucht, aber nicht mehr bestaetigt: Die Zeile ist geprueft und zaehlt.
            pruefsumme.update(offen);
            schreibe_anker(
                &anker,
                &format!("{ANKER_KOPF};{};{}\n", zeilen.len(), hex(&pruefsumme)),
            )
            .map_err(halt)?;
        }
        Ok(Kassenbuch {
            anker,
            datei,
            grenzen,
            buchungen,
            pruefsumme,
            gesperrt: None,
        })
    }

    /// Prueft die Grenzen, bucht die Hoechstkosten dauerhaft und gibt erst dann die Erlaubnis.
    ///
    /// # Errors
    /// Jede verletzte Grenze und jeder Schreibfehler ergibt eine `Sperre`; dann wird nicht
    /// gefragt. Nach einem Schreibfehler bleibt das Kassenbuch gesperrt.
    pub fn reserviere(
        &mut self,
        berater: BeraterId,
        datum: Datum,
        zyklus: u32,
    ) -> Result<AufrufErlaubnis, Sperre> {
        if let Some(grund) = &self.gesperrt {
            return Err(halt(format!("gesperrt nach Fehler: {grund}")));
        }
        let deckel = KiDeckel::standard(berater);
        let reserviert = pruefe(
            &self.grenzen,
            &deckel,
            &self.buchungen,
            berater,
            datum,
            zyklus,
        )?;
        let buchung = Buchung {
            berater,
            datum,
            zyklus,
            reserviert,
        };
        if let Err(grund) = self.anhaengen(&zeile(&buchung)) {
            self.gesperrt = Some(grund.clone());
            return Err(halt(grund));
        }
        self.buchungen.push(buchung.clone());
        Ok(AufrufErlaubnis::neu(buchung, deckel))
    }

    /// Haengt die Zeile an, synchronisiert sie und rueckt dann den Anker vor.
    fn anhaengen(&mut self, z: &str) -> Result<(), String> {
        self.datei
            .write_all(z.as_bytes())
            .and_then(|()| self.datei.sync_all())
            .map_err(|e| format!("schreiben: {e}"))?;
        self.pruefsumme.update(z.as_bytes());
        let zeilen = self.buchungen.len().saturating_add(1);
        schreibe_anker(
            &self.anker,
            &format!("{ANKER_KOPF};{zeilen};{}\n", hex(&self.pruefsumme)),
        )
    }

    pub fn buchungen(&self) -> &[Buchung] {
        &self.buchungen
    }
}
