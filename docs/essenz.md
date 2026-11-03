# DEPOTSCHLOPF – ESSENZ

Status: beratend (advisory only), nicht normativ, keine produktive Autorität.

## Zweck

Die Essenz bewahrt die verdichtete Erfahrung des Projekts, übernommen aus EtuSchlopf und aus dem gescheiterten Vorgänger „Investor by KNN5“.

Sie beantwortet nicht „Was ist verboten?“. Das entscheidet ausschließlich der Kodex: das ausführbare Gate in `crates/depot_kodex/` mit der Festschreibung `kodex/festschreibung.txt`, dazu die Typen und Tests.

Die Essenz beantwortet: „Welche plausibel wirkenden Wege sind gefährlich, woran erkenne ich sie früh, und wo suche ich stattdessen?“

**Die Essenz berät vor der Änderung. Der Kodex entscheidet nach der Änderung.**

## Rangfolge bei Widerspruch

```text
aktuelles Repository
↓
Kodex (Gate, Festschreibung, Typen, Tests)
↓
Essenz als beratende Erklärung
↓
Erinnerung, Übergabetexte, Chatverlauf
```

Die Essenz darf den Kodex weder erweitern noch abschwächen.

## Leitsatz

> Essenz lenkt. KI schlägt vor. Regelwerk entscheidet. Auftragsstelle schreibt. Journal belegt. Abgleich prüft. Kodex wacht.

---

# D00 – Repository vor Erinnerung

**Aussage**

Die Sicherheit von DepotSchlopf darf nie von Chatverlauf, menschlicher Erinnerung oder KI-Gedächtnis abhängen.

**Warum**

Ein neuer Bearbeiter, Mensch oder KI, kennt die Vorgeschichte nicht und baut sonst einen bereits verworfenen Weg nach.

**Typische Fehlform**

Eine Grenze steht nur in einer Übergabe („wir haben doch 25 % vereinbart“), aber nicht im Code.

**Lösungsraum**

Jede harte Grenze steht im Code und in `kodex/festschreibung.txt`. Der Einstieg für jeden Kaltstart ist `docs/kaltstart.md`.

**Vor der Änderung prüfen**

Ist alles, was ich für diese Entscheidung brauche, aus dem Repository ableitbar?

**Harter Bezug**

Kodex-Schritt Essenz-Kaltstart; Festschreibung.

---

# D01 – KI schlägt vor, Regelwerk entscheidet

**Aussage**

Dachs (Claude) und Maus (GPT) liefern nur Zielgewichte. Ob, was und wie viel gehandelt wird, entscheidet allein das Regelwerk.

**Warum**

Eine KI kann sich irren, Werte erfinden oder Unmögliches verlangen. Ein deterministisches Regelwerk kann man testen, eine KI nicht.

**Typische Fehlform**

Die KI-Antwort enthält Stückzahlen oder Ordertypen, und der Code reicht sie „nur kurz geprüft“ an den Broker weiter.

**Lösungsraum**

KI-Antwort → `Vorschlag` → `entscheide` → `GenehmigterAuftrag`. Nichts anderes erreicht den Broker.

**Vor der Änderung prüfen**

Kann irgendein KI-Text auf einem anderen Weg als über `entscheide` eine Order beeinflussen?

**Harter Bezug**

`GenehmigterAuftrag` ist versiegelt (`crates/depot_regelwerk/src/auftrag.rs`); nur das Regelwerk kann ihn bauen.

---

# D02 – Erkennen, Entscheiden und Ausführen sind getrennt

**Aussage**

Kursprüfung und Wächter erkennen, das Regelwerk entscheidet, die Auftragsstelle führt aus. Keine Schicht übernimmt die Aufgabe einer anderen.

**Warum**

Wenn der Broker-Adapter „vernünftig nachbessert“ (Stückzahl runden, Limit anpassen), gibt es zwei Wahrheiten.

**Typische Fehlform**

Der Broker meldet „zu wenig Geld“, und der Adapter verkleinert die Order selbst.

**Lösungsraum**

Der Adapter sendet genau den genehmigten Auftrag oder bricht ab. Korrekturen entstehen im nächsten Zyklus wieder im Regelwerk.

**Vor der Änderung prüfen**

Trifft diese Stelle eine Entscheidung, die eigentlich dem Regelwerk gehört?

**Harter Bezug**

Typsystem: der Broker nimmt nur `GenehmigterAuftrag` an.

---

# D03 – Im Zweifel nichts tun

**Aussage**

Jeder unklare Zustand führt zu Halt, nie zu Raten. Fehlende Kurse, unplausible Sprünge, kaputte Dateien und Rechenüberläufe sperren.

**Warum**

Ein verpasster Handelstag kostet fast nichts. Eine falsche Order kann viel kosten.

**Typische Fehlform**

Ein fehlender Kurs wird durch den Vortageskurs ersetzt, „damit der Lauf durchgeht“.

**Lösungsraum**

`Option` und `Result` bis oben durchreichen; `Befund::Halt`, `Befund::Rechenfehler`, `Sperre::Kassenbuch`.

**Vor der Änderung prüfen**

Was passiert in meinem Code, wenn ein Wert fehlt oder unsinnig ist? Läuft er dann weiter?

**Harter Bezug**

Clippy verbietet `unwrap`, `expect`, `panic`, Indexzugriff und ungeprüfte Arithmetik; Kodex-Schritt Festschreibung `lints`.

---

# D04 – Kein Produkt, dessen Verlust den Einsatz übersteigen kann

**Aussage**

Gekauft werden nur Aktien aus der Positivliste, später ggf. physische UCITS-ETFs. Keine Derivate, Optionen, Futures, CFDs, Optionsscheine, Zertifikate, Hebel- oder Short-ETFs, keine Leerverkäufe, kein Kredit.

**Warum**

Bei diesen Produkten kann der Verlust größer sein als der Kaufpreis, oder er ist schwer zu begrenzen.

**Typische Fehlform**

„Nur zur Absicherung“ wird ein Put oder ein Short-ETF zugelassen.

**Lösungsraum**

Die Produktart ist ein Typ (`InstrumentArt`) mit genau zwei Varianten. Gehandelt wird nur, was im `Universum` steht. Verkauft wird nie mehr, als im Topf liegt. Gekauft wird nur aus gesichertem Bargeld. Beim Broker: Cash-Konto ohne Margin.

**Vor der Änderung prüfen**

Kann nach meiner Änderung ein Verlust entstehen, der größer ist als das eingesetzte Geld?

**Harter Bezug**

Festschreibung `produktarten`; Zufallstest `zufallstest_harte_grenzen` in `crates/depot_regelwerk/tests/regelwerk.rs`.

---

# D05 – Grenzen ändern sich nie nebenbei

**Aussage**

Harte Grenzen (Anteil je Wert, Umschlag, Notbremse, KI-Budget, Produktarten, Abhängigkeiten, Lints) ändern sich nur in einer eigenen Änderung der Klasse `FestschreibungAenderung`, mit Freigabe des Eigentümers.

**Warum**

Eine gelockerte Grenze in einem großen Umbau fällt beim Lesen niemandem auf.

**Typische Fehlform**

Ein Test schlägt fehl, und die Grenze wird „vorübergehend“ angehoben.

**Lösungsraum**

Gate-Fehler als Hinweis auf die Ursache lesen. Muss die Grenze wirklich anders sein: eigene Änderung, eigener Zweck, neue Festschreibung.

**Vor der Änderung prüfen**

Ändert meine Änderung einen Wert, der in `kodex/festschreibung.txt` steht?

**Harter Bezug**

Kodex-Schritt Festschreibung; Pfadmatrix in `crates/depot_kodex/src/pfade.rs`.

---

# D06 – Erst buchen, dann fragen

**Aussage**

Jeder KI-Aufruf braucht vorher eine `AufrufErlaubnis`. Sie entsteht erst, wenn die Höchstkosten des Aufrufs dauerhaft im Kassenbuch stehen.

**Warum**

Eine Schleife, ein Neustart oder ein Absturz darf nie zu ungezählten Aufrufen führen.

**Typische Fehlform**

Die Kosten werden nach dem Aufruf aus der Antwort gezählt; stürzt das Programm vorher ab, war der Aufruf gratis und unbegrenzt.

**Lösungsraum**

Journal exklusiv öffnen, Kassenbuch daraus lesen, Grenzen prüfen, Buchung als Eintrag anhängen und synchronisieren, dann erst die Erlaubnis. Zählen nach Zyklus, Tag, Monat und Jahr, je KI getrennt.

**Vor der Änderung prüfen**

Gibt es nach meiner Änderung einen Weg zu Claude oder GPT ohne `AufrufErlaubnis`?

**Harter Bezug**

`crates/depot_ki/src/kassenbuch.rs`; Tests in `crates/depot_ki/tests/budget.rs`; Festschreibung `ki_budget`.

---

# D07 – Geld ist ganzzahlig

**Aussage**

Geld wird in Cent gerechnet, Anteile in Basispunkten. Gleitkomma gibt es nicht.

**Warum**

Gleitkomma rundet still. Bei Gebühren, Limits und Budgets summieren sich solche Fehler.

**Typische Fehlform**

`kurs * 1.01` für das Limit.

**Lösungsraum**

`Cent` mit geprüfter Rechnung, `anteil_bp_ab` und `anteil_bp_auf` mit bewusster Rundungsrichtung zugunsten der Sicherheit.

**Vor der Änderung prüfen**

Rundet meine Rechnung im Zweifel zu meinen Ungunsten (Kauf teurer, Verkauf billiger angenommen)?

**Harter Bezug**

Clippy `float_arithmetic` und `arithmetic_side_effects` als harte Lints; `crates/depot_kern/src/geld.rs`.

---

# D08 – Gespeicherte Bytes sind die Wahrheit

**Aussage**

Was nicht auf der Platte steht, ist nicht geschehen. Erfolg wird erst gemeldet, wenn der Eintrag im Journal geschrieben, synchronisiert und im Anhangsvermerk bestätigt ist.

**Warum**

Ein Absturz zwischen Handlung und Notiz erzeugt sonst doppelte Orders oder ungezählte Kosten.

**Typische Fehlform**

Zähler nur im Speicher; nach dem Neustart beginnt alles bei null.

**Lösungsraum**

Nur anhängen: Plan vermerken, Zeile synchronisieren, Stand vorrücken. Beim Öffnen die ganze Kette streng lesen. Gekürzt wird nur ein vermerkter, unvollständiger Anhang nach der Quarantäne; alles andere sperrt.

**Vor der Änderung prüfen**

Was passiert, wenn das Programm genau nach meiner Zeile abstürzt?

**Harter Bezug**

Test `neustart_setzt_nichts_zurueck` und `kaputtes_kassenbuch_sperrt`; `crates/depot_journal/src/datei.rs` mit Tests in `crates/depot_journal/tests/befunde.rs`.

---

# D09 – Abgleich mit dem Broker schlägt eigenes Gedächtnis

**Aussage**

Vor jedem Zyklus wird der eigene Stand mit dem Broker verglichen. Bei Abweichung wird nicht gehandelt.

**Warum**

Teilausführungen, Dividenden, Kapitalmaßnahmen oder manuelle Eingriffe verändern das Depot ohne Wissen des Programms.

**Typische Fehlform**

Das Programm rechnet mit 12 Stück, beim Broker liegen 11, und es verkauft 12.

**Lösungsraum**

Abgleich als eigener Schritt vor dem Regelwerk; Abweichung = Halt mit Nachricht.

**Vor der Änderung prüfen**

Woher kommt die Stückzahl, mit der mein Code rechnet?

**Harter Bezug**

Noch offen (Stufe Broker-Adapter). Bis dahin: kein Echtgeld.

---

# D10 – Kosten vor Rendite

**Aussage**

Jede Umschichtung muss ihre Gebühren erkennbar schlagen können. Kleine Änderungen und kleine Orders unterbleiben.

**Warum**

Bei kleinen Depots fressen Mindestgebühren den Ertrag. Viel Handeln ist der häufigste Weg, mit einer guten Idee Geld zu verlieren.

**Typische Fehlform**

Die KI schichtet jeden Monat 2 % um, und die Gebühren übersteigen den Vorsprung.

**Lösungsraum**

Mindestorder, Mindeständerung, Umschlaggrenze und Orderzahl je Zyklus im Regelwerk.

**Vor der Änderung prüfen**

Erhöht meine Änderung die Zahl der Orders oder den Umschlag?

**Harter Bezug**

Festschreibung `grenzen`; Tests `kleine_aenderung_unter_5_punkten` und `kleiner_kauf_unter_500`.

---

# D11 – Ein Backtest beweist nichts über die Zukunft

**Aussage**

Backtests dienen der Falsifikation, nicht dem Beweis. Regeln werden nie nachträglich an gespeicherte Ergebnisse angepasst.

**Warum**

Daran ist der Vorgänger KNN5 gescheitert: Data Snooping, Survivorship Bias, kein Vergleichsmaßstab.

**Typische Fehlform**

Zehn Varianten testen und die beste als „Strategie“ übernehmen.

**Lösungsraum**

Walk-Forward, Renditen statt Kursniveaus, Vergleich mit „DAX halten“, Kosten immer eingerechnet.

**Vor der Änderung prüfen**

Wurde diese Regel vor oder nach dem Blick auf die Ergebnisse festgelegt?

**Harter Bezug**

Noch offen (Stufe Backtest).

---

# D12 – Normative Tests sind synthetisch

**Aussage**

Tests, die Grenzen beweisen, benutzen erfundene, kontrollierte Lagen. Echte Kurse dienen der Bewertung, nie dem Beweis einer Grenze.

**Warum**

Ein Test, der an echten Kursen hängt, prüft Zufall und bricht, wenn die Daten sich ändern.

**Typische Fehlform**

„Der Test läuft mit den Kursen vom März grün“ als Beleg für eine Grenze.

**Lösungsraum**

Szenariotests mit festen Zahlen und ein Zufallstest mit festem Startwert über viele Lagen.

**Vor der Änderung prüfen**

Hängt mein Test von Marktdaten ab?

**Harter Bezug**

`crates/depot_regelwerk/tests/regelwerk.rs`.

---

# D13 – Billige Strukturprüfungen vor teuren Beweisen

**Aussage**

Erst Deklaration, Pfadrollen, Festschreibung und Quellscan; dann Format, Clippy und Tests.

**Warum**

Ein Strukturfehler soll in Sekunden auffallen, nicht nach einem langen Testlauf.

**Typische Fehlform**

Der Testlauf ist grün, aber die Änderung hat heimlich eine Grenze verschoben.

**Lösungsraum**

`run.cmd` vor jedem Zusammenführen; CI führt dasselbe auf jedem Pull Request aus.

**Vor der Änderung prüfen**

Habe ich `kodex/aenderung.txt` für genau diese Änderung neu geschrieben?

**Harter Bezug**

`crates/depot_kodex/src/lib.rs`.

---

# D14 – Der Kodex ändert sich allein

**Aussage**

Änderungen am Gate kommen nie zusammen mit Produktivcode.

**Warum**

Wer Gate und Code gleichzeitig ändert, kann das Gate passend zur eigenen Änderung lockern.

**Typische Fehlform**

Neue Funktion plus „kleine Anpassung“ am Quellscan im selben Pull Request.

**Lösungsraum**

Erst eine `KodexAenderung` (nur Gate und Doku), danach die `Funktion`.

**Vor der Änderung prüfen**

Berührt meine Änderung `crates/depot_kodex/` oder `kodex/` und zugleich anderen Code?

**Harter Bezug**

Pfadmatrix in `crates/depot_kodex/src/pfade.rs`.

---

# D15 – Kleinster korrekter Abschluss vor zusätzlicher Architektur

**Aussage**

Eine Stufe wird fertig gemacht, getestet und geschlossen, bevor die nächste beginnt.

**Warum**

Halbfertige Schichten erzeugen Lücken, durch die später Fehler laufen.

**Typische Fehlform**

Broker-Adapter, KI-Client und Backtest gleichzeitig halb bauen.

**Lösungsraum**

Reihenfolge: Kern und Regelwerk, KI-Budget, Backtest, Berater, Wächter, Papierhandel, Echtgeld.

**Vor der Änderung prüfen**

Ist die vorige Stufe wirklich geschlossen?

**Harter Bezug**

Klassen in `kodex/aenderung.txt`; Stufenplan im Architekturentwurf.

---

# D16 – Die Essenz hat keine zweite Wahrheit

**Aussage**

Die Essenz erklärt und berät. Sie kopiert keine Grenzwerte und erzeugt keine Erlaubnis.

**Warum**

Zwei Stellen mit denselben Zahlen laufen auseinander, und dann weiß niemand, welche gilt.

**Typische Fehlform**

„Laut Essenz sind 30 % erlaubt“, während der Code 25 % festschreibt.

**Lösungsraum**

Zahlen stehen im Code und in der Festschreibung. Die Essenz verweist darauf.

**Vor der Änderung prüfen**

Schreibe ich gerade eine Zahl in die Essenz, die schon im Code steht?

**Harter Bezug**

Rangfolge oben; Kodex-Schritt Festschreibung.

---

# D17 – Kaltstart ist eine Projektanforderung

**Aussage**

Ein neuer Bearbeiter muss allein aus dem Repository sicher weiterarbeiten können.

**Warum**

Threads enden, Gedächtnis geht verloren, Menschen vergessen.

**Typische Fehlform**

Eine Regel lebt nur im Chat.

**Lösungsraum**

`docs/kaltstart.md` als Einstieg, Essenz mit festen Abschnitten, Kodex prüft beides bei jedem Lauf. Ein Befehl (`run.cmd`) erledigt alles und schreibt am Ende immer `assistant_export_latest.zip`: Laufergebnis, Konsole, Übergabe und Quellstand in einer Datei, die eine neue KI-Sitzung ohne Vorgeschichte versteht.

**Vor der Änderung prüfen**

Würde ein neuer Bearbeiter meine Änderung ohne Rückfrage verstehen?

**Harter Bezug**

Kodex-Schritt Essenz-Kaltstart (`crates/depot_kodex/src/essenz.rs`); Assistenten-Export (`crates/depot_lauf/src/export.rs`).
