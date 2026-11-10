# Vergleich seit Start (#118)

Wolfgang will mit Papiergeld handeln, bis klar ist, dass das System trägt. Ob ein Topf im Plus ist, sagt dafür wenig. Die Frage ist, ob er **besser ist als Nichtstun**. Am fehlenden Vergleichsmaßstab ist KNN5 gescheitert (`docs/essenz.md`). Dazu kommen die KI-Kosten: 20 € im Monat auf 5.000 € sind rund 4,8 % im Jahr, die ein Topf erst verdienen muss.

## Was Live zeigt

Im Papierdepot steht der Block „Vergleich seit Start“ mit vier Zeilen: Dachs (Topf A), Maus (Topf B), das Vergleichsdepot und reines Bargeld. Je Zeile stehen der Wert am letzten abgeschlossenen Papiertag und die Rendite seit Start. Für die Töpfe stehen außerdem die gebuchten KI-Kosten bis zu diesem Tag und die Rendite nach diesen Kosten. Lässt sich ein Wert nicht berechnen, steht dort „nicht bewertbar“ mit Grund, nie eine Zahl. Vor dem ersten abgeschlossenen Papiertag fehlt der Block.

## Rechnung (`depot_lage::vergleich`)

- **Töpfe:** `Stand::tagesschluesse()` liefert je Abschluss die Töpfe zum Abschluss. Das Feld entsteht nur beim Lesen, das Journal schreibt dafür nichts. Bewertet wird der letzte Abschluss mit `tageskurse_am` dieses Tages, abgerundet wie das Lagebild.
- **KI-Kosten:** Summe der `Buchung::reserviert` des Beraters bis einschließlich zum Bewertungstag, wie die Budgetanzeige. Eine nicht abgerechnete Reservierung zählt voll, das ist die vorsichtige Seite.
- **Vergleichsdepot:** Die Werte stehen fest in `liste::VERGLEICH_ISIN` (die 15 Werte vom 10.11.2026). Sie ändern sich nicht, auch wenn die Beobachtungsliste wächst. Das Startgeld ist `START_JE_TOPF` (5.000 €), wie bei jedem Topf. Je Wert stehen `Startgeld / n` zur Verfügung, `n` ist die Zahl der Werte mit Kurs. Davon geht die Gebühr des Papierkontos ab (`Grenzen::gebuehren`). Gekauft werden ganze Stück zum aufgerundeten Cent, der Rest bleibt Bargeld. Ein Wert ohne Kurs am Einstiegstag wird weggelassen und genannt. Danach wird nur gehalten.
- **Vereinfachung:** Das Vergleichsdepot kauft zu den Schlusskursen des ersten Papiertags. Die Töpfe kaufen erst zum nächsten geprüften Schluss nach Versand.
- **Rendite:** `(Wert − Start) × 10.000 / Start` in Basispunkten, Richtung minus unendlich gerundet. 999,99 € gegen 1.000,00 € sind −1 bp und nie 0. `Cent::anteil_bp_ab` wird dafür bewusst nicht verwendet, denn es rundet Richtung null (#110).

## Nachweis

| Test | Prüfwert |
|---|---|
| `vergleich::tests::kaufe_gleich_teilt_und_zieht_gebuehr_ab` | 1.000 € auf 100 €, 33,33 €, 250 € mit 1,50 € Gebühr: 3, 9, 1 Stück, Bargeld 145,53 € |
| `vergleich::tests::bewerte_zum_abrundenden_cent` | danach 110 €, 30 €, 260 €: 1.005,53 €, +55 bp |
| `vergleich::tests::rendite_rundet_verlust_nie_klein` | 999,99 € ergibt −1 bp, 1.000,10 € ergibt +1 bp |
| `vergleich::tests::fehlender_kurs_am_einstieg_wird_genannt` | B ohne Kurs: `ohne = [B]`, Bargeld 347,00 € |
| `vergleich::tests::ki_kosten_zaehlen_bis_zum_tag` | Dachs 24 ct am 1. und 3.: bis zum 2. sind es 24 ct, bis zum 3. sind es 48 ct, Maus getrennt |
| `lage::vergleich_seit_start_mit_pruefwerten` | echtes Journal, zwei Papiertage: Einstieg zu 101 €, am zweiten Tag 102 €, Vergleich 5.022,50 € (+0,45 %), stumme Töpfe ±0,00 % |
| `lage::vergleich_ohne_kurs_ist_nicht_bewertbar` | SAP fehlt am Bewertungstag: „nicht bewertbar: kein Kurs für SAP“ |
| `lage::vergleich_fehlt_vor_dem_ersten_papiertag` | kein Block ohne Abschluss |
| `liste::papierorder_tests::vergleichswerte_sind_gueltig_und_in_der_liste` | jede ISIN gültig und in der Liste |

Keine Grenze, keine neue Abhängigkeit, kein Netz, kein neues Journalformat.
