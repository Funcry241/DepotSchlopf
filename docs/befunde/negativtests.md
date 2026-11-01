# Negativtests – Nachweis am alten Stand

Stufe 2 beginnt damit, dass jeder Befund aus `docs/BEFUNDE.md` ein Negativtest wird (Architektur, Kapitel 19). Diese Datei belegt, welche Tests es gibt, dass sie den Fehler am heutigen Code zeigen und wo ein Befund nur teilweise abgedeckt ist.

**Geprüfter Code:** Produktivcode von Stand `383ee05`. Bis `665ed4b` (Architektur 4.4) hat sich unter `crates/`, `Cargo.toml` und `Cargo.lock` nichts geändert.

**Fassung 3** nach zwei Nachprüfungen durch Maus (Stand `d887591`: N02 bis N08; Stand `e7e7a7b`: R01 bis R03). Die Tests wurden geschärft; kein früheres Gegenbeispiel wurde entfernt oder abgeschwächt.

## So prüfst du es nach

Jedes Crate getrennt ausführen. Ein einziger Workspace-Lauf bricht nach dem ersten roten Testprogramm ab.

```text
cargo test --locked -p depot_kern      --test befunde -- --ignored
cargo test --locked -p depot_ki        --test befunde -- --ignored
cargo test --locked -p depot_regelwerk --test befunde -- --ignored
```

Jeder markierte Test muss am alten Stand fehlschlagen: Kern 7, KI 13, Regelwerk 15, zusammen 35. Im normalen Lauf (`run.cmd`, `cargo test`) tragen sie `#[ignore = "offen: …"]`, damit das Gate grün bleibt. Der Umbau entfernt das `ignore`. Ab dann laufen sie im Kodex und müssen bestehen. Ein Befund ist erst technisch geschlossen, wenn sein Test ohne `ignore` grün ist und eine unabhängige Prüfung das bestätigt.

Tests mit dem Präfix `grenzfall_` sind gezielte Grenzfälle (1·T01). Wo sie schon heute halten, laufen sie ohne `ignore`.

## Wie die Tests falsche Grünmeldungen verhindern

- **Vertrag statt Ausschluss (N02).** Ein Vertrag wird als Programm gegen den Workspace gebaut und ausgeführt (`sonde::muss_laufen`). Er verlangt einen ausdrücklichen Fehlerwert (`None` bei Überlauf) und prüft normale Beträge und Randwerte. Baut das Programm nicht, ist der Vertrag nicht erfüllt. Ein Rückgabewert 0 oder ein anderer falscher Betrag macht den Test also nicht grün.
- **Verbot nur aus dem richtigen Grund (N03).** Die Sonde liest die Compilermeldungen strukturiert (`--message-format=json`). Ein verbotenes Stück gilt nur als abgewiesen, wenn jeder Fehler einen Zugriffscode trägt (E0451, E0616, E0603, E0624, E0423) und die Meldung das betroffene Feld nennt. Ein umbenannter Konstruktor (E0599, E0425), ein Tippfehler oder ein Umgebungsfehler beweist nichts. Zu jedem verbotenen Stück gehört ein erlaubtes Gegenstück, das bauen muss. Die verbotenen Stücke benutzen keinen Konstruktor, sondern bekommen eine vorhandene Grenze übergeben.
- **Vorbedingungen ausdrücklich (Hinweis zu `.ok()`).** Jede Vorbereitung prüft ihren Erfolg. Scheitert sie, endet der Test mit „Vorbedingung …“, nie mit einer Meldung, die wie der Produktfehler aussieht. Im Hardlink-Test wird die erste Öffnung ausdrücklich verlangt.
- **Unabhängiger Nachrechner (N05).** Die Regelwerkstests rechnen den Topf nach allen genehmigten Aufträgen selbst nach: voll zum Limit ausgeführt, mit selbst berechneter Höchstgebühr, in `i128`. Geprüft werden Endgewicht, Positionszahl und Bargeld, nicht bloß, dass irgendein Auftrag entsteht.
- **Gültige Hülle (N07).** Die Historientests schreiben ihre Fälle über eine Funktion `huelle`, die das jeweils aktuelle Dateiformat erzeugt. Eine gültige Historie in derselben Hülle muss öffnen, und die Fehlerursachen der kaputten Fälle müssen sich unterscheiden. Ändert der Umbau das Format, wird nur `huelle` nachgeführt.

## Gegenproben

Wie bei Maus in einer Wegwerfkopie ausgeführt, nicht im Repository:

| Gegenprobe | Ergebnis |
| --- | --- |
| `Cent::euro` liefert bei Überlauf 0 statt eines Fehlers | A01 bleibt rot |
| `Grenzen::standard` nur umbenannt, Felder weiter öffentlich | beide F01-Tests bleiben rot („lässt sich bauen“) |
| Sperre nur nach Öffnungsfehler, nicht nach Schreibfehler | R02-Teilschreibtest bleibt rot |
| nur ein schon überzähliger Bestand wird behandelt, der Kauf einer neunten Position nicht | neuer Test `befund_1_f04_kauf_erzeugt_keine_neunte_position` rot |
| richtiger Umbau: `euro` und `abs` liefern `Option`, Felder von `Grenzen` und `Kurse` privat, `Topf::neu` lehnt negatives Bargeld ab | alle 7 Kerntests grün |
| richtiger Umbau: Sperre nach jedem Schreibfehler, Datumsfelder privat | beide R02-Tests und der K03-Feldtest grün |

Die Gegenproben belegen einzelne Tests, keinen vollständigen Mutantenlauf.

## Ergebnis am alten Stand

| Befund | Test | Ergebnis am alten Stand |
| --- | --- | --- |
| 1·A01 | `depot_kern` `befund_1_a01_euro_meldet_ueberlauf_als_fehler` | rot: `Cent::euro` liefert keinen Fehlerwert |
| 1·A01 | `depot_kern` `befund_1_a01_abs_meldet_ueberlauf_als_fehler` | rot: `Cent::abs` liefert keinen Fehlerwert |
| 1·F01 | `depot_kern` `befund_1_f01_grenze_im_literal_laesst_sich_nicht_bauen` | rot: Grenze mit 100 % Höchstanteil im Literal baubar |
| 1·F01 | `depot_kern` `befund_1_f01_geladene_grenze_laesst_sich_nicht_lockern` | rot: Notbremse einer vorhandenen Grenze änderbar |
| 1·R06 | `depot_kern` `befund_1_r06_kurs_aus_rohem_betrag_laesst_sich_nicht_bauen` | rot: Kurse aus rohen Centbeträgen baubar |
| 1·R06 | `depot_kern` `befund_1_r06_geladene_kurse_lassen_sich_nicht_roh_aendern` | rot: Kurse roh änderbar |
| 1·R06 | `depot_kern` `befund_1_r06_negatives_bargeld_laesst_sich_nicht_anlegen` | rot: `Topf::neu` nimmt jedes Bargeld an, Feld öffentlich |
| 1·K01 | `depot_ki` `befund_1_k01_ki_grenze_im_literal_laesst_sich_nicht_bauen` | rot: Monatsbudget 1.000 € im Literal baubar |
| 1·K01 | `depot_ki` `befund_1_k01_ki_deckel_laesst_sich_nicht_lockern` | rot: Token-Deckel änderbar |
| 1·K02 | `depot_ki` `befund_1_k02_negativer_preis_wird_abgelehnt` | rot: Reservierung −399,67 € und 0,16 € angenommen |
| 1·K03 | `depot_ki` `befund_1_k03_ungueltiges_kalenderdatum_wird_abgelehnt` | rot: 31.02., 29.02.2026, 31.04., 31.11. angenommen |
| 1·K03 | `depot_ki` `befund_1_k03_datum_ueber_felder_umgeht_die_pruefung_nicht` | rot: Reservierung am 31.02. über öffentliche Felder angenommen |
| 1·K04 | `depot_ki` `befund_1_k04_geloeschtes_kassenbuch_fuehrt_zu_halt` | rot: nach dem Löschen beginnt das Budget bei null |
| 1·K04 | `depot_ki` `befund_1_k04_geleertes_kassenbuch_fuehrt_zu_halt` | rot: nach dem Leeren beginnt das Budget bei null |
| 1·K04 | `depot_ki` `befund_1_k04_gekuerztes_kassenbuch_fuehrt_zu_halt` | rot: nach dem Kürzen auf zwei Zeilen ist wieder Budget frei |
| 1·K05 | `depot_ki` `befund_1_k05_kaputte_historie_fuehrt_zu_halt` | rot: Datum rückwärts, Zyklus rückwärts, Reservierung über dem Höchstpreis, zu viele Aufrufe im Zyklus angenommen |
| 1·K06 | `depot_ki` `befund_1_k06_zweiter_pfad_auf_dieselbe_datei_wird_gesperrt` | rot: dieselbe Datei über harten Link zweimal offen |
| 1·K06 | `depot_ki` `befund_1_k06_symbolischer_link_wird_abgelehnt` (nur Unix) | rot: Kassenbuch über Link geöffnet |
| 1·R02 | `depot_ki` `befund_1_r02_oeffnungsfehler_sperrt_das_kassenbuch` | rot: nach dem Öffnungsfehler weitergeschrieben |
| 1·R02 | `depot_ki` `befund_1_r02_teilschreibfehler_sperrt_das_kassenbuch` (nur Unix) | rot: nach dem Teilschreibfehler Erlaubnis erteilt. Die Vorgeschichte hält jede Regel ein (vier Aufrufe je Monat, steigende Daten und Zyklen, Reservierung gleich Höchstkosten), wird Buchung für Buchung gegen `pruefe` geprüft und vor dem Fehler vollständig gelesen |
| 2·A12 | `depot_ki` `grenzfall_2_a12_hoechstpreis_wird_auf_ganze_cent_aufgerundet` | grün: nur der Rundungsteil |
| 1·F02 | `depot_regelwerk` `befund_1_f02_vorschlag_fuer_fremden_bereich_wird_abgelehnt` | rot: Vorschlag von Maus für den Topf von Dachs genehmigt |
| 1·F03 | `depot_regelwerk` `befund_1_f03_nullkurs_erzeugt_keinen_notauftrag_sondern_halt` | rot: Notbremse verkauft mit Limit 0,00 € |
| 1·F03 | `depot_regelwerk` `befund_1_f03_nullkurs_erzeugt_im_regelwerk_keinen_auftrag` | rot: zwingender Ausstieg zum Kurs 0,00 € und −1,00 € |
| 1·F04 | `depot_regelwerk` `befund_1_f04_marktbewegung_ueber_toleranz_erzeugt_abbaupflicht` | rot: nach den Aufträgen 28 % statt höchstens 25 % |
| 1·F04 | `depot_regelwerk` `befund_1_f04_zu_viele_positionen_erzeugen_abbaupflicht` | rot: 9 Werte statt höchstens 8, Vorschlag nur verworfen |
| 1·F04 | `depot_regelwerk` `befund_1_f04_kauf_erzeugt_keine_neunte_position` | rot: aus acht zulässigen Werten wird durch einen Kauf von 20 Stück ein neunter |
| 1·F05 | `depot_regelwerk` `befund_1_f05_umschlag_am_limit_ueber_der_grenze_wird_gekuerzt` | rot: 5.050 € über erlaubten 5.000 € |
| 1·F06 | `depot_regelwerk` `befund_1_f06_teilverkauf_unter_mindestorder_am_limit_abgelehnt` | rot: 5 × 99 € = 495 € unter Mindestorder |
| 1·F08 | `depot_regelwerk` `befund_1_f08_fehlender_vortag_ist_kein_bestandener_sprungtest` | rot: fehlender Vortagskurs still akzeptiert |
| 1·F09 | `depot_regelwerk` `befund_1_f09_notbremse_und_umschichtung_vergeben_verschiedene_kennungen` | rot: doppelte Kennungen |
| 2·A04 | `depot_regelwerk` `befund_2_a04_halt_sperrt_den_zwingenden_ausstieg_nicht` | rot: zwingender Ausstieg fehlt, nur „Halt“ |
| 2·A04 | `depot_regelwerk` `befund_2_a04_umschlaggrenze_kuerzt_den_zwingenden_ausstieg_nicht` | rot: 50 von 80 Stück verkauft |
| 2·A04 | `depot_regelwerk` `befund_2_a04_orderzahl_sperrt_zwingende_ausstiege_nicht` | rot: 2 von 6 Ausstiegen fehlen |
| 2·A04 | `depot_regelwerk` `befund_2_a04_kombination_sperrt_zwingende_ausstiege_nicht` | rot: kein Ausstieg unter Halt |
| 2·A04 | `depot_regelwerk` `grenzfall_2_a04_kleiner_rest_wird_zwingend_verkauft` | grün: Rest unter Mindestorder ohne Halt |
| 1·T01 | `depot_regelwerk` `grenzfall_hoechstanteil_gilt_am_limit` | rot: nach dem Kauf 25,2 % bei vorhandenem Bestand |
| 1·T01 | `depot_regelwerk` `grenzfall_mindestorder_kauf_am_limit` (stückgenau: 4, 5, 5, 6 Stück), `grenzfall_deckung_mit_gebuehr_am_limit`, `grenzfall_orderzahl_genau_an_der_grenze`, `grenzfall_kurssprung_genau_an_der_grenze`, `grenzfall_limit_abstand`, `grenzfall_gesamtverkauf_unter_mindestorder_bleibt_erlaubt` | grün |

Außerdem verspricht der Zufallstest im Namen nur noch, was er prüft: `zufallstest_grenzen_am_schlusskurs` (R27).

## Teilnachweise, die noch fehlen

Ein Befund mit „Test rot“ ist nur so weit abgedeckt, wie diese Liste es zulässt. Die fehlenden Teile entstehen mit dem genannten Schnitt.

| Befund | Fehlender Teil | Grund | entsteht mit |
| --- | --- | --- | --- |
| 1·R02 | Fehler beim Synchronisieren (`sync_all`) | Ohne Speicher-Schnittstelle gibt es am alten Stand keinen Einspeisepunkt. Teilschreibfehler ist über `ulimit -f` abgedeckt, nur unter Unix | Persistenzbasis |
| 1·R06 | Währung, Herkunft und Alter eines Kurses | Der alte Kern kennt diese Angaben nicht | Kern und Regelwerk (Kurstyp) |
| 1·F04 | Kaufgrenze und Bestandsgrenze je Bereich mit festgeschriebener Toleranz | Toleranz und Bereiche fehlen im alten Kern; die Tests nehmen 2 Punkte laut Kapitel 18 | Kern und Regelwerk |
| 2·A04 | Ausgabenbudget, Ausstiegsreserve, Gebührenquote als eigene Prüfung | Budget und Gebührenquote gibt es im alten Regelwerk nicht | Kostenbuch (Stufe 3, 2·A05) |
| 1·F03 | Kursprüfung in allen späteren Auftragsarten (Schutzstopp, Nachziehen) | Diese Auftragsarten fehlen | Stufe 3 und 6 |

## Befunde, deren Negativtest mit einem späteren Schnitt entsteht

Diese Tests lassen sich nicht sinnvoll gegen den alten Stand schreiben, weil das Prüfobjekt dort fehlt oder nur über Kodex, Persistenz oder Export erreichbar ist. Der Kodex verbietet, Tests für den Kodex in einer Änderung der Klasse `TestOnly` anzulegen. Deshalb enthält jeder dieser Schnitte als ersten Commit nur seine Negativtests, mit dem roten Ergebnis am alten Stand im Pull Request.

| Schnitt | Befunde |
| --- | --- |
| Kodex | 1·G01–G06, 1·R03, 1·R08, 2·A11 – Tests und Nachweis in `docs/befunde/kodex.md` |
| Kern und Regelwerk aus der Festschreibung | 2·A07, 2·A12 (Freigabebindung), 6·W1 |
| Persistenzbasis | 1·R01, 2·A08, 3·P4, 4·B02, 5·B02 |
| Export | 1·E01, 1·E02, 1·E03, 1·R04, 1·R05, 2·W3 |
