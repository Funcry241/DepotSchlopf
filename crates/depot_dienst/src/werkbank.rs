//! Die Werkbank: was der Dienst gerade tut, was wartet und was zuletzt geschah.
//!
//! **Ein Auftrag zur Zeit.** Jeder Auftrag ist ein eigener Lauf von `depotschlopf` (wie
//! `run.cmd kurse`, `run.cmd termine`, `run.cmd papier`) im Projektordner; der Dienst selbst
//! schreibt nie ins Journal. Das Lagebild liest das Journal nur, wenn kein Auftrag laeuft, und
//! nur im selben Arbeiter, der auch Auftraege startet: Beide greifen nie zugleich zu.
//!
//! **Eindeutige Kennung:** Jeder Auftrag hat eine Kennung aus Startzeit und Nummer. Dieselbe Art
//! steht nie zweimal in der Warteschlange oder laeuft zugleich.

use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use depot_kern::Zeitpunkt;
use depot_lage::anbieter::Wer as Quelle;
use depot_lage::{Ampel, Betrieb, Lagebild, SchluesselStand, ortszeit};

use crate::zeitplan::{Erledigt, Zeitplan, faellig};

/// Was ein Auftrag tut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    /// Journal einmal anlegen (`run.cmd kurse einrichten`).
    Einrichten,
    Kurse,
    Termine,
    Papier,
    /// Neue Staende auf `main` nennen (`run.cmd update pruefen`).
    UpdatePruefen,
    /// Neuen Stand pruefen und vorwaerts einspielen (`run.cmd update`).
    Update,
}

const ALLE_ARTEN: [Art; 6] = [
    Art::Einrichten,
    Art::Kurse,
    Art::Termine,
    Art::Papier,
    Art::UpdatePruefen,
    Art::Update,
];

impl Art {
    pub fn name(self) -> &'static str {
        match self {
            Art::Einrichten => "Journal einrichten",
            Art::Kurse => "Kurse abrufen",
            Art::Termine => "Termine abrufen",
            Art::Papier => "Papiertag",
            Art::UpdatePruefen => "Nach Update suchen",
            Art::Update => "Update einspielen",
        }
    }

    /// Kurzname fuer Formulare.
    pub fn kurz(self) -> &'static str {
        match self {
            Art::Einrichten => "einrichten",
            Art::Kurse => "kurse",
            Art::Termine => "termine",
            Art::Papier => "papier",
            Art::UpdatePruefen => "update_pruefen",
            Art::Update => "update",
        }
    }

    pub fn aus_kurz(k: &str) -> Option<Art> {
        ALLE_ARTEN.into_iter().find(|a| a.kurz() == k)
    }

    /// Argumente fuer `depotschlopf` (wie hinter `run.cmd`).
    pub fn argumente(self) -> &'static [&'static str] {
        match self {
            Art::Einrichten => &["kurse", "einrichten"],
            Art::Kurse => &["kurse"],
            Art::Termine => &["termine"],
            Art::Papier => &["papier"],
            Art::UpdatePruefen => &["update", "pruefen"],
            Art::Update => &["update"],
        }
    }
}

/// Wer den Auftrag gab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anlass {
    Hand,
    Zeitplan,
}

/// Ob ein Auftrag aus einem frueheren Start des Dienstes stammt (`daten/dienst_verlauf.txt`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frueher {
    /// Aus diesem Start.
    Nein,
    /// Vor dem Neustart beendet.
    Beendet,
    /// Lief, als der Dienst endete: unterbrochen.
    Unterbrochen,
}

/// Ein Auftrag mit Protokoll.
#[derive(Clone, Debug)]
pub struct Auftrag {
    pub nr: u64,
    /// Eindeutig ueber Neustarts: Startzeit in Sekunden und Nummer.
    pub kennung: String,
    pub art: Art,
    pub anlass: Anlass,
    pub start: Zeitpunkt,
    pub ende: Option<Zeitpunkt>,
    pub erfolg: Option<bool>,
    pub protokoll: Vec<String>,
    pub frueher: Frueher,
}

/// Das zuletzt gerechnete Lagebild, schon als HTML.
#[derive(Clone, Debug)]
pub struct Lage {
    pub stand: Zeitpunkt,
    pub gesamt: Ampel,
    pub naechster_schritt: String,
    /// Gesamtstatus aus dem Lagebild; die Oberflaeche ergaenzt den Zeitplan.
    pub betrieb: Betrieb,
    /// Stand jedes Schluessels aus dem Lagebild.
    pub schluessel: Vec<(Quelle, SchluesselStand)>,
    pub html: String,
}

/// Hoechstens so viele Zeilen Protokoll je Auftrag, so viele Zeichen je Zeile.
pub const PROTOKOLL_ZEILEN: usize = 500;
pub const ZEILE_ZEICHEN: usize = 400;
/// So viele abgeschlossene Auftraege bleiben sichtbar.
pub const VERLAUF: usize = 20;
/// So viele Auftraege warten hoechstens.
pub const WARTEN_MAX: usize = 4;
/// So alt darf das Lagebild hoechstens werden, wenn nichts laeuft.
pub const LAGE_SEKUNDEN: i64 = 5;

/// Der veraenderliche Zustand.
#[derive(Debug)]
pub struct Zustand {
    pub naechste_nr: u64,
    pub laufend: Option<Auftrag>,
    pub warteschlange: VecDeque<(Art, Anlass)>,
    pub verlauf: VecDeque<Auftrag>,
    pub zeitplan: Zeitplan,
    /// Warum der Zeitplan nicht gilt (unlesbare Datei); dann ist er aus.
    pub zeitplan_fehler: Option<String>,
    pub erledigt: Erledigt,
    /// Warum nicht feststeht, was heute schon lief (unlesbares `dienst_erledigt.txt`). Solange
    /// das gilt, startet der Zeitplan nichts, auch nicht nach neuem Speichern.
    pub erledigt_fehler: Option<String>,
    pub lage: Option<Lage>,
    pub lage_veraltet: bool,
    /// Ein Update ist eingespielt: Der Dienst endet, sobald nichts mehr laeuft oder wartet.
    pub neustart: bool,
    /// Stand des letzten Updates fuer die Seite; `true` heisst Fehler.
    pub update_meldung: Option<(bool, String)>,
    /// Warum der Verlauf frueherer Starts nicht vollstaendig gezeigt wird.
    pub verlauf_hinweis: Option<String>,
    /// Der Verlauf liess sich zuletzt nicht schreiben; bis das wieder geht, startet nichts.
    pub verlauf_fehler: Option<String>,
}

/// Warum ein Zeitplan nicht gespeichert wurde.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZeitplanAbgelehnt {
    /// Einschalten erst, wenn die Einrichtung vollstaendig ist: was fehlt.
    Einrichtung(Vec<String>),
    /// Das Lagebild ist noch nicht berechnet; gleich noch einmal.
    LageFehlt,
    /// Datei nicht schreibbar; der alte Zeitplan gilt weiter.
    Datei(String),
}

fn quelle_name(w: Quelle) -> &'static str {
    match w {
        Quelle::Kursquelle => "Kursquelle Alpha Vantage",
        Quelle::Dachs => "Dachs (Claude)",
        Quelle::Maus => "Maus (GPT)",
    }
}

/// Ein Auftrag von Hand, der so kurz nach dem Ende desselben Auftrags kommt, gilt als
/// Doppelklick oder erneutes Absenden und wird nicht eingereiht.
pub const DOPPELT_SEKUNDEN: i64 = 30;

impl Zustand {
    /// Der Gesamtstatus: der des Lagebilds, ergaenzt um den Zeitplan des Dienstes. `None`,
    /// solange das Lagebild noch nicht berechnet ist.
    pub fn gesamtstatus(&self) -> Option<Betrieb> {
        let b = self.lage.as_ref()?.betrieb.clone();
        let zeitplan_fehler = self
            .erledigt_fehler
            .as_ref()
            .or(self.zeitplan_fehler.as_ref())
            .map(|f| format!("Zeitplan: {f}. Datei prüfen oder löschen, dann Dienst neu starten."));
        Some(match (b, zeitplan_fehler) {
            (Betrieb::EingriffNoetig(mut g), Some(f)) => {
                g.push(f);
                Betrieb::EingriffNoetig(g)
            }
            (_, Some(f)) => Betrieb::EingriffNoetig(vec![f]),
            (Betrieb::Bereit, None) if !self.zeitplan.an => Betrieb::Pausiert(vec![
                "Zeitplan aus: Es läuft nur, was von Hand gestartet wird. Einschalten unter \
                 Einrichtung."
                    .into(),
            ]),
            (b, None) => b,
        })
    }

    /// Darf der Zeitplan jetzt eingeschaltet werden? Nur mit berechnetem Lagebild und ohne
    /// Eingriff (Maus: Betrieb erst nach vollstaendig gepruefter Einrichtung). Ein Zeitplan
    /// mit Lesefehler zaehlt hier nicht: Neu speichern ist gerade der Weg heraus.
    ///
    /// # Errors
    /// Warum nicht.
    pub fn einschalten_erlaubt(&self) -> Result<(), ZeitplanAbgelehnt> {
        let Some(l) = self.lage.as_ref().filter(|_| !self.lage_veraltet) else {
            return Err(ZeitplanAbgelehnt::LageFehlt);
        };
        // Ausdruecklich jede noetige Einstellung (Maus zu #32): jeder Schluessel hinterlegt und
        // nicht abgelehnt, dazu kein Eingriff. „Läuft ohne KI“ reicht nicht.
        let mut fehlt: Vec<String> = l
            .schluessel
            .iter()
            .filter(|(_, st)| {
                !matches!(
                    st,
                    SchluesselStand::Hinterlegt | SchluesselStand::Geprueft(_)
                )
            })
            .map(|(w, st)| format!("{}: Schlüssel {}", quelle_name(*w), st.text()))
            .collect();
        if l.schluessel.is_empty() {
            fehlt.push("Stand der Schlüssel unbekannt".into());
        }
        if let Betrieb::EingriffNoetig(g) = &l.betrieb {
            fehlt.extend(g.iter().cloned());
        }
        if fehlt.is_empty() {
            Ok(())
        } else {
            Err(ZeitplanAbgelehnt::Einrichtung(fehlt))
        }
    }
}

/// Wie ein Auftrag gestartet wird. Im Betrieb `depotschlopf` ueber Cargo, in Tests ein
/// harmloser Befehl.
pub type Starter = Box<dyn Fn(Art, &Path) -> Command + Send + Sync>;

/// Rechnet das Lagebild als HTML; bekommt den Projektordner und die Uhr.
pub type Lagerechner = Box<dyn Fn(&Path, Zeitpunkt) -> (Lagebild, String) + Send + Sync>;

/// Der Dienst.
pub struct Dienst {
    pub wurzel: PathBuf,
    /// Zufallswert dieses Starts; jedes Formular muss ihn tragen.
    pub token: String,
    /// Kennung dieses Starts, nicht geheim. Die Seite laedt sich neu, wenn sie wechselt.
    pub start: String,
    pub zustand: Mutex<Zustand>,
    starter: Starter,
    lagerechner: Lagerechner,
}

impl std::fmt::Debug for Dienst {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dienst")
            .field("wurzel", &self.wurzel)
            .finish_non_exhaustive()
    }
}

/// Der Standardstarter: `cargo run --quiet -p depot_lauf -- <argumente>` im Projektordner.
pub fn cargo_starter() -> Starter {
    Box::new(|art: Art, wurzel: &Path| {
        let mut c = Command::new("cargo");
        c.current_dir(wurzel)
            .args(["run", "--quiet", "-p", "depot_lauf", "--"])
            .args(art.argumente());
        c
    })
}

fn zeitplan_pfad(wurzel: &Path) -> PathBuf {
    wurzel.join("daten").join("zeitplan.txt")
}

fn erledigt_pfad(wurzel: &Path) -> PathBuf {
    wurzel.join("daten").join("dienst_erledigt.txt")
}

/// Schreibt eine Datei ganz: erst Nebendatei, auf den Datentraeger, dann an ihren Platz.
///
/// # Errors
/// Jeder Schreibfehler.
pub fn schreibe_ganz(pfad: &Path, text: &str) -> Result<(), String> {
    if let Some(o) = pfad.parent() {
        fs::create_dir_all(o).map_err(|e| format!("{}: {e}", o.display()))?;
    }
    let neben = pfad.with_extension("neu");
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
    fs::rename(&neben, pfad).map_err(|e| format!("{}: {e}", pfad.display()))
}

/// Liest eine Datei. `Ok(None)` nur, wenn sie fehlt; jeder andere Fehler ist ein Fehler.
///
/// # Errors
/// Die Datei ist da, laesst sich aber nicht lesen (Rechte, kein UTF-8, Geraetefehler).
pub fn lies_falls_da(pfad: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(pfad) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{} nicht lesbar: {e}", pfad.display())),
    }
}

fn kuerze(mut z: String) -> String {
    if z.chars().count() > ZEILE_ZEICHEN {
        z = z.chars().take(ZEILE_ZEICHEN).collect();
        z.push('…');
    }
    z
}

impl Dienst {
    /// Laedt Zeitplan und Erledigt aus `daten/`. Ein unlesbarer Zeitplan schaltet den Zeitplan
    /// aus und wird gemeldet; nichts startet dann von selbst.
    pub fn neu(
        wurzel: PathBuf,
        token: String,
        starter: Starter,
        lagerechner: Lagerechner,
    ) -> Dienst {
        // Nur „Datei fehlt“ heisst Erststart mit Voreinstellung. Jeder andere Lesefehler und
        // jede fremde Form halten den Zeitplan an (K29-01): Eine ausgeschaltete Automatik darf
        // nicht wieder angehen, ein erledigter Abruf nicht noch einmal starten.
        let (zeitplan, zeitplan_fehler) = match lies_falls_da(&zeitplan_pfad(&wurzel)) {
            Ok(Some(t)) => match Zeitplan::lies(&t) {
                Ok(z) => (z, None),
                Err(e) => (Zeitplan::default(), Some(e)),
            },
            Ok(None) => (Zeitplan::default(), None),
            Err(e) => (Zeitplan::default(), Some(e)),
        };
        let (erledigt, erledigt_fehler) = match lies_falls_da(&erledigt_pfad(&wurzel)) {
            Ok(Some(t)) => match Erledigt::lies(&t) {
                Ok(e) => (e, None),
                Err(e) => (Erledigt::default(), Some(e)),
            },
            Ok(None) => (Erledigt::default(), None),
            Err(e) => (Erledigt::default(), Some(e)),
        };
        let zeitplan = Zeitplan {
            an: zeitplan.an && zeitplan_fehler.is_none() && erledigt_fehler.is_none(),
            ..zeitplan
        };
        let start: String = crate::neues_token().chars().take(16).collect();
        let (verlauf, verlauf_hinweis) = match crate::verlauf::laden(&wurzel) {
            Ok(g) => (
                g.auftraege.into_iter().collect(),
                (g.unlesbar > 0).then(|| {
                    format!(
                        "{} Zeilen in daten/dienst_verlauf.txt unlesbar; sie fehlen im Verlauf.",
                        g.unlesbar
                    )
                }),
            ),
            Err(e) => (
                VecDeque::new(),
                Some(format!("Verlauf früherer Starts: {e}")),
            ),
        };
        Dienst {
            wurzel,
            token,
            start,
            zustand: Mutex::new(Zustand {
                naechste_nr: 1,
                laufend: None,
                warteschlange: VecDeque::new(),
                verlauf,
                zeitplan,
                zeitplan_fehler,
                erledigt,
                erledigt_fehler,
                lage: None,
                lage_veraltet: true,
                neustart: false,
                update_meldung: None,
                verlauf_hinweis,
                verlauf_fehler: None,
            }),
            starter,
            lagerechner,
        }
    }

    pub fn sperre(&self) -> MutexGuard<'_, Zustand> {
        self.zustand.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Reiht einen Auftrag ein.
    ///
    /// # Errors
    /// Dieselbe Art laeuft oder wartet schon, oder die Warteschlange ist voll.
    pub fn einreihen(&self, art: Art, anlass: Anlass) -> Result<(), &'static str> {
        let mut z = self.sperre();
        einreihen(&mut z, art, anlass)
    }

    /// Reiht einen Auftrag von Hand ein. Wie `einreihen`, dazu: Derselbe Auftrag, der vor
    /// weniger als `DOPPELT_SEKUNDEN` endete, wird nicht noch einmal eingereiht (Doppelklick,
    /// erneutes Absenden).
    ///
    /// # Errors
    /// Wie `einreihen`, oder gerade erst erledigt.
    pub fn von_hand(&self, art: Art, jetzt: Zeitpunkt) -> Result<(), &'static str> {
        let mut z = self.sperre();
        let gerade = z.verlauf.iter().any(|a| {
            a.art == art
                && a.frueher == Frueher::Nein
                && a.ende
                    .and_then(|e| jetzt.sekunden_seit(e))
                    .is_some_and(|s| s < DOPPELT_SEKUNDEN)
        });
        if gerade {
            return Err("gerade erst erledigt");
        }
        einreihen(&mut z, art, Anlass::Hand)
    }

    /// Speichert und aktiviert einen neuen Zeitplan. Einschalten (aus wird an) geht nur, wenn
    /// `Zustand::einschalten_erlaubt`; Ausschalten und Aendern eines laufenden Zeitplans immer.
    ///
    /// # Errors
    /// Einschalten nicht erlaubt, oder die Datei laesst sich nicht schreiben; dann gilt der alte
    /// weiter.
    pub fn setze_zeitplan(&self, plan: Zeitplan) -> Result<(), ZeitplanAbgelehnt> {
        let mut z = self.sperre();
        if plan.an && !z.zeitplan.an {
            z.einschalten_erlaubt()?;
        }
        schreibe_ganz(&zeitplan_pfad(&self.wurzel), &plan.als_text())
            .map_err(ZeitplanAbgelehnt::Datei)?;
        z.zeitplan = plan;
        z.zeitplan_fehler = None;
        Ok(())
    }

    /// Ein Schritt des Arbeiters: Zeitplan pruefen, dann den naechsten Auftrag starten, sonst das
    /// Lagebild auffrischen. Gibt einen gestarteten Prozess zurueck; `verfolge` liest ihn aus.
    /// Nur der eine Arbeiter ruft `schritt`: So startet kein Auftrag, waehrend das Lagebild liest.
    pub fn schritt(&self, jetzt: Zeitpunkt) -> Option<(u64, Child)> {
        let mut z = self.sperre();
        self.zeitplan_pruefen(&mut z, jetzt);
        if z.laufend.is_none()
            && let Some((art, anlass)) = z.warteschlange.pop_front()
        {
            let nr = z.naechste_nr;
            z.naechste_nr = nr.saturating_add(1);
            let mut auftrag = Auftrag {
                nr,
                kennung: format!("{}-{nr}", jetzt.sekunden()),
                art,
                anlass,
                start: jetzt,
                ende: None,
                erfolg: None,
                protokoll: Vec::new(),
                frueher: Frueher::Nein,
            };
            // Erst dauerhaft vermerken, dann starten (Maus zu #32): Ein Lauf, der nicht im
            // Verlauf steht, startet nicht, und die Oberflaeche zeigt den Grund.
            if let Err(e) =
                crate::verlauf::vermerke(&self.wurzel, &crate::verlauf::start_zeile(&auftrag))
            {
                auftrag
                    .protokoll
                    .push(format!("Nicht gestartet: Verlauf nicht schreibbar ({e})."));
                auftrag.ende = Some(jetzt);
                auftrag.erfolg = Some(false);
                z.verlauf_fehler = Some(format!(
                    "Verlauf nicht schreibbar ({e}). Aufträge starten erst wieder, wenn \
                     daten/dienst_verlauf.txt geschrieben werden kann."
                ));
                ablegen(&mut z, auftrag);
                return None;
            }
            z.verlauf_fehler = None;
            let mut befehl = (self.starter)(art, &self.wurzel);
            befehl
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            match befehl.spawn() {
                Ok(kind) => {
                    z.laufend = Some(auftrag);
                    return Some((nr, kind));
                }
                Err(e) => {
                    auftrag.protokoll.push(format!("Start fehlgeschlagen: {e}"));
                    auftrag.ende = Some(jetzt);
                    auftrag.erfolg = Some(false);
                    ende_vermerken(&self.wurzel, &mut z, &auftrag);
                    ablegen(&mut z, auftrag);
                }
            }
            return None;
        }
        let noetig = z.laufend.is_none() && {
            let frisch = z.lage.as_ref().is_some_and(|l| {
                jetzt
                    .sekunden_seit(l.stand)
                    .is_some_and(|s| s < LAGE_SEKUNDEN)
            });
            !frisch || z.lage_veraltet
        };
        drop(z);
        if noetig {
            // Ohne Sperre rechnen: Die Oberflaeche bleibt bedienbar. Ein Auftrag startet nur hier
            // im Arbeiter, also nicht, solange das Lagebild das Journal liest.
            let (bild, html) = (self.lagerechner)(&self.wurzel, jetzt);
            let mut z = self.sperre();
            z.lage = Some(Lage {
                stand: jetzt,
                gesamt: bild.gesamt,
                naechster_schritt: bild.naechster_schritt,
                betrieb: bild.betrieb,
                schluessel: bild.schluessel,
                html,
            });
            z.lage_veraltet = false;
        }
        None
    }

    fn zeitplan_pruefen(&self, z: &mut Zustand, jetzt: Zeitpunkt) {
        if z.erledigt_fehler.is_some() {
            return;
        }
        let Some((tag, h, m, _, _)) = ortszeit(jetzt) else {
            return;
        };
        let Some(art) = faellig(&z.zeitplan, &z.erledigt, tag, (h, m)) else {
            return;
        };
        // Erst festhalten, dann starten: Ein Neustart startet nichts doppelt.
        let mut neu = z.erledigt;
        neu.setze(art, tag);
        match schreibe_ganz(&erledigt_pfad(&self.wurzel), &neu.als_text()) {
            Ok(()) => {
                z.erledigt = neu;
                let _ = einreihen(z, art, Anlass::Zeitplan);
            }
            Err(e) => {
                z.zeitplan.an = false;
                z.zeitplan_fehler = Some(format!("Zeitplan angehalten: {e}"));
            }
        }
    }

    /// Liest Ausgabe und Ende eines gestarteten Auftrags ins Protokoll. Blockiert bis zum Ende.
    pub fn verfolge(self: &Arc<Self>, nr: u64, mut kind: Child, uhr: fn() -> Option<Zeitpunkt>) {
        let fehler_leser = kind.stderr.take().map(|e| {
            let d = Arc::clone(self);
            thread::spawn(move || {
                for zeile in BufReader::new(e).lines() {
                    match zeile {
                        Ok(t) => d.protokolliere(nr, t),
                        Err(_) => break,
                    }
                }
            })
        });
        if let Some(o) = kind.stdout.take() {
            for zeile in BufReader::new(o).lines() {
                match zeile {
                    Ok(t) => self.protokolliere(nr, t),
                    Err(_) => break,
                }
            }
        }
        if let Some(h) = fehler_leser {
            let _ = h.join();
        }
        let erfolg = kind.wait().is_ok_and(|s| s.success());
        let mut z = self.sperre();
        let Some(mut a) = z.laufend.take() else {
            return;
        };
        a.ende = uhr();
        a.erfolg = Some(erfolg);
        let papier_danach = erfolg
            && a.art == Art::Kurse
            && a.anlass == Anlass::Zeitplan
            && z.zeitplan.papier_danach;
        let eingespielt =
            erfolg && a.art == Art::Update && crate::rueckfall::offen_pfad(&self.wurzel).exists();
        ende_vermerken(&self.wurzel, &mut z, &a);
        ablegen(&mut z, a);
        z.lage_veraltet = true;
        if eingespielt {
            z.neustart = true;
            z.update_meldung = Some((false, "Update eingespielt. Der Dienst startet neu.".into()));
        }
        if papier_danach {
            let _ = einreihen(&mut z, Art::Papier, Anlass::Zeitplan);
        }
    }

    /// Darf der Dienst fuer das Update jetzt enden? Nur wenn eins eingespielt ist und nichts
    /// laeuft oder wartet: Kein eingereihter Lauf geht verloren.
    pub fn neustart_bereit(&self) -> bool {
        let z = self.sperre();
        z.neustart && z.laufend.is_none() && z.warteschlange.is_empty()
    }

    fn protokolliere(&self, nr: u64, zeile: String) {
        let mut z = self.sperre();
        if let Some(a) = z.laufend.as_mut().filter(|a| a.nr == nr)
            && a.protokoll.len() < PROTOKOLL_ZEILEN
        {
            a.protokoll.push(kuerze(zeile));
        }
    }
}

fn einreihen(z: &mut Zustand, art: Art, anlass: Anlass) -> Result<(), &'static str> {
    // Das Update laeuft nur allein und nur einmal je Start: Danach startet der Dienst neu.
    if art == Art::Update && (z.laufend.is_some() || !z.warteschlange.is_empty() || z.neustart) {
        return Err("Update nur, wenn nichts laeuft oder wartet");
    }
    if z.laufend.as_ref().is_some_and(|a| a.art == art)
        || z.warteschlange.iter().any(|(a, _)| *a == art)
    {
        return Err("laeuft schon oder wartet schon");
    }
    if z.warteschlange.len() >= WARTEN_MAX {
        return Err("zu viele wartende Auftraege");
    }
    z.warteschlange.push_back((art, anlass));
    Ok(())
}

/// Vermerkt das Ende; ein Schreibfehler steht sofort unter Taetigkeit.
fn ende_vermerken(wurzel: &Path, z: &mut Zustand, a: &Auftrag) {
    if let Err(e) = crate::verlauf::vermerke(wurzel, &crate::verlauf::ende_zeile(a)) {
        z.verlauf_fehler = Some(format!(
            "Ende von {} nicht im Verlauf vermerkt ({e}); nach einem Neustart erschiene der Lauf \
             als unterbrochen.",
            a.kennung
        ));
    }
}

fn ablegen(z: &mut Zustand, a: Auftrag) {
    z.verlauf.push_front(a);
    z.verlauf.truncate(VERLAUF);
}
