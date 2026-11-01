//! Grenzen und reine Pruefung. Kein Netz, keine Dateien, keine Uhr.

use depot_kern::{BeraterId, Cent};

/// Kalendertag. Die Uhrzeit spielt keine Rolle: Grenzen gelten je Tag, Monat und Jahr.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Datum {
    pub jahr: u16,
    pub monat: u8,
    pub tag: u8,
}

impl Datum {
    pub fn neu(jahr: u16, monat: u8, tag: u8) -> Option<Datum> {
        ((1..=12).contains(&monat) && (1..=31).contains(&tag)).then_some(Datum { jahr, monat, tag })
    }

    fn gleicher_monat(self, andere: Datum) -> bool {
        self.jahr == andere.jahr && self.monat == andere.monat
    }
}

/// Listenpreise in Cent je Million Token. Dollar werden als Euro gezaehlt; das ueberschaetzt
/// die Kosten, solange der Euro mehr wert ist als der Dollar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preise {
    pub cent_je_mio_ein: i64,
    pub cent_je_mio_aus: i64,
}

/// Deckel fuer einen einzelnen Aufruf. Der KI-Client muss `max_token_aus` als
/// Antwortgrenze an den Anbieter senden und Eingaben ueber `max_token_ein` ablehnen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KiDeckel {
    pub max_token_ein: u32,
    pub max_token_aus: u32,
    pub preise: Preise,
}

impl KiDeckel {
    /// Dachs = Claude: 40.000 Token rein, 16.000 raus, 4 $ / 20 $ je Mio.
    /// Maus = GPT: 40.000 Token rein, 10.000 raus, 10 $ / 50 $ je Mio.
    pub fn standard(berater: BeraterId) -> KiDeckel {
        match berater {
            BeraterId::Dachs => KiDeckel {
                max_token_ein: 40_000,
                max_token_aus: 16_000,
                preise: Preise {
                    cent_je_mio_ein: 400,
                    cent_je_mio_aus: 2_000,
                },
            },
            BeraterId::Maus => KiDeckel {
                max_token_ein: 40_000,
                max_token_aus: 10_000,
                preise: Preise {
                    cent_je_mio_ein: 1_000,
                    cent_je_mio_aus: 5_000,
                },
            },
        }
    }

    /// Was ein Aufruf schlimmstenfalls kostet, aufgerundet auf ganze Cent.
    pub fn hoechstkosten(&self) -> Option<Cent> {
        let ein = i64::from(self.max_token_ein).checked_mul(self.preise.cent_je_mio_ein)?;
        let aus = i64::from(self.max_token_aus).checked_mul(self.preise.cent_je_mio_aus)?;
        let cent = ein
            .checked_add(aus)?
            .checked_add(999_999)?
            .checked_div(1_000_000)?;
        Some(Cent(cent))
    }
}

/// Grenzen je KI. Sie gelten fuer Dachs und Maus getrennt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KiGrenzen {
    pub aufrufe_je_zyklus: u32,
    pub zyklen_je_tag: u32,
    pub aufrufe_je_monat: u32,
    pub kosten_je_monat: Cent,
    pub kosten_je_jahr: Cent,
}

impl KiGrenzen {
    /// 2 Aufrufe je Zyklus, 1 Zyklus je Tag, 4 Aufrufe und 4 € je Monat,
    /// 50 € je Jahr (zusammen 1 % von 10.000 €).
    pub fn standard() -> KiGrenzen {
        KiGrenzen {
            aufrufe_je_zyklus: 2,
            zyklen_je_tag: 1,
            aufrufe_je_monat: 4,
            kosten_je_monat: Cent(400),
            kosten_je_jahr: Cent(5_000),
        }
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
    /// Kassenbuch belegt: ein anderer Lauf ist aktiv oder abgestuerzt.
    KassenbuchBelegt,
    /// Kassenbuch nicht les- oder schreibbar.
    Kassenbuch(String),
}

fn anzahl(grenze: u32) -> usize {
    usize::try_from(grenze).unwrap_or(0)
}

fn summe<'a>(mut buchungen: impl Iterator<Item = &'a Buchung>) -> Option<Cent> {
    buchungen.try_fold(Cent::NULL, |s, b| s.checked_add(b.reserviert))
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
        .filter(|b| b.datum.gleicher_monat(datum))
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
    let jahr = summe(eigene.into_iter().filter(|b| b.datum.jahr == datum.jahr))
        .and_then(|s| s.checked_add(kosten))
        .ok_or(Sperre::Rechenfehler)?;
    if jahr > grenzen.kosten_je_jahr {
        return Err(Sperre::JahresbudgetErschoepft);
    }
    Ok(kosten)
}

mod siegel {
    /// Nur in `depot_ki` erzeugbar.
    #[derive(Debug, PartialEq, Eq)]
    pub struct Siegel(());

    impl Siegel {
        pub(crate) fn neu() -> Siegel {
            Siegel(())
        }
    }
}

/// Berechtigt zu genau einem KI-Aufruf. Nicht kopierbar, nur im Kassenbuch erzeugbar.
/// Der KI-Client nimmt sie beim Aufruf als Wert entgegen und verbraucht sie dabei.
#[derive(Debug, PartialEq, Eq)]
pub struct AufrufErlaubnis {
    buchung: Buchung,
    deckel: KiDeckel,
    _siegel: siegel::Siegel,
}

impl AufrufErlaubnis {
    pub(crate) fn neu(buchung: Buchung, deckel: KiDeckel) -> AufrufErlaubnis {
        AufrufErlaubnis {
            buchung,
            deckel,
            _siegel: siegel::Siegel::neu(),
        }
    }

    pub fn buchung(&self) -> &Buchung {
        &self.buchung
    }

    pub fn deckel(&self) -> &KiDeckel {
        &self.deckel
    }

    /// Ein Token umfasst mindestens ein Byte. Wer hoechstens `max_token_ein` Bytes sendet,
    /// sendet also sicher nicht mehr Token als erlaubt.
    pub fn eingabe_passt(&self, eingabe: &str) -> bool {
        u32::try_from(eingabe.len()).is_ok_and(|n| n <= self.deckel.max_token_ein)
    }
}
