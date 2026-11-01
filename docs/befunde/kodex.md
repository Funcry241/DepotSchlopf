# Kodex-Schritt – Negativtests und Umbau

Stufe 2, Kodex-Schritt. Er besteht aus zwei Änderungen, weil der Kodex eine Kodex-Änderung nie zusammen mit Produktivcode zulässt:

1. **Teil 1 (`RefactorOnly`, PR #6):** Die harten Lints stehen im Kopf von `depot_kern`, `depot_regelwerk`, `depot_ki` und `depot_lauf` auf `forbid`.
2. **Teil 2 (`KodexAenderung`, dieser Schritt):** Erster Commit nur die Negativtests (rot), danach der Umbau des Kodex, bis sie grün sind.

Ein grüner Test schließt keinen Befund. „Technisch geschlossen“ setzt erst die unabhängige Prüfung durch Maus.

## So prüfst du es nach

```text
cargo test --locked -p depot_kodex --test befunde
```

Am ersten Commit von Teil 2 (Tests mit `#[ignore]`) zeigt `-- --ignored` alle zehn Tests rot. Nach dem Umbau laufen sie ohne `ignore` im Gate.

Jeder Test baut eine Kopie des Arbeitsbaums unter `target/kodex-befunde/` mit eigener Git-Basis. Vor jedem Verstoß besteht dieselbe Kopie mit einer harmlosen, sauber deklarierten Änderung das Gate (Kontrolle). Ein Verstoß gilt nur als abgelehnt, wenn die Ablehnung das erwartete Stichwort nennt. Mehrere Fälle eines Befunds laufen einzeln; jeder muss für sich abgelehnt werden.

## Ergebnis

| Befund | Test (`depot_kodex`, `tests/befunde.rs`) | am alten Kodex | nach dem Umbau |
| --- | --- | --- | --- |
| 1·G01 | `befund_1_g01_ausnahme_hinter_cfg_attr_wird_abgelehnt`: `cfg_attr(not(test), allow)`, `cfg_attr(all(), expect)`, verschachteltes `cfg_attr`, Gruppen `restriction` und `pedantic`, `unsafe_code` hinter `cfg_attr` | rot: alle sechs Fälle angenommen | grün |
| 1·G01 | `befund_1_g01_kopf_ohne_forbid_wird_abgelehnt`: `forbid` entfernt, hinter `cfg_attr`, ohne `clippy::panic` | rot: alle drei angenommen | grün |
| 1·G01, 2·A11 | `befund_1_g01_compiler_lehnt_ausnahme_in_jedem_target_ab`: bewusster Verstoß (`allow` und `expect` hinter `cfg_attr`) in allen sechs Produktiv-Targets, `cargo clippy -D warnings`; nur `E0453` zählt | rot: `depot_kodex` Bibliothek und Programm übersetzen (die vier übrigen Targets sind seit Teil 1 geschützt) | grün |
| 1·G02, 2·A11 | `befund_1_g02_reinheit_haelt_gegen_gruppierte_importe`: gruppiert, umbenannt, `self as`, `extern crate std as`, Glob aus `std`, `use std as s` | rot: fünf von sechs angenommen | grün |
| 1·G03 | `befund_1_g03_vorgetaeuschte_lint_vererbung_wird_abgelehnt`: Vererbung nur in einem mehrzeiligen Text; Vererbung abgeschaltet | rot: der Text-Fall angenommen | grün |
| 1·G04 | `befund_1_g04_abhaengigkeit_mit_anderer_identitaet_wird_abgelehnt`: andere Features, anderes Paket unter gleichem Namen, andere Version, veränderte `Cargo.lock` | rot: alle vier angenommen | grün |
| 1·G05 | `befund_1_g05_geloeschte_oder_umbenannte_regel_wird_abgelehnt`: D17 gelöscht, alles ab D01 gelöscht, D05 umbenannt | rot: alle drei angenommen | grün |
| 1·G06 | `befund_1_g06_unicode_pfade_werden_richtig_geprueft`: deklarierte Datei mit Umlaut und Leerzeichen; undeklarierte wird mit echtem Namen gemeldet | rot: Pfad quotiert, deklarierte Datei abgelehnt | grün (Kodex-Teil) |
| 1·R03 | `befund_1_r03_lesefehler_beim_scan_macht_den_kodex_rot`: `tests` ist kein Ordner; Link ins Leere im Quellbaum; Ordner ohne Leserecht | rot: beide ersten Fälle übersprungen | grün |
| 1·R08 | `befund_1_r08_freigabe_ohne_passenden_hash_wird_abgelehnt`: ohne Freigabe, Freigabe für eine andere Festschreibung, nach der Freigabe weiter geändert; Gegenstück: passende Freigabe besteht | rot: alle drei angenommen | grün |

Der Fall „Ordner ohne Leserecht“ läuft nur, wenn das Lesen wirklich scheitert. Unter Administratorrechten meldet der Test „nicht ausgeführt“; die beiden anderen Fälle laufen immer.

## Gegenproben am neuen Kodex

Jede Zeile schwächt den neuen Kodex an genau einer Stelle und zeigt, dass der zuständige Test dann rot wird.

| Gegenprobe | Ergebnis |
| --- | --- |
| Kopf von `depot_kodex` mit `deny` statt `forbid` | Compilertest rot |
| `cfg_attr` wird nicht ausgewertet | G01-Attributtest rot |
| Gruppen `restriction` und `pedantic` gelten nicht als hart | G01-Attributtest rot |
| ein `forbid` hinter `cfg_attr` zählt als Kopf | G01-Kopftest rot |
| Import-Aliasse werden nicht aufgelöst | G02 rot |
| `[lints]` wird nicht semantisch geprüft | G03 rot |
| Abhängigkeiten nur nach Alias festgeschrieben (Festschreibung neu berechnet) | G04 rot: Features, Paket und Version angenommen |
| Essenz ohne Register | G05 rot |
| Pfadliste ohne `-z` | G06 rot |
| Links im Quellbaum erlaubt | R03 rot |
| Lesefehler eines Ordners übersprungen, Nicht-Ordner still ignoriert | R03 rot |
| Freigabe nicht an den Diff gebunden | R08 rot |

## Was der Umbau tut

- **Lexer und Syntax (`lexer.rs`, `rust.rs`):** Rust-Code wird in Token zerlegt; Kommentare und Literale sind kein Code. Lint-Attribute werden samt `cfg_attr` (auch verschachtelt) ausgewertet, ohne die Bedingung als falsch anzunehmen. Importe werden aufgelöst: Gruppen, `self`, `as`, Glob, `extern crate … as …`.
- **Quellscan (`quellscan.rs`):** Jedes Produktiv-Target hat am Dateianfang ein unbedingtes `#![forbid(..)]` mit allen neun harten Lints, auch der Kodex selbst. Keine Datei setzt einen harten Lint oder die Gruppen `clippy::restriction`, `clippy::pedantic`, `warnings` herab. Reine Crates verwenden kein `std::fs`, `std::net`, `std::env`, `std::time`, `std::process`, `std::thread` und hängen nur von reinen Crates und `sha2` ab. `include!` ist im Produktivcode verboten. Lesefehler, Links und Sonderdateien sind Fehler. Der Kodex prüft jetzt auch sich selbst.
- **Manifeste (`toml.rs`, `manifest.rs`):** Ein strenger TOML-Leser. `[lints]` jedes Crates muss genau `workspace = true` sein; der Workspace setzt jeden harten Lint auf `deny` oder `forbid`. Produktiv-Targets liegen unter `src/`; Build-Skripte sind verboten.
- **Festschreibung:** Abhängigkeiten mit voller Identität (Abschnitt, Alias und kanonischer Eintrag mit Version, Paket, Quelle, Features); neuer Schlüssel `cargo_lock` (Hash von `Cargo.lock`); Lints semantisch. Windows-Zeilenenden ändern nichts.
- **Grenzfreigabe (`freigabe.rs`, R30):** Jede Änderung von `kodex/festschreibung.txt` braucht in derselben Änderung eine neue `kodex/freigabe.txt`, gebunden an den Diff und an den Hash der neuen Festschreibung. Die Klassen `FestschreibungAenderung` und `KodexAenderung` dürfen die Festschreibung ändern; eine `KodexAenderung` kann dabei nur die Berechnung ändern, nicht Grenzen, Budgets oder Produktarten, denn die liegen im Produktivcode. Die Vorlage erzeugt `cargo run -p depot_kodex -- freigabe --basis <basis>`.
- **Essenz-Register:** `kodex/essenz_register.txt` nennt D00 bis D17 mit Titel. Fehlt eine Regel, ist sie umbenannt oder kommt eine ohne Registereintrag dazu, ist der Kodex rot. Der Kaltstart muss den Bereich aus dem Register nennen.
- **Git (`git.rs`):** Pfadlisten werden NUL-getrennt und ohne Umbenennungserkennung gelesen (eine Umbenennung zeigt beide Pfade).

## Grenzen, die ausdrücklich bleiben

- **Makros und Reexporte:** Der Kodex expandiert keine Makros und verfolgt keine Reexporte fremder Crates. Dafür stehen die Crate-Grenzen der reinen Crates und das Verbot von `include!`.
- **Urheberschaft der Freigabe (1·R08):** Der Kodex prüft die Bindung an Diff und Festschreibung, nicht, wer die Datei geschrieben hat. Dass Wolfgang sie erteilt, sichern die Regeln auf GitHub (nur Wolfgang führt zusammen). Eine maschinenprüfbare Unterschrift wäre ein eigener späterer Schritt.
- **Export (1·G06, 1·R03):** Der Export in `depot_lauf` liest Pfade noch zeilenweise und überspringt Lesefehler. Das ist Produktivcode und gehört zum Schnitt „Export“.
