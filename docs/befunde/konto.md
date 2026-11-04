# Kontowächter und Auftragszustände

Stufe 3, erster Schritt, nur mit Papiergeld und einem erfundenen Papierkonto. Wolfgang hat am 04.11.2026 den Schritt freigegeben. Maus' Vorgaben für die Abnahme: die Gegenprobe „Absturz direkt nach dem Senden“ (zuerst abgleichen, nichts erneut senden, nichts doppelt buchen, Halt solange unklar) und drei belegte Fälle: normaler Ablauf, Teilausführung mit Neustart, Halt bei widersprüchlichen Beständen.

Ein grüner Test schließt nichts. „Technisch geschlossen“ setzt erst die Prüfung durch Maus.

## Was gebaut ist

**Auftragszustände (R37, Kap. 10)** in `crates/depot_journal/src/auftrag.rs`. Jeder Schritt ist ein eigener Eintrag `zustand;nr;meldung` im Journal; nach einem Neustart folgt der Zustand allein daraus.

| Zustand | weiter nach |
| --- | --- |
| reserviert (der Eintrag `auftrag`) | gesendet, verworfen |
| gesendet: Versand im Journal begonnen | angenommen, abgelehnt, unbekannt |
| unbekannt: keine Antwort | angenommen, abgelehnt; nur über den Abgleich |
| angenommen, teilgefüllt | Ausführung, storniert, verfallen |
| gefüllt, abgelehnt, storniert, verfallen, verworfen | Ende |

Allgemeine Regeln, beim Anhängen und beim Öffnen gleich (R20):

- `gesendet` gibt es nur nach `reserviert`. Ein zweiter Versand desselben Auftrags lässt sich nicht belegen, auch nicht nach „unbekannt“.
- Eine Ausführung (`ausfuehrung;stueck;kurs;gebuehr`) hält das Limit ein, hat mindestens ein Stück und geht nie über die beauftragte Menge. Sie bucht Bargeld und Bestand im Topf. Gefüllt ist ein Auftrag genau dann, wenn seine Ausführungen die Menge ergeben.
- **Deckung je Topf:** Nach jedem Eintrag trägt das Bargeld eines Topfes alle offenen Käufe am Limit, und der Bestand jedes Werts alle offenen Verkäufe. Das gilt für Aufträge, Ausführungen und das Setzen eines Topfes. Stücke eines anderen Topfes zählen nie, auch wenn das Konto sie führt.
- **Tagesabschluss** nur ohne offenen Auftrag: Tagesorders enden am Tag.
- Aufträge entstehen nur über `Journal::reserviere_auftrag` (mit `GenehmigterAuftrag`), Zustände nur über `melde` und `gleiche_ab`. `anhaengen` lehnt beide ab.

**Kontowächter (Kap. 9)** in `crates/depot_journal/src/konto.rs`:

- **Deckung vor der Reservierung:** Ein Kauf wird nur reserviert, wenn Limit mal Menge plus höchstmögliche Gebühr nicht über dem verfügbaren Bargeld liegt. Verfügbar ist das Bargeld des Topfes minus aller offenen Käufe samt Gebühr (`verfuegbar`). Der Auftrag muss unter den heutigen Grenzen genehmigt sein. Die Reservierung ist der Eintrag im Journal; so reserviert eine einzige Stelle, nacheinander.
- **Abgleich (`gleiche_ab`):** Zu jedem offenen Auftrag holt der Wächter den Stand beim Konto über die Kennung und belegt jede neue Meldung: Annahme, Ablehnung, neue Ausführungen (nur die, die noch nicht im Journal stehen), Storno, Verfall. Danach vergleicht er Bargeld, Bestand je Wert und offene Orders des Kontos mit der Summe der Töpfe. Jede Unstimmigkeit ist eine `Abweichung`:
  - Saldo unter null, Saldo in fremder Währung, anderes Bargeld (etwa eine Belastung, die kein Auftrag erklärt), anderer Bestand, offene Order, die das Journal nicht beim Broker führt;
  - gesendeter Auftrag, den das Konto nicht kennt (`OrderFehlt`): Er bleibt offen;
  - Auftrag, den das Konto kennt, dessen Versand aber nie belegt wurde (`OrderOhneVersand`);
  - Meldung, die das Journal nicht annimmt, oder schon gebuchte Ausführungen, die das Konto anders meldet (`Unbuchbar`).
- Ein reservierter, nie als gesendet belegter Auftrag, den das Konto nicht kennt, wird verworfen: Der Versand wird belegt, bevor gesendet wird.
- Einen Fehlbetrag gleicht kein Automatismus aus, weder durch Verkauf noch durch Nachschuss.

**Papierkonto und Auftragsstelle** in `crates/depot_sim`:

- `papier.rs`: `Papierkonto` führt nur das Gesamtkonto, unabhängig vom Journal. Es lehnt eine schon bekannte Kennung ab, prüft die Deckung im Gesamtkonto, führt Tagesorders zum Schlusskurs im Limit mit Gebühr aus und lässt den Rest verfallen. Der Trait `Broker` erweitert `Konto` um `sende` und `markt`.
- `lauf.rs`: Jeder Auftrag wird mit Deckung reserviert, als gesendet belegt und dann gesendet. Bleibt die Antwort aus, gilt „Sendestatus unbekannt“; gesendet wird nicht noch einmal. Nach dem Markt gleicht der Kontowächter ab. Käufe eines Topfes gehen erst raus, wenn alle Verkäufe ganz ausgeführt sind, weil das Regelwerk mit deren Erlös rechnet; sonst entfallen sie. Der Kontowächter prüft auch zu Beginn jedes Tages. Jede Abweichung steht als Begründung von `Kontowaechter` im Journal und hält den Lauf an; der Tag bleibt offen.
- **Wiederanlauf:** Zuerst gleicht der Kontowächter ab und belegt, was aus offenen Aufträgen geworden ist; gesendet und gefragt wird dabei nichts. Bei einer Abweichung ist Halt, ohne weiteren Eintrag. Danach gilt die Regel aus dem simulierten Lauf: Ist der Tag angefangen, ist Halt.

Keine neuen Grenzen, keine neuen Abhängigkeiten, keine Kaufregeln. Die Festschreibung bleibt unverändert.

## Tests

`crates/depot_journal/tests/konto.rs` mit einem Konto als Skript, das genau meldet, was der Test vorgibt:

- `zustandsmaschine_laesst_nur_erlaubte_schritte_zu`: zweiter Versand, Annahme vor dem Versand, Ausführung vor der Annahme, unter dem Limit, über der Menge oder mit null Stück, Schritte nach dem Endzustand und Abschluss mit offenem Auftrag werden abgelehnt, ohne zu schreiben; `anhaengen` lehnt Auftrag und Zustand ab.
- `zweiter_versand_in_der_datei_ist_halt`: Steht ein zweites `gesendet` in der Datei, ist das beim Öffnen Halt.
- `befund_2_a09_reservierung_bindet_stueck_je_topf`: Stücke in einem offenen Verkauf sind gebunden; derselbe Wert in zwei Töpfen; ein Topf ohne Stücke verkauft nichts, auch wenn das Konto sie führt.
- `kauf_wird_nur_mit_gebuehr_reserviert`: Bargeld genau für Limit mal Menge reicht nicht, mit der Gebühr dazu schon; danach ist nichts mehr verfügbar.
- **Absturz direkt nach dem Senden** (`absturz_direkt_nach_dem_senden_wird_nur_abgeglichen`): reserviert, als gesendet belegt, Absturz. Der Neustart belegt, was das Konto meldet: offen, teilweise ausgeführt, ganz ausgeführt, abgelehnt. Ein zweiter Neustart mit demselben Abgleich schreibt nichts. Kennt das Konto den Auftrag nicht, bleibt er gesendet, nichts wird geschrieben, und es ist eine Abweichung.
- `absturz_vor_dem_senden_verwirft_nur_was_nie_gesendet_wurde`: nie gesendet und dem Konto unbekannt wird verworfen; dem Konto bekannt ist eine Abweichung.
- **Teilausführung mit Neustart** (`befund_2_a09_teilausfuehrung_mit_neustart`): 4 von 10 Stück verkauft; nach dem Neustart teilgefüllt, 4 gebucht, 6 gebunden, Tag nicht abschließbar. Meldet das Konto eine gebuchte Ausführung anders, ist das eine Abweichung, und nichts wird gebucht. Kommt der Rest, wird nur er gebucht.
- **Widersprüchliche Bestände** (`befund_6_n01_widerspruechliche_bestaende_sind_abweichungen`): anderer Bestand, Saldo −0,01 €, Saldo in US-Dollar, unbekannte Belastung, fremde offene Order, Wert ohne Topf. Jeder Fall ist eine Abweichung; der Abgleich schreibt nichts.

`crates/depot_sim/tests/konto.rs` mit dem Papierkonto und einem gestörten Konto:

- **Normaler Ablauf** (`normaler_ablauf_stimmt_mit_dem_konto_ueberein`): Jeder Auftrag ist genau einmal gesendet und hat einen Endzustand laut Konto, jede Ausführung ist einmal gebucht, Journal und Konto stimmen überein, der Kontowächter meldet nichts.
- `befund_2_a09_unbekannter_sendestatus_sendet_nicht_noch_einmal`: Jede Antwort geht verloren, die Aufträge kommen an. Der Abgleich klärt sie, eine Sendung je Auftrag, der Lauf geht weiter.
- `verlorener_auftrag_haelt_ohne_zweiten_versand`: Auftrag und Antwort gehen verloren. Halt, der Tag bleibt offen; der Neustart hält wieder, ohne zu senden, zu fragen oder zu schreiben.
- **Absturz direkt nach dem Senden im Lauf** (`absturz_direkt_nach_dem_senden_bucht_einmal_und_sendet_nicht`): Ein Verkauf ist reserviert, gesendet und beim Konto angekommen, dann Absturz; der Markt führt ihn aus. Der Neustart bucht die Ausführung genau einmal, sendet und fragt nichts und hält, weil der Tag angefangen ist. Ein weiterer Neustart ändert nichts.
- **Halt bei widersprüchlichen Beständen im Lauf** (`befund_6_n01_fremde_belastung_haelt_den_lauf`, `befund_6_n01_abweichung_zwischen_zwei_tagen_haelt_beim_neustart`): eine fremde Belastung von 5 € nach dem Markt, eine von 0,01 € zwischen zwei Tagen. Halt mit Grund im Journal, der Tag bleibt offen; danach wird nichts mehr gesendet oder gefragt, und der Neustart schreibt nichts.

Die bisherigen Tests laufen unverändert gegen den neuen Ablauf. Angepasst sind nur ihre Aufrufe: `lauf` bekommt das Papierkonto, das über einen Neustart weiterlebt; Aufträge im Journal entstehen in den Journaltests über `reserviere_auftrag` statt über `anhaengen`, und die Dateifälle zu 1·F09 haben einen Topf, damit sie allein an der Kennung scheitern.

## Gegenproben

Die Tests sind zusammen mit dem Umbau entstanden, weil es die Schnittstelle vorher nicht gab. Dass jeder Test den Fehler zeigt, belegen Gegenproben, jeweils neu übersetzt („Compiling“ in der Ausgabe):

| Gegenprobe | roter Test |
| --- | --- |
| zweiter Versand nach „gesendet“ oder „unbekannt“ erlaubt | `zustandsmaschine_laesst_nur_erlaubte_schritte_zu` |
| Abgleich bucht alle Ausführungen erneut | `absturz_direkt_nach_dem_senden_wird_nur_abgeglichen`, `befund_2_a09_teilausfuehrung_mit_neustart` |
| nie gesendeter Auftrag nicht verworfen | `absturz_vor_dem_senden_verwirft_nur_was_nie_gesendet_wurde` |
| Bargeld nicht verglichen | `befund_6_n01_widerspruechliche_bestaende_sind_abweichungen` |
| Bestand nicht verglichen | `befund_6_n01_widerspruechliche_bestaende_sind_abweichungen` |
| Abschluss trotz offenem Auftrag | `zustandsmaschine_…`, `befund_2_a09_teilausfuehrung_mit_neustart` |
| Verkäufe binden keine Stücke | `befund_2_a09_reservierung_bindet_stueck_je_topf`, `befund_2_a09_teilausfuehrung_mit_neustart` |
| Limit der Ausführung nicht geprüft | `zustandsmaschine_laesst_nur_erlaubte_schritte_zu` |
| Kauf ohne Gebühr reserviert, Kaufdeckung ganz aus | `kauf_wird_nur_mit_gebuehr_reserviert` |
| Neuversand bei unbekanntem Sendestatus | `befund_2_a09_unbekannter_sendestatus_sendet_nicht_noch_einmal` |
| kein Abgleich beim Wiederanlauf | `absturz_direkt_nach_dem_senden_bucht_einmal_und_sendet_nicht`, `befund_6_n01_abweichung_zwischen_zwei_tagen_haelt_beim_neustart` |
| kein Abgleich nach dem Markt | sechs Tests in `crates/depot_sim/tests/konto.rs` |

## Was offen bleibt

- Ausstiegsreserve (3·P3), Steuerrücklage und Gebührenpuffer im verfügbaren Geld: Die Beträge dafür kommen mit dem Freigabepaket.
- Schutzstopps und „Schutz ungewiss“ (R35, 2·A02, 3·P2) gehören zum Kurzfrist-Bereich und kommen mit dem Papierkonto beim Broker (Stufe 6).
- Prüfung von Cash-Konto und Handelsrechten beim Start, Nachricht an Wolfgang bei Halt und die Freigabe eines angefangenen Tages kommen mit dem echten Papierbetrieb.
- Der Kontowächter liegt als Modul `konto` in `depot_journal`, nicht als eigenes Crate `depot_konto`: Die Reservierung ist der Eintrag im Journal, wie bei der `AufrufErlaubnis`. So kann niemand am Wächter vorbei einen Auftrag belegen. Ein eigenes Crate kommt, wenn der Wächter beim echten Broker liest.
