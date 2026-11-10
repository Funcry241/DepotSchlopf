//! Lokale Gesprächsansicht. Das Protokoll ist von Journal und Abrechnung getrennt.
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use depot_kern::json::{self, Json, zitiere};
use depot_kern::{BeraterId, Cent, Datum, Zeitpunkt};

pub const LAUF_ENV: &str = "DEPOT_GESPRAECH_LAUF";
const SCHREIBSPERRE: &str = ".schreibsperre";
const MAX_DATEI: u64 = 8 * 1024 * 1024;
const MAX_ZEILE: usize = 512 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gespraech {
    pub zeit: Zeitpunkt,
    pub lauf: String,
    pub wer: String,
    pub rolle: String,
    pub modell: String,
    pub zweck: String,
    pub ergebnis: String,
    pub http: Option<u16>,
    pub token_ein: Option<u64>,
    pub token_aus: Option<u64>,
    pub buchung: Option<usize>,
    pub kosten_cent: Option<u64>,
    pub system: Option<String>,
    pub frage: Option<String>,
    pub antwort: Option<String>,
    pub fehler: Option<String>,
}

fn zahl(n: Option<impl ToString>) -> String {
    n.map_or_else(|| "null".into(), |n| n.to_string())
}

fn text(t: Option<&str>) -> String {
    t.map_or_else(|| "null".into(), zitiere)
}

impl Gespraech {
    fn zeile(&self) -> String {
        format!(
            "{{\"v\":1,\"zeit\":{},\"lauf\":{},\"wer\":{},\"rolle\":{},\"modell\":{},\"zweck\":{},\"ergebnis\":{},\"http\":{},\"token_ein\":{},\"token_aus\":{},\"buchung\":{},\"kosten_cent\":{},\"system\":{},\"frage\":{},\"antwort\":{},\"fehler\":{}}}",
            zitiere(&self.zeit.to_string()),
            zitiere(&self.lauf),
            zitiere(&self.wer),
            zitiere(&self.rolle),
            zitiere(&self.modell),
            zitiere(&self.zweck),
            zitiere(&self.ergebnis),
            zahl(self.http),
            zahl(self.token_ein),
            zahl(self.token_aus),
            zahl(self.buchung),
            zahl(self.kosten_cent),
            text(self.system.as_deref()),
            text(self.frage.as_deref()),
            text(self.antwort.as_deref()),
            text(self.fehler.as_deref())
        )
    }

    fn lies(zeile: &str) -> Option<Self> {
        let j = json::lies(zeile).ok()?;
        if j.doppelte_felder() || j.feld("v")?.ganz()? != 1 {
            return None;
        }
        let t = |name| j.feld(name)?.text();
        let optional = |name| match j.feld(name)? {
            Json::Null => Some(None),
            wert => Some(Some(wert.text()?.to_owned())),
        };
        let n = |name| match j.feld(name)? {
            Json::Null => Some(None),
            wert => Some(Some(wert.anzahl()?)),
        };
        let rolle = t("rolle")?;
        if !matches!(rolle, "haupt" | "zusatz") {
            return None;
        }
        Some(Self {
            zeit: Zeitpunkt::lies(t("zeit")?)?,
            lauf: t("lauf")?.into(),
            wer: t("wer")?.into(),
            rolle: rolle.into(),
            modell: t("modell")?.into(),
            zweck: t("zweck")?.into(),
            ergebnis: t("ergebnis")?.into(),
            http: match n("http")? {
                Some(http) => Some(u16::try_from(http).ok()?),
                None => None,
            },
            token_ein: n("token_ein")?,
            token_aus: n("token_aus")?,
            buchung: match j.feld("buchung") {
                None | Some(Json::Null) => None,
                Some(wert) => Some(usize::try_from(wert.anzahl()?).ok()?),
            },
            kosten_cent: n("kosten_cent")?,
            system: optional("system")?,
            frage: optional("frage")?,
            antwort: optional("antwort")?,
            fehler: optional("fehler")?,
        })
    }
}

fn ordner(daten: &Path) -> PathBuf {
    daten.join("gespraeche")
}

fn datei(daten: &Path, tag: Datum) -> PathBuf {
    ordner(daten).join(format!("{tag}.jsonl"))
}

fn verzeichnis(daten: &Path) -> io::Result<()> {
    let info = fs::symlink_metadata(ordner(daten))?;
    if info.is_dir() {
        Ok(())
    } else {
        Err(io::Error::other(
            "Gesprächsordner ist kein reguläres Verzeichnis",
        ))
    }
}

/// Der Aufrufer prüft Gesprächsinhalte vor dem Speichern auf Zugangsdaten.
pub fn anhaengen(daten: &Path, e: &Gespraech) -> io::Result<()> {
    let (tag, ..) = e
        .zeit
        .in_frankfurt()
        .ok_or_else(|| io::Error::other("Datum fehlt"))?;
    match fs::create_dir(ordner(daten)) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => verzeichnis(daten)?,
        Err(e) => return Err(e),
    }
    let _sperre = sperre_schreiber(daten)?;
    let pfad = datei(daten, tag);
    if let Ok(info) = fs::symlink_metadata(&pfad)
        && !info.is_file()
    {
        return Err(io::Error::other("Gesprächsdatei ist keine reguläre Datei"));
    }
    let zeile = e.zeile();
    if zeile.len() > MAX_ZEILE {
        return Err(io::Error::other("Gesprächseintrag zu groß"));
    }
    let mut f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(pfad)?;
    // Ein vorheriger abgebrochener Schreibvorgang darf die nächste Zeile nicht verschlucken.
    let bisher = vollstaendiger_anhang(&mut f)?;
    let zeile = format!("{zeile}\n");
    let groesse = u64::try_from(zeile.len()).map_err(io::Error::other)?;
    if bisher.saturating_add(groesse) > MAX_DATEI {
        return Err(io::Error::other("Gesprächsdatei zu groß"));
    }
    f.write_all(zeile.as_bytes())
}

struct Schreibsperre(File);

impl Drop for Schreibsperre {
    fn drop(&mut self) {
        // Ein parallel gestarteter Prozess kann die Dateireferenz bis zum exec behalten.
        let _ = self.0.unlock();
    }
}

fn sperre_schreiber(daten: &Path) -> io::Result<Schreibsperre> {
    let pfad = ordner(daten).join(SCHREIBSPERRE);
    if let Ok(info) = fs::symlink_metadata(&pfad)
        && !info.is_file()
    {
        return Err(io::Error::other("Gesprächssperre ist keine reguläre Datei"));
    }
    let sperrdatei = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(pfad)?;
    sperrdatei.try_lock().map_err(io::Error::from)?;
    Ok(Schreibsperre(sperrdatei))
}

fn vollstaendige_laenge(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(0, |i| i.saturating_add(1))
}

fn vollstaendiger_anhang(f: &mut File) -> io::Result<u64> {
    let groesse = f.metadata()?.len();
    if groesse > MAX_DATEI {
        return Err(io::Error::other("Gesprächsdatei zu groß"));
    }
    if groesse == 0 {
        return Ok(0);
    }
    f.seek(SeekFrom::End(-1))?;
    let mut ende = [0];
    f.read_exact(&mut ende)?;
    if ende == *b"\n" {
        return Ok(groesse);
    }
    f.rewind()?;
    let mut bytes = Vec::new();
    f.take(MAX_DATEI).read_to_end(&mut bytes)?;
    let laenge = u64::try_from(vollstaendige_laenge(&bytes)).map_err(io::Error::other)?;
    f.set_len(laenge)?;
    f.seek(SeekFrom::Start(laenge))?;
    Ok(laenge)
}

/// Beträge in Buchungsreihenfolge aus dem bestätigten KI-Kassenbuch.
pub fn gebuchte_kosten(daten: &Path) -> Result<Vec<(BeraterId, Cent)>, crate::JournalFehler> {
    crate::Journal::pruefe_lesend(&daten.join("journal.txt"), |stand| {
        stand
            .ki()
            .buchungen()
            .iter()
            .map(|b| (b.berater, b.reserviert))
            .collect()
    })
}

pub fn lesen(daten: &Path, tag: Datum) -> io::Result<(Vec<Gespraech>, usize)> {
    let oeffnen = || {
        verzeichnis(daten)?;
        let pfad = datei(daten, tag);
        if !fs::symlink_metadata(&pfad)?.is_file() {
            return Err(io::Error::other("Gesprächsdatei ist keine reguläre Datei"));
        }
        File::open(pfad)
    };
    let handle = match oeffnen() {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
        Err(e) => return Err(e),
    };
    let mut bytes = Vec::new();
    handle.take(MAX_DATEI).read_to_end(&mut bytes)?;
    bytes.truncate(vollstaendige_laenge(&bytes));
    let mut eintraege = Vec::new();
    let mut fehler = 0_usize;
    for zeile in bytes.split(|b| *b == b'\n').filter(|z| !z.is_empty()) {
        match std::str::from_utf8(zeile).ok().and_then(Gespraech::lies) {
            Some(e) if e.zeit.in_frankfurt().is_some_and(|(d, ..)| d == tag) => eintraege.push(e),
            _ => fehler = fehler.saturating_add(1),
        }
    }
    Ok((eintraege, fehler))
}

pub fn aufraeumen(daten: &Path, heute: Datum) -> io::Result<()> {
    match verzeichnis(daten) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        r => r?,
    }
    let grenze = heute
        .plus_tage(-90)
        .ok_or_else(|| io::Error::other("Datum fehlt"))?;
    let mut fehler = None;
    for eintrag in fs::read_dir(ordner(daten))? {
        let eintrag = eintrag?;
        let name = eintrag.file_name();
        let tag = name
            .to_str()
            .and_then(|n| n.strip_suffix(".jsonl"))
            .and_then(Datum::lies);
        if tag.is_some_and(|d| d < grenze)
            && eintrag.file_type()?.is_file()
            && let Err(e) = fs::remove_file(eintrag.path())
        {
            fehler = Some(e);
        }
    }
    fehler.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nur_aeltere_tagesdateien_werden_aufgeraeumt() -> io::Result<()> {
        let daten = std::env::temp_dir().join(format!("depot_gespraeche_{}", std::process::id()));
        fs::create_dir_all(ordner(&daten))?;
        let ergebnis = (|| {
            let heute = Datum::lies("2026-11-10").ok_or_else(|| io::Error::other("Datum fehlt"))?;
            let alt = heute
                .plus_tage(-91)
                .ok_or_else(|| io::Error::other("Datum fehlt"))?;
            let neu = heute
                .plus_tage(-89)
                .ok_or_else(|| io::Error::other("Datum fehlt"))?;
            let fremd = ordner(&daten).join("behalten.jsonl");
            let sperre = ordner(&daten).join(SCHREIBSPERRE);
            for pfad in [
                datei(&daten, alt),
                datei(&daten, neu),
                fremd.clone(),
                sperre.clone(),
            ] {
                fs::write(pfad, "")?;
            }
            aufraeumen(&daten, heute)?;
            assert!(!datei(&daten, alt).exists());
            assert!(datei(&daten, neu).is_file() && fremd.is_file() && sperre.is_file());
            Ok(())
        })();
        fs::remove_dir_all(daten)?;
        ergebnis
    }
    #[test]
    fn lesen_waehrend_schreibsperre_und_unfertiger_zeile() -> io::Result<()> {
        let daten =
            std::env::temp_dir().join(format!("depot_gespraeche_sperre_{}", std::process::id()));
        fs::create_dir_all(&daten)?;
        let ergebnis = (|| {
            let zeit = Zeitpunkt::lies("2026-11-10T10:00:00Z")
                .ok_or_else(|| io::Error::other("Zeit fehlt"))?;
            let (tag, ..) = zeit
                .in_frankfurt()
                .ok_or_else(|| io::Error::other("Tag fehlt"))?;
            let eintrag = Gespraech {
                zeit,
                lauf: "probe".into(),
                wer: "dachs".into(),
                rolle: "haupt".into(),
                modell: "probe".into(),
                zweck: "erstmeinung".into(),
                ergebnis: "ok".into(),
                http: None,
                token_ein: None,
                token_aus: None,
                buchung: Some(7),
                kosten_cent: Some(3),
                system: None,
                frage: None,
                antwort: None,
                fehler: None,
            };
            let alt = eintrag.zeile().replace("\"buchung\":7,", "");
            assert!(Gespraech::lies(&alt).is_some_and(|e| e.buchung.is_none()));
            anhaengen(&daten, &eintrag)?;
            let sperre = sperre_schreiber(&daten)?;
            let mut f = OpenOptions::new().append(true).open(datei(&daten, tag))?;
            f.write_all(eintrag.zeile().as_bytes())?;
            assert_eq!(lesen(&daten, tag)?, (vec![eintrag.clone()], 0));
            assert!(
                anhaengen(&daten, &eintrag).is_err_and(|e| e.kind() == io::ErrorKind::WouldBlock)
            );
            drop(f);
            drop(sperre);
            anhaengen(&daten, &eintrag)?;
            assert_eq!(lesen(&daten, tag)?, (vec![eintrag.clone(), eintrag], 0));
            Ok(())
        })();
        fs::remove_dir_all(daten)?;
        ergebnis
    }
}
