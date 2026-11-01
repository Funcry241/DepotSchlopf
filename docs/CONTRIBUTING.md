# Mitarbeit

Danke für dein Interesse. DepotSchlopf ist ein privat gepflegtes Projekt mit einem klaren Sicherheitsanspruch. Beiträge sind willkommen, wenn sie diesen Anspruch einhalten.

## Bevor du anfängst

1. `docs/kaltstart.md` lesen, dann die Essenz `docs/essenz.md`.
2. Größere Änderungen bitte vorher in einem Issue absprechen.
3. Den Risiko-Hinweis `docs/RISIKO.md` kennen.

## Ablauf je Änderung

1. Branch anlegen.
2. `kodex/aenderung.txt` für genau diese Änderung neu schreiben: Klasse, Zweck, Pfade.
3. Ändern. Für neue Regeln synthetische Tests schreiben (Essenz D12).
4. `run.cmd kodex` ausführen (unter Linux/macOS: `cargo run -p depot_kodex -- --voll`). Nur ein offenes Gate wird zusammengeführt. Dasselbe läuft in GitHub Actions auf jedem Pull Request.
5. Pull Request öffnen und die Vorlage ausfüllen.

## Harte Regeln

- Keine Änderung an `kodex/festschreibung.txt` ohne ausdrückliche Freigabe des Eigentümers (Klasse `FestschreibungAenderung`).
- Das Gate (`crates/depot_kodex/`, `kodex/`, `.github/`) ändert sich nie zusammen mit Produktivcode.
- Kein `unwrap`, `expect`, `panic`, Indexzugriff, ungeprüfte Arithmetik oder Gleitkomma im Produktivcode.
- Keine neuen Produktarten, keine Hebel, keine Derivate, keine Leerverkäufe.
- Keine Zugangsdaten, Kontoauszüge oder echten Depotstände im Repository.

## Sprache

Code-Bezeichner, Kommentare, Dokumentation und Issues sind deutsch. Englische Beiträge werden trotzdem gelesen.

## Lizenz

Mit einem Beitrag stimmst du zu, dass er unter der MIT-Lizenz des Projekts steht.
