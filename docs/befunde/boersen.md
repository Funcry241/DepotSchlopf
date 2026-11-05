# Börsenkalender, Schlusszeit je Wert, nur gehaltene Werte Pflicht

Wolfgangs drei Punkte vom 05.11.2026, nach dem Merge von #33:

1. Ein Lauf um 17:35 Uhr sieht keinen US-Schluss.
2. 25 Abrufe am Tag bei Alpha Vantage: Was nicht abgerufen werden kann, muss sichtbar ausfallen, nicht den Tag kippen.
3. Börsenfeiertage in Deutschland und den USA dürfen nicht als verpasst erscheinen.

## Befund vorher

- **Schluss:** `schlusskurse` kannte nur den Xetra-Schluss um 17:35 Uhr. Ein Wert der NYSE hätte nach 17:35 Uhr Frankfurter Zeit als geschlossen gegolten, obwohl in New York noch gehandelt wird. Ein laufender Kurs wäre als Schlusskurs belegt worden. Heute steht kein US-Wert in der Liste, aber das Problem käme mit dem ersten.
- **Ein Wert kippt den Tag:** `pruefe_kurse` verlangte für jeden Wert der Liste einen gültigen Kurs. Ein falsches Kürzel, ein nicht abgerufener Wert (Tageskontingent) oder ein unplausibler Sprung bei irgendeinem Kandidaten ließ den ganzen Zyklus ausfallen.
- **Feiertage:** Zeitplan und Rückblick kannten nur Montag bis Freitag. An Heiligabend oder Karfreitag lief der Zeitplan an, fand keine Kurse von heute, und der Rückblick zeigte den Tag als ausgefallen.

## Umbau

### Börse je Wert (`depot_kern::boerse`)

| Börse | Schluss | Geschlossen |
|---|---|---|
| Xetra | 17:35 Uhr Frankfurt (MEZ/MESZ) | Neujahr, Karfreitag, Ostermontag, 1. Mai, 24., 25., 26. und 31. Dezember |
| NYSE | 16:00 Uhr New York (EST/EDT), an verkürzten Tagen 13:00 Uhr: 3. Juli, Tag nach Thanksgiving, 24. Dezember, jeweils nur wenn gehandelt wird | New Year's Day, Martin Luther King Jr. Day, Washington's Birthday, Good Friday, Memorial Day, Juneteenth (seit 2022), Independence Day, Labor Day, Thanksgiving, Christmas Day |

- Feste NYSE-Feiertage am Samstag sind am Freitag davor frei, am Sonntag am Montag danach. Ausnahme: Neujahr am Samstag wird nicht nachgeholt, der 31. Dezember bleibt Handelstag.
- Ostern nach Meeus, Jones und Butcher, nur geprüft gerechnet.
- `Zeitpunkt::new_york` rechnet die US-Sommerzeit: vom zweiten Sonntag im März bis zum ersten Sonntag im November, Wechsel um 02:00 Uhr Ortszeit. In den Wochen, in denen Europa und die USA verschieden umstellen, liegt der NYSE-Schluss um 21:00 Uhr Frankfurter Zeit statt um 22:00 Uhr.
- `liste::Wert` trägt seine Börse. Kürzel: Xetra mit `.DEX`, NYSE ohne Endung, höchstens fünf Buchstaben und eine Klasse wie `BRK.B`.
- `schlusskurse` nimmt den Schluss der Börse des Werts. Ein Abruf um 17:40 Uhr sieht für einen NYSE-Wert den Schluss des Vortags.
- Ein Wert in Dollar ergibt weiterhin keinen Kurs (`KursFehler::Waehrung`). Er braucht einen geprüften Umrechnungskurs, bevor er in die Liste kommt. Das ist ein eigener Schritt.

### Nur gehaltene Werte sind Pflicht (`depot_regelwerk::waehle_tageswerte`)

Die Kursprüfung bleibt dieselbe. Neu ist, wie ihre Befunde wirken:

| Befund | Wirkung |
|---|---|
| zum Abruf (`AbrufNichtAmHandelstag`, `VergleichsabrufNachAbruf`) | Zyklus fällt aus, niemand wird gefragt |
| zu einem gehaltenen Wert | Zyklus fällt aus: Ohne seinen Kurs ist das Depot nicht zu bewerten |
| zu einem Kandidaten | nur dieser Wert ist heute nicht handelbar. Er fällt aus dem Universum des Tages, der Zyklus läuft ohne ihn |
| kein Wert handelbar und nichts gehalten (`KeinWertHandelbar`) | Zyklus fällt aus. Es gäbe nichts zu entscheiden, und die KI-Kosten wären verschwendet |

- Ein ausgelassener Kandidat steht im Journal unter `Auswahl` („heute nicht handelbar: …“). Das Lagebild nennt ihn in der Bereitschaft und im Rückblick („Papiertag abgeschlossen; heute nicht handelbar und ausgelassen: 2× Kurs fehlt“). Der Sammler druckt ihn aus.
- Dachs und Maus sehen nur die handelbaren Werte des Tages. Ein Vorschlag für einen ausgelassenen Wert wird vom Regelwerk wie jeder Wert außerhalb des Universums abgelehnt.
- Der Sammler meldet Erfolg (K19-02), wenn kein Befund den Zyklus ausfallen ließe. Pflicht sind auch dort nur die Werte, die das Journal als gehalten führt.
- Ein gehaltener Wert außerhalb des Universums (zwingender Ausstieg) wird wie bisher nicht hier geprüft. Fehlt sein Kurs, hält der Wächter an.
- Keine Grenze ist geändert. Hoechstalter (20 Minuten), Kurssprung und Vergleichskurs gelten unverändert für jeden Wert, der gehandelt wird.

### Geschlossen statt verpasst

- `liste::handelstag(d)`: Handelt an diesem Tag wenigstens eine Börse der Liste? `liste::feiertag(d)` nennt den Grund, etwa „Xetra: Heiligabend“.
- Der Zeitplan startet an einem geschlossenen Tag nichts. Der nächste geplante Lauf ist der nächste Tag, an dem gehandelt wird.
- Der Rückblick zeigt einen Feiertag als „Börse geschlossen (Xetra: Heiligabend): kein Papiertag nötig“, grau statt rot.
- Die Bereitschaft verlangt an einem geschlossenen Tag keinen Abruf.

## Fassung 2 nach Maus' Gegenproben

1. **Sicherheitsabbruch:** Kam erst ein gültiger Kurs und danach eine Antwort mit dem Schlüssel, endete der Lauf. Die bis dahin belegten Kurse galten aber als verwendbar, weil fehlende Kandidaten nicht mehr zählen. Damit hätte der Zeitplan den Papiertag starten können. Jetzt setzt ein Schutzabbruch `Bericht::sicherheitsabbruch`, und nichts aus dem Lauf ist verwendbar. Ein Abbruch wegen des Tageskontingents bleibt ein fehlender Kandidat. Test: `schluesselabbruch_macht_den_ganzen_abruf_unbrauchbar`.
2. **Feiertagszeile:** Der Kalender galt nur im Zeitplan. Eine Xetra-Zeile für den 1. Januar wurde als Schlusskurs gelesen. Jetzt lehnt `schlusskurse` jeden Wert ab, dessen Datei eine Zeile für einen Tag ohne Handel seiner Börse enthält (`QuellenFehler::KeinHandelstag`, Wochenende eingeschlossen). Die Quelle widerspricht dann dem Kalender, und das klärt ein Mensch. Ist der Wert ein Kandidat, fällt er aus. Ist er gehalten, fällt der Zyklus aus. Test: `zeile_an_einem_boersenfeiertag_ist_kein_schlusskurs`.

## Fassung 3 nach Maus' Restbefund

Der Sicherheitsabbruch griff nur über den Ausgang des Sammlers, also nur im Zeitplan. Ein von Hand gestarteter Papiertag nahm die schon belegten Teilkurse trotzdem. Jetzt legt der Sammler nach einem Sicherheitsabbruch `daten/kurse.halt` an (`depot_journal::kurssperre`). Darin steht ein fester Grund, nie der Schlüssel. Solange die Datei liegt, gilt:

- Der Sammler ruft nichts ab.
- `papierlauf::papier` beginnt keinen Papiertag, gleich ob der Zeitplan oder ein Mensch ihn startet. Beide Wege laufen über `run.cmd papier`.
- Das Lagebild zeigt „Kurssperre“ als Halt.

Aufheben heißt: prüfen, den Schlüssel bei Bedarf erneuern, dann in der Einrichtung unter „Kurssperre“ aufheben, wie die Sperre des Kostenschutzes (Abdruck der gesehenen Fassung, ausdrückliche Bestätigung, nur ohne laufenden Auftrag; Grund bleibt in `daten/kurse_halt_aufgehoben.txt`). Nie von Hand löschen. Lässt sich die Sperre nicht schreiben, meldet der Sammler das als Fehler: „KEINEN Papiertag starten“. Tests: `kurssperre_haelt_auch_den_handstart`, `kurssperre::sperre_liegt_bis_sie_geloescht_wird`, `kurssperre_ist_halt`, `kurssperre_gefuehrt_aufheben`.

## Fassung 4 nach Maus' Restbefund (3f0240b)

Ließ sich die Kurssperre nicht schreiben, lief ein Papiertag von Hand weiter mit den schon belegten Teilkursen. Die Ursache lag im Sammler: Er belegte jeden Kurs gleich nach seinem Abruf. Jetzt sammelt er die Belege während des Laufs und schreibt sie erst ins Journal, wenn alle Abrufe ohne Sicherheitsabbruch zu Ende sind. Die Reihenfolge bleibt die des Abrufs. Nach einem Sicherheitsabbruch kommt kein Kurs dieses Laufs ins Journal, und der Bericht nennt die schon gelesenen Werte „Nicht belegt: Sicherheitsabbruch“. Die rohen Antworten ohne Schlüssel bleiben zur Durchsicht unter `daten/roh/`. Die Kurssperre bleibt als zweite Schranke, und sie hält den nächsten Abruf an. Test: `schluesselabbruch_macht_den_ganzen_abruf_unbrauchbar` (Journal ohne Kurse).

## Abruf je Börse, Schritt 2: Dollar in Euro über die EZB

Ein Kurs gilt nur in Euro. Ein Wert der NYSE bekommt einen Kurs erst über `schlusskurse_in_euro` mit den Referenzkursen der EZB (`depot_kern::devisen`, gelesen aus `eurofxref-hist-90d.xml`, nur der Dollar). Regeln:

- Umgerechnet wird mit dem Referenzkurs **desselben Tages**. Die EZB veröffentlicht ihn an Geschäftstagen gegen 16:00 Uhr Frankfurt, vor dem Schluss der NYSE. Fehlt er, ergibt der Wert keinen Kurs (`QuellenFehler::KeinDevisenkurs`); ein älterer Tag ersetzt ihn nie. An TARGET-Feiertagen, an denen die NYSE handelt (1. Mai, 26. Dezember), fällt ein NYSE-Wert damit als Kandidat aus und steht unter „Auswahl“.
- Heute und Vortag werden je mit dem Kurs ihres eigenen Tages umgerechnet.
- Euro = Dollar durch Referenzkurs, exakt gerechnet und auf sechs Nachkommastellen abgeschnitten. Der Fehler liegt unter 0,000001 € je Stück.
- Ein kaputter Eintrag im EZB-Text verwirft den ganzen Text (`DevisenFehler`), statt still zu fehlen.
- `schlusskurse` ohne Referenzkurse lehnt den Dollar weiter ab (`KursFehler::Waehrung`).

**Nach Maus' Gegenproben zu aba39b0:** Ein abgeschnittener Text ergab einen Kurs, und ein Dollarkurs außerhalb seines Tagesblocks wurde dem zuletzt gelesenen Tag zugeschlagen. Jetzt prüft `lies_ezb` den ganzen Aufbau, bevor ein Kurs gilt: genau eine Hülle `gesmes:Envelope`, jedes Element in der Reihenfolge geschlossen, in der es geöffnet wurde, nichts nach der Hülle; darin genau ein `Cube` ohne Attribute, darin nur Tage (`Cube` mit genau `time`), darin nur Währungen (`Cube` mit genau `currency` und `rate`, ohne Inhalt). Ein Dollarkurs gehört nur zu dem Tag, in dessen Block er steht; außerhalb ist er `DevisenFehler::OhneTag`, jeder andere Verstoß `DevisenFehler::Aufbau`, und der ganze Text wird verworfen. Tests: `abgeschnittener_text_ergibt_keine_kurse` (jede Schnittstelle), `dollar_nur_im_eigenen_tagesblock`.

**Nach Maus' Restbefund zu f267b0f:** Bei `rate="1.2" rate='2.4'` galt der erste Wert. Jetzt liest `lies_ezb` jedes Tag vollständig: Name, dann jedes Attribut als `name="wert"` oder `name='wert'` (Leerraum davor, um `=` erlaubt). Ein Name zweimal, ohne Rücksicht auf Anführungszeichen, Leerraum oder Groß- und Kleinschreibung, ein Rest, der kein Attribut ist, ein offenes Anführungszeichen oder ein unbekanntes Attribut an einem `Cube` verwirft den ganzen Text (`Aufbau`). Das gilt für jedes Element, auch für die Hülle. Test: `attribute_vollstaendig_und_eindeutig`.

Noch ohne Netz: Den EZB-Text holt im nächsten Schritt der Sammler, zusammen mit Tiingo und dem Zeitplan je Börse. Tests: `tests/devisen.rs` (`liest_dollar_je_tag`, `kaputter_text_ergibt_keine_kurse`, `umrechnung_nur_am_selben_tag_abgerundet`, `nyse_in_euro`, `xetra_bleibt_unveraendert`); der echte EZB-Text vom 05.11.2026 wurde einmal von Hand gelesen (USD 1,1224).

## Abruf je Börse, Schritt 3: Tiingo und EZB im Sammler

- **Quelle je Börse:** Xetra-Werte kommen weiter von Alpha Vantage, Werte der NYSE von Tiingo (`depot_kern::liste::quelle`, `depot_sammler::Anbieter`). Jede Quelle hat ihren eigenen Schlüssel in der Windows-Anmeldeverwaltung (`DepotSchlopf/alphavantage`, `DepotSchlopf/tiingo`). Der Tiingo-Schlüssel besteht aus 20 bis 64 Kleinbuchstaben und Ziffern. Fehlt der Schlüssel einer Quelle, werden nur ihre Werte nicht abgerufen; der Grund steht im Bericht, und ein Kandidat fällt nur aus der Auswahl.
- **Schlüsselschutz:** Jede Antwort, auch die der EZB, wird gegen alle Schlüssel geprüft, gleich in welcher Schreibweise. Jede Meldung wird von allen bereinigt. Ein Treffer ist ein Sicherheitsabbruch wie bisher: nichts abgelegt, nichts belegt, Kurssperre.
- **Tiingo-Abruf:** `https://api.tiingo.com/tiingo/daily/<kürzel>/prices?startDate=<14 Tage zurück>&format=csv&columns=date,close,high,low&token=…`. Das Kürzel steht klein, eine Klasse mit Bindestrich (`BRK.B` wird `brk-b`). Die Antwortform stammt aus der Dokumentation von Tiingo. Aus der Cloud-Umgebung ist api.tiingo.com gesperrt, deshalb bestätigt sie erst der erste echte Lauf. Weicht sie ab, wird der Wert als unlesbar gemeldet und nicht belegt.
- **Referenzkurse:** Enthält der Lauf einen Wert in Dollar, dessen Schlüssel vorhanden ist, holt der Sammler vorher einmal `eurofxref-hist-90d.xml` und legt die Datei roh unter `daten/roh/ezb-referenzkurse/` ab. Belegt wird der Schluss in Euro (`schlusskurse_in_euro`), unter der Quelle `tiingo`. Lässt sich die EZB-Datei nicht holen oder lesen, wird kein Dollarwert abgerufen; Xetra bleibt unberührt.
- **Lauf je Börse:** `run.cmd kurse xetra` und `run.cmd kurse nyse` rufen nur die Werte einer Börse ab, `run.cmd kurse` alle. Der Sammler prüft nur die Werte seines Laufs, Pflicht sind die gehaltenen Werte dieser Börse; alle zusammen prüft der Papiertag. Den Zeitplan je Börse bringt der nächste Schritt.

Tests: `depot_sammler/tests/tiingo.rs` (`schluessel_von_tiingo`, `abrufadresse_tiingo`, `nyse_von_tiingo_in_euro`, `ohne_tiingo_schluessel_nur_xetra`, `ohne_referenzkurse_kein_dollarwert`, `schluessel_in_antwort_ist_sicherheitsabbruch`).

## Nicht in diesem Schritt

- **Abrufzeit:** Mit nur Xetra-Werten bleibt der Abruf kurz nach 17:35 Uhr richtig. Wolfgang hat am 05.11.2026 entschieden, Maus' Vorschlag zu folgen: Kurse je Börse kurz nach ihrem Schluss abrufen, die Altersprüfung je Abrufzeitpunkt, danach ein gemeinsamer Papiertag. Kein gemeinsamer Abruf um 22:30 Uhr. Schritt 1 ist umgesetzt: Das Alter eines Kurses misst sich an seinem eigenen Abruf (`Kurse::mit_abrufen`), Test `alter_je_abruf_nicht_je_spaetestem_abruf`. Zeitplan je Börse und Papiertag nach dem letzten Abruf kommen mit Tiingo, wenn die ersten US-Werte in der Liste stehen.
- **Rotation der Kandidaten und Tiingo als US-Quelle:** eigener PR, freigegeben am 05.11.2026.
- **Zeitplan je Börse, Tiingo-Schlüssel in der Einrichtung, erste US-Werte in der Liste:** nächster Schritt im Dienst.

## Tests

- `depot_kern`: `boerse::xetra_kalender_2026`, `nyse_kalender_2026_und_verschiebungen`, `schluss_je_boerse`, `zeit::wochentage_und_ostern`, `new_york_mit_sommerzeit`, `tests/quelle.rs::schluss_der_nyse_erst_nach_new_york`
- `depot_regelwerk`: `nur_gehaltene_werte_sind_pflicht`
- `depot_sim`: `kandidat_ohne_kurs_faellt_aus_gehaltener_wert_haelt_den_zyklus`
- `depot_sammler`: `falsches_kuerzel_nur_fuer_seinen_wert` (Kandidat ausgelassen, verwendbar; gehalten, nicht verwendbar), `beobachtungsliste_gueltig` (Kürzel je Börse)
- `depot_lage`: `feiertag_ist_geschlossen_und_auslassung_sichtbar`
- `depot_dienst`: `zeitplan_ruht_an_boersenfeiertagen`
