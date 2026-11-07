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

**Nach Maus' Befunden zu 5b70fa5:**
1. Ein Lauf für eine einzige Börse lud nur deren Schlüssel. Eine Antwort von Alpha Vantage mit dem Tiingo-Schlüssel wäre so abgelegt worden. Jetzt lädt jeder Lauf **alle** hinterlegten Schlüssel für den Schutz (`zugang_laden`). Ein fehlender Schlüssel wird nur gemeldet, wenn ein Wert des Laufs ihn braucht.
2. Ein Tiingo-Erfolg stand im Anbieterprotokoll unter der Kursquelle, die die Oberfläche Alpha Vantage zuordnet. Jetzt bucht der Sammler je Quelle (`einordnen(bericht, quelle)`, neuer Eintrag `tiingo` im Anbieterprotokoll). Gezählt werden nur abgerufene Werte dieser Quelle. Ein Wert, der wegen eines fehlenden Schlüssels oder fehlender Referenzkurse nicht abgerufen wurde, heißt `Uebersprungen` und zählt für keine Quelle.

Tests: `einzelboerse_kennt_alle_schluessel`, `anbieterprotokoll_je_quelle`, `depot_sammler/tests/tiingo.rs` (`schluessel_von_tiingo`, `abrufadresse_tiingo`, `nyse_von_tiingo_in_euro`, `ohne_tiingo_schluessel_nur_xetra`, `ohne_referenzkurse_kein_dollarwert`, `schluessel_in_antwort_ist_sicherheitsabbruch`).

## Abruf je Börse, Schritt 4: Zeitplan je Börse

Ab diesem Schritt ruft der Dienst jede Börse der Liste an ihren Handelstagen kurz nach ihrem eigenen Schluss ab, nur mit ihren Werten (`Art::KurseVon`, `run.cmd kurse xetra|nyse`):

- **Xetra:** zur eingestellten Uhrzeit „Kurse Xetra“, wie bisher 17:45 Uhr.
- **Jede andere Börse:** `NACHLAUF_MIN` = 10 Minuten nach ihrem Schluss an diesem Tag, in Frankfurter Zeit (`zeitplan::abrufe`). Für die NYSE ist das meist 22:10 Uhr. In den Wochen, in denen nur Europa schon Winterzeit hat, ist es 21:10 Uhr, an verkürzten Tagen 19:10 Uhr. Feste Uhrzeiten bleiben so innerhalb des Höchstalters von 20 Minuten.
- **Feiertage:** Ein Tag, an dem nur eine Börse handelt, hat nur deren Abruf, zum Beispiel Thanksgiving nur Xetra und Heiligabend nur die NYSE.
- **Papiertag:** Er beginnt nach dem letzten Abruf des Tages, wenn wenigstens ein Abruf dieses Tages erfolgreich war (`werkbank::papier_nach_abruf`). Welche Kurse er verwendet, prüft er selbst. Ein gescheiterter Abruf einer Börse hält ihn nicht auf; ihre Kandidaten fallen aus der Auswahl, ein gehaltener Wert ohne Kurs lässt den Zyklus wie bisher ausfallen.
- **Einmal je Tag:** `daten/dienst_erledigt.txt` führt jetzt je Börse den Tag ihres Abrufs (`erledigt-v2` mit `kurse_nyse`). Die frühere Form `erledigt-v1` wird gelesen, `kurse` gilt dort für Xetra.
- **Von Hand:** Der Knopf „Kurse abrufen“ holt weiter alle Werte der Liste.

Solange die Liste nur Xetra-Werte hat, läuft alles wie bisher: Termine 17:40 Uhr, Xetra 17:45 Uhr, danach der Papiertag. Tests: `kurse_je_boerse_nach_ihrem_schluss`, `erledigt_frueherer_form`, `papiertag_nach_dem_letzten_abruf_des_tages` (alle vier Ausgänge von Xetra und NYSE).

## Abruf je Börse, Schritt 5: Tiingo-Schlüssel und erste US-Werte

- **Liste:** fünf Werte der NYSE kommen dazu, neben den zehn von Xetra: JPMorgan Chase (`JPM`, US46625H1005), Coca-Cola (`KO`, US1912161007), Johnson & Johnson (`JNJ`, US4781601046), Exxon Mobil (`XOM`, US30231G1022) und Visa (`V`, US92826C8394). Alle sind an der NYSE notiert, nicht an der Nasdaq, deren Kalender der Kern noch nicht kennt. Die ISIN-Prüfziffern prüft `pruefe_liste`.
- **Kontingent je Quelle als Regel:** `pruefe_liste` lehnt eine Liste ab, die einer Quelle mehr Werte gibt, als ein Lauf abrufen darf (`liste::hoechstens_werte`): Alpha Vantage 24 (25 Abrufe am Tag, einer davon für die Nachrichten des Xetra-Laufs, `WEITERE_ABRUFE_ALPHA_VANTAGE`), Tiingo 500 (verschiedene Kürzel im Monat, die engere Grenze der kostenlosen Stufe). Vorher prüfte das nur ein Test, und nur die Gesamtzahl.
- **Schlüssel in der Einrichtung:** `DepotSchlopf/tiingo` lässt sich wie die anderen dort hinterlegen (`schluessel::Wer::KursquelleUs`, 20 bis 64 Kleinbuchstaben und Ziffern, dieselbe Regel wie im Sammler). Das Lagebild zeigt den Punkt „Kursquelle Tiingo (NYSE)“, sobald die Liste einen Wert dieser Quelle hat; fehlt der Schlüssel, ist der nächste Schritt, ihn zu hinterlegen, gleich nach Alpha Vantage. Ohne ihn läuft Xetra weiter, die NYSE-Kandidaten fallen aus der Auswahl.
- **Ablauf ab jetzt:** Termine 17:40 Uhr, Xetra 17:45 Uhr, NYSE meist 22:10 Uhr, danach der Papiertag. An Heiligabend handelt die NYSE bis 19:00 Uhr Frankfurt; der Tag ist damit ein Handelstag mit nur dem NYSE-Abruf um 19:10 Uhr und erscheint im Rückblick als verpasst, wenn kein Abruf kam.

Tests: `zeitplan_der_liste_ruft_xetra_und_nyse_ab`, `fehlender_schluessel_von_tiingo_ist_offen`, `schluesselregeln`, `beobachtungsliste_gueltig` (mit 26 Xetra-Werten abgelehnt), `feiertag_ist_geschlossen_und_auslassung_sichtbar` (Heiligabend jetzt Handelstag). Die älteren Zeitplan-Tests prüfen ausdrücklich nur Xetra (`faellig_x`, `naechster_x`).

## Nicht in diesem Schritt

- **Abrufzeit:** Mit nur Xetra-Werten bleibt der Abruf kurz nach 17:35 Uhr richtig. Wolfgang hat am 05.11.2026 entschieden, Maus' Vorschlag zu folgen: Kurse je Börse kurz nach ihrem Schluss abrufen, die Altersprüfung je Abrufzeitpunkt, danach ein gemeinsamer Papiertag. Kein gemeinsamer Abruf um 22:30 Uhr. Schritt 1 ist umgesetzt: Das Alter eines Kurses misst sich an seinem eigenen Abruf (`Kurse::mit_abrufen`), Test `alter_je_abruf_nicht_je_spaetestem_abruf`. Zeitplan je Börse und Papiertag nach dem letzten Abruf kommen mit Tiingo, wenn die ersten US-Werte in der Liste stehen.
- **Rotation der Kandidaten und Tiingo als US-Quelle:** eigener PR, freigegeben am 05.11.2026.
- **Rotation der Kandidaten:** Mehr Werte, als das Kontingent von Alpha Vantage an einem Tag erlaubt, brauchen eine Rotation; die kommt in einem eigenen Schritt.
- **Tiingo-Antwortform:** bestätigt erst der erste echte Lauf (aus der Cloud-Umgebung gesperrt).

## Tests

- `depot_kern`: `boerse::xetra_kalender_2026`, `nyse_kalender_2026_und_verschiebungen`, `schluss_je_boerse`, `zeit::wochentage_und_ostern`, `new_york_mit_sommerzeit`, `tests/quelle.rs::schluss_der_nyse_erst_nach_new_york`
- `depot_regelwerk`: `nur_gehaltene_werte_sind_pflicht`
- `depot_sim`: `kandidat_ohne_kurs_faellt_aus_gehaltener_wert_haelt_den_zyklus`
- `depot_sammler`: `falsches_kuerzel_nur_fuer_seinen_wert` (Kandidat ausgelassen, verwendbar; gehalten, nicht verwendbar), `beobachtungsliste_gueltig` (Kürzel je Börse)
- `depot_lage`: `feiertag_ist_geschlossen_und_auslassung_sichtbar`
- `depot_dienst`: `zeitplan_ruht_an_boersenfeiertagen`

## Geheimnisfreie Diagnose einer bestehenden Kurssperre

Ein echtes Abrufprotokoll vom 07.11.2026 belegt den Sicherheitsabbruch, enthält aber absichtlich keine verworfene Antwort. Daraus lässt sich die Ursache nicht nachträglich ableiten. Die neue Meldung nennt bei Kurs- und EZB-Antworten ausschließlich feste Metadaten: antwortende Quelle, den Namen des Zugangs mit einem vollständigen Schlüsseltreffer (auch nach JSON-Dekodierung) und ein grobes Antwortformat. Keine Antwortausschnitte, Werte, Teilschlüssel, Längen oder Fingerabdrücke. Auch feste Diagnosetexte werden gegen alle geladenen Zugänge bereinigt.

Die Einrichtung bietet bei vorhandener Sperre einen ausdrücklich gestarteten, rein lesenden Prüfabruf an. Dies ist die einzige Abrufausnahme von der Kurssperre: keine Aufhebung, kein Journal, keine Rohablage, keine Kursbelege, kein Anbieter-Erfolgsbeleg, keine KI, keine Orders. Ohne Sperre beginnt kein Abruf. Alle vorhandenen Kurszugänge werden geladen, unabhängig von der antwortenden Quelle. Es gelten dieselben freigegebenen HTTPS-Adressen, Antwortgrößen und Zeitgrenzen. Je Listenwert nur ein Versuch, 15 Sekunden Abstand, sofortiger Halt beim ersten Schlüsseltreffer. Fehlermeldungen aus dem Netz werden nicht übernommen. Der Prüfabruf verbraucht Kurskontingent; UI und Protokoll sagen dies vorab. Ein fehlender Treffer ist keine Entwarnung und entsperrt niemals. Der Auftrag nutzt dieselbe Warteschlange, Herkunfts-/Tokenprüfung, Doppelklicksperre und Protokollbereinigung wie andere manuelle Aufträge, einschließlich Start aus geprüften Binärdateien ohne Cargo.

Nachweise: `diagnose`-Unit-Tests (fremder/JSON-kodierter Zugang, Antworttypen, Kollision mit festen Namen), `tests/diagnose.rs` (Stop, Abstände, Metadaten, fehlende Zugänge, Netzfehler und keine Entwarnung), `schluessel_in_antwort_ist_sicherheitsabbruch` (Kurs/EZB-Diagnose und keine Teilkurse), `kurssperre_pruefabruf_ist_getrennt_und_doppelklickfest` (bestehende Sperre, manuelle Warteschlange, Doppelklick). Die konkrete ursprüngliche Antwort kann nur ein erneuter Abruf vor Ort untersuchen; Zugangsdaten werden nicht für die Entwicklung benötigt.
