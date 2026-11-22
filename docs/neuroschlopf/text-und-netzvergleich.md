# Neuro2026 · Text- und Netzvergleich

Der Versuch ist ein Baustein der [Kapitalrotation mit sechs Modellen](kapitalrotation.md). Er misst, ob NeuroSchlopf relevante Nachrichten und Termine für eine offene Depotthese besser findet. Er prüft getrennt, was vortrainierte Textmerkmale beitragen und was zusätzliche Netzbreite beiträgt.

## Kandidaten

Alle vier Kandidaten sortieren dieselben bereits zum Wissensstichtag verfügbaren Belege je These. Bei gleichen Werten entscheidet die bisherige Rangfolge und zuletzt die Fallkennung.

| Kandidat | Rangwert | Was der Vergleich prüft |
|---|---|---|
| A · Grundrang | Heutige Belegsortierung in DepotSchlopf | Ausgangswert |
| B · Textähnlichkeit | Kosinusähnlichkeit zwischen These und Beleg | B gegen A misst den Beitrag des Textmodells |
| C · Kompaktes Depotnetz | 1.029 → 512 → 512 → 128 → 1 | C gegen B prüft, ob beaufsichtigtes Depottraining samt Depotmerkmalen hilft |
| D · Breites Depotnetz | 1.029 → 2.048 → 512 → 128 → 1 | D gegen C prüft den Beitrag der zusätzlichen Breite |

Für B, C und D liefert das lokal ausgeführte, eingefrorene [Qwen3-Embedding-0.6B](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B) je Text einen normalisierten Vektor mit 1.024 Komponenten. Die Anfrage besteht aus der damaligen These und der festen Instruktion `Retrieve evidence relevant to assessing this investment thesis at its stated cutoff.` Der Beleg besteht aus der damals vorliegenden Überschrift und dem damals gespeicherten Textauszug oder Termineintrag. Leere Belegtexte werden als fehlende Daten gezählt. Die Auswertung protokolliert die verwendete Hub-Revision, das Tokenizer-Verfahren, Textgrenzen und Eingabeabdrücke. Die Vektoren werden je Text und Modellrevision einmal erzeugt und lokal wiederverwendet.

C und D erhalten das elementweise Produkt beider Vektoren, dessen Summe als Kosinuswert und die vier vorhandenen Depotmerkmale. Beide erzeugen einen Sortierwert pro Paar. Beim Vergleich ändern sich ausschließlich die Breite der ersten verdeckten Schicht und die daraus folgenden Parameter. Architekturregeln: SiLU in den verdeckten Schichten, linearer Ausgang. Für die erste Messung verwenden beide AdamW, quadratischen Fehler für die Ziele 0 und 1 und dieselben dokumentierten Startwerte `42`, `43` und `44`. Der erste Vergleich setzt Stapelgröße 64, maximal 80 Epochen, Lernrate 0,001 und Gewichtszerfall 0,0001 für beide fest. Gewählt wird der früheste Stand mit dem höchsten nDCG@3 im Validierungsfenster. Nach zehn Epochen ohne Verbesserung endet das Training. Fehlen dort vollständig bewertete Prüfgruppen, bleibt der Lauf technisch und liefert keinen ausgewählten Kandidaten. Der Merkmalsvertrag für C und D akzeptiert endliche, auch negative Textmerkmale. Der bisherige Vierwert-Vertrag bleibt für den Referenzlauf verfügbar.

## Fallbestand und Zeittrennung

- **Lokale Falldaten.** DepotSchlopf ordnet These, Beleg und KI-Urteile über die bestehende Fallkennung zu. Jeder Satz enthält die These zum damaligen Stand, den belegten Text, den Wissensstichtag, die vier Merkmale und den Grundrang. Hinzu kommt eine Ereigniskennung, die Wiederholungen desselben Ereignisses über Quellen hinweg zusammenhält. Lässt sich diese Kennung nicht zuverlässig bilden, läuft der Fall in der technischen Vorprüfung mit und wird für die belastbare Zeitprüfung separat ausgewiesen.
- **Urteile.** Dachs und Maus erhalten je Paar unabhängig voneinander denselben eingefrorenen Textstand. Gemeinsames „Kein Bezug“ ergibt 0. Gemeinsames „stärkt“ oder „schwächt“ ergibt 1. Fehlt ein Urteil, ist es offen oder unterscheiden sich die Urteile, bleibt das Paar unbewertet. Für Prüfgruppen bestimmt eine vorab festgelegte, für alle Kandidaten gleiche Stichprobe die zu bewertenden Paare. Für nDCG wird der gesamte jeweilige Kandidatenpool bewertet. Das eingestellte Tageslimit für externe Fragen gilt.
- **Zeitfenster.** Training, Validierung und Prüfung enthalten aufeinanderfolgende Handelszeiträume mit jeweils drei vollen Handelstagen Abstand. Ein unternehmensbezogenes Ereignis einschließlich seiner Folgeberichte darf nur einem Fenster zugeordnet sein. Doppelte und später aktualisierte Quellenfassungen, die mehrere Fenster verbinden, werden für diese Messung aus den betroffenen Fenstern entfernt und gezählt. Kein Dokument und kein Ergebnis mit einem späteren Wissensstand fließt in die Eingabe seines Prüffalls ein.
- **Verfügbare Stichprobe.** Der vorhandene Analyseexport enthält 105 historische Paare von einem einzigen Prüftag. Dieser Export genügt für die technische Durchstichprobe. Für den Zeitvergleich brauchen wir zusätzliche Handelstage mit lokal protokollierten KI-Urteilen. Frühere Meldungen können die Verarbeitung prüfen. Die belastbare Bewertung stützt sich zusätzlich auf einen anschließenden Zeitraum, in dem neue Meldungen erst nach Festlegung der Varianten eingehen.

## Messung und Entscheidung

1. **Belegauswahl.** Pro These und Prüftag zählt, ob wenigstens ein relevanter Beleg unter den ersten drei steht. Die Auswertung vergleicht A bis D auf denselben Gruppen und meldet die Abdeckung. Gibt es nur für Teile des Pools gültige Doppelurteile, gehen in die gemeinsame Top-3-Auswertung ausschließlich Gruppen ein, deren vereinigte Top 3 aller Kandidaten vollständig beurteilt sind. nDCG@3 ergänzt den Vergleich für vollständig beurteilte Gruppen mit mindestens einem relevanten Beleg. Ergebnisse werden zusätzlich nach Prüftag und Ereignis ausgewiesen. Der Nutzenvergleich bewertet die Übereinstimmung mit unabhängigen KI-Urteilen zur Relevanz. Spätere bestätigte Ereignisse und erwartete Auswirkungen brauchen eine eigene Auswertung.
2. **Rechenaufwand.** Auf demselben Windows-Rechner misst der Lauf Textkodierung, Netztraining und Rangberechnung getrennt. Er protokolliert kalten Start, Folgeabruf mit vorhandenen Vektoren und Gesamtdauer. Die RTX wird mit den gleichen Modell- und Dateneinstellungen gegen CPU gemessen, sofern der jeweilige Backend-Pfad diese Architektur unterstützt. GPU-Auslastung ist eine separate Messgröße.
3. **Entscheidung.** Die breitere Variante kommt erst für den regulären Beobachtungsbetrieb infrage, wenn ihr Vorteil gegen C an späteren Ereignissen über Tage und Startwerte wiederkehrt. Wenn B den größten Zugewinn liefert und C oder D ihn nicht verlässlich steigern, reicht zunächst B. Solange die Abdeckung gering ist, kennzeichnet der Bericht die Ergebnisse als vorläufig und zeigt die Zähler.

## Erster Implementierungsschritt

Der nächste Code-Take liest die belegten Fälle und erzeugt einen lokalen Offline-Durchstich für A und B. Die technische Probe enthält eine deutsch-englische Textzuordnung, einen thematischen Beleg ohne Firmennamen, eine unabhängige Meldung über dasselbe Unternehmen und einen Neuabdruck über der Datumsgrenze. Dessen Ausschluss aus dem Trainingsfenster ist Teil der Abnahme. Danach lässt sich derselbe festgeschriebene Fallbestand für C und D verwenden. Der bestehende Pilot mit vier Merkmalen und zwei gleich breiten Schichten bleibt als eigener Referenzlauf nutzbar.

Forschungsbezug: [Qwen3 Embedding](https://arxiv.org/abs/2506.05176), [E2Rank](https://aclanthology.org/2026.emnlp-main.2171/) und [TabM](https://proceedings.iclr.cc/paper_files/paper/2025/hash/c1ba41c694834aeef91ae161711d4939-Abstract-Conference.html). Die Breiten sind Versuchskandidaten für diese Aufgabe.
