//! Die Datei des Journals: Hash-Kette, Anhangsvermerk, Sperre und Wiederherstellung.
//!
//! **Format (Kap. 9):** Jede Zeile ist `laenge;kette;eintrag\n`. `laenge` ist die Bytezahl des
//! Eintrags, `kette` der SHA-256 aus der Kette der Vorzeile und dem Eintrag. Ein Eintrag
//! enthaelt keine Steuerzeichen, also auch keinen Zeilenumbruch. Die erste Zeile ist der Kopf;
//! sie entsteht nur durch `einrichten`.
//!
//! **Anhangsvermerk:** Neben dem Journal liegt `*.vermerk` mit dem bestaetigten Stand (Laenge
//! und Kette) und der Laenge des geplanten Anhangs. Vor jedem Anhaengen wird der Plan
//! vermerkt und synchronisiert, dann die Zeile geschrieben und synchronisiert, dann der Stand
//! vorgerueckt. Erst danach gilt der Eintrag.
//!
//! **Wiederherstellung:** Was jenseits des bestaetigten Stands liegt, wird nur behandelt, wenn
//! der Vermerk es als geplanten Anhang ausweist. Ist es genau der vollstaendige Eintrag, wird
//! er geprueft und bestaetigt. Ist es kuerzer und ohne Zeilenende, war es ein Schreibabbruch:
//! Der Rest kommt byte-gleich in eine Quarantaenedatei, dann wird gekuerzt. Alles andere ist
//! Halt. Jeder Schritt ist wiederholbar und fuehrt beim naechsten Start zum selben Ergebnis.

use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write as _};
use std::path::{Component, Path, PathBuf};

use depot_kern::{BeraterId, Cent, Datum, Topf, TopfId};
use depot_ki::{Sperre, grenzabdruck};
use depot_regelwerk::{GenehmigterAuftrag, Kennungsvergabe};
use sha2::{Digest, Sha256};

use crate::auftrag::{Meldung, meldung_text};
use crate::kasse::{AufrufErlaubnis, buchung_text, naechste};
use crate::kursbeleg::{KursBeleg, kursbeleg_text};
use crate::stand::{
    Abschluss, Art, FORMAT, FORMAT_ALT, Stand, abschluss_text, begruendung_text, beleg_text,
    beobachtung_text, topf_text, trenne,
};
use crate::terminbeleg::{Fortschreibung, TerminBeleg, terminbeleg_text};

/// Kopf eines Journals im Format 2; neue Journale bekommen ihn.
const KOPF: &str = "journal-v2";
/// Kopf eines Journals im Format 1 (bis PR #15). Es wird nach seinen Regeln gelesen und erst
/// nach der Umstellung mit neuen Auftraegen fortgesetzt (K16-02).
const KOPF_ALT: &str = "journal-v1";
const VERMERK_KOPF: &str = "vermerk-v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JournalFehler {
    /// Ein anderer Lauf hat das Journal offen.
    Belegt,
    /// Das Journal fehlt, ist unstimmig, nicht les- oder schreibbar oder nach einem Fehler
    /// gesperrt. Halt bis zum geprueften Wiederanlauf (R25, R29).
    Halt(String),
    /// Der Eintrag ist unzulaessig. Nichts wurde geschrieben; das Journal arbeitet weiter.
    Abgelehnt(String),
}

fn halt(text: impl Into<String>) -> JournalFehler {
    JournalFehler::Halt(text.into())
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn kette(vorher: &str, eintrag: &str) -> String {
    let mut p = Sha256::new();
    p.update(vorher.as_bytes());
    p.update(b";");
    p.update(eintrag.as_bytes());
    hex(&p.finalize())
}

fn kettenanfang() -> String {
    "0".repeat(64)
}

fn zeile(vorher: &str, eintrag: &str) -> (String, String) {
    let k = kette(vorher, eintrag);
    (format!("{};{k};{eintrag}\n", eintrag.len()), k)
}

fn als_u64(n: usize) -> Result<u64, String> {
    u64::try_from(n).map_err(|_| "Laenge zu gross".to_string())
}

/// Prueft vollstaendige Zeilen ab der Kette `vorher`. Liefert die Eintraege und die letzte
/// Kette. Jede Abweichung ist ein Fehler: Laenge, Kette, Zeichen, fehlendes Zeilenende.
fn pruefe_zeilen(bytes: &[u8], vorher: &str) -> Result<(Vec<String>, String), String> {
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return Err("letzte Zeile ohne Zeilenende".into());
    }
    let mut k = vorher.to_string();
    let mut eintraege = Vec::new();
    for (i, z) in bytes.split_inclusive(|b| *b == b'\n').enumerate() {
        let nr = i.saturating_add(1);
        let text = std::str::from_utf8(z)
            .ok()
            .and_then(|t| t.strip_suffix('\n'))
            .ok_or_else(|| format!("Zeile {nr} unlesbar"))?;
        let mut teile = text.splitn(3, ';');
        let (Some(laenge), Some(summe), Some(eintrag)) = (teile.next(), teile.next(), teile.next())
        else {
            return Err(format!("Zeile {nr} unlesbar"));
        };
        if laenge != eintrag.len().to_string() {
            return Err(format!("Zeile {nr}: Laenge stimmt nicht"));
        }
        if eintrag.chars().any(char::is_control) {
            return Err(format!("Zeile {nr}: Steuerzeichen"));
        }
        let neu = kette(&k, eintrag);
        if summe != neu {
            return Err(format!("Zeile {nr}: Pruefsumme stimmt nicht"));
        }
        k = neu;
        eintraege.push(eintrag.to_string());
    }
    Ok((eintraege, k))
}

/// Bestaetigter Stand und geplanter Anhang laut Vermerk.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Vermerk {
    laenge: u64,
    kette: String,
    geplant: u64,
}

impl Vermerk {
    fn text(&self) -> String {
        format!(
            "{VERMERK_KOPF};{};{};{}\n",
            self.laenge, self.kette, self.geplant
        )
    }

    fn lies(text: &str) -> Option<Vermerk> {
        let teile: Vec<&str> = text.strip_suffix('\n')?.split(';').collect();
        let [kopf, laenge, kette, geplant] = teile.as_slice() else {
            return None;
        };
        let v = Vermerk {
            laenge: laenge.parse().ok()?,
            kette: (*kette).to_string(),
            geplant: geplant.parse().ok()?,
        };
        (*kopf == VERMERK_KOPF && v.text() == text).then_some(v)
    }
}

/// Der Inhalt eines eingerichteten Journals mit genau diesen Eintraegen, im aktuellen Format.
/// Er traegt keine Berechtigung und wird nicht geprueft: Wer ihn schreiben kann, kann auch
/// das Journal schreiben. Fuer Tests und Gegenproben.
pub fn inhalt_fuer(eintraege: &[(Art, &str)]) -> Vec<u8> {
    let (mut aus, mut k) = zeile(&kettenanfang(), KOPF);
    for (art, inhalt) in eintraege {
        let (z, neu) = zeile(&k, &format!("{};{inhalt}", art.name()));
        aus.push_str(&z);
        k = neu;
    }
    aus.into_bytes()
}

/// Der Vermerk fuer ein Journal mit genau diesem bestaetigten Inhalt und einem geplanten
/// Anhang von `geplant` Byte. `None`, wenn der Inhalt keine gueltige Kette ist.
pub fn vermerk_fuer(inhalt: &[u8], geplant: u64) -> Option<String> {
    let (_, k) = pruefe_zeilen(inhalt, &kettenanfang()).ok()?;
    Some(
        Vermerk {
            laenge: als_u64(inhalt.len()).ok()?,
            kette: k,
            geplant,
        }
        .text(),
    )
}

/// Kein Teil des uebergebenen Pfads darf ein Link sein (R24, K12-02). Unter Windows meldet
/// `is_symlink` auch Verzeichnisverknuepfungen (Junctions). Ein Teil, den es noch nicht gibt,
/// beendet die Pruefung. `..` ist verboten.
fn kein_link_im_pfad(pfad: &Path) -> Result<(), JournalFehler> {
    let mut bisher = PathBuf::new();
    for teil in pfad.components() {
        bisher.push(teil);
        match teil {
            Component::Normal(_) => {}
            Component::ParentDir => {
                return Err(halt(format!("{}: `..` im Pfad verboten", pfad.display())));
            }
            Component::Prefix(_) | Component::RootDir | Component::CurDir => continue,
        }
        match fs::symlink_metadata(&bisher) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(halt(format!("{}: Link im Pfad verboten", bisher.display())));
            }
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(halt(format!("{}: nicht lesbar: {e}", bisher.display()))),
        }
    }
    Ok(())
}

/// Gibt es die Datei? Ein Link oder etwas anderes als eine gewoehnliche Datei ist ein Fehler.
fn datei_da(pfad: &Path) -> Result<bool, JournalFehler> {
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

/// Synchronisiert eine Datei. `was` nennt sie fuer die Modultests: Dort wird jede
/// Synchronisation mitgeschrieben, und es laesst sich ein Fehler einspeisen.
fn sync(datei: &File, was: &'static str) -> std::io::Result<()> {
    #[cfg(test)]
    crate::tests::protokolliere(was);
    #[cfg(not(test))]
    let _ = was;
    #[cfg(test)]
    if crate::tests::sync_scheitert() {
        return Err(std::io::Error::other(
            "eingespeister Synchronisationsfehler",
        ));
    }
    datei.sync_all()
}

/// Schreibt eine Datei atomar: Zwischendatei exklusiv neu anlegen, synchronisieren,
/// umbenennen, Verzeichnis synchronisieren. Eine liegen gebliebene Zwischendatei wird zuerst
/// entfernt; das entfernt nur ihren Namen, nie das Ziel eines Links (K12-01). Eine fremde
/// Datei wird nie geoeffnet, und das Umbenennen ersetzt nur den Namen des Ziels.
fn schreibe_atomar(ziel: &Path, inhalt: &[u8]) -> Result<(), String> {
    let mut name = ziel.as_os_str().to_owned();
    name.push(".tmp");
    let tmp = PathBuf::from(name);
    match fs::remove_file(&tmp) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(format!("{}: alte Zwischendatei: {e}", tmp.display())),
    }
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| format!("{}: Zwischendatei: {e}", tmp.display()))?;
    f.write_all(inhalt)
        .and_then(|()| sync(&f, "Zwischendatei"))
        .map_err(|e| format!("{}: schreiben: {e}", tmp.display()))?;
    drop(f);
    fs::rename(&tmp, ziel).map_err(|e| format!("{}: umbenennen: {e}", ziel.display()))?;
    verzeichnis_sync(ziel)
}

fn vermerk_pfad(pfad: &Path) -> PathBuf {
    pfad.with_extension("vermerk")
}

/// Die Quarantaene eines abgeschnittenen Rests. Der Name haengt an Stelle und Inhalt: Ein
/// wiederholter Start findet dieselbe Datei, ein anderer Rest bekommt eine eigene.
fn quarantaene_pfad(pfad: &Path, stelle: u64, rest: &[u8]) -> PathBuf {
    let summe = hex(&Sha256::digest(rest));
    let kurz = summe.get(..16).unwrap_or(&summe);
    pfad.with_extension(format!("q{stelle}-{kurz}"))
}

/// Sichert den Rest byte-gleich, auch wenn die Quarantaene schon existiert.
fn sichere(ziel: &Path, rest: &[u8]) -> Result<(), JournalFehler> {
    if !datei_da(ziel)? {
        schreibe_atomar(ziel, rest).map_err(halt)?;
    }
    let da = File::open(ziel).and_then(|f| {
        f.sync_all()?;
        fs::read(ziel)
    });
    match da {
        Ok(da) if da == rest => Ok(()),
        Ok(_) => Err(halt(format!(
            "{}: Quarantaene weicht vom Rest ab; nichts gekuerzt",
            ziel.display()
        ))),
        Err(e) => Err(halt(format!("{}: Quarantaene: {e}", ziel.display()))),
    }
}

#[derive(Debug)]
pub struct Journal {
    vermerk: PathBuf,
    /// Die geoeffnete und vom Betriebssystem gesperrte Datei. Die Sperre endet mit ihr.
    datei: File,
    laenge: u64,
    kette: String,
    eintraege: Vec<(Art, String)>,
    stand: Stand,
    /// Grund, falls ein Schreibfehler das Journal gesperrt hat (R29).
    gesperrt: Option<String>,
}

/// Prueft alle Eintraege nach dem Kopf und rechnet den Stand nach.
fn nachrechnen(zeilen: &[String]) -> Result<(Vec<(Art, String)>, Stand), JournalFehler> {
    let mut alle = zeilen.iter();
    let format = match alle.next().map(String::as_str) {
        Some(KOPF) => FORMAT,
        Some(KOPF_ALT) => FORMAT_ALT,
        _ => return Err(halt("Kopfzeile fehlt")),
    };
    let mut stand = Stand::im_format(format);
    let mut eintraege = Vec::new();
    for (i, z) in alle.enumerate() {
        let nr = i.saturating_add(2);
        let (art, inhalt) = trenne(z).ok_or_else(|| halt(format!("Zeile {nr}: Art unbekannt")))?;
        stand
            .anwenden(art, inhalt)
            .map_err(|e| halt(format!("Zeile {nr}: {e}")))?;
        eintraege.push((art, inhalt.to_string()));
    }
    Ok((eintraege, stand))
}

impl Journal {
    /// Richtet ein Journal mit Kopfzeile und Vermerk ein. Nur ein ausdruecklicher Aufruf legt
    /// es an; gibt es Journal oder Vermerk schon, wird nichts veraendert.
    ///
    /// # Errors
    /// `Halt`, wenn eine der Dateien schon da ist oder nicht geschrieben werden kann.
    pub fn einrichten(pfad: &Path) -> Result<(), JournalFehler> {
        kein_link_im_pfad(pfad)?;
        let vermerk = vermerk_pfad(pfad);
        if datei_da(pfad)? || datei_da(&vermerk)? {
            return Err(halt("Einrichtung: Journal oder Vermerk gibt es schon"));
        }
        let (kopf, k) = zeile(&kettenanfang(), KOPF);
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(pfad)
            .and_then(|mut f| {
                f.write_all(kopf.as_bytes())?;
                sync(&f, "Journal")
            })
            .map_err(|e| halt(format!("Einrichtung: {e}")))?;
        verzeichnis_sync(pfad).map_err(halt)?;
        let v = Vermerk {
            laenge: als_u64(kopf.len()).map_err(halt)?,
            kette: k,
            geplant: 0,
        };
        schreibe_atomar(&vermerk, v.text().as_bytes()).map_err(halt)
    }

    /// Oeffnet exklusiv, prueft die ganze Kette gegen den Vermerk, behandelt einen vermerkten
    /// Anhang und rechnet den Stand nach. Jede Unstimmigkeit ist Halt.
    ///
    /// # Errors
    /// `Belegt`, wenn ein anderer Lauf es offen hat; sonst `Halt`.
    pub fn oeffnen(pfad: &Path) -> Result<Journal, JournalFehler> {
        kein_link_im_pfad(pfad)?;
        let vermerk = vermerk_pfad(pfad);
        if !datei_da(pfad)? {
            return Err(halt(if datei_da(&vermerk)? {
                "Journal fehlt"
            } else {
                "nicht eingerichtet: Journal und Vermerk fehlen"
            }));
        }
        // Erst sperren, dann alles Weitere lesen: Die Sperre gilt der Datei, nicht dem Namen.
        let mut datei = OpenOptions::new()
            .read(true)
            .write(true)
            .open(pfad)
            .map_err(|e| halt(format!("oeffnen: {e}")))?;
        match datei.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(JournalFehler::Belegt),
            Err(TryLockError::Error(e)) => return Err(halt(format!("sperren: {e}"))),
        }
        if !datei_da(&vermerk)? {
            return Err(halt("Anhangsvermerk fehlt"));
        }
        let v = fs::read_to_string(&vermerk)
            .ok()
            .and_then(|t| Vermerk::lies(&t))
            .ok_or_else(|| halt("Anhangsvermerk unlesbar"))?;
        let mut inhalt = Vec::new();
        datei
            .read_to_end(&mut inhalt)
            .map_err(|e| halt(format!("lesen: {e}")))?;

        let grenze = usize::try_from(v.laenge).map_err(|_| halt("Vermerk: Laenge"))?;
        let (bestaetigt, rest) = inhalt.split_at_checked(grenze).ok_or_else(|| {
            halt(format!(
                "kuerzer als der bestaetigte Stand: {} von {grenze} Byte",
                inhalt.len()
            ))
        })?;
        let (mut zeilen, k) = pruefe_zeilen(bestaetigt, &kettenanfang())
            .map_err(|e| halt(format!("bestaetigter Stand beschaedigt: {e}")))?;
        if k != v.kette {
            return Err(halt("weicht vom bestaetigten Stand ab"));
        }
        let rest_laenge = als_u64(rest.len()).map_err(halt)?;

        // Was jenseits des bestaetigten Stands liegt, entscheidet allein der Vermerk.
        let mut neu = Vermerk {
            laenge: v.laenge,
            kette: k.clone(),
            geplant: 0,
        };
        let mut abschneiden = false;
        if rest.is_empty() {
            // Geplant, aber nichts geschrieben, oder schon gekuerzt.
        } else if v.geplant == 0 {
            return Err(halt("Anhang ohne Vermerk"));
        } else if rest_laenge > v.geplant {
            return Err(halt("mehr als der vermerkte Anhang"));
        } else if rest_laenge == v.geplant {
            let (z, k2) = pruefe_zeilen(rest, &k)
                .map_err(|e| halt(format!("unbestaetigter Eintrag beschaedigt: {e}")))?;
            if z.len() != 1 {
                return Err(halt("unbestaetigter Anhang ist nicht genau ein Eintrag"));
            }
            zeilen.extend(z);
            neu.laenge = v
                .laenge
                .checked_add(rest_laenge)
                .ok_or_else(|| halt("Laenge"))?;
            neu.kette = k2;
        } else if rest.contains(&b'\n') {
            return Err(halt("unvollstaendiger Anhang mit Zeilenende: beschaedigt"));
        } else {
            abschneiden = true;
        }

        // Erst den ganzen Stand pruefen, dann schreiben.
        let (eintraege, stand) = nachrechnen(&zeilen)?;
        if abschneiden {
            sichere(&quarantaene_pfad(pfad, v.laenge, rest), rest)?;
            datei
                .set_len(v.laenge)
                .map_err(|e| halt(format!("kuerzen: {e}")))?;
        }
        // Der Vermerk beschreibt nur, was dauerhaft auf der Platte steht (K13-01): Ein nach
        // einem Fehler nur im Cache lesbarer Eintrag oder eine Kuerzung wird erst
        // synchronisiert, dann rueckt der Vermerk vor.
        if neu != v {
            sync(&datei, "Journal").map_err(|e| halt(format!("synchronisieren: {e}")))?;
            schreibe_atomar(&vermerk, neu.text().as_bytes()).map_err(halt)?;
        }
        Ok(Journal {
            vermerk,
            datei,
            laenge: neu.laenge,
            kette: neu.kette,
            eintraege,
            stand,
            gesperrt: None,
        })
    }

    /// Alle Eintraege nach dem Kopf, mit Art und Inhalt.
    pub fn eintraege(&self) -> &[(Art, String)] {
        &self.eintraege
    }

    pub fn stand(&self) -> &Stand {
        &self.stand
    }

    /// Der bestaetigte Stand: Laenge in Byte und Kette der letzten Zeile. Er gehoert in den
    /// Abendbericht, als Anker ausserhalb des Rechners (R38).
    pub fn bestaetigt(&self) -> (u64, String) {
        (self.laenge, self.kette.clone())
    }

    /// Die Vergabe der Auftragskennungen hinter der letzten belegten Kennung (R24, 1·F09).
    /// Belegt werden kann nur eine groessere Kennung; zwei Vergaben fuer dieselben Nummern
    /// scheitern also spaetestens beim Belegen.
    pub fn kennungsvergabe(&self) -> Kennungsvergabe {
        Kennungsvergabe::nach(self.stand.letzte_kennung())
    }

    /// Haengt einen Eintrag an. Er wird vorher genauso geprueft wie beim Oeffnen.
    ///
    /// KI-Buchungen und Grenzabdruecke entstehen nur ueber `reserviere`, immer unter den
    /// heutigen Grenzen (K13-03). Aeltere stehen nur im Journal, das geoeffnet wird. Auftraege
    /// entstehen nur ueber `reserviere_auftrag` mit der Deckung des Kontowaechters, ihre
    /// Zustaende nur ueber `melde` und `gleiche_ab`.
    ///
    /// # Errors
    /// `Abgelehnt` fuer einen unzulaessigen Eintrag, fuer jeden KI-Eintrag, jeden Auftrag und
    /// jeden Auftragszustand; nichts wird
    /// geschrieben. `Halt` nach einem Schreib- oder Synchronisationsfehler; dann bleibt das
    /// Journal gesperrt.
    pub fn anhaengen(&mut self, art: Art, inhalt: &str) -> Result<(), JournalFehler> {
        match art {
            Art::Ki | Art::KiGrenzen => Err(JournalFehler::Abgelehnt(
                "KI-Eintraege entstehen nur ueber reserviere".into(),
            )),
            Art::Auftrag => Err(JournalFehler::Abgelehnt(
                "Auftraege entstehen nur ueber reserviere_auftrag".into(),
            )),
            Art::Zustand => Err(JournalFehler::Abgelehnt(
                "Auftragszustaende entstehen nur ueber melde und gleiche_ab".into(),
            )),
            Art::Format => Err(JournalFehler::Abgelehnt(
                "Die Umstellung entsteht nur ueber stelle_um".into(),
            )),
            Art::Beobachtung
            | Art::Topf
            | Art::Kurs
            | Art::Termin
            | Art::Begruendung
            | Art::Abschluss => self.schreibe_eintrag(art, inhalt),
        }
    }

    /// Prueft und schreibt einen Eintrag jeder Art.
    fn schreibe_eintrag(&mut self, art: Art, inhalt: &str) -> Result<(), JournalFehler> {
        if let Some(grund) = &self.gesperrt {
            return Err(halt(format!("gesperrt nach Fehler: {grund}")));
        }
        if inhalt.chars().any(char::is_control) {
            return Err(JournalFehler::Abgelehnt("Steuerzeichen im Eintrag".into()));
        }
        let mut stand = self.stand.clone();
        stand
            .anwenden(art, inhalt)
            .map_err(JournalFehler::Abgelehnt)?;
        let eintrag = format!("{};{inhalt}", art.name());
        let (z, k) = zeile(&self.kette, &eintrag);
        if let Err(grund) = self.schreibe(&z, &k) {
            self.gesperrt = Some(grund.clone());
            return Err(halt(grund));
        }
        self.stand = stand;
        self.eintraege.push((art, inhalt.to_string()));
        Ok(())
    }

    /// Vermerkt den Plan, schreibt und synchronisiert die Zeile, rueckt den Stand vor.
    fn schreibe(&mut self, z: &str, k: &str) -> Result<(), String> {
        let n = als_u64(z.len())?;
        let plan = Vermerk {
            laenge: self.laenge,
            kette: self.kette.clone(),
            geplant: n,
        };
        schreibe_atomar(&self.vermerk, plan.text().as_bytes())?;
        self.datei
            .seek(SeekFrom::Start(self.laenge))
            .and_then(|_| self.datei.write_all(z.as_bytes()))
            .and_then(|()| sync(&self.datei, "Journal"))
            .map_err(|e| format!("schreiben: {e}"))?;
        let laenge = self.laenge.checked_add(n).ok_or("Laenge zu gross")?;
        let fertig = Vermerk {
            laenge,
            kette: k.to_string(),
            geplant: 0,
        };
        schreibe_atomar(&self.vermerk, fertig.text().as_bytes())?;
        self.laenge = laenge;
        self.kette = k.to_string();
        Ok(())
    }

    /// Belegt eine Beobachtung (R40).
    ///
    /// # Errors
    /// Wie `anhaengen`.
    pub fn beobachte(&mut self, datum: Datum, text: &str) -> Result<(), JournalFehler> {
        self.anhaengen(Art::Beobachtung, &beobachtung_text(datum, text))
    }

    /// Belegt eine Begruendung: wer was warum entschieden oder unterlassen hat.
    ///
    /// # Errors
    /// Wie `anhaengen`; `Abgelehnt` auch bei leerem `wer` oder Text und bei `;` in `wer`.
    pub fn begruende(&mut self, datum: Datum, wer: &str, text: &str) -> Result<(), JournalFehler> {
        if wer.contains(';') {
            return Err(JournalFehler::Abgelehnt("`;` in wer".into()));
        }
        self.anhaengen(Art::Begruendung, &begruendung_text(datum, wer, text))
    }

    /// Schliesst einen Tag ab, mit dem Hoechststand des Depotwerts bis dahin.
    ///
    /// # Errors
    /// Wie `anhaengen`; `Abgelehnt` auch, wenn das Datum nicht nach dem letzten Abschluss liegt
    /// oder der Hoechststand nicht positiv ist.
    pub fn schliesse_ab(
        &mut self,
        datum: Datum,
        hoechststand: Cent,
        abdruck: &str,
    ) -> Result<(), JournalFehler> {
        self.anhaengen(
            Art::Abschluss,
            &abschluss_text(&Abschluss {
                datum,
                hoechststand,
                abdruck: abdruck.to_string(),
            }),
        )
    }

    /// Belegt einen Kurs mit Quelle, Abrufzeit und Abdruck der rohen Antwort, bevor er
    /// verwendet wird (R12).
    ///
    /// # Errors
    /// Wie `anhaengen`; `Abgelehnt` auch im Format 1 und bei einem Abruf vor dem letzten
    /// belegten.
    pub fn belege_kurs(&mut self, beleg: &KursBeleg) -> Result<(), JournalFehler> {
        self.anhaengen(Art::Kurs, &kursbeleg_text(beleg))
    }

    /// Belegt einen Termin mit Quelle, Abrufzeit und Abdruck der rohen Antwort (R39, R12). Ein
    /// geaenderter Termin wird unter seiner Kennung fortgeschrieben; ein unveraenderter wird
    /// nicht noch einmal geschrieben.
    ///
    /// # Errors
    /// Wie `anhaengen`; `Abgelehnt` auch im Format 1 und bei einem Abruf vor dem letzten
    /// belegten.
    pub fn belege_termin(&mut self, beleg: &TerminBeleg) -> Result<Fortschreibung, JournalFehler> {
        let ergebnis = match self.stand.termin(beleg.termin().kennung()) {
            Some(alt) if alt.termin() == beleg.termin() => return Ok(Fortschreibung::Unveraendert),
            Some(_) => Fortschreibung::Geaendert,
            None => Fortschreibung::Neu,
        };
        self.anhaengen(Art::Termin, &terminbeleg_text(beleg))?;
        Ok(ergebnis)
    }

    /// Belegt den ganzen Stand eines Topfes im Papierdepot.
    ///
    /// # Errors
    /// Wie `anhaengen`.
    pub fn setze_topf(&mut self, topf: &Topf) -> Result<(), JournalFehler> {
        self.anhaengen(Art::Topf, &topf_text(topf))
    }

    /// Schreibt einen genehmigten Auftrag; die Deckung hat `reserviere_auftrag` geprueft.
    pub(crate) fn schreibe_auftrag(
        &mut self,
        topf: TopfId,
        auftrag: &GenehmigterAuftrag,
    ) -> Result<(), JournalFehler> {
        self.schreibe_eintrag(Art::Auftrag, &beleg_text(topf, auftrag))
    }

    /// Stellt ein abgeschlossenes Journal im Format 1 auf Format 2 um (K16-02). Seine Auftraege
    /// bleiben, wie sie sind: belegt, im Topf gebucht, ohne Reservierung. Danach gelten fuer
    /// jeden neuen Eintrag Auftragszustaende und Deckung. Im Format 2 tut der Aufruf nichts.
    ///
    /// # Errors
    /// `Abgelehnt`, wenn ein Tag angefangen ist; sonst wie `anhaengen`.
    pub fn stelle_um(&mut self) -> Result<(), JournalFehler> {
        if self.stand().format() == FORMAT {
            return Ok(());
        }
        self.schreibe_eintrag(Art::Format, &FORMAT.to_string())
    }

    /// Schreibt den naechsten Zustand eines Auftrags.
    pub(crate) fn schreibe_meldung(&mut self, nr: u64, m: &Meldung) -> Result<(), JournalFehler> {
        self.schreibe_eintrag(Art::Zustand, &meldung_text(nr, m))
    }

    /// Prueft die KI-Grenzen, bucht die Hoechstkosten dauerhaft und gibt erst dann die
    /// Erlaubnis (R8). Vor der ersten Buchung unter neuen Grenzen steht deren Abdruck.
    ///
    /// # Errors
    /// Jede verletzte Grenze ergibt ihre `Sperre`; jeder Fehler des Journals
    /// `Sperre::Kassenbuch`. Dann wird nicht gefragt.
    pub fn reserviere(
        &mut self,
        berater: BeraterId,
        datum: Datum,
        zyklus: u32,
    ) -> Result<AufrufErlaubnis, Sperre> {
        let (buchung, deckel) = naechste(self.stand.ki(), berater, datum, zyklus)?;
        let sperre = |e: JournalFehler| Sperre::Kassenbuch(format!("Journal: {e:?}"));
        if !self.stand.ki().abdruck_aktuell() {
            self.schreibe_eintrag(Art::KiGrenzen, &grenzabdruck())
                .map_err(sperre)?;
        }
        self.schreibe_eintrag(Art::Ki, &buchung_text(&buchung))
            .map_err(sperre)?;
        Ok(AufrufErlaubnis::neu(buchung, deckel))
    }
}
