//! Festgeschriebene Messlatte für den internen Papierhandel und separates IBKR-Profil.

use crate::Cent;

pub const MINDEST_TAGE: usize = 60;
pub const MINDEST_VORSPRUNG_BP: i64 = 0;
pub const HOECHSTER_RUECKGANG_BP: i64 = 1_500;
pub const HOECHSTER_AUSFALL_BP: i64 = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ergebnis {
    Traegt,
    NochNicht,
    Verfehlt,
    NichtBewertbar,
}

pub fn bewertung(
    tage: usize,
    vorsprung: Option<i64>,
    rueckgang: Option<i64>,
    ausfall: Option<i64>,
) -> Ergebnis {
    let (Some(v), Some(r), Some(a)) = (vorsprung, rueckgang, ausfall) else {
        return Ergebnis::NichtBewertbar;
    };
    if v < MINDEST_VORSPRUNG_BP || r >= HOECHSTER_RUECKGANG_BP || a > HOECHSTER_AUSFALL_BP {
        Ergebnis::Verfehlt
    } else if tage < MINDEST_TAGE {
        Ergebnis::NochNicht
    } else {
        Ergebnis::Traegt
    }
}

/// Größter Rückgang mit Startgeld als erstem Höchststand und Aufrundung in Basispunkten.
pub fn rueckgang_bp(verlauf: &[Option<Cent>], start: Cent) -> Option<i64> {
    let fehlt = verlauf.iter().filter(|w| w.is_none()).count();
    if verlauf.is_empty() || fehlt.saturating_mul(10) > verlauf.len() || start.0 <= 0 {
        return None;
    }
    let mut hoch = start;
    let mut max = 0;
    for wert in verlauf.iter().flatten().copied() {
        hoch = hoch.max(wert);
        let nenner = i128::from(hoch.0);
        let zaehler = i128::from(hoch.0)
            .checked_sub(i128::from(wert.0))?
            .checked_mul(10_000)?;
        let bp = zaehler
            .checked_add(nenner.checked_sub(1)?)?
            .checked_div(nenner)?;
        max = max.max(i64::try_from(bp).ok()?);
    }
    Some(max)
}

/// Anteil ausgefallener und begonnener Tage an allen Handelstagen, in Basispunkten.
pub fn ausfall_bp(ausfaelle: usize, tage: usize) -> Option<i64> {
    let zaehler = ausfaelle.checked_mul(10_000)?;
    let bp = zaehler
        .checked_add(tage.checked_sub(1)?)?
        .checked_div(tage)?;
    i64::try_from(bp).ok()
}

/// Das in #357 freigegebene Profil für den Langfrist-Bereich am IBKR-Papierkonto
/// und die spätere Echtgeldstufe. Der Kodex bindet diese Datei an die Festschreibung.
pub mod ibkr {
    use crate::Cent;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct KiTakt {
        pub ki_pflichtblick_woechentlich: bool,
        pub ki_gruendlich_monatlich: bool,
        pub ki_ausserhalb_des_takts_nur_bei_anlass: bool,
        pub ki_anfragen_pro_tag_max: u8,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Schutz {
        pub halt_tagesverlust_bp: u16,
        pub notbremse_hochstand_bp: u16,
        pub notbremse_storniert_kauforders: bool,
        pub notbremse_verkauf_nur_nach_bestaetigung: bool,
        pub schutzstopp_beim_broker: bool,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Vergleich {
        pub etf_kaufen_und_halten_nach_kosten: bool,
        pub estr_daneben_berichten: bool,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Echtgeldkriterien {
        pub papiermonate_min_nach_technischer_abnahme: u8,
        pub regelverletzungen_max: u8,
        pub mehrkosten_max_bp: u16,
        pub abgleiche_muessen_stimmen: bool,
        pub identitaetspruefung: bool,
        pub separate_echtgeldfreigabe: bool,
        pub aufstockung_nach_echtmonaten_min: u8,
    }

    /// Alle Prozentwerte sind Basispunkte, alle Geldbeträge Eurocent.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Profil {
        pub startkapital: Cent,
        pub langfrist: Cent,
        pub sicher: Cent,
        pub kurzfrist: Cent,
        pub gebuehrenpuffer: Cent,
        pub kaufpuffer_bp: u16,
        pub kontoart: &'static str,
        pub margin_erlaubt: bool,
        pub waehrung: &'static str,
        pub tarif: &'static str,
        pub routing: &'static str,
        pub etf_isin: &'static str,
        pub aktien_isins: [&'static str; 10],
        pub positionen_max: u8,
        pub etf_anteil_max_bp: u16,
        pub aktienanteil_max_bp: u16,
        pub ordergebuehr_max: Cent,
        pub gebuehrenquote_max_bp: u16,
        pub mindestorder: Cent,
        pub orders_je_jahr_max_ohne_erstkauf: u8,
        pub erstkauforders_max: u8,
        pub orders_im_ersten_monat_max: u8,
        pub umschichtung_min_bp: u16,
        pub verkaufserloes_nur_nach_gutschrift: bool,
        pub handelsbudget_jahr_ohne_erstkauf_bp: u16,
        pub handelsbudget_jahr_ohne_erstkauf: Cent,
        pub ki_budget_monat_gemeinsam: Cent,
        pub ki_budget_jahr_gemeinsam: Cent,
        pub ki_und_gebuehrenbudget_jahr_bp: u16,
        pub ki_und_gebuehrenbudget_jahr: Cent,

        pub ki_takt: KiTakt,
        pub schutz: Schutz,
        pub vergleich: Vergleich,
        pub echtgeldkriterien: Echtgeldkriterien,
    }

    pub const LANGFRIST: Profil = Profil {
        startkapital: Cent(100_000),
        langfrist: Cent(100_000),
        sicher: Cent(0),
        kurzfrist: Cent(0),
        gebuehrenpuffer: Cent(3_000),
        kaufpuffer_bp: 100,
        kontoart: "Cash",
        margin_erlaubt: false,
        waehrung: "EUR",
        tarif: "Tiered",
        routing: "SmartRouting",
        etf_isin: "IE00B4L5Y983", // EUNL
        // Feste Liste aus #357. Änderungen an der allgemeinen Beobachtungsliste wirken hier nicht mit.
        aktien_isins: [
            "DE0007164600", // SAP
            "DE0007236101", // Siemens
            "DE0008404005", // Allianz
            "DE0005557508", // Deutsche Telekom
            "DE000BASF111", // BASF
            "DE0007100000", // Mercedes-Benz
            "DE0008430026", // Münchener Rück
            "DE0005810055", // Deutsche Börse
            "DE0006231004", // Infineon
            "DE0005190003", // BMW
        ],
        positionen_max: 3,
        etf_anteil_max_bp: 6_000,
        aktienanteil_max_bp: 3_000,
        ordergebuehr_max: Cent(218),
        gebuehrenquote_max_bp: 100,
        mindestorder: Cent(22_000),
        orders_je_jahr_max_ohne_erstkauf: 6,
        erstkauforders_max: 3,
        orders_im_ersten_monat_max: 2,
        umschichtung_min_bp: 500,
        verkaufserloes_nur_nach_gutschrift: true,
        handelsbudget_jahr_ohne_erstkauf_bp: 140,
        handelsbudget_jahr_ohne_erstkauf: Cent(1_400),
        ki_budget_monat_gemeinsam: Cent(150),
        ki_budget_jahr_gemeinsam: Cent(1_800),
        ki_und_gebuehrenbudget_jahr_bp: 500,
        ki_und_gebuehrenbudget_jahr: Cent(5_000),

        ki_takt: KiTakt {
            ki_pflichtblick_woechentlich: true,
            ki_gruendlich_monatlich: true,
            ki_ausserhalb_des_takts_nur_bei_anlass: true,
            ki_anfragen_pro_tag_max: 1,
        },
        schutz: Schutz {
            halt_tagesverlust_bp: 500,
            notbremse_hochstand_bp: 2_000,
            notbremse_storniert_kauforders: true,
            notbremse_verkauf_nur_nach_bestaetigung: true,
            schutzstopp_beim_broker: false,
        },
        vergleich: Vergleich {
            etf_kaufen_und_halten_nach_kosten: true,
            estr_daneben_berichten: true,
        },
        echtgeldkriterien: Echtgeldkriterien {
            papiermonate_min_nach_technischer_abnahme: 3,
            regelverletzungen_max: 0,
            mehrkosten_max_bp: 2_000,
            abgleiche_muessen_stimmen: true,
            identitaetspruefung: true,
            separate_echtgeldfreigabe: true,
            aufstockung_nach_echtmonaten_min: 12,
        },
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ibkr_positivliste_deckt_nur_die_freigegebenen_xetra_werte_ab() -> Result<(), String> {
        use crate::{Boerse, Isin, liste};

        let profil = ibkr::LANGFRIST;
        let liste = liste::beobachtungsliste()?;
        let aktien: std::collections::BTreeSet<_> = profil.aktien_isins.into_iter().collect();
        assert_eq!(aktien.len(), profil.aktien_isins.len());
        for isin in aktien {
            assert!(!liste::ETF_ISIN.contains(&isin));
            assert!(
                liste
                    .iter()
                    .any(|wert| wert.boerse == Boerse::Xetra && wert.isin.als_str() == isin)
            );
        }
        let etf = Isin::neu(profil.etf_isin).map_err(|e| format!("{e:?}"))?;
        assert!(
            liste
                .iter()
                .any(|wert| wert.kuerzel == "EUNL.DEX" && wert.isin == etf)
        );
        Ok(())
    }

    #[test]
    fn m3_rueckgang_1500() {
        let v = [500_000, 520_000, 442_000, 480_000].map(|n| Some(Cent(n)));
        let r = rueckgang_bp(&v, Cent(500_000));
        assert_eq!(r, Some(1_500));
        assert_eq!(bewertung(60, Some(0), r, Some(0)), Ergebnis::Verfehlt);
    }

    #[test]
    fn m3_rueckgang_1499() {
        let v = [500_000, 520_000, 442_100, 480_000].map(|n| Some(Cent(n)));
        let r = rueckgang_bp(&v, Cent(500_000));
        assert_eq!(r, Some(1_499));
        assert_eq!(bewertung(60, Some(0), r, Some(0)), Ergebnis::Traegt);
    }

    #[test]
    fn rueckgang_zaehlt_startgeld_als_hoechststand() {
        assert_eq!(
            rueckgang_bp(&[Some(Cent(400_000))], Cent(500_000)),
            Some(2_000)
        );
    }

    #[test]
    fn zu_viele_unbewertbare_tage() {
        let mut v = vec![Some(Cent(500_000)); 17];
        v.extend([None; 3]);
        assert_eq!(rueckgang_bp(&v, Cent(500_000)), None);
    }

    #[test]
    fn m4_ausfaelle_500() {
        assert_eq!(ausfall_bp(5, 100), Some(500));
        assert_eq!(
            bewertung(100, Some(0), Some(0), ausfall_bp(5, 100)),
            Ergebnis::Traegt
        );
    }

    #[test]
    fn m4_ausfaelle_501() {
        assert_eq!(ausfall_bp(51, 1000), Some(510));
        assert_eq!(ausfall_bp(51, 1019), Some(501));
        assert_eq!(
            bewertung(100, Some(0), Some(0), ausfall_bp(51, 1019)),
            Ergebnis::Verfehlt
        );
    }

    #[test]
    fn m1_zu_frueh() {
        assert_eq!(
            bewertung(59, Some(0), Some(0), Some(0)),
            Ergebnis::NochNicht
        );
    }

    #[test]
    fn m1_genau() {
        assert_eq!(bewertung(60, Some(0), Some(0), Some(0)), Ergebnis::Traegt);
    }

    #[test]
    fn vorsprung_null_reicht() {
        assert_eq!(bewertung(60, Some(0), Some(0), Some(0)), Ergebnis::Traegt);
        assert_eq!(
            bewertung(60, Some(-1), Some(0), Some(0)),
            Ergebnis::Verfehlt
        );
    }
}
