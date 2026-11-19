//! Abschnitt „Belegauswahl“ im Analyse-Export (#325, Vorbereitung NeuroSchlopf).
//!
//! Misst, wie oft der Beleg, auf den eine KI ihre Weltprognose stützt, bei einem Sortierer
//! unter den ersten drei steht. Heute zeigt die Frage die Belege in fester Reihenfolge; der
//! Grundrang ist der Maßstab für jeden späteren Sortierer. Nur lesend.

use std::{collections::BTreeMap, fmt::Write as _};

use depot_journal::Stand;
use depot_kern::{Datum, Isin, Zeitpunkt};
use depot_sim::belegrang::{
    BELEGPRUEFUNG, Grundrang, Heute, Sortierer, hat_fragebeleg, kandidatenanzahl, kaufbelege,
    messe, tage_mit_zielbelegen,
};

pub fn bericht(stand: &Stand, von: Datum, bis: Datum) -> String {
    let mut text = String::from(
        "\n### Belegauswahl (Vorbereitung NeuroSchlopf)\n\nDer Vergleich misst, ob ein zitierter Prognose- oder Kaufbeleg im Frageabschnitt des Journals unter den ersten drei steht. Welche Ereignisse im Wortlaut oder als Titel gesendet wurden, steht bei gespeicherten Aufrufen in den KI-Gesprächen. Unabhängige Relevanzurteile fehlen. nDCG@3 bleibt unbewertet.\n\n| Sortierer | Prognosen | unter den ersten drei | Quote | Käufe mit Beleg | unter den ersten drei | Quote |\n|---|---:|---:|---:|---:|---:|---:|\n",
    );
    let werte = depot_kern::liste::beobachtungsliste().unwrap_or_default();
    let universum = depot_kern::liste::als_universum(&werte);
    let grundrang = Grundrang::neu(&werte);
    let sortierer: [&dyn Sortierer; 2] = [&Heute, &grundrang];
    let mut kurze_listen = None;
    for s in sortierer {
        let m = messe(stand, &universum, s, von..=bis);
        kurze_listen.get_or_insert((
            m.prognosen_ohne_rangunterschied,
            m.kaeufe_ohne_rangunterschied,
        ));
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
    if let Some((prognosen, kaeufe)) = kurze_listen {
        let _ = writeln!(
            text,
            "\nFälle mit höchstens drei Kandidaten (Prognosen / Käufe): {prognosen} / {kaeufe}. Bei diesen Fällen führt jede Reihenfolge zu einem Treffer unter den ersten drei."
        );
    }
    datenbasis(&mut text, stand, von, bis);
    belegpruefungen(&mut text, stand, von, bis);
    text
}

pub fn schlagzeilen_abdeckung(stand: &Stand, von: Datum, bis: Datum, jetzt: Zeitpunkt) -> String {
    let Ok((aktien, anzahl)) = depot_sim::papiertag::schlagzeilen_abdeckung(stand, von, bis, jetzt)
    else {
        return "\n### Schlagzeilenabdeckung\n\nBeobachtungsliste nicht verfügbar.\n".into();
    };
    let mut text = format!(
        "\n### Schlagzeilenabdeckung\n\n**{} von {anzahl} beobachteten Aktien** hatten im Exportzeitraum mindestens eine belegte Schlagzeile mit erkanntem Wertbezug. Gezählt wird je Aktie einmal nach dem Frankfurter Abrufdatum. Das Wochenziel aus S4 liegt bei 10 Aktien.\n",
        aktien.len()
    );
    if !aktien.is_empty() {
        let isin = aktien.iter().map(Isin::als_str).collect::<Vec<_>>();
        let _ = writeln!(text, "\nErkannte ISINs im Zeitraum · {}", isin.join(", "));
    }
    text
}

#[derive(Default)]
struct Tagesbasis {
    kandidaten: usize,
    prognosen: usize,
    kaeufe: usize,
    ziele: usize,
}

/// Zählt Fälle mit mindestens einer auflösbaren Verknüpfung. Ein genannter Beleg
/// ist ein Hinweis auf Relevanz. Die übrigen Kandidaten bleiben unbewertet.
fn datenbasis(text: &mut String, stand: &Stand, von: Datum, bis: Datum) {
    let zeitraum = von..=bis;
    let mut tage = BTreeMap::<Datum, Tagesbasis>::new();
    for (tag, _) in stand.beobachtungen() {
        if zeitraum.contains(tag) {
            tage.entry(*tag).or_default();
        }
    }
    for (tag, basis) in &mut tage {
        basis.kandidaten = kandidatenanzahl(stand, *tag);
    }
    for p in stand.weltprognosen() {
        if let Some(tag) = tage.get_mut(&p.datum)
            && hat_fragebeleg(stand, p.datum, &[p.prognose.ereignis_nr])
        {
            tag.prognosen = tag.prognosen.saturating_add(1);
        }
    }
    for kauf in kaufbelege(stand) {
        if let Some(tag) = tage.get_mut(&kauf.datum)
            && hat_fragebeleg(stand, kauf.datum, &kauf.nummern)
        {
            tag.kaeufe = tag.kaeufe.saturating_add(1);
        }
    }
    for datum in tage_mit_zielbelegen(stand) {
        if let Some(tag) = tage.get_mut(&datum) {
            tag.ziele = tag.ziele.saturating_add(1);
        }
    }
    text.push_str("\n#### Datenbasis für NeuroSchlopf\n\nEin genannter Beleg ist ein Hinweis auf Relevanz. Kandidaten ohne Bewertung bleiben unbewertet. Die Tabelle zeigt Fälle mit mindestens einer auflösbaren Belegnummer. Bei Altständen ohne Frageabschnitt gilt die vollständige Beobachtungsliste des Tages. Vorschlagsziele zählen je Berater und Wert unabhängig vom späteren Urteil oder Auftrag.\n\n");
    if tage.is_empty() {
        text.push_str("Im Zeitraum sind keine Kandidaten im Journal belegt.\n");
        return;
    }
    let tage_mit_verknuepfung = tage
        .values()
        .filter(|t| t.prognosen != 0 || t.kaeufe != 0 || t.ziele != 0)
        .count();
    let _ = writeln!(
        text,
        "**Tage mit verknüpften Belegen: {tage_mit_verknuepfung} von {} Tagen mit Kandidaten.**\n",
        tage.len()
    );
    text.push_str(
        "| Tag | Kandidaten | Prognosen mit Beleg | Käufe mit Beleg | Vorschlagsziele mit Beleg |\n|---|---:|---:|---:|---:|\n",
    );
    for (tag, t) in tage {
        let _ = writeln!(
            text,
            "| {tag} | {} | {} | {} | {} |",
            t.kandidaten, t.prognosen, t.kaeufe, t.ziele
        );
    }
}

fn belegpruefungen(text: &mut String, stand: &Stand, von: Datum, bis: Datum) {
    let zeitraum = von..=bis;
    let mut thesen = BTreeMap::<_, usize>::new();
    for (tag, status) in depot_sim::schlopfmodell::belegte_pruefungen(stand) {
        if zeitraum.contains(&tag) {
            let anzahl = thesen.entry((tag, status)).or_default();
            *anzahl = anzahl.saturating_add(1);
        }
    }
    let mut verwerfungen = BTreeMap::<_, usize>::new();
    for (tag, zweck, grund) in stand.begruendungen() {
        if zweck == BELEGPRUEFUNG
            && zeitraum.contains(tag)
            && let Some((anzahl, grund)) = grund.split_once(' ')
            && let Ok(anzahl) = anzahl.parse::<usize>()
            && anzahl > 0
        {
            let summe = verwerfungen.entry((*tag, grund)).or_default();
            *summe = summe.saturating_add(anzahl);
        }
    }
    text.push_str("\n#### Thesenprüfungen und verworfene Belege\n\n");
    if thesen.is_empty() && verwerfungen.is_empty() {
        text.push_str("Im Zeitraum sind keine Thesenprüfungen mit Quelle und keine Belegverwerfungen protokolliert. Ältere Stände protokollieren die neuen Verwerfungsgründe noch nicht.\n");
        return;
    }
    text.push_str("| Tag | Protokollierte Prüfung oder Verwerfung | Anzahl |\n|---|---|---:|\n");
    for ((tag, status), anzahl) in thesen {
        let _ = writeln!(text, "| {tag} | These {status} | {anzahl} |");
    }
    for ((tag, grund), anzahl) in verwerfungen {
        let _ = writeln!(text, "| {tag} | {grund} | {anzahl} |");
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
        assert!(
            t.contains("| Grundrang v2 | 0 | 0 | – | 0 | 0 | – |"),
            "{t}"
        );
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
            j.beobachte(danach, "Beleg für Reduktion").map_err(fehler)?;
            for text in [
                "Maus DE0007164600 0 1",
                "Maus DE0007164600 0 1",
                "Dachs DE0007164600 2000 2",
            ] {
                j.begruende(danach, depot_sim::belegrang::ZIELBELEG, text)
                    .map_err(fehler)?;
            }
            j.begruende(
                danach,
                BELEGPRUEFUNG,
                "2 Dachs Zielbeleg nicht in der Frage sichtbar",
            )
            .map_err(fehler)?;
            j.begruende(danach, "SchlopfModell",
                r#"These v1 {"wer":"Maus","tag":"2026-11-16","isin":"DE0007164600","stand":1794898800,"status":"zurückziehen","begründung":"Gegenbeleg","beleg":2}"#)
                .map_err(fehler)?;
            j.beobachte(ausserhalb, "Außerhalb").map_err(fehler)?;
            let t = bericht(j.stand(), tag, danach);
            assert!(t.contains("Tage mit verknüpften Belegen: 2 von 2"), "{t}");
            assert!(t.contains("| 2026-11-16 | 2 | 0 | 1 | 0 |"), "{t}");
            assert!(t.contains("| 2026-11-17 | 1 | 0 | 0 | 1 |"), "{t}");
            assert!(t.contains("| 2026-11-17 | These zurückziehen | 1 |"), "{t}");
            assert!(
                t.contains("| 2026-11-17 | Dachs Zielbeleg nicht in der Frage sichtbar | 2 |"),
                "{t}"
            );
            assert!(
                t.contains("höchstens drei Kandidaten (Prognosen / Käufe): 0 / 1"),
                "{t}"
            );
            assert!(!t.contains("| 2026-11-18 |"), "{t}");
            Ok(())
        })();
        std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
        ergebnis
    }
}
