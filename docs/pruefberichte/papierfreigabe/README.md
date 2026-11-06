# Windows-Praxistest der Papierfreigabe

**Die drei synthetischen Dateien gehören ausschließlich in einen separaten Prüfordner.**

1. Beende den laufenden DepotSchlopf-Dienst. Kopiere die drei Dateien aus dem Unterordner `daten` in den Ordner `daten` deiner separaten Prüfinstallation.
2. Starte `DepotSchlopf.cmd` in der Prüfinstallation. Prüfe in der Einrichtung unter **Dienst** den angezeigten Projektordner. Das Freigabeformular zeigt 10.000 € Bargeld und 33,33 % Gesamtverlust.
3. Öffne die Freigabe. Prüfe, ob der Windows-Dialog sichtbar ist und **Nein** vorausgewählt ist. Bestätige zuerst mit Enter und prüfe, dass die Sperre aktiv bleibt. Öffne den Dialog erneut und warte zwei Minuten. Auch dann bleibt die Sperre aktiv.
4. Wähle zuletzt ausdrücklich **Ja**. Im Journal erscheint ein Eintrag `papier-freigabe` mit dem Weg `einrichtung;dialog` und dem UTC-Zeitpunkt. Nach einem Neustart bleibt die Freigabe erhalten.
5. Wiederhole den Test mit Windows-Autostart in einem separaten Windows-Testkonto. Der Autostart-Eintrag gilt pro Windows-Konto. Beende den Prüfdienst und spiele vor dem zweiten Durchlauf alle drei Prüfdateien in der Prüfinstallation erneut ein.

Die Dateien verwenden denselben synthetischen Ausgangsstand wie der automatisierte Diensttest `papierfreigabe_ueber_einrichtung_schreibt_bestaetigten_stand`.
