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
             vorwaerts ein und überschreibt nichts."
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
        "[Update] Neu auf {ZWEIG}: {} Stand/Staende, {} -> {}",
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

/// Prueft den neuen Stand in seiner Kopie mit dem ganzen Gate.
fn pruefe_kopie(wurzel: &Path, k: &Kandidat) -> Result<abbild::Abbild, String> {
    let ablage = abbild::ablage(wurzel);
    println!("[Update] Kopie von {} anlegen", kurz(&k.neu));
    let abbild = abbild::festhalten_commit(wurzel, &ablage, "update", &k.neu)?;
    let basis = format!("{}^1", k.neu);
    let args: Vec<String> = vec!["--voll".into(), "--basis".into(), basis];
    println!(
        "[Update] Gate im Prüfordner: cargo run -p depot_kodex -- {}",
        args.join(" ")
    );
    let status = abbild::gate_befehl(&abbild.kopie, &ablage.join("bau-update"), &args)
        .status()
        .map_err(|e| format!("cargo nicht startbar: {e}"))?;
    if !status.success() {
        return Err("Das Gate hat den neuen Stand abgelehnt. Der alte Stand bleibt.".into());
    }
    let probleme = abbild::vergleiche(wurzel, &abbild);
    if !probleme.is_empty() {
        return Err(format!(
            "Die Kopie hat sich waehrend des Gates veraendert; der neue Stand gilt als \
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
        return Err("Der Projektordner hat sich waehrend der Prüfung bewegt. Kein Update.".into());
    }
    offen_schreiben(wurzel, &offen_text(&k.alt, &k.neu))?;
    if let Err(e) = git(wurzel, &["merge", "--ff-only", "--quiet", &k.neu]) {
        // Nichts eingespielt: Die Markierung darf keinen Rueckfall ausloesen.
        let weg = fs::remove_file(offen_pfad(wurzel));
        return Err(match weg {
            Ok(()) => format!("Vorruecken gescheitert, alter Stand bleibt: {e}"),
            Err(w) => format!(
                "Vorruecken gescheitert, alter Stand bleibt: {e}. daten/update_offen.txt bitte \
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
        [o] if o == "pruefen" => (true, None),
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
        println!("[Update] Nur geprüft; eingespielt wird mit `run.cmd update`.");
        println!("[Update] Kandidat: {}", k.neu);
        return ExitCode::SUCCESS;
    }
    let ergebnis = crate::updatedaten::update_sperren(&wurzel.join("daten")).and_then(|sperre| {
        let ergebnis = pruefe_kopie(wurzel, &k).and_then(|kopie| {
            einspielen_mit_datenprobe(wurzel, &k, &abbild::ablage(wurzel), |probe| {
                pruefe_datenkopie(wurzel, &kopie, probe)
            })?;
            vermerke_geprueft(wurzel, &kopie.baum);
            Ok(())
        });
        let _ = sperre.unlock();
        ergebnis
    });
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
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@depotschlopf.invalid",
            ])
            .args(args)
            .output()
            .map_err(|e| format!("git: {e}"))?;
        if !aus.status.success() {
            return Err(format!(
                "git {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&aus.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&aus.stdout).trim().to_string())
    }

    /// Ein Ursprung (`origin`), ein Projektordner und ein zweiter Klon, der auf `main` schiebt.
    struct Aufbau {
        projekt: PathBuf,
        andere: PathBuf,
    }

    fn aufbau() -> Result<Aufbau, String> {
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("depot_update_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let ursprung = dir.join("ursprung.git");
        let projekt = dir.join("projekt");
        let andere = dir.join("andere");
        g(
            &dir,
            &[
                "init",
                "-q",
                "--bare",
                "-b",
                "main",
                &ursprung.to_string_lossy(),
            ],
        )?;
        g(
            &dir,
            &["init", "-q", "-b", "main", &andere.to_string_lossy()],
        )?;
        fs::write(andere.join("a.txt"), "eins\n").map_err(|e| e.to_string())?;
        g(&andere, &["add", "a.txt"])?;
        g(&andere, &["commit", "-q", "-m", "eins"])?;
        g(
            &andere,
            &["remote", "add", "origin", &ursprung.to_string_lossy()],
        )?;
        g(&andere, &["push", "-q", "origin", "main"])?;
        g(
            &dir,
            &[
                "clone",
                "-q",
                &ursprung.to_string_lossy(),
                &projekt.to_string_lossy(),
            ],
        )?;
        fs::create_dir_all(projekt.join("daten")).map_err(|e| e.to_string())?;
        Ok(Aufbau { projekt, andere })
    }

    fn neuer_stand(a: &Aufbau, text: &str) -> Result<String, String> {
        fs::write(a.andere.join("a.txt"), text).map_err(|e| e.to_string())?;
        g(&a.andere, &["commit", "-q", "-am", text])?;
        g(&a.andere, &["push", "-q", "origin", "main"])?;
        g(&a.andere, &["rev-parse", "HEAD"])
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
        // Pruefen allein aendert nichts.
        assert_eq!(g(&a.projekt, &["rev-parse", "HEAD"])?, alt);
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
