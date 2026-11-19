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

### Lesender IBKR-Papierabruf

Melde im IB Gateway das Papierkonto an. Aktiviere unter den API-Einstellungen die lokalen Socket-Verbindungen auf Port 4002 und lass „Read-Only API“ eingeschaltet. Lege in `daten/ibkr-papierkonto.txt` ausschließlich die unveränderte Nummer dieses Papierkontos ab. DepotSchlopf akzeptiert `DU` oder `DUR` mit anschließenden Ziffern. Die Datei bleibt lokal.

`run.cmd ibkr papier` liest Bargeld, abgerechnetes Bargeld, verfügbare Mittel, Positionen und offene Orders. Der Befehl zeigt die Werte in der Konsole an. Fehlende Daten, ein anderes Konto oder nicht erlaubte Währungsbestände führen zu einem Fehler. Der Abruf prüft die Kontoart und Handelsrechte noch nicht und führt keinen Journalabgleich durch.

Mit `run.cmd ibkr journal-einrichten` legst du je Papierkonto ein separates Journal an. Es beginnt mit einem Langfristbereich und 1.000 € Soll-Bargeld. Der Befehl setzt ein eingerichtetes Journal nicht zurück und bucht kein Geld beim Broker. Das Papierkonto muss für den ersten Abgleich tatsächlich den passenden Bestand aufweisen.

`run.cmd ibkr abgleichen` vergleicht Gesamtbargeld, abgerechnetes Bargeld, verfügbare Mittel, Positionen und offene Orders und protokolliert das Ergebnis im IBKR-Papierjournal. Für Positionen und offene Orders braucht der Vergleich die lokal verifizierte Zuordnung in `daten/ibkr-papier/<Papierkontonummer>/kontrakte.txt` mit einer Zeile `Kontrakt-ID ISIN` je Wert. Für offene Broker-Orders muss die Order-Referenz der DepotSchlopf-Auftragsnummer entsprechen. Eine Abweichung liefert einen Fehlerstatus. Die Prüfung von Kontoart und Handelsrechten sowie ein Abgleich von Ausführungen und Valuta-Belegen stehen noch aus.

### Vergleichsmaßstäbe im Analyse-Export

Der Analyse-Export zeigt EUNL und den verzinsten €STR-Vergleich zwischen dem ersten und letzten belegten Depotabschluss im gewählten Zeitraum. EUNL nutzt die vorhandenen Journal-Kursbelege. Lege für €STR die [EZB-Indexreihe im CSV-Format](https://data-api.ecb.europa.eu/service/data/EST/B.EU000A2QQF08.CI?format=csvdata&detail=dataonly) als `daten/estr-index.csv` ab. Der Export erwartet die Spalten `KEY`, `TIME_PERIOD` und `OBS_VALUE` und braucht für beide Stichtage einen Indexstand. Die Datei bleibt lokal.
