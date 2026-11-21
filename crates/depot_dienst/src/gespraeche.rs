//! Lokale Ansicht der protokollierten Aufrufe aller KI-Rollen.
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use depot_journal::neurofrage::{NeuroFrage, NeuroRunde, NeuroUrteil, NeuroVersuch};
use depot_journal::{Journal, KiAufruf, KiStand};

use depot_kern::json::{Json, erstes_objekt};
use depot_kern::modelle::{Modell, Modellwahl};
use depot_kern::{BeraterId, Cent, Datum, Zeitpunkt};
use depot_lage::gespraeche::{self, Gespraech};
use depot_lage::html::maskiere;

pub(crate) fn beim_start(wurzel: &Path) {
    if let Some((tag, ..)) = crate::jetzt().and_then(depot_kern::Zeitpunkt::in_frankfurt)
        && let Err(e) = gespraeche::aufraeumen(&wurzel.join("daten"), tag)
    {
        eprintln!("[Hinweis] Gesprächsprotokoll nicht aufgeräumt: {e}");
    }
}

pub(crate) fn name(wer: &str) -> &str {
    match wer {
        "dachs" => "Dachs",
        "maus" => "Maus",
        "gemini" => "Gemini",
        wer => wer,
    }
}

pub(crate) fn status(ergebnis: &str) -> &str {
    match ergebnis {
        "ok" => "Antwort erhalten",
        "uebersprungen" => "Nicht gefragt",
        "netz" => "Netzfehler",
        "limit" => "Anbieterlimit",
        "guthaben" => "Guthaben fehlt",
        "kosten" => "Kostengrenze",
        "unklar" => "Antwort unklar",
        "abgelehnt" => "Abgelehnt",
        "ueberlastet" => "Anbieter überlastet",
        "fehler" => "Fehler",
        wert => wert,
    }
}

pub(crate) fn zweck_text(zweck: &str) -> &str {
    match zweck {
        "erstmeinung" => "Erstmeinung",
        "zweitmeinung" => "Zweitmeinung",
        "abstimmung" => "Abstimmung",
        "vorbereitung" => "Vorbereitung",
        "simulation" => "Simulation",
        "wochenrueckschau" => "Wochenrückschau",
        "nachrichten-vorsortieren" => "Nachrichten vorsortieren",
        "neurofrage" => "Belege bewerten",
        wert => wert,
    }
}

struct Belege<'a> {
    daten: &'a Path,
    ki: Option<&'a KiStand>,
    neurofragen: BTreeMap<usize, &'a NeuroVersuch>,
}

impl Belege<'_> {
    fn buchung(&self, e: &Gespraech) -> Option<(usize, Cent)> {
        let nr = e.buchung?;
        let buchung = self.ki?.buchungen().get(nr)?;
        let wer = match buchung.berater {
            BeraterId::Dachs => "dachs",
            BeraterId::Maus => "maus",
        };
        (wer == e.wer).then_some((nr, buchung.reserviert))
    }

    fn aufruf(&self, nr: usize) -> Option<&KiAufruf> {
        self.ki?.aufruf(nr)
    }

    fn neurofrage(&self, e: &Gespraech) -> Option<&NeuroVersuch> {
        if e.zweck != "neurofrage" {
            return None;
        }
        let (nr, _) = self.buchung(e)?;
        self.neurofragen.get(&nr).copied()
    }
}

fn lokale_zeit(zeit: Zeitpunkt) -> String {
    zeit.in_frankfurt().map_or_else(
        || depot_lage::zeit_text(zeit),
        |(datum, stunde, minute, ..)| {
            format!("{}, {stunde:02}:{minute:02}", depot_lage::datum_text(datum))
        },
    )
}

fn rohdatei(daten: &Path, sha: &str) -> PathBuf {
    daten.join("roh/ki").join(format!("{sha}.txt"))
}

fn rohdatei_vorhanden(pfad: &Path) -> bool {
    fs::symlink_metadata(pfad).is_ok_and(|info| info.is_file())
}

pub(crate) fn rohtext(daten: &Path, nr: usize, art: &str) -> Option<Vec<u8>> {
    // Anbieterantworten sind auf 4 MiB begrenzt. Auch lokale Dateien begrenzt lesen.
    const MAX_BYTES: u64 = 4 * 1024 * 1024;
    let sha = Journal::pruefe_lesend(&daten.join("journal.txt"), |stand| {
        let beleg = stand.ki().aufruf(nr)?;
        match art {
            "frage" => beleg.frage.clone(),
            "antwort" => beleg.antwort.clone(),
            _ => None,
        }
    })
    .ok()??;
    let pfad = rohdatei(daten, &sha);
    if !rohdatei_vorhanden(&pfad) {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(pfad)
        .ok()?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (u64::try_from(bytes.len()).ok()? <= MAX_BYTES
        && depot_kern::abdruck::sha256_hex(&bytes) == sha)
        .then_some(bytes)
}

fn rohlink(s: &mut String, daten: &Path, nr: usize, art: &str, titel: &str, sha: Option<&str>) {
    let Some(sha) = sha else {
        let _ = write!(s, " · {titel}: kein Abdruck");
        return;
    };
    if rohdatei_vorhanden(&rohdatei(daten, sha)) {
        let _ = write!(
            s,
            " · <a href=\"/gespraeche/rohtext?nr={nr}&amp;art={art}\">{titel}</a>"
        );
    } else {
        let _ = write!(s, " · {titel}: Rohtext fehlt");
    }
    let _ = write!(s, " <code>{sha}</code>");
}

fn journalbeleg(s: &mut String, daten: &Path, beleg: &KiAufruf) {
    let kurz = beleg.vorlage.get(..8).unwrap_or(&beleg.vorlage);
    let _ = write!(
        s,
        "<p>Journalbeleg · Modell {} · Vorlage <code title=\"{}\">{kurz}</code>",
        maskiere(&beleg.modell),
        beleg.vorlage,
    );
    let vorlage = daten
        .join("roh/ki/vorlage")
        .join(format!("{}.txt", beleg.vorlage));
    if !rohdatei_vorhanden(&vorlage) {
        s.push_str(" (Rohtext fehlt)");
    }
    rohlink(s, daten, beleg.nr, "frage", "Frage", beleg.frage.as_deref());
    rohlink(
        s,
        daten,
        beleg.nr,
        "antwort",
        "Antwort",
        beleg.antwort.as_deref(),
    );
    s.push_str("</p>");
}

fn absatz(s: &mut String, vorspann: &str, text: &str) {
    let _ = write!(s, "<p>{vorspann}{}</p>", maskiere(text));
}

fn urteil(s: &mut String, obj: &Json, feld: &str, ja: &str, nein: &str) {
    if let Some(zustimmung) = obj.feld(feld).and_then(Json::wahr) {
        absatz(s, "", if zustimmung { ja } else { nein });
    }
}

fn handelsziele(s: &mut String, obj: &Json) {
    match obj.feld("ziele") {
        Some(Json::Null) => absatz(s, "", "Keine neuen Handelsziele."),
        Some(Json::Liste(ziele)) if ziele.is_empty() => {
            absatz(s, "", "Ziel: vollständig in Bargeld wechseln.");
        }
        Some(Json::Liste(ziele)) => {
            for ziel in ziele {
                let isin = ziel.feld("isin").and_then(Json::text);
                let gewicht = ziel.feld("gewicht_bp").and_then(Json::ganz);
                if let (Some(isin), Some(gewicht)) = (isin, gewicht) {
                    let prozent = depot_lage::html::prozent(gewicht);
                    let grund = ziel.feld("begruendung").and_then(Json::text).unwrap_or("");
                    absatz(
                        s,
                        "Ziel: ",
                        &format!("{isin} {}. {grund}", prozent.trim_start_matches(['+', '±'])),
                    );
                }
            }
        }
        _ => {}
    }
}

fn antwort_text(s: &mut String, antwort: &str) {
    let Ok(obj) = erstes_objekt(antwort) else {
        let text = antwort.trim();
        if text.is_empty() || text.contains(['{', '}']) || text.starts_with(['[', '`']) {
            absatz(s, "", "Keine lesbare Antwort belegt.");
        } else {
            absatz(s, "", text);
        }
        return;
    };
    let vorher = s.len();
    if let Some(text) = obj.feld("einschaetzung").and_then(Json::text) {
        absatz(s, "", text);
    }
    urteil(
        s,
        &obj,
        "zustimmung",
        "Stimmt dem Vorschlag zu.",
        "Lehnt den Vorschlag ab.",
    );
    urteil(
        s,
        &obj,
        "zustimmung_fremd",
        "Stimmt dem fremden Vorschlag zu.",
        "Lehnt den fremden Vorschlag ab.",
    );
    if let Some(text) = obj.feld("begruendung").and_then(Json::text) {
        absatz(s, "Begründung: ", text);
    }
    handelsziele(s, &obj);
    if let Some(prognose) = obj.feld("prognose") {
        let isin = prognose.feld("isin").and_then(Json::text);
        let richtung = prognose.feld("richtung").and_then(Json::text);
        let tage = prognose.feld("handelstage").and_then(Json::anzahl);
        if let (Some(isin), Some(richtung), Some(tage)) = (isin, richtung, tage) {
            let richtung = if richtung == "faellt" {
                "fällt"
            } else {
                richtung
            };
            absatz(
                s,
                "Prognose: ",
                &format!("{isin} {richtung} innerhalb von {tage} Handelstagen."),
            );
        }
        if let Some(text) = prognose.feld("begruendung").and_then(Json::text) {
            absatz(s, "Begründung der Prognose: ", text);
        }
        if let Some(text) = prognose.feld("widerlegung").and_then(Json::text) {
            absatz(s, "Woran sie scheitert: ", text);
        }
    }
    for meldung in obj
        .feld("meldungen")
        .and_then(Json::liste)
        .unwrap_or_default()
    {
        if let Some(text) = meldung.feld("kurz").and_then(Json::text) {
            absatz(s, "", text);
        }
    }
    if s.len() == vorher {
        absatz(s, "", "Keine lesbare Antwort belegt.");
    }
}

pub(crate) fn gemini_uebersprungen(e: &Gespraech) -> Option<&'static str> {
    use depot_kern::gemini::{
        self, JOURNAL_UNVERFUEGBAR, KEIN_HANDELSTAG, KEINE_NACHRICHTEN, WARTET_AUF_UPDATE,
    };
    if e.wer != gemini::DIENST || e.ergebnis != "uebersprungen" {
        return None;
    }
    match e.fehler.as_deref()? {
        JOURNAL_UNVERFUEGBAR => Some(JOURNAL_UNVERFUEGBAR),
        KEINE_NACHRICHTEN => Some(KEINE_NACHRICHTEN),
        KEIN_HANDELSTAG => Some(KEIN_HANDELSTAG),
        WARTET_AUF_UPDATE => Some(WARTET_AUF_UPDATE),
        _ => None,
    }
}

fn fehler_text(e: &Gespraech) -> &'static str {
    if let Some(grund) = gemini_uebersprungen(e) {
        return grund;
    }
    match e.ergebnis.as_str() {
        "guthaben" => "Beim Anbieter fehlt Guthaben. Bitte lade es im Anbieterkonto auf.",
        "abgelehnt" => {
            "Der Anbieter hat den Aufruf abgelehnt. Prüfe die Zugangsdaten in der Einrichtung."
        }
        "netz" => "Der Anbieter war nicht erreichbar.",
        "limit" => {
            "Das Limit des Anbieters ist erreicht. DepotSchlopf wartet auf den nächsten möglichen Aufruf."
        }
        "ueberlastet" => "Der Anbieter war überlastet.",
        "kosten" => {
            "Die Kostengrenze wurde überschritten. Prüfe den Kostenschutz in der Einrichtung."
        }
        "uebersprungen" => match e.fehler.as_deref().unwrap_or_default() {
            f if f.contains("Anbieterguthaben fehlt") => {
                "Die KI pausiert bis zum nächsten Tag, weil Guthaben fehlt."
            }
            f if f.contains("Schlüssel") => {
                "Kein Zugangsschlüssel verfügbar. Prüfe die Einrichtung."
            }
            f if f.contains("Anbieterlimit") => "Die KI pausiert wegen des Anbieterlimits.",
            _ => "Die KI wurde bei diesem Lauf nicht gefragt.",
        },
        _ => "Keine lesbare Antwort belegt.",
    }
}

fn sichtbarer_modellname(kennung: &str) -> Option<&str> {
    if kennung == depot_kern::gemini::MODELL {
        return Some("Gemini 3.8 Flash");
    }
    [BeraterId::Dachs, BeraterId::Maus]
        .into_iter()
        .flat_map(Modell::fuer_berater)
        .find(|modell| modell.passt_meldung(kennung))
        .map(|modell| modell.name())
}

fn modellangabe(s: &mut String, e: &Gespraech, beleg: Option<&KiAufruf>, wahl: Option<Modellwahl>) {
    let modell = if let Some(gemeldet) = beleg.map(|a| a.modell.as_str()) {
        match gemeldet.strip_suffix('?') {
            Some(auswahl) => {
                sichtbarer_modellname(auswahl).map(|name| (name, " · damalige Auswahl"))
            }
            None => Some((
                sichtbarer_modellname(gemeldet).unwrap_or("Anbietermodell"),
                "",
            )),
        }
    } else if let Some(wahl) = wahl {
        Some((wahl.modell().name(), " · damalige Auswahl"))
    } else {
        sichtbarer_modellname(&e.modell).map(|name| (name, " · laut Gesprächsprotokoll"))
    };
    if let Some((name, belegart)) = modell {
        let _ = write!(s, " · {name}{belegart}");
    }
    if let Some(wahl) = wahl {
        let _ = write!(s, " · Denktiefe {}", wahl.denktiefe().name());
    }
}

fn neurofrage(e: &Gespraech, v: &NeuroVersuch) -> Option<NeuroFrage> {
    let f = NeuroFrage::lies(e.frage.as_deref()?)?;
    (f.id == v.id && f.wer == v.wer).then_some(f)
}

fn neurofrage_text(s: &mut String, frage: &NeuroFrage) {
    absatz(s, "These: ", &frage.these);
    if let Some(bisher) = &frage.bisher {
        absatz(s, "Bisher: ", bisher);
    }
    absatz(s, "Beleg: ", &frage.jetzt);
}

fn neurourteil_text(s: &mut String, urteil: &NeuroUrteil) {
    absatz(s, "", crate::neuroauftraege::bewertungstext(urteil.wert));
    absatz(s, "Begründung: ", &urteil.begruendung);
}

fn antwort(s: &mut String, e: &Gespraech, belege: &Belege<'_>, rueckfragen: &[&Gespraech]) {
    let buchung = belege.buchung(e);
    let wahl = buchung.and_then(|(nr, _)| belege.ki?.modellwahl(nr));
    let beleg = buchung.and_then(|(nr, _)| belege.aufruf(nr));
    let neuro = belege.neurofrage(e);
    let bestaetigt = neuro
        .and_then(|v| belege.ki?.neuroergebnis(v.buchung))
        .and_then(|e| e.urteil.as_ref());
    let text = e.antwort.as_deref().filter(|_| e.ergebnis == "ok");
    let wer = maskiere(name(&e.wer));
    s.push_str("<div class=\"ki-antwort\"><p><strong>");
    if bestaetigt.is_some() || text.is_some() {
        let _ = write!(s, "{wer} antwortet");
    } else {
        let _ = write!(s, "Keine Antwort von {wer}");
    }
    s.push_str("</strong>");
    if e.wer == depot_kern::gemini::DIENST {
        s.push_str(" · Zusatz-KI");
    }
    modellangabe(s, e, beleg, wahl);
    let kosten = if e.buchung.is_none() {
        e.kosten_cent
            .and_then(|cent| i64::try_from(cent).ok())
            .map(Cent)
    } else {
        buchung
            .map(|(_, cent)| cent)
            .filter(|cent| u64::try_from(cent.0).ok() == e.kosten_cent)
    };
    if let Some(cent) = kosten {
        let _ = write!(s, " · {cent}");
    } else if e.buchung.is_some() {
        s.push_str(" · Kosten noch offen");
    }
    s.push_str("</p>");
    let unbestaetigt = text
        .zip(neuro)
        .and_then(|(text, v)| NeuroUrteil::lies(text, v.id.runde()));
    if let Some(urteil) = bestaetigt.or(unbestaetigt.as_ref()) {
        if bestaetigt.is_none() {
            absatz(s, "", "Urteil noch nicht im Journal bestätigt.");
        }
        neurourteil_text(s, urteil);
        if let Some(nachfrage) = &urteil.nachfrage {
            absatz(s, "Rückfrage: ", nachfrage);
        }
    } else if let Some(text) = text {
        antwort_text(s, text);
    } else {
        absatz(s, "", fehler_text(e));
    }
    for folge in rueckfragen {
        let _ = write!(
            s,
            "<p><strong>NeuroSchlopf fragt bei {wer} nach</strong></p>"
        );
        antwort(s, folge, belege, &[]);
    }
    s.push_str("</div>");
}

fn faden(s: &mut String, eintraege: &[&Gespraech], belege: &Belege<'_>) {
    s.push_str("<div class=\"ki-frage\">");
    let erstfragen = || {
        eintraege
            .iter()
            .filter_map(|e| Some((*e, belege.neurofrage(e)?)))
            .filter(|(_, v)| v.id.runde() == NeuroRunde::Erstfrage)
    };
    if erstfragen().next().is_some() {
        s.push_str("<p><strong>NeuroSchlopf fragt</strong> · Belege bewerten</p>");
        if let Some(frage) = erstfragen().find_map(|(e, v)| neurofrage(e, v)) {
            neurofrage_text(s, &frage);
        }
        let mut folgen = BTreeMap::<_, Vec<_>>::new();
        for &e in eintraege {
            if let Some(v) = belege.neurofrage(e)
                && v.id.runde() == NeuroRunde::Rueckfrage
            {
                folgen.entry(v.wer).or_default().push(e);
            }
        }
        for (e, v) in erstfragen() {
            let rueckfragen = folgen.remove(&v.wer);
            antwort(s, e, belege, rueckfragen.as_deref().unwrap_or_default());
        }
        for e in folgen.into_values().flatten() {
            einzelne_frage(s, e, belege);
        }
    } else {
        for &e in eintraege {
            einzelne_frage(s, e, belege);
        }
    }
    s.push_str("</div>");
}

fn einzelne_frage(s: &mut String, e: &Gespraech, belege: &Belege<'_>) {
    let wer = maskiere(name(&e.wer));
    let zweck = maskiere(zweck_text(&e.zweck));
    if let Some(v) = belege.neurofrage(e) {
        let _ = write!(
            s,
            "<p><strong>NeuroSchlopf fragt bei {wer} nach</strong> · {zweck}</p>"
        );
        if let Some(frage) = neurofrage(e, v)
            && let Some(nachfrage) = frage.nachfrage
        {
            absatz(s, "", &nachfrage);
        }
    } else {
        let fragt = if e.zweck == "neurofrage" {
            "NeuroSchlopf"
        } else {
            "DepotSchlopf"
        };
        let _ = write!(s, "<p><strong>{fragt} fragt {wer}</strong> · {zweck}</p>");
    }
    antwort(s, e, belege, &[]);
}

fn technische_details(s: &mut String, eintraege: &[&Gespraech], belege: &Belege<'_>) {
    s.push_str("<details class=\"ki-details\"><summary>Technische Details</summary>");
    for e in eintraege {
        let _ = write!(
            s,
            "<p>{} · <time datetime=\"{}\">{}</time> · Modell <code>{}</code> · Lauf <code>{}</code></p>",
            maskiere(name(&e.wer)),
            e.zeit,
            e.zeit,
            maskiere(&e.modell),
            maskiere(&e.lauf),
        );
        if let Some(beleg) = belege.buchung(e).and_then(|(nr, _)| belege.aufruf(nr)) {
            journalbeleg(s, belege.daten, beleg);
        }
    }
    s.push_str("</details>");
}

fn filter(
    s: &mut String,
    datum: Option<Datum>,
    wer: &str,
    lauf: &str,
    tage: &[Tag],
    gemini_an: bool,
) {
    let mut namen = BTreeMap::from([("dachs", "Dachs"), ("maus", "Maus")]);
    if gemini_an {
        namen.insert("gemini", name("gemini"));
    }
    for e in tage.iter().flat_map(|t| &t.eintraege) {
        namen.insert(&e.wer, name(&e.wer));
    }
    let _ = write!(
        s,
        "<form method=\"get\" action=\"/gespraeche\"><div class=\"formularfelder\"><label>Tag <input type=\"date\" name=\"tag\" value=\"{}\"></label>\
         <label>KI <select name=\"wer\"><option value=\"\">Alle KIs</option>",
        datum.map_or_else(String::new, |d| d.to_string())
    );
    for (id, titel) in namen {
        let auswahl = if id == wer { " selected" } else { "" };
        let _ = write!(
            s,
            "<option value=\"{}\"{auswahl}>{}</option>",
            maskiere(id),
            maskiere(titel)
        );
    }
    s.push_str("</select></label></div>");
    if !lauf.is_empty() {
        let _ = write!(
            s,
            "<input type=\"hidden\" name=\"lauf\" value=\"{}\"> <span>Ein Lauf ausgewählt</span>",
            maskiere(lauf)
        );
    }
    s.push_str(
        "<div class=\"aktionszeile\"><button type=\"submit\">Filtern</button> \
                <a href=\"/gespraeche\">Filter zurücksetzen</a></div></form>",
    );
}

struct Tag {
    datum: Datum,
    eintraege: Vec<Gespraech>,
    fehler: usize,
    lesefehler: bool,
}

fn tag_lesen(daten: &Path, datum: Datum) -> Tag {
    let (mut eintraege, fehler, lesefehler) = match gespraeche::lesen(daten, datum) {
        Ok((e, f)) => (e, f, false),
        Err(_) => (Vec::new(), 0, true),
    };
    eintraege.sort_by_key(|e| std::cmp::Reverse(e.zeit));
    Tag {
        datum,
        eintraege,
        fehler,
        lesefehler,
    }
}

fn aufrufe_nach_lauf(s: &mut String, aufrufe: Vec<&Gespraech>, belege: &Belege<'_>) {
    let mut laeufe = BTreeMap::<_, Vec<_>>::new();
    for e in aufrufe.into_iter().rev() {
        let gruppe = belege
            .neurofrage(e)
            .map_or((false, e.lauf.as_str()), |v| (true, v.id.fall()));
        laeufe.entry(gruppe).or_default().push(e);
    }
    for eintraege in laeufe.values_mut() {
        eintraege.sort_by_key(|e| e.zeit);
    }
    let mut laeufe: Vec<_> = laeufe.into_values().collect();
    laeufe.sort_by_key(|eintraege| std::cmp::Reverse(eintraege.first().map(|e| e.zeit)));
    for eintraege in laeufe {
        let Some(erste) = eintraege.first() else {
            continue;
        };
        let _ = write!(
            s,
            "<div class=\"ki-lauf\"><h3><time datetime=\"{}\">{}</time></h3>",
            erste.zeit,
            lokale_zeit(erste.zeit)
        );
        faden(s, &eintraege, belege);
        technische_details(s, &eintraege, belege);
        s.push_str("</div>");
    }
}

fn uebersprungene(s: &mut String, ausgelassen: Vec<&Gespraech>) {
    let mut gruppen = BTreeMap::<(&str, &str, &str), Vec<&Gespraech>>::new();
    for e in ausgelassen {
        gruppen
            .entry((&e.wer, &e.zweck, fehler_text(e)))
            .or_default()
            .push(e);
    }
    for ((wer, zweck, grund), eintraege) in gruppen {
        let _ = write!(
            s,
            "<details><summary>{} · {} · {}-mal nicht gefragt</summary><p>{}</p><ul>",
            maskiere(name(wer)),
            maskiere(zweck_text(zweck)),
            eintraege.len(),
            maskiere(grund)
        );
        for e in eintraege {
            let _ = write!(s, "<li>{}</li>", depot_lage::zeit_text(e.zeit));
        }
        s.push_str("</ul></details>");
    }
}

fn tagesstand<'a>(
    hinweise: &mut String,
    tag: &'a Tag,
    wer: &str,
    lauf: &str,
    aufrufe: &mut Vec<&'a Gespraech>,
    leer_zeigen: bool,
) {
    let datum = depot_lage::datum_text(tag.datum);
    if tag.lesefehler {
        let _ = write!(hinweise, "<p>{datum} · Gesprächsdatei nicht lesbar.</p>");
        return;
    }
    let mut ausgelassen = Vec::new();
    let vorher = aufrufe.len();
    for e in tag
        .eintraege
        .iter()
        .filter(|e| (wer.is_empty() || wer == e.wer) && (lauf.is_empty() || lauf == e.lauf))
    {
        if e.ergebnis == "uebersprungen" {
            ausgelassen.push(e);
        } else {
            aufrufe.push(e);
        }
    }
    if aufrufe.len() == vorher && ausgelassen.is_empty() && leer_zeigen {
        hinweise.push_str("<p>Keine Gespräche an diesem Tag für die Auswahl.</p>");
    }
    if !ausgelassen.is_empty() {
        let _ = write!(
            hinweise,
            "<details><summary>{datum} · Nicht gefragt ({})</summary>",
            ausgelassen.len()
        );
        uebersprungene(hinweise, ausgelassen);
        hinweise.push_str("</details>");
    }
    if tag.fehler > 0 {
        let _ = write!(
            hinweise,
            "<p>{datum} · Nicht lesbare Zeilen: {}</p>",
            tag.fehler
        );
    }
}

pub(crate) fn inhalt(
    daten: &Path,
    heute: Option<Datum>,
    datum: &str,
    wer: &str,
    lauf: &str,
) -> String {
    let mut s = String::from(
        "<section id=\"gespraeche\"><h2>KI-Gespräche</h2>\
        <p>Antworten und Begründungen von Dachs, Maus und den Zusatz-KIs.</p>",
    );
    let einzel = Datum::lies(datum);
    if !datum.is_empty() && einzel.is_none() {
        s.push_str("<p>Das gewählte Datum ist ungültig.</p></section>");
        return s;
    }
    let tage: Vec<_> = if let Some(d) = einzel {
        vec![tag_lesen(daten, d)]
    } else {
        (0_i64..7)
            .filter_map(|n| heute?.plus_tage(n.saturating_neg()))
            .map(|d| tag_lesen(daten, d))
            .collect()
    };
    let ki = if tage
        .iter()
        .flat_map(|tag| &tag.eintraege)
        .any(|e| e.buchung.is_some())
    {
        Journal::pruefe_lesend(&daten.join("journal.txt"), |stand| stand.ki().clone()).ok()
    } else {
        None
    };
    filter(
        &mut s,
        einzel,
        wer,
        lauf,
        &tage,
        gespraeche::gemini_aktiv(daten),
    );
    let belege = Belege {
        daten,
        ki: ki.as_ref(),
        neurofragen: ki
            .iter()
            .flat_map(KiStand::neurofragen)
            .map(|v| (v.buchung, v))
            .collect(),
    };
    let mut aufrufe = Vec::new();
    let mut hinweise = String::new();
    for tag in &tage {
        tagesstand(
            &mut hinweise,
            tag,
            wer,
            lauf,
            &mut aufrufe,
            einzel.is_some(),
        );
    }
    if aufrufe.is_empty() && hinweise.is_empty() {
        s.push_str("<p>Keine Gespräche für die Auswahl.</p>");
    }
    aufrufe_nach_lauf(&mut s, aufrufe, &belege);
    s.push_str(&hinweise);
    s.push_str("</section>");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ungeklärte_kosten_erscheinen_nicht_als_betrag() -> Result<(), &'static str> {
        let zeit = depot_kern::Zeitpunkt::lies("2026-11-10T10:00:00Z").ok_or("Zeit fehlt")?;
        let (datum, ..) = zeit.in_frankfurt().ok_or("Datum fehlt")?;
        let tag = Tag {
            datum,
            fehler: 0,
            lesefehler: false,
            eintraege: vec![Gespraech {
                zeit,
                lauf: "probe".into(),
                wer: "dachs".into(),
                rolle: "haupt".into(),
                modell: "probe".into(),
                zweck: "erstmeinung".into(),
                ergebnis: "unklar".into(),
                http: None,
                token_ein: None,
                token_aus: None,
                cache: None,
                buchung: Some(7),
                schlopfmodell: None,
                kosten_cent: Some(3),
                system: None,
                frage: None,
                antwort: None,
                fehler: None,
            }],
        };
        let belege = Belege {
            daten: Path::new(""),
            ki: None,
            neurofragen: BTreeMap::new(),
        };
        let mut html = String::new();
        let mut aufrufe = Vec::new();
        tagesstand(&mut html, &tag, "", "", &mut aufrufe, true);
        aufrufe_nach_lauf(&mut html, aufrufe, &belege);
        assert!(html.contains("Kosten noch offen"));
        assert!(!html.contains("0,20 €") && !html.contains("0,03 €"));
        let eintrag = tag.eintraege.first().ok_or("Gespräch fehlt")?;
        let ausgelassen = Gespraech {
            wer: "gemini".into(),
            ergebnis: "uebersprungen".into(),
            fehler: Some("Schlüssel fehlt".into()),
            ..eintrag.clone()
        };
        let tag = Tag {
            eintraege: vec![
                eintrag.clone(),
                eintrag.clone(),
                ausgelassen.clone(),
                ausgelassen,
            ],
            ..tag
        };
        let mut html = String::new();
        let mut aufrufe = Vec::new();
        tagesstand(&mut html, &tag, "", "", &mut aufrufe, true);
        aufrufe_nach_lauf(&mut html, aufrufe, &belege);
        assert_eq!(html.matches("<h3><time").count(), 1);
        assert_eq!(html.matches("Kein Zugangsschlüssel verfügbar").count(), 1);
        assert!(html.contains("2-mal nicht gefragt"));
        Ok(())
    }
}
