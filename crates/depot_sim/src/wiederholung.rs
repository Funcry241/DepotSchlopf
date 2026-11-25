//! Eine verfallene KI-Kauforder einmal ohne neue KI-Antwort wiederholen (#311 H2b).
//!
//! Wiederholt wird nur eine Order, an deren Versandtag der Topf eine Erstmeinung hatte. Eine
//! Wiederholung hat am eigenen Versandtag keine Erstmeinung; verfällt sie, wird das zum Anlass
//! und die KI entscheidet neu. Die Prüfung läuft nur an Tagen ohne anderen Anlass; mit Anlass
//! fragt der Lauf ohnehin die KI. Eine neue Meldung zum Wert seit der Erstmeinung ist bereits
//! ein Anlass (`anlass.rs`).

use depot_journal::{AuftragsZustand, KiZweck, Stand};
use depot_kern::{Grenzen, Preis, Rundung};
use depot_regelwerk::{Anlass, Richtung, Wiederholung};

use crate::szenario::Lage;

/// Höchstens so weit über dem Entschlusskurs darf das Limit der Wiederholung liegen.
pub const HOECHSTENS_UEBER_ENTSCHLUSS_BP: i64 = 300;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Pruefung {
    /// Die Order `nr` wird mit diesen Angaben wiederholt.
    Wiederholen { nr: u64, kauf: Wiederholung },
    /// Keine Wiederholung möglich: Der Grund wird zum Anlass, die KI wird gefragt.
    Anlass(String),
}

fn erstmeinung_am(stand: &Stand, lage: &Lage<'_>, tag: depot_kern::Datum) -> bool {
    let ki = stand.ki();
    ki.buchungen().iter().enumerate().any(|(nr, b)| {
        b.berater == lage.topf.id().berater()
            && b.datum == tag
            && ki
                .buchungsbeleg(nr)
                .is_some_and(|(zweck, _)| zweck == KiZweck::Erstmeinung)
    })
}

/// `None`, wenn am letzten Schluss keine wiederholbare KI-Kauforder dieses Topfes verfallen ist.
pub(crate) fn pruefe(stand: &Stand, g: &Grenzen, lage: &Lage<'_>) -> Option<Pruefung> {
    let buch = crate::ausfuehrung::ausfuehrungsbuch(stand);
    let verfallen: Vec<_> = buch
        .iter()
        .filter(|z| {
            z.topf == lage.topf.id()
                && z.richtung == Richtung::Kauf
                && z.anlass == Anlass::Umschichtung
                && z.zustand == AuftragsZustand::Verfallen
                && !crate::kernanlage::ist_kernauftrag(stand, z.nr)
        })
        .filter(|z| {
            // Verfallen am jüngsten belegten Schluss dieses Werts.
            z.frist.and_then(depot_kern::Zeitpunkt::frankfurt_tag)
                == lage
                    .kurse
                    .eintrag(&z.isin)
                    .map(depot_kern::Kurs::handelstag)
        })
        .collect();
    // Ohne Erstmeinung am Versandtag war die Order selbst eine Wiederholung: keine zweite.
    if let Some(z) = verfallen.iter().find(|z| {
        !z.versand
            .and_then(depot_kern::Zeitpunkt::frankfurt_tag)
            .is_some_and(|tag| erstmeinung_am(stand, lage, tag))
    }) {
        return Some(Pruefung::Anlass(format!(
            "verfallene Kauforder bei {}: Wiederholung {} verfallen",
            z.isin, z.nr
        )));
    }
    let [z] = verfallen.as_slice() else {
        if verfallen.is_empty() {
            return None;
        }
        let nummern: Vec<_> = verfallen.iter().map(|z| format!("{}", z.nr)).collect();
        return Some(Pruefung::Anlass(format!(
            "verfallene Kauforders {}",
            nummern.join(", ")
        )));
    };
    let anlass = |grund: &str| {
        Some(Pruefung::Anlass(format!(
            "verfallene Kauforder bei {}: {grund}",
            z.isin
        )))
    };
    let (Some(bezug), Some(schluss), Some(abstand)) =
        (z.bezug, lage.kurse.kurs(&z.isin), z.limit_bp)
    else {
        return anlass("Entschlusskurs oder Schluss nicht belegt");
    };
    let Some(obergrenze) = bezug
        .mal_bp(
            10_000_i64.checked_add(HOECHSTENS_UEBER_ENTSCHLUSS_BP)?,
            Rundung::Ab,
        )
        .and_then(Preis::aus_cent)
    else {
        return anlass("Obergrenze nicht berechenbar");
    };
    if schluss > obergrenze {
        return anlass("Schluss mehr als 3 % über dem Entschlusskurs");
    }
    let spielraum = obergrenze.in_bp_von(schluss)?.checked_sub(10_000)?;
    let limit_bp = abstand.min(spielraum);
    if limit_bp < g.limit_abstand_min_bp() {
        return anlass("kein Limitspielraum unter der Obergrenze");
    }
    let stopp_bp = stand.auftrag(z.nr).and_then(|a| a.beleg.stopp_bp);
    Some(Pruefung::Wiederholen {
        nr: z.nr,
        kauf: Wiederholung {
            isin: z.isin.clone(),
            stueck: z.stueck,
            limit_bp,
            stopp_bp,
        },
    })
}
