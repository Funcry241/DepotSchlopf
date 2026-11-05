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
        warten: None,
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
    )
    .mit_warten(|_| {}))
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
        warten: None,
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
    assert_eq!(
        ohne.warum_nicht().as_deref(),
        Some("kein Schlüssel hinterlegt")
    );
    let gut = berater(&ort, BeraterId::Dachs, Attrappe::default())?;
    assert_eq!(gut.warum_nicht(), None);
    vorbedingung(fs::write(ort.sperre(), "geprueft?"), "Sperrdatei")?;
    let mit = berater(&ort, BeraterId::Dachs, Attrappe::default())?;
    assert!(!mit.angebunden());
    assert!(
        mit.warum_nicht().is_some_and(|g| g.contains("ki.halt")),
        "{:?}",
        mit.warum_nicht()
    );
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
    )
    .mit_warten(|_| {});
    let mut maus = KiBerater::neu(
        BeraterId::Maus,
        schluessel()?,
        netz_maus,
        universum()?,
        sperre.clone(),
    )
    .mit_warten(|_| {});
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
fn reservierung_ist_eine_gesicherte_obergrenze() -> Result<(), String> {
    let mut ort = Ort::neu()?;
    let mut dachs = berater(&ort, BeraterId::Dachs, Attrappe::default())?;
    let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
    let max = e.deckel().max_token_ein();
    let lang = vec!["x".repeat(usize::try_from(max).map_err(|e| e.to_string())?)];
    let topf = vorbedingung(Topf::neu(TopfId::A, Cent(500_000)), "Topf")?;
    let kurse = kurse()?;
    let lage = Lage {
        datum: tag()?,
        topf: &topf,
        kurse: &kurse,
        ereignisse: &lang,
    };
    let a = dachs.vorschlagen(e, &lage);
    assert!(a.vorschlag.is_none());
    assert!(a.einschaetzung.contains("nicht gesendet"), "{a:?}");
    assert!(dachs.netz().gesendet.is_empty(), "nichts gesendet");
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

fn uhr() -> Option<Zeitpunkt> {
    Zeitpunkt::aus_sekunden(1_793_980_000)
}

fn abgelehnt(status: u16, inhalt: &str, warten: Option<u64>) -> Result<Rueckmeldung, NetzFehler> {
    Ok(Rueckmeldung {
        status,
        inhalt: inhalt.as_bytes().to_vec(),
        warten,
    })
}

#[test]
fn jeder_aufruf_steht_im_anbieterprotokoll() -> Result<(), String> {
    use depot_journal::anbieter::{Ergebnis, Kosten, Wer, lies_alle};
    let ablage = Ort::neu()?;
    let protokoll = ablage.dir.join("anbieter.txt");
    let antworten = vec![
        claude(VORSCHLAG, 20_000, 1_500, 0),
        abgelehnt(400, "Your credit balance is too low", None),
        abgelehnt(
            401,
            &format!("{{\"error\":\"invalid x-api-key {SCHLUESSEL}\"}}"),
            None,
        ),
        Err(NetzFehler::NichtGesendet("host not found".into())),
        // Zuletzt: Nach einem Limit pausiert der Berater (eigener Test unten).
        abgelehnt(429, "{\"type\":\"rate_limit_error\"}", Some(90)),
    ];
    let mut texte = Vec::new();
    for antwort in antworten {
        // Je Aufruf ein eigenes Journal: Das Tageskontingent gilt hier nicht.
        let mut ort = Ort::neu()?;
        let mut netz = Attrappe::default();
        netz.antworten.push_back(antwort);
        let mut dachs =
            berater(&ort, BeraterId::Dachs, netz)?.mit_protokoll(protokoll.clone(), uhr);
        let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
        texte.push(mit_lage(|l| dachs.vorschlagen(e, l))?.einschaetzung);
        assert!(!ort.sperre().exists());
    }
    let text = vorbedingung(fs::read_to_string(&protokoll), "Protokoll")?;
    assert!(!text.contains(SCHLUESSEL));
    let (e, schlecht) = lies_alle(&text);
    assert_eq!(schlecht, 0);
    let art: Vec<Ergebnis> = e.iter().map(|x| x.ergebnis).collect();
    assert_eq!(
        art,
        vec![
            Ergebnis::Ok,
            Ergebnis::Guthaben,
            Ergebnis::Abgelehnt,
            // Sicher nicht gesendet: dreimal versucht, jedes Mal ohne Kosten.
            Ergebnis::Netz,
            Ergebnis::Netz,
            Ergebnis::Netz,
            Ergebnis::Limit
        ]
    );
    assert!(
        e.iter()
            .all(|x| x.wer == Wer::Dachs && x.reserviert.is_some())
    );
    assert!(matches!(e.first().map(|x| x.ist), Some(Kosten::Betrag(_))));
    let t = some(uhr(), "Uhr")?;
    assert_eq!(
        e.get(6).and_then(|x| x.warten_bis),
        Zeitpunkt::aus_sekunden(t.sekunden().saturating_add(90))
    );
    // Die Meldungen sagen in Worten, was los ist.
    assert!(
        texte
            .get(4)
            .is_some_and(|t| t.contains("Anbieterlimit erreicht")),
        "{texte:?}"
    );
    assert!(
        texte
            .get(1)
            .is_some_and(|t| t.contains("Guthaben beim Anbieter fehlt"))
    );
    assert!(
        texte
            .get(2)
            .is_some_and(|t| t.contains("lehnt den Schluessel ab"))
    );
    Ok(())
}

#[test]
fn fehlende_antwort_steht_als_unklar_im_protokoll() -> Result<(), String> {
    use depot_journal::anbieter::{Ergebnis, Kosten, lies_alle};
    let mut ort = Ort::neu()?;
    let protokoll = ort.dir.join("anbieter.txt");
    let mut netz = Attrappe::default();
    netz.antworten
        .push_back(Err(NetzFehler::Unklar("timeout".into())));
    let mut maus = berater(&ort, BeraterId::Maus, netz)?.mit_protokoll(protokoll.clone(), uhr);
    let e = ort.erlaubnis(BeraterId::Maus, 1)?;
    let _ = mit_lage(|l| maus.vorschlagen(e, l))?;
    assert!(ort.sperre().exists());
    let (e, _) = lies_alle(&vorbedingung(fs::read_to_string(&protokoll), "Protokoll")?);
    assert_eq!(
        e.iter().map(|x| (x.ergebnis, x.ist)).collect::<Vec<_>>(),
        vec![(Ergebnis::Unklar, Kosten::Unbekannt)]
    );
    Ok(())
}

/// Maus' Befund zu #31: Eine angezeigte Pause muss auch eingehalten werden. Nennt der Anbieter
/// eine Wartezeit (hier 48 Stunden), wird bis dahin nichts gesendet und nichts reserviert.
#[test]
fn pause_des_anbieters_wird_vor_dem_aufruf_eingehalten() -> Result<(), String> {
    use depot_journal::anbieter::{Eintrag, Ergebnis, Wer, anhaengen};
    let mut ort = Ort::neu()?;
    let protokoll = ort.dir.join("anbieter.txt");
    let jetzt = some(uhr(), "Uhr")?;
    let mut limit = Eintrag::neu(
        some(
            Zeitpunkt::aus_sekunden(jetzt.sekunden().saturating_sub(3600)),
            "Zeit",
        )?,
        Wer::Dachs,
        Ergebnis::Limit,
        Some(429),
    );
    limit.warten_bis = Zeitpunkt::aus_sekunden(jetzt.sekunden().saturating_add(48 * 3600));
    anhaengen(&protokoll, &limit)?;
    let mut netz = Attrappe::default();
    netz.antworten
        .push_back(claude(VORSCHLAG, 20_000, 1_500, 0));
    let mut dachs = berater(&ort, BeraterId::Dachs, netz)?.mit_protokoll(protokoll.clone(), uhr);
    // Nicht angebunden: Der Papiertag reserviert fuer ihn nichts.
    assert!(!dachs.angebunden());
    assert!(dachs.pause().is_some());
    assert!(
        dachs
            .warum_nicht()
            .is_some_and(|g| g.starts_with("Anbieterpause")),
        "{:?}",
        dachs.warum_nicht()
    );
    // Auch mit Erlaubnis wird nichts gesendet.
    let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
    let a = mit_lage(|l| dachs.vorschlagen(e, l))?;
    assert!(a.einschaetzung.contains("pausiert"), "{a:?}");
    assert!(dachs.netz().gesendet.is_empty(), "nichts gesendet");
    // Maus betrifft Dachs' Limit nicht.
    let maus =
        berater(&ort, BeraterId::Maus, Attrappe::default())?.mit_protokoll(protokoll.clone(), uhr);
    assert!(maus.pause().is_none());
    // Nach der Wartezeit, oder nach einem erfolgreichen Aufruf, geht es weiter.
    let mut ok = Eintrag::neu(jetzt, Wer::Dachs, Ergebnis::Ok, Some(200));
    ok.reserviert = Some(Cent(1));
    anhaengen(&protokoll, &ok)?;
    assert!(dachs.pause().is_none());
    Ok(())
}

/// Ein Limit in diesem Lauf pausiert bis zum Ende des Laufs, auch ohne Protokoll.
#[test]
fn limit_im_lauf_pausiert_auch_ohne_protokoll() -> Result<(), String> {
    let mut ort = Ort::neu()?;
    let mut netz = Attrappe::default();
    netz.antworten
        .push_back(abgelehnt(429, "{\"type\":\"rate_limit_error\"}", None));
    let mut maus = berater(&ort, BeraterId::Maus, netz)?;
    assert!(maus.angebunden());
    let e = ort.erlaubnis(BeraterId::Maus, 1)?;
    let _ = mit_lage(|l| maus.vorschlagen(e, l))?;
    assert!(!maus.angebunden());
    assert!(maus.pause().is_some());
    Ok(())
}

/// Maus zu KI-Aufrufen (05.11.2026): Hat die Anfrage den Anbieter sicher nicht erreicht, wird sie
/// unter derselben Reservierung erneut gesendet, hoechstens dreimal. Sie kostet nichts, und
/// hoechstens eine Anfrage je Reservierung kann den Anbieter erreichen.
#[test]
fn sicher_nicht_gesendet_wird_unter_derselben_reservierung_wiederholt() -> Result<(), String> {
    let mut ort = Ort::neu()?;
    let mut netz = Attrappe::default();
    netz.antworten
        .push_back(Err(NetzFehler::NichtGesendet("host not found".into())));
    netz.antworten
        .push_back(Err(NetzFehler::NichtGesendet("connection failed".into())));
    netz.antworten
        .push_back(claude(VORSCHLAG, 20_000, 1_500, 0));
    let mut dachs = berater(&ort, BeraterId::Dachs, netz)?;
    let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
    let a = mit_lage(|l| dachs.vorschlagen(e, l))?;
    assert!(a.verwertbar && a.vorschlag.is_some(), "{a:?}");
    assert!(!ort.sperre().exists());
    let gesendet = &dachs.netz().gesendet;
    assert_eq!(gesendet.len(), 3);
    assert!(
        gesendet.windows(2).all(|w| w.first() == w.last()),
        "dieselbe Anfrage"
    );
    Ok(())
}

#[test]
fn sperre_waehrend_des_wartens_verhindert_neuen_versand() -> Result<(), String> {
    thread_local! {
        static SPERRE: std::cell::RefCell<Option<std::path::PathBuf>> = const { std::cell::RefCell::new(None) };
    }
    let mut ort = Ort::neu()?;
    SPERRE.with(|s| s.replace(Some(ort.sperre())));
    let mut dachs = berater(&ort, BeraterId::Dachs, Attrappe::default())?.mit_warten(|_| {
        SPERRE.with(|s| {
            if let Some(pfad) = s.take() {
                assert!(fs::write(pfad, "Kosten unbekannt").is_ok());
            }
        });
    });
    let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
    let a = mit_lage(|l| dachs.vorschlagen(e, l))?;
    assert!(a.einschaetzung.contains("KI gesperrt"), "{a:?}");
    assert_eq!(dachs.netz().gesendet.len(), 1);
    Ok(())
}

/// Nach drei Versuchen ohne Verbindung: kein Vorschlag, keine Sperre, nichts gekostet.
#[test]
fn ohne_verbindung_nach_drei_versuchen_aufgegeben() -> Result<(), String> {
    let mut ort = Ort::neu()?;
    let mut dachs = berater(&ort, BeraterId::Dachs, Attrappe::default())?;
    let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
    let a = mit_lage(|l| dachs.vorschlagen(e, l))?;
    assert!(!a.verwertbar && a.vorschlag.is_none(), "{a:?}");
    assert!(a.einschaetzung.contains("nicht gesendet"), "{a:?}");
    assert!(!ort.sperre().exists());
    assert_eq!(dachs.netz().gesendet.len(), 3);
    Ok(())
}

/// Abnahmefall 3: Verbindung nach dem Absenden einer KI-Anfrage kappen. Die Anfrage kann
/// angekommen sein und gekostet haben: kein zweiter Versand, Kostensperre fuer beide KIs, und
/// die Sperrdatei sagt in Worten, warum.
#[test]
fn verbindung_nach_dem_absenden_gekappt_sperrt_ohne_zweiten_versand() -> Result<(), String> {
    let mut ort = Ort::neu()?;
    let mut netz = Attrappe::default();
    netz.antworten
        .push_back(Err(NetzFehler::Unklar("connection reset by peer".into())));
    netz.antworten
        .push_back(claude(VORSCHLAG, 20_000, 1_500, 0));
    let mut dachs = berater(&ort, BeraterId::Dachs, netz)?;
    let e = ort.erlaubnis(BeraterId::Dachs, 1)?;
    let a = mit_lage(|l| dachs.vorschlagen(e, l))?;
    assert!(a.vorschlag.is_none(), "{a:?}");
    assert_eq!(dachs.netz().gesendet.len(), 1, "nie ein zweiter Versand");
    assert_eq!(dachs.netz().antworten.len(), 1);
    let text = vorbedingung(fs::read_to_string(ort.sperre()), "Sperrdatei")?;
    assert!(text.contains("nach dem Absenden abgebrochen"), "{text}");
    assert!(text.contains("kann angekommen sein"), "{text}");
    // Auch der andere Berater ist gesperrt.
    let maus = berater(&ort, BeraterId::Maus, Attrappe::default())?;
    assert!(!maus.angebunden());
    Ok(())
}
