//! Schluesselstatus, Lage beim Anbieter und der Gesamtstatus des Betriebs.
//!
//! Alles kommt aus dem Anbieterprotokoll (`daten/anbieter.txt`), dem Stand der Schluessel und
//! dem Budget. Nichts davon entscheidet, ob eine KI gefragt wird; das tun weiter nur
//! Kostenschutz und Kassenbuch. Die Anzeige sagt nur in Worten, was gerade gilt.

use depot_journal::anbieter::{self, Eintrag, Ergebnis, Wer};
use depot_kern::{BeraterId, Zeitpunkt};

use crate::{BudgetStatus, Hinterlegt, KiBudget, zeit_text};

/// Stand eines Schluessels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchluesselStand {
    Fehlt,
    /// Hinterlegt, aber seither nicht benutzt.
    Hinterlegt,
    /// Beim letzten Aufruf vom Anbieter angenommen.
    Geprueft(Zeitpunkt),
    /// Beim letzten Aufruf vom Anbieter abgelehnt.
    Abgelehnt(Zeitpunkt),
    /// Hier nicht pruefbar (ausserhalb von Windows) und nie benutzt.
    Unbekannt,
}

impl SchluesselStand {
    pub fn text(self) -> String {
        match self {
            SchluesselStand::Fehlt => "fehlt".into(),
            SchluesselStand::Hinterlegt => "hinterlegt, noch nicht benutzt".into(),
            SchluesselStand::Geprueft(z) => format!("geprüft (angenommen {})", zeit_text(z)),
            SchluesselStand::Abgelehnt(z) => format!("abgelehnt ({})", zeit_text(z)),
            SchluesselStand::Unbekannt => "unbekannt (nur unter Windows prüfbar)".into(),
        }
    }

    /// Klasse fuer die Anzeige: gut, offen oder halt.
    pub fn klasse(self) -> &'static str {
        match self {
            SchluesselStand::Geprueft(_) => "gut",
            SchluesselStand::Abgelehnt(_) | SchluesselStand::Fehlt => "halt",
            SchluesselStand::Hinterlegt | SchluesselStand::Unbekannt => "offen",
        }
    }
}

pub fn wer_fuer(b: BeraterId) -> Wer {
    match b {
        BeraterId::Dachs => Wer::Dachs,
        BeraterId::Maus => Wer::Maus,
    }
}

/// Der Stand eines Schluessels aus der Anmeldeverwaltung und dem letzten Aufruf, der etwas
/// ueber ihn sagt. Ein neu hinterlegter Schluessel ist wieder nur „hinterlegt“.
pub fn schluessel_stand(h: Hinterlegt, protokoll: &[Eintrag], wer: Wer) -> SchluesselStand {
    if h == Hinterlegt::Nein {
        return SchluesselStand::Fehlt;
    }
    let letzter = protokoll.iter().rev().filter(|e| e.wer == wer).find(|e| {
        e.ergebnis == Ergebnis::Neu
            || e.ergebnis == Ergebnis::Abgelehnt
            || e.ergebnis.schluessel_angenommen()
    });
    match letzter {
        Some(e) if e.ergebnis == Ergebnis::Abgelehnt => SchluesselStand::Abgelehnt(e.zeit),
        Some(e) if e.ergebnis.schluessel_angenommen() => SchluesselStand::Geprueft(e.zeit),
        Some(_) => SchluesselStand::Hinterlegt,
        None if h == Hinterlegt::Ja => SchluesselStand::Hinterlegt,
        None => SchluesselStand::Unbekannt,
    }
}

/// Was der Anbieter zuletzt gemeldet hat, wenn es den Betrieb beruehrt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnbieterHinweis {
    /// Limit erreicht. `aktiv`: heute oder die genannte Wartezeit laeuft noch.
    Limit {
        zeit: Zeitpunkt,
        warten_bis: Option<Zeitpunkt>,
        aktiv: bool,
    },
    Guthaben(Zeitpunkt),
    Ueberlastet(Zeitpunkt),
}

impl AnbieterHinweis {
    pub fn text(self) -> String {
        match self {
            AnbieterHinweis::Limit {
                zeit, warten_bis, ..
            } => {
                let naechster = warten_bis.map_or_else(
                    || "beim nächsten Papiertag".to_string(),
                    |w| format!("beim ersten Papiertag nach {}", zeit_text(w)),
                );
                format!(
                    "Anbieterlimit erreicht ({}). Nächster Versuch: {naechster}.",
                    zeit_text(zeit)
                )
            }
            AnbieterHinweis::Guthaben(z) => format!(
                "Beim Anbieter fehlt Guthaben ({}). Bitte im Konto des Anbieters aufladen; der \
                 nächste Papiertag versucht es wieder.",
                zeit_text(z)
            ),
            AnbieterHinweis::Ueberlastet(z) => format!(
                "Anbieter war überlastet oder gestört ({}). Nächster Versuch beim nächsten \
                 Papiertag.",
                zeit_text(z)
            ),
        }
    }
}

/// Der letzte Hinweis des Anbieters, solange danach kein erfolgreicher Aufruf kam.
pub fn anbieter_hinweis(
    protokoll: &[Eintrag],
    wer: Wer,
    jetzt: Zeitpunkt,
) -> Option<AnbieterHinweis> {
    let e = anbieter::letzte_meldung(protokoll, wer)?;
    match e.ergebnis {
        Ergebnis::Limit => Some(AnbieterHinweis::Limit {
            zeit: e.zeit,
            warten_bis: e.warten_bis,
            // Dieselbe Regel, nach der der Berater vor dem Aufruf pausiert.
            aktiv: anbieter::limit_gilt(e, jetzt),
        }),
        Ergebnis::Guthaben => Some(AnbieterHinweis::Guthaben(e.zeit)),
        Ergebnis::Ueberlastet => Some(AnbieterHinweis::Ueberlastet(e.zeit)),
        _ => None,
    }
}

/// Der Gesamtstatus in einem Wort, mit Gruenden.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Betrieb {
    /// Alles bereit; was offen ist, erledigen Zeitplan oder Knoepfe.
    Bereit,
    /// Laeuft, aber ohne mindestens eine KI. Ohne beide entstehen keine neuen Auftraege.
    OhneKi(Vec<String>),
    /// Wartet von selbst (Budget, Anbieterlimit), keine KI ist gerade frei.
    Pausiert(Vec<String>),
    /// Ein Mensch muss etwas tun.
    EingriffNoetig(Vec<String>),
}

impl Betrieb {
    pub fn titel(&self) -> &'static str {
        match self {
            Betrieb::Bereit => "Vollständig bereit",
            Betrieb::OhneKi(_) => "Läuft ohne KI",
            Betrieb::Pausiert(_) => "Pausiert",
            Betrieb::EingriffNoetig(_) => "Eingriff nötig",
        }
    }

    pub fn klasse(&self) -> &'static str {
        match self {
            Betrieb::Bereit => "gut",
            Betrieb::OhneKi(_) | Betrieb::Pausiert(_) => "offen",
            Betrieb::EingriffNoetig(_) => "halt",
        }
    }

    pub fn gruende(&self) -> &[String] {
        match self {
            Betrieb::Bereit => &[],
            Betrieb::OhneKi(g) | Betrieb::Pausiert(g) | Betrieb::EingriffNoetig(g) => g,
        }
    }
}

/// Was der Gesamtstatus von einer KI wissen muss.
#[derive(Clone, Debug)]
pub struct KiLage {
    pub name: &'static str,
    pub schluessel: SchluesselStand,
    pub hinweis: Option<AnbieterHinweis>,
    pub budget: Option<BudgetStatus>,
}

/// Rechnet den Gesamtstatus. `halt` sind die Punkte, bei denen ein Mensch etwas tun muss:
/// jeder Halt und jede offene Voraussetzung der Einrichtung (Name: Text).
pub fn betrieb(halt: Vec<String>, kis: &[KiLage]) -> Betrieb {
    let mut eingriff = halt;
    let mut pause = Vec::new();
    let mut ohne = Vec::new();
    for k in kis {
        match k.schluessel {
            SchluesselStand::Abgelehnt(_) => eingriff.push(format!(
                "{}: Schlüssel vom Anbieter abgelehnt. Bitte unter Einrichtung neu hinterlegen.",
                k.name
            )),
            SchluesselStand::Fehlt => {
                ohne.push(format!("{}: kein Schlüssel, wird nicht gefragt.", k.name));
            }
            _ => {}
        }
        match k.hinweis {
            Some(h @ AnbieterHinweis::Guthaben(_)) => {
                eingriff.push(format!("{}: {}", k.name, h.text()));
            }
            Some(h @ AnbieterHinweis::Limit { aktiv: true, .. }) => {
                pause.push(format!("{}: {}", k.name, h.text()));
            }
            _ => {}
        }
        if let Some(
            b @ (BudgetStatus::PausiertBisMorgen
            | BudgetStatus::PausiertBisMonatsanfang
            | BudgetStatus::PausiertBisJahresanfang),
        ) = &k.budget
        {
            pause.push(format!("{}: KI-Budget {}.", k.name, b.text()));
        }
    }
    if !eingriff.is_empty() {
        // Was sonst noch gilt, steht dahinter: Nach dem Eingriff soll nichts ueberraschen.
        eingriff.extend(ohne);
        eingriff.extend(pause);
        return Betrieb::EingriffNoetig(eingriff);
    }
    let pausiert = kis.iter().filter(|k| {
        let n = k.name;
        pause.iter().any(|p| p.starts_with(n))
    });
    if !pause.is_empty() && pausiert.count() == kis.len() && ohne.is_empty() {
        return Betrieb::Pausiert(pause);
    }
    let mut gruende = ohne;
    gruende.extend(pause);
    if gruende.is_empty() {
        return Betrieb::Bereit;
    }
    gruende.push(
        "Ohne beide KIs entstehen keine neuen Aufträge, nur die Pflichtaufträge des Regelwerks."
            .into(),
    );
    Betrieb::OhneKi(gruende)
}

/// Fuer `KiLage` aus dem Budget.
pub fn budget_status(budget: &[KiBudget], b: BeraterId) -> Option<BudgetStatus> {
    budget
        .iter()
        .find(|x| x.berater == b)
        .map(|x| x.status.clone())
}
