# Papiertag am Morgen

An Tagen mit mindestens einer geöffneten Börse startet der Standardzeitplan die Termine um 07:20 Uhr und die Kursabrufe um 07:30 Uhr Frankfurter Zeit. Die Kursläufe für Xetra und NYSE folgen nacheinander. Der Zeitplan startet danach bis einschließlich 08:30 Uhr den Papiertag, sofern mindestens ein Kurslauf erfolgreich war. Die Einstellungen 17:45 und 17:52 werden beim ersten Dienststart auf 07:30 Uhr umgestellt. Individuelle Abendzeiten außerhalb dieser beiden Vorgaben behalten ihren Ablauf.

Der Bezugstag ist je Börse ihr letzter Handelstag vor dem Starttag. DepotSchlopf prüft den belegten Schlusskurs dieses Tages und misst das Höchstalter von 20 Minuten ab dem Abruf am Starttag. Fehlt ein gültiger Kurs bei einem gehaltenen Wert, hält der Morgenlauf an. Kandidaten ohne gültigen Kurs fallen aus der Auswahl. Wenn kein Wert übrig bleibt, beginnt kein Papiertag. Das Lagebild zeigt Bezugstage und Abrufzeit.

Neue Papierorders werden am Starttag versendet und gelten nur an Börsen, die an diesem Tag geöffnet sind. Die Simulation verwendet weiterhin einen passenden, geprüften Schlusskurs nach dem Versand. Liegt dieser erst am nächsten Morgen vor, kann das Papierkonto die Ausführung zum ursprünglichen Börsentag nachweisen und verbuchen. Die Buchung verwendet die Valuta des Ausführungstags. Eine Simulation zum Eröffnungskurs wäre eine eigene Änderung des Ausführungsmodells.

Die Montagprobe startet mit Freitagskursen, legt die Order für Montag an, verbucht sie nach Abruf des Montagskurses und prüft den Kontoabgleich. Eine Gegenprobe mit Donnerstagskursen schließt keinen Papiertag ab.
