# Schnitt Regelwerk – Zwang vor Wahl, Prüfung am Limit

Stufe 2, Schnitt Regelwerk. Er baut das Regelwerk entlang R20 bis R24 und der Matrix R36 (Kapitel 8) um und schließt damit die offenen Regelwerkstests zu 1·F02, 1·F04, 1·F05, 1·F06, 1·F08, 1·F09, 2·A04 und 1·T01. Dazu kommt der Regelwerksteil von 1·R06: das Höchstalter der Kurse.

Klasse `FestschreibungAenderung`. Die Grenzen bekommen zwei neue Felder, die Toleranz der Bestandsgrenze (2 Punkte) und das Höchstalter eines Kurses (20 Minuten, Kapitel 18). Damit ändert sich der Fingerabdruck `grenzen` in `kodex/festschreibung.txt`, und das Gate verlangt eine Grenzfreigabe (R30). Die Vorlage dafür entsteht erst nach Maus' Abnahme. Beide Zahlen sind unbestätigte Konfiguration.

Ein grüner Test schließt keinen Befund. „Technisch geschlossen“ setzt erst die unabhängige Prüfung durch Maus.

**Fassung 2 nach Maus' Prüfung von `231364c` (K9-01 bis K9-03):** Maus' Gegenprogramm steht als drei Tests in `crates/depot_regelwerk/tests/befunde.rs`, am Stand `231364c` rot (eigener Commit `7233417`). Behoben ist allgemein:

- **K9-01 (P1):** Welche Werte zwingend verkauft werden, entscheidet allein eine Regel: bei zu vielen Positionen zuerst der kleinste Wert, bei gleichem Wert nach ISIN. KI-Ziele gehen in keine Zwangsauswahl ein, weder gültige noch verworfene, mit oder ohne Halt. Der Vorschlag wird erst geprüft, nachdem der Zwang feststeht, und wirkt nur auf freiwillige Aufträge.
- **K9-02 (P2):** Alle Aufträge eines Plans mindern denselben Topfwert. Jeder Abbau wird gegen die Minderung des ganzen Plans gerechnet: andere Abbauorders, Ausstiege aus der Positivliste und freiwillige Aufträge. Das Regelwerk plant so lange neu, bis jeder Abbau gegen den ganzen Plan hält. Abbaumengen wachsen dabei nur, der freiwillige Teil wird jedes Mal ganz neu geplant und hält so alle seine Grenzen (Umschlag, Orderzahl, Deckung, Kaufgrenze) gegen den endgültigen Zwang. Weil die Abbaumengen nur wachsen und durch den Bestand begrenzt sind, endet die Rechnung.
- **K9-03 (P3):** Kennungen werden erst vergeben, wenn alle Aufträge samt Limit stehen, und dann alle auf einmal. Reichen die Nummern nicht, entsteht kein Auftrag, und die Vergabe bleibt unverändert. Das gilt für Entscheidung und Notbremse.

## So prüfst du es nach

```text
cargo test --locked -p depot_regelwerk
```

Am ersten Commit des Schnitts (`6dc1fe9`, nur Tests) zeigt `cargo test --locked -p depot_regelwerk --test befunde -- --ignored` alle 15 markierten Regelwerkstests rot: die 13 aus Schritt 1 und zwei neue. Danach steht kein Regelwerkstest mehr auf `ignore`.

## Ergebnis

| Befund | Test | am alten Stand | nach dem Umbau |
| --- | --- | --- | --- |
| 1·F02 | `befund_1_f02_vorschlag_fuer_fremden_bereich_wird_abgelehnt` | rot: Vorschlag von Maus für den Topf von Dachs genehmigt | grün: `VerwerfGrund::FremderBerater` |
| 1·R06 (neu) | `befund_1_r06_veralteter_kurs_erzeugt_keinen_auftrag`: Kurse 20 und 21 Minuten alt | rot: 21 Minuten alte Kurse ergeben Aufträge, Notbremse und Datenprüfung nehmen sie an | grün: kein Auftrag, Notbremse `None`, Datenprüfung meldet `KursZuAlt` |
| 1·F04 | `befund_1_f04_marktbewegung_ueber_toleranz_erzeugt_abbaupflicht` | rot: 28 % bleiben | grün: Abbau auf höchstens 25 % am Limit |
| 1·F04 | `befund_1_f04_zu_viele_positionen_erzeugen_abbaupflicht` | rot: 9 Werte, Vorschlag nur verworfen | grün: der überzählige Wert wird ganz verkauft |
| 1·F04 | `befund_1_f04_kauf_erzeugt_keine_neunte_position` | rot: Kauf erzeugt den neunten Wert | grün: `PositionsgrenzeErreicht` |
| 1·F05 | `befund_1_f05_umschlag_am_limit_ueber_der_grenze_wird_gekuerzt` | rot | grün: Umschlag am Limit gekürzt |
| 1·F06 | `befund_1_f06_teilverkauf_unter_mindestorder_am_limit_abgelehnt` | rot: 99 € Teilverkauf | grün |
| 1·F08 | `befund_1_f08_fehlender_vortag_ist_kein_bestandener_sprungtest` | rot | grün: `DatenFehler::KeinVergleichskurs` |
| 1·F09 | `befund_1_f09_notbremse_und_umschichtung_vergeben_verschiedene_kennungen` | rot: doppelte Kennungen | grün: eine gemeinsame Vergabe, fortlaufend ohne Lücke |
| 2·A04 | `befund_2_a04_halt_sperrt_den_zwingenden_ausstieg_nicht` | rot | grün |
| 2·A04 | `befund_2_a04_umschlaggrenze_kuerzt_den_zwingenden_ausstieg_nicht` | rot: 50 von 80 Stück | grün: alle 80 |
| 2·A04 | `befund_2_a04_orderzahl_sperrt_zwingende_ausstiege_nicht` | rot: 2 von 6 fehlen | grün |
| 2·A04 | `befund_2_a04_kombination_sperrt_zwingende_ausstiege_nicht` | rot | grün |
| 1·T01 | `grenzfall_hoechstanteil_gilt_am_limit` | rot | grün |
| K9-01 | `befund_k9_01_vorschlag_steuert_keinen_zwangsverkauf`: neun Werte, ohne Vorschlag, fremder Berater, negatives Gewicht, Summe über 100 %, gültig mit und ohne Halt | rot an `231364c`: der Vorschlag ändert den verkauften Wert | grün: immer derselbe Zwangsverkauf |
| K9-02 | `befund_k9_02_gemeinsamer_abbau_haelt_den_hoechstanteil`: zwei Abbauorders unter Halt (Maus' Fall), dazu ein Ausstieg aus der Positivliste, dazu freiwillige Käufe | rot an `231364c`: je 2.498 € von 9.990,96 € = 25,0026 % | grün: jeder Rest höchstens 25 %, jeder Kauf am Limit höchstens 25 % |
| K9-03 | `befund_k9_03_abbruch_verbraucht_keine_kennung`: Startwert `u64::MAX − 1`, zwei Aufträge, in Notbremse und Entscheidung | rot an `231364c`: Zähler auf `u64::MAX`, kein Auftrag | grün: kein Auftrag, Zähler unverändert |
| 1·T01 (neu) | `grenzfall_deckung_kauft_was_passt`: 506,50 €, 600 €, 1.011,50 € Bargeld | rot: eine Stückzahl zu wenig | grün: 5, 5 und 10 Stück |

## Was der Umbau tut

- **Reihenfolge (R36).** Das Regelwerk plant zuerst die zwingenden Ausstiege: Werte außerhalb der Positivliste, Abbau über der Bestandsgrenze. Halt, Orderzahl, Umschlag und Mindestorder sperren sie nicht. Sie brauchen nur einen frischen, plausiblen Kurs; fehlt der, gibt es für den Wert keinen Auftrag, nur Halt. Erst danach, ohne Halt und mit gültigem Vorschlag, kommen die freiwilligen Aufträge. Für sie gelten Orderzahl und Umschlag mit dem, was die zwingenden übrig lassen.
- **Drei Stufen der Anteilsgrenzen (R22).** Zielgrenze: zu große Gewichte werden gekürzt. Kaufgrenze: ein Kauf hält den Wert nach dem Kauf, voll zum Limit ausgeführt und mit höchster Gebühr, beim Höchstanteil; ein neuer Wert entsteht nur, solange die Positionszahl reicht. Bestandsgrenze: liegt ein Wert mehr als die Toleranz über dem Höchstanteil, ist Zukauf gesperrt, und er wird so weit verkauft, dass er danach höchstens beim Höchstanteil liegt. Gerechnet wird gegen den Topfwert nach allen Aufträgen des Plans (K9-02). Bleiben mehr Werte als erlaubt, werden die überzähligen ganz verkauft: zuerst die mit dem kleinsten Wert, bei gleichem Wert nach ISIN. KI-Ziele gehen in diese Auswahl nicht ein (K9-01).
- **Prüfbasis am Limit (R21).** Mindestorder, Kaufgrenze und Deckung rechnen am Limit mit der höchsten Gebühr. Die Deckung sucht die größte Stückzahl, die samt Gebühr ins Budget passt, statt sie zu schätzen. Der Umschlag rechnet je Auftrag mit dem höheren Preis aus Kurs und Limit: Ein Kauf führt höchstens zum Limit aus, ein Verkauf mindestens zum Limit, oft zum Kurs oder darüber.
- **Kennungen (R24).** Eine `AuftragsKennung` entsteht nur in der `Kennungsvergabe`. Alle Planer eines Laufs, Entscheidung und Notbremse, ziehen aus derselben Vergabe. Jede Nummer ist um eins größer als die vorige. Den Startwert liefert später das Journal; bis zur Persistenzbasis reicht ihn der Aufrufer herein. Vergeben wird erst, wenn alle Aufträge samt Limit stehen, und dann alle Nummern auf einmal oder keine (K9-03).
- **Vorschlag und Berater (R20).** Ein Vorschlag, der nicht vom Berater des Topfes stammt, wird ganz verworfen.
- **Kursdaten (R20, R06).** Die Datenprüfung meldet einen fehlenden Vortagskurs als eigenen Fehler `KeinVergleichskurs` und einen zu alten Kurs als `KursZuAlt`. Regelwerk und Notbremse nehmen nur Kurse, die nicht älter als das Höchstalter sind.

## Testanpassungen

Keine Aussage eines Tests ist schwächer geworden, mit einer Ausnahme, die R36 verlangt (Zufallstest, unten).

- **Schnittstelle.** `entscheide` und `notbremse_auftraege` nehmen die gemeinsame `Kennungsvergabe` statt einer Zyklusnummer. Die Tests übergeben sie. Der F09-Test teilt jetzt eine Vergabe zwischen Entscheidung und Notbremse und verlangt zusätzlich fortlaufende Nummern ab dem Startwert.
- **`befund_1_f05_…` geschärft.** Der alte Fall (zwei Käufe zu je 25 %) wurde nach dem Umbau schon von der Kaufgrenze am Limit begrenzt. Er blieb deshalb auch grün, wenn man den Umschlag wieder am Schlusskurs rechnete. Jetzt: vier Käufe zu je 12,5 % in 16.000 €, die die Umschlaggrenze am Schlusskurs genau ausfüllen. Begrenzen kann allein der Umschlag. Am alten Stand ist der neue Fall rot.
- **`grenzfall_hoechstanteil_gilt_am_limit` geschärft.** Der alte Fall (20 Stück gehalten, Ziel 25 %) kaufte nach dem Umbau gar nicht mehr, weil der mögliche Kauf unter der Mindestorder lag; er war damit leer erfüllt. Jetzt zwei Lagen, die tatsächlich kaufen: 10 Stück gehalten in 10.000 €, und ein Bargeldtopf von 1 Mio. €, in dem der Abstand von Kurs und Limit mehrere Stück ausmacht. Der Test verlangt, dass gekauft wird. Am alten Stand ist er rot.
- **`grenzfall_deckung_mit_gebuehr_am_limit`.** Der alte Aufbau (drei Werte zu je 5 oder 10 Stück) läge mit einem Kauf von 5 Stück über 25 % am Limit; die Kaufgrenze hätte die Deckung verdeckt. Er nutzt jetzt den Aufbau des neuen Deckungstests: vier Werte zu je 25 Stück, Ziel je 20 %. Die Aussagen sind gleich (kein Kauf unter 506,50 €, 10 Stück genau ab 1.011,50 €).
- **`zu_grosses_gewicht_wird_auf_25_prozent_gekuerzt`.** Erwartet jetzt 123 statt 125 Stück: 25 % von 5.000 € am Limit von 10,10 € mit Gebühr. Strenger als vorher.
- **`zufallstest_grenzen_am_schlusskurs`.** Zwingende Aufträge sind von Orderzahl, Umschlag und Mindestorder ausgenommen (R36). Orderzahl und Umschlag prüft der Test deshalb nur, wenn es daneben freiwillige Aufträge gibt; dann für alle zusammen. Der Test hat im Umbau einen Fehler gefunden: Verkäufe wurden für den Umschlag am Limit gerechnet, also zu niedrig. Behoben durch die Regel „höherer Preis aus Kurs und Limit“ oben.

## Gegenproben

Jede Zeile schwächt den Umbau an genau einer Stelle. Jedes Mal wird der zuständige Test rot.

| Gegenprobe | Ergebnis |
| --- | --- |
| Berater nicht geprüft | F02 rot |
| Höchstalter im Regelwerk nicht geprüft | R06 rot |
| Höchstalter in der Notbremse nicht geprüft | R06 rot |
| Höchstalter in der Datenprüfung nicht geprüft | R06 rot |
| kein Abbau über der Toleranz | F04 Marktbewegung rot |
| kein Abbau überzähliger Positionen | F04 zu viele Positionen rot |
| Positionsgrenze beim Kauf fehlt | F04 neunte Position rot |
| Umschlag am Schlusskurs | F05 rot |
| Umschlag eines Verkaufs am Limit | Zufallstest rot |
| Mindestorder am Schlusskurs | F06 rot |
| fehlender Vortagskurs nicht gemeldet | F08 rot |
| jede Entscheidung mit eigener Vergabe | F09 rot |
| Halt sperrt zwingende Ausstiege | 2 A04-Tests rot |
| Orderzahl kappt zwingende Ausstiege | A04 Orderzahl rot |
| Umschlag kürzt zwingende Ausstiege | A04 Umschlag rot |
| Kaufgrenze am Schlusskurs | T01 Höchstanteil rot |
| Deckung rechnet eine Gebühr zu viel ab | beide Deckungstests rot |
| Zielgewicht des Vorschlags geht in die Auswahl der überzähligen Positionen ein | K9-01 rot |
| Abbau nicht gegen den ganzen Plan nachgezogen | K9-02 rot |
| Abbau nur gegen die anderen Zwangsaufträge nachgezogen, ohne freiwillige | K9-02 rot |
| Notbremse vergibt Kennungen einzeln | K9-03 rot |
| Entscheidung vergibt Kennungen einzeln | K9-03 rot |

## Grenzen, die ausdrücklich bleiben

- **Bereiche.** Grenzen gelten weiter je Topf. Die Umstellung auf Bereiche ist ein eigener Schnitt; der Teilnachweis von 1·F04 „je Bereich“ bleibt offen.
- **Budget, Ausstiegsreserve, Gebührenquote.** Gibt es im Regelwerk noch nicht; Teilnachweis von 2·A04 entsteht mit dem Kostenbuch (Stufe 3, 2·A05).
- **Kennungen aus dem Journal.** Die Vergabe ist zentral, ihren Startwert liest erst die Persistenzbasis aus dem Journal.
- **Weitere Auftragsarten.** Kursprüfung in Schutzstopp und Nachziehen entsteht mit Stufe 3 und 6 (1·F03).
- **Zahlen.** Toleranz 2 Punkte und Höchstalter 20 Minuten sind unbestätigte Konfiguration aus Kapitel 18.
