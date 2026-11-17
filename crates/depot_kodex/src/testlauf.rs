//! Baut die von Cargo ausgewählten Testprogramme und führt sie mit begrenzter Parallelität aus.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use depot_kern::json::{self, Json};

use crate::testumgebung::Testumgebung;

const TESTS: &[&str] = &["test", "--workspace", "--no-fail-fast"];

struct Testprogramm {
    binary: PathBuf,
    paket: PathBuf,
    name: String,
}

struct Testbau {
    programme: Vec<Testprogramm>,
    bibliotheken: BTreeSet<PathBuf>,
}

fn text<'a>(wert: &'a Json, feld: &str) -> Result<&'a str, String> {
    wert.feld(feld)
        .and_then(Json::text)
        .ok_or_else(|| format!("Cargo-Meldung ohne {feld}"))
}

fn testbau(ausgabe: &str, wurzel: &Path, ziel: &Path) -> Result<Testbau, String> {
    let mut programme = BTreeMap::new();
    let mut bibliotheken = BTreeSet::new();
    let mut fertig = false;
    for zeile in ausgabe.lines().filter(|z| z.starts_with('{')) {
        let wert = json::lies(zeile)?;
        match text(&wert, "reason")? {
            "compiler-artifact"
                if wert
                    .feld("profile")
                    .and_then(|p| p.feld("test"))
                    .and_then(Json::wahr)
                    == Some(true) =>
            {
                let binary = PathBuf::from(text(&wert, "executable")?);
                let manifest = Path::new(text(&wert, "manifest_path")?);
                let paket = manifest.parent().ok_or("Paketpfad fehlt")?;
                let testziel = wert.feld("target").ok_or("Testziel fehlt")?;
                let quellpfad = Path::new(text(testziel, "src_path")?);
                let name = quellpfad
                    .strip_prefix(wurzel)
                    .map_err(|_| "Testziel liegt außerhalb des Prüfstands")?
                    .to_str()
                    .ok_or("Testziel ist kein UTF-8")?
                    .replace('\\', "/");
                if !binary.is_absolute() || !paket.is_absolute() {
                    return Err("Cargo lieferte einen relativen Testpfad".into());
                }
                let echt = binary.canonicalize().map_err(|e| e.to_string())?;
                if !echt.starts_with(ziel) || !echt.is_file() {
                    return Err("Testprogramm liegt außerhalb des Prüf-Bauordners".into());
                }
                if !paket.starts_with(wurzel) {
                    return Err("Testpaket liegt außerhalb des Prüfstands".into());
                }
                programme.insert(
                    binary.clone(),
                    Testprogramm {
                        binary,
                        paket: paket.to_owned(),
                        name,
                    },
                );
            }
            "build-script-executed" => {
                if let Some(pfade) = wert.feld("linked_paths").and_then(Json::liste) {
                    for pfad in pfade.iter().filter_map(Json::text) {
                        let pfad = pfad.split_once('=').map_or(pfad, |(_, pfad)| pfad);
                        bibliotheken.insert(PathBuf::from(pfad));
                    }
                }
            }
            "build-finished" => fertig = wert.feld("success").and_then(Json::wahr) == Some(true),
            _ => {}
        }
    }
    if !fertig || programme.is_empty() {
        return Err("Kein vollständiger Cargo-Testbau".into());
    }
    Ok(Testbau {
        programme: programme.into_values().collect(),
        bibliotheken,
    })
}

fn anzahl_aufgelistet(liste: &str) -> usize {
    liste
        .lines()
        .filter(|z| z.ends_with(": test") || z.ends_with(": benchmark"))
        .count()
}

fn anzahl_ausgefuehrt(ausgabe: &str) -> Option<usize> {
    let mut zeilen = ausgabe
        .lines()
        .filter_map(|z| z.strip_prefix("test result: "));
    let zeile = zeilen.next()?;
    if zeilen.next().is_some() {
        return None;
    }
    let (_, zahlen) = zeile.split_once(". ")?;
    let mut teile = zahlen.split("; ");
    let bestanden: usize = teile.next()?.strip_suffix(" passed")?.parse().ok()?;
    let fehler: usize = teile.next()?.strip_suffix(" failed")?.parse().ok()?;
    let ignoriert: usize = teile.next()?.strip_suffix(" ignored")?.parse().ok()?;
    let gemessen: usize = teile.next()?.strip_suffix(" measured")?.parse().ok()?;
    let gefiltert: usize = teile.next()?.strip_suffix(" filtered out")?.parse().ok()?;
    (gefiltert == 0)
        .then_some(bestanden)?
        .checked_add(fehler)?
        .checked_add(ignoriert)?
        .checked_add(gemessen)
}

fn code(ausgabe: &Output) -> i32 {
    ausgabe.status.code().unwrap_or(-1)
}

fn bericht(name: &str, start: Instant, ausgabe: &str, code: i32) {
    println!(
        "[Kodex] Testprogramm {name} · Beginn\n{ausgabe}\n[Kodex] Testprogramm {name} · Exit {code} · {} ms",
        start.elapsed().as_millis()
    );
}

fn pfadvariable() -> &'static str {
    if cfg!(windows) {
        "PATH"
    } else if cfg!(target_os = "macos") {
        "DYLD_FALLBACK_LIBRARY_PATH"
    } else {
        "LD_LIBRARY_PATH"
    }
}

fn bibliothekspfad(bau: &Testbau, bauordner: &Path) -> Result<OsString, String> {
    let sysroot = Command::new("rustc")
        .args(["--print", "target-libdir"])
        .output()
        .map_err(|e| e.to_string())?;
    if !sysroot.status.success() {
        return Err("Rust-Bibliothekspfad fehlt".into());
    }
    let sysroot = String::from_utf8(sysroot.stdout).map_err(|e| e.to_string())?;
    let mut pfade = BTreeSet::new();
    pfade.extend(
        bau.bibliotheken
            .iter()
            .filter(|p| p.starts_with(bauordner))
            .cloned(),
    );
    for programm in &bau.programme {
        if let Some(deps) = programm.binary.parent() {
            pfade.insert(deps.to_owned());
            if let Some(profil) = deps.parent() {
                pfade.insert(profil.to_owned());
            }
        }
    }
    pfade.insert(PathBuf::from(sysroot.trim()));
    let vorhanden = std::env::var_os(pfadvariable()).unwrap_or_default();
    std::env::join_paths(pfade.into_iter().chain(std::env::split_paths(&vorhanden)))
        .map_err(|e| e.to_string())
}

fn test_befehl(
    programm: &Testprogramm,
    umgebung: &Testumgebung,
    bibliotheken: &OsStr,
    threads: usize,
) -> Command {
    let mut befehl = Command::new(&programm.binary);
    befehl
        .current_dir(&programm.paket)
        .env("CARGO_MANIFEST_DIR", &programm.paket)
        .env(pfadvariable(), bibliotheken)
        .env("RUST_TEST_THREADS", threads.to_string());
    umgebung.anwenden(&mut befehl);
    befehl
}

fn fuehre_aus(programm: &Testprogramm, bibliotheken: &OsStr, threads: usize) -> bool {
    let start = Instant::now();
    let ergebnis = (|| {
        let umgebung = Testumgebung::neu()?;
        let mut befehl = test_befehl(programm, &umgebung, bibliotheken, threads);
        let liste = befehl
            .args(["--list", "--format", "terse"])
            .output()
            .map_err(|e| e.to_string())?;
        if !liste.status.success() {
            return Err(format!(
                "Testliste fehlgeschlagen:\n{}",
                String::from_utf8_lossy(&liste.stderr)
            ));
        }
        let erwartet = anzahl_aufgelistet(&String::from_utf8_lossy(&liste.stdout));
        let ausgabe = test_befehl(programm, &umgebung, bibliotheken, threads)
            .output()
            .map_err(|e| e.to_string())?;
        let stdout = String::from_utf8_lossy(&ausgabe.stdout);
        let stderr = String::from_utf8_lossy(&ausgabe.stderr);
        let gezählt = anzahl_ausgefuehrt(&stdout);
        let exit = code(&ausgabe);
        let mut inhalt = format!("{stdout}{stderr}");
        if gezählt != Some(erwartet) {
            let _ = writeln!(
                inhalt,
                "\n[Kodex] Testanzahl stimmt nicht: Testliste {erwartet}, Ergebnis {gezählt:?}."
            );
        }
        Ok((
            inhalt,
            if exit == 0 && gezählt != Some(erwartet) {
                1
            } else {
                exit
            },
        ))
    })();
    let (ausgabe, exit) = ergebnis.unwrap_or_else(|e: String| (e, -1));
    bericht(&programm.name, start, &ausgabe, exit);
    exit == 0
}

fn dokumentationstests() -> Result<bool, String> {
    let start = Instant::now();
    let mut cargo = Command::new("cargo");
    let umgebung = Testumgebung::neu()?;
    cargo.args(TESTS).arg("--doc");
    umgebung.anwenden(&mut cargo);
    let ausgabe = cargo.output().map_err(|e| e.to_string())?;
    let exit = code(&ausgabe);
    bericht(
        "Doc-tests",
        start,
        &format!(
            "{}{}",
            String::from_utf8_lossy(&ausgabe.stderr),
            String::from_utf8_lossy(&ausgabe.stdout)
        ),
        exit,
    );
    Ok(exit == 0)
}

pub(super) fn parallel(wurzel: &Path) -> Result<bool, String> {
    let start = Instant::now();
    let mut cargo = Command::new("cargo");
    let umgebung = Testumgebung::neu()?;
    println!("[Kodex] cargo {}", TESTS.join(" "));
    println!("[Kodex] Tests laufen mit eigenen temporären Dateien und eigener Git-Konfiguration.");
    cargo
        .args(TESTS)
        .args(["--no-run", "--message-format=json"]);
    umgebung.anwenden(&mut cargo);
    let ausgabe = cargo.output().map_err(|e| e.to_string())?;
    eprint!("{}", String::from_utf8_lossy(&ausgabe.stderr));
    let meldungen = String::from_utf8(ausgabe.stdout).map_err(|e| e.to_string())?;
    if !ausgabe.status.success() {
        eprintln!("{meldungen}");
        return Ok(false);
    }
    let bauordner =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| wurzel.join("target"), PathBuf::from);
    let ziel = bauordner.canonicalize().map_err(|e| e.to_string())?;
    let bau = testbau(&meldungen, wurzel, &ziel)?;
    let bibliotheken = bibliothekspfad(&bau, &bauordner)?;
    let cpus = std::thread::available_parallelism().map_or(1, usize::from);
    let arbeiter = cpus.min(4).min(bau.programme.len());
    let threads = cpus.checked_div(arbeiter).unwrap_or(1).max(1);
    println!(
        "[Kodex] {} Testprogramme, {arbeiter} parallel, je {threads} Testthreads.",
        bau.programme.len()
    );
    let naechstes = AtomicUsize::new(0);
    let binaries_ok = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..arbeiter)
            .map(|_| {
                scope.spawn(|| {
                    let mut ok = true;
                    while let Some(programm) =
                        bau.programme.get(naechstes.fetch_add(1, Ordering::Relaxed))
                    {
                        ok = fuehre_aus(programm, &bibliotheken, threads) && ok;
                    }
                    ok
                })
            })
            .collect();
        let mut alle_ok = true;
        for handle in handles {
            alle_ok = handle.join().unwrap_or(false) && alle_ok;
        }
        alle_ok
    });
    let docs_ok = dokumentationstests()?;
    println!(
        "[Kodex] cargo {} · Dauer: {} ms",
        TESTS.join(" "),
        start.elapsed().as_millis()
    );
    Ok(binaries_ok && docs_ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testanzahl_zaehlt_ignorierte_und_fehler() {
        assert_eq!(anzahl_aufgelistet("a: test\nb: test\n"), 2);
        assert_eq!(
            anzahl_ausgefuehrt(
                "test result: FAILED. 2 passed; 3 failed; 4 ignored; 0 measured; 0 filtered out; finished in 0.40s"
            ),
            Some(9)
        );
    }

    #[test]
    fn abbruch_oder_gefiltertes_programm_lehnt_ab() {
        assert_eq!(anzahl_ausgefuehrt("Absturz\n"), None);
        assert_eq!(
            anzahl_ausgefuehrt(
                "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.01s"
            ),
            None
        );
    }
}
