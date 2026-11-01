//! Negativtests zu den Kodex-Befunden aus `docs/BEFUNDE.md` (Stufe 2, Kodex-Schritt).
//!
//! Jeder Test baut eine Kopie des Arbeitsbaums in `target/kodex-befunde/`, legt dort ein
//! Git-Repository mit der Basis `basis` an und fuehrt dann genau einen Verstoss aus dem Befund
//! ein. Der Kodex muss ihn ablehnen, und zwar aus dem richtigen Grund: Die Ablehnung muss ein
//! erwartetes Stichwort nennen. Vor jedem Verstoss prueft eine Kontrolle, dass dieselbe Kopie
//! mit einer harmlosen, sauber deklarierten Aenderung das Gate besteht. Eine kaputte Kopie macht
//! einen Test also nie gruen.
//!
//! Die Tests nutzen nur `depot_kodex::pruefe`, `pruefe_ohne_diff` und die Festschreibung
//! (`berechne`, `als_text`). Diese Schnittstelle bleibt beim Umbau gleich.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use sha2::{Digest, Sha256};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let aus = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=Kodex-Probe",
            "-c",
            "user.email=probe@depotschlopf.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .map_err(|e| format!("git nicht startbar: {e}"))?;
    if !aus.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&aus.stderr)
        ));
    }
    Ok(aus.stdout)
}

fn sha_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Eine Kopie des Arbeitsbaums mit eigenem Git-Repository; die Basis heisst `basis`.
struct Baum {
    pfad: PathBuf,
}

impl Baum {
    fn neu() -> Result<Baum, String> {
        let ws = workspace();
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let pfad = ws
            .join("target/kodex-befunde")
            .join(format!("baum_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&pfad);
        fs::create_dir_all(&pfad).map_err(|e| format!("{}: {e}", pfad.display()))?;
        let liste = git(
            &ws,
            &[
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
            ],
        )?;
        for roh in liste.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            let rel = String::from_utf8(roh.to_vec()).map_err(|e| format!("Pfad: {e}"))?;
            let quelle = ws.join(&rel);
            if !quelle.is_file() {
                continue; // im Arbeitsbaum geloescht
            }
            let ziel = pfad.join(&rel);
            if let Some(eltern) = ziel.parent() {
                fs::create_dir_all(eltern).map_err(|e| format!("{rel}: {e}"))?;
            }
            fs::copy(&quelle, &ziel).map_err(|e| format!("{rel}: {e}"))?;
        }
        git(&pfad, &["init", "-q"])?;
        git(&pfad, &["add", "-A"])?;
        git(&pfad, &["commit", "-q", "-m", "basis"])?;
        git(&pfad, &["branch", "basis"])?;
        Ok(Baum { pfad })
    }

    fn datei(&self, rel: &str) -> PathBuf {
        self.pfad.join(rel)
    }

    fn lies(&self, rel: &str) -> Result<String, String> {
        fs::read_to_string(self.datei(rel)).map_err(|e| format!("{rel}: {e}"))
    }

    fn schreibe(&self, rel: &str, inhalt: &str) -> Result<(), String> {
        let ziel = self.datei(rel);
        if let Some(eltern) = ziel.parent() {
            fs::create_dir_all(eltern).map_err(|e| format!("{rel}: {e}"))?;
        }
        fs::write(ziel, inhalt).map_err(|e| format!("{rel}: {e}"))
    }

    /// Ersetzt genau ein Vorkommen von `alt`.
    fn ersetze(&self, rel: &str, alt: &str, neu: &str) -> Result<(), String> {
        let text = self.lies(rel)?;
        if text.matches(alt).count() != 1 {
            return Err(format!("Vorbedingung: {alt:?} nicht genau einmal in {rel}"));
        }
        self.schreibe(rel, &text.replacen(alt, neu, 1))
    }

    fn haenge_an(&self, rel: &str, code: &str) -> Result<(), String> {
        let text = self.lies(rel)?;
        self.schreibe(rel, &format!("{text}\n{code}"))
    }

    fn deklariere(&self, klasse: &str, pfade: &[&str]) -> Result<(), String> {
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let mut t = format!("klasse: {klasse}\nzweck: Kodex-Probe {n}\npfade:\n");
        for p in pfade {
            let _ = writeln!(t, "{p}");
        }
        self.schreibe("kodex/aenderung.txt", &t)
    }

    /// Setzt den Arbeitsbaum auf die Basis zurueck (auch neue Dateien).
    fn zuruecksetzen(&self) -> Result<(), String> {
        git(&self.pfad, &["checkout", "-q", "--", "."])?;
        git(&self.pfad, &["clean", "-q", "-f", "-d"])?;
        Ok(())
    }

    fn gate(&self) -> Vec<String> {
        depot_kodex::pruefe(&self.pfad, "basis")
    }

    /// Kontrolle: eine harmlose, sauber deklarierte Aenderung besteht das Gate.
    fn kontrolle(&self) -> Result<(), String> {
        self.deklariere("DokuOnly", &["docs/kontrolle.md"])?;
        self.schreibe("docs/kontrolle.md", "Kontrolle\n")?;
        let fehler = self.gate();
        self.zuruecksetzen()?;
        if fehler.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "Vorbedingung: die unveraenderte Kopie besteht das Gate nicht: {fehler:#?}"
            ))
        }
    }
}

/// Der Kodex muss ablehnen und dabei `stichwort` nennen.
fn abgelehnt(fall: &str, fehler: &[String], stichwort: &str) -> Result<(), String> {
    if fehler.iter().any(|f| f.contains(stichwort)) {
        return Ok(());
    }
    Err(format!(
        "{fall}: nicht aus dem erwarteten Grund abgelehnt (Stichwort {stichwort:?}): {fehler:#?}"
    ))
}

/// Ein Verstoss: aendert die Kopie und liefert das Urteil des Kodex.
type Verstoss<'a> = dyn Fn(&Baum) -> Result<Vec<String>, String> + 'a;

/// Fuehrt jeden Fall auf derselben Kopie aus und sammelt alle Fehlschlaege.
fn alle_faelle(baum: &Baum, faelle: &[(&str, &Verstoss<'_>, &str)]) -> Result<(), String> {
    let mut offen = Vec::new();
    for (fall, verstoss, stichwort) in faelle {
        let fehler = verstoss(baum)?;
        if let Err(e) = abgelehnt(fall, &fehler, stichwort) {
            offen.push(e);
        }
        baum.zuruecksetzen()?;
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(offen.join("\n\n"))
    }
}

// ---------------------------------------------------------------------------------------------
// 1·G01 – harte Lints aussetzbar (R26)
// ---------------------------------------------------------------------------------------------

const TOML: &str = "crates/depot_kern/Cargo.toml";
const ESSENZ: &str = "docs/essenz.md";
const UNICODE: &str = "docs/prüfung mit leer.md";
const GELD: &str = "crates/depot_kern/src/geld.rs";
const KERN_LIB: &str = "crates/depot_kern/src/lib.rs";

fn funktion_mit(baum: &Baum, code: &str) -> Result<Vec<String>, String> {
    baum.deklariere("Funktion", &[GELD])?;
    baum.haenge_an(GELD, code)?;
    Ok(baum.gate())
}

/// Der Kodex wertet Lint-Attribute syntaktisch aus, samt `cfg_attr`, Verschachtelung und
/// Lintgruppen, die einen harten Lint enthalten.
#[test]
#[ignore = "offen: 1·G01 Lint-Ausnahmen hinter cfg_attr und ueber Gruppen werden nicht erkannt"]
fn befund_1_g01_ausnahme_hinter_cfg_attr_wird_abgelehnt() -> Result<(), String> {
    let baum = Baum::neu()?;
    baum.kontrolle()?;
    alle_faelle(
        &baum,
        &[
            (
                "cfg_attr(not(test), allow(..))",
                &|b| {
                    funktion_mit(
                        b,
                        "#[cfg_attr(not(test), allow(clippy::unwrap_used, reason = \"x\"))]\n\
                         fn probe(x: Option<u8>) -> u8 { x.unwrap() }\n",
                    )
                },
                "unwrap_used",
            ),
            (
                "cfg_attr(all(), expect(..))",
                &|b| {
                    funktion_mit(
                        b,
                        "#[cfg_attr(all(), expect(clippy::indexing_slicing, reason = \"x\"))]\n\
                         fn probe(x: &[u8]) -> u8 { x[0] }\n",
                    )
                },
                "indexing_slicing",
            ),
            (
                "verschachteltes cfg_attr",
                &|b| {
                    funktion_mit(
                        b,
                        "#[cfg_attr(not(test), cfg_attr(all(), allow(clippy::panic, reason = \"x\")))]\n\
                         fn probe() { panic!() }\n",
                    )
                },
                "panic",
            ),
            (
                "Gruppe restriction",
                &|b| {
                    funktion_mit(
                        b,
                        "#[allow(clippy::restriction, reason = \"x\")]\nfn probe() {}\n",
                    )
                },
                "restriction",
            ),
            (
                "Gruppe pedantic",
                &|b| {
                    funktion_mit(
                        b,
                        "#[allow(clippy::pedantic, reason = \"x\")]\nfn probe() {}\n",
                    )
                },
                "pedantic",
            ),
            (
                "unsafe_code ueber cfg_attr",
                &|b| {
                    funktion_mit(
                        b,
                        "#[cfg_attr(not(test), allow(unsafe_code, reason = \"x\"))]\nfn probe() {}\n",
                    )
                },
                "unsafe_code",
            ),
        ],
    )
}

/// Jedes Produkt-Crate verbietet die harten Lints im Kopf mit `forbid`, unbedingt und
/// vollstaendig.
#[test]
#[ignore = "offen: 1·G01 forbid im Crate-Kopf wird nicht geprueft"]
fn befund_1_g01_kopf_ohne_forbid_wird_abgelehnt() -> Result<(), String> {
    let baum = Baum::neu()?;
    baum.kontrolle()?;
    let kopf = baum.lies(KERN_LIB)?;
    let Some(start) = kopf.find("#![forbid(") else {
        return Err(format!("Vorbedingung: {KERN_LIB} hat keinen forbid-Kopf"));
    };
    let Some(laenge) = kopf.get(start..).and_then(|r| r.find(")]")) else {
        return Err(format!(
            "Vorbedingung: forbid-Kopf in {KERN_LIB} nicht geschlossen"
        ));
    };
    let block = kopf
        .get(start..start.saturating_add(laenge).saturating_add(2))
        .unwrap_or_default()
        .to_string();
    if !block.contains("clippy::panic,") {
        return Err("Vorbedingung: clippy::panic fehlt im forbid-Kopf".into());
    }
    let ohne = |b: &Baum, neu: &str| -> Result<Vec<String>, String> {
        b.deklariere("RefactorOnly", &[KERN_LIB])?;
        b.ersetze(KERN_LIB, &block, neu)?;
        Ok(b.gate())
    };
    let innen = block
        .strip_prefix("#![forbid(")
        .and_then(|r| r.strip_suffix(")]"))
        .unwrap_or_default();
    let hinter_cfg = format!("#![cfg_attr(not(test), forbid({innen}))]");
    let ohne_panic = block.replacen("clippy::panic,", "", 1);
    alle_faelle(
        &baum,
        &[
            ("forbid entfernt", &|b| ohne(b, ""), "forbid"),
            (
                "forbid hinter cfg_attr",
                &|b| ohne(b, &hinter_cfg),
                "forbid",
            ),
            (
                "forbid ohne clippy::panic",
                &|b| ohne(b, &ohne_panic),
                "clippy::panic",
            ),
        ],
    )
}

/// Ein Produktiv-Target: Paket, Auswahl fuer cargo und Wurzeldatei.
const TARGETS: [(&str, &str, &str); 6] = [
    ("depot_kern", "--lib", "crates/depot_kern/src/lib.rs"),
    (
        "depot_regelwerk",
        "--lib",
        "crates/depot_regelwerk/src/lib.rs",
    ),
    ("depot_ki", "--lib", "crates/depot_ki/src/lib.rs"),
    ("depot_kodex", "--lib", "crates/depot_kodex/src/lib.rs"),
    ("depot_kodex", "--bins", "crates/depot_kodex/src/main.rs"),
    ("depot_lauf", "--bins", "crates/depot_lauf/src/main.rs"),
];

const VERSTOSS: &str = "
#[cfg_attr(not(test), allow(clippy::unwrap_used, reason = \"Gegenprobe 1·G01\"))]
#[allow(dead_code, reason = \"Gegenprobe 1·G01\")]
/// # Panics
/// Gegenprobe.
fn gegenprobe_g01_a(x: Option<u8>) -> u8 {
    x.unwrap()
}

#[cfg_attr(all(), expect(clippy::arithmetic_side_effects, reason = \"Gegenprobe 1·G01\"))]
#[allow(dead_code, reason = \"Gegenprobe 1·G01\")]
fn gegenprobe_g01_b(x: u8) -> u8 {
    x + 1
}
";

/// Der Compiler selbst lehnt in jedem Produktiv-Target die Ausnahme ab (E0453). Gilt nur, wenn
/// jeder Fehler E0453 ist und beide Lints genannt werden; ein anderer Fehler beweist nichts.
#[test]
#[ignore = "offen: 1·G01, 2·A11 der Compiler lehnt die Ausnahme nicht in jedem Target ab"]
fn befund_1_g01_compiler_lehnt_ausnahme_in_jedem_target_ab() -> Result<(), String> {
    let baum = Baum::neu()?;
    let ziel = workspace().join("target/kodex-sonde");
    let mut offen = Vec::new();
    for (paket, auswahl, wurzel) in TARGETS {
        baum.haenge_an(wurzel, VERSTOSS)?;
        let aus = Command::new(option_env!("CARGO").unwrap_or("cargo"))
            .args([
                "clippy",
                "--quiet",
                "--offline",
                "--locked",
                "-p",
                paket,
                auswahl,
                "--message-format=short",
                "--",
                "-D",
                "warnings",
            ])
            .current_dir(&baum.pfad)
            .env("CARGO_TARGET_DIR", &ziel)
            .output()
            .map_err(|e| format!("cargo nicht startbar: {e}"))?;
        baum.zuruecksetzen()?;
        let text = String::from_utf8_lossy(&aus.stderr).into_owned();
        let fehler: Vec<&str> = text
            .lines()
            .filter(|z| z.contains("error"))
            .filter(|z| !z.contains("could not compile") && !z.contains("aborting due to"))
            .collect();
        let nur_e0453 = !fehler.is_empty() && fehler.iter().all(|z| z.contains("E0453"));
        let nennt = text.contains("allow(clippy::unwrap_used) incompatible with previous forbid")
            && text.contains(
                "expect(clippy::arithmetic_side_effects) incompatible with previous forbid",
            );
        if aus.status.success() || !nur_e0453 || !nennt {
            offen.push(format!("{paket} {auswahl} ({wurzel}):\n{text}"));
        }
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Ausnahme nicht vom Compiler abgelehnt:\n{}",
            offen.join("\n")
        ))
    }
}

// ---------------------------------------------------------------------------------------------
// 1·G02, 2·A11 – Reinheit nur per Textsuche (R26)
// ---------------------------------------------------------------------------------------------

/// Gruppierte, umbenannte und ueber `extern crate` oder Glob erreichte Zugriffe auf `std::env`
/// und `std::fs` im reinen Kern werden erkannt.
#[test]
#[ignore = "offen: 1·G02 gruppierte und umbenannte Importe umgehen die Reinheitspruefung"]
fn befund_1_g02_reinheit_haelt_gegen_gruppierte_importe() -> Result<(), String> {
    let baum = Baum::neu()?;
    baum.kontrolle()?;
    alle_faelle(
        &baum,
        &[
            (
                "gruppierter Import",
                &|b| {
                    funktion_mit(
                        b,
                        "use std::{env, string::String as Text};\n\
                         pub fn probe() -> Option<Text> { env::var(\"X\").ok() }\n",
                    )
                },
                "std::env",
            ),
            (
                "umbenannter Import",
                &|b| {
                    funktion_mit(
                        b,
                        "use std::env as umwelt;\npub fn probe() -> bool { umwelt::var(\"X\").is_ok() }\n",
                    )
                },
                "std::env",
            ),
            (
                "verschachtelter Import mit self",
                &|b| {
                    funktion_mit(
                        b,
                        "use std::{fs::{self as dateien}};\n\
                         pub fn probe() -> bool { dateien::metadata(\"x\").is_ok() }\n",
                    )
                },
                "std::fs",
            ),
            (
                "extern crate std als Alias",
                &|b| {
                    funktion_mit(
                        b,
                        "extern crate std as basis;\npub fn probe() -> bool { basis::env::var(\"X\").is_ok() }\n",
                    )
                },
                "std::env",
            ),
            (
                "Glob-Import aus std",
                &|b| {
                    funktion_mit(
                        b,
                        "use std::*;\npub fn probe() -> bool { env::var(\"X\").is_ok() }\n",
                    )
                },
                "Glob",
            ),
            (
                "Alias der Wurzel",
                &|b| {
                    funktion_mit(
                        b,
                        "use std as s;\npub fn probe() -> bool { s::env::var(\"X\").is_ok() }\n",
                    )
                },
                "std::env",
            ),
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// 1·G03, 2·A11 – Lint-Vererbung per Text vorgetaeuscht (R26)
// ---------------------------------------------------------------------------------------------

/// Die Zeichenfolge `[lints]\nworkspace = true` in einem Text-Wert ist keine Vererbung.
#[test]
#[ignore = "offen: 1·G03 Lint-Vererbung wird per Textsuche geprueft"]
fn befund_1_g03_vorgetaeuschte_lint_vererbung_wird_abgelehnt() -> Result<(), String> {
    let baum = Baum::neu()?;
    baum.kontrolle()?;
    alle_faelle(
        &baum,
        &[
            (
                "Vererbung nur in einem mehrzeiligen Text",
                &|b| {
                    b.deklariere("Funktion", &[TOML])?;
                    b.ersetze(
                        TOML,
                        "[lints]\nworkspace = true",
                        "[package.metadata.depot]\nnotiz = \"\"\"\n[lints]\nworkspace = true\n\"\"\"",
                    )?;
                    Ok(b.gate())
                },
                "Workspace-Lints",
            ),
            (
                "Vererbung abgeschaltet",
                &|b| {
                    b.deklariere("Funktion", &[TOML])?;
                    b.ersetze(
                        TOML,
                        "[lints]\nworkspace = true",
                        "[lints]\nworkspace = false\n# [lints]\n# workspace = true",
                    )?;
                    Ok(b.gate())
                },
                "Workspace-Lints",
            ),
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// 1·G04 – Abhaengigkeiten nur nach Alias festgeschrieben (R26)
// ---------------------------------------------------------------------------------------------

/// Version, Paket, Features und `Cargo.lock` gehoeren zur Festschreibung.
#[test]
#[ignore = "offen: 1·G04 Abhaengigkeiten nur nach Alias festgeschrieben"]
fn befund_1_g04_abhaengigkeit_mit_anderer_identitaet_wird_abgelehnt() -> Result<(), String> {
    let baum = Baum::neu()?;
    baum.kontrolle()?;
    let lock = baum.lies("Cargo.lock")?;
    let Some(zeile) = lock.lines().find(|z| z.starts_with("checksum = ")) else {
        return Err("Vorbedingung: Cargo.lock ohne checksum".into());
    };
    let zeile = zeile.to_string();
    let falsch = format!("checksum = \"{}\"", "0".repeat(64));
    alle_faelle(
        &baum,
        &[
            (
                "andere Features",
                &|b| {
                    b.deklariere("Funktion", &[TOML])?;
                    b.ersetze(
                        TOML,
                        "sha2 = { workspace = true }",
                        "sha2 = { version = \"0.10\", default-features = false }",
                    )?;
                    Ok(b.gate())
                },
                "Festschreibung verletzt",
            ),
            (
                "anderes Paket unter gleichem Namen",
                &|b| {
                    b.deklariere("Funktion", &[TOML])?;
                    b.ersetze(
                        TOML,
                        "sha2 = { workspace = true }",
                        "sha2 = { package = \"sha3\", version = \"0.10\" }",
                    )?;
                    Ok(b.gate())
                },
                "Festschreibung verletzt",
            ),
            (
                "andere Version im Workspace",
                &|b| {
                    b.deklariere("Funktion", &["Cargo.toml"])?;
                    b.ersetze("Cargo.toml", "sha2 = \"0.10\"", "sha2 = \"0.9\"")?;
                    Ok(b.gate())
                },
                "Festschreibung verletzt",
            ),
            (
                "veraenderte Cargo.lock",
                &|b| {
                    b.deklariere("Funktion", &["Cargo.lock"])?;
                    b.ersetze("Cargo.lock", &zeile, &falsch)?;
                    Ok(b.gate())
                },
                "Festschreibung verletzt",
            ),
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// 1·G05 – Essenz kann verschwinden (R26)
// ---------------------------------------------------------------------------------------------

/// Die Essenz hat ein festes Register. Fehlt oder aendert sich eine Regel, ist der Kodex rot.
#[test]
#[ignore = "offen: 1·G05 Essenz ohne festes Register"]
fn befund_1_g05_geloeschte_oder_umbenannte_regel_wird_abgelehnt() -> Result<(), String> {
    let baum = Baum::neu()?;
    baum.kontrolle()?;
    let text = baum.lies(ESSENZ)?;
    let bis = |nr: &str| -> Result<String, String> {
        text.find(&format!("\n# {nr} – "))
            .and_then(|i| text.get(..=i))
            .map(str::to_string)
            .ok_or_else(|| format!("Vorbedingung: {nr} fehlt in der Essenz"))
    };
    let ohne_d17 = bis("D17")?;
    let nur_d00 = bis("D01")?;
    let Some(titel) = text.lines().find(|z| z.starts_with("# D05 – ")) else {
        return Err("Vorbedingung: D05 fehlt".into());
    };
    let titel = titel.to_string();
    alle_faelle(
        &baum,
        &[
            (
                "letzte Regel geloescht",
                &|b| {
                    b.deklariere("DokuOnly", &[ESSENZ])?;
                    b.schreibe(ESSENZ, &ohne_d17)?;
                    Ok(b.gate())
                },
                "D17",
            ),
            (
                "alle Regeln ab D01 geloescht",
                &|b| {
                    b.deklariere("DokuOnly", &[ESSENZ])?;
                    b.schreibe(ESSENZ, &nur_d00)?;
                    Ok(b.gate())
                },
                "D01",
            ),
            (
                "Regel umbenannt",
                &|b| {
                    b.deklariere("DokuOnly", &[ESSENZ])?;
                    b.ersetze(ESSENZ, &titel, "# D05 – Grenzen sind verhandelbar")?;
                    Ok(b.gate())
                },
                "D05",
            ),
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// 1·G06 – Unicode-Dateinamen (R24)
// ---------------------------------------------------------------------------------------------

/// Eine deklarierte Datei mit Umlaut und Leerzeichen besteht das Gate; eine undeklarierte wird
/// mit ihrem echten Namen gemeldet.
#[test]
#[ignore = "offen: 1·G06 Pfade werden zeilenweise und quotiert gelesen"]
fn befund_1_g06_unicode_pfade_werden_richtig_geprueft() -> Result<(), String> {
    let baum = Baum::neu()?;
    baum.kontrolle()?;
    baum.deklariere("DokuOnly", &[UNICODE])?;
    baum.schreibe(UNICODE, "Umlaut und Leerzeichen\n")?;
    let fehler = baum.gate();
    baum.zuruecksetzen()?;
    if !fehler.is_empty() {
        return Err(format!("deklarierte Unicode-Datei abgelehnt: {fehler:#?}"));
    }
    baum.deklariere("DokuOnly", &["docs/kontrolle.md"])?;
    baum.schreibe("docs/kontrolle.md", "Kontrolle\n")?;
    baum.schreibe("docs/früh.md", "nicht deklariert\n")?;
    let fehler = baum.gate();
    abgelehnt("undeklarierte Unicode-Datei", &fehler, "docs/früh.md")
}

// ---------------------------------------------------------------------------------------------
// 1·R03 – Lesefehler beim Scan uebersprungen (R29)
// ---------------------------------------------------------------------------------------------

/// Ein Verzeichnis, das sich nicht lesen laesst, und ein Link im Quellbaum machen den Kodex rot.
/// Geprueft ohne Git (`pruefe_ohne_diff`), damit Deklaration und Pfadrollen nicht vorher greifen.
///
/// Grenze: Als Administrator laesst sich kein unlesbares Verzeichnis anlegen. Der Fall mit
/// Dateirechten 000 laeuft deshalb nur, wenn das Lesen auch wirklich scheitert; sonst steht
/// „nicht ausgefuehrt“ in der Ausgabe. Die beiden anderen Faelle laufen immer.
#[test]
#[ignore = "offen: 1·R03 Lesefehler beim Scan werden uebersprungen"]
fn befund_1_r03_lesefehler_beim_scan_macht_den_kodex_rot() -> Result<(), String> {
    let baum = Baum::neu()?;
    let vorher = depot_kodex::pruefe_ohne_diff(&baum.pfad);
    if !vorher.is_empty() {
        return Err(format!(
            "Vorbedingung: Kopie ohne Git nicht sauber: {vorher:#?}"
        ));
    }
    let mut offen = Vec::new();

    // Ein Verzeichnis, das keins ist: `tests` als Datei.
    let tests = baum.datei("crates/depot_kern/tests");
    fs::remove_dir_all(&tests).map_err(|e| format!("tests: {e}"))?;
    fs::write(&tests, "keine Liste\n").map_err(|e| format!("tests: {e}"))?;
    let fehler = depot_kodex::pruefe_ohne_diff(&baum.pfad);
    if let Err(e) = abgelehnt("tests ist kein Verzeichnis", &fehler, "nicht lesbar") {
        offen.push(e);
    }
    fs::remove_file(&tests).map_err(|e| format!("tests: {e}"))?;
    baum.zuruecksetzen()?;

    #[cfg(unix)]
    {
        // Ein Link ins Leere im Quellbaum.
        let link = baum.datei("crates/depot_kern/src/modul");
        std::os::unix::fs::symlink("/nirgends/depotschlopf", &link)
            .map_err(|e| format!("Link: {e}"))?;
        let fehler = depot_kodex::pruefe_ohne_diff(&baum.pfad);
        if let Err(e) = abgelehnt("Link im Quellbaum", &fehler, "Link") {
            offen.push(e);
        }
        fs::remove_file(&link).map_err(|e| format!("Link: {e}"))?;

        // Ein Verzeichnis ohne Leserecht.
        let dir = baum.datei("crates/depot_kern/src/gesperrt");
        fs::create_dir_all(&dir).map_err(|e| format!("gesperrt: {e}"))?;
        fs::write(dir.join("x.rs"), "pub fn x() {}\n").map_err(|e| format!("x.rs: {e}"))?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o000))
            .map_err(|e| format!("Rechte: {e}"))?;
        if fs::read_dir(&dir).is_err() {
            let fehler = depot_kodex::pruefe_ohne_diff(&baum.pfad);
            if let Err(e) = abgelehnt("Verzeichnis ohne Leserecht", &fehler, "nicht lesbar") {
                offen.push(e);
            }
        } else {
            println!("1·R03 Verzeichnis ohne Leserecht: nicht ausgefuehrt (Administratorrechte)");
        }
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Rechte: {e}"))?;
        fs::remove_dir_all(&dir).map_err(|e| format!("gesperrt: {e}"))?;
    }

    if offen.is_empty() {
        Ok(())
    } else {
        Err(offen.join("\n\n"))
    }
}

// ---------------------------------------------------------------------------------------------
// 1·R08 – Eigentuemerfreigabe nur prozedural (R30)
// ---------------------------------------------------------------------------------------------

const FREIGABE: &str = "kodex/freigabe.txt";

/// Eine Grenzaenderung: ein Lint wird strenger, die Festschreibung neu berechnet.
fn grenzaenderung(b: &Baum, mit_freigabe: bool, weitere: &[&str]) -> Result<(), String> {
    let mut pfade = vec!["Cargo.toml", "kodex/festschreibung.txt"];
    if mit_freigabe {
        pfade.push(FREIGABE);
    }
    pfade.extend_from_slice(weitere);
    b.deklariere("FestschreibungAenderung", &pfade)?;
    b.ersetze(
        "Cargo.toml",
        "missing_debug_implementations = \"warn\"",
        "missing_debug_implementations = \"deny\"",
    )?;
    let f = depot_kodex::festschreibung::berechne(&b.pfad)?;
    b.schreibe(
        "kodex/festschreibung.txt",
        &depot_kodex::festschreibung::als_text(&f),
    )
}

/// Unabhaengige Nachrechnung des Diff-Hashes: je geaenderter Pfad (ohne die Freigabe selbst)
/// eine Zeile `pfad TAB sha256(inhalt)` oder `pfad TAB geloescht`, nach Pfad sortiert.
fn diff_hash(b: &Baum) -> Result<String, String> {
    let mut pfade: Vec<String> = Vec::new();
    for args in [
        &["diff", "-z", "--name-only", "basis"][..],
        &["ls-files", "-z", "--others", "--exclude-standard"][..],
    ] {
        for roh in git(&b.pfad, args)?
            .split(|x| *x == 0)
            .filter(|r| !r.is_empty())
        {
            pfade.push(String::from_utf8(roh.to_vec()).map_err(|e| format!("Pfad: {e}"))?);
        }
    }
    pfade.sort();
    pfade.dedup();
    let mut text = String::new();
    for p in pfade.iter().filter(|p| p.as_str() != FREIGABE) {
        match fs::read(b.datei(p)) {
            Ok(inhalt) => {
                let _ = writeln!(text, "{p}\t{}", sha_hex(&inhalt));
            }
            Err(_) => {
                let _ = writeln!(text, "{p}\tgeloescht");
            }
        }
    }
    Ok(sha_hex(text.as_bytes()))
}

fn freigabe(diff: &str, festschreibung: &str) -> String {
    format!(
        "typ: Grenzfreigabe\ndiff: {diff}\nfestschreibung: {festschreibung}\nerteilt: Wolfgang\n"
    )
}

fn festschreibung_hash(b: &Baum) -> Result<String, String> {
    fs::read(b.datei("kodex/festschreibung.txt"))
        .map(|t| sha_hex(&t))
        .map_err(|e| format!("festschreibung: {e}"))
}

/// Eine Grenzaenderung braucht eine Freigabe, die an Diff und neue Festschreibung gebunden ist.
/// Ohne Freigabe, mit falschem Hash oder nach einer spaeteren Aenderung wird abgelehnt; die
/// passende Freigabe besteht.
#[test]
#[ignore = "offen: 1·R08 Grenzaenderung ohne gebundene Freigabe wird angenommen"]
fn befund_1_r08_freigabe_ohne_passenden_hash_wird_abgelehnt() -> Result<(), String> {
    let baum = Baum::neu()?;
    baum.kontrolle()?;
    alle_faelle(
        &baum,
        &[
            (
                "ohne Freigabe",
                &|b| {
                    grenzaenderung(b, false, &[])?;
                    Ok(b.gate())
                },
                "Grenzfreigabe",
            ),
            (
                "Freigabe fuer eine andere Festschreibung",
                &|b| {
                    grenzaenderung(b, true, &[])?;
                    b.schreibe(FREIGABE, &freigabe("0", "0"))?;
                    let d = diff_hash(b)?;
                    b.schreibe(FREIGABE, &freigabe(&d, &"0".repeat(64)))?;
                    Ok(b.gate())
                },
                "Grenzfreigabe",
            ),
            (
                "nach der Freigabe weiter geaendert",
                &|b| {
                    grenzaenderung(b, true, &["docs/nachtrag.md"])?;
                    b.schreibe(FREIGABE, &freigabe("0", "0"))?;
                    let d = diff_hash(b)?;
                    let f = festschreibung_hash(b)?;
                    b.schreibe(FREIGABE, &freigabe(&d, &f))?;
                    b.schreibe("docs/nachtrag.md", "nachgeschoben\n")?;
                    Ok(b.gate())
                },
                "Grenzfreigabe",
            ),
        ],
    )?;
    // Gegenstueck: die passende Freigabe besteht.
    grenzaenderung(&baum, true, &[])?;
    baum.schreibe(FREIGABE, &freigabe("0", "0"))?;
    let d = diff_hash(&baum)?;
    let f = festschreibung_hash(&baum)?;
    baum.schreibe(FREIGABE, &freigabe(&d, &f))?;
    let fehler = baum.gate();
    if fehler.is_empty() {
        Ok(())
    } else {
        Err(format!("passende Freigabe abgelehnt: {fehler:#?}"))
    }
}
