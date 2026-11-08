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
fn betriebsleiste_zeigt_auftrag_wartehinweis_und_abschluss_auf_beiden_seiten() -> Result<(), String>
{
    let d = dienst(ordner()?);
    let t = mittag()?;
    d.einreihen(Art::Nachrichten, Anlass::Hand)?;
    d.einreihen(Art::Termine, Anlass::Hand)?;
    let (nr, kind) = some(d.schritt(t), "Auftrag gestartet")?;
    some(d.sperre().laufend.as_mut(), "laufender Auftrag")?.hinweis =
        Some("Verbindung weg. Nächster Versuch <17:45>.".into());
    for pfad in ["/", "/einrichtung", "/teil/kopf"] {
        let html = get(&d, pfad)?;
        assert!(
            html.contains("Jetzt: Schlagzeilen abrufen"),
            "{pfad}: {html}"
        );
        assert!(html.contains("aria-label=\"Auftrag läuft\""));
        assert!(html.contains("Verbindung weg. Nächster Versuch &lt;17:45&gt;."));
        assert!(!html.contains("<17:45>"));
        assert!(html.contains("Danach warten 1 Aufträge."));
        assert!(html.contains("href=\"/#taetigkeit\""));
        if pfad != "/teil/kopf" {
            assert!(html.contains("id=\"betriebsleiste\" data-teil=\"/teil/kopf\""));
        }
    }
    assert!(get(&d, "/teil/live")?.contains("id=\"taetigkeit\" class=\"aktiver-bereich\""));
    d.verfolge(nr, kind, || mittag().ok());
    let html = get(&d, "/teil/kopf")?;
    assert!(!html.contains("<progress"));
    assert!(html.contains("Zuletzt: Schlagzeilen abrufen · erfolgreich"));
    assert!(html.contains("Im Moment läuft kein Auftrag."));
    Ok(())
}

#[test]
fn gefundenes_update_ist_global_sichtbar_und_braucht_weiter_bestaetigung() -> Result<(), String> {
    let d = dienst(ordner()?);
    assert!(!get(&d, "/teil/kopf")?.contains("Update verfügbar"));
    suche_update(&d, mittag()?)?;
    for pfad in ["/", "/einrichtung", "/teil/kopf"] {
        let html = get(&d, pfad)?;
        assert!(html.contains("<strong>Update verfügbar</strong>"), "{pfad}");
        assert!(
            html.contains("class=\"aktion primaer\" href=\"/einrichtung#update\">Update ansehen")
        );
        assert!(html.contains("Einspielen erst nach deiner Bestätigung."));
    }
    assert!(d.sperre().warteschlange.is_empty());
    let ziel = format!("/bestaetigen?art=update&sha={UPDATE_SHA}");
    let html = get(&d, &ziel)?;
    assert!(html.contains("name=\"geprueft\" value=\"ja\" required"));
    assert!(html.contains(&format!("name=\"sha\" value=\"{UPDATE_SHA}\"")));
    assert!(html.contains("class=\"primaer\" type=\"submit\">Update einspielen"));
    assert!(
        post_auftrag(&d, &format!("update&sha={UPDATE_SHA}"))?.contains("m=update_bestaetigen")
    );
    assert!(bestaetige_update(&d)?.contains("m=update#update"));
    assert!(!get(&d, "/teil/kopf")?.contains("Update verfügbar"));
    Ok(())
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
    assert!(m.contains("zurückgesetzt"), "{m}");
    assert_eq!(g(&w, &["rev-parse", "HEAD"])?, vorher);
    assert_eq!(g(&w, &["symbolic-ref", "--short", "HEAD"])?, "main");
    let inhalt = fs::read_to_string(w.join("a.txt")).map_err(|e| e.to_string())?;
    assert!(matches!(inhalt.as_str(), "alt\n" | "alt\r\n"), "{inhalt:?}");
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

/// Ein Kindprozess fuer einen einfachen Exit oder einen absichtlich haengenden Lauf.
fn kind(skript_unix: &str, skript_windows: &str) -> Result<std::process::Child, String> {
    #[cfg(windows)]
    let mut c = {
        use std::os::windows::process::CommandExt as _;

        let _ = skript_unix;
        let mut c = Command::new("cmd.exe");
        c.args(["/D", "/V:OFF", "/C"]).raw_arg(skript_windows);
        c
    };
    #[cfg(not(windows))]
    let mut c = {
        let _ = skript_windows;
        let mut c = Command::new("sh");
        c.args(["-c", skript_unix]);
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
        let mut k = kind(u, w)?;
        let a = waechter::begleite(&mut k, &offen, std::time::Duration::from_secs(30), KURZ);
        assert!(matches!(a, waechter::Ausgang::Rueckfall(_)), "{u}: {a:?}");
    }
    Ok(())
}

#[test]
fn haengender_neuer_stand_wird_nach_der_frist_beendet() -> Result<(), String> {
    let offen = markierung()?;
    let start = std::time::Instant::now();
    let mut k = kind("sleep 30", "ping -n 30 127.0.0.1 >nul")?;
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
    const KIND_MARKIERUNG: &str = "DEPOTSCHLOPF_TEST_WAECHTER_BESTAETIGUNG";
    if let Some(offen) = std::env::var_os(KIND_MARKIERUNG) {
        fs::remove_file(offen).map_err(|e| format!("Bestätigung im Kindprozess: {e}"))?;
        std::process::exit(3);
    }

    let offen = markierung()?;
    // Derselbe Test im Kindprozess bestätigt per Rust und beendet sich mit genau Code 3.
    let mut k = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .args(["--exact", "nach_der_bestaetigung_zaehlt_der_code"])
        .env(KIND_MARKIERUNG, &offen)
        .spawn()
        .map_err(|e| format!("Vorbedingung Kind: {e}"))?;
    let a = waechter::begleite(&mut k, &offen, std::time::Duration::from_secs(30), KURZ);
    assert_eq!(a, waechter::Ausgang::Ende(Some(3)));
    assert!(!offen.exists());
    // Ohne Markierung keine Probezeit: Code 0 ist ein gewoehnliches Ende.
    let mut k = kind("exit 0", "exit /b 0")?;
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

#[test]
fn update_abschluss_bleibt_nach_bestaetigung_und_rueckfall_sichtbar() -> Result<(), String> {
    use depot_dienst::aktualisierung::{self, Phase};
    for bestaetigen in [true, false] {
        let (w, vorher, neu) = zwei_staende()?;
        let d = dienst(w.clone());
        let t = mittag()?;
        suche_update(&d, t)?;
        bestaetige_update(&d)?;
        let (nr, kind) = some(d.schritt(t), "Update startet")?;
        offen_schreiben(&w, &vorher, &neu)?;
        d.verfolge(nr, kind, || mittag().ok());
        let pending = get(&d, "/teil/live")?;
        assert!(pending.contains("Neustart noch nicht bestätigt"));
        let kennung = aktualisierung::laden(&w)?
            .last()
            .ok_or("Beleg fehlt")?
            .lauf
            .clone();
        let neu_gestartet = dienst(w.clone());
        assert!(get(&neu_gestartet, "/teil/live")?.contains("Neustart noch nicht bestätigt"));
        assert!(get(&neu_gestartet, "/teil/verbindung")?.contains("data-bereit=\"nein\""));
        if bestaetigen {
            rueckfall::bestaetige(&w)?;
        } else {
            rueckfall::zuruecksetzen_mit_grund(&w, "Erfundener Startfehler <i>")?;
        }
        let belege = aktualisierung::laden(&w)?;
        let e = belege.last().ok_or("Abschluss fehlt")?;
        assert_eq!(e.lauf, kennung);
        assert_eq!(
            e.phase,
            if bestaetigen {
                Phase::Bestaetigt
            } else {
                Phase::Rueckfall
            }
        );
        let alt_lesbar = depot_dienst::verlauf::lies(
            &fs::read_to_string(depot_dienst::verlauf::pfad(&w)).map_err(|e| e.to_string())?,
        );
        assert_eq!(
            alt_lesbar
                .auftraege
                .iter()
                .find(|a| a.kennung == kennung)
                .and_then(|a| a.erfolg),
            Some(bestaetigen)
        );
        let fertig = dienst(w.clone());
        let live = get(&fertig, "/teil/live")?;
        let update = get(&fertig, "/teil/update")?;
        assert!(get(&fertig, "/teil/verbindung")?.contains("data-bereit=\"ja\""));
        if bestaetigen {
            assert!(live.contains("Update bestätigt") && update.contains("bestätigt"));
            assert_eq!(g(&w, &["rev-parse", "HEAD"])?, neu);
        } else {
            assert!(
                live.contains("zurückgesetzt")
                    && update.contains("Erfundener Startfehler &lt;i&gt;")
            );
            assert_eq!(g(&w, &["rev-parse", "HEAD"])?, vorher);
        }
    }
    Ok(())
}

#[test]
fn abschlussschreibfehler_bestaetigt_nichts_und_fremde_belege_werden_nicht_gruen()
-> Result<(), String> {
    use depot_dienst::aktualisierung;
    let (w, vorher, neu) = zwei_staende()?;
    offen_schreiben(&w, &vorher, &neu)?;
    let p = aktualisierung::pfad(&w);
    fs::create_dir(&p).map_err(|e| e.to_string())?;
    assert!(rueckfall::bestaetige(&w).is_err());
    assert!(rueckfall::offen_pfad(&w).exists());
    assert!(get(&dienst(w.clone()), "/teil/update")?.contains("halt"));
    fs::remove_dir(&p).map_err(|e| e.to_string())?;
    for text in ["kaputt", "{\"typ\":\"update-ergebnis-v1\"}"] {
        fs::write(&p, text).map_err(|e| e.to_string())?;
        assert!(aktualisierung::laden(&w).is_err());
        assert!(rueckfall::bestaetige(&w).is_err());
        assert!(rueckfall::offen_pfad(&w).exists());
    }
    assert!(rueckfall::zuruecksetzen_mit_grund(&w, "Beleg beschädigt")?.is_some());
    assert_eq!(g(&w, &["rev-parse", "HEAD"])?, vorher);
    assert!(
        !rueckfall::offen_pfad(&w).exists(),
        "Diagnosefehler blockiert Rückfall nicht"
    );
    Ok(())
}

#[test]
fn verbindungspruefung_ist_auf_bestaetigung_und_protokoll_mit_startkennung_moeglich()
-> Result<(), String> {
    let d = dienst(ordner()?);
    suche_update(&d, mittag()?)?;
    let s = get(&d, &format!("/bestaetigen?art=update&sha={UPDATE_SHA}"))?;
    assert!(s.contains(&format!("data-start=\"{}\"", d.start)));
    assert!(s.contains("Cache-Control: no-store"));
    let s = get(&d, "/teil/verbindung")?;
    assert!(s.contains(&format!("data-start=\"{}\"", d.start)));
    assert!(s.contains("Cache-Control: no-store"));
    let a = d
        .sperre()
        .verlauf
        .front()
        .cloned()
        .ok_or("Suchlauf fehlt")?;
    let s = get(&d, &format!("/protokoll?lauf={}", a.kennung))?;
    assert!(s.contains(&format!("data-start=\"{}\"", d.start)));
    Ok(())
}

#[test]
fn waechter_baut_stets_ausserhalb_des_urspruenglichen_bauordners() -> Result<(), String> {
    let (w, vorher, _) = zwei_staende()?;
    let neu = waechter::bau_ordner(&w);
    g(&w, &["reset", "--hard", &vorher])?;
    let alt = waechter::bau_ordner(&w);
    assert_eq!(
        alt, neu,
        "Baucache bleibt über Programmstände wiederverwendbar"
    );
    assert!(neu.starts_with(w.join("target")));
    assert_ne!(neu.join("debug"), w.join("target/debug"));
    let c = waechter::baubefehl(&w);
    assert_eq!(
        c.get_envs()
            .find(|(k, _)| *k == "CARGO_TARGET_DIR")
            .and_then(|(_, v)| v),
        Some(alt.as_os_str())
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_laufender_waechter_stub() {
    if std::env::var_os("DEPOT_WAECHTER_STUB").is_some() {
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}

/// Ein echtes laufendes Windows-EXE belegt target/debug, während der Produktionsbefehl
/// dieselben vier Programmziele in einem kleinen, paketfreien Testprojekt neu baut.
#[cfg(windows)]
#[test]
fn windows_bau_beruehrt_den_laufenden_waechter_nicht() -> Result<(), String> {
    let w = ordner()?;
    fs::write(w.join("Cargo.toml"), "[workspace]\nmembers = [\"depot_dienst\",\"depot_sammler\",\"depot_berater\"]\nresolver = \"2\"\n").map_err(|e| e.to_string())?;
    for name in ["depot_dienst", "depot_sammler", "depot_berater"] {
        let p = w.join(name);
        fs::create_dir_all(p.join("src")).map_err(|e| e.to_string())?;
        fs::write(
            p.join("Cargo.toml"),
            format!("[package]\nname=\"{name}\"\nversion=\"0.1.0\"\nedition=\"2021\"\n"),
        )
        .map_err(|e| e.to_string())?;
        fs::write(p.join("src/main.rs"), "fn main() {}\n").map_err(|e| e.to_string())?;
        if name == "depot_dienst" {
            fs::create_dir_all(p.join("src/bin")).map_err(|e| e.to_string())?;
            fs::write(p.join("src/bin/depot_waechter.rs"), "fn main() {}\n")
                .map_err(|e| e.to_string())?;
        }
    }
    let alt = w.join("target/debug/depot_waechter.exe");
    fs::create_dir_all(w.join("target/debug")).map_err(|e| e.to_string())?;
    fs::copy(std::env::current_exe().map_err(|e| e.to_string())?, &alt)
        .map_err(|e| e.to_string())?;
    let bytes = fs::read(&alt).map_err(|e| e.to_string())?;
    let mut kind = Command::new(&alt)
        .args(["--exact", "windows_laufender_waechter_stub"])
        .env("DEPOT_WAECHTER_STUB", "ja")
        .stdout(std::process::Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let aus = waechter::baubefehl(&w).output();
    let lief = kind.try_wait().map_err(|e| e.to_string())?.is_none();
    let _ = kind.kill();
    let _ = kind.wait();
    let aus = aus.map_err(|e| e.to_string())?;
    assert!(lief);
    assert!(
        aus.status.success(),
        "{}",
        String::from_utf8_lossy(&aus.stderr)
    );
    assert_eq!(fs::read(&alt).map_err(|e| e.to_string())?, bytes);
    assert!(waechter::programm_aus_bau(&String::from_utf8_lossy(&aus.stdout)).is_some());
    for name in [
        "depot_dienst",
        "depot_waechter",
        "depot_sammler",
        "depot_berater",
    ] {
        assert!(
            waechter::bau_ordner(&w)
                .join("debug")
                .join(format!("{name}.exe"))
                .is_file()
        );
    }
    Ok(())
}

/// Das Gate führt Verhaltenstests des tatsächlich ausgelieferten Skripts aus. GitHub stellt
/// Node bereit; ein reiner Rust-Rechner braucht es nicht zum Betrieb oder Update.
#[test]
fn browser_verhalten_bei_update_und_neustart() -> Result<(), String> {
    let test = ordner()?.join("browser.cjs");
    let code = format!(
        "const script = {};\n{}",
        depot_kern::json::zitiere(depot_dienst::seiten::APP_JS),
        BROWSER_TESTS
    );
    fs::write(&test, code).map_err(|e| e.to_string())?;
    match Command::new("node").arg("--test").arg(test).output() {
        Ok(a) => {
            assert!(
                a.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&a.stdout),
                String::from_utf8_lossy(&a.stderr)
            );
            println!("{}", String::from_utf8_lossy(&a.stdout));
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && std::env::var_os("CI").is_none() => {
            eprintln!(
                "Browser-Verhalten nicht geprüft: Node fehlt (nur Entwicklung/CI, keine Voraussetzung zum Betrieb)."
            );
            Ok(())
        }
        Err(e) => Err(format!("Browser-Verhalten in CI nicht prüfbar: {e}")),
    }
}

const BROWSER_TESTS: &str = r#"// Verhaltensprüfungen des ausgelieferten Skripts, ohne Browser-/Paketabhängigkeit.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const vm=require('node:vm');
const reply=t=>Promise.resolve({ok:true,text:()=>Promise.resolve(t)});
const health=(start='alt',bereit=true)=>`<span data-start="${start}"></span><span data-bereit="${bereit?'ja':'nein'}"></span>`;
function browser({parts=0,response=()=>reply(health()),href='http://127.0.0.1:47410/einrichtung?m=update#update',known='alt',urls=['/teil/status','/teil/update','/teil/journal'],forms=[],confirm=()=>false}={}) {
  let now=0, nr=0, timers=new Map(), requests=[], navigations=[], events={},docEvents={};
  const banner={hidden:true,textContent:''}, reconnect={hidden:true};
  const elements=Array.from({length:parts},(_,i)=>({innerHTML:'alt',getAttribute:()=>urls[i],querySelectorAll:()=>[]}));
  const document={addEventListener:(n,f)=>docEvents[n]=f,getElementById:id=>id==='neuverbinden'?reconnect:banner,querySelector:()=>known?{getAttribute:()=>known}:null,querySelectorAll:s=>s==='[data-teil]'?elements:s==='form[data-aenderungen]'?forms:[]};
  vm.runInNewContext(script,{document,navigator:{},URL,AbortController,Set,
    window:{confirm,addEventListener:(n,f)=>events[n]=f,location:{href,replace:u=>navigations.push(u)}},
    setTimeout:(f,delay)=>{const id=++nr;timers.set(id,{f,at:now+delay});return id},clearTimeout:id=>timers.delete(id),
    fetch:(url,options)=>{requests.push({url,options});return response(url,options)}
  });
  return {banner,reconnect,elements,requests,navigations,events,docEvents,get timers(){return timers},
    async advance(ms){const end=now+ms;for(let i=0;i<200;i++){const due=[...timers.entries()].filter(([,t])=>t.at<=end).sort((a,b)=>a[1].at-b[1].at)[0];if(!due)break;timers.delete(due[0]);now=due[1].at;due[1].f();await new Promise(r=>setImmediate(r));}now=end;}
  };
}
function formular(input={type:'time',value:'17:45',defaultValue:'17:45'}) {
  const status={textContent:''},classes=new Set(),button={dataset:{},textContent:'Speichern',disabled:false};
  return {dataset:{},input,status,button,classes,
    querySelectorAll:()=>[input],querySelector:s=>s==='[data-speicherstand]'?status:button,
    classList:{toggle:(c,an)=>an?classes.add(c):classes.delete(c)}};
}
function ereignis(target,submitter){return {target,submitter,preventDefault(){this.verhindert=true;}};}
test('Ändern und Zurücksetzen zeigen genau den Speicherbedarf; Verlassen warnt',()=>{
  const f=formular(),b=browser({forms:[f]});
  assert.ok(!f.classes.has('ungespeichert'));
  f.input.value='18:05';b.docEvents.input({target:{closest:()=>f}});
  assert.match(f.status.textContent,/Noch nicht gespeichert/);
  assert.ok(f.classes.has('ungespeichert'));
  const weg=ereignis();b.events.beforeunload(weg);assert.equal(weg.verhindert,true);
  f.input.value='17:45';b.docEvents.change({target:{closest:()=>f}});
  assert.ok(!f.classes.has('ungespeichert'));
  const bleibt=ereignis();b.events.beforeunload(bleibt);assert.ok(!bleibt.verhindert);
});
test('Gespeicherter Fehlerentwurf und ungültige Felder bleiben als unerledigt sichtbar',()=>{
  const f=formular();f.dataset.entwurf='ja';const b=browser({forms:[f]});
  assert.match(f.status.textContent,/Noch nicht gespeichert/);
  b.docEvents.invalid({target:{closest:()=>f}});
  assert.match(f.status.textContent,/markierte Feld/);
});
test('Eigenes Speichern nutzt den angeklickten Knopf; fremde Eingaben brauchen Entscheidung',async()=>{
  const f=formular(),g=formular();g.input.value='19:00';
  let zustimmen=false;const b=browser({forms:[f,g],confirm:()=>zustimmen});
  const abbruch=ereignis(f,f.button);b.docEvents.submit(abbruch);
  assert.equal(abbruch.verhindert,true);assert.ok(!f.button.dataset.gesendet);
  zustimmen=true;
  const anderer={dataset:{},textContent:'Speichern und einschalten',disabled:false};
  const ok=ereignis(f,anderer);b.docEvents.submit(ok);await b.advance(0);
  assert.ok(!ok.verhindert);assert.equal(anderer.disabled,true);assert.equal(f.button.disabled,false);
  const verlassen=ereignis();b.events.beforeunload(verlassen);assert.ok(!verlassen.verhindert);
});
test('Neustart verwirft ungespeicherte Zugangsdaten nicht automatisch',async()=>{
  const f=formular({type:'password',value:'nur-im-formular',defaultValue:''});
  const b=browser({forms:[f],response:()=>reply(health('neu'))});await b.advance(0);
  assert.deepEqual(b.navigations,[]);assert.equal(b.reconnect.hidden,false);
  assert.match(b.banner.textContent,/ungespeicherten Eingaben/);
  const speichern=ereignis(f,f.button);b.docEvents.submit(speichern);
  assert.equal(speichern.verhindert,true);
  assert.equal(f.input.value,'nur-im-formular');
  const weg=ereignis();b.events.beforeunload(weg);assert.equal(weg.verhindert,true);
  f.input.value='';await b.advance(2000);assert.deepEqual(b.navigations,['/einrichtung#update']);
});
test('auch Protokoll und Bestätigung ohne Live-Bereich prüfen die Verbindung',async()=>{
  for(const href of ['http://127.0.0.1:47410/protokoll?lauf=123-4#ende','http://127.0.0.1:47410/bestaetigen?art=update&sha=abc']){
    const b=browser({href,response:()=>reply(health('neu'))});await b.advance(0);
    assert.equal(b.requests[0].url,'/teil/status');
    const u=new URL(href);assert.deepEqual(b.navigations,[u.pathname+u.search+u.hash]);
  }
});
test('nur ein Neuladen, Anker und fachliche Parameter bleiben, flüchtige Meldung entfällt',async()=>{
  const b=browser({parts:3,response:u=>reply(u==='/teil/status'?health('neu'):health('neu'))});
  await b.advance(12000);assert.deepEqual(b.navigations,['/einrichtung#update']);
});
test('neuer Dienst in Probe wird erst nach Bestätigung geladen',async()=>{
  let bereit=false;const b=browser({parts:3,response:u=>reply(u==='/teil/status'?health('neu',bereit):health('neu'))});
  await b.advance(4000);assert.equal(b.navigations.length,0);assert.equal(b.banner.hidden,false);
  assert.ok(b.elements.every(e=>e.innerHTML==='alt'));
  bereit=true;await b.advance(2000);assert.deepEqual(b.navigations,['/einrichtung#update']);
});
test('abgewiesene Verbindung erholt sich und erkennt auch den Rückfall',async()=>{
  let offline=true;const b=browser({response:()=>offline?Promise.reject(Error('offline')):reply(health('rueckfall'))});
  await b.advance(0);assert.equal(b.banner.hidden,false);assert.equal(b.navigations.length,0);
  offline=false;await b.advance(5000);assert.deepEqual(b.navigations,['/einrichtung#update']);
});
test('auch hängender Abruf und hängender Antwortkörper werden abgebrochen und erneut versucht',async()=>{
  for(const body of [false,true]){
    let haengt=true;
    const b=browser({response:(_,o)=>{
      if(!haengt)return reply(health());
      const pending=new Promise((_,reject)=>o.signal.addEventListener('abort',()=>reject(Error('aborted'))));
      return body?Promise.resolve({ok:true,text:()=>pending}):pending;
    }});
    await b.advance(5000);assert.equal(b.banner.hidden,false);assert.equal(b.requests[0].options.signal.aborted,true);
    haengt=false;await b.advance(5000);assert.equal(b.banner.hidden,true);assert.ok(b.requests.length>=2);
  }
});
test('Zurück aus dem Seitenspeicher startet genau einen neuen Prüfzyklus',async()=>{
  const b=browser({parts:1});await b.advance(0);b.events.pagehide();
  const vorher=b.requests.length;await b.advance(20000);assert.equal(b.requests.length,vorher);
  b.events.pageshow({persisted:true});await b.advance(2000);
  assert.equal(b.requests.length,vorher+3); // Gesundheit sofort und nach 2s, Teil nach 2s.
});
test('Teilaktualisierung läuft weiter und alle Abrufe umgehen den Cache',async()=>{
  const b=browser({parts:2,response:u=>reply(u==='/teil/status'?health():'aktuell')});
  const detail={id:'log',open:true};b.elements[0].querySelectorAll=()=>[detail];
  await b.advance(2000);assert.equal(b.elements[1].innerHTML,'aktuell');
  assert.ok(b.requests.every(r=>r.options.cache==='no-store'&&r.options.credentials==='same-origin'&&r.options.signal));
});

test('Betriebsleiste wechselt von Lauf zu Update-Hinweis ohne Seitenneuladung',async()=>{
  let inhalt=health()+'Jetzt: Kurse abrufen';
  const b=browser({parts:1,urls:['/teil/kopf'],response:u=>reply(u==='/teil/kopf'?inhalt:health())});
  await b.advance(2000);assert.match(b.elements[0].innerHTML,/Jetzt: Kurse abrufen/);
  inhalt=health()+'Update verfügbar';
  await b.advance(2000);assert.match(b.elements[0].innerHTML,/Update verfügbar/);
  assert.deepEqual(b.navigations,[]);
  assert.ok(b.requests.some(r=>r.url==='/teil/kopf'));
});

test('Rückfall auf älteren Dienst ohne Bereitschaftsmarker wird erkannt',async()=>{
  const b=browser({response:()=>reply('<span data-start="alter-rueckfall"></span>')});
  await b.advance(0);assert.deepEqual(b.navigations,['/einrichtung#update']);
});
"#;

/// Die Probe startet in einem eigenen Prozess mit privaten temporären Dateien. Auch
/// bei einem echten laufenden Wächter darf sie dessen Sperre und Port nicht übernehmen.
fn doppelstart_kind(temp: &Path) -> Result<Command, String> {
    let mut kind = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    kind.args([
        "--exact",
        "gegenprobe_doppelstart_bei_portluecke_beruehrt_update_nicht",
        "--nocapture",
    ])
    .env("TEMP", temp)
    .env("TMP", temp)
    .env("TMPDIR", temp)
    .env(
        "GIT_CONFIG_GLOBAL",
        if cfg!(windows) { "NUL" } else { "/dev/null" },
    )
    .env("GIT_CONFIG_NOSYSTEM", "1");
    Ok(kind)
}

#[test]
fn doppelstart_probe_laesst_eine_bestehende_waechtersperre_unveraendert() -> Result<(), String> {
    use std::fs::OpenOptions;
    let temp = ordner()?;
    let pfad = temp.join("depotschlopf-waechter-47410.lock");
    fs::write(&pfad, "bestehender Wächter").map_err(|e| e.to_string())?;
    let sperre = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&pfad)
        .map_err(|e| e.to_string())?;
    sperre.try_lock().map_err(|e| e.to_string())?;
    let aus = doppelstart_kind(&temp)?
        .output()
        .map_err(|e| e.to_string())?;
    assert!(
        aus.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&aus.stdout),
        String::from_utf8_lossy(&aus.stderr)
    );
    let zweite = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&pfad)
        .map_err(|e| e.to_string())?;
    assert!(matches!(
        zweite.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    drop(sperre);
    assert_eq!(
        fs::read_to_string(&pfad).map_err(|e| e.to_string())?,
        "bestehender Wächter"
    );
    Ok(())
}

/// Ein zweiter Start bei freiem Dienst-Port darf während eines Updates weder neu bauen noch
/// den ersten Wächter zurücksetzen. Die Prozesssperre gilt auch für andere Projektkopien.
#[test]
fn gegenprobe_doppelstart_bei_portluecke_beruehrt_update_nicht() -> Result<(), String> {
    use std::fs::OpenOptions;
    if std::env::var_os("DEPOT_DOPPELSTART_KIND").is_none() {
        let temp = ordner()?;
        let aus = doppelstart_kind(&temp)?
            .env("DEPOT_DOPPELSTART_KIND", "ja")
            .output()
            .map_err(|e| e.to_string())?;
        assert!(
            aus.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&aus.stdout),
            String::from_utf8_lossy(&aus.stderr)
        );
        return Ok(());
    }
    let (w, vorher, neu) = zwei_staende()?;
    fs::create_dir_all(w.join("kodex")).map_err(|e| e.to_string())?;
    fs::write(
        w.join("kodex/festschreibung.txt"),
        "Wurzelmarker der Gegenprobe",
    )
    .map_err(|e| e.to_string())?;
    offen_schreiben(&w, &vorher, &neu)?;
    let pfad = std::env::temp_dir().join("depotschlopf-waechter-47410.lock");
    let sperre = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&pfad)
        .map_err(|e| e.to_string())?;
    sperre.try_lock().map_err(|e| e.to_string())?;
    // Die gesperrte zweite Instanz muss vor jedem Portzugriff aussteigen. Ein echter
    // Dienst darf Port 47410 während dieser Probe weiterhin belegen.
    let aus = Command::new(env!("CARGO_BIN_EXE_depot_waechter"))
        .arg("--ohne-browser")
        .current_dir(&w)
        .output()
        .map_err(|e| e.to_string())?;
    assert_eq!(
        g(&w, &["rev-parse", "HEAD"])?,
        neu,
        "zweiter Start hat trotz laufendem Wächter zurückgesetzt: {}",
        String::from_utf8_lossy(&aus.stderr)
    );
    assert!(rueckfall::offen_pfad(&w).exists());
    assert!(!rueckfall::abgelehnt_pfad(&w).exists());
    assert!(
        aus.status.success(),
        "{}",
        String::from_utf8_lossy(&aus.stderr)
    );
    assert!(String::from_utf8_lossy(&aus.stdout).contains("startet bereits"));
    assert!(waechter::prozess_sperre()?.is_none());
    // Andere parallel laufende Tests können gerade einen Kindprozess erzeugen. Ein
    // geerbter Deskriptor hält den Lock bis exec kurz weiter; auf tatsächliche Freigabe warten.
    let uebernehmen = || -> Result<fs::File, String> {
        for _ in 0..100 {
            if let Some(f) = waechter::prozess_sperre()? {
                return Ok(f);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Err("Sperre blieb nach Prozessende belegt".into())
    };
    drop(sperre);
    let wieder = uebernehmen()?;
    assert!(waechter::prozess_sperre()?.is_none());
    drop(wieder);
    drop(uebernehmen()?);
    Ok(())
}

#[test]
fn gegenprobe_lange_abschlussgruende_bleiben_wieder_lesbar() -> Result<(), String> {
    use depot_dienst::aktualisierung::{self, Phase};
    let w = ordner()?;
    let grund = "\u{0001}".repeat(2048);
    for n in 0..25 {
        let o = Offen {
            vorher: "f".repeat(40),
            neu: format!("{n:040x}"),
        };
        aktualisierung::vermerke(&w, &o, Phase::Probe, &grund, None)?;
        let alle = aktualisierung::laden(&w)?;
        assert!(alle.len() <= 20);
        assert_eq!(alle.last().map(|e| e.neu.as_str()), Some(o.neu.as_str()));
        assert!(
            aktualisierung::pfad(&w)
                .metadata()
                .map_err(|e| e.to_string())?
                .len()
                <= 128 * 1024
        );
    }
    Ok(())
}

#[test]
fn gegenprobe_volles_diagnoseprotokoll_blockiert_bestaetigung_nicht() -> Result<(), String> {
    let (w, vorher, neu) = zwei_staende()?;
    let d = dienst(w.clone());
    let t = mittag()?;
    suche_update(&d, t)?;
    bestaetige_update(&d)?;
    let (nr, kind) = some(d.schritt(t), "Update startet")?;
    offen_schreiben(&w, &vorher, &neu)?;
    d.verfolge(nr, kind, || mittag().ok());
    let kennung = depot_dienst::aktualisierung::laden(&w)?
        .last()
        .ok_or("Beleg fehlt")?
        .lauf
        .clone();
    let p = w.join("daten/protokolle").join(format!("{kennung}.log"));
    let log = fs::OpenOptions::new()
        .write(true)
        .open(p)
        .map_err(|e| e.to_string())?;
    log.set_len(depot_dienst::protokoll::DATEI_MAX)
        .map_err(|e| e.to_string())?;
    drop(log);
    assert!(
        rueckfall::bestaetige(&w)?.is_some(),
        "volles Diagnoseprotokoll ist kein Fehler der neuen Version oder der Papierdaten"
    );
    assert!(!rueckfall::offen_pfad(&w).exists());
    assert_eq!(
        depot_dienst::aktualisierung::laden(&w)?
            .last()
            .map(|e| e.phase),
        Some(depot_dienst::aktualisierung::Phase::Bestaetigt)
    );
    Ok(())
}
