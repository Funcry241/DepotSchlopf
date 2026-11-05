#![allow(
    clippy::unnecessary_wraps,
    reason = "Testcode: Antworten der Attrappe haben die Form der Netzantwort"
)]

//! Dachs und Maus mit einem erfundenen Anbieter: Anfrage, Antwort, Kostenpruefung, Sperre.
//! Kein Netz; alle Antworten sind fuer den Test geschrieben.

use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use depot_berater::json::zitiere;
use depot_berater::{KiBerater, Netz, NetzFehler, Rueckmeldung, Schluessel};
use depot_journal::{AufrufErlaubnis, Journal};
use depot_kern::{
    BeraterId, Cent, Datum, Herkunft, Instrument, InstrumentArt, Isin, Kurs, Kurse, Topf, TopfId,
    Universum, Vorschlag, Zeitpunkt,
};
use depot_sim::{Berater, Lage};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

/// Ein erfundener Schluessel; er darf nirgends ausser im Kopf der Anfrage erscheinen.
const SCHLUESSEL: &str = "testschluessel-0123456789abcdef";

fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>, was: &str) -> Result<T, String> {
    r.map_err(|e| format!("Vorbedingung {was}: {e:?}"))
}

fn some<T>(o: Option<T>, was: &str) -> Result<T, String> {
    o.ok_or_else(|| format!("Vorbedingung {was}"))
}

/// Gesendete Anfrage: Adresse, Kopf, Inhalt.
type Gesendet = (String, Vec<(String, String)>, String);

#[derive(Default)]
struct Attrappe {
    antworten: VecDeque<Result<Rueckmeldung, NetzFehler>>,
    gesendet: Vec<Gesendet>,
}

impl Netz for Attrappe {
    fn sende(
        &mut self,
        adresse: &str,
        kopf: &[(&str, &str)],
        inhalt: &[u8],
    ) -> Result<Rueckmeldung, NetzFehler> {
        self.gesendet.push((
            adresse.to_string(),
            kopf.iter()
                .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
                .collect(),
            String::from_utf8_lossy(inhalt).into_owned(),
        ));
        self.antworten.pop_front().unwrap_or_else(|| {
            Err(NetzFehler::NichtGesendet(
                "keine Antwort vorbereitet".into(),
            ))
        })
    }
}

fn ok(inhalt: &str) -> Result<Rueckmeldung, NetzFehler> {
    Ok(Rueckmeldung {
        status: 200,
        inhalt: inhalt.as_bytes().to_vec(),
    })
}

/// Eine Antwort von Claude mit Text, Token und Suchen.
fn claude(text: &str, ein: u64, aus: u64, suchen: u64) -> Result<Rueckmeldung, NetzFehler> {
    ok(&format!(
        concat!(
            "{{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[",
            "{{\"type\":\"server_tool_use\",\"id\":\"s1\",\"name\":\"web_search\",\"input\":{{\"query\":\"SAP\"}}}},",
            "{{\"type\":\"web_search_tool_result\",\"tool_use_id\":\"s1\",\"content\":[]}},",
            "{{\"type\":\"text\",\"text\":{}}}],",
            "\"stop_reason\":\"end_turn\",",
            "\"usage\":{{\"input_tokens\":{},\"cache_creation_input_tokens\":0,",
            "\"cache_read_input_tokens\":0,\"output_tokens\":{},",
            "\"server_tool_use\":{{\"web_search_requests\":{}}}}}}}"
        ),
        zitiere(text),
        ein,
        aus,
        suchen
    ))
}

/// Eine Antwort von GPT mit Text, Token und einer Suche.
fn gpt(text: &str, ein: u64, aus: u64) -> Result<Rueckmeldung, NetzFehler> {
    ok(&format!(
        concat!(
            "{{\"id\":\"resp_1\",\"status\":\"completed\",\"output\":[",
            "{{\"type\":\"web_search_call\",\"id\":\"ws_1\",\"status\":\"completed\"}},",
            "{{\"type\":\"message\",\"role\":\"assistant\",\"content\":[",
            "{{\"type\":\"output_text\",\"text\":{}}}]}}],",
            "\"usage\":{{\"input_tokens\":{},\"output_tokens\":{}}}}}"
        ),
        zitiere(text),
        ein,
        aus
    ))
}

struct Ort {
    dir: PathBuf,
    journal: Journal,
}

impl Ort {
    fn neu() -> Result<Ort, String> {
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("depot_berater_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        vorbedingung(fs::create_dir_all(&dir), "Testverzeichnis")?;
        let pfad = dir.join("journal.txt");
        vorbedingung(Journal::einrichten(&pfad), "Einrichtung")?;
        let journal = vorbedingung(Journal::oeffnen(&pfad), "Journal")?;
        Ok(Ort { dir, journal })
    }

    fn sperre(&self) -> PathBuf {
        self.dir.join("ki.halt")
    }

    fn erlaubnis(&mut self, wer: BeraterId, zyklus: u32) -> Result<AufrufErlaubnis, String> {
        vorbedingung(self.journal.reserviere(wer, tag()?, zyklus), "Erlaubnis")
    }
}

impl Drop for Ort {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn tag() -> Result<Datum, String> {
    some(Datum::neu(2026, 11, 6), "Datum")
}

fn sap() -> Result<Isin, String> {
    vorbedingung(Isin::neu("DE0007164600"), "ISIN SAP")
}

fn universum() -> Result<Universum, String> {
    Ok(Universum::neu(vec![Instrument {
        isin: sap()?,
        name: "SAP".into(),
        art: InstrumentArt::Aktie,
    }]))
}

fn kurse() -> Result<Kurse, String> {
    let d = tag()?;
    let zeit = some(Zeitpunkt::neu(d, 17, 30, 0), "Zeit")?;
    let k = vorbedingung(
        Kurs::neu(Cent(23_000), "EUR", d, zeit, Herkunft::Quelle),
        "Kurs",
    )?;
    vorbedingung(Kurse::neu(zeit, vec![(sap()?, k)]), "Kurse")
}

fn berater(ort: &Ort, wer: BeraterId, netz: Attrappe) -> Result<KiBerater<Attrappe>, String> {
    Ok(KiBerater::neu(
        wer,
        Some(vorbedingung(Schluessel::neu(SCHLUESSEL), "Schluessel")?),
        netz,
        universum()?,
        ort.sperre(),
    ))
}

const VORSCHLAG: &str = "Nach den Zahlen:\n```json\n{\"einschaetzung\": \"SAP nach Zahlen fest\", \"ziele\": [{\"isin\": \"DE0007164600\", \"gewicht_bp\": 3000, \"begruendung\": \"Zahlen ueber Erwartung\"}]}\n```";

fn mit_lage<T>(f: impl FnOnce(&Lage<'_>) -> T) -> Result<T, String> {
    let topf = vorbedingung(Topf::neu(TopfId::A, Cent(500_000)), "Topf")?;
    let kurse = kurse()?;
    let ereignisse = vec!["SAP: Quartalszahlen am 06.11.".to_string()];
    let lage = Lage {
        datum: tag()?,
        topf: &topf,
        kurse: &kurse,
        ereignisse: &ereignisse,
    };
    Ok(f(&lage))
}

#[test]
fn dachs_schlaegt_vor_und_die_kosten_stehen_dabei() -> Result<(), String> {
    let mut ort = Ort::neu()?;
    let mut netz = Attrappe::default();
    netz.antworten
        .push_back(claude(VORSCHLAG, 20_000, 1_500, 1));
    let mut dachs = berater(&ort, BeraterId::Dachs, netz)?;
    assert!(dachs.angebunden());
    let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
    let antwort = mit_lage(|l| dachs.vorschlagen(e, l))?;
    let v = some(antwort.vorschlag.clone(), "Vorschlag")?;
    assert_eq!(v.berater, BeraterId::Dachs);
    assert_eq!(v.zielgewichte_bp.get(&sap()?), Some(&3_000));
    // 20.000 x 2 $ + 1.500 x 10 $ je Mio. + 1 Suche zu 1 Cent = 6,5 Cent, aufgerundet 7.
    assert!(
        antwort
            .einschaetzung
            .contains("Kosten 7 von 41 Cent reserviert"),
        "{}",
        antwort.einschaetzung
    );
    assert!(!ort.sperre().exists());
    let (adresse, kopf, inhalt) = some(dachs.netz().gesendet.first(), "gesendet")?;
    assert_eq!(adresse, "https://api.anthropic.com/v1/messages");
    assert!(
        kopf.iter()
            .any(|(k, w)| k == "x-api-key" && w == SCHLUESSEL)
    );
    assert!(!inhalt.contains(SCHLUESSEL), "Schluessel nur im Kopf");
    for teil in [
        "\"model\":\"claude-sonnet-5-5\"",
        "\"max_tokens\":16000",
        "\"thinking\":{\"type\":\"adaptive\"}",
        "\"effort\":\"medium\"",
        "\"max_uses\":5",
        "web_search_20260209",
        "DE0007164600",
        "Quartalszahlen",
    ] {
        assert!(inhalt.contains(teil), "{teil} fehlt in {inhalt}");
    }
    Ok(())
}

#[test]
fn maus_prueft_mit_gpt_und_nur_true_ist_zustimmung() -> Result<(), String> {
    let mut ort = Ort::neu()?;
    let mut netz = Attrappe::default();
    netz.antworten.push_back(gpt(
        "{\"zustimmung\": true, \"begruendung\": \"beobachtet, nicht vorhergesagt\"}",
        10_000,
        800,
    ));
    netz.antworten.push_back(gpt(
        "{\"zustimmung\": \"ja\", \"begruendung\": \"passt\"}",
        10_000,
        800,
    ));
    let mut maus = berater(&ort, BeraterId::Maus, netz)?;
    let v = Vorschlag {
        berater: BeraterId::Dachs,
        zielgewichte_bp: [(sap()?, 3_000)].into_iter().collect(),
        begruendung: [(sap()?, "Zahlen".to_string())].into_iter().collect(),
    };
    let e = ort.erlaubnis(BeraterId::Maus, 1)?;
    let u = mit_lage(|l| maus.pruefen(e, l, &v))?;
    assert!(u.zustimmung, "{u:?}");
    // 10.000 x 1 $ + 800 x 5 $ je Mio. + 1 Suche = 2,4 Cent, aufgerundet 3.
    assert!(u.begruendung.contains("Kosten 3 von 23 Cent"), "{u:?}");
    let e = ort.erlaubnis(BeraterId::Maus, 1)?;
    let u = mit_lage(|l| maus.pruefen(e, l, &v))?;
    assert!(!u.zustimmung, "\"ja\" ist keine Zustimmung: {u:?}");
    let (adresse, kopf, inhalt) = some(maus.netz().gesendet.first(), "gesendet")?;
    assert_eq!(adresse, "https://api.openai.com/v1/responses");
    assert!(
        kopf.iter()
            .any(|(k, w)| k == "authorization" && w == &format!("Bearer {SCHLUESSEL}"))
    );
    assert!(!inhalt.contains(SCHLUESSEL));
    for teil in [
        "\"model\":\"gpt-6.1-terra\"",
        "\"max_output_tokens\":16000",
        "\"reasoning\":{\"effort\":\"medium\"}",
        "\"max_tool_calls\":5",
        "\"store\":false",
    ] {
        assert!(inhalt.contains(teil), "{teil} fehlt in {inhalt}");
    }
    Ok(())
}

#[test]
fn kosten_ueber_der_reservierung_sperren_alle_kis() -> Result<(), String> {
    let mut ort = Ort::neu()?;
    let mut netz = Attrappe::default();
    // 200.000 x 2 $ + 1.500 x 10 $ + 2 Suchen = 43,5 Cent, aufgerundet 44 > 41 reserviert.
    netz.antworten
        .push_back(claude(VORSCHLAG, 200_000, 1_500, 2));
    let mut dachs = berater(&ort, BeraterId::Dachs, netz)?;
    let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
    let antwort = mit_lage(|l| dachs.vorschlagen(e, l))?;
    assert!(antwort.vorschlag.is_none(), "{antwort:?}");
    assert!(
        antwort.einschaetzung.contains("44 Cent ueber 41"),
        "{antwort:?}"
    );
    assert!(ort.sperre().exists(), "Sperrdatei geschrieben");
    assert!(!dachs.angebunden());
    // Maus liest dieselbe Sperrdatei und ist damit auch nicht angebunden.
    let maus = berater(&ort, BeraterId::Maus, Attrappe::default())?;
    assert!(!maus.angebunden());
    Ok(())
}

#[test]
fn zu_viele_suchen_sperren() -> Result<(), String> {
    let mut ort = Ort::neu()?;
    let mut netz = Attrappe::default();
    netz.antworten.push_back(claude(VORSCHLAG, 1_000, 100, 6));
    let mut dachs = berater(&ort, BeraterId::Dachs, netz)?;
    let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
    let antwort = mit_lage(|l| dachs.vorschlagen(e, l))?;
    assert!(antwort.vorschlag.is_none());
    assert!(antwort.einschaetzung.contains("6 Suchen"), "{antwort:?}");
    assert!(ort.sperre().exists());
    Ok(())
}

#[test]
fn fehlende_antwort_sperrt_abgewiesene_nicht() -> Result<(), String> {
    let mut ort = Ort::neu()?;
    let mut netz = Attrappe::default();
    netz.antworten
        .push_back(Err(NetzFehler::NichtGesendet("host not found".into())));
    netz.antworten.push_back(Ok(Rueckmeldung {
        status: 401,
        inhalt: format!("{{\"error\":\"falscher Schluessel {SCHLUESSEL}\"}}").into_bytes(),
    }));
    let mut dachs = berater(&ort, BeraterId::Dachs, netz)?;
    for _ in 0..2 {
        let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
        let a = mit_lage(|l| dachs.vorschlagen(e, l))?;
        assert!(a.vorschlag.is_none());
        assert!(!a.einschaetzung.contains(SCHLUESSEL), "{a:?}");
        assert!(!ort.sperre().exists(), "{a:?}");
    }
    // Eine Antwort ohne Verbrauch: Kosten unbekannt, Sperre.
    let mut netz = Attrappe::default();
    netz.antworten.push_back(ok("{\"kein\": \"Verbrauch\"}"));
    let mut maus = berater(&ort, BeraterId::Maus, netz)?;
    let e = ort.erlaubnis(BeraterId::Maus, 1)?;
    let a = mit_lage(|l| maus.vorschlagen(e, l))?;
    assert!(a.vorschlag.is_none());
    assert!(ort.sperre().exists(), "{a:?}");
    Ok(())
}

#[test]
fn abgebrochene_antwort_sperrt() -> Result<(), String> {
    let mut ort = Ort::neu()?;
    let mut netz = Attrappe::default();
    netz.antworten
        .push_back(Err(NetzFehler::Unklar("timeout".into())));
    let mut maus = berater(&ort, BeraterId::Maus, netz)?;
    let e = ort.erlaubnis(BeraterId::Maus, 1)?;
    let a = mit_lage(|l| maus.vorschlagen(e, l))?;
    assert!(a.vorschlag.is_none());
    assert!(ort.sperre().exists(), "{a:?}");
    Ok(())
}

#[test]
fn unpassende_antworten_ergeben_keinen_vorschlag() -> Result<(), String> {
    let faelle = [
        "kein JSON",
        "{\"einschaetzung\": \"x\", \"ziele\": [{\"isin\": \"DE0008404005\", \"gewicht_bp\": 1000, \"begruendung\": \"nicht auf der Liste\"}]}",
        "{\"einschaetzung\": \"x\", \"ziele\": [{\"isin\": \"DE0007164600\", \"gewicht_bp\": 30.5, \"begruendung\": \"Bruch\"}]}",
        "{\"einschaetzung\": \"x\", \"ziele\": [{\"isin\": \"DE0007164600\", \"gewicht_bp\": 12000, \"begruendung\": \"zu viel\"}]}",
        "{\"einschaetzung\": \"x\", \"ziele\": [{\"isin\": \"DE0007164600\", \"gewicht_bp\": 100, \"begruendung\": \"a\"}, {\"isin\": \"DE0007164600\", \"gewicht_bp\": 100, \"begruendung\": \"b\"}]}",
        "{\"einschaetzung\": \"x\", \"ziele\": \"SAP\"}",
        "{\"einschaetzung\": \"x\", \"einschaetzung\": \"y\", \"ziele\": null}",
        "{\"ziele\": null}",
    ];
    for antwort in faelle {
        let mut ort = Ort::neu()?;
        let mut netz = Attrappe::default();
        netz.antworten.push_back(claude(antwort, 1_000, 100, 0));
        let mut dachs = berater(&ort, BeraterId::Dachs, netz)?;
        let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
        let a = mit_lage(|l| dachs.vorschlagen(e, l))?;
        assert!(a.vorschlag.is_none(), "{antwort}: {a:?}");
        assert!(
            a.einschaetzung.starts_with("keine verwertbare Antwort"),
            "{a:?}"
        );
        assert!(!ort.sperre().exists());
    }
    Ok(())
}

#[test]
fn ohne_schluessel_oder_mit_sperrdatei_nicht_angebunden() -> Result<(), String> {
    let ort = Ort::neu()?;
    let ohne = KiBerater::neu(
        BeraterId::Dachs,
        None,
        Attrappe::default(),
        universum()?,
        ort.sperre(),
    );
    assert!(!ohne.angebunden());
    vorbedingung(fs::write(ort.sperre(), "geprueft?"), "Sperrdatei")?;
    let mit = berater(&ort, BeraterId::Dachs, Attrappe::default())?;
    assert!(!mit.angebunden());
    Ok(())
}

#[test]
fn schluessel_ohne_textform() -> Result<(), String> {
    let s = vorbedingung(Schluessel::neu(SCHLUESSEL), "Schluessel")?;
    assert_eq!(format!("{s:?}"), "Schluessel(***)");
    assert_eq!(s.bereinige(&format!("a {SCHLUESSEL} b")), "a *** b");
    assert!(Schluessel::neu("kurz").is_err());
    assert!(Schluessel::neu("mit leerzeichen 0123456789abcdef").is_err());
    assert!(Schluessel::neu("zeilenumbruch\r\nx-api-key: 0123456789").is_err());
    Ok(())
}

/// Ein ganzer Papiertag mit beiden KIs: Dachs (Claude) schlaegt SAP vor, Maus (GPT) stimmt zu,
/// das Papierkonto kauft. Maus will nichts aendern, Dachs muss also nichts pruefen. Jede
/// Antwort steht mit ihren Kosten im Journal.
#[test]
fn papiertag_mit_dachs_und_maus() -> Result<(), String> {
    use depot_journal::KursBeleg;
    use depot_kern::{Preis, liste};
    use depot_sim::{Ausgang, Papierdatei, papiertag};

    let mut ort = Ort::neu()?;
    let sperre = ort.sperre();
    let konto_pfad = ort.dir.join("papierkonto.txt");
    let j = &mut ort.journal;
    let heute = tag()?;
    let vortag = some(Datum::neu(2026, 11, 5), "Vortag")?;
    let abruf = some(Zeitpunkt::frankfurt(heute, 17, 40, 0), "Abruf")?;
    for (d, p) in [(vortag, "228"), (heute, "230")] {
        let zeit = some(Zeitpunkt::frankfurt(d, 17, 35, 0), "Schluss")?;
        let preis = some(Preis::lies_euro(p), "Preis")?;
        let kurs = vorbedingung(Kurs::exakt(preis, "EUR", d, zeit, Herkunft::Quelle), "Kurs")?;
        let beleg = vorbedingung(
            KursBeleg::neu(liste::QUELLE, abruf, p.as_bytes(), sap()?, kurs),
            "Beleg",
        )?;
        vorbedingung(j.belege_kurs(&beleg), "Beleg schreiben")?;
    }
    let mut netz_dachs = Attrappe::default();
    netz_dachs
        .antworten
        .push_back(claude(VORSCHLAG, 20_000, 1_500, 1));
    let mut netz_maus = Attrappe::default();
    netz_maus.antworten.push_back(gpt(
        "{\"zustimmung\": true, \"begruendung\": \"Zahlen belegt\"}",
        10_000,
        800,
    ));
    netz_maus.antworten.push_back(gpt(
        "{\"einschaetzung\": \"abwarten\", \"ziele\": null}",
        10_000,
        800,
    ));
    let schluessel = || vorbedingung(Schluessel::neu(SCHLUESSEL), "Schluessel").map(Some);
    let mut dachs = KiBerater::neu(
        BeraterId::Dachs,
        schluessel()?,
        netz_dachs,
        universum()?,
        sperre.clone(),
    );
    let mut maus = KiBerater::neu(
        BeraterId::Maus,
        schluessel()?,
        netz_maus,
        universum()?,
        sperre.clone(),
    );
    let start = vec![
        vorbedingung(Topf::neu(TopfId::A, Cent(500_000)), "Topf A")?,
        vorbedingung(Topf::neu(TopfId::B, Cent(500_000)), "Topf B")?,
    ];
    let mut konto = Papierdatei::oeffnen(&konto_pfad, false, &start)?;
    let aus = papiertag(
        j,
        heute,
        &universum()?,
        &start,
        &mut dachs,
        &mut maus,
        &mut konto,
    );
    assert!(
        matches!(aus, Ok(Ausgang::Abgeschlossen { .. })),
        "Ausgang: {aus:?}"
    );
    let a = some(j.stand().toepfe().get(&TopfId::A), "Topf A")?;
    assert!(a.stueck(&sap()?) > 0, "SAP gekauft");
    assert_eq!(dachs.netz().gesendet.len(), 1);
    assert_eq!(maus.netz().gesendet.len(), 2);
    let mit_kosten = j
        .stand()
        .begruendungen()
        .iter()
        .filter(|(_, _, t)| t.contains("Cent reserviert"))
        .count();
    assert_eq!(mit_kosten, 3, "jede Antwort mit Kosten im Journal");
    assert!(!sperre.exists());
    Ok(())
}
