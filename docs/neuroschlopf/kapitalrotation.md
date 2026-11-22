# NeuroSchlopf · Kapitalrotation

NeuroSchlopf soll erkennen, wo Verkaufsdruck zunimmt, wo Kaufdruck entsteht und ob eine mögliche Rotation nach Kosten noch im Papierdepot nutzbar ist. Zu jedem Entscheidungszeitpunkt liefert es eine Antwort mit möglichen Quellen und Zielen der Rotation, den tragenden Belegen, ihrer Unsicherheit und dem Zeitpunkt, bis zu dem die Aussage gilt.

## Was wir messen können

Das Marktmodell verwendet einen zeitabhängigen Graphen. Knoten sind liquide Instrumente, Sektor-ETFs, Marktbenchmarks und Währungs- oder Zinsfaktoren. Kanten beschreiben belegte wirtschaftliche Verbindungen oder statistisch überprüfte Vorläufe. Die Modelle pflegen Herkunft, Wirkungsrichtung, Horizont und Unsicherheit je Kante. Ein Preisrückgang in einem Sektor und ein gleichzeitiger Anstieg in einem anderen ergeben zunächst eine mögliche Rotation. Gemeinsame Faktoren wie Zinsen und Marktbewegung sowie kurzfristige Liquiditätsschocks kommen als alternative Erklärung hinzu.

Die Strömungsanalogie hilft bei der Modellierung. Druck entspricht gerichteter Nachfrage oder einem Angebotsungleichgewicht. Markttiefe bestimmt mit, wie stark der Preis auf diesen Druck reagiert. Der Graph lässt Zuflüsse, Abflüsse und einen unbekannten Außenbereich zu. Chronologisch aufeinanderfolgende Bewegungen belegen für sich allein keinen Geldtransfer zwischen zwei Instrumenten. Marktwertänderungen und Handelsumsätze lassen sich nicht als erhaltener Geldstrom bilanzieren.

| Evidenz | Was sie aussagt |
|---|---|
| Veröffentlichte Nettozeichnungen und Rücknahmen | Belegter Nettofluss auf der jeweiligen Fondsebene und für den veröffentlichten Zeitraum. Maßgeblich für die Verfügbarkeit ist die Veröffentlichungszeit. |
| Gerichtete Trades und Änderungen an Geld- und Briefseite | Lokaler Kauf- oder Verkaufsdruck am beobachteten Handelsplatz. Die Qualität hängt von der Abdeckung, dem Zeitstempel und gegebenenfalls der Handelsrichtungsschätzung ab. |
| Volumen, relative Renditen und Marktbeteiligung | Indirekte Hinweise auf Rotation. Die Auswertung ordnet sie gegenüber Sektor, Gesamtmarkt und gemeinsamen Risikofaktoren ein. |
| Nachrichten, Termine und Unternehmensbeziehungen | Erklärungen und mögliche Übertragungswege. Die Modelle prüfen Quelle, Veröffentlichungszeit, betroffene Geschäftsbereiche und konkurrierende Erklärungen. |

DepotSchlopf belegt heute Schlusskurse, Nachrichten und Termine. Damit beginnt die Prüfung auf Tageshorizonten. Eine Entscheidung für die nächsten Minuten braucht rechtzeitig verfügbare Intraday-Quotes, Trades und Ausführungsinformationen für dieselben Instrumente. Für einen ersten Intraday-Versuch wird ein liquides Teiluniversum mit zusammenhängender Handelszeit ausgewählt. Die Datenabdeckung entscheidet, welche Laufzeiten die Modelle sinnvoll bewerten können.

## Sechs Modelle mit zwei parallelen Stufen

Alle Eingänge desselben Laufs teilen einen eingefrorenen Wissensstichtag. Die drei vorbereitenden Modelle laufen parallel. Sobald ihr Datenstand für den Lauf geschlossen ist, werten drei weitere Modelle parallel aus. Jede Ausgabe führt den Wissensstichtag, Datenalter, Quellenkennungen, Unsicherheit und Modellversion mit.

| Modell | Aufgabe | Fester Ausgang |
|---|---|---|
| V1 · Marktstruktur | Vereinheitlicht Zeiten, Währungen und Handelssitzungen. Berechnet relative Bewegung, Marktbreite, Volumenabweichungen und bei geeigneten Daten Orderflussungleichgewicht, Spread und Tiefe. | Merkmale je Instrument und Horizont mit Abdeckung und Datenalter. |
| V2 · Ereignisse | Verbindet Nachrichten, Termine und Pflichtmitteilungen mit Unternehmen, Geschäftsbereichen und Sektoren. Gruppiert Folgemeldungen und kennzeichnet Neuigkeit sowie Widersprüche. | Ereignis mit betroffenen Knoten, belegtem Zeitpunkt und Quellen. |
| V3 · Kapital und Verflechtungen | Ordnet Fondsflüsse, ETF-Zusammensetzung, wirtschaftliche Verbindungen und gemeinsame Risikofaktoren ein. | Zeitabhängiger Verflechtungsgraph mit veröffentlichten Flüssen und unbekannten Bereichen. |
| A1 · Rotation | Schätzt Druck auf den Knoten und mögliche zeitversetzte Ausbreitung über Kanten. Prüft, ob ein gemeinsamer Faktor die Bewegung erklärt. | Mögliche Rotation mit Ausgang, Ziel, Verzögerung, Evidenztyp und Unsicherheit. |
| A2 · Prognose | Schätzt die Verteilung der späteren Rendite und wie rasch das Signal typischerweise abklingt. Berücksichtigt Gegenbewegungen und Regimewechsel. | Renditeband je Horizont, Wahrscheinlichkeit eines Vorteils gegenüber der Vergleichsgröße und Gültigkeitsdauer. |
| A3 · Ausführbarkeit | Prüft Einstieg und Ausstieg am nächsten belegbar handelbaren Kurs. Schätzt Gebühren, Spread, Slippage und Liquidität. Vergleicht Handeln mit Halten und Auslassen. | Nettovorteil als Schätzband, möglicher Ausführungszeitpunkt und Gründe für eine Enthaltung. |

Die Zusammenführung ist eine prüfbare Regel. Sie gibt eine Papierchance aus, wenn Richtung und Horizont zusammenpassen, die Daten zum Entscheidungszeitpunkt verfügbar sind und die Ausführung nach Kosten einen plausiblen Vorteil lässt. Andernfalls lautet die Antwort `Beobachten` oder `Daten fehlen`. Bei einer möglichen Chance nennt sie Instrument, Belege, erwartetes Nettoergebnis als Band, Verfallszeit und Widerlegungskriterium. Das tatsächliche Ergebnis wird erst zum passenden späteren Zeitpunkt ergänzt. Für diesen Versuch schreibt die Zusammenführung lediglich Beobachtungen ins Papierprotokoll.

## Selbstwahl der Topologie

Jedes der sechs Modelle darf seine Architektur getrennt auswählen. Die Ausgangskandidaten orientieren sich an seiner Aufgabe. Bei V1 eignen sich einfache Filter und Zeitmodelle als Basis, bei V2 ein vortrainiertes Textmodell und ein anpassbarer Kopf, bei V3 ein kleiner Graph. Für die Auswertung treten unter anderem Zustandsraummodelle, temporale Faltungen, Graphmodelle und breite Netze als Kandidaten an. Die Variante mit 2.048 Neuronen kann dort antreten, wo Eingaben und Fallzahl einen sinnvollen Vergleich zulassen.

Jede Rolle hält ihr Ausgabeschema stabil und führt ein aktuell verwendetes Modell sowie getrennte Alternativen. Die Suche darf Breite, Tiefe, Fenstergröße, Eingabegruppen und bei Graphmodellen die Kantenwahl verändern. Die Modelle bewerten Vorschläge in historischen Vorwärtsfenstern mit bekanntem Wissensstichtag. Eine bessere Variante wird erst nach einer späteren Prüfung in einem Schattenlauf übernommen. Zusätzlich zählen Latenz, Datenabdeckung und Robustheit. Modellwechsel erscheinen mit Daten- und Modellversion im Protokoll. Während eines laufenden Entscheidungsfensters bleibt die eingesetzte Version fest. Bleibt eine Rolle aus, gibt die Zusammenführung bei fehlender entscheidender Evidenz eine Enthaltung aus. Das bisherige Modell bleibt für einen Rückfall verfügbar.

## Der erste überprüfbare Lauf

1. **Zwei bis drei liquide Sektorkörbe, ein Marktbenchmark und ihre beobachteten Titel.** DepotSchlopf spielt vorhandene Quellen in der Reihenfolge ihrer tatsächlichen Verfügbarkeit ab. Der erste Horizont ist der nächste Handelstag. Die sechs Rollen erhalten getrennte einfache Ausgangsmodelle mit denselben Ausgabeverträgen wie spätere komplexere Varianten.
2. **Fester Entscheidungs- und Ausführungszeitpunkt.** Zeitstempel für Ereignis, Veröffentlichung, Abruf und Entscheidung werden getrennt gespeichert. Der simulierte Einstieg verwendet frühestens den nach der Entscheidung verfügbaren ausführbaren Kurs. Fehlt er, bleibt der Fall als nicht ausführbar sichtbar. Intraday-Auswertungen folgen mit einem eigenen, passend belegten Marktfeed und gemessener Gesamtlatenz.
3. **Messung gegen einfachere Ansätze.** Ein Sektormomentum-Signal und die bestehende DepotSchlopf-Basis liefern Vergleichswerte. Gemessen werden Nettoergebnis, Umschlag, Verlustphasen, Treffer nach Horizont und Kalibrierung. Ein weiterer Vergleich prüft für jede Rolle, welche Verbesserung sie tatsächlich beiträgt. Entwicklungs-, Validierungs- und Prüfzeitraum bleiben getrennt. Mehrere auf dasselbe Ereignis zurückgehende Signale werden gemeinsam ausgewertet. Die Zahl der getesteten Varianten und ihre Auswahlregeln werden protokolliert.
4. **Vorwärtsbeobachtung.** Das System friert die ausgewählten Modelle ein, schreibt jede Papierentscheidung vor ihrem Ergebnis fest und prüft nach Ablauf des Horizonts den tatsächlichen Verlauf. Damit wird erkennbar, welche Hinweise bei aktuellen, unbekannten Ereignissen früh genug kommen und nach realistisch modellierten Kosten noch tragen.

Der [Text- und Netzvergleich](text-und-netzvergleich.md) dient in dieser Architektur als Baustein für V2. Er misst die Zuordnung von Meldungen zu Thesen. Die Kapitalrotationsmessung prüft zusätzlich Vorlauf, Ausführung und Nettoergebnis.

## Forschungsbezug

- [Cont, Kukanov und Stoikov: The Price Impact of Order Book Events](https://arxiv.org/abs/1011.6402) untersucht Orderflussungleichgewichte und Markttiefe auf kurzen Zeitskalen.
- [Gabaix und Koijen: The Inelastic Markets Hypothesis](https://www.nber.org/papers/w28967) untersucht die Wirkung von Kapitalflüssen auf den Gesamtmarkt.
- [SEC: Things to Consider before Investing in ETFs](https://www.sec.gov/investor/alerts/etfs.pdf) erläutert Primärmarkt und Handel bestehender ETF-Anteile.
- [Bailey und López de Prado: The Deflated Sharpe Ratio](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=2460551) behandelt die Verzerrung bei der Auswahl aus vielen getesteten Handelsstrategien.
