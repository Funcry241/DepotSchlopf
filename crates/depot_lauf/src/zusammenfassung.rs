//! Kurzbild des Laufs für die CI-Zusammenfassung und das Protokollende (#313 E4, P38).
//!
//! Quelle ist die Konsole des Gates, wie sie in den Export geht. Für jedes Testprogramm
//! entsteht ein [`Testergebnis`] (Schnittstelle mit C1, 190#6945502116). Enthält das Kurzbild
//! ein Geheimnismuster, wird es wie der Export ganz zurückgehalten (R28): Es gibt nur den Hinweis.

use std::fmt::Write as _;

/// Höchstens so viele Zeilen gehen in die Zusammenfassung.
pub const HOECHSTENS: usize = 60;
/// Die langsamsten Testprogramme, die die Tabelle zeigt.
const LANGSAMSTE: usize = 10;
/// Länge der ersten Meldungszeile eines Fehlers.
const MELDUNG: usize = 200;

/// Ergebnis eines Testprogramms oder der Doc-Tests eines Crates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Testergebnis {
    pub binary: String,
    pub dauer_ms: u64,
    pub bestanden: u64,
    pub fehlgeschlagen: u64,
    pub ignoriert: u64,
    /// Exit-Code ohne `test result`-Zeile (Absturz).
    pub abbruch: Option<i32>,
    /// (Testname, erste Zeile der Meldung).
    pub fehler: Vec<(String, String)>,
}

/// Eine Stufe des Gates mit ihrer Dauer (`[Kodex] cargo … · Dauer: N ms`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stufe {
    pub name: String,
    pub dauer_ms: u64,
}

pub fn stufen(konsole: &str) -> Vec<Stufe> {
    konsole
        .lines()
        .filter_map(|z| {
            let rest = z.trim().strip_prefix("[Kodex] cargo ")?;
            let (name, dauer) = rest.split_once(" · Dauer: ")?;
            Some(Stufe {
                name: name.to_owned(),
                dauer_ms: dauer.strip_suffix(" ms")?.trim().parse().ok()?,
            })
        })
        .collect()
}

/// `165.51` (Sekunden) als Millisekunden, ohne Gleitkomma.
fn sekunden_ms(text: &str) -> Option<u64> {
    let (ganz, bruch) = text.split_once('.').unwrap_or((text, ""));
    let ganz: u64 = ganz.parse().ok()?;
    let mut ms: u64 = 0;
    for (stelle, ziffer) in bruch.chars().take(3).enumerate() {
        let wert = u64::from(ziffer.to_digit(10)?);
        let faktor = match stelle {
            0 => 100,
            1 => 10,
            _ => 1,
        };
        ms = ms.saturating_add(wert.saturating_mul(faktor));
    }
    Some(ganz.saturating_mul(1_000).saturating_add(ms))
}

/// Zahlen einer `test result:`-Zeile: (bestanden, fehlgeschlagen, ignoriert, Dauer).
fn ergebniszeile(zeile: &str) -> Option<(u64, u64, u64, u64)> {
    let rest = zeile.trim().strip_prefix("test result: ")?;
    let mut zahlen = (0, 0, 0, 0);
    let woerter: Vec<&str> = rest
        .split(|c: char| c.is_whitespace() || c == ';' || c == '.')
        .filter(|w| !w.is_empty())
        .collect();
    for paar in woerter.windows(2) {
        if let [zahl, wort] = paar
            && let Ok(n) = zahl.parse::<u64>()
        {
            match *wort {
                "passed" => zahlen.0 = n,
                "failed" => zahlen.1 = n,
                "ignored" => zahlen.2 = n,
                _ => {}
            }
        }
    }
    let dauer = rest
        .split_once("finished in ")
        .and_then(|(_, d)| d.trim().strip_suffix('s'))
        .and_then(sekunden_ms)
        .unwrap_or(0);
    Some((zahlen.0, zahlen.1, zahlen.2, dauer))
}

/// Name eines Testprogramms aus der `Running`- oder `Doc-tests`-Zeile von cargo.
fn programmname(zeile: &str) -> Option<String> {
    let z = zeile.trim();
    if let Some(krate) = z.strip_prefix("Doc-tests ") {
        return Some(format!("{} (doc)", krate.trim()));
    }
    let rest = z.strip_prefix("Running ")?;
    let rest = rest.strip_prefix("unittests ").unwrap_or(rest);
    let (quelle, pfad) = rest.split_once(" (")?;
    let datei = pfad
        .trim_end_matches(')')
        .rsplit(['/', '\\'])
        .next()?
        .trim_end_matches(".exe");
    // Der Hash nach dem letzten Bindestrich unterscheidet gleichnamige Testdateien.
    let (stamm, hash) = datei.rsplit_once('-').unwrap_or((datei, ""));
    let kurz: String = hash.chars().take(8).collect();
    Some(format!("{stamm} {} {kurz}", quelle.replace('\\', "/")))
}

/// Erste aussagekräftige Zeile der Meldung je fehlgeschlagenem Test.
fn meldungen(konsole: &str) -> Vec<(String, String)> {
    let mut aus = Vec::new();
    let mut offen: Option<String> = None;
    for zeile in konsole.lines() {
        if let Some(name) = zeile
            .strip_prefix("---- ")
            .and_then(|r| r.strip_suffix(" stdout ----"))
        {
            offen = Some(name.to_owned());
            continue;
        }
        let Some(name) = offen.as_ref() else {
            continue;
        };
        let t = zeile.trim();
        if t.is_empty() || t.starts_with("thread '") || t.starts_with("note: ") {
            continue;
        }
        aus.push((name.clone(), t.chars().take(MELDUNG).collect()));
        offen = None;
    }
    aus
}

/// Ergebnisse aus der seriellen Ausgabe von `cargo test`: Die Namen stehen auf stderr, die
/// Ergebnisse auf stdout, beide in derselben Reihenfolge. Passen die Anzahlen nicht, bleiben die
/// Programme nummeriert, statt falsch benannt.
fn seriell(konsole: &str) -> Vec<Testergebnis> {
    let namen: Vec<String> = konsole.lines().filter_map(programmname).collect();
    let mut ergebnisse: Vec<Testergebnis> = konsole
        .lines()
        .filter_map(ergebniszeile)
        .map(
            |(bestanden, fehlgeschlagen, ignoriert, dauer_ms)| Testergebnis {
                dauer_ms,
                bestanden,
                fehlgeschlagen,
                ignoriert,
                ..Testergebnis::default()
            },
        )
        .collect();
    let benannt = namen.len() == ergebnisse.len();
    for (i, e) in ergebnisse.iter_mut().enumerate() {
        e.binary = if benannt {
            namen.get(i).cloned().unwrap_or_default()
        } else {
            format!("Testprogramm {}", i.saturating_add(1))
        };
    }
    ergebnisse
}

/// Blöcke im Format von C1 (190#6945508779):
/// `[Kodex] Testprogramm <binary> · Beginn` … `[Kodex] Testprogramm <binary> · Exit <code> · <ms> ms`.
fn bloecke(konsole: &str) -> Vec<Testergebnis> {
    const KOPF: &str = "[Kodex] Testprogramm ";
    let mut aus = Vec::new();
    let mut inhalt = String::new();
    for zeile in konsole.lines() {
        let Some(rest) = zeile.trim().strip_prefix(KOPF) else {
            inhalt.push_str(zeile);
            inhalt.push('\n');
            continue;
        };
        if rest.ends_with(" · Beginn") {
            inhalt.clear();
            continue;
        }
        let Some((binary, schluss)) = rest.split_once(" · Exit ") else {
            continue;
        };
        let (code, dauer) = schluss.split_once(" · ").unwrap_or((schluss, ""));
        let code: i32 = code.trim().parse().unwrap_or(-1);
        let zeile_ergebnis = inhalt.lines().filter_map(ergebniszeile).fold(
            None,
            |summe: Option<(u64, u64, u64)>, (b, f, i, _)| {
                let (sb, sf, si) = summe.unwrap_or((0, 0, 0));
                Some((
                    sb.saturating_add(b),
                    sf.saturating_add(f),
                    si.saturating_add(i),
                ))
            },
        );
        let (bestanden, fehlgeschlagen, ignoriert) = zeile_ergebnis.unwrap_or((0, 0, 0));
        aus.push(Testergebnis {
            binary: binary.to_owned(),
            dauer_ms: dauer
                .trim()
                .strip_suffix(" ms")
                .and_then(|d| d.parse().ok())
                .unwrap_or(0),
            bestanden,
            fehlgeschlagen,
            ignoriert,
            abbruch: (zeile_ergebnis.is_none() && code != 0).then_some(code),
            fehler: meldungen(&inhalt),
        });
        inhalt.clear();
    }
    aus.sort_by(|a, b| a.binary.cmp(&b.binary));
    aus
}

/// Alle Testergebnisse eines Laufs, aus C1-Blöcken oder aus der seriellen Ausgabe.
pub fn testergebnisse(konsole: &str) -> Vec<Testergebnis> {
    let aus_bloecken = bloecke(konsole);
    if !aus_bloecken.is_empty() {
        return aus_bloecken;
    }
    let mut ergebnisse = seriell(konsole);
    // Seriell lassen sich Meldungen nicht sicher einem Programm zuordnen: Sie gehen an das
    // erste Programm mit Fehlern, die Namen der Tests bleiben eindeutig.
    let fehler = meldungen(konsole);
    if let Some(e) = ergebnisse.iter_mut().find(|e| e.fehlgeschlagen > 0) {
        e.fehler = fehler;
    } else if !fehler.is_empty() {
        ergebnisse.push(Testergebnis {
            binary: "ohne Zuordnung".into(),
            fehler,
            ..Testergebnis::default()
        });
    }
    ergebnisse
}

fn dauer(ms: u64) -> String {
    let s = ms.checked_div(1_000).unwrap_or(0);
    format!(
        "{}:{:02}",
        s.checked_div(60).unwrap_or(0),
        s.checked_rem(60).unwrap_or(0)
    )
}

/// Das Kurzbild, höchstens [`HOECHSTENS`] Zeilen.
pub fn kurzbild(konsole: &str, erfolgreich: bool) -> Vec<String> {
    let mut z = vec![format!(
        "### Kodex: {}",
        if erfolgreich {
            "Gate offen"
        } else {
            "Gate geschlossen"
        }
    )];
    let stufen = stufen(konsole);
    if !stufen.is_empty() {
        z.push(String::new());
        z.push("| Stufe | Dauer |".into());
        z.push("|---|---:|".into());
        for s in &stufen {
            z.push(format!("| {} | {} |", s.name, dauer(s.dauer_ms)));
        }
    }
    let mut tests = testergebnisse(konsole);
    let rot: Vec<&Testergebnis> = tests
        .iter()
        .filter(|t| t.fehlgeschlagen > 0 || t.abbruch.is_some() || !t.fehler.is_empty())
        .collect();
    let mut fehlerzeilen = Vec::new();
    for t in &rot {
        if let Some(code) = t.abbruch {
            fehlerzeilen.push(format!(
                "- {}: Absturz ohne Testergebnis (Exit {code})",
                t.binary
            ));
        }
        for (name, meldung) in &t.fehler {
            fehlerzeilen.push(format!("- {} :: {name} – {meldung}", t.binary));
        }
    }
    if !fehlerzeilen.is_empty() {
        z.push(String::new());
        z.push("**Fehlgeschlagen:**".into());
        z.extend(fehlerzeilen);
    }
    tests.sort_by(|a, b| b.dauer_ms.cmp(&a.dauer_ms).then(a.binary.cmp(&b.binary)));
    if !tests.is_empty() {
        z.push(String::new());
        z.push("| Langsamste Testprogramme | Dauer | Tests |".into());
        z.push("|---|---:|---:|".into());
        for t in tests.iter().take(LANGSAMSTE) {
            z.push(format!(
                "| {} | {} | {} |",
                t.binary,
                dauer(t.dauer_ms),
                t.bestanden.saturating_add(t.fehlgeschlagen)
            ));
        }
    }
    if z.len() > HOECHSTENS {
        let weitere = z.len().saturating_sub(HOECHSTENS.saturating_sub(1));
        z.truncate(HOECHSTENS.saturating_sub(1));
        z.push(format!("… {weitere} weitere Zeilen im Export"));
    }
    let mut text = String::new();
    for zeile in &z {
        let _ = writeln!(text, "{zeile}");
    }
    if !depot_kodex::geheimnis::pruefe_inhalt("Zusammenfassung", text.as_bytes()).is_empty() {
        return vec![
            "### Kodex: Zusammenfassung zurückgehalten".into(),
            "Geheimnisverdacht in der Ausgabe. Details nur im lokalen Protokoll (R28).".into(),
        ];
    }
    z
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERIELL: &str = "[Kodex] cargo clippy --workspace · Dauer: 25195 ms
running 2 tests
test a ... ok
test b ... FAILED

failures:

---- b stdout ----
thread 'b' panicked at src/lib.rs:3:5:
Erwartet 3, war 4
note: run with `RUST_BACKTRACE=1`

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 165.51s

running 1 test
test c ... ok

test result: ok. 1 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.04s

[Kodex] cargo test --workspace --no-fail-fast · Dauer: 516502 ms
     Running tests\\invarianten.rs (D:\\a\\bau\\debug\\deps\\invarianten-770c2f435fb5d3fe.exe)
   Doc-tests depot_kern
";

    /// Gegenprobe `zusammenfassung_nennt_fehlgeschlagenen_test`.
    #[test]
    fn zusammenfassung_nennt_fehlgeschlagenen_test() {
        let t = testergebnisse(SERIELL);
        assert_eq!(t.len(), 2);
        let erstes = t.first().cloned().unwrap_or_default();
        assert_eq!(erstes.binary, "invarianten tests/invarianten.rs 770c2f43");
        assert_eq!((erstes.bestanden, erstes.fehlgeschlagen), (1, 1));
        assert_eq!(erstes.dauer_ms, 165_510);
        assert_eq!(
            erstes.fehler,
            vec![("b".to_string(), "Erwartet 3, war 4".to_string())]
        );
        assert_eq!(
            t.get(1).map(|e| (e.binary.as_str(), e.ignoriert)),
            Some(("depot_kern (doc)", 2))
        );
        let bild = kurzbild(SERIELL, false).join("\n");
        assert!(bild.contains("Gate geschlossen"), "{bild}");
        assert!(bild.contains(":: b – Erwartet 3, war 4"), "{bild}");
        assert!(
            bild.contains("| test --workspace --no-fail-fast | 8:36 |"),
            "{bild}"
        );
        assert!(
            bild.contains("| invarianten tests/invarianten.rs 770c2f43 | 2:45 | 2 |"),
            "{bild}"
        );
    }

    /// Gegenprobe `zusammenfassung_schwaerzt`: Ein Schlüssel in einer Meldung hält das ganze
    /// Kurzbild zurück, wie beim Export.
    #[test]
    fn zusammenfassung_schwaerzt() {
        let schluessel = format!("AIza{}", "x".repeat(35));
        let konsole = SERIELL.replace("Erwartet 3, war 4", &format!("Antwort {schluessel}"));
        let bild = kurzbild(&konsole, false).join("\n");
        assert!(!bild.contains(&schluessel), "{bild}");
        assert!(bild.contains("zurückgehalten"), "{bild}");
    }

    /// Gegenprobe `zusammenfassung_begrenzt`: 500 Fehler ergeben 60 Zeilen mit Hinweis.
    #[test]
    fn zusammenfassung_begrenzt() {
        let mut konsole = String::from("running 500 tests\n");
        for i in 0..500 {
            let _ = write!(konsole, "---- t{i} stdout ----\nFehler {i}\n\n");
        }
        konsole.push_str("test result: FAILED. 0 passed; 500 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s\n");
        let bild = kurzbild(&konsole, false);
        assert_eq!(bild.len(), HOECHSTENS);
        assert!(
            bild.last().is_some_and(|z| z.contains("weitere Zeilen")),
            "{bild:?}"
        );
    }

    /// C1-Blöcke: sortiert nach Name, Absturz ohne `test result` gilt als rot.
    #[test]
    fn bloecke_aus_c1_und_absturz() {
        let konsole = "[Kodex] Testprogramm z/tests/b · Beginn
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
[Kodex] Testprogramm z/tests/b · Exit 0 · 120 ms
[Kodex] Testprogramm a/tests/a · Beginn
Segmentation fault
[Kodex] Testprogramm a/tests/a · Exit 139 · 30 ms
";
        let t = testergebnisse(konsole);
        assert_eq!(
            t.iter().map(|e| e.binary.as_str()).collect::<Vec<_>>(),
            vec!["a/tests/a", "z/tests/b"]
        );
        assert_eq!(t.first().and_then(|e| e.abbruch), Some(139));
        assert_eq!(
            t.get(1).map(|e| (e.bestanden, e.dauer_ms, e.abbruch)),
            Some((4, 120, None))
        );
        let bild = kurzbild(konsole, false).join("\n");
        assert!(
            bild.contains("a/tests/a: Absturz ohne Testergebnis (Exit 139)"),
            "{bild}"
        );
    }

    /// Passen Namen und Ergebnisse nicht zusammen, wird nichts falsch benannt.
    #[test]
    fn ohne_passende_namen_nummeriert() {
        let konsole = "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n";
        assert_eq!(
            testergebnisse(konsole).first().map(|e| e.binary.clone()),
            Some("Testprogramm 1".into())
        );
        assert_eq!(sekunden_ms("2.5"), Some(2_500));
        assert_eq!(sekunden_ms("0.04"), Some(40));
    }
}
