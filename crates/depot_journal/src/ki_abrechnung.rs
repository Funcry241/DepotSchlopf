//! Dauerhafte Minderkostenabrechnung genau einer reservierten Anfrage.
use depot_kern::Cent;

#[derive(Debug, PartialEq, Eq)]
pub struct KiAbrechnung {
    pub(crate) nr: usize,
    pub(crate) reserviert: Cent,
    pub(crate) ist: Cent,
}
impl KiAbrechnung {
    pub fn kosten(&self) -> Cent {
        self.ist
    }
    pub(crate) fn text(&self) -> String {
        format!("{};{};{}", self.nr, self.reserviert.0, self.ist.0)
    }
    pub(crate) fn lies(text: &str) -> Option<Self> {
        let t: Vec<_> = text.split(';').collect();
        let [nr, reserviert, ist] = t.as_slice() else {
            return None;
        };
        let a = Self {
            nr: nr.parse().ok()?,
            reserviert: Cent(reserviert.parse().ok()?),
            ist: Cent(ist.parse().ok()?),
        };
        (a.ist.0 >= 0 && a.ist <= a.reserviert && a.text() == text).then_some(a)
    }
}
