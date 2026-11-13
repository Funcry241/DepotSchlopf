//! Namensformat abgeschlossener DepotSchlopf-Sicherungen.

/// Erkennt nur das Namensformat fertiger Sicherungsordner.
pub fn sicherungsname(name: &str) -> bool {
    let Some((uhr, commit)) = name.strip_prefix("utc-").and_then(|s| s.split_once('_')) else {
        return false;
    };
    let Some((sekunden, nanos)) = uhr.split_once('-') else {
        return false;
    };
    sekunden.len() == 20
        && nanos.len() == 9
        && sekunden
            .bytes()
            .chain(nanos.bytes())
            .all(|b| b.is_ascii_digit())
        && (commit.len() == 40 || commit.len() == 64)
        && commit
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}
