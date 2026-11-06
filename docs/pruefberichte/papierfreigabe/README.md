# Entwickler-Prüfdaten für die Papierfreigabe

Für den Windows-Dialog im laufenden DepotSchlopf gibt es in der Einrichtung den Knopf **Freigabedialog testen**. Dieser Weg zeigt den echten Dialog und schreibt weder ins Journal noch ins Papierkonto. Die folgenden Prüfdaten dienen der Entwicklung und dem vollständigen Test eines Freigabebelegs in einer getrennten Installation.

**Die drei synthetischen Prüfdateien gehören ausschließlich in eine separate Prüfinstallation.**

1. Beende den laufenden DepotSchlopf-Dienst und sichere den echten Ordner `daten\`. Lege einen frischen, eindeutig benannten Prüfordner mit einer eigenen Programmkopie an. Dort muss `daten\` fehlen oder leer sein. Falls dort bereits Dateien liegen, verwende einen anderen frischen Prüfordner.
2. Starte `DepotSchlopf.cmd` aus dem Prüfordner. Prüfe in der Einrichtung unter **Dienst**, dass der angezeigte Projektordner wirklich der Prüfordner ist. Beende den Prüfdienst wieder.
3. Lade [`journal.txt`](daten/journal.txt), [`journal.vermerk`](daten/journal.vermerk) und [`papierkonto.txt`](daten/papierkonto.txt) einzeln über **Raw** herunter. Speichere sie unverändert unter diesen Namen in `daten\` des Prüfordners. Öffne und speichere sie dabei nicht in einem Texteditor. Das Prüfjournal muss genau 606 Byte lang sein. Bei 612 Byte wurden die Zeilenenden umgeschrieben.
4. Starte den Prüfdienst erneut und öffne nur **Einrichtung**. Lass den Zeitplan ausgeschaltet und starte weder einen Papiertag noch einen Abruf. Im selben Windows-Konto können die hinterlegten API-Schlüssel auch im Prüfordner verfügbar sein. Das Freigabeformular zeigt 10.000 € Bargeld und 33,33 % Gesamtverlust.
5. Öffne die Freigabe. Prüfe, ob der Windows-Dialog sichtbar ist und **Nein** vorausgewählt ist. Drücke Enter. Die Sperre bleibt aktiv. Öffne den Dialog erneut und warte zwei Minuten. Auch dann bleibt die Sperre aktiv.
6. Wähle zuletzt ausdrücklich **Ja**. Im Prüfjournal erscheint ein Eintrag `papier-freigabe` mit dem Weg `einrichtung;dialog` und dem UTC-Zeitpunkt. Nach einem Neustart bleibt die Freigabe erhalten.

## Zweiter Durchlauf über den Autostart

1. Öffne im Windows-Dialog **Ausführen** den Ordner `shell:startup`. Sichere eine vorhandene `DepotSchlopf.cmd` außerhalb dieses Autostart-Ordners und merke dir, falls noch kein Eintrag vorhanden ist. Der Eintrag gilt für das ganze Windows-Konto.
2. Beende den Prüfdienst. Setze ausschließlich im geprüften Prüfordner alle drei Prüfdateien auf den Ausgangsstand zurück. Starte den Prüfdienst wieder und richte den Autostart vorübergehend über dessen **Einrichtung** ein. Melde dich ab und wieder an.
3. Prüfe unter **Dienst** erneut den Projektordner und wiederhole den Dialogtest. Beende danach den Prüfdienst. Stelle den gesicherten Autostart-Eintrag wieder her oder entferne den Testeintrag, wenn vorher keiner vorhanden war. Prüfe das Ergebnis in `shell:startup`.

Die Dateien entsprechen dem Ausgangsstand des automatisierten Diensttests `papierfreigabe_ueber_einrichtung_schreibt_bestaetigten_stand`.
