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

/// Nur Xetra, unabhaengig davon, welche Boersen die festgelegte Liste hat.
fn faellig_x(p: Zeitplan, e: &Erledigt, tag: Datum, zeit: (u8, u8)) -> Option<Art> {
    faellig_fuer(&p, e, tag, zeit, &[Boerse::Xetra])
}

fn naechster_x(p: Zeitplan, e: &Erledigt, tag: Datum) -> Option<(Art, Datum, (u8, u8))> {
    naechster_fuer(&p, e, tag, &[Boerse::Xetra])
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

/// Schaltet den Zeitplan ein, wie es der Mensch in der Einrichtung tut (ab Werk ist er aus).
fn abendplan() -> Zeitplan {
    Zeitplan {
        termine: (17, 40),
        kurse: (17, 45),
        ..Zeitplan::default()
    }
}

fn eingeschaltet(wurzel: PathBuf) -> Result<PathBuf, String> {
    let p = Zeitplan {
        an: true,
        kurse: (17, 44),
        ..abendplan()
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
    papierfreigabe: bool,
    sperrfreigabe: bool,
    sperrdatei_wechsel: Option<PathBuf>,
    gemini_schluessel: Option<Hinterlegt>,
    sicherungsziel: Option<String>,
}

impl Umwelt for Attrappe {
    fn schluessel(&self, wer: Wer) -> Hinterlegt {
        if wer == Wer::Gemini {
            self.gemini_schluessel.unwrap_or(Hinterlegt::Ja)
        } else {
            Hinterlegt::Ja
        }
    }

    fn schluessel_ablegen(&self, wer: Wer, text: &str) -> Result<(), String> {
        let s = wer.pruefe(text)?;
        self.abgelegt
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((wer, s));
        Ok(())
    }

    fn sicherungsziel_waehlen(&self) -> Result<Option<String>, String> {
        Ok(self.sicherungsziel.clone())
    }

    fn autostart(&self, _wurzel: &Path) -> depot_dienst::autostart::Stand {
        depot_dienst::autostart::Stand::Aus
    }

    fn papierfreigabe_bestaetigen(
        &self,
        _angebot: &depot_lage::PapierFreigabeAngebot,
    ) -> Option<Zeitpunkt> {
        self.papierfreigabe
            .then_some("2026-11-05T11:01:23Z")
            .and_then(Zeitpunkt::lies)
    }

    fn sperre_bestaetigen(&self, _: depot_dienst::Sperre) -> Option<Zeitpunkt> {
        if let Some(pfad) = &self.sperrdatei_wechsel {
            fs::write(pfad, "Neue Sperre").ok()?;
        }
        self.sperrfreigabe.then(|| donnerstag(18, 1).ok()).flatten()
    }

    fn papierdialog_testen(&self) -> depot_dienst::DialogErgebnis {
        depot_dienst::DialogErgebnis::Ja
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
    d.sperre()
        .warteschlange
        .iter()
        .map(|a| (a.art, a.anlass))
        .collect()
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
        let antwort = behandle(&d, &umw, &a, t);
        assert_eq!(status(&antwort)?, 403, "{falsch}");
        assert!(text(&antwort).contains("Zurück zu Live"));
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
    assert!(r.contains("Einstellungen gespeichert."));
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
    // Tiingo wie im Sammler: 20 bis 64 Kleinbuchstaben und Ziffern.
    let tiingo = Wer::KursquelleUs;
    assert_eq!(tiingo.adresse(), "DepotSchlopf/tiingo");
    assert!(tiingo.pruefe(&"a1".repeat(20)).is_ok());
    assert!(tiingo.pruefe(&"A1".repeat(20)).is_err());
    assert!(tiingo.pruefe(&"a1".repeat(9)).is_err());
    assert!(tiingo.pruefe(&"a".repeat(65)).is_err());
    assert_eq!(Wer::aus_kurz("tiingo"), Some(tiingo));
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
        ..Erledigt::default()
    };
    assert_eq!(Erledigt::lies(&e.als_text()), Ok(e));
    assert!(Erledigt::lies("erledigt-v1\ntermine=2026-13-01\nkurse=\n").is_err());
    Ok(())
}

#[test]
fn zeitplan_faellig_einmal_je_handelstag() -> Result<(), String> {
    let p = Zeitplan {
        an: true,
        ..abendplan()
    };
    let do_ = some(Datum::neu(2026, 11, 5), "Donnerstag")?;
    let sa = some(Datum::neu(2026, 11, 7), "Samstag")?;
    let mo = some(Datum::neu(2026, 11, 9), "Montag")?;
    let leer = Erledigt::default();
    assert_eq!(faellig_x(p, &leer, do_, (17, 39)), None);
    assert_eq!(faellig_x(p, &leer, do_, (17, 40)), Some(Art::Termine));
    assert_eq!(faellig_x(p, &leer, do_, (23, 0)), Some(Art::Termine));
    let termine = Erledigt {
        termine: Some(do_),
        kurse: None,
        kurse_nyse: None,
        ..Erledigt::default()
    };
    assert_eq!(faellig_x(p, &termine, do_, (17, 44)), None);
    assert_eq!(
        faellig_x(p, &termine, do_, (17, 45)),
        Some(Art::KurseVon(Boerse::Xetra))
    );
    let beide = Erledigt {
        termine: Some(do_),
        kurse: Some(do_),
        kurse_nyse: None,
        ..Erledigt::default()
    };
    assert_eq!(faellig_x(p, &beide, do_, (23, 59)), None);
    assert_eq!(faellig_x(p, &leer, sa, (18, 0)), None);
    let aus = Zeitplan { an: false, ..p };
    assert_eq!(faellig_x(aus, &leer, do_, (18, 0)), None);
    assert_eq!(naechster_x(aus, &leer, do_), None);
    assert_eq!(
        naechster_x(p, &beide, do_),
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
        ..Erledigt::default()
    };
    assert_eq!(
        naechster_x(
            p,
            &freitag_fertig,
            some(Datum::neu(2026, 11, 6), "Freitag")?
        ),
        Some((Art::Termine, mo, (17, 40)))
    );
    Ok(())
}

/// Die festgelegte Liste hat Werte von Xetra und NYSE: Der Zeitplan ruft beide ab, die NYSE
/// nach ihrem Schluss, und kennt keinen weiteren Lauf, wenn beide erledigt sind.
#[test]
fn zeitplan_der_liste_ruft_xetra_und_nyse_ab() -> Result<(), String> {
    assert_eq!(
        depot_kern::liste::boersen(),
        vec![Boerse::Xetra, Boerse::Nyse]
    );
    let p = Zeitplan {
        an: true,
        ..abendplan()
    };
    let do_ = some(Datum::neu(2026, 11, 5), "Donnerstag")?;
    let xetra = Erledigt {
        termine: Some(do_),
        kurse: Some(do_),
        ..Erledigt::default()
    };
    assert_eq!(faellig(&p, &xetra, do_, (22, 9)), None);
    assert_eq!(
        faellig(&p, &xetra, do_, (22, 10)),
        Some(Art::KurseVon(Boerse::Nyse))
    );
    assert_eq!(
        naechster(&p, &xetra, do_),
        Some((Art::KurseVon(Boerse::Nyse), do_, (22, 10)))
    );
    let alle = Erledigt {
        kurse_nyse: Some(do_),
        ..xetra
    };
    assert_eq!(faellig(&p, &alle, do_, (23, 59)), None);
    Ok(())
}

/// An Feiertagen aller Boersen (hier nur Xetra) laeuft nichts; der naechste Lauf ist der naechste Tag,
/// an dem gehandelt wird.
#[test]
fn zeitplan_ruht_an_boersenfeiertagen() -> Result<(), String> {
    let p = Zeitplan {
        an: true,
        ..abendplan()
    };
    let leer = Erledigt::default();
    let heiligabend = some(Datum::neu(2026, 12, 24), "Heiligabend")?;
    assert_eq!(faellig_x(p, &leer, heiligabend, (18, 0)), None);
    let karfreitag = some(Datum::neu(2026, 4, 3), "Karfreitag")?;
    assert_eq!(faellig_x(p, &leer, karfreitag, (18, 0)), None);
    let d23 = some(Datum::neu(2026, 12, 23), "23.12.")?;
    let fertig = Erledigt {
        termine: Some(d23),
        kurse: Some(d23),
        kurse_nyse: None,
        ..Erledigt::default()
    };
    assert_eq!(
        naechster_x(p, &fertig, d23),
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
    let detail = format!("<details id=\"protokoll-{}-{nr}\"", t.sekunden());
    assert!(get(&d, "/teil/live", t).contains(&format!("{detail} open>")));
    d.verfolge(nr, kind, || donnerstag(12, 1).ok());
    let z = d.sperre();
    assert!(z.laufend.is_none());
    let a = some(z.verlauf.front(), "Verlauf")?;
    assert_eq!(a.erfolg, Some(true));
    assert_eq!(a.protokoll, vec!["lauf termine".to_string()]);
    assert_eq!(a.kennung, format!("{}-{nr}", t.sekunden()));
    let seite = seiten::live(TOKEN, "s", &z, t);
    assert!(seite.contains(&format!("{detail}>")));
    assert!(seite.contains("data-protokoll-kopieren>Auszug kopieren"));
    assert!(seite.contains("<pre class=\"konsole\" tabindex=\"0\">lauf termine\n</pre>"));
    drop(z);
    assert!(d.schritt(t).is_none());
    assert!(d.sperre().lage.is_some(), "danach das Lagebild");
    Ok(())
}

#[test]
fn zeitplan_startet_termine_kurse_und_danach_papier() -> Result<(), String> {
    let wurzel = eingeschaltet(ordner()?)?;
    let d = dienst(wurzel.clone(), true);
    d.sperre().boersen = vec![Boerse::Xetra];
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
        "erledigt-v3\ntermine=2026-11-05\nkurse=2026-11-05\nkurse_nyse=\nkurse_ok=2026-11-05\npapier=2026-11-05\n"
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
    let an = formular_post("/zeitplan", &format!("t={TOKEN}&aktion=an"));
    assert!(
        text(&behandle(&d2, &umw, &an, donnerstag(18, 0)?)).contains("m=zeitplan_einrichtung#")
    );
    let seite = get(&d2, "/einrichtung", donnerstag(18, 0)?);
    assert!(seite.contains("<b>angehalten</b>"));
    assert!(seite.contains("disabled>Speichern und Zeitplan einschalten"));
    assert!(!d2.sperre().zeitplan.an);
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
        &format!("t={TOKEN}&aktion=speichern&termine=8:00&kurse=18:00"),
    );
    assert!(text(&behandle(&d, &umw, &ungueltig, t)).contains("m=zeitplan_ungueltig"));
    assert_eq!(d.sperre().zeitplan, Zeitplan::default());
    let a = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&aktion=speichern&termine=08%3A00&kurse=18%3A05"),
    );
    assert!(text(&behandle(&d, &umw, &a, t)).contains("m=zeitplan#zeitplan"));
    let erwartet = Zeitplan {
        an: false,
        termine: (8, 0),
        kurse: (18, 5),
        papier_danach: false,
        nachrichten_an: false,
        vorbereitung: depot_dienst::zeitplan::VorbereitungsPlan::Aus,
        hinweis: depot_dienst::zeitplan::HinweisPlan::Aus,
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
    assert!(seite.contains("Einstellungen gespeichert."));
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
    let aus = formular_post("/autostart", &format!("t={TOKEN}&an=aus"));
    let t = donnerstag(12, 0)?;
    assert!(text(&behandle(&d, &umw, &aus, t)).contains("m=autostart_bestaetigen"));
    assert_eq!(
        *umw.autostart.lock().unwrap_or_else(PoisonError::into_inner),
        Some(true)
    );
    let aus = formular_post("/autostart", &format!("t={TOKEN}&an=aus&geprueft=ja"));
    assert!(text(&behandle(&d, &umw, &aus, t)).contains("m=autostart_aus"));
    assert_eq!(
        *umw.autostart.lock().unwrap_or_else(PoisonError::into_inner),
        Some(false)
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

#[test]
fn erste_schritte_zeigen_bereits_angenommene_zugangsdaten() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let t = donnerstag(12, 0)?;
    let datei = wurzel.join("daten").join(depot_lage::anbieter::DATEI);
    for wer in depot_dienst::schluessel::ALLE {
        let eintrag = depot_lage::anbieter::Eintrag::neu(
            t,
            wer.protokoll(),
            depot_lage::anbieter::Ergebnis::Ok,
            Some(200),
        );
        depot_lage::anbieter::anhaengen(&datei, &eintrag)?;
    }
    let seite = get(&d, "/einrichtung", t);
    assert!(seite.contains("alle Schlüssel wurden vom jeweiligen Anbieter angenommen"));
    assert!(seite.contains("geprüft (angenommen"));
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
            kursquelle_us: Hinterlegt::Ja,
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
        l
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
    let live = |d: &Dienst| seiten::live(TOKEN, "s", &d.sperre(), t);
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
    if let Some(lage) = d.sperre().lage.as_mut() {
        lage.bild.betrieb = depot_lage::Betrieb::OhneKi(vec!["Maus: kein Schlüssel.".into()]);
    }
    let s = live(&d);
    assert!(
        s.contains(">Läuft ohne KI<") && s.contains("Automatik: Zeitplan aus"),
        "{s}"
    );
    assert!(s.contains("Maus: kein Schlüssel."), "{s}");
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
    let s = seiten::status(TOKEN, "s", &d.sperre(), heute);
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
    assert_eq!(
        s.matches("id=\"ki-verbindung\"").count(),
        1,
        "Aktive Sperre: keine doppelten Abschnitte"
    );
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
    assert!(r.contains("m=kostenschutz_dialog_unbestaetigt"), "{r}");
    assert!(halt.exists());
    let umw = Attrappe {
        sperrfreigabe: true,
        ..umw
    };

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
    let werkzeuge = s.find("id=\"werkzeuge\"").ok_or("Werkzeuge fehlen")?;
    let frei = s
        .find("id=\"kostenschutz\"")
        .ok_or("Rücksprung nach Aufheben fehlt")?;
    assert!(frei > werkzeuge && s.contains("Keine Sperre"), "{s}");
    assert_eq!(
        s.matches("id=\"ki-verbindung\"").count(),
        1,
        "Freie Sperre: keine doppelten Abschnitte"
    );
    assert!(
        s.contains("id=\"ki-verbindung\""),
        "Die Erklärung bleibt unter Werkzeuge erreichbar: {s}"
    );
    Ok(())
}

#[test]
fn bestaetigter_dialog_hebt_keine_inzwischen_geaenderte_sperre_auf() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let halt = kostenschutz::pfad(&wurzel);
    fs::write(&halt, "Alte Sperre").map_err(|e| e.to_string())?;
    let umw = Attrappe {
        sperrfreigabe: true,
        sperrdatei_wechsel: Some(halt.clone()),
        ..Attrappe::default()
    };
    let post = aufheben(&kostenschutz::abdruck("Alte Sperre"), true);
    let r = text(&behandle(&d, &umw, &post, donnerstag(18, 0)?));
    assert!(r.contains("m=kostenschutz_geaendert"), "{r}");
    assert_eq!(
        fs::read_to_string(halt).map_err(|e| e.to_string())?,
        "Neue Sperre"
    );
    assert!(!kostenschutz::protokoll_pfad(&wurzel).exists());
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
        s.contains("id=\"kurssperre\""),
        "Rücksprung bleibt ohne Sperre vorhanden"
    );
    assert!(
        !s.contains("action=\"/kurssperre\""),
        "Keine Aufhebung ohne Sperre"
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
    assert!(r.contains("m=kurssperre_dialog_unbestaetigt"), "{r}");
    assert!(halt.exists());
    let umw = Attrappe {
        sperrfreigabe: true,
        ..umw
    };

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
fn live_zeitstrahl_zeigt_boersenzeiten_und_bedingten_papiertag() -> Result<(), String> {
    let d = dienst(ordner()?, false);
    let t = donnerstag(17, 39)?;
    let tag = some(t.datum(), "Tag des Zeitplans")?;
    let html = get(&d, "/teil/live", t);
    assert!(!html.contains("<h2>Zeitstrahl</h2><p>Zeitplan aus"));
    assert_eq!(get(&d, "/", t).matches("Zeitplan aus.").count(), 1);
    {
        let mut z = d.sperre();
        z.boersen = vec![Boerse::Xetra, Boerse::Nyse];
        z.zeitplan.an = true;
        z.zeitplan.termine = (17, 40);
        z.zeitplan.kurse = (17, 51);
        z.zeitplan.papier_danach = true;
        z.zeitplan.nachrichten_an = true;
    }
    let html = get(&d, "/teil/live", t);
    let termine = some(html.find("<b>17:40 Uhr</b> · Termine"), "Terminabruf")?;
    let xetra = some(html.find("<b>17:51 Uhr</b> · Kurse Xetra"), "Xetra-Abruf")?;
    let nyse = some(html.find("<b>22:10 Uhr</b> · Kurse US-Börsen"), "US-Abruf")?;
    assert!(termine < xetra && xetra < nyse);
    assert!(html.contains("Kurse Xetra abrufen · ohne KI"));
    assert!(html.contains(&format!("Dachs ({})", depot_kern::modelle::MODELL_DACHS)));
    assert!(html.contains(&format!("Maus ({})", depot_kern::modelle::MODELL_MAUS)));
    assert!(html.contains("nach dem letzten Kursabruf, wenn mindestens einer erfolgreich war"));
    assert!(html.contains("täglich von 08:00 bis vor 23:00 Uhr"));
    assert!(!html.contains("danach optional kostenlose Gemini-Zuarbeit"));
    let schalter = depot_lage::gespraeche::gemini_pfad(&d.wurzel.join("daten"));
    fs::write(schalter, "gratisprojekt=ja\n").map_err(|e| e.to_string())?;
    assert!(get(&d, "/teil/live", t).contains("danach optional kostenlose Gemini-Zuarbeit"));
    assert!(get(&d, "/einrichtung", t).contains("Beim Abendplan wird jede andere Börse"));

    {
        let mut z = d.sperre();
        z.erledigt.kurse = Some(tag);
        z.erledigt.kurse_nyse = Some(tag);
    }
    let html = get(&d, "/teil/live", donnerstag(22, 20)?);
    assert!(html.contains("entfällt: kein erfolgreicher Kursabruf"));
    d.sperre().erledigt.termine = Some(tag);
    let html = get(&d, "/teil/live", donnerstag(22, 20)?);
    assert!(html.contains("Nächster Börsentag · Fr 06.11.2026"));
    d.sperre().zeitplan.kurse = (7, 30);
    let einrichtung = get(&d, "/einrichtung", t);
    assert!(einrichtung.contains("Im Morgenlauf gilt der letzte abgeschlossene Handelstag"));
    assert!(!einrichtung.contains("Beim Abendplan wird jede andere Börse"));

    Ok(())
}

#[test]
fn navigation_markiert_aktuelle_seite_und_zeigt_laufenden_stand() -> Result<(), String> {
    let mut d = Dienst::neu(
        ordner()?,
        TOKEN.into(),
        echo_starter(true),
        leeres_lagebild(),
    );
    let t = donnerstag(12, 0)?;
    assert!(get(&d, "/", t).contains("Commit unbekannt"));
    let sha = "a".repeat(40);
    d.version = Some(sha.clone());
    for (pfad, titel) in [("/", "Live"), ("/einrichtung", "Einrichtung")] {
        let html = get(&d, pfad, t);
        assert_eq!(html.matches("aria-current=\"page\"").count(), 1);
        assert!(html.contains(&format!(
            "<a href=\"{pfad}\" aria-current=\"page\">{titel}</a>"
        )));
        assert!(html.contains(&format!("title=\"{sha}\">Version aaaaaaaaaa</span>")));
    }
    Ok(())
}

#[test]
fn papierfreigabe_verlangt_bestaetigung_und_journalabdruck() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let umw = Attrappe::default();
    let t = donnerstag(12, 0)?;
    let post = |f: &str| {
        let a = formular_post("/papierfreigabe", &format!("t={TOKEN}&{f}"));
        text(&behandle(&d, &umw, &a, t))
    };
    assert!(post("").contains("papierfreigabe_unbestaetigt"));
    assert!(post("geprueft=ja&abdruck=ungültig").starts_with("HTTP/1.1 400"));
    assert!(!get(&d, "/einrichtung", t).contains("Papierhandel nach Verlustsperre freigeben"));
    Ok(())
}

#[test]
fn testdialog_ist_ohne_sperre_verfuegbar_und_schreibt_nichts() -> Result<(), String> {
    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    let d = dienst(wurzel, true);
    let t = donnerstag(12, 0)?;
    assert!(get(&d, "/einrichtung", t).contains("Freigabedialog testen"));
    for (name, inhalt) in [
        ("journal.txt", "Originaljournal"),
        ("journal.vermerk", "Originalvermerk"),
        ("papierkonto.txt", "Originalkonto"),
    ] {
        fs::write(daten.join(name), inhalt).map_err(|e| e.to_string())?;
    }
    let a = formular_post("/papierdialog-test", &format!("t={TOKEN}"));
    assert!(text(&behandle(&d, &Attrappe::default(), &a, t)).contains("dialogtest_ja"));
    for (name, inhalt) in [
        ("journal.txt", "Originaljournal"),
        ("journal.vermerk", "Originalvermerk"),
        ("papierkonto.txt", "Originalkonto"),
    ] {
        assert_eq!(
            fs::read(daten.join(name)).map_err(|e| e.to_string())?,
            inhalt.as_bytes()
        );
    }
    Ok(())
}

#[test]
fn papierfreigabe_ueber_einrichtung_schreibt_bestaetigten_stand() -> Result<(), String> {
    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    // Kanonisches Journal mit zwei Papier-Toepfen, Gesamtverlustsperre und Tagesabschluss.
    for (name, inhalt) in [
        (
            "journal.txt",
            concat!(
                "10;277cd26269b17f0a01f2d37944a7b625b839c8e433f0a1c7b8de229fc619ca67;journal-v2\n",
                "14;7b07d15e2cdbee2e29d32e6c3aaf5b49ff371bee867e3cf54d435e3ded8b6b03;topf;A;500000;\n",
                "14;fe147cc4e9a53116352cd71f6356db399e4320dd5ed3e60f74189c928f5b6871;topf;B;500000;\n",
                "25;284202d38e87a7d710bea9c848eeb6f46ddd11bccfa81fa08cafc800d72726b2;abrechnungstag;2026-11-04\n",
                "36;ecfa1ce0c24ac8bed0ccce2b440f8bf8c4c3454d579131113d54498cf7bc55fc;papier-halt;2026-11-04;gesamtverlust\n",
                "93;5e56b0f77370678f32322b1fe88395cad52a46b2a9bc91369339af7092cd38e4;abschluss;2026-11-04;1500000;aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
            ),
        ),
        (
            "journal.vermerk",
            "vermerk-v1;606;5e56b0f77370678f32322b1fe88395cad52a46b2a9bc91369339af7092cd38e4;0\n",
        ),
        (
            "papierkonto.txt",
            concat!("papierkonto-v1\n", "bargeld;1000000\n", "sendungen;0\n",),
        ),
    ] {
        fs::write(daten.join(name), inhalt).map_err(|e| e.to_string())?;
    }
    let d = dienst(wurzel.clone(), true);
    let t = donnerstag(12, 0)?;
    assert!(get(&d, "/einrichtung", t).contains("33,33 %"));
    let angebot = some(
        depot_lage::papier_freigabe_angebot(&wurzel),
        "Freigabeangebot",
    )?;
    let umw = Attrappe {
        papierfreigabe: true,
        ..Attrappe::default()
    };
    let formular = |abdruck: &str| {
        formular_post(
            "/papierfreigabe",
            &format!("t={TOKEN}&geprueft=ja&abdruck={abdruck}"),
        )
    };
    let r = |a: &Anfrage, umwelt: &Attrappe| text(&behandle(&d, umwelt, a, t));
    assert!(r(&formular(&"a".repeat(64)), &umw).contains("papierfreigabe_geaendert"));
    let anfrage = formular(&angebot.abdruck);
    assert!(r(&anfrage, &Attrappe::default()).contains("papierfreigabe_unbestaetigt"));
    let antwort = r(&anfrage, &umw);
    assert!(antwort.contains("papierfreigabe_ok"), "{antwort}");
    assert!(depot_lage::papier_freigabe_angebot(&wurzel).is_none());
    let journal = fs::read_to_string(daten.join("journal.txt")).map_err(|e| e.to_string())?;
    assert!(journal.contains(concat!(
        "papier-freigabe;2026-11-04;1000000;einrichtung;dialog;",
        "2026-11-05T11:01:23Z"
    )));
    assert!(depot_lage::mit_journal(&daten, |zugang| {
        let Zugang::Offen(stand) = zugang else {
            return false;
        };
        stand.begruendungen().iter().any(|(_, wer, text)| {
            wer == "Einrichtung, Dialog" && text.contains("2026-11-05T11:01:23Z")
        })
    }));
    Ok(())
}

#[test]
fn zeitplan_einschalten_erst_nach_vollstaendiger_einrichtung() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let umw = Attrappe::default();
    let t = donnerstag(12, 0)?;
    let an = formular_post("/zeitplan", &format!("t={TOKEN}&aktion=an"));
    // Noch kein Lagebild: nicht eingeschaltet.
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
    // Die Seite erklärt, warum der Einschaltknopf gesperrt ist.
    let seite = get(&d, &ort(&r), t);
    assert!(seite.contains("Zeitplan nicht eingeschaltet"), "{seite}");
    assert!(
        seite.contains("disabled>Speichern und Zeitplan einschalten"),
        "{seite}"
    );
    assert!(seite.contains("Einschalten geht erst"), "{seite}");
    // Ohne Entwurf zeigt die Seite den aktiven Plan.
    let seite = get(&d, "/einrichtung", t);
    assert!(seite.contains("value=\"07:20\""), "{seite}");
    // Uhrzeiten aendern ohne Einschalten geht immer.
    let aus = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&aktion=speichern&termine=08%3A15&kurse=18%3A05"),
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
    assert!(text(&behandle(&fertig, &umw, &an, t)).contains("m=zeitplan_an#"));
    assert!(fertig.sperre().zeitplan.an);
    assert!(text(&behandle(&fertig, &umw, &aus, t)).contains("m=zeitplan#"));
    assert!(fertig.sperre().zeitplan.an, "Speichern schaltet nicht aus");
    let altes_formular = formular_post("/zeitplan", &format!("t={TOKEN}&an=ja"));
    assert!(text(&behandle(&fertig, &umw, &altes_formular, t)).contains("m=zeitplan_neu_laden"));
    let ausschalten = formular_post("/zeitplan", &format!("t={TOKEN}&aktion=aus"));
    assert!(text(&behandle(&fertig, &umw, &ausschalten, t)).contains("m=zeitplan_aus#"));
    let plan = fertig.sperre().zeitplan;
    assert!(!plan.an);
    assert_eq!(plan.termine, (8, 15));
    assert_eq!(plan.kurse, (18, 5));
    Ok(())
}

#[test]
fn zeitplanformular_bewahrt_entwurf_bei_schalterwechsel_im_anderen_tab() -> Result<(), String> {
    let d = Dienst::neu(
        ordner()?,
        TOKEN.into(),
        echo_starter(true),
        lagebild_mit_schluesseln(true),
    );
    let t = donnerstag(12, 0)?;
    assert!(d.schritt(t).is_none());
    assert!(get(&d, "/einrichtung", t).contains("name=\"planstand\" value=\"aus\""));
    let umw = Attrappe::default();
    let an = formular_post("/zeitplan", &format!("t={TOKEN}&aktion=an"));
    assert!(text(&behandle(&d, &umw, &an, t)).contains("m=zeitplan_an"));
    let vorher = d.sperre().zeitplan;
    let speichern = |stand| {
        let post = formular_post(
            "/zeitplan",
            &format!("t={TOKEN}&aktion=speichern&planstand={stand}&termine=08%3A15&kurse=18%3A05"),
        );
        text(&behandle(&d, &umw, &post, t))
    };
    let antwort = speichern("aus");
    assert!(antwort.contains("m=zeitplan_schalter_geaendert"));
    assert_eq!(d.sperre().zeitplan, vorher);
    let seite = get(&d, &ort(&antwort), t);
    assert!(seite.contains("name=\"planstand\" value=\"an\""));
    assert!(seite.contains("Der Zeitplan bleibt eingeschaltet."));
    assert!(!seite.contains("Nur speichern"));
    assert!(seite.contains("value=\"18:05\""));
    assert!(seite.contains("Deine Eingaben stehen als Entwurf"));
    assert!(speichern("an").contains("m=zeitplan#"));
    let nachher = d.sperre().zeitplan;
    assert!(nachher.an);
    assert_eq!(nachher.kurse, (18, 5));
    Ok(())
}

#[test]
fn speichern_und_einschalten_uebernimmt_genau_die_ausgefuellten_werte() -> Result<(), String> {
    let w = ordner()?;
    let d = Arc::new(Dienst::neu(
        w.clone(),
        TOKEN.into(),
        echo_starter(true),
        lagebild_mit_schluesseln(true),
    ));
    let t = donnerstag(12, 0)?;
    assert!(d.schritt(t).is_none());
    let sende = |werte: &str| {
        text(&behandle(
            &d,
            &Attrappe::default(),
            &formular_post(
                "/zeitplan",
                &format!("t={TOKEN}&aktion=speichern_an&{werte}"),
            ),
            t,
        ))
    };
    let r = sende("termine=08%3A15&kurse=18%3A05&nachrichten_an=ja");
    assert!(r.contains("m=zeitplan_gespeichert_an"), "{r}");
    let plan = d.sperre().zeitplan;
    assert!(plan.an && plan.nachrichten_an);
    assert_eq!(plan.termine, (8, 15));
    assert_eq!(plan.kurse, (18, 5));
    assert_eq!(
        Zeitplan::lies(
            &fs::read_to_string(w.join("daten/zeitplan.txt")).map_err(|e| e.to_string())?
        ),
        Ok(plan)
    );
    let seite = get(&d, &ort(&r), t);
    assert!(seite.contains("Einstellungen gespeichert und Zeitplan eingeschaltet"));
    // Ungültig heißt weder speichern noch einschalten; die bisherigen Werte gelten weiter.
    let r = sende("termine=ungueltig&kurse=19%3A05");
    assert!(r.contains("m=zeitplan_ungueltig"));
    assert_eq!(d.sperre().zeitplan, plan);
    let html = get(&d, &ort(&r), t);
    assert!(html.contains("value=\"19:05\""));
    assert!(html.contains("data-entwurf=\"ja\""));
    assert!(html.contains("href=\"#zeitplan\">Zeitplan und Eingaben ansehen"));
    Ok(())
}

#[test]
fn kombiniertes_einschalten_bleibt_bei_fehlender_einrichtung_ohne_wirkung() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let jetzt = donnerstag(12, 0)?;
    assert!(d.schritt(jetzt).is_none());
    let formular = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&aktion=speichern_an&termine=08%3A15&kurse=18%3A05"),
    );
    let antwort = text(&behandle(&d, &Attrappe::default(), &formular, jetzt));
    assert!(antwort.contains("m=zeitplan_einrichtung"));
    assert!(!d.sperre().zeitplan.an);
    assert!(!wurzel.join("daten/zeitplan.txt").exists());
    let html = get(&d, &ort(&antwort), jetzt);
    assert!(html.contains("value=\"18:05\""));
    assert!(html.contains("disabled>Speichern und Zeitplan einschalten"));
    assert!(html.contains("Du kannst die Einstellungen schon speichern."));
    assert!(!html.contains("wähle unten <b>Speichern und Zeitplan einschalten</b>"));
    Ok(())
}

#[test]
fn quellenbereich_zeigt_den_tatsaechlichen_abruf_und_sein_ergebnis() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(12, 0)?;
    assert!(get(&d, "/einrichtung", t).contains("data-teil=\"/teil/quellen\""));
    assert!(get(&d, "/teil/journal", t).contains("data-start="));
    let quellen = || get(&d, "/teil/quellen", t);
    assert!(quellen().contains("data-start="));
    assert!(quellen().contains("Kein Abruf im Verlauf"));
    for beschriftung in ["Kurse abrufen", "Termine abrufen", "Schlagzeilen abrufen"] {
        assert!(quellen().contains(&format!(">{beschriftung}</button>")));
    }
    let xetra = Art::KurseVon(Boerse::Xetra);
    d.einreihen(xetra, Anlass::Hand)?;
    assert!(quellen().contains("Kurse Xetra abrufen · wartet"));
    let (nr, kind) = some(d.schritt(t), "Kursabruf")?;
    assert!(quellen().contains("Kurse Xetra abrufen · läuft"));
    d.verfolge(nr, kind, || donnerstag(12, 0).ok());
    assert!(quellen().contains("Xetra · <span class=\"gut\">erfolgreich</span>"));
    assert!(!quellen().contains("Kurse Xetra abrufen · läuft"));
    Ok(())
}

#[test]
fn ungueltige_eingabe_laesst_die_gueltigen_stehen() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(12, 0)?;
    let f = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&aktion=speichern&termine=8%3A00&kurse=18%3A20&papier_danach=ja"),
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
fn einrichten_bleibt_auf_der_config_seite_und_zeigt_den_arbeitsstand() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(12, 0)?;
    assert!(d.schritt(t).is_none());
    let html = get(&d, "/einrichtung", t);
    let pos = |anker| {
        html.find(anker)
            .ok_or_else(|| format!("Abschnitt fehlt: {anker}"))
    };
    assert!(pos("id=\"journal\"")? < pos("id=\"schluessel\"")?);
    assert!(pos("id=\"erste-schritte\"")? < pos("id=\"quellen\"")?);
    assert!(html.contains("href=\"#journal\">Journal einrichten"));
    assert!(pos("id=\"quellen\"")? < pos("id=\"zeitplan\"")?);
    assert!(pos("id=\"zeitplan\"")? < pos("id=\"update\"")?);
    assert!(pos("id=\"update\"")? < pos("id=\"selten\"")?);
    assert!(pos("id=\"schluessel\"")? < pos("id=\"quellen\"")?);
    assert!(pos("id=\"schluessel\"")? < pos("id=\"autostart\"")?);
    assert!(pos("id=\"autostart\"")? < pos("id=\"werkzeuge\"")?);
    assert!(html.contains("name=\"ziel\" value=\"einrichtung\""));
    for anker in [
        "kostenschutz",
        "kurssperre",
        "papierfreigabe",
        "ki-verbindung",
    ] {
        assert_eq!(
            html.matches(&format!("id=\"{anker}\"")).count(),
            1,
            "Rücksprung fehlt oder ist doppelt: {anker}"
        );
    }
    let post = formular_post(
        "/ausfuehren",
        &format!("t={TOKEN}&art=einrichten&ziel=einrichtung"),
    );
    let antwort = text(&behandle(&d, &Attrappe::default(), &post, t));
    assert!(
        antwort.contains("/einrichtung?m=auftrag#journal"),
        "{antwort}"
    );
    assert_eq!(wartend(&d), vec![(Art::Einrichten, Anlass::Hand)]);
    let html = get(&d, "/teil/journal", t);
    assert!(
        html.contains("disabled>Journal einrichten · wartet"),
        "{html}"
    );
    assert!(html.contains("Bitte warte auf den Abschluss."), "{html}");
    let antwort = text(&behandle(&d, &Attrappe::default(), &post, t));
    assert!(
        antwort.contains("/einrichtung?m=auftrag_belegt#journal"),
        "{antwort}"
    );
    assert_eq!(wartend(&d).len(), 1);
    // Ein frei eingesandtes Ziel wird nicht als Umleitungsadresse uebernommen.
    let fremd = formular_post(
        "/ausfuehren",
        &format!("t={TOKEN}&art=einrichten&ziel=https://fremd.test"),
    );
    let antwort = text(&behandle(&d, &Attrappe::default(), &fremd, t));
    assert!(!antwort.contains("fremd.test"), "{antwort}");
    Ok(())
}

#[test]
fn journal_knopf_zeigt_arbeitsstand() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(12, 0)?;
    assert!(d.schritt(t).is_none());
    assert!(get(&d, "/", t).contains(">Journal einrichten</button>"));
    assert_eq!(
        get(&d, "/teil/live", t)
            .matches("Journal: Noch nicht eingerichtet.")
            .count(),
        1
    );
    d.einreihen(Art::Einrichten, Anlass::Hand)?;
    assert!(get(&d, "/teil/live", t).contains("disabled>Journal einrichten · wartet"));
    let (nr, kind) = some(d.schritt(t), "Start")?;
    let live = get(&d, "/teil/live", t);
    assert!(live.contains("disabled>Journal einrichten · läuft"));
    assert!(live.contains("Der Lauf wird vorbereitet"));
    d.verfolge(nr, kind, || donnerstag(12, 0).ok());
    Ok(())
}

#[test]
fn doppelklick_startet_nichts_doppelt() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let umw = Attrappe::default();
    let t = donnerstag(12, 0)?;
    let bestaetigung = get(&d, "/bestaetigen?art=papier", t);
    assert!(bestaetigung.contains("echtes KI-Budget") && bestaetigung.contains("Abbrechen"));
    assert!(
        wartend(&d).is_empty(),
        "Öffnen der Bestätigung startet nichts"
    );
    let ohne = formular_post("/ausfuehren", &format!("t={TOKEN}&art=papier"));
    assert!(text(&behandle(&d, &umw, &ohne, t)).contains("m=papier_bestaetigen"));
    assert!(wartend(&d).is_empty());
    let klick = formular_post("/ausfuehren", &format!("t={TOKEN}&art=papier&geprueft=ja"));
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
    let seite = seiten::live(TOKEN, "s", &z, t);
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
            kursquelle_us: Hinterlegt::Ja,
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
        l
    });
    let d = Arc::new(Dienst::neu(
        ordner()?,
        TOKEN.into(),
        echo_starter(true),
        ohne_maus,
    ));
    let t = donnerstag(12, 0)?;
    assert!(d.schritt(t).is_none());
    let an = formular_post("/zeitplan", &format!("t={TOKEN}&aktion=an"));
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
    d.sperre().zeitplan.an = true;
    let t = donnerstag(12, 0)?;
    d.einreihen(Art::Termine, Anlass::Hand)
        .map_err(|e| format!("Vorbedingung: {e}"))?;
    assert!(d.schritt(t).is_none(), "nichts gestartet");
    let z = d.sperre();
    assert!(z.laufend.is_none());
    let a = some(z.verlauf.front(), "Verlauf")?;
    assert_eq!(a.erfolg, Some(false));
    assert!(a.protokoll.iter().any(|p| p.starts_with("Nicht gestartet")));
    let seite = seiten::live(TOKEN, "s", &z, t);
    drop(z);
    assert!(seite.contains("Verlauf nicht schreibbar"), "{seite}");
    assert!(seite.contains("Zeitplan angehalten"));
    assert!(!seite.contains("Nächster geplanter Lauf"));
    assert!(get(&d, "/einrichtung", t).contains("<b>angehalten</b>"));
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

#[test]
fn fehlender_abschlussvermerk_zeigt_hinweis_und_haelt_zeitplan_nicht_an() -> Result<(), String> {
    let wurzel = ordner()?;
    let datei = depot_dienst::verlauf::pfad(&wurzel);
    let sicherung = datei.with_extension("test-sicherung");
    let d = dienst(wurzel, true);
    d.sperre().zeitplan.an = true;
    let t = donnerstag(12, 0)?;
    d.einreihen(Art::Termine, Anlass::Hand)?;
    let (nr, kind) = some(d.schritt(t), "Terminabruf startet")?;
    fs::rename(&datei, &sicherung).map_err(|e| e.to_string())?;
    fs::create_dir(&datei).map_err(|e| e.to_string())?;
    d.verfolge(nr, kind, || donnerstag(12, 1).ok());

    let seite = get(&d, "/teil/live", t);
    assert!(seite.contains("nicht im Verlauf vermerkt"));
    assert!(seite.contains("Geplante Startzeiten in Frankfurter Ortszeit"));
    assert!(!seite.contains("Zeitplan angehalten"));
    assert!(get(&d, "/einrichtung", t).contains("<b>an</b>"));

    fs::remove_dir(&datei).map_err(|e| e.to_string())?;
    fs::rename(&sicherung, &datei).map_err(|e| e.to_string())?;
    d.einreihen(Art::Kurse, Anlass::Hand)?;
    let (nr, kind) = some(d.schritt(t), "weiterer Abruf startet")?;
    assert!(d.sperre().verlauf_vermerkfehler.is_none());
    d.verfolge(nr, kind, || donnerstag(12, 2).ok());
    Ok(())
}

#[test]
fn angehaltene_automatik_zeigt_wartenden_handauftrag_und_rot_im_zeitstrahl() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(12, 0)?;
    {
        let mut z = d.sperre();
        z.zeitplan.an = true;
        z.erledigt_fehler = Some("Laufstand unlesbar".into());
    }
    d.einreihen(Art::Termine, Anlass::Hand)?;
    let html = get(&d, "/teil/live", t);
    assert!(html.contains("Als Nächstes: Termine abrufen (von Hand)"));
    assert!(html.contains(r#"<h2>Zeitstrahl</h2><p class="halt">Zeitplan angehalten"#));
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
    d.einreihen(Art::Kurse, Anlass::Hand)
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
    let seite = seiten::live(TOKEN, "s", &z, t);
    assert!(
        seite.contains(
            "<p class=\"kopf halt\" id=\"hinweis\">Kurse abrufen: Papiertag heute ausgefallen."
        ),
        "{seite}"
    );
    // Am naechsten Tag steht der alte Hinweis nicht mehr oben, nur im Verlauf.
    let morgen = some(t.datum().and_then(|d| d.plus_tage(1)), "morgen")?;
    let morgen = some(Zeitpunkt::frankfurt(morgen, 12, 0, 0), "Zeit morgen")?;
    let seite = seiten::live(TOKEN, "s", &z, morgen);
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

/// Maus, Befund 3 zu #38: Ob es nach einem Ausfall von selbst weitergeht, sagt der Dienst aus
/// seinem tatsaechlichen Zeitplan, nicht der Sammler. Ohne Zeitplan: nichts von selbst; mit
/// Zeitplan: der naechste geplante Lauf. Bei unlesbarem Erledigt-Stand bleibt der Zeitplan
/// auch nach dem Einschalten angehalten.
#[test]
fn nach_einem_ausfall_sagt_der_zeitplan_wie_es_weitergeht() -> Result<(), String> {
    let zeilen: &'static [&'static str] =
        &["[Hinweis] Papiertag heute ausgefallen. Die Verbindung kam bis 17:54 Uhr nicht wieder."];
    let t = donnerstag(17, 45)?;
    for (an, angehalten) in [(false, false), (true, false), (true, true)] {
        let wurzel = if an {
            eingeschaltet(ordner()?)?
        } else {
            ordner()?
        };
        if angehalten {
            fs::write(
                wurzel.join("daten").join("dienst_erledigt.txt"),
                b"unlesbar",
            )
            .map_err(|e| format!("Vorbedingung Erledigt-Stand: {e}"))?;
        }
        let d = Arc::new(Dienst::neu(
            wurzel,
            TOKEN.into(),
            zeilen_starter(zeilen),
            lagebild_mit_schluesseln(true),
        ));
        if angehalten {
            assert!(d.schritt(t).is_none());
            // Simuliert einen bereits aktiven Zeitplan mit beschädigtem Laufstand.
            d.sperre().zeitplan.an = true;
        }
        d.einreihen(Art::Kurse, Anlass::Hand)
            .map_err(|e| format!("Vorbedingung einreihen: {e}"))?;
        let (nr, kind) = some(d.schritt(t), "Start")?;
        d.verfolge(nr, kind, || donnerstag(17, 55).ok());
        let seite = seiten::live(TOKEN, "s", &d.sperre(), t);
        let (_, rest) = some(seite.split_once("id=\"hinweis\">"), "Hinweis")?;
        let (hinweis, _) = some(rest.split_once("</p>"), "Ende")?;
        if angehalten {
            assert!(hinweis.contains("Zeitplan angehalten"), "{hinweis}");
            assert!(hinweis.contains("Dienst neu starten"), "{hinweis}");
            assert!(!hinweis.contains("geht es von selbst"), "{hinweis}");
        } else if an {
            assert!(
                hinweis.contains("Nach Zeitplan geht es von selbst weiter"),
                "{hinweis}"
            );
        } else {
            assert!(
                hinweis.contains("DepotSchlopf startet nichts von selbst"),
                "{hinweis}"
            );
            assert!(!hinweis.contains("geht es von selbst"), "{hinweis}");
        }
    }
    Ok(())
}

/// Kurse je Boerse: Xetra zur eingestellten Uhrzeit, die NYSE zehn Minuten nach ihrem eigenen
/// Schluss in Frankfurter Zeit, mit Sommerzeit und verkuerzten Tagen. An einem Tag, an dem nur
/// eine Boerse handelt, nur diese. Der letzte Abruf des Tages bestimmt den Papiertag.
#[test]
fn kurse_je_boerse_nach_ihrem_schluss() -> Result<(), String> {
    let p = Zeitplan {
        an: true,
        ..abendplan()
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
            ..Erledigt::default()
        }
    );
    assert_eq!(
        alt.als_text(),
        "erledigt-v3\ntermine=2026-11-05\nkurse=2026-11-05\nkurse_nyse=\nkurse_ok=\npapier=\n"
    );
    assert!(Erledigt::lies("erledigt-v1\ntermine=\nkurse=\nkurse_nyse=\n").is_err());
    assert!(Erledigt::lies("erledigt-v2\ntermine=\nkurse=\n").is_err());
    // Aus der Form ohne Erfolg folgt kein Papiertag: kurse_ok bleibt leer.
    let v2 = Erledigt::lies("erledigt-v2\ntermine=\nkurse=2026-11-05\nkurse_nyse=\n")?;
    assert_eq!(v2.kurse_ok, None);
    assert!(Erledigt::lies("erledigt-v3\ntermine=\nkurse=\nkurse_nyse=\nkurse_ok=\n").is_err());
    Ok(())
}

/// Mit zwei Boersen: Der Papiertag folgt erst dem letzten Abruf des Tages (NYSE), und nur, wenn
/// ein Abruf dieses Tages erfolgreich war. Ein gescheiterter Xetra-Abruf haelt ihn nicht auf,
/// wenn die NYSE gelingt, und umgekehrt.
#[test]
fn papiertag_nach_dem_letzten_abruf_des_tages() -> Result<(), String> {
    for (xetra_ok, nyse_ok, zeitplan_an, papier) in [
        (true, true, true, true),
        (false, true, true, true),
        (true, false, true, true),
        (false, false, true, false),
        (true, true, false, false),
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
        if !zeitplan_an {
            d.setze_zeitplan(Zeitplan {
                an: false,
                ..Zeitplan::default()
            })
            .map_err(|e| format!("Zeitplan ausschalten: {e:?}"))?;
        }
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

/// Ein Dienst, dessen Liste nur Xetra-Werte hat: Der Papiertag folgt dem Xetra-Abruf, gleich
/// welche Boersen die echte Liste hat.
fn nur_xetra(wurzel: PathBuf) -> Arc<Dienst> {
    let d = dienst(wurzel, true);
    d.sperre().boersen = vec![Boerse::Xetra];
    d
}

#[test]
fn zeitplan_holt_kursabruf_und_papiertag_nach_update_probe_nach() -> Result<(), String> {
    let faellig = donnerstag(17, 45)?;
    let spaeter = donnerstag(17, 47)?;
    let tag = some(faellig.datum(), "Handelstag")?;
    for (kurs_vorher, erwartet) in [
        (None, Art::KurseVon(Boerse::Xetra)),
        (Some(tag), Art::Papier),
    ] {
        let wurzel = eingeschaltet(ordner()?)?;
        let erledigt = Erledigt {
            termine: Some(tag),
            kurse: kurs_vorher,
            kurse_ok: kurs_vorher,
            ..Erledigt::default()
        };
        let datei = wurzel.join("daten/dienst_erledigt.txt");
        fs::write(&datei, erledigt.als_text()).map_err(|e| e.to_string())?;
        let offen = depot_dienst::rueckfall::offen_pfad(&wurzel);
        fs::write(&offen, "Update auf Probe").map_err(|e| e.to_string())?;
        let d = nur_xetra(wurzel);
        assert!(d.schritt(faellig).is_none());
        assert!(d.schritt(spaeter).is_none());
        assert!(wartend(&d).is_empty());
        assert_eq!(
            fs::read_to_string(datei).map_err(|e| e.to_string())?,
            erledigt.als_text()
        );

        fs::remove_file(offen).map_err(|e| e.to_string())?;
        let (nr, kind) = some(d.schritt(spaeter), "Fälliger Auftrag nach Bestätigung")?;
        assert_eq!(
            d.sperre().laufend.as_ref().map(|a| (a.art, a.anlass)),
            Some((erwartet, Anlass::Zeitplan))
        );
        d.verfolge(nr, kind, || donnerstag(17, 48).ok());
    }
    Ok(())
}

/// Die Live-Ansicht nach einem Neustart: der Satz mit `id="neustart"`.
fn neustart_satz(d: &Dienst, t: Zeitpunkt) -> Option<String> {
    let seite = seiten::live(TOKEN, "s", &d.sperre(), t);
    let (_, rest) = seite.split_once("id=\"neustart\">")?;
    rest.split_once("</p>").map(|(s, _)| s.to_string())
}

/// Maus' Abnahmefall 2: Windows startet zwischen Kursabruf und Papiertag neu. Der neue Dienst
/// holt den nicht begonnenen Papiertag mit den Kursen von heute nach und sagt das; danach
/// startet nichts mehr. Ohne Neustart steht kein solcher Satz da.
#[test]
fn neustart_zwischen_kursabruf_und_papiertag_holt_ihn_nach() -> Result<(), String> {
    let wurzel = eingeschaltet(ordner()?)?;
    let t = donnerstag(17, 46)?;
    {
        let d = nur_xetra(wurzel.clone());
        let (nr, kind) = some(d.schritt(t), "Termine")?;
        d.verfolge(nr, kind, || donnerstag(17, 46).ok());
        let (nr, kind) = some(d.schritt(t), "Kurse")?;
        d.verfolge(nr, kind, || donnerstag(17, 47).ok());
        assert_eq!(wartend(&d), vec![(Art::Papier, Anlass::Zeitplan)]);
        assert_eq!(neustart_satz(&d, t), None, "kein Neustart");
        // Hier endet der Dienst, bevor der Papiertag beginnt.
    }
    let spaeter = donnerstag(18, 20)?;
    let d = nur_xetra(wurzel.clone());
    let (nr, kind) = some(d.schritt(spaeter), "Papiertag nachgeholt")?;
    assert_eq!(
        d.sperre().laufend.as_ref().map(|a| (a.art, a.anlass)),
        Some((Art::Papier, Anlass::Zeitplan))
    );
    let satz = some(neustart_satz(&d, spaeter), "Satz")?;
    assert!(satz.contains("holt DepotSchlopf den Papiertag"), "{satz}");
    assert!(satz.contains("Du musst nichts tun"), "{satz}");
    d.verfolge(nr, kind, || donnerstag(18, 21).ok());
    let satz = some(neustart_satz(&d, spaeter), "Satz")?;
    assert!(satz.contains("nachgeholt"), "{satz}");
    assert!(d.schritt(spaeter).is_none(), "heute nichts mehr");
    // Ein weiterer Neustart startet ihn nicht ein zweites Mal.
    let wieder = nur_xetra(wurzel);
    assert!(wieder.schritt(donnerstag(19, 0)?).is_none());
    assert!(wartend(&wieder).is_empty());
    Ok(())
}

/// Lief der Papiertag schon, als der Dienst endete, bleibt der heutige Schutz: Kein neuer
/// Start von selbst, und die Ansicht sagt, wo es stehen blieb und wo es weitergeht.
#[test]
fn neustart_waehrend_des_papiertags_startet_ihn_nicht_neu() -> Result<(), String> {
    let wurzel = eingeschaltet(ordner()?)?;
    let t = donnerstag(17, 46)?;
    {
        let d = nur_xetra(wurzel.clone());
        let (nr, kind) = some(d.schritt(t), "Termine")?;
        d.verfolge(nr, kind, || donnerstag(17, 46).ok());
        let (nr, kind) = some(d.schritt(t), "Kurse")?;
        d.verfolge(nr, kind, || donnerstag(17, 47).ok());
        let (_, mut kind) = some(d.schritt(t), "Papier")?;
        let _ = kind.wait();
        // Der Dienst endet, waehrend der Papiertag laeuft: Sein Ende steht nie im Verlauf.
    }
    let spaeter = donnerstag(18, 20)?;
    let d = nur_xetra(wurzel);
    assert!(d.schritt(spaeter).is_none(), "kein zweiter Papiertag");
    assert!(wartend(&d).is_empty());
    let satz = some(neustart_satz(&d, spaeter), "Satz")?;
    assert!(satz.contains("Der Papiertag lief schon"), "{satz}");
    assert!(satz.contains("heutige Schutz bleibt"), "{satz}");
    assert!(seiten::live(TOKEN, "s", &d.sperre(), spaeter).contains("kopf halt\" id=\"neustart\""));
    Ok(())
}

/// Endet der Dienst waehrend des Kursabrufs, gibt es heute keinen Papiertag von selbst: Die
/// Kurse fehlen. Die Ansicht sagt das; der Abruf startet nicht doppelt.
#[test]
fn neustart_waehrend_des_kursabrufs_ohne_papiertag() -> Result<(), String> {
    let wurzel = eingeschaltet(ordner()?)?;
    let t = donnerstag(17, 46)?;
    {
        let d = nur_xetra(wurzel.clone());
        let (nr, kind) = some(d.schritt(t), "Termine")?;
        d.verfolge(nr, kind, || donnerstag(17, 46).ok());
        let (_, mut kind) = some(d.schritt(t), "Kurse")?;
        let _ = kind.wait();
    }
    let spaeter = donnerstag(18, 20)?;
    let d = nur_xetra(wurzel);
    assert!(d.schritt(spaeter).is_none());
    assert!(wartend(&d).is_empty());
    let satz = some(neustart_satz(&d, spaeter), "Satz")?;
    assert!(satz.contains("Der Kursabruf lief"), "{satz}");
    assert!(satz.contains("kein Papiertag"), "{satz}");
    Ok(())
}

#[test]
fn wartender_papiertag_startet_bei_ausgeschaltetem_zeitplan_nicht() -> Result<(), String> {
    let d = nur_xetra(eingeschaltet(ordner()?)?);
    let t = donnerstag(17, 46)?;
    let (nr, kind) = some(d.schritt(t), "Termine")?;
    d.verfolge(nr, kind, || donnerstag(17, 46).ok());
    let (nr, kind) = some(d.schritt(t), "Kurse")?;
    d.verfolge(nr, kind, || donnerstag(17, 47).ok());
    assert_eq!(wartend(&d), vec![(Art::Papier, Anlass::Zeitplan)]);
    d.setze_zeitplan(Zeitplan::default())
        .map_err(|e| format!("Zeitplan ausschalten: {e:?}"))?;
    assert!(d.schritt(donnerstag(17, 48)?).is_none());
    assert!(wartend(&d).is_empty());
    assert_eq!(d.sperre().erledigt.papier, None);
    Ok(())
}

#[test]
fn ausschalten_entfernt_wartenden_abruf_und_wiedereinschalten_holt_ihn_einmal_nach()
-> Result<(), String> {
    let wurzel = eingeschaltet(ordner()?)?;
    let d = Arc::new(Dienst::neu(
        wurzel.clone(),
        TOKEN.into(),
        echo_starter(true),
        lagebild_mit_schluesseln(true),
    ));
    d.sperre().boersen = vec![Boerse::Xetra];
    let t = donnerstag(17, 46)?;
    let tag = some(t.datum(), "Handelstag")?;
    let (nr, kind) = some(d.schritt(t), "Terminabruf")?;
    assert!(d.schritt(t).is_none());
    assert_eq!(
        wartend(&d),
        vec![(Art::KurseVon(Boerse::Xetra), Anlass::Zeitplan)]
    );
    assert_eq!(d.sperre().erledigt.kurse, None);
    d.einreihen(Art::Nachrichten, Anlass::Hand)?;

    let aus = formular_post("/zeitplan", &format!("t={TOKEN}&aktion=aus"));
    assert!(text(&behandle(&d, &Attrappe::default(), &aus, t)).contains("m=zeitplan_aus"));
    assert_eq!(wartend(&d), vec![(Art::Nachrichten, Anlass::Hand)]);
    assert_eq!(
        d.sperre().laufend.as_ref().map(|a| a.art),
        Some(Art::Termine)
    );
    d.verfolge(nr, kind, || donnerstag(17, 46).ok());
    let (nr, kind) = some(d.schritt(t), "manuelle Schlagzeilen")?;
    d.verfolge(nr, kind, || donnerstag(17, 46).ok());
    assert!(d.schritt(t).is_none());
    assert_eq!(d.sperre().erledigt.kurse, None);

    let an = formular_post("/zeitplan", &format!("t={TOKEN}&aktion=an"));
    assert!(text(&behandle(&d, &Attrappe::default(), &an, t)).contains("m=zeitplan_an"));
    let (nr, kind) = some(d.schritt(t), "nachgeholter Kursabruf")?;
    assert_eq!(d.sperre().erledigt.kurse, Some(tag));
    d.verfolge(nr, kind, || donnerstag(17, 46).ok());
    let live = get(&d, "/", t);
    assert!(
        live.contains("Als Nächstes: Papiertag (Zeitplan)"),
        "{live}"
    );
    assert!(
        live.contains("<li><b>Papiertag</b> <span class=\"offen\">wartet</span>"),
        "{live}"
    );
    assert!(
        live.contains("Danach laut Zeitplan: Termine abrufen am Fr"),
        "{live}"
    );
    drop(d);

    let neu = nur_xetra(wurzel);
    let (nr, kind) = some(neu.schritt(t), "Papiertag nach Neustart")?;
    assert_eq!(
        neu.sperre().laufend.as_ref().map(|a| a.art),
        Some(Art::Papier)
    );
    neu.verfolge(nr, kind, || donnerstag(17, 46).ok());
    assert!(neu.schritt(t).is_none());
    Ok(())
}

/// Der Kindprozess liefert dieselbe lange Ausgabe unter Windows und Linux.
#[test]
fn diagnose_kindprozess() {
    if std::env::var_os("DEPOT_DIAGNOSE_KIND").is_some() {
        println!("Ausgabe vor der Fehlermeldung");
        eprintln!("FEHLER: Beispiel vor langem Folgeprotokoll");
        for n in 0..600 {
            println!("Diagnosezeile {n}");
        }
        std::process::exit(3);
    }
}

#[test]
fn lange_diagnose_bleibt_nach_neustart_kopierbar() -> Result<(), String> {
    let wurzel = ordner()?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut d = Dienst::neu(
        wurzel.clone(),
        TOKEN.into(),
        Box::new(move |_, _| {
            let mut c = Command::new(&exe);
            c.args(["--exact", "diagnose_kindprozess", "--nocapture"])
                .env("DEPOT_DIAGNOSE_KIND", "ja");
            c
        }),
        leeres_lagebild(),
    );
    d.version = Some("abcdef0123456789".into());
    let d = Arc::new(d);
    let t = donnerstag(12, 0)?;
    d.einreihen(Art::Termine, Anlass::Hand)
        .map_err(std::string::ToString::to_string)?;
    let (nr, kind) = some(d.schritt(t), "Start")?;
    d.verfolge(nr, kind, || donnerstag(12, 1).ok());
    let kennung = {
        let z = d.sperre();
        let a = some(z.verlauf.front(), "Verlauf")?;
        assert_eq!(a.erfolg, Some(false));
        assert_eq!(a.protokoll.len(), 500);
        assert!(a.protokoll.iter().any(|l| l.contains("Diagnosezeile 599")));
        a.kennung.clone()
    };
    let gespeichert = depot_dienst::protokoll::lesen(&wurzel, &kennung)?;
    for teil in [
        "Diagnosezeile 0\n",
        "Diagnosezeile 599",
        "FEHLER: Beispiel",
        "Version abcdef0123456789",
        "Lauf abgebrochen",
    ] {
        assert!(gespeichert.contains(teil), "fehlt: {teil}");
    }
    assert_eq!(gespeichert.matches("FEHLER: Beispiel").count(), 1);
    assert!(
        gespeichert.find("Ausgabe vor der Fehlermeldung") < gespeichert.find("FEHLER: Beispiel")
    );
    assert!(gespeichert.find("FEHLER: Beispiel") < gespeichert.find("Diagnosezeile 0"));
    drop(d);
    let neu = dienst(wurzel.clone(), true);
    assert!(
        neu.sperre()
            .verlauf
            .front()
            .is_some_and(|a| a.protokoll.iter().any(|l| l.contains("Diagnosezeile 599")))
    );
    let live = get(&neu, "/", t);
    assert!(live.contains("Letzter fehlgeschlagener Lauf"));
    let html = get(&neu, &format!("/protokoll?lauf={kennung}"), t);
    assert!(html.contains("data-protokoll-kopieren>Protokoll kopieren"));
    assert!(html.contains("<pre class=\"konsole\" tabindex=\"0\">"));
    assert!(html.contains("Diagnosezeile 0\n"));
    let download = get(&neu, &format!("/protokoll.txt?lauf={kennung}"), t);
    assert!(download.contains(&gespeichert));
    assert!(get(&neu, "/protokoll.txt?lauf=..%2Fjournal", t).starts_with("HTTP/1.1 400"));
    fs::remove_dir_all(wurzel).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn konsole_stil_hell_dunkel() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let html = get(&d, "/", donnerstag(12, 0)?);
    assert!(html.contains(":root{--konsole:#f0f0ec}"));
    assert!(html.contains("@media (prefers-color-scheme:dark){:root{--konsole:#10100f}}"));
    assert!(html.contains(".konsole{border:1px solid var(--linie)"));
    assert!(html.contains("max-height:24em;overflow:auto"));
    Ok(())
}

#[test]
fn app_js_rettet_scrollstand() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let js = get(&d, "/app.js", donnerstag(12, 0)?);
    let (vorher, nachher) = some(js.split_once("ziel.innerHTML = t"), "Auffrischung")?;
    assert!(vorher.contains("pre.konsole"));
    assert!(vorher.contains("pre.scrollHeight - pre.scrollTop - pre.clientHeight < 20"));
    assert!(nachher.contains("konsolePosition(d, vorher && vorher.position)"));
    assert!(js.contains("navigator.clipboard.writeText(log.textContent)"));
    Ok(())
}

#[test]
fn andere_pre_unveraendert() -> Result<(), String> {
    let wurzel = ordner()?;
    fs::write(kostenschutz::pfad(&wurzel), "Kosten unbekannt").map_err(|e| e.to_string())?;
    fs::write(kostenschutz::kurse_pfad(&wurzel), "Kursabruf angehalten")
        .map_err(|e| e.to_string())?;
    let d = dienst(wurzel, true);
    let html = get(&d, "/einrichtung", donnerstag(12, 0)?);
    assert!(html.contains("<pre>Kosten unbekannt</pre>"));
    assert!(html.contains("<pre>Kursabruf angehalten</pre>"));
    assert!(!html.contains("<pre class=\"konsole\""));
    Ok(())
}

#[test]
fn fehlendes_diagnoselog_wird_erklaert_ohne_lauf_zu_verhindern() -> Result<(), String> {
    let wurzel = ordner()?;
    fs::write(wurzel.join("daten/protokolle"), "kein Ordner").map_err(|e| e.to_string())?;
    let d = dienst(wurzel.clone(), true);
    let t = donnerstag(12, 0)?;
    d.einreihen(Art::Termine, Anlass::Hand)
        .map_err(std::string::ToString::to_string)?;
    let (nr, kind) = some(d.schritt(t), "Start")?;
    d.verfolge(nr, kind, || donnerstag(12, 1).ok());
    assert_eq!(
        d.sperre().verlauf.front().and_then(|a| a.erfolg),
        Some(true)
    );
    assert!(get(&d, "/", t).contains("Diagnoseprotokoll nicht gespeichert"));
    let kennung = format!("{}-{nr}", t.sekunden());
    assert!(get(&d, &format!("/protokoll?lauf={kennung}"), t).starts_with("HTTP/1.1 404"));
    fs::remove_dir_all(wurzel).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn gespeichertes_protokoll_entfernt_nur_die_trefferzeile() -> Result<(), String> {
    let wurzel = ordner()?;
    fs::create_dir(wurzel.join("daten/protokolle")).map_err(|e| e.to_string())?;
    // Ein älteres Log, das noch nicht mit der heutigen Musterschicht geschrieben wurde.
    let text = "Anfang bleibt\r\nAuthorization: Basic beispiel\r\nEnde bleibt ohne Zeilenende";
    fs::write(wurzel.join("daten/protokolle/123-1.log"), text).map_err(|e| e.to_string())?;
    let d = dienst(wurzel.clone(), true);
    let t = donnerstag(12, 0)?;
    for adresse in ["/protokoll?lauf=123-1", "/protokoll.txt?lauf=123-1"] {
        let antwort = get(&d, adresse, t);
        assert!(antwort.starts_with("HTTP/1.1 200"));
        assert!(antwort.contains(
            "Anfang bleibt\r\n[Ausgabe mit Zugangsdaten entfernt.]\r\nEnde bleibt ohne Zeilenende"
        ), "{adresse}: {antwort:?}");
        assert!(!antwort.contains("beispiel"));
    }
    assert_eq!(
        fs::read_to_string(wurzel.join("daten/protokolle/123-1.log")).map_err(|e| e.to_string())?,
        text
    );
    fs::remove_dir_all(wurzel).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn maskierungs_kindprozess() {
    if std::env::var_os("DEPOT_TEST_DIENST_MASKIERUNG").is_some() {
        println!("Anfang bleibt");
        println!("https://example.com/prices?token=unbekannt&x=1");
        println!("Debug EIMSCHLUESSEL");
        println!("Ende bleibt");
    }
}

#[test]
fn dienst_maskiert_vor_speicherung_und_meldet_das_zweite_netz() -> Result<(), String> {
    let wurzel = ordner()?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let d = Arc::new(Dienst::neu(
        wurzel.clone(),
        TOKEN.into(),
        Box::new(move |_, _| {
            let mut c = Command::new(&exe);
            c.args(["--exact", "maskierungs_kindprozess", "--nocapture"])
                .env("DEPOT_TEST_DIENST_MASKIERUNG", "ja");
            c
        }),
        leeres_lagebild(),
    ));
    let t = donnerstag(12, 0)?;
    let key = ["GEHEIM", "SCHLUESSEL", "WEITER"].concat();
    let speichern = formular_post(
        "/schluessel",
        &format!("t={TOKEN}&wer=anthropic&wert={key}"),
    );
    assert_eq!(
        status(&behandle(&d, &Attrappe::default(), &speichern, t))?,
        303
    );
    d.einreihen(Art::Termine, Anlass::Hand)
        .map_err(std::string::ToString::to_string)?;
    let (nr, kind) = some(d.schritt(t), "Start")?;
    d.verfolge(nr, kind, || donnerstag(12, 1).ok());
    let kennung = format!("{}-{nr}", t.sekunden());
    let log = depot_dienst::protokoll::lesen(&wurzel, &kennung)?;
    assert!(log.contains("prices?token=***&x=1"));
    assert!(!log.contains("token=unbekannt"));
    assert!(!log.contains("EIMSCHLUESSEL"));
    assert!(log.contains("Anfang bleibt") && log.contains("Ende bleibt"));
    assert!(get(&d, "/", t).contains("Ein Lauf hat Zugangsdaten ausgegeben"));
    let download = get(&d, &format!("/protokoll.txt?lauf={kennung}"), t);
    assert!(download.contains("prices?token=***&x=1"));
    assert!(download.contains("Ende bleibt"));
    fs::remove_dir_all(wurzel).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn rss_intraday_ist_opt_in_spart_abrufe_und_uebersteht_neustart() -> Result<(), String> {
    let wurzel = eingeschaltet(ordner()?)?;
    let t = some(
        Datum::neu(2026, 11, 7).and_then(|d| Zeitpunkt::frankfurt(d, 12, 0, 0)),
        "Samstag",
    )?;
    let d = dienst(wurzel.clone(), true);
    assert!(
        d.schritt(t).is_none(),
        "kein neuer Netzzugriff ohne Einschalten"
    );
    {
        let mut z = d.sperre();
        z.zeitplan.nachrichten_an = true;
        fs::write(wurzel.join("daten/zeitplan.txt"), z.zeitplan.als_text())
            .map_err(|e| e.to_string())?;
    }
    let (nr, kind) = some(d.schritt(t), "RSS auch am Wochenende")?;
    assert_eq!(
        d.sperre().laufend.as_ref().map(|a| a.art),
        Some(Art::Nachrichten)
    );
    d.verfolge(nr, kind, || {
        Datum::neu(2026, 11, 7).and_then(|d| Zeitpunkt::frankfurt(d, 12, 0, 0))
    });
    assert!(d.schritt(t).is_none());
    drop(d);
    let neu = dienst(wurzel, true);
    assert!(
        neu.schritt(t).is_none(),
        "Neustart setzt die Pause nicht zurueck"
    );
    let spaeter = some(
        Datum::neu(2026, 11, 7).and_then(|d| Zeitpunkt::frankfurt(d, 13, 30, 0)),
        "spaeter",
    )?;
    let (_, mut kind) = some(neu.schritt(spaeter), "RSS nach 90 Minuten")?;
    assert_eq!(
        neu.sperre().laufend.as_ref().map(|a| a.art),
        Some(Art::Nachrichten)
    );
    let _ = kind.wait();
    Ok(())
}

#[test]
fn vorbereitungsautomatik_folgt_erfolgreichen_nachrichten_und_bleibt_nach_neustart_einmalig()
-> Result<(), String> {
    let wurzel = eingeschaltet(ordner()?)?;
    let t = Datum::neu(2026, 11, 7)
        .and_then(|d| Zeitpunkt::frankfurt(d, 12, 0, 0))
        .ok_or("Samstag")?;
    let d = dienst(wurzel.clone(), true);
    {
        let mut z = d.sperre();
        z.zeitplan.nachrichten_an = true;
        z.zeitplan.vorbereitung = depot_dienst::zeitplan::VorbereitungsPlan::An;
        fs::write(wurzel.join("daten/zeitplan.txt"), z.zeitplan.als_text())
            .map_err(|e| e.to_string())?;
    }
    let einrichtung = get(&d, "/einrichtung", t);
    assert!(einrichtung.contains("Diese Vorbereitung nutzt KI-Budget."));
    assert!(!einrichtung.contains("keine zusätzlichen KI-Aufrufe"));
    let spaeter = Zeitpunkt::frankfurt(t.datum().ok_or("Datum")?, 13, 30, 0).ok_or("Zeit")?;
    for (jetzt, erwartet) in [
        (t, Art::Nachrichten),
        (spaeter, Art::Vorbereiten),
        (spaeter, Art::Nachrichten),
    ] {
        let (nr, kind) = d.schritt(jetzt).ok_or("Auftrag fehlt")?;
        assert_eq!(d.sperre().laufend.as_ref().map(|a| a.art), Some(erwartet));
        d.verfolge(nr, kind, || {
            Datum::neu(2026, 11, 7).and_then(|d| Zeitpunkt::frankfurt(d, 13, 30, 0))
        });
    }
    drop(d);
    let neu = dienst(wurzel, true);
    assert!(
        neu.schritt(spaeter).is_none(),
        "Weitere Schlagzeilen lösen keine zweite Vorbereitung aus"
    );
    Ok(())
}

#[test]
fn vorbereiten_braucht_bestaetigung_und_automatik_braucht_schlagzeilen() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = Datum::neu(2026, 11, 7)
        .and_then(|d| Zeitpunkt::frankfurt(d, 12, 0, 0))
        .ok_or("Samstag")?;
    let seite = get(&d, "/bestaetigen?art=vorbereiten", t);
    assert!(
        seite.contains("echtes KI-Budget")
            && seite.contains("geprueft")
            && seite.contains("Abbrechen")
    );
    let post = formular_post("/ausfuehren", &format!("t={TOKEN}&art=vorbereiten"));
    assert!(
        text(&behandle(&d, &Attrappe::default(), &post, t)).contains("m=vorbereitung_bestaetigen")
    );
    assert!(wartend(&d).is_empty());
    let plan = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&aktion=speichern&termine=17%3A40&kurse=17%3A45&vorbereitung_an=ja"),
    );
    let antwort = text(&behandle(&d, &Attrappe::default(), &plan, t));
    assert!(antwort.contains("m=vorbereitung_quellen"));
    assert!(get(&d, &ort(&antwort), t).contains("name=\"vorbereitung_an\" value=\"ja\" checked"));
    assert!(!d.sperre().zeitplan.vorbereitung.aktiv());
    let post = formular_post(
        "/ausfuehren",
        &format!("t={TOKEN}&art=vorbereiten&geprueft=ja"),
    );
    assert!(text(&behandle(&d, &Attrappe::default(), &post, t)).contains("m=auftrag"));
    assert_eq!(wartend(&d), vec![(Art::Vorbereiten, Anlass::Hand)]);
    Ok(())
}

#[test]
fn live_zeigt_den_tatsaechlichen_stand_der_vorbereitungsautomatik() -> Result<(), String> {
    use depot_dienst::zeitplan::VorbereitungsPlan;

    let d = dienst(ordner()?, true);
    let t = donnerstag(12, 0)?;
    assert!(d.schritt(t).is_none());
    for (an, vorbereitung, fehler, erwartet) in [
        (
            false,
            VorbereitungsPlan::An,
            None,
            "Automatische Vorbereitung pausiert.",
        ),
        (true, VorbereitungsPlan::Aus, None, "ist ausgeschaltet."),
        (true, VorbereitungsPlan::An, None, "ist eingeschaltet."),
        (
            true,
            VorbereitungsPlan::An,
            Some("Zeitplan nicht lesbar"),
            "Zeitplan angehalten.",
        ),
    ] {
        {
            let mut z = d.sperre();
            z.zeitplan.an = an;
            z.zeitplan.nachrichten_an = true;
            z.zeitplan.vorbereitung = vorbereitung;
            z.zeitplan_fehler = fehler.map(str::to_owned);
        }
        let live = get(&d, "/teil/live", t);
        let abschnitt = live
            .split_once("<section id=\"vorbereitung\">")
            .and_then(|(_, rest)| rest.split_once("</section>"))
            .map(|(abschnitt, _)| abschnitt)
            .ok_or("Vorbereitung fehlt")?;
        assert!(abschnitt.contains("Noch keine separate Vorbereitung belegt."));
        assert!(abschnitt.contains(erwartet), "{abschnitt}");
        assert!(abschnitt.contains("Heute ist Handelstag."));
        assert!(!abschnitt.contains("Automatik einrichten"));
    }
    Ok(())
}

#[test]
fn vorbereitung_zeigt_frankfurter_zeitfenster_und_prueft_alte_bestaetigungen() -> Result<(), String>
{
    let d = dienst(ordner()?, true);
    let samstag = some(Datum::neu(2026, 11, 7), "Samstag")?;
    let offen = some(Zeitpunkt::frankfurt(samstag, 22, 59, 0), "Zeit vor Schluss")?;
    let geschlossen = some(Zeitpunkt::frankfurt(samstag, 23, 0, 0), "Zeit nach Schluss")?;
    let ziel = "/bestaetigen?art=vorbereiten";
    let post = formular_post(
        "/ausfuehren",
        &format!("t={TOKEN}&art=vorbereiten&geprueft=ja"),
    );
    assert!(d.schritt(offen).is_none());
    assert!(get(&d, "/teil/live", offen).contains(ziel));
    assert!(get(&d, ziel, offen).contains("echtes KI-Budget"));
    let seite = get(&d, "/teil/live", geschlossen);
    assert!(seite.contains("bis vor 23:00 Uhr Frankfurter Zeit"));
    assert!(!seite.contains(ziel));
    for (zeit, grund) in [
        (geschlossen, "vorbereitung_uhrzeit"),
        (donnerstag(12, 0)?, "vorbereitung_handelstag"),
    ] {
        assert!(get(&d, ziel, zeit).contains(grund));
        let antwort = text(&behandle(&d, &Attrappe::default(), &post, zeit));
        assert!(antwort.contains(grund));
        assert!(wartend(&d).is_empty());
    }
    Ok(())
}

#[test]
fn vorbereitung_vor_start_ausschalten_und_alte_zeitplaene_bleiben_aus() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = Datum::neu(2026, 11, 7)
        .and_then(|d| Zeitpunkt::frankfurt(d, 12, 0, 0))
        .ok_or("Samstag")?;
    d.einreihen(Art::Vorbereiten, Anlass::Zeitplan)?;
    assert!(d.schritt(t).is_none());
    assert!(d.sperre().laufend.is_none());
    let alter = Zeitplan::default().als_text();
    assert!(!Zeitplan::lies(&alter)?.vorbereitung.aktiv());
    let plan = Zeitplan {
        nachrichten_an: true,
        vorbereitung: depot_dienst::zeitplan::VorbereitungsPlan::An,
        ..Zeitplan::default()
    };
    assert_eq!(Zeitplan::lies(&plan.als_text())?, plan);
    assert!(Zeitplan::lies(&(plan.als_text() + "vorbereitung_an=ja\n")).is_err());
    Ok(())
}

#[test]
fn rss_checkbox_bleibt_bei_fehlern_erhalten_und_wird_dauerhaft_gespeichert() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let u = Attrappe::default();
    let t = donnerstag(12, 0)?;
    let falsch = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&aktion=speichern&termine=kaputt&kurse=17%3A45&nachrichten_an=ja"),
    );
    let antwort = text(&behandle(&d, &u, &falsch, t));
    assert!(antwort.contains("nachrichten_an=ja"), "{antwort}");
    assert!(!d.sperre().zeitplan.nachrichten_an);
    let richtig = formular_post(
        "/zeitplan",
        &format!("t={TOKEN}&aktion=speichern&termine=17%3A40&kurse=17%3A45&nachrichten_an=ja"),
    );
    assert!(text(&behandle(&d, &u, &richtig, t)).contains("m=zeitplan#zeitplan"));
    assert!(d.sperre().zeitplan.nachrichten_an);
    let inhalt =
        fs::read_to_string(wurzel.join("daten/zeitplan.txt")).map_err(|e| e.to_string())?;
    assert!(Zeitplan::lies(&inhalt)?.nachrichten_an);
    let seite = text(&behandle(
        &d,
        &u,
        &anfrage("GET", "/einrichtung", &[("Host", HOST)], ""),
        t,
    ));
    assert!(seite.contains("name=\"nachrichten_an\""));
    Ok(())
}

#[test]
fn wartende_schlagzeilen_starten_nach_ausschalten_nicht_mehr() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    d.einreihen(Art::Nachrichten, Anlass::Zeitplan)
        .map_err(str::to_string)?;
    assert!(d.schritt(donnerstag(12, 0)?).is_none());
    assert!(d.sperre().laufend.is_none());
    Ok(())
}

/// Auch eine alte, unspezifische Kurssperre kann ohne Aufheben untersucht werden.
#[test]
fn kurssperre_pruefabruf_ist_getrennt_und_doppelklickfest() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let umw = Attrappe::default();
    let t = donnerstag(18, 0)?;
    let seite = anfrage("GET", "/einrichtung", &[("Host", HOST)], "");
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(!s.contains("value=\"kursdiagnose\""));
    let halt = kostenschutz::kurse_pfad(&wurzel);
    let grund = "Sicherheitsabbruch: Die Kursquelle gab den Schluessel zurueck\n";
    fs::write(&halt, grund).map_err(|e| e.to_string())?;
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(s.contains("value=\"kursdiagnose\""));
    assert!(s.contains("Kurssperre sicher untersuchen"));
    assert!(s.contains("Sperre bleibt bestehen") && s.contains("Quellenkontingent"));
    let post = formular_post("/ausfuehren", &format!("t={TOKEN}&art=kursdiagnose"));
    for _ in 0..2 {
        let _ = behandle(&d, &umw, &post, t);
    }
    assert_eq!(d.sperre().warteschlange.len(), 1);
    let s = text(&behandle(&d, &umw, &seite, t));
    assert!(s.contains("disabled>Kurssperre sicher untersuchen"));
    assert_eq!(fs::read_to_string(&halt).map_err(|e| e.to_string())?, grund);
    assert_eq!(Art::Kursdiagnose.argumente(), &["kurse", "diagnose"]);
    assert_eq!(Art::aus_kurz("kursdiagnose"), Some(Art::Kursdiagnose));
    Ok(())
}

#[test]
fn vorbereitung_erst_nach_updatebestaetigung_einschalten_und_starten() -> Result<(), String> {
    let p = ordner()?;
    let d = dienst(p.clone(), true);
    let t = donnerstag(12, 0)?;
    let offen = p.join("daten/update_offen.txt");
    fs::write(&offen, "Probe").map_err(|e| e.to_string())?;
    let post = formular_post(
        "/ausfuehren",
        &format!("t={TOKEN}&art=vorbereiten&geprueft=ja"),
    );
    let antwort = text(&behandle(&d, &Attrappe::default(), &post, t));
    assert!(antwort.contains("m=vorbereitung_update"));
    assert!(wartend(&d).is_empty());
    let plan = Zeitplan {
        nachrichten_an: true,
        vorbereitung: depot_dienst::zeitplan::VorbereitungsPlan::An,
        ..Zeitplan::default()
    };
    assert_eq!(
        d.setze_zeitplan(plan),
        Err(depot_dienst::werkbank::ZeitplanAbgelehnt::UpdateOffen)
    );
    assert!(!d.sperre().zeitplan.vorbereitung.aktiv());
    fs::remove_file(offen).map_err(|e| e.to_string())?;
    d.setze_zeitplan(plan).map_err(|e| format!("{e:?}"))?;
    let samstag = some(
        Datum::neu(2026, 11, 7).and_then(|d| Zeitpunkt::frankfurt(d, 12, 0, 0)),
        "Samstag",
    )?;
    assert!(d.von_hand(Art::Vorbereiten, samstag).is_ok());
    Ok(())
}

// --- Windows-Hinweis bei Eingriff (#122) ---

type Gruende = Arc<Mutex<Vec<String>>>;

/// Ein Lagebild, dessen Gründe für „Eingriff nötig“ der Test vorgibt; mit `ausgefallen` ist der
/// jüngste Handelstag (09.11.2026) ausgefallen.
fn lagebild_mit_gruenden(gruende: &Gruende, ausgefallen: bool) -> Lagerechner {
    let gruende = Arc::clone(gruende);
    let leer = leeres_lagebild();
    Box::new(move |wurzel: &Path, jetzt: Zeitpunkt| {
        let mut bild = leer(wurzel, jetzt);
        let g = gruende
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        bild.betrieb = if g.is_empty() {
            depot_lage::Betrieb::Bereit
        } else {
            depot_lage::Betrieb::EingriffNoetig(g)
        };
        bild.tage = Vec::new();
        if ausgefallen && let Some(d) = Datum::neu(2026, 11, 9) {
            bild.tage.push(depot_lage::TagErgebnis {
                datum: d,
                stand: depot_lage::TagStand::Ausgefallen("keine Schlusskurse".into()),
                ki: Vec::new(),
            });
        }
        bild
    })
}

type Gezeigt = Arc<Mutex<Vec<(String, String)>>>;

fn hinweis_dienst(wurzel: PathBuf, gruende: &Gruende, ausgefallen: bool) -> (Arc<Dienst>, Gezeigt) {
    let gezeigt: Gezeigt = Arc::default();
    let g = Arc::clone(&gezeigt);
    let d = Dienst::neu(
        wurzel,
        TOKEN.into(),
        echo_starter(true),
        lagebild_mit_gruenden(gruende, ausgefallen),
    )
    .mit_melder(Box::new(move |titel: &str, text: &str| {
        g.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((titel.to_string(), text.to_string()));
    }));
    (Arc::new(d), gezeigt)
}

fn setze(gruende: &Gruende, neu: &[&str]) {
    *gruende.lock().unwrap_or_else(PoisonError::into_inner) =
        neu.iter().map(|g| (*g).to_string()).collect();
}

fn gezeigt(g: &Gezeigt) -> Vec<String> {
    g.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .map(|(_, t)| t.clone())
        .collect()
}

/// Lässt den Dienst zu den genannten Sekunden nach 12:00 Uhr (Samstag) das Lagebild rechnen.
fn schritte(d: &Dienst, sekunden: &[i64]) -> Result<(), String> {
    let basis = some(
        Datum::neu(2026, 11, 7).and_then(|t| Zeitpunkt::frankfurt(t, 12, 0, 0)),
        "Samstag",
    )?;
    for s in sekunden {
        let t = some(
            basis
                .sekunden()
                .checked_add(*s)
                .and_then(Zeitpunkt::aus_sekunden),
            "Zeit",
        )?;
        assert!(d.schritt(t).is_none());
    }
    Ok(())
}

fn hinweise_datei(wurzel: &Path) -> String {
    fs::read_to_string(wurzel.join("daten").join("hinweise.txt")).unwrap_or_default()
}

#[test]
fn hinweis_meldet_neuen_grund_einmal_und_nach_rueckkehr_wieder() -> Result<(), String> {
    let wurzel = ordner()?;
    let gruende = Gruende::default();
    setze(&gruende, &["Notbremse: Topf A unter 70 %"]);
    let (d, g) = hinweis_dienst(wurzel.clone(), &gruende, false);
    schritte(&d, &[0, 10, 20, 30])?;
    let gesehen = g.lock().unwrap_or_else(PoisonError::into_inner).clone();
    assert_eq!(gesehen.len(), 1);
    assert_eq!(
        gesehen.first().map(|(t, x)| (t.as_str(), x.as_str())),
        Some((
            "DepotSchlopf: Eingriff nötig",
            "Notbremse: Topf A unter 70 % Details auf http://127.0.0.1:47410/"
        ))
    );
    let h = depot_kern::abdruck::sha256_hex("Notbremse: Topf A unter 70 %".as_bytes());
    assert!(hinweise_datei(&wurzel).starts_with(&format!("{h};2026-11-07T11:00:10Z")));
    setze(&gruende, &[]);
    schritte(&d, &[100, 110])?;
    assert_eq!(hinweise_datei(&wurzel), "");
    setze(&gruende, &["Notbremse: Topf A unter 70 %"]);
    schritte(&d, &[200, 210])?;
    assert_eq!(gezeigt(&g).len(), 2);
    Ok(())
}

#[test]
fn neustart_meldet_nicht_doppelt() -> Result<(), String> {
    let wurzel = ordner()?;
    let h = depot_kern::abdruck::sha256_hex(b"A");
    fs::write(
        wurzel.join("daten").join("hinweise.txt"),
        format!("{h};2026-11-07T10:00:00Z\n"),
    )
    .map_err(|e| format!("Vorbedingung Datei: {e}"))?;
    let gruende = Gruende::default();
    setze(&gruende, &["A"]);
    let (d, g) = hinweis_dienst(wurzel.clone(), &gruende, false);
    schritte(&d, &[0, 10, 20, 80])?;
    assert!(gezeigt(&g).is_empty());
    assert_eq!(
        hinweise_datei(&wurzel),
        format!("{h};2026-11-07T10:00:00Z\n")
    );
    Ok(())
}

#[test]
fn buendelt_in_60_sekunden() -> Result<(), String> {
    let wurzel = ordner()?;
    let gruende = Gruende::default();
    let (d, g) = hinweis_dienst(wurzel, &gruende, false);
    setze(&gruende, &["A"]);
    schritte(&d, &[0])?;
    setze(&gruende, &["A", "B"]);
    schritte(&d, &[5])?;
    setze(&gruende, &["A", "B", "C"]);
    schritte(&d, &[10, 20])?;
    assert_eq!(
        gezeigt(&g),
        vec!["A (und 2 weitere) Details auf http://127.0.0.1:47410/".to_string()]
    );
    // Ein weiterer Grund kurz danach wartet, bis 60 Sekunden seit dem letzten Hinweis um sind.
    setze(&gruende, &["A", "B", "C", "D"]);
    schritte(&d, &[30, 40, 50, 60])?;
    assert_eq!(gezeigt(&g).len(), 1);
    schritte(&d, &[70])?;
    assert_eq!(
        gezeigt(&g).last().map(String::as_str),
        Some("D Details auf http://127.0.0.1:47410/")
    );
    Ok(())
}

#[test]
fn ausgefallener_tag_einmal() -> Result<(), String> {
    let wurzel = ordner()?;
    let gruende = Gruende::default();
    let (d, g) = hinweis_dienst(wurzel, &gruende, true);
    schritte(&d, &[0, 10, 20, 30, 40])?;
    assert_eq!(
        gezeigt(&g),
        vec![
            "Papiertag am 09.11.2026 ausgefallen. Details auf http://127.0.0.1:47410/".to_string()
        ]
    );
    Ok(())
}

#[test]
fn kaputte_hinweisdatei_gilt_leer() -> Result<(), String> {
    let wurzel = ordner()?;
    fs::write(wurzel.join("daten").join("hinweise.txt"), "xyz")
        .map_err(|e| format!("Vorbedingung Datei: {e}"))?;
    let gruende = Gruende::default();
    setze(&gruende, &["A"]);
    let (d, g) = hinweis_dienst(wurzel.clone(), &gruende, false);
    schritte(&d, &[0, 10])?;
    assert_eq!(gezeigt(&g).len(), 1);
    let h = depot_kern::abdruck::sha256_hex(b"A");
    assert_eq!(
        hinweise_datei(&wurzel),
        format!("{h};2026-11-07T11:00:10Z\n")
    );
    Ok(())
}

#[test]
fn hinweis_schalter_aus_pflegt_trotzdem() -> Result<(), String> {
    let wurzel = ordner()?;
    let plan = Zeitplan {
        hinweis: depot_dienst::zeitplan::HinweisPlan::Aus,
        ..Zeitplan::default()
    };
    fs::write(wurzel.join("daten").join("zeitplan.txt"), plan.als_text())
        .map_err(|e| format!("Vorbedingung Zeitplan: {e}"))?;
    let gruende = Gruende::default();
    setze(&gruende, &["A"]);
    let (d, g) = hinweis_dienst(wurzel.clone(), &gruende, false);
    schritte(&d, &[0, 10, 20])?;
    assert!(gezeigt(&g).is_empty());
    let h = depot_kern::abdruck::sha256_hex(b"A");
    assert!(hinweise_datei(&wurzel).starts_with(&h));
    Ok(())
}

#[test]
fn zeitplan_ohne_hinweisfeld_heisst_an() -> Result<(), String> {
    let alt = "zeitplan-v1\nan=ja\ntermine=17:40\nkurse=17:45\npapier_danach=ja\n";
    assert_eq!(
        Zeitplan::lies(alt)?.hinweis,
        depot_dienst::zeitplan::HinweisPlan::An
    );
    let aus = Zeitplan {
        hinweis: depot_dienst::zeitplan::HinweisPlan::Aus,
        ..Zeitplan::default()
    };
    assert!(aus.als_text().ends_with("hinweis_aus=ja\n"));
    assert_eq!(Zeitplan::lies(&aus.als_text())?, aus);
    assert!(Zeitplan::lies(&(aus.als_text() + "hinweis_aus=ja\n")).is_err());
    Ok(())
}

#[test]
fn alter_standardplan_wird_einmal_auf_den_morgen_umgestellt() -> Result<(), String> {
    let wurzel = ordner()?;
    let pfad = wurzel.join("daten/zeitplan.txt");
    let alt = Zeitplan {
        an: true,
        kurse: (17, 52),
        ..abendplan()
    };
    fs::write(&pfad, alt.als_text()).map_err(|e| e.to_string())?;
    let d = dienst(wurzel.clone(), true);
    let plan = d.sperre().zeitplan;
    assert!(plan.an);
    assert_eq!((plan.termine, plan.kurse), ((7, 20), (7, 30)));
    assert_eq!(
        fs::read_to_string(&pfad).map_err(|e| e.to_string())?,
        plan.als_text()
    );
    assert_eq!(dienst(wurzel, true).sperre().zeitplan, plan);
    let tag = some(donnerstag(7, 30)?.datum(), "Tag")?;
    let beide = [Boerse::Xetra, Boerse::Nyse];
    assert_eq!(
        abrufe_fuer(&plan, tag, &beide),
        vec![(Boerse::Xetra, (7, 30)), (Boerse::Nyse, (7, 30))]
    );
    let mut erledigt = Erledigt::default();
    erledigt.setze(Art::Termine, tag);
    assert_eq!(
        faellig_fuer(&plan, &erledigt, tag, (10, 15), &beide),
        Some(Art::KurseVon(Boerse::Xetra))
    );
    assert_eq!(faellig_fuer(&plan, &erledigt, tag, (17, 35), &beide), None);
    Ok(())
}

#[test]
fn spaeter_neustart_holt_nach_oder_vermerkt_den_ausfall_einmal() -> Result<(), String> {
    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    fs::write(
        daten.join("zeitplan.txt"),
        Zeitplan {
            an: true,
            ..Zeitplan::default()
        }
        .als_text(),
    )
    .map_err(|e| e.to_string())?;
    let d = dienst(wurzel.clone(), true);
    let zeit = donnerstag(10, 15)?;
    for art in [
        Art::Termine,
        Art::KurseVon(Boerse::Xetra),
        Art::KurseVon(Boerse::Nyse),
        Art::Papier,
    ] {
        let (nr, kind) = some(d.schritt(zeit), "Auftrag nachholen")?;
        assert_eq!(d.sperre().laufend.as_ref().map(|a| a.art), Some(art));
        d.verfolge(nr, kind, || donnerstag(10, 16).ok());
    }
    drop(d);
    assert!(dienst(wurzel, true).schritt(donnerstag(10, 20)?).is_none());

    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    fs::write(
        daten.join("zeitplan.txt"),
        Zeitplan {
            an: true,
            ..Zeitplan::default()
        }
        .als_text(),
    )
    .map_err(|e| e.to_string())?;
    let d = dienst(wurzel.clone(), true);
    let (nr, kind) = some(d.schritt(donnerstag(17, 50)?), "Termine bleiben möglich")?;
    assert_eq!(
        d.sperre().laufend.as_ref().map(|a| a.art),
        Some(Art::Termine)
    );
    d.verfolge(nr, kind, || donnerstag(17, 51).ok());
    drop(d);
    let d = dienst(wurzel, true);
    assert!(d.schritt(donnerstag(18, 30)?).is_none());
    let verlauf =
        fs::read_to_string(daten.join("dienst_verlauf.txt")).map_err(|e| e.to_string())?;
    assert_eq!(verlauf.matches(";papier_ausgelassen;").count(), 1);
    let morgen = some(
        Datum::neu(2026, 11, 6).and_then(|d| Zeitpunkt::frankfurt(d, 7, 30, 0)),
        "Freitag",
    )?;
    let (nr, kind) = some(d.schritt(morgen), "Termine am Folgetag")?;
    assert_eq!(
        d.sperre().laufend.as_ref().map(|a| a.art),
        Some(Art::Termine)
    );
    d.verfolge(nr, kind, || None);
    Ok(())
}

#[test]
fn ausgelassener_papiertag_meldet_sich_genau_einmal() -> Result<(), String> {
    let wurzel = ordner()?;
    let tag = some(donnerstag(17, 50)?.datum(), "Tag")?;
    let daten = wurzel.join("daten");
    fs::write(
        daten.join("zeitplan.txt"),
        Zeitplan {
            an: true,
            ..Zeitplan::default()
        }
        .als_text(),
    )
    .map_err(|e| e.to_string())?;
    let mut erledigt = Erledigt::default();
    erledigt.setze(Art::Termine, tag);
    fs::write(daten.join("dienst_erledigt.txt"), erledigt.als_text()).map_err(|e| e.to_string())?;
    let gruende = Gruende::default();
    let (d, meldungen) = hinweis_dienst(wurzel.clone(), &gruende, false);
    assert!(d.schritt(donnerstag(17, 50)?).is_none());
    assert!(d.schritt(donnerstag(17, 51)?).is_none());
    let erste = gezeigt(&meldungen);
    assert_eq!(erste.len(), 1);
    assert!(erste.iter().any(|text| text.contains("ausgelassen")));
    drop(d);
    let (d, meldungen) = hinweis_dienst(wurzel, &gruende, false);
    assert!(d.schritt(donnerstag(18, 30)?).is_none());
    assert!(gezeigt(&meldungen).is_empty());
    Ok(())
}

#[test]
fn geschlossene_xetra_bestimmt_den_nyse_schluss_nicht() -> Result<(), String> {
    let tag = some(Datum::neu(2026, 12, 24), "Heiligabend")?;
    let boersen = [Boerse::Xetra, Boerse::Nyse];
    assert!(!Boerse::Xetra.handelstag(tag) && Boerse::Nyse.handelstag(tag));
    assert_eq!(
        depot_kern::liste::erster_schluss(tag, &boersen),
        Boerse::Nyse.schluss(tag)
    );
    let zeit = some(Zeitpunkt::frankfurt(tag, 18, 0, 0), "Frankfurt")?;
    assert!(depot_kern::liste::morgenlauf(zeit));
    assert!(!depot_kern::liste::morgenlauf(some(
        Boerse::Nyse.schluss(tag),
        "NYSE-Schluss"
    )?));
    Ok(())
}

#[test]
fn gespraeche_zeigt_alle_ki_rollen_filtert_und_maskiert() -> Result<(), String> {
    use depot_lage::gespraeche::{Gespraech, anhaengen};
    use std::io::Write as _;

    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    let d = dienst(wurzel, true);
    let zeit = donnerstag(17, 50)?;
    let tag = some(zeit.in_frankfurt().map(|(d, ..)| d), "Frankfurter Tag")?;
    let eintrag = Gespraech {
        zeit,
        lauf: "123-1".into(),
        wer: "dachs".into(),
        rolle: "haupt".into(),
        modell: "Hauptmodell".into(),
        zweck: "erstmeinung".into(),
        ergebnis: "ok".into(),
        http: Some(200),
        token_ein: Some(123),
        token_aus: Some(45),
        buchung: None,
        kosten_cent: Some(3),
        system: Some("Systemtext bleibt im Protokoll".into()),
        frage: Some("Frage von Dachs".into()),
        antwort: Some(r#"{"einschaetzung":"<script>Antwort von Dachs</script>","ziele":[{"isin":"DE0007164600","gewicht_bp":2000,"begruendung":"Konkreter Kurstreiber."}]}"#.into()),
        fehler: None,
    };
    anhaengen(&daten, &eintrag).map_err(|e| e.to_string())?;
    anhaengen(
        &daten,
        &Gespraech {
            wer: "gemini".into(),
            rolle: "zusatz".into(),
            modell: "Zusatzmodell".into(),
            frage: Some("Frage von Gemini".into()),
            antwort: Some(
                r#"{"meldungen":[{"kurz":"Gemini fasst die Meldung zusammen."}]}"#.into(),
            ),
            ..eintrag.clone()
        },
    )
    .map_err(|e| e.to_string())?;
    anhaengen(
        &daten,
        &Gespraech {
            wer: "maus".into(),
            modell: "Maus-Modell".into(),
            zweck: "zweitmeinung".into(),
            ergebnis: "guthaben".into(),
            http: Some(429),
            antwort: None,
            fehler: Some(r#"{"error":"insufficient_quota"}"#.into()),
            ..eintrag
        },
    )
    .map_err(|e| e.to_string())?;
    std::fs::OpenOptions::new()
        .append(true)
        .open(daten.join("gespraeche").join(format!("{tag}.jsonl")))
        .and_then(|mut f| f.write_all(b"{kaputt}\n"))
        .map_err(|e| e.to_string())?;

    let alles = get(&d, "/gespraeche", zeit);
    assert!(alles.contains("KI-Gespräche") && alles.contains("Zusatz-KI"));
    assert!(alles.contains("Dachs (Hauptmodell)") && alles.contains("Gemini (Zusatzmodell)"));
    assert!(alles.contains("&lt;script&gt;Antwort von Dachs&lt;/script&gt;"));
    assert!(alles.contains("DE0007164600 20,00 %. Konkreter Kurstreiber."));
    assert!(alles.contains("Beim Anbieter fehlt Guthaben."));
    assert!(alles.contains("Nicht lesbare Zeilen: 1"));
    for technik in [
        "<script>Antwort von Dachs</script>",
        "Systemtext bleibt im Protokoll",
        "Frage von Dachs",
        "insufficient_quota",
        "gewicht_bp",
        "HTTP 429",
        "Token ein/aus",
    ] {
        assert!(!alles.contains(technik), "{technik}");
    }
    let gefiltert = get(&d, &format!("/gespraeche?tag={tag}&wer=gemini"), zeit);
    assert!(gefiltert.contains("Gemini fasst die Meldung zusammen."));
    assert!(!gefiltert.contains("Antwort von Dachs"));
    Ok(())
}

#[test]
fn zeitstrahl_ordnet_modelle_und_gespraeche_dem_genauen_lauf_zu() -> Result<(), String> {
    use depot_dienst::werkbank::{Auftrag, Frueher};
    use depot_lage::gespraeche::{Gespraech, anhaengen};

    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    let d = dienst(wurzel, true);
    let zeit = donnerstag(17, 50)?;
    let tag = some(zeit.in_frankfurt().map(|(d, ..)| d), "Frankfurter Tag")?;
    let kennung = format!("{}-1", zeit.sekunden());
    {
        let mut z = d.sperre();
        z.zeitplan.an = true;
        z.zeitplan.papier_danach = true;
        z.boersen = vec![Boerse::Xetra];
        z.verlauf.push_front(Auftrag {
            nr: 1,
            kennung: kennung.clone(),
            art: Art::Papier,
            anlass: Anlass::Zeitplan,
            start: zeit,
            ende: Some(zeit),
            erfolg: Some(true),
            protokoll: Vec::new(),
            hinweis: None,
            frueher: Frueher::Nein,
        });
    }
    let html = get(&d, "/teil/live", zeit);
    assert!(html.contains("Modell nicht protokolliert"));
    assert!(!html.contains("Dachs ("));

    for (lauf, wer, rolle, modell, ergebnis, frage) in [
        (
            kennung.as_str(),
            "dachs",
            "haupt",
            "Dachs-alt<script>",
            "ok",
            "Frage für diesen Lauf",
        ),
        (
            kennung.as_str(),
            "maus",
            "haupt",
            "Maus-alt",
            "limit",
            "Frage trotz Limit",
        ),
        (
            kennung.as_str(),
            "gemini",
            "zusatz",
            "Gemini-alt",
            "ok",
            "Zusatzfrage",
        ),
        (
            "anderer-lauf",
            "dachs",
            "haupt",
            "falsches Modell",
            "ok",
            "Frage aus anderem Lauf",
        ),
    ] {
        let e = Gespraech {
            zeit,
            lauf: lauf.into(),
            wer: wer.into(),
            rolle: rolle.into(),
            modell: modell.into(),
            zweck: "erstmeinung".into(),
            ergebnis: ergebnis.into(),
            http: Some(200),
            token_ein: None,
            token_aus: None,
            buchung: None,
            kosten_cent: None,
            system: None,
            frage: None,
            antwort: Some(frage.into()),
            fehler: None,
        };
        anhaengen(&daten, &e).map_err(|e| e.to_string())?;
    }
    let html = get(&d, "/teil/live", zeit);
    assert!(html.contains("Dachs (Dachs-alt&lt;script&gt;)"));
    assert!(html.contains("Maus (Maus-alt)</a> · Anbieterlimit"));
    assert!(html.contains("Gemini (Gemini-alt)</a> · Antwort erhalten · optional"));
    assert!(!html.contains("falsches Modell") && !html.contains("Modell nicht protokolliert"));
    assert!(html.contains(&format!(
        "/gespraeche?tag={tag}&amp;wer=dachs&amp;lauf={kennung}"
    )));
    let gespraech = get(
        &d,
        &format!("/gespraeche?tag={tag}&wer=dachs&lauf={kennung}"),
        zeit,
    );
    assert!(gespraech.contains("Frage für diesen Lauf"));
    assert!(!gespraech.contains("Frage aus anderem Lauf"));
    Ok(())
}

#[test]
fn betriebsleiste_zaehlt_nur_auftraege_nach_dem_naechsten() -> Result<(), String> {
    let d = dienst(ordner()?, false);
    let jetzt = donnerstag(17, 39)?;
    d.einreihen(Art::Termine, Anlass::Hand)?;
    let html = get(&d, "/teil/kopf", jetzt);
    assert!(html.contains("Als Nächstes: Termine"));
    assert!(!html.contains("Danach warte"));
    d.einreihen(Art::Nachrichten, Anlass::Hand)?;
    assert!(get(&d, "/teil/kopf", jetzt).contains("Danach wartet 1 Auftrag."));
    d.einreihen(Art::Kurse, Anlass::Hand)?;
    assert!(get(&d, "/teil/kopf", jetzt).contains("Danach warten 2 Aufträge."));
    Ok(())
}

#[test]
fn gemini_ohne_anfrage_hat_eigenen_lauf_und_keine_erfolgsmeldung() -> Result<(), String> {
    use depot_dienst::werkbank::{Auftrag, Frueher};
    use depot_lage::gespraeche::{Gespraech, anhaengen};

    let d = dienst(ordner()?, true);
    let zeit = donnerstag(17, 50)?;
    let lauf = format!("{}-2", zeit.sekunden());
    d.sperre().verlauf.push_front(Auftrag {
        nr: 2,
        kennung: lauf.clone(),
        art: Art::Gemini,
        anlass: Anlass::Zeitplan,
        start: zeit,
        ende: Some(zeit),
        erfolg: Some(true),
        protokoll: Vec::new(),
        hinweis: None,
        frueher: Frueher::Nein,
    });
    anhaengen(
        &d.wurzel.join("daten"),
        &Gespraech {
            zeit,
            lauf: lauf.clone(),
            wer: "gemini".into(),
            rolle: "zusatz".into(),
            modell: "Gemini-Test".into(),
            zweck: "nachrichten-vorsortieren".into(),
            ergebnis: "uebersprungen".into(),
            http: None,
            token_ein: None,
            token_aus: None,
            buchung: None,
            kosten_cent: None,
            system: None,
            frage: None,
            antwort: None,
            fehler: Some(depot_kern::gemini::KEINE_NACHRICHTEN.into()),
        },
    )
    .map_err(|e| e.to_string())?;
    let html = get(&d, "/teil/live", zeit);
    let (zeitstrahl, taetigkeit) = some(html.split_once("id=\"taetigkeit\""), "Tätigkeit")?;
    assert!(zeitstrahl.contains(&format!("wer=gemini&amp;lauf={lauf}")));
    assert!(zeitstrahl.contains("Nicht gefragt: Keine belegten Nachrichten. · optional"));
    let (_, gemini_zeile) = some(taetigkeit.split_once(&lauf), "Gemini-Zeile")?;
    let (gemini_ergebnis, _) = some(gemini_zeile.split_once("</tr>"), "Gemini-Ergebnis")?;
    assert!(gemini_ergebnis.contains("Nicht gefragt: Keine belegten Nachrichten."));
    assert!(!gemini_ergebnis.contains("erfolgreich"));
    Ok(())
}

#[test]
fn einrichtung_zeigt_grenzen_und_gemini_filter_vor_dem_ersten_aufruf() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let zeit = donnerstag(17, 39)?;
    let seite = get(&d, "/einrichtung", zeit);
    let journal = some(seite.find("id=\"journal\""), "Journal")?;
    let zugaenge = some(seite.find("id=\"schluessel\""), "Zugangsdaten")?;
    let selten = some(seite.find("id=\"selten\""), "Seltene Einstellungen")?;
    let zusatz = some(seite.find("id=\"zusatz\""), "Zusatz-KIs")?;
    let dienst = some(seite.find("<h2>Dienst</h2>"), "Dienst")?;
    assert!(journal < zugaenge && zugaenge < selten && selten < zusatz && zusatz < dienst);
    for text in [
        "Kursquelle Alpha Vantage (Xetra)",
        "Kursquelle Tiingo (US-Börsen)",
        "Papier-Sperre bei Gesamtverlust</td><td class=\"z\">30 %",
        "Kurshöchstalter",
        "Maximaler Kurssprung",
        "Maximaler Limitabstand",
        "Papiergebühr je Order",
        "Gemini-Aufrufe je Tag",
    ] {
        assert!(seite.contains(text), "{text}");
    }
    assert!(!get(&d, "/gespraeche", zeit).contains("<option value=\"gemini\""));
    fs::write(
        depot_lage::gespraeche::gemini_pfad(&d.wurzel.join("daten")),
        "gratisprojekt=ja\n",
    )
    .map_err(|e| e.to_string())?;
    assert!(get(&d, "/gespraeche", zeit).contains("<option value=\"gemini\">Gemini"));
    Ok(())
}

#[test]
fn gemini_bleibt_ohne_bestaetigtes_gratisprojekt_aus() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(17, 50)?;
    let html = get(&d, "/einrichtung", t);
    assert!(html.contains("Zusatz-KIs (optional)"));
    assert!(html.contains("Ausgeschaltet</strong> · Aufrufe heute 0 von 5"));
    assert!(html.contains(r#"value="nein" disabled>Gemini ausschalten"#));
    let ziel = d.wurzel.join("daten");
    let sende = |text: &str| {
        let a = formular_post("/gemini", &format!("t={TOKEN}&{text}"));
        crate::text(&behandle(&d, &Attrappe::default(), &a, t))
    };
    assert!(sende("an=ja").starts_with("HTTP/1.1 400"));
    assert!(!depot_lage::gespraeche::gemini_aktiv(&ziel));
    assert!(sende("an=ja&gratisprojekt=ja").contains("/einrichtung#zusatz"));
    assert!(depot_lage::gespraeche::gemini_aktiv(&ziel));
    let umw = Attrappe {
        gemini_schluessel: Some(Hinterlegt::Unbekannt),
        ..Attrappe::default()
    };
    let html = text(&behandle(
        &d,
        &umw,
        &anfrage("GET", "/einrichtung", &[("Host", HOST)], ""),
        t,
    ));
    assert!(html.contains("Eingeschaltet</strong>"));
    assert!(html.contains(r#"value="ja" disabled>Gemini einschalten"#));
    assert!(html.contains("Anmeldeinformationsverwaltung nicht prüfbar"));
    let speichere = formular_post(
        "/schluessel",
        &format!("t={TOKEN}&wer=gemini&wert=ungueltigAlsEchterSchluessel"),
    );
    let antwort = text(&behandle(&d, &Attrappe::default(), &speichere, t));
    assert!(antwort.contains("/einrichtung?m=schluessel&wer=gemini#zusatz"));
    assert!(get(&d, "/einrichtung", t).contains("hinterlegt, noch nicht benutzt"));
    let rechnen = depot_dienst::lagerechner(Arc::new(Attrappe::default()));
    let budget = depot_lage::html::seite(&rechnen(&d.wurzel, t));
    assert!(budget.contains("<h3>Gemini</h3><p class=\"gut\">Eingeschaltet"));
    assert!(budget.contains("<dt>Aufrufe heute</dt><dd>0 von 5</dd>"));
    assert!(sende("an=nein").contains("/einrichtung#zusatz"));
    assert!(!depot_lage::gespraeche::gemini_aktiv(&ziel));
    Ok(())
}

#[test]
fn gemini_speichern_meldet_fehlenden_ruecklesebeleg_am_feld() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let t = donnerstag(17, 50)?;
    let umwelt = Attrappe {
        gemini_schluessel: Some(Hinterlegt::Nein),
        ..Attrappe::default()
    };
    let probe = "ungueltigAlsEchterSchluessel";
    let post = formular_post("/schluessel", &format!("t={TOKEN}&wer=gemini&wert={probe}"));
    let antwort = text(&behandle(&d, &umwelt, &post, t));
    assert!(antwort.contains("/einrichtung?m=schluessel_fehler&wer=gemini#zusatz"));
    for meldung in ["schluessel_fehler", "schluessel", "schluessel_protokoll"] {
        let html = text(&behandle(
            &d,
            &umwelt,
            &anfrage(
                "GET",
                &format!("/einrichtung?m={meldung}&wer=gemini"),
                &[("Host", HOST)],
                "",
            ),
            t,
        ));
        let (_, zusatz) = some(html.split_once("id=\"zusatz\""), "Gemini-Abschnitt")?;
        assert!(zusatz.contains("Gemini-Schlüssel konnte nicht bestätigt werden."));
        assert!(!html.contains("Schlüssel in der Anmeldeinformationsverwaltung gespeichert."));
        assert!(!html.contains(probe));
    }
    Ok(())
}

#[test]
fn gemini_laeuft_vor_papier_und_speichert_stderr() -> Result<(), String> {
    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    fs::write(
        depot_lage::gespraeche::gemini_pfad(&daten),
        "gratisprojekt=ja\n",
    )
    .map_err(|e| e.to_string())?;
    let starter: Starter = Box::new(|art, _| {
        let mut c = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/C", "echo Probe-Standardfehler 1>&2"]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", "echo Probe-Standardfehler >&2"]);
            c
        };
        if art != Art::Gemini {
            c = echo_starter(true)(art, Path::new("."));
        }
        c
    });
    let d = Arc::new(Dienst::neu(
        wurzel.clone(),
        TOKEN.into(),
        starter,
        leeres_lagebild(),
    ));
    let zeit = donnerstag(7, 30)?;
    d.einreihen(Art::Papier, Anlass::Hand)?;
    let (nr, kind) = some(d.schritt(zeit), "Gemini gestartet")?;
    assert_eq!(
        d.sperre().laufend.as_ref().map(|a| a.art),
        Some(Art::Gemini)
    );
    assert_eq!(
        d.sperre().warteschlange.front().map(|a| a.art),
        Some(Art::Papier)
    );
    assert!(d.schritt(zeit).is_none());
    d.verfolge(nr, kind, || donnerstag(7, 31).ok());
    assert!(d.sperre().verlauf.front().is_some_and(|a| {
        a.art == Art::Gemini
            && a.protokoll
                .iter()
                .any(|z| z.contains("Probe-Standardfehler"))
    }));
    let neu = dienst(wurzel, true);
    assert_eq!(
        neu.sperre().verlauf.front().map(|a| a.art),
        Some(Art::Gemini)
    );
    let (nr, kind) = some(d.schritt(donnerstag(7, 31)?), "Papier gestartet")?;
    assert_eq!(
        d.sperre().laufend.as_ref().map(|a| a.art),
        Some(Art::Papier)
    );
    d.verfolge(nr, kind, || donnerstag(7, 32).ok());
    Ok(())
}

#[test]
fn otter_auf_live_bleibt_ausserhalb_der_aktualisierten_teile() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let zeit = donnerstag(12, 0)?;
    let html = get(&d, "/", zeit);
    assert_eq!(html.matches("class=\"otter\"").count(), 1);
    assert!(html.contains("prefers-reduced-motion:no-preference"));
    assert!(html.contains("</div>\n<div class=\"otter\" aria-hidden=\"true\">"));
    for pfad in ["/teil/kopf", "/teil/live", "/einrichtung"] {
        assert!(!get(&d, pfad, zeit).contains("class=\"otter\""), "{pfad}");
    }
    Ok(())
}

#[test]
fn otter_wird_bei_einem_auftrag_wach_und_ruht_danach() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let zeit = donnerstag(12, 0)?;
    let pose = |pose| get(&d, "/teil/kopf", zeit).contains(&format!("data-otter=\"{pose}\""));
    assert!(pose("ruht"));
    d.einreihen(Art::Termine, Anlass::Hand)?;
    let (nr, kind) = some(d.schritt(zeit), "Termine gestartet")?;
    assert!(pose("wach"));
    d.verfolge(nr, kind, || donnerstag(12, 1).ok());
    assert!(pose("ruht"));
    Ok(())
}

#[test]
fn betriebsleiste_zeigt_kurssperre_mit_art_zeit_und_grund() -> Result<(), String> {
    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    let d = dienst(wurzel, true);
    let zeit = donnerstag(12, 0)?;
    let vorher = get(&d, "/teil/kopf", zeit);
    assert!(!vorher.contains("Kurssperre seit"));
    let schutz = depot_lage::kurssperre::sperren(&daten)?;
    schutz.sicherheit(
        depot_lage::kurssperre::Quelle::AlphaVantage,
        Some(zeit),
        "Prüfgrund",
    )?;
    drop(schutz);
    let html = get(&d, "/teil/kopf", zeit);
    assert!(html.contains("Kurssperre seit") && html.contains("Prüfgrund · sicherheit"));
    assert!(html.contains(&depot_lage::zeit_text(zeit)));
    assert!(html.contains("data-otter=\"pfote\""));
    assert!(html.contains("/einrichtung#kurssperre"));
    Ok(())
}

#[test]
fn automatisch_aufgehobene_kurssperre_erzeugt_einen_hinweis() -> Result<(), String> {
    use depot_lage::kurssperre;
    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    let zeit = donnerstag(12, 0)?;
    fs::write(
        daten.join(kurssperre::KURSSPERRE),
        "Sicherheitsabbruch: Die Kursquelle gab den Schlüssel zurück.",
    )
    .map_err(|e| e.to_string())?;
    let schutz = kurssperre::sperren(&daten)?;
    let sperre = some(kurssperre::lies(&daten)?, "Altsperre")?;
    schutz.fehlalarm_aufheben(&sperre, zeit)?;
    drop(schutz);
    let gruende = Gruende::default();
    let (d, meldungen) = hinweis_dienst(wurzel.clone(), &gruende, false);
    assert!(d.schritt(zeit).is_none());
    assert!(d.schritt(donnerstag(12, 1)?).is_none());
    assert_eq!(
        gezeigt(&meldungen)
            .iter()
            .filter(|s| s.contains("Kurssperre aufgehoben: Fehlalarm"))
            .count(),
        1
    );
    drop(d);
    let (d, meldungen) = hinweis_dienst(wurzel, &gruende, false);
    assert!(d.schritt(donnerstag(12, 2)?).is_none());
    assert!(gezeigt(&meldungen).is_empty());
    Ok(())
}

#[test]
fn heute_exportieren_liefert_zip_ohne_schreibzugriff() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let jetzt = donnerstag(12, 0)?;
    let daten = wurzel.join("daten");
    let journal = daten.join("journal.txt");
    let vermerk = daten.join("journal.vermerk");
    depot_journal::Journal::einrichten(&journal).map_err(|e| format!("{e:?}"))?;
    let lies = |p| fs::read(p).map_err(|e| e.to_string());
    let vorher = (lies(&journal)?, lies(&vermerk)?);
    let umwelt = Attrappe::default();
    let live = behandle(
        &d,
        &umwelt,
        &anfrage("GET", "/", &[("Host", HOST)], ""),
        jetzt,
    );
    let live = text(&live);
    let (fest, _) = live
        .split_once("data-teil=\"/teil/live\"")
        .ok_or("Live-Teil fehlt")?;
    assert!(fest.contains("action=\"/export\" data-download"));
    let teil = anfrage("GET", "/teil/live", &[("Host", HOST)], "");
    assert!(!text(&behandle(&d, &umwelt, &teil, jetzt)).contains("action=\"/export\""));
    let post = formular_post("/export", &format!("t={TOKEN}"));
    let lesen = behandle(&d, &umwelt, &post, jetzt);
    assert_eq!(status(&lesen)?, 200);
    let antwort = text(&lesen);
    assert!(antwort.contains("Content-Type: application/zip"));
    assert!(antwort.contains("filename=\"DepotSchlopf_Analyse_2026-11-05.zip\""));
    assert!(antwort.contains("\r\n\r\nPK"));
    for datei in [
        "LIESMICH.md",
        "01_ueberblick.md",
        "03_journal.md",
        "ausfuehrungen.csv",
        "MANIFEST.txt",
    ] {
        assert!(antwort.contains(datei));
    }
    let zeitraum = formular_post(
        "/export",
        &format!("t={TOKEN}&zeitraum=frei&von=2026-11-03&bis=2026-11-05"),
    );
    let lesen = behandle(&d, &umwelt, &zeitraum, jetzt);
    assert_eq!(status(&lesen)?, 200);
    assert!(text(&lesen).contains("DepotSchlopf_Analyse_2026-11-03_bis_2026-11-05.zip"));
    let zukunft = formular_post(
        "/export",
        &format!("t={TOKEN}&zeitraum=frei&von=2026-11-03&bis=2026-11-06"),
    );
    assert_eq!(status(&behandle(&d, &umwelt, &zukunft, jetzt))?, 400);
    assert_eq!((lies(&journal)?, lies(&vermerk)?), vorher);

    let sperre = depot_journal::Journal::oeffnen(&journal).map_err(|e| format!("{e:?}"))?;
    assert_eq!(status(&behandle(&d, &umwelt, &post, jetzt))?, 409);
    drop(sperre);

    let fremd = anfrage(
        "POST",
        "/export",
        &[("Host", HOST), ("Origin", "https://example.org")],
        &format!("t={TOKEN}"),
    );
    assert_eq!(status(&behandle(&d, &umwelt, &fremd, jetzt))?, 403);
    let get = anfrage("GET", "/export", &[("Host", HOST)], "");
    assert_eq!(status(&behandle(&d, &umwelt, &get, jetzt))?, 405);
    fs::remove_dir_all(wurzel).map_err(|e| e.to_string())
}

#[test]
fn ohne_ziel_kein_lauf() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let zeit = donnerstag(23, 30)?;
    assert!(d.schritt(zeit).is_none());
    assert!(get(&d, "/teil/live", zeit).contains("Sicherung: aus"));
    assert!(get(&d, "/einrichtung", zeit).contains("Jetzt sichern"));
    Ok(())
}

#[test]
fn zeitplan_alte_datei() -> Result<(), String> {
    let wurzel = eingeschaltet(ordner()?)?;
    let datei = wurzel.join("daten/zeitplan.txt");
    let vorher = fs::read_to_string(&datei).map_err(|e| e.to_string())?;
    assert!(!vorher.contains("sicherung"));
    assert!(Zeitplan::lies(&vorher).is_ok());
    let d = dienst(wurzel, true);
    assert!(get(&d, "/teil/live", donnerstag(12, 0)?).contains("Sicherung: aus"));
    Ok(())
}

#[test]
fn sicherung_formulare_speichern_und_starten() -> Result<(), String> {
    let wurzel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let zeit = donnerstag(12, 0)?;
    let html = get(&d, "/einrichtung", zeit);
    let (_, formular) = some(html.split_once("id=\"sicherung\""), "Sicherungsabschnitt")?;
    assert!(formular.contains("action=\"/sicherung-ziel\""));
    assert!(formular.contains("value=\"starten\">Jetzt sichern"));
    assert!(formular.contains("data-teil=\"/teil/sicherung\""));
    assert_eq!(formular.contains("value=\"waehlen\""), cfg!(windows));
    let ziel = if cfg!(windows) {
        "E%3A%5C"
    } else {
        "%2FDepotSicherung"
    };
    let erwartet = if cfg!(windows) {
        "E:\\"
    } else {
        "/DepotSicherung"
    };
    let starten = formular_post(
        "/sicherung-ziel",
        &format!("t={TOKEN}&aktion=starten&ziel={ziel}"),
    );
    let antwort = text(&behandle(&d, &Attrappe::default(), &starten, zeit));
    assert!(
        antwort.contains("Location: /einrichtung?m=sicherung_start#sicherung"),
        "{antwort}"
    );
    assert_eq!(
        fs::read_to_string(wurzel.join("daten/sicherung_ziel.txt"))
            .map_err(|e| e.to_string())?
            .trim(),
        erwartet
    );
    assert_eq!(
        d.sperre().warteschlange.front().map(|a| a.art),
        Some(Art::Sicherung)
    );
    assert!(get(&d, "/teil/sicherung", zeit).contains("Sicherung wartet auf den Start"));
    let wechseln = formular_post(
        "/sicherung-ziel",
        &format!("t={TOKEN}&aktion=speichern&ziel="),
    );
    assert_eq!(
        status(&behandle(&d, &Attrappe::default(), &wechseln, zeit))?,
        409
    );
    assert_eq!(
        fs::read_to_string(wurzel.join("daten/sicherung_ziel.txt"))
            .map_err(|e| e.to_string())?
            .trim(),
        erwartet
    );
    Ok(())
}

#[test]
fn sicherungsdialog_und_anzahl_am_ziel() -> Result<(), String> {
    let wurzel = ordner()?;
    let ziel = ordner()?;
    let d = dienst(wurzel.clone(), true);
    let zeit = donnerstag(12, 0)?;
    let auswahl = formular_post("/sicherung-ziel", &format!("t={TOKEN}&aktion=waehlen"));
    let antwort = text(&behandle(&d, &Attrappe::default(), &auswahl, zeit));
    assert!(antwort.contains("Location: /einrichtung#sicherung"));
    assert!(!wurzel.join("daten/sicherung_ziel.txt").exists());

    let umwelt = Attrappe {
        sicherungsziel: Some(ziel.display().to_string()),
        ..Attrappe::default()
    };
    let antwort = text(&behandle(&d, &umwelt, &auswahl, zeit));
    assert!(antwort.contains("Location: /einrichtung?m=sicherung_ziel#sicherung"));
    assert!(d.sperre().warteschlange.is_empty());
    assert!(get(&d, "/teil/sicherung", zeit).contains("Sicherungen am Ziel: 0"));

    let sicherungen = ziel.join("DepotSchlopf");
    let name = format!("utc-00000000000000000001-000000001_{}", "a".repeat(40));
    fs::create_dir_all(sicherungen.join(&name)).map_err(|e| e.to_string())?;
    fs::create_dir(sicherungen.join(format!("{name}.tmp"))).map_err(|e| e.to_string())?;
    fs::create_dir(sicherungen.join("fremd")).map_err(|e| e.to_string())?;
    assert!(get(&d, "/teil/sicherung", zeit).contains("Sicherungen am Ziel: 1"));
    fs::remove_dir_all(ziel).map_err(|e| e.to_string())?;
    assert!(get(&d, "/teil/sicherung", zeit).contains("Anzahl nicht verfügbar"));
    let ungueltig = formular_post(
        "/sicherung-ziel",
        &format!("t={TOKEN}&aktion=starten&ziel=relativ"),
    );
    assert_eq!(status(&behandle(&d, &umwelt, &ungueltig, zeit))?, 400);
    assert!(d.sperre().warteschlange.is_empty());
    Ok(())
}

#[test]
fn depot_seite_da() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let html = get(&d, "/depot", donnerstag(12, 0)?);
    assert!(html.contains("200 OK"), "{html}");
    assert!(
        html.contains("aria-current=\"page\" href=\"/depot\"")
            || html.contains("href=\"/depot\" aria-current=\"page\"")
    );
    Ok(())
}

#[test]
fn menue_ueberall() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    for pfad in ["/", "/gespraeche", "/einrichtung"] {
        assert!(
            get(&d, pfad, donnerstag(12, 0)?).contains("href=\"/depot\""),
            "{pfad}"
        );
    }
    Ok(())
}

#[test]
fn depot_ohne_journal() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let html = get(&d, "/depot", donnerstag(12, 0)?);
    assert!(html.contains("Noch kein Depot: Es entsteht mit dem ersten Papiertag."));
    Ok(())
}

#[test]
fn depot_kein_auffrischen() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let html = get(&d, "/depot", donnerstag(12, 0)?);
    assert_eq!(html.matches("data-teil=").count(), 1);
    assert!(html.contains("Seite neu laden für den neuesten Stand"));
    Ok(())
}

#[test]
fn depot_journal_kaputt() -> Result<(), String> {
    let wurzel = ordner()?;
    fs::write(wurzel.join("daten/journal.txt"), "kaputt\n").map_err(|e| e.to_string())?;
    let d = dienst(wurzel, true);
    let html = get(&d, "/depot", donnerstag(12, 0)?);
    assert!(html.contains("class=\"halt\">Halt:"), "{html}");
    assert!(!html.contains("Gesamtwert"));
    Ok(())
}

fn depot_mit_positionen() -> Result<Arc<Dienst>, String> {
    use depot_journal::{Journal, KursBeleg};
    use depot_kern::{Cent, Herkunft, Isin, Kurs, Topf, TopfId};

    let wurzel = ordner()?;
    let journal = wurzel.join("daten/journal.txt");
    Journal::einrichten(&journal).map_err(|e| format!("{e:?}"))?;
    let mut j = Journal::oeffnen(&journal).map_err(|e| format!("{e:?}"))?;
    let mut topf = Topf::neu(TopfId::A, Cent(566_667)).map_err(|e| format!("{e:?}"))?;
    let abruf = donnerstag(17, 40)?;
    let tag = some(abruf.frankfurt_tag(), "Kurstag")?;
    let schluss = some(Boerse::Xetra.schluss(tag), "Schluss")?;
    for (isin, preis) in [
        ("DE0007164600", 333_333),
        ("DE0008404005", 90_000),
        ("DE000BASF111", 10_000),
    ] {
        let isin = Isin::neu(isin).map_err(|e| format!("{e:?}"))?;
        topf.setze_bestand(isin.clone(), 1);
        for datum in [tag, some(Boerse::Xetra.bezugstag(tag), "Vortag")?] {
            let zeit = some(Boerse::Xetra.schluss(datum), "Schluss")?;
            let kurs = Kurs::neu(
                Cent(preis),
                "EUR",
                datum,
                zeit.min(schluss),
                Herkunft::Quelle,
            )
            .map_err(|e| format!("{e:?}"))?;
            let beleg = KursBeleg::neu("alphavantage", abruf, b"Depotprobe", isin.clone(), kurs)
                .map_err(|e| format!("{e:?}"))?;
            j.belege_kurs(&beleg).map_err(|e| format!("{e:?}"))?;
        }
    }
    j.setze_topf(&topf).map_err(|e| format!("{e:?}"))?;
    drop(j);
    Ok(dienst(wurzel, true))
}

#[test]
fn depot_sortierung() -> Result<(), String> {
    let d = depot_mit_positionen()?;
    let html = get(&d, "/depot", donnerstag(18, 0)?);
    let tabelle = some(
        html.split_once("Wo bin ich investiert?"),
        "Positionstabelle",
    )?
    .1;
    let mut zuletzt = 0;
    for wort in ["SAP", "Allianz", "BASF", "Bargeld"] {
        let stelle = some(tabelle.find(wort), wort)?;
        assert!(stelle > zuletzt);
        zuletzt = stelle;
    }
    Ok(())
}

#[test]
fn depot_anteil() -> Result<(), String> {
    let d = depot_mit_positionen()?;
    let html = get(&d, "/depot", donnerstag(18, 0)?);
    assert!(html.contains("33,33 %"), "{html}");
    Ok(())
}

#[test]
fn depot_einstand_nicht_belegt() -> Result<(), String> {
    let d = depot_mit_positionen()?;
    let html = get(&d, "/depot", donnerstag(18, 0)?);
    assert!(html.contains("<td>nicht belegt</td><td>–</td>"));
    Ok(())
}

#[test]
fn depot_tagesergebnis_ohne_vortag() -> Result<(), String> {
    let d = depot_mit_positionen()?;
    assert!(get(&d, "/depot", donnerstag(18, 0)?).contains("noch kein Vortag"));
    Ok(())
}

fn depot_mit_verlauf(tage: &[depot_kern::Datum]) -> Result<Arc<Dienst>, String> {
    use depot_journal::Journal;
    use depot_kern::{Cent, Isin, Topf, TopfId};

    let wurzel = ordner()?;
    let journal = wurzel.join("daten/journal.txt");
    Journal::einrichten(&journal).map_err(|e| format!("{e:?}"))?;
    let mut j = Journal::oeffnen(&journal).map_err(|e| format!("{e:?}"))?;
    let mut topf = Topf::neu(TopfId::A, Cent(900_000)).map_err(|e| format!("{e:?}"))?;
    topf.setze_bestand(Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?, 1);
    j.setze_topf(&topf).map_err(|e| format!("{e:?}"))?;
    for tag in tage {
        j.schliesse_ab(*tag, Cent(1_000_000), &"a".repeat(64))
            .map_err(|e| format!("{e:?}"))?;
    }
    drop(j);
    Ok(dienst(wurzel, true))
}

#[test]
fn zeitraum_links() -> Result<(), String> {
    let alt = some(depot_kern::Datum::neu(2026, 7, 13), "Alter Abschluss")?;
    let neu = some(depot_kern::Datum::neu(2026, 10, 13), "Neuer Abschluss")?;
    let d = depot_mit_verlauf(&[alt, neu])?;
    let jetzt = some(
        Zeitpunkt::frankfurt(
            some(depot_kern::Datum::neu(2026, 11, 13), "Heute")?,
            18,
            0,
            0,
        ),
        "Uhrzeit",
    )?;
    let html = get(&d, "/depot?zeitraum=3m", jetzt);
    assert!(html.contains("/depot?zeitraum=3m\" aria-current=\"true\">3 Monate</a>"));
    assert!(html.contains("<th scope=\"row\">13.10.2026</th>"));
    assert!(!html.contains("<th scope=\"row\">13.07.2026</th>"));
    Ok(())
}

#[test]
fn zeitraum_leer() -> Result<(), String> {
    let alt = some(depot_kern::Datum::neu(2026, 9, 10), "Alter Abschluss")?;
    let d = depot_mit_verlauf(&[alt])?;
    let jetzt = some(
        Zeitpunkt::frankfurt(
            some(depot_kern::Datum::neu(2026, 11, 13), "Heute")?,
            18,
            0,
            0,
        ),
        "Uhrzeit",
    )?;
    let html = get(&d, "/depot?zeitraum=1m", jetzt);
    assert!(html.contains("Im gewählten Zeitraum gibt es noch keinen abgeschlossenen Papiertag."));
    assert!(!html.contains("<svg"));
    Ok(())
}

fn depot_mit_order() -> Result<Arc<Dienst>, String> {
    use depot_journal::Art::{Auftrag, Topf, Zustand};
    let wurzel = ordner()?;
    let inhalt = depot_journal::inhalt_fuer(&[
        (Topf, "A;500000;"),
        (Auftrag, "1;A;DE0007164600;Kauf;10;10000;Umschichtung"),
        (Zustand, "1;gesendet"),
        (Zustand, "1;angenommen"),
    ]);
    let vermerk = some(depot_journal::vermerk_fuer(&inhalt, 0), "Journalvermerk")?;
    fs::write(wurzel.join("daten/journal.txt"), inhalt).map_err(|e| e.to_string())?;
    fs::write(wurzel.join("daten/journal.vermerk"), vermerk).map_err(|e| e.to_string())?;
    Ok(dienst(wurzel, true))
}

#[test]
fn depot_ohne_technik() -> Result<(), String> {
    let d = depot_mit_order()?;
    let html = get(&d, "/depot", donnerstag(18, 0)?);
    assert!(html.contains("liegt beim Papiermakler"), "{html}");
    for roh in [
        "Some(",
        "None",
        "Teilgefuellt",
        "Reserviert",
        "DE0007164600",
    ] {
        assert!(!html.contains(roh), "{roh}");
    }
    Ok(())
}

#[test]
fn live_zustand_in_worten() -> Result<(), String> {
    let d = depot_mit_order()?;
    let zeit = donnerstag(18, 0)?;
    let bild = depot_lage::mit_journal(&d.wurzel.join("daten"), |journal| {
        lagebild(&Umgebung {
            jetzt: zeit,
            journal,
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
    });
    d.sperre().lage = Some(depot_dienst::werkbank::Lage { stand: zeit, bild });
    let html = get(&d, "/teil/live", zeit);
    assert!(
        html.contains("liegt beim Papiermakler") && html.contains("Mehr zum Depot"),
        "{html}"
    );
    assert!(!html.contains("Angenommen"));
    Ok(())
}
