# Dachs und Maus im Papierhandel

Wolfgang am 05.11.2026: mutiger und schneller, täglicher Papierhandel, Dachs und Maus entscheiden aus Kursen und Nachrichten. KI-Kosten höchstens 20 € im Monat je KI, Denktiefe mittel, Maus mit GPT-6.1 Terra (alles Wolfgang, 05.11.2026). Netzzugang für die Berater mit Websuche: „Ja, mit Websuche.“ Die Kodex-Änderung dazu steht in PR #24 (`NETZ_ERLAUBT` um `depot_berater` erweitert).

## Was gebaut ist

- **Neues Crate `depot_berater`** mit Netz, ein Blatt im Abhängigkeitsbaum: Es hängt von `depot_sim` ab, nie umgekehrt. `run.cmd papier` ruft jetzt sein Programm; der Papiertag selbst liegt als `depot_sim::papierlauf::papier` in der Bibliothek.
- **Anbieter:** Dachs fragt Claude Sonnet 5.5 (`claude-sonnet-5-5`, Messages API, adaptives Denken, Aufwand mittel). Maus fragt GPT-6.1 Terra (`gpt-6.1-terra`, Responses API, Denken mittel, nichts gespeichert). Die Antwortgrenze `max_token_aus` (Denken eingeschlossen) kommt aus dem Deckel der Erlaubnis. **Keine Werkzeuge des Anbieters, auch keine Websuche** (K25-01, siehe unten).
- **Gesendet** werden nur Papierdepot des Topfs, Schlusskurse und belegte Termine des Tages, die Positivliste mit Namen und die Grenzen. Keine Schlüssel im Inhalt, keine Kontodaten.
- **Schlüssel** nur in der Windows-Anmeldeinformationsverwaltung (`DepotSchlopf/anthropic`, `DepotSchlopf/openai`), nur im Kopf der Anfrage. `Debug` zeigt `***`. Gültig sind 20 bis 256 Zeichen aus Buchstaben, Ziffern, `-` und `_`; so kann er den Kopf nicht verändern. Ohne Schlüssel ist ein Berater nicht angebunden und wird nicht gefragt.
- **Geheimnisschutz für jede Antwort** (K25-02): Enthält eine Antwort den Schlüssel, roh oder im JSON des Modells verschlüsselt, wird sie ganz verworfen; geprüft werden die Rohantwort und jeder Text des fertigen Ergebnisses (Einschätzung, Begründungen, Urteil). Fehlermeldungen werden vom Schlüssel bereinigt.
- **Antworten streng gelesen:** Das erste JSON-Objekt der Antwort, eigener Leser ohne fremde Bibliothek, feste Tiefe. Ein Vorschlag braucht `einschaetzung` und `ziele` (Liste oder `null`), jedes Ziel eine ISIN der Positivliste, `gewicht_bp` als ganze Zahl von 0 bis 10.000 und eine Begründung. Doppelte Felder oder Werte, fremde ISINs, Brüche: Die ganze Antwort wird verworfen, nichts geraten. Grenzen (Werte, Anteil, Summe) prüft danach das Regelwerk. Eine Zweitmeinung ist nur mit `"zustimmung": true` Zustimmung.
- **Gesicherte Obergrenze vor dem Aufruf** (K25-01): Die ganze Anfrage (Anweisung, Frage, Rahmen) darf höchstens `max_token_ein` Bytes haben, sonst wird sie nicht gesendet. Ein Token umfasst mindestens ein Byte, die wenigen Rahmenzeichen des Anbieters sind durch die JSON-Zeichen der Anfrage mehr als gedeckt, und ohne Werkzeuge fügt der Anbieter nichts ein. Die Antwort ist auf `max_token_aus` begrenzt. Damit sind die gebuchten Höchstkosten eine Obergrenze, die vor dem Aufruf feststeht.
- **Kosten nach jedem Aufruf** als zweite Sicherung: Aus dem gemeldeten Verbrauch rechnet `KiDeckel::kosten_ist` die Kosten, aufgerundet, und schreibt sie mit der Reservierung zu jeder Einschätzung und jedem Urteil ins Journal.
- **Sperre `daten/ki.halt`:** Liegen die Kosten über der Reservierung, meldet der Anbieter eine Suche oder mehr Eingabe- oder Ausgabe-Token als erlaubt, fehlt die Antwort nach dem Senden (Zeitgrenze, Abbruch) oder nennt sie keinen Verbrauch, schreibt der Berater die Sperrdatei mit dem Grund und verwirft die Antwort. Solange sie liegt, ist keine KI angebunden. Wolfgang löscht sie nach der Prüfung. Eine Ablehnung des Anbieters (Status ungleich 200) und ein sicher nicht gesendeter Aufruf sperren nicht.
- **Netz:** nur `POST`, nur HTTPS, keine Weiterleitung, Zeitgrenze 300 Sekunden, Antwort höchstens 4 MB.

## K25-01: Warum keine Websuche des Anbieters

Maus: Die Reservierung rechnete mit höchstens 100.000 Eingabe-Token, die Suchergebnisse des Anbieters können darüber hinausgehen, und gesperrt wurde erst nach der Antwort. Keiner der beiden Anbieter bietet eine harte Grenze für das, was eine Websuche in die Eingabe einfügt; schlimmstenfalls wäre das ganze Kontextfenster je Durchgang. Eine gesicherte Obergrenze gibt es also nur ohne Werkzeuge. Nachrichten sollen deshalb wie Kurse und Termine über den Sammler kommen und im Journal belegt werden (Entscheidung bei Wolfgang). Bis dahin entscheiden Dachs und Maus aus Kursen, Terminen und eigenem Wissen.

## Neue Festwerte (Festschreibung, Grenzfreigabe nötig)

| | bisher | neu |
|---|---|---|
| Dachs je Aufruf | 40.000 / 16.000 Token, 4 $ / 20 $ je Mio. | 40.000 / 16.000 Token (mit Denken), 2 $ / 10 $ je Mio. (Sonnet 5.5) |
| Maus je Aufruf | 40.000 / 10.000 Token, 10 $ / 50 $ je Mio. | 40.000 / 16.000 Token (mit Denken), 1 $ / 5 $ je Mio. (GPT-6.1 Terra) |
| Höchstkosten je Aufruf | 48 / 90 Cent | 24 / 12 Cent |
| je KI im Monat | 4 Aufrufe, 4 € | 46 Aufrufe, 20 € |
| je KI im Jahr | 50 € | 240 € |
| Werte je Topf | 8 | 4 |
| Anteil je Wert | 25 % | 40 % |
| Umschlag am Tag | 50 % | 100 % |
| ETFs | gesperrt | frei |

Je KI höchstens 20 € im Monat (Wolfgang); schlimmstenfalls bucht Dachs 46 × 24 Cent = 11,04 €, Maus 46 × 12 Cent = 5,52 €. 46 Aufrufe reichen für 23 Handelstage mit je einem Vorschlag und einer Zweitmeinung je KI. Dollar zählen als Euro; das überschätzt. Modell-ID, Preise und Parameter von GPT-6.1 Terra hat Maus bei OpenAI bestätigt.

## Tests

`crates/depot_berater/tests/berater.rs`, ohne Netz mit erfundenem Anbieter:

- Dachs schlägt vor: Form der Anfrage (Modell, `max_tokens`, Denken, keine Werkzeuge, ISIN, Termine), Schlüssel nur im Kopf, Kosten 6 von 24 Cent im Text
- Maus prüft: Kosten 2 von 12 Cent, Zustimmung nur bei `true`, nicht bei `"ja"`; Form der Anfrage ohne Werkzeuge
- Kosten über der Reservierung sperren beide KIs; jede Suche sperrt
- Maus' Gegenproben: `reservierung_ist_eine_gesicherte_obergrenze` (Anfrage über dem Deckel wird nicht gesendet, mehr gemeldete Eingabe-Token sperren) und `zurueckgegebener_schluessel_gelangt_nie_ins_ergebnis` (roh und verschlüsselt, in Einschätzung, Begründung und Urteil)
- Antwort fehlt nach dem Senden oder ohne Verbrauch: Sperre; nicht gesendet oder abgelehnt: keine Sperre, Schlüssel in der Meldung bereinigt
- unpassende Antworten (fremde ISIN, Bruch, über 10.000, doppelt, falscher Typ, doppeltes Feld, fehlende Einschätzung) ergeben keinen Vorschlag
- ohne Schlüssel oder mit Sperrdatei nicht angebunden; Schlüssel ohne Textform
- ganzer Papiertag mit beiden KIs: Vorschlag, Zustimmung, Kauf, jede Antwort mit Kosten im Journal

Dazu `voller_monat_fragt_niemanden` in `crates/depot_sim/tests/papiertag.rs`: nach 23 Papiertagen mit je 2 Aufrufen je KI fragt der 24. Tag niemanden. Angepasst an die neuen Werte: die Budgettests im Journal, Regelwerk-Tests mit 4 Werten, 40 % und 100 % Umschlag, die exakte Anteilsprüfung bei 42 %, das Beispielszenario (Alpha jetzt 50 %, gekürzt auf 40 %).

## 08.11.2026: bestätigte Kosten und begrenzte Abstimmung

[KI-Abrechnung und Abstimmung](ki-dialog.md) beschreibt den aktuellen Ablauf einschließlich Tagesrahmen, 30/690 Aufrufen bei unveränderten Eurogrenzen, kurzem Ausgabedeckel, Journalbelegen und den neuen Gegenproben. Frühere Aussagen über ausschließlich gebuchte Höchstkosten sind durch die bestätigte Minderkostenabrechnung ergänzt.
