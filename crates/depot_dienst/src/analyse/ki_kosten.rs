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
    pub(super) gemini_netzfehler: BTreeSet<depot_kern::Zeitpunkt>,
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
                if e.ergebnis == "netz" && e.http.is_none() {
                    self.gemini_netzfehler.insert(e.zeit);
                }
                self.gemini
                    .entry(tag)
                    .or_default()
                    .erfasse(e.token_ein, e.token_aus);
            }
            let (Some(nr), Some(wer)) = (e.buchung, berater(&e.wer)) else {
                continue;
            };
            // Echte Transportwiederholungen erfolgen innerhalb von fragen() und erzeugen
            // gemeinsam einen Gesprächseintrag. Zwei Zeilen derselben Buchung sind mehrdeutig.
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
    reserviert: Option<Cent>,
    offen: usize,
    token: Tokenbelege,
}

impl Default for Tagessumme {
    fn default() -> Self {
        Self {
            kosten: Some(Cent::NULL),
            reserviert: Some(Cent::NULL),
            offen: 0,
            token: Tokenbelege::default(),
        }
    }
}

impl Tagessumme {
    fn buche(&mut self, betrag: Cent, abgerechnet: bool) {
        let summe = if abgerechnet {
            &mut self.kosten
        } else {
            self.offen = self.offen.saturating_add(1);
            &mut self.reserviert
        };
        *summe = summe.and_then(|wert| wert.checked_add(betrag));
    }
}

pub(super) fn bericht(
    stand: &Stand,
    zeitraum: Zeitraum,
    depotwerte: &BTreeMap<Datum, Cent>,
    belege: &Gespraechsbelege,
) -> String {
    let mut text = String::from(
        "\n## KI-Kosten\n\nBeträge stammen aus dem Journal. Offene Reservierungen stehen mit ihrer Höchstsumme separat. Die Tokenzahlen summieren nur eindeutig zugeordnete Belege.\n\n",
    );
    let mut je_tag = BTreeMap::<(Datum, &str), Tagessumme>::new();
    let mut aufrufe = String::new();
    let mut gesamt = Tagessumme::default();
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
        summe.buche(b.reserviert, abgerechnet);
        gesamt.buche(b.reserviert, abgerechnet);
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
    let _ = writeln!(
        text,
        "**Im Zeitraum abgerechnet: {}. Offene Reservierungen: {} ({} Buchungen).**\n",
        geld(gesamt.kosten),
        geld(gesamt.reserviert),
        gesamt.offen
    );
    if !je_tag.is_empty() || !belege.gemini.is_empty() {
        text.push_str("| Tag | KI | Abgerechnet | Offen reserviert | Aufrufe mit vollständigem Tokenbeleg | Token ein | Token aus |\n|---|---|---:|---:|---|---:|---:|\n");
        for ((tag, wer), s) in je_tag {
            let _ = writeln!(
                text,
                "| {tag} | {wer} | {} | {} ({}) | {} von {} | {} | {} |",
                geld(s.kosten),
                geld(s.reserviert),
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
        .enumerate()
        .filter(|(nr, b)| {
            b.datum > *anfang
                && b.datum <= *ende
                && stand
                    .ki()
                    .buchungsbeleg(*nr)
                    .is_some_and(|(_, abgerechnet)| abgerechnet)
        })
        .try_fold(Cent::NULL, |summe, (_, b)| summe.checked_add(b.reserviert));
    let ertrag = schluss.checked_sub(*start);
    let anteil = kosten.zip(ertrag).and_then(|(k, e)| anteil_bp(k, e));
    let quote = anteil.map_or_else(|| "–".into(), |bp| prozent(Some(bp)));
    let _ = writeln!(
        text,
        "\nKI-Kosten im Verhältnis zum Ertrag: {quote}. Von Abschluss {anfang} bis Abschluss {ende}: {} abgerechnete KI-Kosten und {} Depotwertänderung vor KI-Kosten. Die Kosten umfassen Buchungstage nach dem ersten bis einschließlich dem letzten Abschluss. Eine Quote gibt es bei positiver Depotwertänderung.",
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

fn anteil_bp(kosten: Cent, ertrag: Cent) -> Option<i64> {
    if ertrag <= Cent::NULL {
        return None;
    }
    let anteil = i128::from(kosten.0)
        .checked_mul(i128::from(depot_kern::BP_GANZ))?
        .checked_div(i128::from(ertrag.0))?;
    i64::try_from(anteil).ok()
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
        let danach = bis.plus_tage(1).ok_or("Tag außerhalb")?;
        let ausserhalb = journal
            .reserviere(BeraterId::Dachs, danach, 3)
            .map_err(|e| format!("{e:?}"))?;
        assert!(ausserhalb.buchung().reserviert > Cent::NULL);
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
        let gemini = Gespraech {
            wer: "gemini".into(),
            buchung: None,
            token_ein: Some(7),
            token_aus: Some(2),
            ..gespr.clone()
        };
        let uebersprungen = Gespraech {
            ergebnis: "uebersprungen".into(),
            token_ein: Some(100),
            ..gemini.clone()
        };
        belege.erfasse(&[gespr.clone(), gemini, uebersprungen], 0);
        let text = bericht(journal.stand(), zeitraum, &werte, &belege);
        assert!(text.contains("| Dachs | 0,00 € | 0,00 € (0) | 0 von 1 | – | – |"));
        assert!(text.contains(&format!(
            "| Maus | 0,00 € | {betrag} (1) | 1 von 1 | 12 | 3 |"
        )));
        assert!(text.contains("| 0 | 0,00 € | abgerechnet |"));
        assert!(text.contains(&format!("| 1 | {betrag} | offen | 12 | 3 |")));
        assert!(text.contains("0,00 € abgerechnete KI-Kosten und 1,00 € Depotwertänderung"));
        assert!(!text.contains("9,99 €"));
        assert!(text.contains(&format!(
            "Im Zeitraum abgerechnet: 0,00 €. Offene Reservierungen: {betrag} (1 Buchungen)."
        )));
        assert!(!text.contains(&format!("| {danach} |")));
        assert!(text.contains("| Gemini | nicht im Journal gebucht | – | 1 von 1 | 7 | 2 |"));
        for endwert in [Cent(100_000), Cent(99_999)] {
            let werte = [(von, Cent(100_000)), (bis, endwert)].into();
            assert!(
                bericht(journal.stand(), zeitraum, &werte, &belege)
                    .contains("KI-Kosten im Verhältnis zum Ertrag: –.")
            );
        }
        let werte_ein_tag = [(von, Cent(100_000))].into();
        assert!(
            bericht(journal.stand(), zeitraum, &werte_ein_tag, &belege)
                .contains("nicht belegbar. Zwei belegte Tageswerte fehlen.")
        );
        assert_eq!(anteil_bp(Cent(50), Cent(100)), Some(5_000));
        belege.erfasse(&[gespr], 0);
        let text = bericht(journal.stand(), zeitraum, &werte, &belege);
        assert!(text.contains(&format!(
            "| Maus | 0,00 € | {betrag} (1) | 0 von 1 | – | – |"
        )));
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
