# Tagesergebnis ohne Konsole

Maus' Praxistest am 05.11.2026: Wolfgang soll ohne Konsole und ohne Journal sehen,
- ob ein Handelstag verpasst oder nachgeholt wurde, und warum (etwa Kurse zu alt, weil der Rechner zur Abrufzeit schlief),
- welche KI zum Tagesergebnis tatsächlich beigetragen hat und welche ausfiel, samt Grund,
- dass jedes Ergebnis im Papierdepot eine simulierte Ausführung zum Schlusskurs ist.

## Was schon so funktionierte

- **Schlüssel hinterlegt ist nicht angenommen:** Seit `fehlerfaelle.md` zeigt die Einrichtung „hinterlegt, noch nicht benutzt“ getrennt von „geprüft“. Geprüft heißt: Der letzte echte Aufruf wurde angenommen.
- **Gescheiterter Lauf:** Unter Tätigkeit steht jeder Auftrag mit Protokoll, auch nach einem Neustart (`fehlbedienung.md`). Ein angefangener Papiertag ist Halt mit Grund.
- **Simulation im Journal:** Jeder Papiertag beginnt mit dem Vermerk „Papiergeld; Ausfuehrung zum Schlusskurs …“. Neu ist, dass die Oberfläche das auch sagt.

## Tagesergebnis

`depot_lage::tage` liest Journal und Anbieterprotokoll und zeigt die letzten zehn Handelstage (Montag bis Freitag) seit dem ersten Papiertag, neueste zuerst. Nichts davon entscheidet etwas.

| Ausgang | Wann |
|---|---|
| Papiertag abgeschlossen | Der Tag hat seinen Vermerk und ist abgeschlossen. |
| Abgeschlossen ohne Handel | Die Kursprüfung ließ den Tag ausfallen. Die Befunde stehen in Worten da, etwa „Kurse zu alt“, je Art einmal mit Anzahl. |
| Angefangen | Begonnen, nicht abgeschlossen. Den Halt zeigt die Bereitschaft. |
| Ausgefallen: keine Schlusskurse | Kein Papiertag und keine Kurse. Rechner oder Zeitplan zur Abrufzeit aus, Abruf gescheitert oder Börsenfeiertag. |
| Ausgefallen: Kurse da, kein Papiertag | Der Papiertag lief nicht. Nachholen geht nicht, denn ein Tag wird nur mit seinen eigenen Kursen am selben Tag entschieden. |
| Heute noch nicht gelaufen | Der heutige Tag ohne Papiertag. |

Je Tag stehen Dachs und Maus mit ihrem Beitrag. Die Reihenfolge der Prüfung:

1. **Hat beigetragen:** Mindestens ein angenommener Aufruf an diesem Tag (Frankfurter Datum) im Anbieterprotokoll. Weitere Fehlschläge stehen dahinter.
2. **Ausgefallen, nicht gefragt:** Das Journal sagt „nicht angebunden“. Der Grund steht seit diesem Schritt mit im Journal (siehe unten).
3. **Ausgefallen:** Gefragt, aber jeder Aufruf scheiterte. Der Grund kommt aus dem Protokoll: Limit, Guthaben, abgelehnter Schlüssel, überlastet, Netz, Antwort fehlte, Kosten über der Reservierung.
4. **Ohne verwertbare Antwort:** Eine Begründung im Journal, aber kein angenommener Aufruf im Protokoll.
5. **Nicht gefragt:** sonst. An einem Tag ohne Handel heißt das ausdrücklich „Die Kursprüfung ließ den Tag ausfallen“.

Ein grüner Abschluss allein sagt also nicht mehr, wer mitgewirkt hat. Das steht in derselben Zeile.

## Grund im Journal

`depot_sim::Berater` hat die Methode `warum_nicht`. Der Papiertag schreibt damit „nicht angebunden (Grund), nicht gefragt“ ins Journal. `depot_berater` nennt:
- kein Schlüssel hinterlegt,
- Kostenschutz gesperrt (`daten/ki.halt`),
- Anbieterpause mit ihrem Grund (dieselbe Regel `anbieter::pausiert` wie vor dem Aufruf).

`Stumm` nennt keinen Grund. Die Anzeige sagt dann allgemein „kein Schlüssel, Sperre oder Anbieterpause“.

## Kurse zu alt

Die Kursprüfung verlangt höchstens 20 Minuten zwischen Xetra-Schluss (17:35) und Abruf. Holt der Rechner den Abruf nach, etwa nach dem Aufwachen, sind die Kurse zu alt, und der Papiertag fragt niemanden. Der Punkt Schlusskurse sagt das jetzt in Worten. Er bietet dann nicht mehr „Kurse abrufen“ an, denn ein neuer Abruf am selben Abend läge noch später. Er rät, morgen rechtzeitig abzurufen (Zeitplan an, Rechner wach). Andere Befunde behalten den Knopf.

Ob ein später Abruf am selben Abend trotzdem zählen soll, ist eine Grenze und bleibt Wolfgangs Entscheidung. Dieser Schritt ändert sie nicht.

## Simulation

Das Papierdepot beginnt mit: „Nur Papiergeld, simuliert: Jeder Auftrag gilt als zum Schlusskurs des Entscheidungstags ausgeführt. Ob eine echte Order zu diesem Preis ausgeführt worden wäre, bleibt offen; ein Plus hier ist ein Simulationsergebnis.“

## Tests

- **`crates/depot_lage/tests/lage.rs`:**
  - `tagesergebnis_zeigt_ausfall_und_beitrag_je_ki`: heute offen, ein Tag mit Kursen ohne Papiertag, ein Tag ohne Kurse, ein abgeschlossener Tag. Dachs hat beigetragen, Maus fiel aus mit „kein Schlüssel hinterlegt“ aus dem Journal. Alles steht auf der Seite.
  - `tagesergebnis_nennt_den_fehler_des_anbieters`: Ein Limit im Protokoll heißt „ausgefallen: Anbieterlimit“. Ohne Protokoll zählt eine Antwort im Journal nicht als Beitrag.
  - `nachgeholter_abruf_heisst_kurse_zu_alt`: Abruf um 20:00 Uhr, „Kurse zu alt“ ohne Knopf, der Tag danach „abgeschlossen ohne Handel“, beide KIs nicht gefragt.
  - `papierdepot_nennt_die_simulation`.
- **`crates/depot_berater/tests/berater.rs`:** `warum_nicht` für fehlenden Schlüssel, Sperre und Anbieterpause, `None` mit Schlüssel.
- **`crates/depot_sim/tests/papiertag.rs`:** `Stumm` schreibt keinen erfundenen Grund.

**Gegenproben** (jeweils rot):
- Ein angenommener Aufruf zählt nicht als Beitrag.
- Befunde der Kursprüfung werden übergangen.
- Ausgefallene Tage werden übersprungen.
- Zu alte Kurse bieten weiter „Kurse abrufen“ an.
- Ohne Simulationshinweis.
- Der Grund fehlt im Journal.
- `warum_nicht` ohne Anbieterpause.

## Grenzen

- Der Beitrag einer KI ist so genau wie das Anbieterprotokoll. Ein Lauf vor dem Protokoll (vor `fehlerfaelle.md`) zeigt „ohne verwertbare Antwort“.
- Börsenfeiertage kennt die Anzeige nicht. Sie erscheinen als „keine Schlusskurse“ mit dem Feiertag als einem der genannten Gründe.
