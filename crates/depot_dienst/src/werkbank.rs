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
use std::io::{BufReader, PipeReader, Write, pipe};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use depot_kern::{Boerse, Datum, Zeitpunkt};
use depot_lage::anbieter::Wer as Quelle;
use depot_lage::{Betrieb, Lagebild, SchluesselStand, TagStand, ortszeit};

use crate::hinweis::Gedaechtnis;
use crate::zeitplan::{Erledigt, Zeitplan, faellig_fuer};

/// Was ein Auftrag tut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    /// Journal einmal anlegen (`run.cmd kurse einrichten`).
    Einrichten,
    /// Alle Werte der Liste (von Hand).
    Kurse,
    /// Rein lesender Prüfabruf trotz Kurssperre, ohne Kurse oder Rohablage.
    Kursdiagnose,
    /// Nur die Werte einer Boerse, kurz nach ihrem Schluss (Zeitplan).
    KurseVon(Boerse),
    Termine,
    Nachrichten,
    /// Interner Auftrag für die optionale Nachrichtensortierung.
    Gemini,
    Vorbereiten,
    Papier,
    /// Dauerhafter Verlaufseintrag ohne gestarteten Prozess.
    PapierAusgelassen,
    /// Neue Staende auf `main` nennen (`run.cmd update pruefen`).
    UpdatePruefen,
    /// Neuen Stand pruefen und vorwaerts einspielen (`run.cmd update`).
    Update,
}

const ALLE_ARTEN: [Art; 11] = [
    Art::Einrichten,
    Art::Kurse,
    Art::Kursdiagnose,
    Art::KurseVon(Boerse::Xetra),
    Art::KurseVon(Boerse::Nyse),
    Art::Termine,
    Art::Nachrichten,
    Art::Vorbereiten,
    Art::Papier,
    Art::UpdatePruefen,
    Art::Update,
];

impl Art {
    pub fn name(self) -> &'static str {
        match self {
            Art::Einrichten => "Journal einrichten",
            Art::Kurse => "Kurse abrufen",
            Art::Kursdiagnose => "Kurssperre sicher untersuchen",
            Art::KurseVon(Boerse::Xetra) => "Kurse Xetra abrufen",
            Art::KurseVon(Boerse::Nyse) => "Kurse US-Börsen abrufen",
            Art::Termine => "Termine abrufen",
            Art::Nachrichten => "Schlagzeilen abrufen",
            Art::Gemini => "Gemini-Zuarbeit",
            Art::Vorbereiten => "Nächsten Handelstag vorbereiten",
            Art::Papier => "Papiertag",
            Art::PapierAusgelassen => "Papiertag ausgelassen: vor Börsenschluss nicht gestartet",
            Art::UpdatePruefen => "Nach Update suchen",
            Art::Update => "Update einspielen",
        }
    }

    /// Kurzname fuer Formulare.
    pub fn kurz(self) -> &'static str {
        match self {
            Art::Einrichten => "einrichten",
            Art::Kurse => "kurse",
            Art::Kursdiagnose => "kursdiagnose",
            Art::KurseVon(Boerse::Xetra) => "kurse_xetra",
            Art::KurseVon(Boerse::Nyse) => "kurse_nyse",
            Art::Termine => "termine",
            Art::Nachrichten => "nachrichten",
            Art::Gemini => "gemini-vorlauf",
            Art::Vorbereiten => "vorbereiten",
            Art::Papier => "papier",
            Art::PapierAusgelassen => "papier_ausgelassen",
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
            Art::Kursdiagnose => &["kurse", "diagnose"],
            Art::KurseVon(Boerse::Xetra) => &["kurse", "xetra"],
            Art::KurseVon(Boerse::Nyse) => &["kurse", "nyse"],
            Art::Termine => &["termine"],
            Art::Nachrichten => &["nachrichten"],
            Art::Gemini => &["gemini-vorlauf"],
            Art::Vorbereiten => &["vorbereiten"],
            Art::Papier => &["papier"],
            Art::PapierAusgelassen => &["papier_ausgelassen"],
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
    /// Die juengste Hinweiszeile des Laufs (`depot_kern::hinweis`): was los ist und ob der
    /// Mensch etwas tun muss. Die Oberflaeche zeigt sie gross.
    pub hinweis: Option<String>,
    pub frueher: Frueher,
}

/// Das zuletzt gerechnete Lagebild.
#[derive(Clone, Debug)]
pub struct Lage {
    pub stand: Zeitpunkt,
    pub bild: Lagebild,
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

/// Ein Auftrag in der Warteschlange. Ein Update trägt den bereits bestätigten Commit.
#[derive(Debug)]
pub struct WartenderAuftrag {
    pub art: Art,
    pub anlass: Anlass,
    update_commit: Option<String>,
    gemini_vorrang: bool,
    gemini_vorgeschaltet: bool,
}

/// Der veraenderliche Zustand.
#[derive(Debug)]
pub struct Zustand {
    pub naechste_nr: u64,
    pub laufend: Option<Auftrag>,
    pub warteschlange: VecDeque<WartenderAuftrag>,
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
    pub update_ergebnisse: Vec<crate::aktualisierung::Ergebnis>,
    /// Woher die Programme kommen und ob der Stand fuer einen Start ohne Cargo gesichert ist;
    /// `true` heisst Fehler.
    pub programm_meldung: Option<(bool, String)>,
    /// Warum der Verlauf frueherer Starts nicht vollstaendig gezeigt wird.
    pub verlauf_hinweis: Option<String>,
    /// Der Verlauf liess sich zuletzt nicht schreiben; bis das wieder geht, startet nichts.
    pub verlauf_fehler: Option<String>,
    /// Ein Abschluss oder ausgefallener Papiertag ließ sich nicht vermerken.
    pub verlauf_vermerkfehler: Option<String>,
    /// Die Boersen der Liste (`depot_kern::liste::boersen`), je eine mit eigenem Kursabruf.
    pub boersen: Vec<Boerse>,
    pub(crate) protokoll_datei: Option<crate::protokoll::Datei>,
    protokoll_fehler: Option<String>,
}

/// Warum ein Zeitplan nicht gespeichert wurde.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZeitplanAbgelehnt {
    /// Der Schalter steht anders als beim Öffnen des Formulars.
    SchalterGeaendert,
    /// Eine neue Version ist noch nicht bestätigt.
    UpdateOffen,
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
        Quelle::Tiingo => "Kursquelle Tiingo",
        Quelle::Dachs => "Dachs (Claude)",
        Quelle::Maus => "Maus (GPT)",
        Quelle::Gemini => "Gemini",
    }
}

/// Ein Auftrag von Hand, der so kurz nach dem Ende desselben Auftrags kommt, gilt als
/// Doppelklick oder erneutes Absenden und wird nicht eingereiht.
pub const DOPPELT_SEKUNDEN: i64 = 30;

impl Zustand {
    pub(crate) fn automatik_fehler(&self) -> Option<&str> {
        self.verlauf_fehler
            .as_deref()
            .or(self.erledigt_fehler.as_deref())
            .or(self.zeitplan_fehler.as_deref())
    }

    pub(crate) fn automatik_status(&self) -> &'static str {
        if self.automatik_fehler().is_some() {
            "angehalten"
        } else if self.zeitplan.an {
            "an"
        } else {
            "aus"
        }
    }

    /// Stand des Auftrags für die Bedienelemente.
    pub fn auftrag_stand(&self, art: Art) -> Option<&'static str> {
        if self.laufend.as_ref().is_some_and(|a| a.art == art) {
            Some("läuft …")
        } else if self.warteschlange.iter().any(|a| a.art == art) {
            Some("wartet …")
        } else {
            None
        }
    }

    pub(crate) fn vorbereitung_pruefen(&self, jetzt: Zeitpunkt) -> Result<(), &'static str> {
        let tag = depot_lage::vorbereitungstag(jetzt).ok_or("vorbereitung_uhrzeit")?;
        if crate::zeitplan::handelstag(tag) {
            return Err("vorbereitung_handelstag");
        }
        if self
            .lage
            .as_ref()
            .and_then(|l| l.bild.vorbereitung.as_ref())
            .is_some_and(|p| depot_lage::vorbereitungstag(p.zeit) == Some(tag))
        {
            return Err("vorbereitung_begonnen");
        }
        Ok(())
    }

    pub(crate) fn letzter_update_lauf(&self) -> Option<&Auftrag> {
        self.verlauf
            .iter()
            .find(|a| matches!(a.art, Art::Update | Art::UpdatePruefen))
    }

    pub(crate) fn update_wartet_auf_ruhe(&self) -> bool {
        self.laufend.is_some() || !self.warteschlange.is_empty()
    }

    /// Der letzte erfolgreiche Suchlauf kann einen konkreten Commit anbieten.
    pub(crate) fn update_angebot(&self) -> Option<(&str, &[String])> {
        let a = self.letzter_update_lauf()?;
        if a.art != Art::UpdatePruefen
            || a.erfolg != Some(true)
            || a.frueher != Frueher::Nein
            || self.auftrag_stand(Art::UpdatePruefen).is_some()
            || self.neustart
        {
            return None;
        }
        let (kandidat, titel) = a.protokoll.split_last()?;
        let sha = kandidat.strip_prefix("[Update] Kandidat: ")?;
        crate::rueckfall::ist_commit(sha).then_some((sha, titel))
    }

    /// Der Gesamtstatus: der des Lagebilds, ergaenzt um den Zeitplan des Dienstes. `None`,
    /// solange das Lagebild noch nicht berechnet ist.
    pub fn gesamtstatus(&self) -> Option<Betrieb> {
        let b = self.lage.as_ref()?.bild.betrieb.clone();
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
        if let Some(fehler) = &self.erledigt_fehler {
            return Err(ZeitplanAbgelehnt::Einrichtung(vec![format!(
                "Zeitplan angehalten: {fehler}. Datei prüfen, dann Dienst neu starten."
            )]));
        }
        let Some(l) = self.lage.as_ref().filter(|_| !self.lage_veraltet) else {
            return Err(ZeitplanAbgelehnt::LageFehlt);
        };
        // Ausdruecklich jede noetige Einstellung (Maus zu #32): jeder Schluessel hinterlegt und
        // nicht abgelehnt, dazu kein Eingriff. „Läuft ohne KI“ reicht nicht.
        let mut fehlt: Vec<String> = l
            .bild
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
        if l.bild.schluessel.is_empty() {
            fehlt.push("Stand der Schlüssel unbekannt".into());
        }
        if let Betrieb::EingriffNoetig(g) = &l.bild.betrieb {
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

/// Rechnet das Lagebild; bekommt den Projektordner und die Uhr.
pub type Lagerechner = Box<dyn Fn(&Path, Zeitpunkt) -> Lagebild + Send + Sync>;

/// Zeigt einen Hinweis (Titel, Text). Im Betrieb `Umwelt::hinweis`, in Tests eine Attrappe.
pub type Melder = Box<dyn Fn(&str, &str) + Send + Sync>;

/// Der Dienst.
pub struct Dienst {
    pub wurzel: PathBuf,
    /// Zufallswert dieses Starts; jedes Formular muss ihn tragen.
    pub token: String,
    /// Kennung dieses Starts, nicht geheim. Die Seite laedt sich neu, wenn sie wechselt.
    pub start: String,
    /// Commit des laufenden Programms, einmal beim Dienststart ermittelt.
    pub version: Option<String>,
    pub zustand: Mutex<Zustand>,
    pub(crate) gespraechscache: crate::zeitstrahl_ki::Cache,
    pub(crate) papierdialog: Mutex<()>,
    protokoll_schutz: Mutex<crate::protokoll::Schutz>,
    starter: Starter,
    lagerechner: Lagerechner,
    hinweise: Hinweise,
}

/// Windows-Hinweise des Dienstes (#122).
struct Hinweise {
    /// Welche Gründe für „Eingriff nötig“ schon gemeldet sind (`daten/hinweise.txt`).
    gedaechtnis: Mutex<crate::hinweis::Gedaechtnis>,
    /// Ohne Melder kommt kein Hinweis (Tests ohne Attrappe); das Gedächtnis wird trotzdem gepflegt.
    melder: Option<Melder>,
}

impl Hinweise {
    fn laden(wurzel: &Path) -> Hinweise {
        Hinweise {
            gedaechtnis: Mutex::new(Gedaechtnis::laden(&hinweise_pfad(wurzel))),
            melder: None,
        }
    }
}

impl std::fmt::Debug for Dienst {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dienst")
            .field("wurzel", &self.wurzel)
            .finish_non_exhaustive()
    }
}

/// Prozess und gemeinsamer Ausgabekanal für sein Laufprotokoll.
#[derive(Debug)]
pub struct Laufprozess {
    kind: Child,
    ausgabe: PipeReader,
}

impl Laufprozess {
    /// Wartet auf einen Prozess, dessen Ausgabe nicht verfolgt werden muss.
    pub fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        std::io::copy(&mut self.ausgabe, &mut std::io::sink())?;
        self.kind.wait()
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

fn lade_zeitplan(wurzel: &Path) -> (Zeitplan, Option<String>) {
    match lies_falls_da(&zeitplan_pfad(wurzel)) {
        Ok(Some(t)) => match Zeitplan::lies(&t) {
            Ok(mut z) => {
                let fehler = if z.migriere_morgen() {
                    match schreibe_ganz(&zeitplan_pfad(wurzel), &z.als_text()) {
                        Ok(()) => {
                            eprintln!(
                                "[Dienst] Zeitplan auf Morgenlauf umgestellt. Kurse ab 07:30 Uhr."
                            );
                            None
                        }
                        Err(e) => Some(format!("Morgenzeitplan nicht gespeichert: {e}")),
                    }
                } else {
                    None
                };
                (z, fehler)
            }
            Err(e) => (Zeitplan::default(), Some(e)),
        },
        Ok(None) => (Zeitplan::default(), None),
        Err(e) => (Zeitplan::default(), Some(e)),
    }
}

fn erledigt_pfad(wurzel: &Path) -> PathBuf {
    wurzel.join("daten").join("dienst_erledigt.txt")
}

fn hinweise_pfad(wurzel: &Path) -> PathBuf {
    wurzel.join("daten").join("hinweise.txt")
}

/// Die Gründe, die einen Windows-Hinweis wert sind: jeder Grund für „Eingriff nötig“ im
/// Gesamtstatus, dazu der jüngste Handelstag, wenn er ausgefallen ist (einmal je Datum).
fn hinweis_gruende(z: &Zustand, jetzt: Zeitpunkt) -> Vec<String> {
    let mut g = match z.gesamtstatus() {
        Some(Betrieb::EingriffNoetig(g)) => g,
        _ => Vec::new(),
    };
    if let Some(tag) = ortszeit(jetzt).map(|(tag, ..)| tag)
        && z.verlauf.iter().any(|a| {
            a.art == Art::PapierAusgelassen && ortszeit(a.start).is_some_and(|(d, ..)| d == tag)
        })
    {
        g.push(format!(
            "Papiertag am {} ausgelassen: vor Börsenschluss nicht gestartet.",
            depot_lage::datum_text(tag)
        ));
        return g;
    }
    let juengster = z.lage.as_ref().and_then(|l| {
        l.bild
            .tage
            .iter()
            .find(|t| !matches!(t.stand, TagStand::Offen | TagStand::Geschlossen(_)))
    });
    if let Some(t) = juengster
        && matches!(t.stand, TagStand::Ausgefallen(_))
    {
        g.push(format!(
            "Papiertag am {} ausgefallen.",
            depot_lage::datum_text(t.datum)
        ));
    }
    g
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

fn lade_protokoll(wurzel: &Path, a: &mut Auftrag, schutz: &crate::protokoll::Schutz) {
    if let Ok(text) = crate::protokoll::lesen(wurzel, &a.kennung) {
        a.protokoll = text
            .lines()
            .rev()
            .take(PROTOKOLL_ZEILEN)
            .map(|z| kuerze(schutz.bereinige(z)))
            .collect();
        a.protokoll.reverse();
        if crate::protokoll::letzter_fehler(&a.protokoll).is_none()
            && let Some(fehler) = text
                .lines()
                .rev()
                .skip(PROTOKOLL_ZEILEN)
                .find(|zeile| crate::protokoll::ist_fehler(zeile))
        {
            a.protokoll.remove(0);
            a.protokoll.insert(0, kuerze(schutz.bereinige(fehler)));
        }
        if a.frueher == Frueher::Unterbrochen {
            if a.protokoll.len() >= PROTOKOLL_ZEILEN {
                a.protokoll.remove(0);
            }
            a.protokoll.push("Der Dienst endete während dieses Laufs. Das gespeicherte Protokoll ist unvollständig.".into());
        }
    }
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
        let (zeitplan, zeitplan_fehler) = lade_zeitplan(&wurzel);
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
        let protokoll_schutz = crate::protokoll::Schutz::laden();
        let (mut verlauf, verlauf_hinweis): (VecDeque<Auftrag>, _) = match crate::verlauf::laden(
            &wurzel,
        ) {
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
        for a in &mut verlauf {
            lade_protokoll(&wurzel, a, &protokoll_schutz);
        }
        let (update_ergebnisse, mut update_meldung) = match crate::aktualisierung::laden(&wurzel) {
            Ok(e) => {
                let m = e.last().map(crate::aktualisierung::Ergebnis::meldung);
                (e, m)
            }
            Err(e) => (Vec::new(), Some((true, e))),
        };
        if let Some(a) = verlauf.iter().find(|a| a.art == Art::Update)
            && !update_ergebnisse.iter().any(|e| e.lauf == a.kennung)
        {
            update_meldung = Some((true, "Für das letzte Update liegt keine bestätigte Aktivierung vor. Details stehen unter Tätigkeit im Protokoll.".into()));
        }
        Dienst {
            hinweise: Hinweise::laden(&wurzel),
            wurzel,
            token,
            start,
            version: None,
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
                update_meldung,
                update_ergebnisse,
                programm_meldung: None,
                verlauf_hinweis,
                verlauf_fehler: None,
                verlauf_vermerkfehler: None,
                boersen: depot_kern::liste::boersen(),
                protokoll_datei: None,
                protokoll_fehler: None,
            }),
            gespraechscache: crate::zeitstrahl_ki::Cache::default(),
            papierdialog: Mutex::new(()),
            protokoll_schutz: Mutex::new(protokoll_schutz),
            starter,
            lagerechner,
        }
    }

    /// Mit Melder für Windows-Hinweise (`Umwelt::hinweis`).
    #[must_use]
    pub fn mit_melder(mut self, melder: Melder) -> Dienst {
        self.hinweise.melder = Some(melder);
        self
    }

    /// Gleicht die Gründe für „Eingriff nötig“ mit dem Gedächtnis ab und zeigt, wenn es so
    /// weit ist, einen Hinweis. Der Melder läuft außerhalb jeder Sperre.
    fn hinweise_pruefen(&self, jetzt: Zeitpunkt) {
        let (mut gruende, an) = {
            let z = self.sperre();
            (hinweis_gruende(&z, jetzt), z.zeitplan.hinweis.aktiv())
        };
        let daten = self.wurzel.join("daten");
        match depot_lage::kurssperre::lies(&daten) {
            Ok(Some(s)) => gruende.push(format!("Kurssperre aktiv ({}).", s.art.text())),
            Err(_) => gruende.push("Kurssperre aktiv: Datei unlesbar.".into()),
            Ok(None) => {
                if let Some(aufgehoben) = depot_lage::kurssperre::letzter_fehlalarm(&daten)
                    && ortszeit(aufgehoben).map(|(tag, ..)| tag)
                        == ortszeit(jetzt).map(|(tag, ..)| tag)
                {
                    gruende.push(format!(
                        "Kurssperre aufgehoben: Fehlalarm ({}).",
                        depot_lage::zeit_text(aufgehoben)
                    ));
                }
            }
        }
        let gruende: Vec<String> = gruende
            .iter()
            .map(|g| self.protokoll_bereinigen(g))
            .collect();
        let text = self
            .hinweise
            .gedaechtnis
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pruefe(&gruende, an, jetzt, &hinweise_pfad(&self.wurzel));
        if let (Some(text), Some(melder)) = (text, self.hinweise.melder.as_ref()) {
            melder(crate::hinweis::TITEL, &text);
        }
    }

    pub fn sperre(&self) -> MutexGuard<'_, Zustand> {
        self.zustand.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn update_offen(&self) -> bool {
        crate::rueckfall::offen_pfad(&self.wurzel)
            .try_exists()
            .unwrap_or(true)
    }

    pub(crate) fn protokoll_schluessel_merken(&self, text: &str) {
        self.protokoll_schutz
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .merken(text);
    }

    pub(crate) fn protokoll_bereinigen(&self, text: &str) -> String {
        self.protokoll_schutz
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .bereinige(text)
    }

    pub(crate) fn protokoll_datei_bereinigen(&self, text: &str) -> String {
        // Nur die Prüfsummen kopieren. Lange Dateien blockieren weder Live noch den Arbeiter.
        let schutz = self
            .protokoll_schutz
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        schutz.bereinige_zeilen(text)
    }

    fn protokoll_beginnen(&self, z: &mut Zustand, a: &Auftrag) {
        z.verlauf_fehler = None;
        z.verlauf_vermerkfehler = None;
        z.protokoll_fehler = None;
        self.protokoll_schutz
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .auffrischen();
        match crate::protokoll::Datei::neu(
            &self.wurzel,
            &a.kennung,
            self.version.as_deref(),
            a.start,
            a.art.name(),
        ) {
            Ok(log) => z.protokoll_datei = Some(log),
            Err(e) => {
                z.protokoll_datei = None;
                z.verlauf_hinweis = Some(format!("Diagnoseprotokoll nicht gespeichert. {e}"));
            }
        }
    }

    fn protokoll_beenden(&self, z: &mut Zustand, a: &mut Auftrag) {
        let schon_protokolliert = z.protokoll_fehler.is_some();
        if let Some(fehler) = z.protokoll_fehler.take()
            && !a.protokoll.contains(&fehler)
        {
            if a.protokoll.len() >= PROTOKOLL_ZEILEN {
                a.protokoll.remove(0);
            }
            a.protokoll.insert(0, fehler);
        }
        if let Some(mut log) = z.protokoll_datei.take() {
            let zeit = a.ende.unwrap_or(a.start);
            // Fehler beim Start gelangen direkt in den Auftrag, Prozessausgaben schon beim Lesen in die Datei.
            if !schon_protokolliert
                && let Some(f) = crate::protokoll::letzter_fehler(&a.protokoll)
                && let Err(e) = log.zeile(zeit, &self.protokoll_bereinigen(f))
            {
                z.verlauf_hinweis = Some(e);
            }
            let ende = if a.erfolg == Some(true) {
                "Lauf erfolgreich beendet."
            } else if matches!(a.art, Art::Kurse | Art::KurseVon(_)) {
                "Lauf ohne verwendbare Kurse beendet."
            } else {
                "Lauf abgebrochen."
            };
            if let Err(e) = log.abschliessen(zeit, ende) {
                z.verlauf_hinweis = Some(e);
            }
        }
    }

    /// Nach dem Windows-Dialog die Freigabe bei ruhender Werkbank belegen.
    pub(crate) fn papier_freigeben(&self, gesehen: &str, bestaetigt_am: Zeitpunkt) -> &'static str {
        let mut z = self.sperre();
        if z.laufend.is_some() || !z.warteschlange.is_empty() || self.update_offen() {
            return "papierfreigabe_belegt";
        }
        match depot_lage::papier_freigeben(&self.wurzel, gesehen, bestaetigt_am) {
            Ok(()) => {
                z.lage_veraltet = true;
                "papierfreigabe_ok"
            }
            Err(depot_lage::PapierFreigabeFehler::Geaendert) => "papierfreigabe_geaendert",
            Err(depot_lage::PapierFreigabeFehler::Belegt) => "papierfreigabe_belegt",
            Err(depot_lage::PapierFreigabeFehler::Abgelehnt(grund)) => {
                eprintln!("[Papierfreigabe] {grund}");
                "papierfreigabe_fehler"
            }
        }
    }

    /// Reiht einen Auftrag ein.
    ///
    /// # Errors
    /// Dieselbe Art laeuft oder wartet schon, oder die Warteschlange ist voll.
    pub fn einreihen(&self, art: Art, anlass: Anlass) -> Result<(), &'static str> {
        let mut z = self.sperre();
        einreihen(&mut z, art, anlass, None)
    }

    /// Reiht einen Auftrag von Hand ein. Wie `einreihen`, dazu: Derselbe Auftrag, der vor
    /// weniger als `DOPPELT_SEKUNDEN` endete, wird nicht noch einmal eingereiht (Doppelklick,
    /// erneutes Absenden).
    ///
    /// # Errors
    /// Wie `einreihen`, oder gerade erst erledigt.
    pub fn von_hand(&self, art: Art, jetzt: Zeitpunkt) -> Result<(), &'static str> {
        if art == Art::Vorbereiten && self.update_offen() {
            return Err("vorbereitung_update");
        }
        let mut z = self.sperre();
        if art == Art::Vorbereiten {
            z.vorbereitung_pruefen(jetzt)?;
        }
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
        einreihen(&mut z, art, Anlass::Hand, None)
    }

    pub(crate) fn update_von_hand(&self, sha: &str) -> Result<(), &'static str> {
        let mut z = self.sperre();
        if z.update_angebot().is_none_or(|(angebot, _)| angebot != sha) {
            return Err("update_suche");
        }
        einreihen(&mut z, Art::Update, Anlass::Hand, Some(sha.to_owned()))
            .map_err(|_| "update_belegt")
    }

    /// Speichert und aktiviert einen neuen Zeitplan. Einschalten (aus wird an) geht nur, wenn
    /// `Zustand::einschalten_erlaubt`; Ausschalten und Aendern eines laufenden Zeitplans immer.
    ///
    /// # Errors
    /// Einschalten nicht erlaubt, oder die Datei laesst sich nicht schreiben; dann gilt der alte
    /// weiter.
    pub fn setze_zeitplan(&self, plan: Zeitplan) -> Result<(), ZeitplanAbgelehnt> {
        self.aendere_zeitplan(|_| Ok(plan))
    }

    pub(crate) fn aendere_zeitplan(
        &self,
        aenderung: impl FnOnce(Zeitplan) -> Result<Zeitplan, ZeitplanAbgelehnt>,
    ) -> Result<(), ZeitplanAbgelehnt> {
        let mut z = self.sperre();
        let plan = aenderung(z.zeitplan)?;
        if plan.vorbereitung.aktiv() && !z.zeitplan.vorbereitung.aktiv() && self.update_offen() {
            return Err(ZeitplanAbgelehnt::UpdateOffen);
        }
        if plan.an && !z.zeitplan.an {
            z.einschalten_erlaubt()?;
        }
        schreibe_ganz(&zeitplan_pfad(&self.wurzel), &plan.als_text())
            .map_err(ZeitplanAbgelehnt::Datei)?;
        if !plan.an {
            z.warteschlange.retain(|a| a.anlass != Anlass::Zeitplan);
        }
        z.zeitplan = plan;
        z.zeitplan_fehler = None;
        Ok(())
    }

    /// Ein Schritt des Arbeiters: Zeitplan pruefen, dann den naechsten Auftrag starten, sonst das
    /// Lagebild auffrischen. Gibt einen gestarteten Prozess zurueck; `verfolge` liest ihn aus.
    /// Nur der eine Arbeiter ruft `schritt`: So startet kein Auftrag, waehrend das Lagebild liest.
    pub fn schritt(&self, jetzt: Zeitpunkt) -> Option<(u64, Laufprozess)> {
        let mut z = self.sperre();
        (!self.update_offen()).then_some(())?;
        self.vermerke_ausgelassen(&mut z, jetzt);
        Self::zeitplan_pruefen(&mut z, jetzt);
        if z.laufend.is_none()
            && let Some(wartend) = z.warteschlange.pop_front()
        {
            let WartenderAuftrag {
                art,
                anlass,
                update_commit,
                gemini_vorrang,
                ..
            } = self.gemini_vorschalten(&mut z, wartend, jetzt);
            if anlass == Anlass::Zeitplan && !zeitplan_auftrag_erlaubt(&z, art, jetzt) {
                return None;
            }
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
                hinweis: None,
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
            self.protokoll_beginnen(&mut z, &auftrag);
            if anlass == Anlass::Zeitplan
                && matches!(art, Art::Termine | Art::KurseVon(_) | Art::Papier)
                && let Err(e) = self.zeitplan_start_vermerken(&mut z, art, jetzt)
            {
                auftrag.protokoll.push(format!("Nicht gestartet: {e}"));
                auftrag.ende = Some(jetzt);
                auftrag.erfolg = Some(false);
                self.protokoll_beenden(&mut z, &mut auftrag);
                ende_vermerken(&self.wurzel, &mut z, &auftrag);
                ablegen(&mut z, auftrag);
                return None;
            }
            match self.starte_auftrag(
                art,
                update_commit.as_deref(),
                &auftrag.kennung,
                gemini_vorrang,
            ) {
                Ok(kind) => {
                    z.laufend = Some(auftrag);
                    return Some((nr, kind));
                }
                Err(e) => {
                    auftrag.protokoll.push(format!("Start fehlgeschlagen: {e}"));
                    auftrag.ende = Some(jetzt);
                    auftrag.erfolg = Some(false);
                    self.protokoll_beenden(&mut z, &mut auftrag);
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
            let bild = (self.lagerechner)(&self.wurzel, jetzt);
            {
                let mut z = self.sperre();
                z.lage = Some(Lage { stand: jetzt, bild });
                z.lage_veraltet = false;
            }
            self.hinweise_pruefen(jetzt);
        }
        None
    }

    fn gemini_vorschalten(
        &self,
        z: &mut Zustand,
        mut wartend: WartenderAuftrag,
        jetzt: Zeitpunkt,
    ) -> WartenderAuftrag {
        if !matches!(wartend.art, Art::Papier | Art::Vorbereiten)
            || wartend.gemini_vorgeschaltet
            || (wartend.anlass == Anlass::Zeitplan
                && !zeitplan_auftrag_erlaubt(z, wartend.art, jetzt))
            || !depot_lage::gespraeche::gemini_aktiv(&self.wurzel.join("daten"))
        {
            return wartend;
        }
        wartend.gemini_vorgeschaltet = true;
        let anlass = wartend.anlass;
        z.warteschlange.push_front(wartend);
        WartenderAuftrag {
            art: Art::Gemini,
            anlass,
            update_commit: None,
            gemini_vorrang: true,
            gemini_vorgeschaltet: false,
        }
    }

    fn starte_auftrag(
        &self,
        art: Art,
        update_commit: Option<&str>,
        kennung: &str,
        gemini_vorrang: bool,
    ) -> std::io::Result<Laufprozess> {
        if art == Art::Update && update_commit.is_none_or(|sha| !crate::rueckfall::ist_commit(sha))
        {
            return Err(std::io::Error::other(
                "Kein bestätigter Commit. Bitte suche erneut nach einem Update.",
            ));
        }
        let (ausgabe, schreiber) = pipe()?;
        let kind = (self.starter)(art, &self.wurzel)
            .args(update_commit)
            .args(gemini_vorrang.then_some("papiertag"))
            .env(depot_lage::gespraeche::LAUF_ENV, kennung)
            .stdin(Stdio::null())
            .stdout(schreiber.try_clone()?)
            .stderr(schreiber)
            .spawn()?;
        Ok(Laufprozess { kind, ausgabe })
    }

    fn vermerke_ausgelassen(&self, z: &mut Zustand, jetzt: Zeitpunkt) {
        let Some((tag, h, m, ..)) = ortszeit(jetzt) else {
            return;
        };
        if !z.zeitplan.an
            || !z.zeitplan.papier_danach
            || z.erledigt_fehler.is_some()
            || z.erledigt.papier == Some(tag)
            || !crate::zeitplan::morgen_abgelaufen(&z.zeitplan, tag, (h, m), &z.boersen)
            || z.laufend.iter().chain(z.verlauf.iter()).any(|a| {
                matches!(a.art, Art::Papier | Art::PapierAusgelassen)
                    && ortszeit(a.start).is_some_and(|(d, ..)| d == tag)
            })
        {
            return;
        }
        let nr = z.naechste_nr;
        let a = Auftrag {
            nr,
            kennung: format!("{}-{nr}", jetzt.sekunden()),
            art: Art::PapierAusgelassen,
            anlass: Anlass::Zeitplan,
            start: jetzt,
            ende: Some(jetzt),
            erfolg: Some(false),
            protokoll: Vec::new(),
            hinweis: None,
            frueher: Frueher::Nein,
        };
        if let Err(e) = crate::verlauf::vermerke(&self.wurzel, &crate::verlauf::ende_zeile(&a)) {
            z.verlauf_vermerkfehler = Some(format!("Ausgelassenen Papiertag nicht vermerkt: {e}"));
            return;
        }
        z.naechste_nr = nr.saturating_add(1);
        ablegen(z, a);
        z.lage_veraltet = true;
        // Der Verlauf schützt auch dann vor Wiederholungen, wenn diese zweite Datei ausfällt.
        let _ = self.zeitplan_start_vermerken(z, Art::Papier, jetzt);
    }

    fn zeitplan_pruefen(z: &mut Zustand, jetzt: Zeitpunkt) {
        if z.neustart || z.erledigt_fehler.is_some() {
            return;
        }
        let Some((tag, h, m, _, _)) = ortszeit(jetzt) else {
            return;
        };
        let Some(art) = faellig_fuer(&z.zeitplan, &z.erledigt, tag, (h, m), &z.boersen) else {
            if z.laufend.is_none() && z.warteschlange.is_empty() && vorbereitung_faellig(z, jetzt) {
                let _ = einreihen(z, Art::Vorbereiten, Anlass::Zeitplan, None);
                return;
            }
            // Auch am Wochenende Weltgeschehen lesen, aber nur bei bewusst eingeschaltetem
            // Zeitplan. Keine Kurse, keine KI, kein Papiertag durch diesen Auftrag.
            if z.zeitplan.an
                && z.zeitplan.nachrichten_an
                && z.laufend.is_none()
                && z.warteschlange.is_empty()
                && z.verlauf_fehler.is_none()
                && z.verlauf_vermerkfehler.is_none()
                && z.zeitplan_fehler.is_none()
                && (8..23).contains(&h)
                && z.verlauf
                    .iter()
                    .filter(|a| a.art == Art::Nachrichten)
                    .all(|a| jetzt.sekunden_seit(a.start).is_some_and(|s| s >= 5400))
            {
                let _ = einreihen(z, Art::Nachrichten, Anlass::Zeitplan, None);
            }
            return;
        };
        let _ = einreihen(z, art, Anlass::Zeitplan, None);
    }

    /// Liest Ausgabe und Ende eines gestarteten Auftrags ins Protokoll. Blockiert bis zum Ende.
    pub fn verfolge(self: &Arc<Self>, nr: u64, lauf: Laufprozess, uhr: fn() -> Option<Zeitpunkt>) {
        let Laufprozess { mut kind, ausgabe } = lauf;
        crate::protokoll::zeilen(BufReader::new(ausgabe), |t| {
            self.protokolliere(nr, &t, uhr());
        });
        let erfolg = kind.wait().is_ok_and(|s| s.success());
        let mut z = self.sperre();
        let Some(mut a) = z.laufend.take() else {
            return;
        };
        a.ende = uhr();
        a.erfolg = Some(erfolg);
        let ende = a.ende;
        let nachrichten_lauf = (erfolg && a.art == Art::Nachrichten).then_some(a.anlass);
        if erfolg {
            self.kurse_ok_vermerken(&mut z, &a);
        }
        let eingespielt =
            erfolg && a.art == Art::Update && crate::rueckfall::offen_pfad(&self.wurzel).exists();
        if eingespielt && let Ok(Some(o)) = crate::rueckfall::offen(&self.wurzel) {
            match crate::aktualisierung::vermerke(
                &self.wurzel,
                &o,
                crate::aktualisierung::Phase::Probe,
                "Einspielen beendet; Bestätigung des neuen Dienstes steht aus.",
                Some(&a.kennung),
            ) {
                Ok(e) => {
                    z.update_ergebnisse.retain(|alt| alt.neu != e.neu);
                    z.update_ergebnisse.push(e);
                }
                Err(e) => z.verlauf_hinweis = Some(e),
            }
        }
        if a.art == Art::Update && !eingespielt {
            z.update_meldung = Some((!erfolg, if erfolg { "Keine neue Version eingespielt. Der bisherige Stand läuft weiter." } else { "Update abgebrochen. Der bisherige Stand läuft weiter; Details stehen im Protokoll." }.into()));
        }
        self.protokoll_beenden(&mut z, &mut a);
        ende_vermerken(&self.wurzel, &mut z, &a);
        ablegen(&mut z, a);
        z.lage_veraltet = true;
        if eingespielt {
            z.neustart = true;
            z.update_meldung = Some((false, "Update eingespielt. Der Dienst startet neu.".into()));
        }
        // Folgt jetzt der Papiertag, steht er gleich in der Warteschlange.
        if let Some(jetzt) = ende {
            Self::zeitplan_pruefen(&mut z, jetzt);
        }
        if let Some(anlass) = nachrichten_lauf
            && !self.update_offen()
            && !z
                .warteschlange
                .iter()
                .any(|a| matches!(a.art, Art::Papier | Art::Vorbereiten))
            && depot_lage::gespraeche::gemini_aktiv(&self.wurzel.join("daten"))
        {
            let _ = einreihen(&mut z, Art::Gemini, anlass, None);
        }
    }

    /// Vermerkt den Beginn dauerhaft vor dem Prozessstart. Wartende Aufträge bleiben fällig.
    fn zeitplan_start_vermerken(
        &self,
        z: &mut Zustand,
        art: Art,
        jetzt: Zeitpunkt,
    ) -> Result<(), String> {
        let (tag, ..) = ortszeit(jetzt).ok_or("Uhr liefert keinen Tag")?;
        let mut neu = z.erledigt;
        neu.setze(art, tag);
        match schreibe_ganz(&erledigt_pfad(&self.wurzel), &neu.als_text()) {
            Ok(()) => {
                z.erledigt = neu;
                Ok(())
            }
            Err(e) => {
                z.zeitplan.an = false;
                z.zeitplan_fehler = Some(format!("Zeitplan angehalten: {e}"));
                Err(format!("daten/dienst_erledigt.txt nicht schreibbar ({e})"))
            }
        }
    }

    /// Nach einem erfolgreichen Kursabruf des Zeitplans: Sein Tag zaehlt dauerhaft als Tag mit
    /// Kursen (`Erledigt::kurse_ok`). So folgt der Papiertag auch nach einem Neustart. Der Tag
    /// ist der, an dem der Zeitplan den Abruf gestartet hat.
    fn kurse_ok_vermerken(&self, z: &mut Zustand, a: &Auftrag) {
        let Art::KurseVon(boerse) = a.art else {
            return;
        };
        if a.anlass != Anlass::Zeitplan || z.erledigt_fehler.is_some() {
            return;
        }
        let Some(tag) = z.erledigt.kurse_von(boerse) else {
            return;
        };
        let mut neu = z.erledigt;
        neu.kurse_ok = Some(tag);
        match schreibe_ganz(&erledigt_pfad(&self.wurzel), &neu.als_text()) {
            Ok(()) => z.erledigt = neu,
            Err(e) => {
                z.zeitplan.an = false;
                z.zeitplan_fehler = Some(format!("Zeitplan angehalten: {e}"));
            }
        }
    }

    /// Darf der Dienst fuer das Update jetzt enden? Nur wenn eins eingespielt ist und nichts
    /// laeuft oder wartet: Kein eingereihter Lauf geht verloren.
    pub fn neustart_bereit(&self) -> bool {
        let z = self.sperre();
        z.neustart && z.laufend.is_none() && z.warteschlange.is_empty()
    }

    fn protokolliere(&self, nr: u64, zeile: &str, zeit: Option<Zeitpunkt>) {
        let maskiert = depot_kern::geheimnis::maskiere(zeile);
        let zeile = self.protokoll_bereinigen(&maskiert);
        let leck = zeile == "[Ausgabe mit Zugangsdaten entfernt.]";
        let mut z = self.sperre();
        let Some(a) = z.laufend.as_mut().filter(|a| a.nr == nr) else {
            return;
        };
        let zeit = zeit.unwrap_or(a.start);
        if let Some(h) = depot_kern::hinweis::lies(&zeile) {
            a.hinweis = Some(kuerze(h.to_string()));
        }
        if a.protokoll.len() >= PROTOKOLL_ZEILEN {
            a.protokoll.remove(0);
        }
        a.protokoll.push(kuerze(zeile.clone()));
        if leck {
            z.verlauf_hinweis = Some("Ein Lauf hat Zugangsdaten ausgegeben, Zeile entfernt. Bitte das Diagnoseprotokoll zur Prüfung öffnen.".into());
        }
        if crate::protokoll::letzter_fehler(std::slice::from_ref(&zeile)).is_some() {
            z.protokoll_fehler = Some(kuerze(zeile.clone()));
        }
        if let Some(log) = z.protokoll_datei.as_mut()
            && let Err(e) = log.zeile(zeit, &zeile)
        {
            z.protokoll_datei = None;
            z.verlauf_hinweis = Some(e);
        }
    }
}

fn einreihen(
    z: &mut Zustand,
    art: Art,
    anlass: Anlass,
    update_commit: Option<String>,
) -> Result<(), &'static str> {
    if art == Art::PapierAusgelassen {
        return Err("Verlaufseintrag ist kein Auftrag");
    }
    if art == Art::Update && update_commit.is_none() {
        return Err("Kein bestätigter Commit. Bitte suche erneut nach einem Update.");
    }
    // Kein Auftrag darf zwischen dem Vorrücken und der Datenprüfung des neuen Dienstes laufen.
    if z.neustart
        || z.laufend.as_ref().is_some_and(|a| a.art == Art::Update)
        || z.warteschlange.iter().any(|a| a.art == Art::Update)
    {
        return Err("Update läuft, wartet oder startet den Dienst neu");
    }
    if art == Art::Update && z.update_wartet_auf_ruhe() {
        return Err("Update nur, wenn nichts läuft oder wartet");
    }
    if z.laufend.as_ref().is_some_and(|a| a.art == art)
        || z.warteschlange.iter().any(|a| a.art == art)
    {
        return Err("läuft schon oder wartet schon");
    }
    if z.warteschlange.len() >= WARTEN_MAX {
        return Err("zu viele wartende Aufträge");
    }
    z.warteschlange.push_back(WartenderAuftrag {
        art,
        anlass,
        update_commit,
        gemini_vorrang: false,
        gemini_vorgeschaltet: false,
    });
    Ok(())
}

/// Vermerkt das Ende; ein Schreibfehler steht sofort unter Taetigkeit.
fn ende_vermerken(wurzel: &Path, z: &mut Zustand, a: &Auftrag) {
    if let Err(e) = crate::verlauf::vermerke(wurzel, &crate::verlauf::ende_zeile(a)) {
        z.verlauf_vermerkfehler = Some(format!(
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

/// Wo der Tag nach einem Neustart des Dienstes steht (Maus 05.11.2026): was passiert ist, ob
/// DepotSchlopf selbst weitermacht und ob der Mensch etwas tun muss. `None`, wenn heute kein
/// Neustart den Ablauf beruehrt hat. `true` heisst: Der Mensch muss hinsehen.
pub fn nach_neustart(z: &Zustand, tag: Datum) -> Option<(bool, String)> {
    let heute = |a: &Auftrag| ortszeit(a.start).is_some_and(|o| o.0 == tag);
    let kurse = |a: Art| matches!(a, Art::Kurse | Art::KurseVon(_));
    let frueher = |art: &dyn Fn(Art) -> bool, wie: Frueher| {
        z.verlauf
            .iter()
            .any(|a| a.frueher == wie && art(a.art) && heute(a))
    };
    let dieser_start = |art: &dyn Fn(Art) -> bool| {
        z.laufend
            .iter()
            .chain(z.verlauf.iter())
            .any(|a| a.frueher == Frueher::Nein && art(a.art) && heute(a))
    };
    let papier = |a: Art| a == Art::Papier;
    // Ein Papiertag des Zeitplans aus diesem Start, laufend, wartend oder beendet.
    let papier_jetzt = z
        .laufend
        .iter()
        .chain(z.verlauf.iter())
        .find(|a| a.frueher == Frueher::Nein && a.art == Art::Papier && heute(a))
        .filter(|a| a.anlass == Anlass::Zeitplan)
        .map(|a| if a.ende.is_none() { None } else { a.erfolg })
        .or_else(|| {
            (z.erledigt.kurse_ok == Some(tag)
                && z.warteschlange
                    .iter()
                    .any(|a| a.art == Art::Papier && a.anlass == Anlass::Zeitplan))
            .then_some(None)
        });
    let kurse_vorher = frueher(&kurse, Frueher::Beendet) || frueher(&kurse, Frueher::Unterbrochen);
    if let Some(ausgang) = papier_jetzt {
        if dieser_start(&kurse) || !kurse_vorher {
            return None;
        }
        return Some(match ausgang {
            None => (
                false,
                "Nach dem Neustart holt DepotSchlopf den Papiertag mit den Kursen von heute \
                 nach. Du musst nichts tun."
                    .into(),
            ),
            Some(true) => (
                false,
                "Nach dem Neustart hat DepotSchlopf den Papiertag mit den Kursen von heute \
                 nachgeholt. Du musst nichts tun."
                    .into(),
            ),
            Some(false) => (
                true,
                "Nach dem Neustart wollte DepotSchlopf den Papiertag nachholen, er hielt an. \
                 Den Grund zeigt sein Protokoll im Verlauf."
                    .into(),
            ),
        });
    }
    if frueher(&papier, Frueher::Unterbrochen) && !dieser_start(&papier) {
        return Some((
            true,
            "Der Papiertag lief schon, als DepotSchlopf endete (etwa bei einem Neustart von \
             Windows). Der heutige Schutz bleibt: Er startet heute nicht noch einmal von selbst, \
             und ein angefangener Tag hält jeden weiteren Papiertag an, bis du ihn geprüft hast. \
             Was zu tun ist, steht im Lagebild unter „Papiertag“."
                .into(),
        ));
    }
    if frueher(&kurse, Frueher::Unterbrochen)
        && !dieser_start(&kurse)
        && z.erledigt.kurse_ok != Some(tag)
    {
        return Some((
            true,
            "Der Kursabruf lief, als DepotSchlopf endete. Ohne Kurse von heute folgt heute kein \
             Papiertag von selbst. Der aktuelle Zeitplan steht unter Tätigkeit."
                .into(),
        ));
    }
    None
}

fn rss_zeitplan_an(z: &Zustand, jetzt: Zeitpunkt) -> bool {
    z.zeitplan.an
        && z.zeitplan.nachrichten_an
        && ortszeit(jetzt).is_some_and(|(_, h, ..)| (8..23).contains(&h))
}

fn vorbereitung_faellig(z: &Zustand, jetzt: Zeitpunkt) -> bool {
    let Some((tag, ..)) = ortszeit(jetzt) else {
        return false;
    };
    rss_zeitplan_an(z, jetzt)
        && z.zeitplan.vorbereitung.aktiv()
        && !crate::zeitplan::handelstag(tag)
        && z.erledigt_fehler.is_none()
        && z.zeitplan_fehler.is_none()
        && z.verlauf_fehler.is_none()
        && z.verlauf_vermerkfehler.is_none()
        && !z
            .verlauf
            .iter()
            .any(|a| a.art == Art::Vorbereiten && ortszeit(a.start).is_some_and(|(t, ..)| t == tag))
        && z.verlauf.iter().any(|a| {
            a.art == Art::Nachrichten
                && a.erfolg == Some(true)
                && a.ende.is_some_and(|ende| {
                    ortszeit(ende).is_some_and(|(t, ..)| t == tag)
                        && jetzt.sekunden_seit(ende).is_some_and(|s| s < 5_400)
                })
        })
}

fn zeitplan_auftrag_erlaubt(z: &Zustand, art: Art, jetzt: Zeitpunkt) -> bool {
    if !z.zeitplan.an || z.erledigt_fehler.is_some() || z.zeitplan_fehler.is_some() {
        return false;
    }
    match art {
        Art::Termine | Art::KurseVon(_) | Art::Papier => {
            ortszeit(jetzt).is_some_and(|(tag, h, m, _, _)| {
                faellig_fuer(&z.zeitplan, &z.erledigt, tag, (h, m), &z.boersen) == Some(art)
            })
        }
        Art::Nachrichten => rss_zeitplan_an(z, jetzt),
        Art::Gemini => true,
        Art::Vorbereiten => vorbereitung_faellig(z, jetzt),
        _ => false,
    }
}
