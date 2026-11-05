//! Grenzen und reine Pruefung. Kein Netz, keine Dateien, keine Uhr.
//!
//! Grenzen, Deckel und Preise entstehen nur hier, aus den Werten der Festschreibung (R19). Ihre
//! Felder sind privat, es gibt keine Setzer. Frei einstellbare Werte gibt es nur in den Tests
//! dieses Moduls.

use depot_kern::{BeraterId, Cent};

pub use depot_kern::Datum;

/// Listenpreise in Cent je Million Token, beide positiv (R20). Dollar werden als Euro
/// gezaehlt; das ueberschaetzt die Kosten, solange der Euro mehr wert ist als der Dollar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preise {
    cent_je_mio_ein: i64,
    cent_je_mio_aus: i64,
}

impl Preise {
    /// `None`, wenn ein Preis nicht positiv ist.
    pub(crate) fn neu(cent_je_mio_ein: i64, cent_je_mio_aus: i64) -> Option<Preise> {
        (cent_je_mio_ein > 0 && cent_je_mio_aus > 0).then_some(Preise {
            cent_je_mio_ein,
            cent_je_mio_aus,
        })
    }

    pub fn cent_je_mio_ein(&self) -> i64 {
        self.cent_je_mio_ein
    }

    pub fn cent_je_mio_aus(&self) -> i64 {
        self.cent_je_mio_aus
    }

    /// Was so viele Token zu diesen Preisen kosten, aufgerundet auf ganze Cent (R21).
    pub fn kosten(&self, token_ein: u32, token_aus: u32) -> Option<Cent> {
        let ein = i64::from(token_ein).checked_mul(self.cent_je_mio_ein)?;
        let aus = i64::from(token_aus).checked_mul(self.cent_je_mio_aus)?;
        let cent = ein
            .checked_add(aus)?
            .checked_add(999_999)?
            .checked_div(1_000_000)?;
        Some(Cent(cent))
    }
}

/// Deckel fuer einen einzelnen Aufruf. Der KI-Client muss `max_token_aus` als
/// Antwortgrenze und `max_suchen` als Grenze der Websuchen an den Anbieter senden und Eingaben
/// ueber `max_token_ein` ablehnen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KiDeckel {
    max_token_ein: u32,
    max_token_aus: u32,
    max_suchen: u32,
    preise: Preise,
    /// Preis einer Websuche in Cent je 1.000 Suchen.
    cent_je_tausend_suchen: i64,
}

impl KiDeckel {
    /// Papierhandel ab 05.11.2026, KI-Kosten hoechstens 20 € im Monat fuer beide zusammen:
    /// - Dachs = Claude Sonnet 5.5: 50.000 Token rein, 4.000 raus, 2 $ / 10 $ je Mio.
    /// - Maus = GPT-5.4: 50.000 Token rein, 4.000 raus, 2,50 $ / 15 $ je Mio.
    /// - Beide: hoechstens 2 Websuchen je Aufruf zu 10 $ je 1.000 Suchen.
    ///
    /// Die Eingabe-Grenze schliesst die Suchergebnisse ein, die der Anbieter selbst einfuegt.
    /// Sie laesst sich vorher nicht pruefen; der Client rechnet nach jedem Aufruf die
    /// tatsaechlichen Kosten (`kosten_ist`) und sperrt die KI, wenn sie die Reservierung
    /// ueberschreiten.
    pub fn standard(berater: BeraterId) -> KiDeckel {
        let (max_token_ein, max_token_aus, ein, aus) = match berater {
            BeraterId::Dachs => (50_000, 4_000, 200, 1_000),
            BeraterId::Maus => (50_000, 4_000, 250, 1_500),
        };
        KiDeckel {
            max_token_ein,
            max_token_aus,
            max_suchen: 2,
            // Ein ungueltiger Festwert sperrt jeden Aufruf: Mit Hoechstpreisen laesst sich keine
            // Reservierung berechnen, `pruefe` meldet `Rechenfehler` (fail-closed).
            preise: Preise::neu(ein, aus).unwrap_or(Preise {
                cent_je_mio_ein: i64::MAX,
                cent_je_mio_aus: i64::MAX,
            }),
            cent_je_tausend_suchen: 1_000,
        }
    }

    pub fn max_token_ein(&self) -> u32 {
        self.max_token_ein
    }

    pub fn max_token_aus(&self) -> u32 {
        self.max_token_aus
    }

    pub fn max_suchen(&self) -> u32 {
        self.max_suchen
    }

    pub fn preise(&self) -> Preise {
        self.preise
    }

    pub fn cent_je_tausend_suchen(&self) -> i64 {
        self.cent_je_tausend_suchen
    }

    /// Was ein Aufruf schlimmstenfalls kostet, aufgerundet auf ganze Cent: alle Token und
    /// alle erlaubten Suchen.
    pub fn hoechstkosten(&self) -> Option<Cent> {
        self.kosten_ist(
            u64::from(self.max_token_ein),
            u64::from(self.max_token_aus),
            u64::from(self.max_suchen),
        )
    }

    /// Was ein Aufruf nach Angabe des Anbieters gekostet hat, aufgerundet auf ganze Cent.
    /// Token und Suchen werden zusammen gerundet; so ist das Ergebnis nie kleiner als die
    /// Summe der exakten Kosten.
    pub fn kosten_ist(&self, token_ein: u64, token_aus: u64, suchen: u64) -> Option<Cent> {
        if self.cent_je_tausend_suchen < 0 {
            return None;
        }
        let ein = i64::try_from(token_ein)
            .ok()?
            .checked_mul(self.preise.cent_je_mio_ein)?;
        let aus = i64::try_from(token_aus)
            .ok()?
            .checked_mul(self.preise.cent_je_mio_aus)?;
        // Suchen in Cent je Million: je 1.000 Suchen mal 1.000.
        let such = i64::try_from(suchen)
            .ok()?
            .checked_mul(self.cent_je_tausend_suchen)?
            .checked_mul(1_000)?;
        let mio = ein.checked_add(aus)?.checked_add(such)?;
        Some(Cent(mio.checked_add(999_999)?.checked_div(1_000_000)?))
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
}

impl KiGrenzen {
    /// Papierhandel ab 05.11.2026, je KI: 2 Aufrufe je Zyklus (eigener Vorschlag und
    /// Zweitmeinung zum anderen Topf), 1 Zyklus je Tag, 46 Aufrufe und 10 € je Monat,
    /// 120 € je Jahr. Zusammen hoechstens 20 € im Monat (Wolfgang, 05.11.2026). 46 Aufrufe
    /// reichen fuer 23 Handelstage; gebucht werden die Hoechstkosten.
    pub fn standard() -> KiGrenzen {
        KiGrenzen {
            aufrufe_je_zyklus: 2,
            zyklen_je_tag: 1,
            aufrufe_je_monat: 46,
            kosten_je_monat: Cent(1_000),
            kosten_je_jahr: Cent(12_000),
        }
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
    Ok(kosten)
}

/// Der Abdruck der festgeschriebenen KI-Grenzen und beider Deckel: SHA-256 ihrer Werte. Vor
/// der ersten Buchung unter neuen Grenzen steht er im Journal (Uebergangsregel).
pub fn grenzabdruck() -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    let text = format!(
        "{:?}|{:?}|{:?}",
        KiGrenzen::standard(),
        KiDeckel::standard(BeraterId::Dachs),
        KiDeckel::standard(BeraterId::Maus)
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
                Preise::neu(p.cent_je_mio_ein, p.cent_je_mio_aus),
                Some(p),
                "{b:?}"
            );
            assert!(d.max_token_ein > 0 && d.max_token_aus > 0, "{b:?}");
            assert!(d.max_suchen > 0 && d.cent_je_tausend_suchen > 0, "{b:?}");
            assert!(d.hoechstkosten().is_some_and(|c| c.0 > 0), "{b:?}");
        }
    }

    /// 1·K02: Ein Preis, der nicht positiv ist, entsteht nicht.
    #[test]
    fn preise_sind_positiv() {
        assert!(Preise::neu(-1_000_000, 2_000).is_none());
        assert!(Preise::neu(400, -1).is_none());
        assert!(Preise::neu(0, 1).is_none());
        assert!(Preise::neu(1, 1).is_some());
    }

    /// Ein ungueltiger Festwert sperrt: Mit den Ersatzpreisen gibt es keine Reservierung.
    #[test]
    fn ersatzpreise_sperren() -> Result<(), String> {
        let mut d = KiDeckel::standard(MAUS);
        d.preise = Preise {
            cent_je_mio_ein: i64::MAX,
            cent_je_mio_aus: i64::MAX,
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
        d.max_token_aus = 1_000_000; // 0,13 € + 15,00 € + 0,02 € > 10 €
        assert_eq!(
            pruefe(&g, &d, &[], MAUS, tag(5)?, 1),
            Err(Sperre::MonatsbudgetErschoepft)
        );
        let mut klein = KiGrenzen::standard();
        klein.kosten_je_jahr = Cent(30);
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

    /// Die Hoechstkosten beider Deckel passen 46-mal in das Monatsbudget: 20 € im Monat fuer
    /// beide zusammen (Wolfgang, 05.11.2026).
    #[test]
    fn zwanzig_euro_im_monat() {
        let g = KiGrenzen::standard();
        let mut summe = 0;
        for b in [BeraterId::Dachs, MAUS] {
            let h = KiDeckel::standard(b)
                .hoechstkosten()
                .map_or(i64::MAX, |c| c.0);
            let monat = h.saturating_mul(i64::from(g.aufrufe_je_monat));
            assert!(monat <= g.kosten_je_monat.0, "{b:?}: {monat}");
            summe += g.kosten_je_monat.0;
        }
        assert!(summe <= 2_000);
        assert_eq!(
            KiDeckel::standard(BeraterId::Dachs).hoechstkosten(),
            Some(Cent(16))
        );
        assert_eq!(KiDeckel::standard(MAUS).hoechstkosten(), Some(Cent(21)));
    }

    /// Tatsaechliche Kosten: Token und Suchen zusammen aufgerundet.
    #[test]
    fn kosten_ist_rundet_auf() {
        let d = KiDeckel::standard(BeraterId::Dachs);
        assert_eq!(d.kosten_ist(0, 0, 0), Some(Cent(0)));
        assert_eq!(d.kosten_ist(1, 0, 0), Some(Cent(1)));
        assert_eq!(d.kosten_ist(0, 0, 1), Some(Cent(1)));
        assert_eq!(d.kosten_ist(5_000, 0, 1), Some(Cent(2)));
        assert_eq!(d.kosten_ist(u64::MAX, u64::MAX, u64::MAX), None);
    }
}
