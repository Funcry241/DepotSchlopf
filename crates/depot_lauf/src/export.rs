//! Assistenten-Export nach EtuSchlopf-Vorbild: Nach jedem Lauf entsteht im Projektordner
//! `assistant_export_latest.zip`. Diese eine Datei reicht, um eine neue KI-Sitzung ohne
//! Vorgeschichte sicher weiterarbeiten zu lassen (Essenz D17): Laufergebnis, Konsole,
//! Uebergabetext, Festschreibung und ein vollstaendiger Quellstand.
//!
//! Nie enthalten: Laufzeitdaten (`daten/`), Zugangsdaten, Build-Ordner. Der Quellstand nimmt
//! nur Dateien, die Git verfolgt, mit ihrem Inhalt aus dem Git-Index; im Arbeitsbaum liest der
//! Export nichts, also auch keinen Link. Jeder Inhalt wird auf Geheimnismuster geprueft; ein
//! Treffer bricht den Export ab (R28).

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::abbild::Abbild;
use crate::zip::Zip;

pub const EXPORT: &str = "assistant_export_latest.zip";

#[derive(Debug)]
pub struct Schritt {
    pub name: String,
    pub exit: i32,
}

#[derive(Debug)]
pub struct Lauf {
    pub befehl: String,
    pub schritte: Vec<Schritt>,
    pub konsole: String,
    pub warnungen: Vec<String>,
    /// Der Stand vor der Pruefung; aus ihm kommt der Quellstand (1·R04).
    pub abbild: Option<crate::abbild::Abbild>,
}

impl Lauf {
    pub fn erfolgreich(&self) -> bool {
        self.schritte.iter().all(|s| s.exit == 0)
    }
}

fn sha_hex(daten: &[u8]) -> String {
    Sha256::digest(daten)
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn git(wurzel: &Path, args: &[&str]) -> Option<String> {
    let aus = Command::new("git")
        .arg("-C")
        .arg(wurzel)
        .args(args)
        .output()
        .ok()?;
    aus.status
        .success()
        .then(|| String::from_utf8_lossy(&aus.stdout).trim().to_string())
}

/// Dateien, die nie in einen Export gehoeren.
pub(crate) fn geheim_oder_erzeugt(pfad: &str) -> bool {
    let name = pfad.rsplit('/').next().unwrap_or(pfad);
    pfad.starts_with("target/")
        || pfad.starts_with(".git/")
        || pfad.starts_with("daten/")
        || name.starts_with("assistant_export")
        || name == ".env"
        || name.starts_with(".env.")
        || [".key", ".pem", ".p12", ".sperre"]
            .iter()
            .any(|e| name.ends_with(e))
}

/// Hat das Projekt ein Git-Repository? Nur ein fehlendes `.git` heisst „ohne Git“; jeder
/// andere Zustand wird gelesen, und ein Fehler dabei ist ein Fehler (K11-02).
pub(crate) fn hat_git(wurzel: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(wurzel.join(".git")) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!(".git: nicht lesbar: {e}")),
    }
}

/// Quellstand (R28): die gewoehnlichen Dateien aus dem Abbild vor der Pruefung, mit dem
/// Inhalt ihrer Blobs, nie aus dem Arbeitsbaum. Ein Blob aendert sich nie; der Quellstand ist
/// also byte-gleich mit dem Index, den der Lauf vor der Pruefung festgehalten hat (1·R04).
/// Neue, nicht verfolgte Dateien bleiben draussen (1·E01). Verfolgte Links (Modus 120000)
/// fehlen im Abbild, und im Arbeitsbaum wird nichts gelesen; ein Link in irgendeinem Pfadteil
/// kann daher nichts einschleusen (1·E02, K11-01, K11-03). Ohne Git gibt es keinen
/// Quellstand; ein Repository ohne lesbares Abbild ist ein Fehler (K11-02).
fn quellstand(wurzel: &Path, abbild: Option<&Abbild>) -> Result<Vec<(String, Vec<u8>)>, String> {
    if !hat_git(wurzel)? {
        return Ok(Vec::new());
    }
    let abbild = abbild.ok_or("Git-Index nicht lesbar: kein Abbild vor der Pruefung")?;
    abbild
        .dateien
        .iter()
        .map(|(pfad, blob)| {
            let inhalt = depot_kodex::git::blob(wurzel, blob)
                .map_err(|e| format!("{pfad} (Index): nicht lesbar: {e}"))?;
            Ok((pfad.clone(), inhalt))
        })
        .collect()
}

/// Eine Datei des Quellstands; `None`, wenn es sie dort nicht gibt.
fn im_quellstand<'a>(dateien: &'a [(String, Vec<u8>)], pfad: &str) -> Option<&'a [u8]> {
    dateien
        .iter()
        .find(|(p, _)| p == pfad)
        .map(|(_, i)| i.as_slice())
}

/// Nur die Pfade des Quellstands, fuer Tests.
#[cfg(test)]
fn quelldateien(wurzel: &Path) -> Result<Vec<String>, String> {
    let abbild = crate::abbild::aufnehmen(wurzel)?;
    Ok(quellstand(wurzel, abbild.as_ref())?
        .into_iter()
        .map(|(p, _)| p)
        .collect())
}

fn json_text(s: &str) -> String {
    let mut aus = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => aus.push_str("\\\""),
            '\\' => aus.push_str("\\\\"),
            '\n' => aus.push_str("\\n"),
            '\r' => aus.push_str("\\r"),
            '\t' => aus.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(aus, "\\u{:04x}", u32::from(c));
            }
            c => aus.push(c),
        }
    }
    aus.push('"');
    aus
}

/// UTC-Zeitstempel `JJJJ-MM-TTThh:mm:ssZ` aus Sekunden seit 1970.
pub fn utc(sek: u64) -> String {
    let tag_sek = sek.checked_rem(86_400).unwrap_or(0);
    let mut tage = sek.checked_div(86_400).unwrap_or(0);
    let mut jahr: u64 = 1970;
    loop {
        let schalt =
            (jahr.is_multiple_of(4) && !jahr.is_multiple_of(100)) || jahr.is_multiple_of(400);
        let laenge = if schalt { 366 } else { 365 };
        if tage < laenge {
            let feb = if schalt { 29 } else { 28 };
            let mut monat: u64 = 1;
            for m in [31, feb, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31] {
                if tage < m {
                    break;
                }
                tage = tage.saturating_sub(m);
                monat = monat.saturating_add(1);
            }
            return format!(
                "{jahr:04}-{monat:02}-{:02}T{:02}:{:02}:{:02}Z",
                tage.saturating_add(1),
                tag_sek.checked_div(3600).unwrap_or(0),
                tag_sek
                    .checked_rem(3600)
                    .and_then(|r| r.checked_div(60))
                    .unwrap_or(0),
                tag_sek.checked_rem(60).unwrap_or(0)
            );
        }
        tage = tage.saturating_sub(laenge);
        jahr = jahr.saturating_add(1);
    }
}

/// Summe aller `test result:`-Zeilen: (bestanden, fehlgeschlagen).
pub fn testsumme(konsole: &str) -> (u64, u64) {
    let mut summe = (0_u64, 0_u64);
    for zeile in konsole.lines().filter(|z| z.contains("test result:")) {
        let woerter: Vec<&str> = zeile.split_whitespace().collect();
        for paar in woerter.windows(2) {
            if let [zahl, wort] = paar {
                let n: u64 = zahl.parse().unwrap_or(0);
                if wort.starts_with("passed") {
                    summe.0 = summe.0.saturating_add(n);
                } else if wort.starts_with("failed") {
                    summe.1 = summe.1.saturating_add(n);
                }
            }
        }
    }
    summe
}

/// Schreibt den Export atomar (erst `.tmp`, dann umbenennen).
///
/// # Errors
/// Lese- oder Schreibfehler.
#[allow(
    clippy::too_many_lines,
    reason = "der Export liest sich als eine Folge von Abschnitten am besten"
)]
pub fn schreibe(wurzel: &Path, lauf: &Lauf) -> Result<PathBuf, String> {
    let zeit = utc(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()));
    let version = env!("CARGO_PKG_VERSION");
    let commit = git(wurzel, &["rev-parse", "HEAD"]).unwrap_or_else(|| "kein-git".into());
    let zweig = git(wurzel, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|| "-".into());
    let veraendert = git(wurzel, &["status", "--porcelain"]).is_some_and(|s| !s.is_empty());
    let (bestanden, fehlgeschlagen) = testsumme(&lauf.konsole);
    let gate = if lauf.erfolgreich() {
        "offen"
    } else {
        "geschlossen"
    };
    let festschreibung = depot_kodex::festschreibung::berechne(wurzel).unwrap_or_default();
    let dateien = quellstand(wurzel, lauf.abbild.as_ref())?;
    let geprueft = lauf.abbild.is_some()
        && lauf.erfolgreich()
        && lauf.schritte.iter().any(|s| s.name == "stand");
    let essenz = match im_quellstand(&dateien, depot_kodex::essenz::ESSENZ) {
        Some(b) => String::from_utf8(b.to_vec())
            .map_err(|e| format!("{}: kein UTF-8: {e}", depot_kodex::essenz::ESSENZ))?,
        None => String::new(),
    };
    let regeln = depot_kodex::essenz::regeln(&essenz);
    let kurz: String = commit.chars().take(7).collect();

    // Quellstand als eigenes inneres ZIP.
    let mut quelle = Zip::default();
    let mut geheim = Vec::new();
    for (d, inhalt) in &dateien {
        geheim.extend(depot_kodex::geheimnis::pruefe_inhalt(d, inhalt));
        quelle.datei(d, inhalt)?;
    }
    let quelle = quelle.fertig()?;
    let quelle_name = format!(
        "source_snapshot/DepotSchlopf_v{}_{kurz}_quelle.zip",
        version.replace('.', "_")
    );
    let snapshot_txt = format!(
        "Quellstand DepotSchlopf v{version}\nCommit: {commit}\nZweig: {zweig}\nLokal veraendert: {veraendert}\n\
         Dateien: {}\nSHA-256: {}\nInhalt aus dem Git-Index vor der Pruefung, nie aus dem Arbeitsbaum; \
         auf Geheimnisse geprueft. Byte-gleich mit dem geprueften Stand: {}. Ohne Git kein Quellstand. Zum Weiterarbeiten entpacken.\n",
        dateien.len(),
        sha_hex(&quelle),
        if geprueft { "ja" } else { "nein" }
    );

    let mut j = String::from("{\n");
    let mut feld = |k: &str, v: String| {
        let _ = writeln!(j, "  {}: {v},", json_text(k));
    };
    feld("schema_version", "1".into());
    feld("zip_kind", json_text("assistant_export"));
    feld("projekt", json_text("DepotSchlopf"));
    feld("version", json_text(version));
    feld("zeit_utc", json_text(&zeit));
    feld("befehl", json_text(&lauf.befehl));
    feld("git_commit", json_text(&commit));
    feld("git_zweig", json_text(&zweig));
    feld("lokal_veraendert", veraendert.to_string());
    feld("gate", json_text(gate));
    feld("diff_geprueft", wurzel.join(".git").exists().to_string());
    feld("tests_bestanden", bestanden.to_string());
    feld("tests_fehlgeschlagen", fehlgeschlagen.to_string());
    feld(
        "schritte",
        format!(
            "[{}]",
            lauf.schritte
                .iter()
                .map(|s| format!("{{\"name\": {}, \"exit\": {}}}", json_text(&s.name), s.exit))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );
    feld(
        "warnungen",
        format!(
            "[{}]",
            lauf.warnungen
                .iter()
                .map(|w| json_text(w))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );
    feld("essenz_regeln", regeln.len().to_string());
    feld("quelldateien", dateien.len().to_string());
    feld("quellstand_geprueft", geprueft.to_string());
    let fs_json = festschreibung
        .iter()
        .map(|(k, v)| format!("\"{k}\": {}", json_text(v)))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(j, "  \"festschreibung\": {{{fs_json}}}\n}}");

    let mut h = format!(
        "DEPOTSCHLOPF – ASSISTENTEN-UEBERGABE\n\
         ====================================\n\n\
         Essenz lenkt. KI schlaegt vor. Regelwerk entscheidet. Auftragsstelle schreibt.\n\
         Journal belegt. Abgleich prueft. Kodex wacht.\n\n\
         Diese ZIP-Datei entsteht nach jedem Lauf von run.cmd. Sie ist fuer eine neue\n\
         KI-Sitzung gedacht: hochladen, dann diese Datei lesen. Sie ist kein Quellstand\n\
         zum Einspielen; den Quellstand enthaelt {quelle_name}.\n\n\
         STAND\n-----\nZeit (UTC): {zeit}\nVersion: {version}\nCommit: {commit} ({zweig})\n\
         Lokal veraendert: {veraendert}\nBefehl: {}\nGate: {gate}\n\
         Tests: {bestanden} bestanden, {fehlgeschlagen} fehlgeschlagen\n",
        lauf.befehl
    );
    for s in &lauf.schritte {
        let _ = writeln!(h, "Schritt {}: Exit {}", s.name, s.exit);
    }
    for w in &lauf.warnungen {
        let _ = writeln!(h, "WARNUNG: {w}");
    }
    h.push_str(
        "\nKALTSTART-REIHENFOLGE (aus dem Quellstand)\n------------------------------------------\n\
         1. docs/kaltstart.md\n2. docs/essenz.md (beratend)\n3. kodex/festschreibung.txt (nie ohne Freigabe aendern)\n\
         4. kodex/aenderung.txt (fuer jede Aenderung neu schreiben)\n5. README.md, CHANGELOG.md\n\n\
         ESSENZ-REGELN\n-------------\n",
    );
    for (nr, titel, _) in &regeln {
        let _ = writeln!(h, "D{nr:02} {titel}");
    }
    h.push_str("\nFESTSCHREIBUNG IM BAUM\n----------------------\n");
    for (k, v) in &festschreibung {
        let _ = writeln!(h, "{k}: {v}");
    }
    h.push_str(
        "\nHINWEIS\n-------\nMassgeblich ist immer der aktuelle Quellstand und das Gate, nie diese Uebergabe\n\
         (Essenz D00). Die Konsole steht in depotschlopf.console.txt.\n",
    );

    let zip_kind = format!(
        "assistant_export\nDepotSchlopf v{version}\nCommit {commit}\nKein Quellstand zum Einspielen.\n"
    );

    let quelle_pfad = quelle_name.clone();
    let teile: Vec<(String, Vec<u8>)> = vec![
        ("ZIP_KIND.txt".into(), zip_kind.into_bytes()),
        ("assistant_context.json".into(), j.into_bytes()),
        ("assistant_handoff_latest.txt".into(), h.into_bytes()),
        (
            "depotschlopf.console.txt".into(),
            lauf.konsole.clone().into_bytes(),
        ),
        (
            "source_snapshot/SOURCE_SNAPSHOT.txt".into(),
            snapshot_txt.into_bytes(),
        ),
        (quelle_name, quelle),
    ];
    // Jeder Inhalt wird vor dem Schreiben geprueft; ein Treffer bricht den Export ab, und es
    // entsteht keine ZIP-Datei (R28). Die Meldung nennt das Geheimnis nie.
    // Der innere Quellstand ist oben Datei fuer Datei geprueft.
    for (name, inhalt) in &teile {
        if *name != quelle_pfad {
            geheim.extend(depot_kodex::geheimnis::pruefe_inhalt(name, inhalt));
        }
    }
    if !geheim.is_empty() {
        return Err(format!(
            "Export abgebrochen, keine ZIP-Datei geschrieben:\n{}",
            geheim.join("\n")
        ));
    }
    let mut manifest = String::from("# Datei\tBytes\tSHA-256\n");
    let mut zip = Zip::default();
    for (name, inhalt) in &teile {
        let _ = writeln!(manifest, "{name}\t{}\t{}", inhalt.len(), sha_hex(inhalt));
        zip.datei(name, inhalt)?;
    }
    zip.datei("manifest.txt", manifest.as_bytes())?;
    let bytes = zip.fertig()?;

    let ziel = wurzel.join(EXPORT);
    let tmp = wurzel.join(format!("{EXPORT}.tmp"));
    fs::write(&tmp, &bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, &ziel).map_err(|e| format!("{}: {e}", ziel.display()))?;
    Ok(ziel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_bekannte_zeitpunkte() {
        assert_eq!(utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(utc(1_793_536_245), "2026-11-01T12:30:45Z");
    }

    #[test]
    fn testsumme_zaehlt() {
        let k = "test result: ok. 7 passed; 0 failed; 0 ignored\ntest result: FAILED. 3 passed; 2 failed;";
        assert_eq!(testsumme(k), (10, 2));
    }

    #[test]
    fn geheimes_bleibt_draussen() {
        for p in [
            "daten/kassenbuch.txt",
            ".env",
            "x/y.key",
            "target/a",
            "assistant_export_latest.zip",
        ] {
            assert!(geheim_oder_erzeugt(p), "{p}");
        }
        assert!(!geheim_oder_erzeugt("crates/depot_ki/src/budget.rs"));
    }

    // -----------------------------------------------------------------------------------------
    // 1·E01, 1·E02, 2·W3 (Export-Teil) – nur Git, keine Links, keine Geheimnisse (R28)
    // -----------------------------------------------------------------------------------------

    use std::sync::atomic::{AtomicU32, Ordering};

    static NR: AtomicU32 = AtomicU32::new(0);

    /// Ein frisches Git-Repository unter `target/export-befunde/`.
    fn repo() -> Result<PathBuf, String> {
        let n = NR.fetch_add(1, Ordering::SeqCst);
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/export-befunde")
            .join(format!("repo_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        git_muss(&p, &["init", "-q"])?;
        Ok(p)
    }

    fn git_muss(dir: &Path, args: &[&str]) -> Result<(), String> {
        let aus = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=Export-Probe",
                "-c",
                "user.email=probe@depotschlopf.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .map_err(|e| format!("git: {e}"))?;
        if aus.status.success() {
            Ok(())
        } else {
            Err(format!(
                "git {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&aus.stderr)
            ))
        }
    }

    fn schreib(dir: &Path, rel: &str, inhalt: &[u8]) -> Result<(), String> {
        let z = dir.join(rel);
        if let Some(e) = z.parent() {
            fs::create_dir_all(e).map_err(|e| format!("{rel}: {e}"))?;
        }
        fs::write(z, inhalt).map_err(|e| format!("{rel}: {e}"))
    }

    fn lauf(konsole: &str) -> Lauf {
        Lauf {
            befehl: "alles".into(),
            schritte: vec![Schritt {
                name: "kodex".into(),
                exit: 0,
            }],
            konsole: konsole.into(),
            warnungen: Vec::new(),
            abbild: None,
        }
    }

    /// Ein Lauf mit dem Abbild des Repositorys; ist es nicht lesbar, ohne Abbild.
    fn lauf_in(r: &Path, konsole: &str) -> Lauf {
        Lauf {
            abbild: crate::abbild::aufnehmen(r).ok().flatten(),
            ..lauf(konsole)
        }
    }

    /// Erfundene Beispiele, erst zur Laufzeit zusammengesetzt.
    fn beispiel_iban() -> String {
        ["DE89", "3704", "0044", "0532", "0130", "00"].concat()
    }

    fn beispiel_schluessel() -> String {
        ["sk", "-ant-", "api03-", "q7Rk2LmZx9Tb4VwN8yHc3PdJ6sFg1Ue5"].concat()
    }

    /// Exportiert wird nur, was Git verfolgt. Eine neue, nicht ignorierte Datei im
    /// Projektordner bleibt draussen, auch wenn sie harmlos aussieht.
    #[test]
    fn befund_1_e01_nur_verfolgte_dateien_werden_exportiert() -> Result<(), String> {
        let r = repo()?;
        schreib(&r, "README.md", b"# Probe\n")?;
        git_muss(&r, &["add", "README.md"])?;
        git_muss(&r, &["commit", "-q", "-m", "basis"])?;
        schreib(&r, "zugang.txt", b"Benutzer: wolfgang\n")?;
        schreib(&r, "notizen/konto.md", b"privat\n")?;
        let dateien = quelldateien(&r)?;
        assert_eq!(dateien, vec!["README.md".to_string()]);
        Ok(())
    }

    /// Ein Link wird nicht gelesen: Sein Ziel kann ausserhalb des Projekts liegen. Das gilt fuer
    /// einen verfolgten Link und fuer eine verfolgte Datei, die im Arbeitsbaum ein Link ist; von
    /// ihr kommt der Inhalt aus dem Index.
    #[cfg(unix)]
    #[test]
    fn befund_1_e02_link_wird_nicht_gelesen() -> Result<(), String> {
        let r = repo()?;
        let draussen = r.with_extension("draussen");
        fs::create_dir_all(&draussen).map_err(|e| format!("draussen: {e}"))?;
        schreib(&draussen, "fremd.txt", b"fremder Inhalt\n")?;
        schreib(&r, "README.md", b"# Probe\n")?;
        std::os::unix::fs::symlink(draussen.join("fremd.txt"), r.join("link.md"))
            .map_err(|e| format!("Link: {e}"))?;
        schreib(&r, "ersetzt.md", b"harmlos\n")?;
        git_muss(&r, &["add", "README.md", "link.md", "ersetzt.md"])?;
        git_muss(&r, &["commit", "-q", "-m", "basis"])?;
        // Eine verfolgte Datei, im Arbeitsbaum durch einen Link ersetzt.
        fs::remove_file(r.join("ersetzt.md")).map_err(|e| format!("ersetzt: {e}"))?;
        std::os::unix::fs::symlink(draussen.join("fremd.txt"), r.join("ersetzt.md"))
            .map_err(|e| format!("Link: {e}"))?;
        let dateien = quelldateien(&r)?;
        assert!(
            !dateien.contains(&"link.md".to_string()),
            "verfolgter Link exportiert: {dateien:?}"
        );
        assert!(dateien.contains(&"README.md".to_string()), "{dateien:?}");
        let p = schreibe(&r, &lauf_in(&r, "sauber\n"))?;
        let bytes = fs::read(p).map_err(|e| e.to_string())?;
        assert!(
            !enthaelt(&bytes, b"fremder Inhalt"),
            "Linkziel gelesen: fremder Inhalt steht in der ZIP-Datei"
        );
        Ok(())
    }

    /// Ein Geheimnismuster in einer exportierten Datei oder in der Konsole bricht den Export ab:
    /// Es entsteht keine ZIP-Datei, und die Meldung nennt das Geheimnis nicht. Ebenso ein
    /// Inhalt, der sich nicht pruefen laesst.
    #[test]
    fn befund_1_e01_geheimnis_bricht_den_export_ab() -> Result<(), String> {
        let faelle: [(&str, Vec<u8>, String, &str); 4] = [
            (
                "IBAN in verfolgter Datei",
                format!("Konto {}\n", beispiel_iban()).into_bytes(),
                String::new(),
                "Geheimnismuster",
            ),
            (
                "Schluessel in verfolgter Datei",
                format!("key {}\n", beispiel_schluessel()).into_bytes(),
                String::new(),
                "Geheimnismuster",
            ),
            (
                "Schluessel in der Konsole",
                b"harmlos\n".to_vec(),
                format!("Ausgabe {}\n", beispiel_schluessel()),
                "Geheimnismuster",
            ),
            (
                "nicht pruefbarer Inhalt",
                vec![b'#', b' ', 0xC3, 0x28, 0xFF, b'\n'],
                String::new(),
                "nicht pruefbar",
            ),
        ];
        let mut offen = Vec::new();
        for (fall, inhalt, konsole, stichwort) in faelle {
            let r = repo()?;
            schreib(&r, "docs/notiz.md", &inhalt)?;
            git_muss(&r, &["add", "docs/notiz.md"])?;
            git_muss(&r, &["commit", "-q", "-m", "basis"])?;
            match schreibe(&r, &lauf_in(&r, &konsole)) {
                Ok(p) => offen.push(format!("{fall}: Export geschrieben: {}", p.display())),
                Err(e) => {
                    if !e.contains(stichwort) {
                        offen.push(format!("{fall}: falscher Grund: {e}"));
                    }
                    if e.contains(&beispiel_iban()) || e.contains(&beispiel_schluessel()) {
                        offen.push(format!("{fall}: Meldung nennt das Geheimnis"));
                    }
                }
            }
            if r.join(EXPORT).exists() || r.join(format!("{EXPORT}.tmp")).exists() {
                offen.push(format!("{fall}: ZIP-Datei liegt im Ordner"));
            }
        }
        assert!(offen.is_empty(), "{offen:#?}");
        Ok(())
    }

    fn enthaelt(heuhaufen: &[u8], nadel: &[u8]) -> bool {
        heuhaufen.windows(nadel.len()).any(|w| w == nadel)
    }

    /// K11-01: Ein Link in einem Elternordner wird so wenig gelesen wie ein Link als Datei.
    /// Das gilt fuer den Quellstand und fuer die gesonderten Lesewege, etwa die Essenz.
    /// Fremder Inhalt darf in keinem Teil der ZIP-Datei stehen.
    #[cfg(unix)]
    #[test]
    fn befund_k11_01_link_im_elternordner_wird_nicht_gelesen() -> Result<(), String> {
        let r = repo()?;
        schreib(&r, "unterordner/notiz.md", b"intern\n")?;
        schreib(&r, "docs/essenz.md", b"# D01 \xE2\x80\x93 Eigene Regel\n")?;
        git_muss(&r, &["add", "."])?;
        git_muss(&r, &["commit", "-q", "-m", "basis"])?;
        let draussen = r.with_extension("fremd");
        let _ = fs::remove_dir_all(&draussen);
        schreib(&draussen, "notiz.md", b"FREMDER_INHALT_AUSSERHALB\n")?;
        schreib(
            &draussen,
            "essenz.md",
            b"# D01 \xE2\x80\x93 FREMDE_REGEL_AUSSERHALB\n",
        )?;
        for ordner in ["unterordner", "docs"] {
            fs::remove_dir_all(r.join(ordner)).map_err(|e| format!("{ordner}: {e}"))?;
            std::os::unix::fs::symlink(&draussen, r.join(ordner))
                .map_err(|e| format!("Link {ordner}: {e}"))?;
        }
        let p = schreibe(&r, &lauf_in(&r, "sauber\n"))?;
        let bytes = fs::read(p).map_err(|e| e.to_string())?;
        let mut offen = Vec::new();
        for marke in ["FREMDER_INHALT_AUSSERHALB", "FREMDE_REGEL_AUSSERHALB"] {
            if enthaelt(&bytes, marke.as_bytes()) {
                offen.push(format!(
                    "Link im Elternordner gelesen: {marke} steht in der ZIP-Datei"
                ));
            }
        }
        assert!(offen.is_empty(), "{offen:#?}");
        Ok(())
    }

    /// K11-02: Ein vorhandenes, aber nicht lesbares Repository ist ein Fehler, kein leerer
    /// Quellstand. Der Export bricht ab, bevor eine Datei entsteht.
    #[test]
    fn befund_k11_02_kaputter_index_bricht_den_export_ab() -> Result<(), String> {
        let r = repo()?;
        schreib(&r, "README.md", b"sauber\n")?;
        git_muss(&r, &["add", "."])?;
        git_muss(&r, &["commit", "-q", "-m", "basis"])?;
        fs::write(r.join(".git/index"), b"kaputt").map_err(|e| e.to_string())?;
        let mut offen = Vec::new();
        if let Ok(p) = schreibe(&r, &lauf_in(&r, "sauber\n")) {
            offen.push(format!(
                "Defekter Git-Index: Export meldet Erfolg: {}",
                p.display()
            ));
        }
        if r.join(EXPORT).exists() || r.join(format!("{EXPORT}.tmp")).exists() {
            offen.push("Defekter Git-Index: ZIP-Datei liegt im Ordner".to_string());
        }
        assert!(offen.is_empty(), "{offen:#?}");
        Ok(())
    }

    /// K11-03: Was im Arbeitsbaum geschieht, waehrend oder nachdem geprueft wurde, darf den
    /// Export nicht erreichen. Der Export nimmt den Inhalt aus dem Git-Index, den auch das Gate
    /// prueft. Eine gleich lange, andere Datei im Arbeitsbaum steht daher nicht in der ZIP-Datei,
    /// ebenso wenig eine geaenderte Essenz. Das gilt auf jeder Plattform, ohne Dateiidentitaet.
    #[test]
    fn befund_k11_03_export_nimmt_den_index_nicht_den_arbeitsbaum() -> Result<(), String> {
        let r = repo()?;
        schreib(&r, "pruef/datei.md", b"INTERN\n")?;
        schreib(&r, "docs/essenz.md", b"# D01 \xE2\x80\x93 INDEXREGEL\n")?;
        git_muss(&r, &["add", "."])?;
        git_muss(&r, &["commit", "-q", "-m", "basis"])?;
        schreib(&r, "pruef/datei.md", b"FREMD!\n")?;
        schreib(&r, "docs/essenz.md", b"# D01 \xE2\x80\x93 FREMDREGEL\n")?;
        let p = schreibe(&r, &lauf_in(&r, "sauber\n"))?;
        let bytes = fs::read(p).map_err(|e| e.to_string())?;
        let mut offen = Vec::new();
        for marke in ["FREMD!", "FREMDREGEL"] {
            if enthaelt(&bytes, marke.as_bytes()) {
                offen.push(format!("Arbeitsbaum statt Index exportiert: {marke}"));
            }
        }
        for marke in ["INTERN", "INDEXREGEL"] {
            if !enthaelt(&bytes, marke.as_bytes()) {
                offen.push(format!("Indexinhalt fehlt: {marke}"));
            }
        }
        assert!(offen.is_empty(), "{offen:#?}");
        Ok(())
    }

    /// K11-03, Lesefehler: Fehlt ein Blob des Index im Repository, bricht der Export ab,
    /// ohne eine ZIP-Datei zu schreiben.
    #[test]
    fn befund_k11_03_fehlender_blob_bricht_den_export_ab() -> Result<(), String> {
        let r = repo()?;
        schreib(&r, "README.md", b"blob-probe\n")?;
        git_muss(&r, &["add", "README.md"])?;
        let aus = Command::new("git")
            .arg("-C")
            .arg(&r)
            .args(["rev-parse", ":README.md"])
            .output()
            .map_err(|e| e.to_string())?;
        let sha = String::from_utf8_lossy(&aus.stdout).trim().to_string();
        let (kopf, rest) = sha.split_at_checked(2).ok_or("kein Blob")?;
        fs::remove_file(r.join(".git/objects").join(kopf).join(rest))
            .map_err(|e| format!("Blob {sha}: {e}"))?;
        let mut offen = Vec::new();
        if let Ok(p) = schreibe(&r, &lauf_in(&r, "sauber\n")) {
            offen.push(format!(
                "Fehlender Blob: Export meldet Erfolg: {}",
                p.display()
            ));
        }
        if r.join(EXPORT).exists() || r.join(format!("{EXPORT}.tmp")).exists() {
            offen.push("Fehlender Blob: ZIP-Datei liegt im Ordner".to_string());
        }
        assert!(offen.is_empty(), "{offen:#?}");
        Ok(())
    }

    /// Kontrolle zu K11-02: Ohne Git entsteht ein Export ohne Quellstand.
    #[test]
    fn ohne_git_entsteht_export_ohne_quellstand() -> Result<(), String> {
        let r = repo()?;
        fs::remove_dir_all(r.join(".git")).map_err(|e| e.to_string())?;
        schreib(&r, "README.md", b"MARKE_OHNE_GIT\n")?;
        let p = schreibe(&r, &lauf_in(&r, "sauber\n"))?;
        let bytes = fs::read(p).map_err(|e| e.to_string())?;
        assert!(
            !enthaelt(&bytes, b"MARKE_OHNE_GIT"),
            "ohne Git darf kein Quellstand entstehen"
        );
        Ok(())
    }

    /// Kontrolle: Ein sauberes Repository ergibt einen Export.
    #[test]
    fn sauberer_export_entsteht() -> Result<(), String> {
        let r = repo()?;
        schreib(&r, "README.md", b"# Probe\n")?;
        git_muss(&r, &["add", "README.md"])?;
        git_muss(&r, &["commit", "-q", "-m", "basis"])?;
        let p = schreibe(&r, &lauf_in(&r, "alles gut\n"))?;
        assert!(p.is_file());
        Ok(())
    }

    // -----------------------------------------------------------------------------------------
    // 1·R04, K14-01 – Gate und Export nehmen denselben festgehaltenen Stand (R28)
    // -----------------------------------------------------------------------------------------

    fn exportiert(r: &Path, l: &Lauf) -> Result<Vec<u8>, String> {
        let p = schreibe(r, l)?;
        fs::read(p).map_err(|e| e.to_string())
    }

    fn sauberes_repo(inhalt: &[u8]) -> Result<PathBuf, String> {
        let r = repo()?;
        schreib(&r, "README.md", inhalt)?;
        git_muss(&r, &["add", "README.md"])?;
        git_muss(&r, &["commit", "-q", "-m", "basis"])?;
        Ok(r)
    }

    /// Was das Gate in einem Fall zu sehen bekam: `README.md` und ob `src/neu.rs` da war.
    #[derive(Debug, Default)]
    struct Sicht {
        readme: Option<Vec<u8>>,
        neu: bool,
    }

    /// K14-01: Gate und Export pruefen und exportieren denselben festgehaltenen Stand, Byte fuer
    /// Byte, auch wenn Git eine Abweichung des Arbeitsbaums nicht meldet (`assume-unchanged`,
    /// `core.autocrlf`). Was nur im Arbeitsbaum liegt, sieht weder das Gate noch der Export;
    /// eine Aenderung am Index waehrend des Gates erreicht keinen von beiden.
    #[test]
    fn befund_k14_01_gate_und_export_sehen_dieselben_bytes() -> Result<(), String> {
        let index = b"INDEX_STAND\nZEILE_ZWEI\n".to_vec();
        let mut offen = Vec::new();
        for fall in [
            "assume-unchanged",
            "autocrlf",
            "nicht vorgemerkt",
            "neu und unverfolgt",
            "Index waehrend des Gates",
        ] {
            let r = sauberes_repo(&index)?;
            match fall {
                "assume-unchanged" => {
                    git_muss(&r, &["update-index", "--assume-unchanged", "README.md"])?;
                    schreib(
                        &r,
                        "README.md",
                        b"INDEX_STAND\nZEILE_ZWEI\nNUR_ARBEITSBAUM\n",
                    )?;
                }
                "autocrlf" => {
                    git_muss(&r, &["config", "core.autocrlf", "true"])?;
                    schreib(&r, "README.md", b"INDEX_STAND\r\nZEILE_ZWEI\r\n")?;
                }
                "nicht vorgemerkt" => {
                    schreib(
                        &r,
                        "README.md",
                        b"INDEX_STAND\nZEILE_ZWEI\nNUR_ARBEITSBAUM\n",
                    )?;
                }
                "neu und unverfolgt" => schreib(&r, "src/neu.rs", b"// NUR_ARBEITSBAUM\n")?,
                _ => {}
            }
            let mut sicht = Sicht::default();
            let mut fehler = Ok(());
            let l = crate::abbild::gepruefter_lauf(
                &r,
                "alles".into(),
                String::new(),
                Vec::new(),
                |stand, _| {
                    sicht.readme = fs::read(stand.join("README.md")).ok();
                    sicht.neu = stand.join("src/neu.rs").exists();
                    if fall == "Index waehrend des Gates" {
                        fehler = schreib(&r, "README.md", b"NUR_ARBEITSBAUM\n")
                            .and_then(|()| git_muss(&r, &["add", "README.md"]));
                    }
                    0
                },
            );
            fehler?;
            let bytes = exportiert(&r, &l)?;
            if sicht.readme.as_deref() != Some(index.as_slice()) {
                offen.push(format!(
                    "{fall}: Gate sah {:?}",
                    sicht
                        .readme
                        .map(|b| String::from_utf8_lossy(&b).into_owned())
                ));
            }
            if sicht.neu {
                offen.push(format!("{fall}: Gate sah die unverfolgte Datei"));
            }
            if !enthaelt(&bytes, &index) || enthaelt(&bytes, b"NUR_ARBEITSBAUM") {
                offen.push(format!(
                    "{fall}: Export enthaelt nicht genau den Stand des Gates"
                ));
            }
            if !l.erfolgreich() || !enthaelt(&bytes, b"\"quellstand_geprueft\": true") {
                offen.push(format!(
                    "{fall}: gleicher Stand gilt nicht als geprueft: {:?}",
                    l.warnungen
                ));
            }
        }
        assert!(offen.is_empty(), "{offen:#?}");
        Ok(())
    }

    /// Wird der festgehaltene Stand selbst waehrend des Gates geaendert oder ergaenzt, ist der
    /// Lauf gescheitert, und der Export nennt den Quellstand ungeprueft.
    #[test]
    fn befund_k14_01_aenderung_am_festgehaltenen_stand_wird_erkannt() -> Result<(), String> {
        let mut offen = Vec::new();
        for (fall, pfad) in [("geaendert", "README.md"), ("neu", "neu.txt")] {
            let r = sauberes_repo(b"FEST\n")?;
            let mut fehler = Ok(());
            let l = crate::abbild::gepruefter_lauf(
                &r,
                "alles".into(),
                String::new(),
                Vec::new(),
                |stand, _| {
                    fehler = schreib(stand, pfad, b"FREMD\n");
                    0
                },
            );
            fehler?;
            let bytes = exportiert(&r, &l)?;
            if l.erfolgreich() || !enthaelt(&bytes, b"\"quellstand_geprueft\": false") {
                offen.push(format!("{fall}: nicht erkannt"));
            }
            if !l.warnungen.iter().any(|w| w.contains(pfad)) {
                offen.push(format!("{fall}: Warnung ohne {pfad}: {:?}", l.warnungen));
            }
        }
        assert!(offen.is_empty(), "{offen:#?}");
        Ok(())
    }

    /// Kontrolle: Bleibt alles gleich, ist der Lauf erfolgreich, und der Quellstand gilt als
    /// geprueft.
    #[test]
    fn befund_1_r04_unveraenderter_stand_gilt_als_geprueft() -> Result<(), String> {
        let r = sauberes_repo(b"UNVERAENDERT\n")?;
        let l =
            crate::abbild::gepruefter_lauf(&r, "alles".into(), String::new(), Vec::new(), |_, _| 0);
        let bytes = exportiert(&r, &l)?;
        assert!(l.erfolgreich(), "{:?}", l.warnungen);
        assert!(enthaelt(&bytes, b"UNVERAENDERT"));
        assert!(enthaelt(&bytes, b"\"quellstand_geprueft\": true"));
        Ok(())
    }
}
