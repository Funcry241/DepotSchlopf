//! Negativtests zu den Befunden aus `docs/BEFUNDE.md` (Stufe 2, Regelwerk).
//!
//! Jeder Test beschreibt das verlangte Verhalten. Ein Test mit `#[ignore = "offen: …"]` zeigt
//! einen Befund, der im heutigen Code noch besteht: `cargo test -p depot_regelwerk --test befunde
//! -- --ignored` ist am Stand dieses Commits rot. Der Umbau entfernt das `ignore`, und der Test
//! muss dann im Kodex gruen laufen. Tests ohne `ignore` sind gezielte Grenzfaelle (1·T01), die
//! schon heute halten.
//!
//! Der heutige Code kennt noch Toepfe statt Bereiche. Die Tests nutzen seine Schnittstelle; der
//! Umbau passt sie an, ohne die Aussage zu aendern.

use std::collections::{BTreeMap, BTreeSet};

use depot_kern::{
    BeraterId, Boerse, Cent, Datum, Grenzen, Herkunft, Instrument, InstrumentArt, Isin, Kurs,
    KursFehler, Kurse, Topf, TopfId, Universum, Vorschlag, Zeitpunkt,
};
use depot_regelwerk::{
    Anlass, AuftragsKennung, DatenFehler, Eingabe, Entscheidung, GenehmigterAuftrag,
    Kennungsvergabe, Richtung, entscheide, notbremse_auftraege, pruefe_kurse, waehle_tageswerte,
};

const ISINS: [&str; 11] = [
    "DE0007164600",
    "DE0008404005",
    "DE000BASF111",
    "DE0007100000",
    "DE0005190003",
    "DE0007236101",
    "DE0005557508",
    "DE0008430026",
    "DE000BAY0017",
    "DE0005140008",
    "DE0007037129",
];

fn isin(i: usize) -> Isin {
    Isin::neu(ISINS.get(i).copied().unwrap_or_default())
        .unwrap_or_else(|_| unreachable!("Test-ISIN {i} ungueltig"))
}

/// Die ersten 10 ISINs sind das Universum, die elfte ist nicht (mehr) handelbar.
fn universum() -> Universum {
    Universum::neu(
        (0..10)
            .map(|i| Instrument {
                isin: isin(i),
                name: format!("Wert {i}"),
                art: InstrumentArt::Aktie,
            })
            .collect(),
    )
}

/// Stand aller Testkurse: 02.11.2026, 10:00 Uhr UTC. Jeder Testkurs ist genau so alt.
fn stand() -> Zeitpunkt {
    Datum::neu(2026, 11, 2)
        .and_then(|d| Zeitpunkt::neu(d, 10, 0, 0))
        .unwrap_or_else(|| unreachable!("Teststand ungueltig"))
}

fn kurs(cent: i64) -> Result<Kurs, KursFehler> {
    let t = stand();
    Datum::neu(2026, 11, 2)
        .ok_or(KursFehler::Handelstag)
        .and_then(|d| Kurs::neu(Cent(cent), "EUR", d, t, Herkunft::Simulation))
}

/// Preise je Wert in Cent; daraus entstehen gepruefte Kurse.
type Preise = BTreeMap<Isin, i64>;

fn preise_alle(cent: i64) -> Preise {
    (0..11).map(|i| (isin(i), cent)).collect()
}

fn kurse(preise: &Preise) -> Kurse {
    kurse_mit_alter(preise, 0)
}

/// Kurse von 10:00 Uhr, abgerufen `minuten` spaeter: Jeder Kurs ist so alt.
fn kurse_mit_alter(preise: &Preise, minuten: u8) -> Kurse {
    let abruf = Datum::neu(2026, 11, 2)
        .and_then(|d| Zeitpunkt::neu(d, 10, minuten, 0))
        .unwrap_or_else(|| unreachable!("Abrufzeit ungueltig"));
    let eintraege = preise
        .iter()
        .map(|(i, c)| {
            let k = kurs(*c).unwrap_or_else(|e| unreachable!("Testkurs {i} {c}: {e:?}"));
            (i.clone(), k)
        })
        .collect();
    Kurse::neu(abruf, eintraege).unwrap_or_else(|e| unreachable!("Testkurse: {e:?}"))
}

fn kurse_alle(cent: i64) -> Kurse {
    kurse(&preise_alle(cent))
}

/// Der Handelstag aller Testkurse.
fn handelstag() -> Datum {
    Datum::neu(2026, 11, 2).unwrap_or_else(|| unreachable!("Handelstag ungueltig"))
}

/// Vergleichskurse vom vorigen Handelstag, Freitag 30.10.2026, 17:30 Uhr UTC.
fn vortagskurse(preise: &Preise) -> Kurse {
    let tag = Datum::neu(2026, 10, 30).unwrap_or_else(|| unreachable!("Vortag ungueltig"));
    let zeit = Zeitpunkt::neu(tag, 17, 30, 0).unwrap_or_else(|| unreachable!("Vortag ungueltig"));
    let eintraege = preise
        .iter()
        .map(|(i, c)| {
            let k = Kurs::neu(Cent(*c), "EUR", tag, zeit, Herkunft::Simulation)
                .unwrap_or_else(|e| unreachable!("Vortagskurs {i} {c}: {e:?}"));
            (i.clone(), k)
        })
        .collect();
    Kurse::neu(zeit, eintraege).unwrap_or_else(|e| unreachable!("Vortagskurse: {e:?}"))
}

fn topf_neu(id: TopfId, bargeld: Cent) -> Topf {
    Topf::neu(id, bargeld).unwrap_or_else(|e| unreachable!("Testtopf: {e:?}"))
}

fn vorschlag(berater: BeraterId, gewichte: &[(usize, i64)]) -> Vorschlag {
    Vorschlag {
        berater,
        zielgewichte_bp: gewichte.iter().map(|(i, g)| (isin(*i), *g)).collect(),
        begruendung: BTreeMap::new(),
    }
}

fn lauf(
    g: &Grenzen,
    topf: &Topf,
    kurse: &Kurse,
    v: Option<&Vorschlag>,
    halt: bool,
) -> Entscheidung {
    lauf_mit(g, &universum(), topf, kurse, v, halt)
}

fn lauf_mit(
    g: &Grenzen,
    u: &Universum,
    topf: &Topf,
    kurse: &Kurse,
    v: Option<&Vorschlag>,
    halt: bool,
) -> Entscheidung {
    entscheide(
        &Eingabe {
            grenzen: g,
            universum: u,
            topf,
            kurse,
            vorschlag: v,
            halt,
        },
        &mut Kennungsvergabe::nach(0),
    )
}

/// Hoechstmoegliche Gebuehr, unabhaengig vom Produktivcode nachgerechnet: 0,05 % aufgerundet,
/// mindestens 1,25 €, plus 0,25 € Boersengebuehr (Annahme von `Grenzen::standard`).
/// Der Nachrechner rechnet in `i128` mit saettigenden Operationen; bei Testbetraegen weit unter
/// 10^20 Cent kann keine Saettigung eintreten.
fn gebuehr(volumen: i128) -> i128 {
    let prozent = volumen
        .saturating_abs()
        .saturating_mul(5)
        .saturating_add(9_999)
        .checked_div(10_000)
        .unwrap_or(i128::MAX);
    prozent.max(125).saturating_add(25)
}

fn mal(kurs: Cent, stueck: u64) -> i128 {
    i128::from(kurs.0).saturating_mul(i128::from(stueck))
}

/// Unabhaengiger Nachrechner: der Topf nach allen genehmigten Auftraegen, voll ausgefuehrt
/// zum Limit, mit hoechstmoeglicher Gebuehr.
#[derive(Debug)]
struct Danach {
    bargeld: i128,
    stueck: BTreeMap<Isin, u64>,
}

impl Danach {
    fn rechne(topf: &Topf, ent: &Entscheidung) -> Danach {
        let mut d = Danach {
            bargeld: i128::from(topf.bargeld().0),
            stueck: topf.positionen().clone(),
        };
        for a in &ent.verkaeufe {
            let x = a.auftrag();
            let v = mal(x.limit, x.stueck);
            d.bargeld = d.bargeld.saturating_add(v).saturating_sub(gebuehr(v));
            let s = d.stueck.entry(x.isin.clone()).or_insert(0);
            *s = s.saturating_sub(x.stueck);
        }
        for a in &ent.kaeufe {
            let x = a.auftrag();
            let v = mal(x.limit, x.stueck);
            d.bargeld = d.bargeld.saturating_sub(v).saturating_sub(gebuehr(v));
            let s = d.stueck.entry(x.isin.clone()).or_insert(0);
            *s = s.saturating_add(x.stueck);
        }
        d.stueck.retain(|_, s| *s > 0);
        d
    }

    /// Wert zu Kursen.
    fn wert(&self, kurse: &Kurse) -> i128 {
        self.stueck.iter().fold(self.bargeld, |summe, (i, s)| {
            summe.saturating_add(mal(kurse.kurs_cent(i).unwrap_or(Cent::NULL), *s))
        })
    }

    /// Anteil eines Werts in Basispunkten, aufgerundet.
    fn anteil_bp(&self, isin: &Isin, kurse: &Kurse) -> i128 {
        let w = mal(
            kurse.kurs_cent(isin).unwrap_or(Cent::NULL),
            self.stueck.get(isin).copied().unwrap_or(0),
        );
        let gesamt = self.wert(kurse);
        if gesamt <= 0 {
            return i128::MAX;
        }
        w.saturating_mul(10_000)
            .saturating_add(gesamt.saturating_sub(1))
            .checked_div(gesamt)
            .unwrap_or(i128::MAX)
    }
}

/// Volumen am Limit (ungünstigster Fall, R21). `None` bei Ueberlauf.
fn am_limit(a: &GenehmigterAuftrag) -> Option<Cent> {
    a.auftrag().limit.mal_stueck(a.auftrag().stueck)
}

fn alle(ent: &Entscheidung) -> impl Iterator<Item = &GenehmigterAuftrag> {
    ent.verkaeufe.iter().chain(&ent.kaeufe)
}

// ---------------------------------------------------------------------------------------------
// 1·F02 – Vorschlag des falschen Beraters
// ---------------------------------------------------------------------------------------------

/// Topf A gehoert Dachs. Ein Vorschlag von Maus fuer Topf A darf keinen Auftrag erzeugen.
#[test]
fn befund_1_f02_vorschlag_fuer_fremden_bereich_wird_abgelehnt() {
    let g = Grenzen::standard();
    let topf = topf_neu(TopfId::A, Cent(1_000_000));
    let v = vorschlag(BeraterId::Maus, &[(0, 2_000)]);
    let ent = lauf(&g, &topf, &kurse_alle(10_000), Some(&v), false);
    assert_eq!(
        alle(&ent).count(),
        0,
        "Vorschlag von Maus fuer den Topf von Dachs wurde genehmigt"
    );
}

// ---------------------------------------------------------------------------------------------
// 1·F03 – Notauftraege mit Nullkurs
// ---------------------------------------------------------------------------------------------

/// Kurs 0 oder negativ: kein Notbremsen-Auftrag, sondern Halt (`None`). Seit dem Schnitt Kern
/// laesst sich ein solcher Kurs nicht mehr bauen (R20); was bleibt, ist ein fehlender Kurs.
#[test]
fn befund_1_f03_nullkurs_erzeugt_keinen_notauftrag_sondern_halt() {
    let g = Grenzen::standard();
    let mut topf = topf_neu(TopfId::A, Cent::NULL);
    topf.setze_bestand(isin(0), 10);
    let mut angenommen = Vec::new();
    for kurs_cent in [0, -100] {
        assert_eq!(kurs(kurs_cent).err(), Some(KursFehler::NichtPositiv));
        let kurs = Cent(kurs_cent);
        let mut preise = preise_alle(10_000);
        preise.remove(&isin(0));
        let kurse = kurse(&preise);
        let ergebnis = notbremse_auftraege(&g, &topf, &kurse, &mut Kennungsvergabe::nach(0));
        if ergebnis.is_some() {
            angenommen.push(format!("Kurs {kurs}: {ergebnis:?}"));
        }
    }
    assert!(angenommen.is_empty(), "statt Halt: {angenommen:?}");
}

/// (Fall, gehaltener Wert, Ziel)
type Fall = (&'static str, Option<usize>, &'static [(usize, i64)]);

/// Auch das Regelwerk erzeugt bei einem Kurs von 0 oder darunter keinen Auftrag, weder
/// freiwillig noch zwingend (Matrix R36: unplausibler Kurs heisst nur Halt, kein Verkauf). Ein
/// solcher Kurs laesst sich nicht bauen (R20); geprueft wird der fehlende Kurs, der an seine
/// Stelle tritt.
#[test]
fn befund_1_f03_nullkurs_erzeugt_im_regelwerk_keinen_auftrag() {
    let g = Grenzen::standard();
    let mut fehler = Vec::new();
    for kurs_cent in [0, -100] {
        assert_eq!(kurs(kurs_cent).err(), Some(KursFehler::NichtPositiv));
        let kurs = Cent(kurs_cent);
        let faelle: [Fall; 4] = [
            ("gehaltener Wert ohne Ziel", Some(0), &[]),
            ("gehaltener Wert mit kleinerem Ziel", Some(0), &[(0, 100)]),
            (
                "zwingender Ausstieg ausserhalb der Positivliste",
                Some(10),
                &[],
            ),
            ("Kaufziel", None, &[(0, 2_000)]),
        ];
        for (fall, gehalten, ziel) in faelle {
            let mut topf = topf_neu(TopfId::A, Cent(500_000));
            if let Some(i) = gehalten {
                topf.setze_bestand(isin(i), 50);
            }
            let mut preise = preise_alle(10_000);
            preise.remove(&isin(gehalten.unwrap_or(0)));
            let kurse = kurse(&preise);
            let v = vorschlag(BeraterId::Dachs, ziel);
            let ent = lauf(&g, &topf, &kurse, Some(&v), false);
            let n = alle(&ent).count();
            if n > 0 {
                fehler.push(format!("Kurs {kurs}, {fall}: {n} Auftraege"));
            }
        }
    }
    assert!(fehler.is_empty(), "{fehler:#?}");
}

// ---------------------------------------------------------------------------------------------
// 1·R06 – Hoechstalter der Kurse (20 Minuten, Kapitel 18)
// ---------------------------------------------------------------------------------------------

/// Ein Kurs ueber dem Hoechstalter ist nicht frisch genug: Die Datenpruefung meldet ihn, das
/// Regelwerk erzeugt keinen Auftrag, auch keinen zwingenden (Matrix R36: sonst nur Halt), und
/// die Notbremse haelt an. Mit 20 Minuten alten Kursen entstehen dieselben Auftraege.
#[test]
fn befund_1_r06_veralteter_kurs_erzeugt_keinen_auftrag() {
    let g = Grenzen::standard();
    let mut topf = topf_neu(TopfId::A, Cent(800_000));
    topf.setze_bestand(isin(10), 20); // ausserhalb der Positivliste: zwingender Ausstieg
    topf.setze_bestand(isin(0), 10);
    let v = vorschlag(BeraterId::Dachs, &[(1, 2_000)]);
    let vortag = vortagskurse(&preise_alle(10_000));
    let frisch = kurse_mit_alter(&preise_alle(10_000), 20);
    let alt = kurse_mit_alter(&preise_alle(10_000), 21);

    let ent = lauf(&g, &topf, &frisch, Some(&v), false);
    assert!(alle(&ent).count() > 1, "Vorbedingung: {:?}", ent.befunde);
    assert!(
        notbremse_auftraege(&g, &topf, &frisch, &mut Kennungsvergabe::nach(0)).is_some(),
        "Vorbedingung"
    );
    assert!(
        pruefe_kurse(&g, &universum(), handelstag(), &frisch, &vortag).is_empty(),
        "Vorbedingung"
    );

    let ent = lauf(&g, &topf, &alt, Some(&v), false);
    assert_eq!(
        alle(&ent).count(),
        0,
        "Auftraege mit 21 Minuten alten Kursen"
    );
    assert!(
        notbremse_auftraege(&g, &topf, &alt, &mut Kennungsvergabe::nach(0)).is_none(),
        "Notbremse mit veralteten Kursen"
    );
    assert!(
        !pruefe_kurse(&g, &universum(), handelstag(), &alt, &vortag).is_empty(),
        "veraltete Kurse nicht gemeldet"
    );
}

#[test]
fn notbremse_nutzt_am_morgen_nur_frisch_abgerufenen_bezugskurs() -> Result<(), String> {
    let freitag = Datum::neu(2026, 11, 6).ok_or("Freitag")?;
    let montag = Datum::neu(2026, 11, 9).ok_or("Montag")?;
    let dienstag = Datum::neu(2026, 11, 10).ok_or("Dienstag")?;
    let schluss = Boerse::Xetra.schluss(freitag).ok_or("Xetra-Schluss")?;
    let kurs = Kurs::neu(Cent(10_000), "EUR", freitag, schluss, Herkunft::Quelle)
        .map_err(|e| format!("{e:?}"))?;
    let mut topf = topf_neu(TopfId::A, Cent(800_000));
    topf.setze_bestand(isin(0), 10);
    let grenze = Grenzen::standard();

    for (tag, abruf_stunde, abruf_minute, erlaubt) in [
        (montag, 6, 30, true),
        (montag, 6, 9, false),
        (dienstag, 6, 30, false),
    ] {
        let start = Zeitpunkt::neu(tag, 6, 30, 0).ok_or("Start")?;
        let abruf = Zeitpunkt::neu(tag, abruf_stunde, abruf_minute, 0).ok_or("Abruf")?;
        let kurse = Kurse::mit_abrufen(start, vec![(isin(0), kurs.clone(), abruf)])
            .map_err(|e| format!("{e:?}"))?;
        let auftraege = notbremse_auftraege(&grenze, &topf, &kurse, &mut Kennungsvergabe::nach(0));
        assert_eq!(auftraege.as_ref().map(Vec::len), erlaubt.then_some(1));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// 1·F04 – Bestandsgrenze mit Toleranz und Abbaupflicht
// ---------------------------------------------------------------------------------------------

/// Durch Kursbewegung liegt ein Wert bei 28 % (Hoechstanteil 25 %, Toleranz 2 Punkte).
/// Das Ziel 25 % liegt nur 3 Punkte darunter, also unter der Umschichtungsschwelle von
/// 5 Punkten. Die Abbaupflicht gilt trotzdem: Nach den genehmigten Auftraegen (voll zum Limit
/// ausgefuehrt, hoechste Gebuehr) liegt der Wert hoechstens beim Hoechstanteil, und es wird
/// nicht zugekauft.
#[test]
fn befund_1_f04_marktbewegung_ueber_toleranz_erzeugt_abbaupflicht() {
    let g = Grenzen::standard();
    let kurse = kurse_alle(10_000);
    let mut topf = topf_neu(TopfId::A, Cent(720_000));
    topf.setze_bestand(isin(0), 28); // 28 × 100 € = 2.800 € von 10.000 €
    let v = vorschlag(BeraterId::Dachs, &[(0, 2_500)]);
    let ent = lauf(&g, &topf, &kurse, Some(&v), false);
    assert!(
        !ent.kaeufe.iter().any(|a| a.auftrag().isin == isin(0)),
        "Zukauf trotz Bestand ueber der Toleranz"
    );
    let danach = Danach::rechne(&topf, &ent);
    let anteil = danach.anteil_bp(&isin(0), &kurse);
    assert!(
        anteil <= i128::from(g.max_anteil_bp()),
        "nach Abbau {anteil} bp statt hoechstens {} bp; {:?}",
        g.max_anteil_bp(),
        ent.befunde
    );
}

/// Neun gehaltene Werte bei hoechstens acht. Der Vorschlag nennt alle neun. Er darf nicht
/// einfach verworfen werden: Nach den genehmigten Auftraegen sind es hoechstens acht Werte,
/// und kein neuer Wert kommt hinzu.
#[test]
fn befund_1_f04_zu_viele_positionen_erzeugen_abbaupflicht() {
    let g = Grenzen::standard();
    let kurse = kurse_alle(10_000);
    let mut topf = topf_neu(TopfId::A, Cent(100_000));
    for i in 0..9 {
        topf.setze_bestand(isin(i), 10); // je 1.000 € = 10 %
    }
    let v = vorschlag(
        BeraterId::Dachs,
        &(0..9).map(|i| (i, 1_000)).collect::<Vec<_>>(),
    );
    let ent = lauf(&g, &topf, &kurse, Some(&v), false);
    let danach = Danach::rechne(&topf, &ent);
    assert!(
        danach.stueck.len() <= g.max_werte(),
        "nach den Auftraegen {} Werte statt hoechstens {}; {:?}",
        danach.stueck.len(),
        g.max_werte(),
        ent.befunde
    );
    assert!(
        ent.kaeufe
            .iter()
            .all(|a| topf.stueck(&a.auftrag().isin) > 0),
        "neuer Wert trotz zu vieler Positionen"
    );
    assert!(danach.bargeld >= 0, "Bargeld negativ: {danach:?}");
}

/// Kaufgrenze fuer die Positionszahl: Acht Werte zu je 4 Stueck à 100 € sind gehalten (je 4 %),
/// dazu 6.800 € Bargeld. Der zulaessige Vorschlag nennt nur ein Ziel: 20 % in einem neunten
/// Wert. Die acht bleiben wegen der Umschichtungsschwelle stehen. Ein Kauf, der zu neun Werten
/// fuehrt, verletzt die Kaufgrenze und darf nicht genehmigt werden. Eigener Fall neben der
/// Abbaupflicht oben: Dort ist der Bestand schon zu gross, hier wuerde ihn erst der Kauf
/// zu gross machen.
#[test]
fn befund_1_f04_kauf_erzeugt_keine_neunte_position() {
    let g = Grenzen::standard();
    let kurse = kurse_alle(10_000);
    let mut topf = topf_neu(TopfId::A, Cent(680_000));
    for i in 0..8 {
        topf.setze_bestand(isin(i), 4);
    }
    let v = vorschlag(BeraterId::Dachs, &[(8, 2_000)]);
    let ent = lauf(&g, &topf, &kurse, Some(&v), false);
    let danach = Danach::rechne(&topf, &ent);
    assert!(
        danach.stueck.len() <= g.max_werte(),
        "nach den Auftraegen {} Werte statt hoechstens {}; Kaeufe {:?}",
        danach.stueck.len(),
        g.max_werte(),
        ent.kaeufe
            .iter()
            .map(GenehmigterAuftrag::auftrag)
            .collect::<Vec<_>>()
    );
    assert!(danach.bargeld >= 0, "Bargeld negativ: {danach:?}");
}

// ---------------------------------------------------------------------------------------------
// 1·F05 – Umschlaggrenze am Limit
// ---------------------------------------------------------------------------------------------

/// Vier Kaeufe zu je 25 % (40 Stueck zu 100 € in 16.000 €) fuellen die Umschlaggrenze
/// (100 %) am Schlusskurs genau aus.
/// Am Limit (+1 %) liegt der Umschlag darueber und muss gekuerzt werden. Weder die Kaufgrenze
/// (40 %) noch die Orderzahl (4) greift vorher: Begrenzen darf hier allein die Umschlaggrenze
/// (und mit ihr das Bargeld).
#[test]
fn befund_1_f05_umschlag_am_limit_ueber_der_grenze_wird_gekuerzt() {
    let g = Grenzen::standard();
    let topf = topf_neu(TopfId::A, Cent(1_600_000));
    let v = vorschlag(
        BeraterId::Dachs,
        &[(0, 2_500), (1, 2_500), (2, 2_500), (3, 2_500)],
    );
    let ent = lauf(&g, &topf, &kurse_alle(10_000), Some(&v), false);
    assert_eq!(ent.kaeufe.len(), 4, "Vorbedingung: {:?}", ent.befunde);
    let erlaubt = Cent(1_600_000).anteil_bp_ab(g.max_umschlag_bp());
    let umschlag = alle(&ent).try_fold(Cent::NULL, |s, a| s.checked_add(am_limit(a)?));
    assert!(
        matches!((umschlag, erlaubt), (Some(u), Some(e)) if u <= e),
        "Umschlag am Limit {umschlag:?} ueber erlaubt {erlaubt:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 1·F06 – Teilverkauf unter Mindestorder am Limit
// ---------------------------------------------------------------------------------------------

/// Teilverkauf von 5 Stueck zu 20 €: am Schlusskurs 100 € (Mindestorder), am Limit
/// (19,80 €) nur 99 €. Er muss entfallen. Ein Verkauf des ganzen Bestands bleibt erlaubt.
#[test]
fn befund_1_f06_teilverkauf_unter_mindestorder_am_limit_abgelehnt() {
    let g = Grenzen::standard();
    let mut topf = topf_neu(TopfId::A, Cent(180_000));
    topf.setze_bestand(isin(0), 10); // 10 % des Topfes
    let v = vorschlag(BeraterId::Dachs, &[(0, 500)]);
    let ent = lauf(&g, &topf, &kurse_alle(2_000), Some(&v), false);
    assert!(ent.verkaeufe.is_empty(), "{:?}", ent.verkaeufe);
    assert!(
        ent.befunde
            .contains(&depot_regelwerk::Befund::UnterMindestorder(isin(0)))
    );
}

/// Gegenprobe zu 1·F06: Der Verkauf des ganzen Bestands ist auch unter Mindestorder erlaubt.
#[test]
fn grenzfall_gesamtverkauf_unter_mindestorder_bleibt_erlaubt() {
    let g = Grenzen::standard();
    let mut topf = topf_neu(TopfId::A, Cent(94_000));
    topf.setze_bestand(isin(0), 3); // 60 € = 6 % des Topfes, Ziel 0
    let v = vorschlag(BeraterId::Dachs, &[]);
    let ent = lauf(&g, &topf, &kurse_alle(2_000), Some(&v), false);
    assert!(
        ent.verkaeufe
            .iter()
            .any(|a| a.auftrag().isin == isin(0) && a.auftrag().stueck == 3),
        "Gesamtverkauf fehlt: {:?}",
        ent.befunde
    );
}

// ---------------------------------------------------------------------------------------------
// 1·F08 – fehlender Vortagskurs
// ---------------------------------------------------------------------------------------------

/// Fehlt der Vortagskurs, ist der Sprungtest nicht bestanden, sondern ein eigener Zustand.
#[test]
fn befund_1_f08_fehlender_vortag_ist_kein_bestandener_sprungtest() {
    let g = Grenzen::standard();
    let heute = kurse_alle(10_000);
    let mut preise = preise_alle(10_000);
    preise.remove(&isin(3));
    let vortag = vortagskurse(&preise);
    let fehler = pruefe_kurse(&g, &universum(), handelstag(), &heute, &vortag);
    assert!(
        !fehler.is_empty(),
        "fehlender Vortagskurs von {} wurde still akzeptiert",
        isin(3)
    );
}

// ---------------------------------------------------------------------------------------------
// 1·F09 – Auftragskennungen kollidieren
// ---------------------------------------------------------------------------------------------

/// Notbremse und Umschichtung im selben Lauf vergeben verschiedene Kennungen. Beide ziehen aus
/// der einen zentralen Vergabe (R24); eine andere Quelle fuer Kennungen gibt es nicht.
#[test]
fn befund_1_f09_notbremse_und_umschichtung_vergeben_verschiedene_kennungen() {
    let g = Grenzen::standard();
    let mut topf = topf_neu(TopfId::A, Cent(500_000));
    topf.setze_bestand(isin(0), 50);
    let v = vorschlag(BeraterId::Dachs, &[(1, 2_500)]);
    let mut vergabe = Kennungsvergabe::nach(41);
    let ent = entscheide(
        &Eingabe {
            grenzen: &g,
            universum: &universum(),
            topf: &topf,
            kurse: &kurse_alle(10_000),
            vorschlag: Some(&v),
            halt: false,
        },
        &mut vergabe,
    );
    let not = notbremse_auftraege(&g, &topf, &kurse_alle(10_000), &mut vergabe).unwrap_or_default();
    assert!(alle(&ent).count() > 0 && !not.is_empty(), "Vorbedingung");
    let umschichtung: BTreeSet<AuftragsKennung> = alle(&ent).map(|a| a.auftrag().kennung).collect();
    let doppelt: Vec<AuftragsKennung> = not
        .iter()
        .map(|a| a.auftrag().kennung)
        .filter(|k| umschichtung.contains(k))
        .collect();
    assert!(doppelt.is_empty(), "doppelte Kennungen: {doppelt:?}");
    // Fortlaufend ab der zuletzt vergebenen Nummer, ohne Luecke.
    let mut nummern: Vec<u64> = alle(&ent)
        .chain(not.iter())
        .map(|a| a.auftrag().kennung.nr())
        .collect();
    nummern.sort_unstable();
    let erwartet: Vec<u64> = (42..).take(nummern.len()).collect();
    assert_eq!(nummern, erwartet);
    assert_eq!(nummern.last().copied(), Some(vergabe.letzte()));
}

// ---------------------------------------------------------------------------------------------
// 2·A04 – zwingende Ausstiege durch freiwillige Regeln oder Halt blockiert (Matrix R36)
// ---------------------------------------------------------------------------------------------
//
// Jede Schranke wird einzeln aktiviert, der uebrige Zustand bleibt zulaessig. Danach folgt die
// Kombination. Zwingend ist hier der Ausstieg eines Werts aus der Positivliste. Budget und
// Ausstiegsreserve gibt es im heutigen Code nicht; ihre Faelle entstehen mit dem Kostenbuch
// (Stufe 3, 2·A05) und sind in docs/befunde/negativtests.md als fehlend gefuehrt.

/// Universum nur aus den Werten `0..bis`; alles darueber ist ausserhalb der Positivliste.
fn universum_bis(bis: usize) -> Universum {
    Universum::neu(
        (0..bis)
            .map(|i| Instrument {
                isin: isin(i),
                name: format!("Wert {i}"),
                art: InstrumentArt::Aktie,
            })
            .collect(),
    )
}

/// Jeder Wert ausserhalb der Positivliste ist mit genau seiner ganzen Menge verkauft, und es
/// wird nie mehr verkauft als gehalten.
fn ausstiege_vollstaendig(
    topf: &Topf,
    u: &Universum,
    g: &Grenzen,
    ent: &Entscheidung,
) -> Vec<String> {
    let mut fehler = Vec::new();
    for (i, gehalten) in topf.positionen() {
        if u.handelbar(i, g) {
            continue;
        }
        let verkauft: u64 = ent
            .verkaeufe
            .iter()
            .filter(|a| &a.auftrag().isin == i)
            .map(|a| a.auftrag().stueck)
            .sum();
        if verkauft != *gehalten {
            fehler.push(format!("{i}: {verkauft} von {gehalten} verkauft"));
        }
    }
    for a in &ent.verkaeufe {
        if a.auftrag().stueck > topf.stueck(&a.auftrag().isin) {
            fehler.push(format!("Verkauf ueber Bestand: {:?}", a.auftrag()));
        }
    }
    fehler
}

/// Nur Halt: Der zwingende Ausstieg (1.000 €, ueber Mindestorder, unter Umschlaggrenze)
/// laeuft trotzdem; freiwillige Auftraege bleiben gesperrt.
#[test]
fn befund_2_a04_halt_sperrt_den_zwingenden_ausstieg_nicht() {
    let g = Grenzen::standard();
    let u = universum();
    let mut topf = topf_neu(TopfId::A, Cent(800_000));
    topf.setze_bestand(isin(10), 20); // 1.000 € ausserhalb der Positivliste
    topf.setze_bestand(isin(0), 10); // 1.000 € im Universum
    // Freiwillig wuerde der Vorschlag isin(0) abbauen und isin(1) kaufen.
    let v = vorschlag(BeraterId::Dachs, &[(1, 2_000)]);
    let ent = lauf_mit(&g, &u, &topf, &kurse_alle(10_000), Some(&v), true);
    let fehler = ausstiege_vollstaendig(&topf, &u, &g, &ent);
    assert!(fehler.is_empty(), "{fehler:?}; {:?}", ent.befunde);
    assert!(
        ent.kaeufe.is_empty(),
        "Halt muss freiwillige Kaeufe sperren"
    );
    assert!(
        ent.verkaeufe.iter().all(|a| a.auftrag().isin == isin(10)),
        "Halt muss freiwillige Verkaeufe sperren"
    );
}

/// Nur Umschlaggrenze: Der zwingende Ausstieg betraegt 80 % des Topfes (Grenze 50 %). Er
/// wird nicht gekuerzt.
#[test]
fn befund_2_a04_umschlaggrenze_kuerzt_den_zwingenden_ausstieg_nicht() {
    let g = Grenzen::standard();
    let u = universum();
    let mut topf = topf_neu(TopfId::A, Cent(200_000));
    topf.setze_bestand(isin(10), 80);
    let v = vorschlag(BeraterId::Dachs, &[]);
    let ent = lauf_mit(&g, &u, &topf, &kurse_alle(10_000), Some(&v), false);
    let fehler = ausstiege_vollstaendig(&topf, &u, &g, &ent);
    assert!(fehler.is_empty(), "{fehler:?}; {:?}", ent.befunde);
}

/// Nur Orderzahl: Sechs Werte verlassen die Positivliste (hoechstens vier Orders). Alle sechs
/// werden verkauft.
#[test]
fn befund_2_a04_orderzahl_sperrt_zwingende_ausstiege_nicht() {
    // Viel Bargeld: Die sechs Ausstiege (6.000 €) bleiben unter der Umschlaggrenze von 50 %.
    let g = Grenzen::standard();
    let u = universum_bis(5);
    let mut topf = topf_neu(TopfId::A, Cent(4_000_000));
    for i in 5..11 {
        topf.setze_bestand(isin(i), 10); // je 1.000 €
    }
    let v = vorschlag(BeraterId::Dachs, &[]);
    let ent = lauf_mit(&g, &u, &topf, &kurse_alle(10_000), Some(&v), false);
    let fehler = ausstiege_vollstaendig(&topf, &u, &g, &ent);
    assert!(fehler.is_empty(), "{fehler:?}; {:?}", ent.befunde);
}

/// Nur Mindestorder und Gebuehrenquote: Restbestand 2 Stueck zu 40 € (80 €, Gebuehr 1,50 €
/// = 1,875 %). Er wird zwingend verkauft. Haelt schon heute.
#[test]
fn grenzfall_2_a04_kleiner_rest_wird_zwingend_verkauft() {
    let g = Grenzen::standard();
    let u = universum();
    let mut topf = topf_neu(TopfId::A, Cent(100_000));
    topf.setze_bestand(isin(10), 2);
    let v = vorschlag(BeraterId::Dachs, &[]);
    let ent = lauf_mit(&g, &u, &topf, &kurse_alle(4_000), Some(&v), false);
    let fehler = ausstiege_vollstaendig(&topf, &u, &g, &ent);
    assert!(fehler.is_empty(), "{fehler:?}; {:?}", ent.befunde);
}

/// Alles zugleich: Halt, Umschlag ueber der Grenze, mehr Ausstiege als Orders, ein Rest unter
/// Mindestorder. Alle Ausstiege laufen vollstaendig, nichts Freiwilliges.
#[test]
fn befund_2_a04_kombination_sperrt_zwingende_ausstiege_nicht() {
    let g = Grenzen::standard();
    let u = universum_bis(5);
    let mut topf = topf_neu(TopfId::A, Cent(50_000));
    for i in 5..10 {
        topf.setze_bestand(isin(i), 20); // je 2.000 €
    }
    topf.setze_bestand(isin(10), 1); // am Verkaufslimit 99 €, unter Mindestorder
    topf.setze_bestand(isin(0), 10); // freiwillig, im Universum
    let v = vorschlag(BeraterId::Dachs, &[(1, 2_000)]);
    let ent = lauf_mit(&g, &u, &topf, &kurse_alle(10_000), Some(&v), true);
    let fehler = ausstiege_vollstaendig(&topf, &u, &g, &ent);
    assert!(fehler.is_empty(), "{fehler:?}; {:?}", ent.befunde);
    assert!(
        ent.kaeufe.is_empty(),
        "Halt muss freiwillige Kaeufe sperren"
    );
    assert!(
        ent.verkaeufe
            .iter()
            .all(|a| !u.handelbar(&a.auftrag().isin, &g)),
        "Halt muss freiwillige Verkaeufe sperren"
    );
}

// ---------------------------------------------------------------------------------------------
// 1·T01 – gezielte Grenzfaelle je harter Grenze (statt nur Zufall)
// ---------------------------------------------------------------------------------------------

/// Kaufgrenze am Limit: Nach dem Kauf, voll zum Limit ausgefuehrt und mit hoechster Gebuehr,
/// liegt der Bestand hoechstens beim Hoechstanteil (R21, R22). Der Wert wird dabei zum
/// Kauflimit angesetzt, der uebrige Topf zu Kursen. Zwei Lagen: 10 Stueck (10 %) gehalten und
/// Ziel 25 %; dazu ein grosser Bargeldtopf mit Ziel 25 %, in dem der Abstand zwischen Kurs und
/// Limit mehrere Stueck ausmacht.
#[test]
fn grenzfall_hoechstanteil_gilt_am_limit() {
    let g = Grenzen::standard();
    let kurse = kurse_alle(10_000);
    for (bargeld, gehalten) in [(900_000_i64, 10_u64), (100_000_000, 0)] {
        let mut topf = topf_neu(TopfId::A, Cent(bargeld));
        topf.setze_bestand(isin(0), gehalten);
        let v = vorschlag(BeraterId::Dachs, &[(0, 2_500)]);
        let ent = lauf(&g, &topf, &kurse, Some(&v), false);
        assert!(!ent.kaeufe.is_empty(), "Vorbedingung: {:?}", ent.befunde);
        let danach = Danach::rechne(&topf, &ent);
        let limit = ent
            .kaeufe
            .iter()
            .find(|a| a.auftrag().isin == isin(0))
            .map_or(10_000, |a| a.auftrag().limit.0);
        let stueck = danach.stueck.get(&isin(0)).copied().unwrap_or(0);
        let wert = mal(Cent(limit), stueck);
        let gesamt = danach
            .wert(&kurse)
            .saturating_sub(mal(Cent(10_000), stueck))
            .saturating_add(wert);
        assert!(
            wert.saturating_mul(10_000) <= gesamt.saturating_mul(i128::from(g.max_anteil_bp())),
            "Bargeld {bargeld}: nach dem Kauf {stueck} Stueck zu {limit} Cent = {wert} von \
             {gesamt} Cent, ueber {} bp",
            g.max_anteil_bp()
        );
    }
}

/// Mindestorder Kauf am Limit, stueckgenau: Kurs 9,91 €, Limit 10 €, Mindestorder 100 €, Ziel
/// 10 % des Topfes. 9 Stueck (90 €) sind zu wenig, ab 10 Stueck (100 €) wird gekauft.
#[test]
fn grenzfall_mindestorder_kauf_am_limit() {
    let g = Grenzen::standard();
    for (topf_euro, erwartet) in [
        (990, None),
        (991, Some(10)),
        (1_090, Some(10)),
        (1_091, Some(11)),
    ] {
        let topf = topf_neu(TopfId::A, Cent(i64::saturating_mul(topf_euro, 100)));
        let v = vorschlag(BeraterId::Dachs, &[(0, 1_000)]);
        let ent = lauf(&g, &topf, &kurse_alle(991), Some(&v), false);
        let gekauft = ent.kaeufe.first().map(|a| a.auftrag().stueck);
        assert_eq!(gekauft, erwartet, "Topf {topf_euro} €");
        for a in &ent.kaeufe {
            assert!(
                am_limit(a).is_some_and(|v| v >= g.min_order()),
                "Topf {topf_euro} €: Kauf unter Mindestorder"
            );
        }
    }
}

/// Deckung: Kauf am Limit plus Gebuehr uebersteigt nie das Bargeld, auch knapp an der Grenze.
/// Die drei gehaltenen Werte bleiben unter der Umschichtungsschwelle. Der Kauf von Wert 0
/// ist nur durch das verfügbare Bargeld begrenzt.
#[test]
fn grenzfall_deckung_mit_gebuehr_am_limit() {
    let g = Grenzen::standard();
    for (bargeld, bestand, erwartet) in [
        (10_249, 5, None),
        (10_250, 5, Some(1)),
        (10_251, 5, Some(1)),
        (101_000, 25, Some(9)),
        (101_150, 25, Some(10)),
        (101_151, 25, Some(10)),
    ] {
        let mut topf = topf_neu(TopfId::A, Cent(bargeld));
        for i in 1..4 {
            topf.setze_bestand(isin(i), bestand);
        }
        let v = vorschlag(
            BeraterId::Dachs,
            &[(0, 2_000), (1, 2_666), (2, 2_666), (3, 2_666)],
        );
        let ent = lauf(&g, &topf, &kurse_alle(10_000), Some(&v), false);
        assert!(ent.verkaeufe.is_empty(), "{ent:?}");
        let mut kosten = Cent::NULL;
        for a in &ent.kaeufe {
            let v = am_limit(a).unwrap_or(Cent(i64::MAX));
            let gebuehr = g.gebuehren().fuer(v).unwrap_or(Cent(i64::MAX));
            kosten = kosten
                .checked_add(v)
                .and_then(|k| k.checked_add(gebuehr))
                .unwrap_or(Cent(i64::MAX));
        }
        assert!(
            kosten <= Cent(bargeld),
            "Bargeld {bargeld}: Kaeufe kosten {kosten}"
        );
        assert_eq!(
            ent.kaeufe.first().map(|a| a.auftrag().stueck),
            erwartet,
            "Bargeld {bargeld}: {ent:?}"
        );
    }
}

/// Deckung kauft, was passt: Das Ziel (20 % = rund 1.600 €) liegt weit ueber dem Bargeld. Gekauft
/// wird die groesste Stueckzahl, die samt Gebuehr ins Bargeld passt: bei 506,50 € und bei
/// 600 € je 5 Stueck zu 101 € (506,50 €), bei 1.011,50 € 10 Stueck. Beobachtung aus dem
/// Umbau von PR #8: Die Schaetzung lag eine Stueckzahl zu niedrig und zaehlte nur abwaerts.
#[test]
fn grenzfall_deckung_kauft_was_passt() {
    let g = Grenzen::standard();
    for (bargeld, erwartet) in [(50_650_i64, 5_u64), (60_000, 5), (101_150, 10)] {
        let mut topf = topf_neu(TopfId::A, Cent(bargeld));
        for i in 1..4 {
            topf.setze_bestand(isin(i), 25);
        }
        let v = vorschlag(
            BeraterId::Dachs,
            &[(0, 2_000), (1, 2_666), (2, 2_666), (3, 2_666)],
        );
        let ent = lauf(&g, &topf, &kurse_alle(10_000), Some(&v), false);
        let gekauft = ent.kaeufe.first().map(|a| a.auftrag().stueck);
        assert_eq!(
            gekauft,
            Some(erwartet),
            "Bargeld {bargeld}: {:?}",
            ent.befunde
        );
    }
}

/// Orderzahl: genau an der Grenze (4) wird nichts weggelassen, darueber schon.
#[test]
fn grenzfall_orderzahl_genau_an_der_grenze() {
    let g = Grenzen::standard();
    let topf = topf_neu(TopfId::A, Cent(10_000_000));
    let vier = vorschlag(
        BeraterId::Dachs,
        &[(0, 1_000), (1, 1_000), (2, 1_000), (3, 1_000)],
    );
    let ent = lauf(&g, &topf, &kurse_alle(10_000), Some(&vier), false);
    assert_eq!(alle(&ent).count(), 4, "{ent:?}");
    // Fuenf Auftraege bei hoechstens vier Werten: Wert 4 wird ganz verkauft, vier gekauft.
    let mut topf = topf;
    topf.setze_bestand(isin(4), 1_000);
    let ent = lauf(&g, &topf, &kurse_alle(10_000), Some(&vier), false);
    assert_eq!(alle(&ent).count(), g.max_orders_je_topf());
}

/// Kurssprung: genau an der Grenze (15 %) plausibel, einen Cent darueber nicht.
#[test]
fn grenzfall_kurssprung_genau_an_der_grenze() {
    let g = Grenzen::standard();
    let vortag = vortagskurse(&preise_alle(10_000));
    let mut preise = preise_alle(10_000);
    preise.insert(isin(0), 11_500);
    let heute = kurse(&preise);
    assert!(pruefe_kurse(&g, &universum(), handelstag(), &heute, &vortag).is_empty());
    preise.insert(isin(0), 11_501);
    let heute = kurse(&preise);
    assert_eq!(
        pruefe_kurse(&g, &universum(), handelstag(), &heute, &vortag).len(),
        1
    );
}

/// Kauflimit: hoechstens 1 % ueber dem Kurs, Verkaufslimit hoechstens 1 % darunter.
#[test]
fn grenzfall_limit_abstand() {
    let g = Grenzen::standard();
    let mut topf = topf_neu(TopfId::A, Cent(500_000));
    topf.setze_bestand(isin(0), 50);
    let v = vorschlag(BeraterId::Dachs, &[(1, 2_500)]);
    let ent = lauf(&g, &topf, &kurse_alle(9_999), Some(&v), false);
    assert!(
        !ent.kaeufe.is_empty() && !ent.verkaeufe.is_empty(),
        "Vorbedingung"
    );
    for a in &ent.kaeufe {
        assert!(a.auftrag().limit <= Cent(10_099));
    }
    for a in &ent.verkaeufe {
        assert!(a.auftrag().limit >= Cent(9_899));
        assert_eq!(a.auftrag().richtung, Richtung::Verkauf);
    }
}

// ---------------------------------------------------------------------------------------------
// K9 – Maus' Gegenprogramm zu PR #9
// ---------------------------------------------------------------------------------------------

/// Die zwingenden Verkaeufe einer Entscheidung, als (ISIN, Stueck).
fn zwingende(ent: &Entscheidung) -> Vec<(Isin, u64)> {
    ent.verkaeufe
        .iter()
        .filter(|a| a.beleg().anlass.zwingend())
        .map(|a| (a.auftrag().isin.clone(), a.auftrag().stueck))
        .collect()
}

/// K9-01: Neun gleich grosse Werte, erlaubt sind acht. Welcher Wert zwingend verkauft wird,
/// entscheidet allein das Regelwerk. Kein Vorschlag, ein fremder, ein ungueltiger oder ein
/// gueltiger Vorschlag, mit oder ohne Halt: Die zwingenden Verkaeufe sind immer dieselben.
#[test]
fn befund_k9_01_vorschlag_steuert_keinen_zwangsverkauf() {
    let g = Grenzen::standard();
    let kurse = kurse_alle(10_000);
    let mut topf = topf_neu(TopfId::A, Cent::NULL);
    for i in 0..9 {
        topf.setze_bestand(isin(i), 10);
    }
    let basis = zwingende(&lauf(&g, &topf, &kurse, None, false));
    let Some((opfer, _)) = basis.first() else {
        unreachable!("Vorbedingung: ohne Vorschlag wird ein Wert zwingend verkauft")
    };
    let nr = (0..9).find(|i| isin(*i) == *opfer).unwrap_or(0);
    let andere = usize::from(nr == 0);
    let faelle: [(&str, Vorschlag, bool); 5] = [
        (
            "fremder Berater",
            vorschlag(BeraterId::Maus, &[(nr, 1_000)]),
            false,
        ),
        (
            "negatives Gewicht",
            vorschlag(BeraterId::Dachs, &[(nr, 1_000), (andere, -1)]),
            false,
        ),
        (
            "Summe ueber 100 %",
            vorschlag(BeraterId::Dachs, &[(nr, 1_000), (andere, 9_500)]),
            false,
        ),
        (
            "gueltig, mit Halt",
            vorschlag(BeraterId::Dachs, &[(nr, 1_000)]),
            true,
        ),
        (
            "gueltig, ohne Halt",
            vorschlag(BeraterId::Dachs, &[(nr, 1_000)]),
            false,
        ),
    ];
    let mut fehler = Vec::new();
    for (fall, v, halt) in faelle {
        let z = zwingende(&lauf(&g, &topf, &kurse, Some(&v), halt));
        if z != basis {
            fehler.push(format!("{fall}: {z:?} statt {basis:?}"));
        }
    }
    assert!(fehler.is_empty(), "{fehler:#?}");
}

/// K9-02: Mehrere gleichzeitige Auftraege mindern denselben Topfwert. Jeder abgebaute Wert liegt
/// nach allen Auftraegen (voll zum Limit, hoechste Gebuehr) hoechstens beim Hoechstanteil, und
/// jeder gekaufte Wert, zum Limit angesetzt, ebenso. Lagen: zwei Abbauorders unter Halt (Maus'
/// Gegenprobe), dazu ein Ausstieg aus der Positivliste, dazu freiwillige Auftraege.
#[test]
fn befund_k9_02_gemeinsamer_abbau_haelt_den_hoechstanteil() {
    let g = Grenzen::standard();
    let kurse = kurse_alle(100);
    let faelle: [(&str, i64, u64, Option<Vorschlag>, bool); 3] = [
        ("zwei Abbauorders, Halt", 140_000, 0, None, true),
        ("dazu Ausstieg Positivliste, Halt", 90_000, 500, None, true),
        (
            "dazu freiwillige Auftraege",
            90_000,
            500,
            Some(vorschlag(
                BeraterId::Dachs,
                &[(0, 4_000), (1, 4_000), (2, 1_500)],
            )),
            false,
        ),
    ];
    let mut fehler = Vec::new();
    for (fall, bargeld, raus, v, halt) in faelle {
        let mut topf = topf_neu(TopfId::A, Cent(bargeld));
        topf.setze_bestand(isin(0), 4_300);
        topf.setze_bestand(isin(1), 4_300);
        if raus > 0 {
            topf.setze_bestand(isin(10), raus);
        }
        let ent = lauf(&g, &topf, &kurse, v.as_ref(), halt);
        let abbau: Vec<&Isin> = ent
            .verkaeufe
            .iter()
            .filter(|a| a.beleg().anlass == Anlass::Abbau)
            .map(|a| &a.auftrag().isin)
            .collect();
        if abbau.len() != 2 || (v.is_some() && ent.kaeufe.is_empty()) {
            fehler.push(format!(
                "{fall}: Vorbedingung, Abbau {abbau:?}, {:?}",
                ent.befunde
            ));
            continue;
        }
        let danach = Danach::rechne(&topf, &ent);
        let gesamt = danach.wert(&kurse);
        for i in abbau {
            let w = mal(Cent(100), danach.stueck.get(i).copied().unwrap_or(0));
            if w.saturating_mul(10_000) > gesamt.saturating_mul(i128::from(g.max_anteil_bp())) {
                fehler.push(format!("{fall}: {i} {w} von {gesamt} Cent nach dem Abbau"));
            }
        }
        for a in &ent.kaeufe {
            let x = a.auftrag();
            let s = danach.stueck.get(&x.isin).copied().unwrap_or(0);
            let w = mal(x.limit, s);
            let basis = gesamt.saturating_sub(mal(Cent(100), s)).saturating_add(w);
            if w.saturating_mul(10_000) > basis.saturating_mul(i128::from(g.max_anteil_bp())) {
                fehler.push(format!("{fall}: Kauf {} {w} von {basis} Cent", x.isin));
            }
        }
    }
    assert!(fehler.is_empty(), "{fehler:#?}");
}

/// K9-03: Reichen die Nummern nicht fuer alle Auftraege, entsteht keiner, und die Vergabe
/// verbraucht keine Nummer.
#[test]
fn befund_k9_03_abbruch_verbraucht_keine_kennung() {
    let g = Grenzen::standard();
    let kurse = kurse_alle(10_000);
    let start = u64::MAX.saturating_sub(1);

    let mut topf = topf_neu(TopfId::A, Cent::NULL);
    topf.setze_bestand(isin(0), 10);
    topf.setze_bestand(isin(1), 10);
    let mut vergabe = Kennungsvergabe::nach(start);
    let not = notbremse_auftraege(&g, &topf, &kurse, &mut vergabe);
    assert!(
        not.is_none(),
        "Notbremse trotz erschoepfter Nummern: {not:?}"
    );
    assert_eq!(vergabe.letzte(), start, "Notbremse hat Nummern verbraucht");

    let topf = topf_neu(TopfId::A, Cent(1_000_000));
    let v = vorschlag(BeraterId::Dachs, &[(0, 2_000), (1, 2_000)]);
    let mut vergabe = Kennungsvergabe::nach(start);
    let ent = entscheide(
        &Eingabe {
            grenzen: &g,
            universum: &universum(),
            topf: &topf,
            kurse: &kurse,
            vorschlag: Some(&v),
            halt: false,
        },
        &mut vergabe,
    );
    assert_eq!(
        alle(&ent).count(),
        0,
        "Auftraege trotz erschoepfter Nummern"
    );
    assert_eq!(
        vergabe.letzte(),
        start,
        "Entscheidung hat Nummern verbraucht"
    );
}

// ---------------------------------------------------------------------------------------------
// K15-02 – Kurse muessen zum Handelstag des Zyklus gehoeren (Maus, PR #15)
// ---------------------------------------------------------------------------------------------

/// Ein alter Kurssatz mit seinem eigenen Zeitstempel ist am Abrufzeitpunkt frisch. Er darf
/// trotzdem nicht als Kurs eines spaeteren Handelstags gelten. Ebenso gehoert ein Kurs, dessen
/// Zeitpunkt an einem anderen Tag liegt, nicht zu seinem Handelstag, und der Vergleichskurs muss
/// vor dem Handelstag liegen.
#[test]
fn befund_k15_02_kurse_gehoeren_zum_handelstag() {
    let g = Grenzen::standard();
    let vortag = vortagskurse(&preise_alle(10_000));
    let heute = kurse_alle(10_000);
    assert!(
        pruefe_kurse(&g, &universum(), handelstag(), &heute, &vortag).is_empty(),
        "Vorbedingung"
    );

    // Derselbe Kurssatz fuer den naechsten Handelstag.
    let morgen = Datum::neu(2026, 11, 3).unwrap_or_else(|| unreachable!("Datum"));
    let fehler = pruefe_kurse(&g, &universum(), morgen, &heute, &vortag);
    assert!(
        fehler.contains(&DatenFehler::AbrufNichtAmHandelstag),
        "alter Abruf fuer neuen Tag: {fehler:?}"
    );
    assert!(
        fehler.contains(&DatenFehler::KursNichtAmHandelstag(isin(0))),
        "alter Kurs fuer neuen Tag: {fehler:?}"
    );

    // Der Vergleichskurs vom selben Tag ist kein Vortagskurs.
    let fehler = pruefe_kurse(&g, &universum(), handelstag(), &heute, &heute);
    assert!(
        fehler.contains(&DatenFehler::VergleichskursNichtVorher(isin(0))),
        "Vergleich mit sich selbst: {fehler:?}"
    );
}

/// K15-03 (Maus, PR #15): Der Vergleichskurs darf nicht aus der Zukunft stammen. Ein
/// Vergleichsabruf nach dem Abruf des Zyklus wird abgelehnt, auch wenn sein Handelstag davor
/// liegt.
#[test]
fn befund_k15_03_vergleichsabruf_liegt_nicht_nach_dem_abruf() {
    let g = Grenzen::standard();
    let heute = kurse_alle(10_000);
    let vortag_tag = Datum::neu(2026, 10, 30).unwrap_or_else(|| unreachable!("Datum"));
    let kurszeit = Zeitpunkt::neu(vortag_tag, 17, 30, 0).unwrap_or_else(|| unreachable!("Zeit"));
    // Abruf am 03.11., nach dem Abruf des Zyklus (02.11., 10:00).
    let abruf = Datum::neu(2026, 11, 3)
        .and_then(|d| Zeitpunkt::neu(d, 9, 0, 0))
        .unwrap_or_else(|| unreachable!("Zeit"));
    let eintraege = preise_alle(10_000)
        .iter()
        .map(|(i, c)| {
            let k = Kurs::neu(Cent(*c), "EUR", vortag_tag, kurszeit, Herkunft::Simulation)
                .unwrap_or_else(|e| unreachable!("Kurs: {e:?}"));
            (i.clone(), k)
        })
        .collect();
    let spaet = Kurse::neu(abruf, eintraege).unwrap_or_else(|e| unreachable!("{e:?}"));
    let fehler = pruefe_kurse(&g, &universum(), handelstag(), &heute, &spaet);
    assert!(
        fehler.contains(&DatenFehler::VergleichsabrufNachAbruf),
        "Vergleich aus der Zukunft: {fehler:?}"
    );
}

/// Nur gehaltene Werte sind Pflicht (Wolfgang, 05.11.2026): Fehlt einem Kandidaten der Kurs
/// oder springt er unplausibel, ist er heute nur nicht handelbar; der Zyklus laeuft ohne ihn.
/// Ein Befund zu einem gehaltenen Wert oder zum Abruf laesst den Zyklus ausfallen, und ohne
/// jeden handelbaren Wert wird niemand gefragt.
#[test]
fn nur_gehaltene_werte_sind_pflicht() {
    let g = Grenzen::standard();
    let vortag = vortagskurse(&preise_alle(10_000));
    let mut preise = preise_alle(10_000);
    preise.remove(&isin(3));
    preise.insert(isin(4), 20_000);
    let heute = kurse(&preise);
    let gehalten: BTreeSet<Isin> = [isin(0)].into_iter().collect();

    let t = waehle_tageswerte(&g, &universum(), &gehalten, handelstag(), &heute, &vortag);
    assert!(t.fehler.is_empty(), "{:?}", t.fehler);
    assert_eq!(t.ausgelassen.len(), 2, "{:?}", t.ausgelassen);
    assert!(t.ausgelassen.contains(&DatenFehler::KursFehlt(isin(3))));
    assert!(
        t.ausgelassen
            .iter()
            .any(|f| matches!(f, DatenFehler::Kurssprung { isin: i, .. } if *i == isin(4)))
    );
    let rest = t.universum.isins();
    assert_eq!(rest.len(), 8);
    assert!(!rest.contains(&isin(3)) && !rest.contains(&isin(4)) && rest.contains(&isin(0)));

    // Derselbe Befund an einem gehaltenen Wert: Der Zyklus faellt aus.
    let gehalten: BTreeSet<Isin> = [isin(0), isin(3)].into_iter().collect();
    let t = waehle_tageswerte(&g, &universum(), &gehalten, handelstag(), &heute, &vortag);
    assert_eq!(t.fehler, vec![DatenFehler::KursFehlt(isin(3))]);

    // Ein Befund zum Abruf trifft alle: Der Zyklus faellt aus.
    let morgen = Datum::neu(2026, 11, 3).unwrap_or_else(|| unreachable!("Datum"));
    let t = waehle_tageswerte(&g, &universum(), &BTreeSet::new(), morgen, &heute, &vortag);
    assert!(
        t.fehler.contains(&DatenFehler::AbrufNichtAmHandelstag),
        "{:?}",
        t.fehler
    );

    // Kein Kandidat mit Kurs und nichts gehalten: niemand wird gefragt.
    let leer = kurse(&Preise::new());
    let t = waehle_tageswerte(
        &g,
        &universum(),
        &BTreeSet::new(),
        handelstag(),
        &leer,
        &vortag,
    );
    assert_eq!(t.fehler, vec![DatenFehler::KeinWertHandelbar]);
    assert!(t.universum.instrumente().is_empty());
}
