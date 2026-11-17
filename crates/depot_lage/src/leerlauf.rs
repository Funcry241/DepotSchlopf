//! Leerlauf messen (#311 M0): Wie lange liegt Geld als Bargeld, wie oft füllen Orders, wie
//! lange dauert es vom Verkauf bis zum nächsten Kauf. Alles liest nur; das Journal ändert sich
//! nicht. Die Zahlen gelten je Topf und Kalenderwoche (Montag als Schlüssel).

use std::collections::BTreeMap;

use depot_journal::{AuftragsZustand, Stand};
use depot_kern::{Cent, Datum, TopfId};

use crate::ausfuehrung::{Anlass, OrderZeile, Richtung};
use crate::depotverlauf::Tagespunkt;

/// Montag der Kalenderwoche eines Tages.
pub fn wochenbeginn(tag: Datum) -> Option<Datum> {
    let wochentag = i64::from(tag.wochentag()?);
    tag.plus_tage(1_i64.checked_sub(wochentag)?)
}

/// Bargeld eines Topfes zum belegten Tagesabschluss.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BargeldTag {
    pub tag: Datum,
    pub topf: TopfId,
    pub bargeld: Cent,
    /// Topfwert zum selben Abschluss, falls bewertet.
    pub wert: Option<Cent>,
}

/// Bargeld je Topf und Woche.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BargeldWoche {
    pub handelstage: u32,
    /// Summe des Bargelds über die Handelstage, in Cent mal Tagen.
    pub cent_tage: i128,
    /// Mittlere Bargeldquote der bewerteten Tage in Basispunkten.
    pub quote_bp: Option<i64>,
    quoten: i128,
    bewertet: i128,
}

impl BargeldWoche {
    /// Euro-Tage in Bargeld, auf ganze Euro abgerundet.
    pub fn euro_tage(&self) -> i128 {
        self.cent_tage.div_euclid(100)
    }
}

/// Die Bargeldtage aus den belegten Tagesabschlüssen; Tage ohne Handel zählen nicht.
pub fn bargeldtage(stand: &Stand, punkte: &[Tagespunkt]) -> Vec<BargeldTag> {
    let werte: BTreeMap<_, _> = punkte.iter().map(|p| (p.tag, &p.topfwerte)).collect();
    stand
        .tagesschluesse()
        .iter()
        .filter(|(tag, _)| depot_kern::liste::handelstag(*tag))
        .flat_map(|(tag, toepfe)| {
            let werte = werte.get(tag).copied();
            toepfe.iter().map(move |(&topf, t)| BargeldTag {
                tag: *tag,
                topf,
                bargeld: t.bargeld(),
                wert: werte.and_then(|w| w.get(&topf).copied()),
            })
        })
        .collect()
}

pub fn bargeld_je_woche(tage: &[BargeldTag]) -> BTreeMap<(TopfId, Datum), BargeldWoche> {
    let mut aus = BTreeMap::<_, BargeldWoche>::new();
    for t in tage {
        let Some(woche) = wochenbeginn(t.tag) else {
            continue;
        };
        let w = aus.entry((t.topf, woche)).or_default();
        w.handelstage = w.handelstage.saturating_add(1);
        w.cent_tage = w.cent_tage.saturating_add(i128::from(t.bargeld.0));
        if let Some(quote) = t.wert.filter(|wert| wert.0 > 0).and_then(|wert| {
            i128::from(t.bargeld.0)
                .checked_mul(10_000)?
                .checked_div(i128::from(wert.0))
        }) {
            w.quoten = w.quoten.saturating_add(quote);
            w.bewertet = w.bewertet.saturating_add(1);
        }
    }
    for w in aus.values_mut() {
        w.quote_bp = w
            .quoten
            .checked_div(w.bewertet)
            .and_then(|q| i64::try_from(q).ok());
    }
    aus
}

/// Herkunft einer Order für die Auswertung: KI-Vorschlag oder erzwungener Ausstieg.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Herkunft {
    Ki,
    Erzwungen,
}

impl Herkunft {
    pub fn von(anlass: Anlass) -> Self {
        if anlass.zwingend() {
            Self::Erzwungen
        } else {
            Self::Ki
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Self::Ki => "KI",
            Self::Erzwungen => "erzwungen",
        }
    }
}

pub fn anlass_text(anlass: Anlass) -> &'static str {
    match anlass {
        Anlass::Umschichtung => "KI-Vorschlag",
        Anlass::AusstiegPositivliste => "Positivliste",
        Anlass::Abbau => "Abbau",
        Anlass::Notbremse => "Notbremse",
        Anlass::Stopp => "Stopp",
        Anlass::FristOhneBegruendung => "Frist",
    }
}

pub use depot_sim::ausfuehrung::{Quote, Stufe};

/// Topf, Woche des Versands, Richtung („Kauf“/„Verkauf“) und Gruppe.
pub type QuotenSchluessel<T> = (TopfId, Datum, &'static str, T);

pub fn richtung_text(richtung: Richtung) -> &'static str {
    match richtung {
        Richtung::Kauf => "Kauf",
        Richtung::Verkauf => "Verkauf",
    }
}

fn versandwoche(z: &OrderZeile) -> Option<Datum> {
    wochenbeginn(z.versand?.frankfurt_tag()?)
}

/// Füllquote je Topf, Woche des Versands, Richtung und Anlass.
pub fn quote_je_anlass(zeilen: &[OrderZeile]) -> BTreeMap<QuotenSchluessel<&'static str>, Quote> {
    let mut aus = BTreeMap::<_, Quote>::new();
    for z in zeilen {
        if let Some(woche) = versandwoche(z) {
            aus.entry((
                z.topf,
                woche,
                richtung_text(z.richtung),
                anlass_text(z.anlass),
            ))
            .or_default()
            .zaehle(z.zustand);
        }
    }
    aus.retain(|_, q| q.gefuellt > 0 || q.verfallen > 0);
    aus
}

/// Füllquote je Topf, Woche des Versands, Richtung und Stufe des Limitabstands.
pub fn quote_je_stufe(zeilen: &[OrderZeile]) -> BTreeMap<QuotenSchluessel<Stufe>, Quote> {
    let mut aus = BTreeMap::<_, Quote>::new();
    for z in zeilen {
        if let Some(woche) = versandwoche(z) {
            aus.entry((
                z.topf,
                woche,
                richtung_text(z.richtung),
                Stufe::von(z.limit_bp),
            ))
            .or_default()
            .zaehle(z.zustand);
        }
    }
    aus.retain(|_, q| q.gefuellt > 0 || q.verfallen > 0);
    aus
}

/// Ein gefüllter Verkauf und der nächste gefüllte Kauf im selben Topf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verzug {
    pub topf: TopfId,
    pub verkauf: u64,
    pub verkauft_am: Datum,
    pub herkunft: Herkunft,
    /// Nummer und Tag des nächsten gefüllten Kaufs, falls es ihn gibt.
    pub kauf: Option<(u64, Datum)>,
    /// Handelstage nach dem Verkauf bis einschließlich zum Kauf, sonst bis `heute`.
    pub handelstage: u32,
}

fn handelstage_zwischen(von: Datum, bis: Datum) -> u32 {
    let mut n = 0_u32;
    let mut tag = von;
    while tag < bis {
        let Some(naechster) = tag.plus_tage(1) else {
            break;
        };
        tag = naechster;
        if depot_kern::liste::handelstag(tag) {
            n = n.saturating_add(1);
        }
    }
    n
}

/// Verzug vom Füllen jedes Verkaufs bis zum nächsten gefüllten Kauf im selben Topf. Ein Kauf
/// am selben Tag zählt mit 0 Handelstagen.
pub fn verzuege(zeilen: &[OrderZeile], heute: Datum) -> Vec<Verzug> {
    let gefuellt = |richtung| {
        zeilen.iter().filter_map(move |z| {
            (z.richtung == richtung && z.zustand == AuftragsZustand::Gefuellt)
                .then_some(())
                .and(z.ausgefuehrt_am.map(|tag| (z, tag)))
        })
    };
    let mut aus: Vec<_> = gefuellt(Richtung::Verkauf)
        .map(|(v, verkauft_am)| {
            let kauf = gefuellt(Richtung::Kauf)
                .filter(|(k, tag)| k.topf == v.topf && *tag >= verkauft_am)
                .min_by_key(|(k, tag)| (*tag, k.nr))
                .map(|(k, tag)| (k.nr, tag));
            Verzug {
                topf: v.topf,
                verkauf: v.nr,
                verkauft_am,
                herkunft: Herkunft::von(v.anlass),
                kauf,
                handelstage: handelstage_zwischen(verkauft_am, kauf.map_or(heute, |(_, t)| t)),
            }
        })
        .collect();
    aus.sort_by_key(|v| (v.topf, v.verkauft_am, v.verkauf));
    aus
}

/// Kurzer Abschnitt für die Depotseite: die letzten vier Wochen je Topf.
pub fn html(stand: &Stand, punkte: &[Tagespunkt], heute: Datum, text: &mut String) {
    use std::fmt::Write as _;
    let Some(ab) = heute.plus_tage(-27).and_then(wochenbeginn) else {
        return;
    };
    let buch = crate::ausfuehrung::ausfuehrungsbuch(stand);
    let tage: Vec<_> = bargeldtage(stand, punkte)
        .into_iter()
        .filter(|t| t.tag >= ab)
        .collect();
    let bargeld = bargeld_je_woche(&tage);
    if bargeld.is_empty() {
        return;
    }
    let quoten = quote_je_anlass(&buch);
    let verzug = verzuege(&buch, heute);
    text.push_str("<h3>Leerlauf der letzten vier Wochen</h3><table><thead><tr><th>Topf</th><th>Woche ab</th><th>Bargeldquote</th><th>Euro-Tage in Bargeld</th><th>Käufe gefüllt</th><th>Verkäufe gefüllt</th><th>Ø Handelstage Verkauf bis Kauf</th></tr></thead><tbody>");
    for ((topf, woche), w) in &bargeld {
        let summe = |richtung: &str| {
            quoten
                .iter()
                .filter(|((t, wo, r, _), _)| t == topf && wo == woche && *r == richtung)
                .fold(Quote::default(), |a, (_, q)| Quote {
                    gefuellt: a.gefuellt.saturating_add(q.gefuellt),
                    verfallen: a.verfallen.saturating_add(q.verfallen),
                })
        };
        let quote = |q: Quote| {
            q.prozent().map_or_else(
                || "–".into(),
                |p| {
                    format!(
                        "{p} % ({}/{})",
                        q.gefuellt,
                        q.gefuellt.saturating_add(q.verfallen)
                    )
                },
            )
        };
        let verzug: Vec<_> = verzug
            .iter()
            .filter(|v| v.topf == *topf && wochenbeginn(v.verkauft_am) == Some(*woche))
            .map(|v| v.handelstage)
            .collect();
        let mittel = u32::try_from(verzug.len())
            .ok()
            .and_then(|n| verzug.iter().sum::<u32>().checked_div(n))
            .map_or_else(|| "–".into(), |t| t.to_string());
        let _ = write!(
            text,
            "<tr><td>{topf:?}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{mittel}</td></tr>",
            crate::datum_text(*woche),
            w.quote_bp.map_or_else(|| "–".into(), crate::html::prozent),
            w.euro_tage(),
            quote(summe("Kauf")),
            quote(summe("Verkauf")),
        );
    }
    text.push_str("</tbody></table><p class=\"leise\">Nur Messung (#311 M0): Einzelheiten je Anlass und Limitabstand stehen im Analyse-Export.</p>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_kern::{Isin, Preis, Zeitpunkt};

    fn tag(t: u8) -> Result<Datum, String> {
        Datum::neu(2026, 11, t).ok_or_else(|| format!("Tag {t}"))
    }

    fn order(
        nr: u64,
        richtung: Richtung,
        anlass: Anlass,
        zustand: AuftragsZustand,
        versand: u8,
        limit_bp: i64,
    ) -> Result<OrderZeile, String> {
        let versand_tag = tag(versand)?;
        Ok(OrderZeile {
            nr,
            topf: TopfId::A,
            isin: Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?,
            richtung,
            stueck: 1,
            versand: Zeitpunkt::frankfurt(versand_tag, 7, 30, 0),
            frist: None,
            zustand,
            bezug: Preis::aus_cent(Cent(10_000)),
            limit: Cent(10_000),
            limit_bp: Some(limit_bp),
            fuellpreis: None,
            fuellpreis_bp: None,
            modellspread_bp: None,
            schluss: None,
            schluss_bp: None,
            anlass,
            ausgefuehrt_am: (zustand == AuftragsZustand::Gefuellt).then_some(versand_tag),
        })
    }

    #[test]
    fn wochenbeginn_ist_montag() -> Result<(), String> {
        // 16.11.2026 ist ein Montag.
        assert_eq!(wochenbeginn(tag(16)?), Some(tag(16)?));
        assert_eq!(wochenbeginn(tag(20)?), Some(tag(16)?));
        assert_eq!(wochenbeginn(tag(22)?), Some(tag(16)?));
        Ok(())
    }

    #[test]
    fn euro_tage_bargeld() -> Result<(), String> {
        let tage: Vec<_> = [16, 17, 18]
            .into_iter()
            .map(|t| {
                Ok(BargeldTag {
                    tag: tag(t)?,
                    topf: TopfId::A,
                    bargeld: Cent(1_000_000),
                    wert: Some(Cent(4_000_000)),
                })
            })
            .collect::<Result<_, String>>()?;
        let wochen = bargeld_je_woche(&tage);
        let w = wochen.get(&(TopfId::A, tag(16)?)).ok_or("Woche")?;
        assert_eq!(w.handelstage, 3);
        assert_eq!(w.euro_tage(), 30_000);
        assert_eq!(w.quote_bp, Some(2_500));
        Ok(())
    }

    #[test]
    fn ausfuehrungsquote_kauf_50() -> Result<(), String> {
        let zeilen = [
            order(
                1,
                Richtung::Kauf,
                Anlass::Umschichtung,
                AuftragsZustand::Gefuellt,
                16,
                100,
            )?,
            order(
                2,
                Richtung::Kauf,
                Anlass::Umschichtung,
                AuftragsZustand::Verfallen,
                17,
                100,
            )?,
            order(
                3,
                Richtung::Kauf,
                Anlass::Umschichtung,
                AuftragsZustand::Gesendet,
                18,
                100,
            )?,
        ];
        let q = quote_je_anlass(&zeilen);
        let kauf = q
            .get(&(TopfId::A, tag(16)?, "Kauf", "KI-Vorschlag"))
            .ok_or("Quote")?;
        assert_eq!((kauf.gefuellt, kauf.verfallen), (1, 1));
        assert_eq!(kauf.prozent(), Some(50));
        Ok(())
    }

    #[test]
    fn quote_je_abstand() -> Result<(), String> {
        let zeilen = [
            order(
                1,
                Richtung::Verkauf,
                Anlass::Stopp,
                AuftragsZustand::Gefuellt,
                16,
                -100,
            )?,
            order(
                2,
                Richtung::Verkauf,
                Anlass::Stopp,
                AuftragsZustand::Gefuellt,
                17,
                -300,
            )?,
            order(
                3,
                Richtung::Verkauf,
                Anlass::Stopp,
                AuftragsZustand::Verfallen,
                17,
                -300,
            )?,
        ];
        let q = quote_je_stufe(&zeilen);
        let woche = tag(16)?;
        let hole = |stufe| q.get(&(TopfId::A, woche, "Verkauf", stufe)).copied();
        assert_eq!(
            hole(Stufe::Bis100),
            Some(Quote {
                gefuellt: 1,
                verfallen: 0
            })
        );
        assert_eq!(
            hole(Stufe::Bis300),
            Some(Quote {
                gefuellt: 1,
                verfallen: 1
            })
        );
        assert_eq!(hole(Stufe::Bis200), None);
        Ok(())
    }

    #[test]
    fn verzug_nach_stopp() -> Result<(), String> {
        // Stoppverkauf Montag 16.11., nächster Kauf Donnerstag 19.11.: 3 Handelstage.
        let zeilen = [
            order(
                1,
                Richtung::Verkauf,
                Anlass::Stopp,
                AuftragsZustand::Gefuellt,
                16,
                -300,
            )?,
            order(
                2,
                Richtung::Kauf,
                Anlass::Umschichtung,
                AuftragsZustand::Verfallen,
                17,
                100,
            )?,
            order(
                3,
                Richtung::Kauf,
                Anlass::Umschichtung,
                AuftragsZustand::Gefuellt,
                19,
                100,
            )?,
            order(
                4,
                Richtung::Verkauf,
                Anlass::Umschichtung,
                AuftragsZustand::Gefuellt,
                20,
                -100,
            )?,
        ];
        let v = verzuege(&zeilen, tag(24)?);
        assert_eq!(v.len(), 2);
        let erster = v.first().ok_or("Verzug")?;
        assert_eq!(erster.herkunft, Herkunft::Erzwungen);
        assert_eq!(erster.kauf, Some((3, tag(19)?)));
        assert_eq!(erster.handelstage, 3);
        // KI-Verkauf Freitag 20.11. ohne Folgekauf: bis Dienstag 24.11. zwei Handelstage.
        let zweiter = v.get(1).ok_or("Verzug")?;
        assert_eq!(zweiter.herkunft, Herkunft::Ki);
        assert_eq!(zweiter.kauf, None);
        assert_eq!(zweiter.handelstage, 2);
        Ok(())
    }

    #[test]
    fn depotseite_zeigt_bargeld_der_woche() -> Result<(), String> {
        use depot_journal::Journal;
        use depot_kern::Topf;
        let dir = std::env::temp_dir().join(format!("depot_leerlauf_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let pfad = dir.join("journal.txt");
        Journal::einrichten(&pfad).map_err(|e| format!("{e:?}"))?;
        let mut j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
        let topf = Topf::neu(TopfId::A, Cent(1_000_000)).map_err(|e| format!("{e:?}"))?;
        j.setze_topf(&topf).map_err(|e| format!("{e:?}"))?;
        let abschluss = "0".repeat(64);
        j.schliesse_ab(tag(16)?, Cent(1_000_000), &abschluss)
            .map_err(|e| format!("{e:?}"))?;
        let mut text = String::new();
        html(j.stand(), &[], tag(17)?, &mut text);
        assert!(text.contains("Leerlauf der letzten vier Wochen"), "{text}");
        // 10.000 € an einem Handelstag, ohne Bewertung keine Quote.
        assert!(text.contains("<td>10000</td>"), "{text}");
        let mut leer = String::new();
        html(&Stand::default(), &[], tag(17)?, &mut leer);
        assert!(leer.is_empty());
        drop(j);
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
