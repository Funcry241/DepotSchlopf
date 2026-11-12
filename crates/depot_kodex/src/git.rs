//! Realer Diff gegen die Basis: Merge-Base bis Arbeitsbaum, inklusive neuer Dateien.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug)]
pub struct Diff {
    pub pfade: BTreeSet<String>,
    /// Merge-Base, gegen die verglichen wird.
    pub basis: String,
    /// Hatte die Basis schon einen Kodex? Wenn nicht, ist nur `Gruendung` erlaubt.
    pub basis_hat_kodex: bool,
}

fn git(wurzel: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let aus = Command::new("git")
        .arg("-C")
        .arg(wurzel)
        .args(args)
        .output()
        .map_err(|e| format!("git nicht startbar: {e}"))?;
    if !aus.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&aus.stderr).trim()
        ));
    }
    Ok(aus.stdout)
}

/// NUL-getrennte Pfadliste (R24): keine Quotierung, Umlaute und Leerzeichen bleiben erhalten.
/// Ein Pfad, der kein UTF-8 ist, ist ein Fehler.
fn pfadliste(roh: &[u8]) -> Result<Vec<String>, String> {
    roh.split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| {
            String::from_utf8(p.to_vec())
                .map_err(|_| format!("Pfad ist kein UTF-8: {}", String::from_utf8_lossy(p)))
        })
        .collect()
}

/// Alle Dateien, die Git verfolgt, und alle neuen, nicht ignorierten: alles, was ein Commit
/// mitnehmen koennte.
///
/// # Errors
/// Wenn git fehlt oder der Ordner kein Repository ist.
pub fn projektdateien(wurzel: &Path) -> Result<Vec<String>, String> {
    pfadliste(&git(
        wurzel,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )?)
}

/// Eine gewoehnliche Datei im Index: Pfad und Blob. Links und Untermodule fehlen hier.
#[derive(Debug)]
pub struct IndexDatei {
    pub pfad: String,
    pub blob: String,
}

/// Die gewoehnlichen Dateien im Index, also das, was ein Commit jetzt aufnehmen wuerde.
///
/// # Errors
/// Wenn git fehlt, der Ordner kein Repository ist oder eine Zeile nicht lesbar ist.
pub fn index_dateien(wurzel: &Path) -> Result<Vec<IndexDatei>, String> {
    let mut aus = Vec::new();
    for zeile in pfadliste(&git(wurzel, &["ls-files", "-z", "--stage"])?)? {
        // "<modus> <blob> <stufe>\t<pfad>"
        let (kopf, pfad) = zeile
            .split_once('\t')
            .ok_or_else(|| format!("Index-Zeile nicht lesbar: {zeile}"))?;
        let mut teile = kopf.split(' ');
        let (Some(modus), Some(blob)) = (teile.next(), teile.next()) else {
            return Err(format!("Index-Zeile nicht lesbar: {zeile}"));
        };
        if modus == "100644" || modus == "100755" {
            aus.push(IndexDatei {
                pfad: pfad.to_string(),
                blob: blob.to_string(),
            });
        }
    }
    Ok(aus)
}

/// Prüft die Indexdateien über einen gemeinsamen Git-Prozess. Die Inhalte kommen
/// einzeln aus Git und werden nach jeder Datei wieder freigegeben.
pub(crate) fn index_inhalte(
    wurzel: &Path,
    mut pruefe: impl FnMut(&str, &[u8]),
) -> Result<(), String> {
    let index = index_dateien(wurzel)?;
    if index.is_empty() {
        return Ok(());
    }
    let mut kind = Command::new("git")
        .arg("-C")
        .arg(wurzel)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("git cat-file nicht startbar: {e}"))?;
    let ergebnis = (|| -> Result<(), String> {
        let mut eingabe = kind.stdin.take().ok_or("Git-Eingabe fehlt")?;
        let mut ausgabe = BufReader::new(kind.stdout.take().ok_or("Git-Ausgabe fehlt")?);
        for datei in index {
            let inhalt = lies_batch_blob(&mut eingabe, &mut ausgabe, &datei.blob)
                .map_err(|e| format!("{} (Index): nicht lesbar: {e}", datei.pfad))?;
            pruefe(&datei.pfad, &inhalt);
        }
        Ok(())
    })();
    if ergebnis.is_err() {
        let _ = kind.kill();
    }
    let status = kind.wait();
    ergebnis?;
    let status = status.map_err(|e| format!("git cat-file: {e}"))?;
    if !status.success() {
        return Err(format!("git cat-file: {status}"));
    }
    Ok(())
}

fn lies_batch_blob(
    eingabe: &mut impl std::io::Write,
    ausgabe: &mut impl BufRead,
    blob: &str,
) -> Result<Vec<u8>, String> {
    if !matches!(blob.len(), 40 | 64) || !blob.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("ungültige Git-Objektkennung".into());
    }
    writeln!(eingabe, "{blob}").map_err(|e| e.to_string())?;
    eingabe.flush().map_err(|e| e.to_string())?;
    let mut kopf = String::new();
    ausgabe.read_line(&mut kopf).map_err(|e| e.to_string())?;
    let mut felder = kopf.split_ascii_whitespace();
    if felder.next() != Some(blob) || felder.next() != Some("blob") {
        return Err("Git lieferte keinen passenden Blob".into());
    }
    let laenge = felder
        .next()
        .and_then(|n| n.parse::<u64>().ok())
        .filter(|_| felder.next().is_none())
        .ok_or("ungültige Länge in der Git-Antwort")?;
    let mut inhalt = Vec::new();
    ausgabe
        .take(laenge)
        .read_to_end(&mut inhalt)
        .map_err(|e| e.to_string())?;
    let mut ende = [0_u8];
    ausgabe.read_exact(&mut ende).map_err(|e| e.to_string())?;
    if u64::try_from(inhalt.len()).ok() != Some(laenge) || ende != *b"\n" {
        return Err("unvollständiger Blob in der Git-Antwort".into());
    }
    Ok(inhalt)
}

/// Inhalt eines Blobs.
///
/// # Errors
/// Wenn git fehlt oder der Blob unbekannt ist.
pub fn blob(wurzel: &Path, blob: &str) -> Result<Vec<u8>, String> {
    git(wurzel, &["cat-file", "blob", blob])
}

/// Nur die Dateien, die Git verfolgt.
///
/// # Errors
/// Wenn git fehlt oder der Ordner kein Repository ist.
pub fn verfolgte_dateien(wurzel: &Path) -> Result<Vec<String>, String> {
    pfadliste(&git(wurzel, &["ls-files", "-z", "--cached"])?)
}

/// Merge-Base von `basis` und `HEAD`.
///
/// # Errors
/// Wenn git fehlt oder die Basis unbekannt ist.
pub fn merge_base(wurzel: &Path, basis: &str) -> Result<String, String> {
    let mb = git(wurzel, &["merge-base", basis, "HEAD"])?;
    Ok(String::from_utf8_lossy(&mb).trim().to_string())
}

/// Inhalt einer Datei im Stand `rev`; `None`, wenn es sie dort nicht gibt.
pub fn datei_im_stand(wurzel: &Path, rev: &str, pfad: &str) -> Option<Vec<u8>> {
    git(wurzel, &["show", &format!("{rev}:{pfad}")]).ok()
}

/// # Errors
/// Wenn git fehlt oder die Basis unbekannt ist.
pub fn aenderungen(wurzel: &Path, basis: &str) -> Result<Diff, String> {
    let mb = merge_base(wurzel, basis)?;
    // --no-renames: Eine Umbenennung zeigt beide Pfade, den alten und den neuen.
    let mut pfade: BTreeSet<String> = pfadliste(&git(
        wurzel,
        &["diff", "-z", "--no-renames", "--name-only", &mb],
    )?)?
    .into_iter()
    .collect();
    pfade.extend(pfadliste(&git(
        wurzel,
        &["ls-files", "-z", "--others", "--exclude-standard"],
    )?)?);
    let basis_hat_kodex = git(
        wurzel,
        &["cat-file", "-e", &format!("{mb}:{}", crate::DEKLARATION)],
    )
    .is_ok();
    Ok(Diff {
        pfade,
        basis: mb,
        basis_hat_kodex,
    })
}

#[cfg(test)]
mod tests {
    use super::lies_batch_blob;
    use std::io::Cursor;

    #[test]
    fn batch_liest_bytes_und_naechsten_blob_getrennt() -> Result<(), String> {
        let oid = "1234567890".repeat(4);
        let mut antwort = Cursor::new(format!("{oid} blob 3\na\nb\n{oid} blob 0\n\n"));
        let mut anfragen = Vec::new();
        assert_eq!(lies_batch_blob(&mut anfragen, &mut antwort, &oid)?, b"a\nb");
        assert!(lies_batch_blob(&mut anfragen, &mut antwort, &oid)?.is_empty());
        assert_eq!(anfragen, format!("{oid}\n{oid}\n").as_bytes());
        Ok(())
    }

    #[test]
    fn batch_lehnt_fehlende_und_unvollstaendige_blobs_ab() {
        let oid = "1234567890".repeat(4);
        for antwort in [
            format!("{oid} missing\n"),
            format!("{oid} blob 3\nab"),
            format!("{oid} blob 2\nabx"),
        ] {
            assert!(lies_batch_blob(&mut Vec::new(), &mut Cursor::new(antwort), &oid).is_err());
        }
    }
}
