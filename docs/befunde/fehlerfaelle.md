# Fehlerfälle alltagstauglich

Maus am 05.11.2026 auf Wolfgangs Frage, wie DepotSchlopf mit falschen Eingaben, KI-Limits und fehlender Konfiguration umgeht. Gewünscht waren:

- ein Schlüsselstatus (fehlt, hinterlegt, geprüft, abgelehnt),
- klare Meldungen bei Anbieterlimits mit dem nächsten Versuch,
- ein Gesamtstatus (bereit, ohne KI, pausiert, Eingriff nötig),
- eine geführte Aufhebung von `ki.halt` ohne Dateien von Hand,
- ein Budget mit verbraucht, reserviert und verfügbar.

Sicherheitsregeln und Kostengrenzen bleiben unverändert. Die Aufhebung der Sperre umgeht die Kostenprüfung nicht. Maus' Ergänzung desselben Abends: Der Betrieb von selbst beginnt erst, wenn Wolfgang ihn bewusst einschaltet.

## Anbieterprotokoll `daten/anbieter.txt`

Jeder KI-Aufruf von Dachs und Maus und jeder Kursabruf hängt eine Zeile an:

```
anbieter-v1;<sekunden>;<wer>;<ergebnis>;<http>;<ist>;<reserviert>;<warten_bis>
```

- **wer:** `kursquelle`, `dachs` oder `maus`.
- **ergebnis:** `ok`, `limit`, `guthaben`, `abgelehnt`, `ueberlastet`, `fehler`, `netz`, `unklar`, `kosten` oder `neu` (Schlüssel neu hinterlegt).
- **Einordnung** (`depot_journal::anbieter::einordnen`):
  - 401 und 403 sind ein abgelehnter Schlüssel.
  - 429 ist ein Limit.
  - 402 oder ein Hinweis auf Guthaben (`insufficient_quota`, `credit balance`, `billing`) ist fehlendes Guthaben.
  - 5xx heißt überlastet.
  - Eine Wartezeit des Anbieters (`retry-after`, höchstens ein Tag) steht als `warten_bis`.
- **Kursquelle:** Der Sammler ordnet seinen Bericht ein (`depot_sammler::einordnen`). Ein Hinweis zum Kontingent ist ein Limit. Ein Fehler zum Schlüssel bei jedem Wert heißt abgelehnt.
- **Lesen:** Eine Zeile wird nur gelesen, wenn sie beim Neuschreiben genau gleich herauskommt. Unlesbare Zeilen zählt die Anzeige nicht mit, meldet sie aber.

Das Protokoll ist nur Anzeige. Der Kostenschutz entscheidet weiter allein aus dem Kassenbuch im Journal. Ein Schreibfehler am Protokoll ändert keinen Aufruf. Es enthält weder Schlüssel noch Antworttexte.

## Schlüsselstatus ohne Testaufrufe

Der Stand kommt aus der Anmeldeinformationsverwaltung und aus dem letzten Aufruf, der etwas über den Schlüssel sagt:

| Stand | Bedeutung |
|---|---|
| fehlt | Kein Schlüssel hinterlegt. |
| hinterlegt, noch nicht benutzt | Seither kein Aufruf. Wer einen Schlüssel neu speichert, kommt wieder hierher (`neu`). |
| geprüft | Der letzte Aufruf wurde angenommen, auch wenn er an einem Limit, am Guthaben oder an den Kosten scheiterte. |
| abgelehnt | Der Anbieter lehnte den Schlüssel ab. Der Gesamtstatus wird „Eingriff nötig“, mit dem Weg zur Einrichtung. |

Geprüft wird nur bei echten Läufen. Eigene Testaufrufe würden Geld kosten und das Budget belasten.

## Limits und der nächste Versuch

Es gibt keinen automatischen neuen Versuch. Jeder Aufruf ist vorher im Kassenbuch reserviert, und ein Wiederholen würde neu reservieren. Der nächste Versuch ist der nächste Papiertag.

- Nennt der Anbieter eine Wartezeit, steht dort „frühestens …, sonst beim nächsten Papiertag“.
- Ein Limit gilt als aktiv, solange die Wartezeit läuft. Ohne Wartezeit gilt es am selben Tag.
- Fehlendes Guthaben ist ein Eingriff: Wolfgang muss im Konto des Anbieters aufladen.
- Ein erfolgreicher Aufruf löscht den Hinweis. Ein Netzfehler löscht ihn nicht.

## Gesamtstatus

`depot_lage::betrieb::betrieb` ordnet in dieser Reihenfolge:

1. **Eingriff nötig:** ein Punkt mit Ampel Halt, ein abgelehnter Schlüssel oder fehlendes Guthaben.
2. **Pausiert:** Alle KIs warten von selbst (Budget pausiert oder Limit aktiv), und keine ist ohne Schlüssel.
3. **Läuft ohne KI:** Mindestens eine KI fehlt oder wartet. Dazu der Hinweis, dass ohne beide KIs nur die Pflichtaufträge des Regelwerks entstehen.
4. **Vollständig bereit:** sonst.

Der Dienst ergänzt den Zeitplan (`seiten::gesamtstatus`):

- Ein Lesefehler an Zeitplan oder Erledigt ist ein Eingriff.
- Ein ausgeschalteter Zeitplan macht aus „bereit“ ein „pausiert“: Es läuft nur, was von Hand gestartet wird.

Der Gesamtstatus steht oben auf Live und in der Einrichtung.

## Betrieb erst nach bewusstem Einschalten

Ohne `daten/zeitplan.txt` ist der Zeitplan jetzt **aus**; bisher war er an. Ein erster, neugieriger Start von `DepotSchlopf.cmd` startet also nichts von selbst und löst keinen KI-Aufruf aus. Der Zeitplan läuft erst, wenn Wolfgang ihn in der Einrichtung einschaltet und speichert. Ein gespeicherter Zeitplan gilt unverändert weiter.

## Sperre des Kostenschutzes geführt aufheben

Der Berater schreibt `daten/ki.halt` in drei Fällen: Ein Aufruf hat mehr gekostet als reserviert, seine Kosten sind unbekannt, oder die Antwort fehlt.

- **Lagebild:** Es zeigt den Punkt „KI-Kostenschutz“ mit der ersten Zeile des Grundes und einem Verweis „Zur Prüfung“.
- **Einrichtung:** Ein eigener Abschnitt zeigt den ganzen Inhalt (maskiert) und sagt, wie zu prüfen ist. Aufgehoben wird mit einem Knopf, aber nur, wenn alle Bedingungen gelten:
  - Der Haken „Ich habe die Kosten beim Anbieter geprüft“ ist gesetzt.
  - Der Abdruck der gesehenen Fassung stimmt noch. Hat sich die Sperre seither geändert, wird nichts angefasst.
  - Kein Auftrag läuft oder wartet. Die Prüfung läuft unter der Sperre des Dienstzustands, so startet währenddessen keiner.
  - Das Formular trägt die eigene Herkunft und das Token dieses Starts, wie jeder POST.
- **Ablauf:** Erst wird der Grund mit Zeit in `daten/ki_halt_aufgehoben.txt` festgehalten und synchronisiert. Dann wird die Datei entfernt.
- **Was unverändert bleibt:** Kassenbuch und Journal fasst das Aufheben nicht an. Die Reservierung bleibt gebucht und zählt im Budget. Jeder weitere Aufruf durchläuft dieselbe Kostenprüfung und sperrt bei einer Abweichung erneut.

## Budget: verbraucht, reserviert, verfügbar

Die Budgettabelle zeigt je KI drei Werte für den Monat:

- **Verbraucht:** aus dem Anbieterprotokoll, also den vom Anbieter gemeldeten Kosten. Aufrufe mit unbekannten Kosten werden extra gezählt.
- **Reserviert:** aus dem Kassenbuch, die Höchstkosten der gebuchten Aufrufe.
- **Verfügbar:** die Grenze minus das Reservierte.

Der Status (bereit oder pausiert) kommt weiter aus `depot_ki::pruefe`.

## Tests

- **`crates/depot_lage/tests/betrieb.rs`:**
  - Der Schlüsselstand folgt dem letzten aussagekräftigen Aufruf. Netz zählt nicht, ein neu hinterlegter Schlüssel ist wieder nur hinterlegt.
  - Ein Limit nennt den nächsten Versuch und läuft am Tag oder mit der Wartezeit ab.
  - Gesamtstatus in allen vier Fällen.
  - Verbrauch nur aus diesem Monat und von dieser KI, reserviert unverändert aus dem Kassenbuch.
  - `ki.halt` führt zur Prüfung.
  - Ein abgelehnter Schlüssel und ein Limit erscheinen im Lagebild.
- **`crates/depot_berater/tests/berater.rs`:** Jeder Aufruf steht im Protokoll, auch Ablehnungen mit Wartezeit, und eine fehlende Antwort steht als unklar mit unbekannten Kosten.
- **`crates/depot_sammler/tests/sammler.rs`:** Einordnung des Berichts, auch ein ungültiger Schlüssel.
- **`crates/depot_dienst/tests/dienst.rs`:**
  - Ein neuer Schlüssel steht als `neu` im Protokoll (ohne den Schlüssel), und die Einrichtung zeigt ihn als hinterlegt statt abgelehnt.
  - Der Gesamtstatus ergänzt den Zeitplan.
  - Die Sperre wird nur bestätigt, nur in gesehener Fassung, nur ohne laufenden oder wartenden Auftrag und nur mit Token aufgehoben. Danach ist der Grund festgehalten und das Journal unberührt.
  - Ein Erststart startet nichts.

**Gegenproben** (jeweils rot):

- Aufheben ohne Abdruckvergleich, ohne Belegt-Prüfung oder ohne Bestätigung.
- Neuer Schlüssel ohne Eintrag `neu`.
- `neu` setzt den Stand nicht zurück.
- „Pausiert“ schon bei einer KI.
- Ein Limit ohne Wartezeit läuft nicht ab.
- Ein Zeitplanfehler ist kein Eingriff.

## Grenzen und nächster Schritt

- Der Schlüsselstatus ist so frisch wie der letzte echte Aufruf.
- Maus' weitere Punkte zur Fehlbedienung folgen als eigener Schritt:
  - Eingaben bei Fehlern erhalten.
  - Doppelklicks und Neuladen ohne doppelte Aufträge.
  - Nach Unterbrechungen klar zeigen, was erledigt und was offen ist.
  - Einschalten erst nach vollständig geprüfter Einrichtung.
