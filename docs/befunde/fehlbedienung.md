# Fehlbedienung abfangen

Maus am 05.11.2026: Das Programm muss mit Wolfgangs Fehlern rechnen, und er soll es bedienen können, ohne seine technischen Eigenheiten zu kennen. Der erste Teil steht in `docs/befunde/fehlerfaelle.md`: Schlüsselstatus, Gesamtstatus, Anbieterpausen, geführte Aufhebung der Sperre, Zeitplan ab Werk aus. Dieser Schritt deckt die übrigen Punkte ab.

## Betrieb erst nach vollständiger Einrichtung

Der Zeitplan lässt sich nur einschalten (von aus auf an), wenn zwei Bedingungen gelten:
- Das Lagebild ist berechnet und aktuell.
- Der Gesamtstatus verlangt keinen Eingriff mehr (`Zustand::einschalten_erlaubt`).

Ausschalten und das Ändern der Uhrzeiten gehen immer. Ein laufender Zeitplan wird nicht abgeschaltet, nur weil später ein Eingriff nötig wird; das zeigt dann der Gesamtstatus. Die Einrichtung sagt neben dem Formular, warum Einschalten gerade nicht geht. Die Ablehnung nennt den Gesamtstatus oben auf der Seite als Ort, an dem steht, was fehlt.

## Eingaben bleiben erhalten

Lehnt der Dienst einen Zeitplan ab, stehen danach die gültigen Eingaben wieder im Formular. Gründe für eine Ablehnung sind eine ungültige Uhrzeit, eine unvollständige Einrichtung, ein noch nicht berechneter Stand oder eine Datei, die sich nicht schreiben lässt.

- Die Weiterleitung trägt nur geprüfte Werte: `entwurf=ja`, die Haken als `ja` und Uhrzeiten im Format `hh:mm`.
- Fremde Werte in der Abfrage werden verworfen, alles ist maskiert.
- Die Seite sagt, dass das Formular nicht gespeicherte Eingaben zeigt.
- Schlüssel erscheinen nie wieder. Ihre Zeilen sind ohnehin je Schlüssel getrennt.

## Doppelklicks und erneutes Absenden

Kein Auftrag startet doppelt. Dafür sorgen drei Stufen:

1. **Warteschlange:** Wie bisher läuft oder wartet dieselbe Art nie zweimal.
2. **Kurz danach:** Ein Auftrag von Hand, der weniger als 30 Sekunden nach dem Ende desselben Auftrags kommt (`DOPPELT_SEKUNDEN`), wird nicht eingereiht. Das fängt einen Doppelklick nach einem schnellen Lauf und ein erneutes Absenden der Seite ab. Die Meldung sagt, dass nichts doppelt gestartet wurde.
3. **Im Browser:** `/app.js` sperrt den Knopf eines abgesendeten Formulars („wird gesendet …“). Nach „Zurück“ gibt es ihn wieder frei.

Doppelte KI-Kosten sind ohnehin ausgeschlossen: Der Papiertag läuft je Handelstag nur einmal. Ein angefangener Tag ist Halt, und jede KI-Frage ist vorher im Kassenbuch reserviert.

## Unterbrechungen

Neu ist `daten/dienst_verlauf.txt`. Jeder Auftrag schreibt beim Start und beim Ende eine Zeile. Nach einem Neustart zeigt Tätigkeit die letzten Aufträge mit dem Vermerk „vor dem Neustart“:
- Ein Start ohne Ende erscheint als **unterbrochen**: Der Dienst endete während dieses Laufs. Was der Lauf geschafft hat, steht im Journal, und das Lagebild zeigt den Stand; ein angefangener Papiertag zum Beispiel ist Halt mit Grund.
- Was heute schon lief, hält der Zeitplan weiter in `daten/dienst_erledigt.txt` fest; nichts startet doppelt.

Beim Start wird die Datei auf die gezeigten Aufträge gekürzt. Unlesbare Zeilen fehlen im Verlauf und werden gemeldet. Die Datei entscheidet nichts.

## Tests

`crates/depot_dienst/tests/dienst.rs`:

- Einschalten gelingt erst mit Lagebild und ohne Eingriff.
- Die Eingaben stehen danach im Formular, ohne Entwurf der aktive Plan.
- Uhrzeiten ändern geht immer.
- Gültige Eingaben bleiben, ungültige und fremde nicht.
- Ein Doppelklick und erneutes Absenden starten nichts doppelt, mit Abstand geht es wieder.
- Der Verlauf übersteht einen Neustart, auch zwei. Ein Unterbrochenes wird gezeigt.
- Der Verlauf wird streng gelesen.

**Gegenproben** (jeweils rot):

- Einschalten ohne Prüfung.
- Ohne Doppelklick-Schutz.
- Eingaben verworfen.
- Start nicht vermerkt.

## Fassung 2 nach Maus' Prüfung

**Befund 1:** Der Zeitplan ließ sich mit fehlendem KI-Schlüssel einschalten, weil „Läuft ohne KI“ als ausreichende Einrichtung galt.

**Regel:** `Zustand::einschalten_erlaubt` prüft jede nötige Einstellung ausdrücklich:
- Jeder Schlüssel (Kursquelle, Dachs, Maus) ist hinterlegt oder geprüft. Fehlt, abgelehnt oder unbekannt genügt nicht.
- Kein Eingriff ist nötig.

Die Ablehnung liefert die Liste dessen, was fehlt. Die Einrichtung zeigt diese Liste beim Zeitplan.

**Befund 2:** Der Dienst startete einen Auftrag, bevor er den Start vermerkte. Ein Schreibfehler blieb unbemerkt, und der Lauf fehlte nach einem Neustart im Verlauf.

**Regel:** Erst wird die Startzeile dauerhaft geschrieben (mit `sync`), dann gestartet. Lässt sie sich nicht schreiben, startet der Auftrag nicht. Er steht als „Nicht gestartet: Verlauf nicht schreibbar“ im Verlauf, und Tätigkeit zeigt den Fehler sofort und rot. Ein Schreibfehler beim Ende wird ebenso gezeigt. Der Fehler verschwindet, sobald wieder ein Start vermerkt ist.

**Gegenproben** (jeweils rot):
- Schlüssel nicht ausdrücklich geprüft (`einschalten_verlangt_jeden_schluessel`).
- Start trotz Schreibfehler (`ohne_schreibbaren_verlauf_startet_nichts`).

## Seltene Einstellungen am Seitenende

Wolfgang am 07.11.2026: Zugangsdaten und andere selten geänderte Angaben sollen unten in einen roten Bereich. Die Einrichtung zeigt deshalb zuerst aktive Sperren, Datenquellen, Zeitplan und Update. Am Ende fasst ein rot umrandeter und leicht rot hinterlegter Bereich die erste Einrichtung, Journal, Zugangsdaten, Autostart, Werkzeuge, feste Grenzen und technische Dienstangaben zusammen. Der Journal-Abschnitt wird dort weiter automatisch aktualisiert; Rücksprungziele und Doppelklickschutz bleiben erhalten. Aktive Sperren stehen oberhalb des roten Bereichs. Das KI-Budget bleibt auf Live sichtbar.
