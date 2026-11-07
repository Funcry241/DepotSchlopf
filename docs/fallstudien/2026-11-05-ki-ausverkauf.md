# Fall 001: Dachs’ Einschätzung zum KI-Ausverkauf

Stand: 07.11.2026. Status: **Fristen laufen, noch kein Gesamtergebnis**.
Ursprung: [Issue #46](https://github.com/Funcry241/DepotSchlopf/issues/46).
Dieser Fall ist ein Forschungsbeleg für das Weltmodell und ändert keine Handelsregel.

## Herkunft und Schutz vor nachträglichem Wissen

GitHub führt die Eröffnung des Issues mit **06.11.2026, 13:15:37 UTC**
(14:15:37 Uhr Frankfurt). Der ursprüngliche Text selbst nennt 14:20 Uhr Frankfurt.
Beide Angaben werden getrennt bewahrt; die abweichende Eigenangabe ersetzt nicht
den GitHub-Zeitstempel. Dass Dachs unabhängig von Maus gearbeitet hat, ist seine
Angabe, kein technisch nachgewiesener Versuchsaufbau. Eine vorher festgehaltene
Erstprognose von Maus ist hier nicht belegt und wird nicht nachträglich ergänzt.

Die Marktbewegung und ihre politische Erklärung im Original sind Aussagen der
damals verlinkten Meldungen. Sie sind kein nachgewiesener Kausalzusammenhang.
Vor der Bewertung sind Indexstände und Veröffentlichungszeiten mit belastbaren
Quellen zu belegen. Es werden keine Kurse erfunden und keine später gewonnenen
Informationen in ein historisches Laufjournal geschrieben.

## Auswertung und Grenzen

- Kurze Frist: US-Handelstage 06., 09. und 10.11.2026; abschließend nach dem
  US-Schluss am **10.11.2026** auswerten. Zwischenstände separat protokollieren.
- Längere Frist: Kontrolltermine **20.11. und 04.12.2026** (zwei bzw. vier
  Kalenderwochen nach der Prognose). Der Ausgang bleibt bis dahin offen.
- Für den Nasdaq Composite werden derselbe Index und dessen offizielle
  Schlussstände vom 04. und 05.11. sowie jedem Prüftag benutzt, nicht nachträglich
  ein ETF oder ein anderer Index.
- Erholungsanteil pro Schlussstand:
  `q(t) = (S(t) - S(05.11.)) / (S(04.11.) - S(05.11.))`.
  Nur bei positivem Ausgangsverlust auswertbar. Die Originalthese nennt
  höchstens 50 %, ein Gegenkriterium mehr als 75 %. Werte zwischen diesen
  Grenzen sind kein Beweis für eine der beiden Aussagen. Zusätzlich den
  gesamten Verlauf zeigen, damit ein zwischenzeitliches Überschreiten sichtbar bleibt.
- Der Lieferketten-Korb und ein messbarer Vorbörsen-Indikator wurden nicht
  vorab definiert. Die betreffenden zusammengesetzten Kriterien sind deshalb
  **nicht objektiv vollständig auswertbar**. Keine nachträgliche Auswahl passender
  Unternehmen oder Vorbörsen-Proxys. Qualitative Beobachtungen ausdrücklich so benennen.
- Aussagen zu Investitionsplänen und Gesetzgebung brauchen verlinkte,
  zeitgestempelte Originalveröffentlichungen. Auftreten, Inhalt und
  Verfügbarkeit zum jeweiligen Entscheidungszeitpunkt getrennt erfassen.
- Eine richtige Richtung belegt keine profitable Strategie: erreichbarer
  Ausführungskurs, Kosten, Steuern und die Alternative Abwarten gehören in
  einen späteren Vergleich. Dieser Beleg löst keine Orders aus.

| Prüfung | Stand |
| --- | --- |
| Ursprüngliche Prognose und Herkunft erhalten | erledigt |
| Unabhängige Erstprognose Maus | nicht vorhanden, nicht nachholen |
| Schlusskurse und Quellenbelege für die kurze Frist | offen |
| Auswertung nach 10.11.2026 | offen, Frist läuft |
| Auswertung am 20.11. / 04.12.2026 | offen, Frist läuft |
| Vorab definierter Lieferketten-Korb / Vorbörsen-Indikator | fehlt im Original |

Issue #46 bleibt für die ausstehende Auswertung offen. Ein abgeschlossener
Code-PR bedeutet nicht, dass die Prognose schon bestätigt oder widerlegt wurde.

## Unverändertes Original aus Issue #46

**Festgehalten:** 06.11.2026, 14:20 Uhr (Frankfurt), vor dem US-Handel am 6.11. Unabhängig von Maus, ohne Maus' Einschätzung zu kennen. Erster Fall für das SchlopfModell (Schritt B).

## Lage laut Meldungen
Laut CNBC und Washington Post fiel der Nasdaq am 5.11. um rund 5,5 % und der S&P 500 um rund 4,4 %. Als Auslöser gilt die Sorge, dass ein Kongress unter Führung der Demokraten nach den Zwischenwahlen den KI-Ausbau langsamer und teurer macht. Für einzelne Werte gehen die Zahlen auseinander, bei Nvidia zwischen −6 % und −9 %.

## Erwartung
- **1 bis 3 Handelstage:** eine Gegenbewegung, aber nur eine Teilerholung. Der Nasdaq holt den Verlust vom Donnerstag höchstens zur Hälfte auf.
- **2 bis 4 Wochen:** weiterer Verkaufsdruck in der Lieferkette, also bei Rechenzentren, Kühlung, Strom und Netzwerk. Bei den großen Plattformen erwarte ich ihn weniger.

**Begründung:** Ein politischer Schock ändert die Erwartung an künftige Ausgaben, aber noch keine Umsätze. Die erste Reaktion übertreibt meist. Die Neubewertung der Firmen, die von den Investitionsausgaben leben, braucht dagegen länger.

## Woran ich merke, dass ich falsch liege
- Der Markt erholt sich vor der Eröffnung, und der Nasdaq schließt am 6.11. trotzdem unter dem Schluss vom 5.11. Dann ist mehr Verkaufsdruck da, als ich annehme.
- Der Nasdaq holt innerhalb von drei Tagen mehr als drei Viertel des Verlusts auf, und die Lieferkette steigt mit. Dann war es eine Überreaktion und keine Neubewertung.
- Große Plattformen wie Microsoft, Alphabet, Amazon und Meta bestätigen oder erhöhen ihre Investitionspläne. Dann fällt die These für die Lieferkette weg.
- Konkrete Gesetzentwürfe oder Ankündigungen zu Strompreisen, Genehmigungen oder Exportauflagen kommen schnell. Dann kommt der Verkaufsdruck früher und stärker.

Das ist eine Einschätzung aus den Meldungen, keine Rechnung und keine Anlageberatung. Sie gilt nur für den Papierhandel.

## Quellen
- [CNBC, 5.11.2026](https://www.cnbc.com/2026/11/05/stock-market-selloff-ai-fears.html)
- [Washington Post, 4./5.11.2026](https://www.washingtonpost.com/business/2026/11/04/stocks-markets-ai-midterm-oil-trump/fe6cabb2-d8c4-11f1-930a-e08f8086f5ac_story.html)
- [CNN, 5.11.2026](https://us.cnn.com/2026/11/05/markets/asian-markets-us-rout-ai-investment-intl-hnk)
- [Motley Fool, 5.11.2026](`https://www.fool.com/investing/2026/11/05/the-ai-selloff-is-getting-worse-nvidia-dell-and-data-center-stocks-are-plunging-hard-is-it-time-to-buy-the-dip/`)

## Lesbarer Verweis zum letzten Original-Link

Die Backticks im ursprünglichen Motley-Fool-Link sind ein Formatierungsfehler.
Der lesbare Link lautet:
[Motley Fool, 05.11.2026](https://www.fool.com/investing/2026/11/05/the-ai-selloff-is-getting-worse-nvidia-dell-and-data-center-stocks-are-plunging-hard-is-it-time-to-buy-the-dip/).
Das Original oben bleibt zur Nachvollziehbarkeit unverändert.

