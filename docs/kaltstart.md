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

Öffne in DepotSchlopf **Einrichtung → IBKR-Papierkonto einrichten und testen**. Der Abschnitt zeigt den Weg zum Papierzugang im Client Portal und die Einstellungen für das lokale IB Gateway. Ist der Startbestand des Papierkontos abweichend, prüfe im Client Portal im Paper-Modus unter **Einstellungen → Kontoeinstellungen → Paper Trading Account Reset**, ob IBKR 1.000 € in EUR anbietet. Speichere die Papierkontonummer mit `DU` oder `DUR` in DepotSchlopf und klicke auf **Papierabruf testen**. DepotSchlopf speichert die Nummer lokal in `daten/ibkr-papierkonto.txt`. Die aufklappbare Gateway-Diagnose zeigt die Struktur der Antwort ohne Beträge oder Kontonummern.

`run.cmd ibkr papier` zeigt Gesamtbargeld, abgerechnetes Bargeld, verfügbare Mittel, Positionen und offene Orders in der Konsole. Für fehlendes abgerechnetes Bargeld und fehlende verfügbare Mittel zeigt der Befehl „nicht geliefert“. Gesamtbargeld und vollständige Positions- und Orderlisten sind erforderlich. Ein anderes Konto oder nicht erlaubte Währungsbestände führen zu einem Fehler. Die Prüfung von Kontoart und Handelsrechten gehört zum späteren Praxisnachweis.

Mit `run.cmd ibkr journal-einrichten` legst du je Papierkonto ein separates Journal an. Es beginnt mit einem Langfristbereich und 1.000 € Soll-Bargeld. Der Befehl setzt ein eingerichtetes Journal nicht zurück und bucht kein Geld beim Broker. Das Papierkonto muss für den ersten Abgleich tatsächlich den passenden Bestand aufweisen.

`run.cmd ibkr abgleichen` vergleicht Gesamtbargeld, Positionen und offene Orders und protokolliert das Ergebnis im IBKR-Papierjournal. Abgerechnetes Bargeld und verfügbare Mittel hält DepotSchlopf als IBKR-Angaben fest. Fehlt eines dieser Felder, vermerkt der Abgleich „nicht geliefert“. Für Positionen und offene Orders braucht der Vergleich die lokal verifizierte Zuordnung in `daten/ibkr-papier/<Papierkontonummer>/kontrakte.txt` mit einer Zeile `Kontrakt-ID ISIN` je Wert. Für offene Broker-Orders muss die Order-Referenz der DepotSchlopf-Auftragsnummer entsprechen. Eine Abweichung bei Gesamtbargeld, Positionen oder Orders liefert einen Fehlerstatus. Die Prüfung von Kontoart und Handelsrechten sowie ein Abgleich von Ausführungen und Valuta-Belegen stehen noch aus.

### Vergleichsmaßstäbe im Analyse-Export

Der Analyse-Export zeigt EUNL und den verzinsten €STR-Vergleich zwischen dem ersten und letzten belegten Depotabschluss im gewählten Zeitraum. EUNL nutzt die vorhandenen Journal-Kursbelege. Öffne für €STR **Einrichtung → €STR für den Analyse-Export**. Lade dort die EZB-Indexreihe als CSV herunter, wähle die Datei aus und klicke auf **€STR-CSV importieren**. DepotSchlopf prüft die Indexreihe, speichert sie lokal und zeigt den verfügbaren Zeitraum. Für den Vergleich braucht der Export an beiden Stichtagen einen Indexstand.
