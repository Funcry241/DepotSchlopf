//! Der Otter auf Live folgt der Betriebsanzeige.

use crate::werkbank::Zustand;
use depot_lage::Betrieb;

pub(crate) fn pose(z: &Zustand, kurssperre: bool) -> &'static str {
    if kurssperre || matches!(z.gesamtstatus(), Some(Betrieb::EingriffNoetig(_))) {
        "pfote"
    } else if z.laufend.is_some() || z.neustart {
        "wach"
    } else {
        "ruht"
    }
}

pub(crate) const STIL: &str = "
.live-kopf{display:grid;grid-template-columns:minmax(0,1fr) auto;gap:10px;align-items:start;position:sticky;top:0;z-index:1;background:var(--grund)}
.live-kopf #betriebsleiste{position:static;min-width:0}
.otter{--fell:#805742;--licht:#ad7955;--schnauze:#f7d6a4;--kontur:#392722;
width:104px;align-self:start;margin-top:4px;border:1px solid var(--linie);border-radius:18px;
background:var(--flaeche);color:var(--leise)}
@media(prefers-color-scheme:dark){.otter{--fell:#a97451;--licht:#c89164;--schnauze:#ffe0b4;--kontur:#33231d}}
.otter svg{display:block;width:100%;height:auto}
.otter .fell{fill:var(--fell);stroke:var(--kontur);stroke-width:1.8}
.otter .licht{fill:var(--licht)}.otter .schnauze{fill:var(--schnauze)}.otter .kontur{fill:var(--kontur)}
.otter .strich{fill:none;stroke:var(--kontur);stroke-width:1.7;stroke-linecap:round}
.otter .ruhe{color:var(--gut)}.otter .arbeit{color:#005c97}.otter .hilfe{color:var(--halt)}
@media(prefers-color-scheme:dark){.otter .arbeit{color:#74c7ff}}
.otter .pose{opacity:0;transform:translateY(3px)}
.otter .ruhe{opacity:1;transform:none}
.live-kopf:has([data-otter=wach],[data-otter=pfote]) .otter .ruhe{opacity:0;transform:translateY(3px)}
.live-kopf:has([data-otter=wach]) .otter .arbeit,
.live-kopf:has([data-otter=pfote]) .otter .hilfe{opacity:1;transform:none}
@media(prefers-reduced-motion:no-preference){.otter .pose{transition:opacity 170ms ease,transform 210ms ease-out}}
@media(max-width:600px){.live-kopf{gap:6px;position:static}.otter{width:68px;border-radius:12px}}
";

pub(crate) const BILD: &str = r#"
<div class="otter" aria-hidden="true"><svg viewBox="0 0 104 82" focusable="false">
<ellipse cx="52" cy="69" rx="37" ry="5" fill="var(--linie)"/>
<g class="pose ruhe">
  <path class="fell" d="M53 63c-12 9-33 6-38-1-3-4-1-7 2-7 6 6 13 6 23 2z"/>
  <ellipse class="fell" cx="52" cy="54" rx="24" ry="13"/>
  <ellipse class="licht" cx="52" cy="51" rx="18" ry="8"/>
  <circle class="fell" cx="61" cy="39" r="5"/><circle class="fell" cx="82" cy="40" r="5"/>
  <ellipse class="fell" cx="72" cy="50" rx="16" ry="13"/>
  <ellipse class="schnauze" cx="74" cy="54" rx="12" ry="7"/>
  <path class="strich" d="M62 48q3 3 6 0m10 0q3 3 6 0m-16 9q5 4 9 0"/>
  <ellipse class="kontur" cx="74" cy="52" rx="3.8" ry="2.7"/>
  <ellipse class="fell" cx="60" cy="62" rx="9" ry="4"/>
  <path d="M63 28h7l-7 7h7m5-19h9l-9 9h9" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" stroke-linecap="round"/>
</g>
<g class="pose arbeit">
  <path class="fell" d="M50 60c-10 8-25 11-30 5-2-3 0-6 3-6 6 3 12 0 20-6z"/>
  <ellipse class="fell" cx="48" cy="52" rx="17" ry="17"/>
  <ellipse class="schnauze" cx="51" cy="54" rx="11" ry="11"/>
  <circle class="fell" cx="34" cy="27" r="5"/><circle class="fell" cx="60" cy="27" r="5"/>
  <ellipse class="fell" cx="47" cy="36" rx="18" ry="14"/>
  <ellipse class="schnauze" cx="51" cy="41" rx="12" ry="7"/>
  <ellipse class="kontur" cx="42" cy="35" rx="2" ry="2.7"/><ellipse class="kontur" cx="56" cy="35" rx="2" ry="2.7"/>
  <ellipse class="kontur" cx="51" cy="39" rx="4" ry="2.8"/><path class="strich" d="M46 43q5 5 10 0"/>
  <path class="fell" d="M42 54q5 4 13 3l2 5q-10 4-17-1"/>
  <path d="M65 47h23l-5 17H59z" fill="var(--flaeche)" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round"/>
  <path d="M53 65h31m-12-10h5" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"/>
</g>
<g class="pose hilfe">
  <path class="fell" d="M49 59c-9 10-24 12-31 6-2-3 0-6 3-6 7 4 13 0 21-6z"/>
  <ellipse class="fell" cx="49" cy="52" rx="17" ry="17"/>
  <ellipse class="schnauze" cx="49" cy="55" rx="10" ry="12"/>
  <path class="fell" d="M59 50q7-4 9-15l2-10q2-4 5-1l1 5 4-4q4-1 4 3l-2 6q-2 5-7 7-2 13-11 17z"/>
  <circle class="fell" cx="35" cy="28" r="5"/><circle class="fell" cx="61" cy="28" r="5"/>
  <ellipse class="fell" cx="48" cy="37" rx="18" ry="14"/>
  <ellipse class="schnauze" cx="48" cy="42" rx="12" ry="7"/>
  <ellipse class="kontur" cx="41" cy="36" rx="2" ry="2.6"/><ellipse class="kontur" cx="55" cy="36" rx="2" ry="2.6"/>
  <ellipse class="kontur" cx="48" cy="40" rx="4" ry="2.8"/><path class="strich" d="M43 44q5 4 10 0"/>
  <path d="M86 14l3-5m2 11 6-2" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
</g>
</svg></div>
"#;
