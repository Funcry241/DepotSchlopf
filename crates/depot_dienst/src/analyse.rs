//! Lokaler, lesender Export eines Frankfurter Betriebstags.

use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{ErrorKind, Read as _};
use std::path::Path;

use depot_export::Zip;
use depot_journal::{Journal, JournalFehler, Stand};
use depot_kern::json::{Json, erstes_objekt};
use depot_kern::{BeraterId, Cent, Datum, Grenzen, Isin, Zeitpunkt, abdruck::sha256_hex};
use depot_lage::{datum_text, zeit_text};

use crate::{Dienst, Umwelt, http, protokoll::Schutz, schluessel, seiten, verlauf};

const MIB: u64 = 1024 * 1024;
const JOURNAL_MAX: u64 = 48 * MIB;
const TEXT_MAX: u64 = 2 * MIB;
const ZIP_MAX: usize = 25 * 1024 * 1024;

pub(crate) fn formular(d: &Dienst) -> String {
    format!(
        "<form method=\"post\" action=\"/export\"><input type=\"hidden\" name=\"t\" value=\"{}\">\
         <button type=\"submit\">Heute exportieren</button></form>",
        depot_lage::html::maskiere(&d.token)
    )
}

fn am(zeit: Zeitpunkt, tag: Datum) -> bool {
    zeit.in_frankfurt().is_some_and(|(d, ..)| d == tag)
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

fn betrieb(d: &Dienst, daten: &Path, tag: Datum, luecken: &mut String) -> String {
    let z = d.sperre();
    let text = quelle(&verlauf::pfad(&d.wurzel), "Dienstverlauf", luecken);
    let mut verlauf = verlauf::lies_zeitraum(&text, tag, tag);
    if let Some(lauf) = z.laufend.as_ref().filter(|a| am(a.start, tag)) {
        verlauf.auftraege.retain(|a| a.kennung != lauf.kennung);
        verlauf.auftraege.insert(0, lauf.clone());
    }
    let laufend = z.laufend.as_ref().map(|a| a.kennung.clone());
    drop(z);
    let mut aus = "# Betrieb und Abrufe\n\n## Dienstverlauf\n".to_owned();
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
    for e in eintraege.into_iter().filter(|e| am(e.zeit, tag)) {
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

fn letzter_boersenschluss(jetzt: Zeitpunkt, tag: Datum) -> Option<Datum> {
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

fn depot(stand: &Stand, jetzt: Zeitpunkt, tag: Datum, sperren: &str) -> (String, String, String) {
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
    let tagesergebnis = if stand.tagesschluesse().iter().any(|(d, _)| *d < tag) {
        differenz(depot_lage::letzter_tagesschluss(stand, &werte, tag))
    } else {
        "noch kein abgeschlossener Tag".into()
    };
    let mut kurz = format!(
        "Stand {} · Kurse vom {kurstag}\n\n- Gesamtwert: {}\n- Tagesergebnis: {tagesergebnis}\n- Offene Orders: {}\n",
        zeit_text(jetzt),
        geld(lage.gesamt),
        stand.offene_auftraege().count()
    );
    for topf in &lage.toepfe {
        let frei = depot_journal::konto::verfuegbar(&Grenzen::standard(), stand, topf.topf);
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
    (kurz, details, orderbewegungen(stand, &werte, tag))
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
            let _ = writeln!(
                details,
                "| {:?} | {} | {} | {} | {kurs} | {} | nicht belegt |",
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

fn orderbewegungen(stand: &Stand, werte: &[depot_kern::liste::Wert], tag: Datum) -> String {
    let mut details = String::new();
    details.push_str("\n## Heute belegte Orderbewegungen\n");
    for a in stand.auftraege() {
        let versand = a.versand.is_some_and(|z| am(z, tag));
        let fuellungen = || {
            a.ausfuehrungen
                .iter()
                .filter(|f| f.kurszeit.is_some_and(|z| am(z, tag)))
        };
        if versand || fuellungen().next().is_some() {
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
            for f in fuellungen() {
                let _ = writeln!(
                    details,
                    "  - {} Stück zu {} · Gebühr {}",
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

fn ki(daten: &Path, tag: Datum, stand: Option<&Stand>, luecken: &mut String) -> String {
    let mut aus = "# KI-Ergebnisse\n\n".to_owned();
    let ordner = daten.join("gespraeche");
    let pfad = ordner.join(format!("{tag}.jsonl"));
    let gespr = if fs::symlink_metadata(ordner).is_ok_and(|m| m.is_dir())
        && fs::symlink_metadata(pfad).is_ok_and(|m| m.is_file() && m.len() <= 8 * MIB)
    {
        depot_lage::gespraeche::lesen(daten, tag).ok()
    } else {
        None
    };
    if let Some((eintraege, fehler)) = gespr {
        for e in eintraege {
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
                "- {} · Lauf {} · {} / {} · {} · {} · HTTP {http} · Token {}/{} · Journalbuchung {}",
                zeit_text(e.zeit),
                feste_kennung(&e.lauf),
                feste_kennung(&e.wer),
                feste_kennung(&e.modell),
                feste_kennung(&e.zweck),
                feste_kennung(&e.ergebnis),
                token(e.token_ein),
                token(e.token_aus),
                geld(kosten)
            );
            if let Some(antwort) = e.antwort.as_deref() {
                ki_entscheidung(&mut aus, antwort);
            }
        }
        let _ = writeln!(aus, "\nUnlesbare Gesprächszeilen: {fehler}.");
    } else {
        luecken.push_str("- KI-Gespräche: keine verwendbare Tagesdatei vorhanden.\n");
    }
    if let Some(s) = stand {
        aus.push_str("\n## Tagesbuchungen im Journal\n");
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
    aus
}

fn archiv(
    schutz: &Schutz,
    tag: Datum,
    jetzt: Zeitpunkt,
    dateien: &[(&str, String)],
) -> Result<Vec<u8>, String> {
    let mut manifest = format!("depotschlopf-analyse-v1\ntag={tag}\nerstellt_utc={jetzt}\n");
    let mut zip = Zip::default();
    let mut summe = 0_usize;
    for (name, text) in dateien {
        let text = schutz.bereinige_zeilen(text);
        if schutz.bereinige_zeilen(&text) != text {
            return Err("Musterprüfung des Exports fehlgeschlagen.".into());
        }
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
        zip.datei_gepackt(name, text.as_bytes())?;
    }
    if schutz.bereinige_zeilen(&manifest) != manifest {
        return Err("Musterprüfung des Manifests fehlgeschlagen.".into());
    }
    zip.datei_gepackt("MANIFEST.txt", manifest.as_bytes())?;
    let zip = zip.fertig()?;
    if zip.len() > ZIP_MAX {
        return Err("ZIP überschreitet die Größenbegrenzung.".into());
    }
    Ok(zip)
}

pub(crate) fn exportieren(d: &Dienst, u: &dyn Umwelt, jetzt: Zeitpunkt) -> Vec<u8> {
    let Some((tag, ..)) = jetzt.in_frankfurt() else {
        return seiten::fehler(409, "Frankfurter Datum nicht verfügbar.");
    };
    let Ok(schluessel) = schluessel::fuer_protokoll() else {
        return seiten::fehler(409, "Schlüsselablage nicht lesbar. Export abgebrochen.");
    };
    let mut schutz = Schutz::default();
    for geheim in schluessel.iter().chain(std::iter::once(&d.token)) {
        schutz.merken(geheim);
    }
    let daten = d.wurzel.join("daten");
    if !fs::symlink_metadata(&daten).is_ok_and(|m| m.is_dir()) {
        return seiten::fehler(
            409,
            "Datenordner fehlt oder ist kein reguläres Verzeichnis.",
        );
    }
    let mut luecken = String::new();
    let betrieb = betrieb(d, &daten, tag, &mut luecken);
    let konfiguration = konfiguration(d, u, &daten);
    let sperren = format!(
        " · KI-Sperre: {} · Kurssperre: {}",
        an_aus(daten.join("ki.halt").exists()),
        an_aus(daten.join("kurse.halt").exists())
    );
    let (kurz, depotdetails, orders, ki) =
        match Journal::pruefe_lesend_begrenzt(&daten.join("journal.txt"), JOURNAL_MAX, |s| {
            let (kurz, depotdetails, orders) = depot(s, jetzt, tag, &sperren);
            (
                kurz,
                depotdetails,
                orders,
                ki(&daten, tag, Some(s), &mut luecken),
            )
        }) {
            Ok(geprueft) => geprueft,
            Err(JournalFehler::Belegt) => {
                return seiten::fehler(
                    409,
                    "Journal gerade in Benutzung. Bitte gleich noch einmal.",
                );
            }
            Err(_) => {
                luecken.push_str(
                    "- Journalstand nicht bestätigt. Order-, Depot- und Kostenzahlen fehlen.\n",
                );
                let meldung = "Depotstand nicht verfügbar: Journal-Kette nicht bestätigt.\n";
                (
                    meldung.into(),
                    String::new(),
                    "Unverifiziert: Journal-Kette nicht bestätigt.\n".into(),
                    ki(&daten, tag, None, &mut luecken),
                )
            }
        };
    let datum = datum_text(tag);
    let liesmich = format!(
        "# DepotSchlopf · Analyse-Export {datum}\n\n## Aktueller Depotstand\n{kurz}\n\
         ## Datenbasis\nPapiergeld. Die Zeiten sind Frankfurter Ortszeit.\n\
         Freie Anbietertexte, Prompts und Rohprotokolle sind in diesem Export nicht enthalten.\n\
         ## Lücken\n{luecken}"
    );
    let version = d
        .version
        .as_deref()
        .filter(|v| v.len() == 40 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        .unwrap_or("unbekannt");
    let dateien = [
        ("LIESMICH.md", liesmich),
        (
            "01_ueberblick.md",
            format!("# Aktueller Depotstand\n\n{kurz}{depotdetails}"),
        ),
        ("02_ki_gespraeche.md", ki),
        (
            "04_orders_und_depot.md",
            format!("# Papierorders und Depot\n{orders}"),
        ),
        ("05_betrieb_und_fehler.md", betrieb),
        ("06_konfiguration.md", konfiguration),
        (
            "07_version.md",
            format!(
                "# Programmstand\n\n- Commit: {version}\n- Version: {}\n- System: {}\n",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS
            ),
        ),
    ];
    match archiv(&schutz, tag, jetzt, &dateien) {
        Ok(zip) => http::download(&format!("DepotSchlopf_Analyse_{tag}.zip"), &zip),
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
