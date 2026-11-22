# NeuroSchlopf · Topologien für Kapitalrotation

Die [sechs Rollen](kapitalrotation.md) bleiben im Protokoll sichtbar. Der Architekturvergleich untersucht, welche Rollen deterministisch rechnen können, welche sich eine gelernte Marktverarbeitung teilen und welche ein eigenes Netz brauchen.

## Vergleichskandidaten

| Kandidat | Trainierte Netze | Aufbau |
|---|---:|---|
| T0 · Vergleichsregel | 0 neue | Markt- und Fondsdaten nach belegten Zeitstempeln aufbereiten. Sektorrelative Stärke mit einem einfachen zeitlichen Vorlauf vergleichen. Den geschätzten Vorteil gegen die beobachtbaren Kosten prüfen. |
| T1 · Gemeinsames Marktmodell | 1 neues, dazu ein eingefrorenes Textmodell | V1 und V3 berechnen Marktmerkmale und belegte Verbindungen. V2 liefert wiederverwendete Ereignisvektoren. Ein gemeinsames Zeitmodell verarbeitet Markt- und Ereignisdaten. Getrennte Ausgänge schätzen Rotation und spätere Renditeverteilung. A3 rechnet mit Quotes, Gebühren und Ausführungsannahmen. |
| T2 · Zeitmodell und Graphmodell | 2 neue, dazu dasselbe Textmodell | Ein zeitlicher Prädiktor und ein eigenständiges Graphmodell liefern Vorhersage und Rotationshypothese. Der Graph bekommt nur damals belegte Kanten oder anhand des Trainingsfensters ermittelte Vorläufe. A3 verwendet dieselbe Kostenrechnung. |
| T3 · Sechs eigenständige Modelle | bis zu 6 neue | Die ursprüngliche Spezialisierung dient als Vergleich, sobald jede Rolle eigene Trainingssignale und genügend Fälle hat. A3 kann später ein Modell für Slippage und Ausführungswahrscheinlichkeit erhalten, sobald genügend belegte Quote- oder Ausführungsdaten vorliegen. |

Der erste trainierte Kandidat beginnt mit zwei kausalen Faltungsblöcken über die Zeit und 64 bis 128 Kanälen. Die getrennten Ausgänge erhalten dieselbe Marktdarstellung, eigene Zielgrößen und getrennte Gütemaße. Ein lineares Zustandsmodell prüft, ob die Faltungen nötig sind. Im Graphkandidaten prüft ein dünn besetzter, zeitversetzter Graph mit getrennten Kantentypen zuerst die gut nachvollziehbaren Verbindungen. Ein lernendes Graphnetz kommt als weiterer Kandidat hinzu, wenn die einfachere Kantenrechnung erreicht und übertroffen werden kann.

Ein breites Netz mit 2.048 Neuronen lässt sich am Ausgang testen, sobald genug historische Fälle vorliegen. Für den Vergleich erhält es dieselben Eingaben wie ein schmaler Ausgang. Wahlweise können Rotation und Prognose getrennte Pfade bekommen, falls die gemeinsame Darstellung nachweislich einer Aufgabe schadet. Eine automatische Topologiesuche wählt Breite, Tiefe und Fenster aus einer vorab protokollierten Menge. Die Gewichtung der Aufgaben und die Wahl des Modells erfolgen auf dem Validierungszeitraum. Der spätere Prüfsatz bleibt für diese Wahl gesperrt.

## Auswertung und Rechenaufwand

- Alle Kandidaten verwenden denselben Wissensstichtag, dieselben Instrumente, dieselben Ausführungsannahmen und dieselben später beobachteten Entscheidungen. Die Auswertung veröffentlicht Treffer, Nettoergebnis, Verlustphasen, Enthaltungen und Datenabdeckung für dieselben Zeitfenster.
- Der Vergleich misst, welche Rolle zusätzlichen Nutzen bringt, indem jeweils eine Rolle ausgesetzt oder durch die einfache Variante ersetzt wird. Die gemeinsame Verarbeitung und getrennte Netze werden zusätzlich bei vergleichbarem Parameter- oder Rechenbudget ausgewiesen, soweit technisch möglich.
- CPU und RTX werden je mit derselben Modellversion und denselben Daten gemessen. Text wird bei einer neuen Meldung einmal kodiert. Der zeitliche Prädiktor verarbeitet Instrumente und Horizonte in Gruppen. Bei einem GPU-Lauf bleibt die gemeinsame Darstellung bis zu den Ausgängen auf der Karte. Der Bericht misst kalten Start und Folgeläufe, Antwortzeit am langsamen Ende der Verteilung, Spitzenbedarf an Grafikspeicher und Rechenzeit. Er weist außerdem die Zeit aus, die zwischen verfügbarem Datensatz und fertig bewerteter Papierchance liegt.
- Eingesetzt wird die einfachste Variante, die nach Kosten auf späteren Zeitfenstern einen belastbaren Vorteil liefert. Wo sich Ergebnis und Antwortzeit unterschiedlich verbessern, dokumentiert der Bericht beide Größen und die Entscheidung je Horizont. Kein Architekturvergleich verändert das aktive Depot oder sendet Aufträge.

## Forschungsbezug

- [Zeng et al., AAAI 2023](https://arxiv.org/abs/2205.13504): Einfache lineare Modelle sind eine ernsthafte Vergleichsgröße auf Zeitreihen-Benchmarks.
- [Ma et al., KDD 2018](https://doi.org/10.1145/3219819.3220007): Gemeinsame Experten mit aufgabenspezifischer Auswahl können den Nutzen geteilter Merkmale bei mehreren Aufgaben berücksichtigen.
- [Gastinger et al., NeurIPS 2024](https://proceedings.neurips.cc/paper_files/paper/2024/hash/fda026cf2423a01fcbcf1e1e43ee9a50-Abstract-Datasets_and_Benchmarks_Track.html): Kantentypen und einfache Vergleichsregeln sind auf zeitlichen Graph-Benchmarks relevant.
