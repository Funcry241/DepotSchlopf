//! Invarianten für alle Abläufe (docs/MATHEMATIK.md, I2, I4, I5a; Issue #111).
//!
//! Die Tests prüfen Sätze nicht an einzelnen, ausgedachten Fällen, sondern an jeder Folge bis
//! zu einer festen Länge (vollständige Aufzählung) und an zufälligen längeren Folgen mit festem
//! Startwert. Ein roter Lauf nennt Startwert und Folge und lässt sich damit genau wiederholen.
//! Keine Abhängigkeit, kein `f64`, keine Uhr, kein Netz. Erfundene Daten (D12).

#![allow(
    clippy::many_single_char_names,
    reason = "Testcode: kurze Namen für Zufall, Tage und Spieler"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use depot_journal::{
    Art, AuftragsZustand, Ausfuehrung, Journal, JournalFehler, KiZweck, Meldung, inhalt_fuer,
    vermerk_fuer,
};
use depot_kern::{BeraterId, Cent, Datum, Isin, TopfId};
use depot_ki::{KiGrenzen, Sperre};

static ZAEHLER: AtomicU32 = AtomicU32::new(0);

/// Startwerte für alle Zufallsfolgen. Ein roter Test nennt seinen Startwert.
const STARTWERTE: [u64; 3] = [0x5EED_0001, 0xD0C5_2026, 0x0DAC_115E];

/// xorshift64: klein, ohne Abhängigkeit, mit festem Startwert genau wiederholbar.
#[derive(Debug)]
struct Zufall(u64);

impl Zufall {
    fn neu(start: u64) -> Zufall {
        Zufall(start | 1)
    }

    fn naechste(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Gleichverteilt genug in `0..n` für Testfolgen; `n == 0` ergibt 0.
    fn bis(&mut self, n: usize) -> usize {
        let n = u64::try_from(n).unwrap_or(u64::MAX).max(1);
        usize::try_from(self.naechste().checked_rem(n).unwrap_or(0)).unwrap_or(0)
    }
}

fn neuer_pfad(name: &str) -> Result<PathBuf, String> {
    let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "depot_invarianten_{name}_{}_{n}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| format!("Vorbedingung Testverzeichnis: {e}"))?;
    Ok(dir.join("journal.txt"))
}

/// Schreibt ein Journal mit genau diesen, bestätigten Einträgen.
fn schreib(p: &Path, eintraege: &[(Art, &str)]) -> Result<(), String> {
    let inhalt = inhalt_fuer(eintraege);
    let vermerk = vermerk_fuer(&inhalt, 0).ok_or("Vorbedingung: Vermerk")?;
    fs::write(p, &inhalt).map_err(|e| format!("Vorbedingung Datei: {e}"))?;
    fs::write(p.with_extension("vermerk"), vermerk)
        .map_err(|e| format!("Vorbedingung Vermerk: {e}"))
}

// ---------------------------------------------------------------------------------------------
// I4 Auftragsautomat
// ---------------------------------------------------------------------------------------------

/// Der Auftrag aller Tests: Verkauf von 10 Stück aus Topf A zum Limit 99 €.
const STUECK: u64 = 10;
const LIMIT: i64 = 9_900;
const BARGELD: i64 = 100_000;

/// Das Alphabet der Meldungen: jede Variante, dazu Ausführungen im Limit, darunter, über der
/// Menge und mit null Stück.
fn meldungen() -> Vec<(&'static str, Meldung)> {
    let aus = |stueck, kurs, gebuehr| {
        Meldung::Ausfuehrung(Ausfuehrung {
            stueck,
            kurs: Cent(kurs),
            gebuehr: Cent(gebuehr),
            kurszeit: None,
        })
    };
    vec![
        ("gesendet", Meldung::Gesendet),
        ("unbekannt", Meldung::Unbekannt),
        ("angenommen", Meldung::Angenommen),
        ("abgelehnt", Meldung::Abgelehnt),
        ("storniert", Meldung::Storniert),
        ("verfallen", Meldung::Verfallen),
        ("verworfen", Meldung::Verworfen),
        ("ausfuehrung;3;9900;150", aus(3, LIMIT, 150)),
        ("ausfuehrung;7;10000;0", aus(7, 10_000, 0)),
        ("ausfuehrung;1;9899;0", aus(1, 9_899, 0)),
        ("ausfuehrung;11;9900;0", aus(11, LIMIT, 0)),
        ("ausfuehrung;0;9900;0", aus(0, LIMIT, 0)),
    ]
}

/// Das Modell aus der Zustandstabelle in `auftrag.rs`, unabhängig vom Code nachgeschrieben.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Modell {
    zustand: AuftragsZustand,
    gefuellt: u64,
    erloes: i64,
}

impl Modell {
    fn start() -> Modell {
        Modell {
            zustand: AuftragsZustand::Reserviert,
            gefuellt: 0,
            erloes: 0,
        }
    }

    /// Der Zustand nach `m`, oder `None`, wenn die Tabelle den Schritt nicht kennt.
    fn nach(self, m: &Meldung) -> Option<Modell> {
        use AuftragsZustand as Z;
        let mut neu = self;
        neu.zustand = match (self.zustand, m) {
            (Z::Reserviert, Meldung::Gesendet) => Z::Gesendet,
            (Z::Reserviert, Meldung::Verworfen) => Z::Verworfen,
            (Z::Gesendet, Meldung::Unbekannt) => Z::Unbekannt,
            (Z::Gesendet | Z::Unbekannt, Meldung::Angenommen) => Z::Angenommen,
            (Z::Gesendet | Z::Unbekannt, Meldung::Abgelehnt) => Z::Abgelehnt,
            (Z::Angenommen | Z::Teilgefuellt, Meldung::Storniert) => Z::Storniert,
            (Z::Angenommen | Z::Teilgefuellt, Meldung::Verfallen) => Z::Verfallen,
            (Z::Angenommen | Z::Teilgefuellt, Meldung::Ausfuehrung(a)) => {
                let summe = self.gefuellt.checked_add(a.stueck)?;
                if a.stueck == 0 || summe > STUECK || a.kurs.0 < LIMIT {
                    return None;
                }
                let betrag = a.kurs.mal_stueck(a.stueck)?.checked_sub(a.gebuehr)?;
                neu.gefuellt = summe;
                neu.erloes = self.erloes.checked_add(betrag.0)?;
                if summe == STUECK {
                    Z::Gefuellt
                } else {
                    Z::Teilgefuellt
                }
            }
            _ => return None,
        };
        Some(neu)
    }
}

/// Liest das Journal mit genau dieser Folge und prüft es gegen das Modell. Leer heißt: in
/// Ordnung.
fn pruefe_folge(
    p: &Path,
    folge: &[usize],
    alphabet: &[(&str, Meldung)],
) -> Result<Vec<String>, String> {
    let mut texte = vec![
        (Art::Topf, "A;100000;DE0007164600:10".to_string()),
        (
            Art::Auftrag,
            format!("1;A;DE0007164600;Verkauf;{STUECK};{LIMIT};Notbremse"),
        ),
    ];
    let mut modell = Some(Modell::start());
    let mut namen = Vec::new();
    for &i in folge {
        let (name, m) = alphabet.get(i).ok_or("Vorbedingung: Alphabet")?;
        texte.push((Art::Zustand, format!("1;{name}")));
        namen.push(*name);
        modell = modell.and_then(|z| z.nach(m));
    }
    let eintraege: Vec<(Art, &str)> = texte.iter().map(|(a, t)| (*a, t.as_str())).collect();
    schreib(p, &eintraege)?;
    let mut offen = Vec::new();
    match (Journal::oeffnen(p), modell) {
        (Ok(j), Some(m)) => {
            let lage = j.stand().auftrag(1).ok_or("Auftrag fehlt")?;
            if lage.zustand != m.zustand {
                offen.push(format!(
                    "{namen:?}: Zustand {:?} statt {:?}",
                    lage.zustand, m.zustand
                ));
            }
            if lage.gefuellt() != m.gefuellt || lage.gefuellt() > STUECK {
                offen.push(format!(
                    "{namen:?}: {} Stück ausgeführt statt {}",
                    lage.gefuellt(),
                    m.gefuellt
                ));
            }
            if (lage.zustand == AuftragsZustand::Gefuellt) != (lage.gefuellt() == STUECK) {
                offen.push(format!("{namen:?}: gefüllt ohne volle Menge"));
            }
            if lage.ausfuehrungen.iter().any(|a| a.kurs.0 < LIMIT) {
                offen.push(format!("{namen:?}: Ausführung unter dem Limit"));
            }
            let topf = j.stand().toepfe().get(&TopfId::A).ok_or("Topf A fehlt")?;
            let isin = Isin::neu("DE0007164600").map_err(|e| format!("ISIN: {e:?}"))?;
            let rest = STUECK.checked_sub(m.gefuellt).ok_or("Rechnung")?;
            if topf.stueck(&isin) != rest {
                offen.push(format!(
                    "{namen:?}: Bestand {} statt {rest}",
                    topf.stueck(&isin)
                ));
            }
            let bargeld = BARGELD.checked_add(m.erloes).ok_or("Rechnung")?;
            if topf.bargeld() != Cent(bargeld) {
                offen.push(format!(
                    "{namen:?}: Bargeld {:?} statt {bargeld}",
                    topf.bargeld()
                ));
            }
        }
        (Ok(j), None) => offen.push(format!(
            "{namen:?}: unzulässige Folge gelesen, Zustand {:?}",
            j.stand().auftrag(1).map(|l| l.zustand)
        )),
        (Err(JournalFehler::Halt(_)), None) => {}
        (Err(e), Some(m)) => offen.push(format!(
            "{namen:?}: zulässige Folge ({:?}) nicht lesbar: {e:?}",
            m.zustand
        )),
        (Err(e), None) => offen.push(format!("{namen:?}: unzulässige Folge ohne Halt: {e:?}")),
    }
    Ok(offen)
}

/// Jede Folge von bis zu drei Meldungen, und zufällige Folgen bis acht: Gelesen wird genau,
/// was die Zustandstabelle erlaubt. Aus einem Endzustand führt kein Schritt heraus, gesendet
/// wird höchstens einmal, ausgeführt wird höchstens die beauftragte Menge im Limit, und Topf
/// und Bargeld folgen genau den Ausführungen. Jede andere Folge ist Halt.
#[test]
fn auftrag_folgt_fuer_jede_meldungsfolge_der_zustandstabelle() -> Result<(), String> {
    let alphabet = meldungen();
    let n = alphabet.len();
    let mut folgen: Vec<Vec<usize>> = vec![Vec::new()];
    for laenge in 1..=3_u32 {
        let anzahl = n.checked_pow(laenge).ok_or("Rechnung")?;
        for mut k in 0..anzahl {
            let mut f = Vec::new();
            for _ in 0..laenge {
                f.push(k.checked_rem(n).ok_or("Rechnung")?);
                k = k.checked_div(n).ok_or("Rechnung")?;
            }
            folgen.push(f);
        }
    }
    for start in STARTWERTE {
        let mut z = Zufall::neu(start);
        for _ in 0..300 {
            // Erst ein gültiger Weg bis zur Annahme, dann Zufall: so werden auch tiefe
            // Zustände erreicht.
            let mut f = if z.bis(2) == 0 {
                vec![0, 2]
            } else {
                Vec::new()
            };
            for _ in 0..z.bis(7).checked_add(1).ok_or("Rechnung")? {
                f.push(z.bis(n));
            }
            folgen.push(f);
        }
    }
    let offen = pruefe_auftragsfolgen(&folgen, &alphabet, 4)?;
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

fn pruefe_auftragsfolgen(
    folgen: &[Vec<usize>],
    alphabet: &[(&str, Meldung)],
    arbeiterzahl: usize,
) -> Result<Vec<String>, String> {
    // Jeder Arbeiter verwendet einen eigenen Journalpfad für seine Folgen.
    std::thread::scope(|scope| -> Result<Vec<String>, String> {
        let mut arbeiter = Vec::new();
        for teil in folgen.chunks(folgen.len().div_ceil(arbeiterzahl).max(1)) {
            arbeiter.push(scope.spawn(move || -> Result<Vec<String>, String> {
                let p = neuer_pfad("auftrag")?;
                let mut offen = Vec::new();
                for f in teil {
                    offen.extend(pruefe_folge(&p, f, alphabet)?);
                    if offen.len() > 20 {
                        break;
                    }
                }
                Ok(offen)
            }));
        }
        let mut offen = Vec::new();
        for arbeit in arbeiter {
            offen.extend(arbeit.join().map_err(|_| "Auftragsprüfung abgebrochen")??);
        }
        offen.truncate(21);
        Ok(offen)
    })
}

#[test]
fn auftragsfolgen_bei_einem_und_vier_arbeitern() -> Result<(), String> {
    let alphabet = meldungen();
    let mut zufall = Zufall::neu(STARTWERTE.first().copied().ok_or("Startwert")?);
    let folgen: Vec<_> = (0..32)
        .map(|_| vec![0, 2, zufall.bis(alphabet.len())])
        .collect();
    for arbeiterzahl in [1, 4] {
        assert!(pruefe_auftragsfolgen(&folgen, &alphabet, arbeiterzahl)?.is_empty());

        let mut fehlerhafte_folgen = folgen.clone();
        fehlerhafte_folgen.push(vec![alphabet.len()]);
        assert_eq!(
            pruefe_auftragsfolgen(&fehlerhafte_folgen, &alphabet, arbeiterzahl),
            Err("Vorbedingung: Alphabet".into())
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// I5a Beleg-Rundlauf: was geschrieben wird, wird genau so wieder gelesen
// ---------------------------------------------------------------------------------------------

/// Zeichen, an denen eine Text-Kodierung typischerweise bricht.
const ZEICHEN: [char; 16] = [
    'a', 'Z', 'ä', 'ß', '€', ';', '%', '3', 'B', '2', '5', ' ', ':', ',', '"', '\\',
];

fn zufallstext(z: &mut Zufall, max: usize) -> String {
    let laenge = z.bis(max).checked_add(1).unwrap_or(1);
    let mut t: String = (0..laenge)
        .map(|_| ZEICHEN.get(z.bis(ZEICHEN.len())).copied().unwrap_or('a'))
        .collect();
    // Gefährliche Folgen gezielt einstreuen.
    for kniff in ["%3B", "%25", "%253B", ";;"] {
        if z.bis(4) == 0 {
            t.push_str(kniff);
        }
    }
    if t.trim().is_empty() {
        t.push('x');
    }
    t
}

/// Begründung und Beobachtung mit beliebigen Zeichen, `;` und Prozentkodierung
/// werden nach dem Wiederöffnen genau so gelesen, wie sie geschrieben wurden.
#[test]
fn belege_lesen_sich_genau_so_wie_sie_geschrieben_wurden() -> Result<(), String> {
    let datum = Datum::neu(2026, 11, 2).ok_or("Vorbedingung: Datum")?;
    let mut offen = Vec::new();
    for start in STARTWERTE {
        let mut z = Zufall::neu(start);
        let p = neuer_pfad("rundlauf")?;
        Journal::einrichten(&p).map_err(|e| format!("Vorbedingung Einrichtung: {e:?}"))?;
        let mut begruendungen = Vec::new();
        let mut beobachtungen = Vec::new();
        {
            let mut j = Journal::oeffnen(&p).map_err(|e| format!("Vorbedingung: {e:?}"))?;
            for _ in 0..60_u32 {
                let wer = if z.bis(2) == 0 { "Dachs" } else { "Maus" };
                let text = zufallstext(&mut z, 200);
                match j.begruende(datum, wer, &text) {
                    Ok(()) => begruendungen.push((datum, wer.to_string(), text)),
                    Err(e) => offen.push(format!(
                        "Startwert {start:#x}: Begründung abgelehnt {e:?}: {text:?}"
                    )),
                }
                let text = zufallstext(&mut z, 200);
                match j.beobachte(datum, &text) {
                    Ok(()) => beobachtungen.push((datum, text)),
                    Err(e) => offen.push(format!(
                        "Startwert {start:#x}: Beobachtung abgelehnt {e:?}: {text:?}"
                    )),
                }
            }
        }
        let j = Journal::oeffnen(&p).map_err(|e| format!("Wiederöffnen: {e:?}"))?;
        if j.stand().begruendungen() != begruendungen.as_slice() {
            offen.push(format!(
                "Startwert {start:#x}: Begründungen weichen nach dem Lesen ab"
            ));
        }
        if j.stand().beobachtungen() != beobachtungen.as_slice() {
            offen.push(format!(
                "Startwert {start:#x}: Beobachtungen weichen nach dem Lesen ab"
            ));
        }
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

// ---------------------------------------------------------------------------------------------
// I2 KI-Budget
// ---------------------------------------------------------------------------------------------

const ZWECKE: [KiZweck; 3] = [
    KiZweck::Erstmeinung,
    KiZweck::Zweitmeinung,
    KiZweck::Abstimmung,
];

/// Summen der gebuchten Kosten (Reservierung, nach der Abrechnung die echten Kosten) je KI.
fn summen(j: &Journal, wer: BeraterId, datum: Datum) -> Result<(i64, i64, usize, usize), String> {
    let mut monat = 0_i64;
    let mut jahr = 0_i64;
    let mut tag = 0_usize;
    let mut im_monat = 0_usize;
    for b in j
        .stand()
        .ki()
        .buchungen()
        .iter()
        .filter(|b| b.berater == wer)
    {
        if b.datum.jahr() != datum.jahr() {
            continue;
        }
        jahr = jahr.checked_add(b.reserviert.0).ok_or("Überlauf")?;
        if b.datum.monat() == datum.monat() {
            monat = monat.checked_add(b.reserviert.0).ok_or("Überlauf")?;
            im_monat = im_monat.checked_add(1).ok_or("Überlauf")?;
            if b.datum == datum {
                tag = tag.checked_add(1).ok_or("Überlauf")?;
            }
        }
    }
    Ok((monat, jahr, tag, im_monat))
}

/// Vier Monate mit zufälligen Anfragen beider KIs, teils abgerechnet, teils ohne
/// Abrechnung (Absturz): Zu jedem Zeitpunkt bleiben Kosten und Aufrufe je KI in den Grenzen
/// für Tag, Monat und Jahr. Eine Sperre ist immer eine Budgetsperre, nie ein Rechenfehler.
/// Nach dem Wiederöffnen ist das Kassenbuch gleich.
#[test]
fn ki_budget_bleibt_fuer_jede_anfragefolge_in_den_grenzen() -> Result<(), String> {
    let g = KiGrenzen::standard();
    let mut offen = Vec::new();
    let mut tage = Vec::new();
    let mut d = Datum::neu(2026, 1, 1).ok_or("Vorbedingung: Datum")?;
    // Januar bis April: Monatswechsel, Ostern und volle Monate; das Jahr bindet erst später.
    while d.monat() <= 4 {
        tage.push(d);
        d = d.plus_tage(1).ok_or("Vorbedingung: Datum")?;
    }
    let start = *STARTWERTE.first().ok_or("Startwert")?;
    let mut z = Zufall::neu(start);
    let p = neuer_pfad("budget")?;
    Journal::einrichten(&p).map_err(|e| format!("Vorbedingung Einrichtung: {e:?}"))?;
    let mut j = Journal::oeffnen(&p).map_err(|e| format!("Vorbedingung: {e:?}"))?;
    let mut erlaubt = 0_u32;
    for (nr, datum) in tage.into_iter().enumerate() {
        // Ein Zyklus je Tag, streng steigend wie im Papiertag.
        let zyklus = u32::try_from(nr).map_err(|_| "Zyklus")?.saturating_add(1);
        for wer in [BeraterId::Dachs, BeraterId::Maus] {
            // Mal kein, mal ein normaler, mal ein maßloser Tag.
            let versuche = match z.bis(4) {
                0 => 0,
                1 | 2 => z.bis(8),
                _ => 40,
            };
            for _ in 0..versuche {
                let zweck = ZWECKE
                    .get(z.bis(ZWECKE.len()))
                    .copied()
                    .unwrap_or(KiZweck::Erstmeinung);
                match j.reserviere_fuer(wer, datum, zyklus, zweck) {
                    Ok(e) => {
                        erlaubt = erlaubt.saturating_add(1);
                        // Jede vierte Erlaubnis endet ohne Abrechnung (Absturz): Die
                        // Höchstkosten bleiben gebucht.
                        if z.bis(4) != 0 {
                            let ein = u64::try_from(
                                z.bis(usize::try_from(e.deckel().max_token_ein()).unwrap_or(0)),
                            )
                            .unwrap_or(0);
                            let aus = u64::try_from(
                                z.bis(usize::try_from(e.deckel().max_token_aus()).unwrap_or(0)),
                            )
                            .unwrap_or(0);
                            let a = e.abrechnung(ein, aus).ok_or_else(|| {
                                format!("{datum}: Abrechnung im Deckel abgelehnt")
                            })?;
                            j.rechne_ki_ab(&a).map_err(|e| format!("{datum}: {e:?}"))?;
                        }
                    }
                    Err(
                        Sperre::ZyklusVoll
                        | Sperre::TagVoll
                        | Sperre::MonatVoll
                        | Sperre::MonatsbudgetErschoepft
                        | Sperre::JahresbudgetErschoepft
                        | Sperre::TagesrahmenErschoepft
                        | Sperre::ErstmeinungReserviert,
                    ) => {}
                    Err(s) => offen.push(format!("{datum} {wer:?}: unerwartete Sperre {s:?}")),
                }
                let (monat, jahr, tag, im_monat) = summen(&j, wer, datum)?;
                if monat > g.kosten_je_monat().0 || jahr > g.kosten_je_jahr().0 {
                    offen.push(format!(
                        "{datum} {wer:?}: Monat {monat} / Jahr {jahr} Cent über der Grenze"
                    ));
                }
                let je_tag = g.aufrufe_je_zyklus().saturating_mul(g.zyklen_je_tag());
                if u32::try_from(tag).unwrap_or(u32::MAX) > je_tag
                    || u32::try_from(im_monat).unwrap_or(u32::MAX) > g.aufrufe_je_monat()
                {
                    offen.push(format!(
                        "{datum} {wer:?}: {tag} Aufrufe am Tag, {im_monat} im Monat"
                    ));
                }
            }
            if offen.len() > 20 {
                break;
            }
        }
    }
    if erlaubt < 300 {
        offen.push(format!(
            "Vorbedingung: nur {erlaubt} Erlaubnisse, die Folge prüft zu wenig"
        ));
    }
    let vorher = j.stand().ki().clone();
    drop(j);
    let j = Journal::oeffnen(&p).map_err(|e| format!("Wiederöffnen: {e:?}"))?;
    if *j.stand().ki() != vorher {
        offen.push("Kassenbuch nach dem Wiederöffnen verändert".into());
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}

/// Wer an jedem Handelstag den vollen Tagesrahmen verbraucht, bleibt im Monatsbudget, in
/// jedem Monat 2026 und 2027. An börsenfreien Tagen ist der Vorbereitungsrahmen nie größer
/// als der Tagesrahmen des nächsten Handelstags (A3 aus #100).
#[test]
fn tagesrahmen_eines_monats_ergeben_hoechstens_das_monatsbudget() -> Result<(), String> {
    let g = KiGrenzen::standard();
    let mut offen = Vec::new();
    for wer in [BeraterId::Dachs, BeraterId::Maus] {
        let mut buchungen: Vec<depot_ki::Buchung> = Vec::new();
        let mut d = Datum::neu(2026, 1, 1).ok_or("Vorbedingung: Datum")?;
        let mut monat = (d.jahr(), d.monat());
        let mut summe = 0_i64;
        while d.jahr() <= 2027 {
            if (d.jahr(), d.monat()) != monat {
                monat = (d.jahr(), d.monat());
                summe = 0;
            }
            if depot_kern::liste::handelstag(d) {
                let r = depot_ki::tagesrahmen(&g, &buchungen, wer, d)
                    .map_err(|s| format!("{d}: {s:?}"))?;
                summe = summe.checked_add(r.0).ok_or("Überlauf")?;
                buchungen.push(depot_ki::Buchung {
                    berater: wer,
                    datum: d,
                    zyklus: 1,
                    reserviert: r,
                });
                if summe > g.kosten_je_monat().0 {
                    offen.push(format!("{wer:?} {d}: {summe} Cent im Monat"));
                }
            } else {
                let v = depot_ki::vorbereitungsrahmen(&g, &buchungen, wer, d)
                    .map_err(|s| format!("{d}: {s:?}"))?;
                let mut n = d.plus_tage(1).ok_or("Datum")?;
                while !depot_kern::liste::handelstag(n) {
                    n = n.plus_tage(1).ok_or("Datum")?;
                }
                if n.monat() == d.monat() {
                    let t = depot_ki::tagesrahmen(&g, &buchungen, wer, n)
                        .map_err(|s| format!("{n}: {s:?}"))?;
                    if v > t {
                        offen.push(format!(
                            "{wer:?} {d}: Vorbereitung {} > Tagesrahmen {} am {n}",
                            v.0, t.0
                        ));
                    }
                }
            }
            d = d.plus_tage(1).ok_or("Datum")?;
        }
    }
    if offen.is_empty() {
        Ok(())
    } else {
        Err(format!("{offen:#?}"))
    }
}
