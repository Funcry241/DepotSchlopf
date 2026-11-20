//! Die Webseite bleibt bedienbar, während Windows auf die Entscheidung wartet.

use std::fmt::Write as _;
use std::sync::Arc;

use depot_kern::Zeitpunkt;

use crate::http::{self, feld};
use crate::seiten;
use crate::werkbank::{self, Dienst};
use crate::{DialogErgebnis, Sperre, Umwelt, kostenschutz};

pub(crate) struct Stand {
    kennung: String,
    anker: &'static str,
    antwort: Option<Antwort>,
}

enum Antwort {
    Meldung(String),
    Fehler(u16, String),
}

impl Antwort {
    fn meldung(code: &str) -> Self {
        Self::Meldung(code.into())
    }

    fn direkt(&self, anker: &str) -> Vec<u8> {
        match self {
            Self::Meldung(code) if code == "sicherung_dialog_abgebrochen" => {
                http::umleitung("/einrichtung#sicherung")
            }
            Self::Meldung(code) => http::umleitung(&format!("/einrichtung?m={code}#{anker}")),
            Self::Fehler(code, text) => seiten::fehler(*code, text),
        }
    }
}

enum Auftrag {
    Papier(String),
    Sperre(Sperre, String),
    Ordner(String),
    Test,
}

impl Auftrag {
    fn anker(&self) -> &'static str {
        match self {
            Self::Papier(_) => "papierfreigabe",
            Self::Sperre(sperre, _) => sperre.anker(),
            Self::Ordner(_) => "sicherung",
            Self::Test => "papierdialog-test",
        }
    }

    fn ausfuehren(self, d: &Dienst, u: &dyn Umwelt, zeit: Zeitpunkt) -> Antwort {
        let Ok(_dialog) = d.papierdialog.try_lock() else {
            let code = match self {
                Self::Papier(_) => "papierfreigabe_belegt",
                Self::Sperre(Sperre::Kostenschutz, _) => "kostenschutz_dialog_belegt",
                Self::Sperre(Sperre::Kurse, _) => "kurssperre_dialog_belegt",
                Self::Ordner(_) | Self::Test => "dialogtest_belegt",
            };
            return Antwort::meldung(code);
        };
        match self {
            Self::Papier(gesehen) => papier(d, u, &gesehen),
            Self::Sperre(sperre, gesehen) => sperre_aufheben(d, u, sperre, &gesehen),
            Self::Ordner(bisher) => match u.sicherungsziel_waehlen() {
                Ok(Some(ziel)) if !ziel.trim().is_empty() => {
                    match d.sicherung_einstellen(&ziel, false, zeit, Some(&bisher)) {
                        Ok(()) => Antwort::meldung("sicherung_ziel"),
                        Err((code, text)) => Antwort::Fehler(code, text),
                    }
                }
                Ok(_) => Antwort::meldung("sicherung_dialog_abgebrochen"),
                Err(text) => Antwort::Fehler(503, text),
            },
            Self::Test => Antwort::meldung(match u.papierdialog_testen() {
                DialogErgebnis::Ja => "dialogtest_ja",
                DialogErgebnis::Nein => "dialogtest_nein",
                DialogErgebnis::NichtVerfuegbar => "dialogtest_fehler",
            }),
        }
    }
}

pub(crate) type Umgebung<'a> = (&'a Arc<Dienst>, &'a Arc<dyn Umwelt>);

pub(crate) fn post(
    d: &Dienst,
    u: &dyn Umwelt,
    hintergrund: Option<Umgebung<'_>>,
    pfad: &str,
    f: &[(String, String)],
    zeit: Zeitpunkt,
) -> Option<Vec<u8>> {
    let auftrag = match pfad {
        "/sicherung-ziel" if feld(f, "aktion") == Some("waehlen") => {
            match crate::sicherung::ziel_lesen(&d.wurzel) {
                Ok(ziel) => Auftrag::Ordner(ziel),
                Err(_) => {
                    return Some(seiten::fehler(
                        409,
                        "Das bisherige Sicherungsziel ist nicht lesbar.",
                    ));
                }
            }
        }
        "/papierdialog-test" => Auftrag::Test,
        "/papierfreigabe" => {
            if feld(f, "geprueft") != Some("ja") {
                return Some(
                    Antwort::meldung("papierfreigabe_unbestaetigt").direkt("papierfreigabe"),
                );
            }
            let Some(abdruck) = feld(f, "abdruck")
                .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            else {
                return Some(seiten::fehler(
                    400,
                    "Die Freigabe ist veraltet. Öffne die Einrichtung und prüfe den aktuellen Stand.",
                ));
            };
            Auftrag::Papier(abdruck.into())
        }
        "/kostenschutz" | "/kurssperre" => {
            let sperre = if pfad == "/kostenschutz" {
                Sperre::Kostenschutz
            } else {
                Sperre::Kurse
            };
            if feld(f, "geprueft") != Some("ja") {
                return Some(sperre.meldung("unbestaetigt").direkt(sperre.anker()));
            }
            let Some(abdruck) = feld(f, "abdruck") else {
                return Some(seiten::fehler(
                    400,
                    "Die Sperre hat sich möglicherweise geändert. Öffne die Einrichtung erneut.",
                ));
            };
            Auftrag::Sperre(sperre, abdruck.into())
        }
        _ => return None,
    };
    let anker = auftrag.anker();
    Some(if let Some((dienst, umwelt)) = hintergrund {
        starten(dienst, umwelt, auftrag, zeit, anker)
    } else {
        auftrag.ausfuehren(d, u, zeit).direkt(anker)
    })
}

fn starten(
    d: &Arc<Dienst>,
    u: &Arc<dyn Umwelt>,
    auftrag: Auftrag,
    zeit: Zeitpunkt,
    anker: &'static str,
) -> Vec<u8> {
    let mut stand = d
        .desktopdialog
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(aktiv) = stand.as_ref().filter(|stand| stand.antwort.is_none()) {
        return http::umleitung(&format!("/desktop-dialog?id={}", aktiv.kennung));
    }
    let kennung = crate::neues_token();
    let (dienst, umwelt, dialog) = (Arc::clone(d), Arc::clone(u), kennung.clone());
    *stand = Some(Stand {
        kennung: kennung.clone(),
        anker,
        antwort: None,
    });
    let faden = std::thread::Builder::new()
        .name("Desktop-Dialog".into())
        .spawn(move || {
            let antwort = auftrag.ausfuehren(&dienst, umwelt.as_ref(), zeit);
            let mut stand = dienst
                .desktopdialog
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(aktiv) = stand.as_mut().filter(|stand| stand.kennung == dialog) {
                aktiv.antwort = Some(antwort);
            }
        });
    if faden.is_err() {
        *stand = None;
        return seiten::fehler(503, "Der Windows-Dialog konnte nicht gestartet werden.");
    }
    http::umleitung(&format!("/desktop-dialog?id={kennung}"))
}

pub(crate) fn inhalt(d: &Dienst, id: &str) -> String {
    let stand = d
        .desktopdialog
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut html = seiten::marke(&d.start);
    let Some(aktiv) = stand.as_ref().filter(|stand| stand.kennung == id) else {
        html.push_str("<p>Dieses Dialogergebnis ist nicht mehr verfügbar.</p><a href=\"/einrichtung\">Zur Einrichtung</a>");
        return html;
    };
    let (klasse, text) = match &aktiv.antwort {
        None => (
            "leise",
            "Die Antwort steht aus. Schau auf den Windows-Desktop. Du kannst dir Zeit lassen.",
        ),
        Some(Antwort::Meldung(code)) => {
            seiten::meldung(&format!("m={code}")).unwrap_or(("leise", "Der Dialog ist beendet."))
        }
        Some(Antwort::Fehler(_, text)) => ("offen", text.as_str()),
    };
    let _ = write!(
        html,
        "<p class=\"{klasse}\" role=\"status\">{}</p><a href=\"/einrichtung#{}\">Zurück zur Einrichtung</a>",
        depot_lage::html::maskiere(text),
        aktiv.anker
    );
    html
}

fn papier(d: &Dienst, u: &dyn Umwelt, gesehen: &str) -> Antwort {
    let Some(angebot) =
        depot_lage::papier_freigabe_angebot(&d.wurzel).filter(|angebot| angebot.abdruck == gesehen)
    else {
        return Antwort::meldung("papierfreigabe_geaendert");
    };
    let Some(bestaetigt_am) = u.papierfreigabe_bestaetigen(&angebot) else {
        return Antwort::meldung("papierfreigabe_unbestaetigt");
    };
    Antwort::meldung(d.papier_freigeben(gesehen, bestaetigt_am))
}

impl Sperre {
    fn anker(self) -> &'static str {
        match self {
            Self::Kostenschutz => "kostenschutz",
            Self::Kurse => "kurssperre",
        }
    }

    fn meldung(self, ausgang: &str) -> Antwort {
        Antwort::Meldung(format!("{}_{ausgang}", self.anker()))
    }
}

fn sperre_aufheben(d: &Dienst, u: &dyn Umwelt, sperre: Sperre, gesehen: &str) -> Antwort {
    {
        let z = d.sperre();
        if z.laufend.is_some() || !z.warteschlange.is_empty() || d.update_offen() {
            return sperre.meldung("belegt");
        }
        let pfad = match sperre {
            Sperre::Kostenschutz => kostenschutz::pfad(&d.wurzel),
            Sperre::Kurse => kostenschutz::kurse_pfad(&d.wurzel),
        };
        match werkbank::lies_falls_da(&pfad) {
            Ok(Some(inhalt)) if kostenschutz::abdruck(&inhalt) == gesehen => {}
            Ok(Some(_)) => return sperre.meldung("geaendert"),
            Ok(None) => return sperre.meldung("frei"),
            Err(_) => return sperre.meldung("fehler"),
        }
    }
    let Some(jetzt) = u.sperre_bestaetigen(sperre) else {
        return sperre.meldung("dialog_unbestaetigt");
    };
    // Unter der Sperre des Zustands startet kein Auftrag.
    let mut z = d.sperre();
    if z.laufend.is_some() || !z.warteschlange.is_empty() || d.update_offen() {
        return sperre.meldung("belegt");
    }
    let ausgang = match sperre {
        Sperre::Kostenschutz => kostenschutz::aufheben(&d.wurzel, gesehen, jetzt),
        Sperre::Kurse => kostenschutz::kurse_aufheben(&d.wurzel, gesehen, jetzt),
    };
    sperre.meldung(match ausgang {
        Ok(kostenschutz::Aufheben::Aufgehoben) => {
            z.lage_veraltet = true;
            "aufgehoben"
        }
        Ok(kostenschutz::Aufheben::Frei) => "frei",
        Ok(kostenschutz::Aufheben::Geaendert) => "geaendert",
        Err(_) => "fehler",
    })
}
