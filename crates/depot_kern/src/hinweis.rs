//! Hinweiszeilen: Ein Lauf (etwa `run.cmd kurse`) sagt in einer eigenen Zeile seiner Ausgabe in
//! Alltagssprache, was gerade los ist, ob DepotSchlopf selbst weitermacht und ob der Mensch
//! etwas tun muss. Die Oberflaeche des Dienstes zeigt die juengste dieser Zeilen gross ueber dem
//! Protokoll; in der Konsole steht sie wie jede andere Zeile.
//!
//! Die Zeile entscheidet nichts. Sie ist nur Anzeige; was gilt, steht im Journal.

/// Anfang jeder Hinweiszeile.
pub const ANFANG: &str = "[Hinweis] ";

/// Die Zeile zu einem Hinweis: eine Zeile, ohne Zeilenumbruch.
pub fn zeile(text: &str) -> String {
    let flach: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    format!("{ANFANG}{}", flach.trim())
}

/// Der Hinweis einer Zeile, wenn sie eine Hinweiszeile ist.
pub fn lies(zeile: &str) -> Option<&str> {
    zeile
        .strip_prefix(ANFANG)
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

/// Uhrzeit in Frankfurt als `17:48 Uhr`; `None` ausserhalb des Kalenders.
pub fn uhr_text(z: crate::Zeitpunkt) -> Option<String> {
    z.in_frankfurt()
        .map(|(_, h, m, _, _)| format!("{h:02}:{m:02} Uhr"))
}

/// Festes Signal aus dem Versandpfad. Überträgt ausschließlich Berater und Versandstatus.
pub fn berater_anfrage(wer: crate::BeraterId, wartet: bool) -> &'static str {
    match (wer, wartet) {
        (crate::BeraterId::Dachs, true) => "[Beraterstatus] Dachs wartet",
        (crate::BeraterId::Dachs, false) => "[Beraterstatus] Dachs fertig",
        (crate::BeraterId::Maus, true) => "[Beraterstatus] Maus wartet",
        (crate::BeraterId::Maus, false) => "[Beraterstatus] Maus fertig",
    }
}

/// Liest ein vollständiges Signal aus der Ausgabe des aktuellen Prozesses.
pub fn lies_berater_anfrage(zeile: &str) -> Option<(crate::BeraterId, bool)> {
    [crate::BeraterId::Dachs, crate::BeraterId::Maus]
        .into_iter()
        .flat_map(|wer| [(wer, true), (wer, false)])
        .find(|(wer, wartet)| zeile == berater_anfrage(*wer, *wartet))
}
