//! Grenzen und reine Pruefung. Kein Netz, keine Dateien, keine Uhr.
//!
//! Grenzen, Deckel und Preise entstehen nur hier, aus den Werten der Festschreibung (R19). Ihre
//! Felder sind privat, es gibt keine Setzer. Frei einstellbare Werte gibt es nur in den Tests
//! dieses Moduls.

use crate::tagesrahmen;
use depot_kern::modelle::{Modell, Modellwahl};
use depot_kern::{BeraterId, Cent};

pub use depot_kern::Datum;

/// Listenpreise in Cent je Million Token, beide positiv (R20). Dollar werden als Euro
/// gezaehlt; das ueberschaetzt die Kosten, solange der Euro mehr wert ist als der Dollar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preise {
    eingabe: i64,
    ausgabe: i64,
    cache_schreiben: i64,
    cache_lesen: i64,
}

impl Preise {
    /// `None`, wenn ein Preis nicht positiv ist.
    pub(crate) fn neu(
        eingabe: i64,
        ausgabe: i64,
        cache_schreiben: i64,
        cache_lesen: i64,
    ) -> Option<Preise> {
        (ausgabe > 0 && cache_lesen > 0 && eingabe >= cache_lesen && cache_schreiben >= eingabe)
            .then_some(Self {
                eingabe,
                ausgabe,
                cache_schreiben,
                cache_lesen,
            })
    }

    pub fn cent_je_mio_ein(&self) -> i64 {
        self.eingabe
    }

    pub fn cent_je_mio_aus(&self) -> i64 {
        self.ausgabe
    }

    /// Was so viele Token zu diesen Preisen kosten, aufgerundet auf ganze Cent (R21).
    pub fn kosten(&self, token_ein: u32, token_aus: u32) -> Option<Cent> {
        let ein = i64::from(token_ein).checked_mul(self.eingabe)?;
        let aus = i64::from(token_aus).checked_mul(self.ausgabe)?;
        let cent = ein
            .checked_add(aus)?
            .checked_add(999_999)?
            .checked_div(1_000_000)?;
        Some(Cent(cent))
    }
}

/// Deckel fuer einen einzelnen Aufruf. Der KI-Client muss `max_token_aus` als
/// Antwortgrenze (Denken eingeschlossen) an den Anbieter senden, darf keine Werkzeuge des
/// Anbieters freigeben (keine Websuche: deren Ergebnisse liessen sich vorher nicht begrenzen)
/// und muss jede Anfrage ueber `max_token_ein` Bytes ablehnen. Ein Token umfasst mindestens
/// ein Byte; damit sind die Hoechstkosten vor dem Aufruf eine gesicherte Obergrenze.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KiDeckel {
    max_token_ein: u32,
    max_token_aus: u32,
    preise: Preise,
}

impl KiDeckel {
    /// Papierhandel ab 05.11.2026, KI-Kosten hoechstens 20 € im Monat je KI (Wolfgang),
    /// Denktiefe mittel. Nachrichten bringt der Sammler, die KIs suchen nicht selbst.
    /// - Dachs = Claude Sonnet 5.5: 40.000 Token rein, 16.000 raus, 2 $ / 10 $ je Mio.
    /// - Maus = GPT-6.1 Terra: 40.000 Token rein, 16.000 raus, 1 $ / 5 $ je Mio.
    pub fn standard(berater: BeraterId) -> KiDeckel {
        Self::fuer_modell(Modellwahl::standard(berater).modell())
    }

    pub fn fuer_modell(modell: Modell) -> KiDeckel {
        let (ein, aus, schreiben, lesen) = match modell {
            // Anthropic: 5 Minuten. OpenAI: 30 Minuten. Geprüft am 21.11.2026.
            // https://platform.claude.com/docs/en/about-claude/pricing
            // https://developers.openai.com/api/docs/models
            Modell::Sonnet55 | Modell::Sol62 => (200, 1_000, 250, 10),
            Modell::Opus55 => (400, 2_000, 500, 20),
            Modell::Terra61 => (100, 500, 125, 5),
        };
        KiDeckel {
            max_token_ein: 40_000,
            max_token_aus: 16_000,
            // Ein ungueltiger Festwert sperrt jeden Aufruf: Mit Hoechstpreisen laesst sich keine
            // Reservierung berechnen, `pruefe` meldet `Rechenfehler` (fail-closed).
            preise: Preise::neu(ein, aus, schreiben, lesen).unwrap_or(Preise {
                eingabe: i64::MAX,
                ausgabe: i64::MAX,
                cache_schreiben: i64::MAX,
                cache_lesen: i64::MAX,
            }),
        }
    }

    /// Kurze Vorbereitung/Abstimmung: gleiche Preise und Eingabegrenze, 2.000 Ausgabetoken.
    pub fn kurz(berater: BeraterId) -> KiDeckel {
        Self::kurz_fuer_modell(Modellwahl::standard(berater).modell())
    }

    pub fn kurz_fuer_modell(modell: Modell) -> KiDeckel {
        Self {
            max_token_aus: KiGrenzen::standard().kurz_max_token_aus(),
            ..Self::fuer_modell(modell)
        }
    }

    pub fn max_token_ein(&self) -> u32 {
        self.max_token_ein
    }

    pub fn max_token_aus(&self) -> u32 {
        self.max_token_aus
    }

    pub fn preise(&self) -> Preise {
        self.preise
    }

    /// Was ein Aufruf schlimmstenfalls kostet, wenn alle Eingabetoken geschrieben werden.
    pub fn hoechstkosten(&self) -> Option<Cent> {
        let ein = u64::from(self.max_token_ein);
        self.kosten_mit_cache(ein, u64::from(self.max_token_aus), ein, 0)
    }

    /// Was ein Aufruf ohne Cache gekostet hat, aufgerundet auf ganze Cent.
    pub fn kosten_ist(&self, token_ein: u64, token_aus: u64) -> Option<Cent> {
        self.kosten_mit_cache(token_ein, token_aus, 0, 0)
    }

    /// Die Cache-Mengen sind disjunkte Anteile der gesamten Eingabe.
    pub fn kosten_mit_cache(
        &self,
        token_ein: u64,
        token_aus: u64,
        cache_schreiben: u64,
        cache_lesen: u64,
    ) -> Option<Cent> {
        let voll = token_ein
            .checked_sub(cache_schreiben)?
            .checked_sub(cache_lesen)?;
        let anteile = [
            (voll, self.preise.eingabe),
            (token_aus, self.preise.ausgabe),
            (cache_schreiben, self.preise.cache_schreiben),
            (cache_lesen, self.preise.cache_lesen),
        ];
        let kosten = anteile
            .into_iter()
            .try_fold(0_i64, |summe, (menge, preis)| {
                summe.checked_add(i64::try_from(menge).ok()?.checked_mul(preis)?)
            })?;
        Some(Cent(kosten.checked_add(999_999)?.checked_div(1_000_000)?))
    }
}

/// Grenzen je KI. Sie gelten fuer Dachs und Maus getrennt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KiGrenzen {
    aufrufe_je_zyklus: u32,
    zyklen_je_tag: u32,
    aufrufe_je_monat: u32,
    kosten_je_monat: Cent,
    kosten_je_jahr: Cent,
    kurz_max_token_aus: u32,
}

impl KiGrenzen {
    /// Je KI höchstens 30 Aufrufe am Tag und 690 im Monat als Schleifenschutz.
    /// Verbindliche Eurogrenzen bleiben 20 € im Monat und 240 € im Jahr.
    pub fn standard() -> KiGrenzen {
        KiGrenzen {
            aufrufe_je_zyklus: 30,
            zyklen_je_tag: 1,
            aufrufe_je_monat: 690,
            kosten_je_monat: Cent(2_000),
            kosten_je_jahr: Cent(24_000),
            kurz_max_token_aus: 2_000,
        }
    }

    pub fn kurz_max_token_aus(&self) -> u32 {
        self.kurz_max_token_aus
    }

    pub fn aufrufe_je_zyklus(&self) -> u32 {
        self.aufrufe_je_zyklus
    }

    pub fn zyklen_je_tag(&self) -> u32 {
        self.zyklen_je_tag
    }

    pub fn aufrufe_je_monat(&self) -> u32 {
        self.aufrufe_je_monat
    }

    pub fn kosten_je_monat(&self) -> Cent {
        self.kosten_je_monat
    }

    pub fn kosten_je_jahr(&self) -> Cent {
        self.kosten_je_jahr
    }
}

/// Eine gebuchte Erlaubnis (Hoechstkosten, nicht tatsaechliche Kosten).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Buchung {
    pub berater: BeraterId,
    pub datum: Datum,
    pub zyklus: u32,
    pub reserviert: Cent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sperre {
    ZyklusVoll,
    TagVoll,
    MonatVoll,
    MonatsbudgetErschoepft,
    JahresbudgetErschoepft,
    /// Das Datum liegt vor der letzten Buchung: Uhr verstellt.
    UhrRueckwaerts,
    /// Ein frueherer Zyklus soll erneut fragen.
    ZyklusRueckwaerts,
    Rechenfehler,
    TagesrahmenErschoepft,
    ErstmeinungReserviert,
    /// Kassenbuch oder Journal unstimmig, nicht schreibbar oder nach einem Fehler gesperrt.
    /// Halt bis zum geprueften Wiederanlauf (R25, R29).
    Kassenbuch(String),
}

fn anzahl(grenze: u32) -> usize {
    usize::try_from(grenze).unwrap_or(0)
}

fn summe<'a>(mut buchungen: impl Iterator<Item = &'a Buchung>) -> Option<Cent> {
    buchungen.try_fold(Cent::NULL, |s, b| s.checked_add(b.reserviert))
}

fn gleicher_monat(a: Datum, b: Datum) -> bool {
    a.jahr() == b.jahr() && a.monat() == b.monat()
}

/// Prueft, ob ein weiterer Aufruf erlaubt ist, und liefert die zu reservierenden Kosten.
///
/// # Errors
/// Jede verletzte Grenze ergibt eine `Sperre`.
pub fn pruefe(
    grenzen: &KiGrenzen,
    deckel: &KiDeckel,
    buchungen: &[Buchung],
    berater: BeraterId,
    datum: Datum,
    zyklus: u32,
) -> Result<Cent, Sperre> {
    pruefe_mit_tagesanteil(grenzen, deckel, buchungen, berater, datum, zyklus, true)
}

/// Eine manuell bestätigte Simulation verwendet den verbleibenden Monatsrahmen.
/// Die harten Aufruf-, Monats- und Jahresgrenzen gelten auch hier.
pub fn pruefe_simulation(
    grenzen: &KiGrenzen,
    deckel: &KiDeckel,
    buchungen: &[Buchung],
    berater: BeraterId,
    datum: Datum,
    zyklus: u32,
) -> Result<Cent, Sperre> {
    pruefe_mit_tagesanteil(grenzen, deckel, buchungen, berater, datum, zyklus, false)
}

fn pruefe_mit_tagesanteil(
    grenzen: &KiGrenzen,
    deckel: &KiDeckel,
    buchungen: &[Buchung],
    berater: BeraterId,
    datum: Datum,
    zyklus: u32,
    tagesanteil: bool,
) -> Result<Cent, Sperre> {
    let eigene: Vec<&Buchung> = buchungen.iter().filter(|b| b.berater == berater).collect();
    if let Some(letzte) = eigene.last() {
        if datum < letzte.datum {
            return Err(Sperre::UhrRueckwaerts);
        }
        if zyklus < letzte.zyklus {
            return Err(Sperre::ZyklusRueckwaerts);
        }
    }
    let kosten = deckel.hoechstkosten().ok_or(Sperre::Rechenfehler)?;

    if eigene.iter().filter(|b| b.zyklus == zyklus).count() >= anzahl(grenzen.aufrufe_je_zyklus) {
        return Err(Sperre::ZyklusVoll);
    }
    let mut zyklen_heute: Vec<u32> = eigene
        .iter()
        .filter(|b| b.datum == datum)
        .map(|b| b.zyklus)
        .collect();
    zyklen_heute.push(zyklus);
    zyklen_heute.sort_unstable();
    zyklen_heute.dedup();
    if zyklen_heute.len() > anzahl(grenzen.zyklen_je_tag) {
        return Err(Sperre::TagVoll);
    }

    let im_monat: Vec<&Buchung> = eigene
        .iter()
        .copied()
        .filter(|b| gleicher_monat(b.datum, datum))
        .collect();
    if im_monat.len() >= anzahl(grenzen.aufrufe_je_monat) {
        return Err(Sperre::MonatVoll);
    }
    let monat = summe(im_monat.into_iter())
        .and_then(|s| s.checked_add(kosten))
        .ok_or(Sperre::Rechenfehler)?;
    if monat > grenzen.kosten_je_monat {
        return Err(Sperre::MonatsbudgetErschoepft);
    }
    let jahr = summe(
        eigene
            .into_iter()
            .filter(|b| b.datum.jahr() == datum.jahr()),
    )
    .and_then(|s| s.checked_add(kosten))
    .ok_or(Sperre::Rechenfehler)?;
    if jahr > grenzen.kosten_je_jahr {
        return Err(Sperre::JahresbudgetErschoepft);
    }
    if tagesanteil && kosten > tagesrahmen(grenzen, buchungen, berater, datum)? {
        return Err(Sperre::TagesrahmenErschoepft);
    }
    Ok(kosten)
}

/// Der Abdruck der festgeschriebenen KI-Grenzen und beider Deckel: SHA-256 ihrer Werte. Vor
/// der ersten Buchung unter neuen Grenzen steht er im Journal (Uebergangsregel).
pub fn grenzabdruck() -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    let text = format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}",
        KiGrenzen::standard(),
        KiDeckel::standard(BeraterId::Dachs),
        KiDeckel::standard(BeraterId::Maus),
        KiDeckel::kurz(BeraterId::Dachs),
        KiDeckel::kurz(BeraterId::Maus)
    );
    Sha256::digest(text.as_bytes())
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(t: u8) -> Result<Datum, String> {
        Datum::neu(2026, 11, t).ok_or_else(|| format!("Datum {t}"))
    }

    const MAUS: BeraterId = BeraterId::Maus;

    #[test]
    fn standardwerte_sind_gueltig() {
        for b in [BeraterId::Dachs, MAUS] {
            let d = KiDeckel::standard(b);
            let p = d.preise;
            assert_eq!(
                Preise::neu(p.eingabe, p.ausgabe, p.cache_schreiben, p.cache_lesen),
                Some(p),
                "{b:?}"
            );
            assert!(d.max_token_ein > 0 && d.max_token_aus > 0, "{b:?}");
            assert!(d.hoechstkosten().is_some_and(|c| c.0 > 0), "{b:?}");
        }
    }

    /// 1·K02: Ein Preis, der nicht positiv ist, entsteht nicht.
    #[test]
    fn preise_sind_positiv() {
        assert!(Preise::neu(-1_000_000, 2_000, 1, 1).is_none());
        assert!(Preise::neu(400, -1, 500, 20).is_none());
        assert!(Preise::neu(0, 1, 1, 1).is_none());
        assert!(Preise::neu(1, 1, 1, 1).is_some());
    }

    /// Ein ungueltiger Festwert sperrt: Mit den Ersatzpreisen gibt es keine Reservierung.
    #[test]
    fn ersatzpreise_sperren() -> Result<(), String> {
        let mut d = KiDeckel::standard(MAUS);
        d.preise = Preise {
            eingabe: i64::MAX,
            ausgabe: i64::MAX,
            cache_schreiben: i64::MAX,
            cache_lesen: i64::MAX,
        };
        assert_eq!(
            pruefe(&KiGrenzen::standard(), &d, &[], MAUS, tag(5)?, 1),
            Err(Sperre::Rechenfehler)
        );
        Ok(())
    }

    /// Ein teurerer Deckel reisst das Monatsbudget, bevor gefragt wird; ein kleines
    /// Jahresbudget sperrt nach dem ersten Aufruf. Nur hier, im Test, mit eigenen Werten.
    #[test]
    fn budget_vor_dem_aufruf_geprueft() -> Result<(), String> {
        let g = KiGrenzen::standard();
        let mut d = KiDeckel::standard(MAUS);
        d.max_token_aus = 5_000_000; // 0,04 € + 25,00 € > 20 €
        assert_eq!(
            pruefe(&g, &d, &[], MAUS, tag(5)?, 1),
            Err(Sperre::MonatsbudgetErschoepft)
        );
        let mut klein = KiGrenzen::standard();
        klein.kosten_je_jahr = Cent(20);
        let d = KiDeckel::standard(MAUS);
        let reserviert =
            pruefe(&klein, &d, &[], MAUS, tag(5)?, 1).map_err(|e| format!("erster: {e:?}"))?;
        let b = Buchung {
            berater: MAUS,
            datum: tag(5)?,
            zyklus: 1,
            reserviert,
        };
        assert_eq!(
            pruefe(&klein, &d, &[b], MAUS, tag(5)?, 1),
            Err(Sperre::JahresbudgetErschoepft)
        );
        Ok(())
    }

    /// Anzahl und Eurobudget werden unabhängig geprüft: je KI höchstens
    /// 20 € im Monat (Wolfgang, 05.11.2026).
    #[test]
    fn zwanzig_euro_im_monat() {
        let g = KiGrenzen::standard();
        for b in [BeraterId::Dachs, MAUS] {
            let h = KiDeckel::standard(b)
                .hoechstkosten()
                .map_or(i64::MAX, |c| c.0);
            assert!(h <= g.kosten_je_monat.0, "{b:?}: {h}");
        }
        assert!(g.kosten_je_monat.0 <= 2_000);
        assert_eq!(
            KiDeckel::standard(BeraterId::Dachs).hoechstkosten(),
            Some(Cent(26))
        );
        assert_eq!(KiDeckel::standard(MAUS).hoechstkosten(), Some(Cent(13)));
    }

    /// Tatsaechliche Kosten: aufgerundet.
    #[test]
    fn kosten_ist_rundet_auf() {
        let d = KiDeckel::standard(BeraterId::Dachs);
        assert_eq!(d.kosten_ist(0, 0), Some(Cent(0)));
        assert_eq!(d.kosten_ist(1, 0), Some(Cent(1)));
        assert_eq!(d.kosten_ist(5_000, 1), Some(Cent(2)));
        assert_eq!(d.kosten_ist(u64::MAX, u64::MAX), None);
    }
}
