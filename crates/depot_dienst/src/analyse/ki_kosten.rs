//! Kosten aus dem bestätigten Journal. Gesprächsprotokolle ergänzen belegte Token und Fragen.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use depot_journal::Stand;
use depot_kern::{BeraterId, Cent, Datum};
use depot_lage::gespraeche::Gespraech;

use super::{Zeitraum, geld, prozent};

#[derive(Default)]
pub(super) struct Gespraechsbelege {
    buchungen: BTreeMap<usize, Option<Aufruf>>,
    gemini: BTreeMap<Datum, Tokenbelege>,
    fragen: usize,
    doppelte_urls: usize,
    fehlende_tage: usize,
    unlesbare_zeilen: usize,
}

#[derive(Default)]
struct Tokenbelege {
    anzahl: usize,
    vollstaendig: usize,
    ein: Option<u64>,
    aus: Option<u64>,
}

impl Tokenbelege {
    fn erfasse(&mut self, ein: Option<u64>, aus: Option<u64>) {
        self.anzahl = self.anzahl.saturating_add(1);
        self.vollstaendig = self
            .vollstaendig
            .saturating_add(usize::from(ein.is_some() && aus.is_some()));
        addiere_token(&mut self.ein, ein);
        addiere_token(&mut self.aus, aus);
    }
}

struct Aufruf {
    tag: Datum,
    wer: BeraterId,
    zweck: String,
    ein: Option<u64>,
    aus: Option<u64>,
}

impl Gespraechsbelege {
    pub(super) fn fehlender_tag(&mut self) {
        self.fehlende_tage = self.fehlende_tage.saturating_add(1);
    }

    pub(super) fn erfasse(&mut self, eintraege: &[Gespraech], fehler: usize) {
        self.unlesbare_zeilen = self.unlesbare_zeilen.saturating_add(fehler);
        for e in eintraege {
            let Some(tag) = e.zeit.frankfurt_tag() else {
                continue;
            };
            if let Some(frage) = e.frage.as_deref() {
                self.fragen = self.fragen.saturating_add(1);
                self.doppelte_urls = self.doppelte_urls.saturating_add(doppelte_urls(frage));
            }
            if e.wer == "gemini" && e.ergebnis != "uebersprungen" {
                self.gemini
                    .entry(tag)
                    .or_default()
                    .erfasse(e.token_ein, e.token_aus);
            }
            let (Some(nr), Some(wer)) = (e.buchung, berater(&e.wer)) else {
                continue;
            };
            self.buchungen
                .entry(nr)
                .and_modify(|beleg| *beleg = None)
                .or_insert_with(|| {
                    Some(Aufruf {
                        tag,
                        wer,
                        zweck: e.zweck.clone(),
                        ein: e.token_ein,
                        aus: e.token_aus,
                    })
                });
        }
    }
}

fn berater(wer: &str) -> Option<BeraterId> {
    match wer {
        "dachs" => Some(BeraterId::Dachs),
        "maus" => Some(BeraterId::Maus),
        _ => None,
    }
}

fn doppelte_urls(frage: &str) -> usize {
    let mut urls = BTreeSet::new();
    frage
        .lines()
        .filter_map(|zeile| {
            let (nummer, nachricht) = zeile.strip_prefix("- Ereignis ")?.split_once(": ")?;
            if nummer.parse::<usize>().is_err() || !nachricht.starts_with("Nachricht ") {
                return None;
            }
            let (_, link) = nachricht.rsplit_once(" | https://")?;
            link.split_whitespace().next()
        })
        .filter(|url| !urls.insert(*url))
        .count()
}

fn addiere_token(summe: &mut Option<u64>, wert: Option<u64>) {
    if let Some(wert) = wert {
        *summe = Some(summe.unwrap_or(0).saturating_add(wert));
    }
}

fn token(wert: Option<u64>) -> String {
    wert.map_or_else(|| "–".into(), |wert| wert.to_string())
}

struct Tagessumme {
    kosten: Option<Cent>,
    offen: usize,
    token: Tokenbelege,
}

impl Default for Tagessumme {
    fn default() -> Self {
        Self {
            kosten: Some(Cent::NULL),
            offen: 0,
            token: Tokenbelege::default(),
        }
    }
}

pub(super) fn bericht(
    stand: &Stand,
    zeitraum: Zeitraum,
    depotwerte: &BTreeMap<Datum, Cent>,
    belege: &Gespraechsbelege,
) -> String {
    let mut text = String::from(
        "\n## KI-Kosten\n\nBeträge stammen aus dem Journal. Offene Reservierungen zählen mit ihrer Höchstsumme. Die Tokenzahlen summieren nur eindeutig zugeordnete Belege.\n\n",
    );
    let mut je_tag = BTreeMap::<(Datum, &str), Tagessumme>::new();
    let mut aufrufe = String::new();
    let mut gesamt = Some(Cent::NULL);
    for (nr, b) in stand
        .ki()
        .buchungen()
        .iter()
        .enumerate()
        .filter(|(_, b)| (zeitraum.von..=zeitraum.bis).contains(&b.datum))
    {
        let Some((zweck, abgerechnet)) = stand.ki().buchungsbeleg(nr) else {
            continue;
        };
        let wer = match b.berater {
            BeraterId::Dachs => "Dachs",
            BeraterId::Maus => "Maus",
        };
        let aufruf = belege
            .buchungen
            .get(&nr)
            .and_then(Option::as_ref)
            .filter(|a| a.tag == b.datum && a.wer == b.berater && a.zweck == zweck.text());
        let ein = aufruf.and_then(|a| a.ein);
        let aus = aufruf.and_then(|a| a.aus);
        let summe = je_tag.entry((b.datum, wer)).or_default();
        summe.kosten = summe.kosten.and_then(|k| k.checked_add(b.reserviert));
        gesamt = gesamt.and_then(|k| k.checked_add(b.reserviert));
        summe.offen = summe.offen.saturating_add(usize::from(!abgerechnet));
        summe.token.erfasse(ein, aus);
        let status = if abgerechnet { "abgerechnet" } else { "offen" };
        let _ = writeln!(
            aufrufe,
            "| {} | {wer} | {} | {} | {nr} | {} | {status} | {} | {} |",
            b.datum,
            b.zyklus,
            super::gespraeche::zweck_text(zweck.text()),
            b.reserviert,
            token(ein),
            token(aus)
        );
    }
    let _ = writeln!(text, "**Im Zeitraum gebucht: {}.**\n", geld(gesamt));
    if !je_tag.is_empty() || !belege.gemini.is_empty() {
        text.push_str("| Tag | KI | Gebucht | Offene Reservierungen | Aufrufe mit vollständigem Tokenbeleg | Token ein | Token aus |\n|---|---|---:|---:|---|---:|---:|\n");
        for ((tag, wer), s) in je_tag {
            let _ = writeln!(
                text,
                "| {tag} | {wer} | {} | {} | {} von {} | {} | {} |",
                geld(s.kosten),
                s.offen,
                s.token.vollstaendig,
                s.token.anzahl,
                token(s.token.ein),
                token(s.token.aus)
            );
        }
        for (tag, s) in &belege.gemini {
            let _ = writeln!(
                text,
                "| {tag} | Gemini | nicht im Journal gebucht | – | {} von {} | {} | {} |",
                s.vollstaendig,
                s.anzahl,
                token(s.ein),
                token(s.aus)
            );
        }
    }
    if !aufrufe.is_empty() {
        text.push_str("\n### Buchungen je Entscheidung\n\n| Tag | KI | Zyklus | Entscheidungsphase | Buchung | Betrag | Status | Token ein | Token aus |\n|---|---|---:|---|---:|---:|---|---:|---:|\n");
        text.push_str(&aufrufe);
    }
    let _ = writeln!(
        text,
        "\nDoppelte Nachrichten-URLs in {} belegten Fragen: {} Wiederholungen innerhalb derselben Frage. Nicht gespeicherte Fragen bleiben offen.",
        belege.fragen, belege.doppelte_urls
    );
    if belege.fehlende_tage != 0 || belege.unlesbare_zeilen != 0 {
        let _ = writeln!(
            text,
            "Gesprächsbelege fehlen an {} Tagen, {} Zeilen sind unlesbar.",
            belege.fehlende_tage, belege.unlesbare_zeilen
        );
    }
    verhaeltnis(&mut text, stand, zeitraum, depotwerte);
    text
}

fn verhaeltnis(
    text: &mut String,
    stand: &Stand,
    zeitraum: Zeitraum,
    depotwerte: &BTreeMap<Datum, Cent>,
) {
    let Some(((anfang, start), (ende, schluss))) = depotwerte
        .first_key_value()
        .zip(depotwerte.last_key_value())
        .filter(|((anfang, _), (ende, _))| anfang < ende)
    else {
        text.push_str("\nKI-Kosten im Verhältnis zum Ertrag: nicht belegbar. Zwei belegte Tageswerte fehlen.\n");
        return;
    };
    let kosten = stand
        .ki()
        .buchungen()
        .iter()
        .filter(|b| b.datum > *anfang && b.datum <= *ende)
        .try_fold(Cent::NULL, |summe, b| summe.checked_add(b.reserviert));
    let ertrag = schluss.checked_sub(*start);
    let anteil = kosten
        .zip(ertrag)
        .filter(|(_, e)| *e > Cent::NULL)
        .and_then(|(k, e)| depot_lage::vergleich::rendite_bp(e, k)?.checked_add(10_000));
    let _ = writeln!(
        text,
        "\nKI-Kosten im Verhältnis zum Ertrag: {}. Von Abschluss {anfang} bis Abschluss {ende}: {} gebuchte KI-Kosten und {} Depotwertänderung vor KI-Kosten. Die Kosten umfassen Buchungstage nach dem ersten bis einschließlich dem letzten Abschluss. Eine Quote gibt es bei positiver Depotwertänderung.",
        prozent(anteil),
        geld(kosten),
        geld(ertrag)
    );
    if !zeitraum
        .tage()
        .filter(|t| *t >= *anfang && *t <= *ende && depot_kern::liste::handelstag(*t))
        .all(|t| depotwerte.contains_key(&t))
    {
        text.push_str("Tageswerte innerhalb dieses Abschnitts fehlen.\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_journal::{Journal, KiZweck};
    use depot_kern::Zeitpunkt;

    #[test]
    fn kosten_und_token_bleiben_an_ihrer_journalbuchung() -> Result<(), String> {
        let ordner = std::env::temp_dir().join(format!("depot-kosten-{}", crate::neues_token()));
        std::fs::create_dir(&ordner).map_err(|e| e.to_string())?;
        let pfad = ordner.join("journal.txt");
        let fehler = |e| format!("{e:?}");
        Journal::einrichten(&pfad).map_err(fehler)?;
        let mut journal = Journal::oeffnen(&pfad).map_err(fehler)?;
        let von = Datum::neu(2030, 1, 7).ok_or("Erster Tag")?;
        let bis = von.plus_tage(1).ok_or("Zweiter Tag")?;
        let erste = journal
            .reserviere(BeraterId::Dachs, von, 1)
            .map_err(|e| format!("{e:?}"))?;
        journal
            .rechne_ki_ab(&erste.abrechnung(0, 0).ok_or("Abrechnung")?)
            .map_err(fehler)?;
        let zweite = journal
            .reserviere_fuer(BeraterId::Maus, bis, 2, KiZweck::Erstmeinung)
            .map_err(|e| format!("{e:?}"))?;
        let betrag = zweite.buchung().reserviert;
        let gespr = Gespraech {
            zeit: Zeitpunkt::neu(bis, 12, 0, 0).ok_or("Zeit")?,
            lauf: "beispiel".into(),
            wer: "maus".into(),
            rolle: "haupt".into(),
            modell: "testmodell".into(),
            zweck: "erstmeinung".into(),
            ergebnis: "ok".into(),
            http: Some(200),
            token_ein: Some(12),
            token_aus: Some(3),
            buchung: Some(zweite.buchungsnummer()),
            kosten_cent: Some(999),
            system: None,
            frage: None,
            antwort: None,
            fehler: None,
        };
        let werte = [(von, Cent(100_000)), (bis, Cent(100_100))].into();
        let zeitraum = Zeitraum { von, bis };
        let mut belege = Gespraechsbelege::default();
        belege.erfasse(std::slice::from_ref(&gespr), 0);
        let text = bericht(journal.stand(), zeitraum, &werte, &belege);
        assert!(text.contains("| Dachs | 0,00 € | 0 | 0 von 1 | – | – |"));
        assert!(text.contains(&format!("| Maus | {betrag} | 1 | 1 von 1 | 12 | 3 |")));
        assert!(text.contains("| 0 | 0,00 € | abgerechnet |"));
        assert!(text.contains(&format!("| 1 | {betrag} | offen | 12 | 3 |")));
        assert!(text.contains(&format!(
            "{betrag} gebuchte KI-Kosten und 1,00 € Depotwertänderung"
        )));
        assert!(!text.contains("9,99 €"));
        belege.erfasse(&[gespr], 0);
        let text = bericht(journal.stand(), zeitraum, &werte, &belege);
        assert!(text.contains(&format!("| Maus | {betrag} | 1 | 0 von 1 | – | – |")));
        drop(journal);
        std::fs::remove_dir_all(ordner).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn zaehlt_nur_doppelte_links_innerhalb_nummerierter_nachrichten() {
        let frage = "- Ereignis 1: Nachricht Beispiel | https://example.test/eins [Makro]\n\
                     - Ereignis 2: Nachricht Korrektur | https://example.test/eins\n\
                     - Ereignis 3: Nachricht Zweiter Text | https://example.test/zwei\n\
                     - Ereignis 4: Termin | https://example.test/eins\n\
                     Beispiel | https://example.test/eins";
        assert_eq!(doppelte_urls(frage), 1);
        assert_eq!(
            doppelte_urls("- Ereignis 1: Nachricht Beispiel | https://example.test/eins"),
            0
        );
    }
}
