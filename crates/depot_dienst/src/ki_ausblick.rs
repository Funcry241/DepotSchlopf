//! Lesende Vorschau der nächsten KI-Aufgaben auf Live.

use std::fmt::Write as _;

use depot_journal::ki_einstellungen;
use depot_kern::modelle::{BeraterEinstellung, KiEinstellungen, Modellwahl};
use depot_kern::{BeraterId, Datum, Zeitpunkt};
use depot_lage::html::maskiere;
use depot_lage::{BudgetStatus, SchluesselStand, datum_text, ortszeit};

use crate::neuroauftraege::Fragenstand;
use crate::schluessel::Wer;
use crate::werkbank::{
    Art, Dienst, Zustand, gemini_vorlauf_offen, papier_kurse_moeglich, vorbereitung_offen,
};

pub(crate) struct Ausblick {
    pub(crate) einstellungen: Option<KiEinstellungen>,
    gemini_aktiv: bool,
    gemini_aufrufe: Option<usize>,
    gemini_verlauf: GeminiVerlauf,
    gemini_wiederholung: Option<Zeitpunkt>,
    neuro_aktiv: bool,
    neuro_aktivitaet: String,
}

#[derive(Default)]
pub(crate) struct GeminiVerlauf {
    letzter_versuch: Option<(Zeitpunkt, bool)>,
    letzter_erfolg: Option<Zeitpunkt>,
}

impl GeminiVerlauf {
    pub(crate) fn beobachte(&mut self, zeit: Zeitpunkt, erfolg: bool) {
        if self
            .letzter_versuch
            .is_none_or(|(bisher, _)| zeit >= bisher)
        {
            self.letzter_versuch = Some((zeit, erfolg));
        }
        if erfolg && self.letzter_erfolg.is_none_or(|bisher| zeit >= bisher) {
            self.letzter_erfolg = Some(zeit);
        }
    }

    pub(crate) fn hat_erfolg(&self) -> bool {
        self.letzter_erfolg.is_some()
    }

    fn ergaenze(&self, text: &mut String) {
        if let Some((zeit, false)) = self.letzter_versuch {
            let _ = write!(
                text,
                " Letzter Versuch fehlgeschlagen ({}).",
                depot_lage::zeit_text(zeit)
            );
        }
        if let Some(zeit) = self.letzter_erfolg {
            let _ = write!(
                text,
                " Letzte verwertbare Zuarbeit: {}.",
                depot_lage::zeit_text(zeit)
            );
        } else if self.letzter_versuch.is_some() {
            text.push_str(" Keine verwertbare Zuarbeit im geprüften Verlauf.");
        }
    }
}

impl Ausblick {
    pub(crate) fn lesen(d: &Dienst, tag: Option<Datum>, gemini_aktiv: bool) -> Self {
        let daten = d.wurzel.join("daten");
        let neuro = crate::neuroschlopf::einstellung(&daten);
        Self {
            einstellungen: ki_einstellungen::lesen(&daten).ok(),
            gemini_aktiv,
            gemini_aufrufe: tag.map(|tag| depot_lage::gespraeche::gemini_aufrufe(&daten, tag)),
            gemini_verlauf: if gemini_aktiv {
                d.gespraechscache.gemini_verlauf(&daten, tag)
            } else {
                GeminiVerlauf::default()
            },
            gemini_wiederholung: tag.and_then(|tag| {
                depot_lage::gespraeche::gemini_wiederholung(&daten, tag).map(|w| w.faellig)
            }),
            neuro_aktiv: matches!(neuro, Ok(true)),
            neuro_aktivitaet: crate::neuroschlopf::live_aktivitaet(d, &neuro),
        }
    }

    pub(crate) fn html(&self, z: &Zustand, jetzt: Zeitpunkt, update_offen: bool) -> String {
        let tag = jetzt.frankfurt_tag();
        let e = self.einstellungen;
        let wartet_auf_update =
            update_offen || z.neustart || z.auftrag_stand(Art::Update).is_some();
        let aktiv = z.neuro_wartet();
        let fragenstand = if self.neuro_aktiv {
            e.and_then(|e| z.neuro_fragenstand?.stand(tag, e))
        } else {
            None
        };
        let vorgemerkt = match fragenstand {
            Some(Fragenstand::Vorgemerkt(wer)) => Some(wer),
            _ => None,
        };
        let mut s = String::from(
            "<section id=\"ki-ausblick\"><h2>Was die KIs als Nächstes tun</h2><table><tbody>",
        );
        let plan = berater_zeitplan(z, jetzt);
        for (wer, profil) in [
            (BeraterId::Dachs, e.map(|e| e.dachs)),
            (BeraterId::Maus, e.map(|e| e.maus)),
        ] {
            let name = beratername(wer);
            let stand = beraterstand(z, tag, wer, profil, vorgemerkt, wartet_auf_update, &plan);
            let modell = profil.map(|p| modell_text(p.auswahl));
            let _ = write!(
                s,
                "<tr><th scope=\"row\">{name}</th><td>{}",
                maskiere(&stand)
            );
            if profil.is_some_and(|p| p.max_aufrufe > 0)
                && let Some(budget) = beraterbudget(z, tag, wer)
            {
                let _ = write!(s, " <span>{}</span>", maskiere(&budget));
            }
            if let Some(modell) = modell {
                let _ = write!(
                    s,
                    " <small class=\"leise\">Nächster Aufruf · {modell}</small>"
                );
            }
            s.push_str("</td></tr>");
        }
        let mut gemini = self.gemini_stand(z, jetzt, wartet_auf_update);
        if self.gemini_aktiv {
            self.gemini_verlauf.ergaenze(&mut gemini);
        }
        let _ = write!(
            s,
            "<tr><th scope=\"row\">Gemini</th><td>{}</td></tr><tr><th scope=\"row\">NeuroSchlopf</th><td>Belegvergleich · {}",
            maskiere(&gemini),
            self.neuro_aktivitaet,
        );
        if let Some(frage) = self.externer_stand(aktiv, fragenstand, wartet_auf_update) {
            let _ = write!(s, " Externe Fragen · {}", maskiere(&frage));
        }
        s.push_str("</td></tr></tbody></table></section>");
        s
    }

    fn gemini_stand(&self, z: &Zustand, jetzt: Zeitpunkt, update: bool) -> String {
        if z.aktive_auftraege().any(|a| a.art == Art::Gemini) {
            return "Sortiert Schlagzeilen vor.".into();
        }
        if !self.gemini_aktiv {
            return "Ausgeschaltet · Gratis-Projekt aus.".into();
        }
        let Some(e) = self.einstellungen else {
            return "Pausiert · Tageslimit unlesbar.".into();
        };
        if e.gemini == 0 {
            return "Pausiert · Tageslimit 0.".into();
        }
        if self.gemini_aufrufe.is_some_and(|n| n >= e.gemini) {
            return "Pausiert · Kontingent für heute erreicht.".into();
        }
        if update {
            return "Wartet auf das Update.".into();
        }
        if z.warteschlange.iter().any(|a| a.art == Art::Gemini) {
            return "Schlagzeilen vorsortieren · wartet auf den Start.".into();
        }
        if z.zeitplan.an
            && let Some(faellig) = self.gemini_wiederholung
            && !z
                .verlauf
                .iter()
                .any(|a| a.art == Art::Gemini && a.start >= faellig)
        {
            return format!("Folgeversuch ab {}.", depot_lage::zeit_text(faellig));
        }
        if let Some(grund) = schluesselgrund(z, jetzt.frankfurt_tag(), Wer::Gemini) {
            return grund.into();
        }
        if let Some(grund) = z.automatik_fehler().or(z.verlauf_vermerkfehler.as_deref()) {
            return format!("Automatische Zuarbeit pausiert · {grund}");
        }
        if let Some((tag, h, m, ..)) = ortszeit(jetzt)
            && gemini_vorlauf_offen(z, tag, (h, m).max(z.zeitplan.termine))
        {
            return "Sortiert Schlagzeilen im Vorlauf, sobald die Termine vorliegen und das Zeitfenster vor dem Kursabruf frei ist.".into();
        }
        if z.automatik_status() == "an" && z.zeitplan.nachrichten_an {
            "Sortiert Schlagzeilen nach dem nächsten erfolgreichen Abruf vor.".into()
        } else {
            "Wartet auf einen Schlagzeilenabruf von Hand.".into()
        }
    }

    fn externer_stand(
        &self,
        aktiv: Option<BeraterId>,
        stand: Option<Fragenstand>,
        update: bool,
    ) -> Option<String> {
        if let Some(wer) = aktiv {
            return Some(format!("Wartet auf die Antwort von {}.", beratername(wer)));
        }
        if !self.neuro_aktiv {
            return None;
        }
        let Some(e) = self.einstellungen else {
            return Some("Pausiert · KI-Einstellungen unlesbar.".into());
        };
        Some(if e.neuroschlopf == 0 {
            "Pausiert · Tageslimit 0.".into()
        } else if e.neuro_berater().is_empty() {
            "Ausgeschaltet.".into()
        } else if update {
            "Warten auf das Update.".into()
        } else {
            match stand {
                Some(Fragenstand::Tageslimit) => "Pausiert · Tageslimit erreicht.".into(),
                Some(Fragenstand::Journal) => "Warten auf einen bestätigten Journalstand.".into(),
                Some(Fragenstand::Vorgemerkt(wer)) => {
                    format!("Frage an {} wartet auf den Start.", beratername(wer))
                }
                Some(Fragenstand::Wartet) | None => "Warten auf einen geeigneten Beleg.".into(),
            }
        })
    }
}

pub(crate) fn modell_text(wahl: Modellwahl) -> String {
    format!(
        "{} · Denktiefe {}",
        wahl.modell().name(),
        wahl.denktiefe().name()
    )
}

fn beratername(wer: BeraterId) -> &'static str {
    match wer {
        BeraterId::Dachs => "Dachs",
        BeraterId::Maus => "Maus",
    }
}

fn beraterstand(
    z: &Zustand,
    tag: Option<Datum>,
    wer: BeraterId,
    profil: Option<BeraterEinstellung>,
    neuro_vorgemerkt: Option<BeraterId>,
    update: bool,
    plan: &str,
) -> String {
    let naechster = z.warteschlange.iter().find(|a| beraterauftrag(a.art));
    let mit_folge = |mut stand: String| {
        if let Some(a) = naechster {
            let _ = write!(stand, " Danach vorgemerkt · {}.", a.art.name());
        }
        stand
    };
    if z.neuro_wartet() == Some(wer) {
        return mit_folge("Beantwortet eine Frage von NeuroSchlopf.".into());
    }
    if z.berater_warten.contains(&wer) {
        return mit_folge(z.laufend.as_ref().map_or_else(
            || "Aufruf läuft.".into(),
            |a| format!("Aufruf läuft · {}.", a.art.name()),
        ));
    }
    let Some(profil) = profil else {
        return "Pausiert · KI-Einstellungen unlesbar.".into();
    };
    if profil.max_aufrufe == 0 {
        return "Pausiert · Tageslimit 0.".into();
    }
    if let Some(grund) = schluesselgrund(z, tag, Wer::Berater(wer)) {
        return grund.into();
    }
    if update {
        return "Wartet auf das Update.".into();
    }
    if let Some(a) = z.laufend.as_ref().filter(|a| beraterauftrag(a.art)) {
        return mit_folge(format!("{} läuft.", a.art.name()));
    }
    if let Some(a) = naechster {
        return format!("{} · wartet auf den Start.", a.art.name());
    }
    if neuro_vorgemerkt == Some(wer) {
        return "Antwort an NeuroSchlopf vorgemerkt.".into();
    }
    plan.into()
}

fn beraterbudget(z: &Zustand, tag: Option<Datum>, wer: BeraterId) -> Option<String> {
    let aktuell =
        lage(z, tag).and_then(|lage| depot_lage::betrieb::budget_status(&lage.budget, wer));
    match aktuell {
        Some(BudgetStatus::Bereit) => None,
        Some(stand) => Some(format!("Reguläres KI-Budget {}.", stand.text())),
        None => Some(
            z.letztes_ki_budget
                .get(&wer)
                .filter(|(datum, _)| Some(*datum) == tag)
                .map_or_else(
                    || "Reguläres KI-Budget noch nicht bestätigt.".into(),
                    |(_, stand)| format!("Reguläres KI-Budget {} (zuletzt geprüft).", stand.text()),
                ),
        ),
    }
}

fn lage(z: &Zustand, tag: Option<Datum>) -> Option<&depot_lage::Lagebild> {
    z.lage
        .as_ref()
        .filter(|l| !z.lage_veraltet && tag.is_some() && l.stand.frankfurt_tag() == tag)
        .map(|l| &l.bild)
}

fn schluesselgrund(z: &Zustand, tag: Option<Datum>, wer: Wer) -> Option<&'static str> {
    let stand = lage(z, tag)?
        .schluessel
        .iter()
        .find_map(|(w, stand)| (*w == wer.protokoll()).then_some(*stand))?;
    match stand {
        SchluesselStand::Fehlt => Some("Pausiert · Zugangsschlüssel fehlt."),
        SchluesselStand::Abgelehnt(_) => {
            Some("Pausiert · Anbieter hat den Zugangsschlüssel abgelehnt.")
        }
        _ => None,
    }
}

fn beraterauftrag(art: Art) -> bool {
    matches!(
        art,
        Art::Papier | Art::Vorbereiten | Art::Simulation | Art::Wochenrueckschau
    )
}

fn berater_zeitplan(z: &Zustand, jetzt: Zeitpunkt) -> String {
    if let Some(fehler) = z.automatik_fehler().or(z.verlauf_vermerkfehler.as_deref()) {
        return format!("Zeitplan pausiert · {fehler}");
    }
    if !z.zeitplan.an {
        return "Automatik ausgeschaltet · wartet auf einen Auftrag.".into();
    }
    let Some((heute, h, m, ..)) = ortszeit(jetzt) else {
        return "Zeitplan wartet · Ortszeit nicht eindeutig.".into();
    };
    for tag in (0..8).filter_map(|d| heute.plus_tage(d)) {
        if z.zeitplan.papier_danach
            && z.erledigt.papier != Some(tag)
            && !(tag == heute
                && crate::zeitplan::morgen_abgelaufen(&z.zeitplan, tag, (h, m), &z.boersen))
            && papier_kurse_moeglich(z, tag)
            && z.lage
                .as_ref()
                .and_then(|l| l.bild.depot.as_ref())
                .and_then(|d| d.letzter_abschluss)
                .is_none_or(|d| d < tag)
        {
            return format!(
                "Papiertag am {} · nach dem letzten Kursabruf mit gültigen Kursen für alle gehaltenen Werte.",
                datum_text(tag),
            );
        }
        if (tag != heute || h < 23) && vorbereitung_offen(z, tag) {
            return format!(
                "Handelstag vorbereiten am {} · nach erfolgreichem Schlagzeilenabruf.",
                datum_text(tag),
            );
        }
    }
    "Wartet auf einen Auftrag.".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemini_anzeige_bewahrt_erfolg_und_neueren_fehler() -> Result<(), String> {
        let erfolg = Zeitpunkt::lies("2026-11-25T08:00:00Z").ok_or("Erfolg")?;
        let fehler = Zeitpunkt::lies("2026-11-26T08:00:00Z").ok_or("Fehler")?;
        let mut verlauf = GeminiVerlauf::default();
        verlauf.beobachte(fehler, false);
        let mut text = String::new();
        verlauf.ergaenze(&mut text);
        assert!(text.contains("Keine verwertbare Zuarbeit"));
        verlauf.beobachte(erfolg, true);
        text.clear();
        verlauf.ergaenze(&mut text);
        assert!(text.contains(&depot_lage::zeit_text(erfolg)));
        assert!(text.contains(&depot_lage::zeit_text(fehler)));
        assert!(!text.contains("Keine verwertbare Zuarbeit"));
        Ok(())
    }
}
