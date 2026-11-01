# Negativtests – Nachweis am alten Stand

Stufe 2 beginnt damit, dass jeder Befund aus `docs/BEFUNDE.md` ein Negativtest wird (Architektur, Kapitel 19). Diese Datei belegt, welche Tests es gibt und dass sie den Fehler am heutigen Code zeigen.

**Geprüfter Code:** Produktivcode von Stand `383ee05`. Bis `665ed4b` (Architektur 4.4) hat sich unter `crates/`, `Cargo.toml` und `Cargo.lock` nichts geändert.

**So prüfst du es nach:**

```text
cargo test -p depot_kern      --test befunde -- --ignored
cargo test -p depot_ki        --test befunde -- --ignored
cargo test -p depot_regelwerk --test befunde -- --ignored
```

Jeder dieser Tests muss am alten Stand fehlschlagen. Im normalen Lauf (`run.cmd`, `cargo test`) sind sie mit `#[ignore = "offen: …"]` markiert, damit das Gate grün bleibt. Der Umbau entfernt das `ignore`. Ab dann laufen sie im Kodex und müssen bestehen. Ein Befund ist erst technisch geschlossen, wenn sein Test ohne `ignore` grün ist und eine unabhängige Prüfung das bestätigt.

Tests mit dem Präfix `grenzfall_` sind gezielte Grenzfälle je harter Grenze (1·T01). Wo sie schon heute halten, laufen sie ohne `ignore`.

Die Tests, die sich „nicht bauen lassen“ prüfen (1·F01, 1·K01, 1·R06), übersetzen ein kleines fremdes Crate gegen den Workspace (`tests/sonde/mod.rs`). Eine Positivprobe stellt sicher, dass ein Fehlschlag am Code liegt und nicht an der Umgebung.

## Ergebnis am alten Stand

| Befund | Test | Ergebnis am alten Stand |
| --- | --- | --- |
| 1·A01 | `depot_kern` `befund_1_a01_ueberlauf_ergibt_keinen_gekappten_wert` | rot: `Cent::euro` kappt einen Überlauf auf `i64::MAX` |
| 1·F01 | `depot_kern` `befund_1_f01_gelockerte_grenze_laesst_sich_nicht_bauen` | rot: ein fremdes Crate kann Grenzen mit 100 % Höchstanteil bauen |
| 1·R06 | `depot_kern` `befund_1_r06_kurs_ohne_handelstag_laesst_sich_nicht_bauen` | rot: Kurse lassen sich aus rohen Centbeträgen ohne Handelstag bauen |
| 1·K01 | `depot_ki` `befund_1_k01_gelockerte_ki_grenze_laesst_sich_nicht_bauen` | rot: ein fremdes Crate kann das Monatsbudget auf 1.000 € setzen |
| 1·K02 | `depot_ki` `befund_1_k02_negativer_preis_wird_abgelehnt` | rot: Reservierung `Ok(Cent(-39967))` |
| 1·K03 | `depot_ki` `befund_1_k03_ungueltiges_kalenderdatum_wird_abgelehnt` | rot: 31.02.2026 angenommen |
| 1·K04 | `depot_ki` `befund_1_k04_geloeschtes_kassenbuch_fuehrt_zu_halt` | rot: nach dem Löschen beginnt das Budget bei null |
| 1·K04 | `depot_ki` `befund_1_k04_geleertes_kassenbuch_fuehrt_zu_halt` | rot: nach dem Leeren beginnt das Budget bei null |
| 1·K04 | `depot_ki` `befund_1_k04_gekuerztes_kassenbuch_fuehrt_zu_halt` | rot: nach dem Kürzen auf zwei Zeilen ist wieder Budget frei |
| 1·K05 | `depot_ki` `befund_1_k05_kaputte_historie_fuehrt_zu_halt` | rot: Datum rückwärts, Zyklus rückwärts, Reservierung über dem Höchstpreis und zu viele Aufrufe im Zyklus angenommen |
| 1·K06 | `depot_ki` `befund_1_k06_zweiter_pfad_auf_dieselbe_datei_wird_gesperrt` | rot: dieselbe Datei ist über einen harten Link ein zweites Mal offen |
| 1·K06 | `depot_ki` `befund_1_k06_symbolischer_link_wird_abgelehnt` (nur Unix) | rot: Kassenbuch über einen Link geöffnet |
| 1·R02 | `depot_ki` `befund_1_r02_schreibfehler_sperrt_das_kassenbuch` | rot: nach dem Schreibfehler wurde weitergeschrieben |
| 2·A12 | `depot_ki` `grenzfall_2_a12_hoechstpreis_wird_auf_ganze_cent_aufgerundet` | grün: 0,144 € werden als 15 Cent reserviert (Rundungsteil; Freigabebindung folgt) |
| 1·F02 | `depot_regelwerk` `befund_1_f02_vorschlag_fuer_fremden_bereich_wird_abgelehnt` | rot: Vorschlag von Maus für den Topf von Dachs genehmigt |
| 1·F03 | `depot_regelwerk` `befund_1_f03_nullkurs_erzeugt_keinen_notauftrag_sondern_halt` | rot: Notbremse liefert einen Verkauf mit Limit 0,00 € |
| 1·F04 | `depot_regelwerk` `befund_1_f04_marktbewegung_ueber_toleranz_erzeugt_abbaupflicht` | rot: keine Abbaupflicht bei 28 % Bestand |
| 1·F05 | `depot_regelwerk` `befund_1_f05_umschlag_am_limit_ueber_der_grenze_wird_gekuerzt` | rot: Umschlag am Limit 5.050 € über erlaubten 5.000 € |
| 1·F06 | `depot_regelwerk` `befund_1_f06_teilverkauf_unter_mindestorder_am_limit_abgelehnt` | rot: Teilverkauf 5 × 99 € = 495 € unter Mindestorder 500 € |
| 1·F08 | `depot_regelwerk` `befund_1_f08_fehlender_vortag_ist_kein_bestandener_sprungtest` | rot: fehlender Vortagskurs still akzeptiert |
| 1·F09 | `depot_regelwerk` `befund_1_f09_notbremse_und_umschichtung_vergeben_verschiedene_kennungen` | rot: doppelte Kennungen |
| 2·A04 | `depot_regelwerk` `befund_2_a04_zwingender_ausstieg_trotz_halt_und_mindestorder` | rot: zwingender Ausstieg fehlt, nur Befund „Halt“ |
| 1·T01 | `depot_regelwerk` `grenzfall_hoechstanteil_gilt_am_limit` | rot: Kauf von 25 × 101 € = 2.525 € über Höchstanteil 2.500 € |
| 1·T01 | `depot_regelwerk` `grenzfall_mindestorder_kauf_am_limit`, `grenzfall_deckung_mit_gebuehr_am_limit`, `grenzfall_orderzahl_genau_an_der_grenze`, `grenzfall_kurssprung_genau_an_der_grenze`, `grenzfall_limit_abstand`, `grenzfall_gesamtverkauf_unter_mindestorder_bleibt_erlaubt` | grün |

Außerdem verspricht der Zufallstest im Namen nur noch, was er prüft: `zufallstest_grenzen_am_schlusskurs` (R27).

## Befunde, deren Negativtest mit einem späteren Schnitt entsteht

Diese Tests lassen sich nicht sinnvoll gegen den alten Stand schreiben, weil das Prüfobjekt dort fehlt oder nur über Kodex, Persistenz oder Export erreichbar ist. Der Kodex verbietet, Tests für den Kodex in einer Änderung der Klasse `TestOnly` anzulegen. Deshalb enthält jeder dieser Schnitte als ersten Commit nur seine Negativtests, mit dem roten Ergebnis am alten Stand im Pull Request.

| Schnitt | Befunde |
| --- | --- |
| Kodex | 1·G01–G06, 1·R03, 1·R08, 2·A11 |
| Kern und Regelwerk aus der Festschreibung | 2·A07, 2·A12 (Freigabebindung), 6·W1 |
| Persistenzbasis | 1·R01, 2·A08, 3·P4, 4·B02, 5·B02 |
| Export | 1·E01, 1·E02, 1·E03, 1·R04, 1·R05, 2·W3 |
