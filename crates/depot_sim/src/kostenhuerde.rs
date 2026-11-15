//! Geschätzte Handelsgebühren für Kauf und Verkauf bei gleichem Ordervolumen.

use depot_kern::Cent;

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

pub fn hinweis(volumen: Cent, gebuehr: Cent) -> Option<String> {
    let (summe, bp) = berechne(volumen, gebuehr)?;
    Some(format!(
        "Kostenhürde bei {volumen} Ordervolumen: {summe} Gebühren für Kauf und Verkauf, mindestens {},{:02} % Kursgewinn bei gleichem Volumen. Spread nicht belegt.",
        bp.div_euclid(100),
        bp.rem_euclid(100)
    ))
}
