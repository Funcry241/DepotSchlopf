# Papiertag mit echten Kursen

Wolfgang hat am 05.11.2026 umgesteuert: mutiger und schneller, sofort täglicher Papierhandel, in dem Dachs und Maus aus Kursen und Nachrichten entscheiden. Fest bleibt: nur Papiergeld, kein Produkt mit Verlust über den Einsatz, harte KI-Kostengrenzen, Geheimnisse nie in Git oder Export, Kodex. Maus prüft vorab streng Geheimnisse, Geld und Grenzen; den Rest am Journal.

Dieser Schritt baut den Rahmen für den täglichen Tag. Dachs und Maus werden in einem eigenen Schritt angebunden (Netz, Schlüssel, Grenzfreigabe).

## Ablauf am Abend

1. `run.cmd kurse` nach 17:35 Uhr: Der Sammler belegt die Schlusskurse im Journal `daten/journal.txt`.
2. `run.cmd papier`: ein Papiertag.

## Was gebaut ist

- **Kurse** (`depot_sim::tageskurse`): nur Kursbelege mit Herkunft `Quelle` und nur für Werte der Positivliste. Handelstag ist der jüngste belegte Tag. Je Wert gilt der zuletzt belegte Kurs dieses Tages; der Vergleichskurs ist der zuletzt belegte Kurs des jüngsten Tages davor. Geprüft wird wie im Sammler mit `pruefe_kurse`; fällt die Prüfung durch, fällt der Zyklus aus, und niemand wird gefragt.
- **Nur heute:** Ist der Handelstag nicht der Tag der Uhr, hält der Papiertag an, bevor etwas geschrieben wird. Ein Tag wird nie mit Kursen eines anderen Tages entschieden.
- **Einmal je Handelstag:** Ist der Handelstag schon abgeschlossen, geschieht nichts. Ein Abschluss nach dem Handelstag ist Halt.
- **Wiederanlauf:** Hat das Journal ein Depot, gleicht der Kontowächter zuerst mit dem Papierkonto ab; jede Abweichung ist Halt, nichts wird gesendet. Steht nach dem letzten Abschluss ein Eintrag, ist ein Tag angefangen: Halt, niemand wird neu gefragt.
- **Kurs- und Terminbelege beginnen keinen Tag** (`Stand::seit_abschluss`): Der Sammler schreibt sie vor dem Papiertag ins selbe Journal. Sie fragen niemanden und reservieren nichts.
- **Papierkonto als Datei** (`depot_sim::Papierdatei`, `daten/papierkonto.txt`): unabhängig vom Journal. Nach jedem Versand und jedem Marktlauf wird die ganze Datei neu geschrieben, erst als Nebendatei, synchronisiert, dann an ihren Platz. Gelesen wird nur genau die eigene Form (`Papierkonto::aus_text`). Misslingt das Schreiben, gilt der Versand als ohne Antwort, der Lauf endet mit Fehler, und der nächste Lauf hält beim Abgleich. Fehlt die Datei zu einem Depot im Journal oder liegt sie ohne Depot vor, ist Halt.
- **Feine Kurse:** Das Papierkonto führt einen Kauf zum aufgerundeten und einen Verkauf zum abgerundeten Cent aus. Das Limit in ganzen Cent gilt damit genau am exakten Kurs. Bisher verfiel jeder Auftrag zu einem Kurs feiner als ein Cent.
- **Ausführung zum Schlusskurs des Entscheidungstags:** eine Vereinfachung, die an jedem Tag als Begründung `Papiertag` im Journal steht. Wirklich ginge ein Auftrag erst nach dem Schluss an den Markt.
- **Berater ohne Anbindung** (`Berater::angebunden`, `Stumm`): Er wird nicht gefragt, und für ihn wird keine Erlaubnis reserviert und nichts gebucht. Ohne Vorschlag oder ohne Zweitmeinung entsteht kein freiwilliger Auftrag; zwingende Aufträge des Regelwerks und der Wächter laufen weiter.
- **Termine als Ereignisse:** die belegten Termine vom Handelstag bis 30 Tage voraus. Sie stehen als Beobachtung im Journal, also genau das, was die Berater gesehen haben.
- **Positivliste:** die Beobachtungsliste, jetzt als reine Daten in `depot_kern::liste`. Was keinen geprüften Schlusskurs hat, kann nicht gehandelt werden.
- **Startstand:** zwei Töpfe zu je 5.000 € Papiergeld (Topf A Dachs, Topf B Maus).

Keine neuen Grenzen, keine neuen Abhängigkeiten, kein Netz, kein echtes Geld.

## Tests

`crates/depot_sim/tests/papiertag.rs`:

- erster Papiertag eröffnet das Depot, schließt ab, bucht keine KI; derselbe Tag noch einmal ändert nichts
- Kurse nicht von heute: Halt, Journal byte-gleich
- Kursbelege nach einem Abschluss beginnen keinen Tag; der nächste Tag läuft
- verändertes Papierkonto: Halt beim Abgleich durch den Kontowächter
- Papierkonto-Datei nur zusammen mit einem Depot im Journal
- Papierkonto liest nur seine eigene Form
- Kauf zu einem Schluss von 230,105 €: ausgeführt zu 230,11 €, Journal und Konto stimmen überein (mit dem alten Code verfällt der Auftrag)

## Fassung 2 nach Maus' Prüfung (K23-01 bis K23-03)

- **K23-01, Schreibfehler hält nicht an:** Das Konto meldet jetzt eine Störung (`Broker::stoerung`). Misslingt das Schreiben der Datei, kehrt das Papierkonto zum geschriebenen Stand zurück, der Versand gilt als ohne Antwort, und der Lauf hält direkt nach dem Versand an, bevor das Journal noch etwas vom Konto bestätigt. Dasselbe vor jedem Abgleich und nach jedem Marktlauf. Allgemein für jeden Broker, nicht nur für die Datei. Den angefangenen Tag klärt der nächste Lauf über den Abgleich.
- **K23-02, „schon abgeschlossen“ ohne Abgleich:** Der Abgleich läuft jetzt vor jeder Rückgabe, auch für einen schon abgeschlossenen Tag.
- **K23-03, Erststart nicht wiederholbar:** Eine Kontodatei ohne Depot im Journal gilt als angefangene Einrichtung, wenn sie genau dem frisch eröffneten Konto zu den Starttöpfen entspricht. Dann setzt der nächste Lauf sie fort. Jede andere Datei ohne Depot bleibt Halt.

Maus' drei Gegenproben stehen als Tests in `crates/depot_sim/tests/papiertag.rs`, rot an `418e741` (Stand `abc7836` plus Tests): `schreibfehler_des_kontos_haelt_vor_jeder_bestaetigung`, `schon_abgeschlossen_erst_nach_dem_abgleich`, `erststart_ohne_kurse_ist_wiederholbar`. Dazu `konto_mit_bewegung_ohne_depot_haelt`.

## Offen

- Dachs und Maus anbinden (Claude und GPT mit Websuche), KI-Kostendeckel 20 € im Monat (Wolfgang, 05.11.2026), neue Grenzen für konzentriertes Handeln: Freigabepaket mit Grenzfreigabe.
- Auswahl verbreitern innerhalb von 25 Abrufen am Tag.
- `run.cmd tag`: Kurse, Termine und Papiertag in einem Schritt.
