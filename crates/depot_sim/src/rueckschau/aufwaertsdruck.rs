//! Bewertet das Regelsignal für die Erholung einer Gruppe bei noch schwachem Markt.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use depot_journal::Stand;
use depot_kern::geld::BP_GANZ;
use depot_kern::liste::{boerse_von, papierorder_schluss};
use depot_kern::{Boerse, Cent, Datum, Grenzen, Isin, Preis, Rundung, Zeitpunkt};

use crate::aufwaertsdruck::Kursindex;
use crate::aufwaertsdruck_beleg::{Befund, beobachtungen};

use super::ausstiegsvergleich::{
    Historie, handelstage_ab, kaufbetrag, korbnetto, stuecke, verkauf,
};

pub(super) const BUDGET: Cent = Cent(300_000);
pub(super) const HALTETAGE: usize = 5;

struct Kaufidee<'a> {
    isin: &'a Isin,
    referenz: Preis,
    bereit: Zeitpunkt,
    bezugstag: Datum,
}

struct Fenster {
    boerse: Boerse,
    erster_schluss: Zeitpunkt,
    tage: Vec<Datum>,
}

impl Fenster {
    fn neu(kauf: &Kaufidee<'_>) -> Option<Self> {
        let boerse = boerse_von(kauf.isin)?;
        let erster_schluss = papierorder_schluss(kauf.isin, kauf.bereit)?;
        let tag = erster_schluss.frankfurt_tag()?;
        let tage: Vec<_> = std::iter::once(tag)
            .chain(handelstage_ab(boerse, tag).take(HALTETAGE))
            .collect();
        (tage.len() > HALTETAGE).then_some(Self {
            boerse,
            erster_schluss,
            tage,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Ergebnis {
    Groessenhuerde,
    EinstiegOffen(Datum),
    LimitVerfallen {
        tag: Datum,
        kurs: Cent,
        limit: Cent,
    },
    Haelt {
        bis: Datum,
    },
    Luecke(Option<Datum>),
    Abgeschlossen {
        bis: Datum,
        netto: Cent,
        korb: Option<Cent>,
    },
}

struct Auswertung<'a> {
    historie: &'a Historie,
    korb: &'a [Isin],
    jetzt: Zeitpunkt,
    grenzen: Grenzen,
}

impl Auswertung<'_> {
    fn signal(&self, kauf: &Kaufidee<'_>) -> Option<Ergebnis> {
        let fenster = Fenster::neu(kauf)?;
        let (&einstieg, &ende) = fenster.tage.first().zip(fenster.tage.last())?;
        let limit = kauf.referenz.mal_bp(
            BP_GANZ.checked_add(self.grenzen.limit_abstand_bp())?,
            Rundung::Ab,
        )?;
        let stueck = stuecke(BUDGET, limit, &self.grenzen)?;
        if stueck == 0 {
            return Some(Ergebnis::Groessenhuerde);
        }
        if fenster.erster_schluss > self.jetzt {
            return Some(Ergebnis::EinstiegOffen(einstieg));
        }
        let Some(reihe) = self
            .historie
            .fenster(kauf.isin, &[kauf.bezugstag, einstieg])
        else {
            return Some(Ergebnis::Luecke(Some(einstieg)));
        };
        let fuellung =
            self.grenzen
                .papierkurs(fenster.boerse, reihe.last()?.preis, Rundung::Auf)?;
        if fuellung > limit {
            return Some(Ergebnis::LimitVerfallen {
                tag: einstieg,
                kurs: fuellung,
                limit,
            });
        }
        if fenster.boerse.schluss(ende)? > self.jetzt {
            return Some(Ergebnis::Haelt { bis: ende });
        }
        let Some(kurse) = self.historie.fenster(kauf.isin, &fenster.tage) else {
            return Some(Ergebnis::Luecke(Some(ende)));
        };
        let bezahlt = kaufbetrag(fuellung, stueck, &self.grenzen)?;
        let netto = verkauf(fenster.boerse, kurse.last()?.preis, stueck, &self.grenzen)?
            .checked_sub(bezahlt)?;
        Some(Ergebnis::Abgeschlossen {
            bis: ende,
            netto,
            korb: korbnetto(
                self.historie,
                self.korb,
                fenster.boerse,
                &fenster.tage,
                BUDGET,
                &self.grenzen,
            ),
        })
    }

    fn historie(&self, index: &Kursindex<'_>, anfang: Datum, boerse: Boerse) -> Historisch {
        let mut statistik = Historisch::default();
        let Some(bis) = self.jetzt.frankfurt_tag() else {
            return statistik;
        };
        for tag in handelstage_ab(boerse, anfang).take_while(|d| *d <= bis) {
            let Some((bereit, bezugstag)) = Zeitpunkt::frankfurt(tag, 8, 30, 0)
                .filter(|bereit| *bereit <= self.jetzt)
                .zip(boerse.bezugstag(tag))
            else {
                continue;
            };
            statistik.termine = statistik.termine.saturating_add(1);
            let Some(befund) = index.signale(boerse, bezugstag, bereit) else {
                statistik.marktluecken = statistik.marktluecken.saturating_add(1);
                if index.signale(boerse, bezugstag, self.jetzt).is_some() {
                    statistik.spaeter_auswertbar = statistik.spaeter_auswertbar.saturating_add(1);
                }
                continue;
            };
            statistik.gruppen_bewertet = statistik
                .gruppen_bewertet
                .saturating_add(befund.gruppen_bewertet);
            statistik.gruppen_fehlend = statistik
                .gruppen_fehlend
                .saturating_add(befund.gruppen_fehlend);
            if befund.signale.is_empty() && befund.gruppen_bewertet != 0 {
                statistik.ohne_signal = statistik.ohne_signal.saturating_add(1);
            }
            for signal in befund.signale {
                let idee = Kaufidee {
                    isin: &signal.isin,
                    referenz: signal.referenz,
                    bereit,
                    bezugstag,
                };
                statistik
                    .bilanz
                    .buche(&self.signal(&idee).unwrap_or(Ergebnis::Luecke(None)));
            }
        }
        statistik
    }
}

#[derive(Default)]
struct Bilanz {
    signale: usize,
    groessenhuerde: usize,
    verfallen: usize,
    offen: usize,
    luecken: usize,
    abgeschlossen: usize,
    gepaart: usize,
    netto: i128,
    abstand: i128,
    korb: i128,
}

impl Bilanz {
    fn buche(&mut self, ergebnis: &Ergebnis) {
        self.signale = self.signale.saturating_add(1);
        match *ergebnis {
            Ergebnis::Groessenhuerde => self.groessenhuerde = self.groessenhuerde.saturating_add(1),
            Ergebnis::LimitVerfallen { .. } => self.verfallen = self.verfallen.saturating_add(1),
            Ergebnis::EinstiegOffen(_) | Ergebnis::Haelt { .. } => {
                self.offen = self.offen.saturating_add(1);
            }
            Ergebnis::Luecke(_) => self.luecken = self.luecken.saturating_add(1),
            Ergebnis::Abgeschlossen { netto, korb, .. } => {
                self.abgeschlossen = self.abgeschlossen.saturating_add(1);
                self.netto = self.netto.saturating_add(i128::from(netto.0));
                if let Some(korb) = korb {
                    self.gepaart = self.gepaart.saturating_add(1);
                    self.korb = self.korb.saturating_add(i128::from(korb.0));
                    self.abstand = self
                        .abstand
                        .saturating_add(i128::from(netto.0).saturating_sub(i128::from(korb.0)));
                }
            }
        }
    }

    fn zeile(&self, text: &mut String, stichprobe: &str) {
        let mittel = |summe: i128, anzahl: usize| {
            i128::try_from(anzahl)
                .ok()
                .and_then(|anzahl| summe.checked_div(anzahl))
                .and_then(|betrag| i64::try_from(betrag).ok())
                .map_or_else(|| "–".into(), |cent| Cent(cent).to_string())
        };
        let _ = writeln!(
            text,
            "| {stichprobe} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            self.signale,
            self.abgeschlossen,
            self.verfallen,
            self.groessenhuerde,
            self.offen,
            self.luecken,
            self.gepaart,
            mittel(self.netto, self.abgeschlossen),
            mittel(self.korb, self.gepaart),
            mittel(self.abstand, self.gepaart),
        );
    }
}

#[derive(Default)]
struct Historisch {
    termine: usize,
    marktluecken: usize,
    spaeter_auswertbar: usize,
    gruppen_bewertet: usize,
    gruppen_fehlend: usize,
    ohne_signal: usize,
    bilanz: Bilanz,
}

fn status(ergebnis: &Ergebnis) -> String {
    match ergebnis {
        Ergebnis::Groessenhuerde => "Größenhürde".into(),
        Ergebnis::EinstiegOffen(tag) => format!("Einstieg offen am {tag}"),
        Ergebnis::LimitVerfallen { tag, kurs, limit } => {
            format!("Limit {limit} am {tag} verfallen bei {kurs}")
        }
        Ergebnis::Haelt { bis } => format!("Haltefenster offen bis {bis}"),
        Ergebnis::Luecke(Some(tag)) => format!("Kurslücke oder Bruch im Fenster bis {tag}"),
        Ergebnis::Luecke(None) => "Kursgrundlage fehlt".into(),
        Ergebnis::Abgeschlossen { bis, netto, korb } => {
            let korb = korb.map_or_else(|| "Korb offen".into(), |wert| format!("Korb {wert}"));
            format!("Abgeschlossen am {bis} · Netto {netto} · {korb}")
        }
    }
}

struct Live {
    tage: BTreeSet<(Datum, Boerse)>,
    luecken: usize,
    ohne_signal: usize,
    gruppen_bewertet_ohne_signal: usize,
    gruppen_fehlend_ohne_signal: usize,
    bilanz: Bilanz,
    zeilen: String,
}

fn live(stand: &Stand, auswertung: &Auswertung<'_>) -> Live {
    let mut messung = Live {
        tage: BTreeSet::new(),
        luecken: 0,
        ohne_signal: 0,
        gruppen_bewertet_ohne_signal: 0,
        gruppen_fehlend_ohne_signal: 0,
        bilanz: Bilanz::default(),
        zeilen: String::new(),
    };
    let mut beobachtungen = beobachtungen(stand);
    beobachtungen.sort_by_key(|b| b.bereit);
    let mut gesehen = BTreeSet::new();
    for b in beobachtungen
        .into_iter()
        .filter(|b| b.bereit <= auswertung.jetzt)
    {
        messung.tage.insert((b.tag, b.boerse));
        match b.befund {
            Befund::Luecke => messung.luecken = messung.luecken.saturating_add(1),
            Befund::KeinSignal { bewertet, fehlend } => {
                messung.ohne_signal = messung.ohne_signal.saturating_add(1);
                messung.gruppen_bewertet_ohne_signal = messung
                    .gruppen_bewertet_ohne_signal
                    .saturating_add(bewertet);
                messung.gruppen_fehlend_ohne_signal =
                    messung.gruppen_fehlend_ohne_signal.saturating_add(fehlend);
            }
            Befund::Signal(idee) => {
                if !gesehen.insert((b.boerse, b.bezugstag, idee.gruppe.clone())) {
                    continue;
                }
                let kauf = Kaufidee {
                    isin: &idee.isin,
                    referenz: idee.referenz,
                    bereit: b.bereit,
                    bezugstag: b.bezugstag,
                };
                let ergebnis = auswertung.signal(&kauf).unwrap_or(Ergebnis::Luecke(None));
                messung.bilanz.buche(&ergebnis);
                let _ = writeln!(
                    messung.zeilen,
                    "| {} | {} | {} | {} | {} am {}, abgerufen {} | {} | {} | {} / {} | {} / {} | {} / {} / {} | {} |",
                    b.tag,
                    b.boerse,
                    idee.gruppe,
                    idee.isin,
                    idee.referenz.euro_text(),
                    b.bezugstag,
                    idee.referenz_abruf,
                    b.wissen,
                    b.bereit,
                    idee.markt_rueckgang_bp,
                    idee.markt_erholung_bp,
                    idee.gruppe_rueckgang_bp,
                    idee.gruppe_erholung_bp,
                    idee.breite_rueckgang,
                    idee.breite_erholung,
                    idee.titel,
                    status(&ergebnis)
                );
            }
        }
    }
    messung
}

pub(super) fn bericht(
    text: &mut String,
    stand: &Stand,
    historie: &Historie,
    jetzt: Zeitpunkt,
    korb: &[Isin],
) {
    let auswertung = Auswertung {
        historie,
        korb,
        jetzt,
        grenzen: Grenzen::standard(),
    };
    let live = live(stand, &auswertung);
    let index = Kursindex::historie(stand, jetzt);
    let historisch = Boerse::ALLE.map(|boerse| {
        let zahlen = index
            .erster_kurstag(boerse)
            .map_or_else(Historisch::default, |anfang| {
                auswertung.historie(&index, anfang, boerse)
            });
        (boerse, zahlen)
    });
    text.push_str(concat!(
        "\n## Aufwärtsdruck im schwachen Markt\n\n",
        "Das Signal prüft drei aufeinanderfolgende Handelsschlüsse derselben Börse. Der gleich gewichtete Vergleichskorb fällt zuerst um mindestens 0,5 % und steigt am nächsten Handelstag. Die feste Wertgruppe schlägt den Korb an beiden Tagen. Eine strikte Mehrheit ihrer Titel schlägt ihn ebenfalls je Tag. Unter den Titeln, die den Korb an beiden Tagen schlagen, wählt die Regel den mit der höchsten Summe der Tagesrenditen. Die Gruppen sind Versicherer, Autohersteller, Software und Halbleiter auf Xetra sowie Technologie- und Finanzwerte an den US-Börsen.\n\n",
        "Lesende Modellauswertung mit 3.000 € je Signal. Der Kauf kann zum ersten geplanten Börsenschluss nach Signalbereitschaft erfolgen. Das Standardlimit beruht auf dem davor belegten Referenzkurs. Die Rechnung nutzt ganze Stücke, Mindestorder, Papiergebühren und Modellspread. Ein über dem Limit liegender Kaufpreis lässt das Limit verfallen. Der Ausstieg erfolgt am fünften Handelstag desselben Börsenplatzes nach dem Kauf.\n\n",
        "Das Ergebnis vergleicht abgeschlossene Füllungen netto mit unverzinstem Bargeld und einem gleich gewichteten Korb desselben Börsenplatzes. Der Korb erhält je Signal dasselbe Budget und dieselben Handelstage. Der Abstand zum Korb nutzt nur vollständige Paare. Die Beobachtungen können überlappen.\n\n",
        "Beobachtet sind die bereits vor dem möglichen Kauf im Journal festgehaltenen Signale. Je Börse, Bezugstag und Gruppe zählt das zuerst bereite Signal. Die historische Gegenprobe startet an jedem Börsentag fest um 08:30 Uhr Frankfurt und verwendet nur bis dahin belegte Kurse. Nach diesem Zeitpunkt eingegangene Archivkurse können rückwirkend keinen Einstieg auslösen. Die historische Gegenprobe ist ein Nachspiel der heutigen Regel.\n\n",
        "| Stichprobe | Signale | Abgeschlossen | Limit verfallen | Größenhürde | Offen | Kurslücke | Vergleichspaare | Ø Netto / gegen Bar | Ø Korb im Paar | Ø Abstand zum Korb |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n"
    ));
    live.bilanz.zeile(text, "Beobachtet im Papiertag");
    for (boerse, statistik) in &historisch {
        statistik
            .bilanz
            .zeile(text, &format!("Historisch · {boerse}"));
    }
    let _ = writeln!(
        text,
        "\nJournalabdeckung: {} Börsentag-Platz-Paare, {} mit Marktlücke, {} ohne Signal. An Terminen ohne Signal sind {} Gruppen bewertet, für {} Gruppen fehlen Belege.",
        live.tage.len(),
        live.luecken,
        live.ohne_signal,
        live.gruppen_bewertet_ohne_signal,
        live.gruppen_fehlend_ohne_signal
    );
    text.push_str("\n| Historische Abdeckung | Termine | Marktlücke um 08:30 | Davon zum Export auswertbar | Bewertete Gruppen | Gruppenlücke | Ohne Signal |\n|---|---:|---:|---:|---:|---:|---:|\n");
    for (boerse, statistik) in historisch {
        let _ = writeln!(
            text,
            "| {boerse} | {} | {} | {} | {} | {} | {} |",
            statistik.termine,
            statistik.marktluecken,
            statistik.spaeter_auswertbar,
            statistik.gruppen_bewertet,
            statistik.gruppen_fehlend,
            statistik.ohne_signal
        );
    }
    if !live.zeilen.is_empty() {
        text.push_str("\n| Papiertag | Börse | Gruppe | Wert | Referenzkurs (€) | Wissen (UTC) | Bereit (UTC) | Markt Rückgang / Erholung (bp) | Gruppe Rückgang / Erholung (bp) | Titel stärker im Rückgang / in der Erholung / in der Gruppe | Stand |\n|---|---|---|---|---|---|---|---:|---:|---:|---|\n");
        text.push_str(&live.zeilen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nach_xetra_schluss_nur_naechsten_geplanten_schluss_pruefen() -> Result<(), String> {
        let bezugstag = Datum::neu(2026, 11, 24).ok_or("Bezugstag")?;
        let bereit = Boerse::Xetra.schluss(bezugstag).ok_or("Zeit")?;
        let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let kauf = Kaufidee {
            isin: &isin,
            referenz: Preis::aus_cent(Cent(10_000)).ok_or("Kurs")?,
            bereit,
            bezugstag,
        };
        let folgtag = bezugstag.plus_tage(1).ok_or("Folgetag")?;
        let fenster = Fenster::neu(&kauf).ok_or("Fenster")?;
        assert_eq!(
            fenster.erster_schluss,
            Boerse::Xetra.schluss(folgtag).ok_or("Schluss")?
        );
        let historie = Historie::default();
        let mut auswertung = Auswertung {
            historie: &historie,
            korb: &[],
            jetzt: bereit,
            grenzen: Grenzen::standard(),
        };
        assert_eq!(
            auswertung.signal(&kauf),
            Some(Ergebnis::EinstiegOffen(folgtag))
        );
        auswertung.jetzt = Zeitpunkt::neu(folgtag, 23, 0, 0).ok_or("Zeit")?;
        assert_eq!(
            auswertung.signal(&kauf),
            Some(Ergebnis::Luecke(Some(folgtag)))
        );
        Ok(())
    }

    #[test]
    fn fuenf_folgesitzungen_beachten_den_nyse_feiertag() -> Result<(), String> {
        let bezugstag = Datum::neu(2026, 11, 23).ok_or("Bezugstag")?;
        let kaufdatum = bezugstag.plus_tage(1).ok_or("Kauftag")?;
        let bereit = Zeitpunkt::frankfurt(kaufdatum, 8, 30, 0).ok_or("Zeit")?;
        let isin = Isin::neu("US0378331005").map_err(|e| format!("{e:?}"))?;
        let kauf = Kaufidee {
            isin: &isin,
            referenz: Preis::aus_cent(Cent(10_000)).ok_or("Kurs")?,
            bereit,
            bezugstag,
        };
        let fenster = Fenster::neu(&kauf).ok_or("Fenster")?;
        assert_eq!(fenster.tage.first().copied(), Some(kaufdatum));
        assert_eq!(fenster.tage.last().copied(), Datum::neu(2026, 12, 2));
        Ok(())
    }
}
