# Schnitt Export-Schutz

Stufe 2, auf Wolfgangs Wunsch vom 03.11.2026 vorgezogen. Zugangsdaten für Bank, Broker und KI-Anbieter dürfen nie in Git landen und nie im Assistenten-Export. Der Schnitt setzt R28 als Schutzrahmen um: Geheimnisse liegen nie in Dateien des Projekts, und jeder Inhalt wird darauf geprüft.

Der Kodex verlangt, dass eine Änderung am Gate nie zusammen mit Produktivcode kommt. Der Schnitt hat deshalb zwei Teile in zwei Pull Requests:

1. **Teil Gate** (Klasse `KodexAenderung`): Der Kodex prüft jede Datei auf Geheimnismuster. Dieser Teil.
2. **Teil Export** (Klasse `Funktion`): Der Export nimmt nur von Git verfolgte Dateien, folgt keinem Link und bricht bei einem Geheimnismuster ab. Teil 1 ist von Maus abgenommen und gemergt (PR #10).

Auf GitHub sind Secret Protection und Push Protection laut Wolfgang aktiv. Sie ergänzen die Prüfung, ersetzen sie aber nicht: Sie kennen keine IBAN und greifen erst beim Push.

**Fassung 2 nach Maus' Prüfung von `f075092` (K10-01, K10-02):** Maus' Gegenproben stehen als zwei Tests in `crates/depot_kodex/tests/befunde.rs`, am Stand `f075092` rot (eigener Commit `7890c60`). Behoben ist allgemein:

- **K10-01 (P1):** Ein Commit nimmt den Inhalt aus dem Index, nicht aus dem Arbeitsbaum. Das Gate prüft jetzt beide Stände: jede Datei im Arbeitsbaum und jeden Blob im Index. Meldungen zum Index tragen den Zusatz „(Index)“.
- **K10-02 (P1):** Inhalte werden ohne Verlust gelesen: UTF-8 (mit oder ohne BOM) und UTF-16 mit BOM. Enthält der Text Nullzeichen, etwa bei UTF-16 ohne BOM, wird er zusätzlich ohne sie geprüft. Was sich so nicht lesen lässt, ist „nicht prüfbar“ und schließt das Gate. Ungeprüfte Inhalte gelten nie als geheimnisfrei.

Ein grüner Test schließt keinen Befund. „Technisch geschlossen“ setzt erst die unabhängige Prüfung durch Maus.

## So prüfst du es nach

```text
cargo test --locked -p depot_kodex --test befunde befund_2_w3
cargo test --locked -p depot_kodex --test befunde befund_k10
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

| K10-Fall (`befund_k10_01_…`, `befund_k10_02_…`) | an `f075092` | nach dem Umbau |
| --- | --- | --- |
| Schlüssel nur im Index, harmloser Text im Arbeitsbaum | angenommen | abgelehnt |
| Schlüssel nur im Arbeitsbaum, harmloser Text im Index | abgelehnt | abgelehnt |
| UTF-16LE mit BOM (mit Umlaut) | angenommen | abgelehnt |
| UTF-16BE mit BOM (mit Umlaut) | angenommen | abgelehnt |
| UTF-16LE ohne BOM | angenommen | abgelehnt |
| UTF-16BE ohne BOM | angenommen | abgelehnt |
| nicht lesbare Kodierung | angenommen | abgelehnt als „nicht prüfbar“ |

Jede Ablehnung muss das Stichwort „Geheimnismuster“ nennen, die nicht lesbare Kodierung „nicht pruefbar“. Die Meldung nennt Datei, Zeile und Art, nie das Geheimnis selbst; auch das prüft der Test. Vorher besteht dieselbe Kopie mit einer harmlosen Änderung das Gate.

## Was der Umbau tut

- **Was geprüft wird.** Jede Datei, die ein Commit mitnehmen könnte: alles, was Git verfolgt, und jede neue, nicht ignorierte Datei, gleich welcher Rolle und ob deklariert, und zwar im Arbeitsbaum und im Index. Ohne Git jede Datei außer `target/`. Ein Link wird nicht verfolgt. Ein Lesefehler ist ein Fehler (R29).
- **Wie gelesen wird.** UTF-8 und UTF-16 mit BOM, ohne Verlust; Text mit Nullzeichen zusätzlich ohne sie. Alles andere ist nicht prüfbar und ein Fehler. Heute sind alle Dateien des Projekts UTF-8.
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
| Index nicht geprüft | K10-01 rot |
| verlustbehaftetes Lesen statt „nicht prüfbar“ | K10-02 rot |
| UTF-16 mit BOM nicht als UTF-16 gelesen | K10-02 rot |
| Text mit Nullzeichen nicht zusätzlich ohne sie geprüft | K10-02 rot |

## Grenzen

- Bilder oder andere Binärdateien wären heute „nicht prüfbar“ und würden das Gate schließen. Das Projekt hat keine; braucht es später welche, bekommen sie eine eigene, ausdrückliche Regel.
- Ein Muster erkennt nie jedes Geheimnis. Die Prüfung ist das Netz unter der Regel, nicht die Regel: Zugangsdaten liegen in der Windows-Anmeldeverwaltung, nie im Projektordner.
- Eine Kontonummer ohne IBAN-Form oder ein Schlüssel ohne bekanntes Präfix und ohne Namen davor wird nicht erkannt.

## Teil Export: Ergebnis

**Fassung 2 nach Maus' Prüfung von `9d94866` (K11-01, K11-02):** Maus' Gegenproben stehen als Tests in `crates/depot_lauf/src/export.rs`, am Stand `9d94866` rot (eigener Commit `a214d1d`). Behoben ist allgemein:

- **K11-01 (hoch):** Der Export hat genau einen Leseweg für Dateien im Projekt, `lies_im_projekt`. Er prüft jeden Teil des Pfads, nicht nur den letzten. Kein Teil darf ein Link sein, unter Windows auch kein Analysepunkt (Junction). Zusätzlich muss der aufgelöste Pfad genau der erwartete Pfad unter der aufgelösten Projektwurzel sein. Der Weg wird vor und nach dem Lesen geprüft, und die gelesene Datei muss die geprüfte sein; ändert sich etwas dazwischen, bricht der Export ab. Auch die Essenz wird nur so gelesen.
- **K11-02 (mittel):** „Ohne Git“ heißt nur: Es gibt kein `.git`. Ist eines da, muss der Index lesbar sein, sonst bricht der Export ab, bevor eine Datei entsteht. Ein Fehler beim Lesen der Metadaten ist ein Fehler, keine ausgelassene Datei. Ausgelassen wird nur, was im Arbeitsbaum fehlt, keine gewöhnliche Datei ist oder hinter einem Link liegt; diese Pfade stehen mit Grund in `SOURCE_SNAPSHOT.txt`.

Die Negativtests stehen in `crates/depot_lauf/src/export.rs` und sind am alten Stand rot (eigener Commit `3c8899a`). Der Fall „verfolgte Datei im Arbeitsbaum durch einen Link ersetzt“ kam mit dem Umbau-Commit dazu; am alten Stand wäre auch er rot, weil der alte Export jedem Pfad folgte.

```text
cargo test --locked -p depot_lauf befund_1_e0
cargo test --locked -p depot_lauf befund_k11
cargo test --locked -p depot_lauf metadatenfehler
cargo test --locked -p depot_lauf sauberer_export_entsteht
cargo test --locked -p depot_lauf ohne_git
```

| Fall | am alten Stand | nach dem Umbau |
| --- | --- | --- |
| unverfolgte Datei im Projekt | exportiert | nicht exportiert |
| verfolgter Link | gelesen | nicht exportiert |
| verfolgte Datei, im Arbeitsbaum durch Link ersetzt | Linkziel gelesen | nicht exportiert |
| IBAN in einer Datei | exportiert | Abbruch |
| API-Schlüssel in einer Datei | exportiert | Abbruch |
| API-Schlüssel in der Konsole | exportiert | Abbruch |
| nicht lesbarer Inhalt | exportiert | Abbruch als „nicht pruefbar“ |
| sauberer Stand (Kontrolle) | ZIP entsteht | ZIP entsteht |

| K11-Fall | an `9d94866` | nach dem Umbau |
| --- | --- | --- |
| Elternordner einer verfolgten Datei durch Link ersetzt | fremder Inhalt in der ZIP-Datei | nicht gelesen, ausgelassen |
| Ordner der Essenz durch Link ersetzt | fremde Regel in der Übergabe | nicht gelesen |
| `.git/index` beschädigt | leerer Quellstand, ZIP entsteht | Abbruch, keine ZIP-Datei |
| Metadatenfehler (zu langer Name) und Pfade mit `..`, `.` oder leerem Teil | – | Fehler |
| ohne `.git` (Kontrolle) | ZIP ohne Quellstand | ZIP ohne Quellstand |

Bei jedem Abbruch prüft der Test: die Meldung nennt „Geheimnismuster“ oder „nicht pruefbar“, das Geheimnis steht nicht darin, und es gibt weder ZIP-Datei noch Zwischendatei.

## Was der Export-Umbau tut

- **Quelle.** Die Liste der Dateien kommt aus dem Git-Index (`git ls-files --stage`, nur normale Dateien). Gelesen wird jede Datei über `lies_im_projekt` (siehe K11-01). Ohne `.git` gibt es keinen Quellstand; der Export enthält dann nur Bericht und Konsole. Ein vorhandenes, aber nicht lesbares Repository bricht ab.
- **Prüfung.** Jede Quelldatei wird vor dem Schreiben mit derselben Prüfung wie im Gate gelesen (`depot_kodex::geheimnis::pruefe_inhalt`). Ebenso jeder andere Teil der ZIP-Datei, auch die Konsole und der Bericht. Die innere Quell-ZIP enthält nur bereits geprüfte Dateien.
- **Abbruch.** Ein Treffer oder ein nicht prüfbarer Inhalt bricht ab, bevor eine Datei geschrieben wird. Die Meldung nennt Teil, Zeile und Art.

## Gegenproben Teil Export

| Gegenprobe | Ergebnis |
| --- | --- |
| auch unverfolgte Dateien exportiert | E01 rot |
| Link im Arbeitsbaum gelesen | E02 rot |
| Quelldateien nicht geprüft | E01-Geheimnis rot |
| Konsole nicht geprüft | E01-Geheimnis rot |
| Essenz direkt gelesen | K11-01 rot |
| Indexfehler als leere Liste | K11-02 rot |
| vorhandenes, kaputtes Repository gilt als „ohne Git“ | K11-02 rot |
| Metadatenfehler als „ausgelassen“ | Metadatentest rot |
| Pfadteil `..` erlaubt | Metadatentest rot |
| Link-Prüfung je Pfadteil, Ordnerprüfung und Auflösungsvergleich zusammen entfernt | K11-01 rot |

Jede der drei Link-Schichten allein hält K11-01 grün: die Link-Prüfung je Pfadteil, die Prüfung, dass jeder Teil vor dem letzten ein echter Ordner ist, und der Vergleich mit dem aufgelösten Pfad. Rot wird der Test erst, wenn alle drei fehlen. Der Auflösungsvergleich ist die Schicht, die unter Windows auch Umleitungen fängt, die sich nicht als Link melden.

## Grenzen Teil Export

- Der Windows-Weg (Analysepunkte, Junctions) ist für Windows gebaut und mit Clippy geprüft, aber nicht unter Windows ausgeführt. Ein Junction-Test fehlt, weil die Prüfumgebung Linux ist. Unter Windows lässt eine abweichende Groß- und Kleinschreibung zwischen Git und Dateisystem eine Datei als „umgeleitet“ aus; sie fehlt dann im Quellstand und steht in `SOURCE_SNAPSHOT.txt`.
- Zwischen der ersten Prüfung und dem Öffnen bleibt ein sehr kurzes Fenster. Wer den Pfad darin umleitet und vor der zweiten Prüfung zurückstellt, wird nicht erkannt. Die Inhaltsprüfung greift trotzdem. Ganz schließen ließe sich das nur mit Systemaufrufen ohne Linkauflösung, die eine neue Abhängigkeit bräuchten.
- Die Festschreibung im Export berechnet der Kodex. Aus ihr gehen nur Prüfsummen und abgeleitete Namen in den Export, keine Dateiinhalte, und auch sie werden auf Geheimnismuster geprüft.
- Ein Fehler beim Lesen der Rechte (etwa „Zugriff verweigert“) lässt sich in der Prüfumgebung nicht herstellen, weil sie als Administrator läuft. Der Test nutzt deshalb einen zu langen Namen; der Leseweg unterscheidet nur „fehlt“ von allen anderen Fehlern.

- Der Export liest den Arbeitsbaum, nicht den Index. Weicht beides ab, wird der Arbeitsbaum exportiert und geprüft. Dass der Export byte-gleich dem geprüften Stand ist, bleibt 1·R04 (Snapshot) und folgt mit einem eigenen Schnitt.
- Für die Muster gelten dieselben Grenzen wie im Gate.
