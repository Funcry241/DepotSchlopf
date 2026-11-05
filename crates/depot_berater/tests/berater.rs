#![allow(
    clippy::unnecessary_wraps,
    reason = "Testcode: Antworten der Attrappe haben die Form der Netzantwort"
)]

//! Dachs und Maus mit einem erfundenen Anbieter: Anfrage, Antwort, Kostenpruefung, Sperre.
//! Kein Netz; alle Antworten sind fuer den Test geschrieben.

use std::collections::VecDeque;
use std::fmt::Write as _;
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

/// Eine Antwort von Claude mit Text, Token und Suchen (erlaubt sind keine).
fn claude(text: &str, ein: u64, aus: u64, suchen: u64) -> Result<Rueckmeldung, NetzFehler> {
    ok(&format!(
        concat!(
            "{{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[",
            "{{\"type\":\"thinking\",\"thinking\":\"\",\"signature\":\"x\"}},",
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

/// Eine Antwort von GPT mit Text und Token, ohne Suche.
fn gpt(text: &str, ein: u64, aus: u64) -> Result<Rueckmeldung, NetzFehler> {
    ok(&format!(
        concat!(
            "{{\"id\":\"resp_1\",\"status\":\"completed\",\"output\":[",
            "{{\"type\":\"reasoning\",\"id\":\"rs_1\",\"summary\":[]}},",
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
        .push_back(claude(VORSCHLAG, 20_000, 1_500, 0));
    let mut dachs = berater(&ort, BeraterId::Dachs, netz)?;
    assert!(dachs.angebunden());
    let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
    let antwort = mit_lage(|l| dachs.vorschlagen(e, l))?;
    let v = some(antwort.vorschlag.clone(), "Vorschlag")?;
    assert_eq!(v.berater, BeraterId::Dachs);
    assert_eq!(v.zielgewichte_bp.get(&sap()?), Some(&3_000));
    // 20.000 x 2 $ + 1.500 x 10 $ je Mio. = 5,5 Cent, aufgerundet 6.
    assert!(
        antwort
            .einschaetzung
            .contains("Kosten 6 von 24 Cent reserviert"),
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
    assert!(!inhalt.contains("tools"), "keine Werkzeuge: {inhalt}");
    for teil in [
        "\"model\":\"claude-sonnet-5-5\"",
        "\"max_tokens\":16000",
        "\"thinking\":{\"type\":\"adaptive\"}",
        "\"effort\":\"medium\"",
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
    // 10.000 x 1 $ + 800 x 5 $ je Mio. = 1,4 Cent, aufgerundet 2.
    assert!(u.begruendung.contains("Kosten 2 von 12 Cent"), "{u:?}");
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
    assert!(!inhalt.contains("tools"), "keine Werkzeuge: {inhalt}");
    for teil in [
        "\"model\":\"gpt-6.1-terra\"",
        "\"max_output_tokens\":16000",
        "\"reasoning\":{\"effort\":\"medium\"}",
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
    // 200.000 x 2 $ + 1.500 x 10 $ = 41,5 Cent, aufgerundet 42 > 24 reserviert.
    netz.antworten
        .push_back(claude(VORSCHLAG, 200_000, 1_500, 0));
    let mut dachs = berater(&ort, BeraterId::Dachs, netz)?;
    let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
    let antwort = mit_lage(|l| dachs.vorschlagen(e, l))?;
    assert!(antwort.vorschlag.is_none(), "{antwort:?}");
    assert!(
        antwort.einschaetzung.contains("42 Cent ueber 24"),
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
fn jede_suche_sperrt() -> Result<(), String> {
    let mut ort = Ort::neu()?;
    let mut netz = Attrappe::default();
    netz.antworten.push_back(claude(VORSCHLAG, 1_000, 100, 1));
    let mut dachs = berater(&ort, BeraterId::Dachs, netz)?;
    let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
    let antwort = mit_lage(|l| dachs.vorschlagen(e, l))?;
    assert!(antwort.vorschlag.is_none());
    assert!(
        antwort.einschaetzung.contains("1 Suchen, keine erlaubt"),
        "{antwort:?}"
    );
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
        .push_back(claude(VORSCHLAG, 20_000, 1_500, 0));
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

/// Maus' Gegenprobe zu #25: Der Anbieter gibt den Schluessel in einer erfolgreichen Antwort
/// zurueck. Nichts davon gelangt ins Ergebnis, weder roh noch verschluesselt im JSON des
/// Modells, weder in der Einschaetzung noch in einer Begruendung noch in einem Urteil.
#[test]
fn zurueckgegebener_schluessel_gelangt_nie_ins_ergebnis() -> Result<(), String> {
    let maskiert = SCHLUESSEL.chars().fold(String::new(), |mut m, c| {
        let _ = write!(m, "\\u{:04x}", u32::from(c));
        m
    });
    let vorschlaege = [
        format!("{{\"einschaetzung\": \"Schluessel {SCHLUESSEL}\", \"ziele\": null}}"),
        format!(
            "{{\"einschaetzung\": \"x\", \"ziele\": [{{\"isin\": \"DE0007164600\", \"gewicht_bp\": 1000, \"begruendung\": \"{maskiert}\"}}]}}"
        ),
    ];
    for text in &vorschlaege {
        let mut ort = Ort::neu()?;
        let mut netz = Attrappe::default();
        netz.antworten.push_back(claude(text, 1_000, 100, 0));
        let mut dachs = berater(&ort, BeraterId::Dachs, netz)?;
        let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
        let a = mit_lage(|l| dachs.vorschlagen(e, l))?;
        assert!(a.vorschlag.is_none(), "{a:?}");
        assert!(!format!("{a:?}").contains(SCHLUESSEL), "{a:?}");
        assert!(
            a.einschaetzung.contains("Schluessel des Anbieters"),
            "{a:?}"
        );
    }
    let mut ort = Ort::neu()?;
    let mut netz = Attrappe::default();
    netz.antworten.push_back(gpt(
        &format!("{{\"zustimmung\": true, \"begruendung\": \"{maskiert}\"}}"),
        1_000,
        100,
    ));
    let mut maus = berater(&ort, BeraterId::Maus, netz)?;
    let v = Vorschlag {
        berater: BeraterId::Dachs,
        zielgewichte_bp: [(sap()?, 1_000)].into_iter().collect(),
        begruendung: [(sap()?, "x".to_string())].into_iter().collect(),
    };
    let e = ort.erlaubnis(BeraterId::Maus, 1)?;
    let u = mit_lage(|l| maus.pruefen(e, l, &v))?;
    assert!(!u.zustimmung, "{u:?}");
    assert!(!format!("{u:?}").contains(SCHLUESSEL), "{u:?}");
    Ok(())
}

/// Maus' Gegenprobe zu #25 (K25-01): Die Reservierung muss eine gesicherte Obergrenze sein.
/// Ohne Werkzeuge fuegt der Anbieter nichts ein; eine Anfrage ueber dem Deckel wird gar nicht
/// gesendet, und meldet der Anbieter trotzdem mehr Eingabe-Token, sperrt der Berater.
#[test]
#[allow(
    clippy::many_single_char_names,
    reason = "kurze Namen wie in den uebrigen Tests"
)]
fn reservierung_ist_eine_gesicherte_obergrenze() -> Result<(), String> {
    // Termine und Nachrichten kuerzt die Frage selbst (`MAX_EREIGNISSE_BYTES`); zu gross wird
    // die Anfrage hier ueber eine ueberlange Begruendung im Vorschlag des anderen Beraters.
    let mut ort = Ort::neu()?;
    let mut maus = berater(&ort, BeraterId::Maus, Attrappe::default())?;
    let e = ort.erlaubnis(BeraterId::Maus, 1)?;
    let max = e.deckel().max_token_ein();
    let lang = "x".repeat(usize::try_from(max).map_err(|e| e.to_string())?);
    let v = Vorschlag {
        berater: BeraterId::Dachs,
        zielgewichte_bp: [(sap()?, 1_000)].into_iter().collect(),
        begruendung: [(sap()?, lang)].into_iter().collect(),
    };
    let u = mit_lage(|l| maus.pruefen(e, l, &v))?;
    assert!(!u.zustimmung);
    assert!(u.begruendung.contains("nicht gesendet"), "{u:?}");
    assert!(maus.netz().gesendet.is_empty(), "nichts gesendet");
    assert!(!ort.sperre().exists());

    // Meldet der Anbieter mehr Eingabe-Token als erlaubt, sperrt der Berater, auch wenn die
    // Kosten noch unter der Reservierung liegen.
    let mut netz = Attrappe::default();
    netz.antworten
        .push_back(claude(VORSCHLAG, u64::from(max).saturating_add(1), 0, 0));
    let mut maus_ort = Ort::neu()?;
    let mut d = berater(&maus_ort, BeraterId::Dachs, netz)?;
    let e = maus_ort.erlaubnis(BeraterId::Dachs, 1)?;
    let a = mit_lage(|l| d.vorschlagen(e, l))?;
    assert!(a.vorschlag.is_none(), "{a:?}");
    assert!(a.einschaetzung.contains("Eingabe-Token"), "{a:?}");
    assert!(maus_ort.sperre().exists());
    Ok(())
}

/// Gegenprobe von Maus (Nachpruefung K25-02): Auch wenn der Text verworfen wird, weil er den
/// Schluessel enthaelt, wird der Verbrauch geprueft. Kosten ueber der Reservierung sperren,
/// ein unlesbarer Verbrauch auch.
#[test]
fn verworfene_antwort_wird_trotzdem_auf_kosten_geprueft() -> Result<(), String> {
    let text = format!("{{\"einschaetzung\": \"{SCHLUESSEL}\", \"ziele\": null}}");
    // Kosten ueber der Reservierung.
    let mut ort = Ort::neu()?;
    let mut netz = Attrappe::default();
    netz.antworten.push_back(claude(&text, 200_000, 1_500, 0));
    let mut dachs = berater(&ort, BeraterId::Dachs, netz)?;
    let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
    let a = mit_lage(|l| dachs.vorschlagen(e, l))?;
    assert!(a.vorschlag.is_none(), "{a:?}");
    assert!(!format!("{a:?}").contains(SCHLUESSEL), "{a:?}");
    assert!(a.einschaetzung.contains("42 Cent ueber 24"), "{a:?}");
    assert!(ort.sperre().exists(), "Sperrdatei geschrieben");
    // Verbrauch unlesbar: sperren.
    let mut ort = Ort::neu()?;
    let mut netz = Attrappe::default();
    netz.antworten.push_back(ok(&format!(
        "{{\"content\":[{{\"type\":\"text\",\"text\":\"{SCHLUESSEL}\"}}],\"stop_reason\":\"end_turn\"}}"
    )));
    let mut dachs = berater(&ort, BeraterId::Dachs, netz)?;
    let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
    let a = mit_lage(|l| dachs.vorschlagen(e, l))?;
    assert!(a.vorschlag.is_none(), "{a:?}");
    assert!(!format!("{a:?}").contains(SCHLUESSEL), "{a:?}");
    assert!(ort.sperre().exists(), "{a:?}");
    // Kein JSON, aber mit Schluessel: Kosten unbekannt, sperren.
    let mut ort = Ort::neu()?;
    let mut netz = Attrappe::default();
    netz.antworten
        .push_back(ok(&format!("kaputt {SCHLUESSEL}")));
    let mut dachs = berater(&ort, BeraterId::Dachs, netz)?;
    let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
    let a = mit_lage(|l| dachs.vorschlagen(e, l))?;
    assert!(!format!("{a:?}").contains(SCHLUESSEL), "{a:?}");
    assert!(ort.sperre().exists(), "{a:?}");
    assert!(
        !fs_enthaelt(&ort.sperre(), SCHLUESSEL),
        "Sperrdatei ohne Schluessel"
    );
    Ok(())
}

fn fs_enthaelt(pfad: &std::path::Path, was: &str) -> bool {
    std::fs::read_to_string(pfad).is_ok_and(|t| t.contains(was))
}

/// Die groesste Lage passt unter den Deckel der Eingabe (40.000 Bytes, K25-01): alle Werte der
/// Beobachtungsliste mit Kurs, vier Positionen, 30 Termine und 30 Nachrichten in der laengsten
/// Form, mit Zeichen aus vier Bytes und Anfuehrungszeichen. Was nicht passt, wird ausgelassen
/// und gezaehlt; gesendet wird trotzdem, und zwar fuer Vorschlag und Zweitmeinung.
#[test]
#[allow(
    clippy::many_single_char_names,
    reason = "kurze Namen wie in den uebrigen Tests"
)]
fn groesste_lage_passt_unter_den_deckel() -> Result<(), String> {
    let werte = vorbedingung(depot_kern::liste::beobachtungsliste(), "Liste")?;
    let universum = depot_kern::liste::als_universum(&werte);
    let d = tag()?;
    let zeit = some(Zeitpunkt::neu(d, 17, 30, 0), "Zeit")?;
    let mut eintraege = Vec::new();
    for w in &werte {
        let k = vorbedingung(
            Kurs::neu(Cent(123_456), "EUR", d, zeit, Herkunft::Quelle),
            "Kurs",
        )?;
        eintraege.push((w.isin.clone(), k));
    }
    let kurse = vorbedingung(Kurse::neu(zeit, eintraege), "Kurse")?;
    let mut topf = vorbedingung(Topf::neu(TopfId::A, Cent(500_000)), "Topf")?;
    for w in werte.iter().take(4) {
        topf.setze_bestand(w.isin.clone(), 1_000_000);
    }
    let lang = "\"𝄞\" ".repeat(200);
    let ereignisse: Vec<String> = (0..30)
        .map(|i| format!("Termin {i:02}: {lang}"))
        .chain((0..30).map(|i| format!("Nachricht {i:02}: {lang}")))
        .collect();
    let lage = Lage {
        datum: d,
        topf: &topf,
        kurse: &kurse,
        ereignisse: &ereignisse,
    };
    let v = Vorschlag {
        berater: BeraterId::Dachs,
        zielgewichte_bp: werte
            .iter()
            .take(4)
            .map(|w| (w.isin.clone(), 2_500))
            .collect(),
        begruendung: werte
            .iter()
            .take(4)
            .map(|w| (w.isin.clone(), lang.clone()))
            .collect(),
    };
    for wer in [BeraterId::Dachs, BeraterId::Maus] {
        let mut ort = Ort::neu()?;
        let mut b = KiBerater::neu(
            wer,
            Some(vorbedingung(Schluessel::neu(SCHLUESSEL), "Schluessel")?),
            Attrappe::default(),
            universum.clone(),
            ort.sperre(),
        );
        let e = ort.erlaubnis(wer, 1)?;
        let max = e.deckel().max_token_ein();
        let a = b.vorschlagen(e, &lage);
        let e = ort.erlaubnis(wer, 1)?;
        let _ = b.pruefen(e, &lage, &v);
        assert_eq!(b.netz().gesendet.len(), 2, "{wer:?}: beide gesendet; {a:?}");
        for (_, _, inhalt) in &b.netz().gesendet {
            assert!(
                inhalt.len() <= usize::try_from(max).unwrap_or(0),
                "{wer:?}: {} Bytes",
                inhalt.len()
            );
            assert!(inhalt.contains("weitere ausgelassen, Platz erschoepft"));
            assert!(inhalt.contains("Termin 00"), "die ersten bleiben");
        }
    }
    Ok(())
}

/// K28-01 fuer den Berater: Steht der Schluessel nur hinter Escapes in der Antwort, etwa in
/// einem Feld, dessen Fehler ihn sonst in die Meldung truege, wird nichts davon verwendet. Die
/// Kosten werden trotzdem geprueft.
#[test]
fn schluessel_hinter_escapes_gelangt_nie_ins_ergebnis() -> Result<(), String> {
    let maskiert = SCHLUESSEL.chars().fold(String::new(), |mut m, c| {
        let _ = write!(m, "\\u{:04x}", u32::from(c));
        m
    });
    let texte = [
        // Ungueltige ISIN: Ihr Fehler naennte sie.
        format!(
            "{{\"einschaetzung\": \"x\", \"ziele\": [{{\"isin\": \"{maskiert}\", \"gewicht_bp\": 1000, \"begruendung\": \"y\"}}]}}"
        ),
        // Unbekanntes Feld, nie gelesen, aber Teil der Antwort.
        format!("{{\"einschaetzung\": \"x\", \"ziele\": null, \"notiz\": \"{maskiert}\"}}"),
        // Ausserhalb des JSON-Objekts.
        format!("Vorab {maskiert}\n{{\"einschaetzung\": \"x\", \"ziele\": null}}"),
    ];
    for text in &texte {
        let mut ort = Ort::neu()?;
        let mut netz = Attrappe::default();
        netz.antworten.push_back(claude(text, 1_000, 100, 0));
        let mut dachs = berater(&ort, BeraterId::Dachs, netz)?;
        let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
        let a = mit_lage(|l| dachs.vorschlagen(e, l))?;
        assert!(a.vorschlag.is_none(), "{a:?}");
        let alles = format!("{a:?}");
        assert!(
            !depot_berater::json::alle_texte(&alles)
                .iter()
                .any(|t| t.contains(SCHLUESSEL)),
            "{a:?}"
        );
        assert!(
            a.einschaetzung.contains("Schluessel des Anbieters"),
            "{a:?}"
        );
        assert!(a.einschaetzung.contains("Kosten 1 von 24"), "{a:?}");
    }
    Ok(())
}
