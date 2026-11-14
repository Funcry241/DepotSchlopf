//! Das Papierdepot, wie es aus dem Journal folgt, bewertet mit den juengsten belegten
//! Schlusskursen (abgerundet, wie der Waechter den heutigen Wert misst).

use depot_journal::{AuftragsZustand, Stand};
use depot_kern::liste::{Wert, als_universum};
use depot_kern::{Cent, Datum, Grenzen, Isin, Rundung, Topf, TopfId};
use depot_regelwerk::{Anlass, Richtung};
use depot_sim::tageskurse;

/// Eine Position eines Topfes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Position {
    pub isin: String,
    pub name: String,
    pub stueck: u64,
    /// Wert zum juengsten Schlusskurs, abgerundet; `None` ohne Kurs.
    pub wert: Option<Cent>,
    pub kurs: Option<depot_kern::Preis>,
    pub einstand: Option<Cent>,
    pub gv: Option<Cent>,
    pub gv_bp: Option<i64>,
}

/// Ein Topf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopfLage {
    pub topf: TopfId,
    pub bargeld: Cent,
    pub frei: Option<Cent>,
    pub vorgemerkt: Option<Cent>,
    pub nicht_gutgeschrieben: Option<Cent>,
    pub naechste_valuta: Option<Datum>,
    pub wert: Option<Cent>,
    pub positionen: Vec<Position>,
}

/// Ein Auftrag zur Anzeige.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuftragZeile {
    pub nr: u64,
    pub topf: TopfId,
    pub name: String,
    pub richtung: &'static str,
    pub stueck: u64,
    pub limit: Cent,
    pub anlass: &'static str,
    pub zustand: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tagesergebnis {
    OhneKurstag,
    OhneVortag,
    NichtBewertbar(Datum),
    Belegt {
        tag: Datum,
        betrag: Cent,
        bp: Option<i64>,
    },
}

/// Das Papierdepot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepotLage {
    /// Handelstag der Kurse, mit denen bewertet ist.
    pub kurse_vom: Option<Datum>,
    pub toepfe: Vec<TopfLage>,
    pub gesamt: Option<Cent>,
    pub tagesergebnis: Tagesergebnis,
    pub letzter_abschluss: Option<Datum>,
    pub hoechststand: Option<Cent>,
    /// Neueste zuerst.
    pub auftraege: Vec<AuftragZeile>,
    pub ausfuehrungen: Vec<(
        TopfId,
        crate::ausfuehrung::Kennzahlen,
        crate::ausfuehrung::Kennzahlen,
    )>,
    /// Vergleich seit Start (#118); `None` vor dem ersten abgeschlossenen Papiertag.
    pub vergleich: Option<crate::vergleich::VergleichLage>,
}

/// So viele Auftraege zeigt das Lagebild hoechstens.
pub const AUFTRAEGE_MAX: usize = 20;

pub(crate) fn name(werte: &[Wert], isin: &str) -> String {
    werte
        .iter()
        .find(|w| w.isin.als_str() == isin)
        .map_or_else(|| isin.to_string(), |w| w.name.clone())
}

#[derive(Default)]
pub(crate) struct Einstand {
    pub(crate) stueck: u64,
    pub(crate) kosten: Cent,
}

impl Einstand {
    /// Bucht eine Ausführung und liefert für Verkäufe den Nettoertrag nach Einstand und Gebühren.
    pub(crate) fn buche(
        &mut self,
        richtung: Richtung,
        a: &depot_journal::Ausfuehrung,
    ) -> Option<Cent> {
        let verbindlich = a.kurs.mal_stueck(a.stueck)?;
        match richtung {
            Richtung::Kauf => {
                self.stueck = self.stueck.checked_add(a.stueck)?;
                self.kosten = self
                    .kosten
                    .checked_add(verbindlich)?
                    .checked_add(a.gebuehr)?;
                Some(Cent::NULL)
            }
            Richtung::Verkauf => {
                let rest = self.stueck.checked_sub(a.stueck)?;
                let stueck_vorher = i128::from(self.stueck);
                let zaehler = i128::from(self.kosten.0).checked_mul(i128::from(rest))?;
                let ab = zaehler.checked_div(stueck_vorher)?;
                let auf = i128::from(zaehler.checked_rem(stueck_vorher)? != 0);
                let restkosten = Cent(i64::try_from(ab.checked_add(auf)?).ok()?);
                let netto = verbindlich
                    .checked_sub(a.gebuehr)?
                    .checked_sub(self.kosten.checked_sub(restkosten)?)?;
                self.kosten = restkosten;
                self.stueck = rest;
                Some(netto)
            }
        }
    }
}

fn einstand(s: &Stand, topf: TopfId, isin: &Isin, gehalten: u64) -> Option<Cent> {
    let mut einstand = Einstand::default();
    for (auftrag, a) in s
        .ausfuehrungsfolge()
        .filter(|(b, _)| b.topf == topf && &b.isin == isin)
    {
        einstand.buche(auftrag.richtung, a)?;
    }
    (einstand.stueck == gehalten).then_some(einstand.kosten)
}

fn topf_lage(s: &Stand, t: &Topf, kurse: Option<&depot_kern::Kurse>, werte: &[Wert]) -> TopfLage {
    let positionen = t
        .positionen()
        .iter()
        .map(|(isin, stueck)| {
            let kurs = kurse.and_then(|k| k.kurs(isin));
            let wert = kurs.and_then(|p| p.mal_stueck(*stueck, Rundung::Ab));
            let einstand = einstand(s, t.id(), isin, *stueck);
            Position {
                isin: isin.als_str().to_string(),
                name: name(werte, isin.als_str()),
                stueck: *stueck,
                kurs,
                wert,
                einstand,
                gv: wert
                    .zip(einstand)
                    .and_then(|(wert, einstand)| wert.checked_sub(einstand)),
                gv_bp: einstand
                    .zip(wert)
                    .and_then(|(einstand, wert)| crate::vergleich::rendite_bp(einstand, wert)),
            }
        })
        .collect();
    let frei = depot_journal::konto::verfuegbar(&Grenzen::standard(), s, t.id());
    let nicht_gutgeschrieben = s.nicht_gutgeschrieben(t.id());
    TopfLage {
        topf: t.id(),
        bargeld: t.bargeld(),
        frei,
        vorgemerkt: frei
            .zip(nicht_gutgeschrieben)
            .and_then(|(frei, ausstehend)| t.bargeld().checked_sub(frei)?.checked_sub(ausstehend)),
        nicht_gutgeschrieben,
        naechste_valuta: s.naechste_valuta(t.id()),
        wert: kurse.and_then(|k| t.wert_gerundet(k, Rundung::Ab)),
        positionen,
    }
}

fn tagesergebnis(
    s: &Stand,
    werte: &[Wert],
    kurstag: Option<Datum>,
    gesamt: Option<Cent>,
) -> Tagesergebnis {
    let Some(tag) = kurstag else {
        return Tagesergebnis::OhneKurstag;
    };
    if !s.tagesschluesse().iter().any(|(d, _)| *d < tag) {
        return Tagesergebnis::OhneVortag;
    }
    if let Some((gesamt, vortag)) = gesamt.zip(letzter_tagesschluss(s, werte, tag))
        && let Some(betrag) = gesamt.checked_sub(vortag)
    {
        return Tagesergebnis::Belegt {
            tag,
            betrag,
            bp: crate::vergleich::rendite_bp(vortag, gesamt),
        };
    }
    Tagesergebnis::NichtBewertbar(tag)
}

pub(crate) fn depot_lage(s: &Stand, werte: &[Wert]) -> DepotLage {
    let kurse = tageskurse(s, &als_universum(werte)).ok();
    let toepfe: Vec<TopfLage> = s
        .toepfe()
        .values()
        .map(|t| topf_lage(s, t, kurse.as_ref().map(|k| &k.heute), werte))
        .collect();
    let gesamt = toepfe
        .iter()
        .try_fold(Cent::NULL, |summe, t| summe.checked_add(t.wert?))
        .filter(|_| !toepfe.is_empty());
    let auftraege = s
        .auftraege()
        .iter()
        .rev()
        .take(AUFTRAEGE_MAX)
        .map(|a| auftrag_zeile(a, werte))
        .collect();
    let kurse_vom = kurse.map(|k| k.handelstag);
    let ausfuehrungen = kurse_vom
        .or_else(|| s.abschluss().map(|a| a.datum))
        .map_or_else(Vec::new, |tag| {
            let buch = crate::ausfuehrung::ausfuehrungsbuch(s);
            let ab = crate::ausfuehrung::letzte_20_tage(tag);
            s.toepfe()
                .keys()
                .map(|topf| {
                    (
                        *topf,
                        crate::ausfuehrung::je_ki(&buch, *topf, Richtung::Kauf, ab),
                        crate::ausfuehrung::je_ki(&buch, *topf, Richtung::Verkauf, ab),
                    )
                })
                .collect()
        });
    DepotLage {
        kurse_vom,
        tagesergebnis: tagesergebnis(s, werte, kurse_vom, gesamt),
        toepfe,
        gesamt,
        letzter_abschluss: s.abschluss().map(|a| a.datum),
        hoechststand: s.historischer_hoechststand(),
        auftraege,
        ausfuehrungen,
        vergleich: crate::vergleich::vergleich_lage(s, werte),
    }
}

pub fn auftrag_zeile(a: &depot_journal::AuftragsLage, werte: &[Wert]) -> AuftragZeile {
    AuftragZeile {
        nr: a.beleg.nr,
        topf: a.beleg.topf,
        name: name(werte, a.beleg.isin.als_str()),
        richtung: match a.beleg.richtung {
            Richtung::Kauf => "Kauf",
            Richtung::Verkauf => "Verkauf",
        },
        stueck: a.beleg.stueck,
        limit: a.beleg.limit,
        anlass: match a.beleg.anlass {
            Anlass::Umschichtung => "Vorschlag",
            Anlass::AusstiegPositivliste => "Ausstieg Positivliste",
            Anlass::Abbau => "Abbau",
            Anlass::Notbremse => "Notbremse",
        },
        zustand: zustand_text(a.zustand).into(),
    }
}

pub fn zustand_text(zustand: AuftragsZustand) -> &'static str {
    use AuftragsZustand as Z;
    match zustand {
        Z::Reserviert => "vorgemerkt, noch nicht gesendet",
        Z::Gesendet => "gesendet",
        Z::Unbekannt => "Versand ungeklärt, wird abgeglichen",
        Z::Angenommen => "liegt beim Papiermakler",
        Z::Teilgefuellt => "teilweise ausgeführt",
        Z::Gefuellt => "ausgeführt",
        Z::Abgelehnt => "abgelehnt",
        Z::Storniert => "storniert",
        Z::Verfallen => "verfallen",
        Z::Verworfen => "nicht gesendet, verworfen",
        Z::Alt => "ausgeführt (altes Journal)",
    }
}

/// Belegter Wert des letzten abgeschlossenen Papiertags zum Schlusskurs dieses Tags.
pub fn letzter_tagesschluss(s: &Stand, werte: &[Wert], vor: Datum) -> Option<Cent> {
    let (tag, toepfe) = s.tagesschluesse().iter().rfind(|(tag, _)| *tag < vor)?;
    let kurse = depot_sim::tageskurse_am(s, &als_universum(werte), *tag).ok()?;
    toepfe.values().try_fold(Cent::NULL, |summe, topf| {
        summe.checked_add(topf.wert_gerundet(&kurse.heute, Rundung::Ab)?)
    })
}
