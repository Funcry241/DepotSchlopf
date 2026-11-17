//! Aus einem KI-Vorschlag werden genehmigte Auftraege – oder keine.
//!
//! Reine Funktion: kein Netz, keine Dateien, keine Uhr. Gleiche Eingabe, gleiche Ausgabe.
//!
//! Reihenfolge (Kapitel 8, R36): Zuerst die zwingenden Ausstiege. Sie brauchen nur einen
//! frischen Kurs und haben Vorrang vor allen freiwilligen Schranken: Halt, Orderzahl, Umschlag
//! und Mindestorder sperren sie nicht. Danach, nur ohne Halt und nur mit einem gueltigen
//! Vorschlag des zustaendigen Beraters, die freiwilligen Auftraege. Jede Geld- und Mengengrenze
//! wird am Limit mit der hoechstmoeglichen Gebuehr gerechnet (R21).

use std::collections::{BTreeMap, BTreeSet};

use depot_kern::{
    BP_GANZ, BeraterId, Cent, Grenzen, Herkunft, Isin, Kurse, Parkwunsch, Preis, Rundung, Topf,
    TopfId, Universum, Vorschlag,
};

use crate::auftrag::{Anlass, Auftrag, GenehmigterAuftrag, Kennungsvergabe, RegelBeleg, Richtung};

/// Alles, was das Regelwerk fuer einen Topf in einem Zyklus braucht.
#[derive(Debug)]
pub struct Eingabe<'a> {
    pub grenzen: &'a Grenzen,
    pub universum: &'a Universum,
    pub topf: &'a Topf,
    pub kurse: &'a Kurse,
    pub vorschlag: Option<&'a Vorschlag>,
    /// Kill-Switch, Tagesverlust, Datenfehler oder leeres Budget: Dann gibt es keine
    /// freiwilligen Auftraege. Zwingende Ausstiege laufen weiter (R36).
    pub halt: bool,
    /// Mit noch nicht abgerechnetem Erloes gekaufte Werte: Vor deren Valuta kein freiwilliger
    /// Verkauf (#311 H1a, `Stand::vor_valuta`). Zwingende Verkaeufe gehen immer durch.
    pub vor_valuta: &'a [Isin],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerwerfGrund {
    UnbekannterWert(Isin),
    NegativesGewicht(Isin),
    ZuVieleWerte(usize),
    SummeUeber100(i64),
    KursFehlt(Isin),
    /// Der Vorschlag stammt nicht vom Berater dieses Topfes (R20, 1·F02).
    FremderBerater {
        berater: BeraterId,
        topf: TopfId,
    },
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
    /// Kurs eines gehaltenen Werts fehlt oder ist aelter als das Hoechstalter: Fuer diesen Wert
    /// gibt es keinen Auftrag, auch keinen zwingenden, und keine freiwilligen Auftraege.
    KursUnplausibel(Isin),
    /// Bestand mehr als die Toleranz ueber dem Hoechstanteil: Zukauf gesperrt, Abbau (R22).
    BestandUeberToleranz {
        isin: Isin,
        ist_bp: i64,
    },
    /// Mehr Werte gehalten als erlaubt: die ueberzaehligen werden abgebaut (R22).
    ZuVielePositionen {
        ist: usize,
        erlaubt: usize,
    },
    /// Kauf gekuerzt, damit der Wert nach dem Kauf am Limit unter dem Hoechstanteil bleibt.
    KaufgrenzeGekuerzt(Isin),
    /// Kein Kauf eines neuen Werts, weil sonst zu viele Werte gehalten wuerden.
    PositionsgrenzeErreicht(Isin),
    StoppFehlt {
        isin: Isin,
        gefragt: Option<i64>,
    },
    /// Bargeld über der Reserve geht in den Parkwert: neues Zielgewicht (#311 H3).
    Parkwert {
        isin: Isin,
        ziel_bp: i64,
    },
    /// Der Parkwert ist kein handelbarer ETF mit frischem Kurs; es wird nicht geparkt.
    ParkwertUngueltig(Isin),
    /// Freiwilliger Verkauf vor der Valuta des Erloeses, mit dem der Wert gekauft wurde (#311 H1a).
    VorValuta(Isin),
    LimitabstandAusserhalb {
        isin: Isin,
        gefragt: i64,
        verwendet: i64,
    },
    Rechenfehler,
}

/// Verkaeufe zuerst. Die Kaeufe darf die Auftragsstelle erst senden, wenn alle Verkaeufe
/// ausgefuehrt sind; sonst verfallen sie.
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

#[derive(Clone, Debug)]
struct Kandidat {
    isin: Isin,
    richtung: Richtung,
    stueck: u64,
    /// Exakter Kurs; ganze Cent entstehen erst beim Runden zur sicheren Seite.
    kurs: Preis,
    komplett: bool,
    limit_bp: Option<i64>,
    anlass: Anlass,
}

impl Kandidat {
    /// Limit: Kauf hoechstens den Limit-Abstand ueber, Verkauf hoechstens darunter.
    fn limit(&self, g: &Grenzen) -> Option<Cent> {
        limit_fuer(
            self.limit_bp.unwrap_or(g.limit_abstand_bp()),
            self.kurs,
            self.richtung,
        )
    }

    /// Volumen am Limit, dem ungünstigsten Fall (R21).
    fn volumen(&self, g: &Grenzen) -> Option<Cent> {
        self.limit(g)?.mal_stueck(self.stueck)
    }

    /// Volumen fuer die Umschlaggrenze: am hoeheren Preis aus Kurs und Limit, aufgerundet. Ein
    /// Kauf fuehrt hoechstens zum Limit aus, ein Verkauf mindestens zum Limit, oft zum Kurs oder
    /// darueber.
    fn umschlag(&self, g: &Grenzen) -> Option<Cent> {
        Preis::aus_cent(self.limit(g)?)?
            .max(self.kurs)
            .mal_stueck(self.stueck, Rundung::Auf)
    }

    /// Hoechstmoegliche Wertminderung des Topfes durch diesen Auftrag: Abstand zwischen Kurs und
    /// Limit je Stueck, aufgerundet, plus hoechste Gebuehr.
    fn verlust(&self, g: &Grenzen) -> Option<Cent> {
        let limit = self.limit(g)?;
        let abstand = self.kurs.checked_sub(Preis::aus_cent(limit)?)?.abs()?;
        let volumen = limit.mal_stueck(self.stueck)?;
        abstand
            .mal_stueck(self.stueck, Rundung::Auf)?
            .checked_add(g.gebuehren().fuer(volumen)?)
    }
}

/// Das Limit in ganzen Cent zur sicheren Seite: Ein Kauflimit ist ein Hoechstpreis und wird
/// abgerundet, ein Verkaufslimit ist ein Mindestpreis und wird aufgerundet. So kauft kein
/// Auftrag teurer und verkauft keiner billiger als Kurs und Limit-Abstand vorsehen.
fn limit_fuer(abstand: i64, kurs: Preis, richtung: Richtung) -> Option<Cent> {
    match richtung {
        Richtung::Kauf => kurs.mal_bp(BP_GANZ.checked_add(abstand)?, Rundung::Ab),
        Richtung::Verkauf => kurs.mal_bp(BP_GANZ.checked_sub(abstand)?, Rundung::Auf),
    }
}

/// Kurs eines Werts, wenn er vorliegt und nicht aelter als das Hoechstalter ist.
pub(crate) fn frischer_kurs(g: &Grenzen, kurse: &Kurse, isin: &Isin) -> Option<Preis> {
    let kurs = kurse.eintrag(isin)?;
    // Synthetische Kurse in isolierten Simulationen haben keinen Börsenschluss.
    if kurs.herkunft() == Herkunft::Quelle && depot_kern::liste::morgenlauf(kurse.stand()) {
        let start = kurse.stand().in_frankfurt().map(|(tag, ..)| tag);
        return crate::daten::morgen_befund(g, kurse.stand(), start, kurse, isin, kurs)
            .is_none()
            .then_some(kurs.preis());
    }
    let alter = kurse.alter_s(isin)?;
    (alter <= g.kurs_hoechstalter_s())
        .then(|| kurse.kurs(isin))
        .flatten()
}

/// Ob `wert` mehr als `grenze_bp` von `gesamt` ist, exakt gerechnet. Beide Werte sind exakt,
/// nie vorher gerundet: Eine Rundung koennte einen Grenzuebertritt verdecken (K21-01).
fn ueber_anteil(wert: Preis, gesamt: Preis, grenze_bp: i64) -> Option<bool> {
    let links = i128::from(wert.fein()).checked_mul(i128::from(BP_GANZ))?;
    let rechts = i128::from(gesamt.fein()).checked_mul(i128::from(grenze_bp))?;
    Some(links > rechts)
}

/// Ein Betrag in ganzen Cent als exakter Wert.
fn exakt(c: Cent) -> Option<Preis> {
    Preis::aus_cent(c)
}

/// Ganze Stuecke fuer einen exakten Betrag, abgerundet.
fn stueck_fuer(betrag: Preis, kurs: Preis) -> Option<u64> {
    kurs.stueck_fuer(betrag)
}

/// Die groesste Stueckzahl in `0..=hoechstens`, fuer die `passt` gilt. `passt` muss fallend
/// sein: Gilt es fuer `s`, gilt es fuer jede kleinere Zahl. `None` bei einem Rechenfehler.
fn groesste_passende(hoechstens: u64, passt: impl Fn(u64) -> Option<bool>) -> Option<u64> {
    if passt(hoechstens)? {
        return Some(hoechstens);
    }
    let (mut unten, mut oben) = (0_u64, hoechstens);
    // Invariante: passt(unten) oder unten == 0; nicht passt(oben).
    while oben.checked_sub(unten)? > 1 {
        let mitte = unten.checked_add(oben.checked_sub(unten)?.checked_div(2)?)?;
        if passt(mitte)? {
            unten = mitte;
        } else {
            oben = mitte;
        }
    }
    if unten > 0 && !passt(unten)? {
        return Some(0);
    }
    Some(unten)
}

/// Hoechstens `gewuenscht` Stueck, so dass Volumen am Limit plus Gebuehr ins Budget passt.
fn leistbare_stueck(g: &Grenzen, limit: Cent, gewuenscht: u64, budget: Cent) -> Option<u64> {
    groesste_passende(gewuenscht, |s| {
        let v = limit.mal_stueck(s)?;
        Some(v.checked_add(g.gebuehren().fuer(v)?)? <= budget)
    })
}

/// Ganze Positionen, deren belegter Schluss den Schutz-Stopp erreicht hat. Der Ausstieg nutzt
/// den hoechsten erlaubten Limitabstand, damit er auch an einem schwachen Folgetag fuellt (#311 H2a).
pub fn stopp_auftraege(
    g: &Grenzen,
    topf: &Topf,
    kurse: &Kurse,
    stopps: &BTreeMap<Isin, Cent>,
    vergabe: &mut Kennungsvergabe,
) -> Option<Vec<GenehmigterAuftrag>> {
    let mut kandidaten = Vec::new();
    for (isin, stopp) in stopps {
        let Some(kurs) = frischer_kurs(g, kurse, isin) else {
            continue;
        };
        let stueck = topf.stueck(isin);
        if stueck > 0 && kurs <= Preis::aus_cent(*stopp)? {
            kandidaten.push(Kandidat {
                isin: isin.clone(),
                richtung: Richtung::Verkauf,
                stueck,
                kurs,
                komplett: true,
                limit_bp: Some(g.limit_abstand_max_bp()),
                anlass: Anlass::Stopp,
            });
        }
    }
    Some(genehmige(g, &kandidaten, &[], vergabe)?.verkaeufe)
}

/// Ganze Positionen mit fälligem und nach Beratung unbegründetem Halten schließen, mit dem
/// hoechsten erlaubten Limitabstand wie beim Stopp (#311 H2a).
pub fn frist_auftraege(
    g: &Grenzen,
    topf: &Topf,
    kurse: &Kurse,
    faellig: &BTreeSet<Isin>,
    vergabe: &mut Kennungsvergabe,
) -> Option<Vec<GenehmigterAuftrag>> {
    let kandidaten: Vec<_> = faellig
        .iter()
        .filter_map(|isin| {
            let kurs = frischer_kurs(g, kurse, isin)?;
            let stueck = topf.stueck(isin);
            (stueck > 0).then(|| Kandidat {
                isin: isin.clone(),
                richtung: Richtung::Verkauf,
                stueck,
                kurs,
                komplett: true,
                limit_bp: Some(g.limit_abstand_max_bp()),
                anlass: Anlass::FristOhneBegruendung,
            })
        })
        .collect();
    Some(genehmige(g, &kandidaten, &[], vergabe)?.verkaeufe)
}

pub fn entscheide(e: &Eingabe<'_>, vergabe: &mut Kennungsvergabe) -> Entscheidung {
    entscheide_intern(e, vergabe, None, false)
}

/// Plant Käufe mit dem vom Kontowächter berechneten freien Bargeld. Geplante Verkäufe
/// erhöhen dieses Budget erst nach ihrer Füllung; gefüllte Erlöse zählen schon vor der
/// Valuta (#311 H1a).
pub fn entscheide_mit_kaufbudget(
    e: &Eingabe<'_>,
    vergabe: &mut Kennungsvergabe,
    frei: Cent,
) -> Entscheidung {
    entscheide_intern(e, vergabe, Some(frei), false)
}

/// Bei technisch ausgefallener Zweitmeinung nur halbierte Kaufziele und zwingende Verkäufe planen.
pub fn entscheide_ersatzkauf(
    e: &Eingabe<'_>,
    vergabe: &mut Kennungsvergabe,
    frei: Cent,
) -> Entscheidung {
    entscheide_intern(e, vergabe, Some(frei), true)
}

/// Eine am letzten Schluss verfallene KI-Kauforder, die der Morgenlauf einmal ohne neue
/// KI-Antwort wiederholt (#311 H2b). Sie laeuft durch dieselbe Genehmigung wie ein KI-Kauf:
/// Anteil, Positionszahl, Umschlag, Orderzahl, Mindestorder und Deckung.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wiederholung {
    pub isin: Isin,
    pub stueck: u64,
    /// Limitabstand zum juengsten Schluss; muss im Rahmen der Grenzen liegen.
    pub limit_bp: i64,
    /// Stoppabstand der urspruenglichen Order.
    pub stopp_bp: Option<i64>,
}

/// Plant zwingende Verkaeufe wie immer und als einzigen freiwilligen Auftrag die Wiederholung.
/// `e.vorschlag` wird nicht gelesen.
pub fn entscheide_wiederholung(
    e: &Eingabe<'_>,
    vergabe: &mut Kennungsvergabe,
    frei: Cent,
    w: &Wiederholung,
) -> Entscheidung {
    let ohne_vorschlag = Eingabe {
        vorschlag: None,
        ..*e
    };
    entscheide_plan(&ohne_vorschlag, vergabe, Some(frei), false, Some(w))
}

fn entscheide_intern(
    e: &Eingabe<'_>,
    vergabe: &mut Kennungsvergabe,
    kaufbudget: Option<Cent>,
    ersatzkauf: bool,
) -> Entscheidung {
    entscheide_plan(e, vergabe, kaufbudget, ersatzkauf, None)
}

fn entscheide_plan(
    e: &Eingabe<'_>,
    vergabe: &mut Kennungsvergabe,
    kaufbudget: Option<Cent>,
    ersatzkauf: bool,
    wiederholung: Option<&Wiederholung>,
) -> Entscheidung {
    let mut befunde = Vec::new();
    let Some((verkaeufe, kaeufe)) = plane(e, kaufbudget, ersatzkauf, wiederholung, &mut befunde)
    else {
        return Entscheidung::nur(Befund::Rechenfehler);
    };
    let Some(mut ent) = genehmige(e.grenzen, &verkaeufe, &kaeufe, vergabe) else {
        return Entscheidung::nur(Befund::Rechenfehler);
    };
    for kauf in &mut ent.kaeufe {
        let isin = &kauf.auftrag().isin;
        let gefragt = match wiederholung {
            Some(w) => w.stopp_bp.filter(|_| w.isin == *isin),
            None => e.vorschlag.and_then(|v| v.stopp_bp.get(isin).copied()),
        };
        let stopp = gefragt.filter(|bp| {
            !ersatzkauf && (e.grenzen.stopp_min_bp()..=e.grenzen.notbremse_bp()).contains(bp)
        });
        if stopp.is_none() {
            befunde.push(Befund::StoppFehlt {
                isin: isin.clone(),
                gefragt,
            });
        }
        kauf.setze_stopp(stopp);
    }
    ent.befunde = befunde;
    ent
}

/// Vergibt Kennungen, Verkaeufe zuerst, und siegelt die Auftraege. Erst stehen alle Auftraege
/// samt Limit, dann werden alle Kennungen auf einmal vergeben: Ein Abbruch verbraucht keine
/// Nummer (K9-03).
fn genehmige(
    g: &Grenzen,
    verkaeufe: &[Kandidat],
    kaeufe: &[Kandidat],
    vergabe: &mut Kennungsvergabe,
) -> Option<Entscheidung> {
    let abdruck = g.fingerabdruck();
    let mut geplant = Vec::new();
    for k in verkaeufe.iter().chain(kaeufe) {
        geplant.push((k, k.limit(g)?));
    }
    let kennungen = vergabe.vergib(geplant.len())?;
    let mut ent = Entscheidung::default();
    for ((k, limit), kennung) in geplant.into_iter().zip(kennungen) {
        let auftrag = GenehmigterAuftrag::neu(
            Auftrag {
                kennung,
                isin: k.isin.clone(),
                richtung: k.richtung,
                stueck: k.stueck,
                limit,
            },
            RegelBeleg {
                grenzen_abdruck: abdruck.clone(),
                anlass: k.anlass,
            },
        );
        match k.richtung {
            Richtung::Verkauf => ent.verkaeufe.push(auftrag),
            Richtung::Kauf => ent.kaeufe.push(auftrag),
        }
    }
    Some(ent)
}

/// Plant alle Auftraege. `None` nur bei einem Rechenfehler.
fn plane(
    e: &Eingabe<'_>,
    kaufbudget: Option<Cent>,
    ersatzkauf: bool,
    wiederholung: Option<&Wiederholung>,
    befunde: &mut Vec<Befund>,
) -> Option<(Vec<Kandidat>, Vec<Kandidat>)> {
    let g = e.grenzen;

    // Kurse der gehaltenen Werte: fehlt einer oder ist er zu alt, ist der Topf nicht bewertbar.
    let mut kurse_ok = true;
    for isin in e.topf.positionen().keys() {
        if frischer_kurs(g, e.kurse, isin).is_none() {
            befunde.push(Befund::KursUnplausibel(isin.clone()));
            kurse_ok = false;
        }
    }

    let mut zwang = Vec::new();
    let mut gesperrt = BTreeSet::new();
    ausstieg_positivliste(e, &mut zwang, &mut gesperrt, befunde);

    let topfwert = if kurse_ok {
        Some(e.topf.wert_exakt(e.kurse)?).filter(|w| w.fein() > 0)
    } else {
        None
    };
    // Die Auswahl der zwingenden Verkaeufe folgt allein den Regeln. KI-Ziele, gueltig oder
    // nicht, steuern sie nie (K9-01).
    let Some(tw) = topfwert else {
        if e.halt {
            befunde.push(Befund::Halt);
        }
        return Some((zwang, Vec::new()));
    };
    abbau_anteil(e, tw, &mut zwang, &mut gesperrt, befunde)?;
    abbau_positionen(e, &mut zwang, &mut gesperrt, befunde)?;

    let wiederholt = match wiederholung {
        Some(w) if !e.halt => wiederholungs_kandidat(e, w, befunde),
        _ => None,
    };
    let ziele = if e.halt {
        befunde.push(Befund::Halt);
        None
    } else if wiederholung.is_some() {
        None
    } else {
        match e.vorschlag {
            None => {
                befunde.push(Befund::KeinVorschlag);
                None
            }
            Some(v) => match pruefe_vorschlag(e, v, befunde) {
                Err(grund) => {
                    befunde.push(Befund::VorschlagVerworfen(grund));
                    None
                }
                Ok(mut ziele) => {
                    if ersatzkauf {
                        for gewicht in ziele.values_mut() {
                            *gewicht = gewicht.div_euclid(2);
                        }
                    }
                    Some(ziele)
                }
            },
        }
    };

    // Gemeinsame Bewertungsbasis (K9-02): Alle Auftraege eines Plans mindern denselben
    // Topfwert. Der Plan wird so lange neu gerechnet, bis jeder Abbau gegen die Minderung des
    // ganzen Plans haelt. Die Abbaumengen wachsen dabei nur; der freiwillige Teil wird jedes Mal
    // ganz neu geplant und haelt damit alle seine Grenzen gegen den endgueltigen Zwang.
    loop {
        let mut verkaeufe = zwang.clone();
        let mut kaeufe = Vec::new();
        let mut befunde_frei = Vec::new();
        if let Some(ziele) = &ziele {
            freiwillig(
                e,
                kaufbudget,
                tw,
                ziele,
                &gesperrt,
                ersatzkauf,
                &mut verkaeufe,
                &mut kaeufe,
                &mut befunde_frei,
            )?;
        } else if let Some(k) = &wiederholt {
            freiwillig_aus(
                e,
                kaufbudget,
                tw,
                vec![k.clone()],
                &mut verkaeufe,
                &mut kaeufe,
                &mut befunde_frei,
            )?;
        }
        if !abbau_nachziehen(e, tw, &mut zwang, &verkaeufe, &kaeufe)? {
            befunde.extend(befunde_frei);
            return Some((verkaeufe, kaeufe));
        }
    }
}

/// Prueft jeden Abbau gegen die Minderung des ganzen Plans (`verkaeufe` und `kaeufe`) und
/// erhoeht ihn, wo der Rest sonst ueber dem Hoechstanteil laege. `true`, wenn sich ein Abbau
/// geaendert hat.
fn abbau_nachziehen(
    e: &Eingabe<'_>,
    topfwert: Preis,
    zwang: &mut [Kandidat],
    verkaeufe: &[Kandidat],
    kaeufe: &[Kandidat],
) -> Option<bool> {
    let g = e.grenzen;
    let mut verlust = Cent::NULL;
    for k in verkaeufe.iter().chain(kaeufe) {
        verlust = verlust.checked_add(k.verlust(g)?)?;
    }
    let mut geaendert = false;
    for k in zwang.iter_mut() {
        if k.anlass != Anlass::Abbau || k.komplett {
            continue;
        }
        let basis = topfwert.checked_sub(exakt(verlust.checked_sub(k.verlust(g)?)?)?)?;
        let stueck = abbau_menge(g, k, e.topf.stueck(&k.isin), basis)?;
        if stueck > k.stueck {
            k.komplett = stueck == e.topf.stueck(&k.isin);
            k.stueck = stueck;
            geaendert = true;
        }
    }
    Some(geaendert)
}

/// Kleinste Verkaufsmenge, nach der der Rest (zum Kurs) hoechstens den Hoechstanteil von
/// `basis` abzueglich der eigenen Minderung hat. `basis` ist der Topfwert nach allen anderen
/// Auftraegen.
fn abbau_menge(g: &Grenzen, k: &Kandidat, gehalten: u64, basis: Preis) -> Option<u64> {
    let behalten = groesste_passende(gehalten, |r| {
        let probe = Kandidat {
            stueck: gehalten.checked_sub(r)?,
            ..k.clone()
        };
        let gesamt = basis.checked_sub(exakt(probe.verlust(g)?)?)?;
        Some(!ueber_anteil(
            k.kurs.mal_stueck_exakt(r)?,
            gesamt,
            g.max_anteil_bp(),
        )?)
    })?;
    gehalten.checked_sub(behalten)
}

/// Zwingend: Jeder gehaltene Wert ausserhalb der Positivliste wird ganz verkauft, sofern sein
/// Kurs frisch ist.
fn ausstieg_positivliste(
    e: &Eingabe<'_>,
    zwang: &mut Vec<Kandidat>,
    gesperrt: &mut BTreeSet<Isin>,
    befunde: &mut Vec<Befund>,
) {
    for (isin, gehalten) in e.topf.positionen() {
        if e.universum.handelbar(isin, e.grenzen) {
            continue;
        }
        gesperrt.insert(isin.clone());
        if let Some(kurs) = frischer_kurs(e.grenzen, e.kurse, isin) {
            befunde.push(Befund::NichtMehrHandelbarVerkauft(isin.clone()));
            zwang.push(Kandidat {
                isin: isin.clone(),
                richtung: Richtung::Verkauf,
                stueck: *gehalten,
                kurs,
                komplett: true,
                limit_bp: None,
                anlass: Anlass::AusstiegPositivliste,
            });
        }
    }
}

/// Zwingend: Liegt ein Wert mehr als die Toleranz ueber dem Hoechstanteil, wird er so weit
/// verkauft, dass er danach hoechstens beim Hoechstanteil liegt, gerechnet am Limit mit
/// hoechster Gebuehr. Zukauf ist gesperrt, unabhaengig von der Umschichtungsschwelle.
fn abbau_anteil(
    e: &Eingabe<'_>,
    topfwert: Preis,
    zwang: &mut Vec<Kandidat>,
    gesperrt: &mut BTreeSet<Isin>,
    befunde: &mut Vec<Befund>,
) -> Option<()> {
    let g = e.grenzen;
    let schwelle = g.max_anteil_bp().checked_add(g.bestand_toleranz_bp())?;
    for (isin, gehalten) in e.topf.positionen() {
        if gesperrt.contains(isin) {
            continue;
        }
        let kurs = e.kurse.kurs(isin)?;
        let wert = kurs.mal_stueck_exakt(*gehalten)?;
        if !ueber_anteil(wert, topfwert, schwelle)? {
            continue;
        }
        befunde.push(Befund::BestandUeberToleranz {
            isin: isin.clone(),
            ist_bp: wert.in_bp_von(topfwert)?,
        });
        gesperrt.insert(isin.clone());
        let geplant = Kandidat {
            isin: isin.clone(),
            richtung: Richtung::Verkauf,
            stueck: 0,
            kurs,
            komplett: false,
            limit_bp: None,
            anlass: Anlass::Abbau,
        };
        // Erster Ansatz gegen den Topfwert; `abbau_nachziehen` rechnet ihn gegen den ganzen Plan.
        let stueck = abbau_menge(g, &geplant, *gehalten, topfwert)?;
        let behalten = gehalten.checked_sub(stueck)?;
        if stueck > 0 {
            zwang.push(Kandidat {
                isin: isin.clone(),
                richtung: Richtung::Verkauf,
                stueck,
                kurs,
                komplett: behalten == 0,
                limit_bp: None,
                anlass: Anlass::Abbau,
            });
        }
    }
    Some(())
}

/// Zwingend: Bleiben nach den zwingenden Verkaeufen mehr Werte als erlaubt, werden die
/// ueberzaehligen ganz verkauft: zuerst die mit dem kleinsten Wert, bei gleichem Wert nach ISIN.
/// Die Auswahl ist eine reine Regel; KI-Ziele gehen nicht ein (K9-01).
fn abbau_positionen(
    e: &Eingabe<'_>,
    zwang: &mut Vec<Kandidat>,
    gesperrt: &mut BTreeSet<Isin>,
    befunde: &mut Vec<Befund>,
) -> Option<()> {
    let g = e.grenzen;
    let ganz_verkauft: BTreeSet<&Isin> = zwang
        .iter()
        .filter(|k| k.komplett)
        .map(|k| &k.isin)
        .collect();
    let mut bleiben = Vec::new();
    for (isin, gehalten) in e.topf.positionen() {
        if ganz_verkauft.contains(isin) || !e.universum.handelbar(isin, g) {
            continue;
        }
        let wert = e.kurse.kurs(isin)?.mal_stueck_exakt(*gehalten)?;
        bleiben.push((wert, isin.clone(), *gehalten));
    }
    // Der größte gehaltene ETF ist der Parkplatz und zählt nicht gegen die Werteanzahl (#311 H3).
    if let Some(platz) = bleiben
        .iter()
        .enumerate()
        .filter(|(_, (_, isin, _))| e.universum.ist_etf(isin))
        .max_by(|(_, a), (_, b)| a.cmp(b))
        .map(|(i, _)| i)
    {
        bleiben.remove(platz);
    }
    let zu_viel = bleiben.len().saturating_sub(g.max_werte());
    if zu_viel == 0 {
        return Some(());
    }
    befunde.push(Befund::ZuVielePositionen {
        ist: bleiben.len(),
        erlaubt: g.max_werte(),
    });
    bleiben.sort();
    for (_, isin, gehalten) in bleiben.into_iter().take(zu_viel) {
        zwang.retain(|k| k.isin != isin);
        gesperrt.insert(isin.clone());
        zwang.push(Kandidat {
            kurs: e.kurse.kurs(&isin)?,
            isin,
            richtung: Richtung::Verkauf,
            stueck: gehalten,
            komplett: true,
            limit_bp: None,
            anlass: Anlass::Abbau,
        });
    }
    Some(())
}

/// Verwirft den ganzen Vorschlag bei fremdem Berater, erfundenen Werten oder unmoeglichen
/// Gewichten; kuerzt zu grosse Gewichte (Zielgrenze).
fn pruefe_vorschlag(
    e: &Eingabe<'_>,
    v: &Vorschlag,
    befunde: &mut Vec<Befund>,
) -> Result<BTreeMap<Isin, i64>, VerwerfGrund> {
    let topf = e.topf.id();
    if v.berater != topf.berater() {
        return Err(VerwerfGrund::FremderBerater {
            berater: v.berater,
            topf,
        });
    }
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
        if frischer_kurs(e.grenzen, e.kurse, isin).is_none() {
            return Err(VerwerfGrund::KursFehlt(isin.clone()));
        }
        summe = summe.saturating_add(*gewicht);
        ziele.insert(isin.clone(), *gewicht);
    }
    // Ein ETF als Parkplatz zählt nicht gegen die Werteanzahl (#311 H3).
    let park = usize::from(ziele.keys().any(|i| e.universum.ist_etf(i)));
    if ziele.len().saturating_sub(park) > e.grenzen.max_werte() {
        return Err(VerwerfGrund::ZuVieleWerte(ziele.len()));
    }
    if summe > BP_GANZ {
        return Err(VerwerfGrund::SummeUeber100(summe));
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
    if let Some(p) = &v.park {
        parke(e, p, &mut ziele, befunde);
    }
    Ok(ziele)
}

/// Frühestens so viele Handelstage bis zum nächsten anderen Kauf, damit sich das Parken lohnt.
pub const PARK_MIN_TAGE: i64 = 5;
/// Kleinster Zukauf in den Parkwert; darunter fressen die Gebühren den Ertrag.
pub const PARK_MIN_ZUKAUF: Cent = Cent(100_000);

/// Setzt das Zielgewicht des Parkwerts auf das Bargeld über der Reserve, höchstens den
/// Höchstanteil (#311 H3). Erwartet die KI bald einen anderen Kauf oder lohnt der Zukauf nicht,
/// wächst der Parkwert nicht; brauchen andere Ziele das Bargeld, schrumpft er zuerst.
fn parke(
    e: &Eingabe<'_>,
    p: &Parkwunsch,
    ziele: &mut BTreeMap<Isin, i64>,
    befunde: &mut Vec<Befund>,
) {
    let g = e.grenzen;
    if !e.universum.ist_etf(&p.isin)
        || !e.universum.handelbar(&p.isin, g)
        || frischer_kurs(g, e.kurse, &p.isin).is_none()
        || !(0..=BP_GANZ).contains(&p.reserve_bp)
    {
        befunde.push(Befund::ParkwertUngueltig(p.isin.clone()));
        return;
    }
    let andere: i64 = ziele
        .iter()
        .filter(|(isin, _)| **isin != p.isin)
        .map(|(_, bp)| *bp)
        .sum();
    let ist = if e.topf.stueck(&p.isin) == 0 {
        0
    } else {
        ist_gewicht_bp(e.topf, e.kurse, &p.isin).unwrap_or(0)
    };
    let mut ziel = BP_GANZ
        .saturating_sub(andere)
        .saturating_sub(p.reserve_bp)
        .clamp(0, g.max_anteil_bp());
    let zukauf = e
        .topf
        .wert_exakt(e.kurse)
        .and_then(|w| w.mal_bp(ziel.saturating_sub(ist), Rundung::Ab));
    if ziel > ist
        && (p.naechster_kauf_tage < PARK_MIN_TAGE || zukauf.is_none_or(|z| z < PARK_MIN_ZUKAUF))
    {
        ziel = ist;
    }
    ziele.insert(p.isin.clone(), ziel);
    befunde.push(Befund::Parkwert {
        isin: p.isin.clone(),
        ziel_bp: ziel,
    });
}

/// Freiwillige Auftraege. Umschlag und Orderzahl gelten fuer das, was die zwingenden
/// Auftraege uebrig lassen; Deckung, Kaufgrenze und Positionszahl fuer alle Auftraege.
#[allow(
    clippy::too_many_arguments,
    reason = "die Planungsschritte teilen genau diese Zwischenstände"
)]
fn freiwillig(
    e: &Eingabe<'_>,
    kaufbudget: Option<Cent>,
    topfwert: Preis,
    ziele: &BTreeMap<Isin, i64>,
    gesperrt: &BTreeSet<Isin>,
    ersatzkauf: bool,
    verkaeufe: &mut Vec<Kandidat>,
    kaeufe: &mut Vec<Kandidat>,
    befunde: &mut Vec<Befund>,
) -> Option<()> {
    let mut kandidaten = sammle_kandidaten(e, ziele, topfwert, gesperrt, befunde)?;
    if ersatzkauf {
        kandidaten.retain(|k| k.richtung == Richtung::Kauf);
    }
    freiwillig_aus(
        e, kaufbudget, topfwert, kandidaten, verkaeufe, kaeufe, befunde,
    )
}

/// Der Kandidat einer Wiederholung, oder `None` mit Befund, wenn sie nicht zulaessig ist.
fn wiederholungs_kandidat(
    e: &Eingabe<'_>,
    w: &Wiederholung,
    befunde: &mut Vec<Befund>,
) -> Option<Kandidat> {
    let g = e.grenzen;
    if !e.universum.handelbar(&w.isin, g) {
        befunde.push(Befund::VorschlagVerworfen(VerwerfGrund::UnbekannterWert(
            w.isin.clone(),
        )));
        return None;
    }
    let Some(kurs) = frischer_kurs(g, e.kurse, &w.isin) else {
        befunde.push(Befund::KursUnplausibel(w.isin.clone()));
        return None;
    };
    if !(g.limit_abstand_min_bp()..=g.limit_abstand_max_bp()).contains(&w.limit_bp) {
        befunde.push(Befund::LimitabstandAusserhalb {
            isin: w.isin.clone(),
            gefragt: w.limit_bp,
            verwendet: 0,
        });
        return None;
    }
    Some(Kandidat {
        isin: w.isin.clone(),
        richtung: Richtung::Kauf,
        stueck: w.stueck,
        kurs,
        komplett: false,
        limit_bp: Some(w.limit_bp),
        anlass: Anlass::Umschichtung,
    })
}

/// Umschlag, Orderzahl und Kaeufe fuer schon gesammelte freiwillige Kandidaten.
fn freiwillig_aus(
    e: &Eingabe<'_>,
    kaufbudget: Option<Cent>,
    topfwert: Preis,
    mut kandidaten: Vec<Kandidat>,
    verkaeufe: &mut Vec<Kandidat>,
    kaeufe: &mut Vec<Kandidat>,
    befunde: &mut Vec<Befund>,
) -> Option<()> {
    let g = e.grenzen;
    let mut zwang_volumen = Cent::NULL;
    for k in verkaeufe.iter() {
        zwang_volumen = zwang_volumen.checked_add(k.umschlag(g)?)?;
    }
    // Exakt gerechnet, erst der freie Betrag in ganzen Cent abgerundet.
    let umschlag_frei = topfwert
        .mal_bp(g.max_umschlag_bp(), Rundung::Ab)?
        .checked_sub(zwang_volumen)?;
    begrenze_umschlag(g, umschlag_frei, &mut kandidaten, befunde)?;

    // Groesste Orders zuerst, hoechstens so viele, wie die zwingenden uebrig lassen.
    let mut mit_volumen = Vec::new();
    for k in kandidaten {
        mit_volumen.push((k.volumen(g)?, k));
    }
    mit_volumen.sort_by(|(va, a), (vb, b)| vb.cmp(va).then_with(|| a.isin.cmp(&b.isin)));
    let frei = g.max_orders_je_topf().saturating_sub(verkaeufe.len());
    let mut behalten = Vec::new();
    for (_, k) in mit_volumen {
        if behalten.len() < frei {
            behalten.push(k);
        } else {
            befunde.push(Befund::OrderLimitErreicht(k.isin));
        }
    }

    let (freie_verkaeufe, freie_kaeufe): (Vec<Kandidat>, Vec<Kandidat>) = behalten
        .into_iter()
        .partition(|k| k.richtung == Richtung::Verkauf);
    verkaeufe.extend(freie_verkaeufe);
    kaufe(
        e,
        kaufbudget,
        topfwert,
        verkaeufe,
        freie_kaeufe,
        kaeufe,
        befunde,
    )
}

/// Kaeufe mit Deckung, Kaufgrenze des Anteils und Kaufgrenze der Positionszahl,
/// alles am Limit mit hoechster Gebuehr.
fn kaufe(
    e: &Eingabe<'_>,
    kaufbudget: Option<Cent>,
    topfwert: Preis,
    verkaeufe: &[Kandidat],
    geplant: Vec<Kandidat>,
    kaeufe: &mut Vec<Kandidat>,
    befunde: &mut Vec<Befund>,
) -> Option<()> {
    let g = e.grenzen;
    let mut budget = kaufbudget.unwrap_or_else(|| e.topf.bargeld());
    let mut verlust = Cent::NULL;
    for k in verkaeufe {
        if kaufbudget.is_none() {
            let erloes = k.volumen(g)?;
            budget = budget
                .checked_add(erloes)?
                .checked_sub(g.gebuehren().fuer(erloes)?)?;
        }
        verlust = verlust.checked_add(k.verlust(g)?)?;
    }
    for k in &geplant {
        verlust = verlust.checked_add(k.verlust(g)?)?;
    }
    // Kleinster moeglicher Topfwert nach allen Auftraegen: Basis der Kaufgrenze.
    let gesamt_min = topfwert.checked_sub(exakt(verlust)?)?;

    let ganz_verkauft: BTreeSet<&Isin> = verkaeufe
        .iter()
        .filter(|k| k.komplett)
        .map(|k| &k.isin)
        .collect();
    let bleiben: Vec<&Isin> = e
        .topf
        .positionen()
        .keys()
        .filter(|i| !ganz_verkauft.contains(i))
        .collect();
    // Ein ETF ist der Parkplatz und zählt nicht gegen die Werteanzahl (#311 H3).
    let mut park_frei = !bleiben.iter().any(|i| e.universum.ist_etf(i));
    let mut positionen = bleiben.len().saturating_sub(usize::from(!park_frei));

    for k in geplant {
        let gehalten = e.topf.stueck(&k.isin);
        let als_park = gehalten == 0 && park_frei && e.universum.ist_etf(&k.isin);
        if gehalten == 0 && !als_park && positionen >= g.max_werte() {
            befunde.push(Befund::PositionsgrenzeErreicht(k.isin));
            continue;
        }
        let limit = k.limit(g)?;
        let anteil_ok = groesste_passende(k.stueck, |s| {
            let nach = exakt(limit.mal_stueck(gehalten.checked_add(s)?)?)?;
            Some(!ueber_anteil(nach, gesamt_min, g.max_anteil_bp())?)
        })?;
        if anteil_ok < k.stueck {
            befunde.push(Befund::KaufgrenzeGekuerzt(k.isin.clone()));
        }
        let stueck = leistbare_stueck(g, limit, anteil_ok, budget)?;
        let volumen = limit.mal_stueck(stueck)?;
        if stueck == 0 || volumen < g.min_order() {
            let befund = if anteil_ok < k.stueck {
                Befund::UnterMindestorder(k.isin)
            } else {
                Befund::BargeldReichtNicht(k.isin)
            };
            befunde.push(befund);
            continue;
        }
        budget = budget.checked_sub(volumen.checked_add(g.gebuehren().fuer(volumen)?)?)?;
        if als_park {
            park_frei = false;
        } else if gehalten == 0 {
            positionen = positionen.checked_add(1)?;
        }
        kaeufe.push(Kandidat { stueck, ..k });
    }
    Some(())
}

/// Gewicht einer Position mit derselben Bewertung und Rundung wie bei der Orderentscheidung.
pub fn ist_gewicht_bp(topf: &Topf, kurse: &Kurse, isin: &Isin) -> Option<i64> {
    gewicht_bp(
        kurse.kurs(isin)?.mal_stueck_exakt(topf.stueck(isin))?,
        topf.wert_exakt(kurse)?,
    )
}

fn gewicht_bp(wert: Preis, topfwert: Preis) -> Option<i64> {
    wert.in_bp_von(topfwert)
}

fn sammle_kandidaten(
    e: &Eingabe<'_>,
    ziele: &BTreeMap<Isin, i64>,
    topfwert: Preis,
    gesperrt: &BTreeSet<Isin>,
    befunde: &mut Vec<Befund>,
) -> Option<Vec<Kandidat>> {
    let g = e.grenzen;
    let alle: BTreeSet<&Isin> = e.topf.positionen().keys().chain(ziele.keys()).collect();
    let mut kandidaten = Vec::new();
    for isin in alle {
        if gesperrt.contains(isin) {
            continue;
        }
        let kurs = frischer_kurs(g, e.kurse, isin)?;
        let gehalten = e.topf.stueck(isin);
        let ziel_bp = ziele.get(isin).copied().unwrap_or(0);
        let ist_wert = kurs.mal_stueck_exakt(gehalten)?;
        let ziel_wert = exakt(topfwert.mal_bp(ziel_bp, Rundung::Ab)?)?;
        let ist_bp = gewicht_bp(ist_wert, topfwert)?;
        let abweichung = ziel_bp.checked_sub(ist_bp)?;
        if abweichung.checked_abs()? < g.min_aenderung_bp() {
            if abweichung != 0 {
                befunde.push(Befund::UnterSchwelle(isin.clone()));
            }
            continue;
        }
        let limit_bp = e
            .vorschlag
            .and_then(|v| v.limit_bp.get(isin))
            .copied()
            .and_then(|bp| {
                if (g.limit_abstand_min_bp()..=g.limit_abstand_max_bp()).contains(&bp) {
                    Some(bp)
                } else {
                    befunde.push(Befund::LimitabstandAusserhalb {
                        isin: isin.clone(),
                        gefragt: bp,
                        verwendet: g.limit_abstand_bp(),
                    });
                    None
                }
            });
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
                komplett: komplett || stueck == gehalten,
                limit_bp,
                anlass: Anlass::Umschichtung,
            }
        } else {
            Kandidat {
                isin: isin.clone(),
                richtung: Richtung::Kauf,
                stueck: stueck_fuer(ziel_wert.checked_sub(ist_wert)?, kurs)?,
                kurs,
                komplett: false,
                limit_bp,
                anlass: Anlass::Umschichtung,
            }
        };
        // Mit noch nicht abgerechnetem Erloes gekauft: vor der Valuta kein freiwilliger Verkauf.
        if k.richtung == Richtung::Verkauf && e.vor_valuta.contains(isin) {
            befunde.push(Befund::VorValuta(isin.clone()));
            continue;
        }
        // Mindestorder am Limit; der Verkauf des ganzen Bestands ist ausgenommen.
        if k.stueck == 0 || (k.volumen(g)? < g.min_order() && !k.komplett) {
            befunde.push(Befund::UnterMindestorder(isin.clone()));
            continue;
        }
        kandidaten.push(k);
    }
    Some(kandidaten)
}

/// Kuerzt alle freiwilligen Orders anteilig, wenn sie zusammen mehr als `erlaubt` bewegen
/// wuerden, gerechnet am hoeheren Preis aus Kurs und Limit.
fn begrenze_umschlag(
    g: &Grenzen,
    erlaubt: Cent,
    kandidaten: &mut Vec<Kandidat>,
    befunde: &mut Vec<Befund>,
) -> Option<()> {
    let erlaubt = if erlaubt.0 < 0 { Cent::NULL } else { erlaubt };
    let mut geplant = Cent::NULL;
    for k in kandidaten.iter() {
        geplant = geplant.checked_add(k.umschlag(g)?)?;
    }
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
        if k.stueck == 0 || k.volumen(g)? < g.min_order() {
            befunde.push(Befund::UnterMindestorder(k.isin.clone()));
        } else {
            neu.push(k);
        }
    }
    *kandidaten = neu;
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gewicht_wie_regelwerk() -> Result<(), String> {
        use depot_kern::{Datum, Instrument, InstrumentArt, Kurs, Zeitpunkt};
        let tag = Datum::neu(2026, 11, 10).ok_or("Tag")?;
        let zeit = Zeitpunkt::frankfurt(tag, 17, 35, 0).ok_or("Zeit")?;
        let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let universum = Universum::neu(vec![Instrument {
            isin: isin.clone(),
            name: "SAP".into(),
            art: InstrumentArt::Aktie,
        }]);
        for geld in [0, 10_000, 500_000] {
            for preis in [100, 999, 10_091] {
                for stueck in [1, 3, 143] {
                    let mut topf =
                        Topf::neu(TopfId::A, Cent(geld)).map_err(|e| format!("{e:?}"))?;
                    topf.setze_bestand(isin.clone(), stueck);
                    let kurs = Kurs::neu(Cent(preis), "EUR", tag, zeit, Herkunft::Simulation)
                        .map_err(|e| format!("{e:?}"))?;
                    let kurse = Kurse::neu(zeit, vec![(isin.clone(), kurs)])
                        .map_err(|e| format!("{e:?}"))?;
                    let gewicht = ist_gewicht_bp(&topf, &kurse, &isin).ok_or("Gewicht")?;
                    let e = Eingabe {
                        grenzen: &Grenzen::standard(),
                        universum: &universum,
                        topf: &topf,
                        kurse: &kurse,
                        vorschlag: None,
                        halt: false,
                        vor_valuta: &[],
                    };
                    let mut befunde = Vec::new();
                    let kandidaten = sammle_kandidaten(
                        &e,
                        &BTreeMap::from([(isin.clone(), gewicht)]),
                        topf.wert_exakt(&kurse).ok_or("Topfwert")?,
                        &BTreeSet::new(),
                        &mut befunde,
                    )
                    .ok_or("Kandidaten")?;
                    assert!(
                        kandidaten.is_empty() && befunde.is_empty(),
                        "{preis} {stueck} {geld}"
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn groesste_passende_findet_den_rand() {
        for grenze in [0_u64, 1, 2, 7, 1_000, u64::MAX] {
            for hoechstens in [0_u64, 1, 5, 1_000, u64::MAX] {
                let r = groesste_passende(hoechstens, |s| Some(s <= grenze));
                assert_eq!(r, Some(grenze.min(hoechstens)), "{grenze} {hoechstens}");
            }
        }
        assert_eq!(groesste_passende(10, |_| None), None);
    }
}

#[cfg(test)]
mod limitabstand_tests {
    use super::*;

    fn kurs_100() -> Option<Preis> {
        Preis::aus_cent(Cent(10_000))
    }

    #[test]
    fn ohne_angabe_100() {
        assert_eq!(
            kurs_100().and_then(|k| limit_fuer(
                Grenzen::standard().limit_abstand_bp(),
                k,
                Richtung::Kauf
            )),
            Some(Cent(10_100))
        );
    }

    #[test]
    fn kauf_250() {
        assert_eq!(
            kurs_100().and_then(|k| limit_fuer(250, k, Richtung::Kauf)),
            Some(Cent(10_250))
        );
    }

    #[test]
    fn verkauf_30() {
        assert_eq!(
            kurs_100().and_then(|k| limit_fuer(30, k, Richtung::Verkauf)),
            Some(Cent(9_970))
        );
    }

    #[test]
    fn verlust_am_limit() -> Result<(), String> {
        let k = Kandidat {
            isin: Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?,
            richtung: Richtung::Kauf,
            stueck: 9,
            kurs: kurs_100().ok_or("Kurs")?,
            komplett: false,
            limit_bp: Some(300),
            anlass: Anlass::Umschichtung,
        };
        let g = Grenzen::standard();
        let volumen = k.volumen(&g).ok_or("Volumen")?;
        assert_eq!(volumen, Cent(92_700));
        assert_eq!(
            k.verlust(&g),
            Cent(2_700).checked_add(g.gebuehren().fuer(volumen).ok_or("Gebühr")?)
        );
        Ok(())
    }
}

#[cfg(test)]
mod grenzrahmen_tests {
    use super::*;

    fn entscheide_mit_abstand(abstand: i64) -> Result<Entscheidung, String> {
        use depot_kern::{BeraterId, Datum, Instrument, InstrumentArt, Kurs, Zeitpunkt};
        let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let tag = Datum::neu(2026, 11, 5).ok_or("Tag")?;
        let jetzt = Zeitpunkt::neu(tag, 12, 0, 0).ok_or("Zeit")?;
        let kurs = Kurs::neu(Cent(10_000), "EUR", tag, jetzt, Herkunft::Simulation)
            .map_err(|e| format!("{e:?}"))?;
        let kurse = Kurse::neu(jetzt, vec![(isin.clone(), kurs)]).map_err(|e| format!("{e:?}"))?;
        let topf = Topf::neu(TopfId::A, Cent(500_000)).map_err(|e| format!("{e:?}"))?;
        let universum = Universum::neu(vec![Instrument {
            isin: isin.clone(),
            name: "SAP".into(),
            art: InstrumentArt::Aktie,
        }]);
        let vorschlag = Vorschlag {
            erwartung_bp: std::collections::BTreeMap::new(),
            berater: BeraterId::Dachs,
            zielgewichte_bp: [(isin.clone(), 4000)].into_iter().collect(),
            begruendung: BTreeMap::new(),
            limit_bp: [(isin, abstand)].into_iter().collect(),
            stopp_bp: std::collections::BTreeMap::new(),
            park: None,
        };
        Ok(entscheide_mit_kaufbudget(
            &Eingabe {
                grenzen: &Grenzen::standard(),
                universum: &universum,
                topf: &topf,
                kurse: &kurse,
                vorschlag: Some(&vorschlag),
                halt: false,
                vor_valuta: &[],
            },
            &mut Kennungsvergabe::nach(0),
            Cent(100_000),
        ))
    }

    fn rueckfall(abstand: i64) -> Result<(), String> {
        let e = entscheide_mit_abstand(abstand)?;
        assert_eq!(
            e.kaeufe.first().map(|a| a.auftrag().limit),
            Some(Cent(10_100))
        );
        assert!(e.befunde.iter().any(|b| matches!(b, Befund::LimitabstandAusserhalb { gefragt, verwendet: 100, .. } if *gefragt == abstand)));
        Ok(())
    }

    #[test]
    fn ueber_rahmen() -> Result<(), String> {
        rueckfall(450)
    }

    #[test]
    fn unter_rahmen() -> Result<(), String> {
        rueckfall(5)
    }

    #[test]
    fn stueck_am_limit() -> Result<(), String> {
        let e = entscheide_mit_abstand(300)?;
        assert_eq!(
            e.kaeufe
                .first()
                .map(|a| (a.auftrag().limit, a.auftrag().stueck)),
            Some((Cent(10_300), 9))
        );
        Ok(())
    }
}

#[cfg(test)]
mod stopp_tests {
    use super::*;
    use depot_kern::{Datum, Kurs, Zeitpunkt};

    /// Ein Wert mit belegtem Kurs, ein Topf mit 10 Stueck davon.
    fn probe(cent: i64) -> Result<(Isin, Topf, Kurse), String> {
        let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let tag = Datum::neu(2026, 11, 5).ok_or("Tag")?;
        let zeit = Zeitpunkt::neu(tag, 12, 0, 0).ok_or("Zeit")?;
        let kurs = Kurs::neu(Cent(cent), "EUR", tag, zeit, Herkunft::Simulation)
            .map_err(|e| format!("{e:?}"))?;
        let kurse = Kurse::neu(zeit, vec![(isin.clone(), kurs)]).map_err(|e| format!("{e:?}"))?;
        let mut topf = Topf::neu(TopfId::A, Cent(500_000)).map_err(|e| format!("{e:?}"))?;
        topf.setze_bestand(isin.clone(), 10);
        Ok((isin, topf, kurse))
    }

    /// #311 H2a: Stopp bei Schluss 100,00 € verkauft mit Limit 97,00 €, nicht mehr 99,00 €.
    #[test]
    fn stopp_verkauf_mit_hoechstabstand() -> Result<(), String> {
        let (isin, topf, kurse) = probe(10_000)?;
        let stopps = [(isin, Cent(10_000))].into_iter().collect();
        let g = Grenzen::standard();
        let orders = stopp_auftraege(&g, &topf, &kurse, &stopps, &mut Kennungsvergabe::nach(0))
            .ok_or("Aufträge")?;
        assert_eq!(
            orders.iter().map(|a| a.auftrag().limit).collect::<Vec<_>>(),
            vec![Cent(9_700)]
        );
        // Der Folgeschluss 97,50 € (−2,5 %) liegt ueber dem Limit, die Order fuellt.
        assert!(orders.iter().all(|a| a.auftrag().limit <= Cent(9_750)));
        Ok(())
    }

    /// #311 H2a: Der Fristverkauf nutzt denselben Hoechstabstand, krumme Kurse runden auf.
    #[test]
    fn frist_verkauf_mit_hoechstabstand() -> Result<(), String> {
        let (isin, topf, kurse) = probe(12_345)?;
        let faellig = [isin].into_iter().collect();
        let g = Grenzen::standard();
        let orders = frist_auftraege(&g, &topf, &kurse, &faellig, &mut Kennungsvergabe::nach(0))
            .ok_or("Aufträge")?;
        // 123,45 × 0,97 = 119,7465 €, zur sicheren Seite aufgerundet 119,75 €.
        assert_eq!(
            orders
                .iter()
                .map(|a| (a.auftrag().limit, a.beleg().anlass))
                .collect::<Vec<_>>(),
            vec![(Cent(11_975), Anlass::FristOhneBegruendung)]
        );
        Ok(())
    }

    /// #311 H2a: Die Notbremse verkauft jedes Stueck mit Kurs × 0,97, aufgerundet.
    #[test]
    fn notbremse_mit_hoechstabstand() -> Result<(), String> {
        let (_, topf, kurse) = probe(12_345)?;
        let g = Grenzen::standard();
        let orders = crate::notbremse_auftraege(&g, &topf, &kurse, &mut Kennungsvergabe::nach(0))
            .ok_or("Aufträge")?;
        assert_eq!(
            orders.iter().map(|a| a.auftrag().limit).collect::<Vec<_>>(),
            vec![Cent(11_975)]
        );
        Ok(())
    }

    /// Gegenprobe: Ein KI-Verkauf ohne eigenen Abstand bleibt bei der Vorgabe von 100 bp.
    #[test]
    fn ki_verkauf_bleibt_bei_vorgabe() -> Result<(), String> {
        use depot_kern::{BeraterId, Instrument, InstrumentArt};
        let (isin, topf, kurse) = probe(10_000)?;
        let universum = Universum::neu(vec![Instrument {
            isin: isin.clone(),
            name: "SAP".into(),
            art: InstrumentArt::Aktie,
        }]);
        let vorschlag = Vorschlag {
            erwartung_bp: BTreeMap::new(),
            berater: BeraterId::Dachs,
            zielgewichte_bp: BTreeMap::new(),
            begruendung: BTreeMap::new(),
            limit_bp: BTreeMap::new(),
            stopp_bp: BTreeMap::new(),
            park: None,
        };
        let e = entscheide(
            &Eingabe {
                grenzen: &Grenzen::standard(),
                universum: &universum,
                topf: &topf,
                kurse: &kurse,
                vorschlag: Some(&vorschlag),
                halt: false,
                vor_valuta: &[],
            },
            &mut Kennungsvergabe::nach(0),
        );
        let limits: Vec<_> = e.verkaeufe.iter().map(|a| a.auftrag().limit).collect();
        assert_eq!(limits, vec![Cent(9_900)]);
        Ok(())
    }

    #[test]
    fn stopp_gilt_am_schluss_und_fuer_kleine_positionen() -> Result<(), String> {
        let g = Grenzen::standard();
        let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let tag = Datum::neu(2026, 11, 5).ok_or("Tag")?;
        let zeit = Zeitpunkt::neu(tag, 12, 0, 0).ok_or("Zeit")?;
        let topf = &mut Topf::neu(TopfId::A, Cent(500_000)).map_err(|e| format!("{e:?}"))?;
        topf.setze_bestand(isin.clone(), 1);
        let stopps = [(isin.clone(), Cent(6_000))].into_iter().collect();
        for (preis, ausgeloest) in [(5_999, true), (6_000, true), (6_001, false)] {
            let kurs = Kurs::neu(Cent(preis), "EUR", tag, zeit, Herkunft::Simulation)
                .map_err(|e| format!("{e:?}"))?;
            let kurse =
                Kurse::neu(zeit, vec![(isin.clone(), kurs)]).map_err(|e| format!("{e:?}"))?;
            let orders = stopp_auftraege(&g, topf, &kurse, &stopps, &mut Kennungsvergabe::nach(0))
                .ok_or("Aufträge")?;
            assert_eq!(orders.len(), usize::from(ausgeloest));
            if let [a] = orders.as_slice() {
                assert_eq!(a.auftrag().stueck, 1);
                assert_eq!(a.auftrag().richtung, Richtung::Verkauf);
                assert_eq!(a.beleg().anlass, Anlass::Stopp);
                assert!(a.beleg().anlass.zwingend());
            }
        }
        let leer = Kurse::neu(zeit, Vec::new()).map_err(|e| format!("{e:?}"))?;
        assert!(
            stopp_auftraege(&g, topf, &leer, &stopps, &mut Kennungsvergabe::nach(0))
                .is_some_and(|orders| orders.is_empty())
        );
        Ok(())
    }
}

#[cfg(test)]
mod wiederholung_tests {
    use super::*;
    use depot_kern::{Datum, Instrument, InstrumentArt, Kurs, Zeitpunkt};

    fn lauf(bargeld: i64, stueck: u64, halt: bool) -> Result<Entscheidung, String> {
        let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let tag = Datum::neu(2026, 11, 17).ok_or("Tag")?;
        let zeit = Zeitpunkt::neu(tag, 12, 0, 0).ok_or("Zeit")?;
        let kurs = Kurs::neu(Cent(10_150), "EUR", tag, zeit, Herkunft::Simulation)
            .map_err(|e| format!("{e:?}"))?;
        let kurse = Kurse::neu(zeit, vec![(isin.clone(), kurs)]).map_err(|e| format!("{e:?}"))?;
        let topf = Topf::neu(TopfId::A, Cent(bargeld)).map_err(|e| format!("{e:?}"))?;
        let universum = Universum::neu(vec![Instrument {
            isin: isin.clone(),
            name: "SAP".into(),
            art: InstrumentArt::Aktie,
        }]);
        let w = Wiederholung {
            isin,
            stueck,
            limit_bp: 100,
            stopp_bp: Some(500),
        };
        Ok(entscheide_wiederholung(
            &Eingabe {
                grenzen: &Grenzen::standard(),
                universum: &universum,
                topf: &topf,
                kurse: &kurse,
                vorschlag: None,
                halt,
                vor_valuta: &[],
            },
            &mut Kennungsvergabe::nach(0),
            Cent(bargeld),
            &w,
        ))
    }

    /// Gleiche Stueckzahl, Limit 101,50 × 1,01 = 102,515 € abgerundet 102,51 €, Stopp uebernommen.
    #[test]
    fn wiederholung_mit_gleicher_stueckzahl_und_stopp() -> Result<(), String> {
        let e = lauf(1_000_000, 30, false)?;
        let kauf = e.kaeufe.first().ok_or("Kauf")?;
        assert_eq!(e.kaeufe.len(), 1);
        assert_eq!(kauf.auftrag().stueck, 30);
        assert_eq!(kauf.auftrag().limit, Cent(10_251));
        assert_eq!(kauf.stopp_bp(), Some(500));
        assert_eq!(kauf.beleg().anlass, Anlass::Umschichtung);
        Ok(())
    }

    /// 50 Stueck zu 102,51 € waeren 51 % von 10.000 €: auf 40 % des kleinsten moeglichen
    /// Topfwerts nach Limitverlust und Gebuehr gekuerzt (38 Stueck), mit Befund.
    #[test]
    fn wiederholung_respektiert_anteil() -> Result<(), String> {
        let e = lauf(1_000_000, 50, false)?;
        let kauf = e.kaeufe.first().ok_or("Kauf")?;
        assert_eq!(kauf.auftrag().stueck, 38);
        assert!(
            e.befunde
                .iter()
                .any(|b| matches!(b, Befund::KaufgrenzeGekuerzt(_)))
        );
        Ok(())
    }

    #[test]
    fn wiederholung_entfaellt_bei_halt() -> Result<(), String> {
        let e = lauf(1_000_000, 30, true)?;
        assert!(e.kaeufe.is_empty());
        assert!(e.befunde.contains(&Befund::Halt));
        Ok(())
    }
}

#[cfg(test)]
mod vor_valuta_tests {
    use super::*;
    use depot_kern::{Datum, Instrument, InstrumentArt, Kurs, Zeitpunkt};

    /// Topf mit 5.000 € Bargeld und `stueck` SAP zu 101,50 €; die KI will SAP ganz verkaufen.
    fn lauf(stueck: u64, gesperrt: bool) -> Result<Entscheidung, String> {
        let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let tag = Datum::neu(2026, 11, 17).ok_or("Tag")?;
        let zeit = Zeitpunkt::neu(tag, 12, 0, 0).ok_or("Zeit")?;
        let kurs = Kurs::neu(Cent(10_150), "EUR", tag, zeit, Herkunft::Simulation)
            .map_err(|e| format!("{e:?}"))?;
        let kurse = Kurse::neu(zeit, vec![(isin.clone(), kurs)]).map_err(|e| format!("{e:?}"))?;
        let mut topf = Topf::neu(TopfId::A, Cent(500_000)).map_err(|e| format!("{e:?}"))?;
        topf.setze_bestand(isin.clone(), stueck);
        let universum = Universum::neu(vec![Instrument {
            isin: isin.clone(),
            name: "SAP".into(),
            art: InstrumentArt::Aktie,
        }]);
        let vorschlag = Vorschlag {
            erwartung_bp: BTreeMap::new(),
            limit_bp: BTreeMap::new(),
            stopp_bp: BTreeMap::new(),
            berater: BeraterId::Dachs,
            zielgewichte_bp: BTreeMap::new(),
            begruendung: BTreeMap::new(),
            park: None,
        };
        let sperre = if gesperrt { vec![isin] } else { Vec::new() };
        Ok(entscheide(
            &Eingabe {
                grenzen: &Grenzen::standard(),
                universum: &universum,
                topf: &topf,
                kurse: &kurse,
                vorschlag: Some(&vorschlag),
                halt: false,
                vor_valuta: &sperre,
            },
            &mut Kennungsvergabe::nach(0),
        ))
    }

    #[test]
    fn ki_verkauf_ohne_sperre() -> Result<(), String> {
        let e = lauf(20, false)?;
        assert_eq!(e.verkaeufe.len(), 1);
        assert_eq!(
            e.verkaeufe.first().map(|v| v.beleg().anlass),
            Some(Anlass::Umschichtung)
        );
        Ok(())
    }

    /// #311 H1a Gegenprobe 2: Ein KI-Verkauf vor der Valuta wird mit `VorValuta` abgelehnt.
    #[test]
    fn ki_verkauf_vor_valuta_abgelehnt() -> Result<(), String> {
        let e = lauf(20, true)?;
        assert!(e.verkaeufe.is_empty(), "{e:?}");
        assert!(e.befunde.iter().any(|b| matches!(b, Befund::VorValuta(_))));
        Ok(())
    }

    /// Zwingende Verkäufe gehen trotz Sperre durch: 60 Stück sind über 55 % des Topfs.
    #[test]
    fn zwingender_abbau_trotz_sperre() -> Result<(), String> {
        let e = lauf(60, true)?;
        assert_eq!(
            e.verkaeufe.first().map(|v| v.beleg().anlass),
            Some(Anlass::Abbau),
            "{e:?}"
        );
        Ok(())
    }
}

#[cfg(test)]
mod park_tests {
    use super::*;
    use depot_kern::{Datum, Instrument, InstrumentArt, Kurs, Zeitpunkt};

    const SAP: &str = "DE0007164600";
    const ALV: &str = "DE0008404005";
    const BASF: &str = "DE000BASF111";
    const BAYER: &str = "DE000BAY0017";
    const ETF: &str = "IE00B4L5Y983";

    fn isin(s: &str) -> Result<Isin, String> {
        Isin::neu(s).map_err(|e| format!("{e:?}"))
    }

    /// Alle Werte zu 100 €. `bestand` in Stück, `ziele` in bp, `park` als (Reserve, Tage).
    fn lauf(
        bargeld: i64,
        bestand: &[(&str, u64)],
        ziele: &[(&str, i64)],
        park: Option<(&str, i64, i64)>,
        kaufbudget: Option<i64>,
    ) -> Result<Entscheidung, String> {
        let tag = Datum::neu(2026, 11, 17).ok_or("Tag")?;
        let zeit = Zeitpunkt::neu(tag, 12, 0, 0).ok_or("Zeit")?;
        let alle = [SAP, ALV, BASF, BAYER, ETF];
        let mut kurse = Vec::new();
        let mut instrumente = Vec::new();
        for s in alle {
            let kurs = Kurs::neu(Cent(10_000), "EUR", tag, zeit, Herkunft::Simulation)
                .map_err(|e| format!("{e:?}"))?;
            kurse.push((isin(s)?, kurs));
            instrumente.push(Instrument {
                isin: isin(s)?,
                name: s.into(),
                art: if s == ETF {
                    InstrumentArt::EtfPhysischUcits
                } else {
                    InstrumentArt::Aktie
                },
            });
        }
        let kurse = Kurse::neu(zeit, kurse).map_err(|e| format!("{e:?}"))?;
        let mut topf = Topf::neu(TopfId::A, Cent(bargeld)).map_err(|e| format!("{e:?}"))?;
        for (s, stueck) in bestand {
            topf.setze_bestand(isin(s)?, *stueck);
        }
        let mut zielgewichte_bp = BTreeMap::new();
        let mut begruendung = BTreeMap::new();
        for (s, bp) in ziele {
            zielgewichte_bp.insert(isin(s)?, *bp);
            begruendung.insert(isin(s)?, "beobachtet".to_owned());
        }
        let park = match park {
            Some((s, reserve_bp, naechster_kauf_tage)) => Some(Parkwunsch {
                isin: isin(s)?,
                reserve_bp,
                naechster_kauf_tage,
                begruendung: "nichts in Sicht".into(),
            }),
            None => None,
        };
        let vorschlag = Vorschlag {
            erwartung_bp: BTreeMap::new(),
            limit_bp: BTreeMap::new(),
            stopp_bp: BTreeMap::new(),
            berater: BeraterId::Dachs,
            zielgewichte_bp,
            begruendung,
            park,
        };
        let universum = Universum::neu(instrumente);
        let e = Eingabe {
            grenzen: &Grenzen::standard(),
            universum: &universum,
            topf: &topf,
            kurse: &kurse,
            vorschlag: Some(&vorschlag),
            halt: false,
            vor_valuta: &[],
        };
        let mut vergabe = Kennungsvergabe::nach(0);
        Ok(match kaufbudget {
            Some(frei) => entscheide_mit_kaufbudget(&e, &mut vergabe, Cent(frei)),
            None => entscheide(&e, &mut vergabe),
        })
    }

    fn kauf(e: &Entscheidung, s: &str) -> Result<Option<u64>, String> {
        let i = isin(s)?;
        Ok(e.kaeufe
            .iter()
            .find(|k| k.auftrag().isin == i)
            .map(|k| k.auftrag().stueck))
    }

    /// Gegenprobe (a): Topf 10.000 €, Reserve 10 %, Bargeld 6.000 €: Parkziel 50 % auf 40 %
    /// gekappt; der Kauf bleibt unter 40 % des kleinsten Topfwerts nach Limit und Gebühr.
    #[test]
    fn parkt_bargeld_ueber_reserve_bis_hoechstanteil() -> Result<(), String> {
        let e = lauf(
            600_000,
            &[(SAP, 40)],
            &[(SAP, 4_000)],
            Some((ETF, 1_000, 5)),
            None,
        )?;
        assert!(e.befunde.contains(&Befund::Parkwert {
            isin: isin(ETF)?,
            ziel_bp: 4_000
        }));
        let stueck = kauf(&e, ETF)?.ok_or("Parkkauf fehlt")?;
        assert!((38..=40).contains(&stueck), "{stueck}");
        Ok(())
    }

    /// Gegenprobe (b): Erwartet die KI in 2 Handelstagen einen Kauf, wird nicht geparkt.
    #[test]
    fn kein_parken_bei_baldigem_kauf() -> Result<(), String> {
        let e = lauf(
            600_000,
            &[(SAP, 40)],
            &[(SAP, 4_000)],
            Some((ETF, 1_000, 2)),
            None,
        )?;
        assert_eq!(kauf(&e, ETF)?, None);
        assert!(e.befunde.contains(&Befund::Parkwert {
            isin: isin(ETF)?,
            ziel_bp: 0
        }));
        Ok(())
    }

    /// Unter 1.000 € Zukauf lohnt das Parken nicht.
    #[test]
    fn kein_parken_unter_mindestzukauf() -> Result<(), String> {
        // 1.500 € Bargeld, Reserve 6 %: nur 900 € wären zu parken.
        let bestand = [(SAP, 40), (ALV, 40), (BASF, 5)];
        let ziele = [(SAP, 4_000), (ALV, 4_000), (BASF, 500)];
        let e = lauf(150_000, &bestand, &ziele, Some((ETF, 600, 5)), None)?;
        assert_eq!(kauf(&e, ETF)?, None);
        Ok(())
    }

    /// Gegenprobe (c): Zielkauf 3.000 € bei 500 € Bargeld: zuerst wird der Parkwert verkauft,
    /// der Kauf ist bis zum Erlös auf das Bargeld begrenzt.
    #[test]
    fn parkwert_wird_zuerst_verkauft() -> Result<(), String> {
        let e = lauf(
            50_000,
            &[(SAP, 30), (ALV, 30), (ETF, 35)],
            &[(SAP, 3_000), (ALV, 3_000), (BASF, 3_000)],
            Some((ETF, 500, 5)),
            Some(50_000),
        )?;
        let etf = isin(ETF)?;
        let verkauf = e
            .verkaeufe
            .iter()
            .find(|v| v.auftrag().isin == etf)
            .ok_or("Parkverkauf fehlt")?;
        assert_eq!(verkauf.auftrag().stueck, 30);
        assert_eq!(verkauf.beleg().anlass, Anlass::Umschichtung);
        assert!(kauf(&e, BASF)?.is_none_or(|s| s <= 4), "{e:?}");
        Ok(())
    }

    /// Der Parkplatz zählt nicht gegen die Werteanzahl: Kauf neben vier Aktien, kein Abbau.
    #[test]
    fn parkwert_zaehlt_nicht_gegen_werteanzahl() -> Result<(), String> {
        let vier = [(SAP, 15), (ALV, 15), (BASF, 15), (BAYER, 15)];
        let ziele = [(SAP, 1_500), (ALV, 1_500), (BASF, 1_500), (BAYER, 1_500)];
        let e = lauf(400_000, &vier, &ziele, Some((ETF, 0, 10)), None)?;
        assert!(kauf(&e, ETF)?.is_some(), "{e:?}");
        let gehalten = [(SAP, 15), (ALV, 15), (BASF, 15), (BAYER, 15), (ETF, 20)];
        let mit_etf = [
            (SAP, 1_500),
            (ALV, 1_500),
            (BASF, 1_500),
            (BAYER, 1_500),
            (ETF, 2_000),
        ];
        let e = lauf(200_000, &gehalten, &mit_etf, None, None)?;
        assert!(
            !e.befunde
                .iter()
                .any(|b| matches!(b, Befund::ZuVielePositionen { .. })),
            "{e:?}"
        );
        assert!(e.verkaeufe.is_empty(), "{e:?}");
        Ok(())
    }

    /// Ein Parkwert, der kein ETF ist, wird nicht verwendet.
    #[test]
    fn aktie_ist_kein_parkwert() -> Result<(), String> {
        let e = lauf(
            600_000,
            &[(SAP, 40)],
            &[(SAP, 4_000)],
            Some((ALV, 1_000, 5)),
            None,
        )?;
        assert!(e.befunde.contains(&Befund::ParkwertUngueltig(isin(ALV)?)));
        assert_eq!(kauf(&e, ALV)?, None);
        Ok(())
    }
}
