//! `depot_sammler` – holt Schlusskurse und Termine, belegt sie im Journal und prueft sie.
//!
//! Aufruf ueber `run.cmd kurse` (alle Werte), `run.cmd kurse xetra` oder `run.cmd kurse nyse`
//! (nur die Werte einer Boerse, kurz nach ihrem Schluss), `run.cmd kurse einrichten` beim
//! ersten Mal `run.cmd nachrichten` und `run.cmd termine`. Das Journal
//! liegt unter `daten/journal.txt`, die rohen Antworten unter `daten/roh/`; beides bleibt auf dem
//! eigenen Rechner (`.gitignore`, Export ohne `daten/`).

#![forbid(
    unsafe_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::float_arithmetic,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use depot_journal::{
    Journal, anbieter,
    kurssperre::{self, Quelle},
};
use depot_kern::{Boerse, Datum, Grenzen, Preis, Zeitpunkt, hinweis};
use depot_sammler::kontingent::KontingentNetz;
use depot_sammler::termine::sammle_termine_geschuetzt;
use depot_sammler::{
    Anbieter, Ergebnis, Https, Schluessel, Zugang, beobachtungsliste, einordnen,
    ergebnisse::sammle_ergebnisse, ezb::ADRESSE_EZB, netz, sammle, sammle_nachrichten,
    zugang_laden,
};

/// Abstand zwischen zwei Abrufen: hoechstens vier Abrufe je Minute.
const PAUSE: Duration = Duration::from_secs(15);

/// Der Projektordner: der naechste Ordner ab dem aktuellen, der `kodex/festschreibung.txt`
/// enthaelt.
fn wurzel() -> Option<PathBuf> {
    let start = std::env::current_dir().ok()?;
    start
        .ancestors()
        .find(|d| d.join("kodex").join("festschreibung.txt").is_file())
        .map(Path::to_path_buf)
}

fn jetzt() -> Option<Zeitpunkt> {
    let s = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    Zeitpunkt::aus_sekunden(i64::try_from(s).ok()?)
}

/// Exakt, ohne Rundung: `49,925 EUR`.
fn euro(p: Preis) -> String {
    format!("{} EUR", p.euro_text())
}

fn datum(d: Datum) -> String {
    format!("{:02}.{:02}.{}", d.tag(), d.monat(), d.jahr())
}

fn tag_preis(t: Option<(Datum, Preis)>) -> String {
    t.map_or_else(|| "-".into(), |(d, c)| format!("{} {}", datum(d), euro(c)))
}

fn einrichten(journal: &Path) -> ExitCode {
    if let Some(ordner) = journal.parent()
        && let Err(e) = std::fs::create_dir_all(ordner)
    {
        eprintln!("[Sammler] FEHLER: {}: {e}", ordner.display());
        return ExitCode::FAILURE;
    }
    match Journal::einrichten(journal) {
        Ok(()) => {
            println!("[Sammler] Journal eingerichtet: {}", journal.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("[Sammler] FEHLER: {e:?}");
            ExitCode::FAILURE
        }
    }
}

/// Haelt im Anbieterprotokoll fest, was der Lauf ueber Schluessel und Quelle sagt, je Quelle
/// eine Zeile, nur fuer Quellen, die er abgerufen hat.
fn vermerke(wurzel: &Path, bericht: &depot_sammler::Bericht) {
    for a in Anbieter::ALLE {
        if let (Some(ergebnis), Some(zeit)) = (einordnen(bericht, a), jetzt()) {
            let e = anbieter::Eintrag::neu(zeit, a.protokoll(), ergebnis, None);
            if let Err(f) = anbieter::anhaengen(&wurzel.join("daten").join(anbieter::DATEI), &e) {
                eprintln!("[Sammler] Anbieterprotokoll nicht geschrieben: {f}");
            }
        }
    }
}

/// Liegt die Kurssperre, wird nichts abgerufen.
fn gesperrt(wurzel: &Path) -> bool {
    if kurssperre::kurssperre(&wurzel.join("daten")).is_none() {
        return false;
    }
    eprintln!(
        "[Sammler] HALT: daten/{} liegt. Kein regulärer Kursabruf und kein Papiertag. Siehe Einrichtung → Kurssperre.",
        kurssperre::KURSSPERRE
    );
    true
}

/// Nach einem Sicherheitsabbruch: Kurssperre anlegen, damit auch ein Papiertag von Hand die
/// Teilkurse dieses Abrufs nie verwendet.
fn sperre(wurzel: &Path, quelle: Quelle, diagnose: Option<&str>, zugang: &Zugang) {
    let grund = "Sicherheitsabbruch: Die Kursquelle gab den Schlüssel zurück. Teilkurse dieses \
                 Abrufs nicht verwenden; Schlüssel prüfen und bei Bedarf erneuern";
    let grund = diagnose.map_or_else(|| grund.to_string(), |d| format!("{grund}. {d}"));
    let grund = zugang.bereinige(&grund);
    let ergebnis = kurssperre::sperren(&wurzel.join("daten"))
        .and_then(|schutz| schutz.sicherheit(quelle, jetzt(), &grund));
    match ergebnis {
        Ok(()) => eprintln!(
            "[Sammler] Kurssperre daten/{} angelegt.",
            kurssperre::KURSSPERRE
        ),
        Err(e) => eprintln!(
            "[Sammler] FEHLER: Kurssperre nicht angelegt ({e}). KEINEN Papiertag starten."
        ),
    }
}

/// Alle hinterlegten Schluessel, auch fuer Quellen, die dieser Lauf nicht abruft: Der Schutz
/// prueft jede Antwort gegen jeden. Gemeldet wird nur, was ein Wert des Laufs braucht.
fn zugang(werte: &[depot_sammler::Wert]) -> Zugang {
    let (zugang, meldungen) = zugang_laden(werte, Schluessel::aus_anmeldeverwaltung);
    for m in meldungen {
        eprintln!("[Sammler] FEHLER {m}");
    }
    zugang
}

/// Ein Lauf mit echtem Netz, echter Uhr und echtem Warten; jede Wiederholung steht als
/// Hinweiszeile in der Ausgabe.
fn echt_sammeln(
    journal: &mut Journal,
    zugang: &Zugang,
    ablage: &Path,
    werte: &[depot_sammler::Wert],
) -> Result<depot_sammler::Bericht, depot_sammler::SammelFehler> {
    let mut uhr = jetzt;
    let mut schlafe = std::thread::sleep;
    let mut melde = |v: &netz::Verlauf| {
        println!("{}", hinweis::zeile(&netz::verlauf_text(v)));
    };
    let mut takt = netz::Takt {
        uhr: &mut uhr,
        schlafe: &mut schlafe,
        melde: &mut melde,
    };
    let daten = ablage
        .parent()
        .ok_or_else(|| depot_sammler::SammelFehler::Ablage("Datenordner fehlt".into()))?;
    sammle(
        journal,
        &mut KontingentNetz::neu(Https::neu(), daten, jetzt),
        zugang,
        &mut takt,
        ablage,
        PAUSE,
        werte,
    )
}

fn pruefe_kurssperre(daten: &Path, netz: &mut dyn depot_sammler::Netz) {
    use depot_sammler::diagnose::AutomatischePruefung;
    if kurssperre::kurssperre(daten).is_none() {
        return;
    }
    let (Some(zeit), Ok(werte)) = (jetzt(), beobachtungsliste()) else {
        return;
    };
    let (zugang, _) = zugang_laden(&werte, Schluessel::aus_anmeldeverwaltung);
    let text = match depot_sammler::diagnose::pruefe_altsperre(daten, zeit, netz, &zugang, &werte) {
        Ok(AutomatischePruefung::KeinBedarf) => return,
        Ok(AutomatischePruefung::Aufgehoben) => "Kurssperre aufgehoben: Fehlalarm",
        Ok(AutomatischePruefung::Sicherheit) => {
            "Kurssperre bestätigt: Antwort enthielt einen Schlüssel"
        }
        Ok(AutomatischePruefung::Morgen) => {
            "Kurssperre bleibt aktiv; nächste automatische Prüfung frühestens morgen."
        }
        Err(_) => "Kurssperre konnte nicht automatisch geprüft oder aufgehoben werden.",
    };
    println!("{}", hinweis::zeile(&zugang.bereinige(text)));
}

fn kurse(wurzel: &Path, journal_pfad: &Path, boerse: Option<Boerse>) -> ExitCode {
    let daten = wurzel.join("daten");
    pruefe_kurssperre(
        &daten,
        &mut KontingentNetz::neu(Https::neu(), &daten, jetzt),
    );
    if gesperrt(wurzel) {
        return ExitCode::FAILURE;
    }
    let werte = match beobachtungsliste() {
        Ok(w) => w
            .into_iter()
            .filter(|w| boerse.is_none_or(|b| w.boerse == b))
            .collect::<Vec<_>>(),
        Err(e) => {
            eprintln!("[Sammler] FEHLER: {e}");
            return ExitCode::FAILURE;
        }
    };
    if werte.is_empty() {
        println!("[Sammler] Keine Werte dieser Börse in der Liste, nichts abzurufen.");
        return ExitCode::SUCCESS;
    }
    let schluessel = zugang(&werte);
    let mut journal = match Journal::oeffnen(journal_pfad) {
        Ok(j) => j,
        Err(e) => {
            eprintln!(
                "[Sammler] FEHLER: Journal {}: {e:?}. Beim ersten Mal: run.cmd kurse einrichten",
                journal_pfad.display()
            );
            return ExitCode::FAILURE;
        }
    };
    println!(
        "[Sammler] Hole {} Werte (Xetra: Alpha Vantage, US-Börsen: Tiingo mit EZB-Referenzkursen), {} Sekunden Abstand ...",
        werte.len(),
        PAUSE.as_secs()
    );
    let abrufbeginn = jetzt();
    let bericht = match echt_sammeln(
        &mut journal,
        &schluessel,
        &wurzel.join("daten").join("roh"),
        &werte,
    ) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[Sammler] FEHLER, Lauf angehalten: {e:?}");
            return ExitCode::FAILURE;
        }
    };
    let verwendbar = bericht.verwendbar();
    if let Some(quelle) = bericht.sperrquelle {
        sperre(wurzel, quelle, bericht.abbruch.as_deref(), &schluessel);
    }
    vermerke(wurzel, &bericht);
    zeige_werte(&bericht.werte);
    if let Some(a) = &bericht.abbruch {
        println!("[Sammler] Lauf vorzeitig beendet: {a}");
    }
    zeige_pruefung(&bericht);
    if !verwendbar
        && bericht.abbruch.is_none()
        && bericht.verbindung.is_none()
        && let Some(beginn) = abrufbeginn
        && !depot_kern::liste::morgenlauf(beginn)
    {
        for meldung in schlusskursstand(beginn, &bericht) {
            println!("[Sammler] {meldung}");
        }
    }
    if let Some(a) = &bericht.verbindung {
        println!(
            "{}",
            hinweis::zeile(&netz::ausfall_text(a, verwendbar, &Grenzen::standard()))
        );
    }
    println!(
        "[Sammler] Belegt im Journal {}, rohe Antworten unter daten/roh/.",
        journal_pfad.display()
    );
    // Danach kostenlose RSS-Schlagzeilen, unabhaengig von Kursschluesseln. Ohne
    // Nachrichten entscheiden die Berater aus Kursen und Terminen.
    if bericht.abbruch.is_none()
        && bericht.verbindung.is_none()
        && nachrichten_holen(&mut journal, &schluessel, wurzel)
            == NachrichtenAusgang::Sicherheitsabbruch
    {
        return ExitCode::FAILURE;
    }
    // Erfolg nur mit verwendbaren Kursen: Ein spaeterer Lauf darf sich auf den Ausgang
    // verlassen (K19-02).
    let code = kurs_exitcode(&bericht);
    if code == 0 {
        println!("[Sammler] Kurse verwendbar.");
    } else if bericht.abbruch.is_some() || bericht.verbindung.is_some() {
        println!("[Sammler] Kurse nicht verwendbar: Der Kursabruf wurde vorzeitig beendet.");
    } else {
        println!(
            "[Sammler] FEHLER: Kurse nicht verwendbar. Die Kursprüfung hat den Abruf nicht freigegeben."
        );
    }
    ExitCode::from(code)
}

fn kurs_exitcode(bericht: &depot_sammler::Bericht) -> u8 {
    if bericht.verwendbar() {
        0
    } else if bericht.nur_verbindungsfehler() {
        3
    } else {
        1
    }
}

/// Trennt einen noch offenen Handelstag von fehlenden Kursen nach Börsenschluss.
fn schlusskursstand(abrufbeginn: Zeitpunkt, bericht: &depot_sammler::Bericht) -> Vec<String> {
    let Some((tag, ..)) = abrufbeginn.in_frankfurt() else {
        return Vec::new();
    };
    Boerse::ALLE.into_iter().filter_map(|boerse| {
        let schluss = boerse.schluss(tag)?;
        if !boerse.handelstag(tag) {
            return None;
        }
        let mut gesamt = 0usize;
        let mut aktuell = 0usize;
        let mut letzter = None;
        for (_, ergebnis) in bericht.werte.iter().filter(|(w, _)| w.boerse == boerse) {
            gesamt = gesamt.saturating_add(1);
            if let Ergebnis::Belegt { heute: Some((belegt, _)), .. } = ergebnis {
                aktuell = aktuell.saturating_add(usize::from(*belegt == tag));
                letzter = letzter.max(Some(*belegt));
            }
        }
        if gesamt == 0 || aktuell == gesamt {
            return None;
        }
        let name = boerse.name();
        if abrufbeginn < schluss {
            let (_, h, m, ..) = schluss.in_frankfurt()?;
            return Some(format!(
                "{name}: Bei Abrufbeginn lief der Handel für {} noch (Börsenschluss {h:02}:{m:02} Uhr deutscher Zeit).",
                datum(tag)
            ));
        }
        let belegt = letzter.map_or_else(
            || "Kein Schlusskurs belegt.".into(),
            |d| format!("Jüngster belegter Schlusskurs vom {}.", datum(d)),
        );
        Some(format!(
            "{name}: Schlusskurse für {} nach Börsenschluss: {aktuell} von {gesamt} belegt. {belegt}",
            datum(tag)
        ))
    }).collect()
}

/// Je Wert eine Zeile mit Schlusskursen oder dem Grund, warum sie fehlen.
fn zeige_werte(werte: &[(depot_sammler::Wert, Ergebnis)]) {
    for (w, e) in werte {
        match e {
            Ergebnis::Belegt { heute, vortag, .. } => println!(
                "  {:<20} {}  Schluss {}  Vortag {}",
                w.name,
                w.isin,
                tag_preis(*heute),
                tag_preis(*vortag)
            ),
            Ergebnis::Fehler(t) | Ergebnis::Verbindung(t) => {
                println!("  {:<20} {}  FEHLER: {t}", w.name, w.isin);
            }
            Ergebnis::Uebersprungen(t) => {
                println!("  {:<20} {}  nicht abgerufen: {t}", w.name, w.isin);
            }
            Ergebnis::NichtAbgerufen => {
                println!("  {:<20} {}  nicht abgerufen", w.name, w.isin);
            }
        }
    }
}

/// Was die Pruefung sagt, wie der Papiertag sie ansetzt.
fn zeige_pruefung(bericht: &depot_sammler::Bericht) {
    match bericht.handelstag {
        None => println!("[Sammler] Keine Schlusskurse belegt."),
        Some(t) if bericht.pruefung.is_empty() => {
            println!(
                "[Sammler] Prüfung für den Handelstag {}: in Ordnung.",
                datum(t)
            );
            for f in &bericht.ausgelassen {
                println!("  heute nicht handelbar: {f:?}");
            }
        }
        Some(t) => {
            println!(
                "[Sammler] Kursprüfung für den jüngsten belegten Handelstag {}:",
                datum(t)
            );
            for f in &bericht.pruefung {
                if *f == depot_regelwerk::DatenFehler::AbrufNichtAmHandelstag {
                    println!(
                        "  Abrufdatum und der für die Prüfung angesetzte Handelstag stimmen nicht überein."
                    );
                } else {
                    println!("  {f:?}");
                }
            }
        }
    }
}

#[derive(PartialEq, Eq)]
enum NachrichtenAusgang {
    Ok,
    Fehler,
    Sicherheitsabbruch,
}

fn nachrichten_holen(
    journal: &mut Journal,
    schluessel: &Zugang,
    wurzel: &Path,
) -> NachrichtenAusgang {
    println!("[Sammler] Hole kostenlose Schlagzeilen von tagesschau und BBC World ...");
    match sammle_nachrichten(
        journal,
        &mut Https::neu(),
        schluessel,
        &mut jetzt,
        &wurzel.join("daten").join("roh"),
    ) {
        Ok(b) => {
            if b.sicherheitsabbruch {
                sperre(wurzel, Quelle::Rss, None, schluessel);
                return NachrichtenAusgang::Sicherheitsabbruch;
            }
            for text in &b.quellen {
                println!("[Sammler] {text}");
            }
            if let Some(d) = &b.rohdatei {
                println!("[Sammler] Rohe Antwort: {}", d.display());
            }
            if let Some(f) = &b.fehler {
                eprintln!("[Sammler] Nachrichten: nichts belegt: {f}");
                return NachrichtenAusgang::Fehler;
            }
            println!(
                "[Sammler] Nachrichten: {} neu, {} schon belegt.",
                b.neu, b.bekannt
            );
            NachrichtenAusgang::Ok
        }
        Err(e) => {
            eprintln!("[Sammler] FEHLER bei den Nachrichten, angehalten: {e:?}");
            NachrichtenAusgang::Fehler
        }
    }
}

fn nachrichten(wurzel: &Path, journal_pfad: &Path) -> ExitCode {
    if kurssperre::kurssperre(&wurzel.join("daten")).is_some() {
        println!(
            "[Hinweis] Kurssperre liegt; Schlagzeilen werden trotzdem geholt. Kurse und Papiertag bleiben gesperrt."
        );
    }
    let (schluessel, _) = zugang_laden(&[], Schluessel::aus_anmeldeverwaltung);
    let mut journal = match Journal::oeffnen(journal_pfad) {
        Ok(j) => j,
        Err(e) => {
            eprintln!(
                "[Sammler] FEHLER: Journal {}: {e:?}. Beim ersten Mal: run.cmd kurse einrichten",
                journal_pfad.display()
            );
            return ExitCode::FAILURE;
        }
    };
    if nachrichten_holen(&mut journal, &schluessel, wurzel) == NachrichtenAusgang::Ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn termine(wurzel: &Path, journal_pfad: &Path) -> ExitCode {
    let mut journal = match Journal::oeffnen(journal_pfad) {
        Ok(j) => j,
        Err(e) => {
            eprintln!(
                "[Sammler] FEHLER: Journal {}: {e:?}. Beim ersten Mal: run.cmd kurse einrichten",
                journal_pfad.display()
            );
            return ExitCode::FAILURE;
        }
    };
    println!("[Sammler] Hole die Termine der EZB von {ADRESSE_EZB} ...");
    let daten = wurzel.join("daten");
    let mut netz = KontingentNetz::neu(Https::neu(), &daten, jetzt);
    pruefe_kurssperre(&daten, &mut netz);
    let mut uhr = jetzt;
    let (zugang, _) = zugang_laden(&[], Schluessel::aus_anmeldeverwaltung);
    let bericht = match sammle_termine_geschuetzt(
        &mut journal,
        &mut netz,
        &mut uhr,
        &daten.join("roh"),
        &zugang,
    ) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[Sammler] FEHLER, Lauf angehalten: {e:?}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(d) = &bericht.rohdatei {
        println!("[Sammler] Rohe Seite: {}", d.display());
    }
    let ezb_ok = if let Some(f) = &bericht.fehler {
        eprintln!("[Sammler] FEHLER, nichts belegt: {f}");
        false
    } else {
        println!(
            "[Sammler] Termine: {} neu, {} geändert, {} unverändert. Belegt im Journal {}.",
            bericht.neu,
            bericht.geaendert,
            bericht.unveraendert,
            journal_pfad.display()
        );
        for k in &bericht.fehlen {
            println!("  {k}: steht im Journal, aber nicht mehr auf der Seite (nur gemeldet)");
        }
        true
    };
    if bericht.sicherheitsabbruch {
        sperre(wurzel, Quelle::Ezb, bericht.fehler.as_deref(), &zugang);
    }
    let kalender_ok = quartalszahlen(wurzel, &mut journal, &mut netz);
    if ezb_ok && kalender_ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Die Quartalszahlen der US-Werte aus dem Ergebniskalender von Alpha Vantage (#120). Optional:
/// ohne Schluessel, schon heute abgerufen oder mit Hinweis der Quelle laeuft der Lauf weiter.
/// `false` nur nach einem Sicherheitsabbruch (Kurssperre) oder einem Fehler von Journal,
/// Ablage oder Uhr.
fn quartalszahlen(
    wurzel: &Path,
    journal: &mut Journal,
    netz: &mut dyn depot_sammler::Netz,
) -> bool {
    if kurssperre::kurssperre(&wurzel.join("daten")).is_some() {
        println!(
            "[Hinweis] Kurssperre liegt; der Alpha-Vantage-Kalender wird übersprungen. EZB-Termine laufen unabhängig davon."
        );
        return true;
    }
    let werte = match beobachtungsliste() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("[Sammler] Quartalszahlen: Liste: {e}");
            return false;
        }
    };
    let zugang = zugang(&werte);
    let mut uhr = jetzt;
    println!("[Sammler] Hole die Quartalstermine der US-Werte von Alpha Vantage ...");
    let b = match sammle_ergebnisse(
        journal,
        netz,
        &mut uhr,
        &wurzel.join("daten").join("roh"),
        &zugang,
        &werte,
    ) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[Sammler] Quartalszahlen: FEHLER, Lauf angehalten: {e:?}");
            return false;
        }
    };
    if let Some(grund) = &b.uebersprungen {
        println!("[Sammler] Quartalszahlen: kein Abruf ({grund}).");
        return true;
    }
    if b.sicherheitsabbruch {
        sperre(wurzel, Quelle::AlphaVantage, None, &zugang);
        eprintln!("[Sammler] Quartalszahlen: HALT, die Antwort enthielt einen Schlüssel.");
        return false;
    }
    if let Some(d) = &b.rohdatei {
        println!("[Sammler] Rohe Antwort: {}", d.display());
    }
    if let Some(f) = &b.fehler {
        println!("[Sammler] Quartalszahlen: nichts belegt ({f}). Die übrigen Termine gelten.");
        return true;
    }
    println!(
        "[Sammler] Quartalszahlen: {} neu, {} geändert, {} unverändert.",
        b.neu, b.geaendert, b.unveraendert
    );
    for k in &b.fehlen {
        println!("  {k}: steht im Journal, aber nicht mehr im Kalender (nur gemeldet)");
    }
    true
}

/// Die einzige Ausnahme vom Abrufhalt: ausdrücklich gestartete, rein lesende Diagnose.
/// Sie öffnet weder Journal noch Rohablage und ändert die bestehende Sperre nicht.
fn kursdiagnose(wurzel: &Path) -> ExitCode {
    if kurssperre::kurssperre(&wurzel.join("daten")).is_none() {
        println!("[Sammler] Keine Kurssperre vorhanden: kein Prüfabruf nötig, nichts abgerufen.");
        return ExitCode::SUCCESS;
    }
    let Ok(werte) = beobachtungsliste() else {
        eprintln!("[Sammler] FEHLER: Beobachtungsliste ungültig, nichts abgerufen.");
        return ExitCode::FAILURE;
    };
    println!(
        "[Sammler] Prüfabruf: bis zu {} Werte, 15 Sekunden Abstand, ein Versuch je Wert. \
         Verbraucht Abrufkontingent. Keine Kurse oder Antworten werden gespeichert; die Sperre bleibt bestehen.",
        werte.len()
    );
    let (zugang, _) = zugang_laden(&werte, Schluessel::aus_anmeldeverwaltung);
    let daten = wurzel.join("daten");
    depot_sammler::diagnose::pruefabruf(
        &mut KontingentNetz::neu(Https::neu(), &daten, jetzt),
        &zugang,
        &werte,
        &mut std::thread::sleep,
        &mut |text| println!("[Sammler] {}", zugang.bereinige(text)),
    );
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let Some(wurzel) = wurzel() else {
        eprintln!(
            "[Sammler] FEHLER: Projektordner nicht gefunden. run.cmd im Projektordner starten."
        );
        return ExitCode::FAILURE;
    };
    let journal = wurzel.join("daten").join("journal.txt");
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["kurse"] => kurse(&wurzel, &journal, None),
        ["kurse", "diagnose"] => kursdiagnose(&wurzel),
        ["kurse", "xetra"] => kurse(&wurzel, &journal, Some(Boerse::Xetra)),
        ["kurse", "nyse"] => kurse(&wurzel, &journal, Some(Boerse::Nyse)),
        ["einrichten"] | ["kurse", "einrichten"] => einrichten(&journal),
        ["termine"] => termine(&wurzel, &journal),
        ["nachrichten"] => nachrichten(&wurzel, &journal),
        _ => {
            eprintln!(
                "Aufruf: run.cmd kurse [xetra|nyse|diagnose] | run.cmd kurse einrichten | run.cmd nachrichten | run.cmd termine"
            );
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn einordnen_verbindung_ist_netz() -> Result<(), String> {
        use depot_journal::anbieter::Ergebnis as E;
        let wert = beobachtungsliste()?
            .into_iter()
            .next()
            .ok_or("Wert fehlt")?;
        let quelle = Anbieter::fuer(wert.boerse);
        let mut b = depot_sammler::Bericht {
            werte: vec![(
                wert.clone(),
                Ergebnis::Verbindung("Abruf: Verbindung weg".into()),
            )],
            handelstag: None,
            abbruch: None,
            sicherheitsabbruch: false,
            sperrquelle: None,
            pruefung: Vec::new(),
            ausgelassen: Vec::new(),
            verbindung: None,
        };
        assert_eq!(depot_sammler::einordnen(&b, quelle), Some(E::Netz));
        b.werte
            .push((wert.clone(), Ergebnis::Fehler("Daten unlesbar".into())));
        assert_eq!(depot_sammler::einordnen(&b, quelle), Some(E::Fehler));
        b.werte.push((
            wert,
            Ergebnis::Belegt {
                heute: None,
                vortag: None,
                rohdatei: PathBuf::new(),
            },
        ));
        assert_eq!(depot_sammler::einordnen(&b, quelle), Some(E::Ok));
        Ok(())
    }

    #[test]
    fn exitcode_3_nur_bei_verbindung() -> Result<(), String> {
        let wert = beobachtungsliste()?
            .into_iter()
            .next()
            .ok_or("Wert fehlt")?;
        let mut b = depot_sammler::Bericht {
            werte: vec![(wert.clone(), Ergebnis::Verbindung("Verbindung weg".into()))],
            handelstag: None,
            abbruch: None,
            sicherheitsabbruch: false,
            sperrquelle: None,
            pruefung: Vec::new(),
            ausgelassen: Vec::new(),
            verbindung: Some(netz::Ausfall::KeineVersucheMehr {
                versuche: 4,
                fehler: "Verbindung weg".into(),
            }),
        };
        assert_eq!(kurs_exitcode(&b), 3);
        for text in ["HTTP 429", "Tageskontingent reicht nicht"] {
            b.werte.push((wert.clone(), Ergebnis::Fehler(text.into())));
            assert_eq!(kurs_exitcode(&b), 1);
            b.werte.pop();
        }
        b.handelstag = Datum::neu(2026, 11, 5);
        assert_eq!(kurs_exitcode(&b), 0);
        Ok(())
    }

    #[test]
    fn boersenabruf_meldet_erwarteten_und_belegten_handelstag() -> Result<(), String> {
        let freitag = Datum::neu(2026, 11, 6).ok_or("Freitag fehlt")?;
        let montag = Datum::neu(2026, 11, 9).ok_or("Montag fehlt")?;
        let wert = beobachtungsliste()
            .map_err(|e| format!("{e:?}"))?
            .into_iter()
            .find(|w| w.boerse == Boerse::Xetra)
            .ok_or("Xetra-Wert fehlt")?;
        let bericht = depot_sammler::Bericht {
            werte: vec![(
                wert,
                Ergebnis::Belegt {
                    heute: Some((freitag, Preis::aus_fein(100_000_000))),
                    vortag: None,
                    rohdatei: PathBuf::new(),
                },
            )],
            handelstag: Some(freitag),
            abbruch: None,
            sicherheitsabbruch: false,
            sperrquelle: None,
            pruefung: vec![depot_regelwerk::DatenFehler::AbrufNichtAmHandelstag],
            ausgelassen: Vec::new(),
            verbindung: None,
        };
        let nach_schluss = Zeitpunkt::frankfurt(montag, 17, 51, 0).ok_or("Abrufzeit fehlt")?;
        assert_eq!(
            schlusskursstand(nach_schluss, &bericht),
            [
                "Xetra: Schlusskurse für 09.11.2026 nach Börsenschluss: 0 von 1 belegt. Jüngster belegter Schlusskurs vom 06.11.2026."
            ]
        );
        let nyse = beobachtungsliste()
            .map_err(|e| format!("{e:?}"))?
            .into_iter()
            .find(|w| w.boerse == Boerse::Nyse)
            .ok_or("NYSE-Wert fehlt")?;
        let mut gemischt = bericht;
        gemischt.werte.push((
            nyse,
            Ergebnis::Belegt {
                heute: Some((freitag, Preis::aus_fein(100_000_000))),
                vortag: None,
                rohdatei: PathBuf::new(),
            },
        ));
        let vor_nyse_schluss = Zeitpunkt::frankfurt(montag, 21, 35, 0).ok_or("Abrufzeit fehlt")?;
        assert_eq!(
            schlusskursstand(vor_nyse_schluss, &gemischt),
            [
                "Xetra: Schlusskurse für 09.11.2026 nach Börsenschluss: 0 von 1 belegt. Jüngster belegter Schlusskurs vom 06.11.2026.",
                "NYSE: Bei Abrufbeginn lief der Handel für 09.11.2026 noch (Börsenschluss 22:00 Uhr deutscher Zeit).",
            ]
        );
        Ok(())
    }
}
