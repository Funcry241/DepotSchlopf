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

mod budget;
mod depot;
pub mod html;
mod zugang;

pub use budget::{BudgetStatus, KiBudget, ki_budget};
pub use depot::{AuftragZeile, DepotLage, Position, TopfLage};
pub use zugang::{PAPIERKONTO, mit_journal};

use depot_journal::Stand;
use depot_kern::liste::{Wert, als_universum, beobachtungsliste};
use depot_kern::{BeraterId, Datum, Grenzen, Zeitpunkt};
use depot_ki::KiGrenzen;
use depot_regelwerk::pruefe_kurse;
use depot_sim::{LaufFehler, Tageslage, kurse_von_heute, tageskurse, tageslage};

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
    /// Schluessel der Kursquelle (Alpha Vantage).
    pub kursquelle: Hinterlegt,
    /// Schluessel je Berater; ohne Schluessel wird der Berater nicht gefragt.
    pub berater: [(BeraterId, Hinterlegt); 2],
    /// Liegengebliebene Sperrdateien unter `daten/` (abgebrochener frueherer Lauf).
    pub sperren: Vec<String>,
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
    SchluesselBerater(BeraterId),
    KurseAbrufen,
    TermineAbrufen,
    PapiertagStarten,
    /// Nur ein Mensch kann helfen: pruefen, was im Text steht.
    Pruefen,
    /// Abwarten, etwa bis ein laufender Schritt fertig ist oder das Budget wieder frei ist.
    Warten,
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
}

/// So viele Beitraege zeigt das Lagebild hoechstens.
pub const GESPRAECH_MAX: usize = 60;

pub(crate) fn datum_text(d: Datum) -> String {
    format!("{:02}.{:02}.{}", d.tag(), d.monat(), d.jahr())
}

/// Ortszeit in Frankfurt, wie der Kalender des Kerns sie umrechnet; eine doppelt vorkommende
/// Stunde steht in UTC.
pub(crate) fn zeit_text(z: Zeitpunkt) -> String {
    for (stunden, zone) in [(1_i64, "MEZ"), (2, "MESZ")] {
        let ort = stunden
            .checked_mul(3_600)
            .and_then(|s| z.sekunden().checked_add(s))
            .and_then(Zeitpunkt::aus_sekunden);
        if let Some(ort) = ort
            && let (Some(d), Some((h, m, s))) = (ort.datum(), ort.uhrzeit())
            && Zeitpunkt::frankfurt(d, h, m, s) == Some(z)
        {
            return format!("{} {h:02}:{m:02} {zone}", datum_text(d));
        }
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

fn schluessel_punkt(name: &'static str, h: Hinterlegt, abhilfe: Abhilfe) -> Punkt {
    match h {
        Hinterlegt::Ja => punkt(name, Ampel::Gut, "Schlüssel hinterlegt.".into(), None),
        Hinterlegt::Nein => punkt(name, Ampel::Offen, "Schlüssel fehlt.".into(), Some(abhilfe)),
        Hinterlegt::Unbekannt => punkt(
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

fn berater_punkt(b: BeraterId, h: Hinterlegt) -> Punkt {
    let name = berater_name(b);
    match h {
        Hinterlegt::Ja => punkt(name, Ampel::Gut, "Schlüssel hinterlegt.".into(), None),
        Hinterlegt::Nein => punkt(
            name,
            Ampel::Offen,
            "Nicht angebunden: wird nicht gefragt, der Papiertag läuft ohne ihn.".into(),
            Some(Abhilfe::SchluesselBerater(b)),
        ),
        Hinterlegt::Unbekannt => punkt(
            name,
            Ampel::Offen,
            "Hier nicht prüfbar (nur unter Windows).".into(),
            None,
        ),
    }
}

/// Kurse, Termine und Tageslage aus dem Journal, genau wie der Papiertag sie prueft.
fn tag_punkte(s: &Stand, heute: Datum, werte: &[Wert], punkte: &mut Vec<Punkt>) {
    let universum = als_universum(werte);
    let kurse_heute = match kurse_von_heute(s, &universum, heute) {
        Ok(k) => {
            let befunde = pruefe_kurse(
                &Grenzen::standard(),
                &universum,
                k.handelstag,
                &k.heute,
                &k.vortag,
            );
            if befunde.is_empty() {
                punkte.push(punkt(
                    "Schlusskurse",
                    Ampel::Gut,
                    format!(
                        "Vom {} belegt und geprüft, Abruf {}.",
                        datum_text(k.handelstag),
                        zeit_text(k.heute.stand())
                    ),
                    None,
                ));
            } else {
                punkte.push(punkt(
                    "Schlusskurse",
                    Ampel::Offen,
                    format!(
                        "Vom {} belegt, die Prüfung meldet {} Befunde: Der Papiertag liefe \
                         ohne Fragen an die Berater. {befunde:?}",
                        datum_text(k.handelstag),
                        befunde.len()
                    ),
                    Some(Abhilfe::KurseAbrufen),
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
    termin_und_tag(s, heute, kurse_heute, punkte);
}

/// Termine und Tageslage.
fn termin_und_tag(s: &Stand, heute: Datum, kurse_heute: bool, punkte: &mut Vec<Punkt>) {
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
    punkte.push(match tageslage(s, heute) {
        Ok(Tageslage::Abgeschlossen(d)) => punkt(
            "Papiertag",
            Ampel::Gut,
            format!("Der Papiertag {} ist abgeschlossen.", datum_text(d)),
            None,
        ),
        Ok(Tageslage::Offen) if kurse_heute => punkt(
            "Papiertag",
            Ampel::Offen,
            format!(
                "Der Papiertag {} kann laufen. Der Kontowächter gleicht beim Start mit dem \
                 Papierkonto ab.",
                datum_text(heute)
            ),
            Some(Abhilfe::PapiertagStarten),
        ),
        Ok(Tageslage::Offen) => punkt(
            "Papiertag",
            Ampel::Offen,
            "Wartet auf die Kurse von heute.".into(),
            Some(Abhilfe::Warten),
        ),
        Err(e) => punkt(
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

/// Rechnet das Lagebild.
pub fn lagebild(u: &Umgebung<'_>) -> Lagebild {
    let mut punkte = vec![journal_punkt(u.journal)];
    for s in &u.sperren {
        punkte.push(punkt(
            "Sperrdatei",
            Ampel::Halt,
            format!(
                "daten/{s} liegt noch: früherer Lauf abgebrochen. KI-Aufrufe bleiben gesperrt, \
                 bis ein Mensch geprüft und die Datei gelöscht hat."
            ),
            Some(Abhilfe::Pruefen),
        ));
    }
    punkte.push(schluessel_punkt(
        "Kursquelle Alpha Vantage",
        u.kursquelle,
        Abhilfe::SchluesselKursquelle,
    ));
    for (b, h) in u.berater {
        punkte.push(berater_punkt(b, h));
    }
    let heute = u.jetzt.datum();
    let werte = beobachtungsliste();
    let mut budget = Vec::new();
    let mut depot_lage = None;
    let mut gespraech = Vec::new();
    match (u.journal, heute, &werte) {
        (Zugang::Offen(s), Some(heute), Ok(werte)) => {
            tag_punkte(s, heute, werte, &mut punkte);
            budget = [BeraterId::Dachs, BeraterId::Maus]
                .into_iter()
                .map(|b| ki_budget(s.ki().buchungen(), b, heute))
                .collect();
            punkte.extend(budget.iter().filter_map(budget_punkt));
            depot_lage = Some(depot::depot_lage(s, werte));
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
    Lagebild {
        jetzt: u.jetzt,
        gesamt,
        naechster_schritt: naechster_schritt(&punkte),
        punkte,
        budget,
        depot: depot_lage,
        gespraech,
        grenzen: grenzwerte(),
    }
}
