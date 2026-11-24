use std::fmt::Write as _;

use depot_journal::{KiStand, KiZweck, Stand};
use depot_kern::{BeraterId, Cent};
use depot_lage::schattenkandidaten::{Beratungslauf, beratungslaeufe};

use super::Zeitraum;

#[derive(Default)]
struct Betrag {
    abgerechnet: Cent,
    offen: Cent,
    aufrufe: usize,
}

impl Betrag {
    fn buche(&mut self, wert: Cent, abgerechnet: bool) -> Option<()> {
        let ziel = if abgerechnet {
            &mut self.abgerechnet
        } else {
            &mut self.offen
        };
        *ziel = ziel.checked_add(wert)?;
        self.aufrufe = self.aufrufe.saturating_add(1);
        Some(())
    }

    fn text(&self) -> String {
        match (self.aufrufe, self.offen) {
            (0, _) => "–".into(),
            (_, Cent::NULL) => self.abgerechnet.to_string(),
            _ => format!("{} + {} reserviert", self.abgerechnet, self.offen),
        }
    }
}

#[derive(Default)]
struct Kosten {
    erst_dachs: Betrag,
    erst_maus: Betrag,
    zweit_topf_a: Betrag,
    zweit_topf_b: Betrag,
    abstimmung: Betrag,
    vorbereitung: Betrag,
}

fn kosten(ki: &KiStand, lauf: &Beratungslauf) -> Option<Kosten> {
    let mut k = Kosten::default();
    for nr in lauf.buchungen.clone() {
        let buchung = ki.buchungen().get(nr)?;
        let (zweck, abgerechnet) = ki.buchungsbeleg(nr)?;
        let betrag = match (zweck, buchung.berater) {
            (KiZweck::Erstmeinung, BeraterId::Dachs) => &mut k.erst_dachs,
            (KiZweck::Erstmeinung, BeraterId::Maus) => &mut k.erst_maus,
            (KiZweck::Zweitmeinung, BeraterId::Dachs) => &mut k.zweit_topf_b,
            (KiZweck::Zweitmeinung, BeraterId::Maus) => &mut k.zweit_topf_a,
            (KiZweck::Abstimmung, _) => &mut k.abstimmung,
            (KiZweck::Vorbereitung, _) => &mut k.vorbereitung,
            _ => continue,
        };
        betrag.buche(buchung.reserviert, abgerechnet)?;
    }
    Some(k)
}

pub(super) fn bericht(text: &mut String, stand: &Stand, zeitraum: Zeitraum) {
    let laeufe = beratungslaeufe(stand);
    let mut laeufe = laeufe
        .iter()
        .filter(|l| (zeitraum.von..=zeitraum.bis).contains(&l.tag))
        .peekable();
    if laeufe.peek().is_none() {
        return;
    }
    text.push_str(concat!(
        "\n#### KI-Kosten je abgeschlossener Beratung\n\n",
        "Die Kosten gehören zur Beratung, in der DepotSchlopf den Aufruf gebucht hat. Der Bericht weist spätere Abrechnungen und offene Reservierungen getrennt aus. Die Abstimmung betrifft beide Töpfe gemeinsam. Ihre Kosten erscheinen einmal je Lauf. Die Kandidatenrechnung oben enthält nur Handelskosten.\n\n",
        "| Tag | Lauf | Erstmeinung Dachs | Erstmeinung Maus | Zweitmeinung Topf A | Zweitmeinung Topf B | Abstimmung beider Töpfe | Vorbereitung |\n",
        "|---|---:|---:|---:|---:|---:|---:|---:|\n"
    ));
    for lauf in laeufe {
        if let Some(k) = kosten(stand.ki(), lauf) {
            let _ = writeln!(
                text,
                "| {} | {} | {} | {} | {} | {} | {} | {} |",
                lauf.tag,
                lauf.lauf,
                k.erst_dachs.text(),
                k.erst_maus.text(),
                k.zweit_topf_a.text(),
                k.zweit_topf_b.text(),
                k.abstimmung.text(),
                k.vorbereitung.text(),
            );
        } else {
            let _ = writeln!(
                text,
                "| {} | {} | Buchungen unvollständig | – | – | – | – | – |",
                lauf.tag, lauf.lauf
            );
        }
    }
}
