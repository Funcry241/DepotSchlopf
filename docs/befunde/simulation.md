# Simulierter Lauf mit erfundenen Daten

Stufe 2. Wolfgang hat am 03.11.2026 die Reihenfolge freigegeben: Persistenzbasis, dann 1·R04, dann ein simulierter Lauf mit erfundenen Daten. Vorgabe: nur erfundene Daten, keine echten Schnittstellen, kein echtes Geld. Dachs und Maus beobachten ein erfundenes Bild aus Nachrichten, Terminen und Kursen, schreiben ihre Einschätzung und Begründung ins Journal und treffen Papierentscheidungen im Schutzrahmen. Kaufentscheidungen werden nicht als Regeln festgeschrieben. Schlank halten.

Ein grüner Test schließt nichts. „Technisch geschlossen“ setzt erst die Prüfung durch Maus.

## Fassung 2 nach Maus' Prüfung von `7e3a8e9` (K15-01, K15-02)

Maus' Befunde stehen als Tests im Repository, am Stand `7e3a8e9` rot (eigener Commit `efbb364`).

- **K15-01:** Der Wiederanlauf nahm eine geänderte Vorgeschichte an. Es reichte, dass das Datum des letzten Abschlusses irgendwo im Szenario vorkam; Zyklusnummern und Vergleichskurse wurden aus dem neuen Szenario nachgerechnet. Allgemeine Regel jetzt: Jeder Tagesabschluss trägt einen Abdruck der ganzen Vorgeschichte bis zu diesem Tag (`crates/depot_sim/src/abdruck.rs`): Positivliste, Startstand, Vorlauf und jeder Tag mit Datum, allen Kursen samt Herkunft und Zeitpunkt und allen Ereignissen, als Kette mit Feldlängen. Ein Wiederanlauf setzt nur fort, wenn das Szenario bis zum letzten Abschluss genau diesen Abdruck ergibt; sonst ist Halt, bevor etwas geschrieben oder jemand gefragt wird. Die Kette legt auch die Zahl der Tage fest, also Zyklusnummern und Vergleichskurse. Nur die Tage nach dem letzten Abschluss dürfen anders sein. Das Journal prüft nur die Form des Abdrucks (64 Hex-Zeichen, klein). Der Test `befund_k15_01_wiederanlauf_prueft_die_vorgeschichte` prüft sechs Änderungen und als Kontrolle einen geänderten Tag nach dem Abschluss.
- **K15-02:** Ein alter Kurssatz galt für einen neuen Handelstag als frisch, weil das Alter nur gegen den Zeitstempel des Abrufs gemessen wurde. Allgemeine Regel jetzt, im Regelwerk für jeden Lauf: `pruefe_kurse` bekommt den Handelstag des Zyklus. Der Abruf liegt an diesem Tag, jeder Kurs hat diesen Handelstag und einen Zeitpunkt an diesem Tag, und jeder Vergleichskurs stammt von einem früheren Handelstag. Sonst ist es ein Datenfehler, und der Zyklus fällt aus. Dazu `Zeitpunkt::am_tag`. Die Tests `befund_k15_02_kurse_gehoeren_zum_handelstag` (Regelwerk) und `befund_k15_02_alter_kurssatz_gilt_nicht_fuer_einen_neuen_tag` (Simulation) prüfen das; die Regelwerkstests nehmen ihren Vergleichskurs jetzt vom Vortag.

Gegenproben zu Fassung 2, jeweils neu übersetzt: Abdruck nicht verglichen, Kette nicht gekettet, Ereignisse, Startstand, Positivliste, Vorlauf oder Tageskurse nicht im Abdruck, Form des Abdrucks nicht geprüft, Abruf, Kurszeitpunkt oder Vergleichskurs nicht gegen den Handelstag geprüft. Jede macht einen der neuen Tests rot.

## Fassung 3 nach Maus' Nachprüfung von `316e575` (K15-03)

K15-01 und der ursprüngliche Fall von K15-02 hat Maus als behoben bestätigt. Maus' Gegenprobe steht als Test im Repository, am Stand `316e575` rot (eigener Commit `c55b608`).

- **K15-03:** Ein Vergleichskurs konnte aus der Zukunft stammen: ein Vorlaufkurs mit Handelstag 24.11. und Zeitpunkt 26.11. galt im Zyklus des 25.11. `Kurs::neu` nahm jeden Zeitpunkt ab dem Handelstag an, die Kursprüfung schaute beim Vergleichskurs nur auf den Handelstag. Allgemeine Regel jetzt, an zwei Stellen:
  - Ein Kurs liegt an seinem Handelstag, nie davor und nie danach. `Kurs::neu` lehnt jeden anderen Zeitpunkt ab (`KursFehler::Handelstag`). Damit gibt es den Kurs aus der Zukunft gar nicht erst, in keinem Lauf.
  - Die Kursprüfung nimmt nichts zum Vergleich, was es zum Abruf des Zyklus noch nicht gab: Liegt der Vergleichsabruf nach dem Abruf, ist das `VergleichsabrufNachAbruf`, und der Zyklus fällt aus, bevor ein Berater gefragt oder ein Auftrag belegt wird.
  - `Zeitpunkt::ab_tag` hatte keinen Nutzer mehr und ist entfernt.
- Tests: `befund_k15_03_kurszeitpunkt_liegt_am_handelstag` (Kern, mit Tagesanfang und Tagesende als Kontrolle), `befund_k15_03_vergleichsabruf_liegt_nicht_nach_dem_abruf` (Regelwerk), `befund_k15_03_vergleichskurs_aus_der_zukunft` (Simulation, Maus' Fall). Nach dem roten Commit ergänzt: `befund_k15_03_vergleichsabruf_aus_der_zukunft` (Simulation, Vorlaufkurse am 24.11., Abruf am 26.11.). Der Fall „Zeitpunkt am Folgetag“ im Regelwerkstest zu K15-02 entfällt, weil sich ein solcher Kurs nicht mehr bauen lässt; der Kerntest deckt ihn ab.
- Gegenproben, jeweils neu übersetzt: alte Prüfung in `Kurs::neu` (nur ab Handelstag) und Tagesende eingeschlossen machen den Kerntest rot; ohne Prüfung des Vergleichsabrufs werden der Regelwerkstest und der zweite Simulationstest rot. Ohne die Prüfung in `Kurs::neu` bleibt Maus' Simulationsfall grün, weil die Prüfung des Vergleichsabrufs ihn dann fängt: Ein Kurs liegt nie nach seinem Abruf. Beide Stellen halten also unabhängig.

## Was der Lauf festlegt und was nicht

Der Lauf schreibt keine Abwägung vor. Was gekauft wird, schlägt der Berater des Topfes vor und begründet jedes Ziel selbst. Der Rahmen hält nur den Schutz:

- **Kostenschutz (R8):** Jede Frage an einen Berater braucht eine `AufrufErlaubnis` aus `Journal::reserviere`. Ist eine Grenze erreicht, wird nicht gefragt; der Grund steht als Begründung von `Kostenschutz` im Journal.
- **Zweitmeinung (R31):** Den Vorschlag für Topf A prüft Maus, den für Topf B Dachs. Nur eine ausdrückliche Zustimmung lässt ihn weiter. Ohne Erlaubnis für die Zweitmeinung gibt es keinen freiwilligen Auftrag.
- **Regelwerk (D01):** `entscheide` entscheidet; jeder Befund steht als Begründung von `Regelwerk` im Journal.
- **Daten (D03):** `pruefe_kurse` gegen den letzten plausiblen Tag. Bei einem Fehler fällt der Zyklus aus; niemand wird gefragt. Ein gestörter Tag verschiebt den Vergleichskurs nicht.
- **Wächter:** Tagesverlust und unplausible Werte halten an, die Notbremse verkauft alles und hält an. In allen drei Fällen bleibt der Tag offen, also hält auch jeder neue Start (siehe Wiederanlauf), bis Wolfgang geprüft hat.
- **Nur erfundene Kurse:** Trägt ein Kurs eine andere Herkunft als `Simulation`, hält der Lauf an, bevor etwas geschrieben wird.

## Ausführung auf Papier

Jeder genehmigte Auftrag wird zuerst belegt (R12), dann zum Schlusskurs des Tages ausgeführt, wenn dieser das Limit einhält; sonst verfällt er. Verkäufe vor Käufen. Jede Ausführung kostet die Gebühr nach Tarif. Bargeld und Bestand werden nie negativ, sonst Halt. Danach steht der neue Topf im Journal.

Seit dem Schritt Kontowächter und Auftragszustände läuft die Ausführung über ein erfundenes Papierkonto: reservieren mit Deckung, als gesendet belegen, senden, Markt, Abgleich. Jede Ausführung steht als eigener Zustandseintrag im Journal und bucht den Topf. Käufe gehen erst raus, wenn alle Verkäufe des Topfes ausgeführt sind. Siehe `docs/befunde/konto.md`.

## Journal: Begründung und Tagesabschluss

Zwei neue Arten, gelesen und geschrieben wie alle anderen (kanonische Form, R20):

- `begruendung;datum;wer;text`: `wer` nicht leer, ohne `;` und ohne Rand-Leerzeichen; Text nicht leer. Steuerzeichen ersetzt der Lauf vorher durch Leerzeichen, so kann keine Antwort eine Zeile brechen.
- `abschluss;datum;hoechststand;abdruck`: Der Tag ist vollständig belegt. Daten streng steigend, Höchststand positiv, Abdruck der Vorgeschichte (K15-01).

`Stand::seit_abschluss` zählt die Einträge nach dem letzten Abschluss, auch KI-Buchungen.

## Wiederanlauf (Kap. 11)

Der Lauf setzt am Tag nach dem letzten Abschluss fort, wenn das Szenario bis dahin denselben Abdruck ergibt (K15-01). Der Höchststand kommt aus dem Abschluss, der Vergleichskurs aus der so geprüften Vorgeschichte. Ein Lauf in zwei Teilen ergibt byte-gleich dasselbe Journal wie ein ununterbrochener Lauf.

Steht nach dem letzten Abschluss schon ein Eintrag, war ein Tag angefangen. Dann ist Halt, und niemand wird neu gefragt: Was der angefangene Tag gekostet hat, steht im Journal, und ihn zu wiederholen wäre eine zweite Frage im selben Zyklus. Das ist bewusst streng. Wie Wolfgang einen angefangenen Tag freigibt, kommt mit dem echten Papierbetrieb.

## Das Beispiel

`Szenario::beispiel()`: drei erfundene Werte (Alpha Werke, Beta Bau, Gamma Netz, ISIN-Land `XA`), zwei Töpfe zu je 10.000 € Papiergeld.

| Tag | Ereignis | Was geschieht |
| --- | --- | --- |
| 25.11. | Quartalszahlen und Zinsentscheid angekündigt | Beide steigen klein ein, gegenseitige Zustimmung |
| 26.11. | Alpha über den Erwartungen, +7,8 % | Dachs will 40 % Alpha, das Regelwerk kürzt auf 25 %; Dachs legt gegen Maus' Alpha-Kauf ein Veto ein |
| 27.11. | Leitzins unverändert | Monat voll: niemand wird gefragt |
| 30.11. | Datenstörung, Gamma +45 % | Kurssprung: Zyklus fällt aus |
| 01.12. | Hauptversammlung angekündigt | Neuer Monat, beide werden gefragt, keiner will etwas ändern |

## Nachweis

Rote Tests in eigenem Commit `6d7c3ad` mit leerer Schnittstelle: zwölf in `crates/depot_sim/tests/lauf.rs`, fünf in `crates/depot_journal/tests/tag.rs`. Zwei Ablehnungstests waren dort schon grün, weil die leere Schnittstelle alles ablehnt.

Gegenproben am fertigen Stand, jeweils neu übersetzt, mit dem Test, der rot wird:

| Bruch | roter Test |
| --- | --- |
| angefangener Tag wird nicht erkannt | `unvollstaendiger_tag_haelt_ohne_neue_frage`, `tagesverlust_haelt_bis_zur_freigabe` |
| Veto wird übergangen | `veto_sperrt_freiwillige_auftraege` |
| Herkunft wird nicht geprüft | `echte_kurse_werden_abgelehnt` |
| gestörter Tag wird Vergleichskurs | `unplausible_kurse_lassen_den_zyklus_ausfallen` und zwei weitere |
| Steuerzeichen bleiben stehen | `steuerzeichen_in_antworten_brechen_das_journal_nicht` |
| Wächter wird übergangen | `tagesverlust_haelt_bis_zur_freigabe` |
| Notbremse verkauft nicht | `notbremse_verkauft_alles_und_haelt` (nach dem roten Commit ergänzt) |
| Begründung je Ziel fehlt | `beispiel_laeuft_durch_und_steht_im_journal` |
| Abschluss darf rückwärts | `abschluss_nur_vorwaerts_und_mit_positivem_hoechststand`, `nicht_kanonische_eintraege_halten_beim_oeffnen` |
| Höchststand null erlaubt | dieselben |
| `seit_abschluss` zählt nicht oder setzt nicht zurück | `eintraege_seit_dem_abschluss_werden_gezaehlt` |
| leere Begründung erlaubt | `unzulaessige_begruendung_wird_abgelehnt`, `nicht_kanonische_eintraege_halten_beim_oeffnen` |

## Grenzen

- Die Berater sind Drehbücher, keine echten Modelle. Die Anbindung von Claude und GPT kommt später und braucht nur `Berater` umzusetzen.
- Ausführung nur zum Schlusskurs, ohne Teilausführung und ohne Schlupf.
- Die Notbremse ist im Beispiel nicht ausgelöst; sie hat einen eigenen Test mit erfundenem Kursrückgang.
