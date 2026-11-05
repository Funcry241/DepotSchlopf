//! Der gesicherte Programmstand (Maus, 05.11.2026): Nach einem Neustart startet der zuletzt
//! gepruefte Stand ohne Cargo, aber nur, wenn er genau zum Projektordner passt.
//!
//! Abnahmefall 2, erster Teil: Windows neu starten. Der Autostart ruft `DepotSchlopf.cmd`, das
//! `daten/programm/starte.cmd` ruft; der Waechter nimmt den Stand nur, wenn `startklar`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use depot_dienst::programmstand::{
    self, PROGRAMME, datei, geprueft_pfad, sauberer_baum, sichere, starter_pfad, startklar,
};
use depot_dienst::rueckfall;
use depot_dienst::werkbank::Art;

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let aus = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@depotschlopf.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .map_err(|e| format!("Vorbedingung git: {e}"))?;
    if !aus.status.success() {
        return Err(format!(
            "Vorbedingung git {args:?}: {}",
            String::from_utf8_lossy(&aus.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&aus.stdout).trim().to_string())
}

/// Ein Projektordner mit einem Commit, `daten/` ignoriert, und gebaute Programme in `bau/`.
fn projekt() -> Result<(PathBuf, PathBuf), String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_programmstand_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let wurzel = dir.join("projekt");
    let bau = dir.join("bau");
    let v = |r: std::io::Result<()>| r.map_err(|e| format!("Vorbedingung: {e}"));
    v(fs::create_dir_all(wurzel.join("daten")))?;
    v(fs::create_dir_all(&bau))?;
    v(fs::write(wurzel.join(".gitignore"), "/daten/\n"))?;
    v(fs::write(wurzel.join("a.txt"), "eins\n"))?;
    git(&wurzel, &["init", "-q"])?;
    git(&wurzel, &["add", "-A"])?;
    git(&wurzel, &["commit", "-q", "-m", "eins"])?;
    for p in PROGRAMME {
        v(fs::write(bau.join(datei(p)), format!("programm {p}")))?;
    }
    Ok((wurzel, bau))
}

fn als_geprueft(wurzel: &Path, baum: &str) -> Result<(), String> {
    fs::write(geprueft_pfad(wurzel), format!("geprueft-v1;{baum}\n"))
        .map_err(|e| format!("Vorbedingung geprueft: {e}"))
}

/// Gesichert wird nur ein Baum, der das Gate bestanden hat, und nur aus einem sauberen
/// Projektordner auf genau diesem Baum.
#[test]
fn nur_ein_gepruefter_sauberer_stand_wird_gesichert() -> Result<(), String> {
    let (wurzel, bau) = projekt()?;
    let baum = sauberer_baum(&wurzel)?;
    let e = sichere(&wurzel, &baum, &bau).err().unwrap_or_default();
    assert!(e.contains("Gate"), "{e}");
    assert!(!starter_pfad(&wurzel).exists());
    als_geprueft(&wurzel, &baum)?;
    // Lokale Aenderung: nicht sicher, woraus gebaut wurde.
    fs::write(wurzel.join("a.txt"), "anders\n").map_err(|e| e.to_string())?;
    let e = sichere(&wurzel, &baum, &bau).err().unwrap_or_default();
    assert!(e.contains("lokale Änderungen"), "{e}");
    git(&wurzel, &["checkout", "-q", "--", "a.txt"])?;
    let s = sichere(&wurzel, &baum, &bau)?;
    assert_eq!(s.baum, baum);
    for p in PROGRAMME {
        assert_eq!(
            fs::read_to_string(s.programm(p)).map_err(|e| e.to_string())?,
            format!("programm {p}")
        );
    }
    let start = fs::read_to_string(starter_pfad(&wurzel)).map_err(|e| e.to_string())?;
    assert!(
        start.contains(&format!("{}\" %* & exit /b", datei("depot_waechter"))),
        "{start}"
    );
    assert_eq!(startklar(&wurzel)?, s);
    Ok(())
}

/// Nach dem Neustart: startklar nur, wenn alles passt. Sonst sagt der Grund, warum gebaut wird.
#[test]
fn startklar_nur_wenn_alles_passt() -> Result<(), String> {
    let (wurzel, bau) = projekt()?;
    let baum = sauberer_baum(&wurzel)?;
    als_geprueft(&wurzel, &baum)?;
    let s = sichere(&wurzel, &baum, &bau)?;
    assert!(startklar(&wurzel).is_ok());

    // Ein Update auf Probe: immer mit Cargo, damit der Rueckfall greifen kann.
    let offen = rueckfall::offen_pfad(&wurzel);
    fs::write(&offen, "x").map_err(|e| e.to_string())?;
    let e = startklar(&wurzel).err().unwrap_or_default();
    assert!(e.contains("Update"), "{e}");
    fs::remove_file(&offen).map_err(|e| e.to_string())?;

    // Eine veraenderte Programmdatei.
    let dienst = s.programm("depot_dienst");
    fs::write(&dienst, "fremd").map_err(|e| e.to_string())?;
    let e = startklar(&wurzel).err().unwrap_or_default();
    assert!(e.contains("Abdruck"), "{e}");
    fs::write(&dienst, "programm depot_dienst").map_err(|e| e.to_string())?;
    assert!(startklar(&wurzel).is_ok());

    // Ein neuer Stand im Projektordner (etwa von Hand gezogen): bauen.
    fs::write(wurzel.join("a.txt"), "zwei\n").map_err(|e| e.to_string())?;
    git(&wurzel, &["commit", "-q", "-am", "zwei"])?;
    let e = startklar(&wurzel).err().unwrap_or_default();
    assert!(e.contains("anderen Stand"), "{e}");

    // Der neue Stand, geprueft und gesichert, ersetzt den alten; der alte bleibt als Vorgaenger.
    let neu = sauberer_baum(&wurzel)?;
    fs::write(
        geprueft_pfad(&wurzel),
        format!("geprueft-v1;{baum}\ngeprueft-v1;{neu}\n"),
    )
    .map_err(|e| e.to_string())?;
    let s2 = sichere(&wurzel, &neu, &bau)?;
    assert_eq!(startklar(&wurzel)?, s2);
    assert!(s.ordner.exists(), "Vorgaenger bleibt");
    Ok(())
}

/// Aus dem Stand starten Kurse, Termine und Papiertag direkt die Programme des Stands; nur das
/// Update geht ueber Cargo.
#[test]
fn auftraege_starten_die_programme_des_stands() -> Result<(), String> {
    let (wurzel, bau) = projekt()?;
    let baum = sauberer_baum(&wurzel)?;
    als_geprueft(&wurzel, &baum)?;
    let s = sichere(&wurzel, &baum, &bau)?;
    let starter = programmstand::starter(&s);
    let programm = |art: Art| starter(art, &wurzel).get_program().to_os_string();
    let args = |art: Art| -> Vec<String> {
        starter(art, &wurzel)
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    };
    assert_eq!(
        programm(Art::Kurse),
        s.programm("depot_sammler").into_os_string()
    );
    assert_eq!(args(Art::Kurse), vec!["kurse"]);
    assert_eq!(args(Art::Termine), vec!["termine"]);
    assert_eq!(
        programm(Art::Papier),
        s.programm("depot_berater").into_os_string()
    );
    assert_eq!(args(Art::Papier), vec!["papier"]);
    assert_eq!(programm(Art::Update), "cargo");
    Ok(())
}
