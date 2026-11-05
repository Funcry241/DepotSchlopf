//! Der Zeitplan: wann der Dienst an Handelstagen (Montag bis Freitag) von selbst Termine und
//! Kurse holt und danach den Papiertag startet. Uhrzeiten in Frankfurter Ortszeit.
//!
//! **Kurse je Boerse** (Wolfgang am 05.11.2026, Vorschlag von Maus): Jede Boerse der Liste wird
//! an ihren Handelstagen kurz nach ihrem eigenen Schluss abgerufen, nur mit ihren Werten
//! (`run.cmd kurse xetra`, `run.cmd kurse nyse`). Xetra zur eingestellten Uhrzeit `kurse`
//! (17:45), jede andere Boerse `NACHLAUF_MIN` Minuten nach ihrem Schluss an diesem Tag
//! (`Boerse::schluss`, mit Sommerzeit und verkuerzten Tagen), in Frankfurter Zeit; die NYSE
//! also meist um 22:10 Uhr. Der Papiertag beginnt erst nach dem letzten Abruf des Tages, wenn
//! wenigstens ein Abruf dieses Tages erfolgreich war; welche Kurse er nimmt, prueft er selbst.
//!
//! **Einmal je Tag:** Was der Zeitplan an einem Tag gestartet hat, steht in
//! `daten/dienst_erledigt.txt`, bevor der Auftrag beginnt. Ein Neustart des Dienstes startet es
//! an diesem Tag nicht noch einmal. Der Papiertag selbst laeuft ohnehin nur einmal je
//! Handelstag (Journal).

use depot_kern::{Boerse, Datum};

use crate::werkbank::Art;

/// So viele Minuten nach ihrem Schluss wird eine Boerse ausser Xetra abgerufen: wie Xetra mit
/// 17:45 Uhr nach 17:35 Uhr, gut innerhalb des Hoechstalters von 20 Minuten.
pub const NACHLAUF_MIN: i64 = 10;

/// Der Zeitplan, wie er in `daten/zeitplan.txt` steht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Zeitplan {
    pub an: bool,
    /// Stunde und Minute.
    pub termine: (u8, u8),
    /// Abruf der Xetra-Kurse. Andere Boersen nach ihrem eigenen Schluss (`abrufe`).
    pub kurse: (u8, u8),
    /// Nach erfolgreichem Kursabruf des Zeitplans den Papiertag starten.
    pub papier_danach: bool,
}

impl Default for Zeitplan {
    /// Termine 17:40, Kurse 17:45 (nach dem Xetra-Schluss um 17:35), dann der Papiertag, aber
    /// **aus**: Der Betrieb von selbst beginnt erst, wenn der Mensch ihn in der Einrichtung
    /// bewusst einschaltet. Ein erster neugieriger Start loest so keinen KI-Aufruf aus.
    fn default() -> Zeitplan {
        Zeitplan {
            an: false,
            termine: (17, 40),
            kurse: (17, 45),
            papier_danach: true,
        }
    }
}

const KOPF: &str = "zeitplan-v1";
const KOPF_ERLEDIGT: &str = "erledigt-v2";
/// Die fruehere Form ohne Abruf je Boerse; `kurse` war der Abruf von Xetra.
const KOPF_ERLEDIGT_V1: &str = "erledigt-v1";

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
    /// Abruf der Xetra-Kurse.
    pub kurse: Option<Datum>,
    /// Abruf der NYSE-Kurse.
    pub kurse_nyse: Option<Datum>,
}

fn datum_feld(d: Option<Datum>) -> String {
    d.map(|d| d.to_string()).unwrap_or_default()
}

impl Erledigt {
    pub fn als_text(&self) -> String {
        format!(
            "{KOPF_ERLEDIGT}\ntermine={}\nkurse={}\nkurse_nyse={}\n",
            datum_feld(self.termine),
            datum_feld(self.kurse),
            datum_feld(self.kurse_nyse)
        )
    }

    /// Liest genau die eigene Form, auch die fruehere ohne `kurse_nyse` (`erledigt-v1`).
    ///
    /// # Errors
    /// Jede Abweichung.
    pub fn lies(text: &str) -> Result<Erledigt, String> {
        let mut z = text.lines();
        let v1 = match z.next() {
            Some(KOPF_ERLEDIGT) => false,
            Some(KOPF_ERLEDIGT_V1) => true,
            _ => return Err("Erledigt: Kopfzeile fehlt".into()),
        };
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
        let termine = datum("termine")?;
        let kurse = datum("kurse")?;
        let kurse_nyse = if v1 { None } else { datum("kurse_nyse")? };
        let e = Erledigt {
            termine,
            kurse,
            kurse_nyse,
        };
        let eigen = if v1 {
            format!(
                "{KOPF_ERLEDIGT_V1}\ntermine={}\nkurse={}\n",
                datum_feld(termine),
                datum_feld(kurse)
            )
        } else {
            e.als_text()
        };
        if eigen != text {
            return Err("Erledigt: nicht in eigener Form".into());
        }
        Ok(e)
    }

    /// Der Tag, an dem der Abruf dieser Boerse zuletzt gestartet wurde.
    pub fn kurse_von(&self, b: Boerse) -> Option<Datum> {
        match b {
            Boerse::Xetra => self.kurse,
            Boerse::Nyse => self.kurse_nyse,
        }
    }

    pub fn setze(&mut self, art: Art, tag: Datum) {
        match art {
            Art::Termine => self.termine = Some(tag),
            Art::KurseVon(Boerse::Xetra) => self.kurse = Some(tag),
            Art::KurseVon(Boerse::Nyse) => self.kurse_nyse = Some(tag),
            Art::Einrichten | Art::Kurse | Art::Papier | Art::UpdatePruefen | Art::Update => {}
        }
    }
}

/// Ein Handelstag des Zeitplans: ein Tag, an dem wenigstens eine Boerse der Liste handelt.
/// An Wochenenden und Feiertagen aller Boersen der Liste laeuft nichts, und nichts ist verpasst.
pub fn handelstag(d: Datum) -> bool {
    depot_kern::liste::handelstag(d)
}

/// Frankfurter Ortszeit eines Zeitpunkts, Tag und Stunde:Minute.
fn in_frankfurt(z: depot_kern::Zeitpunkt) -> Option<(Datum, (u8, u8))> {
    depot_lage::ortszeit(z).map(|(d, h, m, _, _)| (d, (h, m)))
}

/// Die Kursabrufe eines Tages: je Boerse aus `boersen`, die an diesem Tag handelt, ihre Uhrzeit
/// in Frankfurt, frueheste zuerst. Xetra zur Uhrzeit `kurse` des Plans, jede andere
/// `NACHLAUF_MIN` Minuten nach ihrem Schluss an diesem Tag. Faellt der Abruf nicht auf denselben
/// Tag in Frankfurt, entfaellt er (bei den Boersen der Liste kommt das nicht vor).
pub fn abrufe_fuer(plan: &Zeitplan, tag: Datum, boersen: &[Boerse]) -> Vec<(Boerse, (u8, u8))> {
    let mut a: Vec<(Boerse, (u8, u8))> = boersen
        .iter()
        .filter(|b| b.handelstag(tag))
        .filter_map(|&b| {
            if b == Boerse::Xetra {
                return Some((b, plan.kurse));
            }
            let nach = NACHLAUF_MIN.checked_mul(60)?;
            let zeit =
                depot_kern::Zeitpunkt::aus_sekunden(b.schluss(tag)?.sekunden().checked_add(nach)?)?;
            let (d, u) = in_frankfurt(zeit)?;
            (d == tag).then_some((b, u))
        })
        .collect();
    a.sort_by_key(|(b, u)| (*u, *b));
    a
}

/// Die Kursabrufe eines Tages fuer die Boersen der Liste.
pub fn abrufe(plan: &Zeitplan, tag: Datum) -> Vec<(Boerse, (u8, u8))> {
    abrufe_fuer(plan, tag, &depot_kern::liste::boersen())
}

/// Ist der Abruf dieser Boerse der letzte des Tages? Dann folgt der Papiertag.
pub fn letzter_abruf_fuer(plan: &Zeitplan, tag: Datum, b: Boerse, boersen: &[Boerse]) -> bool {
    abrufe_fuer(plan, tag, boersen).last().map(|(l, _)| *l) == Some(b)
}

/// Was der Zeitplan jetzt starten soll: `Termine` vor den Kursen, jeweils einmal je Tag und erst
/// ab seiner Uhrzeit; die Kurse je Boerse in der Reihenfolge ihrer Uhrzeit. `tag`, `zeit` sind
/// Frankfurter Ortszeit.
pub fn faellig(plan: &Zeitplan, erledigt: &Erledigt, tag: Datum, zeit: (u8, u8)) -> Option<Art> {
    faellig_fuer(plan, erledigt, tag, zeit, &depot_kern::liste::boersen())
}

/// Wie `faellig`, fuer diese Boersen.
pub fn faellig_fuer(
    plan: &Zeitplan,
    erledigt: &Erledigt,
    tag: Datum,
    zeit: (u8, u8),
    boersen: &[Boerse],
) -> Option<Art> {
    if !plan.an || !boersen.iter().any(|b| b.handelstag(tag)) {
        return None;
    }
    if zeit >= plan.termine && erledigt.termine != Some(tag) {
        return Some(Art::Termine);
    }
    abrufe_fuer(plan, tag, boersen)
        .into_iter()
        .find(|(b, u)| zeit >= *u && erledigt.kurse_von(*b) != Some(tag))
        .map(|(b, _)| Art::KurseVon(b))
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
    naechster_fuer(plan, erledigt, tag, &depot_kern::liste::boersen())
}

/// Wie `naechster`, fuer diese Boersen.
pub fn naechster_fuer(
    plan: &Zeitplan,
    erledigt: &Erledigt,
    tag: Datum,
    boersen: &[Boerse],
) -> Option<(Art, Datum, (u8, u8))> {
    if !plan.an {
        return None;
    }
    let mut d = tag;
    for _ in 0..8 {
        if boersen.iter().any(|b| b.handelstag(d)) {
            let mut kandidaten: Vec<((u8, u8), Art, bool)> =
                vec![(plan.termine, Art::Termine, erledigt.termine == Some(d))];
            kandidaten.extend(
                abrufe_fuer(plan, d, boersen)
                    .into_iter()
                    .map(|(b, u)| (u, Art::KurseVon(b), erledigt.kurse_von(b) == Some(d))),
            );
            kandidaten.sort_by_key(|(u, _, _)| *u);
            // Heute zaehlt auch eine verpasste Uhrzeit: Sie startet beim naechsten Blick.
            if let Some((u, art, _)) = kandidaten.into_iter().find(|(_, _, schon)| !schon) {
                return Some((art, d, u));
            }
        }
        d = tag_danach(d)?;
    }
    None
}
