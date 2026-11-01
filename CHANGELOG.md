# Änderungen

Format nach [Keep a Changelog](https://keepachangelog.com/de/1.1.0/). Versionen nach [Semantic Versioning](https://semver.org/lang/de/).

## [Unveröffentlicht]

### Geändert

- Die harten Lints stehen im Kopf von `depot_kern`, `depot_regelwerk`, `depot_ki` und `depot_lauf` auf `forbid`. Der Compiler lehnt damit jede lokale Ausnahme ab, auch hinter `cfg_attr` (1·G01). Der Kopf von `depot_kodex` und die Prüfung durch den Kodex folgen mit dem Kodex-Umbau.

### Tests

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
