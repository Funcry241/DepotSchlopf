//! Reproduzierbare, voneinander unabhängig beurteilbare Paare aus Vorschlagsthese und Quellenbeleg.

use std::collections::{BTreeMap, BTreeSet};

use depot_journal::{KiZweck, Stand, gespraeche::Gespraech};
use depot_kern::{BeraterId, Datum, Herkunft, Isin, Zeitpunkt, abdruck::sha256_hex, liste};
use depot_sim::{
    Ausstiegsplan, belegrang::eindeutige_frageliste, papiertag, vorbereitung::text_fuer_frage,
};

// Begrenzt den Umfang der lokal aufgebauten Tagesvorlage einschließlich mehrfacher Thesen.
const MAX_PAARE: usize = 4096;
const ZU_GROSS: &str = "Zu viele historische Thesen oder Belegpaare an diesem Prüftag";

/// Wird zur späteren Auswertung gespeichert, aber während der Blindprüfung nicht angezeigt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Zuordnung {
    pub berater: BeraterId,
    pub thesentag: Datum,
    pub thesen_sha: String,
    pub isin: Isin,
    /// Derselbe Kandidatenschlüssel wie im Export neuro2026_eingaben.jsonl.
    pub referenz: String,
}

impl Zuordnung {
    /// Domänentrennung und eindeutige Feldgrenzen; die Kennung verrät keinen Rang.
    pub fn kennung(&self, stichtag: Zeitpunkt) -> String {
        sha256_hex(
            format!(
                "blindpaar-v1\0{stichtag}\0{:?}\0{}\0{}\0{}\0{}",
                self.berater, self.thesentag, self.isin, self.thesen_sha, self.referenz
            )
            .as_bytes(),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prueffall {
    pub kennung: String,
    pub these: String,
    pub wert: String,
    pub beleg: String,
    pub quelle: Option<String>,
    pub veroeffentlicht: Option<Zeitpunkt>,
    pub stichtag: Zeitpunkt,
    zuordnung: Zuordnung,
}

impl Prueffall {
    pub fn zuordnung(&self) -> &Zuordnung {
        &self.zuordnung
    }
}

struct These<'a> {
    tag: Datum,
    wert: &'a liste::Wert,
    text: &'a str,
    sha: String,
}

fn thesen<'a>(
    vorher: &'a [(Datum, String, String)],
    werte: &'a [liste::Wert],
    wer: BeraterId,
    tag: Datum,
) -> Result<Vec<These<'a>>, &'static str> {
    let (name, topf) = match wer {
        BeraterId::Dachs => ("Dachs", "Topf A"),
        BeraterId::Maus => ("Maus", "Topf B"),
    };
    let mut aus = BTreeMap::new();
    let fenster = tag
        .plus_tage(-5)
        .ok_or("Thesenzeitraum nicht darstellbar")?;
    for (d, w, zeile) in vorher {
        if w != name || *d >= tag || *d < fenster {
            continue;
        }
        let Some((art, rest)) = zeile.split_once(": ") else {
            continue;
        };
        if art != topf && !matches!(art, "Abstimmung Runde 1" | "Abstimmung Runde 2") {
            continue;
        }
        let Some((ziel, grund)) = rest.split_once(" bp: ") else {
            continue;
        };
        let Some((isin, bp)) = ziel.split_once(' ') else {
            continue;
        };
        let Some(wert) = werte.iter().find(|w| w.isin.als_str() == isin) else {
            continue;
        };
        if bp == "0" {
            aus.retain(|(_, isin, _), _| *isin != &wert.isin);
            continue;
        }
        let grund = grund.trim();
        if !bp.parse::<u16>().is_ok_and(|g| (1..=10_000).contains(&g))
            || grund == "ausgelassen, unverändert gehalten"
            || !beurteilbar(grund.rsplit_once(']').map_or(grund, |(_, s)| s))
            || !blind_lesbar(grund)
        {
            continue;
        }
        if let Some(plan) = Ausstiegsplan::lies(grund) {
            let handelstage = (1..=5)
                .filter_map(|n| d.plus_tage(n))
                .filter(|datum| *datum <= tag && wert.boerse.handelstag(*datum))
                .count();
            if handelstage > usize::from(plan.handelstage) {
                continue;
            }
        }
        // Gleiche These im selben Tagesverlauf zählt einmal; abweichende Texte bleiben getrennt.
        let sha = sha256_hex(grund.as_bytes());
        aus.entry((*d, &wert.isin, sha.clone())).or_insert(These {
            tag: *d,
            wert,
            text: grund,
            sha,
        });
        if aus.len() > MAX_PAARE {
            return Err(ZU_GROSS);
        }
    }
    Ok(aus.into_values().collect())
}

fn beurteilbar(text: &str) -> bool {
    let text = text.trim();
    !text.ends_with('…')
        && !text.ends_with("...")
        && text
            .split_whitespace()
            .filter(|wort| wort.chars().any(char::is_alphabetic))
            .count()
            >= 2
}

// Originalzitate, die den Berater oder eine sichtbare Auswahl verraten, bleiben außerhalb
// der Blindprüfung. Die Originalthese wird weder umformuliert noch nachträglich ergänzt.
fn blind_lesbar(text: &str) -> bool {
    let klein = text.to_lowercase();
    let mut woerter = klein
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .peekable();
    while let Some(wort) = woerter.next() {
        let folgt = woerter.peek().copied();
        if matches!(wort, "maus" | "dachs" | "grundrang" | "fragenplatz")
            || (wort == "topf" && matches!(folgt, Some("a" | "b")))
            || (matches!(wort, "ereignis" | "ereignisse" | "beleg" | "belege")
                && folgt.is_some_and(|s| s.bytes().all(|b| b.is_ascii_digit())))
            || wort
                .strip_prefix('j')
                .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        {
            return false;
        }
    }
    true
}

fn frage<'a>(
    stand: &Stand,
    gespraeche: &'a [Gespraech],
    tag: Datum,
    stichtag: Zeitpunkt,
    wer: BeraterId,
) -> Option<&'a str> {
    let kurz = match wer {
        BeraterId::Dachs => "dachs",
        BeraterId::Maus => "maus",
    };
    let mut fragen = gespraeche.iter().filter(|g| {
        g.rolle == "haupt"
            && g.wer == kurz
            && g.zweck == KiZweck::Erstmeinung.text()
            && g.http == Some(200)
            && g.zeit >= stichtag
            && g.zeit.frankfurt_tag() == Some(tag)
            && g.buchung.is_some_and(|nr| {
                stand
                    .ki()
                    .buchungen()
                    .get(nr)
                    .is_some_and(|b| b.datum == tag && b.berater == wer)
                    && stand
                        .ki()
                        .buchungsbeleg(nr)
                        .is_some_and(|(zweck, _)| zweck == KiZweck::Erstmeinung)
            })
    });
    let g = fragen.next()?;
    if fragen.next().is_some() {
        return None;
    }
    let text = g.frage.as_deref()?;
    (text.lines().next() == Some(format!("Datum: {tag}").as_str())).then_some(text)
}

#[derive(PartialEq, Eq)]
struct Quelle<'a> {
    text: String,
    name: &'a str,
    zeit: Option<Zeitpunkt>,
    url: Option<&'a str>,
}

fn quellen<'a>(
    stand: &'a Stand,
    zeit: Zeitpunkt,
    kandidaten: &[String],
) -> BTreeMap<String, Quelle<'a>> {
    let gesucht: BTreeSet<&str> = kandidaten.iter().map(String::as_str).collect();
    let mut aus = BTreeMap::new();
    // Neueste stichtagsbekannte Fassung jeder Terminkennung, in derselben Form wie im Papiertag.
    let termine: BTreeMap<_, _> = stand
        .termine()
        .iter()
        .filter(|b| b.abruf() <= zeit)
        .map(|b| (b.termin().kennung(), b))
        .collect();
    let mut mehrdeutig = BTreeSet::new();
    for b in termine
        .values()
        .filter(|b| b.herkunft() == Herkunft::Quelle)
    {
        let Some(text) =
            papiertag::termin_text(b.termin()).filter(|t| gesucht.contains(t.as_str()))
        else {
            continue;
        };
        let quelle = Quelle {
            text: text.clone(),
            name: b.quelle(),
            zeit: None,
            url: None,
        };
        if aus.get(&text).is_some_and(|q| q != &quelle) {
            mehrdeutig.insert(text);
        } else {
            aus.insert(text, quelle);
        }
    }
    // Die Wertezuordnung am Ende des historischen Kandidaten kann sich geändert haben.
    // Nur der unveränderte Quellenwortlaut mit Quellname und UTC-Minute verbindet beide.
    let mut originaltexte = BTreeMap::<&str, Vec<&str>>::new();
    let mut meldungen = BTreeSet::new();
    for text in gesucht
        .iter()
        .copied()
        .filter(|t| t.starts_with("Nachricht "))
    {
        let ohne_wert = text
            .rsplit_once(" [betrifft: ")
            .filter(|(_, werte)| werte.ends_with(']'))
            .map(|(anfang, _)| anfang);
        for basis in [Some(text), text.strip_suffix(" [Makro]"), ohne_wert]
            .into_iter()
            .flatten()
        {
            if let Some((_, inhalt)) = basis.split_once("): ") {
                meldungen.insert(inhalt);
                originaltexte.entry(basis).or_default().push(text);
            }
        }
    }
    for n in stand
        .nachrichten()
        .iter()
        .filter(|n| n.abruf() <= zeit && meldungen.contains(n.text()))
    {
        let Some(basis) = papiertag::nachricht_ohne_markierung(n.zeit(), n.quelle(), n.text())
        else {
            continue;
        };
        let Some(kandidaten) = originaltexte.get(basis.as_str()) else {
            continue;
        };
        let (titel, url) = n
            .text()
            .rsplit_once(" | ")
            .filter(|(_, u)| u.starts_with("https://"))
            .map_or((n.text(), None), |(t, u)| (t, Some(u)));
        let titel = titel
            .strip_prefix(n.quelle())
            .and_then(|t| t.strip_prefix(": "))
            .unwrap_or(titel);
        if !beurteilbar(titel) {
            continue;
        }
        for &text in kandidaten {
            let quelle = Quelle {
                text: titel.to_owned(),
                name: n.quelle(),
                zeit: Some(n.zeit()),
                url,
            };
            // Der Fragetext rundet auf Minuten. Zwei unterschiedliche Belege bleiben mehrdeutig.
            if aus.get(text).is_some_and(|q| q != &quelle) {
                mehrdeutig.insert(text.to_owned());
            } else {
                aus.insert(text.to_owned(), quelle);
            }
        }
    }
    aus.retain(|text, _| !mehrdeutig.contains(text));
    aus
}

fn sichtbare<'a>(
    frage: &str,
    kandidaten: &'a [String],
    quellen: &'a BTreeMap<String, Quelle<'_>>,
) -> Vec<(usize, &'a str, &'a Quelle<'a>)> {
    let block = frage.split("\nTopflage:\n").next().unwrap_or(frage);
    let mut zeilen = BTreeMap::new();
    for (nr, text) in block
        .lines()
        .filter_map(|l| l.strip_prefix("- Ereignis ")?.split_once(": "))
    {
        if let Ok(n) = nr.parse::<usize>() {
            zeilen
                .entry(n)
                .and_modify(|v| *v = None)
                .or_insert(Some(text));
        }
    }
    let mut titel = BTreeSet::new();
    let mut urls = BTreeSet::new();
    kandidaten
        .iter()
        .enumerate()
        .filter_map(|(i, text)| {
            let platz = i.checked_add(1)?;
            let gezeigt = zeilen.get(&platz).copied().flatten()?;
            let quelle = quellen.get(text)?;
            let kurz = text.split(" | ").next().unwrap_or(text);
            if (gezeigt != text_fuer_frage(text) && gezeigt != text_fuer_frage(kurz))
                || !gezeigt.contains(&quelle.text)
            {
                return None;
            }
            let gruppe: String = quelle
                .text
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            let neuer_titel = titel.insert(gruppe);
            let neue_url = quelle.url.is_none_or(|url| urls.insert(url));
            (neuer_titel && neue_url).then_some((platz, text.as_str(), quelle))
        })
        .collect()
}

/// Bewertbare historische Thesen in der fixierten Wissenssicht und in der gespeicherten
/// Erstmeinung sichtbare Quellen. Die SHA-Sortierung mischt unabhängig vom Fragen- und Grundrang.
pub fn prueffaelle(
    stand: &Stand,
    gespraeche: &[Gespraech],
    tag: Datum,
) -> Result<Vec<Prueffall>, &'static str> {
    let (stichtag, vorher) = stand
        .wissensstichtag(tag)
        .zip(stand.begruendungen_bis_stichtag(tag))
        .ok_or("Wissensstichtag fehlt")?;
    let kandidaten = eindeutige_frageliste(stand, tag)?;
    if kandidaten.is_empty() {
        return Err("Frageabschnitt leer");
    }
    let werte = liste::beobachtungsliste().map_err(|_| "Werteliste fehlt")?;
    let quellen = quellen(stand, stichtag, &kandidaten);
    let mut aus = Vec::new();
    let mut hat_these = false;
    let mut hat_frage = false;
    for wer in [BeraterId::Dachs, BeraterId::Maus] {
        let thesen = thesen(vorher, &werte, wer, tag)?;
        if thesen.is_empty() {
            continue;
        }
        hat_these = true;
        let Some(frage) = frage(stand, gespraeche, tag, stichtag, wer) else {
            continue;
        };
        hat_frage = true;
        let sichtbar = sichtbare(frage, &kandidaten, &quellen);
        if aus
            .len()
            .saturating_add(sichtbar.len().saturating_mul(thesen.len()))
            > MAX_PAARE
        {
            return Err(ZU_GROSS);
        }
        for (platz, original, quelle) in sichtbar {
            let referenz = format!("{tag}#{platz}:{}", sha256_hex(original.as_bytes()));
            for t in &thesen {
                let isin = &t.wert.isin;
                let zuordnung = Zuordnung {
                    berater: wer,
                    thesentag: t.tag,
                    thesen_sha: t.sha.clone(),
                    isin: isin.clone(),
                    referenz: referenz.clone(),
                };
                aus.push(Prueffall {
                    kennung: zuordnung.kennung(stichtag),
                    these: format!("{} · {}", t.tag, t.text),
                    wert: format!("{} ({isin})", t.wert.name),
                    beleg: quelle.text.clone(),
                    quelle: Some(quelle.name.to_owned()),
                    veroeffentlicht: quelle.zeit,
                    stichtag,
                    zuordnung,
                });
            }
        }
    }
    if !hat_these {
        return Err("Keine beurteilbare frühere Vorschlagsthese belegt");
    }
    if !hat_frage {
        return Err("Passende gespeicherte Erstmeinungsfrage fehlt oder ist mehrdeutig");
    }
    if aus.is_empty() {
        return Err("Kein eindeutig sichtbarer Quellenbeleg zur Prüfung vorhanden");
    }
    aus.sort_unstable_by(|a, b| a.kennung.cmp(&b.kennung));
    Ok(aus)
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_journal::{Journal, NachrichtBeleg, TerminBeleg};
    use depot_kern::{Betroffen, Termin, Verlaesslichkeit, Zeitbezug};

    fn synthetische_frage(tag: Datum, zeit: Zeitpunkt, nr: usize, daten: &[String]) -> Gespraech {
        let text = daten
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let gezeigt = match i {
                    0 => t.split(" | ").next().unwrap_or(t),
                    3 => "(Relevanz 0, ausgeblendet)",
                    _ => t,
                };
                format!("- Ereignis {}: {gezeigt}", i.saturating_add(1))
            })
            .collect::<Vec<_>>()
            .join("\n");
        Gespraech {
            zeit,
            lauf: "test".into(),
            wer: "maus".into(),
            rolle: "haupt".into(),
            modell: "test".into(),
            zweck: "erstmeinung".into(),
            ergebnis: "ok".into(),
            http: Some(200),
            token_ein: None,
            token_aus: None,
            cache: None,
            buchung: Some(nr),
            schlopfmodell: None,
            kosten_cent: None,
            system: None,
            frage: Some(format!("Datum: {tag}\n{text}")),
            antwort: None,
            fehler: None,
        }
    }

    fn kandidaten(j: &mut Journal, tag: Datum, zeit: Zeitpunkt) -> Result<Vec<String>, String> {
        let fehler = |e| format!("{e:?}");
        let titel = "SAP meldet zusätzliche Nachfrage nach Gemini";
        let mut daten = Vec::new();
        for (quelle, wortlaut) in [
            ("rss-a", format!("rss-a: {titel} | https://example.test/a")),
            ("rss-b", format!("rss-b: {titel} | https://example.test/b")),
            (
                "rss-b",
                "Andere Meldung zur Gruppe | https://example.test/a".into(),
            ),
            ("rss-a", "Erhöhte Zinsen für Staatsanleihen".into()),
        ] {
            let b = NachrichtBeleg::neu(quelle, zeit, b"quelle", zeit, &wortlaut)?;
            j.belege_nachricht(&b).map_err(fehler)?;
            let text = papiertag::nachricht_text(&b).ok_or("Testnachricht")?;
            // Ein früherer Stand hatte diesen Wert anders zugeordnet.
            daten.push(if quelle == "rss-a" {
                text.replace(" [betrifft: SAP]", " [betrifft: SAP und Cloud]")
            } else {
                text
            });
        }
        let termin = Termin::neu(
            "termin",
            "SAP nennt Zahlen",
            Zeitbezug::Tag(tag),
            vec![Betroffen::Wert(
                Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?,
            )],
            Verlaesslichkeit::Bestaetigt,
        )
        .map_err(|e| format!("{e:?}"))?;
        let wortlaut = papiertag::termin_text(&termin).ok_or("Termintext")?;
        let beleg = TerminBeleg::neu("kalender", Herkunft::Quelle, zeit, b"termin", termin)?;
        j.belege_termin(&beleg).map_err(fehler)?;
        daten.push(wortlaut);
        daten.push("Frühere KI-Hypothese Dachs: aus der eigenen Antwort".into());
        Ok(daten)
    }

    #[test]
    fn bewertet_originalfragen_ohne_spaetere_thesen_und_doppelte_meldungen() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("depot-blind-{}", std::process::id()));
        std::fs::create_dir(&dir).map_err(|e| e.to_string())?;
        let ergebnis = (|| {
            let fehler = |e| format!("{e:?}");
            let pfad = dir.join("journal.txt");
            Journal::einrichten(&pfad).map_err(fehler)?;
            let mut j = Journal::oeffnen(&pfad).map_err(fehler)?;
            let tag = Datum::neu(2029, 1, 19).ok_or("Datum")?;
            let davor = tag.plus_tage(-1).ok_or("Vortag")?;
            let zeit = Zeitpunkt::neu(tag, 8, 0, 0).ok_or("Zeit")?;
            let quelle = Zeitpunkt::neu(tag, 7, 0, 0).ok_or("Quellenzeit")?;
            let daten = kandidaten(&mut j, tag, quelle)?;
            for text in [
                "Neue Aufträge treiben die Marge",
                "Neue Aufträge treiben die Marge",
                "Höhere Kosten bedrohen die Marge",
                "ausgelassen, unverändert gehalten",
                "Halten",
            ] {
                j.begruende(
                    davor,
                    "Maus",
                    &format!("Topf B: DE0007164600 2000 bp: {text}"),
                )
                .map_err(fehler)?;
            }
            j.schliesse_ab(davor, depot_kern::Cent(1000), &j.bestaetigt().1)
                .map_err(fehler)?;
            j.belege_wissensstichtag(tag, zeit).map_err(fehler)?;
            j.begruende(
                davor,
                "Maus",
                "Topf B: DE0007164600 2000 bp: Rückdatierte neue These",
            )
            .map_err(fehler)?;
            j.beobachte(tag, "Tagesbeobachtung außerhalb der Frage")
                .map_err(fehler)?;
            for text in &daten {
                j.beobachte(tag, text).map_err(fehler)?;
            }
            j.begruende(tag, "Frageliste", &format!("1 {}", daten.len()))
                .map_err(fehler)?;
            let nr = j
                .reserviere_fuer(BeraterId::Maus, tag, 1, KiZweck::Erstmeinung)
                .map_err(|e| format!("{e:?}"))?
                .buchungsnummer();
            let g = synthetische_frage(tag, zeit, nr, &daten);
            let fragen = std::slice::from_ref(&g);
            let faelle = prueffaelle(j.stand(), fragen, tag)?;
            assert_eq!(faelle.len(), 4, "{faelle:?}");
            assert_eq!(faelle, prueffaelle(j.stand(), fragen, tag)?);
            assert!(
                faelle
                    .windows(2)
                    .all(|p| matches!(p, [a,b] if a.kennung < b.kennung))
            );
            let nachrichten: Vec<_> = faelle
                .iter()
                .filter(|f| f.quelle.as_deref() == Some("rss-a"))
                .collect();
            assert_eq!(nachrichten.len(), 2);
            let referenz = format!(
                "{tag}#1:{}",
                sha256_hex(daten.first().ok_or("Original")?.as_bytes())
            );
            for f in nachrichten {
                assert_eq!(f.veroeffentlicht, Some(quelle));
                assert_eq!(f.beleg, "SAP meldet zusätzliche Nachfrage nach Gemini");
                assert_eq!(f.zuordnung().referenz, referenz);
                assert_eq!(f.kennung, f.zuordnung().kennung(zeit));
            }
            assert!(
                faelle
                    .iter()
                    .filter(|f| f.quelle.as_deref() == Some("kalender"))
                    .all(|f| f.veroeffentlicht.is_none())
            );
            assert!(
                faelle
                    .iter()
                    .all(|f| !f.these.contains("Rückdatierte") && !f.these.contains("Maus"))
            );
            assert!(prueffaelle(j.stand(), &[], tag).is_err());
            assert!(prueffaelle(j.stand(), &[g.clone(), g], tag).is_err());
            Ok(())
        })();
        std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
        ergebnis
    }

    #[test]
    fn offene_thesen_beachten_topf_verkaeufe_zeitfenster_und_horizont() -> Result<(), String> {
        let tag = Datum::neu(2029, 1, 19).ok_or("Tag")?;
        let werte = liste::beobachtungsliste()?;
        let zeile = |alter, art, isin, gewicht, text| -> Result<_, String> {
            Ok((
                tag.plus_tage(alter).ok_or("Alter")?,
                "Maus".to_string(),
                format!("{art}: {isin} {gewicht} bp: {text}"),
            ))
        };
        let s = "DE0007164600";
        let andere = "DE0007236101";
        let alt = vec![
            zeile(-6, "Topf B", s, 100, "Zu alt für die Vorlage")?,
            zeile(-3, "Topf B", s, 100, "Nachfrage stützt den Gewinn")?,
            zeile(-2, "Abstimmung Runde 1", s, 0, "Wert vollständig verkaufen")?,
            zeile(
                -3,
                "Topf B",
                andere,
                100,
                "[Meldung] [Ausstieg +400bp 1] Das Echo hält nur kurzfristig an",
            )?,
            zeile(-1, "Topf A", s, 100, "Kein eigener Topf")?,
            zeile(-1, "Topf B", s, 10001, "Über dem zulässigen Gewicht")?,
            zeile(
                -1,
                "Topf B",
                s,
                100,
                "Ereignis 2 wurde durch den Berater gewählt",
            )?,
            zeile(0, "Topf B", s, 100, "Erst heute bestätigt")?,
            zeile(
                -1,
                "Topf B",
                andere,
                100,
                "Der Preis fällt bei geringeren Bestellungen",
            )?,
        ];
        let aus = thesen(&alt, &werte, BeraterId::Maus, tag)?;
        assert_eq!(aus.len(), 1);
        assert_eq!(
            aus.first().map(|t| t.text),
            Some("Der Preis fällt bei geringeren Bestellungen")
        );
        Ok(())
    }
}
