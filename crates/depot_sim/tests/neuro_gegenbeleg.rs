//! #325 Frage 28/31: Der Grundrang gibt Gegenbelegen dieselbe Chance wie Bestätigungen.
//! Gepaarte Belege: gleicher Wert, gleiche Länge und Stelle, nur die Richtung ist vertauscht.
//! Ein Sortierer, der bestätigende Wörter bevorzugt, fällt hier auf.

use depot_kern::Isin;
use depot_sim::belegrang::{Grundrang, Sortierer, These};

/// Test-Sortierer mit eingebauter Bestätigungsneigung, nur zur Gegenprobe.
struct NurBestaetigung;

impl Sortierer for NurBestaetigung {
    fn name(&self) -> &'static str {
        "Bestätigungsneigung"
    }
    fn ordne(&self, k: &[String], _: &[These]) -> Vec<usize> {
        let mut r: Vec<usize> = (0..k.len()).collect();
        r.sort_by_key(|i| {
            let t = k.get(*i).map(|s| s.to_lowercase()).unwrap_or_default();
            std::cmp::Reverse(
                ["hebt", "steigt", "rekord", "übertrifft"]
                    .iter()
                    .filter(|w| t.contains(*w))
                    .count(),
            )
        });
        r
    }
}

fn paare() -> Vec<(String, String)> {
    [
        (
            "NVIDIA hebt Umsatzziel für Rechenzentren an",
            "NVIDIA senkt Umsatzziel für Rechenzentren ab",
        ),
        (
            "SAP übertrifft Cloud-Erwartung deutlich",
            "SAP verfehlt Cloud-Erwartung deutlich",
        ),
        (
            "NVIDIA-Auftrag: Rekord bei Großkunde",
            "NVIDIA-Auftrag: Storno bei Großkunde",
        ),
    ]
    .iter()
    .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
    .collect()
}

/// Je Paar: Bestätigung und Gegenbeleg stehen zwischen Füllmeldungen, einmal in jeder
/// Reihenfolge. Gezählt wird, wie oft jede Art unter den ersten drei landet.
fn unter_drei(s: &dyn Sortierer, thesen: &[These]) -> (usize, usize) {
    let fuell = [
        "DAX leicht im Plus",
        "Ölpreis steigt nach OPEC",
        "Inflation Euroraum 2,1 %",
    ];
    let (mut ja, mut gegen) = (0_usize, 0_usize);
    for (best, wid) in paare() {
        for vorn_best in [true, false] {
            let mut k: Vec<String> = fuell.iter().map(|s| (*s).to_owned()).collect();
            let (a, b) = if vorn_best {
                (&best, &wid)
            } else {
                (&wid, &best)
            };
            k.insert(1, a.clone());
            k.push(b.clone());
            let top: Vec<&String> = s
                .ordne(&k, thesen)
                .into_iter()
                .take(3)
                .filter_map(|i| k.get(i))
                .collect();
            ja = ja.saturating_add(usize::from(top.contains(&&best)));
            gegen = gegen.saturating_add(usize::from(top.contains(&&wid)));
        }
    }
    (ja, gegen)
}

fn thesen() -> Result<Vec<These>, String> {
    Ok(vec![
        These {
            isin: Isin::neu("US67066G1040").map_err(|e| format!("{e:?}"))?,
            name: Some("NVIDIA Corp.".into()),
        },
        These {
            isin: Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?,
            name: Some("SAP SE".into()),
        },
    ])
}

#[test]
fn grundrang_ist_richtungsneutral() -> Result<(), String> {
    let (ja, gegen) = unter_drei(&Grundrang, &thesen()?);
    assert_eq!(ja, gegen, "Grundrang bevorzugt eine Richtung");
    assert_eq!(ja, 6);
    Ok(())
}

#[test]
fn gegenprobe_erkennt_bestaetigungsneigung() -> Result<(), String> {
    let (ja, gegen) = unter_drei(&NurBestaetigung, &thesen()?);
    assert!(ja > gegen, "Gegenprobe zu schwach: {ja} gegen {gegen}");
    Ok(())
}
