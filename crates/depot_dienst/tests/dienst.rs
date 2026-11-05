//! Der Dienst: Schutz gegen fremde Webseiten, Zeitplan einmal je Tag, Auftraege mit Protokoll,
//! Seiten ohne Schluessel. Jede Vorbereitung prueft ihren Erfolg ausdruecklich.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use depot_dienst::http::{self, Anfrage, LeseFehler};
use depot_dienst::schluessel::Wer;
use depot_dienst::werkbank::{Anlass, Art, Dienst, Frueher, Lagerechner, Starter};
use depot_dienst::zeitplan::{
    Erledigt, Zeitplan, abrufe_fuer, faellig, faellig_fuer, letzter_abruf_fuer, naechster,
    naechster_fuer,
};
use depot_dienst::{Umwelt, behandle};
use depot_dienst::{autostart, kostenschutz, seiten};
use depot_kern::{BeraterId, Boerse, Datum, Zeitpunkt};
use depot_lage::{Hinterlegt, Umgebung, Zugang, lagebild};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);
const TOKEN: &str = "0123456789abcdef0123456789abcdef";
const HOST: &str = "127.0.0.1:47410";
const HERKUNFT: &str = "http://127.0.0.1:47410";

fn some<T>(o: Option<T>, was: &str) -> Result<T, String> {
    o.ok_or_else(|| format!("Vorbedingung {was}"))
}

fn ordner() -> Result<PathBuf, String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("depot_dienst_{}_{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("daten")).map_err(|e| format!("Vorbedingung Ordner: {e}"))?;
    Ok(dir)
}

/// Donnerstag, 05.11.2026, Frankfurter Zeit.
fn donnerstag(h: u8, m: u8) -> Result<Zeitpunkt, String> {
    some(
        Datum::neu(2026, 11, 5).and_then(|d| Zeitpunkt::frankfurt(d, h, m, 0)),
        "Zeitpunkt",
    )
}

/// Ein harmloser Starter: gibt eine Zeile aus und endet mit dem gewuenschten Ergebnis.
fn echo_starter(erfolg: bool) -> Starter {
    echo_starter_je_art(move |_| erfolg)
}

/// Wie `echo_starter`, mit Ergebnis je Art des Auftrags.
fn echo_starter_je_art(erfolg: impl Fn(Art) -> bool + Send + Sync + 'static) -> Starter {
    Box::new(move |art: Art, _wurzel: &Path| {
        let code = if erfolg(art) { "0" } else { "3" };
        let zeile = format!("lauf {}", art.kurz());
        #[cfg(windows)]
        {
            let mut c = Command::new("cmd");
            c.args(["/C", &format!("echo {zeile}& exit /b {code}")]);
            c
        }
        #[cfg(not(windows))]
        {
            let mut c = Command::new("sh");
            c.args(["-c", &format!("echo {zeile}; exit {code}")]);
            c
        }
    })
}

fn leeres_lagebild() -> Lagerechner {
    Box::new(|_wurzel: &Path, jetzt: Zeitpunkt| {
        let l = lagebild(&Umgebung {
            jetzt,
            journal: Zugang::NichtEingerichtet,
            kursquelle: Hinterlegt::Nein,
            berater: [
                (BeraterId::Dachs, Hinterlegt::Nein),
                (BeraterId::Maus, Hinterlegt::Nein),
            ],
            sperren: Vec::new(),
            anbieter: Vec::new(),
            anbieter_unlesbar: 0,
            ki_halt: None,
        });
        (l, "<section>LAGEBILD</section>".to_string())
    })
}

/// Schaltet den Zeitplan ein, wie es der Mensch in der Einrichtung tut (ab Werk ist er aus).
fn eingeschaltet(wurzel: PathBuf) -> Result<PathBuf, String> {
    let p = Zeitplan {
        an: true,
        ..Zeitplan::default()
    };
    fs::write(wurzel.join("daten").join("zeitplan.txt"), p.als_text())
        .map_err(|e| format!("Vorbedingung Zeitplan: {e}"))?;
    Ok(wurzel)
}

fn dienst(wurzel: PathBuf, erfolg: bool) -> Arc<Dienst> {
    Arc::new(Dienst::neu(
        wurzel,
        TOKEN.into(),
        echo_starter(erfolg),
        leeres_lagebild(),
    ))
}

/// Eine Umwelt, die nur mitschreibt.
#[derive(Default)]
struct Attrappe {
    abgelegt: Mutex<Vec<(Wer, String)>>,
    autostart: Mutex<Option<bool>>,
}

impl Umwelt for Attrappe {
    fn schluessel(&self, _wer: Wer) -> Hinterlegt {
        Hinterlegt::Ja
    }

    fn schluessel_ablegen(&self, wer: Wer, text: &str) -> Result<(), String> {
        let s = wer.pruefe(text)?;
        self.abgelegt
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((wer, s));
        Ok(())
    }

    fn autostart(&self, _wurzel: &Path) -> bool {
        false
    }

    fn autostart_setzen(&self, _wurzel: &Path, an: bool) -> Result<(), String> {
        *self
            .autostart
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(an);
        Ok(())
    }
}

fn anfrage(methode: &str, ziel: &str, kopf: &[(&str, &str)], rumpf: &str) -> Anfrage {
    let (pfad, abfrage) = ziel.split_once('?').unwrap_or((ziel, ""));
    Anfrage {
        methode: methode.into(),
        pfad: pfad.into(),
        abfrage: abfrage.into(),
        kopf: kopf
            .iter()
            .map(|(n, w)| ((*n).to_string(), (*w).to_string()))
            .collect(),
        rumpf: rumpf.as_bytes().to_vec(),
    }
}

fn formular_post(ziel: &str, rumpf: &str) -> Anfrage {
    anfrage(
        "POST",
        ziel,
        &[
            ("Host", HOST),
            ("Origin", HERKUNFT),
            ("Content-Type", "application/x-www-form-urlencoded"),
        ],
        rumpf,
    )
}

fn status(antwort: &[u8]) -> Result<u16, String> {
    let t = String::from_utf8_lossy(antwort);
    let code = some(t.split(' ').nth(1), "Statuszeile")?;
    code.parse().map_err(|_| "Statuscode unlesbar".into())
}

fn text(antwort: &[u8]) -> String {
    String::from_utf8_lossy(antwort).into_owned()
}

fn wartend(d: &Dienst) -> Vec<(Art, Anlass)> {
    d.sperre().warteschlange.iter().copied().collect()
}

// --- HTTP lesen ---

#[test]
fn http_liest_genau_eine_anfrage() -> Result<(), String> {
    let roh =
        b"POST /zeitplan?x=1 HTTP/1.1\r\nHost: 127.0.0.1:47410\r\nContent-Length: 3\r\n\r\na=b";
    let a = http::lies(&mut &roh[..]).map_err(|e| format!("{e:?}"))?;
    assert_eq!(a.methode, "POST");
    assert_eq!(a.pfad, "/zeitplan");
    assert_eq!(a.abfrage, "x=1");
    assert_eq!(a.kopf("host"), Some("127.0.0.1:47410"));
    assert_eq!(a.rumpf, b"a=b");
    Ok(())
}

#[test]
fn http_lehnt_unklare_anfragen_ab() {
    let faelle: [&[u8]; 6] = [
        b"GET / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
        b"POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\na",
        b"POST / HTTP/1.1\r\nContent-Length: 1\r\n\r\nab",
        b"GET http://fremd/ HTTP/1.1\r\n\r\n",
        b"GET / HTTP/2\r\n\r\n",
        b"GET / HTTP/1.1\r\nKaputt\r\n\r\n",
    ];
    for f in faelle {
        assert_eq!(http::lies(&mut &f[..]), Err(LeseFehler::Kaputt), "{f:?}");
    }
    let gross = format!(
        "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
        http::RUMPF_MAX + 1
    );
    assert_eq!(http::lies(&mut gross.as_bytes()), Err(LeseFehler::ZuGross));
    let mut riesig = b"GET / HTTP/1.1\r\nX: ".to_vec();
    riesig.extend(std::iter::repeat_n(b'a', http::KOPF_MAX + 10));
    assert_eq!(http::lies(&mut &riesig[..]), Err(LeseFehler::ZuGross));
    assert_eq!(
        http::lies(&mut &b"GET / HTTP/1.1\r\n"[..]),
        Err(LeseFehler::Abgebrochen)
    );
}

#[test]
fn formular_entschluesselt_streng() {
    assert_eq!(
        http::formular(b"a=1+2&b=%C3%A4"),
        Some(vec![("a".into(), "1 2".into()), ("b".into(), "ä".into())])
    );
    assert_eq!(http::formular(b"a=%G1"), None);
    assert_eq!(http::formular(b"a=%FF"), None);
    assert!(http::gleich("abc", "abc"));
    assert!(!http::gleich("abc", "abd"));
    assert!(!http::gleich("abc", "abcd"));
}

// --- Schutz gegen fremde Webseiten ---

#[test]
fn fremder_host_wird_abgelehnt() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let umw = Attrappe::default();
    let t = donnerstag(12, 0)?;
    for host in [
        None,
        Some("boese.example:47410"),
        Some("127.0.0.1:80"),
        Some("127.0.0.1"),
        Some("localhost.boese.example:47410"),
    ] {
        let kopf: Vec<(&str, &str)> = host.map(|h| ("Host", h)).into_iter().collect();
        let a = anfrage("GET", "/", &kopf, "");
        assert_eq!(status(&behandle(&d, &umw, &a, t))?, 403, "{host:?}");
    }
    for host in [HOST, "localhost:47410"] {
        let a = anfrage("GET", "/", &[("Host", host)], "");
        assert_eq!(status(&behandle(&d, &umw, &a, t))?, 200, "{host}");
    }
    Ok(())
}

#[test]
fn post_braucht_herkunft_formular_und_token() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let umw = Attrappe::default();
    let t = donnerstag(12, 0)?;
    let rumpf = format!("t={TOKEN}&art=kurse");
    let ohne_herkunft = anfrage(
        "POST",
        "/ausfuehren",
        &[
            ("Host", HOST),
            ("Content-Type", "application/x-www-form-urlencoded"),
        ],
        &rumpf,
    );
    assert_eq!(status(&behandle(&d, &umw, &ohne_herkunft, t))?, 403);
    for herkunft in ["null", "http://boese.example", "http://127.0.0.1:47411"] {
        let a = anfrage(
            "POST",
            "/ausfuehren",
            &[
                ("Host", HOST),
                ("Origin", herkunft),
                ("Content-Type", "application/x-www-form-urlencoded"),
            ],
            &rumpf,
        );
        assert_eq!(status(&behandle(&d, &umw, &a, t))?, 403, "{herkunft}");
    }
    let json = anfrage(
        "POST",
        "/ausfuehren",
        &[
            ("Host", HOST),
            ("Origin", HERKUNFT),
            ("Content-Type", "text/plain"),
        ],
        &rumpf,
    );
    assert_eq!(status(&behandle(&d, &umw, &json, t))?, 400);
    for falsch in ["", "t=", "t=0123456789abcdef0123456789abcdee&art=kurse"] {
        let a = formular_post("/ausfuehren", falsch);
        assert_eq!(status(&behandle(&d, &umw, &a, t))?, 403, "{falsch}");
    }
    assert!(wartend(&d).is_empty(), "nichts darf eingereiht sein");
    let a = formular_post("/ausfuehren", &rumpf);
    let r = behandle(&d, &umw, &a, t);
    assert_eq!(status(&r)?, 303);
    assert!(text(&r).contains("Location: /?m=auftrag\r\n"));
    assert_eq!(wartend(&d), vec![(Art::Kurse, Anlass::Hand)]);
    // Dieselbe Art nicht doppelt.
    let r = behandle(&d, &umw, &a, t);
    assert!(text(&r).contains("Location: /?m=auftrag_belegt\r\n"));
    assert_eq!(wartend(&d).len(), 1);
    Ok(())
}

#[test]
fn antworten_verbieten_einbetten_und_fremde_skripte() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let umw = Attrappe::default();
    let a = anfrage("GET", "/", &[("Host", HOST)], "");
    let r = text(&behandle(&d, &umw, &a, donnerstag(12, 0)?));
    for kopf in [
        "Content-Security-Policy: default-src 'none'; script-src 'self';",
        "frame-ancestors 'none'",
        "X-Frame-Options: DENY",
        "X-Content-Type-Options: nosniff",
        "Cache-Control: no-store",
        // Gleiche Herkunft: Der Browser sendet bei Formularen die eigene Herkunft, nie "null".
        "Referrer-Policy: same-origin",
    ] {
        assert!(r.contains(kopf), "{kopf}");
    }
    assert!(!r.contains("<script>"), "kein eingebettetes Skript");
    assert!(r.contains("<script src=\"/app.js\"></script>"));
    Ok(())
}

#[test]
fn meldungen_spiegeln_keine_eingabe() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let umw = Attrappe::default();
    let a = anfrage(
        "GET",
        "/?m=%3Cscript%3Ealert(1)%3C/script%3E<b>",
        &[("Host", HOST)],
        "",
    );
    let r = text(&behandle(&d, &umw, &a, donnerstag(12, 0)?));
    assert!(!r.contains("alert(1)"));
    assert!(!r.contains("<b>"));
    let a = anfrage("GET", "/einrichtung?m=zeitplan", &[("Host", HOST)], "");
    let r = text(&behandle(&d, &umw, &a, donnerstag(12, 0)?));
    assert!(r.contains("Zeitplan gespeichert und aktiv."));
    Ok(())
}

// --- Schluessel ---

#[test]
fn schluessel_wird_geprueft_abgelegt_und_nie_gezeigt() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let umw = Attrappe::default();
    let t = donnerstag(12, 0)?;
    let ungueltig = formular_post(
        "/schluessel",
        &format!("t={TOKEN}&wer=alphavantage&wert=kurz"),
    );
    let r = text(&behandle(&d, &umw, &ungueltig, t));
    assert!(r.contains("m=schluessel_ungueltig"));
    let geheim = "Attrappe_Dachs-0123456789_ABCDEFGHIJ";
    let gut = formular_post(
        "/schluessel",
        &format!("t={TOKEN}&wer=anthropic&wert=%20{geheim}%20"),
    );
    let r = text(&behandle(&d, &umw, &gut, t));
    assert!(r.contains("Location: /einrichtung?m=schluessel#schluessel"));
    assert!(!r.contains(geheim));
    assert_eq!(
        *umw.abgelegt.lock().unwrap_or_else(PoisonError::into_inner),
        vec![(Wer::Berater(BeraterId::Dachs), geheim.to_string())]
    );
    let seite = anfrage("GET", "/einrichtung", &[("Host", HOST)], "");
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(!s.contains(geheim), "die Seite zeigt keinen Schluessel");
    assert!(s.contains("hinterlegt"));
    assert!(s.contains("DepotSchlopf/anthropic"));
    assert!(s.contains("type=\"password\""));
    Ok(())
}

#[test]
fn schluesselregeln() {
    assert!(Wer::Kursquelle.pruefe("ABCD1234").is_ok());
    assert!(Wer::Kursquelle.pruefe("abcd1234").is_err());
    assert!(Wer::Kursquelle.pruefe("ABC123").is_err());
    let dachs = Wer::Berater(BeraterId::Dachs);
    assert!(dachs.pruefe("Attrappe_Maus-abcdefghij").is_ok());
    assert!(dachs.pruefe("Attrappe Maus-abcdefghij").is_err());
    assert!(dachs.pruefe("kurz").is_err());
    assert!(dachs.pruefe(&"a".repeat(257)).is_err());
    assert_eq!(Wer::aus_kurz("openai"), Some(Wer::Berater(BeraterId::Maus)));
    assert_eq!(Wer::aus_kurz("fremd"), None);
}

// --- Zeitplan ---

#[test]
fn zeitplan_text_streng() -> Result<(), String> {
    let p = Zeitplan::default();
    assert_eq!(Zeitplan::lies(&p.als_text()), Ok(p));
    for kaputt in [
        "zeitplan-v1\nan=ja\ntermine=17:40\nkurse=17:45\n",
        "zeitplan-v1\nan=ja\ntermine=7:40\nkurse=17:45\npapier_danach=ja\n",
        "zeitplan-v1\nan=ja\ntermine=24:00\nkurse=17:45\npapier_danach=ja\n",
        "zeitplan-v1\nan=JA\ntermine=17:40\nkurse=17:45\npapier_danach=ja\n",
        "zeitplan-v1\nan=ja\ntermine=17:40\nkurse=17:45\npapier_danach=ja",
        "zeitplan-v1\r\nan=ja\r\ntermine=17:40\r\nkurse=17:45\r\npapier_danach=ja\r\n",
        "zeitplan-v1\nan=ja\ntermine=17:40\nkurse=17:45\npapier_danach=ja\nmehr\n",
    ] {
        assert!(Zeitplan::lies(kaputt).is_err(), "{kaputt:?}");
    }
    let tag = some(Datum::neu(2026, 11, 5), "Datum")?;
    let e = Erledigt {
        termine: Some(tag),
        kurse: None,
        kurse_nyse: None,
    };
    assert_eq!(Erledigt::lies(&e.als_text()), Ok(e));
    assert!(Erledigt::lies("erledigt-v1\ntermine=2026-13-01\nkurse=\n").is_err());
    Ok(())
}

#[test]
fn zeitplan_faellig_einmal_je_handelstag() -> Result<(), String> {
    let p = Zeitplan {
        an: true,
        ..Zeitplan::default()
    };
    let do_ = some(Datum::neu(2026, 11, 5), "Donnerstag")?;
    let sa = some(Datum::neu(2026, 11, 7), "Samstag")?;
    let mo = some(Datum::neu(2026, 11, 9), "Montag")?;
    let leer = Erledigt::default();
    assert_eq!(faellig(&p, &leer, do_, (17, 39)), None);
    assert_eq!(faellig(&p, &leer, do_, (17, 40)), Some(Art::Termine));
    assert_eq!(faellig(&p, &leer, do_, (23, 0)), Some(Art::Termine));
    let termine = Erledigt {
        termine: Some(do_),
        kurse: None,
        kurse_nyse: None,
    };
    assert_eq!(faellig(&p, &termine, do_, (17, 44)), None);
    assert_eq!(
        faellig(&p, &termine, do_, (17, 45)),
        Some(Art::KurseVon(Boerse::Xetra))
    );
    let beide = Erledigt {
        termine: Some(do_),
        kurse: Some(do_),
        kurse_nyse: None,
    };
    assert_eq!(faellig(&p, &beide, do_, (23, 59)), None);
    assert_eq!(faellig(&p, &leer, sa, (18, 0)), None);
    let aus = Zeitplan { an: false, ..p };
    assert_eq!(faellig(&aus, &leer, do_, (18, 0)), None);
    assert_eq!(naechster(&aus, &leer, do_), None);
    assert_eq!(
        naechster(&p, &beide, do_),
        Some((
            Art::Termine,
            some(Datum::neu(2026, 11, 6), "Freitag")?,
            (17, 40)
        ))
    );
    let freitag_fertig = Erledigt {
        termine: some(Datum::neu(2026, 11, 6), "Freitag")?.into(),
        kurse: some(Datum::neu(2026, 11, 6), "Freitag")?.into(),
        kurse_nyse: None,
    };
    assert_eq!(
        naechster(
            &p,
            &freitag_fertig,
            some(Datum::neu(2026, 11, 6), "Freitag")?
        ),
        Some((Art::Termine, mo, (17, 40)))
    );
    Ok(())
}

/// An Feiertagen aller Boersen der Liste laeuft nichts; der naechste Lauf ist der naechste Tag,
/// an dem gehandelt wird.
#[test]
fn zeitplan_ruht_an_boersenfeiertagen() -> Result<(), String> {
    let p = Zeitplan {
        an: true,
        ..Zeitplan::default()
    };
    let leer = Erledigt::default();
    let heiligabend = some(Datum::neu(2026, 12, 24), "Heiligabend")?;
    assert_eq!(faellig(&p, &leer, heiligabend, (18, 0)), None);
    let karfreitag = some(Datum::neu(2026, 4, 3), "Karfreitag")?;
    assert_eq!(faellig(&p, &leer, karfreitag, (18, 0)), None);
    let d23 = some(Datum::neu(2026, 12, 23), "23.12.")?;
    let fertig = Erledigt {
        termine: Some(d23),
        kurse: Some(d23),
        kurse_nyse: None,
    };
    assert_eq!(
        naechster(&p, &fertig, d23),
        Some((
            Art::Termine,
            some(Datum::neu(2026, 12, 28), "28.12.")?,
            (17, 40)
        ))
    );
    Ok(())
}

// --- Werkbank ---

#[test]
fn auftrag_laeuft_mit_protokoll_und_eindeutiger_kennung() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(12, 0)?;
    d.einreihen(Art::Termine, Anlass::Hand)
        .map_err(|e| format!("Vorbedingung einreihen: {e}"))?;
    let (nr, kind) = some(d.schritt(t), "Start")?;
    assert!(d.sperre().laufend.is_some());
    assert!(
        d.einreihen(Art::Termine, Anlass::Hand).is_err(),
        "laeuft schon"
    );
    // Waehrend ein Auftrag laeuft, wird kein Lagebild gerechnet.
    assert!(d.sperre().lage.is_none());
    d.verfolge(nr, kind, || donnerstag(12, 1).ok());
    let z = d.sperre();
    assert!(z.laufend.is_none());
    let a = some(z.verlauf.front(), "Verlauf")?;
    assert_eq!(a.erfolg, Some(true));
    assert_eq!(a.protokoll, vec!["lauf termine".to_string()]);
    assert_eq!(a.kennung, format!("{}-{nr}", t.sekunden()));
    drop(z);
    assert!(d.schritt(t).is_none());
    assert!(d.sperre().lage.is_some(), "danach das Lagebild");
    Ok(())
}

#[test]
fn zeitplan_startet_termine_kurse_und_danach_papier() -> Result<(), String> {
    let wurzel = eingeschaltet(ordner()?)?;
    let d = dienst(wurzel.clone(), true);
    // 17:46: beide faellig, Termine zuerst.
    let t = donnerstag(17, 46)?;
    let (nr, kind) = some(d.schritt(t), "Termine")?;
    assert_eq!(
        d.sperre().laufend.as_ref().map(|a| (a.art, a.anlass)),
        Some((Art::Termine, Anlass::Zeitplan))
    );
    d.verfolge(nr, kind, || donnerstag(17, 46).ok());
    let (nr, kind) = some(d.schritt(t), "Kurse")?;
    d.verfolge(nr, kind, || donnerstag(17, 47).ok());
    assert_eq!(wartend(&d), vec![(Art::Papier, Anlass::Zeitplan)]);
    let (nr, kind) = some(d.schritt(t), "Papier")?;
    d.verfolge(nr, kind, || donnerstag(17, 48).ok());
    assert!(d.schritt(t).is_none(), "heute nichts mehr");
    // Ein Neustart startet heute nichts noch einmal.
    let erledigt = fs::read_to_string(wurzel.join("daten").join("dienst_erledigt.txt"))
        .map_err(|e| format!("Erledigt fehlt: {e}"))?;
    assert_eq!(
        erledigt,
        "erledigt-v2\ntermine=2026-11-05\nkurse=2026-11-05\nkurse_nyse=\n"
    );
    let neu = dienst(wurzel, true);
    assert!(neu.schritt(donnerstag(20, 0)?).is_none());
    assert!(wartend(&neu).is_empty());
    Ok(())
}

#[test]
fn kein_papiertag_nach_gescheitertem_kursabruf() -> Result<(), String> {
    let d = dienst(eingeschaltet(ordner()?)?, false);
    let t = donnerstag(17, 46)?;
    let (nr, kind) = some(d.schritt(t), "Termine")?;
    d.verfolge(nr, kind, || donnerstag(17, 46).ok());
    let (nr, kind) = some(d.schritt(t), "Kurse")?;
    d.verfolge(nr, kind, || donnerstag(17, 47).ok());
    assert!(wartend(&d).is_empty());
    let z = d.sperre();
    assert_eq!(
        z.verlauf.iter().map(|a| a.erfolg).collect::<Vec<_>>(),
        vec![Some(false), Some(false)]
    );
    Ok(())
}

#[test]
fn unlesbarer_zeitplan_schaltet_ihn_aus() -> Result<(), String> {
    let wurzel = ordner()?;
    fs::write(wurzel.join("daten").join("zeitplan.txt"), "an=ja\n")
        .map_err(|e| format!("Vorbedingung: {e}"))?;
    let d = dienst(wurzel.clone(), true);
    assert!(!d.sperre().zeitplan.an);
    assert!(d.sperre().zeitplan_fehler.is_some());
    assert!(d.schritt(donnerstag(18, 0)?).is_none());
    assert!(wartend(&d).is_empty());
    // Dasselbe bei unlesbarem Erledigt: Es koennte sonst zweimal am Tag starten.
    let w2 = ordner()?;
    fs::write(
        w2.join("daten").join("dienst_erledigt.txt"),
        "erledigt-v1\n",
    )
    .map_err(|e| format!("Vorbedingung: {e}"))?;
    let d2 = dienst(w2, true);
    assert!(!d2.sperre().zeitplan.an);
    assert!(d2.schritt(donnerstag(18, 0)?).is_none());
    Ok(())
}

/// Maus K29-01: Ein Lesefehler ist kein Erststart. Nur eine fehlende Datei nimmt die
/// Voreinstellung; jeder andere Fehler haelt den Zeitplan sichtbar an.
#[test]
fn k29_01_lesefehler_halten_den_zeitplan_an() -> Result<(), String> {
    let umw = Attrappe::default();
    // Zeitplan nicht lesbar (hier ein Ordner an seiner Stelle): nichts startet, Grund sichtbar.
    let w1 = ordner()?;
    fs::create_dir_all(w1.join("daten").join("zeitplan.txt"))
        .map_err(|e| format!("Vorbedingung: {e}"))?;
    let d1 = dienst(w1, true);
    assert!(!d1.sperre().zeitplan.an);
    assert!(d1.sperre().zeitplan_fehler.is_some());
    assert!(d1.schritt(donnerstag(18, 0)?).is_none());
    assert!(wartend(&d1).is_empty());
    let seite = text(&behandle(
        &d1,
        &umw,
        &anfrage("GET", "/einrichtung", &[("Host", HOST)], ""),
        donnerstag(18, 0)?,
    ));
    assert!(seite.contains("nicht lesbar"), "Grund sichtbar");
    // Erledigt nicht lesbar (kein UTF-8): nichts startet, auch nicht nach neuem Speichern.
    let w2 = ordner()?;
    fs::write(
        w2.join("daten").join("dienst_erledigt.txt"),
        [0xff_u8, 0xfe, 0x00],
    )
    .map_err(|e| format!("Vorbedingung: {e}"))?;
    // Mit fertiger Einrichtung, damit das Einschalten selbst erlaubt ist.
    let d2 = Arc::new(Dienst::neu(
        w2,
        TOKEN.into(),
        echo_starter(true),
        lagebild_mit_schluesseln(true),
    ));
    assert!(!d2.sperre().zeitplan.an);
    assert!(d2.sperre().erledigt_fehler.is_some());
    assert!(d2.schritt(donnerstag(17, 59)?).is_none());
    let an = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&an=ja&termine=17%3A40&kurse=17%3A45&papier_danach=ja"),
    );
    assert!(text(&behandle(&d2, &umw, &an, donnerstag(18, 0)?)).contains("m=zeitplan#"));
    assert!(d2.schritt(donnerstag(18, 0)?).is_none());
    assert!(
        wartend(&d2).is_empty(),
        "ohne gesichertes Erledigt startet nichts"
    );
    // Fehlen beide Dateien, gilt die Voreinstellung (Erststart): Zeitplan aus, ohne Fehler.
    // Ein erster neugieriger Start loest nichts aus, auch keinen KI-Aufruf.
    let d3 = dienst(ordner()?, true);
    assert!(!d3.sperre().zeitplan.an);
    assert!(d3.sperre().zeitplan_fehler.is_none());
    assert!(d3.schritt(donnerstag(18, 0)?).is_none());
    assert!(wartend(&d3).is_empty());
    assert!(d3.sperre().verlauf.is_empty());
    Ok(())
}

#[test]
fn zeitplan_ueber_die_seite_speichern_und_aktivieren() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let umw = Attrappe::default();
    let t = donnerstag(12, 0)?;
    let ungueltig = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&an=ja&termine=8:00&kurse=18:00"),
    );
    assert!(text(&behandle(&d, &umw, &ungueltig, t)).contains("m=zeitplan_ungueltig"));
    assert_eq!(d.sperre().zeitplan, Zeitplan::default());
    let a = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&termine=08%3A00&kurse=18%3A05"),
    );
    assert!(text(&behandle(&d, &umw, &a, t)).contains("m=zeitplan#zeitplan"));
    let erwartet = Zeitplan {
        an: false,
        termine: (8, 0),
        kurse: (18, 5),
        papier_danach: false,
    };
    assert_eq!(d.sperre().zeitplan, erwartet);
    let datei = fs::read_to_string(wurzel.join("daten").join("zeitplan.txt"))
        .map_err(|e| format!("Datei fehlt: {e}"))?;
    assert_eq!(Zeitplan::lies(&datei), Ok(erwartet));
    let seite = text(&behandle(
        &d,
        &umw,
        &anfrage("GET", "/einrichtung", &[("Host", HOST)], ""),
        t,
    ));
    assert!(seite.contains("gespeichert und aktiv"));
    Ok(())
}

// --- Autostart ---

#[test]
fn autostart_nur_mit_harmlosem_pfad() -> Result<(), String> {
    let gut = autostart::inhalt(Path::new("C:\\Users\\Wolfgang\\DepotSchlopf"))?;
    assert!(gut.contains("\"C:\\Users\\Wolfgang\\DepotSchlopf\\DepotSchlopf.cmd\" --ohne-browser"));
    for boese in ["C:\\a%PATH%", "C:\\a\"&calc", "C:\\a!x!", "C:\\a\r\nx", ""] {
        assert!(autostart::inhalt(Path::new(boese)).is_err(), "{boese:?}");
    }
    let d = dienst(ordner()?, true);
    let umw = Attrappe::default();
    let a = formular_post("/autostart", &format!("t={TOKEN}&an=an"));
    assert!(text(&behandle(&d, &umw, &a, donnerstag(12, 0)?)).contains("m=autostart_an"));
    assert_eq!(
        *umw.autostart.lock().unwrap_or_else(PoisonError::into_inner),
        Some(true)
    );
    Ok(())
}

// --- Fehlerfaelle: Schluesselstatus, Gesamtstatus, Sperre des Kostenschutzes ---

fn protokoll_text(wurzel: &Path) -> String {
    fs::read_to_string(wurzel.join("daten").join(depot_lage::anbieter::DATEI)).unwrap_or_default()
}

#[test]
fn neuer_schluessel_ist_wieder_nur_hinterlegt() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let umw = Attrappe::default();
    let t = donnerstag(12, 0)?;
    // Der Anbieter hat Maus' alten Schluessel abgelehnt.
    let alt = depot_lage::anbieter::Eintrag::neu(
        donnerstag(11, 0)?,
        depot_lage::anbieter::Wer::Maus,
        depot_lage::anbieter::Ergebnis::Abgelehnt,
        Some(401),
    );
    let datei = wurzel.join("daten").join(depot_lage::anbieter::DATEI);
    depot_lage::anbieter::anhaengen(&datei, &alt)?;
    let seite = anfrage("GET", "/einrichtung", &[("Host", HOST)], "");
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(s.contains("abgelehnt (05.11.2026 11:00"), "{s}");
    let neu = formular_post(
        "/schluessel",
        &format!("t={TOKEN}&wer=openai&wert=Attrappe_Maus-0123456789_ABCDEFGHIJ"),
    );
    let r = text(&behandle(&d, &umw, &neu, t));
    assert!(
        r.contains("Location: /einrichtung?m=schluessel#schluessel"),
        "{r}"
    );
    let zeilen = protokoll_text(&wurzel);
    assert_eq!(zeilen.lines().count(), 2, "{zeilen}");
    assert!(
        zeilen
            .lines()
            .last()
            .is_some_and(|z| z.contains(";maus;neu;")),
        "{zeilen}"
    );
    assert!(!zeilen.contains("Attrappe"), "kein Schluessel im Protokoll");
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(!s.contains("abgelehnt (05.11.2026 11:00"), "{s}");
    assert!(s.contains("hinterlegt, noch nicht benutzt"), "{s}");
    Ok(())
}

/// Ein Lagebild mit allen Schluesseln, aber ohne Journal (also Eingriff noetig). Mit
/// `als_bereit` setzt es den Gesamtstatus danach auf bereit, damit der Test den Zeitplan-Teil
/// fuer sich pruefen kann.
fn lagebild_mit_schluesseln(als_bereit: bool) -> Lagerechner {
    Box::new(move |_wurzel: &Path, jetzt: Zeitpunkt| {
        let mut l = lagebild(&Umgebung {
            jetzt,
            journal: Zugang::NichtEingerichtet,
            kursquelle: Hinterlegt::Ja,
            berater: [
                (BeraterId::Dachs, Hinterlegt::Ja),
                (BeraterId::Maus, Hinterlegt::Ja),
            ],
            sperren: Vec::new(),
            anbieter: Vec::new(),
            anbieter_unlesbar: 0,
            ki_halt: None,
        });
        if als_bereit {
            l.betrieb = depot_lage::Betrieb::Bereit;
        }
        (l, "<section>LAGEBILD</section>".to_string())
    })
}

#[test]
fn gesamtstatus_ergaenzt_den_zeitplan() -> Result<(), String> {
    let mit = |als_bereit| {
        Ok::<_, String>(Arc::new(Dienst::neu(
            ordner()?,
            TOKEN.into(),
            echo_starter(true),
            lagebild_mit_schluesseln(als_bereit),
        )))
    };
    let t = donnerstag(12, 0)?;
    let heute = t.datum();
    let live = |d: &Dienst| seiten::live("s", &d.sperre(), heute);
    // Maus zu #31: Alle Schluessel da, aber kein Journal: nie „vollständig bereit“, auch mit
    // eingeschaltetem Zeitplan.
    let d = mit(false)?;
    assert!(d.schritt(t).is_none());
    d.sperre().zeitplan.an = true;
    let s = live(&d);
    assert!(
        s.contains("Eingriff nötig") && s.contains("Journal: Noch nicht eingerichtet"),
        "{s}"
    );
    assert!(!s.contains("Vollständig bereit"), "{s}");
    let d = mit(true)?;
    assert!(d.schritt(t).is_none());
    // Ab Werk ist der Zeitplan aus: Nichts laeuft von selbst.
    assert!(!d.sperre().zeitplan.an);
    assert!(live(&d).contains(">Pausiert<"), "{}", live(&d));
    d.sperre().zeitplan.an = true;
    assert!(live(&d).contains("Vollständig bereit"), "{}", live(&d));
    d.sperre().zeitplan.an = false;
    let s = live(&d);
    assert!(
        s.contains(">Pausiert<") && s.contains("Zeitplan aus"),
        "{s}"
    );
    d.sperre().zeitplan_fehler = Some("Zeitplan: Kopfzeile fehlt".into());
    let s = seiten::status("s", &d.sperre(), heute);
    assert!(
        s.contains("Eingriff nötig") && s.contains("Kopfzeile fehlt"),
        "{s}"
    );
    // Ohne Schluessel: Die Kursquelle braucht einen Eingriff, die KIs stehen dahinter.
    let d = dienst(ordner()?, true);
    assert!(d.schritt(t).is_none());
    let s = live(&d);
    assert!(
        s.contains("Eingriff nötig") && s.contains("kein Schlüssel, wird nicht gefragt"),
        "{s}"
    );
    Ok(())
}

fn aufheben(abdruck: &str, geprueft: bool) -> Anfrage {
    let haken = if geprueft { "&geprueft=ja" } else { "" };
    formular_post(
        "/kostenschutz",
        &format!("t={TOKEN}&abdruck={abdruck}{haken}"),
    )
}

#[test]
fn sperre_des_kostenschutzes_gefuehrt_aufheben() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let umw = Attrappe::default();
    let t = donnerstag(18, 0)?;
    let halt = kostenschutz::pfad(&wurzel);
    let grund = "Kosten unbekannt: Antwort fehlte (Dachs) <b>\nreserviert 40 Cent\n";
    fs::write(&halt, grund).map_err(|e| format!("Vorbedingung ki.halt: {e}"))?;
    let journal = wurzel.join("daten").join("journal.txt");
    fs::write(&journal, "unberuehrt").map_err(|e| format!("Vorbedingung Journal: {e}"))?;
    let abdruck = kostenschutz::abdruck(grund);

    let seite = anfrage("GET", "/einrichtung", &[("Host", HOST)], "");
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(s.contains("id=\"kostenschutz\""), "{s}");
    assert!(
        s.contains("Antwort fehlte (Dachs) &lt;b&gt;"),
        "maskiert: {s}"
    );
    assert!(s.contains(&format!("name=\"abdruck\" value=\"{abdruck}\"")));
    assert!(s.contains("name=\"geprueft\" value=\"ja\" required"));

    // Ohne Bestaetigung, mit fremdem Abdruck oder waehrend ein Auftrag wartet: nichts.
    let r = text(&behandle(&d, &umw, &aufheben(&abdruck, false), t));
    assert!(r.contains("m=kostenschutz_unbestaetigt"), "{r}");
    let r = text(&behandle(&d, &umw, &aufheben("0000000000000000", true), t));
    assert!(r.contains("m=kostenschutz_geaendert"), "{r}");
    d.einreihen(Art::Papier, Anlass::Hand)
        .map_err(|e| format!("Vorbedingung einreihen: {e}"))?;
    let r = text(&behandle(&d, &umw, &aufheben(&abdruck, true), t));
    assert!(r.contains("m=kostenschutz_belegt"), "{r}");
    assert!(halt.exists(), "die Sperre bleibt");
    d.sperre().warteschlange.clear();
    // Ohne Token wie jedes Formular abgelehnt.
    let fremd = formular_post(
        "/kostenschutz",
        &format!("t=falsch&abdruck={abdruck}&geprueft=ja"),
    );
    assert_eq!(status(&behandle(&d, &umw, &fremd, t))?, 403);
    assert!(halt.exists());

    let r = text(&behandle(&d, &umw, &aufheben(&abdruck, true), t));
    assert!(r.contains("m=kostenschutz_aufgehoben"), "{r}");
    assert!(!halt.exists());
    let festgehalten = fs::read_to_string(kostenschutz::protokoll_pfad(&wurzel))
        .map_err(|e| format!("Protokoll: {e}"))?;
    assert!(festgehalten.contains(grund), "{festgehalten}");
    assert_eq!(
        fs::read_to_string(&journal).map_err(|e| format!("Journal: {e}"))?,
        "unberuehrt",
        "Kassenbuch und Journal bleiben, wie sie sind"
    );
    assert!(d.sperre().lage_veraltet);
    let r = text(&behandle(&d, &umw, &aufheben(&abdruck, true), t));
    assert!(r.contains("m=kostenschutz_frei"), "{r}");
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(s.contains("Keine Sperre"), "{s}");
    Ok(())
}

/// Die Kurssperre nach einem Sicherheitsabbruch wird wie die Sperre des Kostenschutzes gefuehrt
/// aufgehoben, nie von Hand geloescht (Hinweis zu #34).
#[test]
fn kurssperre_gefuehrt_aufheben() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let umw = Attrappe::default();
    let t = donnerstag(18, 0)?;
    let seite = anfrage("GET", "/einrichtung", &[("Host", HOST)], "");
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(
        !s.contains("id=\"kurssperre\""),
        "ohne Sperre kein Abschnitt"
    );

    let halt = kostenschutz::kurse_pfad(&wurzel);
    assert!(halt.ends_with("kurse.halt"));
    let grund = "Sicherheitsabbruch: Die Kursquelle gab den Schluessel zurueck <i>\n";
    fs::write(&halt, grund).map_err(|e| format!("Vorbedingung kurse.halt: {e}"))?;
    let abdruck = kostenschutz::abdruck(grund);
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(s.contains("id=\"kurssperre\""), "{s}");
    assert!(s.contains("zurueck &lt;i&gt;"), "maskiert: {s}");
    assert!(s.contains("action=\"/kurssperre\""), "{s}");

    let post = |abdruck: &str, haken: bool| {
        let h = if haken { "&geprueft=ja" } else { "" };
        formular_post("/kurssperre", &format!("t={TOKEN}&abdruck={abdruck}{h}"))
    };
    let r = text(&behandle(&d, &umw, &post(&abdruck, false), t));
    assert!(r.contains("m=kurssperre_unbestaetigt"), "{r}");
    let r = text(&behandle(&d, &umw, &post("0000000000000000", true), t));
    assert!(r.contains("m=kurssperre_geaendert"), "{r}");
    d.einreihen(Art::Kurse, Anlass::Hand)
        .map_err(|e| format!("Vorbedingung einreihen: {e}"))?;
    let r = text(&behandle(&d, &umw, &post(&abdruck, true), t));
    assert!(r.contains("m=kurssperre_belegt"), "{r}");
    assert!(halt.exists(), "die Sperre bleibt");
    d.sperre().warteschlange.clear();

    let r = text(&behandle(&d, &umw, &post(&abdruck, true), t));
    assert!(r.contains("m=kurssperre_aufgehoben"), "{r}");
    assert!(!halt.exists());
    let festgehalten = fs::read_to_string(kostenschutz::kurse_protokoll_pfad(&wurzel))
        .map_err(|e| format!("Protokoll: {e}"))?;
    assert!(festgehalten.contains(grund), "{festgehalten}");
    let r = text(&behandle(&d, &umw, &post(&abdruck, true), t));
    assert!(r.contains("m=kurssperre_frei"), "{r}");
    Ok(())
}

// --- Fehlbedienung: Einschalten, Eingaben erhalten, Doppelklicks, Unterbrechungen ---

fn ort(location: &str) -> String {
    location
        .lines()
        .find_map(|z| z.strip_prefix("Location: "))
        .unwrap_or_default()
        // Den Teil nach `#` sendet ein Browser nicht.
        .split('#')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn get(d: &Dienst, ziel: &str, t: Zeitpunkt) -> String {
    text(&behandle(
        d,
        &Attrappe::default(),
        &anfrage("GET", ziel, &[("Host", HOST)], ""),
        t,
    ))
}

#[test]
fn zeitplan_einschalten_erst_nach_vollstaendiger_einrichtung() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let umw = Attrappe::default();
    let t = donnerstag(12, 0)?;
    let an = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&an=ja&termine=08%3A15&kurse=18%3A05&papier_danach=ja"),
    );
    // Noch kein Lagebild: nicht eingeschaltet, Eingaben bleiben.
    let r = text(&behandle(&d, &umw, &an, t));
    assert!(r.contains("m=zeitplan_lage"), "{r}");
    assert!(!d.sperre().zeitplan.an);
    // Lagebild da, aber Eingriff noetig (kein Journal, keine Schluessel).
    assert!(d.schritt(t).is_none());
    let r = text(&behandle(&d, &umw, &an, t));
    assert!(r.contains("m=zeitplan_einrichtung"), "{r}");
    assert!(!d.sperre().zeitplan.an);
    assert!(
        !wurzel.join("daten").join("zeitplan.txt").exists(),
        "nichts gespeichert"
    );
    // Die Seite zeigt die eigenen Eingaben wieder und sagt, was fehlt.
    let seite = get(&d, &ort(&r), t);
    assert!(seite.contains("Zeitplan nicht eingeschaltet"), "{seite}");
    assert!(seite.contains("value=\"08:15\""), "{seite}");
    assert!(seite.contains("value=\"18:05\""), "{seite}");
    assert!(seite.contains("nicht gespeicherten Eingaben"), "{seite}");
    assert!(seite.contains("Einschalten geht erst"), "{seite}");
    // Ohne Entwurf zeigt die Seite den aktiven Plan.
    let seite = get(&d, "/einrichtung", t);
    assert!(seite.contains("value=\"17:40\""), "{seite}");
    // Uhrzeiten aendern ohne Einschalten geht immer.
    let aus = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&termine=08%3A15&kurse=18%3A05"),
    );
    assert!(text(&behandle(&d, &umw, &aus, t)).contains("m=zeitplan#"));
    // Mit fertiger Einrichtung geht das Einschalten.
    let fertig = Arc::new(Dienst::neu(
        ordner()?,
        TOKEN.into(),
        echo_starter(true),
        lagebild_mit_schluesseln(true),
    ));
    assert!(fertig.schritt(t).is_none());
    assert!(text(&behandle(&fertig, &umw, &an, t)).contains("m=zeitplan#"));
    assert!(fertig.sperre().zeitplan.an);
    Ok(())
}

#[test]
fn ungueltige_eingabe_laesst_die_gueltigen_stehen() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(12, 0)?;
    let f = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&termine=8%3A00&kurse=18%3A20&papier_danach=ja"),
    );
    let r = text(&behandle(&d, &Attrappe::default(), &f, t));
    let ziel = ort(&r);
    assert!(ziel.contains("m=zeitplan_ungueltig"), "{ziel}");
    assert!(
        ziel.contains("kurse=18%3A20") && !ziel.contains("termine="),
        "{ziel}"
    );
    let seite = get(&d, &ziel, t);
    assert!(seite.contains("value=\"18:20\""), "{seite}");
    // Fremde Werte in der Abfrage erscheinen nicht.
    let seite = get(
        &d,
        "/einrichtung?entwurf=ja&kurse=%3Cb%3E&termine=99%3A99",
        t,
    );
    assert!(
        !seite.contains("&lt;b&gt;") && !seite.contains("99:99"),
        "{seite}"
    );
    Ok(())
}

#[test]
fn doppelklick_startet_nichts_doppelt() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let umw = Attrappe::default();
    let t = donnerstag(12, 0)?;
    let klick = formular_post("/ausfuehren", &format!("t={TOKEN}&art=papier"));
    assert!(text(&behandle(&d, &umw, &klick, t)).contains("m=auftrag"));
    let r = text(&behandle(&d, &umw, &klick, t));
    assert!(r.contains("m=auftrag_belegt"), "{r}");
    assert_eq!(wartend(&d), vec![(Art::Papier, Anlass::Hand)]);
    let (nr, kind) = some(d.schritt(t), "Start")?;
    d.verfolge(nr, kind, || donnerstag(12, 0).ok());
    // Erneut gesendet gleich nach dem Ende: nicht noch einmal.
    let r = text(&behandle(&d, &umw, &klick, donnerstag(12, 0)?));
    assert!(r.contains("m=auftrag_belegt"), "{r}");
    assert!(wartend(&d).is_empty());
    // Mit Abstand ist es ein neuer, gewollter Auftrag.
    let spaeter = some(
        Zeitpunkt::aus_sekunden(t.sekunden().saturating_add(60)),
        "spaeter",
    )?;
    assert!(text(&behandle(&d, &umw, &klick, spaeter)).contains("m=auftrag"));
    assert_eq!(wartend(&d), vec![(Art::Papier, Anlass::Hand)]);
    // Das Skript sperrt abgesendete Knoepfe.
    let js = get(&d, "/app.js", t);
    assert!(js.contains("b.disabled = true"), "{js}");
    Ok(())
}

#[test]
fn verlauf_uebersteht_neustart_und_zeigt_unterbrochenes() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let t = donnerstag(12, 0)?;
    d.einreihen(Art::Termine, Anlass::Hand)
        .map_err(|e| format!("Vorbedingung: {e}"))?;
    let (nr, kind) = some(d.schritt(t), "Termine")?;
    d.verfolge(nr, kind, || donnerstag(12, 1).ok());
    // Der naechste Lauf wird gestartet, aber der Dienst endet, bevor er fertig ist.
    d.einreihen(Art::Kurse, Anlass::Hand)
        .map_err(|e| format!("Vorbedingung: {e}"))?;
    let (_, mut kind) = some(d.schritt(donnerstag(12, 2)?), "Kurse")?;
    let _ = kind.wait();
    drop(d);
    let neu = dienst(wurzel.clone(), true);
    let z = neu.sperre();
    let v: Vec<(Art, Frueher, Option<bool>)> = z
        .verlauf
        .iter()
        .map(|a| (a.art, a.frueher, a.erfolg))
        .collect();
    assert_eq!(
        v,
        vec![
            (Art::Kurse, Frueher::Unterbrochen, Some(false)),
            (Art::Termine, Frueher::Beendet, Some(true)),
        ]
    );
    let seite = seiten::live("s", &z, t.datum());
    drop(z);
    assert!(
        seite.contains("unterbrochen") && seite.contains("vor dem Neustart"),
        "{seite}"
    );
    assert!(
        seite.contains("Der Dienst endete während dieses Laufs"),
        "{seite}"
    );
    // Ein zweiter Neustart zeigt dasselbe (die Datei wird nur gekuerzt).
    drop(neu);
    let wieder = dienst(wurzel, true);
    assert_eq!(wieder.sperre().verlauf.len(), 2);
    Ok(())
}

#[test]
fn verlauf_liest_streng() {
    let g = depot_dienst::verlauf::lies(
        "verlauf-v1;start;1-1;kurse;hand;1800000000\n\
         kaputt\n\
         verlauf-v1;start;1-2;fuchs;hand;1800000000\n\
         verlauf-v1;ende;1-1;kurse;hand;1800000000;1800000060;ok\n\
         verlauf-v1;ende;1-3;termine;zeitplan;1800000000;-;fehler;mehr\n",
    );
    assert_eq!(g.unlesbar, 3);
    assert_eq!(g.auftraege.len(), 1);
    assert!(
        g.auftraege
            .first()
            .is_some_and(|a| a.erfolg == Some(true) && a.frueher == Frueher::Beendet)
    );
}

/// Maus zu #32: „Läuft ohne KI“ reicht zum Einschalten nicht; jeder Schluessel wird
/// ausdruecklich geprueft, und die Seite nennt, was fehlt.
#[test]
fn einschalten_verlangt_jeden_schluessel() -> Result<(), String> {
    let ohne_maus: Lagerechner = Box::new(|_wurzel: &Path, jetzt: Zeitpunkt| {
        let mut l = lagebild(&Umgebung {
            jetzt,
            journal: Zugang::NichtEingerichtet,
            kursquelle: Hinterlegt::Ja,
            berater: [
                (BeraterId::Dachs, Hinterlegt::Ja),
                (BeraterId::Maus, Hinterlegt::Nein),
            ],
            sperren: Vec::new(),
            anbieter: Vec::new(),
            anbieter_unlesbar: 0,
            ki_halt: None,
        });
        // Wie mit eingerichtetem Journal: nur Maus fehlt.
        l.betrieb = depot_lage::Betrieb::OhneKi(vec!["Maus (GPT): kein Schlüssel".into()]);
        (l, String::new())
    });
    let d = Arc::new(Dienst::neu(
        ordner()?,
        TOKEN.into(),
        echo_starter(true),
        ohne_maus,
    ));
    let t = donnerstag(12, 0)?;
    assert!(d.schritt(t).is_none());
    let an = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&an=ja&termine=17%3A40&kurse=17%3A45&papier_danach=ja"),
    );
    let r = text(&behandle(&d, &Attrappe::default(), &an, t));
    assert!(r.contains("m=zeitplan_einrichtung"), "{r}");
    assert!(!d.sperre().zeitplan.an);
    let seite = get(&d, "/einrichtung", t);
    assert!(
        seite.contains("<li>Maus (GPT): Schlüssel fehlt</li>"),
        "{seite}"
    );
    assert!(!seite.contains("<li>Dachs (Claude)"), "{seite}");
    Ok(())
}

/// Maus zu #32: Erst dauerhaft vermerken, dann starten. Laesst sich der Verlauf nicht
/// schreiben, startet nichts, und die Oberflaeche zeigt es sofort.
#[test]
fn ohne_schreibbaren_verlauf_startet_nichts() -> Result<(), String> {
    let wurzel = ordner()?;
    let datei = depot_dienst::verlauf::pfad(&wurzel);
    fs::create_dir_all(&datei).map_err(|e| format!("Vorbedingung: {e}"))?;
    let d = dienst(wurzel, true);
    let t = donnerstag(12, 0)?;
    d.einreihen(Art::Termine, Anlass::Hand)
        .map_err(|e| format!("Vorbedingung: {e}"))?;
    assert!(d.schritt(t).is_none(), "nichts gestartet");
    let z = d.sperre();
    assert!(z.laufend.is_none());
    let a = some(z.verlauf.front(), "Verlauf")?;
    assert_eq!(a.erfolg, Some(false));
    assert!(a.protokoll.iter().any(|p| p.starts_with("Nicht gestartet")));
    let seite = seiten::live("s", &z, t.datum());
    drop(z);
    assert!(seite.contains("Verlauf nicht schreibbar"), "{seite}");
    // Wieder schreibbar: Der naechste Auftrag startet.
    fs::remove_dir(&datei).map_err(|e| format!("Vorbedingung: {e}"))?;
    d.einreihen(Art::Kurse, Anlass::Hand)
        .map_err(|e| format!("Vorbedingung: {e}"))?;
    let (nr, kind) = some(d.schritt(t), "Start")?;
    d.verfolge(nr, kind, || donnerstag(12, 1).ok());
    let text = fs::read_to_string(&datei).map_err(|e| format!("Verlauf: {e}"))?;
    assert!(text.starts_with("verlauf-v1;start;"), "{text}");
    assert!(d.sperre().verlauf_fehler.is_none(), "Fehler verschwindet");
    Ok(())
}

/// Ein Starter, der diese Zeilen ausgibt und dann scheitert.
fn zeilen_starter(zeilen: &'static [&'static str]) -> Starter {
    Box::new(move |_art: Art, _wurzel: &Path| {
        #[cfg(windows)]
        {
            let mut c = Command::new("cmd");
            let echo: Vec<String> = zeilen.iter().map(|z| format!("echo {z}")).collect();
            c.args(["/C", &format!("{}& exit /b 1", echo.join("& "))]);
            c
        }
        #[cfg(not(windows))]
        {
            let mut c = Command::new("sh");
            let echo: Vec<String> = zeilen.iter().map(|z| format!("echo '{z}'")).collect();
            c.args(["-c", &format!("{}; exit 1", echo.join("; "))]);
            c
        }
    })
}

/// Maus zu den Netzausfaellen: Die Live-Ansicht zeigt gross, was der Lauf zuletzt als Hinweis
/// sagte, auch nach seinem Ende: etwa warum der Papiertag heute ausfiel. Der Hinweis steht auch
/// in der Zeile des Verlaufs.
#[test]
fn hinweis_des_laufs_steht_gross_in_der_live_ansicht() -> Result<(), String> {
    let d = Arc::new(Dienst::neu(
        ordner()?,
        TOKEN.into(),
        zeilen_starter(&[
            "[Sammler] Hole 2 Werte",
            "[Hinweis] Verbindung weg. Naechster Versuch um 17:48 Uhr (Versuch 2 von 4).",
            "[Hinweis] Papiertag heute ausgefallen. Die Verbindung kam bis 17:54 Uhr nicht wieder.",
            "[Sammler] Kurse NICHT verwendbar",
        ]),
        leeres_lagebild(),
    ));
    let t = donnerstag(17, 45)?;
    d.einreihen(Art::Kurse, Anlass::Zeitplan)
        .map_err(|e| format!("Vorbedingung einreihen: {e}"))?;
    let (nr, kind) = some(d.schritt(t), "Start")?;
    d.verfolge(nr, kind, || donnerstag(17, 55).ok());
    let z = d.sperre();
    let a = some(z.verlauf.front(), "Verlauf")?;
    assert_eq!(
        a.hinweis.as_deref(),
        Some("Papiertag heute ausgefallen. Die Verbindung kam bis 17:54 Uhr nicht wieder.")
    );
    assert_eq!(a.protokoll.len(), 4, "Hinweise bleiben im Protokoll");
    let seite = seiten::live("s", &z, t.datum());
    assert!(
        seite.contains(
            "<p class=\"kopf halt\" id=\"hinweis\">Kurse abrufen: Papiertag heute ausgefallen."
        ),
        "{seite}"
    );
    // Am naechsten Tag steht der alte Hinweis nicht mehr oben, nur im Verlauf.
    let morgen = some(t.datum().and_then(|d| d.plus_tage(1)), "morgen")?;
    let seite = seiten::live("s", &z, Some(morgen));
    assert!(!seite.contains("id=\"hinweis\""), "{seite}");
    assert!(seite.contains("Papiertag heute ausgefallen"), "{seite}");
    Ok(())
}

/// Maus zu KI-Aufrufen: Die Einrichtung erklaert in Alltagssprache den Unterschied zwischen
/// einer Anfrage, die nie hinausging (wird wiederholt, kostet nichts), und einer, die nach dem
/// Absenden abbrach (Kostensperre, nie wiederholt). Mit und ohne Sperre.
#[test]
fn einrichtung_erklaert_abgebrochene_ki_anfragen() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let t = donnerstag(18, 0)?;
    let ohne = get(&d, "/einrichtung", t);
    assert!(ohne.contains("gar nicht erst hinausgegangen"), "{ohne}");
    assert!(
        ohne.contains("Ein zweiter Versuch könnte doppelt kosten"),
        "{ohne}"
    );
    fs::write(
        kostenschutz::pfad(&wurzel),
        "Dachs am Kostenschutz gesperrt: Verbindung nach dem Absenden abgebrochen\n",
    )
    .map_err(|e| format!("Vorbedingung Sperre: {e}"))?;
    let mit = get(&d, "/einrichtung", t);
    assert!(
        mit.contains("Verbindung nach dem Absenden abgebrochen"),
        "{mit}"
    );
    assert!(mit.contains("id=\"ki-verbindung\""), "{mit}");
    Ok(())
}

/// Kurse je Boerse: Xetra zur eingestellten Uhrzeit, die NYSE zehn Minuten nach ihrem eigenen
/// Schluss in Frankfurter Zeit, mit Sommerzeit und verkuerzten Tagen. An einem Tag, an dem nur
/// eine Boerse handelt, nur diese. Der letzte Abruf des Tages bestimmt den Papiertag.
#[test]
fn kurse_je_boerse_nach_ihrem_schluss() -> Result<(), String> {
    let p = Zeitplan {
        an: true,
        ..Zeitplan::default()
    };
    let beide = [Boerse::Xetra, Boerse::Nyse];
    let tag = |m, t| some(Datum::neu(2026, m, t), "Datum");
    // Donnerstag, 05.11.: beide offen, NYSE 16:00 New York = 22:00 Frankfurt.
    let do_ = tag(11, 5)?;
    assert_eq!(
        abrufe_fuer(&p, do_, &beide),
        vec![(Boerse::Xetra, (17, 45)), (Boerse::Nyse, (22, 10))]
    );
    // 29.10.: Frankfurt schon Winterzeit, New York noch Sommerzeit: 21:00 + 10 Minuten.
    assert_eq!(
        abrufe_fuer(&p, tag(10, 29)?, &beide),
        vec![(Boerse::Xetra, (17, 45)), (Boerse::Nyse, (21, 10))]
    );
    // 27.11., Tag nach Thanksgiving: NYSE schliesst um 13:00 New York = 19:00 Frankfurt.
    assert_eq!(
        abrufe_fuer(&p, tag(11, 27)?, &beide),
        vec![(Boerse::Xetra, (17, 45)), (Boerse::Nyse, (19, 10))]
    );
    // 26.11., Thanksgiving: nur Xetra. 24.12.: nur die NYSE (bis 13:00 New York).
    assert_eq!(
        abrufe_fuer(&p, tag(11, 26)?, &beide),
        vec![(Boerse::Xetra, (17, 45))]
    );
    assert_eq!(
        abrufe_fuer(&p, tag(12, 24)?, &beide),
        vec![(Boerse::Nyse, (19, 10))]
    );
    assert!(letzter_abruf_fuer(&p, do_, Boerse::Nyse, &beide));
    assert!(!letzter_abruf_fuer(&p, do_, Boerse::Xetra, &beide));
    assert!(letzter_abruf_fuer(&p, tag(11, 26)?, Boerse::Xetra, &beide));

    // Faellig: Termine, dann Xetra, dann NYSE, je einmal.
    let mut e = Erledigt::default();
    assert_eq!(
        faellig_fuer(&p, &e, do_, (17, 45), &beide),
        Some(Art::Termine)
    );
    e.setze(Art::Termine, do_);
    assert_eq!(
        faellig_fuer(&p, &e, do_, (17, 45), &beide),
        Some(Art::KurseVon(Boerse::Xetra))
    );
    e.setze(Art::KurseVon(Boerse::Xetra), do_);
    assert_eq!(faellig_fuer(&p, &e, do_, (22, 9), &beide), None);
    assert_eq!(
        naechster_fuer(&p, &e, do_, &beide),
        Some((Art::KurseVon(Boerse::Nyse), do_, (22, 10)))
    );
    assert_eq!(
        faellig_fuer(&p, &e, do_, (22, 10), &beide),
        Some(Art::KurseVon(Boerse::Nyse))
    );
    e.setze(Art::KurseVon(Boerse::Nyse), do_);
    assert_eq!(faellig_fuer(&p, &e, do_, (23, 59), &beide), None);
    assert_eq!(e.kurse_nyse, Some(do_));
    // Heiligabend handelt nur die NYSE: der Zeitplan laeuft, aber nur fuer sie.
    let mut h = Erledigt::default();
    h.setze(Art::Termine, tag(12, 24)?);
    assert_eq!(
        faellig_fuer(&p, &h, tag(12, 24)?, (19, 10), &beide),
        Some(Art::KurseVon(Boerse::Nyse))
    );
    Ok(())
}

/// Ein Erledigt der frueheren Form (ohne Abruf je Boerse) wird gelesen; `kurse` war Xetra.
/// Geschrieben wird nur die neue Form.
#[test]
fn erledigt_frueherer_form() -> Result<(), String> {
    let tag = some(Datum::neu(2026, 11, 5), "Datum")?;
    let alt = Erledigt::lies("erledigt-v1\ntermine=2026-11-05\nkurse=2026-11-05\n")?;
    assert_eq!(
        alt,
        Erledigt {
            termine: Some(tag),
            kurse: Some(tag),
            kurse_nyse: None,
        }
    );
    assert_eq!(
        alt.als_text(),
        "erledigt-v2\ntermine=2026-11-05\nkurse=2026-11-05\nkurse_nyse=\n"
    );
    assert!(Erledigt::lies("erledigt-v1\ntermine=\nkurse=\nkurse_nyse=\n").is_err());
    assert!(Erledigt::lies("erledigt-v2\ntermine=\nkurse=\n").is_err());
    Ok(())
}

/// Mit zwei Boersen: Der Papiertag folgt erst dem letzten Abruf des Tages (NYSE), und nur, wenn
/// ein Abruf dieses Tages erfolgreich war. Ein gescheiterter Xetra-Abruf haelt ihn nicht auf,
/// wenn die NYSE gelingt, und umgekehrt.
#[test]
fn papiertag_nach_dem_letzten_abruf_des_tages() -> Result<(), String> {
    for (xetra_ok, nyse_ok, papier) in [
        (true, true, true),
        (false, true, true),
        (true, false, true),
        (false, false, false),
    ] {
        let wurzel = eingeschaltet(ordner()?)?;
        // Ein Starter, der je Auftrag so endet, wie diese Runde es will.
        let d = Arc::new(Dienst::neu(
            wurzel,
            TOKEN.into(),
            echo_starter_je_art(move |a| match a {
                Art::KurseVon(Boerse::Xetra) => xetra_ok,
                Art::KurseVon(Boerse::Nyse) => nyse_ok,
                _ => true,
            }),
            leeres_lagebild(),
        ));
        d.sperre().boersen = vec![Boerse::Xetra, Boerse::Nyse];
        let (nr, kind) = some(d.schritt(donnerstag(17, 46)?), "Termine")?;
        d.verfolge(nr, kind, || donnerstag(17, 46).ok());
        let (nr, kind) = some(d.schritt(donnerstag(17, 46)?), "Xetra")?;
        assert_eq!(
            d.sperre().laufend.as_ref().map(|a| a.art),
            Some(Art::KurseVon(Boerse::Xetra))
        );
        d.verfolge(nr, kind, || donnerstag(17, 47).ok());
        assert!(wartend(&d).is_empty(), "nach Xetra noch kein Papiertag");
        assert!(
            d.schritt(donnerstag(22, 9)?).is_none(),
            "NYSE erst um 22:10"
        );
        let (nr, kind) = some(d.schritt(donnerstag(22, 10)?), "NYSE")?;
        assert_eq!(
            d.sperre().laufend.as_ref().map(|a| a.art),
            Some(Art::KurseVon(Boerse::Nyse))
        );
        d.verfolge(nr, kind, || donnerstag(22, 11).ok());
        let erwartet = if papier {
            vec![(Art::Papier, Anlass::Zeitplan)]
        } else {
            Vec::new()
        };
        assert_eq!(wartend(&d), erwartet, "Xetra {xetra_ok}, NYSE {nyse_ok}");
    }
    Ok(())
}
