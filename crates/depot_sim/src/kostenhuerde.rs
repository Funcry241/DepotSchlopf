//! Kostenhürde bei gleichem Ordervolumen für Kauf und Verkauf.

use std::fmt::Write as _;

use depot_kern::messlatte::ibkr::LANGFRIST;
use depot_kern::{BP_GANZ, Boerse, Cent, Grenzen, Isin, Kurse, Preis, Rundung, Topf, Vorschlag};

/// Modellabschlag je Seite nach P33. Ein am Markt beobachteter Spread ist nicht verfügbar.
pub fn spread_je_seite_bp(boerse: Boerse) -> i64 {
    Grenzen::standard().spread_bp(boerse)
}

pub fn berechne(volumen: Cent, gebuehr: Cent) -> Option<(Cent, i64)> {
    let summe = gebuehr.checked_add(gebuehr)?;
    let bp = summe.in_bp_von(volumen)?;
    let bp = if volumen.anteil_bp_ab(bp)? < summe {
        bp.checked_add(1)?
    } else {
        bp
    };
    Some((summe, bp))
}

pub fn gesamt_bp(volumen: Cent, gebuehr: Cent, boerse: Option<Boerse>) -> Option<i64> {
    let (_, gebuehren_bp) = berechne(volumen, gebuehr)?;
    let spread_bp = boerse.map_or(0, spread_je_seite_bp).checked_mul(2)?;
    gebuehren_bp.checked_add(spread_bp)
}

pub fn hinweis(volumen: Cent, gebuehr: Cent, boerse: Option<Boerse>) -> Option<String> {
    let (summe, _) = berechne(volumen, gebuehr)?;
    let bp = gesamt_bp(volumen, gebuehr, boerse)?;
    let spread = if let Some(boerse) = boerse {
        let spread = spread_je_seite_bp(boerse).checked_mul(2)?;
        format!("Modellspread für Kauf und Verkauf zusammen {spread} Basispunkte.")
    } else {
        "Spread nicht belegt.".to_owned()
    };
    Some(format!(
        "Kostenhürde bei {volumen} Ordervolumen: {summe} Gebühren für Kauf und Verkauf, mindestens {},{:02} % Kursgewinn bei gleichem Volumen. {spread}",
        bp.div_euclid(100),
        bp.rem_euclid(100)
    ))
}

/// Modellrechnung zu den vor der Runde fixierten Kaufzielen beider Töpfe.
pub fn abstimmung(kurse: &Kurse, toepfe: &[(&Topf, Option<&Vorschlag>)]) -> String {
    let grenzen = Grenzen::standard();
    let mut text = String::new();
    for (topf, vorschlag) in toepfe {
        let Some(vorschlag) = vorschlag else {
            continue;
        };
        let topfwert = topf.wert_exakt(kurse);
        let name = crate::lauf::topf_name(topf.id());
        for (isin, gewicht) in &vorschlag.zielgewichte_bp {
            if *gewicht <= 0 {
                continue;
            }
            let Some((kurs, ziel)) = kurse
                .kurs(isin)
                .zip(topfwert.and_then(|wert| wert.mal_bp(*gewicht, Rundung::Ab)))
            else {
                let _ = writeln!(
                    text,
                    "{name} {isin}: Kostenhürde ohne belegten Kurs oder Depotwert offen."
                );
                continue;
            };
            let Some(kaufziel) = Preis::aus_cent(ziel)
                .and_then(|ziel| ziel.checked_sub(kurs.mal_stueck_exakt(topf.stueck(isin))?))
            else {
                continue;
            };
            if kaufziel <= Preis::NULL {
                continue;
            }
            let papier = depot_kern::liste::boerse_von(isin).and_then(|boerse| {
                modell(kurs, kaufziel, boerse, grenzen.min_order(), |volumen| {
                    grenzen.gebuehren().fuer(volumen)
                })
            });
            let papier = papier.as_deref().unwrap_or("Kostenhürde offen");
            let ibkr = ibkr_startprofil(isin, *gewicht, kurs);
            let _ = writeln!(
                text,
                "{name} {isin} · Papier: {papier} · 1.000-€-IBKR-Startprofil: {ibkr}"
            );
        }
        if vorschlag.park.is_some() {
            let _ = writeln!(
                text,
                "{name} · Parkwert: Die Kaufgröße ergibt sich aus dem verbleibenden Bargeld."
            );
        }
    }
    if !text.is_empty() {
        text.insert_str(0, "\nVom Programm berechnete Kaufkosten. Die Kostenhürde gilt ab dem genannten Kurs und enthält Gebühren und Modellspread für beide Seiten. Das IBKR-Startprofil rechnet ohne Bestand:\n");
        text.truncate(text.trim_end().len());
    }
    text
}

fn ibkr_startprofil(isin: &Isin, gewicht: i64, kurs: Preis) -> String {
    let isin = isin.als_str();
    let hoechstgewicht = if isin == LANGFRIST.etf_isin {
        LANGFRIST.etf_anteil_max_bp
    } else if LANGFRIST.aktien_isins.contains(&isin) {
        LANGFRIST.aktienanteil_max_bp
    } else {
        return "Wert außerhalb des Startprofils".into();
    };
    LANGFRIST
        .startkapital
        .anteil_bp_ab(gewicht.min(i64::from(hoechstgewicht)))
        .and_then(Preis::aus_cent)
        .and_then(|ziel| {
            modell(kurs, ziel, Boerse::Xetra, LANGFRIST.mindestorder, |_| {
                Some(LANGFRIST.ordergebuehr_max)
            })
        })
        .unwrap_or_else(|| "Kostenhürde offen".into())
}

fn modell(
    kurs: Preis,
    ziel: Preis,
    boerse: Boerse,
    mindestorder: Cent,
    gebuehr: impl FnOnce(Cent) -> Option<Cent>,
) -> Option<String> {
    let stueck = kurs.stueck_fuer(ziel)?;
    let referenz = kurs.euro_text();
    if stueck == 0 {
        return Some(format!(
            "kein ganzes Stück im Zielbudget von {} € bei {referenz} € Kurs",
            ziel.euro_text()
        ));
    }
    let kaufkurs = Grenzen::standard().papierkurs(boerse, kurs, Rundung::Auf)?;
    let volumen = kaufkurs.mal_stueck(stueck)?;
    if volumen < mindestorder {
        return Some(format!(
            "{stueck} Stück bei {referenz} € Kurs · unter Mindestorder von {mindestorder}"
        ));
    }
    let (summe, _) = berechne(volumen, gebuehr(volumen)?)?;
    let stueckzahl = i64::try_from(stueck).ok()?;
    let erloes_je_stueck = kaufkurs.0.checked_add(aufgeteilt(summe.0, stueckzahl)?)?;
    let geldfaktor = BP_GANZ.checked_sub(spread_je_seite_bp(boerse))?;
    // Der Geldkurs wird abgerundet. Darum erst den nötigen Erlös je Stück und dann
    // den nötigen Schlusskurs jeweils nach oben auf einen ganzen Cent runden.
    let schwelle = Cent(aufgeteilt(
        erloes_je_stueck.checked_mul(BP_GANZ)?,
        geldfaktor,
    )?);
    let differenz = Preis::aus_cent(schwelle)?.checked_sub(kurs)?;
    let basis = i128::from(kurs.fein());
    let bp = i128::from(differenz.fein())
        .checked_mul(i128::from(BP_GANZ))?
        .checked_add(basis / 2)?
        .checked_div(basis)?;
    Some(format!(
        "{stueck} Stück bei {referenz} € Kurs · Kostenhürde etwa {},{:02} % (Schlusskurs {schwelle}, Gebühren hin und zurück {summe})",
        bp / 100,
        bp % 100,
    ))
}

fn aufgeteilt(betrag: i64, teile: i64) -> Option<i64> {
    betrag
        .checked_div(teile)?
        .checked_add(i64::from(betrag.checked_rem(teile)? > 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ibkr_startprofil_rechnet_ganze_stuecke_und_beide_gebuehren() -> Result<(), String> {
        let telekom = Isin::neu("DE0005557508").map_err(|e| format!("{e:?}"))?;
        let rueck = Isin::neu("DE0008430026").map_err(|e| format!("{e:?}"))?;
        let telekom_kurs = Preis::lies_euro("26.34").ok_or("Kurs")?;
        let rueck_kurs = Preis::lies_euro("580.00").ok_or("Kurs")?;
        let kauf = ibkr_startprofil(&telekom, 3_000, telekom_kurs);
        assert!(kauf.contains("11 Stück bei 26,34 € Kurs"), "{kauf}");
        assert!(
            kauf.contains("1,67 % (Schlusskurs 26,78 €, Gebühren hin und zurück 4,36 €)"),
            "{kauf}"
        );
        let kein_stueck = ibkr_startprofil(&rueck, 3_000, rueck_kurs);
        assert!(
            kein_stueck.contains("kein ganzes Stück im Zielbudget von 300,00 €"),
            "{kein_stueck}"
        );
        Ok(())
    }

    #[test]
    fn kostenhuerde_enthaelt_den_spread_beider_seiten() -> Result<(), &'static str> {
        let text = hinweis(Cent(10_000), Cent(150), Some(Boerse::Xetra)).ok_or("Kostenhürde")?;
        assert!(text.contains("mindestens 3,10 %"), "{text}");
        assert!(text.contains("Modellspread für Kauf und Verkauf zusammen 10 Basispunkte"));
        assert!(!text.contains("Spread nicht belegt"));
        Ok(())
    }
}
