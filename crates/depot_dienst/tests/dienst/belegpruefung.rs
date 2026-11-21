use super::*;
use depot_journal::{
    Journal, KiAufruf, KiAufrufErgebnis, KiZweck, Lesecache, NachrichtBeleg,
    gespraeche::{Gespraech, anhaengen},
    neurofrage::{NeuroAuftragId, NeuroErgebnis, NeuroRunde, NeuroUrteil, NeuroUrteilswert},
};
use depot_kern::{Cent, modelle::BeraterEinstellung};
use depot_lage::blindpruefung::prueffaelle_mit_zeilen;

const PFAD: &str = "/neuroschlopf/belegpruefung";
const TAG: &str = "2026-11-05";

fn beispiel(wurzel: &Path) -> Result<Zeitpunkt, String> {
    let fehler = |e| format!("{e:?}");
    let zeit = donnerstag(9, 0)?;
    let tag = some(zeit.frankfurt_tag(), "Prüftag")?;
    let vorher = some(tag.plus_tage(-1), "Vortag")?;
    let pfad = wurzel.join("daten/journal.txt");
    Journal::einrichten(&pfad).map_err(fehler)?;
    let mut j = Journal::oeffnen(&pfad).map_err(fehler)?;
    j.begruende(
        vorher,
        "Maus",
        "Topf B: DE0007164600 2000 bp: [Meldung] [Ausstieg +400bp 5] Das SAP-Cloudgeschäft wächst.",
    )
    .map_err(fehler)?;
    j.schliesse_ab(vorher, Cent(1000), &j.bestaetigt().1)
        .map_err(fehler)?;
    let publiziert = donnerstag(8, 0)?;
    let titel = [
        "Institut meldet <Vorabzahlen>",
        "Institut meldet <Jahreszahlen>",
        "Ungezeigter dritter Quellenbeleg",
    ];
    let kandidaten = titel.map(|t| format!("Nachricht 05.11.2026 07:00 UTC (testquelle): {t}"));
    for text in titel {
        let beleg = NachrichtBeleg::neu("testquelle", zeit, b"Testquelle", publiziert, text)?;
        j.belege_nachricht(&beleg).map_err(fehler)?;
    }
    j.belege_wissensstichtag(tag, zeit).map_err(fehler)?;
    for beleg in &kandidaten {
        j.beobachte(tag, beleg).map_err(fehler)?;
    }
    j.begruende(tag, "Frageliste", "0 3").map_err(fehler)?;
    j.begruende(
        vorher,
        "Maus",
        "Topf B: DE0007164600 2000 bp: Nachtrag mit späterem Kenntnisstand.",
    )
    .map_err(fehler)?;
    let nr = j
        .reserviere_fuer(BeraterId::Maus, tag, 1, KiZweck::Erstmeinung)
        .map_err(|e| format!("{e:?}"))?
        .buchungsnummer();
    let [erster, zweiter, _] = kandidaten;
    let frage = format!(
        "Datum: {tag}\nBelegte Termine und Nachrichten:\n- Ereignis 1: {erster}\n- Ereignis 2: {zweiter}\n"
    );
    let aufruf = Gespraech {
        zeit,
        lauf: "Prüfung".into(),
        wer: "maus".into(),
        rolle: "haupt".into(),
        modell: "Test".into(),
        zweck: "erstmeinung".into(),
        ergebnis: "ok".into(),
        http: Some(200),
        token_ein: None,
        token_aus: None,
        cache: None,
        buchung: Some(nr),
        schlopfmodell: None,
        kosten_cent: None,
        system: None,
        frage: Some(frage),
        antwort: None,
        fehler: None,
    };
    anhaengen(&wurzel.join("daten"), &aufruf).map_err(|e| e.to_string())?;
    Ok(zeit)
}

fn paar_id(seite: &str) -> Result<&str, String> {
    let (_, rest) = some(seite.split_once("name=\"id\" value=\""), "Prüfformular")?;
    let (id, _) = some(rest.split_once('"'), "Paar-ID")?;
    assert_eq!(id.len(), 64);
    assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
    Ok(id)
}

fn sende(d: &Dienst, zeit: Zeitpunkt, felder: &str) -> Result<u16, String> {
    let post = formular_post(PFAD, &format!("t={TOKEN}&{felder}"));
    status(&behandle(d, &Attrappe::default(), &post, zeit))
}

#[test]
fn unabhaengiges_urteil_bleibt_nach_neustart_einmalig_erhalten() -> Result<(), String> {
    let wurzel = ordner()?;
    let zeit = beispiel(&wurzel)?;
    let journal = wurzel.join("daten/journal.txt");
    let original = fs::read(&journal).map_err(|e| e.to_string())?;
    let d = dienst(wurzel.clone(), true);
    let anfang = get(&d, PFAD, zeit);
    assert!(anfang.contains("Bearbeitet <b>0 von 2</b>"));
    assert!(anfang.contains("data-blindentscheidung"));
    assert!(anfang.contains("Das SAP-Cloudgeschäft wächst."));
    for urteil in ["0", "1", "2-stützt", "2-widerlegt", "2-offen", "unbewertet"] {
        assert!(anfang.contains(&format!("name=\"urteil\" value=\"{urteil}\"")));
    }
    assert!(anfang.contains("testquelle") && anfang.contains("Institut meldet &lt;"));
    for verbergen in [
        "Nachtrag mit",
        "Ungezeigter dritter Quellenbeleg",
        "Grundrang",
        "Topf B:",
        "Frageliste",
        "name=\"grundrang\"",
    ] {
        assert!(!anfang.contains(verbergen), "{verbergen}");
    }
    let id = paar_id(&anfang)?;
    assert!(anfang.contains(&format!("name=\"tag\" value=\"{TAG}\"")));
    let felder = format!("id={id}&tag={TAG}&urteil=2-st%C3%BCtzt");
    let post = formular_post(PFAD, &format!("t={TOKEN}&{felder}"));
    let antwort = behandle(&d, &Attrappe::default(), &post, zeit);
    assert_eq!(status(&antwort)?, 303);
    assert!(text(&antwort).contains(&format!("Location: {PFAD}?tag={TAG}&gespeichert=ja\r\n")));
    drop(d);
    let d = dienst(wurzel, true);
    let tagseite = format!("{PFAD}?tag={TAG}");
    let danach = get(&d, &tagseite, zeit);
    assert!(danach.contains("Bearbeitet <b>1 von 2</b>"));
    let zweite = paar_id(&danach)?;
    assert_ne!(id, zweite);
    assert_eq!(sende(&d, zeit, &felder)?, 303);
    assert_eq!(
        sende(&d, zeit, &format!("id={id}&tag={TAG}&urteil=2-widerlegt"))?,
        409
    );
    assert_eq!(get(&d, &tagseite, zeit), danach);
    assert_eq!(
        sende(
            &d,
            zeit,
            &format!("id={zweite}&tag={TAG}&urteil=unbewertet")
        )?,
        303
    );
    let ende = get(&d, PFAD, zeit);
    assert!(ende.contains("Bearbeitet <b>2 von 2</b>") && ende.contains("ausdrücklich unbewertet"));
    assert!(ende.contains("Keine Bewertung offen."));
    assert!(!ende.contains("name=\"id\""));
    assert!(!ende.contains("data-blindentscheidung"));
    assert_eq!(fs::read(journal).map_err(|e| e.to_string())?, original);
    assert!(wartend(&d).is_empty());
    Ok(())
}

fn belege_kiurteil(
    pfad: &Path,
    tag: Datum,
    id: NeuroAuftragId,
    wer: BeraterId,
    wert: NeuroUrteilswert,
    nachfrage: Option<String>,
) -> Result<(), String> {
    let mut j = Journal::oeffnen(pfad).map_err(|e| format!("{e:?}"))?;
    let erlaubnis = j
        .reserviere_neurofrage(&id, wer, tag, 1, BeraterEinstellung::standard(wer), 8)
        .map_err(|e| format!("{e:?}"))?;
    let nr = erlaubnis.buchung();
    let ergebnis = NeuroErgebnis::neu(
        id,
        wer,
        nr,
        Ok(NeuroUrteil {
            wert,
            begruendung: "Dieser KI-Text bleibt bei der Blindprüfung ausgeblendet.".into(),
            nachfrage,
        }),
        Some(KiAufruf {
            nr,
            anbieter: "Testanbieter".into(),
            modell: "Testmodell".into(),
            vorlage: "a".repeat(64),
            frage: Some("b".repeat(64)),
            antwort: Some("c".repeat(64)),
            token_ein: Some(10),
            token_aus: Some(20),
            ergebnis: KiAufrufErgebnis::Beantwortet,
        }),
        erlaubnis.aufruf().abrechnung(20, 10),
    );
    j.belege_neuroergebnis(&ergebnis)
        .map_err(|e| format!("{e:?}"))
}

#[test]
fn nur_klare_doppelbewertung_ausserhalb_der_stichprobe_entfaellt() -> Result<(), String> {
    use NeuroUrteilswert::{KeinBezug, Schwaecht, Staerkt};

    let wurzel = ordner()?;
    let zeit = beispiel(&wurzel)?;
    let tag = some(zeit.frankfurt_tag(), "Prüftag")?;
    let daten = wurzel.join("daten");
    let pfad = daten.join("journal.txt");
    let mut j = Journal::oeffnen(&pfad).map_err(|e| format!("{e:?}"))?;
    j.schliesse_ab(tag, Cent(1000), &j.bestaetigt().1)
        .map_err(|e| format!("{e:?}"))?;
    drop(j);
    let mut cache = Lesecache::default();
    let (stand, zeilen) = cache
        .lesen(&pfad, 32 * 1024 * 1024)
        .map_err(|e| format!("{e:?}"))?;
    let (gespraeche, 0) = some(
        depot_journal::gespraeche::lesen_begrenzt(&daten, tag, &mut (8 * 1024 * 1024))
            .map_err(|e| format!("{e:?}"))?,
        "Bisherige Erstmeinung",
    )?
    else {
        return Err("Erstmeinung unlesbar".into());
    };
    let faelle = prueffaelle_mit_zeilen(stand, zeilen, &gespraeche, tag)?;
    let [stichprobe, anderer] = faelle.as_slice() else {
        return Err("Zwei historische Paare erwartet".into());
    };
    let erste = some(
        stichprobe.vorpruefung(BeraterId::Maus, NeuroRunde::Erstfrage, None),
        "Erste Vorprüfung",
    )?;
    let zweite = some(
        anderer.vorpruefung(BeraterId::Maus, NeuroRunde::Erstfrage, None),
        "Zweite Vorprüfung",
    )?;
    assert_ne!(erste.beleg_zeile, zweite.beleg_zeile);
    let dachs = some(
        stichprobe.vorpruefung(BeraterId::Dachs, NeuroRunde::Erstfrage, None),
        "Dieselbe Vorprüfung für Dachs",
    )?;
    assert_eq!(
        erste,
        depot_journal::neurofrage::NeuroFrage {
            wer: BeraterId::Maus,
            ..dachs.clone()
        }
    );
    let sammlung = daten.join("neuroschlopf-fragen");
    fs::create_dir(&sammlung).map_err(|e| e.to_string())?;
    for (frage, empfaenger) in [(&erste, "maus"), (&dachs, "dachs")] {
        let dateiname = format!("{}-{empfaenger}-erstfrage.frage.json", frage.id.fall());
        fs::write(sammlung.join(dateiname), frage.text()).map_err(|e| e.to_string())?;
    }

    let d = dienst(wurzel, true);
    let urteil =
        |id: &NeuroAuftragId, wer, wert| belege_kiurteil(&pfad, tag, id.clone(), wer, wert, None);
    urteil(&erste.id, BeraterId::Maus, KeinBezug)?;
    let einzeln = get(&d, "/neuroschlopf", zeit);
    assert!(einzeln.contains("Einzelbewertung · Kein Bezug zur These"));
    assert!(!einzeln.contains("stimmen überein"));
    urteil(&erste.id, BeraterId::Dachs, KeinBezug)?;
    assert!(
        get(&d, "/neuroschlopf", zeit).contains("KI-Bewertung · Dachs und Maus stimmen überein")
    );
    urteil(&zweite.id, BeraterId::Maus, Staerkt)?;
    let rueckfrage = "Ist die Richtung des Hinweises klar?".to_owned();
    belege_kiurteil(
        &pfad,
        tag,
        zweite.id.clone(),
        BeraterId::Dachs,
        Schwaecht,
        Some(rueckfrage),
    )?;

    let blind = get(&d, PFAD, zeit);
    assert!(blind.contains("Bearbeitet <b>0 von 2</b>"));
    assert_eq!(paar_id(&blind)?, erste.id.fall());
    assert!(!blind.contains("Dieser KI-Text"));
    assert!(!blind.contains("stimmen überein"));
    assert_eq!(
        sende(
            &d,
            zeit,
            &format!("id={}&tag={TAG}&urteil=1", erste.id.fall())
        )?,
        303
    );
    assert_eq!(paar_id(&get(&d, PFAD, zeit))?, zweite.id.fall());

    let id = some(
        NeuroAuftragId::neu(zweite.id.fall(), NeuroRunde::Rueckfrage),
        "Rückfrage",
    )?;
    urteil(&id, BeraterId::Dachs, Staerkt)?;
    let ende = get(&d, PFAD, zeit);
    assert!(ende.contains("Keine Bewertung offen."));
    assert!(!ende.contains(r#"name="id""#));
    let protokoll =
        fs::read_to_string(daten.join("neuro2026-belegurteile.tsv")).map_err(|e| e.to_string())?;
    assert!(protokoll.contains(erste.id.fall()));
    assert!(!protokoll.contains(zweite.id.fall()));
    Ok(())
}

#[test]
fn nur_belegte_paare_und_gueltige_urteile_mit_token_werden_angenommen() -> Result<(), String> {
    let wurzel = ordner()?;
    let zeit = beispiel(&wurzel)?;
    let d = dienst(wurzel, true);
    let vorher = get(&d, PFAD, zeit);
    let id = paar_id(&vorher)?;
    let fremd = "0".repeat(64);
    assert_ne!(id, fremd);
    assert_eq!(
        sende(&d, zeit, &format!("id={fremd}&tag={TAG}&urteil=2-offen"))?,
        409
    );
    let anderer_tag = format!("id={id}&tag=2026-11-04&urteil=0");
    assert_eq!(sende(&d, zeit, &anderer_tag)?, 409);
    for falsch in [
        format!("id=xyz&tag={TAG}&urteil=2-offen"),
        format!("id={id}&tag={TAG}&urteil=3"),
        format!("id={id}&tag={TAG}&urteil=2"),
        format!("id={id}&tag=31.02.2026&urteil=unbewertet"),
        format!("id={id}&tag={TAG}&urteil=0&urteil=2-offen"),
    ] {
        assert_eq!(sende(&d, zeit, &falsch)?, 400);
    }
    let ohne_token = formular_post(PFAD, &format!("id={id}&tag={TAG}&urteil=0"));
    assert_eq!(
        status(&behandle(&d, &Attrappe::default(), &ohne_token, zeit))?,
        403
    );
    assert_eq!(get(&d, PFAD, zeit), vorher);
    Ok(())
}
