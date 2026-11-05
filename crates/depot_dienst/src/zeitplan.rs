//! Der Zeitplan: wann der Dienst an Handelstagen (Montag bis Freitag) von selbst Termine und
//! Kurse holt und danach den Papiertag startet. Uhrzeiten in Frankfurter Ortszeit.
//!
//! **Einmal je Tag:** Was der Zeitplan an einem Tag gestartet hat, steht in
//! `daten/dienst_erledigt.txt`, bevor der Auftrag beginnt. Ein Neustart des Dienstes startet es
//! an diesem Tag nicht noch einmal. Der Papiertag selbst laeuft ohnehin nur einmal je
//! Handelstag (Journal).

use depot_kern::Datum;

use crate::werkbank::Art;

/// Der Zeitplan, wie er in `daten/zeitplan.txt` steht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Zeitplan {
    pub an: bool,
    /// Stunde und Minute.
    pub termine: (u8, u8),
    pub kurse: (u8, u8),
    /// Nach erfolgreichem Kursabruf des Zeitplans den Papiertag starten.
    pub papier_danach: bool,
}

impl Default for Zeitplan {
    /// Termine 17:40, Kurse 17:45 (nach dem Xetra-Schluss um 17:35), dann der Papiertag.
    fn default() -> Zeitplan {
        Zeitplan {
            an: true,
            termine: (17, 40),
            kurse: (17, 45),
            papier_danach: true,
        }
    }
}

const KOPF: &str = "zeitplan-v1";
const KOPF_ERLEDIGT: &str = "erledigt-v1";

fn ja_nein(b: bool) -> &'static str {
    if b { "ja" } else { "nein" }
}

fn lies_ja_nein(t: &str) -> Option<bool> {
    match t {
        "ja" => Some(true),
        "nein" => Some(false),
        _ => None,
    }
}

/// Liest genau `hh:mm` mit 00:00 bis 23:59.
pub fn lies_uhrzeit(t: &str) -> Option<(u8, u8)> {
    let (h, m) = t.split_once(':')?;
    if h.len() != 2 || m.len() != 2 || !h.bytes().chain(m.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let h: u8 = h.parse().ok()?;
    let m: u8 = m.parse().ok()?;
    (h <= 23 && m <= 59).then_some((h, m))
}

pub fn uhrzeit_text((h, m): (u8, u8)) -> String {
    format!("{h:02}:{m:02}")
}

impl Zeitplan {
    pub fn als_text(&self) -> String {
        format!(
            "{KOPF}\nan={}\ntermine={}\nkurse={}\npapier_danach={}\n",
            ja_nein(self.an),
            uhrzeit_text(self.termine),
            uhrzeit_text(self.kurse),
            ja_nein(self.papier_danach)
        )
    }

    /// Liest genau die eigene Form.
    ///
    /// # Errors
    /// Jede Abweichung.
    pub fn lies(text: &str) -> Result<Zeitplan, String> {
        let mut z = text.lines();
        if z.next() != Some(KOPF) {
            return Err("Zeitplan: Kopfzeile fehlt".into());
        }
        let mut wert = |name: &str| -> Result<String, String> {
            let zeile = z.next().ok_or_else(|| format!("Zeitplan: {name} fehlt"))?;
            zeile
                .strip_prefix(name)
                .and_then(|r| r.strip_prefix('='))
                .map(str::to_string)
                .ok_or_else(|| format!("Zeitplan: {name} erwartet"))
        };
        let an = lies_ja_nein(&wert("an")?).ok_or("Zeitplan: an")?;
        let termine = lies_uhrzeit(&wert("termine")?).ok_or("Zeitplan: termine")?;
        let kurse = lies_uhrzeit(&wert("kurse")?).ok_or("Zeitplan: kurse")?;
        let papier_danach = lies_ja_nein(&wert("papier_danach")?).ok_or("Zeitplan: papier")?;
        if z.next().is_some() {
            return Err("Zeitplan: Zeile zu viel".into());
        }
        let plan = Zeitplan {
            an,
            termine,
            kurse,
            papier_danach,
        };
        if plan.als_text() != text {
            return Err("Zeitplan: nicht in eigener Form".into());
        }
        Ok(plan)
    }
}

/// Was der Zeitplan an welchem Tag schon gestartet hat.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Erledigt {
    pub termine: Option<Datum>,
    pub kurse: Option<Datum>,
}

fn datum_feld(d: Option<Datum>) -> String {
    d.map(|d| d.to_string()).unwrap_or_default()
}

impl Erledigt {
    pub fn als_text(&self) -> String {
        format!(
            "{KOPF_ERLEDIGT}\ntermine={}\nkurse={}\n",
            datum_feld(self.termine),
            datum_feld(self.kurse)
        )
    }

    /// Liest genau die eigene Form.
    ///
    /// # Errors
    /// Jede Abweichung.
    pub fn lies(text: &str) -> Result<Erledigt, String> {
        let mut z = text.lines();
        if z.next() != Some(KOPF_ERLEDIGT) {
            return Err("Erledigt: Kopfzeile fehlt".into());
        }
        let mut datum = |name: &str| -> Result<Option<Datum>, String> {
            let w = z
                .next()
                .and_then(|l| l.strip_prefix(name))
                .and_then(|r| r.strip_prefix('='))
                .ok_or_else(|| format!("Erledigt: {name} erwartet"))?;
            if w.is_empty() {
                Ok(None)
            } else {
                Datum::lies(w)
                    .map(Some)
                    .ok_or_else(|| format!("Erledigt: {name} unlesbar"))
            }
        };
        let e = Erledigt {
            termine: datum("termine")?,
            kurse: datum("kurse")?,
        };
        if e.als_text() != text {
            return Err("Erledigt: nicht in eigener Form".into());
        }
        Ok(e)
    }

    pub fn setze(&mut self, art: Art, tag: Datum) {
        match art {
            Art::Termine => self.termine = Some(tag),
            Art::Kurse => self.kurse = Some(tag),
            Art::Einrichten | Art::Papier | Art::UpdatePruefen | Art::Update => {}
        }
    }
}

/// Ein Handelstag des Zeitplans: Montag bis Freitag.
pub fn handelstag(d: Datum) -> bool {
    d.wochentag().is_some_and(|w| (1..=5).contains(&w))
}

/// Was der Zeitplan jetzt starten soll: `Termine` vor `Kurse`, jeweils einmal je Tag und erst ab
/// seiner Uhrzeit. `tag`, `zeit` sind Frankfurter Ortszeit.
pub fn faellig(plan: &Zeitplan, erledigt: &Erledigt, tag: Datum, zeit: (u8, u8)) -> Option<Art> {
    if !plan.an || !handelstag(tag) {
        return None;
    }
    if zeit >= plan.termine && erledigt.termine != Some(tag) {
        return Some(Art::Termine);
    }
    if zeit >= plan.kurse && erledigt.kurse != Some(tag) {
        return Some(Art::Kurse);
    }
    None
}

fn tag_danach(d: Datum) -> Option<Datum> {
    let z = depot_kern::Zeitpunkt::neu(d, 12, 0, 0)?;
    depot_kern::Zeitpunkt::aus_sekunden(z.sekunden().checked_add(86_400)?)?.datum()
}

/// Der naechste geplante Lauf ab jetzt: Art, Tag und Uhrzeit. `None`, wenn der Zeitplan aus ist.
pub fn naechster(
    plan: &Zeitplan,
    erledigt: &Erledigt,
    tag: Datum,
) -> Option<(Art, Datum, (u8, u8))> {
    if !plan.an {
        return None;
    }
    let mut d = tag;
    for _ in 0..8 {
        if handelstag(d) {
            let mut kandidaten = [(plan.termine, Art::Termine), (plan.kurse, Art::Kurse)];
            kandidaten.sort_by_key(|(u, _)| *u);
            for (u, art) in kandidaten {
                let schon = match art {
                    Art::Termine => erledigt.termine == Some(d),
                    _ => erledigt.kurse == Some(d),
                };
                // Heute zaehlt auch eine verpasste Uhrzeit: Sie startet beim naechsten Blick.
                if !schon {
                    return Some((art, d, u));
                }
            }
        }
        d = tag_danach(d)?;
    }
    None
}
