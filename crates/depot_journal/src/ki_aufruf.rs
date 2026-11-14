//! Hashbeleg eines KI-Aufrufs mit reservierter Buchung.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KiAufrufErgebnis {
    Beantwortet,
    Abgelehnt,
    Unbrauchbar,
    NichtGesendet,
}

impl KiAufrufErgebnis {
    fn text(self) -> &'static str {
        match self {
            Self::Beantwortet => "beantwortet",
            Self::Abgelehnt => "abgelehnt",
            Self::Unbrauchbar => "unbrauchbar",
            Self::NichtGesendet => "nicht_gesendet",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KiAufruf {
    pub nr: usize,
    pub anbieter: String,
    pub modell: String,
    pub vorlage: String,
    pub frage: Option<String>,
    pub antwort: Option<String>,
    pub token_ein: Option<u64>,
    pub token_aus: Option<u64>,
    pub ergebnis: KiAufrufErgebnis,
}

fn sha(t: &str) -> bool {
    t.len() == 64
        && t.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn textfeld(text: &str) -> bool {
    !text.is_empty() && text.len() <= 128 && !text.chars().any(char::is_control)
}

impl KiAufruf {
    pub(crate) fn text(&self) -> String {
        let token = |n: Option<u64>| n.map_or_else(|| "-".into(), |n| n.to_string());
        format!(
            "{};{};{};{};{};{};{};{};{}",
            self.nr,
            self.anbieter,
            self.modell,
            self.vorlage,
            self.frage.as_deref().unwrap_or("-"),
            self.antwort.as_deref().unwrap_or("-"),
            token(self.token_ein),
            token(self.token_aus),
            self.ergebnis.text()
        )
    }

    pub(crate) fn lies(text: &str) -> Option<Self> {
        let felder: Vec<_> = text.split(';').collect();
        let [
            nr,
            anbieter,
            modell,
            vorlage,
            frage,
            antwort,
            ein,
            aus,
            ergebnis,
        ] = felder.as_slice()
        else {
            return None;
        };
        if !sha(vorlage) || !textfeld(anbieter) || !textfeld(modell) {
            return None;
        }
        let ergebnis = [
            KiAufrufErgebnis::Beantwortet,
            KiAufrufErgebnis::Abgelehnt,
            KiAufrufErgebnis::Unbrauchbar,
            KiAufrufErgebnis::NichtGesendet,
        ]
        .into_iter()
        .find(|e| e.text() == *ergebnis)?;
        let abdruck = |t: &str| sha(t).then(|| t.to_owned());
        let a = Self {
            nr: nr.parse().ok()?,
            anbieter: (*anbieter).into(),
            modell: (*modell).into(),
            vorlage: (*vorlage).into(),
            frage: abdruck(frage),
            antwort: abdruck(antwort),
            token_ein: ein.parse().ok(),
            token_aus: aus.parse().ok(),
            ergebnis,
        };
        ((a.frage.is_some() || ergebnis == KiAufrufErgebnis::NichtGesendet) && a.text() == text)
            .then_some(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aufruf() -> KiAufruf {
        KiAufruf {
            nr: 0,
            anbieter: "anthropic".into(),
            modell: "testmodell".into(),
            vorlage: "a".repeat(64),
            frage: Some("b".repeat(64)),
            antwort: Some("c".repeat(64)),
            token_ein: Some(10),
            token_aus: Some(20),
            ergebnis: KiAufrufErgebnis::Beantwortet,
        }
    }

    #[test]
    fn rundlauf() {
        let a = aufruf();
        assert_eq!(KiAufruf::lies(&a.text()), Some(a));
    }

    #[test]
    fn sha_gross_abgelehnt() {
        let a = KiAufruf {
            frage: Some("B".repeat(64)),
            ..aufruf()
        };
        assert!(KiAufruf::lies(&a.text()).is_none());
    }

    #[test]
    fn sha_63_zeichen_abgelehnt() {
        let a = KiAufruf {
            frage: Some("b".repeat(63)),
            ..aufruf()
        };
        assert!(KiAufruf::lies(&a.text()).is_none());
    }

    #[test]
    fn feld_mit_semikolon_abgelehnt() {
        let a = KiAufruf {
            modell: "gpt;x".into(),
            ..aufruf()
        };
        assert!(KiAufruf::lies(&a.text()).is_none());
    }

    #[test]
    fn ohne_antwort() {
        let a = KiAufruf {
            antwort: None,
            token_aus: None,
            ergebnis: KiAufrufErgebnis::Unbrauchbar,
            ..aufruf()
        };
        assert_eq!(KiAufruf::lies(&a.text()), Some(a));
    }

    #[test]
    fn ergebnis_unbekannt_abgelehnt() {
        assert!(KiAufruf::lies(&aufruf().text().replace("beantwortet", "fertig")).is_none());
    }
}
