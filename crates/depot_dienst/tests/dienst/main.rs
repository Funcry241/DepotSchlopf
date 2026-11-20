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
fn morgenplan_test() -> Zeitplan {
    Zeitplan {
        termine: (7, 20),
        kurse: (7, 30),
        ..Zeitplan::default()
    }
}

fn eingeschaltet(wurzel: PathBuf) -> Result<PathBuf, String> {
    let p = Zeitplan {
        an: true,
        kurse: (7, 30),
        ..morgenplan_test()
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
    gemini_speicherprobe: bool,
    sicherungsziel: Option<String>,
    dialogpause: Option<Dialogpause>,
}

struct Dialogpause {
    offen: std::sync::mpsc::Sender<()>,
    weiter: Mutex<std::sync::mpsc::Receiver<()>>,
}

impl Attrappe {
    fn warte_im_dialog(&self) -> Option<()> {
        if let Some(pause) = &self.dialogpause {
            pause.offen.send(()).ok()?;
            pause.weiter.lock().ok()?.recv().ok()?;
        }
        Some(())
    }
}

impl Umwelt for Attrappe {
    fn schluessel(&self, wer: Wer) -> Hinterlegt {
        if wer == Wer::Gemini && self.gemini_speicherprobe {
            let abgelegt = self.abgelegt.lock().unwrap_or_else(PoisonError::into_inner);
            if abgelegt.iter().any(|(wer, _)| *wer == Wer::Gemini) {
                Hinterlegt::Ja
            } else {
                Hinterlegt::Nein
            }
        } else if wer == Wer::Gemini {
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
        self.warte_im_dialog().ok_or("Testdialog abgebrochen")?;
        Ok(self.sicherungsziel.clone())
    }

    fn autostart(&self, _wurzel: &Path) -> depot_dienst::autostart::Stand {
        if *self
            .autostart
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            == Some(true)
        {
            depot_dienst::autostart::Stand::An
        } else {
            depot_dienst::autostart::Stand::Aus
        }
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
        self.warte_im_dialog()?;
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

fn protokoll_text(wurzel: &Path) -> String {
    fs::read_to_string(wurzel.join("daten").join(depot_lage::anbieter::DATEI)).unwrap_or_default()
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

fn aufheben(abdruck: &str, geprueft: bool) -> Anfrage {
    let haken = if geprueft { "&geprueft=ja" } else { "" };
    formular_post(
        "/kostenschutz",
        &format!("t={TOKEN}&abdruck={abdruck}{haken}"),
    )
}

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

/// Ein Dienst, dessen Liste nur Xetra-Werte hat: Der Papiertag folgt dem Xetra-Abruf, gleich
/// welche Boersen die echte Liste hat.
fn nur_xetra(wurzel: PathBuf) -> Arc<Dienst> {
    let d = dienst(wurzel, true);
    d.sperre().boersen = vec![Boerse::Xetra];
    d
}

/// Die Live-Ansicht nach einem Neustart: der Satz mit `id="neustart"`.
fn neustart_satz(d: &Dienst, t: Zeitpunkt) -> Option<String> {
    let seite = seiten::live(TOKEN, "s", &d.sperre(), t);
    let (_, rest) = seite.split_once("id=\"neustart\">")?;
    rest.split_once("</p>").map(|(s, _)| s.to_string())
}

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

mod depot;
mod dienstbetrieb;
mod einrichtung;
mod http_schnittstelle;
mod ibkr_papier;
mod ki_gespraeche;
mod live;
mod update_leiste;
