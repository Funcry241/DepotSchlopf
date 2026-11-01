//! Aus einem KI-Vorschlag werden genehmigte Auftraege – oder keine.
//!
//! Reine Funktion: kein Netz, keine Dateien, keine Uhr. Gleiche Eingabe, gleiche Ausgabe.

use std::collections::{BTreeMap, BTreeSet};

use depot_kern::{BP_GANZ, Cent, Grenzen, Isin, Kurse, Topf, Universum, Vorschlag};

use crate::auftrag::{Anlass, Auftrag, AuftragsKennung, GenehmigterAuftrag, RegelBeleg, Richtung};

/// Alles, was das Regelwerk fuer einen Topf in einem Zyklus braucht.
#[derive(Debug)]
pub struct Eingabe<'a> {
    pub grenzen: &'a Grenzen,
    pub universum: &'a Universum,
    pub topf: &'a Topf,
    pub kurse: &'a Kurse,
    pub vorschlag: Option<&'a Vorschlag>,
    pub zyklus: u32,
    /// Kill-Switch, Tagesverlust, Datenfehler oder leeres Budget: dann gibt es keine Auftraege.
    pub halt: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerwerfGrund {
    UnbekannterWert(Isin),
    NegativesGewicht(Isin),
    ZuVieleWerte(usize),
    SummeUeber100(i64),
    KursFehlt(Isin),
}

/// Protokoll jeder Entscheidung des Regelwerks; landet im Journal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Befund {
    Halt,
    KeinVorschlag,
    VorschlagVerworfen(VerwerfGrund),
    GewichtGekuerzt {
        isin: Isin,
        von_bp: i64,
        auf_bp: i64,
    },
    UnterSchwelle(Isin),
    UnterMindestorder(Isin),
    UmschlagGekuerzt {
        geplant: Cent,
        erlaubt: Cent,
    },
    OrderLimitErreicht(Isin),
    BargeldReichtNicht(Isin),
    NichtMehrHandelbarVerkauft(Isin),
    Rechenfehler,
}

/// Verkaeufe zuerst. Die Kaeufe darf die Auftragsstelle erst senden, wenn alle Verkaeufe
/// ausgefuehrt sind; sonst verfallen sie. Ihr Budget rechnet nur mit dem garantierten Erloes.
#[derive(Debug, Default)]
pub struct Entscheidung {
    pub verkaeufe: Vec<GenehmigterAuftrag>,
    pub kaeufe: Vec<GenehmigterAuftrag>,
    pub befunde: Vec<Befund>,
}

impl Entscheidung {
    fn nur(befund: Befund) -> Entscheidung {
        Entscheidung {
            befunde: vec![befund],
            ..Entscheidung::default()
        }
    }
}

#[derive(Debug)]
struct Kandidat {
    isin: Isin,
    richtung: Richtung,
    stueck: u64,
    kurs: Cent,
    komplett: bool,
}

impl Kandidat {
    fn volumen(&self) -> Option<Cent> {
        self.kurs.mal_stueck(self.stueck)
    }
}

pub fn entscheide(e: &Eingabe<'_>) -> Entscheidung {
    if e.halt {
        return Entscheidung::nur(Befund::Halt);
    }
    let Some(vorschlag) = e.vorschlag else {
        return Entscheidung::nur(Befund::KeinVorschlag);
    };
    let mut befunde = Vec::new();
    let ziele = match pruefe_vorschlag(e, vorschlag, &mut befunde) {
        Ok(z) => z,
        Err(grund) => return Entscheidung::nur(Befund::VorschlagVerworfen(grund)),
    };
    match plane(e, &ziele, &mut befunde) {
        Some(mut ent) => {
            befunde.append(&mut ent.befunde);
            ent.befunde = befunde;
            ent
        }
        None => Entscheidung::nur(Befund::Rechenfehler),
    }
}

/// Verwirft den ganzen Vorschlag bei erfundenen Werten oder unmoeglichen Gewichten; kuerzt zu grosse Gewichte.
fn pruefe_vorschlag(
    e: &Eingabe<'_>,
    v: &Vorschlag,
    befunde: &mut Vec<Befund>,
) -> Result<BTreeMap<Isin, i64>, VerwerfGrund> {
    let mut ziele = BTreeMap::new();
    let mut summe: i64 = 0;
    for (isin, gewicht) in &v.zielgewichte_bp {
        if *gewicht < 0 {
            return Err(VerwerfGrund::NegativesGewicht(isin.clone()));
        }
        if *gewicht == 0 {
            continue;
        }
        if !e.universum.handelbar(isin, e.grenzen) {
            return Err(VerwerfGrund::UnbekannterWert(isin.clone()));
        }
        if e.kurse.kurs(isin).is_none() {
            return Err(VerwerfGrund::KursFehlt(isin.clone()));
        }
        summe = summe.saturating_add(*gewicht);
        ziele.insert(isin.clone(), *gewicht);
    }
    if ziele.len() > e.grenzen.max_werte() {
        return Err(VerwerfGrund::ZuVieleWerte(ziele.len()));
    }
    if summe > BP_GANZ {
        return Err(VerwerfGrund::SummeUeber100(summe));
    }
    for isin in e.topf.positionen().keys() {
        if e.kurse.kurs(isin).is_none() {
            return Err(VerwerfGrund::KursFehlt(isin.clone()));
        }
    }
    for (isin, gewicht) in &mut ziele {
        if *gewicht > e.grenzen.max_anteil_bp() {
            befunde.push(Befund::GewichtGekuerzt {
                isin: isin.clone(),
                von_bp: *gewicht,
                auf_bp: e.grenzen.max_anteil_bp(),
            });
            *gewicht = e.grenzen.max_anteil_bp();
        }
    }
    Ok(ziele)
}

fn plane(
    e: &Eingabe<'_>,
    ziele: &BTreeMap<Isin, i64>,
    befunde: &mut Vec<Befund>,
) -> Option<Entscheidung> {
    let g = e.grenzen;
    let topfwert = e.topf.wert(e.kurse)?;
    if topfwert.0 <= 0 {
        return Some(Entscheidung::nur(Befund::Rechenfehler));
    }

    let mut kandidaten = sammle_kandidaten(e, ziele, topfwert, befunde)?;
    begrenze_umschlag(g, topfwert, &mut kandidaten, befunde)?;

    // Groesste Orders zuerst, hoechstens max_orders_je_topf.
    kandidaten.sort_by(|a, b| {
        b.volumen()
            .cmp(&a.volumen())
            .then_with(|| a.isin.cmp(&b.isin))
    });
    let mut behalten = Vec::new();
    for k in kandidaten {
        if behalten.len() < g.max_orders_je_topf() {
            behalten.push(k);
        } else {
            befunde.push(Befund::OrderLimitErreicht(k.isin));
        }
    }

    let abdruck = g.fingerabdruck();
    let beleg = || RegelBeleg {
        grenzen_abdruck: abdruck.clone(),
        anlass: Anlass::Umschichtung,
    };
    let mut nr: u8 = 0;
    let mut ent = Entscheidung::default();

    // Verkaeufe: garantierter Erloes zum Limit, abzueglich Gebuehr.
    let mut budget = e.topf.bargeld();
    for k in behalten.iter().filter(|k| k.richtung == Richtung::Verkauf) {
        let limit = k
            .kurs
            .checked_sub(k.kurs.anteil_bp_ab(g.limit_abstand_bp())?)?;
        let erloes = limit.mal_stueck(k.stueck)?;
        budget = budget
            .checked_add(erloes)?
            .checked_sub(g.gebuehren().fuer(erloes)?)?;
        nr = nr.checked_add(1)?;
        ent.verkaeufe.push(GenehmigterAuftrag::neu(
            Auftrag {
                kennung: AuftragsKennung {
                    zyklus: e.zyklus,
                    topf: e.topf.id(),
                    nr,
                },
                isin: k.isin.clone(),
                richtung: Richtung::Verkauf,
                stueck: k.stueck,
                limit,
            },
            beleg(),
        ));
    }

    // Kaeufe: Kosten zum Limit plus Gebuehr muessen ins Budget passen, sonst kleiner oder gar nicht.
    for k in behalten.iter().filter(|k| k.richtung == Richtung::Kauf) {
        let limit = k
            .kurs
            .checked_add(k.kurs.anteil_bp_auf(g.limit_abstand_bp())?)?;
        let stueck = leistbare_stueck(g, limit, k.stueck, budget)?;
        let volumen = limit.mal_stueck(stueck)?;
        if stueck == 0 || volumen < g.min_order() {
            befunde.push(Befund::BargeldReichtNicht(k.isin.clone()));
            continue;
        }
        budget = budget.checked_sub(volumen.checked_add(g.gebuehren().fuer(volumen)?)?)?;
        nr = nr.checked_add(1)?;
        ent.kaeufe.push(GenehmigterAuftrag::neu(
            Auftrag {
                kennung: AuftragsKennung {
                    zyklus: e.zyklus,
                    topf: e.topf.id(),
                    nr,
                },
                isin: k.isin.clone(),
                richtung: Richtung::Kauf,
                stueck,
                limit,
            },
            beleg(),
        ));
    }
    Some(ent)
}

fn sammle_kandidaten(
    e: &Eingabe<'_>,
    ziele: &BTreeMap<Isin, i64>,
    topfwert: Cent,
    befunde: &mut Vec<Befund>,
) -> Option<Vec<Kandidat>> {
    let g = e.grenzen;
    let alle: BTreeSet<&Isin> = e.topf.positionen().keys().chain(ziele.keys()).collect();
    let mut kandidaten = Vec::new();
    for isin in alle {
        let kurs = e.kurse.kurs(isin)?;
        let gehalten = e.topf.stueck(isin);

        // Fliegt ein Wert aus der Positivliste, wird er komplett verkauft.
        if !e.universum.handelbar(isin, g) {
            if gehalten > 0 {
                befunde.push(Befund::NichtMehrHandelbarVerkauft(isin.clone()));
                kandidaten.push(Kandidat {
                    isin: isin.clone(),
                    richtung: Richtung::Verkauf,
                    stueck: gehalten,
                    kurs,
                    komplett: true,
                });
            }
            continue;
        }

        let ziel_bp = ziele.get(isin).copied().unwrap_or(0);
        let ist_wert = kurs.mal_stueck(gehalten)?;
        let ziel_wert = topfwert.anteil_bp_ab(ziel_bp)?;
        let ist_bp = ist_wert.in_bp_von(topfwert)?;
        let abweichung = ziel_bp.checked_sub(ist_bp)?;
        if abweichung.checked_abs()? < g.min_aenderung_bp() {
            if abweichung != 0 {
                befunde.push(Befund::UnterSchwelle(isin.clone()));
            }
            continue;
        }
        let k = if ziel_wert < ist_wert {
            let komplett = ziel_bp == 0;
            let stueck = if komplett {
                gehalten
            } else {
                stueck_fuer(ist_wert.checked_sub(ziel_wert)?, kurs)?.min(gehalten)
            };
            Kandidat {
                isin: isin.clone(),
                richtung: Richtung::Verkauf,
                stueck,
                kurs,
                komplett,
            }
        } else {
            let stueck = stueck_fuer(ziel_wert.checked_sub(ist_wert)?, kurs)?;
            Kandidat {
                isin: isin.clone(),
                richtung: Richtung::Kauf,
                stueck,
                kurs,
                komplett: false,
            }
        };
        if k.stueck == 0 || (k.volumen()? < g.min_order() && !k.komplett) {
            befunde.push(Befund::UnterMindestorder(isin.clone()));
            continue;
        }
        kandidaten.push(k);
    }
    Some(kandidaten)
}

/// Kuerzt alle Orders anteilig, wenn zusammen mehr als `max_umschlag_bp` des Topfes bewegt wuerde.
fn begrenze_umschlag(
    g: &Grenzen,
    topfwert: Cent,
    kandidaten: &mut Vec<Kandidat>,
    befunde: &mut Vec<Befund>,
) -> Option<()> {
    let mut geplant = Cent::NULL;
    for k in kandidaten.iter() {
        geplant = geplant.checked_add(k.volumen()?)?;
    }
    let erlaubt = topfwert.anteil_bp_ab(g.max_umschlag_bp())?;
    if geplant <= erlaubt || geplant.0 <= 0 {
        return Some(());
    }
    befunde.push(Befund::UmschlagGekuerzt { geplant, erlaubt });
    let mut neu = Vec::new();
    for mut k in kandidaten.drain(..) {
        let s = u128::from(k.stueck)
            .checked_mul(u128::try_from(erlaubt.0).ok()?)?
            .checked_div(u128::try_from(geplant.0).ok()?)?;
        k.stueck = u64::try_from(s).ok()?;
        k.komplett = false;
        if k.stueck == 0 || k.volumen()? < g.min_order() {
            befunde.push(Befund::UnterMindestorder(k.isin.clone()));
        } else {
            neu.push(k);
        }
    }
    *kandidaten = neu;
    Some(())
}

/// Ganze Stuecke fuer einen Betrag, abgerundet.
fn stueck_fuer(betrag: Cent, kurs: Cent) -> Option<u64> {
    if kurs.0 <= 0 || betrag.0 <= 0 {
        return Some(0);
    }
    u64::try_from(betrag.0.checked_div(kurs.0)?).ok()
}

/// Hoechstens `gewuenscht` Stueck, so dass Volumen plus Gebuehr ins Budget passt.
fn leistbare_stueck(g: &Grenzen, limit: Cent, gewuenscht: u64, budget: Cent) -> Option<u64> {
    let kosten = |s: u64| -> Option<Cent> {
        let v = limit.mal_stueck(s)?;
        v.checked_add(g.gebuehren().fuer(v)?)
    };
    if kosten(gewuenscht)? <= budget {
        return Some(gewuenscht);
    }
    // Startwert grob ueber Prozentgebuehr und Fixanteil, dann schrittweise nach unten.
    let je_stueck = limit.checked_add(limit.anteil_bp_auf(g.gebuehren().prozent_bp())?)?;
    let fix = g
        .gebuehren()
        .minimum()
        .checked_add(g.gebuehren().boersengebuehr())?;
    let frei = budget.checked_sub(fix)?;
    let mut s = stueck_fuer(frei, je_stueck)?.min(gewuenscht);
    while s > 0 && kosten(s)? > budget {
        s = s.checked_sub(1)?;
    }
    Some(s)
}
