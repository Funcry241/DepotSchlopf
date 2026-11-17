//! Negativtests zu den Kodex-Befunden aus `docs/BEFUNDE.md` (Stufe 2, Kodex-Schritt).
//!
//! Der Testprozess erfasst den Arbeitsbaum einmal als Git-Bundle. Jeder Test klont daraus
//! ein eigenes Git-Repository mit der Basis `basis` und führt dort seine Verstöße ein. Der Kodex muss ihn ablehnen, und zwar aus dem richtigen Grund: Die Ablehnung muss ein
//! erwartetes Stichwort nennen. Vor jedem Verstoss prueft eine Kontrolle, dass dieselbe Kopie
//! mit einer harmlosen, sauber deklarierten Aenderung das Gate besteht. Eine kaputte Kopie macht
//! einen Test also nie gruen.
//!
//! Die Tests nutzen nur `depot_kodex::pruefe`, `pruefe_ohne_diff` und die Festschreibung
//! (`berechne`, `als_text`). Diese Schnittstelle bleibt beim Umbau gleich.
//!
//! Hier stehen die Befunde K7, W3, K10, Netz, RSS und die Prüfkopie selbst. G01 bis R08 stehen
//! in `befunde_regeln.rs`, die Compiler-Gegenprobe zu G01 in `befunde_compiler.rs`. Es sind
//! eigene Testprogramme, damit das Gate sie parallel startet (C1, #190).

mod befunde_gemeinsam;

use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use befunde_gemeinsam::{
    Baum, GELD, KERN_LIB, ZAEHLER, abgelehnt, alle_faelle, funktion_mit, git, reserviere_ordner,
};

// ---------------------------------------------------------------------------------------------
// Pruefung von PR #7 (Maus, Stand ce7b978): K7-01 bis K7-03, Gegenbeispiele unveraendert
// ---------------------------------------------------------------------------------------------

/// Setzt Code vor das Testmodul von `geld.rs`, wie im Gegenbeispiel.
fn vor_testmodul(b: &Baum, code: &str) -> Result<(), String> {
    b.ersetze(GELD, "#[cfg(test)]", &format!("{code}\n#[cfg(test)]"))
}

/// K7-01 (`scope_alias`): Derselbe Alias in zwei Funktionen hat zwei Bedeutungen. Die spaetere
/// darf die verbotene nicht verdecken.
#[test]
fn befund_k7_01_alias_in_anderer_funktion_verdeckt_std_nicht() -> Result<(), String> {
    let baum = Baum::neu()?;
    baum.kontrolle()?;
    baum.deklariere("Funktion", &[GELD])?;
    vor_testmodul(
        &baum,
        "pub fn audit_bad(path: &str) -> bool {
    use std as basis;
    basis::fs::metadata(path).is_ok()
}

pub fn audit_good() -> usize {
    use core as basis;
    basis::mem::size_of::<u8>()
}
",
    )?;
    abgelehnt("scope_alias", &baum.gate(), "std::fs")
}

/// K7-02 (`local_reexport`): Ein Modul im selben Crate reicht `std` unter anderem Namen weiter.
#[test]
fn befund_k7_02_interner_reexport_von_std_wird_abgelehnt() -> Result<(), String> {
    let baum = Baum::neu()?;
    baum.kontrolle()?;
    baum.deklariere("Funktion", &[GELD])?;
    vor_testmodul(
        &baum,
        "pub mod audit_bridge {
    pub use std as platform;
}

pub fn audit_bad(path: &str) -> bool {
    crate::geld::audit_bridge::platform::fs::metadata(path).is_ok()
}
",
    )?;
    abgelehnt("local_reexport", &baum.gate(), "platform")
}

const KERN_TOML: &str = "crates/depot_kern/Cargo.toml";
const ZIEL: &str = "crates/depot_kern/src/audit_target.md";
const NUTZLAST: &str = "crates/depot_kern/src/audit_payload.md";

/// K7-03 (`target_md`): Cargo baut ein Programm aus einer `.md`-Datei. Geprueft wird, was
/// kompiliert wird, nicht was die Endung sagt.
#[test]
fn befund_k7_03_target_aus_md_wird_geprueft() -> Result<(), String> {
    let baum = Baum::neu()?;
    baum.kontrolle()?;
    baum.deklariere("Funktion", &[KERN_TOML, ZIEL])?;
    baum.ersetze(
        KERN_TOML,
        "[lints]\nworkspace = true",
        "[[bin]]\nname = \"audit-target\"\npath = \"src/audit_target.md\"\n\n[lints]\nworkspace = true",
    )?;
    let kopf = format!(
        "#![forbid(\n    {}\n)]\n\nfn main() {{\n    let _result = std::fs::metadata(\".\");\n}}\n",
        [
            "unsafe_code",
            "clippy::unwrap_used",
            "clippy::expect_used",
            "clippy::panic",
            "clippy::indexing_slicing",
            "clippy::arithmetic_side_effects",
            "clippy::float_arithmetic",
            "clippy::cast_possible_truncation",
            "clippy::cast_sign_loss",
        ]
        .join(",\n    ")
    );
    baum.schreibe(ZIEL, &kopf)?;
    abgelehnt("target_md", &baum.gate(), "audit_target.md")
}

/// K7-03 (`module_md`): Ein Modul der Kernbibliothek wird ueber `#[path]` aus einer `.md`-Datei
/// gebaut.
#[test]
fn befund_k7_03_modul_aus_md_wird_geprueft() -> Result<(), String> {
    let baum = Baum::neu()?;
    baum.kontrolle()?;
    baum.deklariere("Funktion", &[KERN_LIB, NUTZLAST])?;
    baum.ersetze(
        KERN_LIB,
        "pub mod geld;",
        "pub mod geld;\n#[path = \"audit_payload.md\"]\npub mod audit_payload;",
    )?;
    baum.schreibe(
        NUTZLAST,
        "pub fn audit_bad(path: &str) -> bool {\n    std::fs::metadata(path).is_ok()\n}\n",
    )?;
    abgelehnt("module_md", &baum.gate(), "audit_payload.md")
}

// ---------------------------------------------------------------------------------------------
// 2·W3 und 1·E01 (Gate-Teil) – Geheimnisse in Dateien des Projekts (R28)
// ---------------------------------------------------------------------------------------------

/// Beispielgeheimnisse, erst zur Laufzeit zusammengesetzt: Diese Quelldatei enthaelt selbst
/// keines und besteht deshalb ihre eigene Pruefung. Die IBAN ist das oeffentliche Lehrbuch-
/// beispiel mit gueltiger Pruefziffer, die Schluessel sind erfunden.
fn beispiel_iban() -> String {
    ["DE89", "3704", "0044", "0532", "0130", "00"].concat()
}

fn beispiel_iban_mit_leerzeichen() -> String {
    ["DE89", "3704", "0044", "0532", "0130", "00"].join(" ")
}

fn beispiel_api_schluessel() -> String {
    ["sk", "-ant-", "api03-", "q7Rk2LmZx9Tb4VwN8yHc3PdJ6sFg1Ue5"].concat()
}

fn beispiel_privater_schluessel() -> String {
    [
        "-----BEGIN ",
        "PRIVATE KEY-----\n",
        "MIIEvQIBADANBgkqhkiG9w0BAQEFAASC\n",
        "-----END ",
        "PRIVATE KEY-----\n",
    ]
    .concat()
}

fn beispiel_zuweisung() -> String {
    ["broker_", "pass", "word = \"", "Wz7#kP2m-Lq9vT4x", "\"\n"].concat()
}

/// Das Gate lehnt jede Datei ab, die nach einem Geheimnis aussieht, gleich welche Rolle sie hat
/// und ob sie deklariert ist. Die Meldung nennt Datei und Zeile, nie das Geheimnis selbst.
#[test]
fn befund_2_w3_geheimnis_in_projektdatei_wird_abgelehnt() -> Result<(), String> {
    let baum = Baum::neu()?;
    baum.kontrolle()?;
    let doku = |inhalt: String| {
        move |b: &Baum| -> Result<Vec<String>, String> {
            b.deklariere("DokuOnly", &["docs/notiz.md"])?;
            b.schreibe("docs/notiz.md", &format!("# Notiz\n\n{inhalt}\n"))?;
            Ok(b.gate())
        }
    };
    let api = doku(beispiel_api_schluessel());
    let iban = doku(beispiel_iban_mit_leerzeichen());
    let schluessel = doku(beispiel_privater_schluessel());
    let zuweisung = doku(beispiel_zuweisung());
    let im_code = |b: &Baum| -> Result<Vec<String>, String> {
        funktion_mit(b, &format!("// Konto: {}\n", beispiel_iban()))
    };
    let im_test = |b: &Baum| -> Result<Vec<String>, String> {
        b.deklariere("TestOnly", &["crates/depot_kern/tests/probe.rs"])?;
        b.schreibe(
            "crates/depot_kern/tests/probe.rs",
            &format!(
                "#[test]\nfn probe() {{ let _ = \"{}\"; }}\n",
                beispiel_api_schluessel()
            ),
        )?;
        Ok(b.gate())
    };
    let ohne_git = |b: &Baum| -> Result<Vec<String>, String> {
        b.schreibe("docs/notiz.md", &beispiel_api_schluessel())?;
        Ok(depot_kodex::pruefe_ohne_diff(&b.pfad))
    };
    alle_faelle(
        &baum,
        &[
            ("API-Schluessel in der Doku", &api, "Geheimnismuster"),
            ("IBAN mit Leerzeichen in der Doku", &iban, "Geheimnismuster"),
            (
                "privater Schluessel in der Doku",
                &schluessel,
                "Geheimnismuster",
            ),
            (
                "Passwort-Zuweisung in der Doku",
                &zuweisung,
                "Geheimnismuster",
            ),
            ("IBAN im Produktivcode", &im_code, "Geheimnismuster"),
            ("API-Schluessel in einem Test", &im_test, "Geheimnismuster"),
            ("API-Schluessel ohne Git", &ohne_git, "Geheimnismuster"),
        ],
    )?;
    // Die Meldung nennt das Geheimnis nie.
    api(&baum).and_then(|fehler| {
        baum.zuruecksetzen()?;
        let geheim = beispiel_api_schluessel();
        if fehler.iter().any(|f| f.contains(&geheim)) {
            Err(format!("Meldung enthaelt das Geheimnis: {fehler:#?}"))
        } else {
            Ok(())
        }
    })
}

// ---------------------------------------------------------------------------------------------
// K10 – Maus' Gegenproben zu PR #10
// ---------------------------------------------------------------------------------------------

/// K10-01: Ein Commit nimmt den Inhalt aus dem Index, nicht aus dem Arbeitsbaum. Das Gate prueft
/// beide: Schluessel im Index und harmloser Text im Arbeitsbaum wird abgelehnt, ebenso die
/// umgekehrte Lage.
#[test]
fn befund_k10_01_index_inhalt_wird_geprueft() -> Result<(), String> {
    let baum = Baum::neu()?;
    baum.kontrolle()?;
    let nur_im_index = |b: &Baum| -> Result<Vec<String>, String> {
        b.deklariere("DokuOnly", &["docs/notiz.md"])?;
        b.schreibe(
            "docs/notiz.md",
            &format!("# Notiz\n{}\n", beispiel_api_schluessel()),
        )?;
        git(&b.pfad, &["add", "docs/notiz.md"])?;
        b.schreibe("docs/notiz.md", "# Notiz\nHarmloser Text\n")?;
        let fehler = b.gate();
        git(&b.pfad, &["reset", "-q", "--", "docs/notiz.md"])?;
        if !fehler
            .iter()
            .any(|m| m.starts_with("docs/notiz.md (Index), Zeile") && m.contains("Geheimnismuster"))
        {
            return Err(format!("Indexfund fehlt: {fehler:?}"));
        }
        Ok(fehler)
    };
    let nur_im_arbeitsbaum = |b: &Baum| -> Result<Vec<String>, String> {
        b.deklariere("DokuOnly", &["docs/notiz.md"])?;
        b.schreibe("docs/notiz.md", "# Notiz\nHarmloser Text\n")?;
        git(&b.pfad, &["add", "docs/notiz.md"])?;
        b.schreibe(
            "docs/notiz.md",
            &format!("# Notiz\n{}\n", beispiel_api_schluessel()),
        )?;
        let fehler = b.gate();
        git(&b.pfad, &["reset", "-q", "--", "docs/notiz.md"])?;
        if !fehler
            .iter()
            .any(|m| m.starts_with("docs/notiz.md, Zeile") && m.contains("Geheimnismuster"))
        {
            return Err(format!("Arbeitsbaumfund fehlt: {fehler:?}"));
        }
        Ok(fehler)
    };
    alle_faelle(
        &baum,
        &[
            ("Schluessel nur im Index", &nur_im_index, "Geheimnismuster"),
            (
                "Schluessel nur im Arbeitsbaum",
                &nur_im_arbeitsbaum,
                "Geheimnismuster",
            ),
        ],
    )
}

fn utf16(text: &str, gross: bool, bom: bool) -> Vec<u8> {
    let mut aus = Vec::new();
    let einheiten = if bom { vec![0xFEFF_u16] } else { Vec::new() };
    for e in einheiten.into_iter().chain(text.encode_utf16()) {
        if gross {
            aus.extend(e.to_be_bytes());
        } else {
            aus.extend(e.to_le_bytes());
        }
    }
    aus
}

/// K10-02: Dieselben Muster werden auch in UTF-16 erkannt, mit und ohne BOM. Was sich in keiner
/// unterstuetzten Kodierung lesen laesst, gilt nicht still als geheimnisfrei, sondern wird als
/// nicht pruefbar abgelehnt.
#[test]
fn befund_k10_02_utf16_umgeht_die_muster_nicht() -> Result<(), String> {
    let baum = Baum::neu()?;
    baum.kontrolle()?;
    // Mit BOM enthaelt der Text einen Umlaut: Er laesst sich nur als UTF-16 richtig lesen.
    // Ohne BOM bleibt er ASCII; dann steht zwischen den Zeichen je ein Nullbyte.
    let text = format!("# Notiz\n{}\n", beispiel_api_schluessel());
    let mit_umlaut = format!("# Schl\u{fc}ssel\n{}\n", beispiel_api_schluessel());
    let mit_bytes = |bytes: Vec<u8>| {
        move |b: &Baum| -> Result<Vec<String>, String> {
            b.deklariere("DokuOnly", &["docs/notiz.md"])?;
            b.schreibe_bytes("docs/notiz.md", &bytes)?;
            Ok(b.gate())
        }
    };
    let le_bom = mit_bytes(utf16(&mit_umlaut, false, true));
    let be_bom = mit_bytes(utf16(&mit_umlaut, true, true));
    let le_ohne = mit_bytes(utf16(&text, false, false));
    let be_ohne = mit_bytes(utf16(&text, true, false));
    let kaputt = mit_bytes(vec![b'#', b' ', 0xC3, 0x28, 0xFF, b'\n']);
    alle_faelle(
        &baum,
        &[
            ("UTF-16LE mit BOM", &le_bom, "Geheimnismuster"),
            ("UTF-16BE mit BOM", &be_bom, "Geheimnismuster"),
            ("UTF-16LE ohne BOM", &le_ohne, "Geheimnismuster"),
            ("UTF-16BE ohne BOM", &be_ohne, "Geheimnismuster"),
            ("nicht lesbare Kodierung", &kaputt, "nicht prüfbar"),
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// Netzzugang bleibt im Blatt (Kap. 6)
// ---------------------------------------------------------------------------------------------

const JOURNAL_TOML: &str = "crates/depot_journal/Cargo.toml";

/// Netzzugang hat nur ein Crate aus der Erlaubnisliste. Kein anderes Crate bekommt ihn ueber eine
/// Abhaengigkeit: weder von einem Netz-Crate, auch nicht umbenannt oder nur fuer ein Zielsystem,
/// noch von einem Crate, das Netz haben darf.
#[test]
fn netz_bleibt_im_blatt() -> Result<(), String> {
    let baum = Baum::neu()?;
    baum.kontrolle()?;
    let abhaengig = |zeilen: &'static str| {
        move |b: &Baum| -> Result<Vec<String>, String> {
            b.deklariere("Funktion", &[JOURNAL_TOML])?;
            b.ersetze(
                JOURNAL_TOML,
                "sha2.workspace = true\n",
                &format!("sha2.workspace = true\n{zeilen}"),
            )?;
            Ok(b.gate())
        }
    };
    let direkt = abhaengig("ureq = \"3\"\n");
    let umbenannt = abhaengig(
        "\n[target.'cfg(windows)'.dependencies]\nweb = { package = \"ureq\", version = \"3\" }\n",
    );
    let ueber_sammler = abhaengig("depot_sammler = { path = \"../depot_sammler\" }\n");
    let ueber_berater = abhaengig("depot_berater = { path = \"../depot_berater\" }\n");
    let ueber_dienst = abhaengig("depot_dienst = { path = \"../depot_dienst\" }\n");
    alle_faelle(
        &baum,
        &[
            ("Netz-Crate direkt", &direkt, "Netzzugang ohne Erlaubnis"),
            (
                "Netz-Crate umbenannt fuer ein Zielsystem",
                &umbenannt,
                "Netzzugang ohne Erlaubnis",
            ),
            (
                "ueber den Sammler",
                &ueber_sammler,
                "Netzzugang ohne Erlaubnis",
            ),
            (
                "ueber den Berater",
                &ueber_berater,
                "Netzzugang ohne Erlaubnis",
            ),
            (
                "ueber den Dienst",
                &ueber_dienst,
                "Netzzugang ohne Erlaubnis",
            ),
        ],
    )
}

#[test]
fn rss_quellen_aendern_nur_mit_festschreibung() -> Result<(), String> {
    let b = Baum::neu()?;
    let pfad = "crates/depot_sammler/src/rss_quellen.rs";
    b.schreibe(pfad, "https://example.com/erlaubt\n")?;
    let fest = depot_kodex::festschreibung::berechne(&b.pfad)?;
    b.schreibe(
        "kodex/festschreibung.txt",
        &depot_kodex::festschreibung::als_text(&fest),
    )?;
    let pruefe = || {
        depot_kodex::festschreibung::pruefe(
            &b.pfad,
            depot_kodex::deklaration::Klasse::Funktion,
            &std::collections::BTreeSet::default(),
        )
    };
    assert!(pruefe().is_empty());
    b.schreibe(pfad, "https://example.com/erlaubt\r\n")?;
    assert!(
        pruefe().is_empty(),
        "Windows-Zeilenenden aendern die Liste nicht"
    );
    b.schreibe(pfad, "https://example.com/fremd\n")?;
    assert!(pruefe().iter().any(|f| f.contains("rss_quellen")));
    fs::remove_file(b.datei(pfad)).map_err(|e| e.to_string())?;
    assert!(pruefe().iter().any(|f| f.contains("rss_quellen")));
    Ok(())
}

#[test]
fn pruefbasis_enthaelt_arbeitsstand_und_haelt_kopien_getrennt() -> Result<(), String> {
    let quelle = Baum::reserviere()?;
    git(&quelle.pfad, &["init", "-q"])?;
    quelle.schreibe(".gitignore", "ignoriert.txt\n")?;
    quelle.schreibe("geaendert.txt", "Index\n")?;
    quelle.schreibe("geloescht.txt", "entfällt\n")?;
    git(&quelle.pfad, &["add", "-A"])?;
    quelle.schreibe("geaendert.txt", "Arbeitsbaum\n")?;
    quelle.schreibe("neu.txt", "Neue Datei\n")?;
    quelle.schreibe("ignoriert.txt", "lokal\n")?;
    fs::remove_file(quelle.datei("geloescht.txt")).map_err(|e| e.to_string())?;

    let basis = Baum::basis_bundle(&quelle.pfad)?;
    let erste = Baum::aus_bundle(&basis)?;
    let zweite = Baum::aus_bundle(&basis)?;
    assert_eq!(erste.lies("geaendert.txt")?, "Arbeitsbaum\n");
    assert_eq!(erste.lies("neu.txt")?, "Neue Datei\n");
    assert!(!erste.datei("geloescht.txt").exists());
    assert!(!erste.datei("ignoriert.txt").exists());
    erste.schreibe("geaendert.txt", "Nur erste Kopie\n")?;
    assert_eq!(zweite.lies("geaendert.txt")?, "Arbeitsbaum\n");
    assert_eq!(quelle.lies("geaendert.txt")?, "Arbeitsbaum\n");
    erste.schreibe_bytes("geaendert.txt", &[0, 255])?;
    erste.schreibe("nur-im-fall.txt", "Vorübergehend\n")?;
    erste.zuruecksetzen()?;
    assert_eq!(erste.lies("geaendert.txt")?, "Arbeitsbaum\n");
    assert!(!erste.datei("nur-im-fall.txt").exists());
    Ok(())
}

#[test]
fn vorhandener_pruefbaum_wird_nicht_geloescht() -> Result<(), String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let basis = reserviere_ordner(
        &std::env::temp_dir(),
        &format!("kodex-reservierung-{}-{n}", std::process::id()),
    )?;
    let alt = reserviere_ordner(&basis, "gleiche-kennung")?;
    fs::write(alt.join("eigentuemer.txt"), "erster Test").map_err(|e| e.to_string())?;
    let neu = reserviere_ordner(&basis, "gleiche-kennung")?;
    assert_ne!(alt, neu);
    assert_eq!(
        fs::read_to_string(alt.join("eigentuemer.txt")).map_err(|e| e.to_string())?,
        "erster Test"
    );
    drop(Baum {
        ordner: neu.clone(),
        pfad: neu.clone(),
        vorher: RefCell::default(),
    });
    assert!(!neu.exists(), "Eigene Kopie wird aufgeraeumt");
    assert_eq!(
        fs::read_to_string(alt.join("eigentuemer.txt")).map_err(|e| e.to_string())?,
        "erster Test"
    );
    fs::remove_dir_all(basis).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn parallele_pruefbaeume_mit_gleicher_kennung_bleiben_getrennt() -> Result<(), String> {
    use std::sync::{Arc, Barrier};
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let basis = reserviere_ordner(
        &std::env::temp_dir(),
        &format!("kodex-parallel-{}-{n}", std::process::id()),
    )?;
    let barriere = Arc::new(Barrier::new(8));
    let pfade = std::thread::scope(|scope| -> Result<Vec<PathBuf>, String> {
        let mut auftraege = Vec::new();
        for i in 0..8 {
            let basis = &basis;
            let barriere = Arc::clone(&barriere);
            auftraege.push(scope.spawn(move || -> Result<PathBuf, String> {
                barriere.wait();
                let p = reserviere_ordner(basis, "gleiche-kennung");
                // Auch bei einem Schreibfehler muessen alle Arbeiter die Barriere erreichen.
                let geschrieben = p.as_ref().map_err(Clone::clone).and_then(|p| {
                    fs::write(p.join("eigentuemer.txt"), i.to_string()).map_err(|e| e.to_string())
                });
                barriere.wait();
                geschrieben?;
                let p = p?;
                assert_eq!(
                    fs::read_to_string(p.join("eigentuemer.txt")).map_err(|e| e.to_string())?,
                    i.to_string()
                );
                Ok(p)
            }));
        }
        auftraege
            .into_iter()
            .map(|a| {
                a.join()
                    .map_err(|_| "Pruefbaum-Arbeiter abgebrochen".to_string())?
            })
            .collect()
    })?;
    let eindeutig: std::collections::BTreeSet<_> = pfade.into_iter().collect();
    assert_eq!(eindeutig.len(), 8);
    fs::remove_dir_all(basis).map_err(|e| e.to_string())?;
    Ok(())
}
