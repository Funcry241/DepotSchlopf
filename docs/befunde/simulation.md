# Simulierter Lauf mit erfundenen Daten

Stufe 2. Wolfgang hat am 03.11.2026 die Reihenfolge freigegeben: Persistenzbasis, dann 1·R04, dann ein simulierter Lauf mit erfundenen Daten. Vorgabe: nur erfundene Daten, keine echten Schnittstellen, kein echtes Geld. Dachs und Maus beobachten ein erfundenes Bild aus Nachrichten, Terminen und Kursen, schreiben ihre Einschätzung und Begründung ins Journal und treffen Papierentscheidungen im Schutzrahmen. Kaufentscheidungen werden nicht als Regeln festgeschrieben. Schlank halten.

Ein grüner Test schließt nichts. „Technisch geschlossen“ setzt erst die Prüfung durch Maus.

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

## Journal: Begründung und Tagesabschluss

Zwei neue Arten, gelesen und geschrieben wie alle anderen (kanonische Form, R20):

- `begruendung;datum;wer;text`: `wer` nicht leer, ohne `;` und ohne Rand-Leerzeichen; Text nicht leer. Steuerzeichen ersetzt der Lauf vorher durch Leerzeichen, so kann keine Antwort eine Zeile brechen.
- `abschluss;datum;hoechststand`: Der Tag ist vollständig belegt. Daten streng steigend, Höchststand positiv.

`Stand::seit_abschluss` zählt die Einträge nach dem letzten Abschluss, auch KI-Buchungen.

## Wiederanlauf (Kap. 11)

Der Lauf setzt am Tag nach dem letzten Abschluss fort. Der Höchststand kommt aus dem Abschluss, der Vergleichskurs aus dem Szenario. Ein Lauf in zwei Teilen ergibt byte-gleich dasselbe Journal wie ein ununterbrochener Lauf.

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
