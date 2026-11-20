//! Auswertung der festen Felder in den Journalzeilen einer Abstimmungsrunde.

struct Rundenbeleg {
    geaendert: bool,
    vorschlag: bool,
    zustimmung_fremd: bool,
}

fn wahrheitswert(text: &str) -> Option<bool> {
    match text {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

impl Rundenbeleg {
    fn lies(zeile: &str) -> Option<Self> {
        let (_, felder) = zeile.split_once(": verwertbar=")?;
        let (verwertbar, felder) = felder.split_once("; Änderung=")?;
        if verwertbar != "true" {
            return None;
        }
        let (geaendert, felder) = felder.split_once("; Ziele=")?;
        let (ziele, felder) = felder.split_once("; Zustimmung zum fremden Stand=")?;
        let (zustimmung, _) = felder.split_once("; Grund: ")?;
        let vorschlag = match ziele {
            "None" => false,
            text if text.starts_with("Some({") && text.ends_with("})") => true,
            _ => return None,
        };
        Some(Self {
            geaendert: wahrheitswert(geaendert)?,
            vorschlag,
            zustimmung_fremd: wahrheitswert(zustimmung)?,
        })
    }
}

fn einzeln(zeilen: &[&str]) -> Option<Rundenbeleg> {
    let mut status = zeilen.iter().filter(|z| {
        z.split_once(": ")
            .is_some_and(|(_, t)| t.starts_with("verwertbar="))
    });
    let beleg = Rundenbeleg::lies(status.next()?)?;
    status.next().is_none().then_some(beleg)
}

/// Folgert nur aus einer vollständig belegten letzten Runde und dem Befund KeinVorschlag.
pub(super) fn grund(runde: u8, eigen: &[&str], fremd: &[&str]) -> Option<String> {
    let (e, f) = (einzeln(eigen)?, einzeln(fremd)?);
    let grund = match (e.vorschlag, e.geaendert, f.zustimmung_fremd) {
        (false, false, _) => "Eigene Entscheidung ohne Vorschlag.",
        (false, true, _) => "Eigenen Vorschlag zurückgezogen.",
        (true, true, true) => {
            "Eigene Ziele geändert. Die Zustimmung galt dem vorgelegten Stand vor der Runde."
        }
        (true, true, false) => {
            "Eigene Ziele geändert. Für den vorgelegten Stand fehlte die Zustimmung des anderen Beraters."
        }
        (true, false, false) => {
            "Für den eigenen Vorschlag fehlte die Zustimmung des anderen Beraters."
        }
        (true, false, true) => return None,
    };
    Some(format!("Runde {runde}: {grund}"))
}
