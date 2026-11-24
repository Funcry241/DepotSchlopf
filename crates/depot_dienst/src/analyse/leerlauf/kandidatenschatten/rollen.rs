//! Rollen, belegte Nullstände und gepaarte Auswahlschritte im Kandidatenschatten.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use depot_journal::Stand;
use depot_kern::liste::papierorder_schluss;
use depot_kern::{Cent, Datum, Grenzen, Isin, TopfId};
use depot_lage::kandidatenschatten::{StandImSchatten, Zeile};
use depot_lage::schattenkandidaten::{Kandidat, Phase, beratungslaeufe, phasenbelege};

use super::{Zeitraum, name, prozent};

#[derive(Clone, Copy)]
struct Kennzahlen {
    netto: Cent,
    netto_bp: i64,
    korbdifferenz_bp: Option<i64>,
    signaldifferenz_bp: Option<i64>,
    gebuehr: Cent,
}

fn kennzahlen(zeile: &Zeile, grenzen: &Grenzen) -> Option<Kennzahlen> {
    let StandImSchatten::Abgeschlossen {
        einstieg,
        mit_stopp,
        korbnetto,
        signalkontrolle,
        ..
    } = &zeile.stand
    else {
        return None;
    };
    let stuecke = zeile.kandidat.stueck?;
    let kauf = einstieg.kurs.mal_stueck(stuecke)?;
    let verkauf = mit_stopp.fuellung.kurs.mal_stueck(stuecke)?;
    let gebuehren = grenzen.gebuehren();
    let kaufgebuehr = gebuehren.fuer(kauf)?;
    let basis = kauf.checked_add(kaufgebuehr)?;
    Some(Kennzahlen {
        netto: mit_stopp.netto,
        netto_bp: mit_stopp.netto.in_bp_von(basis)?,
        korbdifferenz_bp: korbnetto
            .and_then(|korb| mit_stopp.netto.checked_sub(korb)?.in_bp_von(basis)),
        signaldifferenz_bp: signalkontrolle
            .as_ref()
            .and_then(|s| s.idee_netto.checked_sub(s.signal_netto)?.in_bp_von(basis)),
        gebuehr: kaufgebuehr.checked_add(gebuehren.fuer(verkauf)?)?,
    })
}

#[derive(Default)]
struct Mittel {
    summe: i128,
    anzahl: usize,
}

impl Mittel {
    fn sammle(&mut self, wert: Option<i64>) {
        if let Some(wert) = wert {
            self.summe = self.summe.saturating_add(i128::from(wert));
            self.anzahl = self.anzahl.saturating_add(1);
        }
    }

    fn wert(&self) -> Option<i64> {
        i64::try_from(self.summe.checked_div(i128::try_from(self.anzahl).ok()?)?).ok()
    }
}

#[derive(Default)]
struct Rolle {
    beobachtete_stufen: usize,
    ohne_kaufziel: usize,
    ideen: usize,
    davon_alt: usize,
    ohne_einstieg: usize,
    offen: usize,
    luecke: usize,
    netto: Mittel,
    korbdifferenz: Mittel,
    signaldifferenz: Mittel,
    gebuehren: Mittel,
}

type Lauf = (Datum, usize, TopfId);
type Stufe = (Phase, TopfId);
type Stufenvergleich = (Phase, Phase, TopfId);

#[derive(Default)]
struct Beobachtungen<'a> {
    markierungen: usize,
    ideen: BTreeMap<&'a Isin, Option<&'a Zeile>>,
}

impl<'a> Beobachtungen<'a> {
    fn sammle(&mut self, zeile: &'a Zeile) {
        self.ideen
            .entry(&zeile.kandidat.isin)
            .and_modify(|e| *e = None)
            .or_insert(Some(zeile));
    }
}

fn modellschluss(k: &Kandidat) -> Option<depot_kern::Zeitpunkt> {
    papierorder_schluss(&k.isin, k.bereit_ab?)
}

fn gleiche_parameter(a: &Kandidat, b: &Kandidat) -> Option<bool> {
    Some(
        modellschluss(a)? == modellschluss(b)?
            && a.ziel_bp == b.ziel_bp
            && a.stueck? == b.stueck?
            && a.limit? == b.limit?
            && a.stopp_bp == b.stopp_bp
            && a.plan == b.plan,
    )
}

fn gleiches_fenster(a: &Zeile, b: &Zeile) -> bool {
    match (&a.stand, &b.stand) {
        (
            StandImSchatten::Abgeschlossen {
                einstieg: va,
                frist: ba,
                ..
            },
            StandImSchatten::Abgeschlossen {
                einstieg: vb,
                frist: bb,
                ..
            },
        ) => va.datum == vb.datum && ba == bb,
        _ => false,
    }
}

#[derive(Default)]
struct Uebergang {
    zyklen: usize,
    erhalten: usize,
    geaendert: usize,
    entfernt: usize,
    hinzu: usize,
    mehrdeutig: usize,
    entfernt_plus: usize,
    entfernt_minus: usize,
    entfernt_netto: Mittel,
    hinzu_netto: Mittel,
    differenz: Mittel,
}

impl Uebergang {
    fn vergleiche(&mut self, erst: &Beobachtungen<'_>, gemeinsam: &Beobachtungen<'_>, g: &Grenzen) {
        self.zyklen = self.zyklen.saturating_add(1);
        for (isin, quelle) in &erst.ideen {
            match (quelle, gemeinsam.ideen.get(isin)) {
                (Some(a), Some(Some(b))) => match gleiche_parameter(&a.kandidat, &b.kandidat) {
                    Some(true) => self.erhalten = self.erhalten.saturating_add(1),
                    Some(false) => {
                        self.geaendert = self.geaendert.saturating_add(1);
                        let delta = gleiches_fenster(a, b)
                            .then(|| {
                                kennzahlen(b, g)?
                                    .netto_bp
                                    .checked_sub(kennzahlen(a, g)?.netto_bp)
                            })
                            .flatten();
                        self.differenz.sammle(delta);
                    }
                    None => self.mehrdeutig = self.mehrdeutig.saturating_add(1),
                },
                (Some(a), None) => {
                    self.entfernt = self.entfernt.saturating_add(1);
                    if let Some(k) = kennzahlen(a, g) {
                        self.entfernt_plus = self
                            .entfernt_plus
                            .saturating_add(usize::from(k.netto > Cent::NULL));
                        self.entfernt_minus = self
                            .entfernt_minus
                            .saturating_add(usize::from(k.netto < Cent::NULL));
                        self.entfernt_netto.sammle(Some(k.netto_bp));
                    }
                }
                _ => self.mehrdeutig = self.mehrdeutig.saturating_add(1),
            }
        }
        for (isin, ziel) in &gemeinsam.ideen {
            if !erst.ideen.contains_key(isin) {
                if let Some(ziel) = ziel {
                    self.hinzu = self.hinzu.saturating_add(1);
                    self.hinzu_netto
                        .sammle(kennzahlen(ziel, g).map(|k| k.netto_bp));
                } else {
                    self.mehrdeutig = self.mehrdeutig.saturating_add(1);
                }
            }
        }
    }
}

fn rolle_buche(rolle: &mut Rolle, zeile: &Zeile, grenzen: &Grenzen) {
    rolle.ideen = rolle.ideen.saturating_add(1);
    if zeile.kandidat.lauf.is_none() {
        rolle.davon_alt = rolle.davon_alt.saturating_add(1);
    }
    match &zeile.stand {
        StandImSchatten::Abgeschlossen { .. } => {
            if let Some(k) = kennzahlen(zeile, grenzen) {
                rolle.netto.sammle(Some(k.netto_bp));
                rolle.korbdifferenz.sammle(k.korbdifferenz_bp);
                rolle.signaldifferenz.sammle(k.signaldifferenz_bp);
                rolle.gebuehren.sammle(Some(k.gebuehr.0));
            } else {
                rolle.luecke = rolle.luecke.saturating_add(1);
            }
        }
        StandImSchatten::KeineStuecke | StandImSchatten::LimitVerfehlt(_) => {
            rolle.ohne_einstieg = rolle.ohne_einstieg.saturating_add(1);
        }
        StandImSchatten::EinstiegOffen | StandImSchatten::Haelt(_) => {
            rolle.offen = rolle.offen.saturating_add(1);
        }
        _ => rolle.luecke = rolle.luecke.saturating_add(1),
    }
}

fn mit_anzahl(mittel: &Mittel) -> String {
    format!("{} (n = {})", prozent(mittel.wert()), mittel.anzahl)
}

fn uebersicht(text: &mut String, rollen: BTreeMap<Stufe, Rolle>) {
    text.push_str("\n#### Ergebnisse nach Beratungsstufe\n\nJede Zeile beschreibt die beobachteten Kaufideen dieser Stufe. Ein Kandidat kann in mehreren Zeilen erscheinen. „Ohne Kaufziel“ zählt nur ausdrücklich belegte Stufen. Die Durchschnittsrendite beruht auf abgeschlossenen Einzelideen, einschließlich Modellspread und Handelsgebühren. Der Nenner ist der modellierte Kaufpreis einschließlich Kaufgebühr. Die Korbdifferenz zählt nur Fälle mit einem belegten Vergleich im selben Fenster und mit gleichem Kapital. Für die Differenz zur 20-Tage-Signalregel halten beide Kandidaten bis zum selben Fensterende. Die getrennt erfassten KI-Kosten stehen in ihrer eigenen Auswertung.\n\n| Stufe | Topf | Belegte Stufen | Ohne Kaufziel | Ideen (davon alt) | Ohne Einstieg | Offen | Datenlücke | Ø netto zu Bargeld | Ø netto zu Korb | Ø netto zur Signalregel | Ø Handelsgebühren je Abschluss |\n|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n");
    for ((phase, topf), r) in rollen {
        let gebuehr = r
            .gebuehren
            .wert()
            .map_or_else(|| "–".into(), |n| Cent(n).to_string());
        let _ = writeln!(
            text,
            "| {} | {topf:?} | {} | {} | {} ({}) | {} | {} | {} | {} | {} | {} | {gebuehr} |",
            name(phase, topf),
            r.beobachtete_stufen,
            r.ohne_kaufziel,
            r.ideen,
            r.davon_alt,
            r.ohne_einstieg,
            r.offen,
            r.luecke,
            mit_anzahl(&r.netto),
            mit_anzahl(&r.korbdifferenz),
            mit_anzahl(&r.signaldifferenz)
        );
    }
}

fn auswahl(text: &mut String, uebersicht: BTreeMap<Stufenvergleich, Uebergang>) {
    text.push_str("\n#### Änderungen der Kandidatenauswahl\n\nDepotSchlopf paart je Topf ausdrücklich belegte Stufen aus demselben abgeschlossenen Beratungslauf. „Entfallen“ bezeichnet eine Idee, die im späteren Stand fehlt. „Plus / Minus“ zählt abgeschlossene entfallene Ideen mit positivem oder negativem Modellnetto gegenüber Bargeld. Die Modellwerte beschreiben einzelne Kaufideen und ihre Auswahl im beobachteten Verlauf. Für geänderte Ideen erfasst die Differenz nur Fälle mit identischem Einstiegstag und Fensterende.\n\n");
    if uebersicht.is_empty() {
        text.push_str(
            "Im Zeitraum fehlen paarweise belegte Stufen aus abgeschlossenen Beratungsläufen.\n",
        );
        return;
    }
    text.push_str("| Früherer Stand | Späterer Stand | Läufe | Gleich geblieben | Geändert | Entfallen | Hinzugekommen | Mehrdeutig | Entfallen mit Plus / Minus | Ø netto entfallene | Ø netto neue | Ø Differenz bei Änderung |\n|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n");
    for ((vorher, phase, topf), s) in uebersicht {
        let _ = writeln!(
            text,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} / {} | {} | {} | {} |",
            name(vorher, topf),
            name(phase, topf),
            s.zyklen,
            s.erhalten,
            s.geaendert,
            s.entfernt,
            s.hinzu,
            s.mehrdeutig,
            s.entfernt_plus,
            s.entfernt_minus,
            mit_anzahl(&s.entfernt_netto),
            mit_anzahl(&s.hinzu_netto),
            mit_anzahl(&s.differenz)
        );
    }
}

pub(super) fn bericht(text: &mut String, stand: &Stand, zeilen: &[Zeile], zeitraum: Zeitraum) {
    let mut stufen = BTreeMap::<(Lauf, Phase), Beobachtungen<'_>>::new();
    for beleg in phasenbelege(stand)
        .into_iter()
        .filter(|b| (zeitraum.von..=zeitraum.bis).contains(&b.tag))
    {
        let stufe = stufen
            .entry(((beleg.tag, beleg.lauf, beleg.topf), beleg.phase))
            .or_default();
        stufe.markierungen = stufe.markierungen.saturating_add(1);
    }
    let grenzen = Grenzen::standard();
    let mut rollen = BTreeMap::<Stufe, Rolle>::new();
    for z in zeilen {
        let k = &z.kandidat;
        rolle_buche(rollen.entry((k.phase, k.topf)).or_default(), z, &grenzen);
        if let Some(lauf) = k.lauf {
            stufen
                .entry(((k.tag, lauf, k.topf), k.phase))
                .or_default()
                .sammle(z);
        }
    }
    let abgeschlossen: BTreeSet<_> = beratungslaeufe(stand)
        .into_iter()
        .map(|b| (b.tag, b.lauf))
        .collect();
    let mut vergleiche = BTreeMap::<Stufenvergleich, Uebergang>::new();
    for (&(paar @ (tag, lauf, topf), phase), stufe) in &stufen {
        if stufe.markierungen != 1 {
            continue;
        }
        let r = rollen.entry((phase, topf)).or_default();
        r.beobachtete_stufen = r.beobachtete_stufen.saturating_add(1);
        if stufe.ideen.is_empty() {
            r.ohne_kaufziel = r.ohne_kaufziel.saturating_add(1);
        }
        if !abgeschlossen.contains(&(tag, lauf)) {
            continue;
        }
        let folgestufen: &[Phase] = match phase {
            Phase::Erstmeinung => &[Phase::Zweitmeinung, Phase::Gemeinsam],
            Phase::Zweitmeinung => &[Phase::Gemeinsam],
            _ => &[],
        };
        for &danach in folgestufen {
            if let Some(vergleich) = stufen.get(&(paar, danach)).filter(|v| v.markierungen == 1) {
                vergleiche
                    .entry((phase, danach, topf))
                    .or_default()
                    .vergleiche(stufe, vergleich, &grenzen);
            }
        }
    }
    if rollen.is_empty() {
        text.push_str("Im Zeitraum fehlen strukturierte Stufenbelege. Für ältere Läufe lässt sich eine fehlende Kaufidee keinem Beratungsstand zuordnen.\n");
    } else {
        uebersicht(text, rollen);
    }
    auswahl(text, vergleiche);
}

#[cfg(test)]
mod tests {
    use std::fs;

    use depot_journal::Journal;
    use depot_kern::Zeitpunkt;
    use depot_lage::kandidatenschatten::{Abschluss, Ausstieg, Fuellung, Signalkontrolle};

    use super::*;

    fn abgeschlossener_fall(tag: Datum, frist: Datum) -> Result<Zeile, String> {
        let kandidat = Kandidat {
            tag,
            lauf: Some(10),
            topf: TopfId::B,
            phase: Phase::Erstmeinung,
            isin: Isin::neu("DE0005557508").map_err(|e| format!("{e:?}"))?,
            ziel_bp: 2000,
            stueck: Some(10),
            limit: Some(Cent(10_100)),
            stopp_bp: Some(300),
            plan: None,
            bereit_ab: Zeitpunkt::frankfurt(tag, 7, 36, 0),
        };
        let stand = StandImSchatten::Abgeschlossen {
            einstieg: Fuellung {
                datum: tag,
                kurs: Cent(10_000),
            },
            frist,
            mit_stopp: Abschluss {
                fuellung: Fuellung {
                    datum: frist,
                    kurs: Cent(10_130),
                },
                grund: Ausstieg::Frist,
                netto: Cent(1000),
            },
            mit_ziel: None,
            korbnetto: Some(Cent(200)),
            signalkontrolle: Some(Signalkontrolle {
                isin: kandidat.isin.clone(),
                idee_netto: Cent(900),
                signal_netto: Cent(400),
            }),
        };
        Ok(Zeile { kandidat, stand })
    }

    #[test]
    fn endstand_ist_nur_bei_belegtem_selben_lauf_ein_auswahlvergleich() -> Result<(), String> {
        let tag = Datum::neu(2026, 11, 24).ok_or("Tag")?;
        let frist = tag.plus_tage(7).ok_or("Frist")?;
        let abgeschlossen = abgeschlossener_fall(tag, frist)?;
        let kandidat = abgeschlossen.kandidat.clone();
        let zeilen = [
            abgeschlossen,
            Zeile {
                kandidat: Kandidat {
                    lauf: Some(40),
                    ..kandidat.clone()
                },
                stand: StandImSchatten::EinstiegOffen,
            },
            Zeile {
                kandidat: Kandidat {
                    lauf: None,
                    ..kandidat
                },
                stand: StandImSchatten::FehlendeKaufdaten,
            },
        ];
        let ordner =
            std::env::temp_dir().join(format!("depot-schattenrollen-{}", crate::neues_token()));
        fs::create_dir(&ordner).map_err(|e| e.to_string())?;
        let test = || -> Result<(), String> {
            let pfad = ordner.join("journal.txt");
            let fehler = |e| format!("{e:?}");
            Journal::einrichten(&pfad).map_err(fehler)?;
            let mut j = Journal::oeffnen(&pfad).map_err(fehler)?;
            for (wer, beleg) in [
                ("Schattenberatung", "v1 Beginn 10 0"),
                ("Schattenphase", "v1 10 B Erstmeinung"),
                ("Schattenphase", "v1 10 B Zweitmeinung"),
                ("Schattenphase", "v1 10 B Gemeinsam"),
                ("Schattenberatung", "v1 Ende 10 0"),
                ("Schattenberatung", "v1 Beginn 20 0"),
                ("Schattenphase", "v1 20 A Erstmeinung"),
                ("Schattenphase", "v1 20 A Gemeinsam"),
                ("Schattenberatung", "v1 Ende 20 0"),
                ("Schattenberatung", "v1 Beginn 40 0"),
                ("Schattenphase", "v1 40 B Erstmeinung"),
                ("Schattenphase", "v1 40 B Gemeinsam"),
                ("Schattenberatung", "v1 Beginn 50 0"),
                ("Schattenphase", "v1 50 B Erstmeinung"),
                ("Schattenberatung", "v1 Ende 50 0"),
            ] {
                j.begruende(tag, wer, beleg).map_err(fehler)?;
            }
            let mut ausgabe = String::new();
            bericht(
                &mut ausgabe,
                j.stand(),
                &zeilen,
                Zeitraum {
                    von: tag,
                    bis: frist,
                },
            );
            assert!(ausgabe.contains("| Dachs · Erstmeinung | A | 1 | 1 | 0 (0) |"));
            assert!(ausgabe.contains("| Maus · Erstmeinung | B | 3 | 1 | 3 (1) | 0 | 1 | 1 |"));
            assert!(
                ausgabe.contains("| 0,99 % (n = 1) | 0,79 % (n = 1) | 0,49 % (n = 1) | 3,00 € |")
            );
            assert!(ausgabe.contains(
                "| Maus · Erstmeinung | Gemeinsamer Stand | 1 | 0 | 0 | 1 | 0 | 0 | 1 / 0 |"
            ));
            assert!(
                ausgabe.contains(
                    "| Dachs · Zweitmeinung | Gemeinsamer Stand | 1 | 0 | 0 | 0 | 0 | 0 |"
                )
            );
            Ok(())
        };
        let ergebnis = test();
        fs::remove_dir_all(ordner).map_err(|e| e.to_string())?;
        ergebnis
    }
}
