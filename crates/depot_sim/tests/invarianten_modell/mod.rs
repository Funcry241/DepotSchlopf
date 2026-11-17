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
//! Ein Widerruf der Zustimmung wirkt auch dann, wenn die Runde abbricht.
//!
//! Erfundene Daten (D12). Keine Abhängigkeit, kein `f64`.
//!
//! Gemeinsames Modell und Hilfen; die Tests stehen in `invarianten.rs`,
//! `invarianten_zufall.rs` und `invarianten_wiederanlauf.rs`. So startet das Gate sie als
//! eigene Testprogramme parallel (C1, #190).

#![allow(dead_code, reason = "jede Testdatei nutzt nur einen Teil des Modells")]
#![allow(
    clippy::many_single_char_names,
    reason = "Testcode: kurze Namen für Zufall, Tage und Spieler"
)]

use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{Journal, JournalFehler, KursBeleg};
use depot_kern::{
    BeraterId, Cent, Datum, Herkunft, Instrument, InstrumentArt, Isin, Kurs, Preis, Topf, TopfId,
    Universum, Vorschlag, Zeitpunkt,
};
use depot_sim::abstimmung::{AbstimmungsAntwort, AbstimmungsStand};
use depot_sim::{Antwort, Drehbuch, Urteil, papiertag};

pub static ZAEHLER: AtomicU32 = AtomicU32::new(0);

pub const STARTWERTE: [u64; 3] = [0x5EED_0001, 0xD0C5_2026, 0x0DAC_115E];

/// Der Startwert mit dieser Nummer.
pub fn startwert(nr: usize) -> Result<u64, String> {
    STARTWERTE
        .get(nr)
        .copied()
        .ok_or_else(|| format!("Startwert {nr}"))
}

/// xorshift64 mit festem Startwert.
#[derive(Debug)]
pub struct Zufall(pub u64);

impl Zufall {
    pub fn naechste(&mut self) -> u64 {
        let mut x = self.0 | 1;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub fn bis(&mut self, n: usize) -> usize {
        let n = u64::try_from(n).unwrap_or(u64::MAX).max(1);
        usize::try_from(self.naechste().checked_rem(n).unwrap_or(0)).unwrap_or(0)
    }
}

pub fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

pub fn some<T>(o: Option<T>, was: &str) -> Result<T, String> {
    o.ok_or_else(|| format!("Vorbedingung {was}"))
}

pub struct Ort {
    pub dir: PathBuf,
    pub journal: PathBuf,
    pub konto: PathBuf,
}

impl Ort {
    pub fn neu() -> Result<Ort, String> {
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

pub fn tag() -> Result<Datum, String> {
    some(Datum::neu(2026, 11, 5), "Datum")
}

pub fn sap() -> Result<Isin, String> {
    vorbedingung(Isin::neu("DE0007164600"), "ISIN SAP")
}

pub fn allianz() -> Result<Isin, String> {
    vorbedingung(Isin::neu("DE0008404005"), "ISIN Allianz")
}

pub fn universum() -> Result<Universum, String> {
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

pub fn start() -> Result<Vec<Topf>, String> {
    Ok(vec![
        vorbedingung(Topf::neu(TopfId::A, Cent(500_000)), "Topf A")?,
        vorbedingung(Topf::neu(TopfId::B, Cent(500_000)), "Topf B")?,
    ])
}

pub fn belege(j: &mut Journal) -> Result<(), String> {
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
pub const ERST_BP: i64 = 2_000;

/// Was ein Berater in einer Runde tut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Schritt {
    /// Die gezeigten Zielgewichte mit neu formulierter Begründung bestätigen.
    Behalten,
    /// Auf dieses Gewicht ändern (oder nach einem Rückzug neu vorschlagen).
    Aendern(i64),
    /// Den Vorschlag zurückziehen.
    Zurueck,
    /// Keine verwertbare Antwort: Die Abstimmung endet.
    Unverwertbar,
}

pub const SCHRITTE: [Schritt; 5] = [
    Schritt::Behalten,
    Schritt::Aendern(1_000),
    Schritt::Aendern(3_000),
    Schritt::Zurueck,
    Schritt::Unverwertbar,
];

/// Ein Schritt mit der Haltung zum Vorschlag des Partners.
pub type Zug = (Schritt, bool);

/// Der Stand eines Vorschlags im Modell: das Gewicht, `None` für keinen Vorschlag.
pub type Stand = Option<i64>;

/// Das Modell der Regel. Liefert, welcher Vorschlag je Berater gültig bleibt, und was jeder
/// in jeder Runde als fremden Vorschlag sehen muss.
pub fn modell(runden: &[(Zug, Zug)]) -> ([Stand; 2], Vec<Stand>, Vec<Stand>) {
    let mut gezeigt: [Stand; 2] = [Some(ERST_BP), Some(ERST_BP)];
    // Maus' Zweitmeinung legt ein Veto gegen Topf A ein, damit ein Punkt offen ist und die
    // Abstimmung überhaupt läuft (#312-1, P35). Topf B ist nach Dachs' Zustimmung freigegeben.
    let mut frei: [Stand; 2] = [None, gezeigt[1]];
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
    for &((sa, za), (sb, zb)) in runden.iter().chain(std::iter::once(&ende)).take(2) {
        let [va, vb] = gezeigt;
        sieht_a.push(vb);
        let Some(na) = antwort(sa, va) else { break };
        // Eine verwertbare neue Antwort nimmt der alten Freigabe sofort die Gültigkeit.
        if na != frei[0] {
            frei[0] = None;
        }
        if !za && frei[1] == vb {
            frei[1] = None;
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

pub struct Spieler {
    pub wer: BeraterId,
    pub plan: Drehbuch,
    pub zuege: VecDeque<Zug>,
    pub gesehen: Vec<AbstimmungsStand>,
    pub kosten: Option<depot_journal::KiAbrechnung>,
}

pub fn vorschlag(wer: BeraterId, bp: i64) -> Result<Vorschlag, String> {
    Ok(Vorschlag {
        erwartung_bp: std::collections::BTreeMap::new(),
        limit_bp: std::collections::BTreeMap::new(),
        stopp_bp: std::collections::BTreeMap::new(),
        berater: wer,
        zielgewichte_bp: [(sap()?, bp)].into_iter().collect(),
        begruendung: [(sap()?, "Belegte Lage".into())].into_iter().collect(),
        belege: std::collections::BTreeMap::new(),
        park: None,
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
            Schritt::Behalten => stand.eigener_vorschlag.clone().map(|mut v| {
                for grund in v.begruendung.values_mut() {
                    *grund = format!("Belegte Lage in eigenen Worten (Runde {})", stand.runde);
                }
                v
            }),
            Schritt::Aendern(bp) => vorschlag(self.wer, bp).ok().map(|mut v| {
                for grund in v.begruendung.values_mut() {
                    *grund = format!("Begründung der Änderung (Runde {})", stand.runde);
                }
                v
            }),
            Schritt::Zurueck => None,
        };
        self.kosten = e.abrechnung(100, 100);
        AbstimmungsAntwort {
            eigene: Antwort {
                halten: std::collections::BTreeMap::new(),
                prognose: None,
                verwertbar: true,
                einschaetzung: format!("Runde {}: {schritt:?}", stand.runde),
                vorschlag: eigener,
            },
            zustimmung_fremd: zustimmung,
        }
    }
}

pub fn spieler(wer: BeraterId, zuege: Vec<Zug>) -> Result<Spieler, String> {
    let mut plan = Drehbuch::default();
    plan.antworten.insert(
        tag()?,
        Antwort {
            halten: std::collections::BTreeMap::new(),
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
            zustimmung: wer != BeraterId::Maus,
            begruendung: format!("Zweitmeinung {wer:?}"),
            abgelehnt: None,
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
pub fn stueck(bp: i64) -> u64 {
    Cent(500_000)
        .anteil_bp_ab(bp)
        .and_then(|c| c.0.checked_div(23_000))
        .and_then(|n| u64::try_from(n).ok())
        .unwrap_or(0)
}

pub fn gesehen_fremd(s: &Spieler) -> Vec<Stand> {
    s.gesehen
        .iter()
        .map(|g| {
            g.fremder_vorschlag
                .as_ref()
                .and_then(|v| v.zielgewichte_bp.values().next().copied())
        })
        .collect()
}

pub fn pruefe(runden: &[(Zug, Zug)]) -> Result<Vec<String>, String> {
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

pub fn alle_zuege() -> Vec<Zug> {
    SCHRITTE
        .iter()
        .flat_map(|s| [(*s, true), (*s, false)])
        .collect()
}

/// Jede Kombination einer Runde.
pub fn einzelrunden() -> Vec<Vec<(Zug, Zug)>> {
    let zuege = alle_zuege();
    let mut folgen = Vec::new();
    for a in &zuege {
        for b in &zuege {
            folgen.push(vec![(*a, *b)]);
        }
    }
    folgen
}

/// 40 zufällige Folgen von zwei bis drei Runden zu einem festen Startwert.
pub fn zufallsfolgen(start: u64) -> Vec<Vec<(Zug, Zug)>> {
    let zuege = alle_zuege();
    let mut folgen = Vec::new();
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
    folgen
}

/// Es entstehen genau die Aufträge, die das Modell erlaubt. Ein geänderter oder
/// zurückgezogener Vorschlag wird ohne neue Zustimmung zu genau diesem Stand nie ausgeführt
/// (A1 aus #100), auch nicht, wenn die Runde danach abbricht. Beide sehen denselben fixierten
/// Stand.
pub fn nur_was_beide_zuletzt_tragen(folgen: &[Vec<(Zug, Zug)>]) -> Result<(), String> {
    let offen = pruefe_abstimmungsfolgen(folgen, 4, pruefe)?;
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

pub fn pruefe_abstimmungsfolgen(
    folgen: &[Vec<(Zug, Zug)>],
    arbeiterzahl: usize,
    pruefung: impl Fn(&[(Zug, Zug)]) -> Result<Vec<String>, String> + Sync,
) -> Result<Vec<String>, String> {
    // Jede Folge hat ein eigenes Journal und Papierkonto. Die Ergebnisse bleiben geordnet.
    std::thread::scope(|scope| -> Result<Vec<String>, String> {
        let mut arbeiter = Vec::new();
        let pruefung = &pruefung;
        for teil in folgen.chunks(folgen.len().div_ceil(arbeiterzahl).max(1)) {
            arbeiter.push(scope.spawn(move || -> Result<Vec<String>, String> {
                let mut offen = Vec::new();
                for f in teil {
                    offen.extend(pruefung(f)?);
                    if offen.len() > 10 {
                        break;
                    }
                }
                Ok(offen)
            }));
        }
        let mut offen = Vec::new();
        for arbeit in arbeiter {
            offen.extend(
                arbeit
                    .join()
                    .map_err(|_| "Abstimmungsprüfung abgebrochen")??,
            );
        }
        offen.truncate(11);
        Ok(offen)
    })
}

// ---------------------------------------------------------------------------------------------
// I5b Wiederanlauf nach jedem Abbruch
// ---------------------------------------------------------------------------------------------

/// Ein ganzer Papiertag mit Abstimmung und Aufträgen; liefert die Bytes des Journals.
pub fn journal_eines_tages(runden: &[(Zug, Zug)]) -> Result<Vec<u8>, String> {
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
pub fn zeilenenden(inhalt: &[u8]) -> Vec<usize> {
    inhalt
        .iter()
        .enumerate()
        .filter(|(_, b)| **b == b'\n')
        .filter_map(|(i, _)| i.checked_add(1))
        .collect()
}

/// Öffnet ein Journal aus genau diesen Bytes mit diesem Vermerk.
pub fn oeffne_mit(
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
