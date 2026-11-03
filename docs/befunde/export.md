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

**Fassung 3 nach Maus' Nachprüfung von `c592b5b` (K11-03):** Unter Windows hielt der Leseweg zwei gleich lange Dateien für dieselbe. Eine echte Dateiidentität gibt es unter Windows in der stabilen Standardbibliothek nicht, und `unsafe` sowie neue Abhängigkeiten sind gesperrt. Statt den Arbeitsbaum noch genauer zu bewachen, liest der Export ihn jetzt gar nicht mehr. Behoben ist allgemein:

- **K11-03 (mittel):** Jeder Inhalt des Quellstands und die Essenz kommen aus dem Git-Index (`git cat-file blob`), nie aus dem Arbeitsbaum. Ein Blob ist durch seine Prüfsumme bestimmt; es gibt keinen Pfad, der sich beim Lesen ändern könnte, und keine Datei, deren Identität man nachweisen müsste. Das gilt auf jeder Plattform. Ein Blob, der sich nicht lesen lässt, bricht den Export ab. Der Export enthält damit denselben Stand, den das Gate im Index prüft und den ein Commit aufnimmt.
- **K11-01** ist dadurch ohne eigenen Linkweg erfüllt: Im Arbeitsbaum wird nichts gelesen, also auch kein Link in irgendeinem Pfadteil und keine Junction. Verfolgte Links (Modus 120000) fehlen im Index-Auszug. Der Leseweg `lies_im_projekt` aus Fassung 2 ist entfallen.
- **K11-02** bleibt: „Ohne Git“ heißt nur, es gibt kein `.git`. Ist eines da, muss der Index lesbar sein, sonst Abbruch ohne ZIP-Datei.

Maus' Gegenprobe zu K11-03 baute Eingriffspunkte in `lies_im_projekt` ein. Diese Funktion gibt es nicht mehr. Die neuen Tests prüfen die allgemeine Regel und sind an `c592b5b` rot (eigener Commit `ce6e891`): Eine gleich lange, andere Datei im Arbeitsbaum und eine geänderte Essenz dürfen nicht in die ZIP-Datei gelangen, und ein fehlender Blob bricht ab. Ein Austausch während des Lesens ist der Sonderfall „Arbeitsbaum weicht vom Index ab“ und wird damit mit abgedeckt.

**Fassung 2 (K11-01, K11-02),** Tests in Commit `a214d1d`, an `9d94866` rot: Link im Elternordner (auch für die Essenz) und beschädigter Index.

Die ersten Negativtests (1·E01, 1·E02, 2·W3) stehen im Commit `3c8899a` und sind am alten Stand rot. Der Fall „verfolgte Datei im Arbeitsbaum durch einen Link ersetzt“ kam mit dem Umbau-Commit dazu.

```text
cargo test --locked -p depot_lauf befund_1_e0
cargo test --locked -p depot_lauf befund_k11
cargo test --locked -p depot_lauf sauberer_export_entsteht
cargo test --locked -p depot_lauf ohne_git
```

| Fall | am alten Stand | nach dem Umbau |
| --- | --- | --- |
| unverfolgte Datei im Projekt | exportiert | nicht exportiert |
| verfolgter Link | gelesen | nicht exportiert |
| verfolgte Datei, im Arbeitsbaum durch Link ersetzt | Linkziel gelesen | Inhalt aus dem Index, Linkziel nicht gelesen |
| IBAN in einer Datei | exportiert | Abbruch |
| API-Schlüssel in einer Datei | exportiert | Abbruch |
| API-Schlüssel in der Konsole | exportiert | Abbruch |
| nicht lesbarer Inhalt | exportiert | Abbruch als „nicht pruefbar“ |
| sauberer Stand (Kontrolle) | ZIP entsteht | ZIP entsteht |

| K11-Fall | am jeweils geprüften Stand | nach dem Umbau |
| --- | --- | --- |
| Elternordner einer verfolgten Datei durch Link ersetzt (`9d94866`) | fremder Inhalt in der ZIP-Datei | Inhalt aus dem Index |
| Ordner der Essenz durch Link ersetzt (`9d94866`) | fremde Regel in der Übergabe | Essenz aus dem Index |
| `.git/index` beschädigt (`9d94866`) | leerer Quellstand, ZIP entsteht | Abbruch, keine ZIP-Datei |
| gleich lange, andere Datei im Arbeitsbaum (`c592b5b`) | Arbeitsbaum exportiert | Inhalt aus dem Index |
| geänderte Essenz im Arbeitsbaum (`c592b5b`) | Arbeitsbaum in der Übergabe | Essenz aus dem Index |
| Blob des Index fehlt (`c592b5b`) | Export meldet Erfolg | Abbruch, keine ZIP-Datei |
| ohne `.git` (Kontrolle) | ZIP ohne Quellstand | ZIP ohne Quellstand |

Bei jedem Abbruch prüft der Test: die Meldung nennt „Geheimnismuster“ oder „nicht pruefbar“, das Geheimnis steht nicht darin, und es gibt weder ZIP-Datei noch Zwischendatei.

## Was der Export-Umbau tut

- **Quelle.** Liste und Inhalt kommen aus dem Git-Index (`git ls-files --stage`, nur normale Dateien, dann `git cat-file blob`). Im Arbeitsbaum liest der Export nichts. Ohne `.git` gibt es keinen Quellstand; der Export enthält dann nur Bericht und Konsole. Ein vorhandenes, aber nicht lesbares Repository oder ein fehlender Blob bricht ab.
- **Prüfung.** Jede Quelldatei wird vor dem Schreiben mit derselben Prüfung wie im Gate gelesen (`depot_kodex::geheimnis::pruefe_inhalt`). Ebenso jeder andere Teil der ZIP-Datei, auch die Konsole und der Bericht. Die innere Quell-ZIP enthält nur bereits geprüfte Dateien.
- **Abbruch.** Ein Treffer oder ein nicht prüfbarer Inhalt bricht ab, bevor eine Datei geschrieben wird. Die Meldung nennt Teil, Zeile und Art.

## Gegenproben Teil Export

| Gegenprobe | Ergebnis |
| --- | --- |
| auch unverfolgte Dateien exportiert | E01 rot |
| Quelldateien nicht geprüft | E01-Geheimnis rot |
| Konsole nicht geprüft | E01-Geheimnis rot |
| Quellstand aus dem Arbeitsbaum gelesen | E02, K11-01 und K11-03 rot |
| Essenz aus dem Arbeitsbaum gelesen | K11-01 und K11-03 rot |
| Blobfehler: Datei still ausgelassen | K11-03 (fehlender Blob) rot |
| vorhandenes, kaputtes Repository gilt als „ohne Git“ | K11-02 rot |

## Grenzen Teil Export

- **Vorgemerkt, nicht gespeichert.** Eine Änderung, die nur im Arbeitsbaum liegt und nicht mit `git add` vorgemerkt ist, steht nicht im Export. `SOURCE_SNAPSHOT.txt` sagt das, und „Lokal veraendert“ zeigt, dass es solche Änderungen gibt. Das ist Absicht: Exportiert wird der Stand, den das Gate prüft.
- **Zeitpunkt.** Wird der Index zwischen Gate und Export geändert, exportiert der Export den neuen Index, geprüft auf Geheimnismuster. Dass der Export byte-gleich dem vom Gate geprüften Stand ist, bleibt 1·R04 (Snapshot) und folgt mit einem eigenen Schnitt.
- Die Festschreibung im Export berechnet der Kodex aus dem Arbeitsbaum. Aus ihr gehen nur Prüfsummen und abgeleitete Namen in den Export, keine Dateiinhalte, und auch sie werden auf Geheimnismuster geprüft.
- Kein Test läuft unter Windows. Der Export braucht dort aber keinen eigenen Weg mehr; er ruft nur `git` auf.
- Für die Muster gelten dieselben Grenzen wie im Gate.
