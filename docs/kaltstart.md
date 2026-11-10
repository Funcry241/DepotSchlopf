# Kaltstart – hier beginnen

Diese Datei zuerst lesen, wenn ein neuer Bearbeiter (Mensch oder KI) ohne Vorgeschichte an DepotSchlopf arbeitet.

## Ein Befehl

`run.cmd` (ohne Argument) prüft alles und schreibt danach `assistant_export_latest.zip` in den Projektordner, auch wenn etwas scheitert. Für eine neue KI-Sitzung genügt diese eine Datei: hochladen und `assistant_handoff_latest.txt` darin lesen lassen. Laufzeitdaten und Zugangsdaten sind nie enthalten.

## Reihenfolge

1. `README.md` – Zweck, Crates, Leitsatz.
2. `docs/essenz.md` – Erfahrungsregeln D00 bis D17. Beratend (advisory only), nicht normativ.
3. `kodex/festschreibung.txt` – was ohne Freigabe des Eigentümers nie geändert wird.
4. `kodex/aenderung.txt` – die Deklaration der aktuellen Änderung.
5. `docs/MATHEMATIK.md` – Verfahren, Invarianten und ihre Grenzen.
6. Für die konkrete Aufgabe den zuständigen Code und seine Tests lesen.

## Arbeitsablauf je Änderung

1. Vorher: passende Essenz-Regeln lesen („Vor der Änderung prüfen“).
2. `kodex/aenderung.txt` neu schreiben: Klasse, Zweck, Pfade.
3. Ändern.
4. `run.cmd` ausführen. Nur ein offenes Gate darf zusammengeführt werden.
5. Ist eine Festschreibung betroffen: `run.cmd festschreibung`, Klasse `FestschreibungAenderung`. Zuletzt erteilt Wolfgang Mair die Grenzfreigabe `kodex/freigabe.txt`, gebunden an Diff und neue Festschreibung; die Vorlage zeigt `cargo run -p depot_kodex -- freigabe --basis origin/main`. Jede spätere Änderung macht die Freigabe ungültig.

## Klassen

| Klasse | darf ändern |
|---|---|
| `Gruendung` | alles, nur solange die Basis noch keinen Kodex hat |
| `DokuOnly` | Dokumentation |
| `TestOnly` | Dokumentation, Tests |
| `RefactorOnly` | Dokumentation, Tests, Produktivcode (ohne Verhaltensänderung) |
| `Funktion` | Dokumentation, Tests, Produktivcode, Werkzeug-Dateien |
| `KodexAenderung` | Gate (`crates/depot_kodex/`, `kodex/`, `.github/`), Dokumentation, die Berechnung der Festschreibung mit Grenzfreigabe – nie Produktivcode |
| `FestschreibungAenderung` | Festschreibung mit Grenzfreigabe, Produktivcode, Tests, Werkzeug, Dokumentation – nie das Gate |

Die Festschreibung gilt in jeder Klasse: Weicht der Baum davon ab, ist das Gate geschlossen. Die Essenz-Regeln D00 bis D17 stehen im festen Register `kodex/essenz_register.txt`.
