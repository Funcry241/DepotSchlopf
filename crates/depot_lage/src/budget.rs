//! KI-Budget je Berater, direkt aus dem Kassenbuch im Journal.
//!
//! Die Live-Ansicht prüft eine Reservierung mit den Einstellungen des Beraters.
//!
//! Bestätigte Minderkosten vermindern die gebuchten Beträge; ungeklärte Reservierungen
//! bleiben voll gebucht. Das unverbindliche Anbieterprotokoll ist keine Budgetfreigabe.

use depot_journal::anbieter::{Eintrag, Kosten};
use depot_journal::{KiStand, KiZweck};
use depot_kern::modelle::BeraterEinstellung;
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
    /// Eine Einstellung unterbindet Aufrufe, unabhängig vom Kassenbuch.
    KeineAufrufe(String),
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
            BudgetStatus::KeineAufrufe(grund) => format!("Keine Aufrufe · {grund}"),
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
    /// Heute gebucht (abgerechnete Kosten plus ungeklärte Höchstkosten).
    pub heute_gebucht: Cent,
    pub tagesrahmen: Cent,
    pub handelstage_rest: u32,
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
    /// Tatsaechlich verbraucht in diesem Monat, wie der Anbieter es gemeldet hat
    /// (Anbieterprotokoll). Der Kostenschutz rechnet weiter mit dem Gebuchten.
    pub monat_verbraucht: Cent,
    /// Aufrufe dieses Monats mit bekannten Kosten.
    pub monat_abgerechnet: usize,
    /// Aufrufe dieses Monats, deren Kosten unbekannt sind (Antwort fehlte).
    pub monat_unbekannt: usize,
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
        Err(
            Sperre::ZyklusVoll
            | Sperre::TagVoll
            | Sperre::TagesrahmenErschoepft
            | Sperre::ErstmeinungReserviert,
        ) => BudgetStatus::PausiertBisMorgen,
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
    let deckel = KiDeckel::standard(berater);
    let st = match tagnummer(heute) {
        Some(zyklus) => status(pruefe(
            &KiGrenzen::standard(),
            &deckel,
            buchungen,
            berater,
            heute,
            zyklus,
        )),
        None => BudgetStatus::Gesperrt("Tagnummer nicht berechenbar".into()),
    };
    budgetwerte(
        buchungen,
        berater,
        heute,
        Some(BeraterEinstellung::standard(berater)),
        deckel.hoechstkosten(),
        st,
    )
}

/// Zeigt den Rahmen für eine Erstmeinung an Handelstagen und für die Vorbereitung
/// an börsenfreien Tagen. Bei unlesbaren Einstellungen stehen nur gebuchte Werte fest.
pub fn ki_budget_mit_einstellung(
    ki: &KiStand,
    berater: BeraterId,
    heute: Datum,
    einstellung: Result<BeraterEinstellung, &str>,
) -> KiBudget {
    let einstellung = match einstellung {
        Ok(einstellung) => einstellung,
        Err(grund) => {
            return budgetwerte(
                ki.buchungen(),
                berater,
                heute,
                None,
                None,
                BudgetStatus::KeineAufrufe(grund.into()),
            );
        }
    };
    let modell = einstellung.auswahl.modell();
    let (zweck, deckel) = if depot_kern::liste::handelstag(heute) {
        (KiZweck::Erstmeinung, KiDeckel::fuer_modell(modell))
    } else {
        (KiZweck::Vorbereitung, KiDeckel::kurz_fuer_modell(modell))
    };
    let st = if einstellung.max_aufrufe == 0 {
        BudgetStatus::KeineAufrufe("Tageslimit 0".into())
    } else if let Some(zyklus) = tagnummer(heute) {
        status(ki.pruefe_reservierung(berater, heute, zyklus, zweck, einstellung))
    } else {
        BudgetStatus::Gesperrt("Tagnummer nicht berechenbar".into())
    };
    budgetwerte(
        ki.buchungen(),
        berater,
        heute,
        Some(einstellung),
        deckel.hoechstkosten(),
        st,
    )
}

fn budgetwerte(
    buchungen: &[Buchung],
    berater: BeraterId,
    heute: Datum,
    einstellung: Option<BeraterEinstellung>,
    je_aufruf: Option<Cent>,
    status: BudgetStatus,
) -> KiBudget {
    let g = KiGrenzen::standard();
    let eigene = || buchungen.iter().filter(move |b| b.berater == berater);
    let monat = || {
        eigene().filter(move |b| b.datum.jahr() == heute.jahr() && b.datum.monat() == heute.monat())
    };
    let jahr = || eigene().filter(move |b| b.datum.jahr() == heute.jahr());
    let heute_b = || eigene().filter(move |b| b.datum == heute);
    let monat_gebucht = summe(monat());
    let jahr_gebucht = summe(jahr());
    let hartes_tageslimit = g.aufrufe_je_zyklus().saturating_mul(g.zyklen_je_tag());
    let heute_aufrufe_max = einstellung.map_or(0, |e| {
        u32::try_from(e.max_aufrufe)
            .unwrap_or(u32::MAX)
            .min(hartes_tageslimit)
    });
    let tagesrahmen = einstellung
        .and_then(|e| {
            if depot_kern::liste::handelstag(heute) {
                depot_ki::tagesrahmen(&g, buchungen, berater, heute)
            } else {
                depot_ki::vorbereitungsrahmen_fuer_modell(&g, buchungen, e.auswahl.modell(), heute)
            }
            .ok()
        })
        .unwrap_or(Cent::NULL);
    KiBudget {
        berater,
        heute_aufrufe: heute_b().count(),
        heute_aufrufe_max,
        heute_gebucht: summe(heute_b()),
        tagesrahmen,
        handelstage_rest: depot_ki::handelstage_rest(heute),
        monat_aufrufe: monat().count(),
        monat_aufrufe_max: g.aufrufe_je_monat(),
        monat_gebucht,
        monat_grenze: g.kosten_je_monat(),
        monat_rest: rest(g.kosten_je_monat(), monat_gebucht),
        jahr_gebucht,
        jahr_grenze: g.kosten_je_jahr(),
        jahr_rest: rest(g.kosten_je_jahr(), jahr_gebucht),
        je_aufruf,
        status,
        monat_verbraucht: Cent::NULL,
        monat_abgerechnet: 0,
        monat_unbekannt: 0,
    }
}

/// Ergaenzt das Budget um den tatsaechlichen Verbrauch des Monats aus dem Anbieterprotokoll.
pub fn mit_verbrauch(mut b: KiBudget, protokoll: &[Eintrag], heute: Datum) -> KiBudget {
    let wer = crate::betrieb::wer_fuer(b.berater);
    for e in protokoll.iter().filter(|e| e.wer == wer) {
        let Some(d) = e.zeit.frankfurt_tag() else {
            continue;
        };
        if d.jahr() != heute.jahr() || d.monat() != heute.monat() {
            continue;
        }
        match e.ist {
            Kosten::Betrag(c) => {
                b.monat_verbraucht = b.monat_verbraucht.checked_add(c).unwrap_or(Cent(i64::MAX));
                b.monat_abgerechnet = b.monat_abgerechnet.saturating_add(1);
            }
            Kosten::Unbekannt => b.monat_unbekannt = b.monat_unbekannt.saturating_add(1),
            Kosten::Keine => {}
        }
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_journal::{Art, Journal, inhalt_fuer, vermerk_fuer};
    use depot_kern::modelle::{Denktiefe, Modell, Modellwahl};
    use std::fs;

    #[test]
    fn geaenderter_tarif_und_tageslimit_bestimmen_den_live_status() -> Result<(), String> {
        let wer = BeraterId::Maus;
        let heute = Datum::neu(2026, 11, 27).ok_or("Datum")?;
        let gestern = Datum::neu(2026, 11, 26).ok_or("Datum")?;
        let zyklus = tagnummer(gestern).ok_or("Zyklus")?;
        let bisher = format!("Maus;{gestern};{zyklus};1970");
        let zyklus_heute = tagnummer(heute).ok_or("Zyklus")?;
        let aufruf = format!("Maus;{heute};{zyklus_heute};0");
        let bytes = inhalt_fuer(&[
            (Art::KiGrenzen, &"0".repeat(64)),
            (Art::Ki, &bisher),
            (Art::Ki, &aufruf),
        ]);
        let ordner = std::env::temp_dir().join(format!("depot-live-budget-{}", std::process::id()));
        fs::create_dir(&ordner).map_err(|e| e.to_string())?;
        let ergebnis = (|| {
            let pfad = ordner.join("journal.txt");
            fs::write(&pfad, &bytes).map_err(|e| e.to_string())?;
            fs::write(
                pfad.with_extension("vermerk"),
                vermerk_fuer(&bytes, 0).ok_or("Vermerk")?,
            )
            .map_err(|e| e.to_string())?;
            let journal = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
            let ki = journal.stand().ki();

            let terra =
                ki_budget_mit_einstellung(ki, wer, heute, Ok(BeraterEinstellung::standard(wer)));
            let sol = BeraterEinstellung {
                auswahl: Modellwahl::neu(wer, Modell::Sol62, Denktiefe::Mittel)
                    .ok_or("Modellwahl")?,
                max_aufrufe: 3,
            };
            let teurer = ki_budget_mit_einstellung(ki, wer, heute, Ok(sol));
            assert_eq!(terra.status, BudgetStatus::Bereit);
            assert_eq!(terra.je_aufruf, Some(Cent(13)));
            assert_eq!(teurer.status, BudgetStatus::PausiertBisMorgen);
            assert_eq!(teurer.je_aufruf, Some(Cent(26)));
            assert_eq!(teurer.heute_aufrufe_max, 3);
            assert_eq!(teurer.tagesrahmen, Cent(15));
            assert_eq!(teurer.monat_rest, Cent(30));

            let erreicht = ki_budget_mit_einstellung(
                ki,
                wer,
                heute,
                Ok(BeraterEinstellung {
                    max_aufrufe: 1,
                    ..BeraterEinstellung::standard(wer)
                }),
            );
            assert_eq!(erreicht.status, BudgetStatus::PausiertBisMorgen);
            assert_eq!(erreicht.heute_aufrufe, 1);
            assert_eq!(erreicht.heute_aufrufe_max, 1);

            let deaktiviert = ki_budget_mit_einstellung(
                ki,
                wer,
                heute,
                Ok(BeraterEinstellung {
                    max_aufrufe: 0,
                    ..sol
                }),
            );
            assert!(matches!(deaktiviert.status, BudgetStatus::KeineAufrufe(_)));
            assert_eq!(deaktiviert.heute_aufrufe_max, 0);
            let unerreichbar = ki_budget_mit_einstellung(ki, wer, heute, Err("unlesbar"));
            assert_eq!(
                unerreichbar.status,
                BudgetStatus::KeineAufrufe("unlesbar".into())
            );
            assert_eq!(unerreichbar.je_aufruf, None);
            assert_eq!(unerreichbar.monat_gebucht, Cent(1970));
            Ok(())
        })();
        let _ = fs::remove_dir_all(ordner);
        ergebnis
    }
}
