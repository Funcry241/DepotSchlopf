//! Die zwei Seiten der Oberflaeche: **Live** (Taetigkeit, naechster Lauf, Lagebild mit Budget,
//! Depot und Gespraech) und **Einrichtung** (die eine Config-Seite). Jeder Text ist maskiert;
//! das einzige Skript ist `/app.js` (Auffrischen und Verbindungsanzeige).

use std::fmt::Write as _;

use depot_kern::{Datum, Zeitpunkt};
use depot_lage::html::{STIL, maskiere};
use depot_lage::{Abhilfe, Grenzwert, SchluesselStand, datum_text, ortszeit, zeit_text};

use crate::autostart;
use crate::http::feld;
use crate::kostenschutz;
use crate::otter;
use crate::schluessel::Wer;
use crate::werkbank::{Anlass, Art, Auftrag, Frueher, ZeitplanAbgelehnt, Zustand};
use crate::zeitplan::{Zeitplan, geplante_abrufe, lies_uhrzeit, naechster_fuer, uhrzeit_text};
use crate::zeitstrahl_ki::{self, Aufrufe};

const STIL_DIENST: &str = "\
nav{display:flex;gap:12px;align-items:baseline;flex-wrap:wrap;--nav-akzent:#005c97}\
@media (prefers-color-scheme:dark){nav{--nav-akzent:#74c7ff}}\
nav a{color:inherit;text-decoration:none;padding:6px 10px;border-bottom:3px solid transparent;\
border-radius:6px 6px 0 0}\
nav a:hover{background:var(--flaeche)}\
nav a[aria-current=page]{font-weight:700;background:var(--flaeche);border-bottom-color:var(--nav-akzent)}\
nav a:focus-visible{outline:2px solid currentColor;outline-offset:2px}\
.version{margin-inline-start:auto;white-space:nowrap}\
form.knopf{display:inline;margin:0}\
a.aktion{display:inline-block;text-decoration:none;padding:10px 16px;border:1px solid var(--linie);border-radius:6px;color:inherit}\
button{font:inherit;font-weight:600;min-height:44px;padding:10px 16px;border:2px solid var(--linie);border-radius:6px;\
background:var(--flaeche);color:var(--text);cursor:pointer}\
button:disabled{color:var(--leise);border-style:dashed;cursor:default}\
button:not(:disabled):hover,a.aktion:hover{outline:2px solid currentColor;outline-offset:2px}\
button:focus-visible,a.aktion:focus-visible{outline:3px solid var(--text);outline-offset:3px}\
button.primaer,a.primaer,form.ungespeichert:valid button[data-speichern-hervorheben]:not(:disabled){background:#005c97;color:#fff;border:2px solid #005c97;font-weight:700}\
button.primaer:disabled{background:var(--flaeche);color:var(--leise);border-color:var(--linie)}\
button.laeuft{border:2px solid var(--offen);color:var(--offen)}\
#betriebsleiste{position:sticky;top:0;z-index:1;background:var(--grund);padding:4px 0}\
.betrieb{border:2px solid var(--linie);border-radius:8px;padding:10px 14px;margin:0}\
.betrieb.aktiv{border-color:var(--offen)}.betrieb p{margin:4px 0}\
.betrieb progress{width:90px;max-width:30%;vertical-align:middle;margin-right:8px}\
.update-angebot{border:2px solid var(--linie);padding:10px 14px;border-radius:8px;margin-top:6px}\
.update-angebot p{margin:0 0 6px}.aktiver-bereich{border:2px solid var(--offen)}\
.zeitstrahl{list-style:none;margin:0;padding-inline-start:14px;border-inline-start:2px solid var(--linie)}\
.zeitstrahl li{padding:6px 0}.zeitstrahl li span{margin-inline-start:8px}\
#zeitstrahl .optional{border-bottom:1px dashed var(--linie)}\
section[id]{scroll-margin-top:180px}\
@media(max-width:600px){#betriebsleiste{position:static}}\
input[type=password],input[type=time]{font:inherit;padding:3px 6px;border:1px solid var(--linie);\
border-radius:6px;background:var(--grund);color:var(--text);max-width:100%}\
pre{white-space:pre-wrap;word-break:break-word;font:12px/1.4 ui-monospace,Consolas,monospace;\
margin:6px 0}\
.banner{position:sticky;top:0;z-index:3;background:var(--halt);color:#fff;padding:8px 16px;font-weight:600}\
.meldung{background:var(--flaeche);border-left:4px solid currentColor;padding-left:10px}\
.meldungs-aktion{margin-top:-4px}.speicher-aktionen{position:sticky;bottom:0;background:var(--flaeche);border:2px solid var(--linie);border-radius:8px;padding:12px;margin-top:12px;z-index:1}\
.speicher-aktionen p{margin:4px 0}.ungespeichert .speicher-aktionen{border-color:var(--offen)}\
[data-aenderungen].ungespeichert [data-speicherstand]{font-weight:700;color:var(--offen)}\
#neuverbinden{padding:12px}\
@media(max-width:600px),(max-height:700px){.speicher-aktionen{position:static}}\
label{display:block;margin:6px 0}\
.einstellungsgruppe{border:2px solid var(--halt)}.zusatz-einstellungen{border:1px solid var(--linie)}\
.quellen-tabelle td:last-child button{width:100%}\
.zugang-tabelle input{width:100%}.zugang-tabelle td{overflow-wrap:anywhere}\
@media(max-width:600px){.zugang-tabelle,.zugang-tabelle tbody,\
.zugang-tabelle tr,.zugang-tabelle td{display:block;width:100%}\
.zugang-tabelle tr:first-child{display:none}.zugang-tabelle td{border:0}\
.zugang-tabelle tr{border-bottom:1px solid var(--linie);padding:8px 0}\
.zugang-tabelle td[data-titel]::before{content:attr(data-titel) \": \";font-weight:600}\
.zugang-tabelle button{margin-top:6px}}";

/// Ein Bedienelement zu einem Auftrag und seinem aktuellen Stand.
fn auftrag_knopf(token: &str, z: &Zustand, art: Art, text: &str, primaer: bool) -> String {
    let passt = |a: Art| a == art || (art == Art::Kurse && matches!(a, Art::KurseVon(_)));
    let lauf = z
        .laufend
        .iter()
        .map(|a| a.art)
        .chain(z.warteschlange.iter().map(|a| a.art))
        .find(|a| passt(*a));
    if let Some(lauf) = lauf
        && let Some(status) = z.auftrag_stand(lauf)
    {
        let beschriftung = if lauf == art { text } else { lauf.name() };
        return format!(
            "<button type=\"button\" class=\"{}\" disabled>{} · {status}</button>",
            if status.starts_with("läuft") {
                "laeuft"
            } else {
                "wartet"
            },
            maskiere(beschriftung)
        );
    }
    if z.neustart || z.auftrag_stand(Art::Update).is_some() {
        return format!(
            "<button type=\"button\" disabled>{} · wartet auf Update</button>",
            maskiere(text)
        );
    }
    if matches!(art, Art::Papier | Art::Vorbereiten) {
        return bestaetigungslink(art, text, None, primaer);
    }
    let klasse = if primaer { "primaer" } else { "" };
    let ziel = if art == Art::Einrichten {
        "<input type=\"hidden\" name=\"ziel\" value=\"einrichtung\">"
    } else {
        ""
    };
    format!(
        "<form class=\"knopf\" method=\"post\" action=\"/ausfuehren\">\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <input type=\"hidden\" name=\"art\" value=\"{}\">\
         {ziel}<button class=\"{klasse}\" type=\"submit\">{}</button></form>",
        maskiere(token),
        art.kurz(),
        maskiere(text)
    )
}

fn bestaetigungslink(art: Art, text: &str, sha: Option<&str>, primaer: bool) -> String {
    let klasse = if primaer { "aktion primaer" } else { "aktion" };
    let stand = sha.map_or_else(String::new, |s| format!("&amp;sha={}", maskiere(s)));
    format!(
        "<a class=\"{klasse}\" href=\"/bestaetigen?art={}{stand}\">{}</a>",
        art.kurz(),
        maskiere(text)
    )
}

const ZEITFENSTER: &str = "von 08:00 bis vor 23:00 Uhr Frankfurter Zeit";
const VORBEREITUNG_HAEUFIGKEIT: &str = depot_lage::html::VORBEREITUNG_HAEUFIGKEIT;

pub(crate) fn auftrag_bestaetigen(token: &str, art: Art, sha: Option<&str>) -> String {
    let (text, ziel, knopf, haken) = if art == Art::Vorbereiten {
        (
            format!(
                "Dachs und Maus bereiten den nächsten Handelstag unabhängig vor. Das verbraucht echtes KI-Budget. Die Vorbereitung erzeugt keine Orders. {VORBEREITUNG_HAEUFIGKEIT} Möglich {ZEITFENSTER}."
            ),
            "/",
            Art::Vorbereiten.name(),
            "Ich nutze das vorhandene KI-Budget für diese Vorbereitung.",
        )
    } else if sha.is_some() {
        (
            "Das Update sichert deine Papierdaten und prüft die neue Version. Das dauert ein paar Minuten. Danach startet DepotSchlopf neu.".to_owned(),
            "/einrichtung#update",
            "Update einspielen",
            "Ich spiele diese Version ein.",
        )
    } else {
        (
            "Der Papiertag handelt mit Papiergeld. Sind Dachs oder Maus eingerichtet und verfügbar, können ihre Antworten echtes KI-Budget verbrauchen.".to_owned(),
            "/",
            "Papiertag starten",
            "Ich starte jetzt den Papiertag.",
        )
    };
    let stand = sha.map_or_else(String::new, |sha| {
        format!(
            "<p>Version <code>{0}</code></p><input type=\"hidden\" name=\"sha\" value=\"{0}\">",
            maskiere(sha)
        )
    });
    format!(
        "<section><h2>{knopf}</h2><p>{text}</p><form method=\"post\" action=\"/ausfuehren\">\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <input type=\"hidden\" name=\"art\" value=\"{}\">{stand}\
         <label><input type=\"checkbox\" name=\"geprueft\" value=\"ja\" required> {haken}</label>\
         <button class=\"primaer\" type=\"submit\">{knopf}</button> <a href=\"{ziel}\">Abbrechen</a></form></section>",
        maskiere(token),
        art.kurz()
    )
}

/// Das Bedienelement zu einer Abhilfe des Lagebilds.
fn abhilfe_knopf(token: &str, z: &Zustand, a: Abhilfe, primaer: bool) -> Option<String> {
    match a {
        Abhilfe::JournalEinrichten => Some(auftrag_knopf(
            token,
            z,
            Art::Einrichten,
            Art::Einrichten.name(),
            primaer,
        )),
        Abhilfe::KurseAbrufen => Some(auftrag_knopf(
            token,
            z,
            Art::Kurse,
            Art::Kurse.name(),
            primaer,
        )),
        Abhilfe::Vorbereiten => Some(auftrag_knopf(
            token,
            z,
            Art::Vorbereiten,
            Art::Vorbereiten.name(),
            primaer,
        )),
        Abhilfe::TermineAbrufen => Some(auftrag_knopf(
            token,
            z,
            Art::Termine,
            Art::Termine.name(),
            primaer,
        )),
        Abhilfe::PapiertagStarten => Some(auftrag_knopf(
            token,
            z,
            Art::Papier,
            "Papiertag starten",
            primaer,
        )),
        Abhilfe::SchluesselKursquelle
        | Abhilfe::SchluesselKursquelleUs
        | Abhilfe::SchluesselBerater(_) => {
            Some("<a href=\"/einrichtung#schluessel\">Zugangsdaten prüfen</a>".into())
        }
        Abhilfe::KostenschutzPruefen => {
            Some("<a href=\"/einrichtung#kostenschutz\">Kostenschutz prüfen</a>".into())
        }
        Abhilfe::KurssperrePruefen => {
            Some("<a href=\"/einrichtung#kurssperre\">Kurssperre prüfen</a>".into())
        }
        Abhilfe::Pruefen | Abhilfe::Warten => None,
    }
}

/// Meldungen nach dem gefuehrten Aufheben einer Sperre: `kostenschutz_…` oder `kurssperre_…`.
fn sperr_meldung(code: &str) -> Option<(&'static str, &'static str)> {
    let (sperre, ausgang) = code.split_once('_')?;
    let kurse = match sperre {
        "kostenschutz" => false,
        "kurssperre" => true,
        _ => return None,
    };
    Some(match (ausgang, kurse) {
        ("aufgehoben", false) => (
            "gut",
            "Sperre aufgehoben, Grund festgehalten. Die Reservierung bleibt gebucht; jeder \
             weitere Aufruf durchläuft wieder die Kostenprüfung.",
        ),
        ("aufgehoben", true) => (
            "gut",
            "Kurssperre aufgehoben, Grund festgehalten. Der nächste Abruf holt alle Kurse neu.",
        ),
        ("unbestaetigt", false) => (
            "offen",
            "Nicht aufgehoben: Bitte zuerst bestätigen, dass du die Kosten beim Anbieter geprüft hast.",
        ),
        ("unbestaetigt", true) => (
            "offen",
            "Nicht aufgehoben: Bitte zuerst bestätigen, dass du den Schlüssel der Kursquelle \
             geprüft und bei Bedarf erneuert hast.",
        ),
        ("dialog_unbestaetigt", _) => (
            "offen",
            "Nicht aufgehoben: Der Windows-Dialog wurde nicht innerhalb von zwei Minuten mit Ja bestätigt.",
        ),
        ("dialog_belegt", _) => (
            "offen",
            "Nicht aufgehoben: Ein anderer Windows-Freigabedialog ist bereits offen.",
        ),
        ("belegt", _) => (
            "offen",
            "Nicht aufgehoben: Es läuft oder wartet ein Auftrag. Danach erneut versuchen.",
        ),
        ("geaendert", _) => (
            "halt",
            "Nicht aufgehoben: Die Sperre hat sich inzwischen geändert. Bitte den neuen Grund prüfen.",
        ),
        ("frei", false) => ("gut", "Es liegt keine Sperre des Kostenschutzes vor."),
        ("frei", true) => ("gut", "Es liegt keine Kurssperre vor."),
        ("fehler", _) => (
            "halt",
            "Nicht aufgehoben: Datei nicht les- oder schreibbar. Die Sperre gilt weiter.",
        ),
        _ => return None,
    })
}

fn einstellungs_meldung(code: &str) -> Option<(&'static str, &'static str)> {
    Some(match code {
        "zeitplan" => (
            "gut",
            "Einstellungen gespeichert. Ob der Zeitplan an oder aus ist, bleibt unverändert.",
        ),
        "zeitplan_gespeichert_an" => (
            "gut",
            "Einstellungen gespeichert und Zeitplan eingeschaltet. DepotSchlopf führt die geplanten Läufe jetzt selbstständig aus, solange der Dienst läuft.",
        ),
        "zeitplan_an" => (
            "gut",
            "Zeitplan eingeschaltet. Die gespeicherten Einstellungen gelten.",
        ),
        "zeitplan_aus" => (
            "gut",
            "Zeitplan ausgeschaltet. Wartende automatische Aufträge wurden entfernt. Laufende Aufträge laufen zu Ende.",
        ),
        "zeitplan_neu_laden" => (
            "offen",
            "Zeitplan unverändert. Bitte lade die Einrichtung neu und wähle die gewünschte Aktion.",
        ),
        "zeitplan_schalter_geaendert" => (
            "offen",
            "Der Zeitplan wurde inzwischen ein- oder ausgeschaltet. Deine Eingaben stehen als Entwurf im Formular. Prüfe den aktuellen Stand und wähle erneut, wie du speichern möchtest.",
        ),
        "zeitplan_ungueltig" => (
            "halt",
            "Zeitplan nicht gespeichert: Uhrzeit im Format hh:mm. Die übrigen Eingaben stehen \
             noch im Formular.",
        ),
        "zeitplan_einrichtung" => (
            "halt",
            "Zeitplan nicht eingeschaltet: Erst muss die Einrichtung vollständig sein. Was fehlt, \
             steht beim Zeitplan.",
        ),
        "zeitplan_lage" => (
            "offen",
            "Zeitplan nicht eingeschaltet: Der Stand wird gerade neu geprüft. Bitte in ein paar \
             Sekunden den Stand neu laden und erneut einschalten.",
        ),
        "zeitplan_fehler" => (
            "halt",
            "Zeitplan unverändert. Die Datei konnte nicht gespeichert werden.",
        ),
        "autostart_an" => (
            "gut",
            "Autostart eingerichtet. DepotSchlopf startet bei deiner nächsten Windows-Anmeldung.",
        ),
        "autostart_aus" => ("gut", "Autostart entfernt."),
        "autostart_bestaetigen" => (
            "offen",
            "Autostart unverändert. Setze erst den Haken zur Bestätigung.",
        ),
        "autostart_fehler" => (
            "halt",
            "Autostart unverändert. Prüfe den Stand unten. Falls er sich ändern lässt, versuche es erneut.",
        ),
        _ => return None,
    })
}

/// Feste Meldungen nach einem Formular; nur bekannte Kennungen erscheinen.
pub fn meldung(abfrage: &str) -> Option<(&'static str, &'static str)> {
    let code = abfrage
        .split('&')
        .find_map(|p| p.strip_prefix("m="))
        .unwrap_or_default();
    if let Some(grund) = vorbereitung_meldung(code) {
        return Some(("offen", grund));
    }
    if let Some(m) = sperr_meldung(code).or_else(|| einstellungs_meldung(code)) {
        return Some(m);
    }
    Some(match code {
        "auftrag" => ("gut", "Auftrag eingereiht. Er steht unter Tätigkeit."),
        "auftrag_belegt" => (
            "offen",
            "Nicht eingereiht: Dieser Auftrag läuft oder wartet schon, ist gerade erst fertig \
             geworden, oder es warten bereits zu viele. \
             Deine Anfrage ist angekommen. Nichts wurde doppelt gestartet; unter Tätigkeit steht, was läuft.",
        ),
        "schluessel" => (
            "gut",
            "Schlüssel in der Anmeldeinformationsverwaltung gespeichert. Er gilt ab dem nächsten Lauf.",
        ),
        "schluessel_protokoll" => (
            "offen",
            "Schlüssel gespeichert, aber das Anbieterprotokoll war nicht schreibbar. Der Stand \
             zeigt bis zum nächsten Aufruf den alten Befund.",
        ),
        "schluessel_ungueltig" => (
            "halt",
            "Schlüssel nicht gespeichert: Länge oder Zeichen passen nicht. Dein bisheriger Eintrag bleibt unverändert. Bitte gib den Schlüssel nach der Regel in seiner Zeile erneut ein; er wird zu deinem Schutz nicht auf der Fehlerseite ausgegeben.",
        ),
        "schluessel_fehler" => (
            "halt",
            "Schlüssel nicht gespeichert: Die Anmeldeinformationsverwaltung hat abgelehnt.",
        ),
        "dialogtest_ja" => ("gut", "Dialogtest: Ja. Es wurde nichts freigegeben."),
        "dialogtest_nein" => ("gut", "Dialogtest: Nein. Es wurde nichts freigegeben."),
        "dialogtest_zeit" => (
            "gut",
            "Dialogtest: Zeit abgelaufen. Es wurde nichts freigegeben.",
        ),
        "dialogtest_fehler" => ("offen", "Der Windows-Dialog konnte nicht geöffnet werden."),
        "dialogtest_belegt" => ("offen", "Es ist bereits ein Freigabedialog geöffnet."),
        "papierfreigabe_ok" => (
            "gut",
            "Papierhandel freigegeben. Die neue Verlustmessung beginnt beim bestätigten Bargeld. Der nächste reguläre Papiertag kann wieder handeln.",
        ),
        "papierfreigabe_unbestaetigt" => (
            "halt",
            "Die Sperre gilt weiter. Bitte bestätige die neue Verlustmessung ausdrücklich.",
        ),
        "papierfreigabe_belegt" => (
            "offen",
            "Die Sperre gilt weiter. Warte, bis der laufende oder wartende Auftrag abgeschlossen ist.",
        ),
        "papierfreigabe_geaendert" => (
            "halt",
            "Der Journalstand hat sich geändert. Lade die Einrichtung neu und prüfe die aktuellen Beträge.",
        ),
        "papierfreigabe_fehler" => (
            "halt",
            "Die Freigabe konnte nicht bestätigt werden. Lade die Einrichtung neu und prüfe Journal und Papierkonto.",
        ),
        "papier_bestaetigen" => (
            "offen",
            "Papiertag unverändert. Öffne den Bestätigungsschritt und setze den Haken.",
        ),
        "update_bestaetigen" => (
            "offen",
            "Update unverändert. Öffne den Bestätigungsschritt und setze den Haken.",
        ),
        "update_suche" => (
            "offen",
            "Kein passendes Update zur Bestätigung. Suche erneut nach einem Update.",
        ),
        "update" => (
            "gut",
            "Auftrag eingereiht. Sein Protokoll steht auf Live unter Tätigkeit.",
        ),
        "update_belegt" => (
            "offen",
            "Update nicht eingereiht: Es geht nur, wenn kein anderer Auftrag läuft oder wartet.",
        ),
        "updatesuche_belegt" => (
            "offen",
            "Updatesuche nicht eingereiht. Sie läuft oder wartet schon, ist gerade erst fertig geworden, die Warteschlange ist voll oder ein Update wird eingespielt. Den Stand siehst du unter Update.",
        ),
        _ => return None,
    })
}

fn vorbereitung_meldung(code: &str) -> Option<&'static str> {
    Some(match code {
        "vorbereitung_bestaetigen" => "Bitte bestätige zuerst die Vorbereitung und ihre KI-Kosten.",
        "vorbereitung_update" => {
            "Das Update wird noch geprüft. Die Vorbereitung ist erst nach bestätigtem Update verfügbar."
        }
        "vorbereitung_quellen" => {
            "Bitte für die automatische Vorbereitung auch die Schlagzeilen einschalten. Deine Eingaben sind noch nicht gespeichert."
        }
        "vorbereitung_uhrzeit" => {
            "Vorbereitung ist von 08:00 bis vor 23:00 Uhr Frankfurter Zeit möglich."
        }
        "vorbereitung_handelstag" => {
            "Heute ist Handelstag. Eine Vorbereitung gibt es nur an börsenfreien Tagen."
        }
        "vorbereitung_begonnen" => {
            "Vorbereitung heute bereits begonnen. DepotSchlopf fragt die KI heute dafür nicht erneut."
        }
        _ => return None,
    })
}

/// Feste Abhilfe direkt neben der Rückmeldung; niemals Eingabewerte als Ziel verwenden.
pub fn meldungs_aktion(abfrage: &str) -> (&'static str, &'static str) {
    let code = abfrage
        .split('&')
        .find_map(|p| p.strip_prefix("m="))
        .unwrap_or_default();
    match code {
        "zeitplan"
        | "zeitplan_an"
        | "zeitplan_gespeichert_an"
        | "zeitplan_aus"
        | "zeitplan_ungueltig"
        | "zeitplan_schalter_geaendert"
        | "zeitplan_fehler"
        | "vorbereitung_quellen" => ("/einrichtung#zeitplan", "Zeitplan und Eingaben ansehen"),
        "zeitplan_lage" | "zeitplan_neu_laden" | "zeitplan_einrichtung" => {
            ("/einrichtung#zeitplan", "Einrichtung prüfen")
        }
        "schluessel" | "schluessel_protokoll" | "schluessel_ungueltig" | "schluessel_fehler" => {
            ("/einrichtung#schluessel", "Zugangsdaten prüfen")
        }
        "update_suche" | "updatesuche_belegt" | "update_bestaetigen" | "vorbereitung_update" => {
            ("/einrichtung#update", "Zum Update")
        }
        "papier_bestaetigen" => (
            "/bestaetigen?art=papier",
            "Papiertag ansehen und bestätigen",
        ),
        "vorbereitung_bestaetigen" => (
            "/bestaetigen?art=vorbereiten",
            "Vorbereitung ansehen und bestätigen",
        ),
        "auftrag" | "auftrag_belegt" | "update" | "update_belegt" => {
            ("/#taetigkeit", "Auftrag und Ergebnis ansehen")
        }
        c if c.starts_with("kostenschutz_") => ("/einrichtung#kostenschutz", "Kostenschutz prüfen"),
        c if c.starts_with("kurssperre_") => ("/einrichtung#kurssperre", "Kurssperre prüfen"),
        c if c.starts_with("papierfreigabe_") => {
            ("/einrichtung#papierfreigabe", "Papierfreigabe prüfen")
        }
        c if c.starts_with("autostart_") => ("/einrichtung#autostart", "Autostart prüfen"),
        c if c.starts_with("dialogtest_") => ("/einrichtung#papierdialog-test", "Zum Dialogtest"),
        _ => ("", ""),
    }
}

/// Der Seitenrahmen. `teil` ist der Pfad, mit dem `/app.js` den ganzen Inhalt regelmaessig neu
/// laedt; ohne `teil` erneuert es nur einen Bereich mit `data-teil` im Inhalt (falls vorhanden).
pub fn rahmen(
    hier: &str,
    teil: Option<&str>,
    meldung: Option<(&str, &str)>,
    inhalt: &str,
    commit: Option<&str>,
    betrieb: &str,
    hilfe: (&str, &str),
) -> String {
    let mut s = String::with_capacity(inhalt.len().saturating_add(4096));
    let nav = |pfad: &str, text: &str| {
        let aktuell = if pfad == hier {
            " aria-current=\"page\""
        } else {
            ""
        };
        format!("<a href=\"{pfad}\"{aktuell}>{text}</a>")
    };
    let version = commit.map_or_else(
        || {
            format!(
                "<span class=\"version leise\">Version {} · Commit unbekannt</span>",
                env!("CARGO_PKG_VERSION")
            )
        },
        |sha| {
            format!(
                "<span class=\"version leise\" title=\"{}\">Version {}</span>",
                maskiere(sha),
                maskiere(crate::rueckfall::kurz(sha))
            )
        },
    );
    let otter_stil = if hier == "/" { otter::STIL } else { "" };
    let _ = write!(
        s,
        "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>DepotSchlopf</title><style>{STIL}{STIL_DIENST}{otter_stil}</style></head><body>\
         <div id=\"getrennt\" class=\"banner\" hidden>Verbindung zum Dienst getrennt, etwa \
         beim Neustart nach einem Update. Die Seite verbindet sich von selbst neu. Wenn du das schwarze \
         Fenster geschlossen hast, starte DepotSchlopf.cmd erneut.</div>\
         <p id=\"neuverbinden\" hidden><a id=\"neuverbinden-link\" class=\"aktion\" href=\"/einrichtung\">Einrichtung neu laden</a> · Prüfe deine Eingaben vorher. Beim Verlassen fragt dein Browser wegen ungespeicherter Änderungen nach.</p><main>\
         <nav aria-label=\"Hauptnavigation\"><h1>DepotSchlopf</h1>{}{}{}\
         <span class=\"leise\">Nur Papiergeld.</span>{version}</nav>\
         <p class=\"leise\">Lass das schwarze DepotSchlopf-Fenster offen, damit DepotSchlopf läuft. \
         Du kannst es minimieren.</p>",
        nav("/", "Live"),
        nav("/gespraeche", "KI-Gespräche"),
        nav("/einrichtung", "Einrichtung")
    );
    if let Some((klasse, text)) = meldung {
        let _ = write!(
            s,
            "<section role=\"status\" class=\"meldung {klasse}\">{}</section>",
            maskiere(text)
        );
        if !hilfe.0.is_empty() {
            let ziel = if hier == "/einrichtung" {
                hilfe.0.strip_prefix("/einrichtung").unwrap_or(hilfe.0)
            } else {
                hilfe.0
            };
            let _ = write!(
                s,
                "<p class=\"meldungs-aktion\"><a class=\"aktion primaer\" href=\"{}\">{}</a></p>",
                maskiere(ziel),
                maskiere(hilfe.1)
            );
        }
    }
    let teil = teil
        .map(|t| format!(" data-teil=\"{}\"", maskiere(t)))
        .unwrap_or_default();
    if hier == "/" {
        s.push_str("<div class=\"live-kopf\">");
    }
    let _ = write!(
        s,
        "<div id=\"betriebsleiste\" data-teil=\"/teil/kopf\" role=\"status\" aria-live=\"polite\">{betrieb}</div>"
    );
    if hier == "/" {
        s.push_str(otter::BILD);
        s.push_str("</div>");
    }
    let _ = writeln!(
        s,
        "<div id=\"teil\"{teil}>{inhalt}</div></main>\
         <script src=\"/app.js\"></script></body></html>"
    );
    s
}

fn anlass_zeile(a: &Auftrag) -> String {
    let t = anlass_text(a.anlass);
    if a.frueher == Frueher::Nein {
        t.to_string()
    } else {
        format!("{t}, vor dem Neustart")
    }
}

fn anlass_text(a: Anlass) -> &'static str {
    match a {
        Anlass::Hand => "von Hand",
        Anlass::Zeitplan => "Zeitplan",
    }
}

fn wochentag_text(d: Datum) -> &'static str {
    match d.wochentag() {
        Some(1) => "Mo",
        Some(2) => "Di",
        Some(3) => "Mi",
        Some(4) => "Do",
        Some(5) => "Fr",
        Some(6) => "Sa",
        Some(7) => "So",
        _ => "",
    }
}

/// Wie es nach einem gescheiterten Lauf weitergeht, aus dem tatsaechlichen Zeitplan: von selbst
/// nur, wenn er an ist und kein Fehler ihn anhaelt. Der Lauf selbst kennt den Zeitplan nicht.
fn weiter_text(z: &Zustand, heute: Option<Datum>) -> String {
    let naechster = naechster_text(z, heute);
    if z.erledigt_fehler.is_some() || z.zeitplan_fehler.is_some() {
        return naechster;
    }
    let naechster = naechster.trim_end_matches('.');
    if z.zeitplan.an && heute.is_some() {
        let naechster = z
            .warteschlange
            .iter()
            .find(|a| a.anlass == Anlass::Zeitplan)
            .map_or(naechster, |a| a.art.name());
        format!("Nach Zeitplan geht es von selbst weiter: {naechster}.")
    } else {
        format!(
            "DepotSchlopf startet nichts von selbst: {naechster}. Kurse und Papiertag startest \
             du von Hand über die Knöpfe im Lagebild."
        )
    }
}

/// Der naechste Lauf des Zeitplans als Satz.
pub fn naechster_text(z: &Zustand, heute: Option<Datum>) -> String {
    if let Some(f) = z.automatik_fehler() {
        let grund = f.strip_prefix("Zeitplan angehalten: ").unwrap_or(f);
        let abhilfe = if z.verlauf_fehler.is_none() {
            " Datei prüfen, dann Dienst neu starten."
        } else {
            ""
        };
        return format!("Zeitplan angehalten: {grund}{abhilfe}");
    }
    if !z.zeitplan.an {
        return "Zeitplan aus. Einschalten unter Einrichtung.".into();
    }
    let Some(heute) = heute else {
        return "Uhrzeit unklar (Zeitumstellung); der Zeitplan wartet.".into();
    };
    let mut erledigt = z.erledigt;
    for auftrag in &z.warteschlange {
        if auftrag.anlass == Anlass::Zeitplan {
            erledigt.setze(auftrag.art, heute);
        }
    }
    let boersenlauf = match naechster_fuer(&z.zeitplan, &erledigt, heute, &z.boersen) {
        Some((art, d, u)) => {
            let mut t = format!(
                "{} am {} {} um {}",
                art.name(),
                wochentag_text(d),
                datum_text(d),
                uhrzeit_text(u)
            );
            if let Art::KurseVon(b) = art
                && z.zeitplan.papier_danach
                && crate::zeitplan::letzter_abruf_fuer(&z.zeitplan, d, b, &z.boersen)
            {
                t.push_str(", danach Papiertag bei mindestens einem erfolgreichen Kursabruf");
            }
            t
        }
        None => "Kein Lauf in den nächsten Tagen geplant.".into(),
    };
    if z.zeitplan.nachrichten_an {
        format!("{boersenlauf} · Schlagzeilen täglich {ZEITFENSTER}, frühestens alle 90 Minuten.")
    } else {
        boersenlauf
    }
}

fn zeitstrahl(z: &Zustand, heute: Option<Datum>, ki: &Aufrufe) -> String {
    let mut s = String::from("<section id=\"zeitstrahl\"><h2>Zeitstrahl</h2>");
    if z.automatik_status() == "angehalten" {
        let klasse = automatik_halt_klasse(z);
        let _ = write!(s, "<p{klasse}>{}</p>", maskiere(&naechster_text(z, heute)));
    } else if z.zeitplan.an
        && let Some(heute) = heute
    {
        s.push_str("<p class=\"leise\">Geplante Startzeiten in Frankfurter Ortszeit.</p>");
        zeitstrahl_tag(&mut s, z, heute, true, ki);
        if let Some((_, tag, _)) = naechster_fuer(&z.zeitplan, &z.erledigt, heute, &z.boersen)
            && tag != heute
        {
            zeitstrahl_tag(&mut s, z, tag, false, ki);
        }
        if z.zeitplan.nachrichten_an {
            s.push_str("<p><b>Schlagzeilen</b> · Abruf ohne KI");
            if ki.gemini_aktiv {
                s.push_str(", danach optional kostenlose Gemini-Zuarbeit");
            }
            let _ = write!(
                s,
                " · täglich {ZEITFENSTER}, frühestens alle 90 Minuten, wenn der Dienst frei ist.</p>"
            );
            if z.zeitplan.vorbereitung.aktiv()
                && !crate::zeitplan::handelstag(heute)
                && !ki_auftraege(z, heute).any(|a| a.art == Art::Vorbereiten)
            {
                s.push_str("<p><b>Vorbereitung</b> · nach einem erfolgreichen Schlagzeilenabruf.");
                zeitstrahl_ki::geplant(&mut s, heute);
                s.push_str("</p>");
            }
        }
    } else if z.zeitplan.an {
        s.push_str("<p>Die aktuelle Frankfurter Ortszeit ist nicht verfügbar.</p>");
    }
    if let Some(tag) = heute {
        weitere_ki_auftraege(&mut s, z, tag, ki);
    }
    s.push_str("</section>");
    s
}

fn ki_auftraege(z: &Zustand, tag: Datum) -> impl Iterator<Item = &Auftrag> {
    let am_tag = move |zeit: Zeitpunkt| zeit.in_frankfurt().is_some_and(|(d, ..)| d == tag);
    z.laufend
        .iter()
        .chain(
            z.verlauf
                .iter()
                .filter(move |a| am_tag(a.start) || a.ende.is_some_and(am_tag)),
        )
        .filter(|a| matches!(a.art, Art::Papier | Art::Vorbereiten | Art::Gemini))
}

fn hand_kennung<'a>(ki: &'a Aufrufe, a: &Auftrag) -> Option<&'a str> {
    if a.anlass != Anlass::Hand {
        return None;
    }
    ki.hand_kennung(&a.kennung, a.start, a.ende?)
}

fn weitere_ki_auftraege(s: &mut String, z: &Zustand, tag: Datum, ki: &Aufrufe) {
    let geplantes_papier_sichtbar = z.automatik_status() == "an"
        && z.zeitplan.papier_danach
        && !geplante_abrufe(z.zeitplan, &z.erledigt, tag, &z.boersen).is_empty();
    let schon_gezeigt = geplantes_papier_sichtbar
        .then(|| zeitstrahl_auftrag(z, tag, Art::Papier))
        .flatten();
    for a in ki_auftraege(z, tag) {
        if schon_gezeigt.is_some_and(|b| b.kennung == a.kennung) {
            continue;
        }
        let _ = write!(s, "<p><b>{}</b> · {}", a.art.name(), zeit_text(a.start));
        if a.art != Art::Gemini || !ki.hat_eintrag(&a.kennung) {
            let (klasse, stand) = ergebnis(a, z);
            let _ = write!(s, " <span class=\"{klasse}\">{stand}</span>");
        }
        if a.art != Art::Gemini || ki.hat_eintrag(&a.kennung) {
            ki.gelaufen(s, Some(&a.kennung), hand_kennung(ki, a));
        }
        s.push_str("</p>");
    }
    let zugeordnet = ki_auftraege(z, tag)
        .filter_map(|a| hand_kennung(ki, a))
        .collect();
    ki.konsolenlaeufe(s, tag, &zugeordnet);
}

fn zeitstrahl_tag(s: &mut String, z: &Zustand, tag: Datum, heute: bool, ki: &Aufrufe) {
    let abrufe = geplante_abrufe(z.zeitplan, &z.erledigt, tag, &z.boersen);
    let titel = if heute {
        "Heute"
    } else {
        "Nächster Börsentag"
    };
    let _ = write!(
        s,
        "<h3>{titel} · {} {}</h3>",
        wochentag_text(tag),
        datum_text(tag)
    );
    if abrufe.is_empty() {
        s.push_str("<p>Für diesen Tag ist kein Börsenlauf vorgesehen.</p>");
        return;
    }
    s.push_str("<ol class=\"zeitstrahl\">");
    for (uhr, art, vorgemerkt) in &abrufe {
        let ersatz = if *vorgemerkt {
            "vorgemerkt, Ergebnis unbekannt"
        } else {
            "geplant"
        };
        let (klasse, stand) = zeitstrahl_stand(z, tag, heute, *art).unwrap_or(("leise", ersatz));
        let _ = write!(
            s,
            "<li><b>{} Uhr</b> · {} · ohne KI <span class=\"{klasse}\">{stand}</span></li>",
            uhrzeit_text(*uhr),
            maskiere(art.name())
        );
    }
    if z.zeitplan.papier_danach {
        let ersatz = if z.erledigt.papier == Some(tag) {
            ("leise", "gestartet, Ergebnis unbekannt")
        } else if z.erledigt.kurse_ok != Some(tag)
            && abrufe
                .iter()
                .filter(|(_, art, _)| matches!(art, Art::KurseVon(_)))
                .all(|(_, art, vorgemerkt)| *vorgemerkt && z.auftrag_stand(*art).is_none())
        {
            ("halt", "entfällt: kein erfolgreicher Kursabruf")
        } else {
            (
                "leise",
                "nach dem letzten Kursabruf, wenn mindestens einer erfolgreich war",
            )
        };
        let (klasse, stand) = zeitstrahl_stand(z, tag, heute, Art::Papier).unwrap_or(ersatz);
        let _ = write!(
            s,
            "<li><b>Papiertag</b> <span class=\"{klasse}\">{stand}</span>"
        );
        let lauf = zeitstrahl_auftrag(z, tag, Art::Papier);
        if lauf.is_some() || z.erledigt.papier == Some(tag) {
            ki.gelaufen(
                s,
                lauf.map(|a| a.kennung.as_str()),
                lauf.and_then(|a| hand_kennung(ki, a)),
            );
        } else if klasse != "halt" {
            zeitstrahl_ki::geplant(s, tag);
        }
        s.push_str("</li>");
    }
    s.push_str("</ol>");
}

fn zeitstrahl_auftrag(z: &Zustand, tag: Datum, art: Art) -> Option<&Auftrag> {
    z.laufend.iter().chain(z.verlauf.iter()).find(|a| {
        a.art == art
            && a.anlass == Anlass::Zeitplan
            && a.start.in_frankfurt().is_some_and(|(d, ..)| d == tag)
    })
}

fn zeitstrahl_stand(
    z: &Zustand,
    tag: Datum,
    heute: bool,
    art: Art,
) -> Option<(&'static str, &'static str)> {
    if art == Art::Papier
        && z.verlauf.iter().any(|a| {
            a.art == Art::PapierAusgelassen && ortszeit(a.start).is_some_and(|(d, ..)| d == tag)
        })
    {
        return Some(("halt", "ausgelassen: vor Börsenschluss nicht gestartet"));
    }
    let passt = |a: &Auftrag| {
        a.art == art
            && a.anlass == Anlass::Zeitplan
            && ortszeit(a.start).is_some_and(|(d, ..)| d == tag)
    };
    if z.laufend.iter().any(passt) {
        Some(("offen", "läuft"))
    } else if heute
        && z.warteschlange
            .iter()
            .any(|a| a.art == art && a.anlass == Anlass::Zeitplan)
    {
        Some(("offen", "wartet"))
    } else {
        z.verlauf.iter().find(|a| passt(a)).map(|a| ergebnis(a, z))
    }
}

fn automatik_halt_klasse(z: &Zustand) -> &'static str {
    if z.automatik_fehler().is_some() {
        " class=\"halt\""
    } else {
        ""
    }
}

fn naechster_abschnitt(z: &Zustand, heute: Option<Datum>, aus_zeigen: bool) -> String {
    let planung = match z.automatik_status() {
        "aus" if aus_zeigen => {
            "<p>Zeitplan aus. <a href=\"/einrichtung#zeitplan\">Zeitplan ansehen</a></p>".into()
        }
        "aus" => String::new(),
        _ => {
            let klasse = automatik_halt_klasse(z);
            let planung = maskiere(&naechster_text(z, heute));
            let titel = if z.automatik_status() != "an" {
                ""
            } else if z.warteschlange.is_empty() {
                "Nächster geplanter Lauf: "
            } else {
                "Danach laut Zeitplan: "
            };
            format!("<p{klasse}>{titel}{planung}</p>")
        }
    };
    if let Some(a) = z.warteschlange.front() {
        let naechster = if z.verlauf_fehler.is_some() {
            "Wartet"
        } else {
            "Als Nächstes"
        };
        format!(
            "<p>{naechster}: {} ({})</p>{planung}",
            maskiere(a.art.name()),
            anlass_text(a.anlass)
        )
    } else {
        planung
    }
}

/// Fehler mit festen Wegen zurück. Eingabewerte werden dabei nicht übernommen.
pub fn fehler(status: u16, text: &str) -> Vec<u8> {
    let inhalt = format!(
        "<section><h2>Aktion nicht ausgeführt</h2><p>{}</p>\
         <p><a href=\"/\">Zurück zu Live</a> · <a href=\"/einrichtung\">Zugangsdaten prüfen</a></p></section>",
        maskiere(text)
    );
    crate::http::antwort(
        status,
        "text/html; charset=utf-8",
        rahmen("", None, None, &inhalt, None, "", ("", "")).as_bytes(),
    )
}

pub(crate) fn protokoll_seite(kennung: &str, text: &str) -> String {
    format!(
        "<section><h2>Protokoll {0}</h2><p><a href=\"/\">Zurück zu Live</a> · \
         <a href=\"/protokoll.txt?lauf={0}\" download=\"DepotSchlopf-{0}.log\">Herunterladen</a></p>\
         <details open><summary>Vollständiges gespeichertes Protokoll</summary>\
         <button type=\"button\" data-protokoll-kopieren>Protokoll kopieren</button> \
         <span class=\"leise\" role=\"status\"></span><pre>{1}</pre></details></section>",
        maskiere(kennung),
        maskiere(text)
    )
}

fn protokoll(a: &Auftrag, offen: bool) -> String {
    let mut text = String::new();
    for z in &a.protokoll {
        text.push_str(&maskiere(z));
        text.push('\n');
    }
    let kopieren = if text.is_empty() {
        text.push_str("(noch keine Ausgabe)");
        ""
    } else {
        "<button type=\"button\" data-protokoll-kopieren>Auszug kopieren</button> \
         <span class=\"leise\" role=\"status\"></span>"
    };
    format!(
        "<details id=\"protokoll-{}\"{}><summary>Protokoll ({} Zeilen)</summary>\
         {kopieren}<p><a href=\"/protokoll?lauf={0}\">Vollständiges Protokoll öffnen und kopieren</a> · \
         <a href=\"/protokoll.txt?lauf={0}\" download=\"DepotSchlopf-{0}.log\">Herunterladen</a></p>\
         <p class=\"leise\">Auszug mit den letzten 500 Zeilen. Es bleiben höchstens 20 Protokolle, \
         je bis zu 16 MiB. Überlange Zeilen und eine erreichte Größenbegrenzung werden im Protokoll vermerkt.</p>\
         <pre>{text}</pre></details>",
        maskiere(&a.kennung),
        if offen { " open" } else { "" },
        a.protokoll.len()
    )
}

fn ergebnis(a: &Auftrag, z: &Zustand) -> (&'static str, &'static str) {
    if a.art == Art::Update {
        if let Some(e) = z
            .update_ergebnisse
            .iter()
            .rev()
            .find(|e| e.lauf == a.kennung)
        {
            return e.phase.ergebnis();
        }
        if a.erfolg == Some(true) {
            return ("offen", "Einspielen beendet; Abschluss unbekannt");
        }
    }
    if a.frueher == Frueher::Unterbrochen {
        return ("halt", "unterbrochen");
    }
    match a.erfolg {
        Some(true) if a.art == Art::Gemini => ("leise", "Lauf beendet"),
        Some(true) => ("gut", "erfolgreich"),
        Some(false) => ("halt", "fehlgeschlagen"),
        None => ("offen", "läuft"),
    }
}

fn auftragsergebnis_html(a: &Auftrag, z: &Zustand, ki: &Aufrufe) -> String {
    if a.art == Art::Gemini && ki.hat_eintrag(&a.kennung) {
        let mut s = String::new();
        ki.gelaufen(&mut s, Some(&a.kennung), None);
        return s;
    }
    let (klasse, text) = ergebnis(a, z);
    format!("<span class=\"{klasse}\">{text}</span>")
}

/// Was der Dienst tut: laufender Auftrag, Warteschlange, naechster Lauf, Verlauf.
fn taetigkeit(z: &Zustand, heute: Option<Datum>, ki: &Aufrufe) -> String {
    let klasse = z.laufend.as_ref().map_or("", |_| "aktiver-bereich");
    let mut s = format!("<section id=\"taetigkeit\" class=\"{klasse}\"><h2>Tätigkeit</h2>");
    if let Some(a) = &z.laufend {
        let _ = write!(
            s,
            "<p class=\"kopf offen\">Läuft: {} ({}) seit {}</p><p class=\"leise\">Kennung {}</p>{}",
            maskiere(a.art.name()),
            anlass_text(a.anlass),
            maskiere(&zeit_text(a.start)),
            maskiere(&a.kennung),
            protokoll(a, true)
        );
        if a.protokoll.is_empty() {
            s.push_str("<p>Der Lauf wird vorbereitet. Beim ersten Start kann das ein paar Minuten dauern.</p>");
        }
        if let Some(h) = &a.hinweis {
            let _ = write!(
                s,
                "<p class=\"kopf offen\" id=\"hinweis\">{}</p>",
                maskiere(h)
            );
        }
    } else {
        s.push_str("<p class=\"kopf\">Gerade läuft nichts.</p>");
        // Was der letzte Lauf heute zu sagen hatte, etwa warum der Papiertag ausfiel.
        if let Some(a) = z.verlauf.front()
            && let Some(h) = &a.hinweis
            && heute.is_some_and(|t| {
                a.ende
                    .is_some_and(|e| ortszeit(e).is_some_and(|o| o.0 == t))
            })
        {
            let (klasse, weiter) = if a.erfolg == Some(true) {
                ("gut", String::new())
            } else {
                ("halt", format!(" {}", weiter_text(z, heute)))
            };
            let _ = write!(
                s,
                "<p class=\"kopf {klasse}\" id=\"hinweis\">{}: {}{}</p>",
                maskiere(a.art.name()),
                maskiere(h),
                maskiere(&weiter)
            );
        }
    }
    // Nach einem Neustart: wo der Tag stehen blieb und ob DepotSchlopf selbst weitermacht.
    if let Some((halt, text)) = heute.and_then(|t| crate::werkbank::nach_neustart(z, t)) {
        let klasse = if halt { "halt" } else { "gut" };
        let _ = write!(
            s,
            "<p class=\"kopf {klasse}\" id=\"neustart\">{}</p>",
            maskiere(&text)
        );
    }
    if !z.warteschlange.is_empty() {
        let liste: Vec<String> = z
            .warteschlange
            .iter()
            .map(|a| format!("{} ({})", a.art.name(), anlass_text(a.anlass)))
            .collect();
        let _ = write!(s, "<p>Wartet: {}</p>", maskiere(&liste.join(", ")));
    }
    s.push_str(&naechster_abschnitt(z, heute, false));
    letzter_fehler_hinweis(&mut s, z);
    if !z.verlauf.is_empty() {
        s.push_str(
            "<div class=\"breit\"><table><tr><th>Auftrag</th><th>Anlass</th><th>Start</th>\
             <th>Ende</th><th>Ergebnis</th></tr>",
        );
        for a in &z.verlauf {
            let ergebnis = auftragsergebnis_html(a, z, ki);
            let _ = write!(
                s,
                "<tr><td>{}<br><span class=\"leise\">{}</span></td><td>{}</td><td>{}</td><td>{}</td>\
                 <td>{ergebnis}{}{}</td></tr>",
                maskiere(a.art.name()),
                maskiere(&a.kennung),
                maskiere(&anlass_zeile(a)),
                maskiere(&zeit_text(a.start)),
                a.ende.map(|e| maskiere(&zeit_text(e))).unwrap_or_default(),
                a.hinweis
                    .as_ref()
                    .map(|h| format!("<br>{}", maskiere(h)))
                    .unwrap_or_default(),
                protokoll(a, false)
            );
        }
        s.push_str("</table></div>");
    }
    if let Some(f) = &z.verlauf_fehler {
        let _ = write!(s, "<p class=\"kopf halt\">{}</p>", maskiere(f));
    }
    for h in [&z.verlauf_vermerkfehler, &z.verlauf_hinweis]
        .into_iter()
        .flatten()
    {
        let _ = write!(s, "<p class=\"offen\">{}</p>", maskiere(h));
    }
    s.push_str("</section>");
    s
}

fn letzter_fehler_hinweis(s: &mut String, z: &Zustand) {
    if let Some(a) = z.verlauf.iter().find(|a| a.erfolg == Some(false))
        && let Some(fehler) = crate::protokoll::letzter_fehler(&a.protokoll)
    {
        let _ = write!(
            s,
            "<p class=\"halt\"><b>Letzter fehlgeschlagener Lauf</b> · {} · {}\
            <br>{}<br><a href=\"/protokoll?lauf={}\">Protokoll öffnen</a></p>",
            maskiere(a.art.name()),
            maskiere(&zeit_text(a.start)),
            maskiere(fehler),
            maskiere(&a.kennung)
        );
    }
}

/// Die Kennung des Starts im Inhalt; `/app.js` laedt die Seite neu, wenn sie wechselt (neue
/// Formulare nach einem Neustart).
pub fn marke(start: &str) -> String {
    format!("<span hidden data-start=\"{}\"></span>", maskiere(start))
}

/// Hinweis zum Update, falls es einen gibt.
fn update_hinweis(z: &Zustand) -> String {
    match &z.update_meldung {
        Some((fehler, text)) => format!(
            "<p class=\"kopf {}\">{}</p>",
            if *fehler {
                "halt"
            } else if z
                .update_ergebnisse
                .last()
                .is_some_and(|e| e.phase == crate::aktualisierung::Phase::Bestaetigt)
                && !z.neustart
                && z.auftrag_stand(Art::Update).is_none()
            {
                "gut"
            } else {
                "offen"
            },
            maskiere(text)
        ),
        None => String::new(),
    }
}

fn gesamtstatus_teil(token: &str, z: &Zustand, einrichtung: bool) -> String {
    let Some(b) = z.gesamtstatus() else {
        return if einrichtung {
            "<p>Die Einrichtung wird geprüft.</p>".into()
        } else {
            String::new()
        };
    };
    let journal = z.lage.as_ref().and_then(|l| {
        let p = l
            .bild
            .punkte
            .iter()
            .find(|p| p.abhilfe == Some(Abhilfe::JournalEinrichten))?;
        let status = z.auftrag_stand(Art::Einrichten)?;
        Some((format!("{}: {}", p.name, p.text), status))
    });
    let (klasse, titel) = match (&journal, b.gruende()) {
        (Some((grund, _)), [einziger]) if grund == einziger => ("offen", "Journal in Arbeit"),
        _ => (b.klasse(), b.titel()),
    };
    let automatik = z.automatik_status();
    let mut s = format!(
        "<section id=\"betrieb\"><div class=\"kopf {klasse}\">{}</div><p>Automatik: Zeitplan {automatik}</p>",
        maskiere(titel)
    );
    let mut gruende = String::new();
    for g in b.gruende() {
        if let Some((_, status)) = journal.as_ref().filter(|(grund, _)| grund == g) {
            let _ = write!(gruende, "<li>Journal einrichten {status}</li>");
        } else {
            let _ = write!(gruende, "<li>{}</li>", maskiere(g));
        }
    }
    if !gruende.is_empty() {
        let _ = write!(s, "<ul>{gruende}</ul>");
    }
    if einrichtung && !z.zeitplan.an {
        if let Some((ziel, text)) = einrichtungsziel(z) {
            if ziel == "#journal" {
                s.push_str(&auftrag_knopf(token, z, Art::Einrichten, text, true));
            } else {
                let _ = write!(s, "<a class=\"aktion primaer\" href=\"{ziel}\">{text}</a>");
            }
        } else if z.einschalten_erlaubt().is_ok() {
            s.push_str(
                "<a class=\"aktion primaer\" href=\"#zeitplan\">Zum Einschalten des Zeitplans</a>",
            );
        }
    } else if let Some(l) = &z.lage {
        if let Some(k) = l
            .bild
            .punkte
            .iter()
            .filter(|p| p.ampel != depot_lage::Ampel::Gut)
            .find_map(|p| p.abhilfe.and_then(|a| abhilfe_knopf(token, z, a, true)))
        {
            s.push_str(&k);
        }
        let _ = write!(
            s,
            "<div class=\"leise\">Stand {}</div>",
            maskiere(&zeit_text(l.bild.jetzt))
        );
    }
    s.push_str("</section>");
    s
}

/// Gemeinsame, unabhängig von Formularen aufgefrischte Betriebsleiste.
pub fn betriebsleiste(start: &str, z: &Zustand, heute: Option<Datum>) -> String {
    betriebsleiste_mit_kurssperre(start, z, heute, None)
}

pub(crate) fn betriebsleiste_mit_kurssperre(
    start: &str,
    z: &Zustand,
    heute: Option<Datum>,
    kurssperre: Option<&str>,
) -> String {
    let mut s = marke(start);
    let _ = write!(
        s,
        "<span hidden data-otter=\"{}\"></span>",
        otter::pose(z, kurssperre.is_some())
    );
    if let Some(a) = &z.laufend {
        let _ = write!(
            s,
            "<div class=\"betrieb aktiv\"><strong><progress aria-label=\"Auftrag läuft\"></progress>Jetzt: {}</strong> \
             <a href=\"/#taetigkeit\">Fortschritt ansehen</a><p>Seit {} · {}</p>",
            maskiere(a.art.name()),
            maskiere(&zeit_text(a.start)),
            anlass_text(a.anlass)
        );
        if let Some(h) = &a.hinweis {
            let _ = write!(s, "<p>{}</p>", maskiere(h));
        } else {
            s.push_str("<p>DepotSchlopf arbeitet. Du musst den Auftrag nicht erneut starten.</p>");
        }
    } else if z.neustart {
        s.push_str("<div class=\"betrieb aktiv\"><strong>Update: Neustart wird vorbereitet.</strong><p>Die Oberfläche verbindet sich von selbst neu.</p>");
    } else {
        s.push_str("<div class=\"betrieb\"><strong>Im Moment läuft kein Auftrag.</strong>");
        s.push_str(&naechster_abschnitt(z, heute, true));
        if let Some(a) = z.verlauf.front() {
            let (_, ergebnis) = ergebnis(a, z);
            let _ = write!(
                s,
                "<p>Zuletzt: {} · {} · <a href=\"/#taetigkeit\">Ergebnis ansehen</a></p>",
                maskiere(a.art.name()),
                ergebnis
            );
        }
    }
    let danach = if z.laufend.is_some() || z.neustart {
        z.warteschlange.len()
    } else {
        z.warteschlange.len().saturating_sub(1)
    };
    match danach {
        0 => {}
        1 => s.push_str("<p>Danach wartet 1 Auftrag.</p>"),
        n => {
            let _ = write!(s, "<p>Danach warten {n} Aufträge.</p>");
        }
    }
    s.push_str("</div>");
    if z.auftrag_stand(Art::Update).is_none()
        && let Some((sha, _)) = z.update_angebot()
    {
        let _ = write!(
            s,
            "<div class=\"update-angebot\"><p><strong>Update verfügbar</strong> · Version {}</p>\
            <a class=\"aktion primaer\" href=\"/einrichtung#update\">Update ansehen</a> \
            <span>Einspielen erst nach deiner Bestätigung.</span></div>",
            maskiere(crate::rueckfall::kurz(sha))
        );
    }
    s.push_str(&update_hinweis(z));
    if let Some(text) = kurssperre {
        s.push_str(text);
    }
    s
}

/// Der Inhalt der Live-Seite (auch `/teil/live`).
pub fn live(token: &str, start: &str, z: &Zustand, jetzt: Zeitpunkt) -> String {
    live_mit_ki(token, start, z, jetzt, &Aufrufe::default())
}

fn vorbereitung_auf_live(token: &str, z: &Zustand, jetzt: Zeitpunkt) -> String {
    let stand = match z.automatik_status() {
        "angehalten" => "Automatische Vorbereitung pausiert: Zeitplan angehalten.",
        "aus" => "Automatische Vorbereitung pausiert.",
        _ if z.zeitplan.vorbereitung.aktiv() && z.zeitplan.nachrichten_an => {
            "Automatische Vorbereitung an börsenfreien Tagen ist eingeschaltet."
        }
        _ => "Automatische Vorbereitung an börsenfreien Tagen ist ausgeschaltet.",
    };
    let mut s = format!("<p>{stand} <a href=\"/einrichtung#zeitplan\">Zeitplan ansehen</a></p>");
    if let Some(grund) = z
        .vorbereitung_pruefen(jetzt)
        .err()
        .and_then(vorbereitung_meldung)
    {
        let _ = write!(s, "<p class=\"leise\">{}</p>", maskiere(grund));
    } else if let Some(knopf) = abhilfe_knopf(token, z, Abhilfe::Vorbereiten, false) {
        s.push_str(&knopf);
    }
    s
}

pub(crate) fn live_mit_ki(
    token: &str,
    start: &str,
    z: &Zustand,
    jetzt: Zeitpunkt,
    ki: &Aufrufe,
) -> String {
    let heute = ortszeit(jetzt).map(|(d, ..)| d);
    let mut s = marke(start);
    s.push_str(&zeitstrahl(z, heute, ki));
    s.push_str(&taetigkeit(z, heute, ki));
    s.push_str(&gesamtstatus_teil(token, z, false));
    s.push_str(&update_hinweis(z));
    if z.laufend.is_some() && z.lage.is_some() {
        s.push_str("<p class=\"leise\">Das Lagebild zeigt den Stand vor dem laufenden Auftrag und wird danach aktualisiert.</p>");
    }
    match &z.lage {
        Some(l) => s.push_str(&depot_lage::html::inhalt_mit(
            &l.bild,
            &|a| {
                if a == Abhilfe::Vorbereiten {
                    return Some(vorbereitung_auf_live(token, z, jetzt));
                }
                abhilfe_knopf(token, z, a, false)
            },
            false,
        )),
        None => s.push_str("<section><p class=\"leise\">Lagebild wird berechnet …</p></section>"),
    }
    s
}

/// Kurzer Stand fuer die Einrichtung (auch `/teil/status`).
pub fn status(token: &str, start: &str, z: &Zustand, _heute: Option<Datum>) -> String {
    format!("{}{}", marke(start), gesamtstatus_teil(token, z, true))
}

fn regel(w: Wer) -> &'static str {
    match w {
        Wer::Kursquelle => "8 bis 64 Großbuchstaben und Ziffern",
        Wer::KursquelleUs => "20 bis 64 Kleinbuchstaben und Ziffern",
        Wer::Berater(_) | Wer::Gemini => "20 bis 256 Zeichen aus A-Z, a-z, 0-9, _ und -",
    }
}

/// Eingaben des Zeitplan-Formulars, die nach einer Ablehnung wieder im Formular stehen. Nur
/// gueltige Werte werden uebernommen; ein Schluessel ist nie dabei.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Entwurf {
    pub termine: Option<(u8, u8)>,
    pub kurse: Option<(u8, u8)>,
    pub papier_danach: Option<bool>,
    pub nachrichten_an: Option<bool>,
    pub vorbereitung_an: Option<bool>,
    pub hinweis_an: Option<bool>,
}

impl Entwurf {
    /// Aus einem Formular oder der Abfrage einer Seite. Ohne `entwurf=ja` ist nichts zu
    /// uebernehmen (sonst waeren fehlende Haken nicht von einem fehlenden Entwurf zu trennen).
    pub fn aus_formular(f: &[(String, String)]) -> Entwurf {
        let haken = |n: &str| match feld(f, n) {
            None => Some(false),
            Some("ja") => Some(true),
            Some(_) => None,
        };
        let zeit = |n: &str| feld(f, n).and_then(lies_uhrzeit);
        let ist_formular = feld(f, "t").is_some() || feld(f, "entwurf") == Some("ja");
        if !ist_formular {
            return Entwurf::default();
        }
        Entwurf {
            termine: zeit("termine"),
            kurse: zeit("kurse"),
            papier_danach: haken("papier_danach"),
            nachrichten_an: haken("nachrichten_an"),
            vorbereitung_an: haken("vorbereitung_an"),
            hinweis_an: haken("hinweis_an"),
        }
    }

    /// Als Teil einer Abfrage (`&entwurf=ja&...`), nur mit gueltigen Werten.
    pub fn als_abfrage(&self) -> String {
        let mut q = String::from("&entwurf=ja");
        if self.vorbereitung_an == Some(true) {
            q.push_str("&vorbereitung_an=ja");
        }
        if self.hinweis_an == Some(true) {
            q.push_str("&hinweis_an=ja");
        }
        if self.nachrichten_an == Some(true) {
            q.push_str("&nachrichten_an=ja");
        }
        if self.papier_danach == Some(true) {
            q.push_str("&papier_danach=ja");
        }
        for (n, w) in [("termine", self.termine), ("kurse", self.kurse)] {
            if let Some(u) = w {
                let _ = write!(q, "&{n}={}", uhrzeit_text(u).replace(':', "%3A"));
            }
        }
        q
    }

    fn leer(self) -> bool {
        self == Entwurf::default()
    }
}

/// Was die Einrichtung ueber Gespeichertes weiss.
#[derive(Clone, Debug)]
pub struct Einrichtung {
    /// Stand je Schluessel aus Anmeldeverwaltung und Anbieterprotokoll.
    /// Versuche heute und in den letzten 24 Stunden.
    pub alpha_kontingent: Option<(usize, usize)>,
    pub schluessel: Vec<(Wer, SchluesselStand)>,
    /// Inhalt von `daten/ki.halt`, falls die Sperre des Kostenschutzes liegt.
    pub ki_halt: Result<Option<String>, String>,
    /// Inhalt von `daten/kurse.halt`, falls die Kurssperre liegt.
    pub kurse_halt: Result<Option<String>, String>,
    /// Journalabdruck und Bargeld fuer eine freigabefaehige Papier-Sperre.
    pub papier_freigabe: Option<depot_lage::PapierFreigabeAngebot>,
    /// `Some(true)`: Datei stimmt mit dem aktiven Zeitplan ueberein; `Some(false)`: weicht ab
    /// oder ist unlesbar; `None`: keine Datei (Voreinstellung).
    pub zeitplan_gespeichert: Option<bool>,
    /// Abgelehnte Eingaben, die wieder im Formular stehen.
    pub entwurf: Entwurf,
    pub autostart: autostart::Stand,
    pub wurzel: String,
    pub grenzen: Vec<Grenzwert>,
    /// Staende, die nach einem Update zurueckgesetzt wurden (`daten/update_abgelehnt.txt`),
    /// oder warum die Datei nicht lesbar ist.
    pub abgelehnt: Result<Vec<String>, String>,
}

fn schluessel_teil(token: &str, e: &Einrichtung, s: &mut String) {
    s.push_str(
        "<section id=\"schluessel\"><h2>Zugangsdaten (API-Schlüssel)</h2><p class=\"leise\">Gespeichert wird nur in \
         der Windows-Anmeldeinformationsverwaltung, nie im Projektordner. Die Seite zeigt keinen \
         Schlüssel, nur ob einer hinterlegt ist und was der Anbieter beim letzten Aufruf dazu \
         sagte. Geprüft wird nur bei echten Läufen, nie mit eigenen, kostenpflichtigen \
         Testaufrufen. <b>Speichern heißt hier zugleich aktiv:</b> Der nächste Lauf liest ihn.</p>",
    );
    s.push_str(SCHLUESSEL_TABELLE);
    for (w, st) in &e.schluessel {
        let kontingent = if *w == Wer::Kursquelle {
            e.alpha_kontingent.map_or_else(
                || "<br>Alpha-Vantage-Kontingent derzeit unlesbar".into(),
                |(heute, gesamt)| {
                    format!(
                        "<br>Heute {heute} von {} Anfragen · letzte 24 Stunden: {gesamt}",
                        depot_lage::abrufkontingent::AV_TAG
                    )
                },
            )
        } else {
            String::new()
        };
        schluessel_zeile(s, token, *w, *st, &kontingent);
    }
    s.push_str("</table></section>");
}

pub(crate) const SCHLUESSEL_TABELLE: &str =
    "<table class=\"zugang-tabelle\"><tr><th>Wofür</th><th>Stand</th><th>Neu hinterlegen</th></tr>";

pub(crate) fn schluessel_zeile(
    s: &mut String,
    token: &str,
    wer: Wer,
    stand: SchluesselStand,
    zusatz: &str,
) {
    let klasse = stand.klasse();
    let text = maskiere(&stand.text());
    let name = maskiere(wer.name());
    let _ = write!(
        s,
        "<tr><td>{name}<br><span class=\"leise\">{}</span></td>\
         <td data-titel=\"Stand\"><span class=\"{klasse}\">{text}</span>{zusatz}</td><td data-titel=\"Neu hinterlegen\">\
         <form method=\"post\" action=\"/schluessel\" data-aenderungen>\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <input type=\"hidden\" name=\"wer\" value=\"{}\">\
         <input aria-label=\"Schlüssel für {name}\" type=\"password\" name=\"wert\" autocomplete=\"new-password\" required>\
         <div class=\"speicher-aktionen\"><p data-speicherstand role=\"status\">Noch keinen neuen Schlüssel eingegeben.</p><button data-speichern-hervorheben type=\"submit\">Schlüssel speichern</button></div>\
         <div class=\"leise\">{}</div></form></td></tr>",
        maskiere(wer.adresse()),
        maskiere(token),
        wer.kurz(),
        regel(wer)
    );
}

/// Was bei einer abgebrochenen KI-Anfrage geschieht, in Alltagssprache (Maus, 05.11.2026).
pub const KI_VERBINDUNG: &str = "<details id=\"ki-verbindung\"><summary>Wenn die Verbindung bei \
     einer KI-Anfrage wegbricht</summary><ul>\
     <li><b>Die Anfrage ist gar nicht erst hinausgegangen</b> (kein Internet, Adresse nicht \
     gefunden, Verbindung abgewiesen): Sie hat nichts gekostet. DepotSchlopf versucht es nach 30 \
     und nach 60 Sekunden von selbst noch einmal. Klappt es dann nicht, läuft der Papiertag \
     heute ohne diese KI. Du musst nichts tun.</li>\
     <li><b>Die Anfrage war schon abgeschickt, dann brach die Verbindung ab</b>: Sie kann beim \
     Anbieter angekommen sein und Geld gekostet haben, ohne dass DepotSchlopf die Antwort kennt. \
     Ein zweiter Versuch könnte doppelt kosten. Deshalb fragt DepotSchlopf nicht noch einmal, \
     sondern hält Dachs und Maus an. Du siehst im Konto des Anbieters nach, was der Aufruf gekostet \
     hat, und hebst die Sperre hier auf.</li></ul></details>";

fn kostenschutz_teil(token: &str, e: &Einrichtung, s: &mut String) {
    s.push_str("<section id=\"kostenschutz\"><h2>Sperre des Kostenschutzes</h2>");
    let inhalt = match &e.ki_halt {
        Ok(None) => {
            s.push_str(
                "<p><span class=\"gut\">frei</span>: Keine Sperre. Jeder KI-Aufruf durchläuft \
                 trotzdem die Kostenprüfung.</p>",
            );
            s.push_str(KI_VERBINDUNG);
            s.push_str("</section>");
            return;
        }
        Ok(Some(t)) => t,
        Err(f) => {
            let _ = write!(
                s,
                "<p class=\"halt\">{}. Die KI wird nicht gefragt, bis die Datei lesbar ist.</p></section>",
                maskiere(f)
            );
            return;
        }
    };
    let _ = write!(
        s,
        "<p><span class=\"halt\">gesperrt</span>: Ein KI-Aufruf hat mehr gekostet als \
         reserviert, oder seine Kosten sind unbekannt, etwa weil die Verbindung nach dem \
         Absenden abbrach. Bis zur Prüfung pausieren Dachs und Maus beim Papiertag und bei der Vorbereitung. \
         Eingeschaltete kostenlose Gemini-Zuarbeit kann weiterlaufen.</p><pre>{}</pre>{}\
         <p class=\"leise\">So prüfen: im Konto des Anbieters die Kosten des genannten Aufrufs \
         ansehen. Aufheben ändert nichts am Kassenbuch: Die Reservierung bleibt gebucht und zählt \
         im Budget. Jeder weitere Aufruf durchläuft wieder die Kostenprüfung und sperrt bei einer \
         Abweichung erneut. Der Grund bleibt in daten/ki_halt_aufgehoben.txt.</p>\
         <form method=\"post\" action=\"/kostenschutz\">\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <input type=\"hidden\" name=\"abdruck\" value=\"{}\">\
         <label><input type=\"checkbox\" name=\"geprueft\" value=\"ja\" required> \
         Ich habe die Kosten beim Anbieter geprüft.</label>\
         <p>Bestätige das Aufheben innerhalb von zwei Minuten im Windows-Dialog.</p>\
         <button type=\"submit\">Sperre aufheben</button></form></section>",
        maskiere(inhalt),
        KI_VERBINDUNG,
        maskiere(token),
        kostenschutz::abdruck(inhalt)
    );
}

pub(crate) fn kurssperre_status(daten: &std::path::Path, heute: Option<Datum>) -> Option<String> {
    use depot_lage::kurssperre;
    let text = match kurssperre::lies(daten) {
        Ok(None) => return None,
        Ok(Some(sperre)) => {
            let zeit = sperre.seit.map_or_else(|| "unbekannt".into(), zeit_text);
            let naechste = if sperre.art != kurssperre::Art::Unklar {
                ""
            } else if sperre.quelle.is_none() {
                " · Prüfung in der Einrichtung"
            } else if heute.is_some_and(|tag| kurssperre::pruefdatum(daten) == Some(tag)) {
                " · Nächste automatische Prüfung frühestens morgen"
            } else {
                " · Automatische Prüfung beim nächsten Termin- oder Kurslauf"
            };
            format!(
                "Kurssperre seit {zeit}: {} · {}{naechste}",
                sperre.grund,
                sperre.art.text()
            )
        }
        Err(_) => "Kurssperre: Datei unlesbar".into(),
    };
    Some(format!(
        "<div class=\"betrieb halt\">{} · <a href=\"/einrichtung#kurssperre\">Einrichtung</a></div>",
        maskiere(&text)
    ))
}

fn kurssperre_teil(token: &str, z: &Zustand, e: &Einrichtung, s: &mut String) {
    let inhalt = match &e.kurse_halt {
        // Ohne Kurssperre kein eigener Abschnitt: Sie kommt nur nach einem Sicherheitsabbruch vor.
        Ok(None) => return,
        Ok(Some(t)) => t,
        Err(f) => {
            let _ = write!(
                s,
                "<section id=\"kurssperre\"><h2>Kurssperre</h2><p class=\"halt\">{}. Kein \
                 Abruf und kein Papiertag, bis die Datei lesbar ist.</p></section>",
                maskiere(f)
            );
            return;
        }
    };
    let _ = write!(
        s,
        "<section id=\"kurssperre\"><h2>Kurssperre</h2>\
         <p><span class=\"halt\">gesperrt</span>: Kursabrufe sind aus Sicherheitsgründen angehalten. Unklare Altfälle prüft DepotSchlopf automatisch. \
         Bis zur Freigabe gibt es keine regulären Kursabrufe, \
         und kein Papiertag beginnt, auch nicht von Hand.</p><pre>{}</pre>\
         <p>Empfohlen: Zuerst <b>Kurssperre sicher untersuchen</b>. Der Prüfabruf zeigt nur \
         Quelle, erkannten Zugang und Antwortart. Keine Antwort und keine Kurse werden gespeichert, \
         die Sperre bleibt bestehen. Bis zu 15 Abrufe mit 15 Sekunden Abstand verbrauchen \
         Quellenkontingent und können mehrere Minuten dauern. Das Ergebnis steht auf Live \
         unter Tätigkeit im kopierbaren Protokoll.</p>{}\
         <p class=\"leise\">So prüfen: Beim Anbieter nachsehen, ob der Schlüssel noch gilt, und \
         ihn bei Zweifel neu ausstellen und unter Zugangsdaten neu hinterlegen. Aufheben ändert nichts \
         am Journal. Der nächste Abruf holt alle Kurse neu und prüft sie wie immer. Der Grund \
         bleibt in daten/kurse_halt_aufgehoben.txt.</p>\
         <form method=\"post\" action=\"/kurssperre\">\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <input type=\"hidden\" name=\"abdruck\" value=\"{}\">\
         <label><input type=\"checkbox\" name=\"geprueft\" value=\"ja\" required> \
         Ich habe den Schlüssel der Kursquelle geprüft und bei Bedarf erneuert.</label>\
         <p>Bestätige das Aufheben innerhalb von zwei Minuten im Windows-Dialog.</p>\
         <button type=\"submit\">Kurssperre aufheben</button></form></section>",
        maskiere(inhalt),
        auftrag_knopf(
            token,
            z,
            Art::Kursdiagnose,
            "Kurssperre sicher untersuchen",
            true
        ),
        maskiere(token),
        kostenschutz::abdruck(inhalt)
    );
}

pub(crate) fn quellen_teil(
    token: &str,
    start: &str,
    z: &Zustand,
    alpha: Option<(usize, usize)>,
) -> String {
    let mut s = marke(start);
    s.push_str(
        "<section id=\"quellen\"><h2>Datenquellen und Verbindungstest</h2><p class=\"leise\">\
         Jeder Verbindungstest startet einen echten Abruf. DepotSchlopf speichert die Ergebnisse im Journal. \
         Das Protokoll steht auf Live unter Tätigkeit. An börsenfreien Tagen kann ein erfolgreicher \
         Schlagzeilenabruf die eingeschaltete automatische Vorbereitung auslösen. Diese nutzt KI-Budget.</p>\
         <div class=\"breit\"><table class=\"quellen-tabelle\"><tr><th>Quelle</th><th>Letzter Abruf</th><th></th></tr>",
    );
    for (art, name) in [
        (
            Art::Kurse,
            "Schlusskurse aller Börsen der Liste (Xetra: Alpha Vantage, US-Börsen: Tiingo mit \
             EZB-Referenzkursen; je mit Schlüssel)",
        ),
        (Art::Termine, "Termine der EZB (ohne Schlüssel)"),
        (
            Art::Nachrichten,
            "Schlagzeilen: tagesschau und BBC World (kostenlos, ohne Schlüssel; mindestens 90 Minuten Abrufpause)",
        ),
    ] {
        // Ein Kursabruf je Boerse zaehlt fuer die Kursquellen mit.
        let gleich = |a: Art| a == art || (art == Art::Kurse && matches!(a, Art::KurseVon(_)));
        let zuletzt = z.verlauf.iter().find(|a| gleich(a.art)).map_or_else(
            || "<span class=\"leise\">Kein Abruf im Verlauf</span>".into(),
            |a| {
                let (klasse, text) = ergebnis(a, z);
                let umfang = match a.art {
                    Art::KurseVon(boerse) => format!("{} · ", maskiere(boerse.name())),
                    Art::Kurse => "Alle Börsen · ".into(),
                    _ => String::new(),
                };
                format!(
                    "{umfang}<span class=\"{klasse}\">{text}</span> {}",
                    maskiere(&zeit_text(a.start))
                )
            },
        );
        let _ = write!(
            s,
            "<tr><td>{}</td><td>{zuletzt}</td><td>{}</td></tr>",
            maskiere(name),
            auftrag_knopf(token, z, art, art.name(), false)
        );
        if art == Art::Kurse {
            let kontingent = alpha.map_or_else(
                || "Alpha-Vantage-Tageskontingent unbekannt.".into(),
                |(heute, _)| {
                    format!(
                        "Heute {heute} von {} Alpha-Vantage-Anfragen.",
                        depot_lage::abrufkontingent::AV_TAG
                    )
                },
            );
            let _ = write!(
                s,
                "<tr><td colspan=\"3\" class=\"leise\">{kontingent}</td></tr>"
            );
        }
    }
    s.push_str("</table></div></section>");
    s
}

fn zeitplan_voraussetzungen(zustand: &Zustand) -> (String, &'static str) {
    let mut voraussetzung = String::new();
    let mut gesperrt = "";
    if !zustand.zeitplan.an {
        match zustand.einschalten_erlaubt() {
            Ok(()) => {}
            Err(ZeitplanAbgelehnt::Einrichtung(fehlt)) => {
                gesperrt = " disabled";
                voraussetzung
                    .push_str("<p>Einschalten geht erst, wenn diese Punkte erledigt sind.</p><ul>");
                for punkt in fehlt {
                    let _ = write!(voraussetzung, "<li>{}</li>", maskiere(&punkt));
                }
                voraussetzung.push_str("</ul>");
                if let Some((ziel, text)) = einrichtungsziel(zustand) {
                    let _ = write!(voraussetzung, "<p><a href=\"{ziel}\">{text}</a></p>");
                }
            }
            Err(_) => {
                gesperrt = " disabled";
                voraussetzung.push_str(
                    "<p>Der Dienst prüft die Einrichtung. Lade den Stand \
                    nach ein paar Sekunden neu.</p>",
                );
            }
        }
    }
    if !gesperrt.is_empty() {
        voraussetzung.push_str(
            "<form method=\"get\" action=\"/einrichtung#zeitplan\">\
             <button type=\"submit\">Stand neu laden</button></form>",
        );
    }
    (voraussetzung, gesperrt)
}

fn einrichtungsziel(z: &Zustand) -> Option<(&'static str, &'static str)> {
    let l = z.lage.as_ref()?;
    l.bild.punkte.iter().find_map(|p| match p.abhilfe {
        Some(Abhilfe::JournalEinrichten) => Some(("#journal", "Journal einrichten")),
        Some(Abhilfe::KostenschutzPruefen) => Some(("#kostenschutz", "Kostenschutz prüfen")),
        Some(Abhilfe::KurssperrePruefen) => Some(("#kurssperre", "Kurssperre prüfen")),
        Some(
            Abhilfe::SchluesselKursquelle
            | Abhilfe::SchluesselKursquelleUs
            | Abhilfe::SchluesselBerater(_),
        ) => Some(("#schluessel", "Zugangsdaten prüfen")),
        _ => None,
    })
}

fn zeitplan_teil(token: &str, zustand: &Zustand, e: &Einrichtung, s: &mut String) {
    let p: &Zeitplan = &zustand.zeitplan;
    let fehler = zustand.automatik_fehler();
    let gespeichert = match (e.zeitplan_gespeichert, fehler) {
        (_, Some(f)) => format!("<p class=\"halt\">{}</p>", maskiere(f)),
        (Some(true), None) => "<p>Einstellungen gespeichert.</p>".into(),
        (Some(false), None) => "<p class=\"offen\">Datei weicht vom laufenden Stand ab.</p>".into(),
        (None, None) => "<p>Es gelten die voreingestellten Uhrzeiten.</p>".into(),
    };
    let haken = |b: bool| if b { " checked" } else { "" };
    let ent = &e.entwurf;
    let entwurf_hinweis = if ent.leer() {
        ""
    } else {
        "<p class=\"offen\">Das Formular zeigt deine letzten, nicht gespeicherten Eingaben.</p>"
    };
    let (voraussetzung, gesperrt) = zeitplan_voraussetzungen(zustand);
    let stand = if p.an { "an" } else { "aus" };
    let betrieb = zustand.automatik_status();
    let token = maskiere(token);
    let schalter = if p.an {
        format!(
            "<form method=\"post\" action=\"/zeitplan\"><input type=\"hidden\" name=\"t\" value=\"{token}\"><input type=\"hidden\" name=\"aktion\" value=\"aus\"><button type=\"submit\">Zeitplan ausschalten</button></form>"
        )
    } else if gesperrt.is_empty() {
        "<p>Automatische Läufe sind aus. Prüfe die Einstellungen und wähle unten <b>Speichern und Zeitplan einschalten</b>.</p>".into()
    } else {
        "<p>Automatische Läufe sind aus. Du kannst die Einstellungen schon speichern. Zum Einschalten erledige zuerst die Punkte oben.</p>".into()
    };
    let einschalten = if p.an {
        String::new()
    } else {
        format!(
            "<button class=\"primaer\" type=\"submit\" name=\"aktion\" value=\"speichern_an\"{gesperrt}>Speichern und Zeitplan einschalten</button>"
        )
    };
    let entwurf = if ent.leer() { "nein" } else { "ja" };
    let (speicher_text, speicher_hinweis) = if p.an {
        ("Änderungen speichern", "Der Zeitplan bleibt eingeschaltet.")
    } else {
        (
            "Nur speichern · Zeitplan bleibt aus",
            "Mit „Nur speichern“ bleiben automatische Läufe ausgeschaltet.",
        )
    };
    let kurse_hinweis = if crate::zeitplan::morgenplan(p) {
        "Im Morgenlauf gilt der letzte abgeschlossene Handelstag je Börse."
    } else {
        "Beim Abendplan wird jede andere Börse der Liste zehn Minuten nach ihrem eigenen Schluss abgerufen."
    };
    let speichern_hervorheben = if p.an || !gesperrt.is_empty() {
        " data-speichern-hervorheben"
    } else {
        ""
    };
    let _ = write!(
        s,
        "<section id=\"zeitplan\"><h2>Zeitplan</h2>\
         <p><b>{betrieb}</b> · Empfohlen, wenn DepotSchlopf nach einem erfolgreichen manuellen \
         Test selbstständig laufen soll.</p>{gespeichert}{voraussetzung}\
         {schalter}\
         <h3>Uhrzeiten und automatische Läufe</h3>\
         <p>Änderungen gelten ab dem Speichern. Kurs- und Terminläufe richten sich nach den Handelstagen der Börsenliste und nach Frankfurter Zeit. Zusätzliche Schlagzeilen lassen sich unten einschalten: täglich {ZEITFENSTER}, frühestens alle 90 Minuten. An börsenfreien Tagen kann ein erfolgreicher Abruf die unten aktivierte Vorbereitung auslösen. Diese Vorbereitung nutzt KI-Budget. \
         Für beide Börsen ist 07:30 Uhr Frankfurter Zeit voreingestellt.</p>\
         {entwurf_hinweis}\
         <form id=\"zeitplan-formular\" method=\"post\" action=\"/zeitplan\" data-aenderungen data-entwurf=\"{entwurf}\">\
         <input type=\"hidden\" name=\"t\" value=\"{token}\">\
         <input type=\"hidden\" name=\"planstand\" value=\"{stand}\">\
         <label>Termine abrufen um <input type=\"time\" name=\"termine\" value=\"{}\" required> <span class=\"leise\">Frankfurter Zeit · 24 Stunden</span></label>\
         <label>Kursabruf um <input type=\"time\" name=\"kurse\" value=\"{}\" required> <span class=\"leise\">Frankfurter Zeit · 24 Stunden</span></label>\
         <p class=\"leise\">{kurse_hinweis}</p>\
         <label><input type=\"checkbox\" name=\"papier_danach\" value=\"ja\"{}> \
         Nach dem letzten Kursabruf des Tages den Papiertag starten (wenn ein Abruf des Tages \
         erfolgreich war; gilt nur bei eingeschaltetem Zeitplan)</label>\
         <label><input type=\"checkbox\" name=\"nachrichten_an\" value=\"ja\"{}> \
         Zusätzlich tagsüber Schlagzeilen abrufen (empfohlen für das Weltmodell, kostenlos; \
         gilt nur bei eingeschaltetem Zeitplan)</label>\
         <label><input type=\"checkbox\" name=\"vorbereitung_an\" value=\"ja\"{}> \
         An börsenfreien Tagen den nächsten Handelstag vorbereiten (empfohlen; braucht Schlagzeilen und eingeschalteten Zeitplan). {VORBEREITUNG_HAEUFIGKEIT} Möglich {ZEITFENSTER}. \
         Erst nach bestätigtem Update verfügbar. Der Quellenstand wird auch bei fehlendem KI-Budget belegt. Für KI-Aufrufe gelten der Tagesrahmen und die Rücklage für Erstmeinungen. An Handelstagen werden die Hypothesen in den regulären Aufrufen neu geprüft.</label>\
         <label><input type=\"checkbox\" name=\"hinweis_an\" value=\"ja\"{}> \
         Windows-Hinweis bei Eingriff (empfohlen; zeigt im Infobereich eine Sprechblase, sobald \
         ein neuer Grund für „Eingriff nötig“ entsteht oder ein Papiertag ausgefallen ist; löst \
         nichts aus und gilt auch bei ausgeschaltetem Zeitplan)</label>\
         <div class=\"speicher-aktionen\"><p data-speicherstand role=\"status\">Änderungen werden erst durch Speichern wirksam.</p>\
         {einschalten} <button type=\"submit\" name=\"aktion\" value=\"speichern\"{speichern_hervorheben}>{speicher_text}</button>\
         <p>{speicher_hinweis} Du kannst die Einrichtung jederzeit später fortsetzen.</p></div></form></section>",
        uhrzeit_text(ent.termine.unwrap_or(p.termine)),
        uhrzeit_text(ent.kurse.unwrap_or(p.kurse)),
        haken(ent.papier_danach.unwrap_or(p.papier_danach)),
        haken(ent.nachrichten_an.unwrap_or(p.nachrichten_an)),
        haken(ent.vorbereitung_an.unwrap_or(p.vorbereitung.aktiv())),
        haken(ent.hinweis_an.unwrap_or(p.hinweis.aktiv()))
    );
}

fn autostart_teil(token: &str, e: &Einrichtung, s: &mut String) {
    s.push_str(
        "<section id=\"autostart\"><h2>Autostart mit Windows</h2>\
         <p>Optional. Empfohlen, wenn DepotSchlopf bei deiner Windows-Anmeldung von selbst \
         starten soll. Der Browser bleibt geschlossen.</p>",
    );
    let (text, knopf, wert, bestaetigung) = match e.autostart {
        autostart::Stand::An => (
            "Für diesen Projektordner eingeschaltet.",
            "Autostart ausschalten",
            "aus",
            "<label><input type=\"checkbox\" name=\"geprueft\" value=\"ja\" required> \
             Ich möchte DepotSchlopf künftig von Hand starten.</label>",
        ),
        autostart::Stand::Aus => ("Aus.", "Autostart einschalten", "an", ""),
        autostart::Stand::Fremd => {
            s.push_str("<p>Ein Autostart-Eintrag ist bereits vorhanden und gehört nicht zu \
                diesem Ordner. Öffne die Einrichtung im bisherigen Hauptordner, wenn du ihn ändern willst. \
                Hast du den Ordner verschoben oder umbenannt, öffne mit Windows-Taste + R und \
                <code>shell:startup</code> deinen Autostart-Ordner. Entferne dort \
                <code>DepotSchlopf.cmd</code>, wenn du den Eintrag dem alten Ordner zuordnen kannst. \
                Lade dann diese Einrichtung neu und schalte den Autostart hier ein.</p></section>");
            return;
        }
        autostart::Stand::NichtVerfuegbar => {
            s.push_str(
                "<p>Hier nicht verfügbar. Der Windows-Autostartordner oder sein Eintrag \
                ist nicht zugänglich, oder der Projektpfad ist ungeeignet. Du kannst \
                DepotSchlopf weiterhin von Hand starten.</p></section>",
            );
            return;
        }
    };
    let _ = write!(
        s,
        "<p>{text}</p><form method=\"post\" action=\"/autostart\">\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <input type=\"hidden\" name=\"an\" value=\"{wert}\">\
         {bestaetigung}<button type=\"submit\">{knopf}</button></form></section>",
        maskiere(token)
    );
}

pub(crate) fn update_aktionen(token: &str, start: &str, z: &Zustand) -> String {
    let mut s = marke(start);
    s.push_str(&update_hinweis(z));
    if let Some((fehler, text)) = &z.programm_meldung {
        let _ = write!(
            s,
            "<p id=\"programmstand\"><span class=\"{}\">Start nach einem Neustart:</span> {}</p>",
            if *fehler { "offen" } else { "gut" },
            maskiere(text)
        );
    }
    let _ = write!(
        s,
        "<p>{}</p>",
        auftrag_knopf(
            token,
            z,
            Art::UpdatePruefen,
            "Nach Update suchen",
            z.update_angebot().is_none()
        )
    );
    if let Some(status) = z.auftrag_stand(Art::Update) {
        let _ = write!(
            s,
            "<p>Update {status} Unter <a href=\"/\">Live</a> siehst du den Fortschritt.</p>"
        );
    } else if z.auftrag_stand(Art::UpdatePruefen).is_some() {
        s.push_str("<p>Hier erscheint das Ergebnis der Suche.</p>");
    } else if let Some((sha, protokoll)) = z.update_angebot() {
        s.push_str("<p>Eine neue Version ist verfügbar.</p><ul>");
        for titel in protokoll.iter().filter_map(|z| z.strip_prefix("  ")) {
            let _ = write!(s, "<li>{}</li>", maskiere(titel));
        }
        s.push_str("</ul>");
        for hinweis in protokoll
            .iter()
            .filter_map(|z| z.strip_prefix("[Update] Cargo.lock: "))
        {
            let _ = write!(
                s,
                "<p>Neue Abhängigkeiten: {}</p><p>Beim Einspielen ist eine Bestätigung am Windows-Desktop nötig.</p>",
                maskiere(hinweis)
            );
        }
        if z.update_wartet_auf_ruhe() {
            s.push_str(
                "<p><button type=\"button\" disabled>Update einspielen</button> \
                 Möglich, sobald kein anderer Auftrag mehr läuft oder wartet.</p>",
            );
        } else {
            let _ = write!(
                s,
                "<p>{}</p>",
                bestaetigungslink(Art::Update, "Update einspielen", Some(sha), true)
            );
        }
    } else if let Some(a) = z
        .letzter_update_lauf()
        .filter(|a| a.art == Art::UpdatePruefen)
    {
        if a.erfolg == Some(true) && a.frueher == Frueher::Nein {
            s.push_str("<p>Die Suche hat keine neue Version gefunden.</p>");
        } else {
            s.push_str("<p>Die Suche konnte nicht abgeschlossen werden. Details stehen unter <a href=\"/\">Live</a>.</p>");
        }
    } else if z
        .letzter_update_lauf()
        .is_some_and(|a| a.art == Art::Update && a.erfolg == Some(false))
    {
        s.push_str("<p>Das Update wurde abgebrochen. Sieh unter <a href=\"/\">Live</a> nach und starte bei Bedarf eine neue Suche.</p>");
    } else {
        s.push_str("<p>Suche zuerst nach einer neuen Version.</p>");
    }
    s
}

fn update_teil(token: &str, start: &str, z: &Zustand, e: &Einrichtung, s: &mut String) {
    let _ = write!(
        s,
        "<section id=\"update\"><h2>Update</h2>\
         <p>Holt eine neue Version. Vor dem Einspielen prüft DepotSchlopf die Version und sichert deine Papierdaten. \
         Das dauert ein paar Minuten.</p><div data-teil=\"/teil/update\">{}</div>\
         <details><summary>Technische Details</summary><p>Der Dienst prüft die Version aus <code>main</code> \
         in einem eigenen Ordner. Deine Sicherung liegt unter <code>daten/sicherung</code>. \
         Falls der Dienst mit der neuen Version nicht startet, kehrt er zum alten Stand zurück.</p>",
        update_aktionen(token, start, z)
    );
    match &e.abgelehnt {
        Ok(l) if l.is_empty() => {}
        Ok(l) => {
            let _ = write!(
                s,
                "<p class=\"leise\">Zurückgesetzt und nicht mehr angeboten: {}</p>",
                maskiere(&l.join(", "))
            );
        }
        Err(f) => {
            let _ = write!(s, "<p class=\"halt\">{}</p>", maskiere(f));
        }
    }
    s.push_str("</details></section>");
}

fn papierfreigabe_teil(token: &str, e: &Einrichtung, s: &mut String) {
    let Some(angebot) = &e.papier_freigabe else {
        return;
    };
    let _ = write!(
        s,
        "<section id=\"papierfreigabe\"><h2>Papierhandel nach Verlustsperre freigeben</h2>\
         <p>Die neue Verlustmessung beginnt nach dem Kontenabgleich bei <strong>{}</strong>. \
         Gesamtverlust seit dem historischen Höchststand: <strong>{}</strong>. \
         Der historische Höchststand bleibt erhalten. Nach dem Absenden musst du die Freigabe \
         innerhalb von zwei Minuten im separaten Windows-Dialog bestätigen.</p>\
         <form method=\"post\" action=\"/papierfreigabe\">\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <input type=\"hidden\" name=\"abdruck\" value=\"{}\">\
         <label><input type=\"checkbox\" name=\"geprueft\" value=\"ja\" required> \
         Ich bestätige die neue Verlustmessung für den Papierhandel.</label>\
         <button type=\"submit\">Papierhandel freigeben</button></form></section>",
        maskiere(&angebot.neuer_start_text()),
        maskiere(&angebot.gesamtverlust_text()),
        maskiere(token),
        maskiere(&angebot.abdruck)
    );
}

/// Das Journal bleibt in seinem eigenen Abschnitt automatisch aktuell.
pub(crate) fn journal_teil(token: &str, start: &str, z: &Zustand) -> String {
    let mut s = marke(start);
    s.push_str(
        "<section id=\"journal\"><h2>Journal</h2><p>Hier hält DepotSchlopf seine Läufe und Papierdaten dauerhaft fest.</p>",
    );
    match z
        .lage
        .as_ref()
        .and_then(|l| l.bild.punkte.iter().find(|p| p.name == "Journal"))
    {
        Some(p) => {
            if let Some(stand) = z.auftrag_stand(Art::Einrichten) {
                let _ = write!(
                    s,
                    "<p>Journal einrichten {stand}. Bitte warte auf den Abschluss.</p>"
                );
            } else {
                let _ = write!(s, "<p>{}</p>", maskiere(&p.text));
            }
            if p.abhilfe == Some(Abhilfe::JournalEinrichten) {
                let knopf = auftrag_knopf(token, z, Art::Einrichten, "Journal einrichten", false);
                s.push_str(&knopf);
            }
        }
        None => s.push_str("<p class=\"leise\">Wird geprüft. Bitte warte einen Moment.</p>"),
    }
    s.push_str("</section>");
    s
}

fn erste_schritte(z: &Zustand, e: &Einrichtung) -> String {
    let schluessel = if e.schluessel.is_empty()
        || e.schluessel
            .iter()
            .any(|(_, st)| *st == SchluesselStand::Unbekannt)
    {
        "unbekannt"
    } else if e
        .schluessel
        .iter()
        .all(|(_, st)| matches!(st, SchluesselStand::Geprueft(_)))
    {
        "alle Schlüssel wurden vom jeweiligen Anbieter angenommen"
    } else if e.schluessel.iter().all(|(_, st)| {
        matches!(
            st,
            SchluesselStand::Geprueft(_) | SchluesselStand::Hinterlegt
        )
    }) {
        "alle Schlüssel sind hinterlegt. Der jeweilige Prüfstand steht bei den Zugangsdaten"
    } else {
        "bitte ergänzen oder prüfen"
    };
    let plan = if z.automatik_fehler().is_some() {
        "angehalten; den Grund findest du im Zeitplan"
    } else if z.zeitplan.an {
        "eingeschaltet"
    } else {
        "aus; erst nach dem manuellen Test empfohlen"
    };
    let autostart = match e.autostart {
        autostart::Stand::An => "eingeschaltet",
        autostart::Stand::Aus => "aus; optional für den Dauerbetrieb",
        _ => "bitte unten prüfen",
    };
    format!(
        "<section id=\"erste-schritte\"><h2>Erste Schritte</h2><ol>\
        <li><a href=\"#journal\">Journal einrichten</a>: Der Stand wird dort automatisch aktualisiert.</li>\
        <li><a href=\"#schluessel\">Zugangsdaten</a>: {schluessel}.</li>\
        <li><a href=\"#quellen\">Datenquellen prüfen</a>: Termine kannst du jederzeit abrufen. \
        Aktuelle Schlusskurse gibt es erst nach dem jeweiligen Börsenschluss. \
        Ein hinterlegter Schlüssel und ein erfolgreicher Kursabruf werden getrennt angezeigt.</li>\
        <li><a href=\"#zeitplan\">Zeitplan</a>: {plan}.</li>\
        <li><a href=\"#autostart\">Autostart</a>: {autostart}.</li></ol>\
        <p><a href=\"#update\">Zum Update</a> · <a href=\"#werkzeuge\">Zu den Werkzeugen</a></p></section>"
    )
}

/// Der Inhalt der Einrichtung.
fn journal_und_zugaenge(token: &str, start: &str, z: &Zustand, e: &Einrichtung, s: &mut String) {
    let _ = write!(
        s,
        "<div data-teil=\"/teil/journal\">{}</div>",
        journal_teil(token, start, z)
    );
    schluessel_teil(token, e, s);
}

pub fn einrichtung(
    token: &str,
    start: &str,
    z: &Zustand,
    heute: Option<Datum>,
    e: &Einrichtung,
    zusatz: &str,
) -> String {
    let mut s = String::from(
        "<section><h2>Einrichtung – Schritt für Schritt</h2><p>Hier legst du fest, wie DepotSchlopf für dich arbeitet. Du kannst die Einrichtung jederzeit später fortsetzen. Änderungen gelten nach dem Speichern; an jedem Formular steht, was als Nächstes geschieht.</p><p><a href=\"#zeitplan\">Automatische Läufe einrichten</a> · <a href=\"#schluessel\">Zugangsdaten prüfen</a></p></section>",
    );
    let _ = write!(
        s,
        "<div id=\"stand\" data-teil=\"/teil/status\">{}</div>",
        status(token, start, z, heute)
    );
    if !z.zeitplan.an {
        s.push_str(&erste_schritte(z, e));
        journal_und_zugaenge(token, start, z, e, &mut s);
    }
    if !matches!(&e.ki_halt, Ok(None)) {
        kostenschutz_teil(token, e, &mut s);
    }
    kurssperre_teil(token, z, e, &mut s);
    papierfreigabe_teil(token, e, &mut s);
    let _ = write!(
        s,
        "<div data-teil=\"/teil/quellen\">{}</div>",
        quellen_teil(token, start, z, e.alpha_kontingent)
    );
    zeitplan_teil(token, z, e, &mut s);
    update_teil(token, start, z, e, &mut s);
    s.push_str("<section id=\"selten\" class=\"einstellungsgruppe\" aria-labelledby=\"selten-titel\"><h2 id=\"selten-titel\">Seltene Einstellungen</h2><p>Für die erste Einrichtung oder wenn sich etwas ändert: Zugangsdaten, Datenspeicherung, Windows-Start und technische Prüfungen.</p>");
    if z.zeitplan.an {
        s.push_str(&erste_schritte(z, e));
        journal_und_zugaenge(token, start, z, e, &mut s);
    }
    autostart_teil(token, e, &mut s);
    s.push_str("<section id=\"werkzeuge\"><h2>Werkzeuge</h2><p>Diese Prüfungen brauchst du nur bei der ersten Einrichtung oder wenn etwas nicht funktioniert.</p>");
    if matches!(&e.ki_halt, Ok(None)) {
        // Freie Schutzfunktionen bleiben unten erreichbar, auch nach einer erfolgreichen
        // Aufhebung. Bei einer aktiven Sperre steht derselbe Abschnitt bereits oben.
        kostenschutz_teil(token, e, &mut s);
    } else if e.ki_halt.is_err() {
        s.push_str(KI_VERBINDUNG);
    }
    if matches!(&e.kurse_halt, Ok(None)) {
        s.push_str("<p id=\"kurssperre\">Kurssperre · <span class=\"gut\">frei</span></p>");
    }
    if e.papier_freigabe.is_none() {
        s.push_str("<p id=\"papierfreigabe\" class=\"leise\">Derzeit wird keine Papierfreigabe angeboten. Den Betriebszustand siehst du oben.</p>");
    }
    let _ = write!(
        s,
        "<div id=\"papierdialog-test\"><h3>Windows-Dialog prüfen</h3>\
         <p>Zeigt den Freigabedialog als Test. Es wird nichts freigegeben oder gespeichert.</p>\
         <form method=\"post\" action=\"/papierdialog-test\">\
         <input type=\"hidden\" name=\"t\" value=\"{}\">\
         <button type=\"submit\">Freigabedialog testen</button></form></div></section>",
        maskiere(token)
    );
    depot_lage::html::grenzen(&mut s, &e.grenzen);
    s.push_str("</section>");
    s.push_str(zusatz);
    let _ = write!(
        s,
        "<section><h2>Dienst</h2><p>Erreichbar nur auf diesem Rechner: \
         http://127.0.0.1:{}/ · Projektordner {}</p></section>",
        crate::http::PORT,
        maskiere(&e.wurzel)
    );
    s
}

/// Das einzige Skript: laedt den Bereich mit `data-teil` alle zwei Sekunden neu, laesst
/// aufgeklappte Protokolle offen und zeigt, wenn der Dienst nicht antwortet. Antwortet ein neu
/// gestarteter Dienst (andere `data-start`), laedt es die ganze Seite neu: Ihre Formulare
/// tragen dann das Token des neuen Starts.
pub const APP_JS: &str = r#""use strict";
(function () {
  // Änderungen bleiben im Formular; insbesondere kein Schlüssel in Browser-Speichern.
  var gehtWeg = false;
  var andererStart = false;
  var bearbeitet = new WeakSet();
  function formulare() { return Array.from(document.querySelectorAll("form[data-aenderungen]")); }
  function istGeaendert(f) {
    return f.dataset.entwurf === "ja" || Array.from(f.querySelectorAll("input:not([type=hidden])")).some(function (i) {
      if (!bearbeitet.has(i) || (i.type === "password" && i.matches(":autofill"))) { return false; }
      return i.type === "checkbox" ? i.checked !== i.defaultChecked : i.value !== i.defaultValue;
    });
  }
  function offen(ausser) { return formulare().some(function (f) { return f !== ausser && istGeaendert(f); }); }
  function zeige(f) {
    var neu = istGeaendert(f);
    f.classList.toggle("ungespeichert", neu);
    var status = f.querySelector("[data-speicherstand]");
    if (status) { status.textContent = neu ? "Noch nicht gespeichert. Bitte unten speichern, damit deine Änderungen gelten." : "Keine ungespeicherten Änderungen in diesem Formular."; }
  }
  ["input", "change"].forEach(function (ereignis) {
    document.addEventListener(ereignis, function (e) {
      var f = e.target.closest("form[data-aenderungen]");
      if (f) { bearbeitet.add(e.target); zeige(f); }
    });
  });
  document.addEventListener("invalid", function (e) {
    var f = e.target.closest("form[data-aenderungen]");
    var status = f && f.querySelector("[data-speicherstand]");
    if (status) { status.textContent = "Noch nicht gespeichert. Bitte prüfe das markierte Feld. Deine übrigen Eingaben bleiben stehen."; }
  }, true);
  window.addEventListener("beforeunload", function (e) {
    if (!gehtWeg && offen()) { e.preventDefault(); e.returnValue = ""; }
  });
  formulare().forEach(zeige);
  // Delegiert, da Live auch laufende Protokolle regelmäßig ersetzt.
  document.addEventListener("click", async function (e) {
    var b = e.target.closest("[data-protokoll-kopieren]");
    if (!b) { return; }
    var block = b.closest("details");
    var log = block.querySelector("pre");
    var status = block.querySelector("[role=status]");
    try {
      await navigator.clipboard.writeText(log.textContent);
      status.textContent = "Kopiert.";
    } catch (_) {
      var auswahl = window.getSelection();
      if (auswahl && log.isConnected) {
        var bereich = document.createRange();
        bereich.selectNodeContents(log);
        auswahl.removeAllRanges();
        auswahl.addRange(bereich);
        status.textContent = "Text markiert. Drücke Strg+C zum Kopieren.";
      } else {
        status.textContent = "Bitte markiere und kopiere den Text von Hand.";
      }
    }
  });
  // Doppelklicks: Ein abgesendetes Formular sperrt seinen Knopf. Der Dienst lehnt doppelte
  // Aufträge ohnehin ab; so sieht man sofort, dass der Klick angekommen ist.
  document.addEventListener("submit", function (e) {
    if (andererStart) { e.preventDefault(); return; }
    if (offen(e.target) && !window.confirm("In einem anderen Formular stehen noch ungespeicherte Änderungen. Sie werden bei dieser Aktion verworfen. Trotzdem fortfahren?")) {
      e.preventDefault(); return;
    }
    // Ein Download lässt die Seite offen; der Exportknopf bleibt benutzbar.
    if (e.target.dataset.download !== undefined) { return; }
    var b = e.submitter || e.target.querySelector("button[type=submit]");
    if (b) {
      if (b.disabled) { e.preventDefault(); return; }
      if (b.dataset.gesendet) { e.preventDefault(); return; }
      gehtWeg = true;
      b.dataset.gesendet = b.textContent;
      setTimeout(function () { b.disabled = true; b.textContent = "wird gesendet …"; }, 0);
    }
  }, true);
  // Eine Verbindungsprüfung gilt für jede Seite, auch Bestätigung und Protokoll.
  var banner = document.getElementById("getrennt");
  var marke = document.querySelector("[data-start]");
  var start = marke ? marke.getAttribute("data-start") : null;
  var neuladen = false;
  var pausiert = false;
  var generation = 0;
  var timer = new Set();
  var anfragen = new Set();
  function planeAbruf(f, ms) {
    if (pausiert || neuladen) { return; }
    var id = setTimeout(function () { timer.delete(id); f(); }, ms);
    timer.add(id);
  }
  async function lade(url) {
    var steuerung = new AbortController();
    anfragen.add(steuerung);
    var frist = setTimeout(function () { steuerung.abort(); }, 5000);
    try {
      var r = await fetch(url, { cache: "no-store", credentials: "same-origin", signal: steuerung.signal });
      if (!r.ok) { throw new Error(String(r.status)); }
      return await r.text();
    } finally {
      clearTimeout(frist);
      anfragen.delete(steuerung);
    }
  }
  function kennung(t) {
    var m = /data-start="([^"]*)"/.exec(t);
    return m ? m[1] : null;
  }
  function wiederverbindungsziel() {
    // Die Position und fachliche Parameter bleiben erhalten, die alte Rückmeldung entfällt.
    var ziel = new URL(window.location.href);
    ziel.searchParams.delete("m");
    return ziel.pathname + ziel.search + ziel.hash;
  }
  async function verbindung() {
    var g = generation;
    var fehler = false;
    try {
      var t = await lade("/teil/status");
      if (g !== generation || pausiert) { return; }
      var neu = kennung(t);
      if (!neu) { throw new Error("Startkennung fehlt"); }
      // Ältere Rückfallstände kennen nur data-start; sie bleiben erreichbar.
      var bereit = t.indexOf('data-bereit="nein"') === -1;
      if (!bereit) {
        banner.textContent = "DepotSchlopf startet und prüft die neue Version. Diese Seite wartet auf die Bestätigung.";
        banner.hidden = false;
      } else if (start && neu !== start) {
        if (offen()) {
          andererStart = true;
          banner.textContent = "DepotSchlopf wurde neu gestartet. Deine ungespeicherten Eingaben stehen noch hier. Prüfe sie vor dem Neuladen. Zum Speichern braucht diese Seite die neue Verbindung.";
          banner.hidden = false;
          document.getElementById("neuverbinden-link").href = wiederverbindungsziel();
          document.getElementById("neuverbinden").hidden = false;
        } else if (!neuladen) {
          neuladen = true;
          window.location.replace(wiederverbindungsziel());
        }
      } else {
        start = neu;
        banner.hidden = true;
      }
    } catch (_) {
      if (g !== generation || pausiert) { return; }
      fehler = true;
      banner.textContent = "Verbindung zu DepotSchlopf unterbrochen. Die Seite versucht es automatisch erneut. Lass das DepotSchlopf-Fenster offen.";
      banner.hidden = false;
    } finally {
      if (g === generation) { planeAbruf(verbindung, fehler ? 5000 : 2000); }
    }
  }
  var teile = Array.from(document.querySelectorAll("[data-teil]")).map(function (ziel) {
    var url = ziel.getAttribute("data-teil");
    var zuletzt = null;
    return async function hole() {
      var g = generation;
      var fehler = false;
      try {
        var t = await lade(url);
        if (g !== generation || pausiert || neuladen) { return; }
        var neu = kennung(t);
        // Der zentrale Verbindungstest wartet auf die Bestätigung und lädt genau einmal neu.
        if (start && neu && neu !== start) { return; }
        if (t !== zuletzt) {
          var offen = {};
          ziel.querySelectorAll("details[id]").forEach(function (d) { offen[d.id] = d.open; });
          ziel.innerHTML = t;
          zuletzt = t;
          Object.keys(offen).forEach(function (id) {
            var d = document.getElementById(id);
            if (d) { d.open = offen[id]; }
          });
        }
      } catch (_) { fehler = true; }
      finally { if (g === generation) { planeAbruf(hole, fehler ? 5000 : 2000); } }
    };
  });
  function starte() {
    planeAbruf(verbindung, 0);
    teile.forEach(function (f) { planeAbruf(f, 2000); });
  }
  window.addEventListener("pagehide", function () {
    pausiert = true;
    generation += 1;
    timer.forEach(function (id) { clearTimeout(id); });
    timer.clear();
    anfragen.forEach(function (a) { a.abort(); });
  });
  window.addEventListener("pageshow", function (e) {
    gehtWeg = false;
    formulare().forEach(zeige);
    document.querySelectorAll("button[data-gesendet]").forEach(function (b) {
      b.textContent = b.dataset.gesendet;
      delete b.dataset.gesendet;
      b.disabled = false;
    });
    if (e.persisted) { pausiert = false; generation += 1; starte(); }
  });
  starte();
})();
"#;
