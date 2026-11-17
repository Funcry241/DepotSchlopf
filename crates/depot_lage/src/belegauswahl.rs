//! Abschnitt „Belegauswahl“ im Analyse-Export (#325, Vorbereitung NeuroSchlopf).
//!
//! Misst, wie oft der Beleg, auf den eine KI ihre Weltprognose stützt, bei einem Sortierer
//! unter den ersten drei steht. Heute zeigt die Frage die Belege in fester Reihenfolge; der
//! Grundrang ist der Maßstab für jeden späteren Sortierer. Nur lesend.

use std::{collections::BTreeMap, fmt::Write as _};

use depot_journal::Stand;
use depot_kern::Datum;
use depot_sim::belegrang::{Grundrang, Heute, Sortierer, kaufbelege, messe};

pub fn bericht(stand: &Stand, von: Datum, bis: Datum) -> String {
    let mut text = String::from(
        "\n### Belegauswahl (Vorbereitung NeuroSchlopf)\n\nWie oft der Beleg einer Weltprognose unter den ersten drei steht. Kandidaten sind die Belege, die die Frage am Prognosetag gesehen hat; Thesen sind die Werte, die der Berater in den fünf Tagen davor genannt hat. Käufe zählen, wenn die KI ihre Belegnummern genannt hat (Kaufbeleg). Nur lesend, die Frage ändert sich nicht.\n\n| Sortierer | Prognosen | unter den ersten drei | Quote | Käufe mit Beleg | unter den ersten drei | Quote |\n|---|---:|---:|---:|---:|---:|---:|\n",
    );
    let universum = depot_kern::liste::beobachtungsliste()
        .map(|w| depot_kern::liste::als_universum(&w))
        .unwrap_or_default();
    let sortierer: [&dyn Sortierer; 2] = [&Heute, &Grundrang];
    for s in sortierer {
        let m = messe(stand, &universum, s, von..=bis);
        let _ = writeln!(
            text,
            "| {} | {} | {} | {} | {} | {} | {} |",
            s.name(),
            m.faelle,
            m.unter_drei,
            quote(m.unter_drei, m.faelle),
            m.kauf_faelle,
            m.kauf_unter_drei,
            quote(m.kauf_unter_drei, m.kauf_faelle)
        );
    }
    datenbasis(&mut text, stand, von, bis);
    text
}

#[derive(Default)]
struct Tagesbasis {
    kandidaten: usize,
    prognosen: usize,
    kaeufe: usize,
}

/// Zählt Fälle mit mindestens einer auflösbaren Verknüpfung. Ein genannter Beleg
/// ist ein Hinweis auf Relevanz. Die übrigen Kandidaten bleiben unbewertet.
fn datenbasis(text: &mut String, stand: &Stand, von: Datum, bis: Datum) {
    let zeitraum = von..=bis;
    let mut tage = BTreeMap::<Datum, Tagesbasis>::new();
    for (tag, _) in stand.beobachtungen() {
        if zeitraum.contains(tag) {
            let basis = tage.entry(*tag).or_default();
            basis.kandidaten = basis.kandidaten.saturating_add(1);
        }
    }
    for p in stand.weltprognosen() {
        if let Some(tag) = tage.get_mut(&p.datum)
            && (1..=tag.kandidaten).contains(&p.prognose.ereignis_nr)
        {
            tag.prognosen = tag.prognosen.saturating_add(1);
        }
    }
    for kauf in kaufbelege(stand) {
        if let Some(tag) = tage.get_mut(&kauf.datum)
            && kauf
                .nummern
                .iter()
                .any(|n| (1..=tag.kandidaten).contains(n))
        {
            tag.kaeufe = tag.kaeufe.saturating_add(1);
        }
    }
    text.push_str("\n#### Datenbasis für NeuroSchlopf\n\nEin genannter Beleg ist ein Hinweis auf Relevanz. Kandidaten ohne Bewertung bleiben unbewertet. Die Tabelle zeigt Fälle mit mindestens einer auflösbaren Belegnummer.\n\n");
    if tage.is_empty() {
        text.push_str("Im Zeitraum sind keine Kandidaten im Journal belegt.\n");
        return;
    }
    let tage_mit_verknuepfung = tage
        .values()
        .filter(|t| t.prognosen != 0 || t.kaeufe != 0)
        .count();
    let _ = writeln!(
        text,
        "**Tage mit verknüpften Belegen: {tage_mit_verknuepfung} von {} Tagen mit Kandidaten.**\n",
        tage.len()
    );
    text.push_str(
        "| Tag | Kandidaten | Prognosen mit Beleg | Käufe mit Beleg |\n|---|---:|---:|---:|\n",
    );
    for (tag, t) in tage {
        let _ = writeln!(
            text,
            "| {tag} | {} | {} | {} |",
            t.kandidaten, t.prognosen, t.kaeufe
        );
    }
}

fn quote(treffer: u64, faelle: u64) -> String {
    treffer
        .saturating_mul(100)
        .checked_div(faelle)
        .map_or_else(|| "–".into(), |q| format!("{q} %"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ohne_prognosen_leere_quote() -> Result<(), String> {
        let stand = Stand::default();
        let d = Datum::neu(2026, 11, 16).ok_or("Datum")?;
        let t = bericht(&stand, d, d);
        assert!(
            t.contains("| heutige Reihenfolge | 0 | 0 | – | 0 | 0 | – |"),
            "{t}"
        );
        assert!(t.contains("| Grundrang | 0 | 0 | – | 0 | 0 | – |"), "{t}");
        assert!(
            t.contains("Im Zeitraum sind keine Kandidaten im Journal belegt."),
            "{t}"
        );
        Ok(())
    }
    #[test]
    fn datenbasis_zaehlt_nur_aufloesbare_verknuepfungen_im_zeitraum() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("depot_belegauswahl_{}", std::process::id()));
        std::fs::create_dir(&dir).map_err(|e| e.to_string())?;
        let ergebnis = (|| {
            let tag = Datum::neu(2026, 11, 16).ok_or("Datum")?;
            let danach = tag.plus_tage(1).ok_or("Folgetag")?;
            let ausserhalb = danach.plus_tage(1).ok_or("Dritter Tag")?;
            let pfad = dir.join("journal.txt");
            let fehler = |e| format!("{e:?}");
            depot_journal::Journal::einrichten(&pfad).map_err(fehler)?;
            let mut j = depot_journal::Journal::oeffnen(&pfad).map_err(fehler)?;
            j.beobachte(tag, "Erster Kandidat").map_err(fehler)?;
            j.beobachte(tag, "Zweiter Kandidat").map_err(fehler)?;
            j.begruende(tag, depot_sim::belegrang::KAUFBELEG, "1 Dachs 2")
                .map_err(fehler)?;
            j.begruende(tag, depot_sim::belegrang::KAUFBELEG, "2 Maus 3")
                .map_err(fehler)?;
            j.beobachte(danach, "Ohne Bewertung").map_err(fehler)?;
            j.beobachte(ausserhalb, "Außerhalb").map_err(fehler)?;
            let t = bericht(j.stand(), tag, danach);
            assert!(t.contains("Tage mit verknüpften Belegen: 1 von 2"), "{t}");
            assert!(t.contains("| 2026-11-16 | 2 | 0 | 1 |"), "{t}");
            assert!(t.contains("| 2026-11-17 | 1 | 0 | 0 |"), "{t}");
            assert!(!t.contains("| 2026-11-18 |"), "{t}");
            Ok(())
        })();
        std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
        ergebnis
    }
}
