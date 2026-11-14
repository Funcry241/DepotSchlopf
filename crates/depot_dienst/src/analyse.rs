//! Lokaler, lesender Export von Frankfurter Betriebstagen.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{ErrorKind, Read as _};
use std::path::Path;

use depot_export::Zip;
use depot_journal::{Journal, JournalFehler, Stand};
use depot_kern::json::{Json, erstes_objekt};
use depot_kern::{BeraterId, Cent, Datum, Isin, Zeitpunkt, abdruck::sha256_hex};
use depot_lage::gespraeche::Gespraech;
use depot_lage::{datum_text, zeit_text};

use crate::{Dienst, Umwelt, gespraeche, http, protokoll::Schutz, schluessel, seiten, verlauf};

mod historie;
mod journal;
mod pruefbericht;
mod schutz;

const MIB: u64 = 1024 * 1024;
const JOURNAL_MAX: u64 = 48 * MIB;
const TEXT_MAX: u64 = 2 * MIB;
const ZIP_MAX: usize = 25 * 1024 * 1024;

const KI_LESEN_MAX: u64 = 8 * MIB;
const MAX_TAGE: u8 = 31;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Zeitraum {
    von: Datum,
    bis: Datum,
}

impl Zeitraum {
    fn aus_formular(heute: Datum, felder: &[(String, String)]) -> Result<Self, &'static str> {
        let feld = |name| http::feld(felder, name);
        let wahl = feld("zeitraum")
            .or_else(|| (feld("von").is_none() && feld("bis").is_none()).then_some("woche"));
        let (von, bis) = match wahl {
            Some("heute") => (Some(heute), Some(heute)),
            Some("gestern") => (heute.plus_tage(-1), heute.plus_tage(-1)),
            Some("woche") => (
                heute
                    .wochentag()
                    .and_then(|w| heute.plus_tage(1_i64.saturating_sub(i64::from(w)))),
                Some(heute),
            ),
            Some("handelstage") => {
                let mut handelstage = std::iter::successors(Some(heute), |tag| tag.plus_tage(-1))
                    .take(usize::from(MAX_TAGE))
                    .filter(|tag| depot_kern::liste::handelstag(*tag));
                let bis = handelstage.next();
                (handelstage.nth(3), bis)
            }
            Some("frei") => (
                feld("von").and_then(Datum::lies),
                feld("bis").and_then(Datum::lies),
            ),
            _ => return Err("Wähle einen Zeitraum für den Analyse-Export."),
        };
        let (Some(von), Some(bis)) = (von, bis) else {
            return Err("Der Zeitraum braucht ein gültiges Von- und Bis-Datum.");
        };
        if von > bis {
            return Err("Das Von-Datum muss vor oder am Bis-Datum liegen.");
        }
        if bis > heute {
            return Err("Das Bis-Datum darf höchstens heute sein.");
        }
        if bis
            .plus_tage(1_i64.saturating_sub(i64::from(MAX_TAGE)))
            .is_some_and(|frueh| von < frueh)
        {
            return Err("Der Analyse-Export umfasst höchstens 31 Kalendertage.");
        }
        Ok(Self { von, bis })
    }

    fn enthaelt(self, zeit: Zeitpunkt) -> bool {
        zeit.in_frankfurt()
            .is_some_and(|(tag, ..)| (self.von..=self.bis).contains(&tag))
    }

    fn tage(self) -> impl Iterator<Item = Datum> {
        std::iter::successors(Some(self.von), |tag| tag.plus_tage(1))
            .take_while(move |tag| *tag <= self.bis)
    }

    fn titel(self) -> String {
        if self.von == self.bis {
            datum_text(self.von)
        } else {
            format!("{} bis {}", datum_text(self.von), datum_text(self.bis))
        }
    }

    fn dateiname(self) -> String {
        if self.von == self.bis {
            format!("DepotSchlopf_Analyse_{}.zip", self.von)
        } else {
            format!("DepotSchlopf_Analyse_{}_bis_{}.zip", self.von, self.bis)
        }
    }
}

pub(crate) fn formular(d: &Dienst, heute: Option<Datum>) -> String {
    let Some(heute) = heute else {
        return String::new();
    };
    let token = depot_lage::html::maskiere(&d.token);
    format!(
        "<form method=\"post\" action=\"/export\" data-download><input type=\"hidden\" name=\"t\" value=\"{token}\">\
         <button type=\"submit\" name=\"zeitraum\" value=\"woche\">Diese Woche exportieren</button>\
         </form><details id=\"analyse-zeitraum\"><summary>Zeitraum exportieren</summary>\
         <form method=\"post\" action=\"/export\" data-download><input type=\"hidden\" name=\"t\" value=\"{token}\">\
         <button type=\"submit\" name=\"zeitraum\" value=\"heute\" formnovalidate>Heute</button> \
         <button type=\"submit\" name=\"zeitraum\" value=\"gestern\" formnovalidate>Gestern</button> \
         <button type=\"submit\" name=\"zeitraum\" value=\"handelstage\" formnovalidate>Letzte 5 Handelstage</button> \
         <button type=\"submit\" name=\"zeitraum\" value=\"woche\" formnovalidate>Diese Woche</button>\
         <p><label>Von <input type=\"date\" name=\"von\" value=\"{heute}\" required></label> \
         <label>Bis <input type=\"date\" name=\"bis\" value=\"{heute}\" required></label> \
         <button type=\"submit\" name=\"zeitraum\" value=\"frei\">Zeitraum exportieren</button></p>\
         <p>Frankfurter Datum, höchstens 31 Kalendertage.</p></form></details>"
    )
}

fn geld(wert: Option<Cent>) -> String {
    wert.map_or_else(|| "nicht belegt".into(), |wert| wert.to_string())
}

fn prozent(bp: Option<i64>) -> String {
    bp.map_or_else(
        || "nicht belegt".into(),
        |bp| {
            let betrag = bp.unsigned_abs();
            let vorzeichen = if bp < 0 { "-" } else { "" };
            format!(
                "{vorzeichen}{},{:02} %",
                betrag.div_euclid(100),
                betrag.rem_euclid(100)
            )
        },
    )
}

fn textdatei(pfad: &Path) -> Result<Option<String>, &'static str> {
    match fs::symlink_metadata(pfad) {
        Ok(m) if !m.is_file() => return Err("keine reguläre Datei"),
        Ok(m) if m.len() > TEXT_MAX => return Err("Lesegrenze überschritten"),
        Ok(_) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("unlesbar"),
    }
    let mut text = String::new();
    File::open(pfad)
        .and_then(|f| f.take(TEXT_MAX.saturating_add(1)).read_to_string(&mut text))
        .map_err(|_| "unlesbar")?;
    if u64::try_from(text.len()).unwrap_or(u64::MAX) > TEXT_MAX {
        return Err("Lesegrenze überschritten");
    }
    Ok(Some(text))
}

fn quelle(pfad: &Path, name: &str, luecken: &mut String) -> String {
    match textdatei(pfad) {
        Ok(Some(text)) => text,
        Ok(None) => {
            let _ = writeln!(luecken, "- {name}: keine Datei vorhanden.");
            String::new()
        }
        Err(grund) => {
            let _ = writeln!(luecken, "- {name}: {grund}.");
            String::new()
        }
    }
}

fn betrieb(d: &Dienst, daten: &Path, zeitraum: Zeitraum, luecken: &mut String) -> String {
    let z = d.sperre();
    let original = quelle(&verlauf::pfad(&d.wurzel), "Dienstverlauf", luecken);
    let mut verlauf = verlauf::lies_zeitraum(&original, zeitraum.von, zeitraum.bis);
    if let Some(lauf) = z.laufend.as_ref().filter(|a| zeitraum.enthaelt(a.start)) {
        verlauf.auftraege.retain(|a| a.kennung != lauf.kennung);
        verlauf.auftraege.insert(0, lauf.clone());
    }
    let laufend = z.laufend.as_ref().map(|a| a.kennung.clone());
    drop(z);
    let mut aus = "# Betrieb und Abrufe\n\n## Dienstverlauf\n".to_owned();
    let lauftage: BTreeSet<_> = verlauf
        .auftraege
        .iter()
        .flat_map(|lauf| [Some(lauf.start), lauf.ende])
        .flatten()
        .filter_map(|zeit| zeit.in_frankfurt().map(|(tag, ..)| tag))
        .collect();
    for tag in zeitraum.tage().filter(|tag| !lauftage.contains(tag)) {
        let _ = writeln!(
            aus,
            "- {}: kein Lauf im verfügbaren Verlauf belegt.",
            datum_text(tag)
        );
    }
    for lauf in verlauf.auftraege {
        let status = if laufend.as_ref() == Some(&lauf.kennung) {
            "läuft gerade"
        } else {
            match (lauf.frueher, lauf.erfolg) {
                (crate::werkbank::Frueher::Unterbrochen, _) => "unterbrochen",
                (_, Some(true)) => "erfolgreich",
                (_, Some(false)) => "Fehler",
                _ => "Ergebnis unbekannt",
            }
        };
        let ende = lauf.ende.map_or_else(|| "offen".into(), zeit_text);
        let _ = writeln!(
            aus,
            "- {} bis {ende} · {} · {:?} · {status} · {}",
            zeit_text(lauf.start),
            lauf.art.name(),
            lauf.anlass,
            lauf.kennung
        );
    }
    let _ = writeln!(
        aus,
        "\nUnlesbare Verlaufszeilen: {}. Der Dienst kann ältere Einträge bereits gekürzt haben.",
        verlauf.unlesbar
    );
    let text = quelle(
        &daten.join(depot_lage::anbieter::DATEI),
        "Anbieterprotokoll",
        luecken,
    );
    let (eintraege, unlesbar) = depot_lage::anbieter::lies_alle(&text);
    aus.push_str("\n## Anbieterprotokoll\n");
    for e in eintraege.into_iter().filter(|e| zeitraum.enthaelt(e.zeit)) {
        let http = e.http.map_or_else(|| "–".into(), |s| s.to_string());
        let _ = writeln!(
            aus,
            "- {} · {} · {} · HTTP {http}",
            zeit_text(e.zeit),
            e.wer.kurz(),
            e.ergebnis.kurz()
        );
    }
    let _ = writeln!(aus, "\nUnlesbare Anbieterzeilen: {unlesbar}.");
    aus
}

fn konfiguration(d: &Dienst, u: &dyn Umwelt, daten: &Path) -> String {
    let plan = d.sperre().zeitplan;
    let mut aus = format!(
        "# Aktuelle Konfiguration\n\n- Zeitplan: {}\n- Termine: {:02}:{:02}\n- Xetra: {:02}:{:02}\n\
         - Papiertag nach Kursabruf: {}\n- Schlagzeilen: {}\n- Autostart: {:?}\n- Gemini: {}\n",
        an_aus(plan.an),
        plan.termine.0,
        plan.termine.1,
        plan.kurse.0,
        plan.kurse.1,
        an_aus(plan.papier_danach),
        an_aus(plan.nachrichten_an),
        u.autostart(&d.wurzel),
        an_aus(
            textdatei(&daten.join("gemini.txt"))
                .ok()
                .flatten()
                .as_deref()
                == Some("gratisprojekt=ja\n")
        ),
    );
    for wer in schluessel::ALLE
        .into_iter()
        .chain([schluessel::Wer::Gemini])
    {
        let status = match u.schluessel(wer) {
            depot_lage::Hinterlegt::Ja => "hinterlegt",
            depot_lage::Hinterlegt::Nein => "fehlt",
            depot_lage::Hinterlegt::Unbekannt => "unbekannt",
        };
        let _ = writeln!(aus, "- {}: {status}", wer.name());
    }
    for grenze in depot_lage::grenzwerte() {
        let _ = writeln!(aus, "- {}: {}", grenze.name, grenze.wert);
    }
    let _ = writeln!(
        aus,
        "- KI-Sperre: {}\n- Kurssperre: {}",
        an_aus(daten.join("ki.halt").exists()),
        an_aus(daten.join("kurse.halt").exists())
    );
    aus
}

fn an_aus(an: bool) -> &'static str {
    if an { "an" } else { "aus" }
}

pub(crate) fn letzter_boersenschluss(jetzt: Zeitpunkt, tag: Datum) -> Option<Datum> {
    depot_kern::liste::boersen()
        .into_iter()
        .filter_map(|boerse| {
            if boerse.handelstag(tag) && boerse.schluss(tag).is_some_and(|schluss| schluss <= jetzt)
            {
                Some(tag)
            } else {
                boerse.bezugstag(tag)
            }
        })
        .max()
}

fn depot(
    stand: &Stand,
    jetzt: Zeitpunkt,
    tag: Datum,
    sperren: &str,
    zeitraum: Zeitraum,
) -> (String, String, String) {
    let Ok(werte) = depot_kern::liste::beobachtungsliste() else {
        return (
            "Depotstand nicht verfügbar: Beobachtungsliste unlesbar.\n".into(),
            String::new(),
            String::new(),
        );
    };
    let lage = depot_lage::depotstand(stand, &werte);
    let letzter = letzter_boersenschluss(jetzt, tag);
    let kurstag = lage.kurse_vom.map_or_else(
        || "nicht belegt".into(),
        |d| {
            format!(
                "{}{}",
                datum_text(d),
                if letzter.is_some_and(|l| d < l) {
                    " (veraltet)"
                } else {
                    ""
                }
            )
        },
    );
    let differenz = |basis: Option<Cent>| {
        let delta = lage
            .gesamt
            .zip(basis)
            .and_then(|(jetzt, vorher)| jetzt.checked_sub(vorher));
        let bp = delta
            .zip(basis)
            .and_then(|(delta, vorher)| delta.in_bp_von(vorher));
        format!("{} ({})", geld(delta), prozent(bp))
    };
    let tagesergebnis = match lage.tagesergebnis {
        depot_lage::Tagesergebnis::OhneKurstag => "nicht belegt".into(),
        depot_lage::Tagesergebnis::OhneVortag => "noch kein Vortag".into(),
        depot_lage::Tagesergebnis::NichtBewertbar(tag) => {
            format!("am {} nicht bewertbar", datum_text(tag))
        }
        depot_lage::Tagesergebnis::Belegt { tag, betrag, bp } => {
            format!(
                "{} ({}) am {}",
                geld(Some(betrag)),
                prozent(bp),
                datum_text(tag)
            )
        }
    };
    let mut kurz = format!(
        "Stand {} · Kurse vom {kurstag}\n\n- Gesamtwert: {}\n- Tagesergebnis: {tagesergebnis}\n- Offene Orders: {}\n",
        zeit_text(jetzt),
        geld(lage.gesamt),
        stand.offene_auftraege().count()
    );
    for topf in &lage.toepfe {
        let frei = topf.frei;
        let _ = writeln!(
            kurz,
            "- Topf {:?}: {} Bargeld, {} frei für Käufe",
            topf.topf,
            geld(Some(topf.bargeld)),
            geld(frei)
        );
    }
    let _ = writeln!(
        kurz,
        "- Höchststand: {} · Abstand: {}",
        geld(lage.hoechststand),
        differenz(lage.hoechststand)
    );
    let notbremse = stand
        .papier_halt()
        .map_or_else(|| "aus".into(), depot_journal::PapierHalt::meldung);
    let _ = writeln!(kurz, "- Notbremse: {notbremse}{sperren}");
    let details = depotdetails(stand, &lage, &werte);
    (kurz, details, orderbewegungen(stand, &werte, zeitraum))
}

fn depotdetails(
    stand: &Stand,
    lage: &depot_lage::DepotLage,
    werte: &[depot_kern::liste::Wert],
) -> String {
    let mut details = String::from(
        "\n## Bestände\n\n| Topf | Wertpapier | ISIN | Stück | Kurs EUR | Wert | Einstand / G&V |\n|---|---|---|---:|---:|---:|---|\n",
    );
    for topf in &lage.toepfe {
        for p in &topf.positionen {
            let kurs = p
                .kurs
                .map_or_else(|| "nicht belegt".into(), depot_kern::Preis::euro_text);
            let einstand = p.einstand.map_or_else(
                || "nicht belegt / –".into(),
                |e| format!("{} / {} ({})", geld(Some(e)), geld(p.gv), prozent(p.gv_bp)),
            );
            let _ = writeln!(
                details,
                "| {:?} | {} | {} | {} | {kurs} | {} | {einstand} |",
                topf.topf,
                p.name,
                p.isin,
                p.stueck,
                geld(p.wert)
            );
        }
    }
    details.push_str("\n## Offene Papierorders\n");
    for a in stand.offene_auftraege() {
        let order = depot_lage::auftrag_zeile(a, werte);
        let _ = writeln!(
            details,
            "- #{} · {} · {} · {} Stück offen · Limit {} · {} · {}",
            order.nr,
            order.name,
            order.richtung,
            a.offen_stueck(),
            geld(Some(order.limit)),
            order.zustand,
            order.anlass
        );
    }
    if let Some(v) = &lage.vergleich {
        let _ = writeln!(
            details,
            "\n## Vergleich seit {} · bewertet am {}",
            datum_text(v.start),
            datum_text(v.bewertet)
        );
        for z in &v.zeilen {
            let _ = writeln!(
                details,
                "- {}: {} · {}",
                z.name,
                geld(z.wert),
                prozent(z.rendite_bp)
            );
        }
    }
    details
}

fn orderbewegungen(stand: &Stand, werte: &[depot_kern::liste::Wert], zeitraum: Zeitraum) -> String {
    let mut details = String::new();
    details.push_str("\n## Im Zeitraum belegte Orderbewegungen\n");
    for a in stand.auftraege() {
        let versand = a.versand.filter(|z| zeitraum.enthaelt(*z));
        let fuellungen = || {
            a.ausfuehrungen
                .iter()
                .filter(|f| f.kurszeit.is_some_and(|z| zeitraum.enthaelt(z)))
        };
        if versand.is_some() || fuellungen().next().is_some() {
            let order = depot_lage::auftrag_zeile(a, werte);
            let _ = writeln!(
                details,
                "- #{} · {} · {} · {} Stück · Limit {} · aktueller Zustand: {}",
                order.nr,
                order.name,
                order.richtung,
                order.stueck,
                geld(Some(order.limit)),
                order.zustand
            );
            if let Some(zeit) = versand {
                let _ = writeln!(details, "  - Versand: {}", zeit_text(zeit));
            }
            for f in fuellungen() {
                let _ = writeln!(
                    details,
                    "  - {} · {} Stück zu {} · Gebühr {}",
                    f.kurszeit.map_or_else(|| "nicht belegt".into(), zeit_text),
                    f.stueck,
                    geld(Some(f.kurs)),
                    geld(Some(f.gebuehr))
                );
            }
        }
    }
    details.push_str("\nDie Tagesliste verwendet belegte Versand- und Kurszeitpunkte. Bewegungen ohne Zeitbeleg lassen sich keinem Tag zuordnen.\n");
    details
}

fn feste_kennung(text: &str) -> &str {
    if !text.is_empty()
        && text.len() <= 80
        && text
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    {
        text
    } else {
        "unbekannt"
    }
}

/// Exportiert nur typisierte Teile einer KI-Antwort. Freie Texte bleiben lokal.
fn ki_entscheidung(aus: &mut String, antwort: &str) {
    let Ok(objekt) = erstes_objekt(antwort) else {
        return;
    };
    for (feld, titel) in [
        ("zustimmung", "Zustimmung"),
        ("zustimmung_fremd", "Zustimmung zum anderen Vorschlag"),
    ] {
        if let Some(wert) = objekt.feld(feld).and_then(Json::wahr) {
            let _ = writeln!(aus, "  - {titel}: {}", if wert { "ja" } else { "nein" });
        }
    }
    for ziel in objekt
        .feld("ziele")
        .and_then(Json::liste)
        .unwrap_or_default()
    {
        let isin = ziel
            .feld("isin")
            .and_then(Json::text)
            .and_then(|text| Isin::neu(text).ok());
        let gewicht = ziel.feld("gewicht_bp").and_then(Json::ganz);
        if let (Some(isin), Some(gewicht)) = (isin, gewicht) {
            let _ = writeln!(
                aus,
                "  - Zielgewicht {}: {}",
                isin.als_str(),
                prozent(Some(gewicht))
            );
        }
    }
}

/// Liest den belegten Gesprächsausschnitt aus der gespeicherten Abstimmungsfrage.
fn fremde_erstmeinung(aus: &mut String, frage: &str) {
    let zitat = frage
        .lines()
        .find_map(|zeile| zeile.strip_prefix("Fremde fixierte Erst- und Zweitmeinung: "));
    let Some(Ok(Json::Text(text))) = zitat.map(depot_kern::json::lies) else {
        return;
    };
    let _ = writeln!(aus, "\n**Fremde fixierte Erst- und Zweitmeinung**");
    for zeile in text.lines().filter(|zeile| !zeile.trim().is_empty()) {
        let _ = writeln!(aus, "    {zeile}");
    }
}

fn ki_aufruf(aus: &mut String, e: &Gespraech, tag: Datum, stand: Option<&Stand>) {
    let kosten = stand.and_then(|s| {
        let b = s.ki().buchungen().get(e.buchung?)?;
        let wer = match b.berater {
            BeraterId::Dachs => "dachs",
            BeraterId::Maus => "maus",
        };
        (wer == e.wer && b.datum == tag).then_some(b.reserviert)
    });
    let http = e.http.map_or_else(|| "–".into(), |s| s.to_string());
    let token = |n: Option<u64>| n.map_or_else(|| "–".into(), |n| n.to_string());
    let _ = writeln!(
        aus,
        "\n#### {} · {} · {}\n\n- {} · {} · HTTP {http} · Token {}/{} · Journalbuchung {}",
        zeit_text(e.zeit),
        gespraeche::name(feste_kennung(&e.wer)),
        feste_kennung(&e.modell),
        gespraeche::zweck_text(feste_kennung(&e.zweck)),
        gespraeche::status(feste_kennung(&e.ergebnis)),
        token(e.token_ein),
        token(e.token_aus),
        geld(kosten)
    );
    if let Some(antwort) = e.antwort.as_deref() {
        ki_entscheidung(aus, antwort);
    }
    if e.zweck == "abstimmung"
        && let Some(frage) = e.frage.as_deref()
    {
        fremde_erstmeinung(aus, frage);
    }
}

fn ki_laeufe(aus: &mut String, tag: Datum, stand: Option<&Stand>, eintraege: &[Gespraech]) {
    let mut laeufe = BTreeMap::<_, Vec<_>>::new();
    for e in eintraege.iter().filter(|e| {
        e.zeit
            .in_frankfurt()
            .is_some_and(|(datum, ..)| datum == tag)
    }) {
        laeufe.entry(e.lauf.as_str()).or_default().push(e);
    }
    if laeufe.is_empty() {
        aus.push_str("- Keine lesbaren KI-Einträge für diesen Tag belegt.\n");
    }
    for (lauf, mut aufrufe) in laeufe {
        aufrufe.sort_by_key(|e| e.zeit);
        let _ = writeln!(aus, "\n### Lauf {}", feste_kennung(lauf));
        for e in aufrufe {
            ki_aufruf(aus, e, tag, stand);
        }
    }
}

fn ki(daten: &Path, zeitraum: Zeitraum, stand: Option<&Stand>, luecken: &mut String) -> String {
    let mut aus = "# KI-Gespräche\n".to_owned();
    let mut lesebudget = KI_LESEN_MAX;
    for tag in zeitraum.tage() {
        ki_tag(&mut aus, daten, tag, stand, &mut lesebudget, luecken);
    }
    aus
}

fn ki_tag(
    aus: &mut String,
    daten: &Path,
    tag: Datum,
    stand: Option<&Stand>,
    lesebudget: &mut u64,
    luecken: &mut String,
) {
    let datum = datum_text(tag);
    let _ = writeln!(aus, "\n## {datum}");
    let gespr = depot_lage::gespraeche::lesen_begrenzt(daten, tag, lesebudget);
    if let Ok(Some((eintraege, fehler))) = gespr {
        ki_laeufe(aus, tag, stand, &eintraege);
        let _ = writeln!(aus, "\nUnlesbare Gesprächszeilen: {fehler}.");
    } else {
        let grund = if gespr.is_err() {
            "Tagesdatei unlesbar oder oberhalb der Lesegrenze."
        } else {
            "keine Tagesdatei vorhanden."
        };
        let _ = writeln!(aus, "- KI-Gespräche: {grund}");
        let _ = writeln!(luecken, "- KI-Gespräche am {datum}: {grund}");
    }
    if let Some(s) = stand {
        aus.push_str("\n### Tagesbuchungen im Journal\n");
        for wer in [BeraterId::Dachs, BeraterId::Maus] {
            let summe = s
                .ki()
                .buchungen()
                .iter()
                .filter(|b| b.datum == tag && b.berater == wer)
                .try_fold(Cent::NULL, |summe, b| summe.checked_add(b.reserviert));
            let _ = writeln!(aus, "- {wer:?}: {}", geld(summe));
        }
        aus.push_str("\nOffene Reservierungen zählen mit ihrem gebuchten Betrag. Anbieterfehler allein ändern die Buchung nicht.\n");
    } else {
        aus.push_str("\nJournalbuchungen unverifiziert.\n");
    }
}

fn archiv<'a>(
    schutz: &schutz::Exportschutz<'_>,
    zeitraum: Zeitraum,
    jetzt: Zeitpunkt,
    liesmich: &str,
    dateien: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<Vec<u8>, String> {
    let mut manifest = if zeitraum.von == zeitraum.bis {
        format!(
            "depotschlopf-analyse-v1\ntag={}\nerstellt_utc={jetzt}\n",
            zeitraum.von
        )
    } else {
        format!(
            "depotschlopf-analyse-v1\nvon={}\nbis={}\nerstellt_utc={jetzt}\n",
            zeitraum.von, zeitraum.bis
        )
    };
    let mut zip = Zip::default();
    let mut summe = 0_usize;
    let mut bericht = String::from("\n## Schwärzungsbericht\n\n");
    let mut schreibe = |name: &str, text: &str| {
        summe = summe.saturating_add(text.len());
        if summe > ZIP_MAX {
            return Err("Analyse überschreitet die Größenbegrenzung.".into());
        }
        let _ = writeln!(
            manifest,
            "{name};{};{}",
            text.len(),
            sha256_hex(text.as_bytes())
        );
        zip.datei_gepackt(name, text.as_bytes())
    };
    let mut bereinigt = Vec::new();
    let mut zeilen_verdeckt = 0_usize;
    let mut groesse = 0_usize;
    for (name, text) in dateien {
        let jsonl = Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"));
        let (text, zeilen) = schutz.text(text, jsonl);
        if schutz.text(&text, jsonl).1 != 0 {
            return Err("Musterprüfung des Exports fehlgeschlagen.".into());
        }
        groesse = groesse.saturating_add(text.len());
        if groesse > ZIP_MAX {
            return Err("Analyse überschreitet die Größenbegrenzung.".into());
        }
        zeilen_verdeckt = zeilen_verdeckt.saturating_add(zeilen);
        let _ = writeln!(bericht, "- {name}: {zeilen} Zeilen geschwärzt.");
        bereinigt.push((name, text));
    }
    let (mut liesmich, zeilen) = schutz.text(liesmich, false);
    zeilen_verdeckt = zeilen_verdeckt.saturating_add(zeilen);
    let _ = writeln!(bericht, "- LIESMICH.md: {zeilen} Zeilen geschwärzt.");
    liesmich.push_str(&bericht);
    for (name, mut text) in bereinigt {
        if name == "PRUEFBERICHT.md" && zeilen_verdeckt != 0 {
            text.push_str("\n## Durch Schwärzung fehlende Belege\n\n");
            let _ = writeln!(
                text,
                "Gelb: {zeilen_verdeckt} Zeilen fehlen durch Schwärzung."
            );
            text.push_str(&bericht);
            if schutz.text(&text, false).1 != 0 {
                return Err("Musterprüfung des Prüfberichts fehlgeschlagen.".into());
            }
        }
        schreibe(name, &text)?;
    }
    if schutz.text(&liesmich, false).1 != 0 {
        return Err("Musterprüfung des Schwärzungsberichts fehlgeschlagen.".into());
    }
    schreibe("LIESMICH.md", &liesmich)?;
    if schutz.text(&manifest, false).1 != 0 {
        return Err("Musterprüfung des Manifests fehlgeschlagen.".into());
    }
    zip.datei_gepackt("MANIFEST.txt", manifest.as_bytes())?;
    let zip = zip.fertig()?;
    if zip.len() > ZIP_MAX {
        return Err("ZIP überschreitet die Größenbegrenzung.".into());
    }
    Ok(zip)
}

fn programmstand(version: Option<&str>) -> String {
    let version = version
        .filter(|v| v.len() == 40 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        .unwrap_or("unbekannt");
    format!(
        "# Programmstand\n\n- Commit: {version}\n- Version: {}\n- System: {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    )
}

struct Journalpaket {
    pruefbericht: String,
    kurz: String,
    depotdetails: String,
    orders: String,
    ausfuehrungen: String,
    ki: String,
    historie: historie::Historie,
    journal: journal::Auszug,
}

fn ausfuehrungen_csv(stand: Option<&Stand>, zeitraum: Zeitraum) -> String {
    use depot_lage::ausfuehrung::{Richtung, ausfuehrungsbuch};
    let mut csv = String::from(
        "Nr;Topf;ISIN;Richtung;Stück;Versand UTC;Frist UTC;Zustand;Bezug EUR;Limit EUR;Limit bp;Füllpreis EUR;Füllpreis bp;Schluss EUR;Schluss bp\n",
    );
    let Some(stand) = stand else { return csv };
    let preis = |wert: Option<depot_kern::Preis>| {
        wert.map_or_else(String::new, depot_kern::Preis::euro_text)
    };
    let bp = |wert: Option<i64>| wert.map_or_else(String::new, |wert| wert.to_string());
    let zeit = |wert: Option<Zeitpunkt>| wert.map_or_else(String::new, |wert| wert.to_string());
    for z in ausfuehrungsbuch(stand) {
        let fuellung = stand.auftrag(z.nr).is_some_and(|a| {
            a.ausfuehrungen
                .iter()
                .any(|f| f.kurszeit.is_some_and(|t| zeitraum.enthaelt(t)))
        });
        if !fuellung
            && !z.versand.is_some_and(|t| zeitraum.enthaelt(t))
            && !z.frist.is_some_and(|t| zeitraum.enthaelt(t))
        {
            continue;
        }
        let richtung = match z.richtung {
            Richtung::Kauf => "Kauf",
            Richtung::Verkauf => "Verkauf",
        };
        let _ = writeln!(
            csv,
            "{};{:?};{};{richtung};{};{};{};{};{};{};{};{};{};{};{}",
            z.nr,
            z.topf,
            z.isin,
            z.stueck,
            zeit(z.versand),
            zeit(z.frist),
            depot_lage::zustand_text(z.zustand),
            preis(z.bezug),
            preis(depot_kern::Preis::aus_cent(z.limit)),
            bp(z.limit_bp),
            preis(z.fuellpreis),
            bp(z.fuellpreis_bp),
            preis(z.schluss),
            bp(z.schluss_bp)
        );
    }
    csv
}

fn journalpaket(
    daten: &Path,
    jetzt: Zeitpunkt,
    tag: Datum,
    zeitraum: Zeitraum,
    sperren: &str,
    luecken: &mut String,
) -> Result<Journalpaket, JournalFehler> {
    match Journal::pruefe_lesend_mit_eintraegen(
        &daten.join("journal.txt"),
        JOURNAL_MAX,
        |stand, eintraege, _| {
            let (kurz, depotdetails, orders) = depot(stand, jetzt, tag, sperren, zeitraum);
            Journalpaket {
                pruefbericht: pruefbericht::bericht(Some(stand), zeitraum),
                kurz,
                depotdetails,
                orders,
                ausfuehrungen: ausfuehrungen_csv(Some(stand), zeitraum),
                ki: ki(daten, zeitraum, Some(stand), luecken),
                historie: historie::bericht(zeitraum, Some(stand), luecken),
                journal: journal::auszug(zeitraum, stand, eintraege),
            }
        },
    ) {
        Ok(paket) => Ok(paket),
        Err(JournalFehler::Belegt) => Err(JournalFehler::Belegt),
        Err(_) => {
            luecken.push_str(
                "- Journalstand nicht bestätigt. Order-, Depot- und Kostenzahlen fehlen.\n",
            );
            Ok(Journalpaket {
                pruefbericht: pruefbericht::bericht(None, zeitraum),
                kurz: "Depotstand nicht verfügbar: Journal-Kette nicht bestätigt.\n".into(),
                depotdetails: String::new(),
                orders: "Unverifiziert: Journal-Kette nicht bestätigt.\n".into(),
                ausfuehrungen: ausfuehrungen_csv(None, zeitraum),
                ki: ki(daten, zeitraum, None, luecken),
                historie: historie::bericht(zeitraum, None, luecken),
                journal: journal::Auszug::default(),
            })
        }
    }
}

fn exportschutz(token: &str, daten: &Path) -> Result<(Schutz, Option<String>), &'static str> {
    let schluessel = schluessel::fuer_protokoll()
        .map_err(|()| "Schlüsselablage nicht lesbar. Export abgebrochen.")?;
    let mut schutz = Schutz::default();
    for geheim in schluessel
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(token))
    {
        schutz.merken(geheim);
    }
    let kontakt = crate::werkbank::lies_falls_da(&daten.join("sec_kontakt.txt"))
        .map_err(|_| "SEC-Kontakt nicht lesbar. Export abgebrochen.")?;
    Ok((schutz, kontakt))
}

fn sperrentext(daten: &Path) -> String {
    format!(
        " · KI-Sperre: {} · Kurssperre: {}",
        an_aus(daten.join("ki.halt").exists()),
        an_aus(daten.join("kurse.halt").exists())
    )
}

pub(crate) fn exportieren(
    d: &Dienst,
    u: &dyn Umwelt,
    jetzt: Zeitpunkt,
    felder: &[(String, String)],
) -> Vec<u8> {
    let Some((tag, ..)) = jetzt.in_frankfurt() else {
        return seiten::fehler(409, "Frankfurter Datum nicht verfügbar.");
    };
    let zeitraum = match Zeitraum::aus_formular(tag, felder) {
        Ok(zeitraum) => zeitraum,
        Err(grund) => return seiten::fehler(400, grund),
    };
    let daten = d.wurzel.join("daten");
    if !fs::symlink_metadata(&daten).is_ok_and(|m| m.is_dir()) {
        return seiten::fehler(
            409,
            "Datenordner fehlt oder ist kein reguläres Verzeichnis.",
        );
    }
    let (schutz, sec_kontakt) = match exportschutz(&d.token, &daten) {
        Ok(schutz) => schutz,
        Err(grund) => return seiten::fehler(409, grund),
    };
    let mut luecken = String::new();
    let betrieb = betrieb(d, &daten, zeitraum, &mut luecken);
    let konfiguration = konfiguration(d, u, &daten);
    let sperren = sperrentext(&daten);
    let Ok(Journalpaket {
        pruefbericht,
        kurz,
        depotdetails,
        orders,
        ausfuehrungen,
        ki,
        historie,
        journal,
    }) = journalpaket(&daten, jetzt, tag, zeitraum, &sperren, &mut luecken)
    else {
        return seiten::fehler(
            409,
            "Journal gerade in Benutzung. Bitte gleich noch einmal.",
        );
    };
    let datum = zeitraum.titel();
    let liesmich = format!(
        "# DepotSchlopf · Analyse-Export {datum}\n\n## Aktueller Depotstand\n{kurz}\n\
         ## Datenbasis\nPapiergeld. Die Zeiten sind Frankfurter Ortszeit. \
         Depotstand und Konfiguration beziehen sich auf den Exportzeitpunkt.\n\
         Rohdaten, Laufprotokolle und vollständige Prompts sind nie enthalten.\n\
         Alpha-Vantage- und Tiingo-Schlüssel ohne festes Präfix erkennt der Export am hinterlegten Wert.\n\
         ## Lücken\n{luecken}"
    );
    let dateien = [
        (
            "01_ueberblick.md",
            format!(
                "# Aktueller Depotstand\n\n{kurz}{depotdetails}\n{}",
                historie.ueberblick
            ),
        ),
        ("02_ki_gespraeche.md", ki),
        ("03_journal.md", historie.belege + &journal.bericht),
        (
            "04_orders_und_depot.md",
            format!("# Papierorders und Depot\n{orders}"),
        ),
        ("05_betrieb_und_fehler.md", betrieb),
        ("06_konfiguration.md", konfiguration),
        ("07_version.md", programmstand(d.version.as_deref())),
        ("ausfuehrungen.csv", ausfuehrungen),
    ];
    zip_antwort(
        &schutz::Exportschutz::neu(&schutz, sec_kontakt.as_deref()),
        zeitraum,
        jetzt,
        &liesmich,
        std::iter::once(("PRUEFBERICHT.md", pruefbericht.as_str()))
            .chain(dateien.iter().map(|(name, text)| (*name, text.as_str()))),
    )
}

fn zip_antwort<'a>(
    schutz: &schutz::Exportschutz<'_>,
    zeitraum: Zeitraum,
    jetzt: Zeitpunkt,
    liesmich: &str,
    dateien: impl Iterator<Item = (&'a str, &'a str)>,
) -> Vec<u8> {
    match archiv(schutz, zeitraum, jetzt, liesmich, dateien) {
        Ok(zip) => http::download(&zeitraum.dateiname(), &zip),
        Err(_) => seiten::fehler(
            409,
            "Export nicht verfügbar. Die Größen- oder Musterprüfung ist fehlgeschlagen.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_schwaerzt_aiza_ohne_hinterlegten_schluessel() -> Result<(), String> {
        let schutz = Schutz::default();
        let schutz = schutz::Exportschutz::neu(&schutz, None);
        let tag = Datum::neu(2026, 11, 14).ok_or("Datum")?;
        let jetzt = Zeitpunkt::neu(tag, 12, 0, 0).ok_or("Zeit")?;
        let zeitraum = Zeitraum { von: tag, bis: tag };
        let wert = format!("AIza{}", "X".repeat(35));
        let text = format!("Gemini meldet ({wert}).\n");
        let (bereinigt, anzahl) = schutz.text(&text, false);
        assert_eq!(bereinigt, "[Zeile geschwärzt]\n");
        assert_eq!(anzahl, 1);
        assert_eq!(schutz.text(&bereinigt, false).1, 0);
        let zip = archiv(
            &schutz,
            zeitraum,
            jetzt,
            "Analyse",
            [("fehler.md", text.as_str())],
        )?;
        assert!(zip.starts_with(b"PK"));
        assert_eq!(
            zip,
            archiv(
                &schutz,
                zeitraum,
                jetzt,
                "Analyse",
                [("fehler.md", "Debug key=andererWert\n")]
            )?
        );
        Ok(())
    }

    #[test]
    fn export_ohne_zip_bei_resttreffer() -> Result<(), String> {
        let schutz = Schutz::default();
        let schutz = schutz::Exportschutz::neu(&schutz, Some("Zeile geschwärzt"));
        let tag = Datum::neu(2026, 11, 14).ok_or("Datum")?;
        let jetzt = Zeitpunkt::neu(tag, 12, 0, 0).ok_or("Zeit")?;
        let zeitraum = Zeitraum { von: tag, bis: tag };
        let ergebnis = archiv(
            &schutz,
            zeitraum,
            jetzt,
            "Analyse",
            [("fehler.md", "Debug key=andererWert\n")],
        );
        assert_eq!(
            ergebnis,
            Err("Musterprüfung des Exports fehlgeschlagen.".into())
        );
        Ok(())
    }

    #[test]
    fn export_csv() -> Result<(), String> {
        let tag = Datum::neu(2026, 11, 10).ok_or("Datum")?;
        let zeitraum = Zeitraum { von: tag, bis: tag };
        let csv = ausfuehrungen_csv(Some(&Stand::default()), zeitraum);
        assert!(csv.starts_with("Nr;Topf;ISIN;Richtung;Stück;"));
        assert!(csv.contains("Füllpreis EUR;Füllpreis bp;Schluss EUR;Schluss bp\n"));
        assert_eq!(csv, ausfuehrungen_csv(None, zeitraum));
        Ok(())
    }

    #[test]
    fn freie_zeitraeume_verwenden_frankfurter_tage_und_beide_grenzen() -> Result<(), String> {
        let heute = Datum::lies("2026-03-30").ok_or("Datum")?;
        let frei = |von: &str, bis: &str| {
            Zeitraum::aus_formular(
                heute,
                &[
                    ("zeitraum".into(), "frei".into()),
                    ("von".into(), von.into()),
                    ("bis".into(), bis.into()),
                ],
            )
        };
        let zeitraum = frei("2026-03-29", "2026-03-30")?;
        for (utc, drin) in [
            ("2026-03-28T22:59:59Z", false),
            ("2026-03-28T23:00:00Z", true),
            ("2026-03-30T21:59:59Z", true),
            ("2026-03-30T22:00:00Z", false),
        ] {
            assert_eq!(zeitraum.enthaelt(Zeitpunkt::lies(utc).ok_or("Zeit")?), drin);
        }
        assert_eq!(frei("2026-02-28", "2026-03-30")?.tage().count(), 31);
        assert!(frei("2026-02-27", "2026-03-30").is_err());
        assert!(frei("2026-03-30", "2026-03-29").is_err());
        assert!(frei("2026-03-30", "2026-03-31").is_err());
        assert!(frei("2026-02-29", "2026-03-30").is_err());
        Ok(())
    }

    #[test]
    fn schnellwahl_berechnet_den_bereich_am_wochenende() -> Result<(), String> {
        let heute = Datum::lies("2026-11-08").ok_or("Datum")?;
        for (wahl, von, bis) in [
            ("heute", "2026-11-08", "2026-11-08"),
            ("gestern", "2026-11-07", "2026-11-07"),
            ("woche", "2026-11-02", "2026-11-08"),
            ("handelstage", "2026-11-02", "2026-11-06"),
        ] {
            let zeitraum = Zeitraum::aus_formular(heute, &[("zeitraum".into(), wahl.into())])?;
            assert_eq!(
                (zeitraum.von.to_string(), zeitraum.bis.to_string()),
                (von.into(), bis.into())
            );
        }
        Ok(())
    }

    #[test]
    fn entscheidung_exportiert_ziel_und_urteil_ohne_freitext() {
        let mut aus = String::new();
        ki_entscheidung(
            &mut aus,
            r#"{"zustimmung":true,"einschaetzung":"geloeschterZugang","ziele":[{"isin":"DE0007164600","gewicht_bp":2500,"begruendung":"geloeschterZugang"}]}"#,
        );
        assert!(aus.contains("Zustimmung: ja"));
        assert!(aus.contains("DE0007164600: 25,00 %"));
        assert!(!aus.contains("geloeschterZugang"));
    }

    #[test]
    fn fremde_erstmeinung_bleibt_sichtbar() {
        let zugang = "geschuetzterWertFuerDieExportprobe";
        let zitat = depot_kern::json::zitiere(&format!(
            "Topf B: Antwort verwendet: Bargeld behalten.\nLetzte Abstimmung: unverändert.\n{zugang}\nC:\\Users\\Test"
        ));
        let frage = format!(
            "Übrige Frage\nFremde fixierte Erst- und Zweitmeinung: {zitat}\nDein bisheriger Vorschlag: null"
        );
        let mut aus = String::new();
        fremde_erstmeinung(&mut aus, &frage);
        let mut schutz = Schutz::default();
        schutz.merken(zugang);
        let (bericht, zeilen) = schutz::Exportschutz::neu(&schutz, None).text(&aus, false);
        assert!(bericht.contains("Topf B: Antwort verwendet: Bargeld behalten."));
        assert!(bericht.contains("Letzte Abstimmung: unverändert."));
        assert_eq!(zeilen, 2);
        assert!(!bericht.contains(zugang));
        assert!(!bericht.contains("Users"));
        assert!(!bericht.contains("Übrige Frage"));
        assert!(!bericht.contains("Dein bisheriger Vorschlag"));
    }

    #[test]
    fn gespraechsbericht_enthaelt_keine_vollen_prompts() -> Result<(), String> {
        let zeit = Zeitpunkt::lies("2026-11-12T10:00:00Z").ok_or("Zeit")?;
        let (tag, ..) = zeit.in_frankfurt().ok_or("Datum")?;
        let geheim = "geschuetzterWertFuerDieExportprobe";
        let eintrag = Gespraech {
            zeit,
            lauf: "probe".into(),
            wer: "dachs".into(),
            rolle: "haupt".into(),
            modell: "probe".into(),
            zweck: "erstmeinung".into(),
            ergebnis: "ok".into(),
            http: Some(200),
            token_ein: None,
            token_aus: None,
            buchung: None,
            kosten_cent: None,
            system: Some("Gemeinsamer Systemtext".into()),
            frage: Some(format!("Erste Frage\n~~~\n{geheim}\nLetzte Zeile")),
            antwort: Some(r#"{"zustimmung":true}"#.into()),
            fehler: None,
        };
        let mut aus = String::new();
        ki_laeufe(&mut aus, tag, None, &[eintrag]);
        assert!(aus.contains("Zustimmung: ja"));
        assert!(!aus.contains("Erste Frage"));
        assert!(!aus.contains("Gemeinsamer Systemtext"));
        assert!(!aus.contains(geheim));
        Ok(())
    }

    #[test]
    fn vor_boersenschluss_sind_vortageskurse_aktuell() -> Result<(), String> {
        let tag = Datum::neu(2026, 11, 12).ok_or("Datum")?;
        let vormittag = Zeitpunkt::frankfurt(tag, 10, 0, 0).ok_or("Zeit")?;
        let abend = Zeitpunkt::frankfurt(tag, 18, 0, 0).ok_or("Zeit")?;
        assert_eq!(letzter_boersenschluss(vormittag, tag), tag.plus_tage(-1));
        assert_eq!(letzter_boersenschluss(abend, tag), Some(tag));
        Ok(())
    }

    #[test]
    fn verlauf_exportiert_mehr_als_die_live_anzeige() -> Result<(), String> {
        let tag = Datum::neu(2026, 11, 12).ok_or("Datum")?;
        let zeit = Zeitpunkt::frankfurt(tag, 8, 0, 0).ok_or("Zeit")?;
        let mut text = String::new();
        for nummer in 1..=25 {
            writeln!(
                text,
                "verlauf-v1;start;123-{nummer};termine;hand;{}",
                zeit.sekunden()
            )
            .map_err(|e| e.to_string())?;
        }
        assert_eq!(verlauf::lies(&text).auftraege.len(), 20);
        assert_eq!(verlauf::lies_zeitraum(&text, tag, tag).auftraege.len(), 25);
        Ok(())
    }
}
