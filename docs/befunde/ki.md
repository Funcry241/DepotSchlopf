# Schnitt KI-Kostenschutz

Stufe 2. Wolfgang und Maus haben am 03.11.2026 entschieden: zuerst den vorhandenen Budgetwächter samt Kassenbuch absichern und seine offenen Befunde schließen. Neue Budgetbeträge kommen nicht nebenbei, sondern mit dem Freigabepaket. Bis dahin gelten die Werte im Code unverändert (`KiGrenzen::standard`, `KiDeckel::standard`).

Der Schnitt schließt technisch 1·K01 bis 1·K06 und 1·R02, entlang R19, R20, R24, R25, R29 und R38. Ein grüner Test schließt keinen Befund. „Technisch geschlossen“ setzt erst die unabhängige Prüfung durch Maus.

## Nachtrag Persistenzbasis

Seit dem Schnitt Persistenzbasis steht das Kassenbuch im Journal (`docs/befunde/journal.md`). Jede Buchung ist ein Eintrag; Anker, eigene Datei, Sperre und Zwischendatei des Kassenbuchs entfallen und leistet das Journal. Die Tests zu 1·K06, K12-01, K12-02 und den Speicherfehlern aus 1·R02 stehen deshalb in `crates/depot_journal`. Die Abschnitte unten beschreiben den Stand von PR #12. Die dort genannten Grenzen „Anker neben dem Kassenbuch“, „Abgebrochener Schreibvorgang“ und „Grenzen ändern sich“ sind mit der Persistenzbasis behandelt.

## Fassung 2 nach Maus' Prüfung von `7d887ed` (K12-01, K12-02)

Maus' Gegenproben stehen als Tests in `crates/depot_ki/tests/befunde.rs`, am Stand `7d887ed` rot (eigener Commit `a94ae68`). Der K12-01-Test prüft sechs Fälle: symbolischer und harter Link als Zwischendatei, jeweils bei Einrichtung, Reservierung und Bestätigung beim Öffnen. Behoben ist allgemein und ohne neue Prüfschicht:

- **K12-01 (hoch):** Die Zwischendatei des Ankers wird nur noch exklusiv neu angelegt (`create_new`). Eine liegen gebliebene Zwischendatei wird vorher entfernt; das entfernt nur ihren Namen, nie das Ziel eines Links. Eine fremde Datei wird damit nie geöffnet, und das Umbenennen ersetzt nur den Namen des Ankers.
- **K12-02 (mittel):** Kein Teil des übergebenen Pfads darf ein Link sein, `..` ist verboten. Unter Windows meldet `is_symlink` auch Verzeichnisverknüpfungen (Junctions). Die Prüfung ist statisch vor dem Öffnen; gegen einen Austausch danach schützt die Sperre der geöffneten Datei, nicht der Name.
- **1·R02, Teilnachweis Synchronisationsfehler:** Jede Synchronisation läuft über eine Stelle, an der die Modultests einen Fehler einspeisen können. Der Test `befund_1_r02_synchronisationsfehler_sperrt_das_kassenbuch` speist ihn einmal beim Kassenbuch und einmal beim Anker ein. Danach gibt es keine Erlaubnis und keine weitere Zeile. Im gebauten Programm gibt es diese Stelle nicht.

## Rote Tests

Die 13 Negativtests stehen seit Stufe 2, Schritt 1 (Commit `d887591`, geschärft in `e7e7a7b` und `8f14238`) in `crates/depot_ki/tests/befunde.rs`, mit `ignore` markiert. Am Stand `b09dab0` (main vor diesem Schnitt) sind alle 13 mit `--ignored` rot. Der Umbau entfernt das `ignore`.

Wo sich die Schnittstelle geändert hat, sind nur Aufrufe und Vorbereitung nachgeführt: `reserviere` nimmt keine Grenzen mehr entgegen, ein Kassenbuch entsteht nur durch `einrichten`, und zur Hülle eines Kassenbuchs gehört jetzt ein Anker. Zwei Tests mussten neu gefasst werden, weil ihr alter Weg nicht mehr existiert:

- **1·K02:** Der alte Test setzte einen negativen Preis über ein öffentliches Feld. Diese Felder gibt es nicht mehr. Der Test verlangt jetzt, dass sich ein solcher Preis außerhalb von `depot_ki` nicht bauen lässt und jeder gebundene Deckel mehr als null reserviert. Die Prüfung auf positive Werte steht im Modultest `preise_sind_positiv`.
- **1·R02, Öffnungsfehler:** Der alte Test legte an die Stelle des Kassenbuchs ein Verzeichnis. Das Kassenbuch wird jetzt einmal geöffnet und gesperrt und nie wieder über den Namen geöffnet. Der Fehler wird deshalb beim Schreiben des Ankers eingespeist. Verlangt ist wie vorher: danach keine Erlaubnis und keine weitere Zeile. Neu verlangt der Test, dass die schon synchronisierte Zeile beim nächsten Öffnen zählt.

Dazu kamen Tests in `crates/depot_ki/tests/budget.rs`: erschöpfter Monat bleibt nach einem Neustart bis zum Folgemonat gesperrt; geänderte bestätigte Zeile führt zu Halt; zwei unbestätigte Zeilen führen zu Halt; eine unbestätigte Zeile wird geprüft und bestätigt; Einrichten überschreibt nichts. Der K06-Linktest prüft jetzt auch den Anker als Link und verlangt „Link“ als Grund.

```text
cargo test --locked -p depot_ki
```

## Was der Umbau tut

- **Grenzen nur aus der Festschreibung (1·K01, R19).** `KiGrenzen`, `KiDeckel` und `Preise` haben private Felder und keine Setzer. Es gibt sie nur über `standard`. Das Kassenbuch bindet Grenzen und Deckel beim Öffnen; `reserviere` nimmt nur noch Berater, Datum und Zyklus. Eigene Werte gibt es nur in den Modultests.
- **Geprüfte Werte (1·K02, 1·K03, R20).** Preise sind positiv; ein ungültiger Festwert ergibt Ersatzpreise, mit denen jede Reservierung als `Rechenfehler` scheitert (fail-closed). Das Datum ist das geprüfte `Datum` aus `depot_kern`; den 31.02. gibt es nicht, und private Felder verhindern den Umweg.
- **Einrichtung und Anker (1·K04, R25, R38).** Ein Kassenbuch entsteht nur durch `einrichten`. Neben ihm liegt ein Anker mit dem bestätigten Stand (Zeilenzahl und SHA-256). Er wird atomar geschrieben (Zwischendatei, Synchronisation, Umbenennen, Verzeichnis synchronisieren) und rückt erst vor, wenn die neue Zeile synchronisiert ist. Fehlt eine der Dateien, ist das Kassenbuch kürzer oder weicht es in den bestätigten Zeilen ab: Halt. Höchstens eine vollständige Zeile darf über den Anker hinausgehen. Sie ist gebucht, aber nicht mehr bestätigt; sie wird geprüft und zählt. Ein Abbruch gibt also nie Budget zurück.
- **Ganze Historie (1·K05).** Beim Öffnen wird jede Zeile mit denselben Grenzen nachgerechnet wie beim Buchen. Reihenfolge, Zyklen, Tages-, Monats- und Jahresgrenzen und die Höhe jeder Reservierung müssen stimmen. Die Meldung nennt Zeile und Grund.
- **Sperre über die Datei (1·K06, R24).** Gesperrt wird die geöffnete Datei über das Betriebssystem (`File::try_lock`), vor jedem weiteren Lesen. Ein zweiter Name für dieselbe Datei bekommt sie nicht. Nach einem Absturz gibt das Betriebssystem die Sperre frei; die alte Sperrdatei entfällt. Kassenbuch und Anker dürfen keine Links sein.
- **Fehler sperren (1·R02, R29).** Nach einem Schreib-, Synchronisations- oder Ankerfehler gibt dieses Kassenbuch keine Erlaubnis mehr und schreibt nichts mehr. Erst neues Öffnen mit voller Prüfung hebt das auf.

**Maus' Punkte zum Kostenschutz:** Die Höchstkosten stehen vor jeder Erlaubnis synchronisiert im Kassenbuch. Wiederholungen zählen wie jeder Aufruf, ein Abbruch nach dem Buchen zählt, ein Neustart setzt nichts zurück, und ein erschöpfter Monat bleibt bis zum Folgemonat gesperrt. Dass ohne Maus' Prüfbudget kein freiwilliger Auftrag entsteht (R31), hängt am Ablauf der Berater und kommt mit Stufe 5.

## Gegenproben

| Gegenprobe | Ergebnis |
| --- | --- |
| Prüfsumme gegen den Anker nicht verglichen | rot (geänderte bestätigte Zeile) |
| Zeilenzahl und Prüfsumme gegen den Anker nicht geprüft | rot (1·K04 geleert und gekürzt) |
| mehr als ein unbestätigter Eintrag erlaubt | rot |
| Öffnen richtet ein fehlendes Kassenbuch still ein | rot (1·K04 gelöscht) |
| Einrichten überschreibt ein vorhandenes Kassenbuch | rot |
| Historie nicht nachgerechnet | rot (1·K05) |
| Höhe der Reservierung nicht verglichen | rot (1·K05) |
| keine Sperre über das Betriebssystem | rot (1·K06) |
| Link-Verbot entfernt | rot (1·K06) |
| keine Sperre nach Fehler | rot (1·R02, beide Tests) |
| negativer Preis erlaubt | rot (`preise_sind_positiv`) |
| Zwischendatei weder entfernt noch exklusiv angelegt | rot (K12-01) |
| Pfadteile vor dem letzten nicht geprüft | rot (K12-02) |
| Synchronisationsfehler sperrt nicht | rot (Sync-Test) |
| Synchronisationsfehler beim Anker übergangen | rot (Sync-Test) |

Einzelne Schichten sind doppelt gesichert und allein entfernt noch grün: Ohne die Zeilenzahlprüfung fängt die Prüfsumme ein gekürztes Kassenbuch, ohne die Einrichtungsprüfung verweigert `create_new` das Überschreiben, und bei der Zwischendatei reicht jeder der beiden Schritte, Entfernen oder exklusives Anlegen. Rot wird es erst, wenn beide fehlen.

## Grenzen

- **Festschreibung.** Die Prüfsumme braucht `sha2` in `depot_ki`. Die Bibliothek ist im Projekt schon in derselben Version vorhanden, ändert aber die festgeschriebenen Abhängigkeiten und `Cargo.lock`. Deshalb ist dies eine `FestschreibungAenderung` und braucht Wolfgangs Grenzfreigabe. Grenzen, Budgets und Preise bleiben unverändert. Die Mindestversion von Rust steigt auf 1.89 wegen `File::try_lock`.
- **Anker neben dem Kassenbuch.** Bis zur Persistenzbasis liegt der Anker neben dem Kassenbuch, danach im Journal (R38). Wer beide Dateien zusammen löscht oder zurücksetzt, wird so nicht erkannt; dagegen hilft nur der Anker außerhalb des Rechners (Abendbericht) und das Prepaid-Guthaben beim Anbieter.
- **Abgebrochener Schreibvorgang.** Eine unvollständige letzte Zeile führt zu Halt, auch nach einem echten Absturz. Sie automatisch zu kürzen braucht den Anhangsvermerk aus 4·B02 und 5·B02 und kommt mit der Persistenzbasis. Bis dahin repariert ein Mensch.
- **Windows-Dateien in der Cloud.** Unter Windows zählen nur Links und Junctions als Link, nicht jeder Analysepunkt. So bleibt ein Projekt in einem OneDrive-Ordner nutzbar.
- **Grenzen ändern sich.** Die Historie wird mit den heutigen Grenzen nachgerechnet. Senkt das Freigabepaket später eine Grenze, kann eine alte, damals gültige Historie Halt auslösen. Der Übergang braucht dann eine eigene, geprüfte Regel.
- **Wechsel zwischen Prüfung und Öffnen.** Die Link-Prüfung ist statisch. Wer einen Teil des Pfads zwischen Prüfung und Öffnen austauscht, wird nicht erkannt; die Sperre gilt dann der tatsächlich geöffneten Datei. Das Kassenbuch schützt gegen Fehler und Versehen, nicht gegen jemanden mit vollem Schreibzugriff (Kap. 9).
- **Pfad des Aufrufers.** Geprüft wird der Pfad, wie ihn der Aufrufer nennt. Die Anwendung nennt ihr Datenverzeichnis relativ zum Projekt; ein Link oberhalb des Projektordners liegt außerhalb dieser Prüfung.
- **Windows.** Der Code ist für Windows gebaut und mit Clippy geprüft, aber nicht dort ausgeführt. Das Verzeichnis wird unter Windows nicht eigens synchronisiert.
