//! KI-Budget je Berater, direkt aus dem Kassenbuch im Journal.
//!
//! Der Status kommt aus `depot_ki::pruefe` mit denselben Grenzen, demselben Deckel und demselben
//! Zyklus (`depot_sim::tagnummer`), mit denen der Papiertag reserviert. Was die Anzeige
//! „bereit“ nennt, wuerde der Kostenschutz also gerade erlauben.
//!
//! Das Kassenbuch kennt bisher nur Reservierungen der Hoechstkosten je Aufruf. Bis die
//! tatsaechlichen Kosten im Journal stehen, ist „gebucht“ immer die Obergrenze dessen, was
//! verbraucht sein kann.

use depot_kern::{BeraterId, Cent, Datum, Preis};
use depot_ki::{Buchung, KiDeckel, KiGrenzen, Sperre, pruefe};
use depot_sim::tagnummer;

/// Ob die KI jetzt gefragt werden duerfte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BudgetStatus {
    Bereit,
    /// Heute ist nichts mehr frei.
    PausiertBisMorgen,
    /// Aufrufe oder Kosten des Monats sind ausgeschoepft.
    PausiertBisMonatsanfang,
    PausiertBisJahresanfang,
    /// Kassenbuch, Uhr oder Rechnung unstimmig: Halt bis zur Pruefung.
    Gesperrt(String),
}

impl BudgetStatus {
    pub fn text(&self) -> String {
        match self {
            BudgetStatus::Bereit => "bereit".into(),
            BudgetStatus::PausiertBisMorgen => "pausiert bis morgen".into(),
            BudgetStatus::PausiertBisMonatsanfang => "pausiert bis Monatsanfang".into(),
            BudgetStatus::PausiertBisJahresanfang => "pausiert bis Jahresanfang".into(),
            BudgetStatus::Gesperrt(t) => format!("gesperrt: {t}"),
        }
    }
}

/// Das KI-Budget eines Beraters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KiBudget {
    pub berater: BeraterId,
    pub heute_aufrufe: usize,
    pub heute_aufrufe_max: u32,
    /// Heute gebucht (Hoechstkosten der reservierten Aufrufe).
    pub heute_gebucht: Cent,
    pub monat_aufrufe: usize,
    pub monat_aufrufe_max: u32,
    pub monat_gebucht: Cent,
    pub monat_grenze: Cent,
    /// Grenze minus gebucht, nie unter null.
    pub monat_rest: Cent,
    pub jahr_gebucht: Cent,
    pub jahr_grenze: Cent,
    pub jahr_rest: Cent,
    /// Was der naechste Aufruf reserviert; `None`, wenn es sich nicht berechnen laesst.
    pub je_aufruf: Option<Cent>,
    pub status: BudgetStatus,
}

pub(crate) fn euro(c: Cent) -> String {
    Preis::aus_cent(c).map_or_else(
        || format!("{} Cent", c.0),
        |p| format!("{} €", p.euro_text()),
    )
}

fn summe<'a>(b: impl Iterator<Item = &'a Buchung>) -> Cent {
    b.fold(Cent::NULL, |s, x| {
        s.checked_add(x.reserviert).unwrap_or(Cent(i64::MAX))
    })
}

fn rest(grenze: Cent, gebucht: Cent) -> Cent {
    grenze
        .checked_sub(gebucht)
        .filter(|r| r.0 > 0)
        .unwrap_or(Cent::NULL)
}

fn status(s: Result<Cent, Sperre>) -> BudgetStatus {
    match s {
        Ok(_) => BudgetStatus::Bereit,
        Err(Sperre::ZyklusVoll | Sperre::TagVoll) => BudgetStatus::PausiertBisMorgen,
        Err(Sperre::MonatVoll | Sperre::MonatsbudgetErschoepft) => {
            BudgetStatus::PausiertBisMonatsanfang
        }
        Err(Sperre::JahresbudgetErschoepft) => BudgetStatus::PausiertBisJahresanfang,
        Err(Sperre::UhrRueckwaerts) => {
            BudgetStatus::Gesperrt("Uhr steht vor der letzten Buchung".into())
        }
        Err(Sperre::ZyklusRueckwaerts) => {
            BudgetStatus::Gesperrt("Zyklus vor der letzten Buchung".into())
        }
        Err(Sperre::Rechenfehler) => BudgetStatus::Gesperrt("Rechenfehler".into()),
        Err(Sperre::Kassenbuch(t)) => BudgetStatus::Gesperrt(t),
    }
}

/// Rechnet das Budget eines Beraters fuer den Tag `heute`.
pub fn ki_budget(buchungen: &[Buchung], berater: BeraterId, heute: Datum) -> KiBudget {
    let g = KiGrenzen::standard();
    let deckel = KiDeckel::standard(berater);
    let eigene = || buchungen.iter().filter(move |b| b.berater == berater);
    let monat = || {
        eigene().filter(move |b| b.datum.jahr() == heute.jahr() && b.datum.monat() == heute.monat())
    };
    let jahr = || eigene().filter(move |b| b.datum.jahr() == heute.jahr());
    let heute_b = || eigene().filter(move |b| b.datum == heute);
    let monat_gebucht = summe(monat());
    let jahr_gebucht = summe(jahr());
    let st = match tagnummer(heute) {
        Some(zyklus) => status(pruefe(&g, &deckel, buchungen, berater, heute, zyklus)),
        None => BudgetStatus::Gesperrt("Tagnummer nicht berechenbar".into()),
    };
    KiBudget {
        berater,
        heute_aufrufe: heute_b().count(),
        heute_aufrufe_max: g.aufrufe_je_zyklus().saturating_mul(g.zyklen_je_tag()),
        heute_gebucht: summe(heute_b()),
        monat_aufrufe: monat().count(),
        monat_aufrufe_max: g.aufrufe_je_monat(),
        monat_gebucht,
        monat_grenze: g.kosten_je_monat(),
        monat_rest: rest(g.kosten_je_monat(), monat_gebucht),
        jahr_gebucht,
        jahr_grenze: g.kosten_je_jahr(),
        jahr_rest: rest(g.kosten_je_jahr(), jahr_gebucht),
        je_aufruf: deckel.hoechstkosten(),
        status: st,
    }
}
