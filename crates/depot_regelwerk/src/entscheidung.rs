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
    BP_GANZ, BeraterId, Cent, Grenzen, Herkunft, Isin, Kurse, Preis, Rundung, Topf, TopfId,
    Universum, Vorschlag,
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

#[derive(Clone, Debug)]
struct Kandidat {
    isin: Isin,
    richtung: Richtung,
    stueck: u64,
    /// Exakter Kurs; ganze Cent entstehen erst beim Runden zur sicheren Seite.
    kurs: Preis,
    komplett: bool,
    anlass: Anlass,
}

impl Kandidat {
    /// Limit: Kauf hoechstens den Limit-Abstand ueber, Verkauf hoechstens darunter.
    fn limit(&self, g: &Grenzen) -> Option<Cent> {
        limit_fuer(g, self.kurs, self.richtung)
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
fn limit_fuer(g: &Grenzen, kurs: Preis, richtung: Richtung) -> Option<Cent> {
    let abstand = g.limit_abstand_bp();
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

pub fn entscheide(e: &Eingabe<'_>, vergabe: &mut Kennungsvergabe) -> Entscheidung {
    let mut befunde = Vec::new();
    let Some((verkaeufe, kaeufe)) = plane(e, &mut befunde) else {
        return Entscheidung::nur(Befund::Rechenfehler);
    };
    let Some(mut ent) = genehmige(e.grenzen, &verkaeufe, &kaeufe, vergabe) else {
        return Entscheidung::nur(Befund::Rechenfehler);
    };
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
fn plane(e: &Eingabe<'_>, befunde: &mut Vec<Befund>) -> Option<(Vec<Kandidat>, Vec<Kandidat>)> {
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

    let ziele = if e.halt {
        befunde.push(Befund::Halt);
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
                Ok(ziele) => Some(ziele),
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
                tw,
                ziele,
                &gesperrt,
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
    if ziele.len() > e.grenzen.max_werte() {
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
    Ok(ziele)
}

/// Freiwillige Auftraege. Umschlag und Orderzahl gelten fuer das, was die zwingenden
/// Auftraege uebrig lassen; Deckung, Kaufgrenze und Positionszahl fuer alle Auftraege.
#[allow(
    clippy::too_many_arguments,
    reason = "die Planungsschritte teilen genau diese Zwischenstaende"
)]
fn freiwillig(
    e: &Eingabe<'_>,
    topfwert: Preis,
    ziele: &BTreeMap<Isin, i64>,
    gesperrt: &BTreeSet<Isin>,
    verkaeufe: &mut Vec<Kandidat>,
    kaeufe: &mut Vec<Kandidat>,
    befunde: &mut Vec<Befund>,
) -> Option<()> {
    let g = e.grenzen;
    let mut kandidaten = sammle_kandidaten(e, ziele, topfwert, gesperrt, befunde)?;

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
    kaufe(e, topfwert, verkaeufe, freie_kaeufe, kaeufe, befunde)
}

/// Kaeufe mit Deckung (Bargeld plus garantierter Erloes aller Verkaeufe), Kaufgrenze des
/// Anteils und Kaufgrenze der Positionszahl, alles am Limit mit hoechster Gebuehr.
fn kaufe(
    e: &Eingabe<'_>,
    topfwert: Preis,
    verkaeufe: &[Kandidat],
    geplant: Vec<Kandidat>,
    kaeufe: &mut Vec<Kandidat>,
    befunde: &mut Vec<Befund>,
) -> Option<()> {
    let g = e.grenzen;
    let mut budget = e.topf.bargeld();
    let mut verlust = Cent::NULL;
    for k in verkaeufe {
        let erloes = k.volumen(g)?;
        budget = budget
            .checked_add(erloes)?
            .checked_sub(g.gebuehren().fuer(erloes)?)?;
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
    let mut positionen = e
        .topf
        .positionen()
        .keys()
        .filter(|i| !ganz_verkauft.contains(i))
        .count();

    for k in geplant {
        let gehalten = e.topf.stueck(&k.isin);
        if gehalten == 0 && positionen >= g.max_werte() {
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
        if gehalten == 0 {
            positionen = positionen.checked_add(1)?;
        }
        kaeufe.push(Kandidat { stueck, ..k });
    }
    Some(())
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
                komplett: komplett || stueck == gehalten,
                anlass: Anlass::Umschichtung,
            }
        } else {
            Kandidat {
                isin: isin.clone(),
                richtung: Richtung::Kauf,
                stueck: stueck_fuer(ziel_wert.checked_sub(ist_wert)?, kurs)?,
                kurs,
                komplett: false,
                anlass: Anlass::Umschichtung,
            }
        };
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
