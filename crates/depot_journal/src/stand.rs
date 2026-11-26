//! Was im Journal steht und was daraus folgt: Eintragsarten, ihr Textformat und der Stand, der
//! beim Oeffnen aus allen Eintraegen nachgerechnet wird.
//!
//! **Ein Zustandsraum (R20):** Jeder Eintrag wird gelesen und wieder geschrieben; nur wenn
//! dabei genau derselbe Text entsteht, gilt er. Anhaengen und Oeffnen gehen durch dieselbe
//! Pruefung. Was das Journal beim Anhaengen ablehnt, fuehrt in der Datei zu Halt.

use std::collections::{BTreeMap, BTreeSet};

use depot_kern::{Cent, Datum, Grenzen, Herkunft, Isin, Rundung, Topf, TopfId, Zeitpunkt};
use depot_regelwerk::{Anlass, GenehmigterAuftrag, Richtung};

use crate::auftrag::{AuftragsLage, Meldung, lies_meldung, meldung_text};

use crate::kapitalmassnahme::{Bewertungsschritt, GebuchteKapitalmassnahme, Kapitalmassnahme};
use crate::kasse::KiStand;
use crate::konto::{Abrechnungen, papier_valuta};
use crate::kursbeleg::{KURSGRUNDLAGE, KursBeleg, kursbeleg_text, lies_kursbeleg};
use crate::nachrichtbeleg::{NachrichtBeleg, lies_nachrichtbeleg, nachrichtbeleg_text};
use crate::terminbeleg::{TerminBeleg, lies_terminbeleg, terminbeleg_text};

use crate::weltprognose::PrognoseBeleg;

// Historischer Wortlaut fuer Journale aus #48 und frueher.
const ALTE_NOTBREMSE: &str = "Notbremse aktiv: nur offene Verkaeufe weiter abgleichen";

const ALTE_VERLUSTSPERRE: &str = "Notbremse: 30 % Gesamtverlust erreicht. Neue Kaeufe gesperrt; Freigabe des Eigentuemers noetig";

/// Gespeicherter Zustand der Papier-Notbremse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PapierHalt {
    Notbremse,
    Gesamtverlust,
}

impl PapierHalt {
    pub(crate) fn kennung(self) -> &'static str {
        match self {
            Self::Notbremse => "notbremse",
            Self::Gesamtverlust => "gesamtverlust",
        }
    }

    pub fn meldung(self) -> String {
        match self {
            Self::Notbremse => "Notbremse aktiv: Schutzverkäufe laufen weiter".into(),
            Self::Gesamtverlust => {
                let bp = Grenzen::standard().papier_gesamtverlust_bp();
                let ganz = bp.div_euclid(100);
                let rest = bp.rem_euclid(100);
                let prozent = if rest == 0 {
                    ganz.to_string()
                } else {
                    format!("{ganz},{rest:02}")
                };
                format!(
                    "Notbremse: {prozent} % Gesamtverlust erreicht. Neue Käufe gesperrt; Freigabe des Eigentümers nötig"
                )
            }
        }
    }
}

/// Die Arten von Eintraegen. Ein Eintrag steht im Journal als `art;inhalt`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    /// `datum;text`: was in der Welt passiert oder angekuendigt ist (R40).
    Beobachtung,
    /// `topf;bargeld;isin:stueck,…`: der ganze Stand eines Topfes im Papierdepot.
    Topf,
    TopfUmbuchung,
    /// `quelle;herkunft;abruf;roh;isin;preis;zeitpunkt`: ein Kurs von aussen, belegt bevor er
    /// verwendet wird (`kursbeleg`). Nur im Format 2; die Abrufe fallen nie zurueck.
    Kurs,
    /// `quelle;herkunft;abruf;roh;kennung;verlaesslichkeit;zeit;betroffen;titel`: ein Termin
    /// von aussen (`terminbeleg`, R39). Nur im Format 2; die Abrufe fallen nie zurueck.
    Termin,
    /// `quelle;abruf;roh;zeit;text`: eine Nachricht von aussen (`nachrichtbeleg`, R40). Nur im
    /// Format 2; die Abrufe fallen nie zurueck.
    Nachricht,
    /// `quelle;utc-sekunden`: Verwaltungsbeleg für RSS-Abrufpausen, kein Handel.
    RssAbruf,
    /// Fixierte, unabhaengige Prognose eines Beraters im Papiertag.
    Weltprognose,
    /// `datum;berater;isin;ereignis;zieltag;schluss;ergebnis`: Auflösung einer Prognose nach
    /// ihrem Horizont (`aufloesung`, #114). Folgt allein aus dem Stand; jede andere wird
    /// abgelehnt.
    Aufloesung,
    /// Wissensstand, Budget und Antworten einer Vorbereitung ohne Handel.
    Vorbereitung,
    /// `nr;topf;isin;richtung;stueck;limit;anlass`: ein genehmigter Auftrag, bevor er den
    /// Rechner verlaesst (R12, R24). Damit ist er reserviert (R37).
    Auftrag,
    /// `nr;meldung`: der naechste Zustand eines Auftrags (R37, Kap. 10). Eine Ausfuehrung
    /// steht als `nr;ausfuehrung;stueck;kurs;gebuehr` und bucht sie im Topf.
    Zustand,
    /// Ausführung eines Kaufs mit gleichzeitig belegtem Schutz-Stopp.
    Stopp,
    /// Bestätigter, wertneutraler Split einer Papierposition.
    Kapitalmassnahme,
    /// `berater;datum;zyklus;cent`: eine Buchung des KI-Kassenbuchs, geprueft nach den Regeln
    /// aus `depot_ki` (K13-02).
    Ki,
    KiAbrechnung,
    KiAufruf,
    NeuroErgebnis,
    /// Der Abdruck der KI-Grenzen, unter denen die folgenden Buchungen entstehen.
    KiGrenzen,
    /// `datum;wer;text`: warum etwas geschieht oder nicht geschieht. `wer` ist ein Berater
    /// oder eine Stelle des Rahmens; die Kaufentscheidung begruenden die Berater selbst.
    Begruendung,
    Lehre,
    /// `2`: Umstellung eines Journals im Format 1 auf Format 2 (K16-02). Nur einmal, nur ohne
    /// angefangenen Tag; danach gelten Auftragszustaende und Deckung.
    Format,
    /// `datum;hoechststand;abdruck`: Der Tag ist vollstaendig belegt. `abdruck` bindet ihn an
    /// seine ganze Vorgeschichte (64 Hex-Zeichen). Ein Wiederanlauf setzt nur nach einem
    /// Abschluss fort; die Daten steigen streng.
    Abschluss,
    /// Handelstag fuer die Abrechnung nachfolgender Ausfuehrungen.
    Abrechnungstag,
    /// Tagesabschluss, der zugleich die Papier-Notbremse zuruecksetzt.
    NotbremseFreigabe,
    /// `datum;grundkennung`: typisierter Halt des Papierdepots.
    PapierHalt,
    /// `abschlussdatum;bargeld;einrichtung;dialog;zeitpunkt-utc`: Freigabe nach der Gesamtverlustsperre.
    PapierFreigabe,
    /// Handelstag und UTC-Wissensstichtag, vor der Entscheidung belegt.
    Wissensstichtag,
}

impl Art {
    pub fn name(self) -> &'static str {
        match self {
            Art::Beobachtung => "beobachtung",
            Art::Topf => "topf",
            Art::TopfUmbuchung => "topf-umbuchung",
            Art::Kurs => "kurs",
            Art::Termin => "termin",
            Art::Nachricht => "nachricht",
            Art::RssAbruf => "rss-abruf",
            Art::Weltprognose => "weltprognose",
            Art::Aufloesung => "prognose-aufloesung",
            Art::Vorbereitung => "vorbereitung",
            Art::Auftrag => "auftrag",
            Art::Zustand => "zustand",
            Art::Stopp => "stopp",
            Art::Kapitalmassnahme => "kapitalmassnahme",
            Art::Ki => "ki",
            Art::KiAbrechnung => "ki-abrechnung",
            Art::KiAufruf => "ki-aufruf",
            Art::NeuroErgebnis => "neuro-ergebnis",
            Art::KiGrenzen => "ki-grenzen",
            Art::Begruendung => "begruendung",
            Art::Lehre => "lehre",
            Art::Abschluss => "abschluss",
            Art::Format => "format",
            Art::Abrechnungstag => "abrechnungstag",
            Art::NotbremseFreigabe => "notbremse-freigabe",
            Art::PapierHalt => "papier-halt",
            Art::PapierFreigabe => "papier-freigabe",
            Art::Wissensstichtag => "wissensstichtag",
        }
    }

    fn aus_name(name: &str) -> Option<Art> {
        [
            Art::Beobachtung,
            Art::Topf,
            Art::TopfUmbuchung,
            Art::Kurs,
            Art::Termin,
            Art::Nachricht,
            Art::RssAbruf,
            Art::Weltprognose,
            Art::Aufloesung,
            Art::Vorbereitung,
            Art::Auftrag,
            Art::Zustand,
            Art::Stopp,
            Art::Kapitalmassnahme,
            Art::Ki,
            Art::KiAbrechnung,
            Art::KiAufruf,
            Art::NeuroErgebnis,
            Art::KiGrenzen,
            Art::Begruendung,
            Art::Lehre,
            Art::Abschluss,
            Art::Abrechnungstag,
            Art::NotbremseFreigabe,
            Art::PapierHalt,
            Art::PapierFreigabe,
            Art::Wissensstichtag,
            Art::Format,
        ]
        .into_iter()
        .find(|a| a.name() == name)
    }
}

/// Ein belegter Auftrag. Aus dem Journal entsteht nie ein sendbarer `GenehmigterAuftrag` und
/// keine `AuftragsKennung`; die Nummer bleibt eine Zahl.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuftragsBeleg {
    pub nr: u64,
    pub topf: TopfId,
    pub isin: Isin,
    pub richtung: Richtung,
    pub stueck: u64,
    pub limit: Cent,
    pub anlass: Anlass,
    pub stopp_bp: Option<i64>,
    /// Tauschkauf (#311 H1b): rechnet den Erlös dieser Verkäufe desselben Schlusses ein und
    /// füllt nur nach ihnen.
    pub nach: Vec<u64>,
}

/// Ein Tagesabschluss.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Abschluss {
    pub datum: Datum,
    /// Hoechststand des Depotwerts bis zu diesem Tag.
    pub hoechststand: Cent,
    /// Abdruck der Vorgeschichte bis zu diesem Tag: SHA-256 in Kleinbuchstaben-Hex. Wer ihn
    /// bildet, legt der Aufrufer fest; das Journal prueft nur die Form.
    pub abdruck: String,
}

/// Die am Abschluss belegten Bestände und der bis dahin aufgebaute Kursbestand.
#[derive(Debug)]
pub struct Schlussbeleg<'a> {
    pub abschluss: &'a Abschluss,
    pub toepfe: &'a BTreeMap<TopfId, Topf>,
    pub kurse: &'a [KursBeleg],
}

/// Format 1: Journale bis PR #15. Ein Auftrag darin ist mit seinem Ergebnis im naechsten Topf
/// gebucht und hat keine Zustaende.
pub const FORMAT_ALT: u8 = 1;
/// Format 2: Auftraege mit Zustandsmaschine und Deckung (R37).
pub const FORMAT: u8 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Wissensstand {
    zeit: Zeitpunkt,
    begruendungen: usize,
    kurse: usize,
    termine: usize,
    nachrichten: usize,
}

/// Der Stand, wie er aus dem Journal folgt.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stand {
    topf_umbuchungen: BTreeMap<Datum, Cent>,
    format: u8,
    toepfe: BTreeMap<TopfId, Topf>,
    startgeld: BTreeMap<TopfId, Option<Cent>>,
    stopps: BTreeMap<(TopfId, Isin), Cent>,
    /// Mit noch nicht abgerechnetem Erlös gekaufte Werte und die Valuta, vor der ein
    /// freiwilliger Verkauf gesperrt ist (#311 H1a).
    valuta_sperren: BTreeMap<(TopfId, Isin), Datum>,
    auftraege: Vec<AuftragsLage>,
    ausfuehrungsfolge: Vec<(usize, usize)>,
    kapitalmassnahmen: Vec<GebuchteKapitalmassnahme>,
    beobachtungen: Vec<(Datum, String)>,
    rss_abrufe: BTreeMap<String, Zeitpunkt>,
    kurse: Vec<KursBeleg>,
    termine: Vec<TerminBeleg>,
    nachrichten: Vec<NachrichtBeleg>,
    weltprognosen: Vec<PrognoseBeleg>,
    aufloesungen: Vec<crate::Aufloesung>,
    vorbereitungen: Vec<crate::Vorbereitung>,
    letzte_kennung: u64,
    ki: KiStand,
    begruendungen: Vec<(Datum, String, String)>,
    lehren: Vec<(Datum, String, String)>,
    abschluss: Option<Abschluss>,
    historischer_hoechststand: Option<Cent>,
    papier_halt: Option<PapierHalt>,
    papier_verlustreferenz: Option<Cent>,
    papier_neustart: Option<Cent>,
    notbremse_freigabe: Option<Datum>,
    prueft_wiederanlauf: bool,
    seit_abschluss: usize,
    abrechnungstag: Option<Datum>,
    wissensstichtage: BTreeMap<Datum, Wissensstand>,
    abrechnungen: Abrechnungen,
    /// Je abgeschlossenem Tag die Toepfe zum Abschluss, aufsteigend (#118). Entsteht nur beim
    /// Lesen; das Journal schreibt dafuer nichts.
    tagesschluesse: Vec<(Datum, BTreeMap<TopfId, Topf>)>,
    schlussbelege: Vec<(Abschluss, usize)>,
}

pub(crate) fn papier_freigabe_text(tag: Datum, bargeld: Cent) -> String {
    format!("{tag};{};einrichtung;dialog", bargeld.0)
}

impl Stand {
    pub(crate) fn im_format(format: u8) -> Stand {
        Stand {
            format,
            ..Stand::default()
        }
    }

    /// Das Format, nach dem die Eintraege gelten: `FORMAT_ALT` oder `FORMAT`.
    pub fn format(&self) -> u8 {
        self.format
    }

    /// Letzter Abruf dieser Quelle, einschließlich exakt erkannter alter RSS-Vermerke.
    pub fn rss_abruf(&self, quelle: &str) -> Option<Zeitpunkt> {
        self.rss_abrufe.get(quelle).copied()
    }

    fn merke_rss_abruf(&mut self, a: crate::rss_abruf::RssAbruf) {
        self.rss_abrufe
            .entry(a.quelle)
            .and_modify(|t| *t = (*t).max(a.zeit))
            .or_insert(a.zeit);
    }

    /// Der zuletzt belegte Stand je Topf des Papierdepots.
    pub fn toepfe(&self) -> &BTreeMap<TopfId, Topf> {
        &self.toepfe
    }

    /// Bargeld bei der ersten Buchung des Topfs, sofern diese ohne Positionen war.
    pub fn startgeld_je_topf(&self, topf: TopfId) -> Option<Cent> {
        self.startgeld.get(&topf).copied().flatten()
    }

    /// Schutz-Stopp aus der letzten belegten Kaufausführung dieser Position.
    pub fn stoppkurs(&self, topf: TopfId, isin: &Isin) -> Option<Cent> {
        self.stopps.get(&(topf, isin.clone())).copied()
    }

    pub(crate) fn stoppmeldung(&self, nr: u64, m: &Meldung) -> Result<Option<String>, String> {
        let Meldung::Ausfuehrung(a) = m else {
            return Ok(None);
        };
        let Some(lage) = self.auftrag(nr) else {
            return Ok(None);
        };
        let b = &lage.beleg;
        let Some(bp) = b.stopp_bp else {
            return Ok(None);
        };
        let kurs = a
            .kurs
            .anteil_bp_auf(depot_kern::BP_GANZ.checked_sub(bp).ok_or("Stoppabstand")?)
            .filter(|kurs| kurs.0 > 0)
            .ok_or("Stoppkurs nicht berechenbar")?;
        Ok(Some(format!(
            "{};{};{};{}",
            topf_name(b.topf),
            b.isin,
            kurs.0,
            meldung_text(nr, m)
        )))
    }

    /// Die Werte, die irgendein Topf haelt. Ihre Kurse sind Pflicht (`waehle_tageswerte`).
    pub fn gehalten(&self) -> BTreeSet<Isin> {
        self.toepfe
            .values()
            .flat_map(|t| t.positionen().keys().cloned())
            .collect()
    }

    /// Alle Auftraege mit ihrem Zustand, in der Reihenfolge ihrer Kennungen.
    pub fn auftraege(&self) -> &[AuftragsLage] {
        &self.auftraege
    }

    /// Ausführungen in der Reihenfolge ihrer Buchung im Journal.
    pub fn ausfuehrungsfolge(&self) -> impl Iterator<Item = (&AuftragsBeleg, &crate::Ausfuehrung)> {
        self.ausfuehrungsfolge
            .iter()
            .filter_map(|(auftrag, ausfuehrung)| {
                let lage = self.auftraege.get(*auftrag)?;
                Some((&lage.beleg, lage.ausfuehrungen.get(*ausfuehrung)?))
            })
    }

    pub fn kapitalmassnahmen(&self) -> &[GebuchteKapitalmassnahme] {
        &self.kapitalmassnahmen
    }

    /// Ausführungen und Splits in der Reihenfolge ihrer Buchung.
    pub fn bewertungsfolge(&self) -> impl Iterator<Item = Bewertungsschritt<'_>> {
        let mut ausfuehrungen = self.ausfuehrungsfolge();
        let mut massnahmen = self.kapitalmassnahmen.iter().peekable();
        let mut ausgefuehrt = 0_usize;
        std::iter::from_fn(move || {
            if massnahmen
                .peek()
                .is_some_and(|m| m.ausfuehrungen_vorher == ausgefuehrt)
            {
                return massnahmen
                    .next()
                    .map(|m| Bewertungsschritt::Split(&m.massnahme));
            }
            let (auftrag, ausfuehrung) = ausfuehrungen.next()?;
            ausgefuehrt = ausgefuehrt.saturating_add(1);
            Some(Bewertungsschritt::Ausfuehrung(auftrag, ausfuehrung))
        })
    }

    pub(crate) fn pruefe_kapitalmassnahme(&self, m: &Kapitalmassnahme) -> Result<u64, String> {
        if self.format != FORMAT || self.seit_abschluss != 0 {
            return Err(
                "Split nur nach einem abgeschlossenen Papiertag im aktuellen Journalformat".into(),
            );
        }
        let xetra = Kapitalmassnahme::xetra(self, m.topf, &m.quelle).as_ref() == Some(m);
        if !xetra {
            let tiingo = self.nachrichten().iter().any(|beleg| {
                crate::kapitalmassnahme::tiingo_split(beleg).is_some()
                    && crate::splitverdacht::kennung(beleg) == m.quelle
                    && Kapitalmassnahme::tiingo(self, m.topf, beleg).as_ref() == Some(m)
            });
            if !tiingo {
                return Err("Split stimmt mit keinem geprüften Quellenbeleg überein".into());
            }
            if self.kapitalmassnahmen.iter().any(|b| {
                b.massnahme.topf == m.topf
                    && b.massnahme.isin == m.isin
                    && b.massnahme.wirksam > m.wirksam
            }) {
                return Err("Für diese Position ist bereits ein späterer Split gebucht".into());
            }
        }
        if !self.kurse().iter().any(|b| {
            b.isin() == &m.isin
                && b.kurs().handelstag() == m.wirksam
                && b.kurs().herkunft() == Herkunft::Quelle
        }) {
            return Err("Für den Splittag fehlt ein belegter Schlusskurs".into());
        }
        if self.kapitalmassnahmen.iter().any(|b| {
            b.massnahme.topf == m.topf
                && b.massnahme.isin == m.isin
                && b.massnahme.wirksam == m.wirksam
        }) {
            return Err("Split für diesen Topf und Kurstag ist bereits gebucht".into());
        }
        if self.offene_auftraege().any(|a| a.beleg.isin == m.isin) {
            return Err(
                "Vor der Umbuchung müssen offene Aufträge für den Wert beendet sein".into(),
            );
        }
        if self.ausfuehrungsfolge().any(|(a, f)| {
            a.topf == m.topf
                && a.isin == m.isin
                && f.kurszeit
                    .and_then(Zeitpunkt::frankfurt_tag)
                    .is_none_or(|tag| tag >= m.wirksam)
        }) {
            return Err("Für diese Position ist eine Ausführung am oder nach dem Splittag belegt oder ihre Zeit fehlt".into());
        }
        let vorher = self.toepfe.get(&m.topf).map_or(0, |t| t.stueck(&m.isin));
        m.faktor
            .stueck(vorher)
            .filter(|n| *n > 0)
            .ok_or_else(|| "Split ergibt keinen ganzzahligen, positiven Papierbestand".into())
    }

    fn belege_kapitalmassnahme(&mut self, text: &str) -> Result<(), String> {
        let m = Kapitalmassnahme::lies(text).ok_or("Kapitalmaßnahme unlesbar")?;
        kanonisch(&m.text(), text)?;
        let danach = self.pruefe_kapitalmassnahme(&m)?;
        let mut topf = self.toepfe.get(&m.topf).cloned().ok_or("Topf fehlt")?;
        let vorher_gesamt = self
            .toepfe
            .values()
            .try_fold(0_u64, |summe, t| summe.checked_add(t.stueck(&m.isin)))
            .ok_or("Papierbestand: Überlauf")?;
        let danach_gesamt = vorher_gesamt
            .checked_sub(topf.stueck(&m.isin))
            .and_then(|rest| rest.checked_add(danach))
            .ok_or("Papierbestand: Überlauf")?;
        let stoppschluessel = (m.topf, m.isin.clone());
        let stopp = self
            .stopps
            .get(&stoppschluessel)
            .map(|kurs| {
                m.faktor
                    .stopp(*kurs)
                    .ok_or("Schutz-Stopp nach dem Split unberechenbar")
            })
            .transpose()?;
        topf.setze_bestand(m.isin.clone(), danach);
        self.toepfe.insert(m.topf, topf);
        if let Some(stopp) = stopp {
            self.stopps.insert(stoppschluessel, stopp);
        }
        self.kapitalmassnahmen.push(GebuchteKapitalmassnahme {
            massnahme: m,
            vorher_gesamt,
            danach_gesamt,
            ausfuehrungen_vorher: self.ausfuehrungsfolge.len(),
        });
        Ok(())
    }

    /// Der Auftrag mit dieser Kennung.
    pub fn auftrag(&self, nr: u64) -> Option<&AuftragsLage> {
        self.auftraege.iter().find(|l| l.beleg.nr == nr)
    }

    /// Die Auftraege, die noch keinen Endzustand haben.
    pub fn offene_auftraege(&self) -> impl Iterator<Item = &AuftragsLage> {
        self.auftraege.iter().filter(|l| l.zustand.offen())
    }

    pub fn aufloesungen(&self) -> &[crate::Aufloesung] {
        &self.aufloesungen
    }

    /// Die Auflösungen, die jetzt fällig sind: Prognosen ohne Auflösung, deren Zieltag
    /// abgeschlossen ist. Jede folgt allein aus dem Stand. Es zählt der zuletzt belegte
    /// Schlusskurs aus einer Quelle genau am Zieltag.
    pub fn faellige_aufloesungen(&self) -> Vec<crate::Aufloesung> {
        let Some(abgeschlossen) = self.abschluss.as_ref().map(|a| a.datum) else {
            return Vec::new();
        };
        self.weltprognosen
            .iter()
            .filter(|p| !self.aufloesungen.iter().any(|a| a.zu(p)))
            .filter_map(|p| {
                let zieltag =
                    crate::aufloesung::zieltag(&p.prognose.isin, p.datum, p.prognose.handelstage)?;
                if zieltag > abgeschlossen {
                    return None;
                }
                let schluss = self
                    .kurse
                    .iter()
                    .rev()
                    .find(|k| {
                        k.isin() == &p.prognose.isin
                            && k.kurs().handelstag() == zieltag
                            && k.kurs().herkunft() == Herkunft::Quelle
                    })
                    .map(|k| k.kurs().preis());
                Some(crate::Aufloesung {
                    datum: p.datum,
                    berater: p.berater,
                    isin: p.prognose.isin.clone(),
                    ereignis_nr: p.prognose.ereignis_nr,
                    zieltag,
                    schluss,
                    ergebnis: crate::aufloesung::bewerte(p, schluss),
                })
            })
            .collect()
    }

    pub fn vorbereitungen(&self) -> &[crate::Vorbereitung] {
        &self.vorbereitungen
    }

    pub fn weltprognosen(&self) -> &[PrognoseBeleg] {
        &self.weltprognosen
    }

    pub fn beobachtungen(&self) -> &[(Datum, String)] {
        &self.beobachtungen
    }

    /// Alle belegten Kurse in der Reihenfolge des Journals.
    pub fn kurse(&self) -> &[KursBeleg] {
        &self.kurse
    }

    /// Alle belegten Termine in der Reihenfolge des Journals, auch alte Fassungen.
    pub fn termine(&self) -> &[TerminBeleg] {
        &self.termine
    }

    /// Je Kennung die juengste Fassung eines Termins.
    pub fn aktuelle_termine(&self) -> BTreeMap<&str, &TerminBeleg> {
        self.termine
            .iter()
            .map(|b| (b.termin().kennung(), b))
            .collect()
    }

    /// Die juengste Fassung des Termins mit dieser Kennung.
    pub fn termin(&self, kennung: &str) -> Option<&TerminBeleg> {
        self.termine
            .iter()
            .rev()
            .find(|b| b.termin().kennung() == kennung)
    }

    /// Alle belegten Nachrichten in der Reihenfolge des Journals.
    pub fn nachrichten(&self) -> &[NachrichtBeleg] {
        &self.nachrichten
    }

    /// Der letzte Abruf eines belegten Kurses, Termins oder einer Nachricht. Kein Abruf faellt
    /// dahinter zurueck.
    fn letzter_abruf(&self) -> Option<Zeitpunkt> {
        let k = self.kurse.last().map(KursBeleg::abruf);
        let t = self.termine.last().map(TerminBeleg::abruf);
        let n = self.nachrichten.last().map(NachrichtBeleg::abruf);
        k.max(t).max(n)
    }

    /// Das KI-Kassenbuch.
    pub fn ki(&self) -> &KiStand {
        &self.ki
    }

    /// Begruendungen: Datum, wer, Text.
    /// Vorzeichen positiv für Bargeldzugang bei Dachs.
    pub fn topf_umbuchungen(&self) -> &BTreeMap<Datum, Cent> {
        &self.topf_umbuchungen
    }

    pub fn lehren(&self) -> &[(Datum, String, String)] {
        &self.lehren
    }

    pub fn begruendungen(&self) -> &[(Datum, String, String)] {
        &self.begruendungen
    }

    /// Je abgeschlossenem Tag der Stand der Toepfe zum Abschluss, aufsteigend nach Datum. Ein
    /// angefangener Tag ohne Abschluss erscheint nicht.
    pub fn tagesschluesse(&self) -> &[(Datum, BTreeMap<TopfId, Topf>)] {
        &self.tagesschluesse
    }

    /// Der Abschluss und die bis zu seiner Journalzeile vorliegenden Kursbelege.
    pub fn schlussbeleg(&self, tag: Datum) -> Option<Schlussbeleg<'_>> {
        let index = self
            .tagesschluesse
            .binary_search_by_key(&tag, |(tag, _)| *tag)
            .ok()?;
        let (_, toepfe) = self.tagesschluesse.get(index)?;
        let (abschluss, kursanzahl) = self.schlussbelege.get(index)?;
        Some(Schlussbeleg {
            abschluss,
            toepfe,
            kurse: self.kurse.get(..*kursanzahl)?,
        })
    }

    /// Der letzte Tagesabschluss.
    pub fn abschluss(&self) -> Option<&Abschluss> {
        self.abschluss.as_ref()
    }

    /// Zahl der Eintraege seit dem letzten Abschluss, ohne Kurs-, Termin- und Nachrichtenbelege. Mehr als
    /// null heisst: Ein Tag ist angefangen und nicht abgeschlossen.
    pub fn seit_abschluss(&self) -> usize {
        self.seit_abschluss
    }

    /// Die groesste belegte Auftragskennung; 0, wenn es noch keine gibt.
    pub fn letzte_kennung(&self) -> u64 {
        self.letzte_kennung
    }

    pub fn historischer_hoechststand(&self) -> Option<Cent> {
        self.historischer_hoechststand
    }

    pub fn papier_halt(&self) -> Option<PapierHalt> {
        self.papier_halt
    }

    pub fn notbremse_aktiv(&self) -> bool {
        self.papier_halt.is_some()
    }

    pub fn papier_verlustsperre_aktiv(&self) -> bool {
        self.papier_halt == Some(PapierHalt::Gesamtverlust)
    }

    /// Prueft den belegten Gesamtverlust. Beim ersten Tag liefert der Lauf den Startwert.
    pub fn papier_gesamtverlust_erreicht(
        &self,
        wert: Cent,
        startwert: Option<Cent>,
    ) -> Option<bool> {
        let hoch = self
            .papier_verlustreferenz
            .or(self.historischer_hoechststand)
            .max(startwert)?;
        Some(Grenzen::standard().papier_gesamtverlust_erreicht(wert, hoch))
    }

    pub fn papier_waechterreferenz(&self) -> Option<Cent> {
        self.papier_neustart
            .or_else(|| self.abschluss.as_ref().map(|a| a.hoechststand))
    }

    /// Kandidat fuer eine ausdrueckliche Freigabe. Der Kontoabgleich folgt vor dem Schreiben.
    pub fn papier_freigabe_bargeld(&self) -> Option<Cent> {
        if self.papier_verlustsperre_aktiv()
            && self.seit_abschluss == 0
            && self.abschluss.is_some()
            && self.gehalten().is_empty()
            && self.offene_auftraege().next().is_none()
        {
            return self.papier_bargeld().filter(|c| *c > Cent::NULL);
        }
        None
    }

    pub fn prueft_wiederanlauf(&self) -> bool {
        self.prueft_wiederanlauf
    }

    pub(crate) fn papier_bargeld(&self) -> Option<Cent> {
        self.toepfe
            .values()
            .try_fold(Cent::NULL, |summe, topf| summe.checked_add(topf.bargeld()))
    }

    pub fn wissensstichtag(&self, tag: Datum) -> Option<Zeitpunkt> {
        Some(self.wissensstichtage.get(&tag)?.zeit)
    }

    /// Handelstage mit eigenem belegtem Wissensstichtag.
    pub fn tage_mit_wissensstichtag(&self) -> impl Iterator<Item = Datum> + '_ {
        self.wissensstichtage.keys().copied()
    }

    /// Journalbegründungen, die beim Schreiben des Wissensstichtags bereits vorlagen.
    pub fn begruendungen_bis_stichtag(&self, tag: Datum) -> Option<&[(Datum, String, String)]> {
        self.begruendungen
            .get(..self.wissensstichtage.get(&tag)?.begruendungen)
    }

    /// Nur die Quellenzeilen, die beim Schreiben des Wissensstichtags im Journal standen.
    /// Der Aufrufer schränkt zusätzlich über die Abrufzeit ein.
    pub fn kurse_bis_stichtag(&self, tag: Datum) -> Option<&[KursBeleg]> {
        self.kurse.get(..self.wissensstichtage.get(&tag)?.kurse)
    }

    pub fn termine_bis_stichtag(&self, tag: Datum) -> Option<&[TerminBeleg]> {
        self.termine.get(..self.wissensstichtage.get(&tag)?.termine)
    }

    pub fn nachrichten_bis_stichtag(&self, tag: Datum) -> Option<&[NachrichtBeleg]> {
        self.nachrichten
            .get(..self.wissensstichtage.get(&tag)?.nachrichten)
    }

    /// Die für den Papiertag ausdrücklich referenzierten Originalkurse.
    pub fn kursgrundlage(&self, tag: Datum) -> Option<Vec<&KursBeleg>> {
        let stichtag = self.wissensstichtag(tag)?;
        let kurse = self.kurse_bis_stichtag(tag)?;
        let mut belege = self
            .begruendungen
            .iter()
            .filter(|(datum, wer, _)| *datum == tag && wer == KURSGRUNDLAGE);
        let mut felder = belege.next()?.2.split_ascii_whitespace();
        if belege.next().is_some() || felder.next()? != "v1" {
            return None;
        }
        let mut isins = BTreeSet::new();
        felder
            .map(|feld| {
                let beleg = kurse.get(feld.parse::<usize>().ok()?)?;
                (beleg.abruf() <= stichtag
                    && beleg.kurs().herkunft() == Herkunft::Quelle
                    && isins.insert(beleg.isin()))
                .then_some(beleg)
            })
            .collect()
    }

    pub fn abrechnungstag(&self) -> Option<Datum> {
        self.abrechnungstag
    }

    pub fn abrechnungen(&self) -> &Abrechnungen {
        &self.abrechnungen
    }

    /// Werte dieses Topfes, die mit noch nicht abgerechnetem Verkaufserlös gekauft wurden und
    /// vor dieser Valuta nicht freiwillig verkauft werden dürfen (#311 H1a). Stopp, Notbremse
    /// und andere zwingende Ausstiege verkaufen trotzdem.
    pub fn vor_valuta(&self, topf: TopfId, tag: Datum) -> Vec<(Isin, Datum)> {
        self.valuta_sperren
            .iter()
            .filter(|((t, _), valuta)| *t == topf && **valuta > tag)
            .map(|((_, isin), valuta)| (isin.clone(), *valuta))
            .collect()
    }

    /// Ein Tauschkauf, dessen Verkauf ungefüllt endete, kann nicht mehr füllen (#311 H1b).
    pub fn tausch_unmoeglich(&self, l: &AuftragsLage) -> bool {
        l.beleg.nach.iter().any(|v| {
            self.auftrag(*v).is_none_or(|s| {
                !s.zustand.offen() && s.zustand != crate::auftrag::AuftragsZustand::Gefuellt
            })
        })
    }

    /// Schlechtester Erlös der offenen Verkäufe `nach`, an denen Tauschkäufe hängen: Limit mal
    /// offene Stück abzüglich der höchsten Gebühr, jeder Verkauf einmal (#311 H1b).
    pub fn tauscherloes(
        &self,
        nach: impl IntoIterator<Item = u64>,
        gebuehren: &depot_kern::Gebuehren,
    ) -> Option<Cent> {
        let nach: BTreeSet<u64> = nach.into_iter().collect();
        let mut summe = Cent::NULL;
        for v in nach {
            let Some(s) = self.auftrag(v) else {
                continue;
            };
            if s.beleg.richtung != Richtung::Verkauf || !s.zustand.offen() {
                continue;
            }
            let volumen = s.beleg.limit.mal_stueck(s.offen_stueck())?;
            let netto = volumen.checked_sub(gebuehren.fuer(volumen)?)?;
            summe = summe.checked_add(netto.max(Cent::NULL))?;
        }
        Some(summe)
    }

    /// Gefüllte Verkaufserlöse vor ihrer Valuta. Sie sind für Käufe schon frei (#311 H1a);
    /// was damit gekauft wird, sperrt `vor_valuta`.
    pub fn nicht_gutgeschrieben(&self, topf: TopfId) -> Option<Cent> {
        self.gesperrt(topf)
    }

    /// Nächster Valuta-Tag eines noch nicht freigegebenen Verkaufserlöses.
    pub fn naechste_valuta(&self, topf: TopfId) -> Option<Datum> {
        self.offene_valuta(topf).map(|(valuta, _)| *valuta).min()
    }

    pub(crate) fn gesperrt(&self, topf: TopfId) -> Option<Cent> {
        self.offene_valuta(topf)
            .try_fold(Cent::NULL, |summe, (_, netto)| summe.checked_add(*netto))
    }

    fn offene_valuta(&self, topf: TopfId) -> impl Iterator<Item = &(Datum, Cent)> {
        self.auftraege
            .iter()
            .filter(move |l| l.beleg.topf == topf)
            .filter_map(|l| self.abrechnungen.get(&l.beleg.nr))
            .filter(|(valuta, _)| self.abrechnungstag.is_none_or(|tag| *valuta > tag))
    }

    /// Wendet einen Eintrag an. Ein Fehler laesst den Stand unveraendert.
    pub(crate) fn anwenden(&mut self, art: Art, inhalt: &str) -> Result<(), String> {
        match art {
            Art::Kapitalmassnahme => return self.belege_kapitalmassnahme(inhalt),
            Art::Beobachtung => {
                if self.belege_beobachtung(inhalt)? {
                    return Ok(());
                }
            }
            Art::Topf => {
                let t = lies_topf(inhalt).ok_or("Topf unlesbar")?;
                kanonisch(&topf_text(&t), inhalt)?;
                self.pruefe_deckung(&t, None)?;
                self.startgeld
                    .entry(t.id())
                    .or_insert_with(|| t.positionen().is_empty().then(|| t.bargeld()));
                self.stopps
                    .retain(|(id, isin), _| *id != t.id() || t.stueck(isin) > 0);
                self.toepfe.insert(t.id(), t);
            }
            // Belege von aussen beginnen keinen Tag: Der Sammler schreibt sie vor dem Papiertag
            // ins selbe Journal; sie fragen niemanden und reservieren nichts.
            Art::Kurs => {
                self.belege_kurs(inhalt)?;
                return Ok(());
            }
            Art::Termin => {
                self.belege_termin(inhalt)?;
                return Ok(());
            }
            Art::Nachricht => {
                self.belege_nachricht(inhalt)?;
                return Ok(());
            }
            Art::RssAbruf => {
                let a =
                    crate::rss_abruf::RssAbruf::lies(inhalt).ok_or("RSS-Abrufvermerk unlesbar")?;
                self.merke_rss_abruf(a);
                return Ok(());
            }
            Art::Vorbereitung => {
                if self.seit_abschluss > 0 {
                    return Err("Vorbereitung während eines offenen Papiertags".into());
                }
                crate::vorbereitung::anwenden(&mut self.vorbereitungen, &mut self.ki, inhalt)?;
                return Ok(());
            }
            Art::Weltprognose => self.belege_weltprognose(inhalt)?,
            Art::Aufloesung => return self.loese_auf(inhalt),
            Art::TopfUmbuchung => self.buche_topfgeld(inhalt)?,
            Art::Auftrag => self.belege_auftrag(inhalt)?,
            Art::Zustand | Art::Stopp => self.belege_meldung(art, inhalt)?,
            Art::KiGrenzen => self.ki.abdruck(inhalt)?,
            Art::Ki => {
                let wiederholung = inhalt.rsplit_once(";wiederholung;").map(|(_, nr)| nr);
                if let Some(nr) = wiederholung {
                    self.pruefe_ki_folgekontext(nr)?;
                }
                let zweck = self.ki.buche(inhalt)?;
                if zweck.ohne_handelsbeginn()
                    || (wiederholung.is_some() && zweck == crate::KiZweck::Vorbereitung)
                {
                    return Ok(());
                }
            }
            Art::KiAufruf => return self.ki.belege_aufruf(inhalt),
            Art::NeuroErgebnis => return self.ki.belege_neuroergebnis(inhalt),
            Art::KiAbrechnung => {
                self.ki.rechne_ab(inhalt)?;
                return Ok(());
            }
            Art::Lehre => return self.belege_lehre(inhalt),
            Art::Begruendung => {
                let b = lies_begruendung(inhalt).ok_or("Begründung unlesbar")?;
                kanonisch(&begruendung_text(b.0, &b.1, &b.2), inhalt)?;
                self.lies_alten_papier_halt(b.0, &b.1, &b.2);
                self.begruendungen.push(b);
            }
            Art::Format => {
                self.stelle_format_um(inhalt)?;
                return Ok(());
            }
            Art::Abrechnungstag => self.beginne_abrechnung(inhalt)?,
            Art::NotbremseFreigabe => return self.belege_wiederanlauf(inhalt),
            Art::PapierHalt => self.belege_papier_halt(inhalt)?,
            Art::PapierFreigabe => return self.belege_papier_freigabe(inhalt),
            Art::Wissensstichtag => self.belege_wissensstichtag(inhalt)?,
            Art::Abschluss => {
                return self.schliesse_ab(inhalt);
            }
        }
        self.seit_abschluss = self.seit_abschluss.saturating_add(1);
        Ok(())
    }

    fn pruefe_ki_folgekontext(&self, nr: &str) -> Result<(), String> {
        let nr = nr.parse::<usize>().map_err(|_| "KI-Erstbuchung unlesbar")?;
        let (zweck, _) = self.ki.buchungsbeleg(nr).ok_or("KI-Erstbuchung fehlt")?;
        match zweck {
            crate::KiZweck::Vorbereitung => {
                let b = self.ki.buchungen().get(nr).ok_or("KI-Erstbuchung fehlt")?;
                let offen = self.vorbereitungen().last().is_some_and(|p| {
                    p.zeit.datum() == Some(b.datum)
                        && p.reserviert.contains(&b.berater)
                        && !p.antworten.iter().any(|a| a.berater == b.berater)
                });
                if !offen {
                    return Err("Vorbereitung für KI-Wiederholung nicht offen".into());
                }
            }
            crate::KiZweck::Neurofrage if self.seit_abschluss != 0 => {
                return Err("Papierlauf für NeuroSchlopf-Wiederholung offen".into());
            }
            _ => {}
        }
        Ok(())
    }

    fn buche_topfgeld(&mut self, inhalt: &str) -> Result<(), String> {
        let (datum, nach_a) = inhalt.split_once(';').ok_or("Topfumbuchung unlesbar")?;
        let datum = Datum::lies(datum).ok_or("Datum der Topfumbuchung unlesbar")?;
        let nach_a = Cent(
            nach_a
                .parse()
                .map_err(|_| "Betrag der Topfumbuchung unlesbar")?,
        );
        kanonisch(&format!("{datum};{}", nach_a.0), inhalt)?;
        if nach_a == Cent::NULL
            || self.abrechnungstag != Some(datum)
            || self.papier_halt.is_some()
            || self.offene_auftraege().next().is_some()
            || self.topf_umbuchungen.contains_key(&datum)
            || self.abschluss.as_ref().is_some_and(|a| a.datum >= datum)
        {
            return Err("Topfumbuchung nicht zulässig".into());
        }
        let nach_b = Cent(nach_a.0.checked_neg().ok_or("Topfumbuchung Überlauf")?);
        let anwenden = |id, betrag| -> Result<Topf, String> {
            let alt = self.toepfe.get(&id).ok_or("Topf fehlt")?;
            let bar = alt
                .bargeld()
                .checked_add(betrag)
                .ok_or("Bargeld Überlauf")?;
            let mut neu = Topf::neu(id, bar).map_err(|e| format!("{e:?}"))?;
            for (isin, stueck) in alt.positionen() {
                neu.setze_bestand(isin.clone(), *stueck);
            }
            self.pruefe_deckung(&neu, None)?;
            Ok(neu)
        };
        let a = anwenden(TopfId::A, nach_a)?;
        let b = anwenden(TopfId::B, nach_b)?;
        self.toepfe.insert(TopfId::A, a);
        self.toepfe.insert(TopfId::B, b);
        self.topf_umbuchungen.insert(datum, nach_a);
        Ok(())
    }

    fn belege_lehre(&mut self, inhalt: &str) -> Result<(), String> {
        let (datum, wer, text) = lies_begruendung(inhalt).ok_or("Lehre unlesbar")?;
        kanonisch(&begruendung_text(datum, &wer, &text), inhalt)?;
        if datum.wochentag() != Some(6)
            || !matches!(wer.as_str(), "Dachs" | "Maus")
            || text.len() > 3_200
        {
            return Err("Lehre ungültig".into());
        }
        let versuch = self
            .lehren
            .iter()
            .rev()
            .find(|(tag, ki, _)| *tag == datum && ki == &wer);
        match (versuch, text.as_str()) {
            (None, "begonnen") => (),
            (Some((_, _, vorher)), _) if vorher == "begonnen" && text != "begonnen" => (),
            _ => return Err("Rückschau fehlt oder wurde schon protokolliert".into()),
        }
        self.lehren.push((datum, wer, text));
        Ok(())
    }

    fn belege_auftrag(&mut self, inhalt: &str) -> Result<(), String> {
        let b = lies_auftrag(inhalt).ok_or("Auftrag unlesbar")?;
        kanonisch(&auftrag_text(&b), inhalt)?;
        let nr = b.nr;
        if nr <= self.letzte_kennung {
            return Err(format!(
                "Auftragskennung {nr} nicht größer als die letzte ({})",
                self.letzte_kennung
            ));
        }
        if self.format == FORMAT_ALT {
            // Format 1: Das Ergebnis steht im naechsten Topf; der Auftrag bindet nichts.
            self.letzte_kennung = nr;
            self.auftraege.push(AuftragsLage::alt(b));
            return Ok(());
        }
        let topf = self
            .toepfe
            .get(&b.topf)
            .ok_or_else(|| format!("Auftrag {nr}: Topf {:?} fehlt", b.topf))?;
        for v in &b.nach {
            let gueltig = self.auftrag(*v).is_some_and(|l| {
                l.beleg.topf == b.topf && l.beleg.richtung == Richtung::Verkauf && l.zustand.offen()
            });
            if !gueltig {
                return Err(format!(
                    "Auftrag {nr}: Tausch nach {v} ohne offenen Verkauf desselben Topfes"
                ));
            }
        }
        let neu = AuftragsLage::reserviert(b);
        self.pruefe_deckung(topf, Some(&neu))?;
        self.letzte_kennung = nr;
        self.auftraege.push(neu);
        Ok(())
    }

    fn belege_meldung(&mut self, art: Art, inhalt: &str) -> Result<(), String> {
        let (meldung, stopp) = if art == Art::Stopp {
            let felder: Vec<_> = inhalt.splitn(4, ';').collect();
            let [_, _, kurs, meldung] = felder.as_slice() else {
                return Err("Stoppbeleg unlesbar".into());
            };
            (
                *meldung,
                Some(Cent(kurs.parse().map_err(|_| "Stoppkurs unlesbar")?)),
            )
        } else {
            (inhalt, None)
        };
        let (nr, m) = lies_meldung(meldung).ok_or("Zustand unlesbar")?;
        let soll = self.stoppmeldung(nr, &m)?;
        match (soll, stopp) {
            (Some(text), Some(_)) => kanonisch(&text, inhalt)?,
            (None, None) => kanonisch(&meldung_text(nr, &m), inhalt)?,
            _ => return Err("Stoppbeleg passt nicht zur Ausführung".into()),
        }
        self.melde(nr, &m, stopp)
    }

    fn stelle_format_um(&mut self, inhalt: &str) -> Result<(), String> {
        kanonisch(&FORMAT.to_string(), inhalt)?;
        if self.format != FORMAT_ALT {
            return Err(format!("Umstellung im Format {}", self.format));
        }
        if self.seit_abschluss > 0 {
            return Err("Umstellung nur ohne angefangenen Tag".into());
        }
        self.format = FORMAT;
        Ok(())
    }

    fn belege_beobachtung(&mut self, inhalt: &str) -> Result<bool, String> {
        let b = lies_beobachtung(inhalt).ok_or("Beobachtung unlesbar")?;
        kanonisch(&beobachtung_text(b.0, &b.1), inhalt)?;
        let rss = crate::rss_abruf::RssAbruf::aus_alt(b.0, &b.1);
        if let Some(a) = rss {
            self.merke_rss_abruf(a);
            Ok(true)
        } else {
            // Behalte die Tagesnummer für bestehende Verweise. Die Rundungsnotiz
            // gehört zum Kursabruf und beginnt keinen Papiertag.
            let vom_sammler = depot_kern::tiingo::ist_rundungsbeobachtung(&b.1);
            self.beobachtungen.push(b);
            Ok(vom_sammler)
        }
    }

    /// Eine Auflösung gilt nur, wenn sie genau so aus dem Stand folgt (#114).
    fn loese_auf(&mut self, inhalt: &str) -> Result<(), String> {
        let a = crate::Aufloesung::lies(inhalt).ok_or("Auflösung unlesbar")?;
        if !self.faellige_aufloesungen().contains(&a) {
            return Err(format!(
                "Auflösung passt nicht zu Prognose, Zieltag oder Schlusskurs: {inhalt}"
            ));
        }
        self.aufloesungen.push(a);
        Ok(())
    }

    fn belege_weltprognose(&mut self, inhalt: &str) -> Result<(), String> {
        let b = PrognoseBeleg::lies(inhalt).ok_or("Weltprognose unlesbar")?;
        if self.abrechnungstag != Some(b.datum) {
            return Err(
                "Weltprognose: Prognosetag stimmt nicht mit dem Abrechnungstag überein".into(),
            );
        }
        if self.abschluss.as_ref().is_some_and(|a| a.datum >= b.datum) {
            return Err("Weltprognose: Prognosetag ist bereits abgeschlossen".into());
        }
        if self
            .weltprognosen
            .iter()
            .any(|alt| alt.datum == b.datum && alt.berater == b.berater)
        {
            return Err(
                "Weltprognose: Für diesen Berater und Tag liegt bereits ein Beleg vor".into(),
            );
        }
        if b.prognose
            .ereignis_nr
            .checked_sub(1)
            .and_then(|nr| {
                self.beobachtungen
                    .iter()
                    .filter(|(tag, _)| *tag == b.datum)
                    .nth(nr)
            })
            .is_none_or(|(_, e)| *e != b.ereignis)
        {
            return Err("Weltprognose: Ereignisnummer und Ereignistext passen nicht zum Journalbeleg des Prognosetags".into());
        }
        if !self.kurse.iter().any(|k| {
            k.isin() == &b.prognose.isin
                && k.kurs().handelstag() == b.datum
                && k.kurs().preis() == b.ausgangskurs
                && k.kurs().herkunft() == Herkunft::Quelle
        }) {
            return Err(
                "Weltprognose: Kein Quellenkurs mit diesem Ausgangspreis am Prognosetag belegt"
                    .into(),
            );
        }
        self.weltprognosen.push(b);
        Ok(())
    }

    fn schliesse_ab(&mut self, inhalt: &str) -> Result<(), String> {
        let a = lies_abschluss(inhalt).ok_or("Abschluss unlesbar")?;
        kanonisch(&abschluss_text(&a), inhalt)?;
        // Nur bestaetigte, zeitbelegte Papierorders duerfen in den naechsten Tag.
        if let Some(l) = self.offene_auftraege().find(|l| !ueber_nacht(l)) {
            return Err(format!(
                "Abschluss mit offenem Auftrag {} ({:?})",
                l.beleg.nr, l.zustand
            ));
        }
        if let Some(vorher) = &self.abschluss
            && a.datum <= vorher.datum
        {
            return Err(format!(
                "Abschluss {} nicht nach dem letzten ({})",
                datum_text(a.datum),
                datum_text(vorher.datum)
            ));
        }
        if let Some(tag) = self.abrechnungstag
            && a.datum != tag
        {
            return Err("Abschluss passt nicht zum Abrechnungstag".into());
        }
        self.historischer_hoechststand = self.historischer_hoechststand.max(Some(a.hoechststand));
        self.papier_verlustreferenz = self
            .papier_verlustreferenz
            .map(|hoch| hoch.max(a.hoechststand));
        self.papier_neustart = None;
        self.tagesschluesse.push((a.datum, self.toepfe.clone()));
        self.schlussbelege.push((a.clone(), self.kurse.len()));
        self.abschluss = Some(a);
        self.seit_abschluss = 0;
        Ok(())
    }

    fn lies_alten_papier_halt(&mut self, tag: Datum, wer: &str, text: &str) {
        if wer == "Waechter" {
            match text {
                ALTE_VERLUSTSPERRE => self.papier_halt = Some(PapierHalt::Gesamtverlust),
                ALTE_NOTBREMSE
                    if self.papier_halt.is_none()
                        && self
                            .notbremse_freigabe
                            .is_none_or(|freigabe| tag > freigabe) =>
                {
                    self.papier_halt = Some(PapierHalt::Notbremse);
                }
                _ => {}
            }
        }
    }

    fn belege_papier_halt(&mut self, inhalt: &str) -> Result<(), String> {
        let (tag, grund) = inhalt.split_once(';').ok_or("Papier-Halt unlesbar")?;
        let tag = Datum::lies(tag).ok_or("Tag des Papier-Halts unlesbar")?;
        let grund = match grund {
            "notbremse" => PapierHalt::Notbremse,
            "gesamtverlust" => PapierHalt::Gesamtverlust,
            _ => return Err("Grundkennung des Papier-Halts unbekannt".into()),
        };
        kanonisch(&format!("{tag};{}", grund.kennung()), inhalt)?;
        if self.abrechnungstag != Some(tag)
            || self.abschluss.as_ref().is_some_and(|a| a.datum >= tag)
            || !matches!(
                (self.papier_halt, grund),
                (None, _) | (Some(PapierHalt::Notbremse), PapierHalt::Gesamtverlust)
            )
        {
            return Err("Papier-Halt unzulässig".into());
        }
        self.papier_halt = Some(grund);
        self.begruendungen
            .push((tag, "Waechter".into(), grund.meldung()));
        Ok(())
    }

    fn belege_papier_freigabe(&mut self, inhalt: &str) -> Result<(), String> {
        let tag = self
            .abschluss
            .as_ref()
            .ok_or("Papierfreigabe ohne Abschluss")?
            .datum;
        let bargeld = self
            .papier_freigabe_bargeld()
            .ok_or("Papierfreigabe unzulässig")?;
        if !Grenzen::standard().papier_freigabe_neue_messung() {
            return Err("Neue Papier-Verlustmessung ist nicht freigegeben".into());
        }
        let basis = papier_freigabe_text(tag, bargeld);
        let zeit = inhalt
            .strip_prefix(&basis)
            .and_then(|rest| rest.strip_prefix(';'))
            .and_then(Zeitpunkt::lies)
            .ok_or("UTC-Zeit der Papierfreigabe fehlt oder ist unlesbar")?;
        kanonisch(&format!("{basis};{zeit}"), inhalt)?;
        if zeit.in_frankfurt().is_none_or(|(datum, ..)| datum < tag) {
            return Err("Papierfreigabe vor dem Abschlusstag".into());
        }
        self.papier_halt = None;
        self.papier_verlustreferenz = Some(bargeld);
        self.papier_neustart = Some(bargeld);
        self.notbremse_freigabe = Some(tag);
        self.prueft_wiederanlauf = false;
        self.begruendungen.push((tag, "Einrichtung, Dialog".into(), format!(
            "Papier-Gesamtverlustsperre in der Einrichtung freigegeben. Neue Verlustmessung ab {} Cent. Bestätigt am {zeit}.",
            bargeld.0
        )));
        Ok(())
    }

    fn belege_wiederanlauf(&mut self, inhalt: &str) -> Result<(), String> {
        let a = lies_abschluss(inhalt).ok_or("Notbremsenfreigabe unlesbar")?;
        if !self.prueft_wiederanlauf
            || !self.notbremse_aktiv()
            || self.abrechnungstag != Some(a.datum)
            || !self.gehalten().is_empty()
            || self.offene_auftraege().next().is_some()
            || self.papier_verlustsperre_aktiv()
            || self.papier_gesamtverlust_erreicht(a.hoechststand, None) != Some(false)
            || self.papier_bargeld() != Some(a.hoechststand)
        {
            return Err("Notbremsenfreigabe unzulässig".into());
        }
        self.schliesse_ab(inhalt)?;
        self.papier_halt = None;
        self.notbremse_freigabe = Some(a.datum);
        self.prueft_wiederanlauf = false;
        self.begruendungen.push((
            a.datum,
            "Waechter".into(),
            format!(
                "Notbremse automatisch aufgehoben: Konto abgeglichen und Kurse geprüft. Neuer Referenzwert {} Cent; normaler Betrieb ab dem nächsten Papiertag.",
                a.hoechststand.0
            ),
        ));
        Ok(())
    }

    fn belege_wissensstichtag(&mut self, inhalt: &str) -> Result<(), String> {
        let (tag, zeit) = inhalt.split_once(';').ok_or("Wissensstichtag unlesbar")?;
        let tag = Datum::lies(tag).ok_or("Tag des Wissensstichtags unlesbar")?;
        let zeit = Zeitpunkt::lies(zeit).ok_or("Zeit des Wissensstichtags unlesbar")?;
        kanonisch(&format!("{tag};{zeit}"), inhalt)?;
        if self.format != FORMAT
            || zeit.in_frankfurt().map(|(d, ..)| d) != Some(tag)
            || self.seit_abschluss != 0
            || self.abschluss.as_ref().is_some_and(|a| a.datum >= tag)
            || self.wissensstichtage.contains_key(&tag)
        {
            return Err("Wissensstichtag unzulässig".into());
        }
        self.wissensstichtage.insert(
            tag,
            Wissensstand {
                zeit,
                begruendungen: self.begruendungen.len(),
                kurse: self.kurse.len(),
                termine: self.termine.len(),
                nachrichten: self.nachrichten.len(),
            },
        );
        Ok(())
    }

    fn beginne_abrechnung(&mut self, inhalt: &str) -> Result<(), String> {
        let tag = Datum::lies(inhalt).ok_or("Abrechnungstag unlesbar")?;
        kanonisch(&tag.to_string(), inhalt)?;
        if self.format != FORMAT
            || self.offene_auftraege().any(|l| !ueber_nacht(l))
            || self.abschluss.as_ref().is_some_and(|a| tag <= a.datum)
            || self.abrechnungstag.is_some_and(|alt| {
                tag <= alt || self.abschluss.as_ref().is_none_or(|a| a.datum != alt)
            })
        {
            return Err("Abrechnungstag unzulässig".into());
        }
        self.prueft_wiederanlauf = self.notbremse_aktiv()
            && self.gehalten().is_empty()
            && self.offene_auftraege().next().is_none();
        self.abrechnungstag = Some(tag);
        Ok(())
    }

    /// Ein Kurs von aussen: nur im Format 2, Abrufe fallen nie zurueck.
    fn belege_kurs(&mut self, inhalt: &str) -> Result<(), String> {
        if self.format == FORMAT_ALT {
            return Err("Kurs im Format 1: vor dem ersten Kurs stelle_um".into());
        }
        let k = lies_kursbeleg(inhalt)?;
        kanonisch(&kursbeleg_text(&k), inhalt)?;
        if let Some(vorher) = self.letzter_abruf()
            && k.abruf() < vorher
        {
            return Err(format!(
                "Abruf {} vor dem letzten belegten ({vorher})",
                k.abruf()
            ));
        }
        self.kurse.push(k);
        Ok(())
    }

    /// Ein Termin von aussen.
    /// Nur im Format 2, Abrufe fallen nie zurueck, und ein Eintrag aendert den Termin seiner
    /// Kennung: Ein unveraenderter Termin steht nicht zweimal hintereinander im Journal.
    fn belege_termin(&mut self, inhalt: &str) -> Result<(), String> {
        if self.format == FORMAT_ALT {
            return Err("Termin im Format 1: vor dem ersten Termin stelle_um".into());
        }
        let t = lies_terminbeleg(inhalt)?;
        kanonisch(&terminbeleg_text(&t), inhalt)?;
        if let Some(vorher) = self.letzter_abruf()
            && t.abruf() < vorher
        {
            return Err(format!(
                "Abruf {} vor dem letzten belegten ({vorher})",
                t.abruf()
            ));
        }
        if self
            .termin(t.termin().kennung())
            .is_some_and(|alt| alt.termin() == t.termin())
        {
            return Err(format!(
                "Termin {} unverändert noch einmal belegt",
                t.termin().kennung()
            ));
        }
        self.termine.push(t);
        Ok(())
    }

    /// Eine Nachricht von aussen. Nur im Format 2, Abrufe fallen nie zurueck, und dieselbe
    /// Nachricht steht nicht zweimal im Journal.
    fn belege_nachricht(&mut self, inhalt: &str) -> Result<(), String> {
        if self.format == FORMAT_ALT {
            return Err("Nachricht im Format 1: vor der ersten Nachricht stelle_um".into());
        }
        let n = lies_nachrichtbeleg(inhalt)?;
        kanonisch(&nachrichtbeleg_text(&n), inhalt)?;
        if let Some(vorher) = self.letzter_abruf()
            && n.abruf() < vorher
        {
            return Err(format!(
                "Abruf {} vor dem letzten belegten ({vorher})",
                n.abruf()
            ));
        }
        if self.nachrichten.iter().any(|alt| alt.gleich(&n)) {
            return Err("Nachricht noch einmal belegt".into());
        }
        self.nachrichten.push(n);
        Ok(())
    }

    /// Wendet die Meldung zu einem Auftrag an: naechster Zustand und bei einer Ausfuehrung die
    /// Buchung im Topf. Ein Fehler laesst den Stand unveraendert.
    fn melde(&mut self, nr: u64, m: &Meldung, stopp: Option<Cent>) -> Result<(), String> {
        let index = self
            .auftraege
            .iter()
            .position(|l| l.beleg.nr == nr)
            .ok_or_else(|| format!("Auftrag {nr} unbekannt"))?;
        let alt = self.auftraege.get(index).ok_or("Auftrag fehlt")?;
        if matches!(m, Meldung::Ausfuehrung(_))
            && alt.beleg.nach.iter().any(|v| {
                self.auftrag(*v).map(|l| l.zustand)
                    != Some(crate::auftrag::AuftragsZustand::Gefuellt)
            })
        {
            return Err(format!("Tauschkauf {nr} vor seinen Verkäufen ausgeführt"));
        }
        let neu = alt.nach(m)?;
        let topf = self
            .toepfe
            .get(&neu.beleg.topf)
            .ok_or_else(|| format!("Auftrag {nr}: Topf fehlt"))?;
        let sperre = match m {
            Meldung::Ausfuehrung(a) if neu.beleg.richtung == Richtung::Kauf => {
                self.valuta_sperre(topf, a)?
            }
            _ => None,
        };
        let (topf, erloes) = match m {
            Meldung::Ausfuehrung(a) => (buche(topf, &neu.beleg, a)?, self.neuer_erloes(&neu, a)?),
            _ => (topf.clone(), None),
        };
        self.pruefe_deckung(&topf, Some(&neu))?;
        if let Some((valuta, netto)) = erloes {
            let gesamt = self
                .abrechnungen
                .get(&nr)
                .map_or(Cent::NULL, |(_, n)| *n)
                .checked_add(netto)
                .ok_or("Verkaufserlös: Überlauf")?;
            self.abrechnungen.insert(nr, (valuta, gesamt));
        }
        if let Some(valuta) = sperre {
            let platz = self
                .valuta_sperren
                .entry((neu.beleg.topf, neu.beleg.isin.clone()))
                .or_insert(valuta);
            *platz = (*platz).max(valuta);
        }
        if matches!(m, Meldung::Ausfuehrung(_)) {
            self.ausfuehrungsfolge
                .push((index, alt.ausfuehrungen.len()));
            let beleg = &neu.beleg;
            if let Some(kurs) = stopp {
                self.stopps.insert((beleg.topf, beleg.isin.clone()), kurs);
            } else if beleg.richtung == Richtung::Kauf || topf.stueck(&beleg.isin) == 0 {
                self.stopps.remove(&(beleg.topf, beleg.isin.clone()));
            }
        }
        self.toepfe.insert(topf.id(), topf);
        if let Some(platz) = self.auftraege.get_mut(index) {
            *platz = neu;
        }
        Ok(())
    }

    /// Bezahlt ein Kauf mehr als das abgerechnete Bargeld, nutzt er noch nicht abgerechneten
    /// Erlös: Dann gilt die späteste offene Valuta als Sperre für freiwillige Verkäufe.
    fn valuta_sperre(
        &self,
        topf: &Topf,
        a: &crate::auftrag::Ausfuehrung,
    ) -> Result<Option<Datum>, String> {
        let kosten = a
            .kurs
            .mal_stueck(a.stueck)
            .and_then(|v| v.checked_add(a.gebuehr))
            .ok_or("Valuta: Überlauf")?;
        let abgerechnet = self
            .gesperrt(topf.id())
            .and_then(|g| topf.bargeld().checked_sub(g))
            .ok_or("Valuta: Überlauf")?;
        Ok((kosten > abgerechnet)
            .then(|| self.naechste_valuta_spaetest(topf.id()))
            .flatten())
    }

    fn naechste_valuta_spaetest(&self, topf: TopfId) -> Option<Datum> {
        self.offene_valuta(topf).map(|(valuta, _)| *valuta).max()
    }

    fn neuer_erloes(
        &self,
        lage: &AuftragsLage,
        aus: &crate::auftrag::Ausfuehrung,
    ) -> Result<Option<(Datum, Cent)>, String> {
        let beleg = &lage.beleg;
        let Some(tag) = self.abrechnungstag else {
            return if aus.kurszeit.is_some() {
                Err("Kurszeit ohne Abrechnungstag".into())
            } else {
                Ok(None)
            };
        };
        if self.abschluss.as_ref().is_some_and(|a| a.datum == tag) {
            return Err("Ausführung ohne offenen Abrechnungstag".into());
        }
        let mut ausfuehrungstag = tag;
        if let Some(zeit) = aus.kurszeit {
            let datum = zeit.datum().ok_or("Ausführungstag fehlt")?;
            let morgen = self
                .wissensstichtag(tag)
                .filter(|z| depot_kern::liste::morgenlauf(*z));
            let vorabend = morgen.is_some_and(|bis| {
                depot_kern::liste::boerse_von(&beleg.isin).and_then(|b| b.bezugstag(tag))
                    == Some(datum)
                    && zeit < bis
            });
            let rundung = match beleg.richtung {
                Richtung::Kauf => Rundung::Auf,
                Richtung::Verkauf => Rundung::Ab,
            };
            let stichtag = self.wissensstichtag(tag);
            let nachgeholt =
                datum < tag && !vorabend && lage.frist == Some(zeit) && stichtag.is_some();
            if (datum != tag && !vorabend && !nachgeholt)
                || !self.kurse.iter().any(|b| {
                    b.isin() == &beleg.isin
                        && b.kurs().herkunft() == Herkunft::Quelle
                        && b.kurs().zeitpunkt() == zeit
                        && (!(vorabend || nachgeholt)
                            || stichtag.is_some_and(|bis| b.abruf() <= bis))
                        && (b.kurs().preis().mal_stueck(1, rundung) == Some(aus.kurs)
                            || depot_kern::liste::boerse_von(&beleg.isin).is_some_and(|boerse| {
                                depot_kern::Grenzen::standard().papierkurs(
                                    boerse,
                                    b.kurs().preis(),
                                    rundung,
                                ) == Some(aus.kurs)
                            }))
                })
            {
                return Err("Kein passender Kursbeleg am Abrechnungstag".into());
            }
            ausfuehrungstag = datum;
        }
        if beleg.richtung != Richtung::Verkauf {
            return Ok(None);
        }
        let netto = aus
            .kurs
            .mal_stueck(aus.stueck)
            .and_then(|v| v.checked_sub(aus.gebuehr))
            .ok_or("Verkaufserlös: Überlauf")?;
        let valuta = papier_valuta(ausfuehrungstag).ok_or("Valuta ausserhalb des Kalenders")?;
        Ok((netto > Cent::NULL).then_some((valuta, netto)))
    }

    /// Die Deckung eines Topfes (R37, Kap. 9): Das Bargeld traegt alle offenen Kaeufe am Limit,
    /// der Bestand jedes Werts alle offenen Verkaeufe. `ersatz` steht fuer den Auftrag mit
    /// seiner Kennung oder kommt neu hinzu. Was ein Kauf darueber hinaus an Gebuehr braucht,
    /// prueft der Kontowaechter vor der Reservierung (`Journal::reserviere_auftrag`).
    fn pruefe_deckung(&self, topf: &Topf, ersatz: Option<&AuftragsLage>) -> Result<(), String> {
        let ersetzt = ersatz.map(|e| e.beleg.nr);
        let offene: Vec<&AuftragsLage> = self
            .auftraege
            .iter()
            .filter(|l| Some(l.beleg.nr) != ersetzt)
            .chain(ersatz)
            .filter(|l| l.beleg.topf == topf.id() && l.zustand.offen())
            .filter(|l| !self.tausch_unmoeglich(l))
            .collect();
        let nach = offene
            .iter()
            .filter(|l| l.beleg.richtung == Richtung::Kauf)
            .flat_map(|l| l.beleg.nach.iter().copied());
        let erloes = self
            .tauscherloes(nach, &Grenzen::standard().gebuehren())
            .ok_or("Deckung: Überlauf")?;
        let mut kauf = Cent::NULL;
        let mut verkauf: BTreeMap<&Isin, u64> = BTreeMap::new();
        for l in offene {
            let offen = l.offen_stueck();
            match l.beleg.richtung {
                Richtung::Kauf => {
                    kauf = l
                        .beleg
                        .limit
                        .mal_stueck(offen)
                        .and_then(|v| kauf.checked_add(v))
                        .ok_or("Deckung: Überlauf")?;
                }
                Richtung::Verkauf => {
                    let summe = verkauf.entry(&l.beleg.isin).or_insert(0);
                    *summe = summe.checked_add(offen).ok_or("Deckung: Überlauf")?;
                }
            }
        }
        // Gefüllte, noch nicht abgerechnete Erlöse decken Käufe (#311 H1a), Tauschkäufe dazu
        // die offenen Verkäufe, an denen sie hängen (#311 H1b).
        let frei = topf
            .bargeld()
            .checked_add(erloes)
            .ok_or("Deckung: Überlauf")?;
        if kauf > frei {
            return Err(format!(
                "Deckung: offene Käufe {} Cent über dem Bargeld {} Cent",
                kauf.0, frei.0
            ));
        }
        for (isin, stueck) in verkauf {
            if stueck > topf.stueck(isin) {
                return Err(format!(
                    "Deckung: {stueck} Stück {isin} im Verkauf, {} im Bestand",
                    topf.stueck(isin)
                ));
            }
        }
        Ok(())
    }
}

/// Bucht eine Ausfuehrung im Topf: Bargeld und Bestand nie negativ.
fn ueber_nacht(l: &AuftragsLage) -> bool {
    use crate::auftrag::AuftragsZustand as Z;
    l.versand.is_some() && matches!(l.zustand, Z::Angenommen | Z::Teilgefuellt)
}

fn buche(topf: &Topf, b: &AuftragsBeleg, a: &crate::auftrag::Ausfuehrung) -> Result<Topf, String> {
    let volumen = a.kurs.mal_stueck(a.stueck).ok_or("Ausführung: Überlauf")?;
    let gehalten = topf.stueck(&b.isin);
    let (bargeld, stueck) = match b.richtung {
        Richtung::Kauf => (
            topf.bargeld()
                .checked_sub(volumen)
                .and_then(|g| g.checked_sub(a.gebuehr)),
            gehalten.checked_add(a.stueck),
        ),
        Richtung::Verkauf => (
            topf.bargeld()
                .checked_add(volumen)
                .and_then(|g| g.checked_sub(a.gebuehr)),
            gehalten.checked_sub(a.stueck),
        ),
    };
    let (Some(bargeld), Some(stueck)) = (bargeld, stueck) else {
        return Err(format!("Auftrag {}: Ausführung nicht buchbar", b.nr));
    };
    let mut neu = Topf::neu(topf.id(), bargeld)
        .map_err(|_| format!("Auftrag {}: Bargeld würde negativ", b.nr))?;
    for (isin, n) in topf.positionen() {
        neu.setze_bestand(isin.clone(), *n);
    }
    neu.setze_bestand(b.isin.clone(), stueck);
    Ok(neu)
}

fn kanonisch(neu: &str, alt: &str) -> Result<(), String> {
    if neu == alt {
        Ok(())
    } else {
        Err(format!("nicht in kanonischer Form: `{alt}` statt `{neu}`"))
    }
}

/// Trennt `art;inhalt`.
pub(crate) fn trenne(zeile: &str) -> Option<(Art, &str)> {
    let (name, inhalt) = zeile.split_once(';')?;
    Some((Art::aus_name(name)?, inhalt))
}

pub(crate) fn datum_text(d: Datum) -> String {
    format!("{:04}-{:02}-{:02}", d.jahr(), d.monat(), d.tag())
}

pub(crate) fn lies_datum(text: &str) -> Option<Datum> {
    let mut teile = text.split('-');
    let jahr = teile.next()?.parse().ok()?;
    let monat = teile.next()?.parse().ok()?;
    let tag = teile.next()?.parse().ok()?;
    if teile.next().is_some() {
        return None;
    }
    Datum::neu(jahr, monat, tag)
}

pub(crate) fn beobachtung_text(d: Datum, text: &str) -> String {
    format!("{};{text}", datum_text(d))
}

fn lies_beobachtung(inhalt: &str) -> Option<(Datum, String)> {
    let (datum, text) = inhalt.split_once(';')?;
    if text.trim().is_empty() {
        return None;
    }
    Some((lies_datum(datum)?, text.to_string()))
}

pub(crate) fn begruendung_text(d: Datum, wer: &str, text: &str) -> String {
    format!("{};{wer};{text}", datum_text(d))
}

fn lies_begruendung(inhalt: &str) -> Option<(Datum, String, String)> {
    let mut teile = inhalt.splitn(3, ';');
    let (datum, wer, text) = (teile.next()?, teile.next()?, teile.next()?);
    if wer.trim().is_empty() || wer.trim() != wer || text.trim().is_empty() {
        return None;
    }
    Some((lies_datum(datum)?, wer.to_string(), text.to_string()))
}

pub(crate) fn abschluss_text(a: &Abschluss) -> String {
    format!("{};{};{}", datum_text(a.datum), a.hoechststand.0, a.abdruck)
}

pub(crate) fn lies_abschluss(inhalt: &str) -> Option<Abschluss> {
    let teile: Vec<&str> = inhalt.split(';').collect();
    let [datum, hoch, abdruck] = teile.as_slice() else {
        return None;
    };
    let hoch = Cent(hoch.parse().ok()?);
    (hoch.0 > 0).then_some(())?;
    let form = abdruck.len() == 64
        && abdruck
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    form.then_some(())?;
    Some(Abschluss {
        datum: lies_datum(datum)?,
        hoechststand: hoch,
        abdruck: (*abdruck).to_string(),
    })
}

pub(crate) fn topf_name(t: TopfId) -> &'static str {
    match t {
        TopfId::A => "A",
        TopfId::B => "B",
    }
}

pub(crate) fn lies_topf_name(t: &str) -> Option<TopfId> {
    match t {
        "A" => Some(TopfId::A),
        "B" => Some(TopfId::B),
        _ => None,
    }
}

pub(crate) fn topf_text(t: &Topf) -> String {
    let positionen: Vec<String> = t
        .positionen()
        .iter()
        .map(|(isin, stueck)| format!("{isin}:{stueck}"))
        .collect();
    format!(
        "{};{};{}",
        topf_name(t.id()),
        t.bargeld().0,
        positionen.join(",")
    )
}

fn lies_topf(inhalt: &str) -> Option<Topf> {
    let teile: Vec<&str> = inhalt.split(';').collect();
    let [id, bargeld, positionen] = teile.as_slice() else {
        return None;
    };
    let mut topf = Topf::neu(lies_topf_name(id)?, Cent(bargeld.parse().ok()?)).ok()?;
    if !positionen.is_empty() {
        for p in positionen.split(',') {
            let (isin, stueck) = p.split_once(':')?;
            let isin = Isin::neu(isin).ok()?;
            let stueck: u64 = stueck.parse().ok()?;
            // Null wuerde still entfernt, doppelt still ueberschrieben: beides ist unlesbar.
            if stueck == 0 || topf.stueck(&isin) != 0 {
                return None;
            }
            topf.setze_bestand(isin, stueck);
        }
    }
    Some(topf)
}

fn richtung_name(r: Richtung) -> &'static str {
    match r {
        Richtung::Kauf => "Kauf",
        Richtung::Verkauf => "Verkauf",
    }
}

fn anlass_name(a: Anlass) -> &'static str {
    match a {
        Anlass::Umschichtung => "Umschichtung",
        Anlass::AusstiegPositivliste => "AusstiegPositivliste",
        Anlass::Abbau => "Abbau",
        Anlass::Notbremse => "Notbremse",
        Anlass::Stopp => "Stopp",
        Anlass::FristOhneBegruendung => "FristOhneBegruendung",
    }
}

pub(crate) fn beleg_text(topf: TopfId, g: &GenehmigterAuftrag) -> String {
    let a = g.auftrag();
    auftrag_text(&AuftragsBeleg {
        nr: a.kennung.nr(),
        topf,
        isin: a.isin.clone(),
        richtung: a.richtung,
        stueck: a.stueck,
        limit: a.limit,
        anlass: g.beleg().anlass,
        stopp_bp: g.stopp_bp(),
        nach: g.nach().to_vec(),
    })
}

pub(crate) fn auftrag_text(b: &AuftragsBeleg) -> String {
    let text = format!(
        "{};{};{};{};{};{};{}",
        b.nr,
        topf_name(b.topf),
        b.isin,
        richtung_name(b.richtung),
        b.stueck,
        b.limit.0,
        anlass_name(b.anlass)
    );
    let text = if let Some(bp) = b.stopp_bp {
        format!("{text};{bp}")
    } else {
        text
    };
    if b.nach.is_empty() {
        text
    } else {
        let nach: Vec<String> = b.nach.iter().map(u64::to_string).collect();
        format!("{text};nach={}", nach.join("+"))
    }
}

/// `nach=3+4`: aufsteigende, verschiedene Kennungen.
fn lies_nach(t: &str) -> Option<Vec<u64>> {
    let liste = t.strip_prefix("nach=")?;
    let nach: Vec<u64> = liste
        .split('+')
        .map(|n| n.parse().ok())
        .collect::<Option<_>>()?;
    (!nach.is_empty() && nach.windows(2).all(|w| w.first() < w.get(1))).then_some(nach)
}

fn lies_auftrag(inhalt: &str) -> Option<AuftragsBeleg> {
    let teile: Vec<&str> = inhalt.split(';').collect();
    let [nr, topf, isin, richtung, stueck, limit, anlass, rest @ ..] = teile.as_slice() else {
        return None;
    };
    let richtung = [Richtung::Kauf, Richtung::Verkauf]
        .into_iter()
        .find(|r| richtung_name(*r) == *richtung)?;
    let anlass = [
        Anlass::Umschichtung,
        Anlass::AusstiegPositivliste,
        Anlass::Abbau,
        Anlass::Notbremse,
        Anlass::Stopp,
        Anlass::FristOhneBegruendung,
    ]
    .into_iter()
    .find(|a| anlass_name(*a) == *anlass)?;
    let stueck: u64 = stueck.parse().ok()?;
    let limit = Cent(limit.parse().ok()?);
    if stueck == 0 || limit.0 <= 0 {
        return None;
    }
    let (rest, nach) = match rest {
        [vorne @ .., letzt] if letzt.starts_with("nach=") && richtung == Richtung::Kauf => {
            (vorne, lies_nach(letzt)?)
        }
        _ => (rest, Vec::new()),
    };
    let stopp_bp = match rest {
        [] => None,
        [bp] if richtung == Richtung::Kauf => {
            let bp: i64 = bp.parse().ok()?;
            (1..depot_kern::BP_GANZ).contains(&bp).then_some(bp)?.into()
        }
        _ => return None,
    };
    if matches!(anlass, Anlass::Stopp | Anlass::FristOhneBegruendung)
        && richtung != Richtung::Verkauf
    {
        return None;
    }
    Some(AuftragsBeleg {
        nr: nr.parse().ok()?,
        topf: lies_topf_name(topf)?,
        isin: Isin::neu(isin).ok()?,
        richtung,
        stueck,
        limit,
        anlass,
        stopp_bp,
        nach,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn papierfreigabe_verlangt_zeit_ab_abschlusstag_in_frankfurt() -> Result<(), String> {
        let tag = Datum::lies("2026-11-04").ok_or("Abschlusstag")?;
        let bargeld = Cent(1_000_000);
        let topf = Topf::neu(TopfId::A, bargeld).map_err(|e| format!("{e:?}"))?;
        let mut stand = Stand {
            toepfe: BTreeMap::from([(TopfId::A, topf)]),
            abschluss: Some(Abschluss {
                datum: tag,
                hoechststand: Cent(1_500_000),
                abdruck: "a".repeat(64),
            }),
            papier_halt: Some(PapierHalt::Gesamtverlust),
            ..Stand::default()
        };
        let basis = papier_freigabe_text(tag, bargeld);
        assert!(stand.belege_papier_freigabe(&basis).is_err());
        assert!(
            stand
                .belege_papier_freigabe(&format!("{basis};2026-11-03T22:59:59Z"))
                .is_err()
        );
        assert!(stand.papier_verlustsperre_aktiv());
        stand.belege_papier_freigabe(&format!("{basis};2026-11-03T23:00:00Z"))?;
        assert!(!stand.papier_verlustsperre_aktiv());
        Ok(())
    }
}
