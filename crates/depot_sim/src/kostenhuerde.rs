//! Kostenhürde bei gleichem Ordervolumen für Kauf und Verkauf.

use depot_kern::{Boerse, Cent, Grenzen};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kostenhuerde_enthaelt_den_spread_beider_seiten() -> Result<(), &'static str> {
        let text = hinweis(Cent(10_000), Cent(150), Some(Boerse::Xetra)).ok_or("Kostenhürde")?;
        assert!(text.contains("mindestens 3,10 %"), "{text}");
        assert!(text.contains("Modellspread für Kauf und Verkauf zusammen 10 Basispunkte"));
        assert!(!text.contains("Spread nicht belegt"));
        Ok(())
    }
}
