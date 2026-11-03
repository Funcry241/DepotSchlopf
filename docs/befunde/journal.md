# Schnitt Persistenzbasis

Stufe 2. Wolfgang hat am 03.11.2026 die Reihenfolge freigegeben: zuerst die Persistenzbasis, dann 1·R04, dann ein simulierter Lauf mit erfundenen Daten. Ziel: absturzfeste Speicherung für Beobachtungen, Papierdepot und Aufträge; ein Journal, das sich nachträglich nicht unbemerkt ändern lässt; Stände, die einen Neustart überstehen; eine geprüfte Wiederherstellung. Schutz ja, aber schlank.

Der Schnitt betrifft 3·P4, 4·B02, 5·B02, 2·A08, die Persistenz von 1·F09 sowie die vom Kassenbuch übernommenen 1·K04, 1·K06 und 1·R02, entlang R12, R20, R24, R25, R29 und R38. Ein grüner Test schließt keinen Befund. „Technisch geschlossen“ setzt erst die unabhängige Prüfung durch Maus.

## Die Grundentscheidung: eine Datei

Bisher war geplant, dass Kassenbuch und Journal zwei Dateien sind und ein Commitprotokoll sie zusammenhält: erst das Kassenbuch schreiben, dann den Stand im Journal bestätigen (R38). Maus hatte vorgemerkt, dass der Anker des Kassenbuchs ins Journal wandert.

Dieser Schnitt geht einen Schritt weiter und legt das Kassenbuch selbst ins Journal. Jede Buchung ist ein Eintrag im Journal. Damit gibt es nur noch eine Datei, eine Kette, einen Vermerk und eine Sperre. Jeder Vorgang ist genau ein Eintrag in genau einer Datei, also gibt es keinen Stand zwischen zwei Dateien, den ein Absturz zerreißen könnte. R38 ist damit durch die Bauart erfüllt, nicht durch ein Protokoll. Das ist weniger Code und weniger Fälle, also weniger Raum für Fehler. Kommt später eine zweite Datei hinzu, gilt R38 für sie wie beschrieben.

## Fassung 2 nach Maus' Prüfung von `eee64cb` (K13-01, K13-02)

Maus' Gegenproben stehen als Tests im Repository, am Stand `eee64cb` rot (eigener Commit `4bbd1ef`).

- **K13-01:** Nach einem gescheiterten `sync_all` kann ein vollständiger Eintrag beim Wiederöffnen schon lesbar sein, ohne dauerhaft gespeichert zu sein. Allgemeine Regel jetzt: Der Vermerk beschreibt nur, was dauerhaft auf der Platte steht. Bevor der Wiederanlauf den Vermerk ändert (Bestätigen, Kürzen, Zurücksetzen des Plans), synchronisiert er das Journal. Scheitert das, bleibt der Vermerk, wie er war. Der Modultest `befund_k13_01_wiederanlauf_synchronisiert_vor_dem_vermerk` prüft die Reihenfolge über ein Protokoll aller Synchronisationen (nur in den Modultests) und den eingespeisten Fehler.
- **K13-02:** Das Journal nahm KI-Buchungen an, die der Budgetwächter ablehnte. Ursache war die Arbeitsteilung: Das Journal prüfte nur die Zeichen, `depot_ki` den Inhalt. Jetzt prüft das Journal jede Buchung selbst nach den Regeln aus `depot_ki`. Dafür hängt `depot_journal` von `depot_ki` ab, nicht mehr umgekehrt. Kassenbuch und `AufrufErlaubnis` stehen in `depot_journal` (`kasse.rs`); `depot_ki` enthält nur noch Regeln ohne Datei. Damit gibt es einen einzigen Zustandsraum: Was das Journal annimmt, nimmt auch der Budgetwächter an, beim Anhängen und als unbestätigten Anhang beim Wiederanlauf. Der Test `befund_k13_02_journal_prueft_ki_buchungen_selbst` prüft sechs Fälle, darunter eine falsche Reservierung unter den heutigen Grenzen.

Die Tests, die das Kassenbuch brauchen, sind aus `crates/depot_ki/tests/` nach `crates/depot_journal/tests/budget.rs` und `crates/depot_journal/tests/ki.rs` verlegt; nur Aufrufe sind nachgeführt.

## Was der Umbau tut

Neues Crate `depot_journal`, abhängig von `depot_kern`, `depot_regelwerk` und `sha2`. `depot_ki` hängt jetzt von `depot_journal` ab.

- **Datensatzformat (Kap. 9):** Jede Zeile ist `länge;kette;eintrag` mit Zeilenende. `kette` ist der SHA-256 aus der Kette der Vorzeile und dem Eintrag. Ein Eintrag enthält keine Steuerzeichen, also auch keinen Zeilenumbruch. Die erste Zeile ist der Kopf und entsteht nur durch `einrichten`.
- **Anhangsvermerk (5·B02):** Neben dem Journal liegt `*.vermerk` mit dem bestätigten Stand (Länge und Kette) und der Länge des geplanten Anhangs. Vor jedem Anhängen wird der Plan vermerkt und synchronisiert, dann die Zeile geschrieben und synchronisiert, dann der Stand vorgerückt. Der Vermerk wird atomar geschrieben: Zwischendatei exklusiv neu anlegen, synchronisieren, umbenennen, Verzeichnis synchronisieren.
- **Wiederherstellung (3·P4, 4·B02, 5·B02):** Der bestätigte Teil muss eine lückenlose Kette sein und genau zum Vermerk passen. Was dahinter liegt, entscheidet allein der Vermerk:
  - nichts dahinter: in Ordnung; ein offener Plan wird zurückgesetzt;
  - genau der vermerkte Anhang und eine gültige Zeile: wird geprüft und bestätigt, er zählt;
  - kürzer als vermerkt und ohne Zeilenende: Schreibabbruch. Der Rest kommt byte-gleich in eine Quarantänedatei (`*.q<stelle>-<hash>`), sie wird synchronisiert und mit dem Rest verglichen, auch wenn sie schon da ist. Erst dann wird gekürzt;
  - alles andere ist Halt: Anhang ohne Vermerk, mehr als vermerkt, kürzer mit Zeilenende, beschädigter vermerkter Eintrag, abweichende Quarantäne, Journal kürzer als der bestätigte Stand.
  
  Jeder Schritt ist wiederholbar. Ein Absturz nach der Quarantäne oder nach dem Kürzen führt beim nächsten Start zum selben Ergebnis. Geschrieben wird erst, wenn der ganze Stand geprüft ist.
- **Stand aus dem Journal (Kap. 11):** Beim Öffnen wird der Stand nur aus den Einträgen nachgerechnet: Töpfe des Papierdepots, belegte Aufträge mit der letzten Kennung, Beobachtungen. Jeder Eintrag wird gelesen und wieder geschrieben; nur wenn derselbe Text entsteht, gilt er. Anhängen und Öffnen gehen durch dieselbe Prüfung. Ein unzulässiger Eintrag wird beim Anhängen abgelehnt, ohne zu schreiben und ohne zu sperren; in der Datei führt er zu Halt.
- **Auftragskennungen (1·F09, R24):** `Journal::kennungsvergabe` setzt hinter der letzten belegten Kennung fort. Belegt wird nur eine größere Kennung. Zwei Vergaben für dieselben Nummern scheitern also spätestens beim Belegen. Aus dem Journal entsteht nie ein sendbarer `GenehmigterAuftrag` und keine `AuftragsKennung`; der Beleg hält die Nummer als Zahl. Am Regelwerk ändert sich nichts.
- **Kassenbuch im Journal (Fassung 2):** Das Journal prüft jede KI-Buchung selbst nach den Regeln aus `depot_ki`, beim Anhängen wie beim Wiederanlauf. `Journal::reserviere` ist der einzige Weg zu einer `AufrufErlaubnis`; sie entsteht erst, wenn die Buchung synchronisiert und bestätigt im Journal steht. `depot_ki` enthält nur noch die Regeln ohne Datei.
- **Übergangsregel für Grenzänderungen (Maus' Punkt):** Vor der ersten Buchung unter neuen KI-Grenzen steht deren Abdruck im Journal (`ki-grenzen`). Buchungen unter einem früheren Abdruck werden nicht nach den heutigen Grenzen beurteilt, zählen aber weiter mit. Ein voller Monat bleibt also voll, auch über eine Grenzänderung. Eine Buchung ohne vorangehenden Abdruck ist Halt. Die Grenzen selbst ändert nur eine Grenzfreigabe.
- **Sperre und Links (1·K06, K12-01, K12-02, R24):** wie beim bisherigen Kassenbuch. Gesperrt wird die geöffnete Datei über das Betriebssystem, vor jedem weiteren Lesen. Kein Teil des Pfads darf ein Link sein. Zwischendateien werden nur exklusiv neu angelegt; eine liegen gebliebene wird vorher entfernt.
- **Fehler sperren (1·R02, R29):** Nach einem Schreib- oder Synchronisationsfehler schreibt das Journal nichts mehr, bis es neu geöffnet und dabei geprüft wird.
- **Anker außerhalb des Rechners:** `Journal::bestaetigt` liefert Länge und Kette des bestätigten Stands für den Abendbericht.

## Rote Tests

Erster Commit `aeef506`: neues Crate mit leerer Schnittstelle und 20 Tests in `crates/depot_journal/tests/befunde.rs`, alle 20 rot. Die KI-Tests sind auf das Kassenbuch im Journal nachgeführt und bauen am alten Stand nicht.

```text
cargo test --locked -p depot_journal
cargo test --locked -p depot_ki
```

Neu sind unter anderem: Schreibabbruch an jeder Stelle der Zeile wird gesichert und gekürzt (3·P4); entferntes Zeilenende und teilweise gekürzte bestätigte Zeile führen zu Halt, auch wenn der Vermerk einen Anhang ankündigt (5·B02); nachgestellter Absturz nach der Quarantäne und nach dem Kürzen endet gleich (5·B02); beschädigte letzte Zeile, vollständiger unbestätigter Eintrag, zu langer Rest, Anhang ohne Vermerk und abweichende Quarantäne (4·B02); Kennungen über den Neustart (1·F09); Papierdepot und Beobachtungen über den Neustart; unzulässige Einträge werden abgelehnt und führen in der Datei zu Halt; bestätigter Stand für den Abendbericht. Der Modultest `befund_1_r02_synchronisationsfehler_sperrt_das_journal` speist den Fehler beim Plan, bei der Zeile und beim Vorrücken ein und prüft, was nach dem Neustart zählt.

Nach dem Umbau kamen drei Fälle dazu, weil eine Gegenprobe sonst nicht griff: falsche Längenangabe bei gültiger Kette, ein anderes gültiges Journal gleicher Länge, kürzer als vermerkt mit Zeilenende. Dazu eine doppelte ISIN im Topf.

In `depot_ki` stehen die Tests zu Sperre, Links und Speicherfehlern jetzt beim Journal. Neu sind `alte_buchungen_zaehlen_nach_einer_grenzaenderung_weiter` und `grenzabdruck_ist_stabil`. Der Teilschreibtest (1·R02) läuft mit `ulimit -f 2`; die Vorgeschichte endet mit einer langen Beobachtung knapp unter der Grenze.

## Gegenproben

Jeder Eingriff einzeln in einer Kopie des Umbaus; danach `cargo test -p depot_journal`. Die KI-Eingriffe betreffen seit Fassung 2 `crates/depot_journal/src/kasse.rs`.

| Eingriff | Ergebnis |
| --- | --- |
| Quarantäne nicht mit dem Rest verglichen | rot |
| keine Quarantäne vor dem Kürzen | rot |
| Kette nicht geprüft | rot |
| Längenangabe nicht geprüft | rot |
| bestätigter Stand nicht mit dem Vermerk verglichen | rot |
| Rest mit Zeilenende als Schreibabbruch behandelt | rot |
| Kürzen ohne Vermerk (Vermerkprüfung und Längenprüfung entfernt) | rot |
| Rest länger als vermerkt abgeschnitten (beide Prüfungen entfernt) | rot |
| Sperre nach Fehler übergangen | rot |
| Plan nicht vermerkt | rot |
| ohne Betriebssystem-Sperre | rot |
| Link im Elternverzeichnis erlaubt | rot |
| Zwischendatei folgt einem Link | rot |
| Einrichten überschreibt (Prüfung und `create_new` entfernt) | rot |
| Kennung nicht streng steigend | rot |
| Topf nicht kanonisch | rot |
| Bestand null oder doppelte ISIN (jeweils mit kanonischer Prüfung entfernt) | rot |
| leere Beobachtung erlaubt | rot |
| Stand beim Anhängen nicht geprüft | rot |
| Steuerzeichen erlaubt | rot |
| KI: aktueller Abschnitt nicht nachgerechnet | rot |
| KI: früherer Abschnitt nach heutigen Grenzen beurteilt | rot |
| KI: Buchung ohne Abdruck zählt | rot |
| KI: kein Abdruck vor der ersten neuen Buchung | rot |
| KI: Abdruck nicht geprüft | rot |
| KI: Buchung nicht kanonisch | rot |
| KI: Reservierung nicht verglichen | rot |
| K13-01: keine Synchronisation vor dem Vermerk | rot |
| K13-01: Synchronisation erst nach dem Vermerk | rot |
| K13-01: Synchronisationsfehler übergangen | rot |
| K13-02: KI-Buchung im Journal nicht geprüft | rot |

Einige Schichten sind doppelt gesichert und allein entfernt noch grün: Ohne die Prüfung „Anhang ohne Vermerk“ fängt die Längenprüfung den Rest, ohne die Einrichtungsprüfung verweigert `create_new` das Überschreiben, und ohne die Prüfung auf Bestand null oder doppelte ISIN fängt die kanonische Form den Eintrag. Rot wird es erst, wenn beide fehlen.

## Grenzen

- **Festschreibung.** Ein neues Crate ändert die festgeschriebenen Abhängigkeiten, die Crate-Liste und `Cargo.lock`. Seit Fassung 2 hängt `depot_journal` von `depot_ki` ab, nicht mehr `depot_ki` vom Journal. Deshalb ist dies eine `FestschreibungAenderung` und braucht Wolfgangs Grenzfreigabe. Grenzen, Budgets und Preise bleiben unverändert (`grenzen` und `ki_budget` gleich).
- **Gemeinsames Zurücksetzen.** Wer Journal und Vermerk zusammen auf einen alten Stand zurücksetzt oder beide löscht, wird auf dem Rechner nicht erkannt. Dagegen hilft nur der bestätigte Stand im Abendbericht und das Prepaid-Guthaben beim Anbieter. Der Abendbericht selbst kommt mit Stufe 6.
- **Reparatur.** Alles, was nicht nachweislich ein unbestätigter Anhang ist, führt zu Halt. Ein geprüftes Reparaturverfahren mit Wolfgangs Freigabe gibt es noch nicht; bis dahin repariert ein Mensch.
- **Reihenfolge der Wiederherstellung.** Die Tests stellen die Zwischenstände nach, die die Reihenfolge Quarantäne, Kürzen, Vermerk erzeugen kann. Einen Absturz mitten im Systemaufruf stellen sie nicht nach.
- **Auftragszustände.** Belegt wird der genehmigte Auftrag. Die Zustände eines Auftrags (R37, 2·A09) und der Kontowächter kommen mit Stufe 3.
- **Verzeichnis-Synchronisation (1·R01).** Neue Dateien werden samt Verzeichnis synchronisiert. Für einen Fehler dabei gibt es keinen Einspeisepunkt; der Teilnachweis fehlt. Unter Windows wird das Verzeichnis nicht eigens synchronisiert.
- **Wechsel zwischen Prüfung und Öffnen.** Die Link-Prüfung ist statisch; die Sperre gilt der tatsächlich geöffneten Datei. Das Journal schützt gegen Fehler und Versehen, nicht gegen jemanden mit vollem Schreibzugriff (Kap. 9).
- **Windows.** Gebaut und mit Clippy geprüft, aber nicht dort ausgeführt.
