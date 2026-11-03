//! Was im Journal steht und was daraus folgt: Eintragsarten, ihr Textformat und der Stand, der
//! beim Oeffnen aus allen Eintraegen nachgerechnet wird.
//!
//! **Ein Zustandsraum (R20):** Jeder Eintrag wird gelesen und wieder geschrieben; nur wenn
//! dabei genau derselbe Text entsteht, gilt er. Anhaengen und Oeffnen gehen durch dieselbe
//! Pruefung. Was das Journal beim Anhaengen ablehnt, fuehrt in der Datei zu Halt.

use std::collections::BTreeMap;

use depot_kern::{Cent, Datum, Isin, Topf, TopfId};
use depot_regelwerk::{Anlass, GenehmigterAuftrag, Richtung};

use crate::kasse::KiStand;

/// Die Arten von Eintraegen. Ein Eintrag steht im Journal als `art;inhalt`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    /// `datum;text`: was in der Welt passiert oder angekuendigt ist (R40).
    Beobachtung,
    /// `topf;bargeld;isin:stueck,…`: der ganze Stand eines Topfes im Papierdepot.
    Topf,
    /// `nr;topf;isin;richtung;stueck;limit;anlass`: ein genehmigter Auftrag, bevor er den
    /// Rechner verlaesst (R12, R24).
    Auftrag,
    /// `berater;datum;zyklus;cent`: eine Buchung des KI-Kassenbuchs, geprueft nach den Regeln
    /// aus `depot_ki` (K13-02).
    Ki,
    /// Der Abdruck der KI-Grenzen, unter denen die folgenden Buchungen entstehen.
    KiGrenzen,
    /// `datum;wer;text`: warum etwas geschieht oder nicht geschieht. `wer` ist ein Berater
    /// oder eine Stelle des Rahmens; die Kaufentscheidung begruenden die Berater selbst.
    Begruendung,
    /// `datum;hoechststand`: Der Tag ist vollstaendig belegt. Ein Wiederanlauf setzt nur nach
    /// einem Abschluss fort; die Daten steigen streng.
    Abschluss,
}

impl Art {
    pub fn name(self) -> &'static str {
        match self {
            Art::Beobachtung => "beobachtung",
            Art::Topf => "topf",
            Art::Auftrag => "auftrag",
            Art::Ki => "ki",
            Art::KiGrenzen => "ki-grenzen",
            Art::Begruendung => "begruendung",
            Art::Abschluss => "abschluss",
        }
    }

    fn aus_name(name: &str) -> Option<Art> {
        [
            Art::Beobachtung,
            Art::Topf,
            Art::Auftrag,
            Art::Ki,
            Art::KiGrenzen,
            Art::Begruendung,
            Art::Abschluss,
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
}

/// Der Stand, wie er aus dem Journal folgt.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stand {
    toepfe: BTreeMap<TopfId, Topf>,
    auftraege: Vec<AuftragsBeleg>,
    beobachtungen: Vec<(Datum, String)>,
    letzte_kennung: u64,
    ki: KiStand,
    begruendungen: Vec<(Datum, String, String)>,
    abschluss: Option<(Datum, Cent)>,
    seit_abschluss: usize,
}

impl Stand {
    /// Der zuletzt belegte Stand je Topf des Papierdepots.
    pub fn toepfe(&self) -> &BTreeMap<TopfId, Topf> {
        &self.toepfe
    }

    pub fn auftraege(&self) -> &[AuftragsBeleg] {
        &self.auftraege
    }

    pub fn beobachtungen(&self) -> &[(Datum, String)] {
        &self.beobachtungen
    }

    /// Das KI-Kassenbuch.
    pub fn ki(&self) -> &KiStand {
        &self.ki
    }

    /// Begruendungen: Datum, wer, Text.
    pub fn begruendungen(&self) -> &[(Datum, String, String)] {
        &self.begruendungen
    }

    /// Der letzte Tagesabschluss: Datum und Hoechststand des Depotwerts.
    pub fn abschluss(&self) -> Option<(Datum, Cent)> {
        self.abschluss
    }

    /// Zahl der Eintraege seit dem letzten Abschluss. Mehr als null heisst: Ein Tag ist
    /// angefangen und nicht abgeschlossen.
    pub fn seit_abschluss(&self) -> usize {
        self.seit_abschluss
    }

    /// Die groesste belegte Auftragskennung; 0, wenn es noch keine gibt.
    pub fn letzte_kennung(&self) -> u64 {
        self.letzte_kennung
    }

    /// Wendet einen Eintrag an. Ein Fehler laesst den Stand unveraendert.
    pub(crate) fn anwenden(&mut self, art: Art, inhalt: &str) -> Result<(), String> {
        match art {
            Art::Beobachtung => {
                let b = lies_beobachtung(inhalt).ok_or("Beobachtung unlesbar")?;
                kanonisch(&beobachtung_text(b.0, &b.1), inhalt)?;
                self.beobachtungen.push(b);
            }
            Art::Topf => {
                let t = lies_topf(inhalt).ok_or("Topf unlesbar")?;
                kanonisch(&topf_text(&t), inhalt)?;
                self.toepfe.insert(t.id(), t);
            }
            Art::Auftrag => {
                let b = lies_auftrag(inhalt).ok_or("Auftrag unlesbar")?;
                kanonisch(&auftrag_text(&b), inhalt)?;
                let nr = b.nr;
                if nr <= self.letzte_kennung {
                    return Err(format!(
                        "Auftragskennung {nr} nicht groesser als die letzte ({})",
                        self.letzte_kennung
                    ));
                }
                self.letzte_kennung = nr;
                self.auftraege.push(b);
            }
            Art::KiGrenzen => self.ki.abdruck(inhalt)?,
            Art::Ki => self.ki.buche(inhalt)?,
            Art::Begruendung => {
                let b = lies_begruendung(inhalt).ok_or("Begruendung unlesbar")?;
                kanonisch(&begruendung_text(b.0, &b.1, &b.2), inhalt)?;
                self.begruendungen.push(b);
            }
            Art::Abschluss => {
                let (datum, hoch) = lies_abschluss(inhalt).ok_or("Abschluss unlesbar")?;
                kanonisch(&abschluss_text(datum, hoch), inhalt)?;
                if let Some((vorher, _)) = self.abschluss
                    && datum <= vorher
                {
                    return Err(format!(
                        "Abschluss {} nicht nach dem letzten ({})",
                        datum_text(datum),
                        datum_text(vorher)
                    ));
                }
                self.abschluss = Some((datum, hoch));
                self.seit_abschluss = 0;
                return Ok(());
            }
        }
        self.seit_abschluss = self.seit_abschluss.saturating_add(1);
        Ok(())
    }
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

pub(crate) fn abschluss_text(d: Datum, hoechststand: Cent) -> String {
    format!("{};{}", datum_text(d), hoechststand.0)
}

fn lies_abschluss(inhalt: &str) -> Option<(Datum, Cent)> {
    let (datum, hoch) = inhalt.split_once(';')?;
    let hoch = Cent(hoch.parse().ok()?);
    (hoch.0 > 0).then_some(())?;
    Some((lies_datum(datum)?, hoch))
}

fn topf_name(t: TopfId) -> &'static str {
    match t {
        TopfId::A => "A",
        TopfId::B => "B",
    }
}

fn lies_topf_name(t: &str) -> Option<TopfId> {
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
    })
}

fn auftrag_text(b: &AuftragsBeleg) -> String {
    format!(
        "{};{};{};{};{};{};{}",
        b.nr,
        topf_name(b.topf),
        b.isin,
        richtung_name(b.richtung),
        b.stueck,
        b.limit.0,
        anlass_name(b.anlass)
    )
}

fn lies_auftrag(inhalt: &str) -> Option<AuftragsBeleg> {
    let teile: Vec<&str> = inhalt.split(';').collect();
    let [nr, topf, isin, richtung, stueck, limit, anlass] = teile.as_slice() else {
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
    ]
    .into_iter()
    .find(|a| anlass_name(*a) == *anlass)?;
    let stueck: u64 = stueck.parse().ok()?;
    let limit = Cent(limit.parse().ok()?);
    if stueck == 0 || limit.0 <= 0 {
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
    })
}
