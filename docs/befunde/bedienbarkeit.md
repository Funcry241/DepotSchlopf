# Sichtbare Tätigkeit und wichtige Aktionen

Anlass: Otter findet laufende Tätigkeiten und wichtige Knöpfe schwer und möchte gefundene Updates direkt sehen.

- Eine gemeinsame Betriebsleiste steht außerhalb der aktualisierten Inhalte und Formulare. Live, Einrichtung, Bestätigung und vollständiges Protokoll zeigen denselben Stand über `/teil/kopf`. Auf kleinen Bildschirmen bleibt sie im Seitenfluss, damit sie keine Formulare verdeckt.
- Laufende Aufträge zeigen Name, Startzeit, Anlass, neuesten Wartehinweis und Warteschlange. Ein unbestimmter Fortschrittsbalken behauptet keine Prozentwerte. Abschluss und Update-Rückfall bleiben erkennbar; die Verbindungswarnung hat weiterhin Vorrang.
- Primäre Auftragsknöpfe und Bestätigungslinks sind blau gefüllt und mindestens 44 Pixel hoch. Laufende Knöpfe und der Tätigkeitsbereich erhalten einen farbigen Rand und Text. Deaktivierte Knöpfe behalten ihren sichtbaren Grund. Gefährliche Freigaben erhalten keine neue Empfehlung.
- Nur das bestehende, gültige Update-Angebot erzeugt den Hinweis. Einsehen installiert nichts. Bestätigung, Bindung an den Commit und der Schutz vor doppeltem Start bleiben bestehen. Nachfragen nach Updates erfolgen weiter über den vorhandenen Suchknopf; diese Änderung führt keine zusätzlichen Netzabrufe zu GitHub ein.
- Die mobile Schlüsseltabelle verwendet wieder denselben CSS-Klassennamen wie das Formular.

Nachweise: Diensttests lesen dieselbe Tätigkeit auf beiden Seiten und im Teil-Endpunkt, prüfen maskierte Wartehinweise und den Wechsel zum Abschluss. Ein echter harmloser Suchlauf liefert einen Kandidaten; die Anzeige wird auf beiden Seiten geprüft, ein unbestätigtes Update bleibt abgelehnt und ein eingereihtes Update wird nicht erneut angeboten. Der Verhaltenstest des ausgelieferten Browser-Skripts wechselt die Betriebsleiste ohne Seitenneuladung. Es werden keine echten Schlüssel, KI-Aufrufe oder fremden Kurse benötigt.
