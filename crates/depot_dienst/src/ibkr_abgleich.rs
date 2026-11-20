//! Einrichtung und Zeitplan für den lesenden Abgleich mit dem IBKR-Papierjournal.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use depot_journal::Journal;
use depot_kern::ibkr_papier::{ABGLEICH_HALT, ABGLEICH_OK};
use depot_kern::{Boerse, Datum, Zeitpunkt};

use crate::http::{feld, umleitung};
use crate::seiten::{auftrag_knopf, marke};
use crate::werkbank::{Art, Dienst, lies_falls_da, schreibe_ganz};
use crate::zeitplan::{lies_uhrzeit, uhrzeit_text};

const ZEITPLAN: &str = "abgleich-zeitplan.txt";
const LETZTER_START: &str = "abgleich-zeitplan-start.txt";
const VORGABE: (u8, u8) = (9, 0);
const JOURNAL: &str = "journal.txt";
const PRAXISTAGE: usize = 5;

fn ordner(d: &Dienst) -> Option<PathBuf> {
    let konto = crate::ibkr_papier::konto(d)?;
    Some(d.wurzel.join("daten/ibkr-papier").join(konto))
}

#[derive(Clone, Copy)]
struct Plan {
    aktiv: bool,
    uhrzeit: (u8, u8),
}

impl Default for Plan {
    fn default() -> Self {
        Self {
            aktiv: false,
            uhrzeit: VORGABE,
        }
    }
}

fn zeitplan(ordner: &Path) -> Result<Plan, String> {
    let Some(text) = lies_falls_da(&ordner.join(ZEITPLAN))? else {
        return Ok(Plan::default());
    };
    let unlesbar = || "IBKR-Abgleichszeitplan unlesbar".to_owned();
    let (status, zeit) = text
        .strip_suffix('\n')
        .and_then(|t| t.split_once(';'))
        .ok_or_else(unlesbar)?;
    let aktiv = match status {
        "an" => true,
        "aus" => false,
        _ => return Err(unlesbar()),
    };
    let uhrzeit = lies_uhrzeit(zeit).ok_or_else(unlesbar)?;
    Ok(Plan { aktiv, uhrzeit })
}

#[derive(Default)]
struct Nachweis {
    tage: BTreeSet<Datum>,
    letzter: Option<(Datum, bool)>,
    eingerichtet: bool,
}

fn nachweis(ordner: &Path) -> Result<Nachweis, depot_journal::JournalFehler> {
    let pfad = ordner.join(JOURNAL);
    if !Journal::eingerichtet(&pfad)? {
        return Ok(Nachweis::default());
    }
    Journal::pruefe_lesend(&pfad, |stand| {
        let mut n = Nachweis {
            eingerichtet: !stand.toepfe().is_empty(),
            ..Nachweis::default()
        };
        for (tag, text) in stand.beobachtungen() {
            let titel = text
                .split_once(". ")
                .map_or(text.as_str(), |(anfang, _)| anfang);
            if titel == ABGLEICH_OK && Boerse::Xetra.handelstag(*tag) {
                n.tage.insert(*tag);
            }
            if matches!(titel, ABGLEICH_OK | ABGLEICH_HALT) {
                n.letzter = Some((*tag, titel == ABGLEICH_OK));
            }
        }
        n
    })
}

pub(crate) fn zum_zeitplan_faellig(d: &Dienst, jetzt: Zeitpunkt) -> bool {
    let Some((tag, stunde, minute, ..)) = jetzt.in_frankfurt() else {
        return false;
    };
    if !Boerse::Xetra.handelstag(tag) {
        return false;
    }
    let Some(ordner) = ordner(d) else {
        return false;
    };
    if !zeitplan(&ordner).is_ok_and(|u| u.aktiv && (stunde, minute) >= u.uhrzeit) {
        return false;
    }
    let vermerk = ordner.join(LETZTER_START);
    let heute = format!("{tag}\n");
    if !lies_falls_da(&vermerk).is_ok_and(|alt| {
        alt.is_none_or(|alt| {
            alt.strip_suffix('\n')
                .and_then(Datum::lies)
                .is_some_and(|alt| alt < tag)
        })
    }) {
        return false;
    }
    let Ok(journal) = nachweis(&ordner) else {
        return false;
    };
    journal.eingerichtet && schreibe_ganz(&vermerk, &heute).is_ok() && !journal.tage.contains(&tag)
}

fn zurueck(meldung: &str) -> Vec<u8> {
    umleitung(&format!(
        "/einrichtung?ibkrabgleich={meldung}#ibkr-abgleich"
    ))
}

pub(crate) fn speichern(d: &Dienst, felder: &[(String, String)]) -> Vec<u8> {
    let aktiv = feld(felder, "aktiv") == Some("ja");
    let Some(zeit) = feld(felder, "uhrzeit").and_then(lies_uhrzeit) else {
        return zurueck("uhrzeit");
    };
    let status = if aktiv { "an" } else { "aus" };
    let text = format!("{status};{}\n", uhrzeit_text(zeit));

    let z = d.sperre();
    let Some(ordner) = ordner(d) else {
        return zurueck("konto");
    };
    if d.update_offen() || z.neustart || z.auftrag_stand(Art::Update).is_some() {
        return zurueck("belegt");
    }
    if aktiv && !nachweis(&ordner).is_ok_and(|n| n.eingerichtet) {
        return zurueck("journal");
    }
    match schreibe_ganz(&ordner.join(ZEITPLAN), &text) {
        Ok(()) => zurueck("gespeichert"),
        Err(_) => zurueck("schreibfehler"),
    }
}

pub(crate) fn starten(d: &Dienst, art: Art, jetzt: Zeitpunkt) -> Vec<u8> {
    let Some(ordner) = ordner(d) else {
        return zurueck("konto");
    };
    if art == Art::IbkrAbgleich && !Journal::eingerichtet(&ordner.join(JOURNAL)).unwrap_or_default()
    {
        return zurueck("journal");
    }
    if d.update_offen() || d.von_hand(art, jetzt).is_err() {
        return zurueck("belegt");
    }
    zurueck("eingereiht")
}

pub(crate) fn status(d: &Dienst) -> String {
    let mut s = marke(&d.start);
    let Some(ordner) = ordner(d) else {
        s.push_str("<p>Speichere zuerst die Papierkontonummer.</p>");
        return s;
    };
    let Ok(n) = nachweis(&ordner) else {
        s.push_str("<p class=\"offen\">Das IBKR-Papierjournal ist gerade nicht lesbar. Ist ein Abgleich aktiv, warte auf seinen Abschluss.</p>");
        return s;
    };
    let z = d.sperre();
    if !n.eingerichtet {
        s.push_str("<p>Richte das separate IBKR-Papierjournal einmal ein.</p>");
        s.push_str(&auftrag_knopf(
            &d.token,
            &z,
            Art::IbkrJournal,
            Art::IbkrJournal.name(),
            false,
        ));
        return s;
    }
    let tage = n.tage.len().min(PRAXISTAGE);
    let _ = write!(
        s,
        "<p><b>{tage} von {PRAXISTAGE} Handelstagen belegt.</b></p>"
    );
    if let Some((tag, ok)) = n.letzter {
        let stand = if ok { "ohne Abweichung" } else { "mit Halt" };
        let _ = write!(
            s,
            "<p>Letzter Journalabgleich am {} · {stand}.</p>",
            depot_lage::datum_text(tag)
        );
    }
    if let Some(a) = z
        .verlauf
        .iter()
        .find(|a| a.art == Art::IbkrAbgleich && a.nr >= z.ibkr_pruefung_ab_nr)
        && a.erfolg != Some(true)
        && n.letzter
            .is_none_or(|(tag, ok)| !ok || a.start.frankfurt_tag().is_some_and(|d| d > tag))
    {
        s.push_str("<p class=\"offen\">Der jüngste Abgleich im Dienst war nicht erfolgreich. Details stehen unter <a href=\"/\">Live</a>.</p>");
    }
    if zeitplan(&ordner).is_ok_and(|t| t.aktiv) && !z.zeitplan.an {
        s.push_str("<p>Der automatische Abgleich wartet auf den eingeschalteten DepotSchlopf-Zeitplan.</p>");
    }
    s.push_str(&auftrag_knopf(
        &d.token,
        &z,
        Art::IbkrAbgleich,
        "Jetzt abgleichen",
        false,
    ));
    s
}

pub(crate) fn einrichtung(d: &Dienst, felder: &[(String, String)]) -> String {
    let Some(ordner) = ordner(d) else {
        return String::new();
    };
    let plan = zeitplan(&ordner);
    let werte = plan.as_ref().copied().unwrap_or_default();
    let uhrzeit = uhrzeit_text(werte.uhrzeit);
    let haken = if werte.aktiv { " checked" } else { "" };
    let meldung = match feld(felder, "ibkrabgleich") {
        Some("gespeichert") => "Einstellung zum IBKR-Abgleich gespeichert.",
        Some("eingereiht") => "Auftrag eingereiht. Der Stand erscheint hier.",
        Some("uhrzeit") => "Gib eine gültige Uhrzeit ein.",
        Some("journal") => "Richte zuerst das separate IBKR-Papierjournal ein.",
        Some("konto") => "Speichere zuerst die Papierkontonummer.",
        Some("belegt") => "Warte, bis der laufende Auftrag beendet ist, und versuche es erneut.",
        Some("schreibfehler") => "Die Einstellung konnte lokal nicht gespeichert werden.",
        _ if plan.is_err() => {
            "Die Einstellung ist unlesbar. Speichere den gewünschten Zustand erneut."
        }
        _ => "",
    };
    let meldung = if meldung.is_empty() {
        String::new()
    } else {
        format!("<p role=\"status\">{meldung}</p>")
    };
    format!(
        "<div id=\"ibkr-abgleich\"><h3>IBKR-Papierjournal abgleichen</h3>{meldung}\
         <div data-teil=\"/teil/ibkr-abgleich\">{}</div>\
         <form method=\"post\" action=\"/ibkr-abgleich\"><input type=\"hidden\" name=\"t\" value=\"{}\">\
         <label><input type=\"checkbox\" name=\"aktiv\" value=\"ja\"{haken}> Automatisch an Xetra-Handelstagen abgleichen</label>\
         <label>Uhrzeit (Frankfurter Zeit) <input type=\"time\" name=\"uhrzeit\" value=\"{uhrzeit}\" required></label>\
         <p class=\"leise\">Für den automatischen Abgleich müssen der DepotSchlopf-Zeitplan eingeschaltet und das Gateway verbunden sein.</p>\
         <button type=\"submit\">Abgleichszeitplan speichern</button></form></div>",
        status(d),
        depot_lage::html::maskiere(&d.token),
    )
}
