//! Windows-Hinweis, sobald ein neuer Grund für „Eingriff nötig“ entsteht (#122).
//!
//! Die Architektur verspricht: Wolfgang bekommt sofort eine Nachricht (Notbremse,
//! Kontowächter). Der Dienst zeigt dafür eine Sprechblase im Infobereich, wenn im Gesamtstatus
//! ein **neuer** Grund für `Betrieb::EingriffNoetig` auftaucht, und einmal, wenn der jüngste
//! Handelstag ausgefallen ist. Der Hinweis zeigt nur an, er löst nichts aus.
//!
//! **Gedächtnis:** `daten/hinweise.txt` hält je gemeldetem Grund, der noch besteht, eine Zeile
//! `<sha256-hex des Grundtexts>;<UTC-Zeitpunkt>`. So meldet ein Neustart denselben alten Halt
//! nicht noch einmal. Verschwindet ein Grund, fällt seine Zeile weg; kehrt er zurück, kommt
//! wieder ein Hinweis. Eine unlesbare Datei gilt als leer (lieber ein Hinweis zu viel).
//!
//! **Bündeln:** Neue Gründe sammeln sich `SAMMEL_SEKUNDEN` lang, und zwischen zwei Hinweisen
//! liegen mindestens `ABSTAND_SEKUNDEN`. Was in dieser Zeit dazukommt, steht im selben Hinweis.
//!
//! **Geheimnisfrei:** In den Hinweis kommen nur die Gründe, wie die Oberfläche sie zeigt. Das
//! Skript bekommt Titel und Text nur über Umgebungsvariablen, nie in den Befehl formatiert.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use depot_kern::Zeitpunkt;
use depot_kern::abdruck::sha256_hex;

/// So viele gemeldete Gründe merkt sich `daten/hinweise.txt` höchstens.
pub const MAX_BEKANNT: usize = 200;
/// Höchstlänge des ersten Grunds im Hinweis, in Zeichen.
pub const MAX_TEXT: usize = 200;
/// So lange sammeln sich neue Gründe, bevor ein Hinweis kommt.
pub const SAMMEL_SEKUNDEN: i64 = 10;
/// Höchstens ein Hinweis in dieser Zeit.
pub const ABSTAND_SEKUNDEN: i64 = 60;
/// Titel jedes Hinweises.
pub const TITEL: &str = "DepotSchlopf: Eingriff nötig";

/// Die neuen Gründe (in der Reihenfolge des Status) und die Menge der Abdrücke aller
/// aktuellen Gründe. Ein Grund, der verschwunden ist, fehlt in der Menge: Kehrt er zurück,
/// meldet er sich wieder.
pub fn neue_gruende(
    bekannt: &BTreeSet<String>,
    gruende: &[String],
) -> (Vec<String>, BTreeSet<String>) {
    let mut neu = Vec::new();
    let mut menge = BTreeSet::new();
    for g in gruende {
        let h = sha256_hex(g.as_bytes());
        if !bekannt.contains(&h) && !menge.contains(&h) {
            neu.push(g.clone());
        }
        menge.insert(h);
    }
    (neu, menge)
}

/// Der Text eines Hinweises: erster Grund, gekürzt, mit Zahl der weiteren.
pub fn text(neu: &[String]) -> Option<String> {
    let erster = neu.first()?;
    let mut t: String = erster.chars().take(MAX_TEXT).collect();
    let weitere = neu.len().saturating_sub(1);
    if weitere > 0 {
        let _ = write!(t, " (und {weitere} weitere)");
    }
    t.push_str(" Details auf http://127.0.0.1:47410/");
    Some(t)
}

/// `daten/hinweise.txt`: je Zeile `<sha256-hex>;<utc>`. Eine unlesbare Zeile macht die ganze
/// Datei leer (`None`).
pub fn lies(inhalt: &str) -> Option<BTreeMap<String, String>> {
    let mut m = BTreeMap::new();
    for z in inhalt.lines() {
        let (h, zeit) = z.split_once(';')?;
        let hex = h.len() == 64 && h.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        if !hex || zeit.is_empty() || zeit.contains(';') {
            return None;
        }
        m.insert(h.to_string(), zeit.to_string());
    }
    Some(m)
}

fn als_text(bekannt: &BTreeMap<String, String>) -> String {
    let mut s = String::new();
    for (h, zeit) in bekannt {
        s.push_str(h);
        s.push(';');
        s.push_str(zeit);
        s.push('\n');
    }
    s
}

/// Was der Dienst über gemeldete und wartende Gründe weiß.
#[derive(Debug, Default)]
pub struct Gedaechtnis {
    /// Abdruck → Zeitpunkt der Meldung; nur Gründe, die gemeldet sind und noch bestehen.
    bekannt: BTreeMap<String, String>,
    /// Neue Gründe, noch nicht gemeldet, in der Reihenfolge des Status.
    offen: Vec<String>,
    offen_seit: Option<Zeitpunkt>,
    letzter: Option<Zeitpunkt>,
    /// Die Datei war unlesbar oder ließ sich nicht schreiben: beim nächsten Mal neu schreiben.
    neu_schreiben: bool,
}

impl Gedaechtnis {
    /// Liest `daten/hinweise.txt`. Fehlt sie, ist nichts bekannt; ist sie unlesbar, auch, und
    /// sie wird beim nächsten Prüfen gültig neu geschrieben.
    pub fn laden(pfad: &Path) -> Gedaechtnis {
        let (bekannt, neu_schreiben) = match crate::werkbank::lies_falls_da(pfad) {
            Ok(None) => (BTreeMap::new(), false),
            Ok(Some(t)) => lies(&t).map_or_else(|| (BTreeMap::new(), true), |m| (m, false)),
            Err(_) => (BTreeMap::new(), true),
        };
        Gedaechtnis {
            bekannt,
            neu_schreiben,
            ..Gedaechtnis::default()
        }
    }

    /// Gleicht die aktuellen Gründe ab, pflegt die Datei und gibt den Text zurück, wenn jetzt
    /// ein Hinweis zu zeigen ist. Ist der Schalter aus, gelten neue Gründe sofort als gemeldet
    /// (kein Schwall alter Hinweise beim Einschalten), und es kommt kein Text.
    pub fn pruefe(
        &mut self,
        gruende: &[String],
        an: bool,
        jetzt: Zeitpunkt,
        pfad: &Path,
    ) -> Option<String> {
        let vorher: BTreeSet<String> = self.bekannt.keys().cloned().collect();
        let (neu, menge) = neue_gruende(&vorher, gruende);
        self.bekannt.retain(|h, _| menge.contains(h));
        if neu.is_empty() {
            self.offen_seit = None;
        } else if self.offen.is_empty() {
            self.offen_seit = Some(jetzt);
        }
        self.offen = neu;
        let mut zeigen = None;
        let gesammelt = self
            .offen_seit
            .and_then(|s| jetzt.sekunden_seit(s))
            .is_some_and(|s| s >= SAMMEL_SEKUNDEN);
        let abstand = self
            .letzter
            .is_none_or(|l| jetzt.sekunden_seit(l).is_none_or(|s| s >= ABSTAND_SEKUNDEN));
        if !self.offen.is_empty() && (!an || (gesammelt && abstand)) {
            if an {
                zeigen = text(&self.offen);
                self.letzter = Some(jetzt);
            }
            let zeit = jetzt.to_string();
            for g in self.offen.drain(..) {
                self.bekannt.insert(sha256_hex(g.as_bytes()), zeit.clone());
            }
            self.offen_seit = None;
            self.begrenze();
        }
        let geaendert = self.bekannt.keys().ne(vorher.iter());
        if geaendert || self.neu_schreiben {
            match crate::werkbank::schreibe_ganz(pfad, &als_text(&self.bekannt)) {
                Ok(()) => self.neu_schreiben = false,
                Err(e) => {
                    eprintln!("[Dienst] Hinweisgedächtnis nicht schreibbar: {e}");
                    self.neu_schreiben = true;
                }
            }
        }
        zeigen
    }

    /// Höchstens `MAX_BEKANNT` Zeilen; die ältesten Meldungen fallen weg.
    fn begrenze(&mut self) {
        while self.bekannt.len() > MAX_BEKANNT {
            let Some(aeltester) = self
                .bekannt
                .iter()
                .min_by(|a, b| a.1.cmp(b.1))
                .map(|(h, _)| h.clone())
            else {
                return;
            };
            self.bekannt.remove(&aeltester);
        }
    }
}

/// PowerShell-Skript: Titel und Text nur über die Umgebung, nie in den Befehl formatiert.
pub const SKRIPT: &str = "
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$n = New-Object System.Windows.Forms.NotifyIcon
$n.Icon = [System.Drawing.SystemIcons]::Warning
$n.Text = 'DepotSchlopf'
$n.Visible = $true
$n.ShowBalloonTip(15000, $env:DEPOTSCHLOPF_HINWEIS_TITEL, $env:DEPOTSCHLOPF_HINWEIS_TEXT, 'Warning')
Start-Sleep -Seconds 16
$n.Dispose()
";

/// Startet den Hinweis und wartet nicht darauf (ein eigener Faden räumt das Kind ab). Unter
/// anderen Systemen: nichts.
#[cfg(windows)]
pub fn zeige(titel: &str, text: &str) {
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let Some(systemroot) = std::env::var_os("SystemRoot") else {
        return;
    };
    let Ok(mut kind) = Command::new(
        PathBuf::from(systemroot)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe"),
    )
    .creation_flags(CREATE_NO_WINDOW)
    .args(["-NoProfile", "-NonInteractive", "-STA", "-Command", SKRIPT])
    .env("DEPOTSCHLOPF_HINWEIS_TITEL", titel)
    .env("DEPOTSCHLOPF_HINWEIS_TEXT", text)
    .spawn() else {
        return;
    };
    std::thread::spawn(move || {
        let _ = kind.wait();
    });
}

#[cfg(not(windows))]
pub fn zeige(_titel: &str, _text: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn erster_grund_meldet() {
        let (neu, m) = neue_gruende(&BTreeSet::new(), &s(&["A", "B"]));
        assert_eq!(neu, s(&["A", "B"]));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn bekannter_grund_meldet_nicht() {
        let bekannt = BTreeSet::from([sha256_hex(b"A")]);
        let (neu, _) = neue_gruende(&bekannt, &s(&["A", "B"]));
        assert_eq!(neu, s(&["B"]));
    }

    #[test]
    fn weg_und_wieder_da() {
        let bekannt = BTreeSet::from([sha256_hex(b"A")]);
        let (neu, m) = neue_gruende(&bekannt, &[]);
        assert!(neu.is_empty() && m.is_empty());
        let (neu, _) = neue_gruende(&m, &s(&["A"]));
        assert_eq!(neu, s(&["A"]));
    }

    #[test]
    fn text_buendelt_und_kuerzt() {
        assert_eq!(
            text(&s(&["Notbremse", "x", "y"])).as_deref(),
            Some("Notbremse (und 2 weitere) Details auf http://127.0.0.1:47410/")
        );
        assert_eq!(text(&[]), None);
        let lang = "ä".repeat(300);
        let t = text(&[lang]).unwrap_or_default();
        assert_eq!(t.chars().filter(|c| *c == 'ä').count(), MAX_TEXT);
    }

    #[test]
    fn kaputte_datei_gilt_leer() {
        assert_eq!(lies("xyz"), None);
        assert_eq!(lies(&format!("{};\n", sha256_hex(b"A"))), None);
        let h = sha256_hex(b"A");
        assert_eq!(
            lies(&format!("{h};2026-11-10T12:00:00Z\n")).map(|m| m.len()),
            Some(1)
        );
        assert_eq!(lies(""), Some(BTreeMap::new()));
    }

    #[test]
    fn text_nur_ueber_umgebung() {
        assert!(SKRIPT.contains("$env:DEPOTSCHLOPF_HINWEIS_TEXT"));
        assert!(SKRIPT.contains("$env:DEPOTSCHLOPF_HINWEIS_TITEL"));
        assert!(!SKRIPT.contains('{'));
        // Die Quelle setzt den Text nur über `.env(…)`, nie mit `format!` in das Skript.
        let quelle = include_str!("hinweis.rs");
        let zeige = quelle
            .split("pub fn zeige(titel: &str, text: &str) {")
            .nth(1)
            .and_then(|r| r.split("#[cfg(not(windows))]").next())
            .unwrap_or_default();
        assert!(zeige.contains(".env(\"DEPOTSCHLOPF_HINWEIS_TEXT\", text)"));
        assert!(!zeige.contains("format!"));
    }
}
