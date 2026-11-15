//! Diagnose eines begonnenen Papiertags aus dem belegten Journal und dem Papierkonto.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

use depot_journal::{Abweichung, Art, AuftragsZustand, Konto as _, KontoStatus, Stand};
use depot_kern::{Cent, Datum};

use crate::html::maskiere;

const ZEILEN_MAX: usize = 20;

#[derive(Clone, Debug)]
pub struct Diagnose {
    letzter_abschluss: Option<Datum>,
    abrechnungstag: Option<Datum>,
    eintraege: usize,
    ki_aufrufe: usize,
    auftraege: Vec<Auftrag>,
    konto: Result<Vec<Abweichung>, String>,
}

#[derive(Clone, Debug)]
struct Auftrag {
    nr: u64,
    wert: String,
    journal: Option<(AuftragsZustand, u64)>,
    konto: Option<(KontoStatus, u64)>,
    gleich: bool,
}

impl Diagnose {
    pub fn lies(daten: &Path, stand: &Stand, eintraege: &[(Art, String)]) -> Option<Self> {
        if stand.seit_abschluss() == 0 {
            return None;
        }
        let nach_abschluss = eintraege
            .rsplit(|(art, _)| matches!(art, Art::Abschluss | Art::NotbremseFreigabe))
            .next()
            .unwrap_or_default();
        let mut nummern: BTreeSet<u64> = nach_abschluss
            .iter()
            .filter(|(art, _)| matches!(art, Art::Auftrag | Art::Zustand | Art::Stopp))
            .filter_map(|(art, text)| {
                let meldung = if *art == Art::Stopp {
                    text.splitn(4, ';').nth(3)?
                } else {
                    text
                };
                meldung.split_once(';')?.0.parse().ok()
            })
            .chain(stand.offene_auftraege().map(|a| a.beleg.nr))
            .collect();
        let papier = crate::zugang::papierkonto_lesen(&daten.join(crate::PAPIERKONTO));
        let konto = papier
            .as_ref()
            .map(|papier| {
                let auszug = papier.auszug();
                nummern.extend(auszug.offene.iter().copied());
                depot_journal::konto::abweichungen(stand, &auszug)
            })
            .map_err(Clone::clone);
        let auftraege = nummern
            .into_iter()
            .rev()
            .map(|nr| {
                let journal = stand.auftrag(nr);
                let konto = papier.as_ref().ok().and_then(|p| p.order(nr));
                let gleich = match (journal, konto.as_ref()) {
                    (Some(j), Some(k)) => {
                        j.ausfuehrungen == k.ausfuehrungen
                            && kontostatus(j.zustand) == Some(k.status)
                    }
                    (Some(j), None) => !j.zustand.beim_broker() && kontostatus(j.zustand).is_none(),
                    (None, _) => false,
                };
                Auftrag {
                    nr,
                    wert: journal.map_or_else(|| "–".into(), |j| j.beleg.isin.to_string()),
                    journal: journal.map(|j| (j.zustand, j.gefuellt())),
                    konto: konto.map(|k| {
                        let stueck = k
                            .ausfuehrungen
                            .iter()
                            .fold(0_u64, |s, a| s.saturating_add(a.stueck));
                        (k.status, stueck)
                    }),
                    gleich,
                }
            })
            .collect();
        Some(Self {
            letzter_abschluss: stand.abschluss().map(|a| a.datum),
            abrechnungstag: stand
                .abrechnungstag()
                .filter(|tag| stand.abschluss().is_none_or(|a| *tag > a.datum)),
            eintraege: stand.seit_abschluss(),
            ki_aufrufe: nach_abschluss
                .iter()
                .filter(|(art, _)| *art == Art::KiAufruf)
                .count(),
            auftraege,
            konto,
        })
    }

    pub fn html(&self, text: &mut String) {
        let letzter = datum(self.letzter_abschluss);
        let tag = datum(self.abrechnungstag);
        let _ = write!(
            text,
            r#"<section id="papiertag-wiederanlauf"><h2>Papiertag ohne Abschluss</h2>
            <p class="halt">Der letzte begonnene Papiertag muss geprüft werden.</p>
            <p>Letzter bestätigter Abschluss: {letzter} · Offener Abrechnungstag: {tag}.</p>
            <p>Seit dem Abschluss belegt: {} laufbezogene Journaleinträge und {} KI-Aufrufbelege.</p>"#,
            self.eintraege, self.ki_aufrufe
        );
        self.kontovergleich(text);
        self.auftraege(text);
        text.push_str(
            r##"<p>Prüfe vor dem nächsten Papiertag die markierten Aufträge und Abweichungen.
            Das Laufprotokoll findest du unter <a href="#taetigkeit">Tätigkeit</a>.
            Die Diagnose ändert keine Buchungen.</p></section>"##,
        );
    }

    fn kontovergleich(&self, text: &mut String) {
        match &self.konto {
            Ok(abweichungen)
                if abweichungen.is_empty() && self.auftraege.iter().all(|a| a.gleich) =>
            {
                text.push_str("<p>Journal und Papierkonto stimmen in den geprüften Daten überein. Der Tagesabschluss fehlt weiterhin.</p>");
            }
            Ok(abweichungen) => {
                text.push_str(
                    "<p class=\"halt\">Journal und Papierkonto müssen abgeglichen werden.</p>",
                );
                if !abweichungen.is_empty() {
                    text.push_str("<ul>");
                    for abweichung in abweichungen.iter().take(ZEILEN_MAX) {
                        let _ = write!(text, "<li>{}</li>", maskiere(&abweichung_text(abweichung)));
                    }
                    text.push_str("</ul>");
                    weitere_zeilen(abweichungen.len(), text);
                }
            }
            Err(grund) => {
                let _ = write!(
                    text,
                    "<p class=\"offen\">Papierkonto nicht prüfbar: {}</p>",
                    maskiere(grund)
                );
            }
        }
    }

    fn auftraege(&self, text: &mut String) {
        if self.auftraege.is_empty() {
            text.push_str("<p>Seit dem Abschluss sind keine Auftragsbewegungen belegt. Im Journal sind keine offenen Orders verzeichnet.</p>");
            return;
        }
        text.push_str(r#"<details id="wiederanlauf-auftraege" open><summary>Aufträge prüfen</summary><div class="breit"><table>
            <tr><th>Auftrag</th><th>Wert</th><th>Journal</th><th>Papierkonto</th><th>Abgleich</th></tr>"#);
        for a in self.auftraege.iter().take(ZEILEN_MAX) {
            let journal = a.journal.map_or_else(
                || "nicht belegt".into(),
                |(status, stueck)| {
                    format!(
                        "{} · {stueck} Stück ausgeführt",
                        crate::depot::zustand_text(status)
                    )
                },
            );
            let konto = if self.konto.is_err() {
                "nicht prüfbar".into()
            } else {
                a.konto.map_or_else(
                    || "kein Auftrag mit dieser Kennung".into(),
                    |(status, stueck)| {
                        format!("{} · {stueck} Stück ausgeführt", konto_text(status))
                    },
                )
            };
            let vergleich = if self.konto.is_err() {
                "offen"
            } else if a.gleich {
                "stimmt überein"
            } else {
                "prüfen"
            };
            let _ = write!(
                text,
                "<tr><th scope=\"row\">{}</th><td>{}</td><td>{journal}</td><td>{konto}</td><td>{vergleich}</td></tr>",
                a.nr,
                maskiere(&a.wert)
            );
        }
        text.push_str("</table></div>");
        weitere_zeilen(self.auftraege.len(), text);
        text.push_str("</details>");
    }
}

fn weitere_zeilen(anzahl: usize, text: &mut String) {
    if anzahl > ZEILEN_MAX {
        let _ = write!(
            text,
            "<p>{ZEILEN_MAX} von {anzahl} Einträgen angezeigt.</p>"
        );
    }
}

fn datum(tag: Option<Datum>) -> String {
    tag.map_or_else(|| "nicht belegt".into(), crate::datum_text)
}

fn kontostatus(z: AuftragsZustand) -> Option<KontoStatus> {
    match z {
        AuftragsZustand::Angenommen | AuftragsZustand::Teilgefuellt => Some(KontoStatus::Offen),
        AuftragsZustand::Gefuellt => Some(KontoStatus::Gefuellt),
        AuftragsZustand::Abgelehnt => Some(KontoStatus::Abgelehnt),
        AuftragsZustand::Storniert => Some(KontoStatus::Storniert),
        AuftragsZustand::Verfallen => Some(KontoStatus::Verfallen),
        _ => None,
    }
}

fn konto_text(z: KontoStatus) -> &'static str {
    match z {
        KontoStatus::Offen => "offen",
        KontoStatus::Gefuellt => "ausgeführt",
        KontoStatus::Abgelehnt => "abgelehnt",
        KontoStatus::Storniert => "storniert",
        KontoStatus::Verfallen => "verfallen",
    }
}

fn abrechnung(a: Option<(Datum, Cent)>) -> String {
    a.map_or_else(
        || "fehlt".into(),
        |(tag, wert)| format!("{wert} am {}", crate::datum_text(tag)),
    )
}

fn abweichung_text(a: &Abweichung) -> String {
    match a {
        Abweichung::Bargeld { journal, konto } => {
            format!("Bargeld: Journal {journal}, Papierkonto {konto}.")
        }
        Abweichung::Bestand {
            isin,
            journal,
            konto,
        } => format!("Bestand {isin}: Journal {journal} Stück, Papierkonto {konto} Stück."),
        Abweichung::OrderNurBeimKonto(nr) => format!("Auftrag {nr} ist nur im Papierkonto offen."),
        Abweichung::OrderFehlt(nr) | Abweichung::OrderNichtImAuszug(nr) => {
            format!("Auftrag {nr} ist laut Journal offen und fehlt im Auszug des Papierkontos.")
        }
        Abweichung::OrderOhneVersand(nr) => {
            format!("Papierkonto kennt Auftrag {nr}; im Journal fehlt der Versand.")
        }
        Abweichung::Abrechnung { nr, journal, konto } => format!(
            "Abrechnung für Auftrag {nr}: Journal {}, Papierkonto {}.",
            abrechnung(*journal),
            abrechnung(*konto)
        ),
        Abweichung::AbrechnungsnachweisFehlt => {
            "Abrechnungsnachweis des Papierkontos fehlt.".into()
        }
        Abweichung::Fremdwaehrung(w) => format!("Unerwartete Währung im Papierkonto: {w}."),
        Abweichung::SaldoNegativ(b) => format!("Papierkonto zeigt einen negativen Saldo von {b}."),
        Abweichung::Unbuchbar { nr, grund } => format!("Auftrag {nr} ist nicht buchbar: {grund}"),
        Abweichung::Rechnung => "Summen aus dem Journal lassen sich nicht berechnen.".into(),
    }
}
