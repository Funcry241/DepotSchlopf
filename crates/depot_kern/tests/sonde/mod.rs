//! Baut ein kleines fremdes Crate gegen die Crates dieses Workspaces und meldet, ob es sich
//! uebersetzen laesst. So zeigt ein Test, dass etwas Verbotenes sich gar nicht bauen laesst
//! (R3, R19). Die Positivprobe stellt sicher, dass ein Fehlschlag nicht an der Umgebung liegt.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Uebersetzt `code` als `src/lib.rs` eines fremden Crates mit den genannten Abhaengigkeiten
/// aus `crates/`. `Ok` = baut, `Err` = Compilerausgabe.
pub fn baut(abhaengigkeiten: &[&str], code: &str) -> Result<(), String> {
    let ws = workspace();
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "depot_sonde_{}_{}_{n}",
        env!("CARGO_PKG_NAME"),
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).map_err(|e| format!("Sonde: {e}"))?;
    let mut toml = String::from(
        "[package]\nname = \"depot_sonde\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[workspace]\n\n[dependencies]\n",
    );
    for a in abhaengigkeiten {
        let pfad = ws.join("crates").join(a);
        let _ = writeln!(
            toml,
            "{a} = {{ path = {:?} }}",
            pfad.to_string_lossy().replace('\\', "/")
        );
    }
    fs::write(dir.join("Cargo.toml"), toml).map_err(|e| format!("Sonde: {e}"))?;
    fs::write(dir.join("src/lib.rs"), code).map_err(|e| format!("Sonde: {e}"))?;
    for datei in ["Cargo.lock", "rust-toolchain.toml"] {
        fs::copy(ws.join(datei), dir.join(datei)).map_err(|e| format!("Sonde {datei}: {e}"))?;
    }
    let aus = Command::new(option_env!("CARGO").unwrap_or("cargo"))
        .args(["check", "--quiet", "--offline"])
        .current_dir(&dir)
        .env("CARGO_TARGET_DIR", ws.join("target/sonde"))
        .output()
        .map_err(|e| format!("cargo nicht startbar: {e}"))?;
    let _ = fs::remove_dir_all(&dir);
    if aus.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&aus.stderr).into_owned())
    }
}

/// Positivprobe: Ein harmloses Crate muss bauen, sonst sagt ein Fehlschlag nichts.
pub fn umgebung_baut(abhaengigkeiten: &[&str]) -> Result<(), String> {
    baut(abhaengigkeiten, "pub fn harmlos() {}\n")
}
