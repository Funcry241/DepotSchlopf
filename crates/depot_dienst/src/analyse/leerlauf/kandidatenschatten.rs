mod kosten;
mod rollen;

use std::collections::BTreeSet;
use std::fmt::Write as _;

use depot_journal::Stand;
use depot_kern::{TopfId, Zeitpunkt};
use depot_lage::ausfuehrung::{OrderZeile, Richtung};
use depot_lage::kandidatenschatten::{Abschluss, Ausstieg, StandImSchatten, Zeile, auswertung};
use depot_lage::schattenkandidaten::{Kandidat, Phase};

use super::{Zeitraum, prozent};

fn name(phase: Phase, topf: TopfId) -> String {
    let (autor, pruefer) = match topf {
        TopfId::A => ("Dachs", "Maus"),
        TopfId::B => ("Maus", "Dachs"),
    };
    match phase {
        Phase::Erstmeinung => format!("{autor} · Erstmeinung"),
        Phase::Zweitmeinung => format!("{pruefer} · Zweitmeinung"),
        Phase::Runde(nr) => format!("{autor} · Entwurf der Runde {nr}"),
        Phase::Gemeinsam => "Gemeinsamer Stand".into(),
    }
}

fn papierstatus(k: &Kandidat, buch: &[OrderZeile], jetzt: Zeitpunkt) -> &'static str {
    let exporttag = jetzt.frankfurt_tag();
    let am_tag = |o: &&OrderZeile| {
        o.topf == k.topf
            && o.isin == k.isin
            && o.richtung == Richtung::Kauf
            && o.versand
                .is_some_and(|zeit| zeit <= jetzt && zeit.frankfurt_tag() == Some(k.tag))
    };
    let mut auftrag = false;
    for o in buch.iter().filter(am_tag) {
        if o.ausgefuehrt_am
            .zip(exporttag)
            .is_some_and(|(tag, export)| tag <= export)
        {
            return "Ausführung erfasst";
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

fn detail(text: &mut String, zeile: &Zeile, buch: &[OrderZeile], jetzt: Zeitpunkt) {
    let k = &zeile.kandidat;
    let lauf = k.lauf.map_or_else(|| "–".into(), |n| n.to_string());
    let stueck = k.stueck.map_or_else(|| "–".into(), |s| s.to_string());
    let limit = k.limit.map_or_else(|| "–".into(), |l| l.to_string());
    let frist = k
        .plan
        .map_or_else(|| "5 T (Modell)".into(), |p| format!("{} T", p.handelstage));
    let _ = write!(
        text,
        "| {} | {lauf} | {:?} | {} | {} | {} | {stueck} | {limit} | {frist} | {} | ",
        k.tag,
        k.topf,
        name(k.phase, k.topf),
        k.isin,
        prozent(Some(k.ziel_bp)),
        papierstatus(k, buch, jetzt)
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
            signalkontrolle,
            ..
        } => {
            let ziel = mit_ziel.as_ref().map_or_else(|| "–".into(), abschluss);
            let korb = korbnetto.map_or_else(|| "–".into(), |n| n.to_string());
            let signal = signalkontrolle.as_ref().map_or_else(
                || "–".into(),
                |s| {
                    format!(
                        "{} · Idee {} · Signal {}",
                        s.isin, s.idee_netto, s.signal_netto
                    )
                },
            );
            let _ = writeln!(
                text,
                "{} am {} · Fensterende {frist} | {} | {ziel} | {korb} | {signal} |",
                einstieg.kurs,
                einstieg.datum,
                abschluss(mit_stopp)
            );
            return;
        }
    };
    let _ = writeln!(text, "{status} | – | – | – | – |");
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
        "Der Hauptpfad verkauft bei Stopp oder zur Frist. Die zusätzliche Zielvariante verkauft auch nach Erreichen eines lesbaren Kursziels. Eine am Schluss erkannte Schwelle wirkt frühestens am folgenden Börsenschluss. Für Ideen ohne lesbare Frist gelten fünf Handelstage als Modellannahme. Ein Netto erscheint erst bei vollständigem Haltefenster. Der Exportzeitraum wählt die Beratungstage aus. Die Auswertung verwendet auch spätere Kursbelege, soweit sie beim Export vorliegen. Der Korb verwendet dieselben Stichtage und Vergleichswerte derselben Börse. Bargeld bleibt mit 0 € bewertet.\n\n"
    ));
    rollen::bericht(text, stand, &zeilen, zeitraum);
    kosten::bericht(text, stand, zeitraum);
    if zeilen.is_empty() {
        text.push_str("Im Zeitraum liegen noch keine strukturiert erfassten Kaufkandidaten vor.\n");
    } else {
        text.push_str("\nDie Papierspalte zeigt Käufe, die am Beratungstag für denselben Topf und Wert versandt wurden, sowie ihren bis zum Export bekannten Ausführungsstand. Diese Zuordnung gilt für den Tag. Der Vergleich mit der Signalregel beginnt bei einer gefüllten KI-Kaufidee mit vollständigem Haltefenster. Der 20-Tage-Rangführer derselben Börse steigt zum gleichen Schluss mit Modellspread und demselben Kapitalrahmen ein. Sein Einstieg hat kein Kauflimit. Beide Positionen werden zur selben festen Frist bewertet.\n\n| Tag | Lauf | Topf | Stufe | ISIN | Zielgewicht | Stück | Kauflimit | Frist | Papier am Beratungstag (Topf/ISIN) | Modellstatus oder Einstieg | Netto Stopp und Frist | Netto auch mit Ziel | Netto Korb | Netto zur Signalregel |\n|---|---:|---|---|---|---:|---:|---:|---|---|---|---:|---:|---:|---|\n");
        for z in &zeilen {
            detail(text, z, buch, jetzt);
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
