# KI-Aufrufe bei abgebrochener Verbindung

Maus' dritter von drei Punkten am 05.11.2026: Eine KI-Anfrage wird nur dann automatisch erneut gesendet, wenn die erste den Anbieter sicher nicht erreicht hat. Konnte sie angekommen sein, bleibt die Kostensperre aktiv. Die Oberfläche erklärt den Unterschied in Alltagssprache.

## Befund vorher

- `depot_berater::netz` unterschied schon `NetzFehler::NichtGesendet` (Name, Verbindungsaufbau, Verbindung abgewiesen) von `NetzFehler::Unklar` (alles nach dem Aufbau: Abbruch, Zeitgrenze, zu große Antwort).
- `NichtGesendet` beendete den Aufruf ohne Kosten und ohne neuen Versuch. Ein kurzer Ausfall kostete den Beitrag dieser KI für den ganzen Tag.
- `Unklar` sperrte mit `daten/ki.halt`. Die Sperrdatei sagte nur „Antwort fehlt, Kosten unbekannt“.

## Regel

| Fall | Was geschieht |
|---|---|
| sicher nicht gesendet: Name nicht auflösbar, Verbindung nicht aufgebaut oder abgewiesen, Zeitgrenze beim Auflösen oder beim Aufbau samt TLS | Der Versand wird nach 30 und nach 60 Sekunden wiederholt, höchstens dreimal (`ki::WARTEZEITEN`). Das läuft unter derselben Reservierung und mit derselben Anfrage. Jeder Versuch steht als „Netz“ ohne Kosten im Anbieterprotokoll. Vor jedem neuen Versand wird noch einmal auf Sperre und Pause des Anbieters geprüft. |
| vielleicht angekommen: alles nach dem Verbindungsaufbau | Nie wiederholt. `daten/ki.halt` sperrt beide KIs. Die Sperrdatei sagt: „Verbindung nach dem Absenden abgebrochen; die Anfrage kann angekommen sein und gekostet haben, Kosten unbekannt“. |

Warum dieselbe Reservierung reicht: Eine sicher nicht gesendete Anfrage hat den Anbieter nie erreicht. Je Reservierung kann so höchstens eine Anfrage beim Anbieter ankommen, und die gebuchten Höchstkosten bleiben die obere Grenze. Das Kassenbuch ändert sich nicht. Die Zahl der Aufrufe im Monat bleibt so, wie sie gebucht ist.

Im Zweifel gilt „vielleicht angekommen“. Ein TLS- oder E/A-Fehler nach dem Aufbau zählt dazu, auch wenn die Anfrage vielleicht nie hinausging.

## Anzeige

- Hinweiszeilen in der Ausgabe des Papiertags (`depot_kern::hinweis`). Die Live-Ansicht zeigt sie groß:
  - „Dachs: Anfrage hat den Anbieter nicht erreicht (keine Verbindung) und nichts gekostet. Nächster Versuch um 17:58 Uhr (Versuch 2 von 3). DepotSchlopf macht von selbst weiter, du musst nichts tun.“
  - „Dachs: Anfrage hat den Anbieter auch nach 3 Versuchen nicht erreicht … Heute ohne diese KI; der nächste Papiertag fragt wieder von selbst.“
  - „Dachs: Verbindung nach dem Absenden abgebrochen. Die Anfrage kann beim Anbieter angekommen sein und Geld gekostet haben. Deshalb fragt DepotSchlopf nicht noch einmal, sondern sperrt beide KIs. …“
- In der Einrichtung erklärt der Abschnitt „Sperre des Kostenschutzes“ beide Fälle („Wenn die Verbindung bei einer KI-Anfrage wegbricht“). Das gilt mit und ohne Sperre.

## Nachweis

`crates/depot_berater/tests/berater.rs`:

- `sicher_nicht_gesendet_wird_unter_derselben_reservierung_wiederholt`: zwei Mal nicht gesendet, dann Antwort; drei gleiche Anfragen, Vorschlag verwertbar, keine Sperre.
- `ohne_verbindung_nach_drei_versuchen_aufgegeben`: drei Versuche, kein Vorschlag, keine Sperre.
- `verbindung_nach_dem_absenden_gekappt_sperrt_ohne_zweiten_versand` (Abnahmefall 3): genau ein Versand, Sperre mit Grund in Worten, auch der andere Berater ist gesperrt.

`crates/depot_dienst/tests/dienst.rs`: `einrichtung_erklaert_abgebrochene_ki_anfragen`.

Für den Test unter Windows: Während des Papiertags die Verbindung kappen, nachdem „Frage an Dachs“ im Protokoll steht. Kommt die Verbindung erst nach dem Absenden weg, zeigt die Live-Ansicht die Sperre mit dem Grund, und die Einrichtung bietet das geführte Aufheben an. Ist schon vor dem Absenden kein Netz da, zeigt sie „Nächster Versuch um …“.
