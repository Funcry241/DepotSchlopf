//! Die EUNL-Kernanlage ist eine zusätzliche Zielposition des Papierlaufs.
use std::collections::BTreeMap;

use depot_journal::Stand;
use depot_kern::{Cent, Isin, Preis, Topf, Vorschlag};
use depot_regelwerk::{Eingabe, Entscheidung, Richtung};

pub const KERNANLAGE: &str = "Kernanlage";
const EUNL: &str = "IE00B4L5Y983";
const ZIEL_BP: i64 = 3_000;
pub(crate) const LIMIT_BP: i64 = 100;

pub(crate) fn veto_gegen_kernanlage(
    ursprung: &Vorschlag,
    urteil: Option<&crate::teilveto::Urteilsfolge>,
) -> bool {
    use crate::teilveto::Urteilsfolge;
    match urteil {
        Some(Urteilsfolge::Teilveto { abgelehnt, .. }) => {
            abgelehnt.iter().any(|isin| isin.als_str() == EUNL)
        }
        Some(Urteilsfolge::Veto) => {
            ursprung
                .zielgewichte_bp
                .keys()
                .any(|isin| isin.als_str() == EUNL)
                || ursprung
                    .park
                    .as_ref()
                    .is_some_and(|park| park.isin.als_str() == EUNL)
        }
        _ => false,
    }
}

pub fn ist_kernposition(stand: &Stand, topf: &Topf, isin: &Isin) -> bool {
    isin.als_str() == EUNL
        && topf.stueck(isin) > 0
        && stand
            .ausfuehrungsfolge()
            .filter(|(auftrag, _)| {
                auftrag.topf == topf.id()
                    && auftrag.isin == *isin
                    && auftrag.richtung == Richtung::Kauf
            })
            .max_by_key(|(auftrag, fill)| (fill.kurszeit, auftrag.nr))
            .is_some_and(|(auftrag, _)| ist_kernauftrag(stand, auftrag.nr))
}

pub fn ist_kernauftrag(stand: &Stand, nr: u64) -> bool {
    let kennung = nr.to_string();
    stand
        .begruendungen()
        .iter()
        .any(|(_, wer, grund)| wer == KERNANLAGE && grund == &kennung)
}

pub(crate) fn wahre_positionsgrenze(
    e: &Eingabe<'_>,
    plan: &mut Entscheidung,
    stand: &Stand,
    aktiv: bool,
) {
    let Ok(kern) = Isin::neu(EUNL) else { return };
    if !aktiv && !ist_kernposition(stand, e.topf, &kern) {
        return;
    }
    let mut bestand = e.topf.positionen().clone();
    for order in &plan.verkaeufe {
        let order = order.auftrag();
        if let Some(stueck) = bestand.get_mut(&order.isin) {
            *stueck = stueck.saturating_sub(order.stueck);
        }
    }
    bestand.retain(|_, stueck| *stueck > 0);
    if !bestand.contains_key(&kern) && !plan.kaeufe.iter().any(|k| k.auftrag().isin == kern) {
        return;
    }
    plan.kaeufe.retain(|kauf| {
        let isin = &kauf.auftrag().isin;
        if bestand.contains_key(isin) {
            true
        } else if bestand.len() < e.grenzen.max_werte() {
            bestand.insert(isin.clone(), kauf.auftrag().stueck);
            true
        } else {
            plan.befunde
                .push(depot_regelwerk::Befund::PositionsgrenzeErreicht(
                    isin.clone(),
                ));
            false
        }
    });
}

fn positionen(e: &Eingabe<'_>, plan: &Entscheidung) -> Option<BTreeMap<Isin, u64>> {
    let mut positionen = e.topf.positionen().clone();
    for order in plan.verkaeufe.iter().chain(&plan.kaeufe) {
        let order = order.auftrag();
        let stueck = positionen.entry(order.isin.clone()).or_default();
        *stueck = match order.richtung {
            Richtung::Verkauf => stueck.checked_sub(order.stueck)?,
            Richtung::Kauf => stueck.checked_add(order.stueck)?,
        };
    }
    positionen.retain(|_, stueck| *stueck > 0);
    Some(positionen)
}

pub(crate) fn freizugebender_platz(
    stand: &Stand,
    e: &Eingabe<'_>,
    plan: &Entscheidung,
) -> Option<Isin> {
    let isin = Isin::neu(EUNL).ok()?;
    if !ist_kernposition(stand, e.topf, &isin)
        || eigene_ki_anlage(e, &isin)
        || plan.verkaeufe.iter().any(|v| v.auftrag().isin == isin)
    {
        return None;
    }
    let nachher = positionen(e, plan)?;
    (nachher.contains_key(&isin)
        && nachher.len() > e.grenzen.max_werte()
        && plan
            .kaeufe
            .iter()
            .any(|k| e.topf.stueck(&k.auftrag().isin) == 0))
    .then_some(isin)
}

pub(crate) fn platz_geraeumt(e: &Eingabe<'_>, plan: &Entscheidung, kern: &Isin) -> bool {
    let Some(nachher) = positionen(e, plan) else {
        return false;
    };
    !nachher.contains_key(kern)
        && nachher.len() == e.grenzen.max_werte()
        && plan
            .kaeufe
            .iter()
            .any(|k| e.topf.stueck(&k.auftrag().isin) == 0)
}

fn eigene_ki_anlage(e: &Eingabe<'_>, isin: &Isin) -> bool {
    e.vorschlag.is_some_and(|v| {
        (v.zielgewichte_bp.contains_key(isin)
            && v.begruendung
                .get(isin)
                .is_none_or(|grund| grund != crate::ausgelassen::UNVERAENDERT))
            || v.park.as_ref().is_some_and(|p| &p.isin == isin)
    })
}

fn ziel(e: &Eingabe<'_>, plan: &Entscheidung, kern: &Isin) -> Option<(i64, usize)> {
    let mut positionen = positionen(e, plan)?;
    positionen.remove(kern);
    let wert = positionen
        .iter()
        .try_fold(Preis::aus_cent(Cent::NULL)?, |summe, (isin, stueck)| {
            summe.checked_add(e.kurse.kurs(isin)?.mal_stueck_exakt(*stueck)?)
        })?;
    let eingesetzt = wert.in_bp_von(e.topf.wert_exakt(e.kurse)?)?;
    Some((ZIEL_BP.saturating_sub(eingesetzt).max(0), positionen.len()))
}

fn kaufbedarf(e: &Eingabe<'_>, plan: &Entscheidung) -> Option<Cent> {
    plan.kaeufe.iter().try_fold(Cent::NULL, |summe, kauf| {
        let kauf = kauf.auftrag();
        let volumen = kauf.limit.mal_stueck(kauf.stueck)?;
        summe.checked_add(volumen.checked_add(e.grenzen.gebuehren().fuer(volumen)?)?)
    })
}

fn verkaufsziel(e: &Eingabe<'_>, isin: &Isin, ist_bp: i64, fehlbetrag: Cent) -> Option<i64> {
    if fehlbetrag <= Cent::NULL {
        return Some(ist_bp);
    }
    let limit = e
        .kurse
        .kurs(isin)?
        .mal_bp(10_000_i64.checked_sub(LIMIT_BP)?, depot_kern::Rundung::Ab)?;
    let volumen = fehlbetrag
        .checked_add(e.grenzen.gebuehren().fuer(fehlbetrag)?)?
        .max(e.grenzen.min_order());
    let je_stueck = limit.0;
    let abgabe = volumen
        .0
        .checked_add(je_stueck.checked_sub(1)?)?
        .checked_div(je_stueck)?;
    let rest = e
        .topf
        .stueck(isin)
        .saturating_sub(u64::try_from(abgabe).ok()?);
    e.kurse
        .kurs(isin)?
        .mal_stueck_exakt(rest)?
        .in_bp_von(e.topf.wert_exakt(e.kurse)?)
}

fn haltebestand(e: &Eingabe<'_>) -> Vorschlag {
    let mut vorschlag = Vorschlag {
        berater: e.topf.id().berater(),
        erwartung_bp: BTreeMap::new(),
        zielgewichte_bp: BTreeMap::new(),
        limit_bp: BTreeMap::new(),
        stopp_bp: BTreeMap::new(),
        begruendung: BTreeMap::new(),
        belege: BTreeMap::new(),
        park: None,
    };
    crate::ausgelassen::ergaenze_ausgelassene(e.topf, e.kurse, &mut vorschlag);
    vorschlag
}

/// Ändert nur das EUNL-Ziel. Der aufrufende Papierlauf lässt denselben Schutz erneut planen.
pub(crate) fn vorschlag(
    stand: &Stand,
    e: &Eingabe<'_>,
    basis: &Entscheidung,
    frei: Cent,
    probe: impl FnOnce(&Vorschlag, Cent) -> Entscheidung,
) -> Option<(Isin, Vorschlag)> {
    let isin = Isin::neu(EUNL).ok()?;
    if !e.universum.handelbar(&isin, e.grenzen) || e.kurse.kurs(&isin).is_none() {
        return None;
    }
    if eigene_ki_anlage(e, &isin) {
        return None;
    }
    if basis
        .verkaeufe
        .iter()
        .any(|order| order.auftrag().isin == isin)
    {
        return None;
    }
    let stueck = e.topf.stueck(&isin);
    let hat_kern = stueck > 0;
    if hat_kern && !ist_kernposition(stand, e.topf, &isin) {
        return None;
    }
    let ist_bp = depot_regelwerk::entscheidung::ist_gewicht_bp(e.topf, e.kurse, &isin)?;
    let (angestrebter_kern, andere) = ziel(e, basis, &isin)?;
    let mut ziel_bp = angestrebter_kern.max(ist_bp);
    if hat_kern && let Some(ki_ziele) = e.vorschlag {
        let kernwert = e
            .kurse
            .kurs(&isin)?
            .mal_stueck(stueck, depot_kern::Rundung::Ab)?;
        let mut ki_ziele = ki_ziele.clone();
        ki_ziele.zielgewichte_bp.insert(isin.clone(), 0);
        let mit_kern = probe(&ki_ziele, frei.checked_add(kernwert)?);
        let (restziel, weitere) = ziel(e, &mit_kern, &isin)?;
        if !mit_kern.kaeufe.is_empty() {
            let fehlbetrag = kaufbedarf(e, &mit_kern)?.checked_sub(frei)?;
            let (bisheriger_bedarf, _) = ziel(e, &Entscheidung::default(), &isin)?;
            if (bisheriger_bedarf > 0 && restziel == 0) || weitere >= e.grenzen.max_werte() {
                ziel_bp = 0;
            } else if fehlbetrag > Cent::NULL {
                ziel_bp = verkaufsziel(e, &isin, ist_bp, fehlbetrag)?;
            }
        }
    }
    if andere >= e.grenzen.max_werte() && !hat_kern {
        return None;
    }
    if ziel_bp > ist_bp {
        let rest = frei.checked_sub(kaufbedarf(e, basis)?)?;
        if rest < e.grenzen.min_order() {
            return None;
        }
        let zusatz = Preis::aus_cent(rest)?.in_bp_von(e.topf.wert_exakt(e.kurse)?)?;
        ziel_bp = ziel_bp.min(ist_bp.checked_add(zusatz)?);
    }
    if ziel_bp == ist_bp
        || (ziel_bp > ist_bp && ziel_bp.checked_sub(ist_bp)? < e.grenzen.min_aenderung_bp())
    {
        return None;
    }
    let mut vorschlag = e.vorschlag.cloned().unwrap_or_else(|| haltebestand(e));
    vorschlag.zielgewichte_bp.insert(isin.clone(), ziel_bp);
    vorschlag.limit_bp.insert(isin.clone(), LIMIT_BP);
    vorschlag
        .begruendung
        .insert(isin.clone(), KERNANLAGE.into());
    Some((isin, vorschlag))
}
