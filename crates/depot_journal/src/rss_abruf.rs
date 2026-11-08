//! Verwaltungsbeleg für den RSS-Abrufabstand. Er beginnt keinen Papiertag.
use depot_kern::{Datum, Zeitpunkt};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RssAbruf {
    pub quelle: String,
    pub zeit: Zeitpunkt,
}
impl RssAbruf {
    pub fn neu(quelle: &str, zeit: Zeitpunkt) -> Option<Self> {
        ["tagesschau", "bbc-world"].contains(&quelle).then(|| Self {
            quelle: quelle.into(),
            zeit,
        })
    }
    pub(crate) fn text(&self) -> String {
        format!("{};{}", self.quelle, self.zeit.sekunden())
    }
    pub(crate) fn lies(text: &str) -> Option<Self> {
        let (quelle, sekunden) = text.split_once(';')?;
        let a = Self::neu(quelle, Zeitpunkt::aus_sekunden(sekunden.parse().ok()?)?)?;
        (a.text() == text).then_some(a)
    }
    /// Nur das exakt kanonische alte Sammlerformat wird als Verwaltungsbeleg erkannt.
    /// Datum, feste Quelle und Zeitpunkt müssen zusammenpassen; freie Beobachtungen bleiben frei.
    pub(crate) fn aus_alt(datum: Datum, text: &str) -> Option<Self> {
        let a = Self::lies(text.strip_prefix("rss-abruf: ")?)?;
        (a.zeit.datum() == Some(datum)).then_some(a)
    }
}
