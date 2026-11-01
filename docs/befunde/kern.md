# Schnitt Kern – geprüfte Typen

Stufe 2, Schnitt Kern. Er schließt die Kern-Befunde 1·A01, 1·F01 und 1·R06 entlang R13, R19 und R20. Nebenbei wird der Preisteil von 1·F03 grün, weil sich ein Nullkurs nicht mehr bauen lässt.

Klasse `FestschreibungAenderung`: Die Tests im Regelwerk bekommen den Prüfstand als Entwicklungsabhängigkeit, und jede Abhängigkeit steht in der Festschreibung. Die Fingerabdrücke von `grenzen` und `ki_budget` bleiben unverändert.

Ein grüner Test schließt keinen Befund. „Technisch geschlossen“ setzt erst die unabhängige Prüfung durch Maus.

## So prüfst du es nach

```text
cargo test --locked -p depot_kern --test befunde
cargo test --locked -p depot_regelwerk --test befunde befund_1_f03
```

Am ersten Commit des Schnitts (`3c4cf4d`, nur Tests) zeigt `cargo test --locked -p depot_kern --test befunde -- --ignored` alle zehn Kern-Tests rot: die sieben aus Schritt 1 und drei neue für Währung, Handelstag, Herkunft und Alter, die den Teilnachweis von 1·R06 schließen.

## Ergebnis

| Befund | Test | am alten Stand | nach dem Umbau |
| --- | --- | --- | --- |
| 1·A01 | `befund_1_a01_euro_meldet_ueberlauf_als_fehler` | rot: `Cent::euro` sättigt | grün: `Option<Cent>` |
| 1·A01 | `befund_1_a01_abs_meldet_ueberlauf_als_fehler` | rot: `Cent::abs` sättigt | grün: `Option<Cent>` |
| 1·F01 | `befund_1_f01_grenze_im_literal_laesst_sich_nicht_bauen` | rot | grün: Felder privat (E0451) |
| 1·F01 | `befund_1_f01_geladene_grenze_laesst_sich_nicht_lockern` | rot | grün: Felder privat (E0616) |
| 1·R06 | `befund_1_r06_kurs_aus_rohem_betrag_laesst_sich_nicht_bauen` | rot | grün: `Kurse` hat ein privates Feld (E0423) |
| 1·R06 | `befund_1_r06_geladene_kurse_lassen_sich_nicht_roh_aendern` | rot | grün (E0616) |
| 1·R06 | `befund_1_r06_negatives_bargeld_laesst_sich_nicht_anlegen` | rot | grün: `Topf::neu` lehnt ab, Felder privat |
| 1·R06 (neu) | `befund_1_r06_kurs_nur_in_euro_und_positiv`: `USD`, `eur`, leer, `EUR ` mit Leerzeichen, Preis 0 und −1 | rot: Typ fehlt | grün |
| 1·R06 (neu) | `befund_1_r06_kurs_traegt_handelstag_und_herkunft`: 31.02., 29.02.2026, 31.04., Monat 13, Tag 0, Uhrzeit 24:00, 10:60, 10:00:60, Handelstag nach dem Kurszeitpunkt | rot: Typ fehlt | grün |
| 1·R06 (neu) | `befund_1_r06_kurse_kennen_ihr_alter`: Alter 1.200 s, Kurs aus der Zukunft, doppelter Kurs | rot: Typ fehlt | grün |
| 1·F01 (Wächter) | `befund_1_f01_pruefstand_ist_ohne_feature_nicht_erreichbar`: Setzer ohne Feature nicht vorhanden (E0599) | – (Prüfstand neu) | grün |
| 1·F03 | `befund_1_f03_nullkurs_erzeugt_keinen_notauftrag_sondern_halt` | rot: Notbremse verkauft zu 0,00 € | grün |
| 1·F03 | `befund_1_f03_nullkurs_erzeugt_im_regelwerk_keinen_auftrag` | rot: Ausstieg zu 0,00 € und −1,00 € | grün |

**Zu 1·F03:** Die beiden Tests haben einen Kurs von 0 und −100 Cent in die Kurse geschrieben. Das geht nicht mehr, denn `Kurs::neu` lehnt beide ab. Die Tests verlangen jetzt genau das, und danach prüfen sie den Zustand, der an die Stelle des abgelehnten Kurses tritt: Der Kurs fehlt. Notbremse und Regelwerk erzeugen dann keinen Auftrag. Geschlossen wird der Preisteil also durch den Typ (R20), nicht durch eine neue Prüfung im Regelwerk. Der Teilnachweis für spätere Auftragsarten bleibt offen.

**Weitere Testanpassungen im Regelwerk:** Die Tests bauen Kurse über `Kurs::neu` und `Kurse::neu`, Töpfe über `Topf::neu` und Grenzen über die Setzer des Prüfstands. Im Test `kursdaten_fehlend_oder_sprunghaft` entfällt der Fall „Kurs 0 in den Kurstagesdaten“; an seiner Stelle steht, dass `Kurs::neu` den Kurs 0 ablehnt. Keine Aussage eines Tests ist schwächer geworden.

## Gegenproben

Jede Zeile schwächt den neuen Kern an genau einer Stelle. Jedes Mal wird der zuständige Test rot.

| Gegenprobe | Ergebnis |
| --- | --- |
| `Cent::euro` sättigt wieder | A01 `euro` rot |
| `Cent::abs` sättigt wieder | A01 `abs` rot |
| Felder der Grenzen öffentlich | F01 Literal rot, F01 Lockern rot (je einzeln geprüft) |
| Prüfstand als Standard-Feature eingeschaltet | Prüfstand-Wächter rot |
| Feld von `Kurse` öffentlich | R06 roh ändern rot |
| jede nicht leere Währung erlaubt | R06 Euro rot |
| Handelstag nicht gegen den Zeitpunkt geprüft | R06 Handelstag rot |
| Datum erlaubt jeden Tag bis 31 | R06 Handelstag rot |
| Kurs aus der Zukunft erlaubt | R06 Alter rot |
| doppelter Kurs erlaubt | R06 Alter rot |
| negatives Bargeld erlaubt | R06 Bargeld rot |
| Kurs 0 erlaubt | beide F03-Tests rot |

## Was der Umbau tut

- **Geld (`geld.rs`):** `Cent::euro` und `Cent::abs` liefern `Option<Cent>`. Ein Überlauf ist `None`, nie ein gekappter Wert.
- **Grenzen (`grenzen.rs`):** `Grenzen` und `Gebuehren` haben nur private Felder und je Feld einen Leser. Zu bauen sind sie nur über `Grenzen::standard()`, deren Fingerabdruck in der Festschreibung steht. Setzer gibt es allein mit dem Feature `pruefstand`. Das schalten nur die Tests von `depot_regelwerk` als Entwicklungsabhängigkeit ein. Wollte Produktivcode es einschalten, änderte sich die Festschreibung, und das geht nur mit Grenzfreigabe (R30). Die Textform der Grenzen ist unverändert, deshalb auch ihr Fingerabdruck.
- **Kalender (`zeit.rs`):** `Datum` nimmt nur Tage an, die es im gregorianischen Kalender gibt (1970 bis 9999). `Zeitpunkt` ist eine geprüfte Uhrzeit in UTC. Die Uhr liest der Kern nie; den Zeitpunkt „jetzt“ reicht der Aufrufer herein.
- **Kurse (`kurs.rs`):** Ein `Kurs` ist positiv, in Euro und trägt Handelstag, Zeitpunkt und Herkunft (Broker, Aufzeichnung, Simulation). `Kurse` entstehen nur über `Kurse::neu` mit dem Stand des Abrufs. Ein Kurs nach dem Stand oder ein doppelter Kurs wird abgelehnt. Das Alter jedes Kurses ist abrufbar.
- **Topf (`depot.rs`):** Bargeld ist nie negativ, alle Felder sind privat. Den Bestand setzt `setze_bestand`, so wie das Konto ihn meldet.

## Grenzen, die ausdrücklich bleiben

- **Grenzen stehen noch im Code (6·W1):** Die Werte kommen aus `Grenzen::standard()`, gebunden über den Fingerabdruck. Das Laden aus einer Festschreibung mit Betrag und Paketfreigabe ist ein eigener Schnitt.
- **Höchstalter der Kurse:** Der Kern macht das Alter messbar. Dass Entscheidungen nur mit Kursen bis 20 Minuten Alter fallen, setzt das Regelwerk im nächsten Schnitt durch.
- **Herkunft:** Der Kern hält fest, woher ein Kurs stammt. Ob eine Herkunft für Echtgeld zulässig ist, entscheidet später der Broker-Adapter.
- **Zeitzone:** Zeitpunkte sind UTC. Der Handelstag darf nicht nach dem Kurszeitpunkt liegen; die Xetra-Ortszeit kommt mit dem Broker-Adapter.
- **Töpfe statt Bereiche:** Der Kern kennt noch die zwei Töpfe aus Stufe 1. Die Umstellung auf Bereiche gehört zum Schnitt Regelwerk.
- **Kodex:** Ein ausdrückliches Verbot des Features `pruefstand` außerhalb von `dev-dependencies` wäre eine eigene Kodex-Änderung. Heute fängt es die Festschreibung ab.
