//! Beratung auf einem eingefrorenen Depotstand. Nur die KI-Kosten gehen ins Handelsjournal.
use std::path::Path;

use depot_journal::{AufrufErlaubnis, Journal, KiZweck};
use depot_kern::{Cent, Datum, Grenzen, Kurse, Topf, TopfId, Universum, Vorschlag, Zeitpunkt};
use depot_ki::{KiDeckel, Sperre};
use depot_regelwerk::{Eingabe, Kennungsvergabe, entscheide_ersatzkauf, entscheide_mit_kaufbudget};

use crate::abstimmung::{AbstimmungsStand, gleiche_ziele};
use crate::szenario::{Antwort, Berater, Lage};
use crate::{papiertag, vorbereitung};

const RUNDEN: u32 = crate::abstimmung::ABSTIMMUNG_RUNDEN;

/// Pro KI höchstens vier Fragen: Erstmeinung, Prüfung, zwei Abstimmungsrunden.
pub fn hoechstkosten() -> Option<Cent> {
    let dachs = KiDeckel::standard(depot_kern::BeraterId::Dachs).hoechstkosten()?;
    let maus = KiDeckel::standard(depot_kern::BeraterId::Maus).hoechstkosten()?;
    dachs
        .checked_add(maus)?
        .mal_stueck(u64::from(RUNDEN.checked_add(2)?))
}

struct Budget {
    datum: Datum,
    zyklus: u32,
}
impl Budget {
    fn frage<T>(
        &self,
        j: &mut Journal,
        b: &mut dyn Berater,
        id: TopfId,
        frage: impl FnOnce(&mut dyn Berater, AufrufErlaubnis) -> T,
    ) -> Result<Result<T, String>, String> {
        if !b.angebunden() || b.kostensperre() {
            return Ok(Err(b
                .warum_nicht()
                .unwrap_or_else(|| "KI nicht verfügbar".into())));
        }
        let erlaubnis =
            match j.reserviere_fuer(id.berater(), self.datum, self.zyklus, KiZweck::Simulation) {
                Ok(e) => e,
                Err(Sperre::Kassenbuch(e)) => return Err(e),
                Err(e) => return Ok(Err(format!("Kostenschutz ({e:?})"))),
            };
        let antwort = frage(b, erlaubnis);
        crate::lauf::belege_ki(j, b).map_err(|e| format!("Kassenbuch: {e:?}"))?;
        Ok(Ok(antwort))
    }
}

struct Stand {
    id: TopfId,
    text: String,
    vorschlag: Option<Vorschlag>,
    verwertbar: bool,
    zugestimmt: bool,
    ersatzkauf: bool,
}

fn text(s: &str) -> String {
    vorbereitung::text_fuer_frage(s)
}

fn ausgabe(phase: &str, id: TopfId, s: &str) {
    println!("[Simulation] {phase} {:?}: {}", id.berater(), text(s));
}

fn zeige_ziele(phase: &str, id: TopfId, v: Option<&Vorschlag>) {
    if let Some(v) = v {
        if v.zielgewichte_bp.is_empty() {
            ausgabe(phase, id, "Ziel: Bargeld");
        }
        for (isin, bp) in &v.zielgewichte_bp {
            let grund = v.begruendung.get(isin).map_or("", String::as_str);
            ausgabe(phase, id, &format!("{isin} · {bp} Basispunkte · {grund}"));
        }
    }
}

fn erste(
    j: &mut Journal,
    budget: &Budget,
    b: &mut dyn Berater,
    lage: &Lage<'_>,
) -> Result<Stand, String> {
    let id = lage.topf.id();
    let antwort = budget.frage(j, b, id, |b, e| b.vorschlagen(e, lage))?;
    let a = antwort.unwrap_or_else(|grund| Antwort {
        halten: std::collections::BTreeMap::new(),
        prognose: None,
        verwertbar: false,
        einschaetzung: grund,
        vorschlag: None,
    });
    if a.verwertbar
        && let Some(pruefung) = b.thesenpruefung()
        && let Some(text) = crate::schlopfmodell::pruefung_text(&pruefung)
    {
        ausgabe("Thesenprüfung", id, &text);
    }
    let begruendung = if a.verwertbar {
        a.einschaetzung
    } else {
        format!("Ausgefallen · {}", a.einschaetzung)
    };
    ausgabe("Erstmeinung", id, &begruendung);
    let vorschlag = a.vorschlag.filter(|_| a.verwertbar);
    zeige_ziele("Erstmeinung", id, vorschlag.as_ref());
    Ok(Stand {
        id,
        text: begruendung,
        vorschlag,
        verwertbar: a.verwertbar,
        zugestimmt: false,
        ersatzkauf: false,
    })
}

fn zweitmeinung(
    j: &mut Journal,
    budget: &Budget,
    b: &mut dyn Berater,
    lage: &Lage<'_>,
    stand: &mut Stand,
) -> Result<(), String> {
    let Some(v) = stand.vorschlag.as_ref() else {
        return Ok(());
    };
    if v.zielgewichte_bp.is_empty() && v.park.is_none() && lage.topf.positionen().is_empty() {
        return Ok(());
    }
    let phase = format!("Zweitmeinung zu {:?}", stand.id.berater());
    let pruefer = match stand.id {
        TopfId::A => TopfId::B,
        TopfId::B => TopfId::A,
    };
    match budget.frage(j, b, pruefer, |b, e| b.pruefen(e, lage, v))? {
        Ok(u) => {
            stand.zugestimmt = u.verwertbar && u.zustimmung;
            stand.ersatzkauf =
                !u.verwertbar && b.zweitmeinung_technisch_ausgefallen() && !b.kostensperre();
            let urteil = if !u.verwertbar {
                "Ausgefallen"
            } else if u.zustimmung {
                "Zustimmung"
            } else {
                "Veto"
            };
            ausgabe(&phase, pruefer, &format!("{urteil} · {}", u.begruendung));
        }
        Err(e) => ausgabe(&phase, pruefer, &format!("Ausgefallen · {e}")),
    }
    Ok(())
}

fn bargeld(s: &Stand) -> bool {
    s.verwertbar
        && s.vorschlag
            .as_ref()
            .is_some_and(|v| v.zielgewichte_bp.is_empty() && v.park.is_none())
}

struct Dialog<'a> {
    budget: &'a Budget,
    lagen: [&'a Lage<'a>; 2],
}

fn abstimmen(
    j: &mut Journal,
    dialog: &Dialog<'_>,
    berater: [&mut dyn Berater; 2],
    staende: [&mut Stand; 2],
) -> Result<(), String> {
    let [a, b] = staende;
    let [da, ma] = berater;
    let [lage_a, lage_b] = dialog.lagen;
    if !a.verwertbar || !b.verwertbar || !da.kann_abstimmen() || !ma.kann_abstimmen() {
        return Ok(());
    }
    for runde in 1..=RUNDEN {
        if bargeld(a) && bargeld(b) {
            println!("[Simulation] Beide wählen ausdrücklich Bargeld.");
            break;
        }
        let phase = format!("Abstimmung Runde {runde}");
        let frage = |eigen: &Stand, fremd: &Stand| AbstimmungsStand {
            runde,
            eigener_vorschlag: eigen.vorschlag.clone(),
            fremder_vorschlag: fremd.vorschlag.clone(),
            eigene_erstmeinung: eigen.text.clone(),
            fremde_beitraege: fremd.text.clone(),
        };
        a.ersatzkauf = false;
        b.ersatzkauf = false;
        let mut ergebnisse = Vec::with_capacity(2);
        for (eigen, fremd, lage, ki) in [(&*a, &*b, lage_a, &mut *da), (&*b, &*a, lage_b, &mut *ma)]
        {
            let fragen = frage(eigen, fremd);
            let e = dialog
                .budget
                .frage(j, ki, eigen.id, |k, e| k.abstimmen(e, lage, &fragen))?;
            let ergebnis = match e {
                Ok(e) if e.eigene.verwertbar => {
                    let urteil = if fremd.vorschlag.is_none() {
                        "Kein fremder Vorschlag"
                    } else if e.zustimmung_fremd {
                        "Zustimmung zum fremden Vorschlag"
                    } else {
                        "Keine Zustimmung zum fremden Vorschlag"
                    };
                    ausgabe(
                        &phase,
                        eigen.id,
                        &format!("{urteil} · {}", e.eigene.einschaetzung),
                    );
                    zeige_ziele(&phase, eigen.id, e.eigene.vorschlag.as_ref());
                    Some(e)
                }
                Ok(e) => {
                    ausgabe(
                        &phase,
                        eigen.id,
                        &format!("Ausgefallen · {}", e.eigene.einschaetzung),
                    );
                    None
                }
                Err(e) => {
                    ausgabe(&phase, eigen.id, &format!("Ausgefallen · {e}"));
                    None
                }
            };
            ergebnisse.push(ergebnis);
        }
        let mut e = ergebnisse.into_iter();
        let ea = e.next().flatten();
        let eb = e.next().flatten();
        let hat_beide = ea.is_some() && eb.is_some();
        let mut gleich = true;
        let ja = ea.as_ref().is_some_and(|e| e.zustimmung_fremd);
        let jb = eb.as_ref().is_some_and(|e| e.zustimmung_fremd);
        for (s, antwort, zustimmung) in [(&mut *a, ea, jb), (&mut *b, eb, ja)] {
            if let Some(antwort) = antwort {
                let unveraendert =
                    gleiche_ziele(s.vorschlag.as_ref(), antwort.eigene.vorschlag.as_ref());
                gleich &= unveraendert;
                s.zugestimmt = hat_beide && unveraendert && zustimmung;
                s.text = antwort.eigene.einschaetzung;
                s.vorschlag = antwort.eigene.vorschlag;
            } else {
                s.zugestimmt = false;
            }
        }
        if !hat_beide || gleich {
            break;
        }
    }
    Ok(())
}

fn plan(
    j: &Journal,
    grenzen: &Grenzen,
    universum: &Universum,
    lage: &Lage<'_>,
    s: &Stand,
    halt: bool,
) {
    let v = s.vorschlag.as_ref().filter(|v| {
        s.zugestimmt
            || s.ersatzkauf
            || (lage.topf.positionen().is_empty()
                && v.zielgewichte_bp.is_empty()
                && v.park.is_none())
    });
    let vor_valuta: Vec<_> = j
        .stand()
        .vor_valuta(lage.topf.id(), lage.datum)
        .into_iter()
        .map(|(isin, _)| isin)
        .collect();
    let eingabe = Eingabe {
        grenzen,
        universum,
        topf: lage.topf,
        kurse: lage.kurse,
        vorschlag: v,
        halt,
        vor_valuta: &vor_valuta,
    };
    let mut vergabe = Kennungsvergabe::nach(0);
    let entscheidung = if s.ersatzkauf && !halt {
        ausgabe(
            "Regelprüfung",
            s.id,
            "Technischer Ausfall der Zweitmeinung · Ersatzkauf mit halbierten Kaufzielgewichten",
        );
        entscheide_ersatzkauf(&eingabe, &mut vergabe, lage.kaufbar)
    } else {
        entscheide_mit_kaufbudget(&eingabe, &mut vergabe, lage.kaufbar)
    };
    if entscheidung.kaeufe.is_empty() && entscheidung.verkaeufe.is_empty() {
        let grund = if !s.verwertbar {
            "Kein bestätigtes Ergebnis · Erstmeinung ausgefallen"
        } else if bargeld(s) && lage.topf.positionen().is_empty() {
            "Bargeld · kein Auftrag"
        } else if v.is_none() {
            "Kein bestätigter Handelsvorschlag"
        } else {
            "Regelwerk lässt keinen Auftrag zu"
        };
        ausgabe("Ergebnis", s.id, grund);
    }
    for befund in entscheidung.befunde {
        ausgabe("Regelprüfung", s.id, &format!("{befund:?}"));
    }
    for auftrag in entscheidung.verkaeufe.iter().chain(&entscheidung.kaeufe) {
        let a = auftrag.auftrag();
        if j.stand()
            .offene_auftraege()
            .any(|o| o.beleg.topf == s.id && o.beleg.isin == a.isin)
        {
            ausgabe(
                "Regelprüfung",
                s.id,
                &format!("{} · bereits eine offene Order", a.isin),
            );
            continue;
        }
        let volumen = a.limit.mal_stueck(a.stueck);
        let gebuehr = volumen.and_then(|volumen| grenzen.gebuehren().fuer(volumen));
        let huerde = if a.richtung == depot_regelwerk::Richtung::Kauf {
            volumen
                .zip(gebuehr)
                .and_then(|(v, g)| {
                    crate::kostenhuerde::hinweis(v, g, depot_kern::liste::boerse_von(&a.isin))
                })
                .map_or_else(
                    || " · Kostenhürde nicht berechenbar".into(),
                    |text| format!(" · {text}"),
                )
        } else {
            String::new()
        };
        let gebuehr = gebuehr.map_or_else(|| "offen".into(), |c| c.to_string());
        ausgabe(
            "Vorschau",
            s.id,
            &format!(
                "{:?} {} · {} Stück · Limit {} · geschätzte Gebühr {gebuehr} · Ausführung offen{huerde}",
                a.richtung, a.isin, a.stueck, a.limit
            ),
        );
    }
}

struct Markt<'a> {
    datum: Datum,
    grenzen: Grenzen,
    kurse: &'a Kurse,
    ereignisse: &'a [String],
    verlauf: &'a [(depot_kern::Isin, crate::kursverlauf::Verlauf)],
    relative_staerke: &'a [(depot_kern::Isin, i64)],
    relativ_kurz: bool,
}

impl Markt<'_> {
    fn lage<'a>(
        &'a self,
        j: &Journal,
        topf: &'a Topf,
        positionen: &'a [crate::Positionskauf],
    ) -> Result<Lage<'a>, String> {
        let kaufbar = if j.stand().toepfe().is_empty() {
            topf.bargeld()
        } else {
            crate::lauf::kaufbudget(j, &self.grenzen, topf.id()).map_err(|e| format!("{e:?}"))?
        };
        Ok(Lage {
            datum: self.datum,
            topf,
            kaufbar,
            kurse: self.kurse,
            ereignisse: self.ereignisse,
            verlauf: self.verlauf,
            relative_staerke: self.relative_staerke,
            relativ_kurz: self.relativ_kurz,
            positionskaeufe: positionen,
            anlass: Some(
                "Manuelle Handelsvorschau auf belegten Schlusskursen. Keine Order und kein bekannter Ausführungskurs. Wer ausdrücklich alles in Bargeld halten will, gibt eine leere Zielliste an.",
            ),
            kaufbilanz: crate::ausfuehrung::kaufbilanz(j.stand(), topf.id(), self.datum),
        })
    }
}

/// Verwendet den geprüften Journalstand und liefert Vorschläge ausschließlich im Laufprotokoll.
/// Die Journalsperre verhindert parallele Reservierungen und hält den Schnappschuss konsistent.
pub fn lauf(
    wurzel: &Path,
    zeit: Zeitpunkt,
    universum: &Universum,
    dachs: &mut dyn Berater,
    maus: &mut dyn Berater,
) -> Result<(), String> {
    let daten = wurzel.join("daten");
    if daten
        .join("update_offen.txt")
        .try_exists()
        .map_err(|e| e.to_string())?
    {
        return Err("Das Update ist noch nicht bestätigt.".into());
    }
    let mut j =
        Journal::oeffnen(&daten.join("journal.txt")).map_err(|e| format!("Journal: {e:?}"))?;
    if j.stand().seit_abschluss() != 0 {
        return Err("Papiertag unterbrochen. Zuerst den Journalstand klären.".into());
    }
    let datum = zeit.frankfurt_tag().ok_or("Kein Frankfurter Datum")?;
    let fuer = if depot_kern::liste::handelstag(datum)
        && j.stand().abschluss().is_none_or(|a| a.datum < datum)
    {
        datum
    } else {
        vorbereitung::naechster_handelstag(datum).ok_or("Kein nächster Handelstag")?
    };
    let kurslage = papiertag::tageskurse_bis(j.stand(), universum, Some(zeit))
        .map_err(|e| format!("{e:?}"))?;
    let ereignisse = crate::zahlenreaktion::ereignisse(
        j.stand(),
        &kurslage.heute,
        zeit,
        papiertag::weltbelege_am(j.stand(), fuer, zeit),
    );
    let ereignisse = vorbereitung::sichtbare_ereignisse(&ereignisse);
    let budget = Budget {
        datum,
        zyklus: papiertag::tagnummer(datum).ok_or("Ungültiger Kostentag")?,
    };
    let topf = |id| {
        j.stand()
            .toepfe()
            .get(&id)
            .cloned()
            .map_or_else(|| Topf::neu(id, crate::papierlauf::START_JE_TOPF), Ok)
            .map_err(|e| format!("{e:?}"))
    };
    let a = topf(TopfId::A)?;
    let b = topf(TopfId::B)?;
    let positionen_a = crate::positionskauf::lies(j.stand(), &a);
    let positionen_b = crate::positionskauf::lies(j.stand(), &b);
    let verlauf =
        crate::kursverlauf::fuer_lage(j.stand(), &kurslage.heute, universum, kurslage.handelstag);
    let (relative_staerke, relativ_kurz) = crate::relative_staerke::fuer_frage(
        j.stand(),
        &kurslage.heute,
        universum,
        kurslage.handelstag,
    );
    let markt = Markt {
        datum: fuer,
        grenzen: Grenzen::standard(),
        kurse: &kurslage.heute,
        ereignisse: &ereignisse,
        verlauf: &verlauf,
        relative_staerke: &relative_staerke,
        relativ_kurz,
    };
    let lage_a = markt.lage(&j, &a, &positionen_a)?;
    let lage_b = markt.lage(&j, &b, &positionen_b)?;
    let halt = j.stand().notbremse_aktiv()
        || j.stand().papier_verlustsperre_aktiv()
        || depot_journal::kurssperre::kurssperre(&daten).is_some();
    println!(
        "[Simulation] Kursdatum {} · Wissensstand {} · Szenario für {fuer} · {} Meldungen und Termine",
        kurslage.handelstag,
        zeit,
        ereignisse.len()
    );
    crate::schlopfmodell::vorbereiten(&daten, &mut j, zeit, fuer, dachs, maus);
    let mut eb = erste(&mut j, &budget, maus, &lage_b)?;
    let mut ea = erste(&mut j, &budget, dachs, &lage_a)?;
    zweitmeinung(&mut j, &budget, dachs, &lage_b, &mut eb)?;
    zweitmeinung(&mut j, &budget, maus, &lage_a, &mut ea)?;
    abstimmen(
        &mut j,
        &Dialog {
            budget: &budget,
            lagen: [&lage_a, &lage_b],
        },
        [dachs, maus],
        [&mut ea, &mut eb],
    )?;
    let halt = halt || dachs.kostensperre() || maus.kostensperre();
    for (lage, s) in [(&lage_a, &ea), (&lage_b, &eb)] {
        plan(&j, &markt.grenzen, universum, lage, s, halt);
    }
    Ok(())
}
