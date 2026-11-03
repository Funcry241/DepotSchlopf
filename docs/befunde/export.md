# Schnitt Export-Schutz

Stufe 2, auf Wolfgangs Wunsch vom 03.11.2026 vorgezogen. Zugangsdaten für Bank, Broker und KI-Anbieter dürfen nie in Git landen und nie im Assistenten-Export. Der Schnitt setzt R28 als Schutzrahmen um: Geheimnisse liegen nie in Dateien des Projekts, und jeder Inhalt wird darauf geprüft.

Der Kodex verlangt, dass eine Änderung am Gate nie zusammen mit Produktivcode kommt. Der Schnitt hat deshalb zwei Teile in zwei Pull Requests:

1. **Teil Gate** (Klasse `KodexAenderung`): Der Kodex prüft jede Datei auf Geheimnismuster. Dieser Teil.
2. **Teil Export** (Klasse `Funktion`): Der Export nimmt nur von Git verfolgte Dateien, folgt keinem Link und bricht bei einem Geheimnismuster ab. Folgt nach Teil 1.

Auf GitHub sind Secret Protection und Push Protection laut Wolfgang aktiv. Sie ergänzen die Prüfung, ersetzen sie aber nicht: Sie kennen keine IBAN und greifen erst beim Push.

Ein grüner Test schließt keinen Befund. „Technisch geschlossen“ setzt erst die unabhängige Prüfung durch Maus.

## So prüfst du es nach

```text
cargo test --locked -p depot_kodex --test befunde befund_2_w3
cargo test --locked -p depot_kodex --lib geheimnis
```

Am ersten Commit (`29bf4dd`, nur Test) ist `befund_2_w3_geheimnis_in_projektdatei_wird_abgelehnt` mit `-- --ignored` rot: Das Gate lässt alle sieben Fälle zu.

## Teil Gate: Ergebnis

| Fall | am alten Stand | nach dem Umbau |
| --- | --- | --- |
| API-Schlüssel in der Doku | angenommen | abgelehnt |
| IBAN in Vierergruppen in der Doku | angenommen | abgelehnt |
| privater Schlüssel (PEM) in der Doku | angenommen | abgelehnt |
| Passwort-Zuweisung in der Doku | angenommen | abgelehnt |
| IBAN in einem Kommentar im Produktivcode | angenommen | abgelehnt |
| API-Schlüssel in einem Test | angenommen | abgelehnt |
| API-Schlüssel, geprüft ohne Git | angenommen | abgelehnt |

Jede Ablehnung muss das Stichwort „Geheimnismuster“ nennen. Die Meldung nennt Datei, Zeile und Art, nie das Geheimnis selbst; auch das prüft der Test. Vorher besteht dieselbe Kopie mit einer harmlosen Änderung das Gate.

## Was der Umbau tut

- **Was geprüft wird.** Jede Datei, die ein Commit mitnehmen könnte: alles, was Git verfolgt, und jede neue, nicht ignorierte Datei, gleich welcher Rolle und ob deklariert. Ohne Git jede Datei außer `target/`. Ein Link wird nicht verfolgt. Ein Lesefehler ist ein Fehler (R29).
- **Was als Geheimnis gilt.** Private Schlüssel im PEM-Format. Zugangsschlüssel mit bekanntem Präfix (Anthropic, OpenAI, GitHub, AWS, Slack, GitLab) und genug Zeichen dahinter. IBANs mit gültiger Prüfziffer, auch in Vierergruppen. Zuweisungen an Namen wie `password`, `secret` oder `api_key` mit einem Wert, der nach Schlüssel aussieht.
- **Keine Ausnahmeliste.** Beispiele in Tests entstehen erst zur Laufzeit. Der heutige Stand des Projekts besteht die Prüfung ohne Ausnahme.
- **Was nicht anschlägt.** ISINs, Hashes, die Lehrbuch-IBAN mit falscher Prüfziffer, Code wie `password = std::env::var(..)`. Das prüft `harmloses_bleibt_harmlos`.

## Gegenproben

| Gegenprobe | Ergebnis |
| --- | --- |
| PEM-Schlüssel nicht erkannt | W3 rot |
| Präfix-Schlüssel nicht erkannt | W3 rot |
| IBAN nicht erkannt | W3 rot |
| IBAN nur ohne Leerzeichen erkannt | W3 rot |
| Zuweisung nicht erkannt | W3 rot |
| Gate ohne Geheimnisprüfung (mit Git) | W3 rot |
| nur verfolgte Dateien geprüft, neue nicht | W3 rot |

## Grenzen

- Ein Muster erkennt nie jedes Geheimnis. Die Prüfung ist das Netz unter der Regel, nicht die Regel: Zugangsdaten liegen in der Windows-Anmeldeverwaltung, nie im Projektordner.
- Eine Kontonummer ohne IBAN-Form oder ein Schlüssel ohne bekanntes Präfix und ohne Namen davor wird nicht erkannt.
- Teil Export (1·E01, 1·E02, Export-Teil von 2·W3) folgt im nächsten Pull Request.
