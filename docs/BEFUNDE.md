# DepotSchlopf – Befunde und ihr Stand

Diese Liste führt jeden Befund aus den Prüfungen durch Maus vom Befund bis zur Abnahme. Sie ist das Gegenstück zu `docs/ARCHITEKTUR.md`.

**Zwei getrennte Zustände:**

- **Architektur:** „berücksichtigt“ heißt, dass eine allgemeine Regel oder eine Zustandsbeschreibung in `docs/ARCHITEKTUR.md` den Befund abdeckt. Das sagt nichts über den Code.
- **Technisch:** „offen“ bis der Negativtest im Repository steht, im Kodex läuft, den Fehler am alten Stand zeigt und am neuen Stand besteht. „geschlossen“ erst nach erneuter unabhängiger Prüfung. Erst wenn jede Zeile technisch geschlossen ist, ist die technische Abnahme möglich (Architektur, Kapitel 19, Stufe 7).

Die Spalte „Abnahme“ nennt, wer den Befund schließt und wann: in welcher Stufe der Test entsteht und dass eine unabhängige Prüfung ihn bestätigt.

**Zwischenstände von „offen“:** „Test rot“ heißt, dass der Negativtest im Repository steht und am alten Stand fehlschlägt; er ist mit `#[ignore = "offen: …"]` markiert, bis der Umbau ihn grün macht. Nachweis und Befehle stehen in `docs/befunde/negativtests.md`. Ein grüner Test allein schließt nichts: „geschlossen“ setzt erst die unabhängige Prüfung.

Stand: 01.11.2026, Fassung 4.4, Stufe 2 Schritt 1 (Negativtests für Kern, Regelwerk und KI-Budget). Kein Befund ist technisch geschlossen.

## Prüfung 1 – Stand 383ee05 (26 Befunde)

| Befund | Kern | Regel / Änderung | Negativtest | Abnahme | Architektur | Technisch |
| --- | --- | --- | --- | --- | --- | --- |
| 1·G01 | harte Lints über `cfg_attr` aussetzbar | R26, `forbid` im Crate-Kopf, Kap. 16 | `cfg_attr(…, allow(…))` in jedem Crate wird vom Compiler abgelehnt | Stufe 2, unabhängige Prüfung | berücksichtigt | offen |
| 1·G02 | Reinheitsprüfung durch gruppierten Import umgehbar | R26, Crate-Grenzen, `std`-Prüfung, Kap. 16 | `use std::{fs, …}` und Umbenennung in reinem Crate rot | Stufe 2 | berücksichtigt | offen |
| 1·G03 | Lint-Vererbung durch TOML-Text vorgetäuscht | R26, semantische Manifestprüfung, Kap. 16 | Manifest ohne wirksame Vererbung rot, Compiler-Verstoß je Target abgelehnt | Stufe 2 | berücksichtigt | offen |
| 1·G04 | Abhängigkeiten nur nach Alias festgeschrieben | R26, Name, Version, Quelle, Features, Lock-Hash | umbenannte oder andere Quelle derselben Abhängigkeit rot | Stufe 2 | berücksichtigt | offen |
| 1·G05 | Essenz D01–D17 kann verschwinden | R26, festes Register | gelöschte oder umbenannte Regel rot | Stufe 2 | berücksichtigt | offen |
| 1·G06 | Unicode-Dateinamen falsch, fehlen im Export | R24, NUL-getrennte Pfadlisten | Datei mit Umlaut und Leerzeichen wird geprüft und exportiert | Stufe 2 | berücksichtigt | offen |
| 1·F01 | freie Laufzeitgrenzen umgehen die Festschreibung | R19, `FreigegebeneKonfiguration` | gelockerte Grenze außerhalb von Tests lässt sich nicht bauen | Stufe 2 | berücksichtigt | offen, Test rot |
| 1·F02 | Vorschlag des falschen Beraters angenommen | R20, Vorschlag trägt Bereich und Berater | Vorschlag für fremden Bereich wird abgelehnt | Stufe 2 | berücksichtigt | offen, Test rot |
| 1·F03 | Notaufträge mit Nullkurs möglich | R20, geprüfte Kurstypen; R36 Kursprüfung auch bei Zwang | Nullkurs erzeugt keinen Auftrag, sondern Halt | Stufe 2 | berücksichtigt | offen, Test rot |
| 1·F04 | Anteils- und Anzahlgrenze gilt nicht für den Bestand | R22, Ziel-, Kauf- und Bestandsgrenze | Marktbewegung über Toleranz sperrt Zukauf und erzeugt Abbaupflicht | Stufe 2 | berücksichtigt | offen, Test rot |
| 1·F05 | Umschlaggrenze mit Schlusskurs statt Volumen | R21, Prüfbasis am Limit | Umschlag am Limit über der Grenze wird gekürzt | Stufe 2 | berücksichtigt | offen, Test rot |
| 1·F06 | Teilverkauf unter Mindestorder am Limit | R21, R36 Matrix (Mindestorder, Gesamtbestand) | Teilverkauf unter Mindestorder am Limit abgelehnt, Gesamtverkauf erlaubt | Stufe 2 | berücksichtigt | offen, Test rot |
| 1·F07 | „verkauft“ heißt nicht vollständig verkauft | R23, R37 Zustände | Journal meldet nur den Endzustand laut Broker | Stufe 3 | berücksichtigt | offen |
| 1·F08 | fehlender Vortagskurs still akzeptiert | R20, eigener Zustand „kein Vergleichskurs“ | fehlender Vortag ist kein bestandener Sprungtest | Stufe 2 | berücksichtigt | offen, Test rot |
| 1·F09 | Auftragskennungen kollidieren | R24, zentrale Vergabe aus dem Journal | Notbremse und Umschichtung im selben Lauf vergeben verschiedene Kennungen | Stufe 2 | berücksichtigt | offen, Test rot |
| 1·K01 | KI-Grenzen als freie Argumente | R19, gebundene Budgetkonfiguration | gelockerte KI-Grenze lässt sich nicht bauen | Stufe 2 | berücksichtigt | offen, Test rot |
| 1·K02 | negative Preise, negative Reservierung | R20 | negativer Preis wird beim Laden abgelehnt | Stufe 2 | berücksichtigt | offen, Test rot |
| 1·K03 | ungültige Kalenderdaten | R20 | 31.02. wird abgelehnt | Stufe 2 | berücksichtigt | offen, Test rot |
| 1·K04 | leeres oder fehlendes Kassenbuch setzt Grenzen zurück | R25, R38 Commitprotokoll und Anker | gelöschtes, geleertes oder gekürztes Kassenbuch führt zu Halt | Stufe 2 | berücksichtigt | offen, Test rot |
| 1·K05 | semantisch kaputte Historie akzeptiert | R20, R25, Prüfung der ganzen Historie | Zyklus- oder Summenfehler führt zu Halt | Stufe 2 | berücksichtigt | offen, Test rot |
| 1·K06 | zwei Pfade umgehen die Sperre | R24, Sperre über Dateiidentität, Link-Verbot | zweiter Pfad auf dieselbe Datei wird gesperrt | Stufe 2 | berücksichtigt | offen, Test rot |
| 1·E01 | Konto- und Zugangsdaten im Export | R28 Positivliste und Inhaltsprüfung | Datei mit Geheimnismuster bricht den Export ab | Stufe 2 | berücksichtigt | offen |
| 1·E02 | Link exportiert fremde Inhalte | R24, R28 | Link im Projekt wird nicht gelesen | Stufe 2 | berücksichtigt | offen |
| 1·E03 | Festschreibungslauf meldet nie ausgeführte Prüfung | R27 | schreibender Befehl meldet nie „Gate offen“ | Stufe 2 | berücksichtigt | offen |
| 1·A01 | Geldoperationen sättigen still | R13, geprüfte Rechnung, Überlauf führt zu Halt | Überlauf ergibt Fehler, keinen gekappten Wert | Stufe 2 | berücksichtigt | offen, Test rot |
| 1·T01 | Zufallstest beweist die Grenzen nicht | R17, R27 Testnamen versprechen nur, was sie prüfen | gezielte Grenzfälle je harter Grenze | Stufe 2 | berücksichtigt | offen, Test rot |

**Gesonderte Risiken aus Prüfung 1:**

| Befund | Kern | Regel / Änderung | Negativtest | Abnahme | Architektur | Technisch |
| --- | --- | --- | --- | --- | --- | --- |
| 1·R01 | Verzeichniseintrag nicht synchronisiert | R29, Kap. 9 | Neuanlage synchronisiert Datei und Verzeichnis | Stufe 2 | berücksichtigt | offen |
| 1·R02 | Weiterschreiben nach Schreibfehler | R29 Fehler sperren | eingespeister Schreib- und Sync-Fehler sperrt das Objekt | Stufe 2 | berücksichtigt | offen, Test rot |
| 1·R03 | Lesefehler beim Scan übersprungen | R29 | unlesbares Verzeichnis macht den Kodex rot | Stufe 2 | berücksichtigt | offen |
| 1·R04 | Export kein konsistenter Snapshot | R28, byte-gleich mit geprüftem Stand | Änderung zwischen Prüfung und Export wird erkannt | Stufe 2 | berücksichtigt | offen |
| 1·R05 | offenes Gate trotz Laufzeitsperre | R27, Build-Freigabe und Betriebsbereitschaft getrennt | Sperre erscheint als nicht betriebsbereit | Stufe 2 | berücksichtigt | offen |
| 1·R06 | Kurse ohne Datum, rohe Zustände | R20 | Kurs ohne Handelstag, Währung oder Alter wird abgelehnt | Stufe 2 | berücksichtigt | offen, Test rot |
| 1·R07 | Genehmigung ist keine Ausführungssicherheit | R33, R34, R35, R37 | Teilausführung, Kurslücke und unbekannter Sendestatus am Papierkonto | Stufe 6 | berücksichtigt | offen |
| 1·R08 | Eigentümerfreigabe nur prozedural | R30, Freigabetypen | Freigabe ohne passenden Hash wird abgelehnt | Stufe 2 | berücksichtigt | offen |
| 1·R09 | Preis- und Tokenannahmen keine Anbieterrechnung | R8, Kap. 12, Zählung der ganzen Anfrage | Abgleich mit der Anbieterrechnung im Papierbetrieb | Stufe 5 und 7 | berücksichtigt | offen |

## Prüfung 2 – Stand 811aeac (12 Architekturbefunde)

| Befund | Kern | Regel / Änderung | Negativtest | Abnahme | Architektur | Technisch |
| --- | --- | --- | --- | --- | --- | --- |
| 2·A01 | Verlustgrenzen als Garantie formuliert; Stopptyp und Limitpflicht im Widerspruch | R33, R34; Kap. 5, 8, 10, 14; Schutzstopp als bewusste Ausnahme, offene Wahl in Kap. 21 | Kurslücke durch den Stopp: Bericht nennt Auslöser und tatsächlichen Verlust; zwingender Ausstieg mit Rest nach der Schrittgrenze | Stufe 6 | berücksichtigt | offen |
| 2·A02 | Schutz ab der ersten Sekunde bei Teilausführung unbelegt | R35; Kap. 10 Schutzfrist | 10 Stück beauftragt, 3 gefüllt, Verbindung weg: Zustand „Schutz ungewiss“, Halt und Nachricht; nach Wiederverbindung Abgleich, dann bestätigter Stopp über 3 Stück oder Zwangsverkauf (siehe 3·P2) | Stufe 6 am Papierkonto | berücksichtigt | offen |
| 2·A03 | Stopps blockieren reguläre Ausstiege | Kap. 9 Gruppe aus Stopp und Verkauf | Stopp löst während des Verkaufs aus: höchstens der Bestand wird verkauft | Stufe 6 | berücksichtigt | offen |
| 2·A04 | zwingende Ausstiege durch freiwillige Regeln blockierbar | R36; Kap. 8 Matrix | Restbestand unter Mindestorder und über Gebührenquote wird zwingend verkauft; Halt sperrt ihn nicht | Stufe 2 | berücksichtigt | offen, Test rot |
| 2·A05 | Kostendeckel gegen Zwangsaufträge und tatsächliche Kosten | Kap. 5 Budget, Reservierung, Ausstiegsreserve, Laufzeit-Kostenbuch | ausgeschöpftes Budget blockiert keinen zwingenden Ausstieg; Überschreitung stoppt freiwillige Aufträge | Stufe 3 | berücksichtigt | offen |
| 2·A06 | zwölf Papiertrades passen nicht zum Kostenrahmen | Kap. 18: Beobachtung, Evidenz, Ergebnis getrennt; höchstens 18 Monate, sonst „nicht nachgewiesen“ | Auswertung mit 11 Trades nach 18 Monaten ergibt „nicht nachgewiesen“ | Stufe 7 | berücksichtigt | offen |
| 2·A07 | „Sicher ohne Risiko“ und 1-%-Grenze keine Garantie | Kap. 1, 5, 18: Ziel statt Garantie, Melde- und Zukaufsregel | Unterschreiten der Schwelle sperrt nur Zukäufe und meldet | Stufe 2 | berücksichtigt | offen |
| 2·A08 | gegenseitige Anker ohne atomares Protokoll | R38; Kap. 9 Commitprotokoll, Wiederherstellung, Anker außerhalb | Absturz an jedem Schritt: wiederherstellbar; Kürzung: Halt | Stufe 2 | berücksichtigt | offen |
| 2·A09 | Journal, Konto und Broker ohne Zustandsmaschine | R37; Kap. 4, 9, 10 | unbekannter Sendestatus erzeugt keine zweite Order; derselbe Wert in zwei Bereichen; Teilfüllung mit Neustart | Stufe 3 und 6 | berücksichtigt | offen |
| 2·A10 | Stufe 2 hängt von Stufe 3 ab | Kap. 19: Persistenzbasis in Stufe 2 | entfällt (Reihenfolge) | Stufe 2 Abschluss | berücksichtigt | offen |
| 2·A11 | `cargo metadata` liefert keine wirksamen Lints | R26; Kap. 16 | Verstoß je Target vom Compiler abgelehnt; `std::fs` in reinem Crate rot | Stufe 2 | berücksichtigt | offen |
| 2·A12 | Dachs-Höchstpreis abgerundet; Freigabebindung unvollständig | R21 Rundung, R30 Freigabetypen; Kap. 7, 10, 18 | 0,144 € wird als 15 Cent reserviert; Echtgeld-Freigabe mit anderem Commit ungültig; Zustimmung nach Ablauf wirkungslos | Stufe 2 und 5 | berücksichtigt | offen, Rundungstest grün |

**Weitere Grenzen aus Prüfung 2:**

| Befund | Kern | Regel / Änderung | Negativtest | Abnahme | Architektur | Technisch |
| --- | --- | --- | --- | --- | --- | --- |
| 2·W1 | Steuer-, Preis- und Brokerangaben nicht freigegeben | Kap. 13, 15: vor dem Start mit Primärquellen und tatsächlichem Konto prüfen | entfällt (Prüfung vor Stufe 8) | vor Stufe 8 | berücksichtigt | offen |
| 2·W2 | Papierbefund ist kein Renditebeweis | R16, Kap. 19 Stufe 8 | entfällt | laufend | berücksichtigt | offen |
| 2·W3 | Geheimnisse in erlaubten Dateien | R28 Inhaltsprüfung | Geheimnismuster in verfolgter Datei bricht den Export ab | Stufe 2 | berücksichtigt | offen |
| 2·W4 | Kursalter je Schutzfunktion | Kap. 18 Höchstalter | zu alter Kurs führt zu Halt | Stufe 4 | berücksichtigt | offen |

## Prüfung 3 – Stand 254e484 (4 Restpunkte)

| Befund | Kern | Regel / Änderung | Negativtest | Abnahme | Architektur | Technisch |
| --- | --- | --- | --- | --- | --- | --- |
| 3·P1 | Kauf plus Ausstiegsreserve übersteigt das Monatsbudget | Kap. 5 Formel: Budget umfasst nur Gebühren, Höchstgebühr je Order festgeschrieben, Spread gemessen; begrenzt seit Fassung 4.2 durch die Spreadgrenze (4·B01) | Einstieg mit 250 € bei 6 € Budget und 2 € Höchstgebühr wird genehmigt (4 €); ein zweiter im selben Monat wird abgelehnt | Stufe 3 | berücksichtigt | offen |
| 3·P2 | Schutzfrist ohne Brokerverbindung nicht erfüllbar | R35, Kap. 10 Zustand „Schutz ungewiss“ mit Halt, Nachricht, Neuversuch und Auflösung nach Wiederverbindung | Verbindung während Teilfüllung weg: Halt und Nachricht sofort; nach Wiederverbindung Stopp bestätigt oder Zwangsverkauf; Halt bleibt bis zur Aufhebung | Stufe 6 am Papierkonto | berücksichtigt | offen |
| 3·P3 | Order- und Ausstiegsreserve vermischt | Kap. 10 Tabelle der drei Reservierungen mit Anfang, Änderung und Ende | gefüllter Kauf: Orderreservierung endet, Ausstiegsreserve bleibt; Kauf ohne Füllung: beide enden | Stufe 3 | berücksichtigt | offen |
| 3·P4 | Absturzprotokoll ohne Teilschreibvorgänge und Absturz in der Wiederherstellung | R38, Kap. 9 Datensatzformat, Quarantäne, wiederholbare Wiederherstellung | unvollständige letzte Zeile wird erkannt und abgeschnitten; fehlerhafte Zeile davor führt zu Halt (präzisiert in 4·B02); Absturz an jedem Schritt der Wiederherstellung ergibt beim nächsten Start dasselbe Ergebnis | Stufe 2 | berücksichtigt | offen |

## Prüfung 4 – Stand 0af6db1 (3 Befunde, 2 Testergänzungen)

| Befund | Kern | Regel / Änderung | Negativtest | Abnahme | Architektur | Technisch |
| --- | --- | --- | --- | --- | --- | --- |
| 4·B01 | Der Limit-Abstand begrenzt den Spread nicht | Kap. 5 und 18: eigene Spreadgrenze (Vorschlag 0,5 % der Mitte zwischen Bid und Ask), Bid und Ask nicht älter als das Kurshöchstalter, ohne Bid oder Ask kein freiwilliger Auftrag; Kap. 8: zwingende Ausstiege prüfen sie nicht; Spread zählt in Kostenregel und Bewertung | Kauf bei Spread 0,6 % wird abgelehnt; Kauf ohne Ask oder mit veraltetem Bid wird abgelehnt; zwingender Ausstieg bei Spread 2 % läuft | Stufe 3 | berücksichtigt | offen |
| 4·B02 | Falsche Prüfsumme in der letzten Zeile beweist keinen Schreibabbruch | Kap. 9: unvollständig und beschädigt getrennt, Bestätigungsgrenze je Datei, automatisch nur nachweislich Unbestätigtes, sonst Halt und geprüftes Reparaturverfahren mit Wolfgangs Freigabe; Quarantäne synchronisiert und byte-gleich geprüft | (1) beschädigte letzte Zeile nach bestätigtem Vorgang führt zu Halt, nichts wird zurückgerollt; (2) Absturz beim unbestätigten Anhängen behandelt nur diesen Eintrag, ein wiederholter Absturz ändert nichts Bestätigtes; (3) vorhandene, aber abweichende Quarantänedatei verhindert das Abschneiden | Stufe 2 | berücksichtigt | offen |
| 4·E01 | „Gesamtdeckel“ umfasst nur KI und Gebühren | Kap. 5, 13, 18: umbenannt in „Jahresbudget für KI und Gebühren“; Spread und Fondskosten werden gemessen und berichtet, aber nicht vorab gedeckelt; Kap. 21 Punkt 5 als bewusste Kostenentscheidung für Wolfgang | Bericht weist KI, Gebühren und Spread getrennt aus; Spread wird nie gegen das Jahresbudget gebucht | Stufe 3 | berücksichtigt; Entscheidung bei Wolfgang | offen |
| 4·T1 | Meldeweg zum Handy kann ausfallen | Kap. 11: zugestellt erst nach Annahmebestätigung, sonst Protokoll, Wiederholung, Nennung im nächsten Bericht; Halt hängt nie an der Nachricht | Nachrichtendienst nicht erreichbar: Zustellung wird nicht als erfolgt gemeldet, Halt greift trotzdem | Stufe 6 | berücksichtigt | offen |
| 4·T2 | Storno nach Teilfüllung | Kap. 10 Reservierungen: frei wird nur der nicht gefüllte Rest | Kauf über 10 Stück, 4 gefüllt, Rest storniert: frei werden nur die Mittel für 6 Stück, die Ausstiegsreserve bleibt | Stufe 3 | berücksichtigt | offen |

## Prüfung 5 – Stand b5f50ce (1 Restpunkt)

| Befund | Kern | Regel / Änderung | Negativtest | Abnahme | Architektur | Technisch |
| --- | --- | --- | --- | --- | --- | --- |
| 5·B02 | „Unvollständig“ heißt nicht „unbestätigt“; ein abgebrochenes Schreiben und eine später gekürzte, bestätigte Zeile hinterlassen dieselben Bytes | Kap. 9: Anhangsvermerk mit bestätigtem Stand und geplantem Anhang; gekürzt wird nur, was der Vermerk als unbestätigten Anhang ausweist; sonst Halt und Reparaturverfahren, nichts wird still zurückgesetzt; mehrdeutige Fälle bleiben gesperrt | Pflicht vor Schließen von 4·B02: (1) nach bestätigtem Vorgang Abschlusszeichen der letzten Zeile entfernen: Halt, Budget und Vorgänge unverändert; (2) letzte bestätigte Zeile teilweise kürzen: ebenso; (3) echter Schreibabbruch mit passendem Vermerk wird gekürzt; (4) erneuter Absturz in der Wiederherstellung ändert nichts Bestätigtes | Stufe 2 | berücksichtigt | offen |

## Klarstellung Wolfgang – Stand fb66237

| Befund | Kern | Regel / Änderung | Negativtest | Abnahme | Architektur | Technisch |
| --- | --- | --- | --- | --- | --- | --- |
| 6·W1 | Ziel ist ein Plus nach allen Kosten; der Betrag ist offen; keine feste Aufteilung; Dachs und Maus arbeiten die Einzelwerte aus | Kap. 1 Ziel und Budget, R15 Paketfreigabe, Kap. 13 und 18 Beträge als Beispiele, Kap. 19 Stufe 2, Kap. 21 und 22 | Festschreibung mit anderem Betrag (z. B. 2.000 €) lädt ohne Codeänderung; eine Festschreibung ohne Wolfgangs Paketfreigabe wird abgelehnt; Bewertung enthält KI, Gebühren, Spread und Fondskosten | Stufe 2 | berücksichtigt | offen |
