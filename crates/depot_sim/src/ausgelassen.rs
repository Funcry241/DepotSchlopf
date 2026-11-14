use crate::Lage;
use crate::lauf::{Zyklus, begruende};
use depot_journal::Journal;
use depot_kern::{Isin, Kurse, Topf, Vorschlag};
use depot_regelwerk::entscheidung::ist_gewicht_bp;

pub(crate) fn ergaenze_ausgelassene(topf: &Topf, kurse: &Kurse, v: &mut Vorschlag) -> Vec<Isin> {
    let mut ergaenzt = Vec::new();
    for isin in topf.positionen().keys() {
        if !v.zielgewichte_bp.contains_key(isin)
            && let Some(gewicht) = ist_gewicht_bp(topf, kurse, isin)
        {
            v.zielgewichte_bp.insert(isin.clone(), gewicht);
            v.begruendung
                .insert(isin.clone(), "ausgelassen, unverändert gehalten".into());
            ergaenzt.push(isin.clone());
        }
    }
    ergaenzt
}

pub(crate) fn belege_ausgelassene(
    journal: &mut Journal,
    zyklus: &Zyklus<'_>,
    lage: &Lage<'_>,
    vorschlag: &mut Option<Vorschlag>,
    wer: &str,
) -> Result<(), crate::LaufFehler> {
    if let Some(vorschlag) = vorschlag {
        for isin in ergaenze_ausgelassene(lage.topf, lage.kurse, vorschlag) {
            let wert = zyklus
                .universum
                .instrumente()
                .iter()
                .find(|instrument| instrument.isin == isin)
                .map_or(isin.als_str(), |instrument| instrument.name.as_str());
            let topf = crate::lauf::topf_name(lage.topf.id());
            begruende(
                journal,
                zyklus.tag,
                wer,
                &format!("{topf}: ausgelassen, unverändert gehalten: {wert}"),
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use depot_kern::{
        BeraterId, Cent, Datum, Herkunft, Instrument, InstrumentArt, Kurs, TopfId, Universum,
        Zeitpunkt,
    };
    use depot_regelwerk::{
        Befund, Eingabe, Entscheidung, Kennungsvergabe, VerwerfGrund, entscheide,
    };

    fn vorbedingung<T, E: std::fmt::Debug>(r: Result<T, E>) -> Result<T, String> {
        r.map_err(|e| format!("{e:?}"))
    }

    fn auswerten(
        bargeld: i64,
        sap_stueck: u64,
        ziele: impl IntoIterator<Item = (Isin, i64)>,
    ) -> Result<(Entscheidung, Vec<Isin>), String> {
        let sap = vorbedingung(Isin::neu("DE0007164600"))?;
        let alv = vorbedingung(Isin::neu("DE0008404005"))?;
        let universum = Universum::neu(
            [(sap.clone(), "SAP"), (alv.clone(), "Allianz")]
                .into_iter()
                .map(|(isin, name)| Instrument {
                    isin,
                    name: name.into(),
                    art: InstrumentArt::Aktie,
                })
                .collect(),
        );
        let mut topf = vorbedingung(Topf::neu(TopfId::A, Cent(bargeld)))?;
        topf.setze_bestand(sap.clone(), sap_stueck);
        topf.setze_bestand(alv.clone(), 10);
        let tag = Datum::neu(2026, 11, 10).ok_or("Datum")?;
        let zeit = Zeitpunkt::frankfurt(tag, 17, 35, 0).ok_or("Zeit")?;
        let kurs = vorbedingung(Kurs::neu(
            Cent(10_000),
            "EUR",
            tag,
            zeit,
            Herkunft::Simulation,
        ))?;
        let kurse = vorbedingung(Kurse::neu(zeit, vec![(sap, kurs.clone()), (alv, kurs)]))?;
        let mut v = Vorschlag {
            berater: BeraterId::Dachs,
            zielgewichte_bp: ziele.into_iter().collect(),
            begruendung: std::collections::BTreeMap::new(),
        };
        let ergaenzt = ergaenze_ausgelassene(&topf, &kurse, &mut v);
        let entscheidung = entscheide(
            &Eingabe {
                grenzen: &depot_kern::Grenzen::standard(),
                universum: &universum,
                topf: &topf,
                kurse: &kurse,
                vorschlag: Some(&v),
                halt: false,
            },
            &mut Kennungsvergabe::nach(0),
        );
        Ok((entscheidung, ergaenzt))
    }

    #[test]
    fn ausgelassen_keine_order() -> Result<(), String> {
        let alv = vorbedingung(Isin::neu("DE0008404005"))?;
        let sap = vorbedingung(Isin::neu("DE0007164600"))?;
        let (e, ergaenzt) = auswerten(250_000, 15, [(alv, 2_000)])?;
        assert_eq!(ergaenzt, [sap]);
        assert!(e.verkaeufe.is_empty() && e.kaeufe.is_empty(), "{e:?}");
        Ok(())
    }

    #[test]
    fn null_verkauft_weiter() -> Result<(), String> {
        let sap = vorbedingung(Isin::neu("DE0007164600"))?;
        let (e, ergaenzt) = auswerten(250_000, 15, [(sap.clone(), 0)])?;
        assert!(!ergaenzt.contains(&sap));
        assert_eq!(
            e.verkaeufe.first().map(|v| v.auftrag().stueck),
            Some(15),
            "{e:?}"
        );
        assert!(e.kaeufe.is_empty());
        Ok(())
    }

    #[test]
    fn ausgelassen_ueber_hoechstanteil() -> Result<(), String> {
        let sap = vorbedingung(Isin::neu("DE0007164600"))?;
        let (e, ergaenzt) = auswerten(120_000, 18, [])?;
        assert!(ergaenzt.contains(&sap));
        assert!(
            e.befunde
                .iter()
                .any(|b| matches!(b, Befund::GewichtGekuerzt {
            isin, von_bp: 4500, auf_bp: 4000,
        } if *isin == sap)),
            "{e:?}"
        );
        assert!(!e.verkaeufe.is_empty(), "{e:?}");
        Ok(())
    }

    #[test]
    fn ausgelassen_summe_ueber_100() -> Result<(), String> {
        let alv = vorbedingung(Isin::neu("DE0008404005"))?;
        let sap = vorbedingung(Isin::neu("DE0007164600"))?;
        let (e, ergaenzt) = auswerten(250_000, 15, [(alv, 8_000)])?;
        assert_eq!(ergaenzt, [sap]);
        assert!(
            e.befunde
                .contains(&Befund::VorschlagVerworfen(VerwerfGrund::SummeUeber100(
                    11_000
                ))),
            "{e:?}"
        );
        assert!(e.kaeufe.is_empty() && e.verkaeufe.is_empty());
        Ok(())
    }
}
