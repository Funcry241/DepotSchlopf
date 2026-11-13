//! Update mit Rueckfall: `run.cmd update pruefen` und `run.cmd update`.
//!
//! **Pruefen** holt `main` von `origin` (nur lesend, ueber `git fetch`) und nennt die neuen
//! Staende. **Einspielen** prueft den neuen Stand zuerst vollstaendig in einer eigenen Kopie
//! ausserhalb des Projektordners: dasselbe Gate wie `run.cmd`, mit der Basis seines Vorgaengers
//! wie die Pruefung von `main` auf GitHub. Erst wenn es besteht und die Kopie danach Byte fuer
//! Byte unveraendert ist, rueckt der Projektordner vor, und zwar nur vorwaerts (`merge
//! --ff-only`): Nichts Lokales wird ueberschrieben. Scheitert etwas, bleibt der alte Stand.
//!
//! Vor dem Vorruecken haelt `daten/update_offen.txt` fest, von wo nach wo es ging. Der Waechter
//! (`DepotSchlopf.cmd`) startet den Dienst danach neu; kommt der neue Stand nicht hoch, setzt
//! er den Projektordner auf den alten Stand zurueck und vermerkt den neuen in
//! `daten/update_abgelehnt.txt`. Einen abgelehnten Stand spielt das Update nie wieder ein.
//! Bestaetigt wird ein Update erst, wenn der neue Dienst eine Weile laeuft (`depot_dienst`).

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::abbild;

mod update_dialog;

/// Der einzige Zweig, dem das Update folgt.
pub const ZWEIG: &str = "main";
/// Erste Zeile von `daten/update_offen.txt`; `depot_dienst::rueckfall` liest dasselbe Format.
pub const KOPF: &str = "update-v1";
/// So viele neue Staende zeigt die Pruefung hoechstens einzeln.
const LISTE_MAX: usize = 30;

pub fn offen_pfad(wurzel: &Path) -> PathBuf {
    wurzel.join("daten").join("update_offen.txt")
}

pub fn abgelehnt_pfad(wurzel: &Path) -> PathBuf {
    wurzel.join("daten").join("update_abgelehnt.txt")
}

/// Der Inhalt von `daten/update_offen.txt`.
pub fn offen_text(vorher: &str, neu: &str) -> String {
    format!("{KOPF}\nvorher={vorher}\nneu={neu}\n")
}

/// Ein voller Commit-Name: 40 kleine Hexziffern (SHA-1) oder 64 (SHA-256).
pub fn ist_commit(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Steht der Stand in `daten/update_abgelehnt.txt`? Ein unlesbares Verzeichnis zaehlt als
/// Fehler, nicht als leer.
///
/// # Errors
/// Die Datei ist da, aber nicht lesbar.
pub fn abgelehnt(wurzel: &Path, neu: &str) -> Result<bool, String> {
    let p = abgelehnt_pfad(wurzel);
    match fs::read_to_string(&p) {
        Ok(t) => Ok(t.lines().any(|z| z.trim() == neu)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("{}: nicht lesbar: {e}", p.display())),
    }
}

fn git(wurzel: &Path, args: &[&str]) -> Result<String, String> {
    abbild::git_text(wurzel, args)
}

fn sperren_zum_einspielen(daten: &Path) -> Result<(fs::File, fs::File), String> {
    let update = crate::updatedaten::update_sperren(daten)?;
    let sicherung =
        crate::updatedaten::sicherung_sperren(daten, "Eine Sicherung läuft gerade. Kein Update.")?;
    Ok((update, sicherung))
}

/// Was vor jedem Update gelten muss: auf `main`, nichts Verfolgtes geaendert, kein anderer Lauf
/// mit Sperre, kein unbestaetigtes Update.
fn vorbedingungen(wurzel: &Path) -> Result<(), String> {
    let zweig = git(wurzel, &["symbolic-ref", "--short", "-q", "HEAD"])
        .map_err(|_| "Projektordner steht auf keinem Zweig. Update nur auf main.".to_string())?;
    if zweig != ZWEIG {
        return Err(format!(
            "Projektordner steht auf Zweig {zweig:?}. Update nur auf {ZWEIG}."
        ));
    }
    let geaendert = git(wurzel, &["status", "--porcelain", "--untracked-files=no"])?;
    if !geaendert.is_empty() {
        return Err(format!(
            "Verfolgte Dateien sind lokal geändert; das Update überschreibt nichts:\n{geaendert}"
        ));
    }
    if let Ok(eintraege) = fs::read_dir(wurzel.join("daten")) {
        for e in eintraege.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(".sperre") {
                return Err(format!(
                    "daten/{name} liegt: Ein anderer Lauf arbeitet oder brach ab. Kein Update."
                ));
            }
        }
    }
    let offen = offen_pfad(wurzel);
    if offen.exists() {
        return Err(format!(
            "{} liegt: Das letzte Update ist noch nicht bestätigt. Erst den Dienst \
             (DepotSchlopf.cmd) neu starten; er bestätigt es oder setzt es zurück.",
            offen.display()
        ));
    }
    Ok(())
}

/// Ein moegliches Update.
struct Kandidat {
    alt: String,
    neu: String,
    staende: Vec<String>,
}

/// Holt `main` und sagt, ob es Neues gibt, das sich vorwaerts einspielen laesst.
fn hole(wurzel: &Path) -> Result<Option<Kandidat>, String> {
    println!("[Update] git fetch origin {ZWEIG}");
    git(wurzel, &["fetch", "--quiet", "--no-tags", "origin", ZWEIG])?;
    let neu = git(wurzel, &["rev-parse", "--verify", "FETCH_HEAD^{commit}"])?;
    let alt = git(wurzel, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    if !ist_commit(&neu) || !ist_commit(&alt) {
        return Err("Git nennt keinen vollen Commit-Namen.".into());
    }
    if neu == alt {
        return Ok(None);
    }
    if git(wurzel, &["merge-base", "--is-ancestor", &alt, &neu]).is_err() {
        return Err(format!(
            "Der lokale Stand {alt} ist nicht Teil von origin/{ZWEIG}. Das Update spielt nur \
             vorwärts ein und überschreibt nichts."
        ));
    }
    if abgelehnt(wurzel, &neu)? {
        return Err(format!(
            "Stand {neu} wurde schon einmal zurückgesetzt (daten/update_abgelehnt.txt). Erst \
             ein neuerer Stand auf {ZWEIG} wird wieder angeboten."
        ));
    }
    let liste = git(
        wurzel,
        &[
            "log",
            "--oneline",
            "--no-decorate",
            &format!("{alt}..{neu}"),
        ],
    )?;
    Ok(Some(Kandidat {
        alt,
        neu,
        staende: liste.lines().map(str::to_string).collect(),
    }))
}

fn zeige(k: &Kandidat) {
    println!(
        "[Update] Neu auf {ZWEIG}: Stände: {}, {} -> {}",
        k.staende.len(),
        kurz(&k.alt),
        kurz(&k.neu)
    );
    for s in k.staende.iter().take(LISTE_MAX) {
        println!("  {s}");
    }
    if k.staende.len() > LISTE_MAX {
        println!("  …");
    }
}

fn kurz(c: &str) -> &str {
    c.get(..10).unwrap_or(c)
}

/// Schreibt `daten/update_offen.txt` ganz und auf den Datentraeger.
fn offen_schreiben(wurzel: &Path, text: &str) -> Result<(), String> {
    let p = offen_pfad(wurzel);
    if let Some(o) = p.parent() {
        fs::create_dir_all(o).map_err(|e| format!("{}: {e}", o.display()))?;
    }
    let neben = p.with_extension("neu");
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&neben)
        .and_then(|mut f| {
            f.write_all(text.as_bytes())?;
            f.sync_all()
        })
        .map_err(|e| format!("{}: {e}", neben.display()))?;
    fs::rename(&neben, &p).map_err(|e| format!("{}: {e}", p.display()))
}

/// Haelt fest, dass dieser Baum das ganze Gate bestanden hat: `daten/geprueft.txt`, eine Zeile
/// `geprueft-v1;<baum>` (gelesen von `depot_dienst::programmstand`). Nur ein so vermerkter Stand
/// wird fuer den Start ohne Cargo gesichert. Ein Schreibfehler aendert nichts am Gate; der
/// Stand wird dann nur nicht gesichert.
pub fn vermerke_geprueft(wurzel: &Path, baum: &str) {
    let p = wurzel.join("daten").join("geprueft.txt");
    let schon = fs::read_to_string(&p).is_ok_and(|t| {
        t.lines()
            .any(|z| z.strip_prefix("geprueft-v1;") == Some(baum))
    });
    if schon || !ist_commit(baum) {
        return;
    }
    let ergebnis = fs::create_dir_all(wurzel.join("daten")).and_then(|()| {
        let mut f = OpenOptions::new().create(true).append(true).open(&p)?;
        writeln!(f, "geprueft-v1;{baum}")?;
        f.sync_all()
    });
    if let Err(e) = ergebnis {
        eprintln!("[DepotSchlopf] {} nicht geschrieben: {e}", p.display());
    }
}

/// Prüft die Pfade mit dem installierten Updater, bevor der neue Stand gebaut wird.
fn pruefe_baumpfade(wurzel: &Path, commit: &str) -> Result<(), String> {
    let roh = abbild::git_roh(
        wurzel,
        &["ls-tree", "-r", "-z", "--name-only", "--full-tree", commit],
    )?;
    let pfade = std::str::from_utf8(&roh).map_err(
        |_| "Der neue Stand enthält einen ungültigen Dateinamen. Update nicht eingespielt.",
    )?;
    for pfad in pfade.split_terminator('\0') {
        let cargo_pfad = pfad.split('/').any(|teil| {
            let teil = teil.trim_end_matches([' ', '.']);
            teil.eq_ignore_ascii_case(".cargo") || teil.eq_ignore_ascii_case("build.rs")
        });
        if cargo_pfad || depot_kodex::pfade::rolle(pfad) == depot_kodex::pfade::Rolle::Unbekannt {
            return Err(format!(
                "Der neue Stand enthält den unzulässigen Pfad {pfad:?}. Update nicht eingespielt."
            ));
        }
        if pfad.rsplit('/').next().is_some_and(|name| {
            name.trim_end_matches([' ', '.'])
                .eq_ignore_ascii_case("Cargo.toml")
        }) {
            pruefe_manifest(wurzel, commit, pfad)?;
        }
    }
    Ok(())
}

/// Prüft vor jedem Cargo-Aufruf den Manifestinhalt des Kandidaten direkt aus Git.
fn pruefe_manifest(wurzel: &Path, commit: &str, pfad: &str) -> Result<(), String> {
    use depot_kodex::toml::{self, Wert};

    let roh = abbild::git_roh(wurzel, &["show", &format!("{commit}:{pfad}")])?;
    let text = std::str::from_utf8(&roh).map_err(|e| format!("{pfad}: {e}"))?;
    let manifest = toml::lies(text).map_err(|e| format!("{pfad}: {e}"))?;
    if !matches!(
        manifest.pfad(&["package", "build"]),
        None | Some(Wert::Wahr(false))
    ) || !matches!(
        manifest.pfad(&["lib", "proc-macro"]),
        None | Some(Wert::Wahr(false))
    ) || matches!(
        manifest.pfad(&["lib", "crate-type"]),
        Some(Wert::Liste(arten)) if arten.iter().any(|art| art.text() == Some("proc-macro"))
    ) || manifest.feld("patch").is_some()
        || manifest.feld("replace").is_some()
    {
        return Err(format!(
            "{pfad}: Buildskripte, prozedurale Makros und umgeleitete Abhängigkeiten sind beim Update unzulässig."
        ));
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct LockAenderung {
    abdruck: String,
    pakete: Vec<String>,
}

fn lock_aenderung(wurzel: &Path, k: &Kandidat) -> Result<Option<LockAenderung>, String> {
    use std::collections::BTreeSet;

    fn pakete(roh: &[u8]) -> Result<BTreeSet<(String, String, String)>, String> {
        use depot_kodex::toml::{self, Wert};
        let text = std::str::from_utf8(roh).map_err(|e| e.to_string())?;
        let lock = toml::lies(text)?;
        let Some(Wert::Liste(pakete)) = lock.feld("package") else {
            return Err("Cargo.lock: Paketliste fehlt".into());
        };
        pakete
            .iter()
            .map(|paket| {
                let name = paket
                    .feld("name")
                    .and_then(Wert::text)
                    .ok_or("Paketname fehlt")?;
                let version = paket
                    .feld("version")
                    .and_then(Wert::text)
                    .ok_or("Paketversion fehlt")?;
                let quelle = paket
                    .feld("source")
                    .and_then(Wert::text)
                    .unwrap_or_default();
                let pruefsumme = paket
                    .feld("checksum")
                    .and_then(Wert::text)
                    .unwrap_or_default();
                Ok((
                    format!("{name} {version}"),
                    quelle.to_string(),
                    pruefsumme.to_string(),
                ))
            })
            .collect()
    }

    let alt = abbild::git_roh(wurzel, &["show", &format!("{}:Cargo.lock", k.alt)])?;
    let neu = abbild::git_roh(wurzel, &["show", &format!("{}:Cargo.lock", k.neu)])?;
    if alt == neu {
        return Ok(None);
    }
    let alt_pakete = pakete(&alt)?;
    let neu_pakete = pakete(&neu)?;
    Ok(Some(LockAenderung {
        abdruck: depot_kodex::festschreibung::sha_bytes(&neu),
        pakete: neu_pakete
            .difference(&alt_pakete)
            .map(|(name, _, _)| name.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    }))
}

fn lock_pakete(a: &LockAenderung) -> String {
    if a.pakete.is_empty() {
        "Cargo.lock geändert; keine zusätzlichen Paketversionen".into()
    } else {
        a.pakete.join(", ")
    }
}

fn pruefe_lock(
    wurzel: &Path,
    k: &Kandidat,
    bestaetigen: impl FnOnce(&str) -> bool,
) -> Result<(), String> {
    let Some(a) = lock_aenderung(wurzel, k)? else {
        return Ok(());
    };
    let p = wurzel.join("daten").join("update_abhaengigkeiten.txt");
    let zeile = format!("update-lock-v1;{};{}", k.neu, a.abdruck);
    match fs::read_to_string(&p) {
        Ok(text) if text.lines().any(|z| z == zeile) => return Ok(()),
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("{}: {e}", p.display())),
    }
    let hinweis = format!("Neue Abhängigkeiten: {}", lock_pakete(&a));
    if !bestaetigen(&format!(
        "{hinweis}\nUpdate mit geändertem Cargo.lock freigeben?"
    )) {
        return Err(format!(
            "{hinweis} Bitte von Hand freigeben. Das Update wurde nicht gebaut."
        ));
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
        .and_then(|mut f| {
            writeln!(f, "{zeile}")?;
            f.sync_all()
        })
        .map_err(|e| format!("{}: {e}", p.display()))
}

/// Prueft den neuen Stand in seiner Kopie mit dem ganzen Gate.
fn pruefe_kopie(
    wurzel: &Path,
    k: &Kandidat,
    bestaetigen: impl FnOnce(&str) -> bool,
) -> Result<abbild::Abbild, String> {
    pruefe_baumpfade(wurzel, &k.neu)?;
    pruefe_lock(wurzel, k, bestaetigen)?;
    let ablage = abbild::ablage(wurzel);
    println!("[Update] Kopie von {} anlegen", kurz(&k.neu));
    let abbild = abbild::festhalten_commit(wurzel, &ablage, "update", &k.neu)?;
    let basis = format!("{}^1", k.neu);
    let args: Vec<String> = vec!["--voll".into(), "--basis".into(), basis];
    println!(
        "[Update] Gate im Prüfordner: cargo run --locked -p depot_kodex -- {}",
        args.join(" ")
    );
    let status = abbild::cargo_befehl(&abbild.kopie, &ablage.join("bau-update"))
        .args(["run", "--locked", "--quiet", "-p", "depot_kodex", "--"])
        .args(&args)
        .status()
        .map_err(|e| format!("cargo nicht startbar: {e}"))?;
    if !status.success() {
        return Err("Das Gate hat den neuen Stand abgelehnt. Der alte Stand bleibt.".into());
    }
    let probleme = abbild::vergleiche(wurzel, &abbild);
    if !probleme.is_empty() {
        return Err(format!(
            "Die Kopie hat sich während des Gates verändert; der neue Stand gilt als \
             ungeprüft:\n{}",
            probleme.join("\n")
        ));
    }
    Ok(abbild)
}

fn pruefe_datenkopie(wurzel: &Path, kopie: &abbild::Abbild, probe: &Path) -> Result<(), String> {
    let aus = abbild::cargo_befehl(&kopie.kopie, &abbild::ablage(wurzel).join("bau-update"))
        .args([
            "run",
            "--locked",
            "--quiet",
            "-p",
            "depot_lage",
            "--",
            "--update-datenprobe",
        ])
        .arg(probe)
        .output()
        .map_err(|e| format!("Datenprobe konnte nicht starten: {e}"))?;
    if !aus.status.success() {
        return Err(format!(
            "Der neue Stand kann deine Papierdaten nicht prüfen. Update nicht eingespielt: {}",
            String::from_utf8_lossy(&aus.stderr).trim()
        ));
    }
    let probleme = abbild::vergleiche(wurzel, kopie);
    if !probleme.is_empty() {
        return Err(format!(
            "Prüfkopie wurde verändert: {}",
            probleme.join("\n")
        ));
    }
    Ok(())
}

fn einspielen_mit_datenprobe(
    wurzel: &Path,
    k: &Kandidat,
    ablage: &Path,
    pruefe: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let daten = wurzel.join("daten");
    let journal = crate::updatedaten::journal_sperren(&daten)?;
    let stand = crate::updatedaten::lesen(&daten)?;
    let probe = crate::updatedaten::Probe::neu(ablage)?;
    crate::updatedaten::kopiere(&stand, &probe.0)?;
    pruefe(&probe.0)?;
    crate::updatedaten::sichern(&daten, &stand, &k.alt)
        .map_err(|e| format!("Datensicherung fehlgeschlagen. Update nicht eingespielt: {e}"))?;
    if crate::updatedaten::lesen(&daten)? != stand {
        return Err(
            "Papierdaten haben sich während der Prüfung geändert. Update nicht eingespielt.".into(),
        );
    }
    let ergebnis = einspielen(wurzel, k);
    if let Some(f) = journal {
        let _ = f.unlock();
    }
    ergebnis
}

/// Spielt einen geprueften Stand vorwaerts ein.
fn einspielen(wurzel: &Path, k: &Kandidat) -> Result<(), String> {
    // Waehrend des Gates kann sich der Projektordner geaendert haben.
    vorbedingungen(wurzel)?;
    let jetzt = git(wurzel, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    if jetzt != k.alt {
        return Err("Der Projektordner hat sich während der Prüfung bewegt. Kein Update.".into());
    }
    offen_schreiben(wurzel, &offen_text(&k.alt, &k.neu))?;
    if let Err(e) = git(wurzel, &["merge", "--ff-only", "--quiet", &k.neu]) {
        // Nichts eingespielt: Die Markierung darf keinen Rueckfall ausloesen.
        let weg = fs::remove_file(offen_pfad(wurzel));
        return Err(match weg {
            Ok(()) => format!("Vorrücken gescheitert, alter Stand bleibt: {e}"),
            Err(w) => format!(
                "Vorrücken gescheitert, alter Stand bleibt: {e}. daten/update_offen.txt bitte \
                 löschen: {w}"
            ),
        });
    }
    Ok(())
}

/// `run.cmd update [pruefen]`.
pub fn lauf(wurzel: &Path, optionen: &[String]) -> ExitCode {
    let (nur_pruefen, erwartet) = match optionen {
        [] => (false, None),
        [o] if matches!(o.as_str(), "pruefen" | "prüfen") => (true, None),
        [sha] if ist_commit(sha) => (false, Some(sha)),
        _ => {
            eprintln!("[Update] FEHLER: erlaubt sind nur `update` und `update prüfen`.");
            return ExitCode::from(2);
        }
    };
    let ergebnis = vorbedingungen(wurzel).and_then(|()| hole(wurzel));
    let k = match ergebnis {
        Ok(Some(k)) => k,
        Ok(None) => {
            println!("[Update] Schon aktuell: nichts Neues auf {ZWEIG}.");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("[Update] FEHLER: {e}");
            return ExitCode::FAILURE;
        }
    };
    if erwartet.is_some_and(|sha| sha != &k.neu) {
        eprintln!("[Update] Seit der Suche gibt es einen anderen Stand. Bitte suche erneut.");
        return ExitCode::FAILURE;
    }
    zeige(&k);
    if nur_pruefen {
        match lock_aenderung(wurzel, &k) {
            Ok(Some(a)) => println!("[Update] Cargo.lock: {}", lock_pakete(&a)),
            Ok(None) => {}
            Err(e) => {
                eprintln!("[Update] FEHLER: {e}");
                return ExitCode::FAILURE;
            }
        }
        println!("[Update] Nur geprüft; eingespielt wird mit `run.cmd update`.");
        println!("[Update] Kandidat: {}", k.neu);
        return ExitCode::SUCCESS;
    }
    let daten = wurzel.join("daten");
    let ergebnis = (|| {
        let _sperren = sperren_zum_einspielen(&daten)?;
        pruefe_kopie(wurzel, &k, update_dialog::bestaetigen).and_then(|kopie| {
            einspielen_mit_datenprobe(wurzel, &k, &abbild::ablage(wurzel), |probe| {
                pruefe_datenkopie(wurzel, &kopie, probe)
            })?;
            vermerke_geprueft(wurzel, &kopie.baum);
            Ok(())
        })
    })();
    match ergebnis {
        Ok(()) => {
            println!(
                "[Update] Eingespielt: {} -> {}. Der Dienst startet neu und bestätigt es; \
                 kommt er nicht hoch, setzt DepotSchlopf.cmd den alten Stand zurück.",
                kurz(&k.alt),
                kurz(&k.neu)
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("[Update] FEHLER: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::atomic::{AtomicU32, Ordering};

    static ZAEHLER: AtomicU32 = AtomicU32::new(0);

    fn g(dir: &Path, args: &[&str]) -> Result<String, String> {
        let aus = Command::new("git")
            .current_dir(dir)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@depotschlopf.invalid",
            ])
            .args([
                "-c",
                "core.autocrlf=false",
                "-c",
                "core.eol=lf",
                "-c",
                "protocol.file.allow=always",
            ])
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TRACE", "1")
            .env("GIT_TRACE_SETUP", "1")
            .args(args)
            .output()
            .map_err(|e| format!("git: {e}"))?;
        if !aus.status.success() {
            // Einzeln ausgeben: auch das begrenzte Diagnoseprotokoll behält den Kontext.
            eprintln!("[Test-Git] Ordner: {}", dir.display());
            eprintln!(
                "[Test-Git] Befehl: git {} · Exit {:?}",
                args.join(" "),
                aus.status.code()
            );
            for zeile in String::from_utf8_lossy(&aus.stderr).lines() {
                eprintln!("[Test-Git] {zeile}");
            }
            return Err(format!(
                "Lokaler Git-Testbefehl fehlgeschlagen: git {}. Einzelheiten stehen im Test-Git-Protokoll.",
                args.join(" ")
            ));
        }
        Ok(String::from_utf8_lossy(&aus.stdout).trim().to_string())
    }

    /// Zwei Arbeitsstände; ein Git-Bundle liefert die Versionen ohne lokalen Shell-Transport.
    struct Aufbau {
        projekt: PathBuf,
        andere: PathBuf,
    }

    fn aufbau() -> Result<Aufbau, String> {
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("depot_update_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let projekt = dir.join("projekt");
        let andere = dir.join("andere");
        g(
            &dir,
            &["init", "-q", "-b", "main", &andere.to_string_lossy()],
        )?;
        fs::write(andere.join("a.txt"), "eins\n").map_err(|e| e.to_string())?;
        fs::write(andere.join("Cargo.lock"), LOCK).map_err(|e| e.to_string())?;
        g(&andere, &["add", "a.txt", "Cargo.lock"])?;
        g(&andere, &["commit", "-q", "-m", "eins"])?;
        g(&andere, &["bundle", "create", "../ursprung.bundle", "main"])?;
        // Git liest Bundles unmittelbar. Ein lokales Repository startet dagegen auch
        // bei fetch/push die Git-Bash-Shell und upload-pack/receive-pack.
        // Der echte Updater bleibt unverändert und holt hier weiterhin origin/main.
        g(&dir, &["init", "-q", "-b", "main", "projekt"])?;
        g(&projekt, &["remote", "add", "origin", "../ursprung.bundle"])?;
        g(
            &projekt,
            &[
                "fetch",
                "-q",
                "origin",
                "+refs/heads/main:refs/remotes/origin/main",
            ],
        )?;
        g(
            &projekt,
            &["checkout", "-q", "-B", "main", "--track", "origin/main"],
        )?;
        fs::create_dir_all(projekt.join("daten")).map_err(|e| e.to_string())?;
        Ok(Aufbau { projekt, andere })
    }

    #[test]
    fn abgestuerzte_sicherung_blockiert_update_nicht() -> Result<(), String> {
        let a = aufbau()?;
        let daten = a.projekt.join("daten");
        let status = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .args(["--exact", "update::tests::sicherungssperre_kind"])
            .env("DEPOT_TEST_SICHERUNG_KIND", &daten)
            .status()
            .map_err(|e| e.to_string())?;
        assert!(!status.success());
        assert!(daten.join("sicherung.lock").is_file());
        assert!(daten.join("sicherung_stand.txt").is_file());
        let _sperren = sperren_zum_einspielen(&daten)?;
        vorbedingungen(&a.projekt)
    }

    #[test]
    fn sicherungssperre_kind() -> Result<(), String> {
        let Some(daten) = std::env::var_os("DEPOT_TEST_SICHERUNG_KIND") else {
            return Ok(());
        };
        let daten = PathBuf::from(daten);
        let _sperre = crate::updatedaten::sicherung_sperren(&daten, "Belegt")?;
        fs::write(
            daten.join("sicherung_stand.txt"),
            "sicherung-stand-v1\nergebnis=laeuft\n",
        )
        .map_err(|e| e.to_string())?;
        std::process::exit(5);
    }

    #[test]
    fn laufende_sicherung_blockiert_update() -> Result<(), String> {
        let a = aufbau()?;
        let daten = a.projekt.join("daten");
        let _sperre = crate::updatedaten::sicherung_sperren(&daten, "Belegt")?;
        let vorher = git(&a.projekt, &["rev-parse", "HEAD"])?;
        let stand = daten.join("sicherung_stand.txt");
        fs::write(&stand, b"unberuehrt").map_err(|e| e.to_string())?;
        assert!(
            sperren_zum_einspielen(&daten)
                .is_err_and(|e| e == "Eine Sicherung läuft gerade. Kein Update.")
        );
        assert_eq!(git(&a.projekt, &["rev-parse", "HEAD"])?, vorher);
        assert_eq!(fs::read(&stand).map_err(|e| e.to_string())?, b"unberuehrt");
        assert!(!offen_pfad(&a.projekt).exists());
        let _update = crate::updatedaten::update_sperren(&daten)?;
        Ok(())
    }

    #[test]
    fn laufendes_update_blockiert_sicherung() -> Result<(), String> {
        let a = aufbau()?;
        let daten = a.projekt.join("daten");
        let _sperren = sperren_zum_einspielen(&daten)?;
        let stand = daten.join("sicherung_stand.txt");
        fs::write(&stand, b"unberuehrt").map_err(|e| e.to_string())?;
        let ziel = a.andere.join("DepotSchlopf");
        let tmp = ziel.join(format!("utc-{:020}-{:09}_{}.tmp", 1, 0, "a".repeat(40)));
        fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        fs::write(
            daten.join("sicherung_ziel.txt"),
            a.andere.display().to_string(),
        )
        .map_err(|e| e.to_string())?;
        assert_eq!(
            crate::sicherung::sichern(&a.projekt, false),
            Err("Sicherung oder Update läuft gerade".into())
        );
        assert_eq!(fs::read(&stand).map_err(|e| e.to_string())?, b"unberuehrt");
        assert!(tmp.is_dir());
        Ok(())
    }

    fn neuer_stand(a: &Aufbau, text: &str) -> Result<String, String> {
        fs::write(a.andere.join("a.txt"), text).map_err(|e| e.to_string())?;
        g(&a.andere, &["commit", "-q", "-am", text])?;
        g(
            &a.andere,
            &["bundle", "create", "../ursprung.bundle", "main"],
        )?;
        g(&a.andere, &["rev-parse", "HEAD"])
    }

    fn manifest_stand(pfad: &str, inhalt: &str) -> Result<(Aufbau, Kandidat), String> {
        let a = aufbau()?;
        g(&a.andere, &["rm", "a.txt"])?;
        let alt = g(&a.andere, &["rev-parse", "HEAD"])?;
        let datei = a.andere.join(pfad);
        fs::create_dir_all(datei.parent().ok_or("Manifestordner")?).map_err(|e| e.to_string())?;
        fs::write(datei, inhalt).map_err(|e| e.to_string())?;
        g(&a.andere, &["add", "--", pfad])?;
        g(&a.andere, &["commit", "-qm", "Manifestprobe"])?;
        let neu = g(&a.andere, &["rev-parse", "HEAD"])?;
        Ok((
            a,
            Kandidat {
                alt,
                neu,
                staende: Vec::new(),
            },
        ))
    }

    fn manifest_abgelehnt(pfad: &str, inhalt: &str) -> Result<(), String> {
        let (a, k) = manifest_stand(pfad, inhalt)?;
        let fehler = pruefe_kopie(&a.andere, &k, |_| false)
            .err()
            .ok_or("Manifest hätte abgelehnt werden müssen")?;
        assert!(fehler.contains(pfad), "{fehler}");
        assert!(!abbild::ablage(&a.andere).join("bau-update").exists());
        Ok(())
    }

    #[test]
    fn manifest_build_abgelehnt() -> Result<(), String> {
        manifest_abgelehnt(
            "crates/depot_kern/Cargo.toml",
            "[package]\nbuild = \"src/x.rs\"\n",
        )
    }

    #[test]
    fn manifest_build_false_erlaubt() -> Result<(), String> {
        let (a, k) = manifest_stand("crates/depot_kern/Cargo.toml", "[package]\nbuild = false\n")?;
        pruefe_baumpfade(&a.andere, &k.neu)
    }

    #[test]
    fn proc_macro_abgelehnt() -> Result<(), String> {
        manifest_abgelehnt("crates/depot_kern/Cargo.toml", "[lib]\nproc-macro = true\n")?;
        manifest_abgelehnt(
            "crates/depot_kern/Cargo.toml",
            "[lib]\ncrate-type = ['proc-macro']\n",
        )
    }

    #[test]
    fn patch_abgelehnt() -> Result<(), String> {
        manifest_abgelehnt("Cargo.toml", "[patch.crates-io]\n")?;
        manifest_abgelehnt("Cargo.toml", "[replace]\n")
    }

    #[test]
    fn unlesbares_manifest_abgelehnt() -> Result<(), String> {
        manifest_abgelehnt("Cargo.toml", "[package\n")
    }

    const LOCK: &str = "version = 4\n[[package]]\nname = \"bestand\"\nversion = \"1.0.0\"\n";
    const LOCK_NEU: &str = "[[package]]\nname = \"foo\"\nversion = \"1.2.3\"\n";

    fn lock_stand(anhang: &str) -> Result<(Aufbau, Kandidat), String> {
        let a = aufbau()?;
        g(&a.andere, &["rm", "a.txt"])?;
        let alt = g(&a.andere, &["rev-parse", "HEAD"])?;
        fs::create_dir_all(a.andere.join("daten")).map_err(|e| e.to_string())?;
        fs::write(a.andere.join("Cargo.lock"), format!("{LOCK}{anhang}"))
            .map_err(|e| e.to_string())?;
        g(
            &a.andere,
            &["commit", "--allow-empty", "-qam", "Neuer Lock-Stand"],
        )?;
        let neu = g(&a.andere, &["rev-parse", "HEAD"])?;
        Ok((
            a,
            Kandidat {
                alt,
                neu,
                staende: Vec::new(),
            },
        ))
    }

    #[test]
    fn lock_unveraendert_geht_durch() -> Result<(), String> {
        let (a, k) = lock_stand("")?;
        let mut gefragt = false;
        pruefe_lock(&a.andere, &k, |_| {
            gefragt = true;
            false
        })?;
        assert!(!gefragt);
        Ok(())
    }

    #[test]
    fn lock_geaendert_abgelehnt() -> Result<(), String> {
        let (a, k) = lock_stand(LOCK_NEU)?;
        let fehler = pruefe_kopie(&a.andere, &k, |_| false)
            .err()
            .ok_or("Update hätte abgelehnt werden müssen")?;
        assert!(fehler.contains("Bitte von Hand freigeben"), "{fehler}");
        assert!(!abbild::ablage(&a.andere).join("bau-update").exists());
        assert!(!a.andere.join("daten/update_abhaengigkeiten.txt").exists());
        Ok(())
    }

    #[test]
    fn lock_freigabe_dialog_ja() -> Result<(), String> {
        let (a, k) = lock_stand(LOCK_NEU)?;
        pruefe_lock(&a.andere, &k, |hinweis| hinweis.contains("foo 1.2.3"))?;
        let text = fs::read_to_string(a.andere.join("daten/update_abhaengigkeiten.txt"))
            .map_err(|e| e.to_string())?;
        let aenderung = lock_aenderung(&a.andere, &k)?.ok_or("Lock geändert")?;
        assert_eq!(
            text,
            format!("update-lock-v1;{};{}\n", k.neu, aenderung.abdruck)
        );
        pruefe_lock(&a.andere, &k, |_| false)
    }

    #[test]
    fn lock_freigabe_dialog_nein() -> Result<(), String> {
        let (a, k) = lock_stand(LOCK_NEU)?;
        let fehler = pruefe_lock(&a.andere, &k, |_| false)
            .err()
            .ok_or("Update hätte abgelehnt werden müssen")?;
        assert!(fehler.contains("Bitte von Hand freigeben"), "{fehler}");
        assert!(!a.andere.join("daten/update_abhaengigkeiten.txt").exists());
        Ok(())
    }

    #[test]
    fn lock_freigabe_zeigt_pakete() -> Result<(), String> {
        let (a, k) = lock_stand(LOCK_NEU)?;
        let aenderung = lock_aenderung(&a.andere, &k)?.ok_or("Lock geändert")?;
        assert_eq!(lock_pakete(&aenderung), "foo 1.2.3");
        Ok(())
    }

    #[test]
    fn lock_freigabe_gilt_nicht_fuer_anderen_commit() -> Result<(), String> {
        let (a, mut k) = lock_stand(LOCK_NEU)?;
        pruefe_lock(&a.andere, &k, |_| true)?;
        g(
            &a.andere,
            &["commit", "--allow-empty", "-qm", "Anderer Commit"],
        )?;
        k.neu = g(&a.andere, &["rev-parse", "HEAD"])?;
        assert!(pruefe_lock(&a.andere, &k, |_| false).is_err());
        Ok(())
    }

    #[test]
    fn update_lehnt_unzulaessige_baumpfade_vor_der_pruefkopie_ab() -> Result<(), String> {
        let a = aufbau()?;
        g(&a.andere, &["rm", "a.txt"])?;
        fs::write(a.andere.join("README.md"), "# Probe\n").map_err(|e| e.to_string())?;
        g(&a.andere, &["add", "README.md"])?;
        g(&a.andere, &["commit", "-qm", "zulässiger Stand"])?;
        let alt = g(&a.andere, &["rev-parse", "HEAD"])?;
        pruefe_baumpfade(&a.andere, &alt)?;

        for pfad in [
            ".cargo/config.toml",
            "docs/.CARGO/config",
            "crates/probe/src/build.rs",
            "unbekannt.sh",
        ] {
            let datei = a.andere.join(pfad);
            fs::create_dir_all(datei.parent().ok_or("Ordner fehlt")?).map_err(|e| e.to_string())?;
            fs::write(datei, "Probe\n").map_err(|e| e.to_string())?;
            g(&a.andere, &["add", "--", pfad])?;
            g(&a.andere, &["commit", "-qm", "unzulässiger Stand"])?;
            let k = Kandidat {
                alt: alt.clone(),
                neu: g(&a.andere, &["rev-parse", "HEAD"])?,
                staende: Vec::new(),
            };
            let fehler = pruefe_kopie(&a.andere, &k, |_| false)
                .err()
                .ok_or("Update hätte abgelehnt werden müssen")?;
            assert!(fehler.contains(pfad), "{fehler}");
            assert!(!abbild::ablage(&a.andere).join("update").exists());
            g(&a.andere, &["reset", "--hard", &alt])?;
        }
        Ok(())
    }

    #[test]
    fn gate_checkout_behaelt_lf_trotz_globaler_git_einstellung() -> Result<(), String> {
        let a = aufbau()?;
        let config = a.projekt.join("daten/gitconfig");
        fs::write(&config, "[core]\n    autocrlf = true\n    eol = crlf\n")
            .map_err(|e| e.to_string())?;
        let gate = abbild::gate_befehl(&a.projekt, &a.projekt.join("bau"), &[]);
        let mut git = Command::new("git");
        // Die von Cargo an Git vererbte Umgebung übernehmen und eine Nutzerconfig simulieren.
        for (schluessel, wert) in gate.get_envs() {
            if let Some(wert) = wert {
                git.env(schluessel, wert);
            } else {
                git.env_remove(schluessel);
            }
        }
        let datei = a.projekt.join("a.txt");
        fs::remove_file(&datei).map_err(|e| e.to_string())?;
        let aus = git
            .current_dir(&a.projekt)
            .env("GIT_CONFIG_GLOBAL", config)
            .args(["checkout", "--", "a.txt"])
            .output()
            .map_err(|e| e.to_string())?;
        assert!(
            aus.status.success(),
            "{}",
            String::from_utf8_lossy(&aus.stderr)
        );
        assert_eq!(fs::read(&datei).map_err(|e| e.to_string())?, b"eins\n");
        Ok(())
    }

    #[test]
    fn einspielen_lehnt_neueren_als_den_bestaetigten_commit_ab() -> Result<(), String> {
        let a = aufbau()?;
        let alt = g(&a.projekt, &["rev-parse", "HEAD"])?;
        let erwartet = neuer_stand(&a, "zwei")?;
        neuer_stand(&a, "drei")?;
        assert_eq!(lauf(&a.projekt, &[erwartet]), ExitCode::FAILURE);
        assert_eq!(g(&a.projekt, &["rev-parse", "HEAD"])?, alt);
        assert!(!offen_pfad(&a.projekt).exists());
        Ok(())
    }

    #[test]
    fn offen_hat_festes_format() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        assert_eq!(
            offen_text(&a, &b),
            format!("update-v1\nvorher={a}\nneu={b}\n")
        );
    }

    #[test]
    fn nur_volle_commit_namen() {
        assert!(ist_commit(&"0".repeat(40)));
        assert!(ist_commit(&"f".repeat(64)));
        assert!(!ist_commit(&"F".repeat(40)));
        assert!(!ist_commit("abc"));
        assert!(!ist_commit(&format!("{}-", "0".repeat(39))));
    }

    #[test]
    fn testursprung_liefert_neue_staende_ohne_upload_pack() -> Result<(), String> {
        let a = aufbau()?;
        assert_eq!(
            g(&a.projekt, &["remote", "get-url", "origin"])?,
            "../ursprung.bundle"
        );
        // Gegenprobe: Ein Repository-Transport müsste dieses fehlende Programm
        // starten und scheitern. Der Bundle-Abruf braucht es auch im Produktivpfad nicht.
        g(
            &a.projekt,
            &[
                "config",
                "remote.origin.uploadpack",
                "depotschlopf-test-kein-upload-pack",
            ],
        )?;
        assert_eq!(g(&a.projekt, &["symbolic-ref", "--short", "HEAD"])?, "main");
        assert_eq!(
            g(&a.projekt, &["rev-parse", "--abbrev-ref", "@{upstream}"])?,
            "origin/main"
        );
        let alt = g(&a.andere, &["rev-parse", "HEAD"])?;
        assert_eq!(g(&a.projekt, &["rev-parse", "HEAD"])?, alt);
        assert!(g(&a.projekt, &["status", "--porcelain"])?.is_empty());
        assert_eq!(
            fs::read(a.projekt.join("a.txt")).map_err(|e| e.to_string())?,
            b"eins\n"
        );
        let neu = neuer_stand(&a, "lokaler Nachweis\n")?;
        let kandidat = hole(&a.projekt)?.ok_or("Lokaler Kandidat fehlt")?;
        assert_eq!(kandidat.neu, neu);
        assert_eq!(
            g(&a.projekt, &["rev-parse", "HEAD"])?,
            alt,
            "Prüfen spielt nichts ein"
        );
        Ok(())
    }

    #[test]
    fn nichts_neues_ist_kein_update() -> Result<(), String> {
        let a = aufbau()?;
        vorbedingungen(&a.projekt)?;
        assert!(hole(&a.projekt)?.is_none());
        Ok(())
    }

    #[test]
    fn neuer_stand_wird_genannt_und_nur_vorwaerts_eingespielt() -> Result<(), String> {
        let a = aufbau()?;
        let alt = g(&a.projekt, &["rev-parse", "HEAD"])?;
        let neu = neuer_stand(&a, "zwei\n")?;
        let k = hole(&a.projekt)?.ok_or("Vorbedingung: Update erwartet")?;
        assert_eq!(
            (k.alt.as_str(), k.neu.as_str(), k.staende.len()),
            (alt.as_str(), neu.as_str(), 1)
        );
        for option in ["pruefen", "prüfen"] {
            assert_eq!(lauf(&a.projekt, &[option.into()]), ExitCode::SUCCESS);
            assert_eq!(g(&a.projekt, &["rev-parse", "HEAD"])?, alt);
            assert!(!offen_pfad(&a.projekt).exists());
        }
        einspielen(&a.projekt, &k)?;
        assert_eq!(g(&a.projekt, &["rev-parse", "HEAD"])?, neu);
        assert_eq!(
            fs::read_to_string(offen_pfad(&a.projekt)).map_err(|e| e.to_string())?,
            offen_text(&alt, &neu)
        );
        // Solange es offen ist, gibt es kein weiteres Update.
        assert!(vorbedingungen(&a.projekt).is_err());
        Ok(())
    }

    #[test]
    fn update_rueckt_nur_nach_datenprobe_und_gepruefter_sicherung_vor() -> Result<(), String> {
        let a = aufbau()?;
        let neu = neuer_stand(&a, "zwei\n")?;
        let k = hole(&a.projekt)?.ok_or("Update erwartet")?;
        let daten = a.projekt.join("daten");
        let journal = daten.join("journal.txt");
        let schreibe = |p, bytes: &[u8]| fs::write(p, bytes).map_err(|e| e.to_string());
        schreibe(&journal, b"Original")?;
        let sicherung = daten.join("sicherung");
        let pruefe = |probe: &Path| {
            let bytes = fs::read(probe.join("journal.txt")).map_err(|e| e.to_string())?;
            assert_eq!(bytes, b"Original");
            Ok(())
        };
        let e = einspielen_mit_datenprobe(&a.projekt, &k, &a.projekt, |_| {
            Err("unlesbarer Freigabebeleg".into())
        });
        assert_eq!(e, Err("unlesbarer Freigabebeleg".into()));
        assert!(!offen_pfad(&a.projekt).exists());
        assert!(!sicherung.exists());
        assert_eq!(g(&a.projekt, &["rev-parse", "HEAD"])?, k.alt);

        schreibe(&sicherung, b"blockiert")?;
        let e = einspielen_mit_datenprobe(&a.projekt, &k, &a.projekt, pruefe);
        assert!(e.is_err_and(|e| e.contains("Datensicherung fehlgeschlagen")));
        assert!(!offen_pfad(&a.projekt).exists());
        assert_eq!(g(&a.projekt, &["rev-parse", "HEAD"])?, k.alt);

        fs::remove_file(&sicherung).map_err(|e| e.to_string())?;
        einspielen_mit_datenprobe(&a.projekt, &k, &a.projekt, pruefe)?;
        let mut ordner = fs::read_dir(sicherung).map_err(|e| e.to_string())?;
        let backup = ordner
            .next()
            .ok_or("Sicherung fehlt")?
            .map_err(|e| e.to_string())?
            .path();
        assert!(ordner.next().is_none());
        assert_eq!(
            fs::read(backup.join("journal.txt")).map_err(|e| e.to_string())?,
            b"Original"
        );
        assert_eq!(fs::read(journal).map_err(|e| e.to_string())?, b"Original");
        assert_eq!(g(&a.projekt, &["rev-parse", "HEAD"])?, neu);
        Ok(())
    }

    #[test]
    fn lokale_aenderungen_und_fremde_zweige_verhindern_das_update() -> Result<(), String> {
        let a = aufbau()?;
        neuer_stand(&a, "zwei\n")?;
        fs::write(a.projekt.join("a.txt"), "lokal\n").map_err(|e| e.to_string())?;
        assert!(vorbedingungen(&a.projekt).is_err());
        g(&a.projekt, &["checkout", "-q", "--", "a.txt"])?;
        vorbedingungen(&a.projekt)?;
        g(&a.projekt, &["checkout", "-q", "-b", "eigen"])?;
        assert!(vorbedingungen(&a.projekt).is_err());
        g(&a.projekt, &["checkout", "-q", "main"])?;
        fs::write(a.projekt.join("daten").join("papier.sperre"), "").map_err(|e| e.to_string())?;
        assert!(vorbedingungen(&a.projekt).is_err());
        Ok(())
    }

    #[test]
    fn eigener_lokaler_stand_wird_nicht_ueberschrieben() -> Result<(), String> {
        let a = aufbau()?;
        neuer_stand(&a, "zwei\n")?;
        fs::write(a.projekt.join("b.txt"), "eigen\n").map_err(|e| e.to_string())?;
        g(&a.projekt, &["add", "b.txt"])?;
        g(&a.projekt, &["commit", "-q", "-m", "eigen"])?;
        assert!(hole(&a.projekt).is_err());
        Ok(())
    }

    #[test]
    fn abgelehnter_stand_kommt_nicht_wieder() -> Result<(), String> {
        let a = aufbau()?;
        let neu = neuer_stand(&a, "zwei\n")?;
        fs::write(abgelehnt_pfad(&a.projekt), format!("{neu}\n")).map_err(|e| e.to_string())?;
        assert!(hole(&a.projekt).is_err());
        // Ein neuerer Stand wird wieder angeboten.
        let neuer = neuer_stand(&a, "drei\n")?;
        let k = hole(&a.projekt)?.ok_or("Vorbedingung: Update erwartet")?;
        assert_eq!(k.neu, neuer);
        Ok(())
    }
}
