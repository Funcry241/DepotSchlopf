//! Der Zeitplan: wann der Dienst an Handelstagen (Montag bis Freitag) von selbst Termine und
//! Kurse holt und danach den Papiertag startet. Uhrzeiten in Frankfurter Ortszeit.
//!
//! **Morgenlauf:** Die Börsen werden ab 07:30 Uhr nacheinander abgerufen, sofern mindestens
//! eine davon an diesem Tag handelt. Danach folgt bis zum ersten Handelsschluss der Papiertag. Jede Börse
//! hat ihren eigenen Bezugstag. Alte Abendzeiten halten Kursabruf und Papiertag an.
//!
//! **Einmal je Tag:** Was der Zeitplan an einem Tag gestartet hat, steht in
//! `daten/dienst_erledigt.txt`, bevor der Auftrag beginnt. Ein Neustart des Dienstes startet es
//! an diesem Tag nicht noch einmal. Der Papiertag selbst laeuft ohnehin nur einmal je
//! Handelstag (Journal).

use depot_kern::{Boerse, Datum, Zeitpunkt};

use crate::werkbank::Art;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VorbereitungsPlan {
    #[default]
    Aus,
    An,
}
impl VorbereitungsPlan {
    pub fn aktiv(self) -> bool {
        self == Self::An
    }
    pub fn aus_bool(an: bool) -> Self {
        if an { Self::An } else { Self::Aus }
    }
}

/// Windows-Hinweis bei „Eingriff nötig“ (#122). Ab Werk an, weil er nur anzeigt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HinweisPlan {
    #[default]
    An,
    Aus,
}
impl HinweisPlan {
    pub fn aktiv(self) -> bool {
        self == Self::An
    }
    pub fn aus_bool(an: bool) -> Self {
        if an { Self::An } else { Self::Aus }
    }
}

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
    /// Zusaetzliche kostenlose RSS-Abrufe tagsueber. Altinstallationen bleiben aus.
    pub nachrichten_an: bool,
    /// Einmal je boersenfreiem Tag, innerhalb des bestehenden KI-Budgets.
    pub vorbereitung: VorbereitungsPlan,
    /// Eine Datei ohne das Feld bedeutet an.
    pub hinweis: HinweisPlan,
}

impl Default for Zeitplan {
    /// Termine 07:20, Kurse 07:30, dann der Papiertag. Die Automatik ist zunächst
    /// **aus**: Der Betrieb von selbst beginnt erst, wenn der Mensch ihn in der Einrichtung
    /// bewusst einschaltet. Ein erster neugieriger Start loest so keinen KI-Aufruf aus.
    fn default() -> Zeitplan {
        Zeitplan {
            an: false,
            termine: (7, 20),
            kurse: (7, 30),
            papier_danach: true,
            nachrichten_an: false,
            vorbereitung: VorbereitungsPlan::Aus,
            hinweis: HinweisPlan::An,
        }
    }
}

const KOPF: &str = "zeitplan-v1";
const KOPF_ERLEDIGT: &str = "erledigt-v4";
const KOPF_ERLEDIGT_V3: &str = "erledigt-v3";
/// Die fruehere Form ohne `kurse_ok` und `papier`.
const KOPF_ERLEDIGT_V2: &str = "erledigt-v2";
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
        let basis = format!(
            "{KOPF}\nan={}\ntermine={}\nkurse={}\npapier_danach={}\n",
            ja_nein(self.an),
            uhrzeit_text(self.termine),
            uhrzeit_text(self.kurse),
            ja_nein(self.papier_danach)
        );
        let mut text = basis;
        if self.nachrichten_an {
            text.push_str("nachrichten_an=ja\n");
        }
        if self.vorbereitung.aktiv() {
            text.push_str("vorbereitung_an=ja\n");
        }
        if !self.hinweis.aktiv() {
            text.push_str("hinweis_aus=ja\n");
        }
        text
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
        let mut nachrichten_an = false;
        let mut vorbereitung_an = false;
        let mut hinweis_aus = false;
        for zeile in z {
            match zeile {
                "nachrichten_an=ja" if !nachrichten_an => nachrichten_an = true,
                "vorbereitung_an=ja" if !vorbereitung_an => vorbereitung_an = true,
                "hinweis_aus=ja" if !hinweis_aus => hinweis_aus = true,
                _ => return Err("Zeitplan: Einstellung ungültig oder doppelt".into()),
            }
        }
        if vorbereitung_an && !nachrichten_an {
            return Err("Vorbereitung braucht Schlagzeilen".into());
        }
        let plan = Zeitplan {
            an,
            termine,
            kurse,
            papier_danach,
            nachrichten_an,
            vorbereitung: VorbereitungsPlan::aus_bool(vorbereitung_an),
            hinweis: HinweisPlan::aus_bool(!hinweis_aus),
        };
        if plan.als_text() != text {
            return Err("Zeitplan: nicht in eigener Form".into());
        }
        Ok(plan)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbrufAusgang {
    Laeuft,
    Ok,
    Voruebergehend,
    Fehler,
    Abgebrochen,
}

impl AbrufAusgang {
    pub(crate) fn aus_exitcode(code: Option<i32>) -> Self {
        match code {
            Some(0) => Self::Ok,
            Some(3) => Self::Voruebergehend,
            None => Self::Abgebrochen,
            Some(_) => Self::Fehler,
        }
    }

    fn text(self) -> &'static str {
        match self {
            Self::Laeuft => "laeuft",
            Self::Ok => "ok",
            Self::Voruebergehend => "voruebergehend",
            Self::Fehler => "fehler",
            Self::Abgebrochen => "abgebrochen",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Abruf {
    pub anker: Zeitpunkt,
    pub versuche: u8,
    pub ausgang: AbrufAusgang,
}

impl Abruf {
    fn lies(text: &str) -> Option<Self> {
        let [anker, versuche, ausgang] = text.split(';').collect::<Vec<_>>().try_into().ok()?;
        let versuche = versuche.parse().ok()?;
        if !(1..=3).contains(&versuche) {
            return None;
        }
        let ausgang = [
            AbrufAusgang::Laeuft,
            AbrufAusgang::Ok,
            AbrufAusgang::Voruebergehend,
            AbrufAusgang::Fehler,
            AbrufAusgang::Abgebrochen,
        ]
        .into_iter()
        .find(|a| a.text() == ausgang)?;
        Some(Self {
            anker: Zeitpunkt::aus_sekunden(anker.parse().ok()?)?,
            versuche,
            ausgang,
        })
    }

    fn text(self) -> String {
        format!(
            "{};{};{}",
            self.anker.sekunden(),
            self.versuche,
            self.ausgang.text()
        )
    }

    fn wiederholung(self, schluss: Zeitpunkt) -> Option<Zeitpunkt> {
        if !matches!(
            self.ausgang,
            AbrufAusgang::Voruebergehend | AbrufAusgang::Abgebrochen
        ) {
            return None;
        }
        let pause = match self.versuche {
            1 => 30 * 60,
            2 => 90 * 60,
            _ => return None,
        };
        let termin = Zeitpunkt::aus_sekunden(self.anker.sekunden().checked_add(pause)?)?;
        let frist = Zeitpunkt::aus_sekunden(schluss.sekunden().checked_sub(30 * 60)?)?;
        (termin < frist).then_some(termin)
    }
}

/// Der dauerhaft vermerkte Stand des Zeitplans.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Erledigt {
    pub termine: Option<Datum>,
    pub kurse: Option<Datum>,
    pub kurse_nyse: Option<Datum>,
    pub papier: Option<Datum>,
    pub xetra_abruf: Option<Abruf>,
    pub nyse_abruf: Option<Abruf>,
}

fn datum_feld(d: Option<Datum>) -> String {
    d.map(|d| d.to_string()).unwrap_or_default()
}

impl Erledigt {
    pub fn als_text(&self) -> String {
        format!(
            "{KOPF_ERLEDIGT}\ntermine={}\nkurse={}\nkurse_nyse={}\npapier={}\nxetra_abruf={}\nnyse_abruf={}\n",
            datum_feld(self.termine),
            datum_feld(self.kurse),
            datum_feld(self.kurse_nyse),
            datum_feld(self.papier),
            self.xetra_abruf.map(Abruf::text).unwrap_or_default(),
            self.nyse_abruf.map(Abruf::text).unwrap_or_default()
        )
    }

    /// Liest die eigene Form und die älteren Vermerke. Ein alter globaler Erfolg gilt
    /// als kein Erfolgsbeleg für eine einzelne Börse.
    ///
    /// # Errors
    /// Der Vermerk weicht von seiner kanonischen Form ab.
    pub fn lies(text: &str) -> Result<Self, String> {
        let mut zeilen = text.lines();
        let form = match zeilen.next() {
            Some(KOPF_ERLEDIGT) => 4,
            Some(KOPF_ERLEDIGT_V3) => 3,
            Some(KOPF_ERLEDIGT_V2) => 2,
            Some(KOPF_ERLEDIGT_V1) => 1,
            _ => return Err("Erledigt: Kopfzeile fehlt".into()),
        };
        let mut feld = |name: &str| {
            zeilen
                .next()
                .and_then(|l| l.strip_prefix(name))
                .and_then(|l| l.strip_prefix('='))
                .ok_or_else(|| format!("Erledigt: {name} erwartet"))
        };
        let mut datum = |name| {
            let w = feld(name)?;
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
        let kurse_nyse = if form >= 2 {
            datum("kurse_nyse")?
        } else {
            None
        };
        let alt_ok = if form == 3 { datum("kurse_ok")? } else { None };
        let papier = if form >= 3 { datum("papier")? } else { None };
        let mut abruf = |name, tag| {
            let w = feld(name)?;
            if w.is_empty() {
                Ok(None)
            } else {
                Abruf::lies(w)
                    .filter(|a| a.anker.frankfurt_tag() == tag && tag.is_some())
                    .map(Some)
                    .ok_or_else(|| format!("Erledigt: {name} unlesbar"))
            }
        };
        let (xetra_abruf, nyse_abruf) = if form == 4 {
            (
                abruf("xetra_abruf", kurse)?,
                abruf("nyse_abruf", kurse_nyse)?,
            )
        } else {
            (None, None)
        };
        let e = Self {
            termine,
            kurse,
            kurse_nyse,
            papier,
            xetra_abruf,
            nyse_abruf,
        };
        let eigen = match form {
            1 => format!(
                "{KOPF_ERLEDIGT_V1}\ntermine={}\nkurse={}\n",
                datum_feld(termine),
                datum_feld(kurse)
            ),
            2 => format!(
                "{KOPF_ERLEDIGT_V2}\ntermine={}\nkurse={}\nkurse_nyse={}\n",
                datum_feld(termine),
                datum_feld(kurse),
                datum_feld(kurse_nyse)
            ),
            3 => format!(
                "{KOPF_ERLEDIGT_V3}\ntermine={}\nkurse={}\nkurse_nyse={}\nkurse_ok={}\npapier={}\n",
                datum_feld(termine),
                datum_feld(kurse),
                datum_feld(kurse_nyse),
                datum_feld(alt_ok),
                datum_feld(papier)
            ),
            _ => e.als_text(),
        };
        if eigen != text {
            return Err("Erledigt: nicht in eigener Form".into());
        }
        Ok(e)
    }

    pub fn kurse_von(&self, b: Boerse) -> Option<Datum> {
        match b {
            Boerse::Xetra => self.kurse,
            Boerse::Nyse => self.kurse_nyse,
        }
    }

    pub fn abruf(&self, b: Boerse) -> Option<Abruf> {
        match b {
            Boerse::Xetra => self.xetra_abruf,
            Boerse::Nyse => self.nyse_abruf,
        }
    }

    pub fn erfolgreich(&self, b: Boerse, tag: Datum) -> bool {
        self.kurse_von(b) == Some(tag)
            && self.abruf(b).is_some_and(|a| a.ausgang == AbrufAusgang::Ok)
    }

    fn abruf_mut(&mut self, b: Boerse) -> &mut Option<Abruf> {
        match b {
            Boerse::Xetra => &mut self.xetra_abruf,
            Boerse::Nyse => &mut self.nyse_abruf,
        }
    }

    pub(crate) fn abruf_start(&mut self, b: Boerse, tag: Datum, jetzt: Zeitpunkt) {
        let vorher = self
            .abruf(b)
            .filter(|a| a.anker.frankfurt_tag() == Some(tag));
        self.setze(Art::KurseVon(b), tag);
        *self.abruf_mut(b) = Some(Abruf {
            anker: vorher.map_or(jetzt, |a| a.anker),
            versuche: vorher.map_or(1, |a| a.versuche.saturating_add(1)),
            ausgang: AbrufAusgang::Laeuft,
        });
    }

    pub(crate) fn abruf_ende(&mut self, b: Boerse, ausgang: AbrufAusgang) {
        if let Some(a) = self.abruf_mut(b).as_mut() {
            a.ausgang = ausgang;
        }
    }

    pub fn setze(&mut self, art: Art, tag: Datum) {
        match art {
            Art::Termine => self.termine = Some(tag),
            Art::KurseVon(Boerse::Xetra) => self.kurse = Some(tag),
            Art::KurseVon(Boerse::Nyse) => self.kurse_nyse = Some(tag),
            Art::Papier => self.papier = Some(tag),
            _ => {}
        }
    }
}

/// Ein Handelstag des Zeitplans: ein Tag, an dem wenigstens eine Boerse der Liste handelt.
/// An Wochenenden und Feiertagen aller Boersen der Liste laeuft nichts, und nichts ist verpasst.
pub fn handelstag(d: Datum) -> bool {
    depot_kern::liste::handelstag(d)
}

// Zeitpläne aus dem Morgenfenster von #140 behalten ihre Bedeutung; die Laufzeitgrenze
// ergibt sich unabhängig davon aus den Börsenschlüssen.
pub(crate) fn morgenplan(plan: &Zeitplan) -> bool {
    ((7, 0)..=(8, 30)).contains(&plan.kurse)
}

pub(crate) fn morgen_abgelaufen(
    plan: &Zeitplan,
    tag: Datum,
    zeit: (u8, u8),
    boersen: &[Boerse],
) -> bool {
    morgenplan(plan)
        && Zeitpunkt::frankfurt(tag, zeit.0, zeit.1, 0)
            .zip(depot_kern::liste::erster_schluss(tag, boersen))
            .is_some_and(|(jetzt, schluss)| jetzt >= schluss)
}

/// Kursabrufe für die Börsen mit Bezugstag, sobald wenigstens eine Börse handelt.
pub fn abrufe_fuer(plan: &Zeitplan, tag: Datum, boersen: &[Boerse]) -> Vec<(Boerse, (u8, u8))> {
    if !morgenplan(plan) || !boersen.iter().any(|b| b.handelstag(tag)) {
        return Vec::new();
    }
    boersen
        .iter()
        .filter(|b| b.bezugstag(tag).is_some())
        .map(|b| (*b, plan.kurse))
        .collect()
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
    faellig_mit_bestand(plan, erledigt, tag, zeit, boersen, boersen)
}

pub(crate) fn faellig_mit_bestand(
    plan: &Zeitplan,
    erledigt: &Erledigt,
    tag: Datum,
    zeit: (u8, u8),
    boersen: &[Boerse],
    bestand: &[Boerse],
) -> Option<Art> {
    if !plan.an || !boersen.iter().any(|b| b.handelstag(tag)) {
        return None;
    }
    if zeit >= plan.termine && erledigt.termine != Some(tag) {
        return Some(Art::Termine);
    }
    if !morgenplan(plan) || morgen_abgelaufen(plan, tag, zeit, boersen) {
        return None;
    }
    let abrufe = abrufe_fuer(plan, tag, boersen);
    if let Some((b, _)) = abrufe
        .iter()
        .find(|(b, u)| zeit >= *u && erledigt.kurse_von(*b) != Some(tag))
    {
        return Some(Art::KurseVon(*b));
    }
    let jetzt = Zeitpunkt::frankfurt(tag, zeit.0, zeit.1, 0)?;
    let schluss = depot_kern::liste::erster_schluss(tag, boersen)?;
    let frist = Zeitpunkt::aus_sekunden(schluss.sekunden().checked_sub(30 * 60)?)?;
    if jetzt < frist
        && let Some((b, _)) = abrufe.iter().find(|(b, _)| {
            erledigt.kurse_von(*b) == Some(tag)
                && erledigt
                    .abruf(*b)
                    .and_then(|a| a.wiederholung(schluss))
                    .is_some_and(|t| t <= jetzt)
        })
    {
        return Some(Art::KurseVon(*b));
    }
    (plan.papier_danach
        && erledigt.papier != Some(tag)
        && abrufe
            .iter()
            .all(|(b, _)| erledigt.kurse_von(*b) == Some(tag))
        && abrufe.iter().any(|(b, _)| erledigt.erfolgreich(*b, tag))
        && bestand.iter().all(|b| erledigt.erfolgreich(*b, tag)))
    .then_some(Art::Papier)
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
        // Heute zaehlt auch eine verpasste Uhrzeit: Sie startet beim naechsten Blick.
        if let Some((u, art, _)) = geplante_abrufe(*plan, erledigt, d, boersen)
            .into_iter()
            .find(|(_, _, schon)| !schon)
        {
            return Some((art, d, u));
        }
        d = tag_danach(d)?;
    }
    None
}

/// Uhrzeiten und Vormerkungen der Börsenläufe, in der Reihenfolge ihrer Sollzeit.
pub(crate) fn geplante_abrufe(
    plan: Zeitplan,
    erledigt: &Erledigt,
    tag: Datum,
    boersen: &[Boerse],
) -> Vec<((u8, u8), Art, bool)> {
    if !boersen.iter().any(|b| b.handelstag(tag)) {
        return Vec::new();
    }
    let mut kandidaten = vec![(plan.termine, Art::Termine, erledigt.termine == Some(tag))];
    kandidaten.extend(
        abrufe_fuer(&plan, tag, boersen)
            .into_iter()
            .map(|(b, u)| (u, Art::KurseVon(b), erledigt.kurse_von(b) == Some(tag))),
    );
    kandidaten.sort_by_key(|(u, _, _)| *u);
    kandidaten
}

#[cfg(test)]
mod tests {
    use super::*;

    fn donnerstag() -> Result<Datum, String> {
        Datum::neu(2026, 11, 5).ok_or_else(|| "Donnerstag fehlt".into())
    }

    fn zustand(tag: Datum, ausgang: AbrufAusgang) -> Result<Erledigt, String> {
        let anker = Zeitpunkt::frankfurt(tag, 7, 30, 0).ok_or("Anker fehlt")?;
        let mut e = Erledigt {
            termine: Some(tag),
            ..Erledigt::default()
        };
        e.abruf_start(Boerse::Xetra, tag, anker);
        e.abruf_ende(Boerse::Xetra, ausgang);
        Ok(e)
    }

    fn geplant(e: &Erledigt, tag: Datum, zeit: (u8, u8)) -> Option<Art> {
        let plan = Zeitplan {
            an: true,
            ..Zeitplan::default()
        };
        faellig_fuer(&plan, e, tag, zeit, &[Boerse::Xetra])
    }

    #[test]
    fn neustart_zaehlt_versuche_weiter() -> Result<(), String> {
        let tag = donnerstag()?;
        let e = zustand(tag, AbrufAusgang::Voruebergehend)?;
        let mut e = Erledigt::lies(&e.als_text())?;
        assert_eq!(geplant(&e, tag, (7, 59)), None);
        assert_eq!(geplant(&e, tag, (8, 0)), Some(Art::KurseVon(Boerse::Xetra)));
        e.abruf_start(
            Boerse::Xetra,
            tag,
            Zeitpunkt::frankfurt(tag, 8, 15, 0).ok_or("Start fehlt")?,
        );
        e.abruf_ende(Boerse::Xetra, AbrufAusgang::Voruebergehend);
        let mut e = Erledigt::lies(&e.als_text())?;
        assert_eq!(geplant(&e, tag, (8, 59)), None);
        assert_eq!(geplant(&e, tag, (9, 0)), Some(Art::KurseVon(Boerse::Xetra)));
        assert_eq!(geplant(&e, tag, (17, 5)), None);
        e.abruf_start(
            Boerse::Xetra,
            tag,
            Zeitpunkt::frankfurt(tag, 9, 0, 0).ok_or("Zuletzt fehlt")?,
        );
        e.abruf_ende(Boerse::Xetra, AbrufAusgang::Voruebergehend);
        assert_eq!(geplant(&e, tag, (9, 1)), None);
        Ok(())
    }

    #[test]
    fn abbruch_ohne_code_wiederholt() -> Result<(), String> {
        let tag = donnerstag()?;
        let e = zustand(tag, AbrufAusgang::aus_exitcode(None))?;
        assert_eq!(
            geplant(&e, tag, (8, 25)),
            Some(Art::KurseVon(Boerse::Xetra))
        );
        Ok(())
    }

    #[test]
    fn exit_1_keine_wiederholung() -> Result<(), String> {
        let tag = donnerstag()?;
        let e = zustand(tag, AbrufAusgang::aus_exitcode(Some(1)))?;
        assert_eq!(geplant(&e, tag, (9, 0)), None);
        Ok(())
    }

    #[test]
    fn erfolg_nur_beim_bestand_erforderlich() -> Result<(), String> {
        let tag = donnerstag()?;
        let mut e = zustand(tag, AbrufAusgang::Ok)?;
        e.abruf_start(
            Boerse::Nyse,
            tag,
            Zeitpunkt::frankfurt(tag, 7, 31, 0).ok_or("NYSE fehlt")?,
        );
        e.abruf_ende(Boerse::Nyse, AbrufAusgang::Fehler);
        let plan = Zeitplan {
            an: true,
            ..Zeitplan::default()
        };
        assert_eq!(
            faellig_mit_bestand(&plan, &e, tag, (7, 35), &Boerse::ALLE, &[Boerse::Xetra]),
            Some(Art::Papier)
        );
        assert_eq!(
            faellig_mit_bestand(&plan, &e, tag, (7, 35), &Boerse::ALLE, &[Boerse::Nyse]),
            None
        );
        Ok(())
    }

    #[test]
    fn erledigt_v3_lesbar() -> Result<(), String> {
        let tag = donnerstag()?;
        let text = format!(
            "erledigt-v3\ntermine={tag}\nkurse={tag}\nkurse_nyse={tag}\nkurse_ok={tag}\npapier={tag}\n"
        );
        let e = Erledigt::lies(&text)?;
        assert_eq!(e.papier, Some(tag));
        assert!(Boerse::ALLE.iter().all(|b| !e.erfolgreich(*b, tag)));
        Ok(())
    }
}
