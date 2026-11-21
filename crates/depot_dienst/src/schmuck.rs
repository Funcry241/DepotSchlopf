//! Betriebsanzeige in der Navigation.

use depot_kern::BeraterId;
use depot_lage::Betrieb;

pub(crate) fn ampel(betrieb: Option<&Betrieb>, kurssperre: bool) -> &'static str {
    if kurssperre {
        return "halt";
    }
    betrieb.map_or("offen", Betrieb::klasse)
}

pub(crate) const HERZ: &str = r#"<span id="herz" data-zustand="offen" role="img" aria-label="Wartet" title="Wartet"><svg viewBox="0 0 32 32" focusable="false" aria-hidden="true">
<circle class="herz-ring" cx="16" cy="16" r="14"/>
<path d="M16 25 7.6 17C2 11.1 10 5.1 16 11.7 22 5.1 30 11.1 24.4 17Z"/>
<path class="herz-pause" d="M14 14v5m4-5v5"/>
<path class="herz-warnung" d="M16 13v4m0 3v.1"/>
</svg></span>"#;

pub(crate) const STIL: &str = "
#herz{display:inline-flex;vertical-align:middle;align-items:center;margin-inline-end:6px;color:var(--offen)}
#herz svg{width:1.25em;height:1.25em;fill:currentColor}
#herz[data-zustand=gut]{color:var(--gut)}
#herz[data-zustand=halt]{color:var(--halt)}
#herz .herz-ring,#herz .herz-pause,#herz .herz-warnung{display:none;fill:none;stroke-width:2;stroke-linecap:round}
#herz[data-zustand=offen] .herz-pause,#herz[data-zustand=halt] .herz-warnung{display:inline;stroke:var(--flaeche)}
#herz[data-zustand=halt] .herz-ring{display:inline;stroke:currentColor}
.kopffiguren{display:flex;gap:clamp(4px,1vw,12px)}
.kopffigur{display:grid;justify-items:center;min-width:45px;color:var(--text);font-size:.8rem}
.kopffigur svg{width:36px;height:36px;overflow:visible;fill:currentColor}
.dachs-fell{fill:#8b8275}.maus-fell{fill:#a995ae}.otter-fell{fill:#9d6a47}.figur-hell{fill:var(--flaeche)}
@media(prefers-color-scheme:dark){.dachs-fell{fill:#aca395}.maus-fell{fill:#c2accb}.otter-fell{fill:#bd8b63}.figur-hell{fill:var(--text)}}
.kopfblase{position:absolute;z-index:3;top:100%;inset-inline-end:0;width:max-content;max-width:min(20rem,80vw);display:none;
  background:var(--flaeche);padding:6px 10px;border:1px solid var(--linie);border-radius:6px;overflow-wrap:anywhere}
.kopffigur:hover .kopfblase,.kopffigur:focus .kopfblase{display:block}
.kopffigur:focus-visible{outline:2px solid currentColor;outline-offset:2px;border-radius:3px}
.helfer-stern,.otter-pfote{display:none}
.kopffiguren[data-denkt=helfer] .helfer-stern{display:inline;fill:var(--offen)}
.kopffigur .figur-auge{transform-box:fill-box;transform-origin:center;transform:scaleY(.45)}
.kopffiguren:is([data-denkt=dachs],[data-denkt=beide]) .dachs,
.kopffiguren:is([data-denkt=maus],[data-denkt=beide]) .maus,
.kopffiguren[data-otter=dran] .kopf-otter{color:var(--gut)}
.kopffiguren:is([data-denkt=dachs],[data-denkt=beide]) .dachs svg,
.kopffiguren:is([data-denkt=maus],[data-denkt=beide]) .maus svg,
.kopffiguren[data-otter=dran] .kopf-otter svg{transform:translateY(-2px)}
.kopffiguren:is([data-denkt=dachs],[data-denkt=beide]) .dachs .figur-auge,
.kopffiguren:is([data-denkt=maus],[data-denkt=beide]) .maus .figur-auge,
.kopffiguren[data-otter=dran] .kopf-otter .figur-auge{transform:none}
.kopffiguren[data-otter=dran] .otter-pfote{display:inline;stroke:var(--text);stroke-width:1}
@media(prefers-reduced-motion:no-preference){
  .kopffigur svg,.kopffigur .figur-auge{transition:transform .18s ease-out}
}
@media(max-width:600px){.kopffigur svg{width:28px;height:28px}.kopffigur{min-width:38px}}
.handelskarten{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,18rem),1fr));gap:16px;align-items:start;margin-block:16px}
.handelskarte{--figur-akzent:#877968;min-width:0;padding:clamp(14px,2.2vw,22px);
  border:1px solid var(--linie);border-radius:14px;background:var(--flaeche);
  background:color-mix(in srgb,var(--figur-akzent) 8%,var(--flaeche))}
.handelskarte[data-berater=maus]{--figur-akzent:#a38aab}
.handelskarte header{display:flex;align-items:center;gap:12px}
.handelskarte h4{margin:0;font-size:1.125rem}
.handelskarte-figur{display:grid;place-items:center;width:46px;height:46px;border-radius:50%;
  background:color-mix(in srgb,var(--figur-akzent) 20%,var(--grund));color:var(--text)}
.handelskarte-figur svg{width:32px;height:32px;fill:currentColor}
.handelskarte-netto,.handelskarte-vergleich{display:grid;gap:3px;margin-top:14px}
.handelskarte-netto>span,.handelskarte-vergleich>span,.handelskarte small{color:var(--leise)}
.handelskarte-netto strong{font-size:clamp(1.45rem,3vw,2rem);line-height:1.2;font-variant-numeric:tabular-nums}
.handelskarte-vergleich{border-top:1px solid var(--linie);padding-top:12px;min-height:6.5em}
.handelskarte-skala{position:relative;height:8px;margin-block:6px;background:var(--grund);border-radius:8px}
.handelskarte-skala::after{content:'';position:absolute;left:50%;top:-3px;bottom:-3px;
  width:1px;background:var(--leise);transform:translateX(-50%)}
.handelskarte-skala span{position:absolute;height:100%;width:var(--laenge);border-radius:8px}
.handelskarte-skala [data-richtung=plus]{left:50%;background:var(--gut)}
.handelskarte-skala [data-richtung=minus]{right:50%;background:var(--halt)}
.handelskarte details{margin-top:12px}
[data-wert]{font-variant-numeric:tabular-nums}
[data-wert].steigt{--wertfarbe:var(--gut)}
[data-wert].faellt{--wertfarbe:var(--halt)}
[data-wert].steigt,[data-wert].faellt{animation:wert-hinweis 1.2s ease-out}
@keyframes wert-hinweis{from{background:color-mix(in srgb,var(--wertfarbe) 20%,transparent)}to{background:transparent}}
@media(prefers-reduced-motion:reduce){[data-wert].steigt,[data-wert].faellt{animation-timing-function:step-end}}
@keyframes herz-puls{0%,72%,100%{transform:scale(1)} 36%{transform:scale(1.13)}}
@media(prefers-reduced-motion:no-preference){
  #herz[data-zustand=gut] svg{animation:herz-puls 4s ease-in-out infinite}
  #herz[data-zustand=offen] svg{animation:herz-puls 6s ease-in-out infinite}
}
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_und_kurssperre_bestimmen_die_ampel() {
        for (zustand, erwartet) in [
            (None, "offen"),
            (Some(Betrieb::Bereit), "gut"),
            (Some(Betrieb::OhneKi(Vec::new())), "offen"),
            (Some(Betrieb::Pausiert(Vec::new())), "offen"),
            (Some(Betrieb::EingriffNoetig(Vec::new())), "halt"),
        ] {
            assert_eq!(ampel(zustand.as_ref(), false), erwartet);
            assert_eq!(ampel(zustand.as_ref(), true), "halt");
        }
    }
}

pub(crate) const DACHS_FIGUR: &str = r#"<svg viewBox="0 0 36 36" aria-hidden="true" focusable="false">
<path class="dachs-fell" d="M4 16 5 8 12 10Q18 5 24 10L31 8 32 16Q34 27 18 32 2 27 4 16"/>
<path class="figur-hell" d="M16 10 9 15 12 27 18 31 24 27 27 15 20 10 18 18Z"/>
<path d="M8 15 12 13 16 23 11 23Zm20 0-4-2-4 10 5 0ZM15 26h6l-3 3Z"/>
<circle cx="12" cy="18" r="1" class="figur-hell figur-auge"/><circle cx="24" cy="18" r="1" class="figur-hell figur-auge"/>
<path class="helfer-stern" d="m30 2 1.2 2.8 3 .2-2.3 2 .8 3-2.7-1.5L27.3 10l.8-3-2.3-2 3-.2Z"/>
</svg>"#;

pub(crate) const MAUS_FIGUR: &str = r#"<svg viewBox="0 0 36 36" aria-hidden="true" focusable="false">
<circle class="maus-fell" cx="8" cy="11" r="7"/><circle class="maus-fell" cx="28" cy="11" r="7"/>
<circle class="figur-hell" cx="8" cy="11" r="4"/><circle class="figur-hell" cx="28" cy="11" r="4"/>
<path class="maus-fell" d="M5 20Q5 11 18 11 31 11 31 20 31 26 18 33 5 26 5 20"/>
<circle cx="12" cy="19" r="1.7" class="figur-auge"/><circle cx="24" cy="19" r="1.7" class="figur-auge"/><ellipse cx="18" cy="28" rx="3" ry="2"/>
</svg>"#;

pub(crate) fn figuren() -> String {
    format!(
        r#"<span id="kopffiguren" class="kopffiguren" data-denkt="niemand" data-otter="ruht">
<span class="kopffigur dachs" tabindex="0" aria-describedby="kopf-dachs">{DACHS_FIGUR}<span>Dachs</span><span class="kopfblase" id="kopf-dachs">Heute noch nichts gesagt.</span></span>
<span class="kopffigur maus" tabindex="0" aria-describedby="kopf-maus">{MAUS_FIGUR}<span>Maus</span><span class="kopfblase" id="kopf-maus">Heute noch nichts gesagt.</span></span>
<span class="kopffigur kopf-otter" tabindex="0" aria-describedby="kopf-otter"><svg viewBox="0 0 36 36" aria-hidden="true" focusable="false">
<circle class="otter-fell" cx="7" cy="11" r="4.5"/><circle class="otter-fell" cx="27" cy="11" r="4.5"/>
<ellipse class="otter-fell" cx="17" cy="20" rx="13" ry="14"/>
<ellipse class="figur-hell" cx="17" cy="26" rx="10" ry="6"/>
<circle cx="11" cy="18" r="1.6" class="figur-auge"/><circle cx="23" cy="18" r="1.6" class="figur-auge"/><ellipse cx="17" cy="24" rx="3" ry="2"/>
<path class="otter-pfote otter-fell" d="M24 31q0-4 4-8l1-10q1-3 4-2l1 11q0 9-10 9"/>
</svg><span>Otter</span><span class="kopfblase" id="kopf-otter">Keine Entscheidung offen.</span></span></span>"#
    )
}

pub(crate) fn aussagen(
    z: &crate::werkbank::Zustand,
    heute: Option<depot_kern::Datum>,
    update_freigabe_offen: bool,
) -> String {
    use depot_lage::html::maskiere;
    let beitraege = z
        .lage
        .as_ref()
        .map_or(&[][..], |lage| lage.bild.gespraech.as_slice());
    let art = z.laufend.as_ref().map(|a| a.art);
    let wartet = |wer| {
        art.filter(|_| z.berater_warten.contains(&wer))
            .or_else(|| (z.neuro_wartet() == Some(wer)).then_some(crate::werkbank::Art::NeuroFrage))
    };
    let dachs_wartet = wartet(BeraterId::Dachs);
    let maus_wartet = wartet(BeraterId::Maus);
    let gemini = z
        .aktive_auftraege()
        .find(|a| a.art == crate::werkbank::Art::Gemini)
        .map(|a| a.art);
    let denkt = denkt(dachs_wartet.is_some(), maus_wartet.is_some(), gemini);
    let papier_freigabe_offen = z.lage.as_ref().is_some_and(|l| l.bild.papierfreigabe_offen);
    let (otter, satz) = match (update_freigabe_offen, papier_freigabe_offen) {
        (true, true) => (
            "dran",
            "Update- und Papierfreigabe warten auf deine Entscheidung.",
        ),
        (true, false) => ("dran", "Update-Freigabe wartet auf deine Entscheidung."),
        (false, true) => ("dran", "Papierfreigabe wartet auf deine Entscheidung."),
        (false, false) => ("ruht", "Keine Entscheidung offen."),
    };
    let (dachs, dachs_titel) = beitrag(beitraege, heute, "Dachs", dachs_wartet);
    let (maus, maus_titel) = beitrag(beitraege, heute, "Maus", maus_wartet);
    format!(
        "<span hidden data-figuren data-denkt=\"{denkt}\" data-otter=\"{otter}\" \
         data-dachs=\"{}\" data-dachs-titel=\"{}\" data-maus=\"{}\" data-maus-titel=\"{}\" data-otter-satz=\"{}\"></span>",
        maskiere(&dachs),
        maskiere(&dachs_titel),
        maskiere(&maus),
        maskiere(&maus_titel),
        maskiere(satz)
    )
}

fn denkt(dachs: bool, maus: bool, art: Option<crate::werkbank::Art>) -> &'static str {
    match (dachs, maus) {
        (true, true) => "beide",
        (true, false) => "dachs",
        (false, true) => "maus",
        (false, false) if art == Some(crate::werkbank::Art::Gemini) => "helfer",
        (false, false) => "niemand",
    }
}

fn beitrag(
    beitraege: &[depot_lage::Beitrag],
    heute: Option<depot_kern::Datum>,
    wer: &str,
    wartet: Option<crate::werkbank::Art>,
) -> (String, String) {
    if let Some(art) = wartet {
        return (format!("Anfrage läuft · {}", art.name()), wer.into());
    }
    beitraege
        .iter()
        .find(|b| b.wer == wer && Some(b.datum) == heute)
        .map_or_else(
            || ("Heute noch nichts gesagt.".into(), wer.into()),
            |b| {
                (
                    erster_satz(&b.text),
                    format!("{wer}, {}", depot_lage::datum_text(b.datum)),
                )
            },
        )
}

fn erster_satz(text: &str) -> String {
    let text = text.trim();
    let ende = text.char_indices().find_map(|(i, c)| {
        let danach = i.saturating_add(c.len_utf8());
        (matches!(c, '.' | '!' | '?') && text.get(danach..)?.starts_with(char::is_whitespace))
            .then_some(danach)
    });
    let satz = ende.and_then(|i| text.get(..i)).unwrap_or(text);
    if satz.chars().count() > 140 {
        satz.chars().take(139).chain(std::iter::once('…')).collect()
    } else {
        satz.to_owned()
    }
}

#[cfg(test)]
mod figuren_tests {
    use super::*;
    use crate::werkbank::Art;

    #[test]
    fn figuren_unterscheiden_die_offenen_berateranfragen() {
        assert_eq!(denkt(false, false, Some(Art::Papier)), "niemand");
        assert_eq!(denkt(true, false, Some(Art::Papier)), "dachs");
        assert_eq!(denkt(true, true, Some(Art::Papier)), "beide");
        assert_eq!(denkt(false, true, Some(Art::Papier)), "maus");
        assert_eq!(denkt(false, false, Some(Art::Gemini)), "helfer");
        let figuren = figuren();
        for verboten in ["href", "url(", "http", "data:"] {
            assert!(!figuren.contains(verboten));
        }
        assert!(figuren.len() < 6144);
    }

    #[test]
    fn sprechblase_zeigt_den_ersten_satz_bis_140_zeichen() {
        assert_eq!(erster_satz("Kauf SAP. Grund: Zahlen."), "Kauf SAP.");
        let satz = erster_satz(&format!("{}🙂weiter", "ä".repeat(138)));
        assert_eq!(satz.chars().count(), 140);
        assert!(satz.ends_with("🙂…"));
        assert_eq!(
            beitrag(&[], None, "Maus", None).0,
            "Heute noch nichts gesagt."
        );
        assert_eq!(
            beitrag(&[], None, "Maus", Some(Art::Papier)).0,
            "Anfrage läuft · Papiertag"
        );
    }
}
