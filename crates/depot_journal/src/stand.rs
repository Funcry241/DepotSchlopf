//! Was im Journal steht und was daraus folgt: Eintragsarten, ihr Textformat und der Stand, der
//! beim Oeffnen aus allen Eintraegen nachgerechnet wird.
//!
//! **Ein Zustandsraum (R20):** Jeder Eintrag wird gelesen und wieder geschrieben; nur wenn
//! dabei genau derselbe Text entsteht, gilt er. Anhaengen und Oeffnen gehen durch dieselbe
//! Pruefung. Was das Journal beim Anhaengen ablehnt, fuehrt in der Datei zu Halt.

use std::collections::BTreeMap;

use depot_kern::{Cent, Datum, Isin, Topf, TopfId, Zeitpunkt};
use depot_regelwerk::{Anlass, GenehmigterAuftrag, Richtung};

use crate::auftrag::{AuftragsLage, Meldung, lies_meldung, meldung_text};

use crate::kasse::KiStand;
use crate::kursbeleg::{KursBeleg, kursbeleg_text, lies_kursbeleg};
use crate::terminbeleg::{TerminBeleg, lies_terminbeleg, terminbeleg_text};

/// Die Arten von Eintraegen. Ein Eintrag steht im Journal als `art;inhalt`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    /// `datum;text`: was in der Welt passiert oder angekuendigt ist (R40).
    Beobachtung,
    /// `topf;bargeld;isin:stueck,…`: der ganze Stand eines Topfes im Papierdepot.
    Topf,
    /// `quelle;herkunft;abruf;roh;isin;preis;zeitpunkt`: ein Kurs von aussen, belegt bevor er
    /// verwendet wird (`kursbeleg`). Nur im Format 2; die Abrufe fallen nie zurueck.
    Kurs,
    /// `quelle;herkunft;abruf;roh;kennung;verlaesslichkeit;zeit;betroffen;titel`: ein Termin
    /// von aussen (`terminbeleg`, R39). Nur im Format 2; die Abrufe fallen nie zurueck.
    Termin,
    /// `nr;topf;isin;richtung;stueck;limit;anlass`: ein genehmigter Auftrag, bevor er den
    /// Rechner verlaesst (R12, R24). Damit ist er reserviert (R37).
    Auftrag,
    /// `nr;meldung`: der naechste Zustand eines Auftrags (R37, Kap. 10). Eine Ausfuehrung
    /// steht als `nr;ausfuehrung;stueck;kurs;gebuehr` und bucht sie im Topf.
    Zustand,
    /// `berater;datum;zyklus;cent`: eine Buchung des KI-Kassenbuchs, geprueft nach den Regeln
    /// aus `depot_ki` (K13-02).
    Ki,
    /// Der Abdruck der KI-Grenzen, unter denen die folgenden Buchungen entstehen.
    KiGrenzen,
    /// `datum;wer;text`: warum etwas geschieht oder nicht geschieht. `wer` ist ein Berater
    /// oder eine Stelle des Rahmens; die Kaufentscheidung begruenden die Berater selbst.
    Begruendung,
    /// `2`: Umstellung eines Journals im Format 1 auf Format 2 (K16-02). Nur einmal, nur ohne
    /// angefangenen Tag; danach gelten Auftragszustaende und Deckung.
    Format,
    /// `datum;hoechststand;abdruck`: Der Tag ist vollstaendig belegt. `abdruck` bindet ihn an
    /// seine ganze Vorgeschichte (64 Hex-Zeichen). Ein Wiederanlauf setzt nur nach einem
    /// Abschluss fort; die Daten steigen streng.
    Abschluss,
}

impl Art {
    pub fn name(self) -> &'static str {
        match self {
            Art::Beobachtung => "beobachtung",
            Art::Topf => "topf",
            Art::Kurs => "kurs",
            Art::Termin => "termin",
            Art::Auftrag => "auftrag",
            Art::Zustand => "zustand",
            Art::Ki => "ki",
            Art::KiGrenzen => "ki-grenzen",
            Art::Begruendung => "begruendung",
            Art::Abschluss => "abschluss",
            Art::Format => "format",
        }
    }

    fn aus_name(name: &str) -> Option<Art> {
        [
            Art::Beobachtung,
            Art::Topf,
            Art::Kurs,
            Art::Termin,
            Art::Auftrag,
            Art::Zustand,
            Art::Ki,
            Art::KiGrenzen,
            Art::Begruendung,
            Art::Abschluss,
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

/// Format 1: Journale bis PR #15. Ein Auftrag darin ist mit seinem Ergebnis im naechsten Topf
/// gebucht und hat keine Zustaende.
pub const FORMAT_ALT: u8 = 1;
/// Format 2: Auftraege mit Zustandsmaschine und Deckung (R37).
pub const FORMAT: u8 = 2;

/// Der Stand, wie er aus dem Journal folgt.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stand {
    format: u8,
    toepfe: BTreeMap<TopfId, Topf>,
    auftraege: Vec<AuftragsLage>,
    beobachtungen: Vec<(Datum, String)>,
    kurse: Vec<KursBeleg>,
    termine: Vec<TerminBeleg>,
    letzte_kennung: u64,
    ki: KiStand,
    begruendungen: Vec<(Datum, String, String)>,
    abschluss: Option<Abschluss>,
    seit_abschluss: usize,
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

    /// Der zuletzt belegte Stand je Topf des Papierdepots.
    pub fn toepfe(&self) -> &BTreeMap<TopfId, Topf> {
        &self.toepfe
    }

    /// Alle Auftraege mit ihrem Zustand, in der Reihenfolge ihrer Kennungen.
    pub fn auftraege(&self) -> &[AuftragsLage] {
        &self.auftraege
    }

    /// Der Auftrag mit dieser Kennung.
    pub fn auftrag(&self, nr: u64) -> Option<&AuftragsLage> {
        self.auftraege.iter().find(|l| l.beleg.nr == nr)
    }

    /// Die Auftraege, die noch keinen Endzustand haben.
    pub fn offene_auftraege(&self) -> impl Iterator<Item = &AuftragsLage> {
        self.auftraege.iter().filter(|l| l.zustand.offen())
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

    /// Der letzte Abruf eines belegten Kurses oder Termins. Kein Abruf faellt dahinter zurueck.
    fn letzter_abruf(&self) -> Option<Zeitpunkt> {
        let k = self.kurse.last().map(KursBeleg::abruf);
        let t = self.termine.last().map(TerminBeleg::abruf);
        k.max(t)
    }

    /// Das KI-Kassenbuch.
    pub fn ki(&self) -> &KiStand {
        &self.ki
    }

    /// Begruendungen: Datum, wer, Text.
    pub fn begruendungen(&self) -> &[(Datum, String, String)] {
        &self.begruendungen
    }

    /// Der letzte Tagesabschluss.
    pub fn abschluss(&self) -> Option<&Abschluss> {
        self.abschluss.as_ref()
    }

    /// Zahl der Eintraege seit dem letzten Abschluss, ohne Kurs- und Terminbelege. Mehr als
    /// null heisst: Ein Tag ist angefangen und nicht abgeschlossen.
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
                self.pruefe_deckung(&t, None)?;
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
                if self.format == FORMAT_ALT {
                    // Format 1: Das Ergebnis steht im naechsten Topf; der Auftrag bindet nichts.
                    self.letzte_kennung = nr;
                    self.auftraege.push(AuftragsLage::alt(b));
                    self.seit_abschluss = self.seit_abschluss.saturating_add(1);
                    return Ok(());
                }
                let topf = self
                    .toepfe
                    .get(&b.topf)
                    .ok_or_else(|| format!("Auftrag {nr}: Topf {:?} fehlt", b.topf))?;
                let neu = AuftragsLage::reserviert(b);
                self.pruefe_deckung(topf, Some(&neu))?;
                self.letzte_kennung = nr;
                self.auftraege.push(neu);
            }
            Art::Zustand => {
                let (nr, m) = lies_meldung(inhalt).ok_or("Zustand unlesbar")?;
                kanonisch(&meldung_text(nr, &m), inhalt)?;
                self.melde(nr, &m)?;
            }
            Art::KiGrenzen => self.ki.abdruck(inhalt)?,
            Art::Ki => self.ki.buche(inhalt)?,
            Art::Begruendung => {
                let b = lies_begruendung(inhalt).ok_or("Begruendung unlesbar")?;
                kanonisch(&begruendung_text(b.0, &b.1, &b.2), inhalt)?;
                self.begruendungen.push(b);
            }
            Art::Format => {
                kanonisch(&FORMAT.to_string(), inhalt)?;
                if self.format != FORMAT_ALT {
                    return Err(format!("Umstellung im Format {}", self.format));
                }
                if self.seit_abschluss > 0 {
                    return Err("Umstellung nur ohne angefangenen Tag".into());
                }
                // Die Umstellung beginnt keinen Tag: Sie aendert nur, nach welchen Regeln die
                // folgenden Eintraege gelten.
                self.format = FORMAT;
                return Ok(());
            }
            Art::Abschluss => {
                let a = lies_abschluss(inhalt).ok_or("Abschluss unlesbar")?;
                kanonisch(&abschluss_text(&a), inhalt)?;
                // Tagesorders enden am Tag (Kap. 10): Ein Tag mit offenem Auftrag ist nicht
                // vollstaendig belegt.
                if let Some(l) = self.offene_auftraege().next() {
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
                self.abschluss = Some(a);
                self.seit_abschluss = 0;
                return Ok(());
            }
        }
        self.seit_abschluss = self.seit_abschluss.saturating_add(1);
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
                "Termin {} unveraendert noch einmal belegt",
                t.termin().kennung()
            ));
        }
        self.termine.push(t);
        Ok(())
    }

    /// Wendet die Meldung zu einem Auftrag an: naechster Zustand und bei einer Ausfuehrung die
    /// Buchung im Topf. Ein Fehler laesst den Stand unveraendert.
    fn melde(&mut self, nr: u64, m: &Meldung) -> Result<(), String> {
        let index = self
            .auftraege
            .iter()
            .position(|l| l.beleg.nr == nr)
            .ok_or_else(|| format!("Auftrag {nr} unbekannt"))?;
        let alt = self.auftraege.get(index).ok_or("Auftrag fehlt")?;
        let neu = alt.nach(m)?;
        let topf = self
            .toepfe
            .get(&neu.beleg.topf)
            .ok_or_else(|| format!("Auftrag {nr}: Topf fehlt"))?;
        let topf = match m {
            Meldung::Ausfuehrung(a) => buche(topf, &neu.beleg, a)?,
            _ => topf.clone(),
        };
        self.pruefe_deckung(&topf, Some(&neu))?;
        self.toepfe.insert(topf.id(), topf);
        if let Some(platz) = self.auftraege.get_mut(index) {
            *platz = neu;
        }
        Ok(())
    }

    /// Die Deckung eines Topfes (R37, Kap. 9): Das Bargeld traegt alle offenen Kaeufe am Limit,
    /// der Bestand jedes Werts alle offenen Verkaeufe. `ersatz` steht fuer den Auftrag mit
    /// seiner Kennung oder kommt neu hinzu. Was ein Kauf darueber hinaus an Gebuehr braucht,
    /// prueft der Kontowaechter vor der Reservierung (`Journal::reserviere_auftrag`).
    fn pruefe_deckung(&self, topf: &Topf, ersatz: Option<&AuftragsLage>) -> Result<(), String> {
        let ersetzt = ersatz.map(|e| e.beleg.nr);
        let offene = self
            .auftraege
            .iter()
            .filter(|l| Some(l.beleg.nr) != ersetzt)
            .chain(ersatz)
            .filter(|l| l.beleg.topf == topf.id() && l.zustand.offen());
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
                        .ok_or("Deckung: Ueberlauf")?;
                }
                Richtung::Verkauf => {
                    let summe = verkauf.entry(&l.beleg.isin).or_insert(0);
                    *summe = summe.checked_add(offen).ok_or("Deckung: Ueberlauf")?;
                }
            }
        }
        if kauf > topf.bargeld() {
            return Err(format!(
                "Deckung: offene Kaeufe {} Cent ueber dem Bargeld {} Cent",
                kauf.0,
                topf.bargeld().0
            ));
        }
        for (isin, stueck) in verkauf {
            if stueck > topf.stueck(isin) {
                return Err(format!(
                    "Deckung: {stueck} Stueck {isin} im Verkauf, {} im Bestand",
                    topf.stueck(isin)
                ));
            }
        }
        Ok(())
    }
}

/// Bucht eine Ausfuehrung im Topf: Bargeld und Bestand nie negativ.
fn buche(topf: &Topf, b: &AuftragsBeleg, a: &crate::auftrag::Ausfuehrung) -> Result<Topf, String> {
    let volumen = a
        .kurs
        .mal_stueck(a.stueck)
        .ok_or("Ausfuehrung: Ueberlauf")?;
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
        return Err(format!("Auftrag {}: Ausfuehrung nicht buchbar", b.nr));
    };
    let mut neu = Topf::neu(topf.id(), bargeld)
        .map_err(|_| format!("Auftrag {}: Bargeld wuerde negativ", b.nr))?;
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

fn lies_abschluss(inhalt: &str) -> Option<Abschluss> {
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

pub(crate) fn auftrag_text(b: &AuftragsBeleg) -> String {
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
