//! Betriebsanzeige in der Navigation.

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
