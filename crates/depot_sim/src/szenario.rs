//! Das erfundene Bild der Welt und die Schnittstelle zu den Beratern.
//!
//! Alles hier ist erfunden: Werte, Kurse, Ereignisse und die Antworten von Dachs und Maus. Die
//! Kurse tragen die Herkunft `Simulation`; mit anderen Kursen laeuft die Simulation nicht.

use std::collections::BTreeMap;

use depot_journal::AufrufErlaubnis;
use depot_kern::{
    BeraterId, Cent, Datum, Herkunft, Instrument, InstrumentArt, Isin, Kurs, Kurse, Topf, TopfId,
    Universum, Vorschlag, Zeitpunkt,
};

/// Ein Handelstag: Schlusskurse und was an diesem Tag in der Welt passiert oder angekuendigt ist.
#[derive(Clone, Debug)]
pub struct Tag {
    pub datum: Datum,
    pub kurse: Kurse,
    pub ereignisse: Vec<String>,
}

/// Ein ganzer simulierter Zeitraum.
#[derive(Clone, Debug)]
pub struct Szenario {
    pub universum: Universum,
    /// Startstand der Toepfe im Papierdepot.
    pub start: Vec<Topf>,
    /// Schlusskurse vor dem ersten Tag, fuer den Sprungtest und den Tagesverlust.
    pub vorlauf: Kurse,
    pub tage: Vec<Tag>,
}

/// Was ein Berater zu sehen bekommt.
#[derive(Clone, Copy, Debug)]
pub struct Lage<'a> {
    pub datum: Datum,
    pub topf: &'a Topf,
    pub kurse: &'a Kurse,
    pub ereignisse: &'a [String],
}

/// So beginnt die Einschaetzung oder Begruendung, wenn ein Berater gefragt wurde, seine Antwort
/// aber nicht verwerten konnte. Die Anzeige erkennt daran auch Eintraege alter Journale, die
/// `verwertbar` noch nicht kannten.
pub const KEINE_VERWERTBARE_ANTWORT: &str = "keine verwertbare Antwort";

/// Die Antwort auf eine Frage nach Zielgewichten: Einschaetzung der Lage und vielleicht ein
/// Vorschlag. Die Kaufentscheidung trifft der Berater; das Journal haelt seine Begruendung fest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Antwort {
    pub prognose: Option<depot_journal::Weltprognose>,
    /// Die Antwort kam und ist verwertbar. Nur dann gilt sie im Journal als verwendet; eine
    /// verworfene (unlesbar, Fehler, Kosten) steht dort ausdruecklich als verworfen.
    pub verwertbar: bool,
    pub einschaetzung: String,
    pub vorschlag: Option<Vorschlag>,
}

/// Die Zweitmeinung zu einem Vorschlag. Nur eine ausdrueckliche Zustimmung laesst ihn weiter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Urteil {
    /// Wie bei `Antwort`: Nur ein verwertbares Urteil ist Zustimmung oder Veto des Pruefers.
    pub verwertbar: bool,
    pub zustimmung: bool,
    pub begruendung: String,
}

/// Ein Berater. Jede Frage kostet eine `AufrufErlaubnis`; die gibt es nur, wenn das Journal die
/// Hoechstkosten vorher gebucht hat (R8). Der Berater verbraucht sie.
pub trait Berater {
    fn kann_abstimmen(&self) -> bool {
        false
    }
    fn abstimmen(
        &mut self,
        _erlaubnis: AufrufErlaubnis,
        _lage: &Lage<'_>,
        _stand: &crate::abstimmung::AbstimmungsStand,
    ) -> crate::abstimmung::AbstimmungsAntwort {
        crate::abstimmung::AbstimmungsAntwort::unverwertbar()
    }
    /// Einmalig abholen und vor einer weiteren Anfrage dauerhaft im Journal abrechnen.
    fn abrechnung(&mut self) -> Option<depot_journal::KiAbrechnung> {
        None
    }
    /// Ist der Berater erreichbar? Ein Berater ohne Anbindung wird nicht gefragt; fuer ihn
    /// wird keine Erlaubnis reserviert und nichts gebucht.
    fn angebunden(&self) -> bool {
        true
    }
    /// Warum der Berater nicht angebunden ist, in Worten fuer das Journal (etwa „kein
    /// Schlüssel“). `None`, wenn er angebunden ist oder es keinen genaueren Grund gibt.
    fn warum_nicht(&self) -> Option<String> {
        None
    }
    fn vorschlagen(&mut self, erlaubnis: AufrufErlaubnis, lage: &Lage<'_>) -> Antwort;
    fn pruefen(
        &mut self,
        erlaubnis: AufrufErlaubnis,
        lage: &Lage<'_>,
        vorschlag: &Vorschlag,
    ) -> Urteil;

    /// Vorschlag mit der getrennten Auswertung bisheriger Prognosen.
    fn vorschlagen_mit_bilanz(
        &mut self,
        erlaubnis: AufrufErlaubnis,
        lage: &Lage<'_>,
        _bilanz: &[(BeraterId, depot_journal::Auswertung)],
    ) -> Antwort {
        self.vorschlagen(erlaubnis, lage)
    }

    /// Zweitmeinung mit derselben Auswertung.
    fn pruefen_mit_bilanz(
        &mut self,
        erlaubnis: AufrufErlaubnis,
        lage: &Lage<'_>,
        vorschlag: &Vorschlag,
        _bilanz: &[(BeraterId, depot_journal::Auswertung)],
    ) -> Urteil {
        self.pruefen(erlaubnis, lage, vorschlag)
    }
}

/// Ein Berater ohne Anbindung: Er wird nie gefragt. Bis Dachs und Maus angebunden sind,
/// laeuft der Papiertag mit ihm; dann entstehen nur zwingende Auftraege des Regelwerks.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stumm;

impl Berater for Stumm {
    fn angebunden(&self) -> bool {
        false
    }

    fn vorschlagen(&mut self, _erlaubnis: AufrufErlaubnis, _lage: &Lage<'_>) -> Antwort {
        Antwort {
            prognose: None,
            verwertbar: false,
            einschaetzung: "nicht angebunden".into(),
            vorschlag: None,
        }
    }

    fn pruefen(
        &mut self,
        _erlaubnis: AufrufErlaubnis,
        _lage: &Lage<'_>,
        _vorschlag: &Vorschlag,
    ) -> Urteil {
        Urteil {
            verwertbar: false,
            zustimmung: false,
            begruendung: "nicht angebunden".into(),
        }
    }
}

/// Ein Berater nach Drehbuch: Er antwortet je Tag, wie es vorher aufgeschrieben ist. Ohne
/// Eintrag schlaegt er nichts vor und stimmt nicht zu; Schweigen ist nie Zustimmung (R31).
#[derive(Clone, Debug, Default)]
pub struct Drehbuch {
    pub antworten: BTreeMap<Datum, Antwort>,
    pub urteile: BTreeMap<Datum, Urteil>,
    /// Wie oft der Berater gefragt wurde.
    pub aufrufe: u32,
}

impl Berater for Drehbuch {
    fn vorschlagen(&mut self, _erlaubnis: AufrufErlaubnis, lage: &Lage<'_>) -> Antwort {
        self.aufrufe = self.aufrufe.saturating_add(1);
        self.antworten
            .get(&lage.datum)
            .cloned()
            .unwrap_or_else(|| Antwort {
                prognose: None,
                verwertbar: true,
                einschaetzung: "nichts Neues, keine Änderung".into(),
                vorschlag: None,
            })
    }

    fn pruefen(
        &mut self,
        _erlaubnis: AufrufErlaubnis,
        lage: &Lage<'_>,
        _vorschlag: &Vorschlag,
    ) -> Urteil {
        self.aufrufe = self.aufrufe.saturating_add(1);
        self.urteile
            .get(&lage.datum)
            .cloned()
            .unwrap_or_else(|| Urteil {
                verwertbar: false,
                zustimmung: false,
                begruendung: "keine Antwort".into(),
            })
    }
}

/// Eine erfundene ISIN mit gueltiger Pruefziffer.
fn isin(stamm: &str) -> Option<Isin> {
    (0..=9).find_map(|d| Isin::neu(&format!("{stamm}{d}")).ok())
}

fn tag(t: u8) -> Option<Datum> {
    if t > 30 {
        Datum::neu(2026, 12, t.checked_sub(30)?)
    } else {
        Datum::neu(2026, 11, t)
    }
}

fn kurse(datum: Datum, preise: &[(&Isin, i64)]) -> Option<Kurse> {
    let zeit = Zeitpunkt::neu(datum, 17, 30, 0)?;
    let eintraege = preise
        .iter()
        .map(|(i, p)| {
            Kurs::neu(Cent(*p), "EUR", datum, zeit, Herkunft::Simulation)
                .ok()
                .map(|k| ((*i).clone(), k))
        })
        .collect::<Option<Vec<_>>>()?;
    Kurse::neu(zeit, eintraege).ok()
}

fn vorschlag(berater: BeraterId, ziele: &[(&Isin, i64, &str)]) -> Vorschlag {
    Vorschlag {
        berater,
        zielgewichte_bp: ziele.iter().map(|(i, bp, _)| ((*i).clone(), *bp)).collect(),
        begruendung: ziele
            .iter()
            .map(|(i, _, t)| ((*i).clone(), (*t).to_string()))
            .collect(),
    }
}

fn antwort(einschaetzung: &str, v: Option<Vorschlag>) -> Antwort {
    Antwort {
        prognose: None,
        verwertbar: true,
        einschaetzung: einschaetzung.into(),
        vorschlag: v,
    }
}

fn urteil(zustimmung: bool, begruendung: &str) -> Urteil {
    Urteil {
        verwertbar: true,
        zustimmung,
        begruendung: begruendung.into(),
    }
}

/// Die drei erfundenen Werte des Beispiels.
struct Werte {
    alpha: Isin,
    beta: Isin,
    gamma: Isin,
}

fn tage(w: &Werte) -> Option<Vec<Tag>> {
    let (alpha, beta, gamma) = (&w.alpha, &w.beta, &w.gamma);
    let (t25, t26, t27, t30, t31) = (tag(25)?, tag(26)?, tag(27)?, tag(30)?, tag(31)?);
    Some(vec![
        Tag {
            datum: t25,
            kurse: kurse(t25, &[(alpha, 5_010), (beta, 7_980), (gamma, 2_005)])?,
            ereignisse: vec![
                "Alpha Werke kuendigt Quartalszahlen fuer den 26.11. an".into(),
                "Notenbank kuendigt Zinsentscheid fuer den 27.11. an".into(),
            ],
        },
        Tag {
            datum: t26,
            kurse: kurse(t26, &[(alpha, 5_400), (beta, 7_950), (gamma, 2_010)])?,
            ereignisse: vec!["Alpha Werke: Umsatz ueber den Erwartungen, Kurs +7,8 %".into()],
        },
        Tag {
            datum: t27,
            kurse: kurse(t27, &[(alpha, 5_450), (beta, 8_050), (gamma, 2_000)])?,
            ereignisse: vec!["Notenbank laesst den Leitzins unveraendert".into()],
        },
        Tag {
            datum: t30,
            kurse: kurse(t30, &[(alpha, 5_500), (beta, 8_100), (gamma, 2_900)])?,
            ereignisse: vec!["Datenlieferant meldet Stoerung".into()],
        },
        Tag {
            datum: t31,
            kurse: kurse(t31, &[(alpha, 5_480), (beta, 8_120), (gamma, 2_020)])?,
            ereignisse: vec!["Gamma Netz kuendigt Hauptversammlung fuer den 10.12. an".into()],
        },
    ])
}

/// Dachs: steigt vor den Terminen breit und klein ein, will nach den Zahlen Alpha (mehr, als
/// das Regelwerk erlaubt) und haelt Maus' Alpha-Kauf fuer verfrueht.
fn dachs(w: &Werte) -> Option<Drehbuch> {
    let (alpha, beta, gamma) = (&w.alpha, &w.beta, &w.gamma);
    let (t25, t26, t31) = (tag(25)?, tag(26)?, tag(31)?);
    Some(Drehbuch {
        antworten: [
            (
                t25,
                antwort(
                    "Zwei Termine stehen an; vor dem Ausgang breit und klein einsteigen",
                    Some(vorschlag(
                        BeraterId::Dachs,
                        &[
                            (beta, 2_000, "ruhiger Wert, kein Termin betroffen"),
                            (gamma, 1_500, "ruhiger Wert, kein Termin betroffen"),
                        ],
                    )),
                ),
            ),
            (
                t26,
                antwort(
                    "Alpha hat nach den Zahlen deutlich zugelegt; die Reaktion ist beobachtet",
                    Some(vorschlag(
                        BeraterId::Dachs,
                        &[
                            (alpha, 5_000, "bestätigte Reaktion auf die Zahlen"),
                            (beta, 2_000, "unveraendert"),
                            (gamma, 1_500, "unveraendert"),
                        ],
                    )),
                ),
            ),
            (
                t31,
                antwort(
                    "Neuer Monat; Gamma nach der Stoerung wieder plausibel, nichts zu tun",
                    None,
                ),
            ),
        ]
        .into_iter()
        .collect(),
        urteile: [
            (t25, urteil(true, "Streuung und kleine Schritte passen")),
            (
                t26,
                urteil(
                    false,
                    "Reaktion auf die Zahlen erst einen Tag alt; erst beobachten",
                ),
            ),
        ]
        .into_iter()
        .collect(),
        aufrufe: 0,
    })
}

/// Maus: faengt klein an und nimmt nach den Zahlen etwas Alpha dazu.
fn maus(w: &Werte) -> Option<Drehbuch> {
    let (alpha, beta) = (&w.alpha, &w.beta);
    let (t25, t26) = (tag(25)?, tag(26)?);
    Some(Drehbuch {
        antworten: [
            (
                t25,
                antwort(
                    "Vor Zinsentscheid und Zahlen nur ein kleiner Anfang",
                    Some(vorschlag(
                        BeraterId::Maus,
                        &[(beta, 1_000, "kein Termin betroffen")],
                    )),
                ),
            ),
            (
                t26,
                antwort(
                    "Ich bleibe bei Beta und warte den Zinsentscheid ab",
                    Some(vorschlag(
                        BeraterId::Maus,
                        &[
                            (alpha, 1_000, "Reaktion auf die Zahlen beobachtet"),
                            (beta, 1_000, "unveraendert"),
                        ],
                    )),
                ),
            ),
        ]
        .into_iter()
        .collect(),
        urteile: [
            (
                t25,
                urteil(true, "breit gestreut, keine Wette auf einen Termin"),
            ),
            (
                t26,
                urteil(true, "Alpha ist beobachtet, nicht vorhergesagt"),
            ),
        ]
        .into_iter()
        .collect(),
        aufrufe: 0,
    })
}

impl Szenario {
    /// Ein erfundener Zeitraum vom 25.11. bis 01.12.2026 mit drei erfundenen Werten und zwei
    /// Toepfen zu je 10.000 Euro Papiergeld. Er zeigt: angekuendigte Termine, Beobachtung nach
    /// dem Ereignis, eine Kuerzung durch das Regelwerk, ein Veto, unplausible Kurse und den
    /// neuen Monat. Jeder Aufruf ist gebucht; den vollen Monat beim Kostenschutz zeigt der
    /// Papiertag (`tests/papiertag.rs`), denn 46 Aufrufe im Monat fuellt dieser Zeitraum nicht. Dazu die Drehbuecher von Dachs und
    /// Maus. `None` nur, wenn die erfundenen Daten selbst ungueltig waeren.
    pub fn beispiel() -> Option<(Szenario, Drehbuch, Drehbuch)> {
        let w = Werte {
            alpha: isin("XA000ALPHA0")?,
            beta: isin("XA0000BETA0")?,
            gamma: isin("XA000GAMMA0")?,
        };
        let universum = Universum::neu(
            [
                ("Alpha Werke", &w.alpha),
                ("Beta Bau", &w.beta),
                ("Gamma Netz", &w.gamma),
            ]
            .into_iter()
            .map(|(name, i)| Instrument {
                isin: i.clone(),
                name: name.into(),
                art: InstrumentArt::Aktie,
            })
            .collect(),
        );
        let start = vec![
            Topf::neu(TopfId::A, Cent(1_000_000)).ok()?,
            Topf::neu(TopfId::B, Cent(1_000_000)).ok()?,
        ];
        let vorlauf = kurse(
            tag(24)?,
            &[(&w.alpha, 5_000), (&w.beta, 8_000), (&w.gamma, 2_000)],
        )?;
        Some((
            Szenario {
                universum,
                start,
                vorlauf,
                tage: tage(&w)?,
            },
            dachs(&w)?,
            maus(&w)?,
        ))
    }
}
