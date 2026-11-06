//! Das Papierkonto als Datei (`daten/papierkonto.txt`), unabhaengig vom Journal.
//!
//! Nach jedem Versand und jedem Marktlauf wird die ganze Datei neu geschrieben: erst in eine
//! Nebendatei, auf den Datentraeger, dann an ihren Platz. Misslingt das, gilt die Aenderung als
//! nicht geschehen: Das Konto kehrt zum geschriebenen Stand zurueck, der Versand gilt als ohne
//! Antwort, und das Konto meldet eine Stoerung. Der Lauf haelt dann an, bevor das Journal noch
//! etwas bestaetigt (K23-01). Kein Automatismus gleicht etwas aus.
//!
//! **Einrichtung:** Die Datei entsteht vor dem Depot im Journal. Haelt der erste Lauf
//! dazwischen an, liegt eine Datei ohne Depot vor. Sie gilt als angefangene Einrichtung, wenn
//! sie genau dem frisch eroeffneten Konto zu den Starttoepfen entspricht; dann setzt der naechste
//! Lauf sie fort (K23-03). Jede andere Datei ohne Depot ist Halt.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use depot_journal::{Konto, KontoOrder, Kontoauszug};
use depot_kern::{Datum, Kurse, Topf, Zeitpunkt};
use depot_regelwerk::GenehmigterAuftrag;

use crate::papier::{Broker, Papierkonto, Sendeantwort};

/// Das Papierkonto mit seiner Datei.
#[derive(Debug)]
pub struct Papierdatei {
    konto: Papierkonto,
    pfad: PathBuf,
    fehler: Option<String>,
}

fn schreibe(pfad: &Path, text: &str) -> Result<(), String> {
    let neben = pfad.with_extension("txt.neu");
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&neben)
        .and_then(|mut f| {
            f.write_all(text.as_bytes())?;
            f.sync_all()
        })
        .map_err(|e| format!("{}: {e}", neben.display()))?;
    fs::rename(&neben, pfad).map_err(|e| format!("{}: {e}", pfad.display()))
}

fn lies(pfad: &Path) -> Result<Papierkonto, String> {
    let text = fs::read_to_string(pfad).map_err(|e| format!("{}: {e}", pfad.display()))?;
    Papierkonto::aus_text(&text)
}

impl Papierdatei {
    /// Oeffnet das Papierkonto. Gibt es noch keine Datei und hat das Journal noch kein Depot,
    /// wird das Konto mit den Starttoepfen eroeffnet und sofort geschrieben. Sonst muessen
    /// beide da sein.
    ///
    /// # Errors
    /// Datei fehlt zu einem vorhandenen Depot, Datei ohne Depot, Datei unlesbar.
    pub fn oeffnen(pfad: &Path, depot_im_journal: bool, start: &[Topf]) -> Result<Self, String> {
        let da = pfad
            .try_exists()
            .map_err(|e| format!("{}: {e}", pfad.display()))?;
        let konto = match (da, depot_im_journal) {
            (true, true) => lies(pfad)?,
            (true, false) => {
                let k = lies(pfad)?;
                let frisch =
                    Papierkonto::eroeffne(start).ok_or("Papierkonto: Startwerte ungueltig")?;
                if k != frisch {
                    return Err(format!(
                        "{} liegt mit Bewegungen vor, aber das Journal fuehrt kein Papierdepot. \
                         Halt bis zur Pruefung",
                        pfad.display()
                    ));
                }
                k
            }
            (false, false) => {
                let k = Papierkonto::eroeffne(start).ok_or("Papierkonto: Startwerte ungueltig")?;
                schreibe(pfad, &k.als_text())?;
                k
            }
            (false, true) => {
                return Err(format!(
                    "{} fehlt, aber das Journal fuehrt ein Papierdepot. Halt bis zur Pruefung",
                    pfad.display()
                ));
            }
        };
        Ok(Papierdatei {
            konto,
            pfad: pfad.to_path_buf(),
            fehler: None,
        })
    }

    pub fn konto(&self) -> &Papierkonto {
        &self.konto
    }

    /// Der erste Fehler beim Schreiben, wenn es einen gab. Dann stimmt die Datei nicht mit
    /// dem Lauf ueberein, und der naechste Lauf haelt beim Abgleich an.
    pub fn fehler(&self) -> Option<&str> {
        self.fehler.as_deref()
    }

    /// Schreibt den neuen Stand. Misslingt es, kehrt das Konto zu `vorher` zurueck, dem
    /// zuletzt geschriebenen Stand, und die Stoerung bleibt stehen.
    fn sichere(&mut self, vorher: Papierkonto) -> bool {
        if self.fehler.is_some() {
            self.konto = vorher;
            return false;
        }
        match schreibe(&self.pfad, &self.konto.als_text()) {
            Ok(()) => true,
            Err(e) => {
                self.konto = vorher;
                self.fehler = Some(e);
                false
            }
        }
    }
}

impl Konto for Papierdatei {
    fn auszug(&self) -> Kontoauszug {
        self.konto.auszug()
    }

    fn order(&self, nr: u64) -> Option<KontoOrder> {
        self.konto.order(nr)
    }
}

impl Broker for Papierdatei {
    fn sende(&mut self, auftrag: &GenehmigterAuftrag) -> Sendeantwort {
        let vorher = self.konto.clone();
        let antwort = self.konto.sende(auftrag);
        if self.sichere(vorher) {
            antwort
        } else {
            Sendeantwort::KeineAntwort
        }
    }

    fn sende_am(&mut self, auftrag: &GenehmigterAuftrag, zeit: Zeitpunkt) -> Sendeantwort {
        let vorher = self.konto.clone();
        let antwort = self.konto.sende_am(auftrag, zeit);
        if self.sichere(vorher) {
            antwort
        } else {
            Sendeantwort::KeineAntwort
        }
    }

    fn stoppe_kaeufe(&mut self) -> bool {
        let vorher = self.konto.clone();
        self.konto.stoppe_kaeufe() && self.sichere(vorher)
    }

    fn rechne_ab(&mut self, tag: Datum) {
        self.konto.rechne_ab(tag);
    }

    fn markt(&mut self, kurse: &Kurse) {
        let vorher = self.konto.clone();
        self.konto.markt(kurse);
        if self.konto != vorher {
            self.sichere(vorher);
        }
    }

    fn stoerung(&self) -> Option<String> {
        self.fehler
            .as_ref()
            .map(|e| format!("Papierkonto nicht geschrieben: {e}"))
    }
}
