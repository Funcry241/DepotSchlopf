//! Update mit Rueckfall im Dienst: nur allein, Neustart erst wenn nichts mehr wartet, Rueckfall
//! nur mit Git und nur vom bekannten Stand, die Seite laedt sich nach einem Neustart neu.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use depot_dienst::rueckfall::{self, Offen};
use depot_dienst::werkbank::{Anlass, Art, Dienst, Lagerechner, Starter};
use depot_dienst::{Umwelt, behandle, waechter};
use depot_kern::{BeraterId, Datum, Zeitpunkt};
use depot_lage::{Hinterlegt, Umgebung, Zugang, lagebild};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);
const TOKEN: &str = "0123456789abcdef0123456789abcdef";
const UPDATE_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HOST: &str = "127.0.0.1:47410";
const HERKUNFT: &str = "http://127.0.0.1:47410";

fn some<T>(o: Option<T>, was: &str) -> Result<T, String> {
    o.ok_or_else(|| format!("Vorbedingung {was}"))
}

fn ordner() -> Result<PathBuf, String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_dienst_update_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("daten")).map_err(|e| format!("Vorbedingung Ordner: {e}"))?;
    Ok(dir)
}

fn mittag() -> Result<Zeitpunkt, String> {
    some(
        Datum::neu(2026, 11, 5).and_then(|d| Zeitpunkt::frankfurt(d, 12, 0, 0)),
        "Zeitpunkt",
    )
}

fn echo_starter() -> Starter {
    Box::new(|art, _wurzel| {
        let zeile = if art == Art::UpdatePruefen {
            format!("[Update] Kandidat: {UPDATE_SHA}")
        } else {
            format!("lauf {}", art.kurz())
        };
        #[cfg(windows)]
        let mut c = Command::new("cmd");
        #[cfg(windows)]
        c.args(["/C", format!("echo {zeile}").as_str()]);
        #[cfg(not(windows))]
        let mut c = Command::new("sh");
        #[cfg(not(windows))]
        c.args(["-c", "printf '%s\\n' \"$@\"", "test", &zeile]);
        c
    })
}

fn leeres_lagebild() -> Lagerechner {
    Box::new(|_wurzel: &Path, jetzt: Zeitpunkt| {
        lagebild(&Umgebung {
            jetzt,
            journal: Zugang::NichtEingerichtet,
            kursquelle: Hinterlegt::Nein,
            kursquelle_us: Hinterlegt::Nein,
            berater: [
                (BeraterId::Dachs, Hinterlegt::Nein),
                (BeraterId::Maus, Hinterlegt::Nein),
            ],
            sperren: Vec::new(),
            anbieter: Vec::new(),
            anbieter_unlesbar: 0,
            ki_halt: None,
        })
    })
}

fn dienst(wurzel: PathBuf) -> Arc<Dienst> {
    Arc::new(Dienst::neu(
        wurzel,
        TOKEN.into(),
        echo_starter(),
        leeres_lagebild(),
    ))
}

struct Leer;

impl Umwelt for Leer {
    fn schluessel(&self, _wer: depot_dienst::schluessel::Wer) -> Hinterlegt {
        Hinterlegt::Nein
    }
    fn schluessel_ablegen(
        &self,
        _wer: depot_dienst::schluessel::Wer,
        _text: &str,
    ) -> Result<(), String> {
        Err("nicht in Tests".into())
    }
    fn autostart(&self, _wurzel: &Path) -> depot_dienst::autostart::Stand {
        depot_dienst::autostart::Stand::Aus
    }
    fn autostart_setzen(&self, _wurzel: &Path, _an: bool) -> Result<(), String> {
        Err("nicht in Tests".into())
    }
}

fn get(d: &Dienst, pfad: &str) -> Result<String, String> {
    let (p, abfrage) = pfad.split_once('?').unwrap_or((pfad, ""));
    let a = depot_dienst::http::Anfrage {
        methode: "GET".into(),
        pfad: p.into(),
        abfrage: abfrage.into(),
        kopf: vec![("Host".into(), HOST.into())],
        rumpf: Vec::new(),
    };
    Ok(String::from_utf8_lossy(&behandle(d, &Leer, &a, mittag()?)).into_owned())
}

fn post_auftrag(d: &Dienst, art: &str) -> Result<String, String> {
    let a = depot_dienst::http::Anfrage {
        methode: "POST".into(),
        pfad: "/ausfuehren".into(),
        abfrage: String::new(),
        kopf: vec![
            ("Host".into(), HOST.into()),
            ("Origin".into(), HERKUNFT.into()),
            (
                "Content-Type".into(),
                "application/x-www-form-urlencoded".into(),
            ),
        ],
        rumpf: format!("t={TOKEN}&art={art}").into_bytes(),
    };
    Ok(String::from_utf8_lossy(&behandle(d, &Leer, &a, mittag()?)).into_owned())
}

fn offen_schreiben(wurzel: &Path, vorher: &str, neu: &str) -> Result<(), String> {
    fs::write(
        rueckfall::offen_pfad(wurzel),
        format!("update-v1\nvorher={vorher}\nneu={neu}\n"),
    )
    .map_err(|e| format!("Vorbedingung offen: {e}"))
}

fn lauf_ab(d: &Arc<Dienst>, t: Zeitpunkt) -> Result<Art, String> {
    let (nr, kind) = some(d.schritt(t), "Start")?;
    let art = some(d.sperre().laufend.as_ref().map(|a| a.art), "laufend")?;
    d.verfolge(nr, kind, || mittag().ok());
    Ok(art)
}

fn suche_update(d: &Arc<Dienst>, t: Zeitpunkt) -> Result<(), String> {
    d.einreihen(Art::UpdatePruefen, Anlass::Hand)?;
    assert_eq!(lauf_ab(d, t)?, Art::UpdatePruefen);
    Ok(())
}

fn bestaetige_update(d: &Dienst) -> Result<String, String> {
    post_auftrag(d, &format!("update&geprueft=ja&sha={UPDATE_SHA}"))
}

#[test]
fn update_laeuft_allein_und_sperrt_auftraege_bis_zum_neustart() -> Result<(), String> {
    let wurzel = ordner()?;
    let t = mittag()?;
    let d = dienst(wurzel.clone());
    assert!(d.einreihen(Art::Update, Anlass::Hand).is_err());
    assert!(d.von_hand(Art::Update, t).is_err());
    assert!(d.schritt(t).is_none());
    suche_update(&d, t)?;
    d.einreihen(Art::Termine, Anlass::Hand)?;
    assert!(bestaetige_update(&d)?.contains("m=update_belegt"));
    let d = dienst(wurzel.clone());
    suche_update(&d, t)?;
    assert!(bestaetige_update(&d)?.contains("m=update#update"));
    assert!(d.einreihen(Art::Kurse, Anlass::Hand).is_err());
    assert_eq!(lauf_ab(&d, t)?, Art::Update);
    // Ohne update_offen.txt hat der Lauf nichts eingespielt: kein Neustart.
    assert!(!d.sperre().neustart);
    let d = dienst(wurzel.clone());
    suche_update(&d, t)?;
    assert!(bestaetige_update(&d)?.contains("m=update#update"));
    let (nr, kind) = some(d.schritt(t), "Update gestartet")?;
    assert!(d.einreihen(Art::Kurse, Anlass::Hand).is_err());
    offen_schreiben(&wurzel, &"a".repeat(40), &"b".repeat(40))?;
    d.verfolge(nr, kind, || mittag().ok());
    assert!(d.neustart_bereit());
    assert!(d.einreihen(Art::Update, Anlass::Hand).is_err());
    assert!(d.einreihen(Art::Kurse, Anlass::Hand).is_err());
    let probe = dienst(wurzel);
    probe.einreihen(Art::Kurse, Anlass::Hand)?;
    assert!(probe.schritt(t).is_none(), "Probezeit hält Aufträge zurück");
    assert!(get(&d, "/teil/live")?.contains("Der Dienst startet neu."));
    Ok(())
}

#[test]
fn knoepfe_auf_der_einrichtung_und_marke_des_starts() -> Result<(), String> {
    let d = dienst(ordner()?);
    let seite = get(&d, "/einrichtung")?;
    assert!(seite.contains("id=\"update\""));
    assert!(seite.contains("name=\"art\" value=\"update_pruefen\""));
    assert!(!seite.contains("/bestaetigen?art=update"));
    let marke = format!("data-start=\"{}\"", d.start);
    assert!(d.start.len() == 16 && !TOKEN.starts_with(&d.start));
    for pfad in ["/", "/teil/live", "/teil/status", "/einrichtung"] {
        assert!(get(&d, pfad)?.contains(&marke), "{pfad}");
    }
    assert_ne!(
        dienst(ordner()?).start,
        d.start,
        "jeder Start eine eigene Marke"
    );
    let js = get(&d, "/app.js")?;
    assert!(js.contains("data-start") && js.contains("location.replace"));

    let r = post_auftrag(&d, "update_pruefen")?;
    assert!(r.contains("Location: /einrichtung?m=update#update\r\n"));
    let r = post_auftrag(&d, "update")?;
    assert!(
        r.contains("Location: /einrichtung?m=update_bestaetigen#update\r\n"),
        "Bestätigung fehlt"
    );
    let r = post_auftrag(&d, "update&geprueft=ja")?;
    assert!(r.contains("Location: /einrichtung?m=update_suche#update\r\n"));
    assert!(get(&d, "/teil/update")?.contains("wartet"));
    Ok(())
}

#[test]
fn update_ohne_commit_startet_auch_bei_fehlerhaftem_warteschlangeneintrag_nicht()
-> Result<(), String> {
    let d = dienst(ordner()?);
    d.einreihen(Art::UpdatePruefen, Anlass::Hand)?;
    some(
        d.sperre().warteschlange.front_mut(),
        "Warteschlangeneintrag",
    )?
    .art = Art::Update;
    assert!(d.schritt(mittag()?).is_none());
    let z = d.sperre();
    let lauf = some(z.verlauf.front(), "abgelehnter Update-Lauf")?;
    assert_eq!(lauf.erfolg, Some(false));
    assert!(
        lauf.protokoll
            .iter()
            .any(|z| z.contains("Bitte suche erneut"))
    );
    Ok(())
}

#[test]
fn update_bestaetigung_bindet_den_gefundenen_commit() -> Result<(), String> {
    let d = dienst(ordner()?);
    let t = mittag()?;
    suche_update(&d, t)?;
    let ziel = format!("/bestaetigen?art=update&sha={UPDATE_SHA}");
    assert!(get(&d, "/teil/update")?.contains("Eine neue Version ist verfügbar"));
    assert!(get(&d, &ziel)?.contains("Ich spiele diese Version ein."));
    assert!(get(&d, "/bestaetigen?art=update&sha=alt")?.contains("m=update_suche"));
    assert!(post_auftrag(&d, "update&geprueft=ja&sha=alt")?.contains("m=update_suche"));
    assert!(d.sperre().warteschlange.is_empty());
    assert!(bestaetige_update(&d)?.contains("m=update#update"));
    // Nach dem Einreihen ist das Suchergebnis für den Start entbehrlich.
    d.sperre().verlauf.clear();
    assert_eq!(lauf_ab(&d, t)?, Art::Update);
    let z = d.sperre();
    let lauf = some(z.verlauf.front(), "abgeschlossener Update-Lauf")?;
    assert!(
        lauf.protokoll
            .iter()
            .any(|zeile| zeile.contains(UPDATE_SHA))
    );

    Ok(())
}

#[test]
fn offen_wird_streng_gelesen() {
    let a = "a".repeat(40);
    let b = "b".repeat(40);
    assert_eq!(
        rueckfall::lies(&format!("update-v1\nvorher={a}\nneu={b}\n")),
        Ok(Offen {
            vorher: a.clone(),
            neu: b.clone()
        })
    );
    for falsch in [
        String::new(),
        format!("update-v2\nvorher={a}\nneu={b}\n"),
        format!("update-v1\nneu={b}\nvorher={a}\n"),
        format!("update-v1\nvorher={a}\nneu={a}\n"),
        format!("update-v1\nvorher={}\nneu={b}\n", "A".repeat(40)),
        format!("update-v1\nvorher=HEAD~1\nneu={b}\n"),
        format!("update-v1\nvorher={a}\nneu={b}\nmehr\n"),
    ] {
        assert!(rueckfall::lies(&falsch).is_err(), "{falsch:?}");
    }
}

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

/// Ein Projektordner mit zwei Staenden; `daten/` ist nicht verfolgt.
fn zwei_staende() -> Result<(PathBuf, String, String), String> {
    let w = ordner()?;
    g(&w, &["init", "-q", "-b", "main"])?;
    fs::write(w.join(".gitignore"), "daten/\n").map_err(|e| e.to_string())?;
    fs::write(w.join("a.txt"), "alt\n").map_err(|e| e.to_string())?;
    g(&w, &["add", "."])?;
    g(&w, &["commit", "-q", "-m", "alt"])?;
    let vorher = g(&w, &["rev-parse", "HEAD"])?;
    fs::write(w.join("a.txt"), "neu\n").map_err(|e| e.to_string())?;
    g(&w, &["commit", "-q", "-am", "neu"])?;
    let neu = g(&w, &["rev-parse", "HEAD"])?;
    Ok((w, vorher, neu))
}

#[test]
fn rueckfall_setzt_auf_den_alten_stand_zurueck_und_lehnt_den_neuen_ab() -> Result<(), String> {
    let (w, vorher, neu) = zwei_staende()?;
    // Eine lokale, nicht verfolgte Datei bleibt.
    fs::write(w.join("eigen.txt"), "x").map_err(|e| e.to_string())?;
    assert_eq!(rueckfall::zuruecksetzen(&w)?, None, "nichts offen");
    offen_schreiben(&w, &vorher, &neu)?;
    let m = some(rueckfall::zuruecksetzen(&w)?, "Meldung")?;
    assert!(m.contains("zurueckgesetzt"), "{m}");
    assert_eq!(g(&w, &["rev-parse", "HEAD"])?, vorher);
    assert_eq!(g(&w, &["symbolic-ref", "--short", "HEAD"])?, "main");
    assert_eq!(
        fs::read_to_string(w.join("a.txt")).map_err(|e| e.to_string())?,
        "alt\n"
    );
    assert!(w.join("eigen.txt").exists());
    assert!(!rueckfall::offen_pfad(&w).exists());
    assert_eq!(
        fs::read_to_string(rueckfall::abgelehnt_pfad(&w)).map_err(|e| e.to_string())?,
        format!("{neu}\n")
    );
    // Die Einrichtung nennt den abgelehnten Stand.
    let d = dienst(w.clone());
    assert!(get(&d, "/einrichtung")?.contains(rueckfall::kurz(&neu)));
    Ok(())
}

#[test]
fn rueckfall_faesst_fremde_staende_nicht_an() -> Result<(), String> {
    let (w, vorher, neu) = zwei_staende()?;
    // Projektordner steht weder vor noch nach dem Update: ein dritter Stand.
    fs::write(w.join("a.txt"), "dritter\n").map_err(|e| e.to_string())?;
    g(&w, &["commit", "-q", "-am", "dritter"])?;
    let dritter = g(&w, &["rev-parse", "HEAD"])?;
    offen_schreiben(&w, &vorher, &neu)?;
    assert!(rueckfall::zuruecksetzen(&w).is_err());
    assert!(rueckfall::bestaetige(&w).is_err());
    assert_eq!(g(&w, &["rev-parse", "HEAD"])?, dritter);
    assert!(rueckfall::offen_pfad(&w).exists());
    assert!(!rueckfall::abgelehnt_pfad(&w).exists());
    // Vorruecken kam nie zustande: nur die Markierung geht.
    offen_schreiben(&w, &dritter, &neu)?;
    assert!(some(rueckfall::zuruecksetzen(&w)?, "Meldung")?.contains("nicht eingespielt"));
    assert_eq!(g(&w, &["rev-parse", "HEAD"])?, dritter);
    assert!(!rueckfall::abgelehnt_pfad(&w).exists());
    // Unlesbare Markierung: nichts anfassen.
    fs::write(rueckfall::offen_pfad(&w), "kaputt").map_err(|e| e.to_string())?;
    assert!(rueckfall::zuruecksetzen(&w).is_err());
    assert_eq!(g(&w, &["rev-parse", "HEAD"])?, dritter);
    Ok(())
}

#[test]
fn bestaetigen_nur_auf_dem_neuen_stand() -> Result<(), String> {
    let (w, vorher, neu) = zwei_staende()?;
    offen_schreiben(&w, &vorher, &neu)?;
    assert_eq!(
        rueckfall::bestaetige(&w)?,
        Some(Offen {
            vorher: vorher.clone(),
            neu: neu.clone()
        })
    );
    assert!(!rueckfall::offen_pfad(&w).exists());
    assert_eq!(rueckfall::bestaetige(&w)?, None);
    assert_eq!(g(&w, &["rev-parse", "HEAD"])?, neu);
    Ok(())
}

#[test]
fn bestaetigen_lehnt_unlesbares_journal_ab_und_erlaubt_code_rueckfall() -> Result<(), String> {
    let (w, vorher, neu) = zwei_staende()?;
    offen_schreiben(&w, &vorher, &neu)?;
    let journal = w.join("daten").join("journal.txt");
    fs::write(&journal, b"kaputt").map_err(|e| e.to_string())?;
    assert!(
        rueckfall::bestaetige_mit_wiederholung(
            &w,
            std::time::Duration::from_secs(1),
            std::time::Duration::from_secs(1),
        )
        .is_err_and(|e| e.contains("Papierdaten"))
    );
    assert!(rueckfall::offen_pfad(&w).exists());
    assert!(rueckfall::zuruecksetzen(&w)?.is_some());
    assert_eq!(g(&w, &["rev-parse", "HEAD"])?, vorher);
    assert_eq!(fs::read(journal).map_err(|e| e.to_string())?, b"kaputt");
    Ok(())
}

#[test]
fn bestaetigung_wiederholt_eine_voruebergehende_journalsperre() -> Result<(), String> {
    use std::{fs::OpenOptions, thread, time::Duration};

    let (w, vorher, neu) = zwei_staende()?;
    offen_schreiben(&w, &vorher, &neu)?;
    let daten = w.join("daten");
    for (name, inhalt) in [
        (
            "journal.txt",
            include_bytes!("../../../docs/pruefberichte/papierfreigabe/daten/journal.txt")
                .as_slice(),
        ),
        (
            "journal.vermerk",
            include_bytes!("../../../docs/pruefberichte/papierfreigabe/daten/journal.vermerk")
                .as_slice(),
        ),
        (
            "papierkonto.txt",
            include_bytes!("../../../docs/pruefberichte/papierfreigabe/daten/papierkonto.txt")
                .as_slice(),
        ),
    ] {
        fs::write(daten.join(name), inhalt).map_err(|e| e.to_string())?;
    }
    let sperre = OpenOptions::new()
        .read(true)
        .write(true)
        .open(daten.join("journal.txt"))
        .map_err(|e| e.to_string())?;
    sperre.try_lock().map_err(|e| e.to_string())?;
    assert!(
        rueckfall::bestaetige_mit_wiederholung(&w, Duration::ZERO, Duration::ZERO)
            .is_err_and(|e| e.contains("durchgehend belegt"))
    );
    assert!(rueckfall::offen_pfad(&w).exists());
    let freigabe = thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        sperre.unlock()
    });
    let ergebnis = rueckfall::bestaetige_mit_wiederholung(
        &w,
        Duration::from_secs(3),
        Duration::from_millis(20),
    );
    freigabe
        .join()
        .map_err(|_| "Journalsperre freigeben")?
        .map_err(|e| e.to_string())?;
    assert_eq!(ergebnis?, Some(Offen { vorher, neu }));
    assert!(!rueckfall::offen_pfad(&w).exists());
    Ok(())
}

// --- Waechter: Probezeit mit Frist (Maus, Befund zu #30) ---

/// Ein Kindprozess aus einem kleinen Skript; `{offen}` steht fuer den Pfad der Markierung.
fn kind(
    skript_unix: &str,
    skript_windows: &str,
    offen: &Path,
) -> Result<std::process::Child, String> {
    let p = offen.display().to_string();
    #[cfg(windows)]
    let mut c = {
        let _ = skript_unix;
        let mut c = Command::new("cmd");
        c.args(["/C", &skript_windows.replace("{offen}", &p)]);
        c
    };
    #[cfg(not(windows))]
    let mut c = {
        let _ = skript_windows;
        let mut c = Command::new("sh");
        c.args(["-c", &skript_unix.replace("{offen}", &p)]);
        c
    };
    c.spawn().map_err(|e| format!("Vorbedingung Kind: {e}"))
}

fn markierung() -> Result<PathBuf, String> {
    let w = ordner()?;
    let p = rueckfall::offen_pfad(&w);
    offen_schreiben(&w, &"a".repeat(40), &"b".repeat(40))?;
    Ok(p)
}

const KURZ: std::time::Duration = std::time::Duration::from_millis(50);

#[test]
fn ende_vor_der_bestaetigung_faellt_zurueck_auch_mit_code_0() -> Result<(), String> {
    let offen = markierung()?;
    for (u, w) in [
        ("exit 0", "exit /b 0"),
        ("exit 1", "exit /b 1"),
        ("exit 3", "exit /b 3"),
    ] {
        let mut k = kind(u, w, &offen)?;
        let a = waechter::begleite(&mut k, &offen, std::time::Duration::from_secs(30), KURZ);
        assert!(matches!(a, waechter::Ausgang::Rueckfall(_)), "{u}: {a:?}");
    }
    Ok(())
}

#[test]
fn haengender_neuer_stand_wird_nach_der_frist_beendet() -> Result<(), String> {
    let offen = markierung()?;
    let start = std::time::Instant::now();
    let mut k = kind("sleep 30", "ping -n 30 127.0.0.1 >nul", &offen)?;
    let a = waechter::begleite(&mut k, &offen, std::time::Duration::from_secs(1), KURZ);
    assert!(matches!(a, waechter::Ausgang::Rueckfall(_)), "{a:?}");
    assert!(start.elapsed() < std::time::Duration::from_secs(20));
    assert!(
        k.try_wait().map_err(|e| e.to_string())?.is_some(),
        "Dienst beendet"
    );
    Ok(())
}

#[test]
fn nach_der_bestaetigung_zaehlt_der_code() -> Result<(), String> {
    let offen = markierung()?;
    let mut k = kind(
        "rm -f '{offen}'; sleep 0.3; exit 3",
        "del /q \"{offen}\" & ping -n 2 127.0.0.1 >nul & exit /b 3",
        &offen,
    )?;
    let a = waechter::begleite(&mut k, &offen, std::time::Duration::from_secs(30), KURZ);
    assert_eq!(a, waechter::Ausgang::Ende(Some(3)));
    // Ohne Markierung keine Probezeit: Code 0 ist ein gewoehnliches Ende.
    let mut k = kind("exit 0", "exit /b 0", &offen)?;
    let a = waechter::begleite(&mut k, &offen, std::time::Duration::from_secs(30), KURZ);
    assert_eq!(a, waechter::Ausgang::Ende(Some(0)));
    Ok(())
}

#[test]
fn programmdatei_aus_der_bauausgabe() {
    let lib = r#"{"reason":"compiler-artifact","target":{"kind":["lib"],"name":"depot_dienst"},"executable":null}"#;
    let bin = r#"{"reason":"compiler-artifact","target":{"kind":["bin"],"crate_types":["bin"],"name":"depot_dienst"},"filenames":["x"],"executable":"C:\\Users\\W \"M\"\\target\\debug\\depot_dienst.exe","fresh":true}"#;
    let anderes = r#"{"reason":"compiler-artifact","target":{"kind":["bin"],"name":"depot_waechter"},"executable":"/t/depot_waechter"}"#;
    assert_eq!(
        waechter::programm_aus_bau(&format!(
            "{lib}\n{anderes}\n{bin}\n{{\"reason\":\"build-finished\",\"success\":true}}"
        )),
        Some(PathBuf::from(
            "C:\\Users\\W \"M\"\\target\\debug\\depot_dienst.exe"
        ))
    );
    assert_eq!(
        waechter::programm_aus_bau(&format!("{lib}\n{anderes}")),
        None
    );
}
