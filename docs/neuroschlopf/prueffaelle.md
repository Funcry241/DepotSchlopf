# Blindbewertung der Belegauswahl

Die Bewertung prüft, wie gut die Rangfolge Belege zu einer zum Wissensstichtag offenen These priorisiert. Pro Prüffall verwenden alle Sortierer dieselbe rekonstruierte Belegliste und dieselbe These. Die gespeicherte KI-Frage belegt, welche Ereignisse im Wortlaut oder als Titel sichtbar waren.

## Unterlagen für die Prüfung

Die Prüfer erhalten anonymisierte Thesen mit Wert, Richtung, Horizont und Widerlegungskriterium, soweit diese Angaben vorliegen. Zu jedem Beleg sehen sie Text, Quelle und Veröffentlichungszeit. Fall- und Belegkennungen erlauben eine eindeutige Rückzuordnung. Die Belege erscheinen in einer reproduzierbar zufälligen Reihenfolge. Die Position in der Frage, die Sortiererränge, die Auswahl durch die KI, Orders und spätere Kursdaten bleiben in der Auswertung bei der Zuordnung.

| Stufe | Bedeutung |
|---|---|
| 0 | Der sichtbare Beleg hat keinen Bezug zur These. |
| 1 | Der Beleg betrifft die These, liefert aber keinen entscheidenden Hinweis. |
| 2 | Der Beleg kann die These stützen oder widerlegen. Ein passender Termin im Horizont kann eine entscheidende Prüfung ankündigen. |
| Unbewertet | Der Prüfer kann den Beleg für diese These mit den vorliegenden Informationen nicht einordnen. |

Die Richtung wird gesondert erfasst als stützt, widerlegt oder offen. Ohne offene These, nachvollziehbaren Wissensstichtag oder beurteilbaren Belegtext bleibt der Fall unbewertet. Das gilt auch für Belege, die erst nach dem Stichtag verfügbar waren, und für weitere Exemplare derselben Meldungsgruppe am selben Tag. Verschiedene Thesen zum selben Wert werden einzeln bewertet. Branchenmeldungen zählen, wenn sie den Wert oder einen in der These genannten Zusammenhang betreffen. Allgemeine Makroereignisse ohne Bezug zu einem Treiber der These erhalten Stufe 0.

## Synthetische Sollfälle

Die folgende Tabelle ist der Lösungsschlüssel für den Trockenlauf. Beim Blindtest erhalten Prüfer aus der Tabelle die Fallkennung, die These und den Beleg.

| Fall | These | Sichtbarer Beleg | Sollurteil |
|---|---|---|---|
| 1 | A · NVIDIA steigt in fünf Handelstagen, entscheidend ist der Rechenzentrumsumsatz | NVIDIA legt innerhalb des Horizonts Quartalszahlen vor. | 2 · offen |
| 2 | A | NVIDIA hebt das Umsatzziel für Rechenzentren an. | 2 · stützt |
| 3 | A | Ein Großkunde storniert einen NVIDIA-Rechenzentrumsauftrag. | 2 · widerlegt |
| 4 | A und B · NVIDIA fällt in fünf Tagen wegen schwacher Rechenzentrumsumsätze | NVIDIA hebt das Umsatzziel für Rechenzentren an. | A: 2 · stützt. B: 2 · widerlegt. |
| 5 | C · SAP steigt wegen des Cloudgeschäfts | Sap-Anbau in Brasilien leidet. | 0 |
| 6 | C | Fed senkt den Leitzins. | 0 · Makro |
| 7 | C | SAP-Aktie im frühen Handel gefragt. | 1 · offen |
| 8 | C | Gerücht über einen SAP-Großauftrag. | 1 · offen |
| 9 | C | SAP-Meldung zu … | Unbewertet · Text zu kurz |
| 10 | Keine offene These | Siemens erhält einen Großauftrag. | Unbewertet · keine These |
| 11 | C · Stichtag 17. November | Meldung veröffentlicht am 18. November. | Unbewertet · nach Stichtag |
| 12 | A | NVIDIA legt erst nach dem Horizont Quartalszahlen vor. | 1 · offen |

## Auswertung des ersten Exports

1. Anzahl der rekonstruierbaren Fälle, Prüftage und unterschiedlichen Meldungsgruppen ausweisen. Zeigen, bei welchen Aufrufen die gespeicherte Frage oder die passende These fehlt.
2. Für den Blindtest die anonymisierten Fälle erstellen. Eine unabhängige Zweitbewertung verwendet dieselben Unterlagen für eine Stichprobe von 10 % der Prüftage.
3. Treffer unter den ersten drei, nDCG@3 und den Anteil sichtbarer Gegenbelege der Stufe 2 vergleichen, sobald unabhängige Urteile vorliegen. Unbewertete Belege bleiben als solche ausgewiesen. Fälle ohne relevanten Beleg und Fälle mit höchstens drei Kandidaten separat zählen. Meldungsgruppen nach ihrem ersten Auftreten zeitlich trennen, bevor ältere Fälle zum Training und spätere zur Prüfung verwendet werden.

Die automatisierte Gegenprobe `grundrang_holt_einen_gegenbeleg_vor_die_platzgrenze_der_frage` prüft die Sichtbarkeit eines synthetischen Gegenbelegs mit dem vorhandenen Fragegenerator. Sie läuft lokal und verwendet dieselben 45 Kandidaten in beiden Fragen.

## Einen Prüftag in DepotSchlopf bewerten

Unter **Live → NeuroSchlopf** öffnest du die Belegprüfung. Wähle einen Prüftag. DepotSchlopf zeigt dir für jede Bewertung den damaligen Thesentext und einen in der gespeicherten Frage sichtbaren Beleg. Die Reihenfolge ist für denselben Datenstand reproduzierbar gemischt. So kannst du die Prüfung unterbrechen und später beim nächsten offenen Paar fortsetzen.

Mit freigegebenen externen Fragen kann NeuroSchlopf die Paare des jüngsten belegten Prüftags Maus und Dachs zur unabhängigen Vorprüfung geben. Empfängerauswahl und Tageslimit gelten auch hier. Klare Übereinstimmungen gelten als KI-Bewertung. Die Blindprüfung hält strittige und unklare Fälle, Einzelbewertungen und eine gemischte Stichprobe aus den Übereinstimmungen offen. Sie blendet die KI-Urteile aus. Sobald du ein Urteil speicherst, verschwindet dieses Paar aus der offenen Liste.

Entscheide anhand des angezeigten Wortlauts. Stufe 1 genügt, wenn die Meldung einen Treiber der These erwähnt, die Behauptung aber weder bestätigt noch widerlegt. Bei Stufe 2 gibst du zusätzlich die Richtung an. Kannst du den Zusammenhang anhand der Unterlagen nicht beurteilen, wählst du **Unbewertet**. Diese Entscheidung zählt zum Bearbeitungsfortschritt und bleibt für die spätere Messung von Stufe 0 unterscheidbar.

DepotSchlopf speichert die Urteile auf dem Rechner. Die externe Sicherung enthält auch die Bewertungsdatei `daten/neuro2026-belegurteile.tsv`. Darin verweist jede Bewertung auf ihre konkrete historische These und auf den Beleg im Analyse-Export. Die manuelle Prüfung lässt sich auch bei ausgeschaltetem Hintergrundvergleich öffnen.

[Bewertungsregeln in #325](https://github.com/Funcry241/DepotSchlopf/issues/325#issuecomment-6958388675) · [Messplan in #325](https://github.com/Funcry241/DepotSchlopf/issues/325#issuecomment-6958343353)

## Belegte Modelleingaben im Analyse-Export

`neuro2026_eingaben.jsonl` liefert für den ausgewählten Zeitraum Zahlenvektoren vor der Kürzung der KI-Frage. Der Kopf nennt Zeitraum, Länge und SHA-256 des bestätigten Exportjournals sowie das Merkmalsschema mit SHA-256 über dessen UTF-8-Text. Der Programmstand steht im selben Analyse-Archiv.

Eine Datenzeile gehört zu einem Tag, dem Wissensstichtag in UTC, einem Berater und dessen These zu einer ISIN. Die These folgt der bestehenden Grundrang-Regel für positive Vorschlagsziele in den fünf vorangegangenen Kalendertagen. Es zählen nur Begründungen, die beim Festhalten des Stichtags bereits im Journal standen. Das Feld `ref` verbindet Datum, einsbasierte Nummer im Frageabschnitt und SHA-256 des dort gespeicherten Belegtexts. Für die Sicht des Beraters nach Auswahl und Kürzung bleibt sein gespeicherter Fragetext maßgeblich.

Im Vorbereitungsformat `grundrang-paar-v1` enthält `x` in fester Reihenfolge den Wertbezug zur einzelnen These (0 oder 1), das Terminmerkmal (0 oder 1) und den ursprünglichen Fragenplatz (ab 1). Beispiel für einen SAP-Termin an zweiter Stelle bei einer SAP-These ist `[1,1,2]`. Das Feld `grundrang` nennt den einsbasierten Vergleichsplatz unter denselben Kandidaten für diese einzelne These. Rohtexte, Beträge und Zielwerte werden für diese Vektoren nicht benötigt. Ein unabhängiges Relevanzurteil ist für überwachte Trainingsbeispiele separat zu erheben. Das Multiplikationsmodell von Neuro2026 hat zwei Eingänge und gehört zu einem anderen Lernversuch.

Tage mit fehlendem Stichtag sowie fehlende, mehrfache, leere oder unvollständige Frageabschnitte erhalten Statuszeilen. Überschreiten die Eingaben die Textgrenze des Exports, enthält die Datei den Größenhinweis. Wähle dann einen kürzeren Zeitraum.

## Genaue Belegpaare für den Modellversuch

`neuro2026_belegpaare.jsonl` liefert dieselben historischen Paare wie die Blindprüfung. Die Fallkennung bindet den Wissensstichtag, die genaue Vorschlagsthese und den eindeutig in der gespeicherten Erstmeinungsfrage sichtbaren Quellenbeleg. Der Grundrang sortiert diese sichtbaren Kandidaten pro These. Die Datei enthält auch ihren ursprünglichen Fragenplatz.

Im Schema `depot-belegpaar-v1` stehen Wertbezug und Terminkennzeichen, der Anteil gemeinsamer Inhaltswörter im Beleg und die Aktualität am Wissensstichtag. Alle vier Werte liegen zwischen 0 und 1. Die Aktualität nimmt über sieben Tage linear ab. Termine und Belege ohne Veröffentlichungszeit erhalten dafür den Wert 0. Fragenplatz und Grundrang gehören zum Vergleich und stehen außerhalb des Merkmalsvektors.

Der Export ordnet Urteile anhand der Fallkennung zu. Die Kopfzeile des Exports zählt bearbeitete und bewertete Paare sowie KI-Übereinstimmungen. Thesen- und Belegtexte und die einzelnen Urteile bleiben auf dem Rechner. Fehlende Tagesunterlagen und ein nicht lesbarer Urteilsbestand werden im Export ausgewiesen.

## Lokaler Neuro2026-Beobachtungslauf

Der Beobachtungslauf lernt aus journalbelegten Übereinstimmungen von Dachs und Maus bis einschließlich des gewählten Trainingsendes. Die KIs urteilen unabhängig und erhalten den damaligen Wissensstand. Ein gemeinsam als stärkend oder schwächend bewerteter Beleg zählt als relevant. „Kein Bezug“ zählt als irrelevant. Einzelantworten, Widersprüche, offene Urteile und ausstehende Rückfragen liefern kein Trainingssignal. Die spätere Prüfmenge beginnt nach drei vollen Handelstagen Abstand. Meldungen mit identischem normalisiertem Text in der Prüfmenge entfallen im Training. Für den Start braucht das Modell aus dem Trainingsfenster mindestens eine KI-Übereinstimmung ohne Bezug und eine mit Bezug. Die Auswertung ordnet jedem Modellwert die genaue Fallkennung zu und vergleicht je These dieselben sichtbaren Belege mit dem Grundrang.

Der eigenständige Aufruf `depot-neuro-pilot` benötigt den gebauten DepotSchlopf-Dienst, den Projektordner sowie die drei Daten Beginn, Trainingsende und Prüfende in der Form JJJJ-MM-TT. Das lokale Neuro2026-Modellprogramm liegt neben dem Aufrufprogramm. Beende DepotSchlopf für den Modelllauf. Nach Neustart findest du den Bericht unter **NeuroSchlopf → Neuro2026 · Depotmodell**. Der Beobachtungslauf liest Journal und Gesprächsdaten. Modellantworten müssen zum exakten Inhalt der Trainings- und Prüfeingaben passen. Der Bericht ändert weder Journal noch Beraterfragen oder Aufträge.

Der Blindvergleich mischt die ersten drei Belege beider Rangfolgen sowie eine Kontrollauswahl pro angezeigter These. Rang und Herkunft aus der Auswahl bleiben in der Blindprüfung verborgen. Der Beobachtungsbericht wertet Treffer je These, sobald für beide Top-3-Listen vollständige KI-Übereinstimmungen vorliegen. nDCG@3 setzt eine vollständig beurteilte Kandidatenliste mit mindestens einem relevanten Beleg voraus. Kurze Listen mit höchstens drei Belegen fließen in keine der beiden Wertungen ein.

## GPU-Vergleich auf Windows

Der optionale Neuro2026-Probelauf verwendet Burn WGPU 0.21.0 mit einem ausdrücklich ausgewählten Hardwareadapter. CPU und GPU erhalten für jede Zeile der Auswertung dieselben Eingaben und Trainingsparameter. Die Gesamtdauer enthält Start, Training, Modellablage, erneutes Laden und Rangberechnung. Die separate Zeit für das Kerntraining umfasst auch die erste Initialisierung des Backends.

| Synthetischer Testfall | CPU gesamt | GPU gesamt |
|---|---:|---:|
| Depotnetz mit 16 Neuronen je verdeckter Schicht, Einzelabrufe | 3,37 s | 240,30 s |
| Dasselbe Depotnetz, gebündelte Fehler- und Rangabrufe | 3,08 s | 145,75 s |
| Lastprobe, 256 Neuronen je verdeckter Schicht, 24 Epochen, Batchgröße 128 | 3,07 s | 107,70 s |

Die ersten beiden Zeilen stammen aus demselben Windows-Stapeltest. Die separat wiederholte Lastprobe verwendete 4.096 synthetische Trainingspaare und 32 Prüfgruppen. CPU und GPU hatten dieselben Eingabeabdrücke, lasen den Trainingsfehler 24-mal zurück und erreichten je 32 Top-3-Treffer. Kerntraining dauerte 2,91 s auf der CPU und 103,04 s auf der GPU. Das Speichern dauerte jeweils rund 0,02 s. Der Bericht schlüsselt die Zeit innerhalb des Trainingskerns nicht nach Shader-Kompilierung, Rechenoperationen und Synchronisierung auf.

Die CPU bleibt für den Depotmodelllauf der Standard. Ein neuer GPU-Vergleich ist für eine deutlich andere Arbeitslast sinnvoll, etwa die 4-Bit-Textkodierung. Deren Messung ist vom WGPU-Training getrennt. Die vorliegenden Läufe messen keine GPU-Auslastung. Die 10 % bleiben ein Ziel für den späteren Hintergrundbetrieb. Die Grenze des Windows-Job-Objekts betrifft die CPU-Zeit.

Quelle für das optionale Backend: [Burn WGPU 0.21.0](https://docs.rs/crate/burn-wgpu/0.21.0/source/README.md).
