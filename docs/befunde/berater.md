# Dachs und Maus im Papierhandel

Wolfgang am 05.11.2026: mutiger und schneller, täglicher Papierhandel, Dachs und Maus entscheiden aus Kursen und Nachrichten. KI-Kosten höchstens 20 € im Monat. Netzzugang für die Berater mit Websuche: „Ja, mit Websuche.“ Die Kodex-Änderung dazu steht in PR #24 (`NETZ_ERLAUBT` um `depot_berater` erweitert).

## Was gebaut ist

- **Neues Crate `depot_berater`** mit Netz, ein Blatt im Abhängigkeitsbaum: Es hängt von `depot_sim` ab, nie umgekehrt. `run.cmd papier` ruft jetzt sein Programm; der Papiertag selbst liegt als `depot_sim::papierlauf::papier` in der Bibliothek.
- **Anbieter:** Dachs fragt Claude Sonnet 5.5 (`claude-sonnet-5-5`, Messages API, Werkzeug `web_search_20260209`, Denken nur zwischen Werkzeugen, Aufwand mittel). Maus fragt GPT-5.4 (`gpt-5.4`, Responses API, Werkzeug `web_search`, Denken niedrig, nichts gespeichert). Antwortgrenze `max_token_aus` und Suchgrenze `max_suchen` kommen aus dem Deckel der Erlaubnis.
- **Gesendet** werden nur Papierdepot des Topfs, Schlusskurse und belegte Termine des Tages, die Positivliste mit Namen und die Grenzen. Keine Schlüssel im Inhalt, keine Kontodaten.
- **Schlüssel** nur in der Windows-Anmeldeinformationsverwaltung (`DepotSchlopf/anthropic`, `DepotSchlopf/openai`), nur im Kopf der Anfrage. `Debug` zeigt `***`, jede Meldung wird von ihm bereinigt. Gültig sind 20 bis 256 Zeichen aus Buchstaben, Ziffern, `-` und `_`; so kann er den Kopf nicht verändern. Ohne Schlüssel ist ein Berater nicht angebunden und wird nicht gefragt.
- **Antworten streng gelesen:** Das erste JSON-Objekt der Antwort, eigener Leser ohne fremde Bibliothek, feste Tiefe. Ein Vorschlag braucht `einschaetzung` und `ziele` (Liste oder `null`), jedes Ziel eine ISIN der Positivliste, `gewicht_bp` als ganze Zahl von 0 bis 10.000 und eine Begründung. Doppelte Felder oder Werte, fremde ISINs, Brüche: Die ganze Antwort wird verworfen, nichts geraten. Grenzen (Werte, Anteil, Summe) prüft danach das Regelwerk. Eine Zweitmeinung ist nur mit `"zustimmung": true` Zustimmung.
- **Kosten nach jedem Aufruf:** Aus dem gemeldeten Verbrauch (Eingabe-Token samt Suchergebnissen und Zwischenspeicher, Ausgabe-Token, Suchen) rechnet `KiDeckel::kosten_ist` die Kosten, aufgerundet. Sie stehen mit der Reservierung bei jeder Einschätzung und jedem Urteil im Journal.
- **Sperre `daten/ki.halt`:** Liegen die Kosten über der Reservierung, sind mehr Suchen oder Ausgabe-Token verbraucht als erlaubt, fehlt die Antwort nach dem Senden (Zeitgrenze, Abbruch) oder nennt sie keinen Verbrauch, schreibt der Berater die Sperrdatei mit dem Grund und verwirft die Antwort. Solange sie liegt, ist keine KI angebunden. Wolfgang löscht sie nach der Prüfung. Eine Ablehnung des Anbieters (Status ungleich 200) und ein sicher nicht gesendeter Aufruf sperren nicht; sie kosten nichts.
- **Netz:** nur `POST`, nur HTTPS, keine Weiterleitung, Zeitgrenze 300 Sekunden, Antwort höchstens 4 MB.

## Neue Festwerte (Festschreibung, Grenzfreigabe nötig)

| | bisher | neu |
|---|---|---|
| Dachs je Aufruf | 40.000 / 16.000 Token, 4 $ / 20 $ je Mio. | 50.000 / 4.000 Token, 2 $ / 10 $ je Mio. (Sonnet 5.5), 2 Suchen |
| Maus je Aufruf | 40.000 / 10.000 Token, 10 $ / 50 $ je Mio. | 50.000 / 4.000 Token, 2,50 $ / 15 $ je Mio. (GPT-5.4), 2 Suchen |
| Suche | – | 10 $ je 1.000 Suchen |
| Höchstkosten je Aufruf | 48 / 90 Cent | 16 / 21 Cent |
| je KI im Monat | 4 Aufrufe, 4 € | 46 Aufrufe, 10 € |
| je KI im Jahr | 50 € | 120 € |
| Werte je Topf | 8 | 4 |
| Anteil je Wert | 25 % | 40 % |
| Umschlag am Tag | 50 % | 100 % |
| ETFs | gesperrt | frei |

Zusammen höchstens 20 € im Monat. 46 Aufrufe reichen für 23 Handelstage mit je einem Vorschlag und einer Zweitmeinung je KI. Dollar zählen als Euro; das überschätzt. Die Eingabegrenze schließt die Suchergebnisse ein, die der Anbieter selbst einfügt; vorher prüfbar ist nur die eigene Frage (`eingabe_passt`), den Rest prüft die Kostenrechnung danach.

**Offen zu prüfen:** Die Preise von GPT-5.4 und der OpenAI-Websuche stammen nicht von der Seite von OpenAI selbst; sie war aus der Bauumgebung nicht erreichbar. Wolfgang oder Maus bestätigen sie vor der Grenzfreigabe. Sind sie höher, rechnet `kosten_ist` zu niedrig.

## Tests

`crates/depot_berater/tests/berater.rs`, ohne Netz mit erfundenem Anbieter:

- Dachs schlägt vor: Form der Anfrage (Modell, `max_tokens`, `max_uses`, Werkzeug, ISIN, Termine), Schlüssel nur im Kopf, Kosten 7 von 16 Cent im Text
- Maus prüft: Zustimmung nur bei `true`, nicht bei `"ja"`; Form der Anfrage
- Kosten über der Reservierung sperren beide KIs; zu viele Suchen sperren
- Antwort fehlt nach dem Senden oder ohne Verbrauch: Sperre; nicht gesendet oder abgelehnt: keine Sperre, Schlüssel in der Meldung bereinigt
- unpassende Antworten (fremde ISIN, Bruch, über 10.000, doppelt, falscher Typ, doppeltes Feld, fehlende Einschätzung) ergeben keinen Vorschlag
- ohne Schlüssel oder mit Sperrdatei nicht angebunden; Schlüssel ohne Textform
- ganzer Papiertag mit beiden KIs: Vorschlag, Zustimmung, Kauf, jede Antwort mit Kosten im Journal

Dazu `voller_monat_fragt_niemanden` in `crates/depot_sim/tests/papiertag.rs`: nach 23 Papiertagen mit je 2 Aufrufen je KI fragt der 24. Tag niemanden. Angepasst an die neuen Werte: die Budgettests im Journal, Regelwerk-Tests mit 4 Werten, 40 % und 100 % Umschlag, die exakte Anteilsprüfung bei 42 %, das Beispielszenario (Alpha jetzt 50 %, gekürzt auf 40 %).
