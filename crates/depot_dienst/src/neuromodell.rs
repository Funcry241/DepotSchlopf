//! Lokaler, nachträglicher Neuro2026-Vergleich. Die Rangfolge fließt nur in den Bericht.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{ErrorKind, Read as _},
    path::Path,
};

use depot_journal::neurofrage::NeuroUrteilswert;
use depot_kern::{
    Datum,
    abdruck::sha256_hex,
    json::{self, Json, zitiere},
};
use depot_lage::relevanzmessung::{self, Beleg, Urteil, Vergleich};

use crate::{
    neurodaten::{SCHEMA, Zeile},
    werkbank::schreibe_ganz,
};

const FORMAT: &str = "depot-neuro-worker-v1";
const VERSION: &str = "0.1.16";
const BERICHT: &str = "neuro2026-beobachtung.json";
const MAX_BYTES: usize = 4_000_000;
const MAX_PAARE: usize = 8192;

#[derive(Debug)]
pub(crate) struct ModellBericht {
    pub modellversion: Option<String>,
    pub modellabdruck: Option<String>,
    pub lernkurve: Option<Lernkurve>,
    pub stand: String,
    pub paare: usize,
    pub urteile: usize,
    pub ki_konsens: bool,
    pub trainingstage: usize,
    pub prueftage: usize,
    pub gruppen: Vec<Ranggruppe>,
}

#[derive(Debug)]
pub(crate) struct Ranggruppe {
    pub tag: String,
    pub berater: String,
    pub isin: String,
    pub these: String,
    pub belege: Vec<Rangbeleg>,
}

#[derive(Debug)]
pub(crate) struct Rangbeleg {
    pub fall_id: String,
    pub referenz: String,
    pub titel: String,
    pub quelle: String,
    pub grundrang: usize,
    pub modellrang: usize,
    pub wert: String,
    pub merkmale: [u16; 4],
}

fn text<'a>(j: &'a Json, name: &str) -> Option<&'a str> {
    j.feld(name)?.text()
}

fn anzahl(j: &Json, name: &str) -> Option<usize> {
    usize::try_from(j.feld(name)?.anzahl()?).ok()
}

fn merkmale(werte: [u16; 4]) -> String {
    let [bezug, termin, woerter, alter] = werte.map(|x| format!("{}.{:03}", x / 1000, x % 1000));
    format!("[{bezug},{termin},{woerter},{alter}]")
}

impl Rangbeleg {
    fn lies(j: &Json) -> Option<Self> {
        let [bezug, termin, woerter, alter] = j.feld("merkmale")?.liste()? else {
            return None;
        };
        let merkmal = |wert: &Json| u16::try_from(wert.anzahl()?).ok().filter(|x| *x <= 1000);
        let wert = text(j, "wert")?;
        score(&Json::Zahl(wert.to_owned()))?;
        Some(Self {
            fall_id: text(j, "fall_id")
                .filter(|s| crate::belegurteile::kennung_gueltig(s))?
                .into(),
            referenz: text(j, "referenz")?.into(),
            titel: text(j, "titel")?.into(),
            quelle: text(j, "quelle")?.into(),
            grundrang: anzahl(j, "grundrang")?,
            modellrang: anzahl(j, "modellrang")?,
            wert: wert.into(),
            merkmale: [
                merkmal(bezug)?,
                merkmal(termin)?,
                merkmal(woerter)?,
                merkmal(alter)?,
            ],
        })
    }

    fn json(&self) -> String {
        let [a, b, c, d] = self.merkmale;
        format!(
            "{{\"fall_id\":{},\"referenz\":{},\"titel\":{},\"quelle\":{},\"grundrang\":{},\
             \"modellrang\":{},\"wert\":{},\"merkmale\":[{a},{b},{c},{d}]}}",
            zitiere(&self.fall_id),
            zitiere(&self.referenz),
            zitiere(&self.titel),
            zitiere(&self.quelle),
            self.grundrang,
            self.modellrang,
            zitiere(&self.wert)
        )
    }
}

impl Ranggruppe {
    fn lies(j: &Json) -> Option<Self> {
        Some(Self {
            tag: text(j, "tag")?.into(),
            berater: text(j, "berater")?.into(),
            isin: text(j, "isin")?.into(),
            these: text(j, "these")?.into(),
            belege: j
                .feld("belege")?
                .liste()?
                .iter()
                .map(Rangbeleg::lies)
                .collect::<Option<_>>()?,
        })
    }

    fn json(&self) -> String {
        let belege = self
            .belege
            .iter()
            .map(Rangbeleg::json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"tag\":{},\"berater\":{},\"isin\":{},\"these\":{},\"belege\":[{belege}]}}",
            zitiere(&self.tag),
            zitiere(&self.berater),
            zitiere(&self.isin),
            zitiere(&self.these)
        )
    }
}

#[derive(Debug)]
pub(crate) struct Lernkurve {
    schichten: [usize; 4],
    paare: usize,
    anfang: f64,
    ende: f64,
    fehler: Vec<f64>,
}

impl Lernkurve {
    fn lies(j: &Json) -> Option<Self> {
        let [eingang, erste, zweite, ausgang] = j.feld("schichten")?.liste()? else {
            return None;
        };
        let breite = erste.anzahl().filter(|b| (2..=32).contains(b))?;
        if eingang.anzahl()? != 4 || zweite.anzahl()? != breite || ausgang.anzahl()? != 1 {
            return None;
        }
        let breite = usize::try_from(breite).ok()?;
        let fehler = j.feld("fehler")?.liste()?;
        if fehler.is_empty() || fehler.len() > 10_000 {
            return None;
        }
        Some(Self {
            schichten: [4, breite, breite, 1],
            paare: anzahl(j, "paare").filter(|n| (1..=MAX_PAARE).contains(n))?,
            anfang: verlust(j.feld("anfang")?)?,
            ende: verlust(j.feld("ende")?)?,
            fehler: fehler.iter().map(verlust).collect::<Option<_>>()?,
        })
    }

    pub fn json(&self) -> String {
        let [a, b, c, d] = self.schichten;
        let fehler = self
            .fehler
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"schichten\":[{a},{b},{c},{d}],\"paare\":{},\"anfang\":{},\"ende\":{},\"fehler\":[{fehler}]}}",
            self.paare, self.anfang, self.ende
        )
    }
}

fn verlust(j: &Json) -> Option<f64> {
    let (_, wert) = score(j)?;
    (0.0..=f64::from(f32::MAX)).contains(&wert).then_some(wert)
}

impl ModellBericht {
    fn lies(j: &Json) -> Option<Self> {
        if text(j, "schema")? != SCHEMA {
            return None;
        }
        let modellversion = match j.feld("modellversion")? {
            Json::Null => None,
            Json::Text(s) if s == VERSION => Some(s.clone()),
            _ => return None,
        };
        let modellabdruck = match j.feld("modellabdruck")? {
            Json::Null if modellversion.is_none() => None,
            Json::Text(s) if modellversion.is_some() && sha_gueltig(s) => Some(s.clone()),
            _ => return None,
        };
        let gruppen = j
            .feld("gruppen")?
            .liste()?
            .iter()
            .map(Ranggruppe::lies)
            .collect::<Option<Vec<_>>>()?;
        if modellversion.is_none() && !gruppen.is_empty() {
            return None;
        }
        Some(Self {
            modellversion,
            modellabdruck,
            lernkurve: match j.feld("lernkurve") {
                None | Some(Json::Null) => None,
                Some(kurve) => Some(Lernkurve::lies(kurve)?),
            },
            stand: text(j, "stand")?.into(),
            paare: anzahl(j, "paare")?,
            urteile: anzahl(j, "urteile")?,
            ki_konsens: matches!(j.feld("ki_konsens"), Some(Json::Bool(true))),
            trainingstage: anzahl(j, "trainingstage")?,
            prueftage: anzahl(j, "prueftage")?,
            gruppen,
        })
    }

    fn speichere(&self, daten: &Path) -> Result<(), String> {
        let gruppen = self
            .gruppen
            .iter()
            .map(Ranggruppe::json)
            .collect::<Vec<_>>()
            .join(",");
        let version = self
            .modellversion
            .as_deref()
            .map_or_else(|| "null".into(), zitiere);
        let abdruck = self
            .modellabdruck
            .as_deref()
            .map_or_else(|| "null".into(), zitiere);
        let kurve = self
            .lernkurve
            .as_ref()
            .map_or_else(|| "null".into(), Lernkurve::json);
        let inhalt = format!(
            "{{\"schema\":\"{SCHEMA}\",\"modellversion\":{version},\"modellabdruck\":{abdruck},\"stand\":{},\
             \"paare\":{},\"urteile\":{},\"ki_konsens\":{},\"trainingstage\":{},\"prueftage\":{},\
             \"lernkurve\":{kurve},\"gruppen\":[{gruppen}]}}\n",
            zitiere(&self.stand),
            self.paare,
            self.urteile,
            self.ki_konsens,
            self.trainingstage,
            self.prueftage
        );
        if inhalt.len() > MAX_BYTES {
            return Err("Der Neuro2026-Bericht ist zu groß.".into());
        }
        schreibe_ganz(&daten.join(BERICHT), &inhalt)
    }
}

fn lese_datei(pfad: &Path) -> Result<Option<String>, String> {
    let datei = match File::open(pfad) {
        Ok(d) => d,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let mut inhalt = String::new();
    datei
        .take(u64::try_from(MAX_BYTES.saturating_add(1)).map_err(|e| e.to_string())?)
        .read_to_string(&mut inhalt)
        .map_err(|e| e.to_string())?;
    if inhalt.len() > MAX_BYTES {
        return Err("Die Neuro2026-Datei ist zu groß.".into());
    }
    Ok(Some(inhalt))
}

fn lese_json(pfad: &Path) -> Result<Option<Json>, String> {
    lese_datei(pfad)?.as_deref().map(json::lies).transpose()
}

pub(crate) fn ansicht(daten: &Path) -> Result<Option<ModellBericht>, String> {
    lese_json(&daten.join(BERICHT))?
        .map(|j| ModellBericht::lies(&j).ok_or_else(|| "Ungültiger Neuro2026-Bericht.".into()))
        .transpose()
}

pub(crate) fn pruefpool(daten: &Path, tag: Datum) -> Result<BTreeSet<String>, String> {
    let mut pool = BTreeSet::new();
    if let Some(bericht) = ansicht(daten)? {
        let tag = tag.to_string();
        for gruppe in bericht
            .gruppen
            .iter()
            .filter(|g| g.tag == tag && g.belege.len() > 3)
        {
            let oben = |b: &Rangbeleg| b.modellrang <= 3 || b.grundrang <= 3;
            pool.extend(
                gruppe
                    .belege
                    .iter()
                    .filter(|b| oben(b))
                    .map(|b| b.fall_id.clone()),
            );
            if let Some(kontrolle) = gruppe
                .belege
                .iter()
                .filter(|b| !oben(b))
                .min_by_key(|b| &b.fall_id)
            {
                pool.insert(kontrolle.fall_id.clone());
            }
        }
    }
    Ok(pool)
}

fn score(j: &Json) -> Option<(&str, f64)> {
    let Json::Zahl(wert) = j else { return None };
    let zahl: f64 = wert.parse().ok()?;
    zahl.is_finite().then_some((wert, zahl))
}

fn austausch(
    ordner: &Path,
    op: &str,
    zeilen: &[String],
    feld_eingabe_sha: &str,
) -> Result<Option<Json>, String> {
    let eingabe = format!(
        "{{\"format\":\"{FORMAT}\",\"schema\":\"{SCHEMA}\",\"op\":\"{op}\",\"rows\":[{}]}}",
        zeilen.join(",")
    );
    if eingabe.len() > MAX_BYTES {
        return Err("Zu viele Neuro2026-Eingaben für einen Lauf.".into());
    }
    schreibe_ganz(&ordner.join(format!("{op}-eingabe.json")), &eingabe)?;
    let Some(antwort) = lese_json(&ordner.join(format!("{op}-antwort.json")))? else {
        return Ok(None);
    };
    let erwartet = sha256_hex(eingabe.as_bytes());
    if antwort.doppelte_felder()
        || text(&antwort, "format") != Some(FORMAT)
        || text(&antwort, "op") != Some(op)
        || text(&antwort, "schema") != Some(SCHEMA)
        || text(&antwort, "neuro2026_version") != Some(VERSION)
        || text(&antwort, feld_eingabe_sha) != Some(&erwartet)
    {
        return Err("Die Antwort gehört zu einer anderen Neuro2026-Eingabe.".into());
    }
    Ok(Some(antwort))
}

fn sha_gueltig(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn modellabdruck(j: &Json) -> Result<&str, String> {
    text(j, "model_manifest_sha256")
        .filter(|s| sha_gueltig(s))
        .ok_or_else(|| "Neuro2026 hat keinen gültigen Modellabdruck gemeldet.".into())
}

fn ki_relevanz(z: &Zeile) -> Option<Urteil> {
    match z.ki_urteil? {
        NeuroUrteilswert::KeinBezug => Some(Urteil::KeinBezug),
        NeuroUrteilswert::Staerkt | NeuroUrteilswert::Schwaecht => Some(Urteil::Bezug),
        NeuroUrteilswert::Offen => None,
    }
}

/// Beginn des Prüfzeitraums nach drei vollständigen Handelstagen Abstand.
fn pruefstart(mut tag: Datum) -> Option<Datum> {
    let mut aus = 0_u8;
    while aus < 3 {
        tag = tag.plus_tage(1)?;
        if depot_kern::liste::handelstag(tag) {
            aus = aus.saturating_add(1);
        }
    }
    tag.plus_tage(1)
}

fn meldung(z: &Zeile) -> String {
    z.fall
        .beleg
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

type Rangwerte = BTreeMap<String, (String, f64)>;

fn rangwerte(antwort: &Json, erwartet: &[&Zeile]) -> Result<Rangwerte, String> {
    let scores = antwort
        .feld("scores")
        .and_then(Json::liste)
        .ok_or("Modellwerte fehlen.")?;
    let mut werte = BTreeMap::new();
    for eintrag in scores {
        let id = text(eintrag, "id").ok_or("Fall-ID im Modellwert fehlt.")?;
        let (wert, zahl) = eintrag
            .feld("score")
            .and_then(score)
            .ok_or("Ungültiger Modellwert.")?;
        if eintrag.doppelte_felder() || werte.insert(id.into(), (wert.into(), zahl)).is_some() {
            return Err("Doppelter Fall im Modelllauf.".into());
        }
    }
    if werte.len() != erwartet.len()
        || erwartet
            .iter()
            .any(|z| !werte.contains_key(&z.fall.kennung))
    {
        return Err("Die Modellwerte decken eine andere Kandidatenmenge ab.".into());
    }
    Ok(werte)
}

type Gruppen<'a> = BTreeMap<(Datum, String, String, String), Vec<(&'a Zeile, &'a (String, f64))>>;

#[derive(Default)]
struct Vergleichszahlen {
    zu_kurz: usize,
    mit_auswahl: usize,
    verglichen: usize,
    grund_treffer: usize,
    modell_treffer: usize,
    vollstaendig: usize,
    grund_ndcg: u64,
    modell_ndcg: u64,
    fehlende_top3: BTreeSet<String>,
}

impl Vergleichszahlen {
    fn erfasse(&mut self, zeilen: &[&Zeile], modell: &[usize], grund: &[usize]) {
        if zeilen.len() <= 3 {
            self.zu_kurz = self.zu_kurz.saturating_add(1);
            return;
        }
        self.mit_auswahl = self.mit_auswahl.saturating_add(1);
        let offen: Vec<_> = modell
            .iter()
            .take(3)
            .chain(grund.iter().take(3))
            .filter_map(|i| zeilen.get(*i))
            .filter(|z| ki_relevanz(z).is_none())
            .collect();
        if !offen.is_empty() {
            self.fehlende_top3
                .extend(offen.into_iter().map(|z| z.fall.kennung.clone()));
            return;
        }
        let urteile: Vec<_> = zeilen
            .iter()
            .map(|z| Beleg {
                urteil: ki_relevanz(z),
                pflicht: false,
            })
            .collect();
        if let (Some(m), Some(g)) = (
            relevanzmessung::messe(&urteile, modell),
            relevanzmessung::messe(&urteile, grund),
        ) {
            self.verglichen = self.verglichen.saturating_add(1);
            self.modell_treffer = self
                .modell_treffer
                .saturating_add(usize::from(m.relevanter_treffer));
            self.grund_treffer = self
                .grund_treffer
                .saturating_add(usize::from(g.relevanter_treffer));
            if let (Vergleich::Bewertet(a), Vergleich::Bewertet(b)) = (m.vergleich, g.vergleich) {
                self.vollstaendig = self.vollstaendig.saturating_add(1);
                self.modell_ndcg = self.modell_ndcg.saturating_add(u64::from(a));
                self.grund_ndcg = self.grund_ndcg.saturating_add(u64::from(b));
            }
        }
    }

    fn text(&self) -> String {
        use std::fmt::Write as _;

        if self.mit_auswahl == 0 {
            return if self.zu_kurz == 0 {
                "Für den Top-3-Vergleich liegen keine Prüfgruppen vor.".into()
            } else {
                format!(
                    "Top-3-Vergleich nicht aussagekräftig. Jede Prüfgruppe enthält höchstens drei Belege, die damit alle in die Top 3 passen. Zahl der Prüfgruppen insgesamt {}.",
                    self.zu_kurz
                )
            };
        }
        let mut t = format!(
            "KI-Konsens für beide Top-3-Listen ({}/{} Prüfgruppen mit mindestens vier Belegen).",
            self.verglichen, self.mit_auswahl
        );
        if self.zu_kurz > 0 {
            let _ = write!(
                t,
                " Prüfgruppen mit höchstens drei Belegen ({}) gehen nicht in den Vergleich ein.",
                self.zu_kurz
            );
        }
        if self.verglichen == 0 {
            return t;
        }
        let _ = write!(
            t,
            " Relevanter Beleg unter den ersten drei in {} von {} mit KI-Konsens bewerteten Vergleichen (Grundrang {}).",
            self.modell_treffer, self.verglichen, self.grund_treffer
        );
        if let Ok(n) = u64::try_from(self.vollstaendig)
            && n != 0
        {
            let _ = write!(
                t,
                " nDCG@3 in Basispunkten {} (Grundrang {}, {} vollständig bewertete Prüfgruppen).",
                self.modell_ndcg.checked_div(n).unwrap_or(0),
                self.grund_ndcg.checked_div(n).unwrap_or(0),
                n
            );
        }
        t
    }
}

fn ranggruppen(
    zeilen: &[&Zeile],
    werte: &Rangwerte,
    gleichstand: fn(&Zeile) -> usize,
) -> (Vec<Ranggruppe>, Vergleichszahlen) {
    let mut gruppen = Gruppen::new();
    for z in zeilen {
        let z = *z;
        let zu = z.fall.zuordnung();
        if let Some(wert) = werte.get(&z.fall.kennung) {
            gruppen
                .entry((
                    z.tag,
                    format!("{:?}", zu.berater),
                    zu.isin.to_string(),
                    zu.thesen_sha.clone(),
                ))
                .or_default()
                .push((z, wert));
        }
    }
    let mut aus = Vec::new();
    let mut zahlen = Vergleichszahlen::default();
    for ((tag, berater, isin, _), mut gruppe) in gruppen.into_iter().rev() {
        gruppe.sort_by(|(a, aw), (b, bw)| {
            bw.1.total_cmp(&aw.1)
                .then_with(|| gleichstand(a).cmp(&gleichstand(b)))
                .then_with(|| a.fall.kennung.cmp(&b.fall.kennung))
        });
        let modell: Vec<_> = (0..gruppe.len()).collect();
        let mut grund = modell.clone();
        grund.sort_by_key(|i| gruppe.get(*i).map(|(z, _)| z.grundrang));
        zahlen.erfasse(
            &gruppe.iter().map(|(z, _)| *z).collect::<Vec<_>>(),
            &modell,
            &grund,
        );
        if aus.len() >= 8 {
            continue;
        }
        if let Some((erste, _)) = gruppe.first() {
            aus.push(Ranggruppe {
                tag: tag.to_string(),
                berater,
                isin,
                these: erste.fall.these.clone(),
                belege: gruppe
                    .iter()
                    .enumerate()
                    .filter(|(i, (z, _))| *i < 20 || z.grundrang <= 3)
                    .map(|(i, (z, (wert, _)))| Rangbeleg {
                        fall_id: z.fall.kennung.clone(),
                        referenz: z.fall.zuordnung().referenz.clone(),
                        titel: z.fall.beleg.clone(),
                        quelle: z.fall.quelle.clone().unwrap_or_default(),
                        grundrang: z.grundrang,
                        modellrang: i.saturating_add(1),
                        wert: wert.clone(),
                        merkmale: z.merkmale,
                    })
                    .collect(),
            });
        }
    }
    (aus, zahlen)
}

fn messwerte(
    ordner: &Path,
    antwort: &Json,
    abdruck: &str,
    paare: usize,
) -> Result<Lernkurve, String> {
    let fehlt = || {
        "Die Neuro2026-Trainingsmesswerte sind unvollständig oder gehören zu einem anderen Modell."
            .to_owned()
    };
    let modell = ordner.join("modell");
    let stand = lese_datei(&modell.join("stand.json"))?.ok_or_else(fehlt)?;
    let metadaten = lese_datei(&modell.join("modell.neuro.json"))?.ok_or_else(fehlt)?;
    if sha256_hex(stand.as_bytes()) != abdruck {
        return Err(fehlt());
    }
    let stand = json::lies(&stand)?;
    if text(&stand, "metadaten_sha256") != Some(&sha256_hex(metadaten.as_bytes())) {
        return Err(fehlt());
    }
    let metadaten = json::lies(&metadaten)?;
    let config = metadaten.feld("model").ok_or_else(fehlt)?;
    let breite = anzahl(config, "hidden_width")
        .filter(|b| (2..=32).contains(b))
        .ok_or_else(fehlt)?;
    if text(&metadaten, "format") != Some("neuro2026-model-v1")
        || text(&metadaten, "neuro2026_version") != Some(VERSION)
        || anzahl(config, "input_size") != Some(4)
        || anzahl(config, "output_size") != Some(1)
    {
        return Err(fehlt());
    }
    let epochen = antwort
        .feld("metrics")
        .and_then(Json::liste)
        .ok_or_else(fehlt)?;
    if epochen.is_empty()
        || epochen.len() > 10_000
        || anzahl(antwort, "epochs") != Some(epochen.len())
        || anzahl(antwort, "samples") != Some(paare)
    {
        return Err(fehlt());
    }
    let fehler = epochen
        .iter()
        .zip(1..)
        .map(|(epoche, nummer)| {
            if anzahl(epoche, "epoch") != Some(nummer) || epoche.doppelte_felder() {
                return None;
            }
            verlust(epoche.feld("mean_loss")?)
        })
        .collect::<Option<_>>()
        .ok_or_else(fehlt)?;
    Ok(Lernkurve {
        schichten: [4, breite, breite, 1],
        paare,
        anfang: antwort
            .feld("initial_loss")
            .and_then(verlust)
            .ok_or_else(fehlt)?,
        ende: antwort
            .feld("final_loss")
            .and_then(verlust)
            .ok_or_else(fehlt)?,
        fehler,
    })
}

struct ModellLauf {
    gruppen: Vec<Ranggruppe>,
    vergleich: Vergleichszahlen,
    abdruck: String,
    lernkurve: Lernkurve,
}

fn bericht_lauf(
    train: &[(&Zeile, u8)],
    pruef: &[&Zeile],
    ordner: &Path,
) -> Result<Option<ModellLauf>, String> {
    let train: Vec<_> = train
        .iter()
        .map(|(z, label)| format!("{{\"x\":{},\"stufe\":{label}}}", merkmale(z.merkmale)))
        .collect();
    let rang: Vec<_> = pruef
        .iter()
        .map(|z| {
            format!(
                "{{\"x\":{},\"id\":{}}}",
                merkmale(z.merkmale),
                zitiere(&z.fall.kennung)
            )
        })
        .collect();
    let lernlauf = austausch(ordner, "train", &train, "training_sha256")?;
    let antwort = austausch(ordner, "rank", &rang, "input_sha256")?;
    let (Some(lernlauf), Some(antwort)) = (lernlauf, antwort) else {
        return Ok(None);
    };
    let abdruck = modellabdruck(&lernlauf)?;
    if abdruck != modellabdruck(&antwort)? {
        return Err("Der Ranglauf hat ein anderes Modell geladen.".into());
    }
    let lernkurve = messwerte(ordner, &lernlauf, abdruck, train.len())?;
    let (gruppen, vergleich) = ranggruppen(pruef, &rangwerte(&antwort, pruef)?, |z| z.fragenplatz);
    Ok(Some(ModellLauf {
        gruppen,
        vergleich,
        abdruck: abdruck.into(),
        lernkurve,
    }))
}

fn prueffaelle_ab(zeilen: &[Zeile], ab: Datum) -> Vec<&Zeile> {
    zeilen.iter().filter(|z| z.tag >= ab).collect()
}

fn trainingsfaelle<'a>(
    zeilen: &'a [Zeile],
    train_bis: Datum,
    pruef: &[&Zeile],
) -> Vec<(&'a Zeile, u8)> {
    let spaetere_meldungen: BTreeSet<_> = pruef.iter().map(|z| meldung(z)).collect();
    zeilen
        .iter()
        .filter(|z| z.tag <= train_bis && !spaetere_meldungen.contains(&meldung(z)))
        .filter_map(|z| Some((z, u8::from(ki_relevanz(z)? == Urteil::Bezug))))
        .collect()
}

pub(super) struct Lernstand {
    pub von: Datum,
    pub train_bis: Datum,
    pub bis: Datum,
    pub mit_bezug: usize,
    pub ohne_bezug: usize,
    pub prueffaelle: usize,
    pub pruefurteile: usize,
}

/// Prüft einen vorgeschlagenen Zeitraum anhand derselben Fälle wie der Modelllauf.
/// Der Vorschlag nimmt den jüngsten verfügbaren Prüftag und das spätestmögliche Trainingsende.
pub(super) fn lernstand(daten: &Path, heute: Datum) -> Result<Option<Lernstand>, String> {
    let beginn = Datum::neu(1970, 1, 1).ok_or("Historischer Zeitraum ist ungültig.")?;
    let datensatz = crate::neurodaten::laden(daten, beginn, heute)?;
    let zeilen = &datensatz.zeilen;
    if zeilen.len() > MAX_PAARE {
        return Err("Zu viele Belegpaare für einen gemeinsamen Lauf. Wähle beim lokalen Modelllauf einen kürzeren Zeitraum.".into());
    }
    let (Some(erste), Some(letzte)) = (zeilen.first(), zeilen.last()) else {
        return Ok(None);
    };
    let trainingsende = zeilen
        .iter()
        .rev()
        .map(|z| z.tag)
        .find(|&tag| pruefstart(tag).is_some_and(|start| start <= letzte.tag));
    let train_bis = trainingsende.unwrap_or(letzte.tag);
    let pruef = trainingsende
        .and_then(pruefstart)
        .map_or_else(Vec::new, |start| prueffaelle_ab(zeilen, start));
    let train = trainingsfaelle(zeilen, train_bis, &pruef);
    let mit_bezug = train.iter().filter(|(_, stufe)| *stufe == 1).count();
    Ok(Some(Lernstand {
        von: erste.tag,
        train_bis,
        bis: letzte.tag,
        mit_bezug,
        ohne_bezug: train.len().saturating_sub(mit_bezug),
        prueffaelle: pruef.len(),
        pruefurteile: pruef.iter().filter(|z| ki_relevanz(z).is_some()).count(),
    }))
}

const TEXT_FORMAT: &str = "depot-neuro-textprobe-v1";

/// Bereitet eine lokale Textprobe vor oder wertet die passende Rangantwort lesend aus.
pub fn neuro_textprobe(
    daten: &Path,
    von: Datum,
    bis: Datum,
    ordner: &Path,
) -> Result<String, String> {
    textprobe(daten, von, bis, ordner, false)
}

/// Merkt fehlende KI-Urteile der beiden historischen Top-3-Listen lokal vor.
pub fn neuro_textprobe_ki(
    daten: &Path,
    von: Datum,
    bis: Datum,
    ordner: &Path,
) -> Result<String, String> {
    textprobe(daten, von, bis, ordner, true)
}

fn textprobe(
    daten: &Path,
    von: Datum,
    bis: Datum,
    ordner: &Path,
    ki_vormerken: bool,
) -> Result<String, String> {
    let datensatz = crate::neurodaten::laden(daten, von, bis)?;
    let faelle: Vec<_> = datensatz
        .zeilen
        .iter()
        .filter(|z| !z.fall.these.trim().is_empty() && !z.fall.beleg.trim().is_empty())
        .collect();
    if faelle.is_empty() {
        return Err("Im Zeitraum liegen keine belegten These-Beleg-Paare vor.".into());
    }
    if faelle.len() > MAX_PAARE {
        return Err("Zu viele These-Beleg-Paare. Bitte einen kürzeren Zeitraum wählen.".into());
    }
    let zeilen = faelle
        .iter()
        .map(|z| {
            format!(
                "{{\"id\":{},\"these\":{},\"beleg\":{}}}",
                zitiere(&z.fall.kennung),
                zitiere(&z.fall.these),
                zitiere(&z.fall.beleg)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let eingabe = format!("{{\"format\":\"{TEXT_FORMAT}\",\"rows\":[{zeilen}]}}\n");
    if eingabe.len() > MAX_BYTES {
        return Err(
            "Die lokalen Textdaten sind zu groß. Bitte einen kürzeren Zeitraum wählen.".into(),
        );
    }
    let pfad = ordner.join("textprobe-eingabe.json");
    let Some(antwort) = lese_json(&ordner.join("textprobe-antwort.json"))? else {
        if ki_vormerken {
            return Err("Für die KI-Vorprüfung fehlt die lokale Rangantwort.".into());
        }
        schreibe_ganz(&pfad, &eingabe)?;
        return Ok(format!(
            "{} Paare für die lokale Textprobe vorbereitet. Die Rangantwort steht aus.",
            faelle.len()
        ));
    };
    if antwort.doppelte_felder()
        || text(&antwort, "format") != Some(TEXT_FORMAT)
        || text(&antwort, "input_sha256") != Some(sha256_hex(eingabe.as_bytes()).as_str())
    {
        return Err("Die Textantwort gehört zu einem anderen Fallbestand.".into());
    }
    let werte = rangwerte(&antwort, &faelle)?;
    let (_, vergleich) = ranggruppen(&faelle, &werte, |z| z.grundrang);
    let status = format!(
        "Textähnlichkeit gegen Grundrang · {} geprüfte Paare. {}",
        faelle.len(),
        vergleich.text()
    );
    if vergleich.fehlende_top3.is_empty() {
        return Ok(status);
    }
    let fragen = crate::neuroauftraege::textprobe_fragen(
        daten,
        faelle
            .iter()
            .map(|z| &z.fall)
            .filter(|f| vergleich.fehlende_top3.contains(&f.kennung)),
        ki_vormerken,
    );
    match fragen {
        Ok(fragen) => Ok(format!("{status}\n{fragen}")),
        Err(e) if !ki_vormerken => Ok(format!(
            "{status}\nKI-Fragenstatus gerade nicht verfügbar · {e}"
        )),
        Err(e) => Err(e),
    }
}

/// Bereitet Trainings- und Prüfeingaben vor oder übernimmt den belegten Ranglauf.
/// Der Aufruf selbst startet keine weiteren Programme.
pub fn neuro_pilot(
    daten: &Path,
    von: Datum,
    train_bis: Datum,
    bis: Datum,
    austauschordner: Option<&Path>,
) -> Result<String, String> {
    if von > train_bis || train_bis >= bis {
        return Err("Die Daten müssen in der Reihenfolge VON, TRAIN_BIS, BIS liegen.".into());
    }
    let datensatz = crate::neurodaten::laden(daten, von, bis)?;
    if datensatz.zeilen.len() > MAX_PAARE {
        return Err("Zu viele Belegpaare. Bitte einen kürzeren Zeitraum wählen.".into());
    }
    let erster_prueftag = pruefstart(train_bis).ok_or("Prüfzeitraum ist nicht darstellbar.")?;
    let pruef = prueffaelle_ab(&datensatz.zeilen, erster_prueftag);
    let train = trainingsfaelle(&datensatz.zeilen, train_bis, &pruef);
    let klassen: BTreeSet<_> = train.iter().map(|(_, k)| *k).collect();
    let mut bericht = ModellBericht {
        modellversion: None,
        modellabdruck: None,
        lernkurve: None,
        stand: String::new(),
        paare: datensatz.zeilen.len(),
        urteile: datensatz
            .zeilen
            .iter()
            .filter(|z| ki_relevanz(z).is_some())
            .count(),
        ki_konsens: true,
        trainingstage: train
            .iter()
            .map(|(z, _)| z.tag)
            .collect::<BTreeSet<_>>()
            .len(),
        prueftage: pruef.iter().map(|z| z.tag).collect::<BTreeSet<_>>().len(),
        gruppen: Vec::new(),
    };
    bericht.stand = if klassen.len() < 2 {
        format!(
            "{} verwendbare KI-Übereinstimmungen im Training. Für den Start braucht Neuro2026 ältere Fälle mit und ohne Belegbezug.",
            train.len()
        )
    } else if pruef.is_empty() {
        "Es fehlen spätere Prüffälle mit drei Handelstagen Abstand zum Training.".into()
    } else if let Some(ordner) = austauschordner {
        if let Some(lauf) = bericht_lauf(&train, &pruef, ordner)? {
            bericht.modellversion = Some(VERSION.into());
            bericht.modellabdruck = Some(lauf.abdruck);
            bericht.lernkurve = Some(lauf.lernkurve);
            bericht.gruppen = lauf.gruppen;
            lauf.vergleich.text()
        } else {
            "Die Eingaben sind vorbereitet. Der lokale Modelllauf steht aus.".into()
        }
    } else {
        "Der Fallbestand ist für einen ersten Lauf vorbereitet. Es fehlt ein Beobachtungslauf mit Neuro2026.".into()
    };
    bericht.speichere(daten)?;
    Ok(bericht.stand)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn pruefung_beginnt_nach_drei_vollen_handelstagen() -> Result<(), String> {
        let freitag = Datum::lies("2026-11-20").ok_or("Testdatum")?;
        assert_eq!(pruefstart(freitag), Datum::lies("2026-11-26"));
        Ok(())
    }

    #[test]
    fn antwort_muss_zu_den_genauen_eingabebytes_gehoeren() -> Result<(), String> {
        let ordner = std::env::temp_dir().join(crate::neues_token());
        fs::create_dir(&ordner).map_err(|e| e.to_string())?;
        let pruefe = || -> Result<(), String> {
            let zeilen = [r#"{"x":[0,0,1,0],"id":"fall"}"#.to_owned()];
            assert!(austausch(&ordner, "rank", &zeilen, "input_sha256")?.is_none());
            let bytes = fs::read(ordner.join("rank-eingabe.json")).map_err(|e| e.to_string())?;
            let sha = sha256_hex(&bytes);
            let antwort = format!(
                "{{\"format\":\"{FORMAT}\",\"schema\":\"{SCHEMA}\",\"op\":\"rank\",\
                 \"neuro2026_version\":\"{VERSION}\",\"input_sha256\":\"{sha}\"}}"
            );
            fs::write(ordner.join("rank-antwort.json"), antwort).map_err(|e| e.to_string())?;
            assert!(austausch(&ordner, "rank", &zeilen, "input_sha256")?.is_some());
            let geaendert = [r#"{"x":[0,1,1,0],"id":"fall"}"#.to_owned()];
            assert!(austausch(&ordner, "rank", &geaendert, "input_sha256").is_err());
            Ok(())
        };
        let ergebnis = pruefe();
        fs::remove_dir_all(ordner).map_err(|e| e.to_string())?;
        ergebnis
    }
}
