//! Invarianten der Abstimmung und des Wiederanlaufs (docs/MATHEMATIK.md, I3 und I5b; Issue #112).
//!
//! Zwei erfundene Berater spielen je Runde einen Schritt ab: behalten, ändern, zurückziehen
//! oder unverwertbar antworten, und dabei dem Partner zustimmen oder nicht. Geprüft wird jede
//! Folge von einer Runde und zufällige Folgen bis drei Runden mit festem Startwert gegen ein
//! kleines Modell der Regel aus `docs/MATHEMATIK.md`:
//!
//! Ausgeführt wird ein Vorschlag nur, wenn sein Autor ihn zuletzt verwertbar vertreten hat und
//! der Partner genau diesem Stand in einer vollständigen Runde, in der er ihn sah, zugestimmt
//! hat (oder ihn vor der Abstimmung in der Zweitmeinung geprüft hat und sich nichts änderte).
//!
//! Erfundene Daten (D12). Keine Abhängigkeit, kein `f64`.

#![allow(
    clippy::many_single_char_names,
    reason = "Testcode: kurze Namen für Zufall, Tage und Spieler"
)]

use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{Journal, JournalFehler, KursBeleg, vermerk_fuer};
use depot_kern::{
    BeraterId, Cent, Datum, Herkunft, Instrument, InstrumentArt, Isin, Kurs, Preis, Topf, TopfId,
    Universum, Vorschlag, Zeitpunkt,
};
use depot_sim::abstimmung::{AbstimmungsAntwort, AbstimmungsStand};
use depot_sim::{Antwort, Drehbuch, Urteil, papiertag};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

const STARTWERTE: [u64; 3] = [0x5EED_0001, 0xD0C5_2026, 0x0DAC_115E];

/// xorshift64 mit festem Startwert.
#[derive(Debug)]
struct Zufall(u64);

impl Zufall {
    fn naechste(&mut self) -> u64 {
        let mut x = self.0 | 1;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn bis(&mut self, n: usize) -> usize {
        let n = u64::try_from(n).unwrap_or(u64::MAX).max(1);
        usize::try_from(self.naechste().checked_rem(n).unwrap_or(0)).unwrap_or(0)
    }
}

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

fn some<T>(o: Option<T>, was: &str) -> Result<T, String> {
    o.ok_or_else(|| format!("Vorbedingung {was}"))
}

struct Ort {
    dir: PathBuf,
    journal: PathBuf,
    konto: PathBuf,
}

impl Ort {
    fn neu() -> Result<Ort, String> {
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("depot_abstimmung_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
        let journal = dir.join("journal.txt");
        vorbedingung(Journal::einrichten(&journal), "Einrichtung")?;
        let konto = dir.join("papierkonto.txt");
        Ok(Ort {
            dir,
            journal,
            konto,
        })
    }
}

impl Drop for Ort {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn tag() -> Result<Datum, String> {
    some(Datum::neu(2026, 11, 5), "Datum")
}

fn sap() -> Result<Isin, String> {
    vorbedingung(Isin::neu("DE0007164600"), "ISIN SAP")
}

fn allianz() -> Result<Isin, String> {
    vorbedingung(Isin::neu("DE0008404005"), "ISIN Allianz")
}

fn universum() -> Result<Universum, String> {
    Ok(Universum::neu(
        [("SAP", sap()?), ("Allianz", allianz()?)]
            .into_iter()
            .map(|(name, isin)| Instrument {
                isin,
                name: name.into(),
                art: InstrumentArt::Aktie,
            })
            .collect(),
    ))
}

fn start() -> Result<Vec<Topf>, String> {
    Ok(vec![
        vorbedingung(Topf::neu(TopfId::A, Cent(500_000)), "Topf A")?,
        vorbedingung(Topf::neu(TopfId::B, Cent(500_000)), "Topf B")?,
    ])
}

fn belege(j: &mut Journal) -> Result<(), String> {
    let heute = tag()?;
    let vortag = some(Datum::neu(2026, 11, 4), "Vortag")?;
    let abruf = some(Zeitpunkt::frankfurt(heute, 17, 40, 0), "Abruf")?;
    for (isin, alt, neu) in [(sap()?, "229", "230"), (allianz()?, "350", "351")] {
        for (d, p) in [(vortag, alt), (heute, neu)] {
            let zeit = some(Zeitpunkt::frankfurt(d, 17, 35, 0), "Schluss")?;
            let preis = some(Preis::lies_euro(p), "Preis")?;
            let kurs = vorbedingung(Kurs::exakt(preis, "EUR", d, zeit, Herkunft::Quelle), "Kurs")?;
            let beleg = vorbedingung(
                KursBeleg::neu("alphavantage", abruf, p.as_bytes(), isin.clone(), kurs),
                "Beleg",
            )?;
            vorbedingung(j.belege_kurs(&beleg), "Beleg schreiben")?;
        }
    }
    Ok(())
}

/// Gewicht der Erstmeinung in Basispunkten; 2000 bp von 5.000 € sind 4 SAP-Aktien zu 230 €.
const ERST_BP: i64 = 2_000;

/// Was ein Berater in einer Runde tut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Schritt {
    /// Den eigenen, gezeigten Stand unverändert zurückgeben.
    Behalten,
    /// Auf dieses Gewicht ändern (oder nach einem Rückzug neu vorschlagen).
    Aendern(i64),
    /// Den Vorschlag zurückziehen.
    Zurueck,
    /// Keine verwertbare Antwort: Die Abstimmung endet.
    Unverwertbar,
}

const SCHRITTE: [Schritt; 5] = [
    Schritt::Behalten,
    Schritt::Aendern(1_000),
    Schritt::Aendern(3_000),
    Schritt::Zurueck,
    Schritt::Unverwertbar,
];

/// Ein Schritt mit der Haltung zum Vorschlag des Partners.
type Zug = (Schritt, bool);

/// Der Stand eines Vorschlags im Modell: das Gewicht, `None` für keinen Vorschlag.
type Stand = Option<i64>;

/// Das Modell der Regel. Liefert, welcher Vorschlag je Berater gültig bleibt, und was jeder
/// in jeder Runde als fremden Vorschlag sehen muss.
fn modell(runden: &[(Zug, Zug)]) -> ([Stand; 2], Vec<Stand>, Vec<Stand>) {
    let mut gezeigt: [Stand; 2] = [Some(ERST_BP), Some(ERST_BP)];
    // Nach der Zweitmeinung mit Zustimmung ist die Erstmeinung freigegeben.
    let mut frei: [Stand; 2] = gezeigt;
    let (mut sieht_a, mut sieht_b) = (Vec::new(), Vec::new());
    let antwort = |s: Schritt, eigen: Stand| match s {
        Schritt::Behalten => Some(eigen),
        Schritt::Aendern(bp) => Some(Some(bp)),
        Schritt::Zurueck => Some(None),
        Schritt::Unverwertbar => None,
    };
    // Nach dem letzten Zug antwortet jeder Spieler unverwertbar; das beendet die Abstimmung.
    let ende = (
        (Schritt::Unverwertbar, false),
        (Schritt::Unverwertbar, false),
    );
    for &((sa, za), (sb, zb)) in runden.iter().chain(std::iter::once(&ende)) {
        let [va, vb] = gezeigt;
        sieht_a.push(vb);
        let Some(na) = antwort(sa, va) else { break };
        // Eine verwertbare neue Antwort nimmt der alten Freigabe sofort die Gültigkeit.
        if na != frei[0] {
            frei[0] = None;
        }
        sieht_b.push(va);
        let Some(nb) = antwort(sb, vb) else { break };
        if nb != frei[1] {
            frei[1] = None;
        }
        // Freigegeben ist genau ein unverändert vertretener Stand, dem der Partner zustimmt.
        frei[0] = (na == va && zb).then_some(na).flatten();
        frei[1] = (nb == vb && za).then_some(nb).flatten();
        gezeigt = [na, nb];
        if na == va && nb == vb {
            break;
        }
    }
    (frei, sieht_a, sieht_b)
}

struct Spieler {
    wer: BeraterId,
    plan: Drehbuch,
    zuege: VecDeque<Zug>,
    gesehen: Vec<AbstimmungsStand>,
    kosten: Option<depot_journal::KiAbrechnung>,
}

fn vorschlag(wer: BeraterId, bp: i64) -> Result<Vorschlag, String> {
    Ok(Vorschlag {
        berater: wer,
        zielgewichte_bp: [(sap()?, bp)].into_iter().collect(),
        begruendung: [(sap()?, "Belegte Lage".into())].into_iter().collect(),
    })
}

impl depot_sim::Berater for Spieler {
    fn vorschlagen(
        &mut self,
        e: depot_journal::AufrufErlaubnis,
        lage: &depot_sim::Lage<'_>,
    ) -> Antwort {
        self.kosten = e.abrechnung(100, 100);
        depot_sim::Berater::vorschlagen(&mut self.plan, e, lage)
    }
    fn pruefen(
        &mut self,
        e: depot_journal::AufrufErlaubnis,
        lage: &depot_sim::Lage<'_>,
        v: &Vorschlag,
    ) -> Urteil {
        self.kosten = e.abrechnung(100, 100);
        depot_sim::Berater::pruefen(&mut self.plan, e, lage, v)
    }
    fn kann_abstimmen(&self) -> bool {
        true
    }
    fn abrechnung(&mut self) -> Option<depot_journal::KiAbrechnung> {
        self.kosten.take()
    }
    fn abstimmen(
        &mut self,
        e: depot_journal::AufrufErlaubnis,
        _: &depot_sim::Lage<'_>,
        stand: &AbstimmungsStand,
    ) -> AbstimmungsAntwort {
        self.gesehen.push(stand.clone());
        let Some((schritt, zustimmung)) = self.zuege.pop_front() else {
            return AbstimmungsAntwort::unverwertbar();
        };
        let eigener = match schritt {
            Schritt::Unverwertbar => return AbstimmungsAntwort::unverwertbar(),
            Schritt::Behalten => stand.eigener_vorschlag.clone(),
            Schritt::Aendern(bp) => vorschlag(self.wer, bp).ok(),
            Schritt::Zurueck => None,
        };
        self.kosten = e.abrechnung(100, 100);
        AbstimmungsAntwort {
            eigene: Antwort {
                prognose: None,
                verwertbar: true,
                einschaetzung: format!("Runde {}: {schritt:?}", stand.runde),
                vorschlag: eigener,
            },
            zustimmung_fremd: zustimmung,
        }
    }
}

fn spieler(wer: BeraterId, zuege: Vec<Zug>) -> Result<Spieler, String> {
    let mut plan = Drehbuch::default();
    plan.antworten.insert(
        tag()?,
        Antwort {
            prognose: None,
            verwertbar: true,
            einschaetzung: format!("Erstmeinung {wer:?}"),
            vorschlag: Some(vorschlag(wer, ERST_BP)?),
        },
    );
    plan.urteile.insert(
        tag()?,
        Urteil {
            verwertbar: true,
            zustimmung: true,
            begruendung: format!("Zweitmeinung {wer:?}"),
        },
    );
    Ok(Spieler {
        wer,
        plan,
        zuege: zuege.into(),
        gesehen: Vec::new(),
        kosten: None,
    })
}

/// Stück SAP, die ein Gewicht in einem Topf mit 5.000 € zu 230 € ergibt.
fn stueck(bp: i64) -> u64 {
    Cent(500_000)
        .anteil_bp_ab(bp)
        .and_then(|c| c.0.checked_div(23_000))
        .and_then(|n| u64::try_from(n).ok())
        .unwrap_or(0)
}

fn gesehen_fremd(s: &Spieler) -> Vec<Stand> {
    s.gesehen
        .iter()
        .map(|g| {
            g.fremder_vorschlag
                .as_ref()
                .and_then(|v| v.zielgewichte_bp.values().next().copied())
        })
        .collect()
}

fn pruefe(runden: &[(Zug, Zug)]) -> Result<Vec<String>, String> {
    let mut a = spieler(BeraterId::Dachs, runden.iter().map(|(a, _)| *a).collect())?;
    let mut b = spieler(BeraterId::Maus, runden.iter().map(|(_, b)| *b).collect())?;
    let ort = Ort::neu()?;
    let mut j = vorbedingung(Journal::oeffnen(&ort.journal), "Journal")?;
    belege(&mut j)?;
    let mut k = depot_sim::Papierdatei::oeffnen(&ort.konto, false, &start()?)?;
    papiertag(
        &mut j,
        tag()?,
        &universum()?,
        &start()?,
        &mut a,
        &mut b,
        &mut k,
    )
    .map_err(|e| format!("{runden:?}: Papiertag {e:?}"))?;
    let (frei, sieht_a, sieht_b) = modell(runden);
    let mut offen = Vec::new();
    for (topf, erwartet) in [(TopfId::A, frei[0]), (TopfId::B, frei[1])] {
        let ist: Vec<u64> = j
            .stand()
            .auftraege()
            .iter()
            .filter(|l| l.beleg.topf == topf)
            .map(|l| l.beleg.stueck)
            .collect();
        let soll: Vec<u64> = erwartet
            .map(stueck)
            .into_iter()
            .filter(|s| *s > 0)
            .collect();
        if ist != soll {
            offen.push(format!(
                "{runden:?}: Topf {topf:?} Aufträge {ist:?} statt {soll:?}"
            ));
        }
    }
    // Beide sehen in jeder Runde den vor der Runde fixierten Stand des anderen.
    if gesehen_fremd(&a) != sieht_a || gesehen_fremd(&b) != sieht_b {
        offen.push(format!(
            "{runden:?}: gesehen Dachs {:?} / Maus {:?}, erwartet {sieht_a:?} / {sieht_b:?}",
            gesehen_fremd(&a),
            gesehen_fremd(&b)
        ));
    }
    let je_tag = depot_ki::KiGrenzen::standard().aufrufe_je_zyklus();
    for wer in [BeraterId::Dachs, BeraterId::Maus] {
        let n = j
            .stand()
            .ki()
            .buchungen()
            .iter()
            .filter(|x| x.berater == wer)
            .count();
        if u32::try_from(n).unwrap_or(u32::MAX) > je_tag {
            offen.push(format!("{runden:?}: {n} Aufrufe für {wer:?}"));
        }
    }
    Ok(offen)
}

fn alle_zuege() -> Vec<Zug> {
    SCHRITTE
        .iter()
        .flat_map(|s| [(*s, true), (*s, false)])
        .collect()
}

/// Jede Kombination einer Runde und zufällige Folgen bis drei Runden: Es entstehen genau die
/// Aufträge, die das Modell erlaubt. Ein geänderter oder zurückgezogener Vorschlag wird ohne
/// neue Zustimmung zu genau diesem Stand nie ausgeführt (A1 aus #100), auch nicht, wenn die
/// Runde danach abbricht. Beide sehen denselben fixierten Stand.
#[test]
fn abstimmung_fuehrt_nur_aus_was_beide_zuletzt_tragen() -> Result<(), String> {
    let zuege = alle_zuege();
    let mut folgen: Vec<Vec<(Zug, Zug)>> = Vec::new();
    for a in &zuege {
        for b in &zuege {
            folgen.push(vec![(*a, *b)]);
        }
    }
    for start in STARTWERTE {
        let mut z = Zufall(start);
        for _ in 0..40 {
            let runden = z.bis(2).saturating_add(2);
            let mut f = Vec::new();
            for _ in 0..runden {
                // Unverwertbar seltener, damit tiefe Runden erreicht werden.
                let zug = |z: &mut Zufall| {
                    let i = z.bis(zuege.len());
                    let i = if i >= 8 && z.bis(3) != 0 { z.bis(8) } else { i };
                    zuege.get(i).copied().unwrap_or((Schritt::Behalten, true))
                };
                let a = zug(&mut z);
                let b = zug(&mut z);
                f.push((a, b));
            }
            folgen.push(f);
        }
    }
    let mut offen = Vec::new();
    for f in &folgen {
        offen.extend(pruefe(f)?);
        if offen.len() > 10 {
            break;
        }
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

// ---------------------------------------------------------------------------------------------
// I5b Wiederanlauf nach jedem Abbruch
// ---------------------------------------------------------------------------------------------

/// Ein ganzer Papiertag mit Abstimmung und Aufträgen; liefert die Bytes des Journals.
fn journal_eines_tages(runden: &[(Zug, Zug)]) -> Result<Vec<u8>, String> {
    let mut a = spieler(BeraterId::Dachs, runden.iter().map(|(a, _)| *a).collect())?;
    let mut b = spieler(BeraterId::Maus, runden.iter().map(|(_, b)| *b).collect())?;
    let ort = Ort::neu()?;
    {
        let mut j = vorbedingung(Journal::oeffnen(&ort.journal), "Journal")?;
        belege(&mut j)?;
        let mut k = depot_sim::Papierdatei::oeffnen(&ort.konto, false, &start()?)?;
        papiertag(
            &mut j,
            tag()?,
            &universum()?,
            &start()?,
            &mut a,
            &mut b,
            &mut k,
        )
        .map_err(|e| format!("Papiertag {e:?}"))?;
        if j.stand().auftraege().is_empty() {
            return Err("Vorbedingung: Der Tag soll Aufträge enthalten".into());
        }
    }
    vorbedingung(fs::read(&ort.journal), "Journal lesen")
}

/// Zeilenenden (Position nach jedem `\n`) eines Journals.
fn zeilenenden(inhalt: &[u8]) -> Vec<usize> {
    inhalt
        .iter()
        .enumerate()
        .filter(|(_, b)| **b == b'\n')
        .filter_map(|(i, _)| i.checked_add(1))
        .collect()
}

/// Öffnet ein Journal aus genau diesen Bytes mit diesem Vermerk.
fn oeffne_mit(
    ort: &Ort,
    inhalt: &[u8],
    vermerk: &str,
) -> Result<Result<Journal, JournalFehler>, String> {
    vorbedingung(fs::write(&ort.journal, inhalt), "Datei")?;
    vorbedingung(
        fs::write(ort.journal.with_extension("vermerk"), vermerk),
        "Vermerk",
    )?;
    Ok(Journal::oeffnen(&ort.journal))
}

/// Nach einem Abbruch an jeder Stelle eines echten Papiertags, nach jeder ganzen Zeile und
/// in der nächsten (an ihren Rändern, in der Mitte und bei der letzten Zeile an jedem Byte),
/// gilt: Der Stand ist genau der Stand der bestätigten Zeilen,
/// und eine vollständig geschriebene nächste Zeile zählt genau dann, wenn der Vermerk sie
/// angekündigt hat. Ein halber Eintrag wird nie gelesen. Der Wiederanlauf ist selbst
/// wiederholbar: Ein zweites Öffnen ergibt denselben Stand. Nie eine Panik.
#[test]
fn wiederanlauf_nach_jedem_abbruch_ergibt_den_bestaetigten_stand() -> Result<(), String> {
    let mut offen = Vec::new();
    let tage = [
        vec![((Schritt::Behalten, true), (Schritt::Behalten, true))],
        vec![
            ((Schritt::Aendern(1_000), true), (Schritt::Behalten, true)),
            ((Schritt::Behalten, true), (Schritt::Behalten, true)),
        ],
    ];
    for runden in &tage {
        let voll = journal_eines_tages(runden)?;
        let enden = zeilenenden(&voll);
        let ort = Ort::neu()?;
        // Die Kopfzeile ist immer bestätigt; danach jede Länge k.
        let mut vorher: Option<depot_journal::Stand> = None;
        for (nr, paar) in enden.windows(2).enumerate() {
            let [k, naechste] = paar else { continue };
            let bis_k = voll.get(..*k).ok_or("Bereich")?;
            let mit_naechster = voll.get(..*naechste).ok_or("Bereich")?;
            let geplant = u64::try_from(naechste.saturating_sub(*k)).map_err(|_| "Länge")?;
            // (a) sauber nach Zeile k.
            let sauber = some(vermerk_fuer(bis_k, 0), "Vermerk")?;
            let stand_k = match oeffne_mit(&ort, bis_k, &sauber)? {
                Ok(j) => Some(j.stand().clone()),
                Err(JournalFehler::Halt(_)) => None,
                Err(e) => {
                    offen.push(format!("{runden:?} Zeile {nr}: unerwartet {e:?}"));
                    None
                }
            };
            // (b) Anhang angekündigt, 0 bis alle Bytes der nächsten Zeile geschrieben.
            let angekuendigt = some(vermerk_fuer(bis_k, geplant), "Vermerk")?;
            // Jede Zeile an ihren Rändern und in der Mitte, die letzte Zeile an jedem Byte.
            let letzte = Some(naechste) == enden.last();
            let mitte = k.saturating_add(naechste.saturating_sub(*k) / 2);
            let stellen: Vec<usize> = if letzte {
                (*k..=*naechste).collect()
            } else {
                vec![
                    *k,
                    k.saturating_add(1),
                    mitte,
                    naechste.saturating_sub(1),
                    *naechste,
                ]
            };
            for j in stellen {
                let teil = voll.get(..j).ok_or("Bereich")?;
                let erwartet = if j == *naechste {
                    let v = some(vermerk_fuer(mit_naechster, 0), "Vermerk")?;
                    match oeffne_mit(&ort, mit_naechster, &v)? {
                        Ok(x) => Some(x.stand().clone()),
                        Err(_) => None,
                    }
                } else {
                    stand_k.clone()
                };
                match (oeffne_mit(&ort, teil, &angekuendigt)?, &erwartet) {
                    (Ok(x), Some(s)) => {
                        let gelesen = x.stand().clone();
                        drop(x);
                        if gelesen != *s {
                            offen.push(format!("{runden:?} Zeile {nr}, {j} Byte: anderer Stand"));
                        }
                        // Der Wiederanlauf hat Datei und Vermerk bereinigt: wieder öffnen.
                        match Journal::oeffnen(&ort.journal) {
                            Ok(y) if *y.stand() == gelesen => {}
                            anders => offen.push(format!(
                                "{runden:?} Zeile {nr}, {j} Byte: zweites Öffnen {:?}",
                                anders.map(|_| "anderer Stand")
                            )),
                        }
                    }
                    (Err(JournalFehler::Halt(_)), None) => {}
                    (Ok(_), None) => offen.push(format!(
                        "{runden:?} Zeile {nr}, {j} Byte: gelesen, obwohl der saubere Stand hält"
                    )),
                    (Err(e), Some(_)) => offen.push(format!(
                        "{runden:?} Zeile {nr}, {j} Byte: Abbruch nicht wiederhergestellt: {e:?}"
                    )),
                    (Err(e), None) => {
                        offen.push(format!("{runden:?} Zeile {nr}, {j} Byte: {e:?}"));
                    }
                }
            }
            // Mit jeder Zeile kommt höchstens etwas hinzu: Was einmal galt, bleibt.
            if let (Some(alt), Some(neu)) = (&vorher, &stand_k)
                && neu.auftraege().len() < alt.auftraege().len()
            {
                offen.push(format!("{runden:?} Zeile {nr}: Aufträge verschwunden"));
            }
            if stand_k.is_some() {
                vorher.clone_from(&stand_k);
            }
            if offen.len() > 10 {
                break;
            }
        }
        if vorher.is_none() {
            offen.push(format!("{runden:?}: kein einziger Präfix lesbar"));
        }
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

/// Derselbe Kursbeleg zweimal ändert den Stand nicht: Er wird abgelehnt, oder er steht nur
/// als zusätzlicher Beleg da, ohne Topf, Aufträge, Kassenbuch oder Abschluss zu berühren.
#[test]
fn doppelter_kursbeleg_aendert_den_stand_nicht() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = vorbedingung(Journal::oeffnen(&ort.journal), "Journal")?;
    belege(&mut j)?;
    let vorher = j.stand().clone();
    let erneut = belege(&mut j);
    let nachher = j.stand();
    let gleich = nachher.toepfe() == vorher.toepfe()
        && nachher.auftraege() == vorher.auftraege()
        && nachher.ki() == vorher.ki()
        && nachher.abschluss() == vorher.abschluss();
    match erneut {
        Err(_) if *nachher == vorher => Ok(()),
        Ok(()) if gleich => Ok(()),
        anders => Err(format!("zweiter Beleg: {anders:?}, Stand gleich: {gleich}")),
    }
}
