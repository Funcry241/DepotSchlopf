use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{
    Art, AufrufErlaubnis, Journal, NachrichtBeleg, Prognoserichtung, VorbereitungsAntwort,
    Weltprognose,
};
use depot_kern::liste::{als_universum, beobachtungsliste};
use depot_kern::{BeraterId, Cent, Datum, Universum, Vorschlag, Zeitpunkt};
use depot_sim::vorbereitung::{naechster_handelstag, vorbereiten};
use depot_sim::{Antwort, Berater, Lage, Urteil, tagnummer, weltbelege_am, weltlage_am};

static NR: AtomicU32 = AtomicU32::new(0);
fn datum(j: u16, maus: u8, t: u8) -> Result<Datum, String> {
    Datum::neu(j, maus, t).ok_or("Datum".into())
}
fn zeit(t: u8) -> Result<Zeitpunkt, String> {
    Zeitpunkt::frankfurt(datum(2026, 11, t)?, 12, 0, 0).ok_or("Zeit".into())
}
fn universum() -> Result<Universum, String> {
    Ok(als_universum(&beobachtungsliste()?))
}
struct Ort(PathBuf);
impl Ort {
    fn neu() -> Result<Self, String> {
        let n = NR.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("depot_vorbereitung_{}_{n}", std::process::id()));
        fs::create_dir(&p).map_err(|e| e.to_string())?;
        Journal::einrichten(&p.join("journal.txt")).map_err(|e| format!("{e:?}"))?;
        Ok(Self(p))
    }
    fn journal(&self) -> Result<Journal, String> {
        Journal::oeffnen(&self.0.join("journal.txt")).map_err(|e| format!("{e:?}"))
    }
}
impl Drop for Ort {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
type Fragen = Rc<RefCell<Vec<(BeraterId, Vec<String>)>>>;
struct Modell {
    wer: BeraterId,
    fragen: Fragen,
    isin: depot_kern::Isin,
}
impl Berater for Modell {
    fn vorschlagen(&mut self, e: AufrufErlaubnis, l: &Lage<'_>) -> Antwort {
        assert_eq!(e.buchung().berater, self.wer);
        self.fragen
            .borrow_mut()
            .push((self.wer, l.ereignisse.to_vec()));
        Antwort {
            verwertbar: true,
            vorschlag: None,
            einschaetzung: format!("{:?}-Erstmeinung <script>", self.wer),
            prognose: Some(Weltprognose {
                isin: self.isin.clone(),
                richtung: Prognoserichtung::Steigt,
                handelstage: 3,
                ereignis_nr: 1,
                begruendung: "Konkrete Wirkung".into(),
                widerlegung: "Neue Gegenbelege beachten".into(),
            }),
        }
    }
    fn pruefen(&mut self, _: AufrufErlaubnis, _: &Lage<'_>, _: &Vorschlag) -> Urteil {
        Urteil {
            verwertbar: false,
            zustimmung: false,
            begruendung: "Keine Handelsprüfung in der Vorbereitung".into(),
        }
    }
}
fn modelle() -> Result<(Modell, Modell, Fragen), String> {
    let fragen = Rc::new(RefCell::new(Vec::new()));
    let isin = depot_kern::Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
    Ok((
        Modell {
            wer: BeraterId::Dachs,
            fragen: fragen.clone(),
            isin: isin.clone(),
        },
        Modell {
            wer: BeraterId::Maus,
            fragen: fragen.clone(),
            isin,
        },
        fragen,
    ))
}
fn nachricht(j: &mut Journal, wann: Zeitpunkt, abruf: Zeitpunkt, text: &str) -> Result<(), String> {
    let b = NachrichtBeleg::neu("rss-test", abruf, b"rohe Quelle", wann, text)?;
    j.belege_nachricht(&b)
        .map(|_| ())
        .map_err(|e| format!("{e:?}"))
}

#[test]
fn zwei_unabhaengige_erstmeinungen_ohne_handelszustand_und_ohne_wiederholung() -> Result<(), String>
{
    let ort = Ort::neu()?;
    let mut j = ort.journal()?;
    let t = zeit(7)?;
    nachricht(&mut j, t, t, "Politisches Ereignis am Samstag")?;
    let (mut dachs, mut maus, fragen) = modelle()?;
    vorbereiten(&mut j, t, &universum()?, &mut dachs, &mut maus, || Some(t))?;
    assert_eq!(fragen.borrow().len(), 2);
    assert_eq!(
        fragen.borrow().first().map(|(_, e)| e),
        fragen.borrow().last().map(|(_, e)| e)
    );
    assert!(
        !fragen
            .borrow()
            .last()
            .is_some_and(|(_, e)| e.iter().any(|s| s.contains("Dachs-Erstmeinung")))
    );
    assert_eq!(j.stand().seit_abschluss(), 0);
    assert!(j.stand().toepfe().is_empty() && j.stand().auftraege().is_empty());
    assert!(j.stand().abrechnungstag().is_none());
    assert_eq!(j.stand().ki().buchungen().len(), 2);
    assert!(
        j.stand()
            .vorbereitungen()
            .last()
            .is_some_and(depot_journal::Vorbereitung::abgeschlossen)
    );
    let vorher = j.bestaetigt();
    drop(j);
    let mut j = ort.journal()?;
    vorbereiten(&mut j, t, &universum()?, &mut dachs, &mut maus, || Some(t))?;
    assert_eq!(vorher, j.bestaetigt());
    assert_eq!(fragen.borrow().len(), 2);
    let montag = datum(2026, 11, 9)?;
    assert!(
        weltlage_am(j.stand(), montag, zeit(9)?)
            .iter()
            .any(|s| s.contains("Frühere KI-Hypothese"))
    );
    assert!(
        !weltbelege_am(j.stand(), montag, zeit(9)?)
            .iter()
            .any(|s| s.contains("Erstmeinung"))
    );
    assert!(
        !weltlage_am(j.stand(), datum(2026, 11, 13)?, zeit(13)?)
            .iter()
            .any(|s| s.contains("Frühere KI-Hypothese")),
        "Hypothesen verfallen nach ihrem Horizont"
    );
    // Die reguläre Montagsreservierung folgt ohne Zyklus-Ruecksprung.
    assert!(
        j.reserviere(BeraterId::Dachs, montag, tagnummer(montag).ok_or("Zyklus")?)
            .is_ok()
    );
    Ok(())
}

#[test]
fn unterbrechung_nach_reservierung_fragt_nach_neustart_niemanden_nochmal() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.journal()?;
    let t = zeit(7)?;
    nachricht(&mut j, t, t, "Ereignis")?;
    let fuer = naechster_handelstag(t.datum().ok_or("Datum")?).ok_or("Handelstag")?;
    j.starte_vorbereitung(t, fuer, vec!["Nachricht Ereignis".into()])
        .map_err(|e| format!("{e:?}"))?;
    let _ = j
        .reserviere_vorbereitung(t, BeraterId::Dachs)
        .map_err(|e| format!("{e:?}"))?;
    let vorher = j.bestaetigt();
    drop(j);
    let mut j = ort.journal()?;
    let (mut dachs, mut maus, fragen) = modelle()?;
    vorbereiten(&mut j, t, &universum()?, &mut dachs, &mut maus, || Some(t))?;
    assert!(fragen.borrow().is_empty());
    assert_eq!(vorher, j.bestaetigt());
    assert_eq!(j.stand().ki().buchungen().len(), 1);
    assert_eq!(j.stand().seit_abschluss(), 0);
    assert!(
        j.stand()
            .vorbereitungen()
            .last()
            .is_some_and(|p| !p.abgeschlossen())
    );
    Ok(())
}

#[test]
fn erschoepftes_monatsbudget_verhindert_auch_vorbereitungsaufrufe() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.journal()?;
    for n in 1..=23 {
        let tag = datum(2026, 11, n)?;
        let zyklus = tagnummer(tag).ok_or("Zyklus")?;
        for wer in [BeraterId::Dachs, BeraterId::Maus] {
            for _ in 0..2 {
                let _ = j.reserviere(wer, tag, zyklus);
            }
        }
    }
    j.schliesse_ab(datum(2026, 11, 23)?, Cent(500_000), &"a".repeat(64))
        .map_err(|e| format!("{e:?}"))?;
    let t = zeit(28)?;
    nachricht(&mut j, t, t, "Neues Ereignis")?;
    let vorher = j.stand().ki().buchungen().len();
    let (mut dachs, mut maus, fragen) = modelle()?;
    vorbereiten(&mut j, t, &universum()?, &mut dachs, &mut maus, || Some(t))?;
    assert!(fragen.borrow().is_empty());
    assert_eq!(j.stand().ki().buchungen().len(), vorher);
    let p = j.stand().vorbereitungen().last().ok_or("Vorbereitung")?;
    assert_eq!(p.antworten.len(), 2);
    assert!(
        p.antworten
            .iter()
            .all(|a| !a.gefragt && a.text.contains("Kostenschutz"))
    );
    Ok(())
}

#[test]
fn samstag_und_boersenfeiertag_bleiben_bekannt_aber_spaeterer_abruf_nicht() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.journal()?;
    let freitag = Zeitpunkt::neu(datum(2026, 4, 3)?, 12, 0, 0).ok_or("Zeit")?;
    let samstag = Zeitpunkt::neu(datum(2026, 4, 4)?, 12, 0, 0).ok_or("Zeit")?;
    let montag = Zeitpunkt::neu(datum(2026, 4, 6)?, 12, 0, 0).ok_or("Zeit")?;
    nachricht(&mut j, freitag, samstag, "Ereignis am Karfreitag")?;
    nachricht(&mut j, samstag, samstag, "Ereignis am Samstag")?;
    nachricht(&mut j, freitag, montag, "Erst am Montag bekannt geworden")?;
    let aus = weltbelege_am(j.stand(), datum(2026, 4, 6)?, samstag);
    assert!(aus.iter().any(|s| s.contains("Karfreitag")));
    assert!(aus.iter().any(|s| s.contains("Samstag")));
    assert!(!aus.iter().any(|s| s.contains("Erst am Montag")));
    Ok(())
}

#[test]
fn journal_lehnt_umwege_doppelte_reservierung_und_falsche_mitwirkung_ab() -> Result<(), String> {
    let ort = Ort::neu()?;
    let mut j = ort.journal()?;
    let t = zeit(7)?;
    assert!(
        j.anhaengen(Art::Vorbereitung, "gefälschte Buchung")
            .is_err()
    );
    assert!(
        j.starte_vorbereitung(zeit(6)?, datum(2026, 11, 9)?, vec!["Ereignis".into()])
            .is_err()
    );
    j.starte_vorbereitung(t, datum(2026, 11, 9)?, vec!["Ereignis".into()])
        .map_err(|e| format!("{e:?}"))?;
    assert!(j.reserviere_vorbereitung(t, BeraterId::Dachs).is_ok());
    assert!(j.reserviere_vorbereitung(t, BeraterId::Dachs).is_err());
    assert!(
        j.belege_vorbereitungsantwort(
            t,
            VorbereitungsAntwort {
                zeit: t,
                berater: BeraterId::Maus,
                gefragt: true,
                verwertbar: true,
                text: "erfunden".into(),
                prognose: None
            }
        )
        .is_err()
    );
    assert_eq!(j.stand().ki().buchungen().len(), 1);
    let vorher = j.bestaetigt();
    drop(j);
    let j = ort.journal()?;
    assert_eq!(vorher, j.bestaetigt());
    Ok(())
}
