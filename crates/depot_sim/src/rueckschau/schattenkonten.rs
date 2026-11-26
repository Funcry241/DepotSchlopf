//! Vier lesende Papierkonten für dasselbe Exportfenster.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use depot_journal::{KiZweck, Stand};
use depot_kern::liste::papierorder_schluss;
use depot_kern::{BP_GANZ, Boerse, Cent, Datum, Grenzen, Isin, Rundung, Zeitpunkt};

use crate::aufwaertsdruck_beleg::{Befund, Beobachtung, Idee, beobachtungen};
use crate::schattenkandidaten::{Phase, beratungslaeufe, kandidaten, phasenbelege};

use super::aufwaertsdruck::HALTETAGE;
use super::ausstiegsvergleich::{Historie, handelstage_ab, kaufbetrag, stuecke, verkauf};

const START: Cent = Cent(500_000);

#[derive(Clone, Copy)]
struct Zeitraum {
    von: Datum,
    bis: Datum,
    jetzt: Zeitpunkt,
}

impl Zeitraum {
    fn schluss(self, boerse: Boerse) -> Option<Datum> {
        std::iter::successors(Some(self.bis.min(self.jetzt.frankfurt_tag()?)), |tag| {
            tag.plus_tage(-1)
        })
        .take_while(|tag| *tag >= self.von)
        .find(|&tag| {
            boerse.handelstag(tag) && boerse.schluss(tag).is_some_and(|zeit| zeit <= self.jetzt)
        })
    }
}

struct Kauf<'a> {
    isin: &'a Isin,
    boerse: Boerse,
    schluss: Zeitpunkt,
    limit: Cent,
    bezugstag: Datum,
}

impl<'a> Kauf<'a> {
    fn neu(b: &Beobachtung, idee: &'a Idee, bereit: Zeitpunkt, grenzen: &Grenzen) -> Option<Self> {
        Some(Self {
            isin: &idee.isin,
            boerse: b.boerse,
            schluss: papierorder_schluss(&idee.isin, bereit)?,
            limit: idee.referenz.mal_bp(
                BP_GANZ.checked_add(grenzen.limit_abstand_bp())?,
                Rundung::Ab,
            )?,
            bezugstag: b.bezugstag,
        })
    }
}

struct Position<'a> {
    isin: &'a Isin,
    boerse: Boerse,
    kauf: Datum,
    ausstieg: Datum,
    stueck: u64,
}

impl Position<'_> {
    fn erloes(&self, historie: &Historie, tag: Datum, grenzen: &Grenzen) -> Option<Cent> {
        let tage: Vec<_> = std::iter::once(self.kauf)
            .chain(handelstage_ab(self.boerse, self.kauf).take_while(|d| *d <= tag))
            .collect();
        let kurs = historie.fenster(self.isin, &tage)?.last()?.preis;
        verkauf(self.boerse, kurs, self.stueck, grenzen)
    }
}

struct Konto<'a> {
    bar: Cent,
    position: Option<Position<'a>>,
    kaeufe: usize,
    kosten: Cent,
    luecken: usize,
}

impl<'a> Konto<'a> {
    fn neu() -> Self {
        Self {
            bar: START,
            position: None,
            kaeufe: 0,
            kosten: Cent::NULL,
            luecken: 0,
        }
    }

    fn luecke(&mut self) {
        self.luecken = self.luecken.saturating_add(1);
    }

    fn belaste(&mut self, kosten: Option<Cent>) {
        if let Some((bar, summe)) =
            kosten.and_then(|k| self.bar.checked_sub(k).zip(self.kosten.checked_add(k)))
        {
            self.bar = bar;
            self.kosten = summe;
        } else {
            self.luecke();
        }
    }

    fn verkaufe_faellig(&mut self, historie: &Historie, vor: Zeitpunkt, grenzen: &Grenzen) {
        let Some(p) = &self.position else { return };
        if p.boerse.schluss(p.ausstieg).is_none_or(|zeit| zeit >= vor) {
            return;
        }
        if let Some(bar) = p
            .erloes(historie, p.ausstieg, grenzen)
            .and_then(|erloes| self.bar.checked_add(erloes))
        {
            self.bar = bar;
        } else {
            self.luecke();
        }
        self.position = None;
    }

    fn kaufe(&mut self, historie: &Historie, kauf: &Kauf<'a>, grenzen: &Grenzen) -> Option<()> {
        self.verkaufe_faellig(historie, kauf.schluss, grenzen);
        if self.position.is_some() || self.bar <= Cent::NULL {
            return Some(());
        }
        let tag = kauf.schluss.frankfurt_tag()?;
        let bezug: Vec<_> = if kauf.bezugstag == tag {
            vec![tag]
        } else {
            vec![kauf.bezugstag, tag]
        };
        let kurs = historie.fenster(kauf.isin, &bezug)?.last()?.preis;
        let fuellung = grenzen.papierkurs(kauf.boerse, kurs, Rundung::Auf)?;
        if fuellung > kauf.limit {
            return Some(());
        }
        let budget = self.bar.anteil_bp_ab(grenzen.max_anteil_bp())?;
        let stueck = stuecke(budget, kauf.limit, grenzen)?;
        if stueck == 0 || fuellung.mal_stueck(stueck)? < grenzen.min_order() {
            return Some(());
        }
        let ausstieg = handelstage_ab(kauf.boerse, tag).take(HALTETAGE).last()?;
        self.bar = self
            .bar
            .checked_sub(kaufbetrag(fuellung, stueck, grenzen)?)?;
        self.kaeufe = self.kaeufe.saturating_add(1);
        self.position = Some(Position {
            isin: kauf.isin,
            boerse: kauf.boerse,
            kauf: tag,
            ausstieg,
            stueck,
        });
        Some(())
    }

    fn wert(&self, historie: &Historie, zeitraum: Zeitraum, grenzen: &Grenzen) -> Option<Cent> {
        if self.luecken != 0 {
            return None;
        }
        let Some(p) = &self.position else {
            return Some(self.bar);
        };
        let tag = zeitraum.schluss(p.boerse)?.min(p.ausstieg);
        self.bar.checked_add(p.erloes(historie, tag, grenzen)?)
    }
}

#[derive(Default)]
struct Beratung {
    gemeinsame_zeit: BTreeMap<Isin, Zeitpunkt>,
}

fn beratungskosten(stand: &Stand, zeitraum: Zeitraum) -> BTreeMap<Datum, Option<Cent>> {
    let ki = stand.ki();
    let mut tage = BTreeMap::new();
    for (nr, buchung) in ki.buchungen().iter().enumerate() {
        if !(zeitraum.von..=zeitraum.bis).contains(&buchung.datum) {
            continue;
        }
        let Some((KiZweck::Erstmeinung | KiZweck::Zweitmeinung | KiZweck::Abstimmung, abgerechnet)) =
            ki.buchungsbeleg(nr)
        else {
            continue;
        };
        let kosten = tage.entry(buchung.datum).or_insert(Some(Cent::NULL));
        *kosten = kosten.and_then(|summe| {
            abgerechnet
                .then_some(buchung.reserviert)
                .and_then(|betrag| summe.checked_add(betrag))
        });
    }
    let mut rest = 1_i64;
    for kosten in tage.values_mut().flatten() {
        if let Some(cent) = kosten.0.checked_add(rest) {
            *kosten = Cent(cent.div_euclid(2));
            rest = cent.rem_euclid(2);
        }
    }
    tage
}

fn beratung(stand: &Stand, zeitraum: Zeitraum) -> BTreeMap<Datum, Beratung> {
    let mut tage = BTreeMap::<Datum, Beratung>::new();
    let laeufe = beratungslaeufe(stand);
    let abgeschlossen: BTreeSet<_> = laeufe.iter().map(|lauf| (lauf.tag, lauf.lauf)).collect();
    let gemeinsam: BTreeSet<_> = phasenbelege(stand)
        .into_iter()
        .filter(|p| p.phase == Phase::Gemeinsam)
        .map(|p| (p.tag, p.lauf, p.topf))
        .collect();
    for lauf in laeufe
        .iter()
        .filter(|l| (zeitraum.von..=zeitraum.bis).contains(&l.tag))
    {
        tage.entry(lauf.tag).or_default();
    }
    for k in kandidaten(stand)
        .into_iter()
        .filter(|k| k.phase == Phase::Gemeinsam)
    {
        let Some((lauf, zeit)) = k.lauf.zip(k.bereit_ab) else {
            continue;
        };
        let Some(tag) = tage.get_mut(&k.tag) else {
            continue;
        };
        if zeit > zeitraum.jetzt
            || !abgeschlossen.contains(&(k.tag, lauf))
            || !gemeinsam.contains(&(k.tag, lauf, k.topf))
        {
            continue;
        }
        tag.gemeinsame_zeit
            .entry(k.isin)
            .and_modify(|bisher| *bisher = (*bisher).min(zeit))
            .or_insert(zeit);
    }
    tage
}

fn offene_signale(stand: &Stand, zeitraum: Zeitraum) -> Vec<Beobachtung> {
    let mut befunde: Vec<_> = beobachtungen(stand)
        .into_iter()
        .filter(|b| (zeitraum.von..=zeitraum.bis).contains(&b.tag) && b.bereit <= zeitraum.jetzt)
        .collect();
    befunde.sort_by_key(|b| b.bereit);
    befunde
}

fn kaeufer<'a>(
    befunde: &'a [Beobachtung],
    mit_beratung: Option<&BTreeMap<Datum, Beratung>>,
    zeitraum: Zeitraum,
    grenzen: &Grenzen,
) -> Vec<Kauf<'a>> {
    let mut gesehen = BTreeSet::new();
    let mut kaeufer = Vec::new();
    for b in befunde {
        let Befund::Signal(idee) = &b.befund else {
            continue;
        };
        if !gesehen.insert((b.boerse, b.bezugstag, &idee.gruppe)) {
            continue;
        }
        let bereit = if let Some(tage) = mit_beratung {
            let Some(zeit) = tage
                .get(&b.tag)
                .and_then(|tag| tag.gemeinsame_zeit.get(&idee.isin))
            else {
                continue;
            };
            b.bereit.max(*zeit)
        } else {
            b.bereit
        };
        if let Some(kauf) = Kauf::neu(b, idee, bereit, grenzen)
            && kauf.schluss <= zeitraum.jetzt
            && kauf
                .schluss
                .frankfurt_tag()
                .is_some_and(|tag| tag <= zeitraum.bis)
        {
            kaeufer.push(kauf);
        }
    }
    kaeufer.sort_by(|a, b| a.schluss.cmp(&b.schluss).then(a.isin.cmp(b.isin)));
    kaeufer
}

fn rechne<'a>(
    historie: &Historie,
    kaeufer: &[Kauf<'a>],
    kosten: &BTreeMap<Datum, Option<Cent>>,
    grenzen: &Grenzen,
) -> Konto<'a> {
    let mut konto = Konto::neu();
    let mut kosten = kosten.iter().peekable();
    for kauf in kaeufer {
        let tag = kauf.schluss.frankfurt_tag();
        while kosten.peek().is_some_and(|(datum, _)| Some(**datum) <= tag) {
            if let Some((_, betrag)) = kosten.next() {
                konto.belaste(*betrag);
            }
        }
        if konto.kaufe(historie, kauf, grenzen).is_none() {
            konto.luecke();
        }
    }
    for (_, betrag) in kosten {
        konto.belaste(*betrag);
    }
    konto
}

fn eunl(historie: &Historie, zeitraum: Zeitraum, grenzen: &Grenzen) -> Option<(Cent, usize)> {
    let isin = Isin::neu("IE00B4L5Y983").ok()?;
    let boerse = Boerse::Xetra;
    let ende = zeitraum.schluss(boerse)?;
    let anfang = std::iter::once(zeitraum.von)
        .chain(handelstage_ab(boerse, zeitraum.von))
        .take_while(|&tag| tag <= ende)
        .find(|&tag| boerse.handelstag(tag))?;
    let tage: Vec<_> = std::iter::once(anfang)
        .chain(handelstage_ab(boerse, anfang).take_while(|&tag| tag <= ende))
        .collect();
    let kurse = historie.fenster(&isin, &tage)?;
    let kurs = grenzen.papierkurs(boerse, kurse.first()?.preis, Rundung::Auf)?;
    let stueck = stuecke(START, kurs, grenzen)?;
    let bar = START.checked_sub(kaufbetrag(kurs, stueck, grenzen)?)?;
    let wert = bar.checked_add(verkauf(boerse, kurse.last()?.preis, stueck, grenzen)?)?;
    Some((wert, usize::from(stueck > 0)))
}

pub(super) fn bericht(
    stand: &Stand,
    historie: &Historie,
    von: Datum,
    bis: Datum,
    jetzt: Zeitpunkt,
) -> String {
    let zeitraum = Zeitraum { von, bis, jetzt };
    let grenzen = Grenzen::standard();
    let befunde = offene_signale(stand, zeitraum);
    let beratung = beratung(stand, zeitraum);
    let kosten = beratungskosten(stand, zeitraum);
    let signaltage: BTreeSet<_> = befunde
        .iter()
        .filter(|b| matches!(b.befund, Befund::Signal(_)))
        .map(|b| b.tag)
        .collect();
    let allein = rechne(
        historie,
        &kaeufer(&befunde, None, zeitraum, &grenzen),
        &BTreeMap::new(),
        &grenzen,
    );
    let beraten = rechne(
        historie,
        &kaeufer(&befunde, Some(&beratung), zeitraum, &grenzen),
        &kosten,
        &grenzen,
    );
    let etf = eunl(historie, zeitraum, &grenzen);
    let marktluecken = befunde
        .iter()
        .filter(|b| matches!(b.befund, Befund::Luecke))
        .count();
    let mut text = format!(
        "\n## Vier Schattenkonten\n\nModellrechnung mit je 5.000 € Startkapital im Exportfenster {von} bis {bis}. Bargeld bleibt unverzinst. Beide Aufwärtsdruck-Konten nutzen vorab protokollierte Signale, höchstens eine offene Position und bis zu 40 % des belegten Kontowerts pro Einstieg. Es gelten Standardlimit, ganze Stücke, Papiergebühren und Modellspread. Der Ausstieg erfolgt am fünften folgenden Börsentag. Fehlt der Ausstiegskurs, bleibt der Erlös offen. Weitere Käufe verwenden das belegte Restbargeld. Offene Positionen bewertet der Bericht am letzten vorgesehenen Schluss nach modellierten Verkaufskosten.\n\nDas Beratungskonto verlangt zusätzlich einen belegten gemeinsamen Kaufstand für denselben Wert am Signaltag und wartet dessen Bereitschaft ab. Es trägt einmal die Hälfte der gebuchten Erst- und Zweitmeinungs- sowie Abstimmungskosten des gesamten Exportfensters, insgesamt auf ganze Cent aufgerundet. Ohne Beratung fallen keine Kosten an. Offene Abrechnungen lassen den Endwert offen. EUNL hält ab dem ersten Xetra-Schluss im Fenster bis zum letzten vorgesehenen Schluss. Die fortgeschriebenen Journalstrategien Stärke, Zahlen und Korb laufen separat. Der Abschnitt EUNL und €STR zeigt zusätzlich verzinstes Bargeld.\n\nBörsentag-Belege: {}, davon {marktluecken} mit Marktlücke. Tage mit Aufwärtsdruck-Signal: {}. Fehlende Kurs- oder Kostenbelege können einen Endwert offenlassen.\n\n| Konto | Modell-Endwert | Netto | Käufe | Abgerechnete Beratungskosten |\n|---|---:|---:|---:|---:|\n",
        befunde.len(),
        signaltage.len()
    );
    let zeilen = [
        ("Nur Bargeld", Some(START), Some(0), Cent::NULL),
        (
            "Aufwärtsdruck allein",
            allein.wert(historie, zeitraum, &grenzen),
            Some(allein.kaeufe),
            Cent::NULL,
        ),
        (
            "Aufwärtsdruck mit Beratung",
            beraten.wert(historie, zeitraum, &grenzen),
            Some(beraten.kaeufe),
            beraten.kosten,
        ),
        (
            "Welt-ETF EUNL",
            etf.map(|(wert, _)| wert),
            etf.map(|(_, anzahl)| anzahl),
            Cent::NULL,
        ),
    ];
    let geld =
        |betrag: Option<Cent>| betrag.map_or_else(|| "nicht belegt".into(), |c| c.to_string());
    for (name, wert, anzahl, gebuehren) in zeilen {
        let anzahl = anzahl.map_or_else(|| "–".into(), |n| n.to_string());
        let _ = writeln!(
            text,
            "| {name} | {} | {} | {anzahl} | {gebuehren} |",
            geld(wert),
            geld(wert.and_then(|c| c.checked_sub(START))),
        );
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_journal::{Journal, KursBeleg};
    use depot_kern::{Herkunft, Kurs, Preis};

    #[test]
    fn bewertung_vor_boersenschluss_am_wochenende_und_mit_kursluecke() -> Result<(), String> {
        let freitag = Datum::lies("2026-11-20").ok_or("Freitag")?;
        let montag = Datum::lies("2026-11-23").ok_or("Montag")?;
        let sonntag = montag.plus_tage(-1).ok_or("Sonntag")?;
        let mittag = Zeitpunkt::frankfurt(montag, 12, 0, 0).ok_or("Mittag")?;
        let zeit = Zeitraum {
            von: freitag,
            bis: montag,
            jetzt: mittag,
        };
        assert_eq!(zeit.schluss(Boerse::Xetra), Some(freitag));
        assert_eq!(
            Zeitraum {
                bis: sonntag,
                ..zeit
            }
            .schluss(Boerse::Xetra),
            Some(freitag)
        );

        let pfad =
            std::env::temp_dir().join(format!("depot-schattenkonten-{}", std::process::id()));
        std::fs::create_dir_all(&pfad).map_err(|e| e.to_string())?;
        let journal = pfad.join("journal.txt");
        Journal::einrichten(&journal).map_err(|e| format!("{e:?}"))?;
        let mut j = Journal::oeffnen(&journal).map_err(|e| format!("{e:?}"))?;
        let isin = Isin::neu("IE00B4L5Y983").map_err(|e| format!("{e:?}"))?;
        let schluss = Boerse::Xetra.schluss(freitag).ok_or("Schluss")?;
        let kurs = Kurs::neu(Cent(10_000), "EUR", freitag, schluss, Herkunft::Quelle)
            .map_err(|e| format!("{e:?}"))?;
        let beleg = KursBeleg::neu("test", schluss, b"Probe", isin, kurs)?;
        j.belege_kurs(&beleg).map_err(|e| format!("{e:?}"))?;
        let grenzen = Grenzen::standard();
        let jetzt = Zeitpunkt::frankfurt(montag, 18, 0, 0).ok_or("Abend")?;
        let historie = Historie::laden(j.stand(), jetzt);
        let (wert, kaeufe) = eunl(&historie, zeit, &grenzen).ok_or("EUNL")?;
        assert_eq!(kaeufe, 1);
        assert!(wert < START);
        assert!(eunl(&historie, Zeitraum { jetzt, ..zeit }, &grenzen).is_none());
        let bericht = bericht(j.stand(), &historie, freitag, montag, jetzt);
        for konto in [
            "Nur Bargeld",
            "Aufwärtsdruck allein",
            "Aufwärtsdruck mit Beratung",
            "Welt-ETF EUNL",
        ] {
            assert!(bericht.contains(&format!("| {konto} |")));
        }
        drop(j);
        std::fs::remove_dir_all(pfad).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn gemeinsame_beratung_nutzt_erst_den_schluss_nach_freigabe() -> Result<(), String> {
        let montag = Datum::lies("2026-11-23").ok_or("Montag")?;
        let dienstag = montag.plus_tage(1).ok_or("Dienstag")?;
        let freitag = Boerse::Xetra.bezugstag(montag).ok_or("Freitag")?;
        let bereit = Zeitpunkt::frankfurt(montag, 9, 0, 0).ok_or("Signalzeit")?;
        let freigabe = Zeitpunkt::frankfurt(montag, 18, 0, 0).ok_or("Beratungszeit")?;
        let jetzt = Zeitpunkt::frankfurt(dienstag, 18, 0, 0).ok_or("Exportzeit")?;
        let zeitraum = Zeitraum {
            von: montag,
            bis: dienstag,
            jetzt,
        };
        let isin = Isin::neu("DE0007164600").map_err(|e| format!("{e:?}"))?;
        let befunde = [Beobachtung {
            tag: montag,
            boerse: Boerse::Xetra,
            bereit,
            bezugstag: freitag,
            wissen: bereit,
            befund: Befund::Signal(Idee {
                isin: isin.clone(),
                referenz: Preis::aus_cent(Cent(10_000)).ok_or("Referenz")?,
                gruppe: "Software".into(),
                markt_rueckgang_bp: -50,
                markt_erholung_bp: 10,
                gruppe_rueckgang_bp: 10,
                gruppe_erholung_bp: 20,
                breite_rueckgang: 2,
                breite_erholung: 2,
                titel: 3,
                referenz_abruf: bereit,
            }),
        }];
        let grenzen = Grenzen::standard();
        let allein = kaeufer(&befunde, None, zeitraum, &grenzen);
        assert_eq!(
            allein.first().map(|k| k.schluss),
            Boerse::Xetra.schluss(montag)
        );
        assert!(kaeufer(&befunde, Some(&BTreeMap::new()), zeitraum, &grenzen).is_empty());
        let beratung = [(
            montag,
            Beratung {
                gemeinsame_zeit: [(isin, freigabe)].into(),
            },
        )]
        .into();
        let beraten = kaeufer(&befunde, Some(&beratung), zeitraum, &grenzen);
        assert_eq!(
            beraten.first().map(|k| k.schluss),
            Boerse::Xetra.schluss(dienstag)
        );
        let historienluecke = Historie::default();
        let mut konto = Konto::neu();
        konto.position = Some(Position {
            isin: befunde
                .first()
                .and_then(|b| match &b.befund {
                    Befund::Signal(idee) => Some(&idee.isin),
                    _ => None,
                })
                .ok_or("ISIN")?,
            boerse: Boerse::Xetra,
            kauf: freitag,
            ausstieg: montag,
            stueck: 1,
        });
        konto.verkaufe_faellig(&historienluecke, jetzt, &grenzen);
        assert!(konto.position.is_none());
        assert!(konto.wert(&historienluecke, zeitraum, &grenzen).is_none());
        let ohne_beratung = rechne(&historienluecke, &[], &BTreeMap::new(), &grenzen);
        assert_eq!(
            ohne_beratung.wert(&historienluecke, zeitraum, &grenzen),
            Some(START)
        );
        let historie = Historie::default();
        let konto = rechne(
            &historie,
            &beraten,
            &[(montag, Some(Cent(8)))].into(),
            &grenzen,
        );
        assert_eq!(konto.kosten, Cent(8));
        assert_eq!(konto.kaeufe, 0);
        assert!(konto.wert(&historie, zeitraum, &grenzen).is_none());
        Ok(())
    }
}
