//! Kodex-Befunde 1·G01 bis 1·G06, 1·R03 und 1·R08 im Quellscan: Lint-Ausnahmen, Reinheit, Lint-Vererbung, Abhängigkeiten, Essenz, Unicode-Pfade, Lesefehler und Grenzfreigabe.
//!
//! Prüfkopie und Hilfen stehen in `befunde_gemeinsam/mod.rs`; Aufbau wie in `befunde.rs`.
//! Eigenes Testprogramm, damit das Gate es parallel startet (C1, #190).

mod befunde_gemeinsam;

use std::fmt::Write as _;
use std::fs;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;

use befunde_gemeinsam::{
    Baum, ESSENZ, KERN_LIB, TOML, UNICODE, abgelehnt, alle_faelle, funktion_mit, git, sha_hex,
};

/// Der Kodex wertet Lint-Attribute syntaktisch aus, samt `cfg_attr`, Verschachtelung und
/// Lintgruppen, die einen harten Lint enthalten.
#[test]
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

// ---------------------------------------------------------------------------------------------
// 1·G02, 2·A11 – Reinheit nur per Textsuche (R26)
// ---------------------------------------------------------------------------------------------

/// Gruppierte, umbenannte und ueber `extern crate` oder Glob erreichte Zugriffe auf `std::env`
/// und `std::fs` im reinen Kern werden erkannt.
#[test]
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
    fs::remove_dir_all(&tests)
        .map_err(|e| format!("Testverzeichnis entfernen {}: {e}", tests.display()))?;
    fs::write(&tests, "keine Liste\n").map_err(|e| {
        format!(
            "Testdatei anlegen {}: {e}; Typ={:?}",
            tests.display(),
            fs::symlink_metadata(&tests).map(|m| m.file_type())
        )
    })?;
    let fehler = depot_kodex::pruefe_ohne_diff(&baum.pfad);
    if let Err(e) = abgelehnt("tests ist kein Verzeichnis", &fehler, "nicht lesbar") {
        offen.push(e);
    }
    fs::remove_file(&tests).map_err(|e| {
        format!(
            "Testdatei entfernen {}: {e}; Typ={:?}",
            tests.display(),
            fs::symlink_metadata(&tests).map(|m| m.file_type())
        )
    })?;
    // Dieser Fall ersetzt ein ganzes Verzeichnis außerhalb von `schreibe`.
    git(&baum.pfad, &["reset", "--hard", "-q"])?;

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
