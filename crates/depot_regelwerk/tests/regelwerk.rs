//! Normative Tests mit erfundenen Zahlen (keine echten Kursdaten).
#![allow(
    clippy::many_single_char_names,
    reason = "kurze Namen im Zufallstest sind hier lesbarer"
)]

use std::collections::BTreeMap;

use depot_kern::{
    BeraterId, Cent, Grenzen, Instrument, InstrumentArt, Isin, Kurse, Topf, TopfId, Universum,
    Vorschlag,
};
use depot_regelwerk::{
    Anlass, Befund, DatenFehler, Eingabe, Entscheidung, GenehmigterAuftrag, Richtung, VerwerfGrund,
    entscheide, notbremse_auftraege, pruefe_kurse,
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

/// Die ersten 10 ISINs sind das Universum, die elfte ist "aus dem DAX geflogen".
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

fn vorschlag(gewichte: &[(usize, i64)]) -> Vorschlag {
    Vorschlag {
        berater: BeraterId::Dachs,
        zielgewichte_bp: gewichte.iter().map(|(i, g)| (isin(*i), *g)).collect(),
        begruendung: BTreeMap::new(),
    }
}

fn lauf(
    grenzen: &Grenzen,
    topf: &Topf,
    kurse: &Kurse,
    v: Option<&Vorschlag>,
    halt: bool,
) -> Entscheidung {
    entscheide(&Eingabe {
        grenzen,
        universum: &universum(),
        topf,
        kurse,
        vorschlag: v,
        zyklus: 7,
        halt,
    })
}

fn volumen(a: &GenehmigterAuftrag) -> Cent {
    a.auftrag()
        .limit
        .mal_stueck(a.auftrag().stueck)
        .unwrap_or(Cent(i64::MAX))
}

#[test]
fn erstkauf_wird_auf_halben_topf_begrenzt() {
    let g = Grenzen::standard();
    let topf = Topf::neu(TopfId::A, Cent::euro(5_000));
    let kurse = kurse_alle(5_000); // 50 € je Stueck
    let v = vorschlag(&[(0, 2_500), (1, 2_500), (2, 2_500), (3, 2_500)]);
    let ent = lauf(&g, &topf, &kurse, Some(&v), false);

    assert!(ent.verkaeufe.is_empty());
    assert_eq!(ent.kaeufe.len(), 4);
    assert!(
        ent.befunde
            .iter()
            .any(|b| matches!(b, Befund::UmschlagGekuerzt { .. }))
    );
    let mut summe = Cent::NULL;
    for a in &ent.kaeufe {
        let x = a.auftrag();
        assert_eq!(x.richtung, Richtung::Kauf);
        assert_eq!(x.limit, Cent(5_050), "Limit = Schlusskurs + 1 %");
        assert_eq!(
            x.stueck, 12,
            "625 € Ziel je Wert nach Kuerzung, 50 € je Stueck"
        );
        assert_eq!(a.beleg().anlass, Anlass::Umschichtung);
        assert_eq!(a.beleg().grenzen_abdruck, g.fingerabdruck());
        summe = summe.checked_add(volumen(a)).unwrap_or(Cent(i64::MAX));
    }
    assert!(summe <= Cent::euro(2_525));
}

#[test]
fn halt_bedeutet_keine_auftraege() {
    let g = Grenzen::standard();
    let topf = Topf::neu(TopfId::A, Cent::euro(5_000));
    let v = vorschlag(&[(0, 2_000)]);
    let ent = lauf(&g, &topf, &kurse_alle(5_000), Some(&v), true);
    assert!(ent.kaeufe.is_empty() && ent.verkaeufe.is_empty());
    assert_eq!(ent.befunde, vec![Befund::Halt]);
}

#[test]
fn kein_vorschlag_keine_auftraege() {
    let g = Grenzen::standard();
    let topf = Topf::neu(TopfId::B, Cent::euro(5_000));
    let ent = lauf(&g, &topf, &kurse_alle(5_000), None, false);
    assert_eq!(ent.befunde, vec![Befund::KeinVorschlag]);
}

#[test]
fn erfundener_wert_verwirft_den_ganzen_vorschlag() {
    let g = Grenzen::standard();
    let topf = Topf::neu(TopfId::A, Cent::euro(5_000));
    let v = vorschlag(&[(0, 2_000), (10, 1_000)]);
    let ent = lauf(&g, &topf, &kurse_alle(5_000), Some(&v), false);
    assert!(ent.kaeufe.is_empty());
    assert_eq!(
        ent.befunde,
        vec![Befund::VorschlagVerworfen(VerwerfGrund::UnbekannterWert(
            isin(10)
        ))]
    );
}

#[test]
fn unmoegliche_gewichte_werden_verworfen() {
    let g = Grenzen::standard();
    let topf = Topf::neu(TopfId::A, Cent::euro(5_000));
    let k = kurse_alle(5_000);
    let neun: Vec<(usize, i64)> = (0..9).map(|i| (i, 1_000)).collect();
    let faelle = [
        (vorschlag(&neun), VerwerfGrund::ZuVieleWerte(9)),
        (
            vorschlag(&[(0, 2_500), (1, 2_500), (2, 2_500), (3, 2_500), (4, 100)]),
            VerwerfGrund::SummeUeber100(10_100),
        ),
        (
            vorschlag(&[(0, -100)]),
            VerwerfGrund::NegativesGewicht(isin(0)),
        ),
    ];
    for (v, grund) in faelle {
        let ent = lauf(&g, &topf, &k, Some(&v), false);
        assert_eq!(ent.befunde, vec![Befund::VorschlagVerworfen(grund)]);
    }
}

#[test]
fn zu_grosses_gewicht_wird_auf_25_prozent_gekuerzt() {
    let g = Grenzen::standard();
    let topf = Topf::neu(TopfId::A, Cent::euro(5_000));
    let v = vorschlag(&[(0, 4_000)]);
    let ent = lauf(&g, &topf, &kurse_alle(1_000), Some(&v), false);
    assert!(ent.befunde.contains(&Befund::GewichtGekuerzt {
        isin: isin(0),
        von_bp: 4_000,
        auf_bp: 2_500
    }));
    let kauf = ent.kaeufe.first().map(GenehmigterAuftrag::auftrag);
    assert_eq!(
        kauf.map(|a| a.stueck),
        Some(125),
        "25 % von 5.000 € bei 10 € je Stueck"
    );
}

#[test]
fn kleine_aenderung_unter_5_punkten_wird_nicht_gehandelt() {
    let g = Grenzen::standard();
    let mut topf = Topf::neu(TopfId::A, Cent::euro(4_000));
    topf.positionen.insert(isin(0), 20); // 20 × 50 € = 1.000 € = 20 %
    let v = vorschlag(&[(0, 2_300)]);
    let ent = lauf(&g, &topf, &kurse_alle(5_000), Some(&v), false);
    assert!(ent.kaeufe.is_empty() && ent.verkaeufe.is_empty());
    assert!(ent.befunde.contains(&Befund::UnterSchwelle(isin(0))));
}

#[test]
fn kleiner_kauf_unter_500_euro_entfaellt() {
    let g = Grenzen::standard();
    let topf = Topf::neu(TopfId::A, Cent::euro(5_000));
    let v = vorschlag(&[(0, 600)]); // 300 €
    let ent = lauf(&g, &topf, &kurse_alle(5_000), Some(&v), false);
    assert!(ent.kaeufe.is_empty());
    assert!(ent.befunde.contains(&Befund::UnterMindestorder(isin(0))));
}

#[test]
fn restposition_wird_komplett_verkauft_auch_unter_500_euro() {
    let g = Grenzen::standard();
    let mut topf = Topf::neu(TopfId::A, Cent::euro(4_700));
    topf.positionen.insert(isin(0), 6); // 300 € = 6 %
    let v = vorschlag(&[]);
    let ent = lauf(&g, &topf, &kurse_alle(5_000), Some(&v), false);
    let verkauf = ent.verkaeufe.first().map(GenehmigterAuftrag::auftrag);
    assert_eq!(verkauf.map(|a| (a.stueck, a.limit)), Some((6, Cent(4_950))));
}

#[test]
fn wert_ausserhalb_der_positivliste_wird_verkauft() {
    let g = Grenzen::standard();
    let mut topf = Topf::neu(TopfId::A, Cent::euro(4_000));
    topf.positionen.insert(isin(10), 20);
    let v = vorschlag(&[]);
    let ent = lauf(&g, &topf, &kurse_alle(5_000), Some(&v), false);
    assert!(
        ent.befunde
            .contains(&Befund::NichtMehrHandelbarVerkauft(isin(10)))
    );
    assert_eq!(ent.verkaeufe.len(), 1);
}

#[test]
fn hoechstens_vier_orders_je_topf() {
    let mut g = Grenzen::standard();
    g.max_umschlag_bp = 10_000;
    let topf = Topf::neu(TopfId::A, Cent::euro(10_000));
    let v = vorschlag(&[
        (0, 1_600),
        (1, 1_600),
        (2, 1_600),
        (3, 1_600),
        (4, 1_600),
        (5, 1_600),
    ]);
    let ent = lauf(&g, &topf, &kurse_alle(5_000), Some(&v), false);
    assert_eq!(ent.kaeufe.len(), 4);
    assert_eq!(
        ent.befunde
            .iter()
            .filter(|b| matches!(b, Befund::OrderLimitErreicht(_)))
            .count(),
        2
    );
}

#[test]
fn verkaeufe_finanzieren_kaeufe_nur_mit_garantiertem_erloes() {
    let mut g = Grenzen::standard();
    g.max_umschlag_bp = 20_000;
    let mut topf = Topf::neu(TopfId::A, Cent::NULL);
    topf.positionen.insert(isin(0), 50); // 2.500 € = 100 %
    let v = vorschlag(&[(1, 2_500)]);
    let ent = lauf(&g, &topf, &kurse_alle(5_000), Some(&v), false);
    assert_eq!(ent.verkaeufe.len(), 1);
    let erloes = volumen(ent.verkaeufe.first().unwrap_or_else(|| unreachable!()));
    let kauf = volumen(ent.kaeufe.first().unwrap_or_else(|| unreachable!()));
    assert!(kauf < erloes);
    assert!(
        ent.kaeufe.iter().all(|a| a.auftrag().kennung.nr > 1),
        "Kaeufe nach den Verkaeufen nummeriert"
    );
}

#[test]
fn gleiche_eingabe_gleiche_ausgabe() {
    let g = Grenzen::standard();
    let mut topf = Topf::neu(TopfId::B, Cent::euro(3_000));
    topf.positionen.insert(isin(2), 30);
    let v = vorschlag(&[(0, 2_000), (1, 1_500), (3, 2_500)]);
    let a = lauf(&g, &topf, &kurse_alle(4_321), Some(&v), false);
    let b = lauf(&g, &topf, &kurse_alle(4_321), Some(&v), false);
    assert_eq!(a.kaeufe, b.kaeufe);
    assert_eq!(a.verkaeufe, b.verkaeufe);
    assert_eq!(a.befunde, b.befunde);
}

#[test]
fn kursdaten_fehlend_oder_sprunghaft() {
    let g = Grenzen::standard();
    let vortag = kurse_alle(10_000);
    let mut heute = kurse_alle(10_000);
    heute.0.remove(&isin(0));
    heute.0.insert(isin(1), Cent(12_000));
    heute.0.insert(isin(2), Cent(0));
    let fehler = pruefe_kurse(&g, &universum(), &heute, &vortag);
    assert!(fehler.contains(&DatenFehler::KursFehlt(isin(0))));
    assert!(fehler.contains(&DatenFehler::Kurssprung {
        isin: isin(1),
        sprung_bp: 2_000
    }));
    assert!(fehler.contains(&DatenFehler::KursNichtPositiv(isin(2))));
    assert!(pruefe_kurse(&g, &universum(), &vortag, &vortag).is_empty());
}

#[test]
fn notbremse_verkauft_alles_mit_limit() {
    let g = Grenzen::standard();
    let mut topf = Topf::neu(TopfId::A, Cent::euro(100));
    topf.positionen.insert(isin(0), 3);
    topf.positionen.insert(isin(5), 40);
    let auftraege = notbremse_auftraege(&g, &topf, &kurse_alle(10_000), 9).unwrap_or_default();
    assert_eq!(auftraege.len(), 2);
    for a in &auftraege {
        assert_eq!(a.auftrag().richtung, Richtung::Verkauf);
        assert_eq!(a.auftrag().limit, Cent(9_900));
        assert_eq!(a.beleg().anlass, Anlass::Notbremse);
        assert_eq!(a.auftrag().stueck, topf.stueck(&a.auftrag().isin));
    }
}

/// Einfacher, reproduzierbarer Zufallsgenerator (keine externe Abhaengigkeit).
struct Lcg(u64);

impl Lcg {
    fn naechste(&mut self, bis: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33).checked_rem(bis.max(1)).unwrap_or(0)
    }
    fn i(&mut self, bis: u64) -> i64 {
        i64::try_from(self.naechste(bis)).unwrap_or(0)
    }
}

/// 5.000 zufaellige Lagen, geprueft am Schlusskurs: Keine Entscheidung verletzt dort eine Grenze.
/// Am Limit, also im ungünstigsten Fall (R21), pruefen die gezielten Grenzfaelle in `befunde.rs`.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "alle Invarianten an einer Stelle pruefen ist hier uebersichtlicher"
)]
fn zufallstest_grenzen_am_schlusskurs() {
    let g = Grenzen::standard();
    let u = universum();
    let mut r = Lcg(2026);
    for runde in 0..5_000 {
        let mut kurse = Kurse::default();
        for i in 0..11 {
            kurse
                .0
                .insert(isin(i), Cent(r.i(50_000).saturating_add(100)));
        }
        let mut topf = Topf::neu(TopfId::A, Cent(r.i(1_000_000)));
        for _ in 0..r.naechste(6) {
            topf.positionen.insert(
                isin(usize::try_from(r.naechste(11)).unwrap_or(0)),
                r.naechste(200),
            );
        }
        let mut gewichte = Vec::new();
        for _ in 0..r.naechste(10) {
            gewichte.push((usize::try_from(r.naechste(10)).unwrap_or(0), r.i(3_500)));
        }
        let v = vorschlag(&gewichte);
        let ent = entscheide(&Eingabe {
            grenzen: &g,
            universum: &u,
            topf: &topf,
            kurse: &kurse,
            vorschlag: Some(&v),
            zyklus: 1,
            halt: false,
        });

        let topfwert = topf.wert(&kurse).unwrap_or(Cent::NULL);
        let alle: Vec<&GenehmigterAuftrag> = ent.verkaeufe.iter().chain(&ent.kaeufe).collect();
        assert!(
            alle.len() <= g.max_orders_je_topf,
            "Runde {runde}: zu viele Orders"
        );

        let mut budget = topf.bargeld;
        let mut umschlag = Cent::NULL;
        for a in &ent.verkaeufe {
            let x = a.auftrag();
            let gehalten = topf.stueck(&x.isin);
            assert!(
                x.stueck >= 1 && x.stueck <= gehalten,
                "Runde {runde}: Verkauf ueber Bestand"
            );
            let kurs = kurse.kurs(&x.isin).unwrap_or(Cent::NULL);
            assert!(
                x.limit < kurs && x.limit >= Cent(kurs.0.saturating_mul(99).saturating_div(100)),
                "Runde {runde}: Verkaufslimit"
            );
            let v = volumen(a);
            let zum_kurs = kurs.mal_stueck(x.stueck).unwrap_or(Cent::NULL);
            assert!(
                zum_kurs >= g.min_order || x.stueck == gehalten,
                "Runde {runde}: Mindestorder Verkauf"
            );
            budget = Cent(
                budget
                    .0
                    .saturating_add(v.0)
                    .saturating_sub(g.gebuehren.fuer(v).unwrap_or(Cent(i64::MAX)).0),
            );
            umschlag = Cent(
                umschlag.0.saturating_add(
                    kurs.0
                        .saturating_mul(i64::try_from(x.stueck).unwrap_or(i64::MAX)),
                ),
            );
        }
        for a in &ent.kaeufe {
            let x = a.auftrag();
            assert!(
                u.handelbar(&x.isin, &g),
                "Runde {runde}: Kauf ausserhalb der Positivliste"
            );
            let kurs = kurse.kurs(&x.isin).unwrap_or(Cent::NULL);
            assert!(
                x.limit > kurs
                    && x.limit
                        <= Cent(
                            kurs.0
                                .saturating_mul(101)
                                .saturating_div(100)
                                .saturating_add(1)
                        ),
                "Runde {runde}: Kauflimit"
            );
            let v = volumen(a);
            assert!(v >= g.min_order, "Runde {runde}: Mindestorder Kauf");
            budget = Cent(
                budget
                    .0
                    .saturating_sub(v.0)
                    .saturating_sub(g.gebuehren.fuer(v).unwrap_or(Cent(i64::MAX)).0),
            );
            assert!(budget.0 >= 0, "Runde {runde}: Kauf ohne Deckung");
            let danach = kurs.0.saturating_mul(
                i64::try_from(topf.stueck(&x.isin).saturating_add(x.stueck)).unwrap_or(i64::MAX),
            );
            assert!(
                danach
                    <= topfwert
                        .anteil_bp_ab(g.max_anteil_bp)
                        .unwrap_or(Cent::NULL)
                        .0,
                "Runde {runde}: ueber 25 %"
            );
            umschlag = Cent(
                umschlag.0.saturating_add(
                    kurs.0
                        .saturating_mul(i64::try_from(x.stueck).unwrap_or(i64::MAX)),
                ),
            );
        }
        let erlaubt = topfwert
            .anteil_bp_ab(g.max_umschlag_bp)
            .unwrap_or(Cent::NULL);
        assert!(umschlag <= erlaubt, "Runde {runde}: Umschlag");
    }
}
