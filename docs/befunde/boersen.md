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

## Nicht in diesem Schritt

- **Abrufzeit:** Mit nur Xetra-Werten bleibt der Abruf kurz nach 17:35 Uhr richtig. Wolfgang hat am 05.11.2026 entschieden, Maus' Vorschlag zu folgen: Kurse je Börse kurz nach ihrem Schluss abrufen, die Altersprüfung je Abrufzeitpunkt, danach ein gemeinsamer Papiertag. Kein gemeinsamer Abruf um 22:30 Uhr. Das kommt in einem eigenen Schritt.
- **Rotation der Kandidaten und Tiingo als US-Quelle:** eigener PR, freigegeben am 05.11.2026.
- **Umrechnung in Euro** für Werte in Dollar: Voraussetzung für den ersten NYSE-Wert.

## Tests

- `depot_kern`: `boerse::xetra_kalender_2026`, `nyse_kalender_2026_und_verschiebungen`, `schluss_je_boerse`, `zeit::wochentage_und_ostern`, `new_york_mit_sommerzeit`, `tests/quelle.rs::schluss_der_nyse_erst_nach_new_york`
- `depot_regelwerk`: `nur_gehaltene_werte_sind_pflicht`
- `depot_sim`: `kandidat_ohne_kurs_faellt_aus_gehaltener_wert_haelt_den_zyklus`
- `depot_sammler`: `falsches_kuerzel_nur_fuer_seinen_wert` (Kandidat ausgelassen, verwendbar; gehalten, nicht verwendbar), `beobachtungsliste_gueltig` (Kürzel je Börse)
- `depot_lage`: `feiertag_ist_geschlossen_und_auslassung_sichtbar`
- `depot_dienst`: `zeitplan_ruht_an_boersenfeiertagen`
