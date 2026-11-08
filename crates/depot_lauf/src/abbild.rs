//! Der gepruefte Stand (R28, 1·R04, K14-01).
//!
//! Gate und Export nehmen denselben festgehaltenen Stand. Vor dem Gate haelt der Lauf den
//! Git-Index einmal als Baum fest (`git write-tree`) und schreibt jede gewoehnliche Datei
//! dieses Baums Byte fuer Byte aus ihrem Blob in eine eigene Arbeitskopie ausserhalb des
//! Projektordners (`ablage`), ohne Filter von Git (Zeilenenden, Attribute). Das Gate laeuft in
//! dieser Kopie, der Export nimmt dieselben Blobs. Was nur im Arbeitsbaum liegt, sehen also
//! weder Gate noch Export, gleich was Git ueber den Arbeitsbaum meldet. Nach dem Gate wird die
//! Kopie Byte fuer Byte mit den Blobs verglichen; weicht sie ab oder kam eine Datei hinzu, ist
//! der Lauf gescheitert, und der Export nennt den Quellstand ungeprueft.
//!
//! **Grenze:** Eine Aenderung an der Kopie, die waehrend des Gates entsteht und vor seinem Ende
//! genau zurueckgenommen wird, sieht der Vergleich nicht. Die Kopie gehoert nur dem Lauf.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::export::{Lauf, Schritt, hat_git};

/// Ordner fuer Kopie und Build-Ordner des Gates: im temporaeren Ordner des Systems, je
/// Projektordner ein eigener, fest benannter. Ausserhalb des Projekts erbt Cargo dort keine
/// Konfiguration aus dem Projektordner, auch keine nicht vorgemerkte (K14-02). Fest bleibt er,
/// damit die Abhaengigkeiten nicht jedes Mal neu bauen.
pub fn ablage(wurzel: &Path) -> PathBuf {
    let abdruck = Sha256::digest(wurzel.to_string_lossy().as_bytes());
    let kurz: String = abdruck.iter().take(8).fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    });
    std::env::temp_dir().join(format!("depotschlopf-{kurz}"))
}

/// Build-Ordner des Gates neben der Kopie. Teilte es den des Projekts, koennte Cargo veraltete
/// Ergebnisse des einen fuer den anderen halten.
pub fn bau_ordner(wurzel: &Path) -> PathBuf {
    ablage(wurzel).join("bau")
}

/// Der Ordner der Cargo-Einstellungen des Nutzers: `CARGO_HOME`, sonst `.cargo` im
/// Benutzerordner.
fn cargo_home() -> Option<PathBuf> {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|h| PathBuf::from(h).join(".cargo"))
        })
}

/// Cargo-Konfigurationen in den Ordnern oberhalb der Kopie. Cargo liest sie alle; die Kopie
/// selbst bringt nur, was im festgehaltenen Stand steht. Die Einstellungen des Nutzers in
/// `cargo_home` gehoeren zur Werkzeugkette und zaehlen nicht.
pub fn fremde_cargo_konfiguration(kopie: &Path, cargo_home: Option<&Path>) -> Vec<PathBuf> {
    kopie
        .ancestors()
        .skip(1)
        .map(|a| a.join(".cargo"))
        .filter(|d| cargo_home != Some(d.as_path()))
        .flat_map(|d| [d.join("config.toml"), d.join("config")])
        .filter(|f| f.exists())
        .collect()
}

/// Ist das eine Umgebungsvariable, die beeinflusst, wie Cargo, Rust oder Git arbeiten?
/// Ausgenommen sind die Orte der Werkzeuge und die Netzangaben fuer das Laden von
/// Abhaengigkeiten, deren Inhalt `Cargo.lock` mit Pruefsummen festlegt.
fn bau_variable(name: &str) -> bool {
    let bau = ["CARGO", "RUST", "GIT_"]
        .iter()
        .any(|p| name.starts_with(p));
    let erlaubt = ["CARGO_HOME", "RUSTUP_HOME"].contains(&name)
        || name.starts_with("CARGO_HTTP_")
        || name.starts_with("CARGO_NET_");
    bau && !erlaubt
}

/// Der Aufruf des Gates in `dir` mit dem Build-Ordner `bau`. Das Gate erbt keine
/// Bau-Einstellungen aus der Umgebung (K14-02).
pub fn gate_befehl(dir: &Path, bau: &Path, args: &[String]) -> Command {
    let mut c = cargo_befehl(dir, bau);
    c.args(["run", "--quiet", "-p", "depot_kodex", "--"])
        .args(args);
    c
}

pub fn cargo_befehl(dir: &Path, bau: &Path) -> Command {
    let mut c = Command::new("cargo");
    c.current_dir(dir);
    for (k, _) in std::env::vars_os() {
        if bau_variable(&k.to_string_lossy()) {
            c.env_remove(k);
        }
    }
    // Die Prüfkopie enthält die Bytes der Git-Blobs. Auch Git-Aufrufe des Gates
    // und seiner Test-Repositories müssen LF behalten, unabhängig von Nutzer- und Systemconfig.
    c.envs([
        ("GIT_CONFIG_COUNT", "2"),
        ("GIT_CONFIG_KEY_0", "core.autocrlf"),
        ("GIT_CONFIG_VALUE_0", "false"),
        ("GIT_CONFIG_KEY_1", "core.eol"),
        ("GIT_CONFIG_VALUE_1", "lf"),
    ]);
    c.env("CARGO_TARGET_DIR", bau);
    c
}

/// Der festgehaltene Stand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Abbild {
    /// Der Baum des Index beim Festhalten.
    pub baum: String,
    /// Jede gewoehnliche Datei des Baums mit ihrem Blob, nach Pfad sortiert.
    pub dateien: Vec<(String, String)>,
    /// Die Arbeitskopie, in der das Gate prueft.
    pub kopie: PathBuf,
}

pub fn git_roh(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    // Feste Angaben fuer `commit-tree`: Der Lauf braucht keine Identitaet des Nutzers.
    let aus = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "DepotSchlopf")
        .env("GIT_AUTHOR_EMAIL", "lauf@depotschlopf.invalid")
        .env("GIT_COMMITTER_NAME", "DepotSchlopf")
        .env("GIT_COMMITTER_EMAIL", "lauf@depotschlopf.invalid")
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

pub fn git_text(dir: &Path, args: &[&str]) -> Result<String, String> {
    Ok(String::from_utf8_lossy(&git_roh(dir, args)?)
        .trim()
        .to_string())
}

/// Die gewoehnlichen Dateien eines Baums: Pfad, Blob, ausfuehrbar.
fn dateien_im_baum(wurzel: &Path, baum: &str) -> Result<Vec<(String, String, bool)>, String> {
    let roh = git_roh(wurzel, &["ls-tree", "-r", "-z", "--full-tree", baum])?;
    let mut aus = Vec::new();
    for zeile in roh.split(|b| *b == 0).filter(|z| !z.is_empty()) {
        let zeile = String::from_utf8(zeile.to_vec())
            .map_err(|_| "Baumzeile ist kein UTF-8".to_string())?;
        // "<modus> <typ> <objekt>\t<pfad>"
        let (kopf, pfad) = zeile
            .split_once('\t')
            .ok_or_else(|| format!("Baumzeile nicht lesbar: {zeile}"))?;
        let teile: Vec<&str> = kopf.split(' ').collect();
        let [modus, typ, objekt] = teile.as_slice() else {
            return Err(format!("Baumzeile nicht lesbar: {zeile}"));
        };
        if *typ == "blob" && (*modus == "100644" || *modus == "100755") {
            aus.push((pfad.to_string(), (*objekt).to_string(), *modus == "100755"));
        }
    }
    Ok(aus)
}

/// Setzt das Ausfuehrungsrecht unter Unix; unter Windows gibt es keins.
fn ausfuehrbar(p: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(p, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("{}: {e}", p.display()))?;
    }
    // Unter Windows bleibt nur zu pruefen, dass die Datei da ist.
    fs::metadata(p)
        .map(|_| ())
        .map_err(|e| format!("{}: {e}", p.display()))
}

/// Entfernt eine fruehere Kopie samt ihrer Anmeldung bei Git.
fn alte_kopie_weg(wurzel: &Path, kopie: &Path) -> Result<(), String> {
    let pfad = kopie.to_string_lossy();
    let _ = git_roh(wurzel, &["worktree", "remove", "--force", &pfad]);
    match fs::remove_dir_all(kopie) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("{}: nicht entfernbar: {e}", kopie.display())),
    }
    git_roh(wurzel, &["worktree", "prune"]).map(|_| ())
}

/// Haelt den Index als Baum fest und legt die Arbeitskopie an. Ohne Git `None`.
///
/// # Errors
/// Git fehlt, der Index ist nicht lesbar oder hat offene Konflikte, oder die Kopie laesst sich
/// nicht anlegen.
pub fn festhalten(wurzel: &Path) -> Result<Option<Abbild>, String> {
    festhalten_in(wurzel, &ablage(wurzel))
}

/// Wie `festhalten`, mit der Kopie unter `ablage/stand`.
///
/// # Errors
/// Wie `festhalten`; dazu, wenn oberhalb der Kopie eine fremde Cargo-Konfiguration liegt.
pub fn festhalten_in(wurzel: &Path, ablage: &Path) -> Result<Option<Abbild>, String> {
    if !hat_git(wurzel)? {
        return Ok(None);
    }
    let baum = git_text(wurzel, &["write-tree"])?;
    let kopf = git_text(wurzel, &["rev-parse", "--verify", "-q", "HEAD"]).ok();
    let mut commit_args = vec!["commit-tree", baum.as_str(), "-m", "geprüfter Stand"];
    if let Some(k) = &kopf {
        commit_args.extend(["-p", k.as_str()]);
    }
    let commit = git_text(wurzel, &commit_args)?;
    kopie_anlegen(wurzel, &ablage.join("stand"), baum, &commit).map(Some)
}

/// Legt die Arbeitskopie eines vorhandenen Commits unter `ablage/<name>` an, Byte fuer Byte aus
/// seinen Blobs wie bei `festhalten` (fuer das Update).
///
/// # Errors
/// Der Commit ist unbekannt, die Kopie laesst sich nicht anlegen, oder oberhalb der Kopie liegt
/// eine fremde Cargo-Konfiguration.
pub fn festhalten_commit(
    wurzel: &Path,
    ablage: &Path,
    name: &str,
    commit: &str,
) -> Result<Abbild, String> {
    let baum = git_text(
        wurzel,
        &["rev-parse", "--verify", &format!("{commit}^{{tree}}")],
    )?;
    kopie_anlegen(wurzel, &ablage.join(name), baum, commit)
}

fn kopie_anlegen(
    wurzel: &Path,
    kopie: &Path,
    baum: String,
    commit: &str,
) -> Result<Abbild, String> {
    let dateien = dateien_im_baum(wurzel, &baum)?;
    let kopie = kopie.to_path_buf();
    alte_kopie_weg(wurzel, &kopie)?;
    let pfad = kopie.to_string_lossy();
    git_roh(
        wurzel,
        &[
            "worktree",
            "add",
            "-q",
            "--no-checkout",
            "--detach",
            &pfad,
            commit,
        ],
    )?;
    for (datei, blob, x) in &dateien {
        let ziel = kopie.join(datei);
        if let Some(eltern) = ziel.parent() {
            fs::create_dir_all(eltern).map_err(|e| format!("{datei}: {e}"))?;
        }
        let inhalt = depot_kodex::git::blob(wurzel, blob)
            .map_err(|e| format!("{datei} (Index): nicht lesbar: {e}"))?;
        fs::write(&ziel, inhalt).map_err(|e| format!("{datei}: {e}"))?;
        if *x {
            ausfuehrbar(&ziel)?;
        }
    }
    // Index der Kopie auf den festgehaltenen Stand, ohne die Dateien anzufassen.
    git_roh(&kopie, &["reset", "-q"])?;
    let fremd = fremde_cargo_konfiguration(&kopie, cargo_home().as_deref());
    if !fremd.is_empty() {
        return Err(format!(
            "Cargo-Konfiguration oberhalb der Kopie wuerde das Gate beeinflussen: {}",
            fremd
                .iter()
                .map(|f| f.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(Abbild {
        baum,
        dateien: dateien.into_iter().map(|(p, b, _)| (p, b)).collect(),
        kopie,
    })
}

/// Alle Dateien der Kopie ausser `.git` und dem Build-Ordner `target/`, relativ und mit `/`.
fn dateien_der_kopie(dir: &Path, rel: &str, aus: &mut Vec<String>, fehler: &mut Vec<String>) {
    let eintraege = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            fehler.push(format!("{rel}: nicht lesbar: {e}"));
            return;
        }
    };
    for e in eintraege {
        let e = match e {
            Ok(e) => e,
            Err(err) => {
                fehler.push(format!("{rel}: nicht lesbar: {err}"));
                continue;
            }
        };
        let name = e.file_name().to_string_lossy().into_owned();
        let pfad = if rel.is_empty() {
            name
        } else {
            format!("{rel}/{name}")
        };
        if pfad == ".git" || pfad == "target" {
            continue;
        }
        match e.file_type() {
            Ok(t) if t.is_dir() => dateien_der_kopie(&e.path(), &pfad, aus, fehler),
            Ok(t) if t.is_file() => aus.push(pfad),
            Ok(_) => fehler.push(format!("{pfad}: weder Datei noch Ordner")),
            Err(err) => fehler.push(format!("{pfad}: nicht lesbar: {err}")),
        }
    }
}

/// Was die Kopie nach dem Gate vom festgehaltenen Stand trennt; leer, wenn sie Byte fuer Byte
/// gleich ist.
pub fn vergleiche(wurzel: &Path, abbild: &Abbild) -> Vec<String> {
    let mut vorhanden = Vec::new();
    let mut probleme = Vec::new();
    dateien_der_kopie(&abbild.kopie, "", &mut vorhanden, &mut probleme);
    let soll: BTreeMap<&str, &str> = abbild
        .dateien
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_str()))
        .collect();
    for p in &vorhanden {
        if !soll.contains_key(p.as_str()) {
            probleme.push(format!("{p}: im festgehaltenen Stand nicht enthalten"));
        }
    }
    for (p, blob) in &soll {
        let ist = fs::read(abbild.kopie.join(p));
        let erwartet = depot_kodex::git::blob(wurzel, blob);
        match (ist, erwartet) {
            (Ok(i), Ok(e)) if i == e => {}
            (Ok(_), Ok(_)) => probleme.push(format!("{p}: waehrend des Gates geändert")),
            (Err(e), _) => probleme.push(format!("{p}: nicht lesbar: {e}")),
            (_, Err(e)) => probleme.push(format!("{p} (Index): nicht lesbar: {e}")),
        }
    }
    probleme.sort();
    probleme
}

/// Hinweis fuer den Menschen: was im Arbeitsbaum anders ist und deshalb weder geprueft noch
/// exportiert wird. Nur ein Hinweis nach Git; die Zusage haengt nicht daran.
fn hinweise(wurzel: &Path) -> Vec<String> {
    git_roh(
        wurzel,
        &["status", "--porcelain", "-z", "--untracked-files=all"],
    )
    .map(|roh| {
        roh.split(|b| *b == 0)
            // "XY pfad": Y ist der Arbeitsbaum gegen den Index; "??" heisst unverfolgt.
            .filter(|z| z.get(1).is_some_and(|y| *y != b' '))
            .filter_map(|z| z.get(3..))
            .filter(|p| !p.is_empty())
            .map(|p| {
                format!(
                    "Hinweis: {} ist nicht vorgemerkt, weder geprüft noch exportiert",
                    String::from_utf8_lossy(p)
                )
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Haelt den Stand fest, fuehrt die Pruefung in seiner Kopie aus und vergleicht danach. Der
/// Schritt `stand` scheitert, wenn Gate und Export nicht denselben Stand saehen. Ohne Git
/// prueft das Gate den Projektordner, und es gibt keinen Quellstand.
pub fn gepruefter_lauf(
    wurzel: &Path,
    befehl: String,
    mut konsole: String,
    mut warnungen: Vec<String>,
    pruefung: impl FnOnce(&Path, &mut String) -> i32,
) -> Lauf {
    let mut probleme = Vec::new();
    let (exit, abbild) = match festhalten(wurzel) {
        Ok(Some(a)) => {
            warnungen.extend(hinweise(wurzel));
            let exit = pruefung(&a.kopie, &mut konsole);
            probleme = vergleiche(wurzel, &a);
            (exit, Some(a))
        }
        Ok(None) => (pruefung(wurzel, &mut konsole), None),
        Err(e) => {
            probleme.push(format!(
                "Stand nicht festhaltbar, Gate nicht ausgeführt: {e}"
            ));
            (1, None)
        }
    };
    for p in &probleme {
        let z = format!("[DepotSchlopf] Quellstand ungeprüft: {p}\n");
        eprint!("{z}");
        konsole.push_str(&z);
        warnungen.push(format!("Quellstand ungeprüft: {p}"));
    }
    Lauf {
        befehl,
        schritte: vec![
            Schritt {
                name: "kodex".into(),
                exit,
            },
            Schritt {
                name: "stand".into(),
                exit: i32::from(!probleme.is_empty()),
            },
        ],
        konsole,
        warnungen,
        abbild,
    }
}
