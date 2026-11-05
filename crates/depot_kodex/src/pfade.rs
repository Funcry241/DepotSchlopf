//! Pfadrollen und die Matrix Klasse x Rolle.

use std::collections::BTreeSet;

use crate::deklaration::Klasse;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rolle {
    Deklaration,
    Festschreibung,
    /// `kodex/freigabe.txt`: die Grenzfreigabe zur Festschreibung (R30).
    Freigabe,
    Kodex,
    Doku,
    Test,
    Produktiv,
    Werkzeug,
    Unbekannt,
}

const WERKZEUG: [&str; 8] = [
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "rustfmt.toml",
    ".gitignore",
    "run.cmd",
    // Starter per Doppelklick fuer den Dienst mit Oberflaeche (Wolfgang 05.11.2026).
    "DepotSchlopf.cmd",
    "LICENSE",
];

pub fn rolle(pfad: &str) -> Rolle {
    if pfad == crate::DEKLARATION {
        return Rolle::Deklaration;
    }
    if pfad == crate::FESTSCHREIBUNG {
        return Rolle::Festschreibung;
    }
    if pfad == crate::freigabe::FREIGABE {
        return Rolle::Freigabe;
    }
    if pfad.starts_with("kodex/")
        || pfad.starts_with("crates/depot_kodex/")
        || pfad.starts_with(".github/")
    {
        return Rolle::Kodex;
    }
    if pfad.starts_with("docs/")
        || std::path::Path::new(pfad)
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("md"))
    {
        return Rolle::Doku;
    }
    if WERKZEUG.contains(&pfad) {
        return Rolle::Werkzeug;
    }
    if let Some(rest) = pfad.strip_prefix("crates/") {
        let mut teile = rest.splitn(3, '/');
        let _crate = teile.next();
        return match (teile.next(), teile.next()) {
            (Some("Cargo.toml"), None) => Rolle::Werkzeug,
            (Some("src"), Some(_)) => Rolle::Produktiv,
            (Some("tests"), Some(_)) => Rolle::Test,
            _ => Rolle::Unbekannt,
        };
    }
    Rolle::Unbekannt
}

pub fn erlaubt(klasse: Klasse, rolle: Rolle) -> bool {
    use Rolle as R;
    match rolle {
        R::Deklaration => true,
        R::Unbekannt => false,
        // Die Festschreibung und ihre Freigabe aendern nur diese beiden Klassen; jede Aenderung
        // der Festschreibung braucht die gebundene Grenzfreigabe (freigabe.rs). Eine
        // KodexAenderung darf nur die Berechnung aendern, denn Grenzen, Budgets und Produktarten
        // liegen im Produktivcode, den sie nicht beruehren darf.
        R::Festschreibung | R::Freigabe => matches!(
            klasse,
            Klasse::Gruendung | Klasse::FestschreibungAenderung | Klasse::KodexAenderung
        ),
        _ => match klasse {
            Klasse::Gruendung => true,
            Klasse::DokuOnly => rolle == R::Doku,
            Klasse::TestOnly => matches!(rolle, R::Doku | R::Test),
            Klasse::RefactorOnly => matches!(rolle, R::Doku | R::Test | R::Produktiv),
            Klasse::Funktion => matches!(rolle, R::Doku | R::Test | R::Produktiv | R::Werkzeug),
            Klasse::KodexAenderung => matches!(rolle, R::Kodex | R::Doku),
            Klasse::FestschreibungAenderung => {
                matches!(rolle, R::Doku | R::Test | R::Produktiv | R::Werkzeug)
            }
        },
    }
}

pub fn pruefe_rollen(
    klasse: Klasse,
    diff: &BTreeSet<String>,
    basis_hat_kodex: bool,
) -> Vec<String> {
    let mut fehler = Vec::new();
    if klasse == Klasse::Gruendung && basis_hat_kodex {
        fehler.push("Gruendung ist nur erlaubt, solange die Basis keinen Kodex hat".into());
    }
    for pfad in diff {
        let r = rolle(pfad);
        if !erlaubt(klasse, r) {
            fehler.push(format!("{klasse:?} darf {pfad} ({r:?}) nicht aendern"));
        }
    }
    fehler
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollen() {
        assert_eq!(rolle("kodex/aenderung.txt"), Rolle::Deklaration);
        assert_eq!(rolle("kodex/festschreibung.txt"), Rolle::Festschreibung);
        assert_eq!(rolle("kodex/freigabe.txt"), Rolle::Freigabe);
        assert_eq!(rolle("kodex/essenz_register.txt"), Rolle::Kodex);
        assert_eq!(rolle("crates/depot_kodex/src/lib.rs"), Rolle::Kodex);
        assert_eq!(rolle(".github/workflows/kodex.yml"), Rolle::Kodex);
        assert_eq!(rolle("docs/essenz.md"), Rolle::Doku);
        assert_eq!(rolle("crates/depot_ki/src/budget.rs"), Rolle::Produktiv);
        assert_eq!(rolle("crates/depot_ki/tests/budget.rs"), Rolle::Test);
        assert_eq!(rolle("crates/depot_ki/Cargo.toml"), Rolle::Werkzeug);
        assert_eq!(rolle("DepotSchlopf.cmd"), Rolle::Werkzeug);
        assert_eq!(rolle("geheim.sh"), Rolle::Unbekannt);
        assert_eq!(rolle("anders.cmd"), Rolle::Unbekannt);
    }

    #[test]
    fn kodex_nie_mit_produktivcode() {
        assert!(!erlaubt(Klasse::KodexAenderung, Rolle::Produktiv));
        assert!(!erlaubt(Klasse::Funktion, Rolle::Kodex));
        assert!(!erlaubt(Klasse::Funktion, Rolle::Festschreibung));
        assert!(!erlaubt(Klasse::FestschreibungAenderung, Rolle::Kodex));
        assert!(!erlaubt(Klasse::Gruendung, Rolle::Unbekannt));
        assert!(!erlaubt(Klasse::Funktion, Rolle::Freigabe));
        assert!(erlaubt(Klasse::FestschreibungAenderung, Rolle::Freigabe));
    }
}
