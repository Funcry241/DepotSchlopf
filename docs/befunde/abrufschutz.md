# Abrufschutz für Kurse und Termine

DepotSchlopf erkennt einen Limithinweis von Alpha Vantage an genau einem JSON-Feld namens `Information` oder `Note`. Das Feld enthält einen Text mit `rate limit` oder `requests per day`. Enthält die Antwort einen hinterlegten Schlüssel, darf nur der eigene Alpha-Vantage-Schlüssel im Meldungstext stehen. DepotSchlopf verwirft die Antwort, meldet das Limit mit einem festen Text und beendet weitere Alpha-Vantage-Abrufe dieses Laufs. Ein Schlüsseltreffer in einer anderen Antwort löst weiterhin den Sicherheitsabbruch aus.

Bei einer Kurssperre laufen separate RSS- und EZB-Abrufe weiter. Beide prüfen die Antworten auf hinterlegte Schlüssel. Reguläre Kursabrufe, der Alpha-Vantage-Quartalskalender und der Papiertag bleiben gesperrt. Der ausdrücklich gestartete Prüfabruf bleibt möglich und unterliegt ebenfalls dem Kontingent.

Kurse, Quartalskalender und Prüfabrufe teilen sich höchstens 20 Alpha-Vantage-Versuche innerhalb der letzten 24 Stunden. DepotSchlopf schreibt jeden Versuch vor dem Netzaufruf. Das gilt auch bei Wiederholungen und Netzfehlern. Der Xetra-Kurslauf prüft vorab, ob das Kontingent für alle vorgesehenen Xetra-Werte reicht. Der Kalender prüft zusätzlich, ob danach genug Versuche für einen vollständigen Xetra-Lauf übrig bleiben. Tiingo und die öffentlichen Quellen haben eigene Abrufwege.

Die Einrichtung zeigt die Zahl für den heutigen Frankfurter Kalendertag und für die letzten 24 Stunden. Unlesbare Vermerke oder eine nicht verfügbare Sperrdatei halten Alpha-Vantage-Abrufe an. Der Dienst räumt beim Start reguläre Kontingentdateien auf, die älter als sieben Tage sind. Die Dateien enthalten Zeitpunkte und Kürzel.
