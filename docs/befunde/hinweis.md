# Windows-Hinweis bei „Eingriff nötig“ (#122)

Wolfgang, 10.11.2026 (Runde 2, #116). Klasse Funktion; kein Netz, keine neue Abhängigkeit,
keine Grenze. Die Architektur verspricht an mehreren Stellen, dass Wolfgang sofort eine
Nachricht bekommt (Notbremse, Kontowächter). Bisher sah er einen Halt nur auf der Live-Seite.

## Ablauf

- Nach jedem neuen Lagebild prüft der Dienst den Gesamtstatus. Jeder **neue** Grund für
  „Eingriff nötig“ (Notbremse, 30-%-Sperre, `daten/ki.halt`, Kurssperre, Abgleich-Halt,
  angefangener Tag, Zeitplanfehler) ergibt einen Windows-Hinweis im Infobereich.
- Ist der jüngste Handelstag im Tagesergebnis ausgefallen, kommt einmal der Grund
  „Papiertag am TT.MM.JJJJ ausgefallen.“ Heute (noch offen) und Börsenfeiertage zählen nicht.
- Titel `DepotSchlopf: Eingriff nötig`, Text: erster neuer Grund (höchstens 200 Zeichen), bei
  mehreren „(und N weitere)“, am Ende „Details auf http://127.0.0.1:47410/“. Ein Klick öffnet
  nichts.
- Bündeln: Neue Gründe sammeln sich 10 Sekunden (`SAMMEL_SEKUNDEN`), und zwischen zwei
  Hinweisen liegen mindestens 60 Sekunden (`ABSTAND_SEKUNDEN`). Das Lagebild wird alle
  5 Sekunden neu gerechnet, ein Hinweis kommt also nach 10 bis 15 Sekunden.
- Einrichtung → Zeitplan: Schalter „Windows-Hinweis bei Eingriff“, **ab Werk an**, weil er
  nur anzeigt. Gespeichert in `daten/zeitplan.txt` als `hinweis_aus=ja`, nur wenn er aus ist.
  Alte Dateien ohne das Feld bedeuten an. Der Schalter gilt auch bei ausgeschaltetem
  Zeitplan.

## Gedächtnis `daten/hinweise.txt`

- Eine Zeile je gemeldetem Grund, der noch besteht: `<sha256-hex des Grundtexts>;<UTC>`,
  höchstens 200 Zeilen (älteste fallen weg). Geschrieben über eine Nebendatei, dann an ihren
  Platz (`schreibe_ganz`).
- Beim Start eingelesen: Ein Neustart meldet denselben alten Halt nicht noch einmal. Ein
  neuer Grund, der vor dem Neustart noch nicht gemeldet war, steht nicht in der Datei und
  wird nach dem Start gemeldet.
- Verschwindet ein Grund, fällt seine Zeile weg; kehrt er zurück, kommt wieder ein Hinweis.
- Unlesbar heißt leer, und die Datei wird gültig neu geschrieben: lieber ein Hinweis zu viel
  als einer zu wenig.
- Bei ausgeschaltetem Hinweis wird die Datei trotzdem gepflegt. Beim Einschalten kommt kein
  Schwall alter Hinweise.

## Sicherheit

- Die Gründe sind Oberflächentext und geheimnisfrei. Nichts anderes kommt in den Hinweis.
- `hinweis::zeige` startet PowerShell wie der Papierdialog (`CREATE_NO_WINDOW`,
  `-NoProfile -NonInteractive -STA`). Titel und Text gehen **nur** über die Umgebungsvariablen
  `DEPOTSCHLOPF_HINWEIS_TITEL` und `DEPOTSCHLOPF_HINWEIS_TEXT`, nie in den Befehl. Der Dienst
  wartet nicht darauf; ein eigener Faden räumt das Kind ab. Unter anderen Systemen tut
  `zeige` nichts.
- Der Melder läuft außerhalb der Sperre der Werkbank. Ein hängender Hinweis hält weder Live
  noch den Arbeiter auf.

## Tests

| Test | Prüfwert |
|---|---|
| `hinweis::tests::erster_grund_meldet` | `{}` und `[A, B]` ergeben `[A, B]`, 2 Abdrücke |
| `hinweis::tests::bekannter_grund_meldet_nicht` | `{h(A)}` und `[A, B]` ergeben `[B]` |
| `hinweis::tests::weg_und_wieder_da` | `{h(A)}` und `[]` ergeben `[]`, `{}`; danach `[A]` ergibt `[A]` |
| `hinweis::tests::text_buendelt_und_kuerzt` | „Notbremse (und 2 weitere) Details auf …“, 200 Zeichen |
| `hinweis::tests::kaputte_datei_gilt_leer` | `xyz` ist unlesbar, eine gültige Zeile ergibt 1 |
| `hinweis::tests::text_nur_ueber_umgebung` | Skript liest `$env:…`, `zeige` ohne `format!` |
| `hinweis_meldet_neuen_grund_einmal_und_nach_rueckkehr_wieder` | 4 Lagebilder 1 Hinweis, Datei mit Abdruck; weg: Datei leer; zurück: 2. Hinweis |
| `neustart_meldet_nicht_doppelt` | Datei mit `h(A)`, Lagebild `[A]`: 0 Hinweise |
| `buendelt_in_60_sekunden` | A, B, C in 10 s: 1 Hinweis „A (und 2 weitere)…“; D nach 20 s erst bei 60 s Abstand |
| `ausgefallener_tag_einmal` | 5 Lagebilder: 1 Hinweis „Papiertag am 09.11.2026 ausgefallen.“ |
| `kaputte_hinweisdatei_gilt_leer` | Datei `xyz`: 1 Hinweis, Datei danach gültig |
| `hinweis_schalter_aus_pflegt_trotzdem` | 0 Hinweise, Datei mit `h(A)` |
| `zeitplan_ohne_hinweisfeld_heisst_an` | alte Datei: an; `hinweis_aus=ja` hin und zurück, doppelt ungültig |

Offen: Praxistest unter Windows (Wolfgang oder Maus) mit Bildschirmfoto: `daten/ki.halt` von
Hand anlegen, Hinweis erscheint; Dienst neu starten, kein zweiter Hinweis; `ki.halt`
aufheben und wieder anlegen, neuer Hinweis.
