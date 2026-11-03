//! Sonde: Baut ein kleines fremdes Crate gegen die Crates dieses Workspaces.
//!
//! So zeigt ein Test, dass etwas Verbotenes sich gar nicht bauen laesst (R3, R19), oder dass ein
//! verlangter Vertrag wirklich gilt. Damit ein Fehlschlag nicht faelschlich als Beweis zaehlt:
//!
//! - Die Compilermeldungen werden strukturiert gelesen (`--message-format=json`). Ein verbotenes
//!   Stueck Code gilt nur dann als abgewiesen, wenn **jeder** Fehler einen der erwarteten
//!   Fehlercodes traegt und die Meldungen die erwarteten Namen nennen. Ein Tippfehler, ein
//!   umbenannter Konstruktor (E0599, E0425) oder ein Umgebungsfehler beweist nichts.
//! - Zu jedem verbotenen Stueck gehoert ein verwandtes erlaubtes Stueck, das bauen muss.
//! - Ein Umgebungsfehler (cargo nicht startbar, Datei nicht schreibbar, Fehlschlag ohne
//!   Compilerdiagnose) ist ein eigenes Ergebnis und laesst jeden Test scheitern.
//! - `laeuft` baut ein Programm und fuehrt es aus. Baut es nicht, ist der verlangte Vertrag
//!   nicht erfuellt; ein solcher Test kann also nie durch eine API-Aenderung gruen werden.
//!
//! Aendert der Umbau die oeffentliche Schnittstelle, wird die Sonde mitgefuehrt, nicht
//! abgeschwaecht. Grenze: Die Sonde beweist Eigenschaften der genannten Stuecke, nicht, dass es
//! keinen anderen oeffentlichen Weg gibt. Ob Grenzen nur aus der Festschreibung entstehen, prueft
//! der Umbau zusaetzlich am versiegelten Konstruktor.

#![allow(dead_code, reason = "jede Testdatei nutzt nur einen Teil der Sonde")]

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

/// Eine Fehlermeldung des Compilers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnose {
    pub code: Option<String>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ergebnis {
    /// Uebersetzt; bei `laeuft` auch ausgefuehrt, mit Erfolg (`true`) oder Abbruch (`false`).
    Baut { lauf_ok: bool, ausgabe: String },
    /// Der Compiler lehnt ab.
    Fehler(Vec<Diagnose>),
    /// Kein Urteil moeglich: Werkzeug oder Dateisystem versagt.
    Umgebung(String),
}

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Wie die Sonde ihre Abhaengigkeiten einbindet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bau {
    /// Wie ein gewoehnliches Crate: Standard-Features, Entwicklungsprofil.
    Normal,
    /// Jedes Feature jeder Abhaengigkeit eingeschaltet, Release-Profil. So baut ein Aufrufer,
    /// der jeden Schalter ausnutzt, den die Crates anbieten (K8-01).
    AlleFeaturesRelease,
}

/// Alle Features, die `paket` in seinem Manifest anbietet, laut `cargo metadata`.
fn features(paket: &str) -> Result<Vec<String>, String> {
    let aus = Command::new(option_env!("CARGO").unwrap_or("cargo"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .current_dir(workspace())
        .output()
        .map_err(|e| format!("cargo metadata nicht startbar: {e}"))?;
    if !aus.status.success() {
        return Err(format!(
            "cargo metadata: {}",
            String::from_utf8_lossy(&aus.stderr)
        ));
    }
    let text = String::from_utf8_lossy(&aus.stdout);
    let wert = json::lies(text.trim()).ok_or("cargo metadata: kein JSON")?;
    let Some(json::Wert::Liste(pakete)) = wert.feld("packages") else {
        return Err("cargo metadata: keine Pakete".into());
    };
    let p = pakete
        .iter()
        .find(|p| p.feld("name").and_then(json::Wert::text) == Some(paket))
        .ok_or_else(|| format!("cargo metadata: Paket {paket} fehlt"))?;
    match p.feld("features") {
        Some(json::Wert::Objekt(f)) => Ok(f.iter().map(|(k, _)| k.clone()).collect()),
        _ => Err(format!("cargo metadata: Features von {paket} fehlen")),
    }
}

fn crate_anlegen(abhaengigkeiten: &[&str], datei: &str, code: &str) -> Result<PathBuf, String> {
    crate_anlegen_mit(abhaengigkeiten, datei, code, Bau::Normal)
}

fn crate_anlegen_mit(
    abhaengigkeiten: &[&str],
    datei: &str,
    code: &str,
    bau: Bau,
) -> Result<PathBuf, String> {
    let ws = workspace();
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "depot_sonde_{}_{}_{n}",
        env!("CARGO_PKG_NAME"),
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).map_err(|e| format!("Sonde: {e}"))?;
    let mut toml = format!(
        "[package]\nname = \"depot_sonde_{}_{n}\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[workspace]\n\n[dependencies]\n",
        std::process::id()
    );
    for a in abhaengigkeiten {
        let pfad = ws.join("crates").join(a);
        let schalter = match bau {
            Bau::Normal => String::new(),
            Bau::AlleFeaturesRelease => format!(", features = {:?}", features(a)?),
        };
        let _ = writeln!(
            toml,
            "{a} = {{ path = {:?}{schalter} }}",
            pfad.to_string_lossy().replace('\\', "/")
        );
    }
    fs::write(dir.join("Cargo.toml"), toml).map_err(|e| format!("Sonde: {e}"))?;
    fs::write(dir.join("src").join(datei), code).map_err(|e| format!("Sonde: {e}"))?;
    for datei in ["Cargo.lock", "rust-toolchain.toml"] {
        fs::copy(ws.join(datei), dir.join(datei)).map_err(|e| format!("Sonde {datei}: {e}"))?;
    }
    Ok(dir)
}

fn cargo(dir: &Path, befehl: &str, bau: Bau) -> Result<std::process::Output, String> {
    let mut args = vec![befehl, "--quiet", "--offline", "--message-format=json"];
    if bau == Bau::AlleFeaturesRelease {
        args.push("--release");
    }
    Command::new(option_env!("CARGO").unwrap_or("cargo"))
        .args(args)
        .current_dir(dir)
        .env("CARGO_TARGET_DIR", workspace().join("target/sonde"))
        .output()
        .map_err(|e| format!("cargo nicht startbar: {e}"))
}

/// Fehlerdiagnosen aus der JSON-Ausgabe von cargo (nur Ebene „error“, ohne die
/// Schlusszeile „aborting due to …“).
fn diagnosen(stdout: &str) -> Vec<Diagnose> {
    let mut aus = Vec::new();
    for zeile in stdout.lines() {
        let Some(wert) = json::lies(zeile) else {
            continue;
        };
        if wert.feld("reason").and_then(json::Wert::text) != Some("compiler-message") {
            continue;
        }
        let Some(m) = wert.feld("message") else {
            continue;
        };
        if m.feld("level").and_then(json::Wert::text) != Some("error") {
            continue;
        }
        let text = m
            .feld("message")
            .and_then(json::Wert::text)
            .unwrap_or_default()
            .to_string();
        if text.starts_with("aborting due to") {
            continue;
        }
        let code = m
            .feld("code")
            .and_then(|c| c.feld("code"))
            .and_then(json::Wert::text)
            .map(str::to_string);
        aus.push(Diagnose { code, text });
    }
    aus
}

fn urteil(dir: &Path, befehl: &str) -> Ergebnis {
    urteil_mit(dir, befehl, Bau::Normal)
}

fn urteil_mit(dir: &Path, befehl: &str, bau: Bau) -> Ergebnis {
    let aus = match cargo(dir, befehl, bau) {
        Ok(a) => a,
        Err(e) => return Ergebnis::Umgebung(e),
    };
    let stdout = String::from_utf8_lossy(&aus.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&aus.stderr).into_owned();
    let fehler = diagnosen(&stdout);
    if !fehler.is_empty() {
        return Ergebnis::Fehler(fehler);
    }
    // `cargo run` meldet ein abgebrochenes Programm ohne Compilerdiagnose. Ob der Bau
    // gelungen ist, zeigt die Meldung „build-finished“.
    let gebaut = stdout.lines().filter_map(json::lies).any(|w| {
        w.feld("reason").and_then(json::Wert::text) == Some("build-finished")
            && w.feld("success") == Some(&json::Wert::Wahr)
    });
    if !gebaut {
        return Ergebnis::Umgebung(format!(
            "cargo {befehl} ohne Diagnose gescheitert: {stderr}"
        ));
    }
    let ausgabe = stdout
        .lines()
        .filter(|z| !z.starts_with('{'))
        .chain(stderr.lines())
        .collect::<Vec<_>>()
        .join("\n");
    Ergebnis::Baut {
        lauf_ok: aus.status.success(),
        ausgabe,
    }
}

/// Uebersetzt `code` als `src/lib.rs`.
pub fn uebersetze(abhaengigkeiten: &[&str], code: &str) -> Ergebnis {
    match crate_anlegen(abhaengigkeiten, "lib.rs", code) {
        Ok(dir) => {
            let e = urteil(&dir, "check");
            let _ = fs::remove_dir_all(&dir);
            e
        }
        Err(e) => Ergebnis::Umgebung(e),
    }
}

/// Uebersetzt `code` als `src/main.rs` und fuehrt es aus.
pub fn laeuft(abhaengigkeiten: &[&str], code: &str) -> Ergebnis {
    match crate_anlegen(abhaengigkeiten, "main.rs", code) {
        Ok(dir) => {
            let e = urteil(&dir, "run");
            let _ = fs::remove_dir_all(&dir);
            e
        }
        Err(e) => Ergebnis::Umgebung(e),
    }
}

/// Baut `code` als Programm und legt die ausfuehrbare Datei in ein eigenes Verzeichnis.
/// So kann ein Test sie unter besonderen Bedingungen starten (etwa mit Groessengrenze fuer
/// Dateien), ohne dass diese Bedingungen den Bau treffen.
pub fn programm(abhaengigkeiten: &[&str], code: &str) -> Result<PathBuf, String> {
    let dir = crate_anlegen(abhaengigkeiten, "main.rs", code)?;
    let ergebnis = urteil(&dir, "build");
    let name = fs::read_to_string(dir.join("Cargo.toml"))
        .ok()
        .and_then(|t| {
            t.lines().find_map(|z| {
                z.strip_prefix("name = \"")
                    .map(|r| r.trim_end_matches('"').to_string())
            })
        })
        .ok_or("Sonde: Paketname fehlt")?;
    match ergebnis {
        Ergebnis::Baut { .. } => {}
        anders => return Err(format!("Programm baut nicht: {anders:?}")),
    }
    let quelle = workspace()
        .join("target/sonde/debug")
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    let ziel = dir.join(format!("probe{}", std::env::consts::EXE_SUFFIX));
    fs::copy(&quelle, &ziel).map_err(|e| format!("Sonde {}: {e}", quelle.display()))?;
    Ok(ziel)
}

/// Baut `code` als Programm im Release-Profil, mit jedem Feature jeder Abhaengigkeit.
pub fn release_mit_allen_features(abhaengigkeiten: &[&str], code: &str) -> Ergebnis {
    match crate_anlegen_mit(abhaengigkeiten, "main.rs", code, Bau::AlleFeaturesRelease) {
        Ok(dir) => {
            let e = urteil_mit(&dir, "build", Bau::AlleFeaturesRelease);
            let _ = fs::remove_dir_all(&dir);
            e
        }
        Err(e) => Ergebnis::Umgebung(e),
    }
}

/// Wie `verboten`, aber gebaut wie von einem Aufrufer, der jedes Feature einschaltet und im
/// Release baut. Ein Schutz, der nur ohne Feature gilt, faellt hier auf.
pub fn verboten_mit_allen_features(
    abhaengigkeiten: &[&str],
    code: &str,
    codes: &[&str],
    nennt: &[&str],
) -> Result<(), String> {
    pruefe_verbot(
        release_mit_allen_features(abhaengigkeiten, code),
        code,
        codes,
        nennt,
    )
}

/// Das erlaubte Gegenstueck baut auch mit allen Features im Release.
pub fn erlaubt_mit_allen_features(abhaengigkeiten: &[&str], code: &str) -> Result<(), String> {
    match release_mit_allen_features(abhaengigkeiten, code) {
        Ergebnis::Baut { .. } => Ok(()),
        anders => Err(format!(
            "erlaubtes Gegenstueck baut nicht: {anders:?}\n{code}"
        )),
    }
}

/// Das erlaubte Gegenstueck muss bauen.
pub fn erlaubt(abhaengigkeiten: &[&str], code: &str) -> Result<(), String> {
    match uebersetze(abhaengigkeiten, code) {
        Ergebnis::Baut { .. } => Ok(()),
        anders => Err(format!(
            "erlaubtes Gegenstueck baut nicht: {anders:?}\n{code}"
        )),
    }
}

/// Das verbotene Stueck muss abgewiesen werden, und zwar nur mit Fehlern aus `codes`; die
/// Meldungen muessen jeden Namen aus `nennt` enthalten.
pub fn verboten(
    abhaengigkeiten: &[&str],
    code: &str,
    codes: &[&str],
    nennt: &[&str],
) -> Result<(), String> {
    pruefe_verbot(uebersetze(abhaengigkeiten, code), code, codes, nennt)
}

fn pruefe_verbot(
    ergebnis: Ergebnis,
    code: &str,
    codes: &[&str],
    nennt: &[&str],
) -> Result<(), String> {
    match ergebnis {
        Ergebnis::Baut { .. } => Err(format!("laesst sich bauen:\n{code}")),
        Ergebnis::Umgebung(e) => Err(format!("Umgebungsfehler, kein Urteil: {e}")),
        Ergebnis::Fehler(d) => {
            let fremd: Vec<&Diagnose> = d
                .iter()
                .filter(|x| !x.code.as_deref().is_some_and(|c| codes.contains(&c)))
                .collect();
            if !fremd.is_empty() {
                return Err(format!(
                    "abgewiesen, aber nicht aus dem erwarteten Grund {codes:?}: {fremd:?}"
                ));
            }
            for n in nennt {
                if !d.iter().any(|x| x.text.contains(n)) {
                    return Err(format!("Meldungen nennen {n:?} nicht: {d:?}"));
                }
            }
            Ok(())
        }
    }
}

/// Das Programm muss bauen und ohne Abbruch durchlaufen.
pub fn muss_laufen(abhaengigkeiten: &[&str], code: &str) -> Result<(), String> {
    match laeuft(abhaengigkeiten, code) {
        Ergebnis::Baut { lauf_ok: true, .. } => Ok(()),
        Ergebnis::Baut {
            lauf_ok: false,
            ausgabe,
        } => Err(format!("Vertrag verletzt:\n{ausgabe}")),
        Ergebnis::Fehler(d) => Err(format!("Vertrag nicht erfuellt, baut nicht: {d:?}")),
        Ergebnis::Umgebung(e) => Err(format!("Umgebungsfehler, kein Urteil: {e}")),
    }
}

/// Fehlercodes fuer „nicht zugaenglich“: privates Feld im Literal (E0451), privates Feld beim
/// Zugriff (E0616), privates Element (E0603), private Methode (E0624), Tupel-Konstruktor mit
/// privaten Feldern (E0423).
pub const PRIVAT: [&str; 5] = ["E0451", "E0616", "E0603", "E0624", "E0423"];

/// Minimaler JSON-Leser fuer die Ausgabe von cargo. Nur fuer Tests.
mod json {
    #[derive(Clone, Debug, PartialEq)]
    pub enum Wert {
        Null,
        Wahr,
        Falsch,
        Zahl,
        Text(String),
        Liste(Vec<Wert>),
        Objekt(Vec<(String, Wert)>),
    }

    impl Wert {
        pub fn feld(&self, name: &str) -> Option<&Wert> {
            match self {
                Wert::Objekt(f) => f.iter().find(|(k, _)| k == name).map(|(_, v)| v),
                _ => None,
            }
        }

        pub fn text(&self) -> Option<&str> {
            match self {
                Wert::Text(t) => Some(t),
                _ => None,
            }
        }
    }

    pub fn lies(text: &str) -> Option<Wert> {
        let mut z = text.chars().peekable();
        let w = wert(&mut z)?;
        leer(&mut z);
        z.peek().is_none().then_some(w)
    }

    type Z<'a> = std::iter::Peekable<std::str::Chars<'a>>;

    fn leer(z: &mut Z<'_>) {
        while z.peek().is_some_and(|c| c.is_whitespace()) {
            z.next();
        }
    }

    fn wort(z: &mut Z<'_>, w: &str, v: Wert) -> Option<Wert> {
        for c in w.chars() {
            (z.next()? == c).then_some(())?;
        }
        Some(v)
    }

    fn wert(z: &mut Z<'_>) -> Option<Wert> {
        leer(z);
        match *z.peek()? {
            '{' => {
                z.next();
                let mut felder = Vec::new();
                leer(z);
                if z.peek() == Some(&'}') {
                    z.next();
                    return Some(Wert::Objekt(felder));
                }
                loop {
                    leer(z);
                    let Wert::Text(k) = text(z)? else {
                        return None;
                    };
                    leer(z);
                    (z.next()? == ':').then_some(())?;
                    felder.push((k, wert(z)?));
                    leer(z);
                    match z.next()? {
                        ',' => {}
                        '}' => return Some(Wert::Objekt(felder)),
                        _ => return None,
                    }
                }
            }
            '[' => {
                z.next();
                let mut liste = Vec::new();
                leer(z);
                if z.peek() == Some(&']') {
                    z.next();
                    return Some(Wert::Liste(liste));
                }
                loop {
                    liste.push(wert(z)?);
                    leer(z);
                    match z.next()? {
                        ',' => {}
                        ']' => return Some(Wert::Liste(liste)),
                        _ => return None,
                    }
                }
            }
            '"' => text(z),
            't' => wort(z, "true", Wert::Wahr),
            'f' => wort(z, "false", Wert::Falsch),
            'n' => wort(z, "null", Wert::Null),
            _ => {
                let mut gesehen = false;
                while z
                    .peek()
                    .is_some_and(|c| c.is_ascii_digit() || "+-.eE".contains(*c))
                {
                    z.next();
                    gesehen = true;
                }
                gesehen.then_some(Wert::Zahl)
            }
        }
    }

    fn text(z: &mut Z<'_>) -> Option<Wert> {
        (z.next()? == '"').then_some(())?;
        let mut s = String::new();
        loop {
            match z.next()? {
                '"' => return Some(Wert::Text(s)),
                '\\' => match z.next()? {
                    'n' => s.push('\n'),
                    't' => s.push('\t'),
                    'r' => s.push('\r'),
                    'b' | 'f' => {}
                    'u' => {
                        let hex: String = (0..4).filter_map(|_| z.next()).collect();
                        let n = u32::from_str_radix(&hex, 16).ok()?;
                        s.push(char::from_u32(n).unwrap_or('\u{fffd}'));
                    }
                    c => s.push(c),
                },
                c => s.push(c),
            }
        }
    }
}
