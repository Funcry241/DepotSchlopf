# Kaltstart – hier beginnen

Diese Datei zuerst lesen, wenn ein neuer Bearbeiter (Mensch oder KI) ohne Vorgeschichte an DepotSchlopf arbeitet.

## Reihenfolge

1. `README.md` – Zweck, Crates, Leitsatz.
2. `docs/essenz.md` – Erfahrungsregeln D00 bis D17. Beratend (advisory only), nicht normativ.
3. `kodex/festschreibung.txt` – was ohne Freigabe des Eigentümers nie geändert wird.
4. `kodex/aenderung.txt` – die Deklaration der aktuellen Änderung.
5. Für die konkrete Aufgabe den zuständigen Code und seine Tests lesen.

## Arbeitsablauf je Änderung

1. Vorher: passende Essenz-Regeln lesen („Vor der Änderung prüfen“).
2. `kodex/aenderung.txt` neu schreiben: Klasse, Zweck, Pfade.
3. Ändern.
4. `run.cmd kodex` ausführen. Nur ein offenes Gate darf zusammengeführt werden.
5. Ist eine Festschreibung betroffen: `run.cmd festschreibung`, Klasse `FestschreibungAenderung`, Freigabe durch Wolfgang Mair.

## Klassen

| Klasse | darf ändern |
|---|---|
| `Gruendung` | alles, nur solange die Basis noch keinen Kodex hat |
| `DokuOnly` | Dokumentation |
| `TestOnly` | Dokumentation, Tests |
| `RefactorOnly` | Dokumentation, Tests, Produktivcode (ohne Verhaltensänderung) |
| `Funktion` | Dokumentation, Tests, Produktivcode, Werkzeug-Dateien |
| `KodexAenderung` | Gate (`crates/depot_kodex/`, `kodex/`, `.github/`), Dokumentation – nie Produktivcode |
| `FestschreibungAenderung` | Festschreibung, Produktivcode, Tests, Werkzeug, Dokumentation – nie das Gate |

Die Festschreibung gilt in jeder Klasse: Weicht der Baum davon ab, ist das Gate geschlossen.
