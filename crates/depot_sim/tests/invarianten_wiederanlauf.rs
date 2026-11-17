//! Wiederanlauf nach jedem Abbruch (docs/MATHEMATIK.md, I5b) und die Abstimmungsprüfung bei einem und vier Arbeitern.
//!
//! Modell, Spieler und Hilfen stehen in `invarianten_modell/mod.rs`. Die Tests sind auf
//! mehrere Testprogramme verteilt, damit das Gate sie parallel startet (C1, #190).

#![allow(
    clippy::many_single_char_names,
    reason = "Testcode: kurze Namen für Zufall, Tage und Spieler"
)]

mod invarianten_modell;

use depot_journal::{Journal, JournalFehler, vermerk_fuer};
use invarianten_modell::{
    Ort, STARTWERTE, Schritt, Zufall, alle_zuege, journal_eines_tages, oeffne_mit, pruefe,
    pruefe_abstimmungsfolgen, some, zeilenenden,
};

#[test]
fn abstimmungsfolgen_bei_einem_und_vier_arbeitern() -> Result<(), String> {
    let zuege = alle_zuege();
    let mut zufall = Zufall(STARTWERTE.first().copied().ok_or("Startwert")?);
    let folgen: Vec<_> = (0..16)
        .map(|_| {
            let zug = zuege.get(zufall.bis(zuege.len())).copied().ok_or("Zug")?;
            Ok(vec![(zug, (Schritt::Behalten, true))])
        })
        .collect::<Result<_, String>>()?;
    for arbeiterzahl in [1, 4] {
        let offen = pruefe_abstimmungsfolgen(&folgen, arbeiterzahl, pruefe)?;
        assert!(offen.is_empty(), "{offen:?}");
        let fehlerfall = folgen.first().ok_or("Erste Folge")?;
        let offen = pruefe_abstimmungsfolgen(&folgen, arbeiterzahl, |folge| {
            let mut offen = pruefe(folge)?;
            if folge == fehlerfall {
                offen.push("Absichtlich falscher Modellbefund".into());
            }
            Ok(offen)
        })?;
        assert!(!offen.is_empty());
        assert!(
            offen
                .iter()
                .all(|meldung| meldung == "Absichtlich falscher Modellbefund")
        );
    }
    Ok(())
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
