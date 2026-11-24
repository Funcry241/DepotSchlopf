//! Bruttovorsprung eines vorab definierten 20-Tage-Rangs in fünf Gruppen.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use depot_kern::liste::{Wert, boerse_von};
use depot_kern::{Boerse, Datum, Isin, Zeitpunkt};

use super::ausstiegsvergleich::{Historie, handelstage_ab, kursrendite};
use super::{BLICK_ZURUECK, gemeinsamer_tag};

const HALTEFRISTEN: [usize; 3] = [1, 5, 10];
const GRUPPEN: usize = 5;

#[derive(Clone, Copy)]
struct Rendite {
    bp: i64,
    abruf: Zeitpunkt,
}

impl Rendite {
    fn lesbar(self, stichtag: Option<Zeitpunkt>) -> bool {
        stichtag.is_none_or(|stichtag| self.abruf < stichtag)
    }
}

fn rendite(historie: &Historie, isin: &Isin, tage: &[Datum]) -> Option<Rendite> {
    let kurse = historie.fenster(isin, tage)?;
    Some(Rendite {
        bp: kursrendite(&kurse)?,
        abruf: kurse.iter().map(|schluss| schluss.abruf).max()?,
    })
}

fn gleicher_korb(werte: impl Iterator<Item = Option<Rendite>>) -> Option<Rendite> {
    let renditen = werte.collect::<Option<Vec<_>>>()?;
    let summe = renditen
        .iter()
        .try_fold(0_i128, |s, r| s.checked_add(i128::from(r.bp)))?;
    Some(Rendite {
        bp: i64::try_from(summe.checked_div(i128::try_from(renditen.len()).ok()?)?).ok()?,
        abruf: renditen.iter().map(|r| r.abruf).max()?,
    })
}

fn rangfenster(einstieg: Datum, anfang: Datum) -> Option<Vec<Datum>> {
    let mut tage: Vec<_> = std::iter::successors(einstieg.plus_tage(-1), |tag| tag.plus_tage(-1))
        .take_while(|tag| *tag >= anfang)
        .filter(|&tag| gemeinsamer_tag(tag))
        .take(BLICK_ZURUECK.saturating_add(1))
        .collect();
    if tage.len() <= BLICK_ZURUECK {
        return None;
    }
    tage.reverse();
    Some(tage)
}

enum Fenster {
    Offen,
    Luecke,
    Vollstaendig { tage: Vec<Datum>, korb_bp: i64 },
}

#[derive(Clone, Copy)]
enum Ausgang {
    Offen,
    Luecke,
    Vollstaendig { brutto_bp: i64, korb_bp: i64 },
}

impl Fenster {
    fn neu(
        historie: &Historie,
        korb: &[Isin],
        boerse: Boerse,
        einstieg: Datum,
        dauer: usize,
        jetzt: Zeitpunkt,
    ) -> Self {
        let tage: Vec<_> = std::iter::once(einstieg)
            .chain(handelstage_ab(boerse, einstieg).take(dauer))
            .collect();
        let Some(ende) = tage.last().and_then(|&d| boerse.schluss(d)) else {
            return Self::Luecke;
        };
        if tage.len() <= dauer {
            return Self::Luecke;
        }
        if ende > jetzt {
            return Self::Offen;
        }
        let lokal = korb.iter().filter(|isin| boerse_von(isin) == Some(boerse));
        match gleicher_korb(lokal.map(|isin| rendite(historie, isin, &tage))) {
            Some(korb) => Self::Vollstaendig {
                tage,
                korb_bp: korb.bp,
            },
            None => Self::Luecke,
        }
    }

    fn ausgang(&self, historie: &Historie, isin: &Isin) -> Ausgang {
        match self {
            Self::Offen => Ausgang::Offen,
            Self::Luecke => Ausgang::Luecke,
            Self::Vollstaendig { tage, korb_bp } => match rendite(historie, isin, tage) {
                Some(wert) => Ausgang::Vollstaendig {
                    brutto_bp: wert.bp,
                    korb_bp: *korb_bp,
                },
                None => Ausgang::Luecke,
            },
        }
    }
}

struct Beobachtung<'a> {
    isin: &'a Isin,
    rueckblick: Rendite,
    ausgaenge: [Ausgang; HALTEFRISTEN.len()],
}

fn quintile(rangfolge: usize, raenge: usize) -> Option<usize> {
    rangfolge.checked_mul(GRUPPEN)?.checked_div(raenge)
}

#[derive(Default)]
struct Gruppe {
    vollstaendig: usize,
    luecken: usize,
    offen: usize,
    summe_brutto: i128,
    summe_korb: i128,
    wochen: BTreeSet<Datum>,
}

impl Gruppe {
    fn buche(&mut self, ausgang: Ausgang, woche: Datum) {
        match ausgang {
            Ausgang::Offen => self.offen = self.offen.saturating_add(1),
            Ausgang::Luecke => self.luecken = self.luecken.saturating_add(1),
            Ausgang::Vollstaendig { brutto_bp, korb_bp } => {
                self.vollstaendig = self.vollstaendig.saturating_add(1);
                self.summe_brutto = self.summe_brutto.saturating_add(i128::from(brutto_bp));
                self.summe_korb = self.summe_korb.saturating_add(i128::from(korb_bp));
                self.wochen.insert(woche);
            }
        }
    }

    fn zeile(&self, text: &mut String, rang: usize, dauer: usize) {
        let nenner = i128::try_from(self.vollstaendig).unwrap_or_default();
        let mittel = |n: i128| {
            n.checked_div(nenner)
                .map_or_else(|| "–".into(), |bp| bp.to_string())
        };
        let _ = writeln!(
            text,
            "| {rang} | {dauer} | {} | {} | {} | {} | {} | {} | {} |",
            self.vollstaendig,
            self.luecken,
            self.offen,
            self.wochen.len(),
            mittel(self.summe_brutto),
            mittel(self.summe_korb),
            mittel(self.summe_brutto.saturating_sub(self.summe_korb)),
        );
    }
}

#[derive(Default)]
struct Woche {
    moegliche_werte: usize,
    belegte_raenge: usize,
    volle_fenster: [usize; HALTEFRISTEN.len()],
}

#[derive(Default)]
struct Sicht {
    gruppen: [[Gruppe; HALTEFRISTEN.len()]; GRUPPEN],
    wochen: BTreeMap<Datum, Woche>,
}

impl Sicht {
    fn buche(
        &mut self,
        montag: Datum,
        moegliche_werte: usize,
        korb: Option<Rendite>,
        werte: &[Beobachtung<'_>],
        stichtag: Option<Zeitpunkt>,
    ) {
        let woche = self.wochen.entry(montag).or_default();
        woche.moegliche_werte = woche.moegliche_werte.saturating_add(moegliche_werte);
        let Some(korb) = korb.filter(|korb| korb.lesbar(stichtag)) else {
            return;
        };
        let mut rang: Vec<_> = werte
            .iter()
            .filter(|wert| wert.rueckblick.lesbar(stichtag))
            .filter_map(|wert| Some((wert.rueckblick.bp.checked_sub(korb.bp)?, wert)))
            .collect();
        if rang.len() < GRUPPEN {
            return;
        }
        rang.sort_by(|(a, x), (b, y)| b.cmp(a).then_with(|| x.isin.cmp(y.isin)));
        let anzahl = rang.len();
        woche.belegte_raenge = woche.belegte_raenge.saturating_add(anzahl);
        for (platz, (_, wert)) in rang.into_iter().enumerate() {
            let Some(gruppe) = quintile(platz, anzahl).and_then(|i| self.gruppen.get_mut(i)) else {
                continue;
            };
            for ((gruppe, voll), ausgang) in gruppe
                .iter_mut()
                .zip(&mut woche.volle_fenster)
                .zip(wert.ausgaenge)
            {
                gruppe.buche(ausgang, montag);
                if matches!(ausgang, Ausgang::Vollstaendig { .. }) {
                    *voll = voll.saturating_add(1);
                }
            }
        }
    }

    fn schreibe(&self, text: &mut String, titel: &str) {
        let _ = writeln!(text, "\n### {titel}\n");
        if self.wochen.is_empty() {
            text.push_str("Kein auswertbarer Starttag in der gespeicherten Geschichte.\n");
            return;
        }
        text.push_str("| Rangfünftel | Haltedauer in Börsentagen | Vollständig | Lücke | Offen | Wochen mit vollständigen Fenstern | Ø Brutto (bp) | Ø Korb (bp) | Ø Abstand (bp) |\n|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n");
        for (i, fristen) in self.gruppen.iter().enumerate() {
            for (gruppe, dauer) in fristen.iter().zip(HALTEFRISTEN) {
                gruppe.zeile(text, i.saturating_add(1), dauer);
            }
        }
        text.push_str("\n| Woche ab Montag | Mögliche Wertränge | Verwendete Wertränge | Vollständig 1 Tag | Vollständig 5 Tage | Vollständig 10 Tage |\n|---|---:|---:|---:|---:|---:|\n");
        for (montag, woche) in &self.wochen {
            let [ein, fuenf, zehn] = woche.volle_fenster;
            let _ = writeln!(
                text,
                "| {montag} | {} | {} | {ein} | {fuenf} | {zehn} |",
                woche.moegliche_werte, woche.belegte_raenge
            );
        }
    }
}

struct Messung {
    verfuegbar: Sicht,
    kursmodell: Sicht,
}

fn montag(tag: Datum) -> Option<Datum> {
    tag.plus_tage(1_i64.checked_sub(i64::from(tag.wochentag()?))?)
}

fn tagesbeobachtungen<'a>(
    historie: &Historie,
    werte: &'a [Wert],
    korb: &[Isin],
    blick: &[Datum],
    tag: Datum,
    jetzt: Zeitpunkt,
) -> Vec<Beobachtung<'a>> {
    [Boerse::Xetra, Boerse::Nyse]
        .into_iter()
        .filter(|&boerse| boerse.handelstag(tag))
        .flat_map(|boerse| {
            let fenster =
                HALTEFRISTEN.map(|dauer| Fenster::neu(historie, korb, boerse, tag, dauer, jetzt));
            werte
                .iter()
                .filter(move |wert| wert.boerse == boerse)
                .filter_map(move |wert| {
                    let rueckblick = rendite(historie, &wert.isin, blick)?;
                    let ausgaenge = fenster.each_ref().map(|f| f.ausgang(historie, &wert.isin));
                    Some(Beobachtung {
                        isin: &wert.isin,
                        rueckblick,
                        ausgaenge,
                    })
                })
        })
        .collect()
}

fn messe(historie: &Historie, werte: &[Wert], korb: &[Isin], jetzt: Zeitpunkt) -> Option<Messung> {
    let anfang = historie.signaltage().next()?;
    let bis = jetzt.frankfurt_tag()?;
    let mut verfuegbar = Sicht::default();
    let mut kursmodell = Sicht::default();
    for tag in
        std::iter::successors(anfang.plus_tage(1), |d| d.plus_tage(1)).take_while(|d| *d <= bis)
    {
        let stichtag = [Boerse::Xetra, Boerse::Nyse]
            .into_iter()
            .filter(|&boerse| boerse.handelstag(tag))
            .filter_map(|boerse| boerse.schluss(tag))
            .min();
        let Some(((blick, woche), stichtag)) = rangfenster(tag, anfang)
            .zip(montag(tag))
            .zip(stichtag)
            .filter(|(_, stichtag)| *stichtag <= jetzt)
        else {
            continue;
        };
        let moegliche_werte = werte
            .iter()
            .filter(|wert| wert.boerse.handelstag(tag))
            .count();
        let korb_rendite = gleicher_korb(korb.iter().map(|isin| rendite(historie, isin, &blick)));
        let ausgaenge = tagesbeobachtungen(historie, werte, korb, &blick, tag, jetzt);
        verfuegbar.buche(
            woche,
            moegliche_werte,
            korb_rendite,
            &ausgaenge,
            Some(stichtag),
        );
        kursmodell.buche(woche, moegliche_werte, korb_rendite, &ausgaenge, None);
    }
    Some(Messung {
        verfuegbar,
        kursmodell,
    })
}

pub(super) fn bericht(
    text: &mut String,
    historie: &Historie,
    jetzt: Zeitpunkt,
    korb: &[Isin],
    werte: Option<&[Wert]>,
) {
    text.push_str(concat!(
        "\n## Historische Signalprüfung nach Rangfünfteln (M0)\n\n",
        "Das Modell wertet die gesamte gespeicherte Kurshistorie mit der heutigen Beobachtungsliste aus. Jeder Einstiegstag verwendet die Rendite seit 20 gemeinsamen Handelstagen bis zum letzten gemeinsamen Schluss vor dem Einstieg. Den Rang bestimmt der Abstand zum gleich gewichteten festen Korb. Dazu brauchen der Wert und alle Korbmitglieder eine vollständige Kursreihe. Wenn nur eine Börse geöffnet ist, zählen nur deren handelnde Werte. Ab fünf belegten Werten teilt der Bericht die sortierte Liste in fünf Gruppen. Gruppe 1 ist die stärkste, bei Gleichstand entscheidet die ISIN.\n\n",
        "Die Renditemessung beginnt am Schluss des Einstiegstags. Wert und gleich gewichteter Teilkorb seines Börsenplatzes nutzen dieselben Tage und enden nach 1, 5 oder 10 Sitzungen an diesem Börsenplatz. Angezeigt sind reine Kursrenditen ohne Ausschüttungen, Spread, Gebühren, Kaufgrenzen und KI-Kosten. Offene Zeitfenster zählen bis zum vorgesehenen Schluss als offen. Fehlende Kurse und Brüche zählen danach als Lücke. Wochen richten sich nach dem Einstiegstag. Die überlappenden Fenster können voneinander abhängen.\n\n",
        "Die erste Sicht setzt voraus, dass der Dienst alle Kursbelege des Rangs vor dem ersten Börsenschluss des Einstiegstags abgerufen hatte. Die zweite Sicht verwendet zusätzlich später nachgeladene frühere Schlusskurse und geht davon aus, dass sie am jeweiligen Schluss verfügbar waren. Beide Sichten bereinigen die Kursreihen anhand der dem Export bekannten Kapitalmaßnahmen und Kursbrüche.\n"
    ));
    let Some(werte) = werte else {
        text.push_str("\nDie Beobachtungsliste ist nicht lesbar.\n");
        return;
    };
    let Some(messung) = messe(historie, werte, korb, jetzt) else {
        text.push_str("\nKeine geeigneten Quellkurse vorhanden.\n");
        return;
    };
    messung.verfuegbar.schreibe(
        text,
        "Vor dem ersten Börsenschluss des Einstiegstags belegter Rang",
    );
    messung.kursmodell.schreibe(
        text,
        "Kursmodell mit später nachgeladenen historischen Schlusskursen",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_kern::liste::beobachtungsliste;

    #[test]
    fn spaete_belege_zaehlen_nur_zum_kursmodell() -> Result<(), String> {
        let tag = Datum::neu(2026, 11, 23).ok_or("Datum")?;
        let stichtag = Zeitpunkt::neu(tag, 17, 0, 0).ok_or("Stichtag")?;
        let frueh = Zeitpunkt::neu(tag, 16, 0, 0).ok_or("Abruf")?;
        let werte = beobachtungsliste()?;
        let beobachtungen: Vec<_> = (1..=6_i64)
            .zip(&werte)
            .map(|(bp, wert)| Beobachtung {
                isin: &wert.isin,
                rueckblick: Rendite {
                    bp,
                    abruf: if bp == 6 { stichtag } else { frueh },
                },
                ausgaenge: [Ausgang::Vollstaendig {
                    brutto_bp: bp,
                    korb_bp: 0,
                }; HALTEFRISTEN.len()],
            })
            .collect();
        let korb = Some(Rendite {
            bp: 0,
            abruf: frueh,
        });
        let mut verfuegbar = Sicht::default();
        let mut kursmodell = Sicht::default();
        verfuegbar.buche(tag, 6, korb, &beobachtungen, Some(stichtag));
        kursmodell.buche(tag, 6, korb, &beobachtungen, None);
        assert_eq!(
            verfuegbar.wochen.get(&tag).ok_or("Woche")?.belegte_raenge,
            5
        );
        assert_eq!(
            kursmodell.wochen.get(&tag).ok_or("Woche")?.belegte_raenge,
            6
        );

        let erste_gruppe = |sicht: &Sicht| {
            sicht
                .gruppen
                .first()?
                .first()
                .map(|gruppe| (gruppe.vollstaendig, gruppe.summe_brutto))
        };
        assert_eq!(erste_gruppe(&verfuegbar), Some((1, 5)));
        assert_eq!(erste_gruppe(&kursmodell), Some((2, 11)));
        Ok(())
    }

    #[test]
    fn offenes_fenster_und_kursluecke_folgen_dem_boersenplatz() -> Result<(), String> {
        let einstieg = Datum::neu(2026, 11, 25).ok_or("Einstieg")?;
        let folgetag = einstieg.plus_tage(1).ok_or("Folgetag")?;
        let jetzt = Zeitpunkt::neu(folgetag, 23, 0, 0).ok_or("Zeit")?;
        let historie = Historie::default();
        let korb = [];
        assert!(matches!(
            Fenster::neu(&historie, &korb, Boerse::Nyse, einstieg, 1, jetzt),
            Fenster::Offen
        ));
        assert!(matches!(
            Fenster::neu(&historie, &korb, Boerse::Xetra, einstieg, 1, jetzt),
            Fenster::Luecke
        ));
        Ok(())
    }

    #[test]
    fn rangfenster_verwendet_keinen_kurs_des_einstiegstags() -> Result<(), String> {
        let anfang = Datum::neu(2026, 10, 1).ok_or("Anfang")?;
        let einstieg = Datum::neu(2026, 11, 24).ok_or("Einstieg")?;
        let blick = rangfenster(einstieg, anfang).ok_or("Tage")?;
        assert_eq!(blick.last().copied(), einstieg.plus_tage(-1));
        assert_eq!(blick.len(), BLICK_ZURUECK.saturating_add(1));
        Ok(())
    }
}
