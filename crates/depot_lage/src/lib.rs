//! DepotSchlopf – das Lagebild: was gerade gilt, was fuer den naechsten Papiertag fehlt, wie das
//! Papierdepot steht, wie viel KI-Budget uebrig ist und was Dachs und Maus begruendet haben.
//!
//! Reine Auswertung: kein Netz, keine Uhr, kein Schreiben. Der Aufrufer oeffnet das Journal und
//! nennt Uhrzeit und den Stand der Schluessel; das Lagebild rechnet daraus mit denselben
//! Funktionen, mit denen Papiertag und Kostenschutz entscheiden (`depot_sim::kurse_von_heute`,
//! `depot_sim::tageslage`, `depot_ki::pruefe`). Anzeige und Schutz sehen so denselben Stand
//! (D16). Grenzen und Budgets werden nur angezeigt; sie aendern sich nur ueber die Freigabe.
//! Lesen ist nur `mit_journal`: Es oeffnet das Journal wie jeder Lauf und entscheidet, ob ein
//! Datenbestand neu, belegt oder unvollstaendig ist.

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

pub mod betrieb;
/// Das Anbieterprotokoll, fuer die Oberflaeche.
pub use depot_journal::{anbieter, kurssperre};
mod budget;
mod depot;
pub mod html;
pub mod tage;
mod zugang;

pub use betrieb::{AnbieterHinweis, Betrieb, SchluesselStand};
pub use budget::{BudgetStatus, KiBudget, ki_budget, mit_verbrauch};
pub use depot::{AuftragZeile, DepotLage, Position, TopfLage};
pub use tage::{KiBeitrag, TagErgebnis, TagStand};
pub use zugang::{PAPIERKONTO, anbieterprotokoll, ki_halt, mit_journal, sperrdateien};

use depot_journal::Stand;
use depot_journal::anbieter::{Eintrag, Wer};
use depot_kern::liste::{self, Wert, als_universum, beobachtungsliste};
use depot_kern::{BeraterId, Datum, Grenzen, Zeitpunkt};
use depot_ki::KiGrenzen;
use depot_regelwerk::waehle_tageswerte;
use depot_sim::{LaufFehler, Tageslage, kurse_von_heute, tageskurse, tageslage};
use std::fmt::Write as _;

/// Wie das Journal zu erreichen war.
#[derive(Clone, Copy, Debug)]
pub enum Zugang<'a> {
    /// Keine Spur eines Datenbestands: weder Journal noch Vermerk noch Papierkonto.
    NichtEingerichtet,
    /// Ein Lauf hat das Journal gerade offen.
    Belegt,
    /// Das Journal laesst sich nicht oeffnen; der Text sagt warum. Jeder Lauf haelt dann an.
    Fehler(&'a str),
    /// Geoeffnet und geprueft.
    Offen(&'a Stand),
}

/// Ob ein Schluessel in der Anmeldeinformationsverwaltung liegt. Der Schluessel selbst
/// erreicht das Lagebild nie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hinterlegt {
    Ja,
    Nein,
    /// Hier nicht pruefbar, etwa ausserhalb von Windows.
    Unbekannt,
}

/// Was der Aufrufer von aussen beitraegt.
#[derive(Clone, Debug)]
pub struct Umgebung<'a> {
    /// Uhr des Rechners.
    pub jetzt: Zeitpunkt,
    pub journal: Zugang<'a>,
    /// Schluessel der Kursquelle (Alpha Vantage, Xetra).
    pub kursquelle: Hinterlegt,
    /// Schluessel der Kursquelle fuer die NYSE (Tiingo). Das Lagebild zeigt ihn nur, wenn die
    /// Liste einen Wert dieser Quelle hat.
    pub kursquelle_us: Hinterlegt,
    /// Schluessel je Berater; ohne Schluessel wird der Berater nicht gefragt.
    pub berater: [(BeraterId, Hinterlegt); 2],
    /// Liegengebliebene Sperrdateien unter `daten/` (abgebrochener frueherer Lauf).
    pub sperren: Vec<String>,
    /// Das Anbieterprotokoll (`daten/anbieter.txt`) und wie viele Zeilen unlesbar waren.
    pub anbieter: Vec<Eintrag>,
    pub anbieter_unlesbar: usize,
    /// Inhalt von `daten/ki.halt`, wenn die Datei liegt und lesbar ist.
    pub ki_halt: Option<String>,
}

/// Ampel eines Punktes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ampel {
    /// Erledigt oder bereit.
    Gut,
    /// Fehlt noch, ist aber normal und mit einem Schritt erledigt (etwa Kurse abrufen).
    Offen,
    /// Halt: Ein Mensch muss pruefen; kein Lauf geht weiter.
    Halt,
}

/// Was ein Punkt braucht, damit er gut wird. Die Oberflaeche macht daraus einen Knopf oder einen
/// Verweis auf die Einrichtung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abhilfe {
    JournalEinrichten,
    SchluesselKursquelle,
    /// Schluessel der Kursquelle fuer die NYSE (Tiingo).
    SchluesselKursquelleUs,
    SchluesselBerater(BeraterId),
    /// Die Sperre des Kostenschutzes (`daten/ki.halt`) pruefen und gefuehrt aufheben.
    KostenschutzPruefen,
    /// Die Kurssperre (`daten/kurse.halt`) pruefen und gefuehrt aufheben.
    KurssperrePruefen,
    KurseAbrufen,
    TermineAbrufen,
    PapiertagStarten,
    /// Nur ein Mensch kann helfen: pruefen, was im Text steht.
    Pruefen,
    /// Abwarten, etwa bis ein laufender Schritt fertig ist oder das Budget wieder frei ist.
    Warten,
}

impl Abhilfe {
    /// Muss ein Mensch etwas einrichten oder pruefen, bevor der Betrieb vollstaendig ist?
    /// Nein nur fuer das, was Zeitplan oder Knopf ohnehin tun (Abrufe, Papiertag, Warten),
    /// und fuer die Schluessel der KIs: Ohne sie laeuft der Betrieb ohne KI weiter, das zeigt
    /// der Gesamtstatus eigens.
    pub fn braucht_menschen(self) -> bool {
        match self {
            Abhilfe::JournalEinrichten
            | Abhilfe::SchluesselKursquelle
            | Abhilfe::SchluesselKursquelleUs
            | Abhilfe::KostenschutzPruefen
            | Abhilfe::KurssperrePruefen
            | Abhilfe::Pruefen => true,
            Abhilfe::SchluesselBerater(_)
            | Abhilfe::KurseAbrufen
            | Abhilfe::TermineAbrufen
            | Abhilfe::PapiertagStarten
            | Abhilfe::Warten => false,
        }
    }
}

/// Ein Punkt der Einrichtung oder Bereitschaft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Punkt {
    pub name: &'static str,
    pub ampel: Ampel,
    pub text: String,
    pub abhilfe: Option<Abhilfe>,
}

/// Eine Begruendung aus dem Journal: Vorschlag, Einwand, Veto oder Entscheidung des Rahmens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Beitrag {
    pub datum: Datum,
    pub wer: String,
    pub text: String,
}

/// Eine festgeschriebene Grenze zur Anzeige. Sie gilt; aendern laesst sie sich nur ueber die
/// Grenzfreigabe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grenzwert {
    pub name: &'static str,
    pub wert: String,
}

/// Das ganze Lagebild.
#[derive(Clone, Debug)]
pub struct Lagebild {
    pub jetzt: Zeitpunkt,
    /// Gesamturteil fuer den naechsten Papiertag: die schlechteste Ampel der Punkte.
    pub gesamt: Ampel,
    /// Ein Satz: was als Naechstes geschieht oder fehlt.
    pub naechster_schritt: String,
    pub punkte: Vec<Punkt>,
    pub budget: Vec<KiBudget>,
    pub depot: Option<DepotLage>,
    /// Neueste zuerst.
    pub gespraech: Vec<Beitrag>,
    pub grenzen: Vec<Grenzwert>,
    /// Gesamtstatus: bereit, ohne KI, pausiert oder Eingriff noetig.
    pub betrieb: Betrieb,
    /// Stand jedes Schluessels.
    pub schluessel: Vec<(Wer, SchluesselStand)>,
    /// Die letzten Handelstage seit dem ersten Papiertag, neueste zuerst.
    pub tage: Vec<TagErgebnis>,
}

/// So viele Beitraege zeigt das Lagebild hoechstens.
pub const GESPRAECH_MAX: usize = 60;

/// `TT.MM.JJJJ`.
pub fn datum_text(d: Datum) -> String {
    format!("{:02}.{:02}.{}", d.tag(), d.monat(), d.jahr())
}

/// Ortszeit in Frankfurt: Tag, Stunde, Minute, Sekunde und Zone, wie der Kalender des Kerns sie
/// umrechnet. `None` fuer die doppelt vorkommende Stunde im Oktober.
pub fn ortszeit(z: Zeitpunkt) -> Option<(Datum, u8, u8, u8, &'static str)> {
    for (stunden, zone) in [(1_i64, "MEZ"), (2, "MESZ")] {
        let ort = stunden
            .checked_mul(3_600)
            .and_then(|s| z.sekunden().checked_add(s))
            .and_then(Zeitpunkt::aus_sekunden);
        if let Some(ort) = ort
            && let (Some(d), Some((h, m, s))) = (ort.datum(), ort.uhrzeit())
            && Zeitpunkt::frankfurt(d, h, m, s) == Some(z)
        {
            return Some((d, h, m, s, zone));
        }
    }
    None
}

/// Ortszeit in Frankfurt als Text; eine doppelt vorkommende Stunde steht in UTC.
pub fn zeit_text(z: Zeitpunkt) -> String {
    if let Some((d, h, m, _, zone)) = ortszeit(z) {
        return format!("{} {h:02}:{m:02} {zone}", datum_text(d));
    }
    match (z.datum(), z.uhrzeit()) {
        (Some(d), Some((h, m, _))) => format!("{} {h:02}:{m:02} UTC", datum_text(d)),
        _ => z.to_string(),
    }
}

fn halt_text(e: LaufFehler) -> String {
    match e {
        LaufFehler::Halt(t) => t,
    }
}

fn punkt(name: &'static str, ampel: Ampel, text: String, abhilfe: Option<Abhilfe>) -> Punkt {
    Punkt {
        name,
        ampel,
        text,
        abhilfe,
    }
}

fn journal_punkt(z: Zugang<'_>) -> Punkt {
    let name = "Journal";
    match z {
        Zugang::NichtEingerichtet => punkt(
            name,
            Ampel::Offen,
            "Noch nicht eingerichtet.".into(),
            Some(Abhilfe::JournalEinrichten),
        ),
        Zugang::Belegt => punkt(
            name,
            Ampel::Offen,
            "Ein Lauf hat das Journal gerade offen; das Lagebild folgt danach.".into(),
            Some(Abhilfe::Warten),
        ),
        Zugang::Fehler(t) => punkt(
            name,
            Ampel::Halt,
            format!("Nicht lesbar: {t}"),
            Some(Abhilfe::Pruefen),
        ),
        Zugang::Offen(s) => punkt(
            name,
            Ampel::Gut,
            format!(
                "Eingerichtet und geprüft, {} Kursbelege, {} Termine.",
                s.kurse().len(),
                s.aktuelle_termine().len()
            ),
            None,
        ),
    }
}

fn schluessel_punkt(name: &'static str, st: SchluesselStand, abhilfe: Abhilfe) -> Punkt {
    match st {
        SchluesselStand::Geprueft(_) | SchluesselStand::Hinterlegt => {
            punkt(name, Ampel::Gut, format!("Schlüssel {}.", st.text()), None)
        }
        SchluesselStand::Fehlt => {
            punkt(name, Ampel::Offen, "Schlüssel fehlt.".into(), Some(abhilfe))
        }
        SchluesselStand::Abgelehnt(_) => punkt(
            name,
            Ampel::Halt,
            format!(
                "Schlüssel {}: Die Quelle nimmt ihn nicht an. Bitte neu hinterlegen.",
                st.text()
            ),
            Some(abhilfe),
        ),
        SchluesselStand::Unbekannt => punkt(
            name,
            Ampel::Offen,
            "Hier nicht prüfbar (nur unter Windows).".into(),
            None,
        ),
    }
}

fn berater_name(b: BeraterId) -> &'static str {
    match b {
        BeraterId::Dachs => "Dachs (Claude)",
        BeraterId::Maus => "Maus (GPT)",
    }
}

fn berater_punkt(b: BeraterId, st: SchluesselStand, hinweis: Option<AnbieterHinweis>) -> Punkt {
    let name = berater_name(b);
    let abhilfe = Some(Abhilfe::SchluesselBerater(b));
    let mut p = match st {
        SchluesselStand::Geprueft(_) | SchluesselStand::Hinterlegt => {
            punkt(name, Ampel::Gut, format!("Schlüssel {}.", st.text()), None)
        }
        SchluesselStand::Fehlt => punkt(
            name,
            Ampel::Offen,
            "Nicht angebunden: wird nicht gefragt, der Papiertag läuft ohne ihn.".into(),
            abhilfe,
        ),
        SchluesselStand::Abgelehnt(_) => punkt(
            name,
            Ampel::Offen,
            format!(
                "Schlüssel {}: Der Anbieter nimmt ihn nicht an; der Papiertag läuft ohne ihn. \
                 Bitte neu hinterlegen.",
                st.text()
            ),
            abhilfe,
        ),
        SchluesselStand::Unbekannt => punkt(
            name,
            Ampel::Offen,
            "Hier nicht prüfbar (nur unter Windows).".into(),
            None,
        ),
    };
    if let Some(h) = hinweis {
        p.ampel = p.ampel.max(Ampel::Offen);
        p.text = format!("{} {}", p.text, h.text());
    }
    p
}

/// Kurse, Termine und Tageslage aus dem Journal, genau wie der Papiertag sie prueft.
fn tag_punkte(s: &Stand, heute: Datum, werte: &[Wert], punkte: &mut Vec<Punkt>) {
    if geschlossen(s, heute, punkte) {
        return;
    }
    let universum = als_universum(werte);
    let kurse_heute = match kurse_von_heute(s, &universum, heute) {
        Ok(k) => {
            let wahl = waehle_tageswerte(
                &Grenzen::standard(),
                &universum,
                &s.gehalten(),
                k.handelstag,
                &k.heute,
                &k.vortag,
            );
            // Faellt der Zyklus aus, zaehlen auch die Gruende der ausgelassenen Kandidaten:
            // Ohne sie hiesse es nur „kein Wert mit gültigem Kurs“.
            let befunde: Vec<_> = if wahl.fehler.is_empty() {
                Vec::new()
            } else {
                wahl.fehler
                    .iter()
                    .chain(&wahl.ausgelassen)
                    .filter(|b| **b != depot_regelwerk::DatenFehler::KeinWertHandelbar)
                    .cloned()
                    .collect()
            };
            if wahl.fehler.is_empty() {
                let mut text = format!(
                    "Vom {} belegt und geprüft, Abruf {}.",
                    datum_text(k.handelstag),
                    zeit_text(k.heute.stand())
                );
                if !wahl.ausgelassen.is_empty() {
                    let roh: Vec<String> =
                        wahl.ausgelassen.iter().map(|b| format!("{b:?}")).collect();
                    let _ = write!(
                        text,
                        " Heute nicht handelbar, der Papiertag läuft ohne sie: {}.",
                        tage::befunde_text(roh.iter().map(String::as_str)).join("; ")
                    );
                }
                punkte.push(punkt("Schlusskurse", Ampel::Gut, text, None));
            } else {
                let roh: Vec<String> = befunde.iter().map(|b| format!("{b:?}")).collect();
                // Zu alte Kurse heilt kein neuer Abruf am selben Abend: Er laege noch spaeter.
                let nur_zu_alt = befunde
                    .iter()
                    .all(|b| matches!(b, depot_regelwerk::DatenFehler::KursZuAlt(_)));
                let (rat, abhilfe) = if nur_zu_alt {
                    (
                        " Ein neuer Abruf hilft heute nicht, er läge noch später. Morgen \
                         rechtzeitig nach 17:35 abrufen (Zeitplan an, Rechner wach).",
                        None,
                    )
                } else {
                    ("", Some(Abhilfe::KurseAbrufen))
                };
                punkte.push(punkt(
                    "Schlusskurse",
                    Ampel::Offen,
                    format!(
                        "Vom {} belegt, Abruf {}. Die Prüfung meldet: {}. Der Papiertag liefe \
                         ohne Fragen an die Berater und ohne Handel.{rat}",
                        datum_text(k.handelstag),
                        zeit_text(k.heute.stand()),
                        tage::befunde_text(roh.iter().map(String::as_str)).join("; ")
                    ),
                    abhilfe,
                ));
            }
            true
        }
        Err(e) => {
            let text = match tageskurse(s, &universum) {
                Ok(k) => format!(
                    "Noch keine Kurse von heute ({}); jüngste belegte Kurse vom {}. Abruf nach \
                     17:35 Uhr.",
                    datum_text(heute),
                    datum_text(k.handelstag)
                ),
                Err(_) => halt_text(e),
            };
            punkte.push(punkt(
                "Schlusskurse",
                Ampel::Offen,
                text,
                Some(Abhilfe::KurseAbrufen),
            ));
            false
        }
    };
    let heute_ist = if kurse_heute {
        Heute::Kurse
    } else {
        Heute::OhneKurse
    };
    termin_und_tag(s, heute, &heute_ist, punkte);
}

/// Handelt heute keine Boerse der Liste, sagen Schlusskurse und Papiertag das, und es wird
/// nichts verlangt. `true` in diesem Fall.
fn geschlossen(s: &Stand, heute: Datum, punkte: &mut Vec<Punkt>) -> bool {
    if liste::handelstag(heute) {
        return false;
    }
    let grund = liste::feiertag(heute).unwrap_or_else(|| "Wochenende".into());
    punkte.push(punkt(
        "Schlusskurse",
        Ampel::Gut,
        format!("Heute handelt keine Börse der Liste ({grund}): kein Abruf nötig."),
        None,
    ));
    termin_und_tag(s, heute, &Heute::Geschlossen(grund), punkte);
    true
}

/// Was heute fuer den Papiertag vorliegt.
enum Heute {
    Kurse,
    OhneKurse,
    /// Keine Boerse der Liste handelt, mit Grund.
    Geschlossen(String),
}

/// Termine und Tageslage.
fn termin_und_tag(s: &Stand, heute: Datum, kurse_heute: &Heute, punkte: &mut Vec<Punkt>) {
    let termine = s.aktuelle_termine();
    let letzter = termine.values().map(|t| t.abruf()).max();
    punkte.push(match letzter {
        Some(z) => punkt(
            "Termine",
            Ampel::Gut,
            format!(
                "{} Termine belegt, zuletzt abgerufen {}.",
                termine.len(),
                zeit_text(z)
            ),
            None,
        ),
        None => punkt(
            "Termine",
            Ampel::Offen,
            "Noch keine Termine belegt.".into(),
            Some(Abhilfe::TermineAbrufen),
        ),
    });
    punkte.push(match (tageslage(s, heute), kurse_heute) {
        (Ok(Tageslage::NurSchutzverkaeufe), _) => punkt(
            "Papiertag",
            Ampel::Halt,
            "Notbremse aktiv. Nur offene Verkaufsorders warten auf einen späteren Kurs.".into(),
            Some(Abhilfe::Pruefen),
        ),
        (Ok(Tageslage::Abgeschlossen(d)), _) => punkt(
            "Papiertag",
            Ampel::Gut,
            format!("Der Papiertag {} ist abgeschlossen.", datum_text(d)),
            None,
        ),
        (Ok(Tageslage::Offen), Heute::Geschlossen(g)) => punkt(
            "Papiertag",
            Ampel::Gut,
            format!("Heute kein Papiertag: Börse geschlossen ({g})."),
            None,
        ),
        (Ok(Tageslage::Offen), Heute::Kurse) => punkt(
            "Papiertag",
            Ampel::Offen,
            format!(
                "Der Papiertag {} kann laufen. Der Kontowächter gleicht beim Start mit dem \
                 Papierkonto ab.",
                datum_text(heute)
            ),
            Some(Abhilfe::PapiertagStarten),
        ),
        (Ok(Tageslage::Offen), Heute::OhneKurse) => punkt(
            "Papiertag",
            Ampel::Offen,
            "Wartet auf die Kurse von heute.".into(),
            Some(Abhilfe::Warten),
        ),
        (Err(e), _) => punkt(
            "Papiertag",
            Ampel::Halt,
            halt_text(e),
            Some(Abhilfe::Pruefen),
        ),
    });
}

fn budget_punkt(b: &KiBudget) -> Option<Punkt> {
    match &b.status {
        BudgetStatus::Gesperrt(t) => Some(punkt(
            berater_name(b.berater),
            Ampel::Halt,
            format!("KI-Kassenbuch gesperrt: {t}"),
            Some(Abhilfe::Pruefen),
        )),
        _ => None,
    }
}

fn naechster_schritt(punkte: &[Punkt]) -> String {
    if let Some(p) = punkte.iter().find(|p| p.ampel == Ampel::Halt) {
        return format!("Halt – {}: {}", p.name, p.text);
    }
    let erster = |a: Abhilfe| punkte.iter().find(|p| p.abhilfe == Some(a));
    for a in [
        Abhilfe::JournalEinrichten,
        Abhilfe::SchluesselKursquelle,
        Abhilfe::SchluesselKursquelleUs,
        Abhilfe::KurseAbrufen,
        Abhilfe::PapiertagStarten,
        Abhilfe::Warten,
    ] {
        if let Some(p) = erster(a) {
            return format!("{}: {}", p.name, p.text);
        }
    }
    "Alles erledigt. Der nächste Papiertag folgt nach dem Abruf der Kurse.".into()
}

fn bp(wert: i64) -> String {
    let ganz = wert.checked_div(100).unwrap_or(0);
    let rest = wert.checked_rem(100).unwrap_or(0).unsigned_abs();
    if rest == 0 {
        format!("{ganz} %")
    } else {
        format!("{ganz},{rest:02} %")
    }
}

/// Die festgeschriebenen Grenzen und KI-Budgets zur Anzeige.
pub fn grenzwerte() -> Vec<Grenzwert> {
    let g = Grenzen::standard();
    let k = KiGrenzen::standard();
    let w = |name, wert| Grenzwert { name, wert };
    vec![
        w("Höchstanteil je Wert", bp(g.max_anteil_bp())),
        w("Höchstens Werte je Topf", g.max_werte().to_string()),
        w("Orders je Topf und Tag", g.max_orders_je_topf().to_string()),
        w("Mindestorder", budget::euro(g.min_order())),
        w("Umschlag je Tag höchstens", bp(g.max_umschlag_bp())),
        w("Halt bei Tagesverlust", bp(g.tagesverlust_bp())),
        w("Notbremse unter Höchststand", bp(g.notbremse_bp())),
        w(
            "ETFs",
            if g.etf_freigegeben() {
                "freigegeben".into()
            } else {
                "gesperrt".into()
            },
        ),
        w(
            "KI-Aufrufe je KI und Tag",
            k.aufrufe_je_zyklus()
                .saturating_mul(k.zyklen_je_tag())
                .to_string(),
        ),
        w(
            "KI-Aufrufe je KI und Monat",
            k.aufrufe_je_monat().to_string(),
        ),
        w(
            "KI-Kosten je KI und Monat",
            budget::euro(k.kosten_je_monat()),
        ),
        w("KI-Kosten je KI und Jahr", budget::euro(k.kosten_je_jahr())),
    ]
}

/// Ein Berater mit Schluesselstand und letztem Hinweis des Anbieters.
type BeraterLage = (BeraterId, SchluesselStand, Option<AnbieterHinweis>);

/// Punkte zu Schluesseln und Anbietern; dazu der Stand je Schluessel und je Berater.
fn schluessel_punkte(
    u: &Umgebung<'_>,
    punkte: &mut Vec<Punkt>,
) -> (Vec<(Wer, SchluesselStand)>, Vec<BeraterLage>) {
    let kursquelle = betrieb::schluessel_stand(u.kursquelle, &u.anbieter, Wer::Kursquelle);
    let mut schluessel = vec![(Wer::Kursquelle, kursquelle)];
    punkte.push(schluessel_punkt(
        "Kursquelle Alpha Vantage",
        kursquelle,
        Abhilfe::SchluesselKursquelle,
    ));
    let braucht_us = beobachtungsliste().is_ok_and(|l| {
        l.iter()
            .any(|w| liste::quelle(w.boerse) == liste::QUELLE_US)
    });
    if braucht_us {
        let st = betrieb::schluessel_stand(u.kursquelle_us, &u.anbieter, Wer::Tiingo);
        schluessel.push((Wer::Tiingo, st));
        punkte.push(schluessel_punkt(
            "Kursquelle Tiingo (NYSE)",
            st,
            Abhilfe::SchluesselKursquelleUs,
        ));
    }
    let mut hinweise = Vec::new();
    for (b, h) in u.berater {
        let wer = betrieb::wer_fuer(b);
        let st = betrieb::schluessel_stand(h, &u.anbieter, wer);
        let hinweis = betrieb::anbieter_hinweis(&u.anbieter, wer, u.jetzt);
        schluessel.push((wer, st));
        hinweise.push((b, st, hinweis));
        punkte.push(berater_punkt(b, st, hinweis));
    }
    if u.anbieter_unlesbar > 0 {
        punkte.push(punkt(
            "Anbieterprotokoll",
            Ampel::Offen,
            format!(
                "{} Zeilen in daten/anbieter.txt unlesbar; sie zählen in der Anzeige nicht. \
                 Der Kostenschutz hängt nicht daran.",
                u.anbieter_unlesbar
            ),
            None,
        ));
    }
    (schluessel, hinweise)
}

/// Je Sperrdatei im Datenordner ein Punkt.
fn sperr_punkte(u: &Umgebung<'_>, punkte: &mut Vec<Punkt>) {
    for s in &u.sperren {
        if s == "ki.halt" {
            punkte.push(kostenschutz_punkt(u.ki_halt.as_deref()));
            continue;
        }
        if s == depot_journal::kurssperre::KURSSPERRE {
            punkte.push(punkt(
                "Kurssperre",
                Ampel::Halt,
                format!(
                    "daten/{s} liegt: Ein Kursabruf endete aus Sicherheitsgründen (die Quelle gab \
                     den Schlüssel zurück). Kein Abruf und kein Papiertag, auch nicht von Hand, \
                     bis ein Mensch den Schlüssel geprüft und die Sperre in der Einrichtung \
                     aufgehoben hat."
                ),
                Some(Abhilfe::KurssperrePruefen),
            ));
            continue;
        }
        punkte.push(punkt(
            "Sperrdatei",
            Ampel::Halt,
            format!(
                "daten/{s} liegt: Ein Lauf hat angehalten oder ist abgebrochen. KI-Aufrufe bleiben \
                 gesperrt, bis ein Mensch geprüft und die Datei gelöscht hat."
            ),
            Some(Abhilfe::Pruefen),
        ));
    }
}

/// Rechnet das Lagebild.
pub fn lagebild(u: &Umgebung<'_>) -> Lagebild {
    let mut punkte = vec![journal_punkt(u.journal)];
    sperr_punkte(u, &mut punkte);
    let (schluessel, hinweise) = schluessel_punkte(u, &mut punkte);
    let heute = u.jetzt.datum();
    let werte = beobachtungsliste();
    let mut budget = Vec::new();
    let mut depot_lage = None;
    let mut gespraech = Vec::new();
    let mut tage = Vec::new();
    match (u.journal, heute, &werte) {
        (Zugang::Offen(s), Some(heute), Ok(werte)) => {
            tag_punkte(s, heute, werte, &mut punkte);
            budget = [BeraterId::Dachs, BeraterId::Maus]
                .into_iter()
                .map(|b| mit_verbrauch(ki_budget(s.ki().buchungen(), b, heute), &u.anbieter, heute))
                .collect();
            punkte.extend(budget.iter().filter_map(budget_punkt));
            depot_lage = Some(depot::depot_lage(s, werte));
            tage = tage::tage(s, &als_universum(werte), &u.anbieter, heute);
            gespraech = s
                .begruendungen()
                .iter()
                .rev()
                .take(GESPRAECH_MAX)
                .map(|(datum, wer, text)| Beitrag {
                    datum: *datum,
                    wer: wer.clone(),
                    text: text.clone(),
                })
                .collect();
        }
        (_, None, _) => punkte.push(punkt(
            "Uhr",
            Ampel::Halt,
            "Die Uhr des Rechners liefert kein Datum.".into(),
            Some(Abhilfe::Pruefen),
        )),
        (_, _, Err(e)) => punkte.push(punkt(
            "Beobachtungsliste",
            Ampel::Halt,
            e.clone(),
            Some(Abhilfe::Pruefen),
        )),
        _ => {}
    }
    let gesamt = punkte.iter().map(|p| p.ampel).max().unwrap_or(Ampel::Gut);
    // Eingriff: jeder Halt und jede offene Voraussetzung, die nur ein Mensch erfuellen kann
    // (Maus zu #31: kein „vollständig bereit“ ohne Journal).
    let halt = punkte
        .iter()
        .filter(|p| {
            p.ampel == Ampel::Halt
                || (p.ampel == Ampel::Offen && p.abhilfe.is_some_and(Abhilfe::braucht_menschen))
        })
        .map(|p| format!("{}: {}", p.name, p.text))
        .collect();
    let kis: Vec<betrieb::KiLage> = hinweise
        .into_iter()
        .map(|(b, st, hinweis)| betrieb::KiLage {
            name: berater_name(b),
            schluessel: st,
            hinweis,
            budget: betrieb::budget_status(&budget, b),
        })
        .collect();
    let betrieb = betrieb::betrieb(halt, &kis);
    Lagebild {
        jetzt: u.jetzt,
        gesamt,
        naechster_schritt: naechster_schritt(&punkte),
        punkte,
        budget,
        depot: depot_lage,
        gespraech,
        grenzen: grenzwerte(),
        betrieb,
        schluessel,
        tage,
    }
}

/// Die Sperre des Kostenschutzes: Halt mit dem Grund aus der Datei.
fn kostenschutz_punkt(inhalt: Option<&str>) -> Punkt {
    let grund = inhalt
        .and_then(|t| t.lines().next())
        .map_or_else(|| "Grund nicht lesbar".to_string(), str::to_string);
    punkt(
        "KI-Kostenschutz",
        Ampel::Halt,
        format!(
            "Beide KIs gesperrt: {grund}. Unter Einrichtung prüfen und die Sperre aufheben; \
             die Kostenprüfung gilt danach weiter für jeden Aufruf."
        ),
        Some(Abhilfe::KostenschutzPruefen),
    )
}
