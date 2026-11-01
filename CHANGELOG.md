# Änderungen

Format nach [Keep a Changelog](https://keepachangelog.com/de/1.1.0/). Versionen nach [Semantic Versioning](https://semver.org/lang/de/).

## [Unveröffentlicht]

### Geändert

- Schnitt Kern (1·A01, 1·F01, 1·R06, Preisteil von 1·F03): `Cent::euro` und `Cent::abs` melden Überlauf als `None`. `Grenzen` und `Gebuehren` haben private Felder und keine Setzer, auch nicht hinter einem Feature (K8-01). Neue geprüfte Typen `Datum`, `Zeitpunkt`, `Kurs` (positiv, Euro, Handelstag, Zeitpunkt, Herkunft) und `Kurse` (mit Stand und Alter). Bargeld eines Topfes ist nie negativ, seine Felder sind privat. Nachweis in `docs/befunde/kern.md`.
- Kodex-Umbau (1·G01–G06 Kodex-Teil, 1·R03, 1·R08, 2·A11): Rust-Code wird in Token zerlegt und syntaktisch geprüft (Lint-Attribute samt `cfg_attr`, aufgelöste Importe), Manifeste mit eigenem TOML-Leser semantisch. Jedes Produktiv-Target verbietet die harten Lints im Kopf mit `forbid`, auch der Kodex selbst. Abhängigkeiten mit voller Identität und `Cargo.lock`-Hash festgeschrieben, Essenz mit festem Register, Pfadlisten NUL-getrennt, Lesefehler und Links sind Fehler. Nach Maus' Prüfung (K7-01 bis K7-03): Aliasse behalten alle Bedeutungen, mehrdeutige Aliasse und die Einbindung von `std` als Ganzes sind in reinen Crates verboten, `#[path]` ist verboten, Crates enthalten nur `.rs`-Quellen, und jede davon wird vollständig geprüft. Jede Änderung der Festschreibung braucht eine an Diff und Festschreibung gebundene Grenzfreigabe `kodex/freigabe.txt`. Nachweis in `docs/befunde/kodex.md`.
- Die harten Lints stehen im Kopf von `depot_kern`, `depot_regelwerk`, `depot_ki` und `depot_lauf` auf `forbid`. Der Compiler lehnt damit jede lokale Ausnahme ab, auch hinter `cfg_attr` (1·G01). Der Kopf von `depot_kodex` und die Prüfung durch den Kodex folgen mit dem Kodex-Umbau.

### Tests

- Schnitt Kern: drei neue Negativtests für Währung, Handelstag, Herkunft und Alter (am alten Stand rot) und Maus' Gegenprobe K8-01 (Release mit allen Features); die sieben Kern-Tests und die zwei F03-Tests laufen ohne `ignore`. Die Sonde baut auf Wunsch im Release mit allen Features.
- Kodex-Schritt: 14 Negativtests (davon 4 aus Maus' Gegenbeispielen zu PR #7) in `crates/depot_kodex/tests/befunde.rs`, am alten Kodex rot, nach dem Umbau grün; jeder mit Kontrolle an einer sauberen Kopie und Ablehnung nur mit erwartetem Stichwort.
- Stufe 2, Schritt 1: 35 Negativtests für 19 Befunde aus Kern, Regelwerk und KI-Budget (`tests/befunde.rs` je Crate). Sie sind am alten Stand rot und bis zum Umbau mit `ignore` markiert; Nachweis, Gegenproben und fehlende Teilnachweise in `docs/befunde/negativtests.md`.
- Sonde für Bau- und Vertragstests: liest Compilermeldungen strukturiert und verlangt den erwarteten Zugriffsfehler; Verträge werden gebaut und ausgeführt.
- Gezielte Grenzfälle je harter Grenze; der Zufallstest heißt jetzt `zufallstest_grenzen_am_schlusskurs`, weil er nur am Schlusskurs prüft.

### Behoben

- Kodex: Ein unveränderter Stand (frischer Klon von main) wird nicht mehr wegen fehlender Deklaration abgelehnt.

### Hinzugefügt

- `depot_kern`: Geld in Cent mit geprüfter Rechnung, ISIN mit Prüfziffer, Töpfe A (Dachs = Claude) und B (Maus = GPT), Grenzen mit Fingerabdruck, Positivliste.
- `depot_regelwerk`: reine Entscheidungsfunktion, versiegelter `GenehmigterAuftrag`, Kursprüfung, Wächter mit Tagesverlust-Halt und Notbremse.
- `depot_ki`: harter KI-Budgetwächter mit Kassenbuch; Erlaubnis erst nach dauerhafter Buchung der Höchstkosten.
- `depot_kodex`: ausführbares Gate (`run.cmd`, GitHub Actions) mit Deklaration, Pfadrollen, Festschreibung, Quellscan und Essenz-Readback.
- `depot_lauf`: Ein-Befehl-Einstieg `run.cmd` mit Assistenten-Export `assistant_export_latest.zip` nach jedem Lauf (auch als Artefakt in GitHub Actions).
- Essenz D00–D17, Kaltstart, Risiko-Hinweis, Sicherheitsrichtlinie, Beitragsregeln, Vorlagen, MIT-Lizenz.
