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

[Bewertungsregeln in #325](https://github.com/Funcry241/DepotSchlopf/issues/325#issuecomment-6958388675) · [Messplan in #325](https://github.com/Funcry241/DepotSchlopf/issues/325#issuecomment-6958343353)
