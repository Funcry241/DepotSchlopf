//! Ein eingefrorener Wissensstand, zwei voneinander getrennte Erstmeinungen, keine Orders.
use std::path::Path;

use depot_journal::{Journal, VorbereitungsAntwort};
use depot_kern::{BeraterId, Cent, Datum, Kurse, Topf, TopfId, Universum, Zeitpunkt};
use depot_ki::Sperre;

use crate::{Berater, Lage, weltbelege_am};

/// Dieselben Grenzen fuer Handelsfragen und Vorbereitung, gemessen im gesendeten JSON.
pub const MAX_EREIGNISSE_BYTES: usize = 24_000;
pub const MAX_TEXT: usize = 600;

pub fn text_fuer_frage(text: &str) -> String {
    let mut aus: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_TEXT)
        .collect();
    if text.chars().nth(MAX_TEXT).is_some() {
        aus.push_str(" …");
    }
    aus
}

/// Nur die tatsaechlich sichtbare, fuer beide KIs identische Auswahl wird eingefroren.
pub fn sichtbare_ereignisse(ereignisse: &[String]) -> Vec<String> {
    let mut aus = Vec::new();
    let mut platz = MAX_EREIGNISSE_BYTES;
    for e in ereignisse
        .iter()
        .take(depot_journal::vorbereitung::MAX_EREIGNISSE)
    {
        let text = text_fuer_frage(e);
        let nr = aus.len().saturating_add(1);
        let n = depot_kern::json::zitiere(&format!("- Ereignis {nr}: {text}\n")).len();
        if n > platz {
            break;
        }
        platz = platz.saturating_sub(n);
        aus.push(text);
    }
    aus
}

/// Der naechste Tag, an dem wenigstens eine Boerse der Liste handelt.
pub fn naechster_handelstag(tag: Datum) -> Option<Datum> {
    (1..=31)
        .filter_map(|n| tag.plus_tage(n))
        .find(|d| depot_kern::liste::handelstag(*d))
}

/// Nur 08 bis 23 Uhr Frankfurt: dieselben erlaubten Zeiten wie die Schlagzeilen.
pub fn tag_jetzt(zeit: Zeitpunkt) -> Option<Datum> {
    let (tag, stunde, ..) = zeit.in_frankfurt()?;
    (8..23).contains(&stunde).then_some(tag)
}

fn sauber(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(1_200)
        .collect();
    if t.trim().is_empty() {
        "(keine Aussage)".into()
    } else {
        t
    }
}

/// Startet hoechstens einmal je boersenfreiem Tag, auch nach einem Absturz. Jeder Aufruf
/// braucht eine bereits synchronisierte Budgetreservierung. Eine unvollstaendige Vorbereitung
/// sperrt keinen spaeteren Papiertag und wird nicht als abgeschlossen ausgegeben.
pub fn vorbereiten<'a>(
    j: &mut Journal,
    zeit: Zeitpunkt,
    universum: &Universum,
    dachs: &'a mut (dyn Berater + 'a),
    maus: &'a mut (dyn Berater + 'a),
    mut uhr: impl FnMut() -> Option<Zeitpunkt>,
) -> Result<String, String> {
    let tag = tag_jetzt(zeit).ok_or("Vorbereitung: nur zwischen 08 und 23 Uhr Frankfurt")?;
    if depot_kern::liste::handelstag(tag) {
        return Ok("Heute ist Handelstag: Die Vorbereitung fließt in die regulären KI-Aufrufe ein; kein zusätzlicher Aufruf.".into());
    }
    if j.stand()
        .vorbereitungen()
        .iter()
        .any(|p| p.zeit.datum() == Some(tag))
    {
        return Ok("Vorbereitung heute bereits begonnen; keine erneuten KI-Aufrufe, auch nach einem Abbruch.".into());
    }
    let fuer = naechster_handelstag(tag).ok_or("Kein nächster Handelstag")?;
    // Vor beiden Antworten und allen Reservierungen fixieren. Kein Ergebnis des anderen
    // Beraters wird der zweiten KI uebergeben.
    let ereignisse = sichtbare_ereignisse(&weltbelege_am(j.stand(), fuer, zeit));
    if ereignisse.is_empty() {
        return Ok("Noch keine belegten Ereignisse für die Vorbereitung. Zuerst Schlagzeilen und Termine abrufen.".into());
    }
    j.starte_vorbereitung(zeit, fuer, ereignisse.clone())
        .map_err(|e| format!("{e:?}"))?;
    let kurse = Kurse::neu(zeit, Vec::new()).map_err(|e| format!("{e:?}"))?;
    let positiv = universum.isins();
    for (wer, id, b) in [
        (BeraterId::Dachs, TopfId::A, dachs),
        (BeraterId::Maus, TopfId::B, maus),
    ] {
        let topf = Topf::neu(id, Cent::NULL).map_err(|e| format!("{e:?}"))?;
        let mut a = VorbereitungsAntwort {
            zeit: uhr().ok_or("Uhr liefert keine Zeit")?,
            berater: wer,
            gefragt: false,
            verwertbar: false,
            text: String::new(),
            prognose: None,
        };
        if a.zeit < zeit {
            return Err(
                "Uhr liegt vor dem gespeicherten Wissensstichtag; keine neue Anfrage".into(),
            );
        }
        if tag_jetzt(a.zeit) != Some(tag) {
            a.text = "Nicht gefragt: Zeitfenster abgelaufen.".into();
        } else if b.angebunden() {
            match j.reserviere_vorbereitung(zeit, wer) {
                Ok(erlaubnis) => {
                    a.gefragt = true;
                    println!(
                        "{}",
                        depot_kern::hinweis::zeile(&format!(
                            "Vorbereitung für {fuer}: {wer:?} wird mit dem gespeicherten Wissensstand gefragt."
                        ))
                    );
                    let antwort = b.vorschlagen(
                        erlaubnis,
                        &Lage {
                            datum: fuer,
                            topf: &topf,
                            kaufbar: Cent::NULL,
                            kurse: &kurse,
                            verlauf: &[],
                            kaufbilanz: crate::ausfuehrung::Kennzahlen::default(),
                            positionskaeufe: &[],
                            ereignisse: &ereignisse,
                        },
                    );
                    crate::lauf::belege_ki(j, b).map_err(|e| format!("{e:?}"))?;
                    a.zeit = uhr().ok_or("Uhr liefert keine Zeit")?;
                    a.verwertbar = antwort.verwertbar && antwort.vorschlag.is_none();
                    a.text = sauber(&antwort.einschaetzung);
                    if a.verwertbar {
                        a.prognose = antwort.prognose.filter(|p| {
                            p.gueltig()
                                && p.ereignis_nr <= ereignisse.len()
                                && positiv.contains(&p.isin)
                        });
                    } else if antwort.vorschlag.is_some() {
                        a.text =
                            "Antwort verworfen: Vorbereitung darf keine Handelsziele enthalten."
                                .into();
                    }
                }
                Err(Sperre::Kassenbuch(e)) => return Err(e),
                Err(e) => a.text = format!("Nicht gefragt: Kostenschutz {e:?}."),
            }
        } else {
            a.text = sauber(&format!(
                "Nicht gefragt: {}.",
                b.warum_nicht().unwrap_or_else(|| "nicht angebunden".into())
            ));
        }
        if !a.gefragt {
            b.anfrage_uebersprungen(depot_journal::KiZweck::Vorbereitung, &a.text);
        }
        println!("[Vorbereitung] {:?}: {}", wer, a.text);
        j.belege_vorbereitungsantwort(zeit, a)
            .map_err(|e| format!("{e:?}"))?;
    }
    Ok(format!(
        "Vorbereitung für {fuer} dokumentiert. Keine Orders erzeugt; Ergebnisse und Ausfälle stehen auf Live."
    ))
}

fn oeffnen(pfad: &Path) -> Result<Journal, String> {
    for versuch in 0..10 {
        match Journal::oeffnen(pfad) {
            Ok(j) => return Ok(j),
            Err(depot_journal::JournalFehler::Belegt) if versuch < 9 => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => return Err(format!("{e:?}")),
        }
    }
    Err("Journal weiterhin belegt".into())
}

pub fn lauf<'a>(
    wurzel: &Path,
    zeit: Zeitpunkt,
    universum: &Universum,
    dachs: &'a mut (dyn Berater + 'a),
    maus: &'a mut (dyn Berater + 'a),
    uhr: impl FnMut() -> Option<Zeitpunkt>,
) -> Result<String, String> {
    if wurzel
        .join("daten/update_offen.txt")
        .try_exists()
        .map_err(|e| e.to_string())?
    {
        return Err(
            "Das Update ist noch nicht bestätigt. Keine Vorbereitung während der Probezeit.".into(),
        );
    }
    if let Some(grund) = depot_journal::kurssperre::kurssperre(&wurzel.join("daten")) {
        return Err(format!(
            "Quellensperre aktiv: {grund}. Keine Vorbereitung, bis die Quelle geprüft ist."
        ));
    }
    let mut j = oeffnen(&wurzel.join("daten/journal.txt"))?;
    vorbereiten(&mut j, zeit, universum, dachs, maus, uhr)
}
