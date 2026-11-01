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
    BeraterId, Cent, Grenzen, Instrument, InstrumentArt, Isin, Kurse, Topf, TopfId, Universum,
    Vorschlag,
};
use depot_regelwerk::{
    AuftragsKennung, Eingabe, Entscheidung, GenehmigterAuftrag, Richtung, entscheide,
    notbremse_auftraege, pruefe_kurse,
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

fn kurse_alle(cent: i64) -> Kurse {
    Kurse((0..11).map(|i| (isin(i), Cent(cent))).collect())
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
    entscheide(&Eingabe {
        grenzen: g,
        universum: u,
        topf,
        kurse,
        vorschlag: v,
        zyklus: 7,
        halt,
    })
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
            bargeld: i128::from(topf.bargeld.0),
            stueck: topf.positionen.clone(),
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
            summe.saturating_add(mal(kurse.kurs(i).unwrap_or(Cent::NULL), *s))
        })
    }

    /// Anteil eines Werts in Basispunkten, aufgerundet.
    fn anteil_bp(&self, isin: &Isin, kurse: &Kurse) -> i128 {
        let w = mal(
            kurse.kurs(isin).unwrap_or(Cent::NULL),
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
#[ignore = "offen: 1·F02 Vorschlag des falschen Beraters wird angenommen"]
fn befund_1_f02_vorschlag_fuer_fremden_bereich_wird_abgelehnt() {
    let g = Grenzen::standard();
    let topf = Topf::neu(TopfId::A, Cent::euro(10_000));
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

/// Kurs 0 oder negativ: kein Notbremsen-Auftrag, sondern Halt (`None`).
#[test]
#[ignore = "offen: 1·F03 Notbremse erzeugt Auftraege mit Nullkurs"]
fn befund_1_f03_nullkurs_erzeugt_keinen_notauftrag_sondern_halt() {
    let g = Grenzen::standard();
    let mut topf = Topf::neu(TopfId::A, Cent::NULL);
    topf.positionen.insert(isin(0), 10);
    let mut angenommen = Vec::new();
    for kurs in [Cent(0), Cent(-100)] {
        let mut kurse = kurse_alle(10_000);
        kurse.0.insert(isin(0), kurs);
        let ergebnis = notbremse_auftraege(&g, &topf, &kurse, 1);
        if ergebnis.is_some() {
            angenommen.push(format!("Kurs {kurs}: {ergebnis:?}"));
        }
    }
    assert!(angenommen.is_empty(), "statt Halt: {angenommen:?}");
}

/// (Fall, gehaltener Wert, Ziel)
type Fall = (&'static str, Option<usize>, &'static [(usize, i64)]);

/// Auch das Regelwerk erzeugt bei einem Kurs von 0 oder darunter keinen Auftrag, weder
/// freiwillig noch zwingend (Matrix R36: unplausibler Kurs heisst nur Halt, kein Verkauf).
#[test]
#[ignore = "offen: 1·F03 Regelwerk verkauft zum Nullkurs"]
fn befund_1_f03_nullkurs_erzeugt_im_regelwerk_keinen_auftrag() {
    let g = Grenzen::standard();
    let mut fehler = Vec::new();
    for kurs in [Cent(0), Cent(-100)] {
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
            let mut topf = Topf::neu(TopfId::A, Cent::euro(5_000));
            if let Some(i) = gehalten {
                topf.positionen.insert(isin(i), 50);
            }
            let mut kurse = kurse_alle(10_000);
            kurse.0.insert(isin(gehalten.unwrap_or(0)), kurs);
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
// 1·F04 – Bestandsgrenze mit Toleranz und Abbaupflicht
// ---------------------------------------------------------------------------------------------

/// Durch Kursbewegung liegt ein Wert bei 28 % (Hoechstanteil 25 %, Toleranz 2 Punkte).
/// Das Ziel 25 % liegt nur 3 Punkte darunter, also unter der Umschichtungsschwelle von
/// 5 Punkten. Die Abbaupflicht gilt trotzdem: Nach den genehmigten Auftraegen (voll zum Limit
/// ausgefuehrt, hoechste Gebuehr) liegt der Wert hoechstens beim Hoechstanteil, und es wird
/// nicht zugekauft.
#[test]
#[ignore = "offen: 1·F04 Bestandsgrenze ohne Abbaupflicht"]
fn befund_1_f04_marktbewegung_ueber_toleranz_erzeugt_abbaupflicht() {
    let g = Grenzen::standard();
    let kurse = kurse_alle(10_000);
    let mut topf = Topf::neu(TopfId::A, Cent::euro(7_200));
    topf.positionen.insert(isin(0), 28); // 28 × 100 € = 2.800 € von 10.000 €
    let v = vorschlag(BeraterId::Dachs, &[(0, 2_500)]);
    let ent = lauf(&g, &topf, &kurse, Some(&v), false);
    assert!(
        !ent.kaeufe.iter().any(|a| a.auftrag().isin == isin(0)),
        "Zukauf trotz Bestand ueber der Toleranz"
    );
    let danach = Danach::rechne(&topf, &ent);
    let anteil = danach.anteil_bp(&isin(0), &kurse);
    assert!(
        anteil <= i128::from(g.max_anteil_bp),
        "nach Abbau {anteil} bp statt hoechstens {} bp; {:?}",
        g.max_anteil_bp,
        ent.befunde
    );
}

/// Neun gehaltene Werte bei hoechstens acht. Der Vorschlag nennt alle neun. Er darf nicht
/// einfach verworfen werden: Nach den genehmigten Auftraegen sind es hoechstens acht Werte,
/// und kein neuer Wert kommt hinzu.
#[test]
#[ignore = "offen: 1·F04 Positionszahl ueber der Grenze ohne Abbaupflicht"]
fn befund_1_f04_zu_viele_positionen_erzeugen_abbaupflicht() {
    let g = Grenzen::standard();
    let kurse = kurse_alle(10_000);
    let mut topf = Topf::neu(TopfId::A, Cent::euro(1_000));
    for i in 0..9 {
        topf.positionen.insert(isin(i), 10); // je 1.000 € = 10 %
    }
    let v = vorschlag(
        BeraterId::Dachs,
        &(0..9).map(|i| (i, 1_000)).collect::<Vec<_>>(),
    );
    let ent = lauf(&g, &topf, &kurse, Some(&v), false);
    let danach = Danach::rechne(&topf, &ent);
    assert!(
        danach.stueck.len() <= g.max_werte,
        "nach den Auftraegen {} Werte statt hoechstens {}; {:?}",
        danach.stueck.len(),
        g.max_werte,
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

// ---------------------------------------------------------------------------------------------
// 1·F05 – Umschlaggrenze am Limit
// ---------------------------------------------------------------------------------------------

/// Zwei Kaeufe zu je 25 % fuellen die Umschlaggrenze (50 %) am Schlusskurs genau aus.
/// Am Limit (+1 %) liegt der Umschlag darueber und muss gekuerzt werden.
#[test]
#[ignore = "offen: 1·F05 Umschlaggrenze am Schlusskurs statt am Limit geprueft"]
fn befund_1_f05_umschlag_am_limit_ueber_der_grenze_wird_gekuerzt() {
    let g = Grenzen::standard();
    let topf = Topf::neu(TopfId::A, Cent::euro(10_000));
    let v = vorschlag(BeraterId::Dachs, &[(0, 2_500), (1, 2_500)]);
    let ent = lauf(&g, &topf, &kurse_alle(10_000), Some(&v), false);
    let erlaubt = Cent::euro(10_000).anteil_bp_ab(g.max_umschlag_bp);
    let umschlag = alle(&ent).try_fold(Cent::NULL, |s, a| s.checked_add(am_limit(a)?));
    assert!(
        matches!((umschlag, erlaubt), (Some(u), Some(e)) if u <= e),
        "Umschlag am Limit {umschlag:?} ueber erlaubt {erlaubt:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 1·F06 – Teilverkauf unter Mindestorder am Limit
// ---------------------------------------------------------------------------------------------

/// Teilverkauf von 5 Stueck zu 100 €: am Schlusskurs 500 € (Mindestorder), am Limit
/// (99 €) nur 495 €. Er muss entfallen. Ein Verkauf des ganzen Bestands bleibt erlaubt.
#[test]
#[ignore = "offen: 1·F06 Teilverkauf unter Mindestorder am Limit"]
fn befund_1_f06_teilverkauf_unter_mindestorder_am_limit_abgelehnt() {
    let g = Grenzen::standard();
    let mut topf = Topf::neu(TopfId::A, Cent::euro(9_000));
    topf.positionen.insert(isin(0), 10); // 10 % des Topfes
    let v = vorschlag(BeraterId::Dachs, &[(0, 500)]);
    let ent = lauf(&g, &topf, &kurse_alle(10_000), Some(&v), false);
    for a in &ent.verkaeufe {
        let ganz = a.auftrag().stueck == topf.stueck(&a.auftrag().isin);
        let volumen = am_limit(a);
        assert!(
            ganz || volumen.is_some_and(|v| v >= g.min_order),
            "Teilverkauf {:?} am Limit {volumen:?} unter Mindestorder {}",
            a.auftrag(),
            g.min_order
        );
    }
}

/// Gegenprobe zu 1·F06: Der Verkauf des ganzen Bestands ist auch unter Mindestorder erlaubt.
#[test]
fn grenzfall_gesamtverkauf_unter_mindestorder_bleibt_erlaubt() {
    let g = Grenzen::standard();
    let mut topf = Topf::neu(TopfId::A, Cent::euro(4_700));
    topf.positionen.insert(isin(0), 3); // 300 € = 6 % des Topfes, Ziel 0
    let v = vorschlag(BeraterId::Dachs, &[]);
    let ent = lauf(&g, &topf, &kurse_alle(10_000), Some(&v), false);
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
#[ignore = "offen: 1·F08 fehlender Vortagskurs gilt als bestandener Sprungtest"]
fn befund_1_f08_fehlender_vortag_ist_kein_bestandener_sprungtest() {
    let g = Grenzen::standard();
    let heute = kurse_alle(10_000);
    let mut vortag = kurse_alle(10_000);
    vortag.0.remove(&isin(3));
    let fehler = pruefe_kurse(&g, &universum(), &heute, &vortag);
    assert!(
        !fehler.is_empty(),
        "fehlender Vortagskurs von {} wurde still akzeptiert",
        isin(3)
    );
}

// ---------------------------------------------------------------------------------------------
// 1·F09 – Auftragskennungen kollidieren
// ---------------------------------------------------------------------------------------------

/// Notbremse und Umschichtung im selben Lauf vergeben verschiedene Kennungen.
#[test]
#[ignore = "offen: 1·F09 Auftragskennungen von Notbremse und Umschichtung kollidieren"]
fn befund_1_f09_notbremse_und_umschichtung_vergeben_verschiedene_kennungen() {
    let g = Grenzen::standard();
    let mut topf = Topf::neu(TopfId::A, Cent::euro(5_000));
    topf.positionen.insert(isin(0), 50);
    let v = vorschlag(BeraterId::Dachs, &[(1, 2_500)]);
    let ent = lauf(&g, &topf, &kurse_alle(10_000), Some(&v), false);
    let not = notbremse_auftraege(&g, &topf, &kurse_alle(10_000), 7).unwrap_or_default();
    assert!(alle(&ent).count() > 0 && !not.is_empty(), "Vorbedingung");
    let umschichtung: BTreeSet<AuftragsKennung> = alle(&ent).map(|a| a.auftrag().kennung).collect();
    let doppelt: Vec<AuftragsKennung> = not
        .iter()
        .map(|a| a.auftrag().kennung)
        .filter(|k| umschichtung.contains(k))
        .collect();
    assert!(doppelt.is_empty(), "doppelte Kennungen: {doppelt:?}");
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
    for (i, gehalten) in &topf.positionen {
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
#[ignore = "offen: 2·A04 Halt sperrt den zwingenden Ausstieg"]
fn befund_2_a04_halt_sperrt_den_zwingenden_ausstieg_nicht() {
    let g = Grenzen::standard();
    let u = universum();
    let mut topf = Topf::neu(TopfId::A, Cent::euro(8_000));
    topf.positionen.insert(isin(10), 20); // 1.000 € ausserhalb der Positivliste
    topf.positionen.insert(isin(0), 10); // 1.000 € im Universum
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
#[ignore = "offen: 2·A04 Umschlaggrenze kuerzt den zwingenden Ausstieg"]
fn befund_2_a04_umschlaggrenze_kuerzt_den_zwingenden_ausstieg_nicht() {
    let g = Grenzen::standard();
    let u = universum();
    let mut topf = Topf::neu(TopfId::A, Cent::euro(2_000));
    topf.positionen.insert(isin(10), 80);
    let v = vorschlag(BeraterId::Dachs, &[]);
    let ent = lauf_mit(&g, &u, &topf, &kurse_alle(10_000), Some(&v), false);
    let fehler = ausstiege_vollstaendig(&topf, &u, &g, &ent);
    assert!(fehler.is_empty(), "{fehler:?}; {:?}", ent.befunde);
}

/// Nur Orderzahl: Sechs Werte verlassen die Positivliste (hoechstens vier Orders). Alle sechs
/// werden verkauft.
#[test]
#[ignore = "offen: 2·A04 Orderzahl sperrt zwingende Ausstiege"]
fn befund_2_a04_orderzahl_sperrt_zwingende_ausstiege_nicht() {
    let mut g = Grenzen::standard();
    g.max_umschlag_bp = 10_000; // Umschlag hier bewusst offen
    let u = universum_bis(5);
    let mut topf = Topf::neu(TopfId::A, Cent::euro(4_000));
    for i in 5..11 {
        topf.positionen.insert(isin(i), 10); // je 1.000 €
    }
    let v = vorschlag(BeraterId::Dachs, &[]);
    let ent = lauf_mit(&g, &u, &topf, &kurse_alle(10_000), Some(&v), false);
    let fehler = ausstiege_vollstaendig(&topf, &u, &g, &ent);
    assert!(fehler.is_empty(), "{fehler:?}; {:?}", ent.befunde);
}

/// Nur Mindestorder und Gebuehrenquote: Restbestand 2 Stueck zu 50 € (100 €, Gebuehr 1,50 €
/// = 1,5 %). Er wird zwingend verkauft. Haelt schon heute.
#[test]
fn grenzfall_2_a04_kleiner_rest_wird_zwingend_verkauft() {
    let g = Grenzen::standard();
    let u = universum();
    let mut topf = Topf::neu(TopfId::A, Cent::euro(1_000));
    topf.positionen.insert(isin(10), 2);
    let v = vorschlag(BeraterId::Dachs, &[]);
    let ent = lauf_mit(&g, &u, &topf, &kurse_alle(5_000), Some(&v), false);
    let fehler = ausstiege_vollstaendig(&topf, &u, &g, &ent);
    assert!(fehler.is_empty(), "{fehler:?}; {:?}", ent.befunde);
}

/// Alles zugleich: Halt, Umschlag ueber der Grenze, mehr Ausstiege als Orders, ein Rest unter
/// Mindestorder. Alle Ausstiege laufen vollstaendig, nichts Freiwilliges.
#[test]
#[ignore = "offen: 2·A04 Kombination sperrt zwingende Ausstiege"]
fn befund_2_a04_kombination_sperrt_zwingende_ausstiege_nicht() {
    let g = Grenzen::standard();
    let u = universum_bis(5);
    let mut topf = Topf::neu(TopfId::A, Cent::euro(500));
    for i in 5..10 {
        topf.positionen.insert(isin(i), 20); // je 2.000 €
    }
    topf.positionen.insert(isin(10), 2); // 200 €, unter Mindestorder
    topf.positionen.insert(isin(0), 10); // freiwillig, im Universum
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

/// Kaufgrenze am Limit mit vorhandenem Bestand: 20 Stueck (20 %) sind gehalten, das Ziel ist
/// 25 %. Nach dem Kauf, voll zum Limit ausgefuehrt und mit hoechster Gebuehr, liegt der Bestand
/// hoechstens beim Hoechstanteil (R21, R22). Der Wert wird dabei zum Kauflimit angesetzt,
/// der uebrige Topf zu Kursen.
#[test]
#[ignore = "offen: 1·T01 Hoechstanteil am Schlusskurs statt am Limit geprueft"]
fn grenzfall_hoechstanteil_gilt_am_limit() {
    let g = Grenzen::standard();
    let kurse = kurse_alle(10_000);
    let mut topf = Topf::neu(TopfId::A, Cent::euro(8_000));
    topf.positionen.insert(isin(0), 20);
    let v = vorschlag(BeraterId::Dachs, &[(0, 2_500)]);
    let ent = lauf(&g, &topf, &kurse, Some(&v), false);
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
        wert.saturating_mul(10_000) <= gesamt.saturating_mul(i128::from(g.max_anteil_bp)),
        "nach dem Kauf {stueck} Stueck zu {limit} Cent = {wert} von {gesamt} Cent, \
         ueber {} bp",
        g.max_anteil_bp
    );
}

/// Mindestorder Kauf am Limit, stueckgenau: Kurs 100 €, Limit 101 €, Mindestorder 500 €, Ziel
/// 10 % des Topfes. 4 Stueck (404 €) sind zu wenig, ab 5 Stueck (505 €) wird gekauft.
#[test]
fn grenzfall_mindestorder_kauf_am_limit() {
    let g = Grenzen::standard();
    for (topf_euro, erwartet) in [
        (4_999, None),
        (5_000, Some(5)),
        (5_999, Some(5)),
        (6_000, Some(6)),
    ] {
        let topf = Topf::neu(TopfId::A, Cent::euro(topf_euro));
        let v = vorschlag(BeraterId::Dachs, &[(0, 1_000)]);
        let ent = lauf(&g, &topf, &kurse_alle(10_000), Some(&v), false);
        let gekauft = ent.kaeufe.first().map(|a| a.auftrag().stueck);
        assert_eq!(gekauft, erwartet, "Topf {topf_euro} €");
        for a in &ent.kaeufe {
            assert!(
                am_limit(a).is_some_and(|v| v >= g.min_order),
                "Topf {topf_euro} €: Kauf unter Mindestorder"
            );
        }
    }
}

/// Deckung: Kauf am Limit plus Gebuehr uebersteigt nie das Bargeld, auch knapp an der Grenze.
/// Anteil und Umschlag sind hier bewusst offen, damit allein die Deckung begrenzt.
#[test]
fn grenzfall_deckung_mit_gebuehr_am_limit() {
    let mut g = Grenzen::standard();
    g.max_anteil_bp = 10_000;
    g.max_umschlag_bp = 10_000;
    for bargeld in [50_649_i64, 50_650, 50_651, 101_000, 101_150, 101_151] {
        let topf = Topf::neu(TopfId::A, Cent(bargeld));
        let v = vorschlag(BeraterId::Dachs, &[(0, 10_000)]);
        let ent = lauf(&g, &topf, &kurse_alle(10_000), Some(&v), false);
        let mut kosten = Cent::NULL;
        for a in &ent.kaeufe {
            let v = am_limit(a).unwrap_or(Cent(i64::MAX));
            let gebuehr = g.gebuehren.fuer(v).unwrap_or(Cent(i64::MAX));
            kosten = kosten
                .checked_add(v)
                .and_then(|k| k.checked_add(gebuehr))
                .unwrap_or(Cent(i64::MAX));
        }
        assert!(
            kosten <= Cent(bargeld),
            "Bargeld {bargeld}: Kaeufe kosten {kosten}"
        );
        // 5 Stueck zu 101 € plus 1,50 € Gebuehr = 506,50 €: ab dort muss gekauft werden.
        assert_eq!(ent.kaeufe.is_empty(), bargeld < 50_650, "Bargeld {bargeld}");
    }
}

/// Orderzahl: genau an der Grenze (4) wird nichts weggelassen, darueber schon.
#[test]
fn grenzfall_orderzahl_genau_an_der_grenze() {
    let g = Grenzen::standard();
    let topf = Topf::neu(TopfId::A, Cent::euro(100_000));
    let vier = vorschlag(
        BeraterId::Dachs,
        &[(0, 1_000), (1, 1_000), (2, 1_000), (3, 1_000)],
    );
    let ent = lauf(&g, &topf, &kurse_alle(10_000), Some(&vier), false);
    assert_eq!(alle(&ent).count(), 4);
    let fuenf = vorschlag(
        BeraterId::Dachs,
        &[(0, 1_000), (1, 1_000), (2, 1_000), (3, 1_000), (4, 1_000)],
    );
    let ent = lauf(&g, &topf, &kurse_alle(10_000), Some(&fuenf), false);
    assert_eq!(alle(&ent).count(), g.max_orders_je_topf);
}

/// Kurssprung: genau an der Grenze (15 %) plausibel, einen Cent darueber nicht.
#[test]
fn grenzfall_kurssprung_genau_an_der_grenze() {
    let g = Grenzen::standard();
    let vortag = kurse_alle(10_000);
    let mut heute = kurse_alle(10_000);
    heute.0.insert(isin(0), Cent(11_500));
    assert!(pruefe_kurse(&g, &universum(), &heute, &vortag).is_empty());
    heute.0.insert(isin(0), Cent(11_501));
    assert_eq!(pruefe_kurse(&g, &universum(), &heute, &vortag).len(), 1);
}

/// Kauflimit: hoechstens 1 % ueber dem Kurs, Verkaufslimit hoechstens 1 % darunter.
#[test]
fn grenzfall_limit_abstand() {
    let g = Grenzen::standard();
    let mut topf = Topf::neu(TopfId::A, Cent::euro(5_000));
    topf.positionen.insert(isin(0), 50);
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
