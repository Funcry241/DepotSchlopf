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
    entscheide(&Eingabe {
        grenzen: g,
        universum: &universum(),
        topf,
        kurse,
        vorschlag: v,
        zyklus: 7,
        halt,
    })
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
    for kurs in [Cent(0), Cent(-100)] {
        let mut kurse = kurse_alle(10_000);
        kurse.0.insert(isin(0), kurs);
        let ergebnis = notbremse_auftraege(&g, &topf, &kurse, 1);
        assert!(
            ergebnis.is_none(),
            "Kurs {kurs}: Notbremse liefert {ergebnis:?} statt Halt"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 1·F04 – Bestandsgrenze mit Toleranz und Abbaupflicht
// ---------------------------------------------------------------------------------------------

/// Durch Kursbewegung liegt ein Wert bei 28 % (Hoechstanteil 25 %, Toleranz 2 Punkte).
/// Das Ziel 25 % liegt nur 3 Punkte darunter, also unter der Umschichtungsschwelle von
/// 5 Punkten. Die Abbaupflicht gilt trotzdem: Es muss verkauft und darf nicht zugekauft werden.
#[test]
#[ignore = "offen: 1·F04 Bestandsgrenze ohne Abbaupflicht"]
fn befund_1_f04_marktbewegung_ueber_toleranz_erzeugt_abbaupflicht() {
    let g = Grenzen::standard();
    let mut topf = Topf::neu(TopfId::A, Cent::euro(7_200));
    topf.positionen.insert(isin(0), 28); // 28 × 100 € = 2.800 € von 10.000 €
    let v = vorschlag(BeraterId::Dachs, &[(0, 2_500)]);
    let ent = lauf(&g, &topf, &kurse_alle(10_000), Some(&v), false);
    assert!(
        !ent.kaeufe.iter().any(|a| a.auftrag().isin == isin(0)),
        "Zukauf trotz Bestand ueber der Toleranz"
    );
    assert!(
        ent.verkaeufe.iter().any(|a| a.auftrag().isin == isin(0)),
        "keine Abbaupflicht bei 28 % Bestand: {:?}",
        ent.befunde
    );
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
// 2·A04 – zwingende Ausstiege durch freiwillige Regeln oder Halt blockiert
// ---------------------------------------------------------------------------------------------

/// Ein Restbestand ausserhalb der Positivliste (2 Stueck zu 50 €: unter Mindestorder,
/// Gebuehr ueber der Gebuehrenquote) wird zwingend verkauft. Halt sperrt ihn nicht.
#[test]
#[ignore = "offen: 2·A04 Halt sperrt den zwingenden Ausstieg"]
fn befund_2_a04_zwingender_ausstieg_trotz_halt_und_mindestorder() {
    let g = Grenzen::standard();
    let mut topf = Topf::neu(TopfId::A, Cent::euro(1_000));
    topf.positionen.insert(isin(10), 2);
    let v = vorschlag(BeraterId::Dachs, &[]);
    let ent = lauf(&g, &topf, &kurse_alle(5_000), Some(&v), true);
    assert!(
        ent.verkaeufe
            .iter()
            .any(|a| a.auftrag().isin == isin(10) && a.auftrag().stueck == 2),
        "zwingender Ausstieg fehlt: {:?}",
        ent.befunde
    );
    assert!(
        ent.kaeufe.is_empty(),
        "Halt muss freiwillige Kaeufe sperren"
    );
}

// ---------------------------------------------------------------------------------------------
// 1·T01 – gezielte Grenzfaelle je harter Grenze (statt nur Zufall)
// ---------------------------------------------------------------------------------------------

/// Kaufgrenze am Limit: Ein Kauf auf genau 25 % am Schlusskurs liegt am Limit darueber.
/// Der Bestand nach dem Kauf muss auch am Limit hoechstens 25 % des Topfes sein (R21, R22).
#[test]
#[ignore = "offen: 1·T01 Hoechstanteil am Schlusskurs statt am Limit geprueft"]
fn grenzfall_hoechstanteil_gilt_am_limit() {
    let g = Grenzen::standard();
    let topf = Topf::neu(TopfId::A, Cent::euro(10_000));
    let v = vorschlag(BeraterId::Dachs, &[(0, 2_500)]);
    let ent = lauf(&g, &topf, &kurse_alle(10_000), Some(&v), false);
    let grenze = Cent::euro(10_000).anteil_bp_ab(g.max_anteil_bp);
    for a in &ent.kaeufe {
        let v = am_limit(a);
        assert!(
            matches!((v, grenze), (Some(v), Some(gr)) if v <= gr),
            "Kauf {:?} am Limit {v:?} ueber Hoechstanteil {grenze:?}",
            a.auftrag()
        );
    }
}

/// Mindestorder Kauf am Limit: jeder Kauf hat am Limit mindestens die Mindestorder.
#[test]
fn grenzfall_mindestorder_kauf_am_limit() {
    let g = Grenzen::standard();
    for bargeld in [49_900_i64, 50_000, 50_600, 51_000, 100_000] {
        let topf = Topf::neu(TopfId::A, Cent(bargeld.saturating_mul(10)));
        let v = vorschlag(BeraterId::Dachs, &[(0, 1_000)]);
        let ent = lauf(&g, &topf, &kurse_alle(10_000), Some(&v), false);
        for a in &ent.kaeufe {
            assert!(
                am_limit(a).is_some_and(|v| v >= g.min_order),
                "Bargeld {bargeld}: Kauf unter Mindestorder"
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
