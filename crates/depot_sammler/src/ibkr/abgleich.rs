//! Gleicht das lokale Journal für ein IBKR-Papierkonto mit einem vollständigen Abruf ab.

use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use depot_journal::konto::{Kontoauszug, abweichungen};
use depot_journal::{Journal, Stand};
use depot_kern::messlatte::ibkr::LANGFRIST;
use depot_kern::{Isin, Topf, TopfId};
use depot_regelwerk::Richtung;

use super::{Auszug, hole, kontonummer};

fn ordner(wurzel: &Path, konto: &str) -> PathBuf {
    wurzel.join("daten").join("ibkr-papier").join(konto)
}

fn ausgang(ergebnis: Result<(), String>) -> ExitCode {
    match ergebnis {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("[IBKR-Papier] Halt: {e}");
            ExitCode::FAILURE
        }
    }
}

pub(crate) fn einrichten(wurzel: &Path) -> ExitCode {
    ausgang((|| {
        let konto = kontonummer(wurzel)?;
        let ordner = ordner(wurzel, &konto);
        std::fs::create_dir_all(&ordner).map_err(|_| "Journalordner nicht verfügbar.")?;
        let pfad = ordner.join("journal.txt");
        if !Journal::eingerichtet(&pfad).map_err(|_| "Journal nicht prüfbar.")? {
            Journal::einrichten(&pfad).map_err(|_| "Journal nicht eingerichtet.")?;
        }
        let mut journal = Journal::oeffnen(&pfad).map_err(|_| "Journal nicht verfügbar.")?;
        if !journal.stand().toepfe().is_empty() {
            bereich(journal.stand())?;
            println!("[IBKR-Papier] Journal bereits eingerichtet.");
            return Ok(());
        }
        let topf = Topf::neu(TopfId::A, LANGFRIST.startkapital)
            .map_err(|_| "IBKR-Startkapital unzulässig.")?;
        journal
            .setze_topf(&topf)
            .map_err(|_| "Startkapital konnte nicht belegt werden.")?;
        println!("[IBKR-Papier] Separates Journal eingerichtet. Soll-Bargeld: {} Cent.", LANGFRIST.startkapital.0);
        Ok(())
    })())
}

fn bereich(stand: &Stand) -> Result<&Topf, &'static str> {
    stand
        .toepfe()
        .get(&TopfId::A)
        .filter(|_| stand.toepfe().len() == 1)
        .ok_or("Das IBKR-Papierjournal braucht genau einen Langfristbereich.")
}

fn kontrakte(text: &str) -> Result<BTreeMap<i32, Isin>, &'static str> {
    let mut werte = BTreeSet::new();
    let mut karte = BTreeMap::new();
    for zeile in text.lines().filter(|z| !z.trim().is_empty()) {
        let mut felder = zeile.split_whitespace();
        let id = felder
            .next()
            .and_then(|k| k.parse::<i32>().ok())
            .filter(|id| *id > 0)
            .ok_or("Ungültige IBKR-Kontraktkennung.")?;
        let isin = felder
            .next()
            .and_then(|i| Isin::neu(i).ok())
            .ok_or("Ungültige ISIN in der Kontraktzuordnung.")?;
        if felder.next().is_some()
            || !(LANGFRIST.aktien_isins.contains(&isin.als_str())
                || isin.als_str() == LANGFRIST.etf_isin)
            || !werte.insert(isin.clone())
            || karte.insert(id, isin).is_some()
        {
            return Err("Kontraktzuordnung doppelt, außerhalb des Langfristprofils oder ungültig.");
        }
    }
    Ok(karte)
}

fn vergleiche(
    stand: &Stand,
    a: &Auszug,
    karte: &BTreeMap<i32, Isin>,
) -> Result<(), String> {
    let topf = bereich(stand)?;
    let isin_von = |id| karte.get(&id).ok_or("Für einen Broker-Kontrakt fehlt die ISIN.");
    let bestand = a
        .positionen
        .iter()
        .map(|(id, n)| Ok((isin_von(id)?.clone(), *n)))
        .collect::<Result<BTreeMap<_, _>, &str>>()?;
    let mut offene = BTreeSet::new();
    for o in &a.orders {
        let nummer = o
            .referenz
            .parse::<u64>()
            .ok()
            .filter(|n| *n != 0 && n.to_string() == o.referenz)
            .ok_or("Offene Order hat keine eindeutige DepotSchlopf-Referenz.")?;
        let soll = stand
            .auftrag(nummer)
            .filter(|l| l.zustand.beim_broker())
            .ok_or("Gateway meldet eine offene Order ohne Journalbeleg.")?;
        let richtung = match soll.beleg.richtung {
            Richtung::Kauf => "Kauf",
            Richtung::Verkauf => "Verkauf",
        };
        if &soll.beleg.isin != isin_von(&o.kontrakt)?
            || soll.beleg.stueck != o.stueck
            || richtung != o.richtung
            || !offene.insert(nummer)
        {
            return Err("Eine offene Order weicht vom Journalbeleg ab.".into());
        }
    }

    let konto = Kontoauszug {
        bargeld: a.salden.gesamt,
        bestand,
        offene,
        ..Kontoauszug::default()
    };
    let fehler = abweichungen(stand, &konto);
    if !fehler.is_empty() {
        return Err(format!("Abweichung vom IBKR-Papierjournal: {fehler:?}"));
    }

    let abgerechnet = stand
        .nicht_gutgeschrieben(TopfId::A)
        .and_then(|offen| topf.bargeld().checked_sub(offen))
        .ok_or("Abgerechnetes Bargeld aus Journal nicht berechenbar.")?;
    let frei = stand
        .offene_auftraege()
        .filter(|l| l.zustand.beim_broker() && l.beleg.richtung == Richtung::Kauf)
        .try_fold(abgerechnet, |saldo, l| {
            let bindung = l
                .beleg
                .limit
                .mal_stueck(l.offen_stueck())?
                .checked_add(LANGFRIST.ordergebuehr_max)?;
            saldo.checked_sub(bindung)
        })
        .ok_or("Verfügbare Mittel aus Journal nicht berechenbar.")?;
    if a.salden.abgerechnet != abgerechnet || a.salden.verfuegbar != frei {
        return Err(format!(
            "Abgerechnetes Bargeld, verfügbare Mittel (Cent): Soll {}/{}, IBKR {}/{}.",
            abgerechnet.0, frei.0, a.salden.abgerechnet.0, a.salden.verfuegbar.0
        ));
    }
    Ok(())
}

pub(crate) fn abgleichen(wurzel: &Path) -> ExitCode {
    ausgang((|| {
        let konto = kontonummer(wurzel)?;
        let ordner = ordner(wurzel, &konto);
        let pfad = ordner.join("journal.txt");
        let mut journal = Journal::oeffnen(&pfad).map_err(|_| "Journal nicht verfügbar.")?;
        let tag = crate::jetzt()
            .and_then(depot_kern::Zeitpunkt::frankfurt_tag)
            .ok_or("Datum nicht verfügbar.")?;
        let vergleich = (|| {
            let text = match std::fs::read_to_string(ordner.join("kontrakte.txt")) {
                Ok(t) => t,
                Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
                Err(_) => return Err("Kontraktzuordnung unlesbar.".into()),
            };
            let karte = kontrakte(&text)?;
            vergleiche(journal.stand(), &hole(&konto)?, &karte)
        })();
        let beleg = if vergleich.is_ok() {
            "IBKR-Papierabgleich: ohne Abweichung"
        } else {
            "IBKR-Papierabgleich: Halt"
        };
        if journal.beobachte(tag, beleg).is_err() {
            return Err(vergleich.err().map_or_else(
                || "Der Abgleich konnte nicht im Journal belegt werden.".into(),
                |e| format!("{e} Der Halt konnte nicht im Journal belegt werden."),
            ));
        }
        vergleich?;
        println!("[IBKR-Papier] Abgleich ohne Abweichung, im Journal belegt.");
        Ok(())
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ibkr::{OffeneOrder, Salden};
    use depot_kern::Cent;

    #[test]
    fn erkennt_bargeld_mittel_position_und_fremde_order() -> Result<(), String> {
        let ordner = std::env::temp_dir().join(format!("depot-ibkr-test-{}", std::process::id()));
        std::fs::create_dir(&ordner).map_err(|e| e.to_string())?;
        let ergebnis = (|| {
            let pfad = ordner.join("journal.txt");
            let fehler = |e| format!("{e:?}");
            Journal::einrichten(&pfad).map_err(fehler)?;
            let mut journal = Journal::oeffnen(&pfad).map_err(fehler)?;
            let start = LANGFRIST.startkapital;
            journal
                .setze_topf(&Topf::neu(TopfId::A, start).map_err(|e| format!("{e:?}"))?)
                .map_err(fehler)?;
            let mut a = Auszug {
                salden: Salden {
                    gesamt: start,
                    abgerechnet: start,
                    verfuegbar: start,
                },
                positionen: BTreeMap::new(),
                orders: Vec::new(),
            };
            let karte = kontrakte("123 DE0007164600")?;
            assert!(vergleiche(journal.stand(), &a, &karte).is_ok());
            a.salden.gesamt = Cent::NULL;
            assert!(vergleiche(journal.stand(), &a, &karte).is_err());
            a.salden.gesamt = start;
            a.salden.verfuegbar = Cent::NULL;
            assert!(vergleiche(journal.stand(), &a, &karte).is_err());
            a.salden.verfuegbar = start;
            a.positionen.insert(123, 1);
            assert!(vergleiche(journal.stand(), &a, &karte).is_err());
            a.positionen.clear();
            a.orders.push(OffeneOrder {
                dauerhafte_id: 4,
                referenz: "7".into(),
                kontrakt: 123,
                richtung: "Kauf",
                stueck: 1,
            });
            assert!(vergleiche(journal.stand(), &a, &karte).is_err());
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(ordner);
        ergebnis
    }

    #[test]
    fn kontraktzuordnung_muss_eindeutig_und_freigegeben_sein() {
        assert!(kontrakte("123 DE0007164600").is_ok());
        assert!(kontrakte("123 DE0007164600\n124 DE0007164600").is_err());
        assert!(kontrakte("123 US0378331005").is_err());
    }
}
