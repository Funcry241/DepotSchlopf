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
    beispiel_am(
        wurzel,
        some(Datum::lies(TAG), "Prüftag")?,
        &[
            "Institut meldet <Vorabzahlen>",
            "Institut meldet <Jahreszahlen>",
            "Ungezeigter dritter Quellenbeleg",
        ],
        2,
    )
}

#[test]
fn textprobe_bindet_lokale_rangwerte_an_dieselben_belege() -> Result<(), String> {
    use depot_kern::{abdruck::sha256_hex, json};

    let wurzel = ordner()?;
    let zeit = beispiel(&wurzel)?;
    let daten = wurzel.join("daten");
    let tag = some(zeit.frankfurt_tag(), "Prüftag")?;
    let austausch = wurzel.join("textprobe");
    fs::create_dir(&austausch).map_err(|e| e.to_string())?;
    assert!(depot_dienst::neuro_textprobe(&daten, tag, tag, &austausch)?.contains("2 Paare"));
    let eingabe = fs::read(austausch.join("textprobe-eingabe.json")).map_err(|e| e.to_string())?;
    let j = json::lies(std::str::from_utf8(&eingabe).map_err(|e| e.to_string())?)?;
    let faelle = some(j.feld("rows").and_then(json::Json::liste), "Textfälle")?;
    assert_eq!(faelle.len(), 2);
    assert!(faelle.iter().any(|f| {
        f.feld("beleg")
            .and_then(json::Json::text)
            .is_some_and(|b| b.contains("<Vorabzahlen>"))
    }));
    let werte = faelle
        .iter()
        .map(|f| {
            let id = some(f.feld("id").and_then(json::Json::text), "Fall-ID")?;
            Ok(format!("{{\"id\":{},\"score\":0.29}}", json::zitiere(id)))
        })
        .collect::<Result<Vec<_>, String>>()?
        .join(",");
    let antwort = |hash: &str| {
        format!(
            r#"{{"format":"depot-neuro-textprobe-v1","input_sha256":"{hash}","scores":[{werte}]}}"#
        )
    };
    let pfad = austausch.join("textprobe-antwort.json");
    fs::write(&pfad, antwort("ungueltig")).map_err(|e| e.to_string())?;
    assert!(depot_dienst::neuro_textprobe(&daten, tag, tag, &austausch).is_err());
    fs::write(&pfad, antwort(&sha256_hex(&eingabe))).map_err(|e| e.to_string())?;
    let result = depot_dienst::neuro_textprobe(&daten, tag, tag, &austausch)?;
    assert!(
        result.contains("Textähnlichkeit gegen Grundrang") && result.contains("2 geprüfte Paare")
    );
    assert!(result.contains("höchstens drei Belege"));
    assert!(
        depot_dienst::neuro_textprobe_ki(&daten, tag, tag, &austausch)?
            .contains("höchstens drei Belege")
    );
    assert!(!daten.join("neuroschlopf-fragen").exists());
    assert!(!daten.join("neuro2026-beobachtung.json").exists());
    Ok(())
}

#[test]
fn textprobe_merkt_nur_die_vereinigten_top3_fuer_gewahlte_ki_vor() -> Result<(), String> {
    use depot_journal::{ki_einstellungen, neurofrage::NeuroFrage};
    use depot_kern::{
        abdruck::sha256_hex,
        json,
        modelle::{KiEinstellungen, NeuroEmpfaenger},
    };

    let wurzel = ordner()?;
    let tag = some(Datum::lies(TAG), "Prüftag")?;
    let titel = [
        "Beleg A", "Beleg B", "Beleg C", "Beleg D", "Beleg E", "Beleg F", "Beleg G",
    ];
    beispiel_am(&wurzel, tag, &titel, titel.len())?;
    let daten = wurzel.join("daten");
    let austausch = wurzel.join("textprobe");
    fs::create_dir(&austausch).map_err(|e| e.to_string())?;
    let journal = daten.join("journal.txt");
    let mut j = Journal::oeffnen(&journal).map_err(|e| format!("{e:?}"))?;
    j.schliesse_ab(tag, Cent(1000), &j.bestaetigt().1)
        .map_err(|e| format!("{e:?}"))?;
    drop(j);
    assert!(depot_dienst::neuro_textprobe_ki(&daten, tag, tag, &austausch).is_err());
    depot_dienst::neuro_textprobe(&daten, tag, tag, &austausch)?;
    let eingabe =
        fs::read_to_string(austausch.join("textprobe-eingabe.json")).map_err(|e| e.to_string())?;
    let json = json::lies(&eingabe)?;
    let zeilen = some(json.feld("rows").and_then(json::Json::liste), "Textfälle")?;
    textprobe_belegte_urteile(zeilen, &journal, tag)?;
    let vorher = fs::read(&journal).map_err(|e| e.to_string())?;
    let werte = zeilen
        .iter()
        .map(|z| {
            let id = some(z.feld("id").and_then(json::Json::text), "Fall-ID")?;
            let beleg = some(z.feld("beleg").and_then(json::Json::text), "Beleg")?;
            let platz = some(titel.iter().position(|t| beleg.ends_with(t)), "Belegtext")?;
            Ok(format!(
                "{{\"id\":{},\"score\":0.{} }}",
                json::zitiere(id),
                platz.saturating_add(1)
            ))
        })
        .collect::<Result<Vec<_>, String>>()?
        .join(",");
    fs::write(austausch.join("textprobe-antwort.json"), format!(
        "{{\"format\":\"depot-neuro-textprobe-v1\",\"input_sha256\":\"{}\",\"scores\":[{werte}]}}",
        sha256_hex(eingabe.as_bytes()),
    )).map_err(|e| e.to_string())?;
    let fragen = daten.join("neuroschlopf-fragen");
    depot_dienst::neuro_textprobe(&daten, tag, tag, &austausch)?;
    assert!(!fragen.exists());
    assert!(depot_dienst::neuro_textprobe_ki(&daten, tag, tag, &austausch)?.contains("pausiert"));
    let speichern = |neuro_empfaenger| -> Result<(), String> {
        let e = KiEinstellungen {
            neuro_empfaenger,
            neuroschlopf: 1,
            ..KiEinstellungen::default()
        };
        fs::write(
            ki_einstellungen::pfad(&daten),
            ki_einstellungen::als_text(e),
        )
        .map_err(|e| e.to_string())
    };
    fs::write(daten.join("neuroschlopf.txt"), "an\n").map_err(|e| e.to_string())?;
    speichern(NeuroEmpfaenger::Maus)?;
    let vormerken = || depot_dienst::neuro_textprobe_ki(&daten, tag, tag, &austausch);
    let zuerst = vormerken()?;
    assert!(zuerst.contains("2 neue Aufträge für Maus · Tageslimit 1"));
    assert!(zuerst.contains("0/1 Prüfgruppen mit mindestens vier Belegen"));
    let erneut = vormerken()?;
    assert!(erneut.contains("0 neue Aufträge"));
    assert!(erneut.contains("2 bereits vorgemerkt · 1 Antwort oder Rückfrage ausstehend"));
    assert!(erneut.contains("1 abschließend beantwortet · 1 ohne verwertbares Urteil"));
    speichern(NeuroEmpfaenger::Beide)?;
    assert!(vormerken()?.contains("5 neue Aufträge für Beide"));
    let mut anzahl = 0_usize;
    for datei in fs::read_dir(fragen).map_err(|e| e.to_string())? {
        let pfad = datei.map_err(|e| e.to_string())?.path();
        if pfad.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let text = fs::read_to_string(pfad).map_err(|e| e.to_string())?;
        assert!(!text.contains("score") && !text.contains("grundrang"));
        let frage = some(NeuroFrage::lies(&text), "Vorgemerkte Frage")?;
        assert!(frage.ist_vorpruefung());
        assert!(!frage.jetzt.ends_with("Beleg B") && !frage.jetzt.ends_with("Beleg D"));
        assert!(!frage.jetzt.ends_with("Beleg A") || frage.wer == BeraterId::Dachs);
        anzahl = anzahl.saturating_add(1);
    }
    assert_eq!(anzahl, 7);
    assert_eq!(fs::read(journal).map_err(|e| e.to_string())?, vorher);
    Ok(())
}

fn textprobe_belegte_urteile(
    zeilen: &[depot_kern::json::Json],
    journal: &Path,
    tag: Datum,
) -> Result<(), String> {
    use depot_kern::json;

    let kennung = |titel: &str| -> Result<NeuroAuftragId, String> {
        let zeile = some(
            zeilen.iter().find(|z| {
                z.feld("beleg")
                    .and_then(json::Json::text)
                    .is_some_and(|b| b.ends_with(titel))
            }),
            "Belegtext",
        )?;
        let id = some(zeile.feld("id").and_then(json::Json::text), "Fall-ID")?;
        some(
            NeuroAuftragId::neu(id, NeuroRunde::Erstfrage),
            "Auftrags-ID",
        )
    };
    for (titel, wer) in [
        ("Beleg A", BeraterId::Maus),
        ("Beleg B", BeraterId::Maus),
        ("Beleg B", BeraterId::Dachs),
    ] {
        belege_kiurteil(
            journal,
            tag,
            kennung(titel)?,
            wer,
            NeuroUrteilswert::KeinBezug,
            None,
        )?;
    }
    for (titel, nachfrage) in [("Beleg E", None), ("Beleg F", Some("Stand?".into()))] {
        belege_kiurteil(
            journal,
            tag,
            kennung(titel)?,
            BeraterId::Maus,
            NeuroUrteilswert::Offen,
            nachfrage,
        )?;
    }
    Ok(())
}

fn beispiel_am(
    wurzel: &Path,
    tag: Datum,
    titel: &[&str],
    sichtbar: usize,
) -> Result<Zeitpunkt, String> {
    use std::fmt::Write as _;

    let fehler = |e| format!("{e:?}");
    let zeit = some(Zeitpunkt::frankfurt(tag, 9, 0, 0), "Wissensstichtag")?;
    let vorher = some(tag.plus_tage(-1), "Vortag")?;
    let pfad = wurzel.join("daten/journal.txt");
    if !Journal::eingerichtet(&pfad).map_err(fehler)? {
        Journal::einrichten(&pfad).map_err(fehler)?;
    }
    let mut j = Journal::oeffnen(&pfad).map_err(fehler)?;
    j.begruende(
        vorher,
        "Maus",
        "Topf B: DE0007164600 2000 bp: [Meldung] [Ausstieg +400bp 5] Das SAP-Cloudgeschäft wächst.",
    )
    .map_err(fehler)?;
    j.schliesse_ab(vorher, Cent(1000), &j.bestaetigt().1)
        .map_err(fehler)?;
    let publiziert = some(Zeitpunkt::frankfurt(tag, 8, 0, 0), "Veröffentlichungszeit")?;
    let (stunde, minute, _) = some(publiziert.uhrzeit(), "UTC-Uhrzeit")?;
    let kandidaten: Vec<_> = titel
        .iter()
        .map(|t| {
            format!(
                "Nachricht {:02}.{:02}.{} {stunde:02}:{minute:02} UTC (testquelle): {t}",
                tag.tag(),
                tag.monat(),
                tag.jahr()
            )
        })
        .collect();
    for &text in titel {
        let beleg = NachrichtBeleg::neu("testquelle", zeit, b"Testquelle", publiziert, text)?;
        j.belege_nachricht(&beleg).map_err(fehler)?;
    }
    j.belege_wissensstichtag(tag, zeit).map_err(fehler)?;
    for beleg in &kandidaten {
        j.beobachte(tag, beleg).map_err(fehler)?;
    }
    j.begruende(tag, "Frageliste", &format!("0 {}", kandidaten.len()))
        .map_err(fehler)?;
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
    let mut frage = format!("Datum: {tag}\nBelegte Termine und Nachrichten:\n");
    for (index, beleg) in kandidaten.iter().take(sichtbar).enumerate() {
        writeln!(frage, "- Ereignis {}: {beleg}", index.saturating_add(1))
            .map_err(|e| e.to_string())?;
    }
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

#[test]
fn analyseexport_enthaelt_genaue_paare_und_urteilsanzahl() -> Result<(), String> {
    use std::fmt::Write as _;

    let wurzel = ordner()?;
    let zeit = beispiel(&wurzel)?;
    let tag = some(zeit.frankfurt_tag(), "Prüftag")?;
    let daten = wurzel.join("daten");
    let d = dienst(wurzel, true);
    let blindseite = get(&d, PFAD, zeit);
    let id = paar_id(&blindseite)?;
    assert_eq!(
        sende(&d, zeit, &format!("id={id}&tag={TAG}&urteil=2-widerlegt"))?,
        303
    );

    let mut budget = 128 * 1024;
    let (fragen, _) = some(
        depot_journal::gespraeche::lesen_begrenzt(&daten, tag, &mut budget)
            .map_err(|e| e.to_string())?,
        "KI-Frage",
    )?;
    let (faelle, abdruck) = Journal::pruefe_lesend_mit_eintraegen(
        &daten.join("journal.txt"),
        128 * 1024,
        |stand, eintraege, journal| {
            (
                prueffaelle_mit_zeilen(stand, eintraege, &fragen, tag),
                depot_kern::abdruck::sha256_hex(journal.as_bytes()),
            )
        },
    )
    .map_err(|e| format!("{e:?}"))?;
    let mut faelle = faelle.map_err(str::to_owned)?;
    assert_eq!(faelle.len(), 2);
    faelle.sort_by_key(|fall| fall.grundmerkmale);

    let mut erwartet = format!(
        "{{\"format\":\"depot-belegpaar-v1\",\"merkmale\":[\"wertbezug\",\"termin\",\"thesenwortanteil\",\"aktualitaet\"],\"journal_sha\":\"{abdruck}\",\"von\":\"{tag}\",\"bis\":\"{tag}\",\"urteile_lesbar\":true,\"menschlich_bearbeitet\":1,\"menschlich_bewertet\":1,\"ki_uebereinstimmungen\":0}}\n"
    );
    for (index, fall) in faelle.iter().enumerate() {
        let z = fall.zuordnung();
        let platz = index.saturating_add(1);
        writeln!(
            erwartet,
            "{{\"tag\":\"{tag}\",\"stichtag\":\"{zeit}\",\"fall_id\":\"{}\",\"berater\":\"Maus\",\"isin\":\"DE0007164600\",\"thesentag\":\"2026-11-04\",\"thesen_sha\":\"{}\",\"ref\":\"{}\",\"x\":[0.000,0.000,0.000,0.994],\"fragenplatz\":{platz},\"grundrang\":{platz}}}",
            fall.kennung, z.thesen_sha, z.referenz
        ).map_err(|e| e.to_string())?;
    }

    let mut zip = depot_export::Zip::default();
    zip.datei_gepackt("neuro2026_belegpaare.jsonl", erwartet.as_bytes())?;
    let zip = zip.fertig()?;
    let ende = some(
        zip.windows(4).position(|w| w == b"PK\x01\x02"),
        "ZIP-Verzeichnis",
    )?;
    let eintrag = some(zip.get(..ende), "ZIP-Eintrag")?;
    let export = formular_post("/export", &format!("t={TOKEN}&zeitraum=heute"));
    let antwort = behandle(&d, &Attrappe::default(), &export, zeit);
    assert_eq!(status(&antwort)?, 200);
    assert!(antwort.windows(eintrag.len()).any(|w| w == eintrag));
    Ok(())
}

#[test]
fn neuro_pilot_nutzt_ki_konsens_und_spaetere_echte_fallkennungen() -> Result<(), String> {
    use NeuroUrteilswert::{KeinBezug, Offen, Schwaecht, Staerkt};
    use depot_kern::json;

    let wurzel = ordner()?;
    let daten = wurzel.join("daten");
    let trainingstag = some(Datum::lies(TAG), "Trainingstag")?;
    let prueftag = some(Datum::lies("2026-11-12"), "Späterer Prüftag")?;
    let trainingsfaelle = [
        (
            "SAP-Cloudgeschäft meldet Quartalszahlen",
            Staerkt,
            Some(Staerkt),
        ),
        (
            "Institut veröffentlicht Wahlumfrage",
            KeinBezug,
            Some(KeinBezug),
        ),
        (
            "SAP meldet verfehltes Vertriebsziel",
            Schwaecht,
            Some(Schwaecht),
        ),
        (
            "Institut prüft Software-Nutzerzahlen",
            Staerkt,
            Some(Schwaecht),
        ),
        ("Stadt meldet neue Vorschriften", KeinBezug, None),
        ("Studie erscheint mit vagen Prognosen", Offen, Some(Offen)),
    ];
    prueftag_mit_ki_urteilen(&wurzel, trainingstag, &trainingsfaelle)?;

    let prueffaelle = [
        (
            "SAP-Cloudgeschäft bekommt neuen Großauftrag",
            Staerkt,
            Some(Staerkt),
        ),
        (
            "Lokalradio meldet Theatereröffnung",
            KeinBezug,
            Some(KeinBezug),
        ),
        (
            "Pendler befragen kommunale Verkehrsbetriebe",
            KeinBezug,
            Some(KeinBezug),
        ),
        (
            "Softwaresparte berichtet neue Kunden",
            Staerkt,
            Some(Staerkt),
        ),
    ];
    let pruefung = prueftag_mit_ki_urteilen(&wurzel, prueftag, &prueffaelle)?;
    let journal = daten.join("journal.txt");

    let d = dienst(wurzel.clone(), true);
    let journal_vorher = fs::read(&journal).map_err(|e| e.to_string())?;
    let austausch = wurzel.join("test-austausch");
    fs::create_dir(&austausch).map_err(|e| e.to_string())?;
    depot_dienst::neuro_pilot(
        &daten,
        trainingstag,
        trainingstag,
        prueftag,
        Some(&austausch),
    )?;
    let lesefile = |name| fs::read_to_string(austausch.join(name)).map_err(|e| e.to_string());
    let train = json::lies(&lesefile("train-eingabe.json")?)?;
    let rank = json::lies(&lesefile("rank-eingabe.json")?)?;
    let mut stufen: Vec<_> = some(
        train.feld("rows").and_then(json::Json::liste),
        "Trainingszeilen",
    )?
    .iter()
    .filter_map(|zeile| zeile.feld("stufe").and_then(json::Json::ganz))
    .collect();
    stufen.sort_unstable();
    assert_eq!(stufen, [0, 1, 1]);
    assert!(get(&d, "/neuroschlopf", pruefung).contains("7 KI-Übereinstimmungen"));
    let prueffall_ids: std::collections::BTreeSet<_> =
        some(rank.feld("rows").and_then(json::Json::liste), "Rangzeilen")?
            .iter()
            .filter_map(|zeile| {
                zeile
                    .feld("id")
                    .and_then(json::Json::text)
                    .map(str::to_owned)
            })
            .collect();
    assert_eq!(prueffall_ids.len(), 4);

    if let Some(pilot) = std::env::var_os("DEPOT_NEURO_PILOT_EXE") {
        pruefe_originalkern(pilot, &d, trainingstag, pruefung, &prueffall_ids)?;
    }
    assert_eq!(
        fs::read(journal).map_err(|e| e.to_string())?,
        journal_vorher
    );
    assert!(!daten.join("neuro2026-belegurteile.tsv").exists());
    assert!(wartend(&d).is_empty());
    Ok(())
}

#[test]
fn modellseite_zeigt_aktuellen_lernstand_vor_dem_ersten_lauf() -> Result<(), String> {
    use NeuroUrteilswert::{KeinBezug, Staerkt};

    let wurzel = ordner()?;
    let trainingstag = some(Datum::lies(TAG), "Trainingstag")?;
    let prueftag = some(Datum::lies("2026-11-12"), "Späterer Prüftag")?;
    let train = [
        ("SAP meldet neue Cloud-Kunden", Staerkt, Some(Staerkt)),
        (
            "Institut veröffentlicht Wahlumfrage",
            KeinBezug,
            Some(KeinBezug),
        ),
        (
            "Lokalradio meldet Preisverleihung",
            KeinBezug,
            Some(KeinBezug),
        ),
    ];
    let training = prueftag_mit_ki_urteilen(&wurzel, trainingstag, &train)?;
    let d = dienst(wurzel.clone(), true);
    let vorher = get(&d, "/neuroschlopf", training);
    assert!(vorher.contains("1 mit Belegbezug · 2 ohne Bezug"));
    assert!(!vorher.contains("Empfänger für KI-Vorprüfung einstellen"));
    assert!(vorher.contains("Es fehlen spätere Prüffälle mit drei vollen Handelstagen Abstand"));

    let pruefung = beispiel_am(
        &wurzel,
        prueftag,
        &[
            "Lokalradio meldet Preisverleihung",
            "SAP-Cloudgeschäft bekommt neuen Großauftrag",
        ],
        2,
    )?;
    let nachher = get(&d, "/neuroschlopf", pruefung);
    assert!(nachher.contains("1 mit Belegbezug · 1 ohne Bezug"));
    assert!(nachher.contains("Die Fälle für den lokalen Modelllauf liegen vor."));
    assert!(nachher.contains("2 spätere Prüffälle, davon 0 mit KI-Übereinstimmung"));
    assert!(!wurzel.join("daten/neuro2026-beobachtung.json").exists());
    Ok(())
}

fn prueftag_mit_ki_urteilen(
    wurzel: &Path,
    tag: Datum,
    faelle_mit_urteilen: &[(&str, NeuroUrteilswert, Option<NeuroUrteilswert>)],
) -> Result<Zeitpunkt, String> {
    let titel: Vec<_> = faelle_mit_urteilen
        .iter()
        .map(|(titel, _, _)| *titel)
        .collect();
    let zeit = beispiel_am(wurzel, tag, &titel, titel.len())?;
    let daten = wurzel.join("daten");
    let journal = daten.join("journal.txt");
    let mut j = Journal::oeffnen(&journal).map_err(|e| format!("{e:?}"))?;
    j.schliesse_ab(tag, Cent(1000), &j.bestaetigt().1)
        .map_err(|e| format!("{e:?}"))?;
    drop(j);
    let mut cache = Lesecache::default();
    let (stand, zeilen) = cache
        .lesen(&journal, 32 * 1024 * 1024)
        .map_err(|e| format!("{e:?}"))?;
    let (gespraeche, 0) = some(
        depot_journal::gespraeche::lesen_begrenzt(&daten, tag, &mut (8 * 1024 * 1024))
            .map_err(|e| format!("{e:?}"))?,
        "Erstmeinung",
    )?
    else {
        return Err("Erstmeinung unlesbar".into());
    };
    let faelle = prueffaelle_mit_zeilen(stand, zeilen, &gespraeche, tag)?;
    for (titel, maus, dachs) in faelle_mit_urteilen.iter().copied() {
        let fall = some(faelle.iter().find(|fall| fall.beleg.contains(titel)), titel)?;
        let id = some(
            NeuroAuftragId::neu(&fall.kennung, NeuroRunde::Erstfrage),
            "Fall-ID",
        )?;
        belege_kiurteil(&journal, tag, id.clone(), BeraterId::Maus, maus, None)?;
        if let Some(dachs) = dachs {
            belege_kiurteil(&journal, tag, id, BeraterId::Dachs, dachs, None)?;
        }
    }
    Ok(zeit)
}

/// Prüft den optional gebauten Originalkern über denselben Aufruf wie auf dem Depot-PC.
fn pruefe_originalkern(
    pilot: std::ffi::OsString,
    d: &Dienst,
    trainingstag: Datum,
    pruefung: Zeitpunkt,
    prueffall_ids: &std::collections::BTreeSet<String>,
) -> Result<(), String> {
    use depot_kern::json;

    let wurzel = &d.wurzel;
    let daten = wurzel.join("daten");
    let prueftag = some(pruefung.frankfurt_tag(), "Späterer Prüftag")?;
    fs::create_dir(wurzel.join("kodex")).map_err(|e| e.to_string())?;
    fs::write(wurzel.join("kodex/festschreibung.txt"), "Testprojekt\n")
        .map_err(|e| e.to_string())?;
    let ausgabe = Command::new(pilot)
        .arg(env!("CARGO_BIN_EXE_depot_dienst"))
        .arg(wurzel)
        .args([
            trainingstag.to_string(),
            trainingstag.to_string(),
            prueftag.to_string(),
        ])
        .output()
        .map_err(|e| e.to_string())?;
    assert!(
        ausgabe.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&ausgabe.stdout),
        String::from_utf8_lossy(&ausgabe.stderr)
    );
    let lauf = some(
        fs::read_dir(daten.join("neuro2026-pilotlaeufe"))
            .map_err(|e| e.to_string())?
            .next(),
        "Pilotlauf",
    )?
    .map_err(|e| e.to_string())?
    .path();
    let eingabe = fs::read(lauf.join("rank-eingabe.json")).map_err(|e| e.to_string())?;
    let antwortpfad = lauf.join("rank-antwort.json");
    let antwort = fs::read_to_string(&antwortpfad).map_err(|e| e.to_string())?;
    let rang = json::lies(&antwort)?;
    let sha = depot_kern::abdruck::sha256_hex(&eingabe);
    assert_eq!(
        rang.feld("input_sha256").and_then(json::Json::text),
        Some(sha.as_str())
    );
    let seite = get(d, "/neuroschlopf", pruefung);
    assert!(seite.contains("Modellwert") && seite.contains("bekommt neuen Großauftrag"));
    assert!(seite.contains("mit KI-Konsens bewerteten Vergleichen"));
    assert!(
        seite.contains("Netz des Depotmodells") && seite.contains("Lernkurve des Depotmodells")
    );
    assert!(!seite.contains("Multiplikation"));
    let bericht =
        fs::read_to_string(daten.join("neuro2026-beobachtung.json")).map_err(|e| e.to_string())?;
    let bericht = json::lies(&bericht)?;
    let kurve = some(bericht.feld("lernkurve"), "Depotlernkurve")?;
    let verluste = some(
        kurve.feld("fehler").and_then(json::Json::liste),
        "Epochenverluste",
    )?;
    let train = json::lies(
        &fs::read_to_string(lauf.join("train-antwort.json")).map_err(|e| e.to_string())?,
    )?;
    let epochen = some(
        train.feld("metrics").and_then(json::Json::liste),
        "Originalepochen",
    )?;
    assert_eq!(verluste.len(), epochen.len());
    let zahlenwert = |j: &json::Json| match j {
        json::Json::Zahl(s) => s.parse::<f64>().ok(),
        _ => None,
    };
    assert!(epochen.iter().zip(verluste).all(|(epoche, verlust)| {
        epoche.feld("mean_loss").and_then(zahlenwert) == zahlenwert(verlust)
    }));
    let scores = some(
        rang.feld("scores").and_then(json::Json::liste),
        "Modellwerte",
    )?;
    assert_eq!(scores.len(), 4);
    for score in scores {
        let id = some(score.feld("id").and_then(json::Json::text), "Modellfall")?;
        let Some(json::Json::Zahl(wert)) = score.feld("score") else {
            return Err("Modellwert fehlt".into());
        };
        assert!(prueffall_ids.contains(id) && seite.contains(&wert.replace('.', ",")));
    }
    fs::write(&antwortpfad, antwort.replace(&sha, &"0".repeat(64))).map_err(|e| e.to_string())?;
    assert!(
        depot_dienst::neuro_pilot(&daten, trainingstag, trainingstag, prueftag, Some(&lauf))
            .is_err()
    );
    Ok(())
}

#[test]
fn rangvergleich_bleibt_beim_tagwechsel_und_speichern_blind() -> Result<(), String> {
    let wurzel = ordner()?;
    let zeit = beispiel(&wurzel)?;
    let d = dienst(wurzel.clone(), true);
    let normal = get(&d, PFAD, zeit);
    let id = paar_id(&normal)?;
    let pfad = format!("{PFAD}?tag={TAG}&vergleich=ja");
    let vorab = get(&d, &pfad, zeit);
    assert!(vorab.contains("Für diesen Prüftag liegt keine Stichprobe aus dem Rangvergleich vor."));
    assert!(!vorab.contains("name=\"id\""));
    let abdruck = "a".repeat(64);
    let berichtszeile = |kennung: &str, rang: usize| {
        format!(
            r#"{{"fall_id":"{kennung}","referenz":"Berichtsreferenz","titel":"Berichtstitel",
            "quelle":"Berichtsquelle","grundrang":{rang},"modellrang":{rang},"wert":"0.6","merkmale":[0,0,0,0]}}"#,
        )
    };
    let belege = [
        berichtszeile(id, 1),
        berichtszeile(&"1".repeat(64), 2),
        berichtszeile(&"2".repeat(64), 3),
        berichtszeile(&"3".repeat(64), 4),
    ]
    .join(",");
    let bericht = format!(
        r#"{{"schema":"depot-belegpaar-v1","modellversion":"0.1.16","modellabdruck":"{abdruck}",
        "stand":"Testbericht","paare":2,"urteile":0,"trainingstage":1,"prueftage":1,
        "gruppen":[{{"tag":"{TAG}","berater":"Maus","isin":"DE0007164600","these":"Berichtsthese",
        "belege":[{belege}]}}]}}"#,
    );
    fs::write(wurzel.join("daten/neuro2026-beobachtung.json"), bericht)
        .map_err(|e| e.to_string())?;
    let seite = get(&d, &pfad, zeit);
    assert!(seite.contains("Blindprüfung zum Rangvergleich"));
    assert!(seite.contains("Bearbeitet <b>0 von 1</b>"));
    assert_eq!(seite.matches("name=\"vergleich\" value=\"ja\"").count(), 2);
    assert_eq!(paar_id(&seite)?, id);
    for verborgen in ["Berichtstitel", "Neuro2026-Rang", "Grundrang", "Modellwert"] {
        assert!(!seite.contains(verborgen));
    }
    let felder = format!("t={TOKEN}&id={id}&tag={TAG}&urteil=1&vergleich=ja");
    let post = formular_post(PFAD, &felder);
    let antwort = behandle(&d, &Attrappe::default(), &post, zeit);
    assert_eq!(status(&antwort)?, 303);
    assert!(text(&antwort).contains(&format!(
        "Location: {PFAD}?tag={TAG}&gespeichert=ja&vergleich=ja\r\n"
    )));
    let danach = get(&d, &pfad, zeit);
    assert!(danach.contains("Bearbeitet <b>1 von 1</b>"));
    assert!(!danach.contains("name=\"id\""));
    let normal_danach = get(&d, PFAD, zeit);
    assert!(normal_danach.contains("Bearbeitet <b>1 von 2</b>"));
    assert_ne!(paar_id(&normal_danach)?, id);
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
        .reserviere_neurofrage(&id, wer, tag, 1, BeraterEinstellung::standard(wer), 20)
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
