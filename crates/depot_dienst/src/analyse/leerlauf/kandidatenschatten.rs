use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use depot_journal::Stand;
use depot_kern::{Cent, TopfId, Zeitpunkt};
use depot_lage::ausfuehrung::{OrderZeile, Richtung};
use depot_lage::kandidatenschatten::{Abschluss, Ausstieg, StandImSchatten, Zeile, auswertung};
use depot_lage::schattenkandidaten::{Kandidat, Phase};

use super::{Zeitraum, prozent};

fn name(phase: Phase, topf: TopfId) -> String {
    match (phase, topf) {
        (Phase::Erstmeinung, TopfId::A) => "Dachs allein".into(),
        (Phase::Erstmeinung, TopfId::B) => "Maus allein".into(),
        (Phase::Zweitmeinung, _) => "Stand nach Zweitmeinung".into(),
        (Phase::Runde(nr), _) => format!("Entwurf der Runde {nr}"),
        (Phase::Gemeinsam, _) => "Gemeinsamer Stand".into(),
    }
}

fn papierstatus(k: &Kandidat, buch: &[OrderZeile], zeitraum: Zeitraum) -> &'static str {
    let am_tag = |o: &&OrderZeile| {
        o.topf == k.topf
            && o.isin == k.isin
            && o.richtung == Richtung::Kauf
            && o.versand.and_then(Zeitpunkt::frankfurt_tag) == Some(k.tag)
    };
    let mut auftrag = false;
    for o in buch.iter().filter(am_tag) {
        if o.ausgefuehrt_am.is_some_and(|tag| tag <= zeitraum.bis) {
            return "gefüllt";
        }
        auftrag = true;
    }
    if auftrag {
        "versandt"
    } else {
        "kein Kaufversand"
    }
}

fn abschluss(a: &Abschluss) -> String {
    let grund = match a.grund {
        Ausstieg::Frist => "Frist",
        Ausstieg::Stopp => "Stopp",
        Ausstieg::Ziel => "Ziel",
    };
    format!("{} · {} ({grund})", a.netto, a.fuellung.datum)
}

fn detail(text: &mut String, zeile: &Zeile, buch: &[OrderZeile], zeitraum: Zeitraum) {
    let k = &zeile.kandidat;
    let stueck = k.stueck.map_or_else(|| "–".into(), |s| s.to_string());
    let limit = k.limit.map_or_else(|| "–".into(), |l| l.to_string());
    let frist = k
        .plan
        .map_or_else(|| "5 T (Modell)".into(), |p| format!("{} T", p.handelstage));
    let _ = write!(
        text,
        "| {} | {:?} | {} | {} | {} | {stueck} | {limit} | {frist} | {} | ",
        k.tag,
        k.topf,
        name(k.phase, k.topf),
        k.isin,
        prozent(Some(k.ziel_bp)),
        papierstatus(k, buch, zeitraum)
    );
    let status = match &zeile.stand {
        StandImSchatten::FehlendeKaufdaten => "Kaufdaten fehlen".into(),
        StandImSchatten::KeineStuecke => "keine Stücke".into(),
        StandImSchatten::EinstiegOffen => "Einstieg offen".into(),
        StandImSchatten::LimitVerfehlt(f) => {
            format!("Limit am {} nicht erreicht ({})", f.datum, f.kurs)
        }
        StandImSchatten::Kursluecke(tag) => format!("Schlusskurs am {tag} fehlt"),
        StandImSchatten::Kapitalmassnahme(tag) => format!("Kapitalmaßnahme am {tag}"),
        StandImSchatten::Haelt(e) => format!("Haltefenster ab {} offen ({})", e.datum, e.kurs),
        StandImSchatten::Abgeschlossen {
            einstieg,
            frist,
            mit_stopp,
            mit_ziel,
            korbnetto,
        } => {
            let ziel = mit_ziel.as_ref().map_or_else(|| "–".into(), abschluss);
            let korb = korbnetto.map_or_else(|| "–".into(), |n| n.to_string());
            let _ = writeln!(
                text,
                "{} am {} · Fensterende {frist} | {} | {ziel} | {korb} |",
                einstieg.kurs,
                einstieg.datum,
                abschluss(mit_stopp)
            );
            return;
        }
    };
    let _ = writeln!(text, "{status} | – | – | – |");
}

#[derive(Default)]
struct Summe {
    anzahl: u32,
    abgeschlossen: u32,
    kein_einstieg: u32,
    netto: i128,
}

fn zusammenfassung(text: &mut String, zeilen: &[Zeile]) {
    let mut gruppen = BTreeMap::<(Phase, TopfId), Summe>::new();
    for z in zeilen {
        let s = gruppen
            .entry((z.kandidat.phase, z.kandidat.topf))
            .or_default();
        s.anzahl = s.anzahl.saturating_add(1);
        match &z.stand {
            StandImSchatten::Abgeschlossen { mit_stopp, .. } => {
                s.abgeschlossen = s.abgeschlossen.saturating_add(1);
                s.netto = s.netto.saturating_add(i128::from(mit_stopp.netto.0));
            }
            StandImSchatten::KeineStuecke | StandImSchatten::LimitVerfehlt(_) => {
                s.kein_einstieg = s.kein_einstieg.saturating_add(1);
            }
            _ => {}
        }
    }
    text.push_str("| Stufe | Topf | Ideen | Ende belegt | Ohne Einstieg | Offen oder Datenlücke | Ø netto je abgeschlossener Idee |\n|---|---|---:|---:|---:|---:|---:|\n");
    for ((phase, topf), s) in gruppen {
        let offen = s
            .anzahl
            .saturating_sub(s.abgeschlossen.saturating_add(s.kein_einstieg));
        let netto = s
            .netto
            .checked_div(i128::from(s.abgeschlossen))
            .and_then(|n| i64::try_from(n).ok())
            .map_or_else(|| "–".into(), |n| Cent(n).to_string());
        let _ = writeln!(
            text,
            "| {} | {topf:?} | {} | {} | {} | {offen} | {netto} |",
            name(phase, topf),
            s.anzahl,
            s.abgeschlossen,
            s.kein_einstieg
        );
    }
}

pub(super) fn bericht(
    text: &mut String,
    stand: &Stand,
    buch: &[OrderZeile],
    zeitraum: Zeitraum,
    jetzt: Zeitpunkt,
) {
    let zeilen = auswertung(stand, zeitraum.von, zeitraum.bis, jetzt);
    text.push_str(concat!(
        "\n### Schattendepot der Kaufkandidaten\n\n",
        "DepotSchlopf bewertet protokollierte Kaufideen als einzelne Modellfälle mit belegten Börsenschlüssen, festgehaltenem Limit und Modell-Handelskosten. Teilnehmer und Entscheidungsstufen können denselben Wert enthalten. Die Übersicht zählt diese Fälle je Stufe, ihre Ergebnisse ergeben kein gemeinsames Portfolio.\n\n",
        "Der Hauptpfad verkauft bei Stopp oder zur Frist. Die zusätzliche Zielvariante verkauft auch nach Erreichen eines lesbaren Kursziels. Eine am Schluss erkannte Schwelle wirkt frühestens am folgenden Börsenschluss. Für Ideen ohne lesbare Frist gelten fünf Handelstage als Modellannahme. Ein Netto erscheint erst bei vollständigem Haltefenster. Der Korb verwendet dieselben Stichtage und Vergleichswerte derselben Börse. Bargeld bleibt mit 0 € bewertet.\n\n"
    ));
    if zeilen.is_empty() {
        text.push_str("Im Zeitraum liegen noch keine strukturiert erfassten Kaufkandidaten vor.\n");
    } else {
        zusammenfassung(text, &zeilen);
        text.push_str("\nDie Spalte Papier erfasst Versand und Füllung für denselben Topf und Wert am Beratungstag.\n\n| Tag | Topf | Stufe | ISIN | Zielgewicht | Stück | Kauflimit | Frist | Papier | Modellstatus oder Einstieg | Netto Stopp und Frist | Netto auch mit Ziel | Netto Korb |\n|---|---|---|---|---:|---:|---:|---|---|---|---:|---:|---:|\n");
        for z in &zeilen {
            detail(text, z, buch, zeitraum);
        }
    }
    let erfasst: BTreeSet<_> = zeilen
        .iter()
        .filter(|z| z.kandidat.phase == Phase::Erstmeinung)
        .map(|z| (z.kandidat.tag, z.kandidat.topf, &z.kandidat.isin))
        .collect();
    let altvetos: Vec<_> = depot_lage::vetoschatten::vetos(stand.begruendungen())
        .into_iter()
        .filter(|v| {
            (zeitraum.von..=zeitraum.bis).contains(&v.tag)
                && !erfasst.contains(&(v.tag, v.topf, &v.isin))
        })
        .collect();
    if !altvetos.is_empty() {
        text.push_str("\n#### Ältere Vetobelege ohne vollständigen Kaufplan\n\n| Tag | Topf | ISIN | Zielgewicht |\n|---|---|---|---:|\n");
        for v in &altvetos {
            let _ = writeln!(
                text,
                "| {} | {:?} | {} | {} |",
                v.tag,
                v.topf,
                v.isin,
                prozent(Some(v.ziel_bp))
            );
        }
    }
}
