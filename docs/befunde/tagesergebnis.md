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
| Ausgefallen: keine Schlusskurse | Kein Papiertag und keine Kurse. Rechner oder Zeitplan zur Abrufzeit aus, oder Abruf gescheitert. Ein Börsenfeiertag steht als „Börse geschlossen“ (`boersen.md`). |
| Ausgefallen: Kursprüfung | Kein Papiertag, und die Kursprüfung mit den Kursen, die bis zum Ende dieses Tages abgerufen waren, meldet Befunde, etwa „Kurse zu alt“. Spätere Abrufe zählen für diesen Tag nicht. |
| Ausgefallen: Kurse da, kein Papiertag | Der Papiertag lief nicht. Nachholen geht nicht, denn ein Tag wird nur mit seinen eigenen Kursen am selben Tag entschieden. |
| Heute noch nicht gelaufen | Der heutige Tag ohne Papiertag. |

Je Tag stehen Dachs und Maus mit ihrem Beitrag. Ob eine KI beigetragen hat, sagt allein das Journal (Fassung 2, siehe unten). Die Reihenfolge der Prüfung:

1. **Hat beigetragen:** Der Papiertag hat mindestens eine Antwort der KI verwendet (Vorschlag, Zustimmung oder Veto). Verworfene Antworten desselben Tages stehen dahinter.
2. **Ausgefallen, gefragt ohne verwendbare Antwort:** Jede Antwort wurde verworfen. Den Grund liefert das Anbieterprotokoll dieses Tages (Limit, Guthaben, abgelehnter Schlüssel, überlastet, Netz, Antwort fehlte, Kosten über der Reservierung), sonst „Antwort verworfen“.
3. **Ausgefallen, nicht gefragt:** Das Journal sagt „nicht angebunden“, mit dem Grund (siehe unten).
4. **Kein verwendeter Beitrag im Journal:** Eine andere Begründung, etwa aus einem Journal vor Fassung 2.
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

Das Papierdepot beginnt mit: „Nur Papiergeld, simuliert: Neue Orders warten auf einen geprüften Schlusskurs nach ihrem Versand. Ob eine echte Order zu diesem Preis ausgeführt worden wäre, bleibt offen.“ (Bis #47 hieß es: zum Schlusskurs des Entscheidungstags.)

## Fassung 2 nach Maus' Prüfung

**Befund 1:** Holte der Rechner die Kurse um 20 Uhr nach, startete wegen der Kursprüfung kein Papiertag. Am nächsten Morgen stand nur „Kurse liegen vor, aber der Papiertag lief nicht“.

**Regel:** Für einen Tag ohne Papiertag wiederholt die Anzeige die Kursprüfung genau so, wie ein Papiertag an diesem Tag sie gesehen hätte. Dafür gibt es `depot_sim::tageskurse_am`: nur Belege, die bis zum Ende dieses Tages (Frankfurt) abgerufen waren, und nur Kurse dieses Handelstags. Befunde stehen in Worten in derselben Zeile.

**Befund 2:** „Hat beigetragen“ zählte erfolgreiche Aufrufe im Anbieterprotokoll nach Datum. So konnte die Zeile grün sein, obwohl der Papiertag die KI nicht gefragt oder ihre Antwort verworfen hatte.

**Regel:** Der Papiertag schreibt ausdrücklich ins Journal, was er verwendet hat, und nur das zählt.
- `Antwort` und `Urteil` tragen `verwertbar`. `depot_berater` setzt es nur bei einer gelesenen, geprüften Antwort.
- Der Lauf schreibt „Antwort verwendet“ oder „Antwort verworfen“, beim Urteil „Zustimmung“, „Veto“ oder „Antwort verworfen, kein Urteil“.
- Eine verworfene Antwort trägt nie einen Vorschlag weiter. Ein verworfenes Urteil ist keine Zustimmung (R31).
- `depot_sim::gefragt` liest diese Worte zurück. Schreiben und Lesen nutzen dieselben Festwerte.
- Das Anbieterprotokoll erklärt nur noch den Grund eines Ausfalls und macht nie etwas grün.

**Gegenproben** (jeweils rot):
- Keine Kursprüfung für verpasste Tage.
- Spätere Abrufe zählen für einen vergangenen Tag mit.
- Ein erfolgreicher Aufruf im Protokoll macht grün.
- Eine verworfene Antwort steht als verwendet.
- Ein verworfenes Urteil gilt als Zustimmung.
- Eine verworfene Antwort trägt ihren Vorschlag weiter.

## Fassung 3: alte Journale

**Befund (Maus, Restbefund):** Vor Fassung 2 stand ein verworfenes Urteil als `Topf B: Veto: keine verwertbare Antwort: …` im Journal. Die Anzeige wertete das als echtes Veto und zeigte es grün.

**Regel:** Wer gefragt wurde und nichts verwerten konnte, schreibt seit jeher mit `depot_sim::KEINE_VERWERTBARE_ANTWORT` (`depot_berater` nimmt dieselbe Festkonstante). `gefragt` wertet einen Eintrag als verworfen, wenn er direkt nach dem Topf oder nach `Zustimmung`, `Veto` oder `Antwort verwendet` mit diesen Worten beginnt. Im Zweifel zählt ein Eintrag als verworfen, nie als Beitrag. Ein echtes Veto bleibt ein Beitrag.

**Gegenproben** (jeweils rot):
- Altes Veto ohne verwertbare Antwort zählt als Beitrag.
- Nur der alte Vorschlag wird erkannt, das alte Veto nicht.

## Tests

- **`crates/depot_lage/tests/lage.rs`:**
  - `tagesergebnis_zeigt_ausfall_und_beitrag_je_ki`:
    - heute offen;
    - ein Tag mit Kursen ohne Papiertag;
    - ein Tag mit nachgeholtem Abruf: „Kursprüfung … Kurse zu alt“, obwohl der nächste Abruf den Tag noch einmal belegt;
    - ein Tag ohne Kurse;
    - ein abgeschlossener Tag. Dachs hat beigetragen. Maus fiel aus mit „kein Schlüssel hinterlegt“ aus dem Journal, trotz eines erfolgreichen Aufrufs im Protokoll.
  - `tagesergebnis_nennt_den_fehler_des_anbieters`: Eine verworfene Antwort heißt „gefragt ohne verwendbare Antwort: Anbieterlimit“, auch neben einem erfolgreichen Aufruf. Ohne Protokoll bleibt sie rot mit „Antwort verworfen“.
  - `nachgeholter_abruf_heisst_kurse_zu_alt`: Abruf um 20:00 Uhr, „Kurse zu alt“ ohne Knopf, der Tag danach „abgeschlossen ohne Handel“, beide KIs nicht gefragt.
  - `papierdepot_nennt_die_simulation`.
  - `altes_veto_ohne_verwertbare_antwort_ist_kein_beitrag`: alte Einträge „Veto: keine verwertbare Antwort“ und „keine verwertbare Antwort“ sind rot, ein echtes Veto ist grün.
- **`crates/depot_berater/tests/berater.rs`:** `warum_nicht` für fehlenden Schlüssel, Sperre und Anbieterpause, `None` mit Schlüssel.
- **`crates/depot_sim/tests/papiertag.rs`:**
  - `Stumm` schreibt keinen erfundenen Grund.
  - `verworfene_antwort_und_urteil_stehen_als_verworfen`: Nur verwendbare Antwort und verwendbares Urteil führen zum Kauf, alles steht so im Journal, und `gefragt` liest nur die eigenen Worte nach dem Topf.

**Gegenproben** (jeweils rot):
- Ein angenommener Aufruf zählt nicht als Beitrag.
- Befunde der Kursprüfung werden übergangen.
- Ausgefallene Tage werden übersprungen.
- Zu alte Kurse bieten weiter „Kurse abrufen“ an.
- Ohne Simulationshinweis.
- Der Grund fehlt im Journal.
- `warum_nicht` ohne Anbieterpause.

## Grenzen

- Ein Journal vor Fassung 2 kennt „Antwort verwendet“ noch nicht. Solche Tage zeigen „kein verwendeter Beitrag im Journal“, außer bei Zustimmung oder Veto.
- Börsenfeiertage kennt die Anzeige nicht. Sie erscheinen als „keine Schlusskurse“ mit dem Feiertag als einem der genannten Gründe.
