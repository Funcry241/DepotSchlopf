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
