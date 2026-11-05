# Lagebild

Wolfgang am 05.11.2026: DepotSchlopf soll dauerhaft laufen und über eine lokale Weboberfläche mit genau einer Config-Seite eingerichtet werden, ohne Konsole. Die Oberfläche zeigt live, was läuft, was fehlt, wie das Depot steht, wie viel KI-Budget übrig ist und was Dachs und Maus vorschlagen, einwenden und ablehnen. Maus hat ergänzt: Speichern und Aktivieren sichtbar unterscheiden, KI-Budget heute und im Monat je KI mit Reservierung und Rest, alle Zahlen direkt aus dem KI-Kassenbuch. Wolfgang hat den lokalen Server auf 127.0.0.1 ohne Zusatzbibliothek freigegeben („Ja, nur 127.0.0.1“).

Dieser Schritt baut die Grundlage ohne Netz: das Lagebild. Der Dienst mit Server, Config-Seite, Knöpfen, Autostart und Update folgt in eigenen Schritten und zeigt genau dieses Lagebild.

## Eine Wahrheit (D16)

Das Lagebild rechnet nichts selbst nach, was ein Lauf entscheidet. Es ruft dieselben Funktionen:

| Anzeige | Entscheidet im Lauf | Gemeinsame Funktion |
|---|---|---|
| Schlusskurse von heute | Papiertag | `depot_sim::kurse_von_heute` |
| Kurse bestehen die Prüfung | Papiertag (Zyklus fällt sonst aus) | `depot_regelwerk::pruefe_kurse` |
| Papiertag kann laufen / abgeschlossen / Halt | Papiertag | `depot_sim::tageslage` |
| KI-Budget bereit / pausiert / gesperrt | Kostenschutz bei `reserviere` | `depot_ki::pruefe` mit `depot_sim::tagnummer` als Zyklus |

`kurse_von_heute` und `tageslage` sind aus dem Papiertag herausgelöst; der Papiertag ruft sie in derselben Reihenfolge auf wie bisher (Kurse, Abgleich mit dem Konto, Tageslage). Sein Verhalten ändert sich nicht, die Tests des Papiertags laufen unverändert.

## Was das Lagebild zeigt

- **Bereitschaft:** je Punkt eine Ampel (bereit, offen, Halt), ein Satz und was hilft: Journal einrichten, Schlüssel der Kursquelle, Schlüssel je Berater, Kurse abrufen, Termine abrufen, Papiertag starten, warten oder prüfen. Ein Halt kommt aus einem unlesbaren Journal, einer liegengebliebenen Sperrdatei, einem angefangenen Tag, einem Abschluss nach dem Handelstag oder einem gesperrten Kassenbuch. Oben steht der nächste Schritt; ein Halt geht allem vor.
- **KI-Budget je KI:** heute gebucht und Aufrufe, Monat gebucht mit Grenze, Rest und Aufrufen, Jahr mit Grenze, Höchstkosten je Aufruf und der Status (bereit, pausiert bis morgen, bis Monatsanfang, bis Jahresanfang, gesperrt). Gebucht ist, was das Kassenbuch reserviert hat, also die Höchstkosten jedes Aufrufs. Tatsächliche Kosten folgen mit der Anbindung der Berater.
- **Papierdepot:** Töpfe mit Bargeld und Positionen, bewertet mit den jüngsten belegten Schlusskursen und abgerundet wie beim Wächter, Höchststand und letzter Papiertag. Dazu die letzten 20 Aufträge mit Anlass und Zustand.
- **Gespräch:** die letzten 60 Begründungen aus dem Journal, neueste zuerst, mit Datum und wer (Dachs, Maus, Regelwerk, Papiertag).
- **Grenzen und Budgets:** die festgeschriebenen Werte. Sie gelten und ändern sich nur über eine Grenzfreigabe, nie mit einem Klick.
- Zeiten in Frankfurter Ortszeit (MEZ/MESZ) über den Kalender des Kerns.

Die Schlüssel selbst erreichen das Lagebild nie, nur „hinterlegt ja/nein/nicht prüfbar“. `run.cmd lage` prüft noch keine Schlüssel und zeigt „nicht prüfbar“; das übernimmt der Dienst.

## Fassung 2 nach Maus' Prüfung (K26-01)

**Befund:** Ein gelöschtes Journal bei vorhandenem Vermerk erschien als „Noch nicht eingerichtet“. `run.cmd lage` sah nur nach, ob die Journaldatei da ist, und bot damit einen Neubeginn an, wo ein unvollständiger Datenbestand lag.

**Regel:** Neu ist nur ein Datenbestand ohne jede Spur. Ob ein Journal eingerichtet ist, entscheidet allein `Journal::eingerichtet`: `false` nur ohne Journal und ohne Vermerk; fehlt nur das Journal, ist es Halt („Datenbestand unvollständig“). `Journal::oeffnen` beginnt mit derselben Prüfung, `Journal::einrichten` lehnt in diesem Fall ohnehin ab. Das Lagebild liest nur über `depot_lage::mit_journal`: kein Journal, kein Vermerk, kein Papierkonto heißt neu; ein Papierkonto ohne Journal und Vermerk ist ebenfalls Halt; sonst wird das Journal wie bei jedem Lauf geöffnet, und jeder Fehler daraus ist Halt.

**Gegenprobe:** Mit der alten Prüfung (nur „Journaldatei da?“) in `mit_journal` werden `geloeschtes_journal_mit_vermerk_ist_halt` und `papierkonto_ohne_journal_ist_halt` rot, mit der neuen grün. `k26_01_fehlendes_journal_bei_vermerk_ist_halt` in `crates/depot_journal/tests/befunde.rs` zeigt, dass `eingerichtet` und `oeffnen` mit demselben Grund halten.

## `run.cmd lage`

Öffnet das Journal `daten/journal.txt` über `mit_journal` wie jeder Lauf (geprüft, exklusiv) und schreibt die Seite `daten/lage.html`, erst als Nebendatei, dann an ihren Platz. Hat ein anderer Lauf das Journal offen, zeigt die Seite „belegt“. Die Seite hat kein Skript und lädt nichts von außen; jeder Text ist maskiert.

Kein Netz, keine Schlüssel, keine neuen fremden Abhängigkeiten, keine neuen Grenzen, kein echtes Geld.

## Tests

`crates/depot_lage/tests/lage.rs`:

- ohne Journal ist Einrichten der nächste Schritt
- belegtes Journal wartet, unlesbares Journal ist Halt
- Sperrdatei ist Halt
- fehlender Schlüssel der Kursquelle und der Berater wird mit Abhilfe gezeigt
- leeres Journal braucht Kurse und Termine; Budget beider KIs bereit
- Kurse von gestern verlangen den Abruf und nennen den Tag
- bereit, dann abgeschlossen wie der Papiertag: Das Lagebild sagt „kann laufen“, der Papiertag läuft wirklich; danach „abgeschlossen“, zwei Töpfe zu 10.000 € zusammen, Begründung `Papiertag` im Gespräch
- angefangener Tag ist Halt mit demselben Grund, und der Papiertag hält wirklich (Gegenprobe)
- Budget: zwei Aufrufe heute pausieren bis morgen, Summe und Rest stimmen, die andere KI bleibt unberührt, am nächsten Tag wieder bereit
- Budget: Monat voll pausiert bis Monatsanfang, Uhr vor der letzten Buchung sperrt, neuer Monat wieder bereit und das Jahr zählt weiter
- die Seite maskiert jeden Text (`<script>` aus einer Begründung erscheint nur maskiert)
- Zeiten in Frankfurter Ortszeit, im Winter MEZ und im Sommer MESZ
- K26-01: gelöschtes Journal bei vorhandenem Vermerk ist Halt, nie „einrichten“ (Maus' Gegenprobe)
- Papierkonto ohne Journal und Vermerk ist Halt
- ohne jede Spur neu, eingerichtet offen, während eines Laufs belegt, alles über `mit_journal`
