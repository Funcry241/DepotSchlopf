//! Abschnitt „Belegauswahl“ im Analyse-Export (#325, Vorbereitung NeuroSchlopf).
//!
//! Misst, wie oft der Beleg, auf den eine KI ihre Weltprognose stützt, bei einem Sortierer
//! unter den ersten drei steht. Heute zeigt die Frage die Belege in fester Reihenfolge; der
//! Grundrang ist der Maßstab für jeden späteren Sortierer. Nur lesend.

use std::fmt::Write as _;

use depot_journal::Stand;
use depot_kern::Datum;
use depot_sim::belegrang::{Grundrang, Heute, Sortierer, messe};

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
    text
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
        Ok(())
    }
}
