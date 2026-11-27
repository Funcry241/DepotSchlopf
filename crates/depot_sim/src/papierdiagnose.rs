//! Wiederholbarer Funktionstest mit festen Antworten und isolierten Papierdaten.

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use depot_journal::{AuftragsZustand, Journal, Konto, KursBeleg};
use depot_kern::{
    BeraterId, Boerse, Cent, Datum, Herkunft, Instrument, InstrumentArt, Isin, Kurs, Topf, TopfId,
    Universum, Vorschlag, Zeitpunkt,
};
use depot_regelwerk::{Anlass, Richtung};

use crate::{Antwort, Drehbuch, Papierdatei, Urteil, papiertag};

struct Probeordner(PathBuf);

impl Probeordner {
    fn neu() -> Result<Self, String> {
        static NUMMER: AtomicU64 = AtomicU64::new(0);
        let nr = NUMMER.fetch_add(1, Ordering::Relaxed);
        let zeit = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("Systemzeit: {e}"))?
            .as_nanos();
        let ordner = std::env::temp_dir().join(format!(
            "depotschlopf-papierdiagnose-{}-{zeit}-{nr}",
            std::process::id()
        ));
        fs::create_dir(&ordner).map_err(|e| format!("Testordner anlegen: {e}"))?;
        Ok(Self(ordner))
    }
}

impl Drop for Probeordner {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn datum(tag: u8) -> Result<Datum, String> {
    Datum::neu(2026, 11, tag).ok_or_else(|| format!("Testdatum fehlt: 2026-11-{tag:02}"))
}

fn belege(
    journal: &mut Journal,
    isin: &Isin,
    tag: u8,
    preis: i64,
    abruf: Zeitpunkt,
) -> Result<(), String> {
    let datum = datum(tag)?;
    let schluss = Boerse::Xetra.schluss(datum).ok_or("Testschluss fehlt")?;
    let kurs = Kurs::neu(Cent(preis), "EUR", datum, schluss, Herkunft::Quelle)
        .map_err(|e| format!("Testkurs: {e:?}"))?;
    // Die Herkunft entspricht einem Sammlerbeleg. Preis und Abrufzeit sind Testdaten.
    let beleg = KursBeleg::neu("alphavantage", abruf, b"Papierdiagnose", isin.clone(), kurs)
        .map_err(|e| format!("Testkursbeleg: {e:?}"))?;
    journal
        .belege_kurs(&beleg)
        .map_err(|e| format!("Testkurs verbuchen: {e:?}"))?;
    Ok(())
}

fn pruefe<T: Copy + Debug + PartialEq>(schritt: &str, ist: T, erwartet: T) -> Result<(), String> {
    if ist == erwartet {
        Ok(())
    } else {
        Err(format!(
            "{schritt}: erwartet {erwartet:?}, erhalten {ist:?}."
        ))
    }
}

fn antwort(vorschlag: Option<Vorschlag>) -> Antwort {
    Antwort {
        halten: BTreeMap::new(),
        prognose: None,
        verwertbar: true,
        einschaetzung: "Bestand geprüft".into(),
        vorschlag,
    }
}

fn berater(
    isin: &Isin,
    kauftag: Datum,
    verkaufstag: Datum,
) -> Result<(Drehbuch, Drehbuch), String> {
    let mut dachs = Drehbuch::default();
    let mut maus = Drehbuch::default();
    dachs.antworten.insert(
        kauftag,
        antwort(Some(Vorschlag {
            berater: BeraterId::Dachs,
            zielgewichte_bp: BTreeMap::from([(isin.clone(), 2_000)]),
            erwartung_bp: BTreeMap::new(),
            limit_bp: BTreeMap::new(),
            stopp_bp: BTreeMap::new(),
            begruendung: BTreeMap::from([(
                isin.clone(),
                "[Zahlen] [Ausstieg +800bp 2] Testthese".into(),
            )]),
            belege: BTreeMap::new(),
            park: None,
        })),
    );
    dachs.antworten.insert(datum(17)?, antwort(None));
    dachs.antworten.insert(verkaufstag, antwort(None));
    maus.urteile.insert(
        kauftag,
        Urteil {
            verwertbar: true,
            zustimmung: true,
            begruendung: "passt".into(),
            abgelehnt: None,
        },
    );
    Ok((dachs, maus))
}

/// Prüft den Kauf, die Haltefrist, den Verkauf und den Abgleich mit neu geöffneten Dateien.
///
/// # Errors
/// Nennt den fehlgeschlagenen Schritt oder einen Fehler der temporären Dateien.
pub fn papierhandel_pruefen(mut melde: impl FnMut(&str)) -> Result<(), String> {
    let ordner = Probeordner::neu()?;
    let journalpfad = ordner.0.join("journal.txt");
    let kontopfad = ordner.0.join("papierkonto.txt");
    Journal::einrichten(&journalpfad).map_err(|e| format!("Testjournal anlegen: {e:?}"))?;
    let mut journal =
        Journal::oeffnen(&journalpfad).map_err(|e| format!("Testjournal öffnen: {e:?}"))?;
    let start = [
        Topf::neu(TopfId::A, Cent(500_000)),
        Topf::neu(TopfId::B, Cent(500_000)),
    ]
    .into_iter()
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| format!("Testtöpfe anlegen: {e:?}"))?;
    let mut konto = Papierdatei::oeffnen(&kontopfad, false, &start)?;
    let sap = Isin::neu("DE0007164600").map_err(|e| format!("Testwert SAP: {e:?}"))?;
    let isin = Isin::neu("DE0008404005").map_err(|e| format!("Testwert Allianz: {e:?}"))?;
    let universum = Universum::neu(
        [("SAP", &sap), ("Allianz", &isin)]
            .into_iter()
            .map(|(name, isin)| Instrument {
                isin: isin.clone(),
                name: name.into(),
                art: InstrumentArt::Aktie,
            })
            .collect(),
    );
    let kauftag = datum(16)?;
    let verkaufstag = datum(18)?;
    let (mut dachs, mut maus) = berater(&isin, kauftag, verkaufstag)?;
    for (lauf, alt, neu, preis, schritt) in [
        (16, 12, 13, 5_000, "Kaufauftrag"),
        (17, 13, 16, 5_000, "Kauf und Haltefrist"),
        (18, 16, 17, 5_050, "Fristverkauf"),
        (19, 17, 18, 5_150, "Verkaufsbuchung"),
    ] {
        let heute = datum(lauf)?;
        let abruf = Zeitpunkt::frankfurt(heute, 7, 30, 0).ok_or("Testabrufzeit fehlt")?;
        belege(&mut journal, &sap, alt, 25_000, abruf)?;
        belege(&mut journal, &sap, neu, 25_000, abruf)?;
        belege(&mut journal, &isin, neu, preis, abruf)?;
        if lauf == 16 {
            belege(&mut journal, &isin, alt, 5_000, abruf)?;
        }
        papiertag(
            &mut journal,
            heute,
            &universum,
            &start,
            &mut dachs,
            &mut maus,
            &mut konto,
        )
        .map_err(|e| format!("{schritt}: {e:?}"))?;
        pruefe_tag(&journal, &isin, lauf, heute).map_err(|e| format!("{schritt}: {e}"))?;
        match lauf {
            16 => melde("Kaufauftrag geprüft: 20 Stück zum vorgesehenen Börsenschluss."),
            17 => {
                melde(
                    "Kauf und Haltefrist geprüft: 20 Stück im Bestand, kein vorzeitiger Verkauf.",
                );
            }
            18 => melde(
                "Fristverkauf geprüft: 20 Stück nach zwei Handelstagen zum Verkauf vorgemerkt.",
            ),
            _ => {}
        }
    }
    drop(konto);
    drop(journal);
    let mut journal = Journal::oeffnen(&journalpfad)
        .map_err(|e| format!("Kontoabgleich, Testjournal erneut öffnen: {e:?}"))?;
    let konto = Papierdatei::oeffnen(&kontopfad, true, &start)
        .map_err(|e| format!("Kontoabgleich, Papierkonto erneut öffnen: {e}"))?;
    pruefe_abschluss(&mut journal, &konto, &isin, kauftag, verkaufstag)?;
    melde("Ausführungen geprüft: Kauf zu 50,03 €, Verkauf zu 51,47 €, je 1,50 € Gebühren.");
    melde("Kontoabgleich bestanden: 0 Stück im Bestand, Testgewinn nach Gebühren 25,80 €.");
    Ok(())
}

fn pruefe_tag(journal: &Journal, isin: &Isin, lauf: u8, heute: Datum) -> Result<(), String> {
    let offen: Vec<_> = journal.stand().offene_auftraege().collect();
    match lauf {
        16 | 18 => {
            let kauf = lauf == 16;
            let richtung = if kauf {
                Richtung::Kauf
            } else {
                Richtung::Verkauf
            };
            pruefe("Offene Aufträge", offen.len(), 1)?;
            let order = offen.first().ok_or("Erwarteter Auftrag fehlt")?;
            pruefe("Auftragsrichtung", order.beleg.richtung, richtung)?;
            pruefe("Auftragsstückzahl", order.beleg.stueck, 20)?;
            pruefe(
                "Vorgesehener Börsenschluss",
                order.frist,
                Boerse::Xetra.schluss(heute),
            )?;
            pruefe(
                "Ausführungen vor Börsenschluss",
                order.ausfuehrungen.len(),
                0,
            )?;
            if !kauf {
                pruefe(
                    "Verkaufsanlass",
                    order.beleg.anlass,
                    Anlass::FristOhneBegruendung,
                )?;
            }
        }
        17 => {
            let topf = journal
                .stand()
                .toepfe()
                .get(&TopfId::A)
                .ok_or("Kaufbuchung: Topf A fehlt")?;
            pruefe("Bestand während der Haltefrist", topf.stueck(isin), 20)?;
            pruefe("Bargeld nach dem Kauf in Cent", topf.bargeld().0, 399_790)?;
            pruefe("Offene Aufträge während der Haltefrist", offen.len(), 0)?;
        }
        _ => pruefe("Offene Aufträge nach dem Verkauf", offen.len(), 0)?,
    }
    Ok(())
}

fn pruefe_abschluss(
    journal: &mut Journal,
    konto: &Papierdatei,
    isin: &Isin,
    kauftag: Datum,
    verkaufstag: Datum,
) -> Result<(), String> {
    let topf = journal
        .stand()
        .toepfe()
        .get(&TopfId::A)
        .ok_or("Kontoabgleich: Topf A fehlt")?;
    pruefe("Restbestand nach dem Verkauf", topf.stueck(isin), 0)?;
    pruefe("Bargeld Topf A in Cent", topf.bargeld().0, 502_580)?;
    pruefe(
        "Bargeld im Papierkonto in Cent",
        konto.auszug().bargeld.0,
        1_002_580,
    )?;
    let abweichungen = journal
        .gleiche_ab(konto)
        .map_err(|e| format!("Kontoabgleich: {e:?}"))?;
    pruefe("Abweichungen beim Kontoabgleich", abweichungen.len(), 0)?;
    let auftraege = journal.stand().auftraege();
    pruefe("Aufträge nach dem Verkauf", auftraege.len(), 2)?;
    for (order, (datum, kurs)) in auftraege
        .iter()
        .zip([(kauftag, 5_003), (verkaufstag, 5_147)])
    {
        let schritt = format!("{:?} am {datum}", order.beleg.richtung);
        pruefe(
            &format!("{schritt}, Zustand"),
            order.zustand,
            AuftragsZustand::Gefuellt,
        )?;
        pruefe(
            &format!("{schritt}, Ausführungen"),
            order.ausfuehrungen.len(),
            1,
        )?;
        let ausfuehrung = order
            .ausfuehrungen
            .first()
            .ok_or_else(|| format!("{schritt}: Ausführung fehlt"))?;
        pruefe(&format!("{schritt}, Stück"), ausfuehrung.stueck, 20)?;
        pruefe(
            &format!("{schritt}, Kurs in Cent"),
            ausfuehrung.kurs.0,
            kurs,
        )?;
        pruefe(
            &format!("{schritt}, Gebühr in Cent"),
            ausfuehrung.gebuehr.0,
            150,
        )?;
        pruefe(
            &format!("{schritt}, Kurszeitpunkt"),
            ausfuehrung.kurszeit,
            Boerse::Xetra.schluss(datum),
        )?;
    }
    Ok(())
}
